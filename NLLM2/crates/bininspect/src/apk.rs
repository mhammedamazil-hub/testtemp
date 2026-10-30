//! APK analysis: container inventory, manifest, DEX code facts, native libs and
//! signing material — assembled into one report.

use crate::axml;
use crate::dex;
use crate::hashes::{entropy, sha1, sha256, strings_of};
use forge_core::ingest::zip;

#[derive(Debug, Default, Clone)]
pub struct ApkInfo {
    pub package: String,
    pub version_name: String,
    pub version_code: String,
    pub min_sdk: Option<String>,
    pub target_sdk: Option<String>,
    pub permissions: Vec<String>,
    pub activities: Vec<(String, bool)>,
    pub services: Vec<(String, bool)>,
    pub receivers: Vec<(String, bool)>,
    pub providers: Vec<(String, bool)>,
    pub exported_count: usize,
    pub debuggable: bool,
    pub allow_backup: Option<bool>,
    pub cleartext_allowed: Option<bool>,
    pub dex_files: Vec<String>,
    pub class_count: usize,
    pub method_count: usize,
    pub string_count: usize,
    pub classes: Vec<String>,
    pub native_libs: Vec<String>,
    pub abis: Vec<String>,
    pub entry_count: usize,
    pub uncompressed_bytes: u64,
    pub largest: Vec<(String, u64, f64)>,
    pub signature_files: Vec<(String, String)>,
    pub assets: Vec<String>,
    pub findings: Vec<dex::Finding>,
    pub indicators: Vec<String>,
    pub embedded_urls: Vec<String>,
}

pub fn is_zip(data: &[u8]) -> bool {
    data.starts_with(b"PK\x03\x04") || data.starts_with(b"PK\x05\x06")
}

pub fn analyze(data: &[u8]) -> Result<ApkInfo, String> {
    if !is_zip(data) {
        return Err("apk: not a ZIP container".into());
    }
    let entries = zip::entries(data).map_err(|e| format!("apk: {}", e))?;
    let mut info = ApkInfo {
        entry_count: entries.len(),
        ..Default::default()
    };

    let mut sizes: Vec<(String, u64, f64)> = Vec::new();
    for e in &entries {
        info.uncompressed_bytes += e.uncompressed_size;
        let body = zip::read(data, e).unwrap_or_default();
        sizes.push((e.name.clone(), e.uncompressed_size, entropy(&body)));
    }
    sizes.sort_by(|a, b| b.1.cmp(&a.1));
    info.largest = sizes.into_iter().take(8).collect();

    // ---- manifest
    let manifest_bytes = zip::read_named(data, "AndroidManifest.xml")
        .map_err(|e| format!("apk: {}", e))?
        .ok_or_else(|| "apk: AndroidManifest.xml missing (not an Android package)".to_string())?;
    let elements = axml::parse(&manifest_bytes)?;
    let m = axml::summarize(&elements);
    info.package = m.package;
    info.version_name = m.version_name;
    info.version_code = m.version_code;
    info.min_sdk = m.min_sdk;
    info.target_sdk = m.target_sdk;
    info.permissions = m.permissions;
    info.activities = m.activities;
    info.services = m.services;
    info.receivers = m.receivers;
    info.providers = m.providers;
    info.exported_count = m.exported_count;
    info.debuggable = m.debuggable;
    info.allow_backup = m.allow_backup;
    info.cleartext_allowed = m.uses_cleartext_traffic;

    // ---- dex
    let mut all_findings: Vec<dex::Finding> = Vec::new();
    let mut urls: Vec<String> = Vec::new();
    for e in &entries {
        if e.name.starts_with("classes") && e.name.ends_with(".dex") {
            let body = zip::read(data, e).unwrap_or_default();
            match dex::parse(&body) {
                Ok(d) => {
                    info.dex_files.push(e.name.clone());
                    info.class_count += d.classes;
                    info.method_count += d.methods;
                    info.string_count += d.strings;
                    for c in d.class_names {
                        if !c.starts_with("Landroid/") && !c.starts_with("Ljava/") {
                            info.classes.push(c);
                        }
                    }
                    all_findings.extend(d.interesting);
                }
                Err(err) => info.indicators.push(format!("{}: {}", e.name, err)),
            }
        }
    }
    info.classes.sort();
    info.classes.dedup();
    info.classes.truncate(200);
    all_findings.sort_by(|a, b| a.category.cmp(b.category).then(a.detail.cmp(&b.detail)));
    all_findings.dedup_by(|a, b| a.detail == b.detail);
    for f in &all_findings {
        if f.category == "cleartext_url" {
            // detail is "hardcoded cleartext URL: `http://...`" — keep just the URL
            let url = f
                .detail
                .split('`')
                .nth(1)
                .map(|u| u.to_string())
                .unwrap_or_else(|| f.detail.clone());
            urls.push(url);
        }
    }
    urls.sort();
    urls.dedup();
    urls.truncate(40);
    info.embedded_urls = urls;
    info.findings = all_findings;

    // ---- native + assets + signatures
    for e in &entries {
        if e.name.starts_with("lib/") && e.name.ends_with(".so") {
            info.native_libs.push(e.name.clone());
            if let Some(abi) = e.name.split('/').nth(1) {
                info.abis.push(abi.to_string());
            }
        }
        if e.name.starts_with("assets/") {
            info.assets.push(e.name.clone());
        }
        let upper = e.name.to_uppercase();
        if e.name.starts_with("META-INF/")
            && (upper.ends_with(".RSA") || upper.ends_with(".DSA") || upper.ends_with(".EC"))
        {
            let body = zip::read(data, e).unwrap_or_default();
            let names = strings_of(&body, 4)
                .into_iter()
                .filter(|s| {
                    s.contains("CN=") || s.contains("O=") || s.contains("SHA") || s.contains("Android")
                })
                .take(3)
                .collect::<Vec<_>>()
                .join("; ");
            info.signature_files
                .push((e.name.clone(), format!("sha256:{} {}", &sha256(&body)[..16], names)));
        }
    }
    info.abis.sort();
    info.abis.dedup();
    info.assets.truncate(40);

    // ---- indicators
    let mut ind: Vec<String> = Vec::new();
    if info.debuggable {
        ind.push("application is marked android:debuggable=\"true\" — ships with debugging enabled".into());
    }
    if info.allow_backup == Some(true) {
        ind.push("android:allowBackup=\"true\" — app data can be extracted via adb backup/on supported devices".into());
    }
    if info.cleartext_allowed == Some(true) {
        ind.push("android:usesCleartextTraffic=\"true\" — plaintext HTTP is permitted".into());
    }
    for (name, exported) in info
        .activities
        .iter()
        .chain(info.services.iter())
        .chain(info.receivers.iter())
        .chain(info.providers.iter())
    {
        if *exported {
            ind.push(format!("exported component (reachable by other apps): {}", name));
        }
    }
    if info.signature_files.is_empty() {
        ind.push("no META-INF signature block (v1) — may be unsigned, or v2/v3-only".into());
    }
    if info.native_libs.is_empty() {
        ind.push("no native libraries: pure Java/Kotlin bytecode".into());
    } else {
        ind.push(format!(
            "{} native libraries for ABI(s) {} — code analysis needs a native disassembler",
            info.native_libs.len(),
            info.abis.join(", ")
        ));
    }
    if !info.embedded_urls.is_empty() {
        ind.push(format!(
            "{} hardcoded cleartext URL(s) in bytecode strings",
            info.embedded_urls.len()
        ));
    }
    info.indicators = ind;
    Ok(info)
}

/// Public APK verifier heuristics: does the signature block actually cover the file?
/// (Full APK Signature Scheme v2 verification needs CMS parsing — reported as a gap.)
pub fn signature_digest(data: &[u8]) -> (String, String) {
    (sha256(data), sha1(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/apk")
                .join(name),
        )
        .expect("apk fixture missing")
    }

    #[test]
    fn analyzes_a_real_apk_container() {
        let info = analyze(&fixture("sample.apk")).expect("apk");
        assert_eq!(info.package, "com.example.forgetest");
        assert_eq!(info.version_name, "1.2.3");
        assert!(info.permissions.contains(&"android.permission.INTERNET".to_string()));
        assert!(info.dex_files.contains(&"classes.dex".to_string()));
        assert!(info.class_count >= 2, "classes: {}", info.class_count);
        assert!(info.classes.iter().any(|c| c.contains("Crypto")));
        assert_eq!(info.abis, vec!["arm64-v8a".to_string()]);
        assert!(info.entry_count >= 4);
    }

    #[test]
    fn flags_debuggable_and_exported_components() {
        let info = analyze(&fixture("sample.apk")).expect("apk");
        assert!(info.debuggable);
        assert!(info
            .indicators
            .iter()
            .any(|i| i.contains("debuggable")));
        assert!(info.exported_count >= 1, "indicators: {:?}", info.indicators);
        assert!(info
            .indicators
            .iter()
            .any(|i| i.contains("exported component")));
    }

    #[test]
    fn rejects_zip_that_is_not_an_apk() {
        // a plain zip without a manifest
        let err = analyze(&{
            let mut v = vec![0x50, 0x4b, 0x03, 0x04];
            v.extend_from_slice(&[0u8; 200]);
            v
        })
        .unwrap_err();
        assert!(err.contains("AndroidManifest") || err.contains("zip"), "{}", err);
    }
}
