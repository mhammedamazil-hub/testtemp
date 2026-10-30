//! Minimal ZIP reader (enough for OOXML/EPUB): central directory + stored/deflated entries.
//!
//! Only what the format needs: no encryption, no ZIP64, no multi-disk. Anything
//! outside that returns a clear error instead of silently producing garbage.

pub struct Entry {
    pub name: String,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub method: u16,
    pub local_offset: u64,
}

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

const EOCD: u32 = 0x0605_4b50;
const CEN: u32 = 0x0201_4b50;
const LOC: u32 = 0x0403_4b50;

/// Locate and parse the central directory.
pub fn entries(data: &[u8]) -> Result<Vec<Entry>, String> {
    if data.len() < 22 {
        return Err("zip: file too small".into());
    }
    // The EOCD record is at the end; the comment may be up to 65535 bytes.
    let mut eocd = None;
    let start = data.len().saturating_sub(65557);
    let mut i = data.len().saturating_sub(22);
    loop {
        if u32le(data, i) == Some(EOCD) {
            eocd = Some(i);
            break;
        }
        if i == start {
            break;
        }
        i -= 1;
    }
    let eocd = eocd.ok_or_else(|| "zip: end-of-central-directory record not found".to_string())?;

    let count = u16le(data, eocd + 10).ok_or("zip: truncated EOCD")? as usize;
    let cd_size = u32le(data, eocd + 12).ok_or("zip: truncated EOCD")? as usize;
    let cd_off = u32le(data, eocd + 16).ok_or("zip: truncated EOCD")? as usize;
    if cd_off == 0xFFFF_FFFF || count == 0xFFFF {
        return Err("zip: ZIP64 archives are not supported".into());
    }
    if cd_off.saturating_add(cd_size) > data.len() {
        return Err("zip: central directory out of bounds".into());
    }

    let mut out = Vec::with_capacity(count);
    let mut p = cd_off;
    for _ in 0..count {
        if u32le(data, p) != Some(CEN) {
            return Err("zip: bad central directory entry".into());
        }
        let method = u16le(data, p + 10).ok_or("zip: truncated entry")?;
        let compressed_size = u32le(data, p + 20).ok_or("zip: truncated entry")? as u64;
        let uncompressed_size = u32le(data, p + 24).ok_or("zip: truncated entry")? as u64;
        let name_len = u16le(data, p + 28).ok_or("zip: truncated entry")? as usize;
        let extra_len = u16le(data, p + 30).ok_or("zip: truncated entry")? as usize;
        let comment_len = u16le(data, p + 32).ok_or("zip: truncated entry")? as usize;
        let local_offset = u32le(data, p + 42).ok_or("zip: truncated entry")? as u64;
        let name_bytes = data
            .get(p + 46..p + 46 + name_len)
            .ok_or("zip: entry name out of bounds")?;
        let name = String::from_utf8_lossy(name_bytes).to_string();
        out.push(Entry {
            name,
            compressed_size,
            uncompressed_size,
            method,
            local_offset,
        });
        p += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

pub fn read(data: &[u8], entry: &Entry) -> Result<Vec<u8>, String> {
    let at = entry.local_offset as usize;
    if u32le(data, at) != Some(LOC) {
        return Err(format!("zip: bad local header for {}", entry.name));
    }
    let name_len = u16le(data, at + 26).ok_or("zip: truncated local header")? as usize;
    let extra_len = u16le(data, at + 28).ok_or("zip: truncated local header")? as usize;
    let data_start = at + 30 + name_len + extra_len;
    let end = data_start + entry.compressed_size as usize;
    let raw = data
        .get(data_start..end)
        .ok_or_else(|| format!("zip: data out of bounds for {}", entry.name))?;
    match entry.method {
        0 => Ok(raw.to_vec()),
        8 => crate::inflate::inflate_raw(raw)
            .map_err(|e| format!("zip: deflate failed for {}: {}", entry.name, e)),
        m => Err(format!("zip: compression method {} is not supported", m)),
    }
}

/// Read an entry by exact name.
pub fn read_named(data: &[u8], name: &str) -> Result<Option<Vec<u8>>, String> {
    let all = entries(data)?;
    match all.iter().find(|e| e.name == name) {
        None => Ok(None),
        Some(e) => read(data, e).map(Some),
    }
}

/// Find entries whose name starts with `prefix` and ends with `suffix`.
pub fn find(data: &[u8], prefix: &str, suffix: &str) -> Result<Vec<Entry>, String> {
    Ok(entries(data)?
        .into_iter()
        .filter(|e| e.name.starts_with(prefix) && e.name.ends_with(suffix))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inflate;

    #[test]
    fn reads_stored_and_deflated_entries() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/sample.docx");
        let data = std::fs::read(&path).expect("fixture sample.docx missing");
        let doc = read_named(&data, "word/document.xml").unwrap().expect("document.xml");
        let xml = String::from_utf8_lossy(&doc);
        assert!(xml.contains("<w:t"), "stored entry text");
        let styles = read_named(&data, "word/styles.xml").unwrap().expect("styles.xml");
        assert!(styles.len() > 50, "deflated entry expanded");
        assert!(read_named(&data, "word/nope.xml").unwrap().is_none());
    }

    #[test]
    fn rejects_non_zip_and_garbage() {
        assert!(entries(b"plain text, definitely not a zip").is_err());
        assert!(inflate::inflate_raw(b"garbage").is_err());
    }
}
