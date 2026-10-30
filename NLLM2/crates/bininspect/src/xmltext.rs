//! Plain-text Android manifest support.
//!
//! Build intermediates (`merged_manifest/debug/AndroidManifest.xml`) ship the manifest as
//! ordinary XML rather than compiled AXML. Rather than write a second summariser, this
//! converts text XML into the same `axml::Element` stream so both paths share one set of
//! semantics (package qualification, exported defaults, permissions, launcher detection).

use crate::axml::{Attr, Element};

/// Cheap sniff: does this look like a text XML manifest?
pub fn is_text_manifest(data: &[u8]) -> bool {
    let head = &data[..data.len().min(512)];
    let text = String::from_utf8_lossy(head);
    let t = text.trim_start_matches('\u{feff}').trim_start();
    t.starts_with("<?xml") || t.starts_with("<manifest")
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Scan text XML into elements. Deliberately minimal: tags and attributes only — no
/// namespace resolution, no text nodes, no validation. Comments, CDATA, processing
/// instructions and doctypes are skipped.
pub fn parse(data: &[u8]) -> Result<Vec<Element>, String> {
    let text = String::from_utf8_lossy(data);
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        // skip comments / CDATA / declarations
        if text[i..].starts_with("<!--") {
            match text[i..].find("-->") {
                Some(p) => {
                    i += p + 3;
                    continue;
                }
                None => break,
            }
        }
        if text[i..].starts_with("<![CDATA[") {
            match text[i..].find("]]>") {
                Some(p) => {
                    i += p + 3;
                    continue;
                }
                None => break,
            }
        }
        if text[i..].starts_with("<?") || text[i..].starts_with("<!") {
            match text[i..].find('>') {
                Some(p) => {
                    i += p + 1;
                    continue;
                }
                None => break,
            }
        }
        if text[i..].starts_with("</") {
            match text[i..].find('>') {
                Some(p) => {
                    i += p + 1;
                    continue;
                }
                None => break,
            }
        }

        // opening tag: read the name
        i += 1;
        let start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
            i += 1;
        }
        let name = text[start..i].to_string();
        if name.is_empty() {
            i += 1;
            continue;
        }

        // attributes until '>' (respecting quoted values)
        let mut attributes = Vec::new();
        loop {
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= b.len() || b[i] == b'>' {
                i += 1;
                break;
            }
            if b[i] == b'/' {
                i += 1;
                continue;
            }
            let astart = i;
            while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
                i += 1;
            }
            let full = text[astart..i].to_string();
            if full.is_empty() {
                i += 1;
                continue;
            }
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            let mut value = String::new();
            if i < b.len() && b[i] == b'=' {
                i += 1;
                while i < b.len() && b[i].is_ascii_whitespace() {
                    i += 1;
                }
                if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                    let quote = b[i];
                    i += 1;
                    let vstart = i;
                    while i < b.len() && b[i] != quote {
                        i += 1;
                    }
                    value = decode_entities(&text[vstart..i]);
                    i += 1;
                }
            }
            let (ns, local) = match full.rsplit_once(':') {
                Some((ns, local)) => (Some(ns.to_string()), local.to_string()),
                None => (None, full),
            };
            attributes.push(Attr {
                ns,
                name: local,
                raw: Some(value.clone()),
                value,
                type_id: 0x03,
            });
        }
        out.push(Element { name, attributes });
    }

    if out.is_empty() {
        Err("text manifest: no elements found".into())
    } else {
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axml;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="com.example.text" android:versionName="2.0" android:versionCode="7">
  <!-- a comment <activity android:name=".Nope"/> -->
  <uses-sdk android:minSdkVersion="21" android:targetSdkVersion="33"/>
  <uses-permission android:name="android.permission.INTERNET"/>
  <application android:debuggable="true">
    <activity android:name=".Main" android:exported="true">
      <intent-filter>
        <action android:name="android.intent.action.MAIN"/>
      </intent-filter>
    </activity>
    <service android:name=".Sync"/>
  </application>
</manifest>
"#;

    #[test]
    fn sniffs_text_manifests() {
        assert!(is_text_manifest(SAMPLE.as_bytes()));
        assert!(is_text_manifest(b"\xef\xbb\xbf<?xml version=\"1.0\"?>"));
        assert!(!is_text_manifest(b"\x03\x00\x08\x00"));
        assert!(!is_text_manifest(b"PK\x03\x04"));
    }

    #[test]
    fn text_manifest_reuses_the_axml_summariser() {
        let els = parse(SAMPLE.as_bytes()).expect("parse");
        let s = axml::summarize(&els);
        assert_eq!(s.package, "com.example.text");
        assert_eq!(s.version_name, "2.0");
        assert_eq!(s.version_code, "7");
        assert_eq!(s.min_sdk.as_deref(), Some("21"));
        assert_eq!(s.target_sdk.as_deref(), Some("33"));
        assert!(s.debuggable);
        assert!(s.permissions.contains(&"android.permission.INTERNET".to_string()));
        assert_eq!(s.launcher_activity.as_deref(), Some("com.example.text.Main"));
        assert!(s.activities.iter().any(|(n, e)| n == "com.example.text.Main" && *e));
        assert!(s.services.iter().any(|(n, e)| n == "com.example.text.Sync" && !*e));
        assert_eq!(s.exported_count, 1, "comment must not be parsed as an element");
    }

    #[test]
    fn handles_entities_and_single_quotes() {
        let els = parse(b"<manifest package='a.b'><application android:label='Tom &amp; Jerry'/></manifest>")
            .expect("parse");
        let app = els.iter().find(|e| e.name == "application").expect("app");
        assert_eq!(app.value("label"), Some("Tom & Jerry"));
    }

    #[test]
    fn rejects_non_xml() {
        assert!(parse(b"\x00\x01\x02binary garbage").is_err());
    }
}
