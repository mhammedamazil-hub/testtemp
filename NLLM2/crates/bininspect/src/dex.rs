//! Dalvik executable (`classes*.dex`) structural parser.
//!
//! Not a decompiler: it recovers the class/method inventory, the string table and
//! the *references* a class makes (which is what tells you whether an app talks to
//! a network, uses crypto, loads code dynamically, or checks for root).

use std::collections::BTreeSet;

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

fn uleb128(data: &[u8], at: usize) -> Option<(u32, usize)> {
    let mut result: u32 = 0;
    let mut shift = 0;
    let mut p = at;
    loop {
        let byte = *data.get(p)?;
        p += 1;
        result |= ((byte & 0x7f) as u32) << shift;
        if byte & 0x80 == 0 {
            return Some((result, p));
        }
        shift += 7;
        if shift > 28 {
            return None;
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct DexInfo {
    pub file_size: u32,
    pub checksum_ok: Option<bool>,
    pub strings: usize,
    pub types: usize,
    pub methods: usize,
    pub classes: usize,
    pub class_names: Vec<String>,
    pub method_names: Vec<String>,
    pub interesting: Vec<Finding>,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub category: &'static str,
    pub detail: String,
}

/// MUTF-8 to String (NUL encoded as 0xC0 0x80, surrogate pairs as CESU-8).
fn mutf8_to_string(raw: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        if b == 0 {
            break;
        }
        if b < 0x80 {
            out.push(b as char);
            i += 1;
        } else if b & 0xE0 == 0xC0 {
            if i + 1 >= raw.len() {
                break;
            }
            if b == 0xC0 && raw[i + 1] == 0x80 {
                out.push('\u{0}');
            } else {
                let cp = ((b as u32 & 0x1f) << 6) | (raw[i + 1] as u32 & 0x3f);
                out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
            }
            i += 2;
        } else if b & 0xF0 == 0xE0 {
            if i + 2 >= raw.len() {
                break;
            }
            let cp = ((b as u32 & 0x0f) << 12)
                | ((raw[i + 1] as u32 & 0x3f) << 6)
                | (raw[i + 2] as u32 & 0x3f);
            out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
            i += 3;
        } else if b & 0xF8 == 0xF0 {
            if i + 3 >= raw.len() {
                break;
            }
            let cp = ((b as u32 & 0x07) << 18)
                | ((raw[i + 1] as u32 & 0x3f) << 12)
                | ((raw[i + 2] as u32 & 0x3f) << 6)
                | (raw[i + 3] as u32 & 0x3f);
            out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
            i += 4;
        } else {
            i += 1;
        }
    }
    out
}

fn read_uleb_string(data: &[u8], at: usize) -> Option<String> {
    let (_len, p) = uleb128(data, at)?;
    let mut end = p;
    while *data.get(end)? != 0 {
        end += 1;
    }
    Some(mutf8_to_string(&data[p..end]))
}

const CRYPTO: &[&str] = &[
    "javax/crypto", "java/security", "Cipher", "SecretKey", "AES", "RSA", "DES",
    "MessageDigest", "KeyStore", "KeyGenerator",
];
const NETWORK: &[&str] = &[
    "java/net/Socket", "HttpURLConnection", "okhttp3", "java/net/URL", "retrofit2",
    "WebView", "loadUrl",
];
const DYNAMIC_CODE: &[&str] = &[
    "DexClassLoader", "PathClassLoader", "InMemoryDexClassLoader", "defineClass",
    "java/lang/reflect", "Class/forName", "Runtime/exec", "ProcessBuilder",
];
const ROOT_HINTS: &[&str] = &[
    "/system/xbin/su", "/system/bin/su", "busybox", "Superuser", "magisk", "test-keys",
    "/proc/mounts", "ro.debuggable",
];
const EMULATOR_HINTS: &[&str] = &["goldfish", "generic_x86", "ranchu", "Emulator", "qemu"];

pub fn parse(data: &[u8]) -> Result<DexInfo, String> {
    if data.len() < 0x70 {
        return Err("dex: file too small".into());
    }
    if &data[0..4] != b"dex\n" {
        return Err("dex: bad magic (not a Dalvik executable)".into());
    }
    let version = String::from_utf8_lossy(&data[4..7]).to_string();
    let file_size = u32le(data, 32).ok_or("dex: truncated header")?;
    let string_ids_size = u32le(data, 56).ok_or("dex: truncated")? as usize;
    let string_ids_off = u32le(data, 60).ok_or("dex: truncated")? as usize;
    let type_ids_size = u32le(data, 64).ok_or("dex: truncated")? as usize;
    let type_ids_off = u32le(data, 68).ok_or("dex: truncated")? as usize;
    let method_ids_size = u32le(data, 88).ok_or("dex: truncated")? as usize;
    let method_ids_off = u32le(data, 92).ok_or("dex: truncated")? as usize;
    let class_defs_size = u32le(data, 96).ok_or("dex: truncated")? as usize;
    let class_defs_off = u32le(data, 100).ok_or("dex: truncated")? as usize;

    // Strings
    let mut strings: Vec<String> = Vec::with_capacity(string_ids_size.min(1 << 20));
    for i in 0..string_ids_size {
        let off = u32le(data, string_ids_off + i * 4)
            .ok_or("dex: string id table truncated")? as usize;
        strings.push(read_uleb_string(data, off).unwrap_or_default());
    }

    // Types -> descriptor strings
    let mut types: Vec<String> = Vec::with_capacity(type_ids_size.min(1 << 20));
    for i in 0..type_ids_size {
        let idx = u32le(data, type_ids_off + i * 4).ok_or("dex: type id table truncated")? as usize;
        types.push(strings.get(idx).cloned().unwrap_or_default());
    }

    // Methods
    let mut method_names: Vec<String> = Vec::new();
    let mut method_refs: BTreeSet<String> = BTreeSet::new();
    for i in 0..method_ids_size {
        let at = method_ids_off + i * 8;
        let class_idx = u16le(data, at).ok_or("dex: method table truncated")? as usize;
        let name_idx = u32le(data, at + 4).ok_or("dex: method table truncated")? as usize;
        let cls = types.get(class_idx).cloned().unwrap_or_default();
        let name = strings.get(name_idx).cloned().unwrap_or_default();
        method_refs.insert(format!("{}->{}", cls, name));
        method_names.push(name);
    }

    // Classes (through class_data_item for accuracy)
    let mut class_names: Vec<String> = Vec::new();
    for i in 0..class_defs_size {
        let at = class_defs_off + i * 32;
        let class_idx = u32le(data, at).ok_or("dex: class defs truncated")? as usize;
        let class_data_off = u32le(data, at + 24).ok_or("dex: class defs truncated")? as usize;
        let desc = types.get(class_idx).cloned().unwrap_or_default();
        class_names.push(desc.clone());
        if class_data_off == 0 || class_data_off >= data.len() {
            continue;
        }
        let mut p = class_data_off;
        let Some((sf, np)) = uleb128(data, p) else { continue };
        p = np;
        let Some((inf, np)) = uleb128(data, p) else { continue };
        p = np;
        let Some((dm, np)) = uleb128(data, p) else { continue };
        p = np;
        let Some((vm, _np)) = uleb128(data, p) else { continue };
        // skip fields
        let mut q = p;
        for _ in 0..(sf + inf) {
            let Some((_, n1)) = uleb128(data, q) else { break };
            let Some((_, n2)) = uleb128(data, n1) else { break };
            q = n2;
        }
        for _ in 0..(dm + vm) {
            let Some((_midx, n1)) = uleb128(data, q) else { break };
            let Some((_acc, n2)) = uleb128(data, n1) else { break };
            let Some((_code, n3)) = uleb128(data, n2) else { break };
            q = n3;
        }
    }

    // Findings from the reference graph
    let mut interesting: Vec<Finding> = Vec::new();
    let all_refs = method_refs.iter().cloned().collect::<Vec<_>>().join("\n");
    let mut add_hits = |cat: &'static str, needles: &[&str], label: &str| {
        let hits: Vec<&str> = needles
            .iter()
            .copied()
            .filter(|n| all_refs.contains(n))
            .collect();
        if !hits.is_empty() {
            interesting.push(Finding {
                category: cat,
                detail: format!("{}: {}", label, hits.join(", ")),
            });
        }
    };
    add_hits("crypto", CRYPTO, "crypto APIs referenced");
    add_hits("network", NETWORK, "network APIs referenced");
    add_hits("dynamic_code", DYNAMIC_CODE, "dynamic/reflective code APIs referenced");

    for s in &strings {
        for hint in ROOT_HINTS {
            if s.contains(hint) {
                interesting.push(Finding {
                    category: "root_detection",
                    detail: format!("string looks like a root/emulator check: `{}`", truncate(s, 80)),
                });
            }
        }
        if s.starts_with("http://") && !s.contains("schemas.android.com") && !s.contains("xml.org") {
            interesting.push(Finding {
                category: "cleartext_url",
                detail: format!("hardcoded cleartext URL: `{}`", truncate(s, 120)),
            });
        }
    }
    let _ = EMULATOR_HINTS;

    interesting.sort_by(|a, b| a.category.cmp(b.category).then(a.detail.cmp(&b.detail)));
    interesting.dedup_by(|a, b| a.detail == b.detail);
    interesting.truncate(60);

    Ok(DexInfo {
        file_size,
        checksum_ok: None,
        strings: strings.len(),
        types: types.len(),
        methods: method_ids_size,
        classes: class_defs_size,
        class_names,
        method_names,
        interesting,
        version,
    })
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n).collect();
        format!("{}…", t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/apk/classes.dex"),
        )
        .expect("deck fixture missing")
    }

    #[test]
    fn parses_dex_inventory() {
        let d = parse(&fixture()).expect("dex parse");
        assert_eq!(d.version, "035");
        assert!(d.classes >= 3, "classes: {}", d.classes);
        assert!(d.class_names.iter().any(|c| c.contains("MainActivity")));
        assert!(d.strings > 10);
        assert!(d.methods >= 5);
    }

    #[test]
    fn detects_crypto_and_network_references() {
        let d = parse(&fixture()).expect("dex parse");
        let cats: Vec<&str> = d.interesting.iter().map(|f| f.category).collect();
        assert!(cats.contains(&"crypto"), "expected crypto refs, got {:?}", d.interesting);
        assert!(cats.contains(&"network"), "expected network refs, got {:?}", d.interesting);
        assert!(cats.contains(&"cleartext_url"), "expected a cleartext URL finding");
    }

    #[test]
    fn rejects_non_dex() {
        assert!(parse(b"not a dex file at all").is_err());
        assert!(parse(&[0u8; 200]).is_err());
    }

    #[test]
    fn uleb128_decodes_multibyte() {
        // 0xE5 0x8E 0x26 == 624485
        let d = [0xE5u8, 0x8E, 0x26];
        assert_eq!(uleb128(&d, 0), Some((624485, 3)));
    }
}
