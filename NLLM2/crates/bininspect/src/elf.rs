//! Minimal ELF reader — enough to identify an architecture, entry point and
//! section layout for `.so` files found inside APKs (or standalone Linux binaries).

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

#[derive(Debug, Default, Clone)]
pub struct ElfInfo {
    pub is_64: bool,
    pub little_endian: bool,
    pub object_type: &'static str,
    pub machine: &'static str,
    pub entry: u64,
    pub sections: Vec<String>,
    /// True when the symbol table is stripped.
    pub stripped: bool,
}

pub fn is_elf(data: &[u8]) -> bool {
    data.starts_with(b"\x7fELF")
}

pub fn parse(data: &[u8]) -> Result<ElfInfo, String> {
    if data.len() < 52 || !is_elf(data) {
        return Err("elf: not an ELF file (or truncated header)".into());
    }
    let class = data[4];
    let is_64 = match class {
        2 => true,
        1 => false,
        _ => return Err("elf: unknown class".into()),
    };
    let little_endian = data[5] == 1;
    if !little_endian {
        return Ok(ElfInfo {
            is_64,
            little_endian,
            object_type: "big-endian ELF (sections not parsed)",
            machine: "unknown",
            entry: 0,
            sections: Vec::new(),
            stripped: false,
        });
    }
    let object_type = match u16le(data, 16).unwrap_or(0) {
        1 => "relocatable",
        2 => "executable",
        3 => "shared object (.so)",
        4 => "core dump",
        _ => "unknown",
    };
    let machine = match u16le(data, 18).unwrap_or(0) {
        0x3e => "x86-64",
        0x03 => "x86",
        0xb7 => "ARM64 (AArch64)",
        0x28 => "ARM",
        0xf3 => "RISC-V",
        0x08 => "MIPS",
        _ => "unknown",
    };
    let entry = if is_64 {
        u64le(data, 24).unwrap_or(0)
    } else {
        u32le(data, 24).unwrap_or(0) as u64
    };

    // section headers
    let (shoff, shentsize, shnum, shstrndx) = if is_64 {
        (
            u64le(data, 0x28).unwrap_or(0) as usize,
            u16le(data, 0x3A).unwrap_or(0) as usize,
            u16le(data, 0x3C).unwrap_or(0) as usize,
            u16le(data, 0x3E).unwrap_or(0) as usize,
        )
    } else {
        (
            u32le(data, 0x20).unwrap_or(0) as usize,
            u16le(data, 0x2E).unwrap_or(0) as usize,
            u16le(data, 0x30).unwrap_or(0) as usize,
            u16le(data, 0x32).unwrap_or(0) as usize,
        )
    };

    let mut sections = Vec::new();
    let mut stripped = true;
    if shoff != 0 && shnum > 0 && shoff < data.len() {
        // string table
        let strtab_off = |idx: usize| -> Option<usize> {
            let at = shoff + idx * shentsize;
            Some(if is_64 {
                u64le(data, at + 0x18)? as usize
            } else {
                u32le(data, at + 0x10)? as usize
            })
        };
        let names_base = strtab_off(shstrndx).unwrap_or(0);
        for i in 0..shnum {
            let at = shoff + i * shentsize;
            let name_off = u32le(data, at).unwrap_or(0) as usize;
            let typ = u32le(data, at + 4).unwrap_or(0);
            let name = {
                let start = names_base + name_off;
                let mut end = start;
                while let Some(&c) = data.get(end) {
                    if c == 0 {
                        break;
                    }
                    end += 1;
                }
                String::from_utf8_lossy(data.get(start..end).unwrap_or(&[])).to_string()
            };
            if typ == 2 || typ == 11 {
                stripped = false; // SYMTAB or DYNSYM
            }
            if !name.is_empty() {
                sections.push(name);
            }
        }
    }
    sections.truncate(60);
    Ok(ElfInfo {
        is_64,
        little_endian,
        object_type,
        machine,
        entry,
        sections,
        stripped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_a_real_linux_binary() {
        // /bin/sh is an ELF on this image; skip gracefully if not.
        if let Ok(data) = std::fs::read("/bin/sh") {
            if is_elf(&data) {
                let e = parse(&data).expect("elf");
                assert!(e.is_64);
                assert!(matches!(e.object_type, "executable" | "shared object (.so)"));
                assert!(!e.sections.is_empty());
                return;
            }
        }
        // fallback: header-only synthetic file
        let mut buf = vec![0u8; 64];
        buf[0..4].copy_from_slice(b"\x7fELF");
        buf[4] = 2;
        buf[5] = 1;
        buf[16] = 3;
        buf[18] = 0x3e;
        let e = parse(&buf).expect("elf header");
        assert!(e.is_64);
        assert_eq!(e.machine, "x86-64");
    }

    #[test]
    fn rejects_non_elf() {
        assert!(parse(b"MZ\x90\x00").is_err());
        assert!(!is_elf(b"PK\x03\x04"));
    }
}
