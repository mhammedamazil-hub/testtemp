//! Unified report: format detection + per-format analysis, emitted as JSON.
//!
//! Lives in the library (not the binary) so the HTTP server can call it directly.

use crate::{apk, elf, hashes, pe};
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

    let format;
    if pe::parse(data).is_ok() || data.starts_with(b"MZ") {
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

fn set(v: &mut Value, key: &str, value: Value) {
    if let Value::Obj(ref mut m) = v {
        m.insert(key.to_string(), value);
    }
}

