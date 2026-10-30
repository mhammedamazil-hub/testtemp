//! Windows PE/PE32+ analyser: headers, sections, imports, exports, resources,
//! .NET metadata, packer heuristics and string extraction.
//!
//! This is triage, not decompilation: it answers "what is this binary, what DLLs
//! and APIs does it pull in, is it packed, is it managed code, what does it say".
//! Instruction-level analysis needs a disassembler (see docs/BINARY_ANALYSIS.md).

use crate::hashes::{entropy, strings_of, wide_strings_of};
use std::collections::BTreeMap;

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]))
}
fn u32le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(at)?,
        *b.get(at + 1)?,
        *b.get(at + 2)?,
        *b.get(at + 3)?,
    ]))
}
fn u64le(b: &[u8], at: usize) -> Option<u64> {
    let lo = u32le(b, at)? as u64;
    let hi = u32le(b, at + 4)? as u64;
    Some(lo | (hi << 32))
}

#[derive(Debug, Clone)]
pub struct Section {
    pub name: String,
    pub virtual_address: u32,
    pub virtual_size: u32,
    pub raw_size: u32,
    pub raw_pointer: u32,
    pub characteristics: u32,
    pub entropy: f64,
}

impl Section {
    pub fn flags(&self) -> String {
        let mut f = Vec::new();
        if self.characteristics & 0x2000_0000 != 0 {
            f.push("exec");
        }
        if self.characteristics & 0x4000_0000 != 0 {
            f.push("read");
        }
        if self.characteristics & 0x8000_0000 != 0 {
            f.push("write");
        }
        if self.characteristics & 0x0000_0020 != 0 {
            f.push("code");
        }
        if self.characteristics & 0x0000_0040 != 0 {
            f.push("idata");
        }
        f.join("|")
    }
    pub fn is_executable(&self) -> bool {
        self.characteristics & 0x2000_0000 != 0
    }
    pub fn is_writable(&self) -> bool {
        self.characteristics & 0x8000_0000 != 0
    }
}

#[derive(Debug, Default, Clone)]
pub struct PeInfo {
    pub is_64: bool,
    pub machine: u16,
    pub timestamp: u32,
    pub subsystem: u16,
    pub entry_point_rva: u32,
    pub image_base: u64,
    pub sections: Vec<Section>,
    pub imports: BTreeMap<String, Vec<String>>,
    pub exports: Vec<String>,
    pub has_resources: bool,
    pub is_dotnet: bool,
    pub dotnet_version: String,
    pub dotnet_strings: Vec<String>,
    pub version_info: Vec<(String, String)>,
    pub indicators: Vec<String>,
    pub ascii_strings: Vec<String>,
    pub wide_strings: Vec<String>,
    pub overlay_size: usize,
}

fn rva_to_offset(sections: &[Section], rva: u32) -> Option<usize> {
    for s in sections {
        let start = s.virtual_address;
        let end = start.saturating_add(s.virtual_size.max(s.raw_size));
        if rva >= start && rva < end {
            let delta = rva - start;
            if delta < s.raw_size {
                return Some((s.raw_pointer + delta) as usize);
            }
        }
    }
    None
}

fn cstr(data: &[u8], at: usize, max: usize) -> String {
    let end = data
        .get(at..at.saturating_add(max))
        .and_then(|s| s.iter().position(|&b| b == 0).map(|p| at + p))
        .unwrap_or_else(|| (at + max).min(data.len()));
    if at >= end {
        return String::new();
    }
    String::from_utf8_lossy(&data[at..end]).to_string()
}

/// Quick check: does this look like a PE image (DOS header + `PE\0\0` signature)?
pub fn is_pe(data: &[u8]) -> bool {
    if data.len() < 0x40 || &data[0..2] != b"MZ" {
        return false;
    }
    let e_lfanew = u32::from_le_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]) as usize;
    data.len() > e_lfanew + 4 && &data[e_lfanew..e_lfanew + 4] == b"PE\x00\x00"
}

pub fn parse(data: &[u8]) -> Result<PeInfo, String> {
    if data.len() < 0x40 || &data[0..2] != b"MZ" {
        return Err("pe: not an MZ/DOS executable".into());
    }
    let e_lfanew = u32le(data, 0x3C).ok_or("pe: truncated DOS header")? as usize;
    if data.get(e_lfanew..e_lfanew + 4) != Some(b"PE\x00\x00") {
        return Err("pe: PE signature missing (DOS stub or not a PE file)".into());
    }
    let coff = e_lfanew + 4;
    let machine = u16le(data, coff).ok_or("pe: truncated COFF header")?;
    let num_sections = u16le(data, coff + 2).ok_or("pe: truncated")? as usize;
    let timestamp = u32le(data, coff + 4).ok_or("pe: truncated")?;
    let size_opt = u16le(data, coff + 16).ok_or("pe: truncated")? as usize;

    let opt = coff + 20;
    let magic = u16le(data, opt).ok_or("pe: truncated optional header")?;
    let is_64 = magic == 0x20B;
    if magic != 0x20B && magic != 0x10B {
        return Err(format!("pe: unknown optional header magic 0x{:x}", magic));
    }
    let entry_point_rva = u32le(data, opt + 16).ok_or("pe: truncated")?;
    let image_base = if is_64 {
        u64le(data, opt + 24).ok_or("pe: truncated")?
    } else {
        u32le(data, opt + 28).ok_or("pe: truncated")? as u64
    };
    let subsystem = u16le(data, opt + 68).ok_or("pe: truncated")?;
    let num_dirs = u32le(data, opt + if is_64 { 108 } else { 92 }).unwrap_or(0) as usize;
    let dirs_at = opt + if is_64 { 112 } else { 96 };

    let mut sections = Vec::new();
    let sec_at = opt + size_opt;
    for i in 0..num_sections {
        let at = sec_at + i * 40;
        let name_raw = data.get(at..at + 8).ok_or("pe: truncated section table")?;
        let name = String::from_utf8_lossy(name_raw)
            .trim_end_matches('\0')
            .to_string();
        let virtual_size = u32le(data, at + 8).ok_or("pe: truncated")?;
        let virtual_address = u32le(data, at + 12).ok_or("pe: truncated")?;
        let raw_size = u32le(data, at + 16).ok_or("pe: truncated")?;
        let raw_pointer = u32le(data, at + 20).ok_or("pe: truncated")?;
        let characteristics = u32le(data, at + 36).ok_or("pe: truncated")?;
        let body = data
            .get(raw_pointer as usize..(raw_pointer as usize + raw_size as usize).min(data.len()))
            .unwrap_or(&[]);
        sections.push(Section {
            name,
            virtual_address,
            virtual_size,
            raw_size,
            raw_pointer,
            characteristics,
            entropy: entropy(body),
        });
    }

    let dir = |idx: usize| -> (u32, u32) {
        if idx >= num_dirs {
            return (0, 0);
        }
        let at = dirs_at + idx * 8;
        (
            u32le(data, at).unwrap_or(0),
            u32le(data, at + 4).unwrap_or(0),
        )
    };

    // ---- imports
    let mut imports: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let (imp_rva, _imp_size) = dir(1);
    if imp_rva != 0 {
        if let Some(mut off) = rva_to_offset(&sections, imp_rva) {
            for _ in 0..512 {
                let name_rva = u32le(data, off + 12).unwrap_or(0);
                if name_rva == 0 {
                    break;
                }
                let dll = rva_to_offset(&sections, name_rva)
                    .map(|o| cstr(data, o, 128))
                    .unwrap_or_default();
                let thunk_rva = {
                    let oft = u32le(data, off).unwrap_or(0);
                    if oft != 0 {
                        oft
                    } else {
                        u32le(data, off + 16).unwrap_or(0)
                    }
                };
                let mut funcs = Vec::new();
                if let Some(mut t) = rva_to_offset(&sections, thunk_rva) {
                    for _ in 0..1024 {
                        let entry = if is_64 {
                            u64le(data, t).unwrap_or(0)
                        } else {
                            u32le(data, t).unwrap_or(0) as u64
                        };
                        if entry == 0 {
                            break;
                        }
                        let step = if is_64 { 8 } else { 4 };
                        let is_ordinal = if is_64 {
                            entry & (1u64 << 63) != 0
                        } else {
                            entry & (1u64 << 31) != 0
                        };
                        if is_ordinal {
                            funcs.push(format!("#{}", entry & 0xffff));
                        } else if let Some(no) = rva_to_offset(&sections, entry as u32) {
                            funcs.push(cstr(data, no + 2, 128));
                        }
                        t += step;
                    }
                }
                if !dll.is_empty() {
                    imports.insert(dll, funcs);
                }
                off += 20;
            }
        }
    }

    // ---- exports
    let mut exports = Vec::new();
    let (exp_rva, _) = dir(0);
    if exp_rva != 0 {
        if let Some(off) = rva_to_offset(&sections, exp_rva) {
            let num_names = u32le(data, off + 24).unwrap_or(0) as usize;
            let names_rva = u32le(data, off + 32).unwrap_or(0);
            if let Some(mut np) = rva_to_offset(&sections, names_rva) {
                for _ in 0..num_names.min(2000) {
                    let r = u32le(data, np).unwrap_or(0);
                    if let Some(o) = rva_to_offset(&sections, r) {
                        exports.push(cstr(data, o, 256));
                    }
                    np += 4;
                }
            }
        }
    }

    // ---- .NET / CLR
    let (clr_rva, clr_size) = dir(14);
    let mut is_dotnet = false;
    let mut dotnet_version = String::new();
    let mut dotnet_strings = Vec::new();
    if clr_rva != 0 && clr_size != 0 {
        if let Some(off) = rva_to_offset(&sections, clr_rva) {
            let md_rva = u32le(data, off + 8).unwrap_or(0);
            if let Some(md) = rva_to_offset(&sections, md_rva) {
                if data.get(md..md + 4) == Some(&b"BSJB"[..]) {
                    is_dotnet = true;
                    let ver_len = u32le(data, md + 12).unwrap_or(0) as usize;
                    dotnet_version = cstr(data, md + 16, ver_len.min(64));
                    // walk streams
                    let mut p = md + 16 + ((ver_len + 3) & !3);
                    let count = u16le(data, p + 2).unwrap_or(0) as usize;
                    p += 4;
                    for _ in 0..count {
                        let s_off = u32le(data, p).unwrap_or(0) as usize;
                        let s_size = u32le(data, p + 4).unwrap_or(0) as usize;
                        let name = cstr(data, p + 8, 32);
                        p += 8 + ((name.len() + 1 + 3) & !3);
                        if name == "#Strings" || name == "#US" {
                            let body = data
                                .get(md + s_off..(md + s_off + s_size).min(data.len()))
                                .unwrap_or(&[]);
                            dotnet_strings.extend(strings_of(body, 4));
                        }
                    }
                    dotnet_strings.truncate(400);
                }
            }
        }
    }

    let (res_rva, _) = dir(2);
    let has_resources = res_rva != 0;

    // ---- strings
    let mut ascii_strings = strings_of(data, 6);
    let mut wide_strings = wide_strings_of(data, 6);
    ascii_strings.truncate(4000);
    wide_strings.truncate(4000);

    // ---- version info (VS_VERSIONINFO in .rsrc, UTF-16 key/value strings)
    let mut version_info = Vec::new();
    let interesting_keys = [
        "CompanyName", "FileDescription", "FileVersion", "InternalName", "LegalCopyright",
        "OriginalFilename", "ProductName", "ProductVersion",
    ];
    for (i, s) in wide_strings.iter().enumerate() {
        if interesting_keys.contains(&s.as_str()) {
            if let Some(v) = wide_strings.get(i + 1) {
                version_info.push((s.clone(), v.clone()));
            }
        }
    }
    version_info.dedup();

    // ---- indicators
    let mut indicators = Vec::new();
    let total_raw: u32 = sections.iter().map(|s| s.raw_size).sum();
    let end_of_sections = sections
        .iter()
        .map(|s| s.raw_pointer + s.raw_size)
        .max()
        .unwrap_or(0) as usize;
    let overlay_size = data.len().saturating_sub(end_of_sections);

    if let Some(upx) = sections.iter().find(|s| s.name.starts_with("UPX")) {
        indicators.push(format!("UPX packer signature in section `{}`", upx.name));
    }
    let high_entropy: Vec<&str> = sections
        .iter()
        .filter(|s| s.raw_size > 512 && s.entropy > 7.2)
        .map(|s| s.name.as_str())
        .collect();
    if !high_entropy.is_empty() {
        indicators.push(format!(
            "high-entropy section(s) {} (>7.2 bits/byte): packed or encrypted payload",
            high_entropy.join(", ")
        ));
    }
    let import_count: usize = imports.values().map(|v| v.len()).sum();
    if import_count > 0 && import_count < 10 && sections.len() <= 3 {
        indicators.push(format!(
            "very small import table ({} functions) with {} sections: typical of a packer stub",
            import_count,
            sections.len()
        ));
    }
    if import_count == 0 {
        indicators.push("no import table at all: unusual for a normal executable".into());
    }
    if let Some(ep_sec) = sections
        .iter()
        .find(|s| entry_point_rva >= s.virtual_address && entry_point_rva < s.virtual_address + s.virtual_size.max(s.raw_size))
    {
        if ep_sec.is_writable() && ep_sec.is_executable() {
            indicators.push(format!(
                "entry point is in a writable+executable section `{}` (common in packers)",
                ep_sec.name
            ));
        }
        let last = sections.last().map(|s| s.name.as_str()).unwrap_or("");
        if ep_sec.name == last && sections.len() > 3 {
            indicators.push(format!("entry point is in the last section `{}`", ep_sec.name));
        }
    }
    if let Some(wx) = sections.iter().find(|s| s.is_writable() && s.is_executable()) {
        indicators.push(format!("W+X section `{}` present", wx.name));
    }
    if is_dotnet {
        indicators.push(".NET assembly: managed code (IL can be read with a metadata viewer)".into());
    }
    if overlay_size > 4096 {
        indicators.push(format!(
            "{} bytes of overlay data after the last section (installer or appended payload)",
            overlay_size
        ));
    }
    if total_raw == 0 {
        indicators.push("no raw section data: header-only or fragmented file".into());
    }
    for s in &sections {
        for needle in ["This program cannot be run in DOS mode", ""] {
            let _ = needle;
        }
        let _ = s;
    }

    Ok(PeInfo {
        is_64,
        machine,
        timestamp,
        subsystem,
        entry_point_rva,
        image_base,
        sections,
        imports,
        exports,
        has_resources,
        is_dotnet,
        dotnet_version,
        dotnet_strings,
        version_info,
        indicators,
        ascii_strings,
        wide_strings,
        overlay_size,
    })
}

pub fn machine_name(m: u16) -> &'static str {
    match m {
        0x014c => "x86 (i386)",
        0x8664 => "x86-64",
        0xaa64 => "ARM64",
        0x01c0 => "ARM",
        0x01c4 => "ARMv7 (Thumb-2)",
        0x0200 => "IA64",
        _ => "unknown",
    }
}

pub fn subsystem_name(s: u16) -> &'static str {
    match s {
        1 => "native (driver)",
        2 => "Windows GUI",
        3 => "Windows console",
        5 => "OS/2 console",
        7 => "POSIX console",
        9 => "Windows CE",
        10 => "EFI application",
        16 => "Windows boot",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sample.exe"),
        )
        .expect("exe fixture missing")
    }

    #[test]
    fn parses_headers_and_sections() {
        let p = parse(&fixture()).expect("pe parse");
        assert!(p.is_64, "fixture is PE32+");
        assert_eq!(machine_name(p.machine), "x86-64");
        assert_eq!(p.sections.len(), 4);
        assert_eq!(p.sections[0].name, ".text");
        assert!(p.sections[0].is_executable());
        assert!(p.sections[2].is_writable());
        let evil = p.sections.iter().find(|s| s.name == ".evil").expect(".evil section");
        assert!(evil.is_executable() && evil.is_writable(), "fixture W+X section");
        assert!(evil.entropy > 7.2, "fixture high-entropy payload: {}", evil.entropy);
        assert_eq!(subsystem_name(p.subsystem), "Windows console");
        assert_eq!(p.image_base, 0x140000000);
    }

    #[test]
    fn extracts_imports() {
        let p = parse(&fixture()).expect("pe parse");
        let total: usize = p.imports.values().map(|v| v.len()).sum();
        assert!(total > 0, "expected an import table, got {:?}", p.imports);
    }

    #[test]
    fn extracts_strings_and_flags_suspicious_sections() {
        let p = parse(&fixture()).expect("pe parse");
        assert!(p.ascii_strings.iter().any(|s| s.contains("CreateFileW") || s.contains("kernel32")));
        // the .data fixture is a repeating pattern -> high entropy in a W+X section
        assert!(p.indicators.iter().any(|i| i.contains("entropy") || i.contains("W+X")),
            "indicators: {:?}", p.indicators);
    }

    #[test]
    fn rejects_non_pe() {
        assert!(parse(b"MZ").is_err());
        assert!(parse(b"not an exe").is_err());
        assert!(parse(&[0u8; 300]).is_err());
    }
}
