//! Android binary XML (`AndroidManifest.xml` inside an APK) parser.
//!
//! Android ships the manifest compiled into a chunked binary format, not text.
//! Structure: file header, a string pool, then a stream of elements with typed
//! attribute values. Reading it properly is the difference between "this is an
//! APK" and "this app declares package X, needs the INTERNET permission, and
//! exports activity Y".

#[derive(Debug, Clone, PartialEq)]
pub struct Attr {
    pub ns: Option<String>,
    pub name: String,
    /// Raw string form when present.
    pub raw: Option<String>,
    /// Decoded typed value as text.
    pub value: String,
    pub type_id: u8,
}

#[derive(Debug, Clone)]
pub struct Element {
    pub name: String,
    pub attributes: Vec<Attr>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&Attr> {
        self.attributes.iter().find(|a| a.name == name)
    }
    pub fn value(&self, name: &str) -> Option<&str> {
        self.attr(name).map(|a| a.value.as_str())
    }
}

const RES_STRING_POOL: u16 = 0x0001;
const RES_XML: u16 = 0x0003;
const RES_XML_START_ELEMENT: u16 = 0x0102;

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

struct Pool {
    strings: Vec<String>,
}

fn read_pool(data: &[u8], at: usize) -> Result<Pool, String> {
    let typ = u16le(data, at).ok_or("pool: truncated")?;
    if typ != RES_STRING_POOL {
        return Err(format!("expected string pool, found chunk type 0x{:04x}", typ));
    }
    let header_size = u16le(data, at + 2).ok_or("pool: truncated")? as usize;
    let count = u32le(data, at + 8).ok_or("pool: truncated")? as usize;
    let flags = u32le(data, at + 16).ok_or("pool: truncated")?;
    let strings_start = u32le(data, at + 20).ok_or("pool: truncated")? as usize;
    let utf8 = flags & 0x100 != 0; // UTF8_FLAG
    let offsets_at = at + header_size;
    let base = at + strings_start;

    let mut strings = Vec::with_capacity(count);
    for i in 0..count {
        let off = u32le(data, offsets_at + i * 4).ok_or("pool: offset table truncated")? as usize;
        let p = base + off;
        if p >= data.len() {
            strings.push(String::new());
            continue;
        }
        if utf8 {
            // u8 utf16 len (with high bit = 2-byte length), then u8 byte len, then bytes
            let (mut q, _) = read_len8(data, p);
            let (mut r, blen) = read_len8(data, q);
            q = r;
            r = q + blen;
            let raw = data.get(q..r.min(data.len())).unwrap_or(&[]);
            strings.push(String::from_utf8_lossy(raw).to_string());
        } else {
            let (q, len16) = read_len16(data, p);
            let start = q;
            let end = start + len16 * 2;
            let raw = data.get(start..end.min(data.len())).unwrap_or(&[]);
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            strings.push(String::from_utf16_lossy(&units));
        }
    }
    Ok(Pool { strings })
}

fn read_len8(data: &[u8], at: usize) -> (usize, usize) {
    let a = data.get(at).copied().unwrap_or(0);
    if a & 0x80 != 0 {
        let b = data.get(at + 1).copied().unwrap_or(0);
        (at + 2, ((a as usize & 0x7f) << 8) | b as usize)
    } else {
        (at + 1, a as usize)
    }
}

fn read_len16(data: &[u8], at: usize) -> (usize, usize) {
    let a = u16le(data, at).unwrap_or(0) as usize;
    if a & 0x8000 != 0 {
        let b = u16le(data, at + 2).unwrap_or(0) as usize;
        (at + 4, ((a & 0x7fff) << 16) | b)
    } else {
        (at + 2, a)
    }
}

fn decode_typed(type_id: u8, data: u32, pool: &Pool) -> String {
    match type_id {
        0x03 => pool.strings.get(data as usize).cloned().unwrap_or_default(),
        0x10 => format!("{}", data as i32),
        0x11 => format!("0x{:x}", data),
        0x12 => {
            if data != 0 {
                "true".into()
            } else {
                "false".into()
            }
        }
        0x01 => format!("@0x{:08x}", data),
        0x04 => format!("{}f", f32::from_bits(data)),
        0x00 => "null".into(),
        _ => format!("0x{:x}", data),
    }
}

/// Parse every element in the document, in order.
pub fn parse(data: &[u8]) -> Result<Vec<Element>, String> {
    if data.len() < 8 {
        return Err("axml: file too small".into());
    }
    if u16le(data, 0).ok_or("axml: truncated")? != RES_XML {
        return Err("axml: not a binary XML document (bad magic)".into());
    }
    let total = u32le(data, 4).ok_or("axml: truncated")? as usize;
    let end = total.min(data.len());

    let mut pool: Option<Pool> = None;
    let mut elements = Vec::new();
    let mut p = 8;
    while p + 8 <= end {
        let typ = u16le(data, p).ok_or("axml: truncated chunk")?;
        let header_size = u16le(data, p + 2).ok_or("axml: truncated")? as usize;
        let size = u32le(data, p + 4).ok_or("axml: truncated")? as usize;
        if size < 8 || p + size > data.len() {
            break;
        }
        match typ {
            RES_STRING_POOL => pool = Some(read_pool(data, p)?),
            RES_XML_START_ELEMENT => {
                let Some(ref pool) = pool else {
                    return Err("axml: element before string pool".into());
                };
                let body = p + header_size;
                // ns, name, attributeStart, attributeSize, attributeCount, ...
                let name_idx = u32le(data, body + 4).ok_or("axml: truncated element")? as usize;
                let attr_start = u16le(data, body + 8).ok_or("axml: truncated element")? as usize;
                let attr_size = u16le(data, body + 10).ok_or("axml: truncated element")? as usize;
                let attr_count = u16le(data, body + 12).ok_or("axml: truncated element")? as usize;
                let name = pool
                    .strings
                    .get(name_idx)
                    .cloned()
                    .unwrap_or_else(|| format!("#{}", name_idx));
                let mut attributes = Vec::new();
                for i in 0..attr_count {
                    let a = body + attr_start + i * attr_size.max(20);
                    let ns_idx = u32le(data, a).ok_or("axml: truncated attr")?;
                    let nm_idx = u32le(data, a + 4).ok_or("axml: truncated attr")?;
                    let raw_idx = u32le(data, a + 8).ok_or("axml: truncated attr")?;
                    let type_id = data.get(a + 15).copied().unwrap_or(0);
                    let value_data = u32le(data, a + 16).ok_or("axml: truncated attr")?;
                    let ns = if ns_idx == 0xFFFF_FFFF {
                        None
                    } else {
                        pool.strings.get(ns_idx as usize).cloned()
                    };
                    let raw = if raw_idx == 0xFFFF_FFFF {
                        None
                    } else {
                        pool.strings.get(raw_idx as usize).cloned()
                    };
                    let value = decode_typed(type_id, value_data, pool);
                    attributes.push(Attr {
                        ns,
                        name: pool
                            .strings
                            .get(nm_idx as usize)
                            .cloned()
                            .unwrap_or_else(|| format!("#{}", nm_idx)),
                        raw,
                        value,
                        type_id,
                    });
                }
                elements.push(Element { name, attributes });
            }
            _ => {}
        }
        p += size;
    }
    Ok(elements)
}

#[derive(Debug, Default, Clone)]
pub struct ManifestSummary {
    pub package: String,
    pub version_name: String,
    pub version_code: String,
    pub label: String,
    pub permissions: Vec<String>,
    pub activities: Vec<(String, bool)>,
    pub services: Vec<(String, bool)>,
    pub receivers: Vec<(String, bool)>,
    pub providers: Vec<(String, bool)>,
    pub debuggable: bool,
    pub allow_backup: Option<bool>,
    pub uses_cleartext_traffic: Option<bool>,
    pub min_sdk: Option<String>,
    pub target_sdk: Option<String>,
    pub launcher_activity: Option<String>,
    pub exported_count: usize,
}

/// Turn the element stream into the facts an analyst actually wants.
pub fn summarize(elements: &[Element]) -> ManifestSummary {
    let mut s = ManifestSummary::default();
    let mut current_component: Option<(String, bool, String)> = None; // kind, exported, name

    let flush = |s: &mut ManifestSummary, cur: &mut Option<(String, bool, String)>| {
        if let Some((kind, exported, name)) = cur.take() {
            if exported {
                s.exported_count += 1;
            }
            match kind.as_str() {
                "activity" => s.activities.push((name, exported)),
                "service" => s.services.push((name, exported)),
                "receiver" => s.receivers.push((name, exported)),
                "provider" => s.providers.push((name, exported)),
                _ => {}
            }
        }
    };

    for el in elements {
        match el.name.as_str() {
            "activity" | "activity-alias" | "service" | "receiver" | "provider" => {
                flush(&mut s, &mut current_component);
                let name = el.value("name").unwrap_or("").to_string();
                // Android's default for exported changed with API 31; record only
                // what the manifest states.
                let exported = el.value("exported") == Some("true");
                let kind = if el.name == "activity-alias" { "activity".to_string() } else { el.name.clone() };
                current_component = Some((kind, exported, name));
            }
            "action" => {
                if el.value("name") == Some("android.intent.action.MAIN") {
                    if let Some((_, _, name)) = &current_component {
                        s.launcher_activity = Some(name.clone());
                    }
                }
            }
            "uses-permission" | "uses-permission-sdk-23" => {
                if let Some(n) = el.value("name") {
                    s.permissions.push(n.to_string());
                }
            }
            "uses-sdk" => {
                if let Some(v) = el.value("minSdkVersion") {
                    s.min_sdk = Some(v.to_string());
                }
                if let Some(v) = el.value("targetSdkVersion") {
                    s.target_sdk = Some(v.to_string());
                }
            }
            "application" => {
                s.debuggable = el.value("debuggable") == Some("true");
                s.allow_backup = el.value("allowBackup").map(|v| v == "true");
                s.uses_cleartext_traffic = el.value("usesCleartextTraffic").map(|v| v == "true");
                if let Some(l) = el.value("label") {
                    if !l.starts_with('@') {
                        s.label = l.to_string();
                    }
                }
            }
            "manifest" => {
                if let Some(v) = el.value("package") {
                    s.package = v.to_string();
                }
                if let Some(v) = el.value("versionName") {
                    s.version_name = v.to_string();
                }
                if let Some(v) = el.value("versionCode") {
                    s.version_code = v.to_string();
                }
            }
            _ => {}
        }
    }
    flush(&mut s, &mut current_component);
    s.permissions.sort();
    s.permissions.dedup();
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/apk/AndroidManifest.xml");
        std::fs::read(&p).expect("manifest fixture missing")
    }

    #[test]
    fn parses_real_compiled_manifest() {
        let els = parse(&fixture()).expect("parse");
        assert!(els.iter().any(|e| e.name == "manifest"), "elements: {:?}",
            els.iter().map(|e| &e.name).collect::<Vec<_>>());
        let s = summarize(&els);
        assert_eq!(s.package, "com.example.forgetest");
        assert_eq!(s.version_name, "1.2.3");
        assert!(s.permissions.contains(&"android.permission.INTERNET".to_string()));
        assert!(s.activities.iter().any(|(n, _)| n == "com.example.forgetest.MainActivity"));
        assert_eq!(s.target_sdk.as_deref(), Some("34"));
        assert!(s.debuggable, "fixture sets debuggable");
    }

    #[test]
    fn rejects_non_binary_xml() {
        assert!(parse(b"<?xml version=\"1.0\"?><manifest/>").is_err());
        assert!(parse(b"\x00\x00").is_err());
    }
}
