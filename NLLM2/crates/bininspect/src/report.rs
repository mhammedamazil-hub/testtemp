//! Unified report: format detection + per-format analysis, emitted as JSON.
//!
//! Lives in the library (not the binary) so the HTTP server can call it directly.

use crate::{apk, axml, elf, hashes, pe, xmltext};
use forge_core::json::Value;
use forge_core::obj;
use std::path::Path;

/// Detect and analyse. Returns a JSON report in every case, including failure.
pub fn analyze(path: &str, data: &[u8], string_limit: usize) -> Value {
    let sha256 = hashes::sha256(data);
    let sha1 = hashes::sha1(data);
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());

    let mut base = obj! {
        "file" => name.as_str(),
        "bytes" => data.len() as u64,
        "sha256" => sha256.as_str(),
        "sha1" => sha1.as_str(),
        "entropy" => (hashes::entropy(data) * 1000.0).round() / 1000.0,
    };

    // Git LFS pointers are 130-byte text stubs, not binaries. Saying so beats
    // reporting "unknown" and letting someone think they analysed the file.
    if let Some(lfs) = lfs_pointer(data) {
        set(&mut base, "format", Value::from("git_lfs_pointer"));
        set(&mut base, "lfs", obj! {
            "oid" => lfs.0.as_str(),
            "declared_size" => lfs.1 as u64,
            "note" => "this file is a Git LFS pointer; the real binary was never downloaded",
        });
        return base;
    }

    let format;
    if is_binary_xml(data) || xmltext::is_text_manifest(data) {
        // A manifest on its own, compiled (AXML) or textual (build intermediates).
        format = if is_binary_xml(data) { "android_binary_xml" } else { "android_text_manifest" };
        let parsed = if is_binary_xml(data) {
            axml::parse(data)
        } else {
            xmltext::parse(data)
        };
        match parsed.map(|els| axml::summarize(&els)) {
            Ok(m) => {
                let comp = |list: &Vec<(String, bool)>| -> Value {
                    Value::Arr(list.iter().map(|(n, e)| obj! { "name" => n.as_str(), "exported" => *e }).collect())
                };
                set(&mut base, "manifest", obj! {
                    "package" => m.package.as_str(),
                    "version_name" => m.version_name.as_str(),
                    "version_code" => m.version_code.as_str(),
                    "min_sdk" => m.min_sdk.clone().unwrap_or_default(),
                    "target_sdk" => m.target_sdk.clone().unwrap_or_default(),
                    "debuggable" => m.debuggable,
                    "exported_components" => m.exported_count as u64,
                    "permissions" => Value::Arr(m.permissions.iter().map(|p| Value::from(p.as_str())).collect()),
                    "activities" => comp(&m.activities),
                    "services" => comp(&m.services),
                    "receivers" => comp(&m.receivers),
                    "providers" => comp(&m.providers),
                    "launcher_activity" => m.launcher_activity.unwrap_or_default(),
                });
            }
            Err(e) => set(&mut base, "error", Value::from(e)),
        }
    } else if data.starts_with(b"dex\n") {
        // A bare classes.dex, e.g. extracted from an APK.
        format = "dex";
        match crate::dex::parse(data) {
            Ok(d) => {
                let mut findings: Vec<Value> = Vec::new();
                for f in &d.interesting {
                    findings.push(obj! { "category" => f.category, "detail" => f.detail.as_str() });
                }
                set(&mut base, "dex", obj! {
                    "version" => d.version.as_str(),
                    "declared_size" => d.file_size as u64,
                    "strings" => d.strings as u64,
                    "types" => d.types as u64,
                    "methods" => d.methods as u64,
                    "classes" => d.classes as u64,
                    "class_names" => Value::Arr(d.class_names.iter().take(200).map(|c| Value::from(c.as_str())).collect()),
                    "code_findings" => Value::Arr(findings),
                });
            }
            Err(e) => set(&mut base, "error", Value::from(e)),
        }
    } else if pe::parse(data).is_ok() || data.starts_with(b"MZ") {
        format = "pe";
        match pe::parse(data) {
            Ok(p) => {
                let mut imports: Vec<Value> = Vec::new();
                for (dll, funcs) in &p.imports {
                    imports.push(obj! {
                        "dll" => dll.as_str(),
                        "functions" => Value::Arr(funcs.iter().map(|f| Value::from(f.as_str())).collect()),
                    });
                }
                let mut secs: Vec<Value> = Vec::new();
                for s in &p.sections {
                    secs.push(obj! {
                        "name" => s.name.as_str(),
                        "vaddr" => format!("0x{:x}", s.virtual_address),
                        "raw_size" => s.raw_size as u64,
                        "flags" => s.flags(),
                        "entropy" => (s.entropy * 100.0).round() / 100.0,
                    });
                }
                let mut vers: Vec<Value> = Vec::new();
                for (k, v) in &p.version_info {
                    vers.push(obj! { "key" => k.as_str(), "value" => v.as_str() });
                }
                set(&mut base, "pe", obj! {
                    "bits" => if p.is_64 { 64u64 } else { 32u64 },
                    "machine" => pe::machine_name(p.machine),
                    "subsystem" => pe::subsystem_name(p.subsystem),
                    "entry_point_rva" => format!("0x{:x}", p.entry_point_rva),
                    "image_base" => format!("0x{:x}", p.image_base),
                    "sections" => Value::Arr(secs),
                    "imports" => Value::Arr(imports),
                    "exports" => Value::Arr(p.exports.iter().map(|e| Value::from(e.as_str())).collect()),
                    "dotnet" => p.is_dotnet,
                    "dotnet_version" => p.dotnet_version.as_str(),
                    "has_resources" => p.has_resources,
                    "overlay_bytes" => p.overlay_size as u64,
                    "version_info" => Value::Arr(vers),
                    "indicators" => Value::Arr(p.indicators.iter().map(|i| Value::from(i.as_str())).collect()),
                    "ascii_strings" => Value::Arr(p.ascii_strings.iter().take(string_limit).map(|s| Value::from(s.as_str())).collect()),
                    "wide_strings" => Value::Arr(p.wide_strings.iter().take(string_limit).map(|s| Value::from(s.as_str())).collect()),
                });
            }
            Err(e) => set(&mut base, "error", Value::from(e)),
        }
    } else if apk::is_zip(data) {
        format = "apk_or_zip";
        match apk::analyze(data) {
            Ok(a) => {
                let mut perms: Vec<Value> = a.permissions.iter().map(|p| Value::from(p.as_str())).collect();
                perms.sort_by_key(|v| v.as_str().unwrap_or("").to_string());
                let comp = |list: &Vec<(String, bool)>| -> Value {
                    Value::Arr(
                        list.iter()
                            .map(|(n, e)| obj! { "name" => n.as_str(), "exported" => *e })
                            .collect(),
                    )
                };
                let mut findings: Vec<Value> = Vec::new();
                for f in &a.findings {
                    findings.push(obj! { "category" => f.category, "detail" => f.detail.as_str() });
                }
                let mut largest: Vec<Value> = Vec::new();
                for (n, sz, ent) in &a.largest {
                    largest.push(obj! {
                        "entry" => n.as_str(),
                        "bytes" => *sz,
                        "entropy" => (ent * 100.0).round() / 100.0,
                    });
                }
                let mut sigs: Vec<Value> = Vec::new();
                for (n, d) in &a.signature_files {
                    sigs.push(obj! { "file" => n.as_str(), "detail" => d.as_str() });
                }
                set(&mut base, "apk", obj! {
                    "package" => a.package.as_str(),
                    "version_name" => a.version_name.as_str(),
                    "version_code" => a.version_code.as_str(),
                    "min_sdk" => a.min_sdk.clone().unwrap_or_default(),
                    "target_sdk" => a.target_sdk.clone().unwrap_or_default(),
                    "debuggable" => a.debuggable,
                    "allow_backup" => a.allow_backup.unwrap_or(false),
                    "cleartext_traffic" => a.cleartext_allowed.unwrap_or(false),
                    "permissions" => Value::Arr(perms),
                    "activities" => comp(&a.activities),
                    "services" => comp(&a.services),
                    "receivers" => comp(&a.receivers),
                    "providers" => comp(&a.providers),
                    "exported_components" => a.exported_count as u64,
                    "dex_files" => Value::Arr(a.dex_files.iter().map(|s| Value::from(s.as_str())).collect()),
                    "classes" => a.class_count as u64,
                    "methods" => a.method_count as u64,
                    "strings" => a.string_count as u64,
                    "class_names" => Value::Arr(a.classes.iter().map(|s| Value::from(s.as_str())).collect()),
                    "native_libs" => Value::Arr(a.native_libs.iter().map(|s| Value::from(s.as_str())).collect()),
                    "abis" => Value::Arr(a.abis.iter().map(|s| Value::from(s.as_str())).collect()),
                    "entries" => a.entry_count as u64,
                    "uncompressed_bytes" => a.uncompressed_bytes,
                    "largest_entries" => Value::Arr(largest),
                    "signatures" => Value::Arr(sigs),
                    "assets" => Value::Arr(a.assets.iter().map(|s| Value::from(s.as_str())).collect()),
                    "code_findings" => Value::Arr(findings),
                    "indicators" => Value::Arr(a.indicators.iter().map(|s| Value::from(s.as_str())).collect()),
                    "embedded_urls" => Value::Arr(a.embedded_urls.iter().map(|s| Value::from(s.as_str())).collect()),
                });
            }
            Err(e) => set(&mut base, "error", Value::from(e)),
        }
    } else if elf::is_elf(data) {
        format = "elf";
        match elf::parse(data) {
            Ok(e) => set(&mut base, "elf", obj! {
                "bits" => if e.is_64 { 64u64 } else { 32u64 },
                "machine" => e.machine,
                "type" => e.object_type,
                "entry" => format!("0x{:x}", e.entry),
                "stripped" => e.stripped,
                "sections" => Value::Arr(e.sections.iter().map(|s| Value::from(s.as_str())).collect()),
            }),
            Err(err) => set(&mut base, "error", Value::from(err)),
        }
    } else {
        format = "unknown";
    }
    set(&mut base, "format", Value::from(format));
    base
}

/// Parse a Git LFS pointer: `oid sha256:...\nsize N\n`. Returns (oid, size).
pub fn lfs_pointer(data: &[u8]) -> Option<(String, u64)> {
    if data.len() > 1024 || !data.starts_with(b"version https://git-lfs") {
        return None;
    }
    let text = String::from_utf8_lossy(data);
    let mut oid = String::new();
    let mut size = 0u64;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("oid ") {
            oid = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("size ") {
            size = v.trim().parse().unwrap_or(0);
        }
    }
    if oid.is_empty() {
        None
    } else {
        Some((oid, size))
    }
}

/// Android binary XML starts with a `RES_XML` chunk header (type 0x0003, size 8).
pub fn is_binary_xml(data: &[u8]) -> bool {
    data.len() > 8 && data[0] == 0x03 && data[1] == 0x00 && data[2] == 0x08 && data[3] == 0x00
}

fn set(v: &mut Value, key: &str, value: Value) {
    if let Value::Obj(ref mut m) = v {
        m.insert(key.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_a_bare_dex_file() {
        let data = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/apk/classes.dex"),
        )
        .expect("dex fixture");
        let r = analyze("classes.dex", &data, 20);
        assert_eq!(r.str_of("format"), Some("dex"));
        assert_eq!(r.get("dex").and_then(|d| d.str_of("version")), Some("035"));
        assert!(r.get("dex").and_then(|d| d.get("classes")).and_then(|c| c.as_f64()).unwrap_or(0.0) >= 3.0);
    }

    #[test]
    fn recognises_a_git_lfs_pointer() {
        let stub = b"version https://git-lfs.github.com/spec/v1\noid sha256:ce06c531ab5beebfd915ea74a7c08956340861946c8a104993ba633f2cbcfd54\nsize 6656\n";
        let r = analyze("tool.exe", stub, 20);
        assert_eq!(r.str_of("format"), Some("git_lfs_pointer"));
        assert_eq!(r.get("lfs").and_then(|l| l.get("declared_size")).and_then(|s| s.as_f64()), Some(6656.0));
        assert!(r.get("lfs").and_then(|l| l.str_of("note")).unwrap_or("").contains("LFS"));
    }

    #[test]
    fn truncated_pe_reports_an_error_not_a_panic() {
        let r = analyze("bad.exe", b"MZ\x90\x00\x03\x00\x00\x00", 20);
        assert_eq!(r.str_of("format"), Some("pe"));
        assert!(r.str_of("error").is_some());
    }

    #[test]
    fn detects_a_standalone_binary_manifest() {
        let data = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/apk/AndroidManifest.xml"),
        )
        .expect("manifest fixture");
        assert!(is_binary_xml(&data));
        let r = analyze("AndroidManifest.xml", &data, 20);
        assert_eq!(r.str_of("format"), Some("android_binary_xml"));
        let m = r.get("manifest").expect("manifest section");
        assert_eq!(m.str_of("package"), Some("com.example.forgetest"));
        assert_eq!(m.str_of("launcher_activity"), Some("com.example.forgetest.MainActivity"));
        assert!(m.get("exported_components").and_then(|x| x.as_f64()).unwrap_or(0.0) >= 1.0);
    }

    #[test]
    fn empty_file_is_unknown_not_a_crash() {
        let r = analyze("empty.bin", b"", 20);
        assert_eq!(r.str_of("format"), Some("unknown"));
        assert_eq!(r.get("bytes").and_then(|b| b.as_f64()), Some(0.0));
    }
}
