//! Container-level media probing (ISO base media / MP4 family), written here because
//! crates.io is unreachable (`docs/DECISIONS.md` D1) and the sandbox has no `ffprobe`.
//!
//! What this does **not** do: decode pixels, read captions, or transcribe audio. It reads
//! the boxes an MP4/M4A/MOV file declares, which is enough to answer "how long is this,
//! what streams does it have, at what size and frame rate, and is the file damaged?" —
//! the fields `MASTER_FORGE.md` §7.12 asks for before anyone watches a frame.
//!
//! Every value reported is read from the file. Where a value cannot be read the field is
//! `None` and a warning explains why; nothing is inferred from the file extension.

use crate::json::Value;
use crate::obj;

const MAX_TRACKS: usize = 64;
const MAX_DEPTH: u32 = 8;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub id: u32,
    /// `video` | `audio` | `subtitle` | `hint` | `metadata` | the raw handler fourcc.
    pub kind: String,
    pub codec: String,
    pub language: String,
    pub timescale: u32,
    pub duration_ms: u64,
    pub sample_count: u32,
    pub fps: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Display rotation in degrees (0/90/180/270), read from the track matrix.
    pub rotation: i32,
    pub channels: Option<u16>,
    pub sample_rate: Option<u32>,
    pub sample_format: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaInfo {
    /// `mp4` | `matroska` | `mpeg-ts` | `unknown`
    pub container: String,
    pub brands: Vec<String>,
    pub duration_ms: u64,
    pub timescale: u32,
    pub fragmented: bool,
    pub tracks: Vec<Track>,
    pub warnings: Vec<String>,
}

fn fourcc(b: &[u8]) -> String {
    b.iter().map(|c| if c.is_ascii_graphic() { *c as char } else { '.' }).collect()
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}
fn be16(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}
fn be64(b: &[u8]) -> u64 {
    u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

/// One box: its fourcc and its payload (the bytes after the header).
#[derive(Debug, Clone)]
struct Box_<'a> {
    kind: String,
    payload: &'a [u8],
    /// True when the declared size ran past the end of the buffer.
    truncated: bool,
}

/// Split a buffer into boxes, tolerating (and reporting) truncation.
fn split_boxes(buf: &[u8]) -> (Vec<Box_<'_>>, Vec<String>) {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    let mut i = 0usize;
    while i + 8 <= buf.len() {
        let size32 = be32(&buf[i..]);
        let kind = fourcc(&buf[i + 4..i + 8]);
        let (header, mut size) = match size32 {
            0 => (8usize, (buf.len() - i) as u64), // extends to end of file
            1 => {
                if i + 16 > buf.len() {
                    warnings.push(format!("box {} is truncated before its 64-bit size", kind));
                    break;
                }
                (16usize, be64(&buf[i + 8..]))
            }
            n => (8usize, n as u64),
        };
        if size < header as u64 {
            warnings.push(format!("box {} declares an impossible size {}", kind, size));
            break;
        }
        let body_start = i + header;
        let body_end = i.saturating_add(size as usize);
        let truncated = body_end > buf.len();
        let payload = if body_start <= buf.len() {
            &buf[body_start..body_end.min(buf.len())]
        } else {
            &buf[0..0]
        };
        if truncated {
            warnings.push(format!(
                "box {} is truncated: declares {} bytes, {} present",
                kind,
                size,
                buf.len().saturating_sub(i)
            ));
        }
        out.push(Box_ { kind, payload, truncated });
        // A truncated last box ends parsing; anything after it cannot be trusted.
        if truncated {
            break;
        }
        i = body_end;
        if out.len() > 10_000 {
            warnings.push("too many boxes; stopped parsing".into());
            break;
        }
        size = 0;
        let _ = size;
    }
    (out, warnings)
}

fn find<'a>(boxes: &'a [Box_<'a>], kind: &str) -> Option<&'a Box_<'a>> {
    boxes.iter().find(|b| b.kind == kind)
}

fn find_all<'a>(boxes: &'a [Box_<'a>], kind: &str) -> Vec<&'a Box_<'a>> {
    boxes.iter().filter(|b| b.kind == kind).collect()
}

/// Parse `moov`/`trak`/`mdia`/`minf`/`stbl`/`udta` style containers one level down.
fn children<'a>(b: &'a Box_<'a>) -> (Vec<Box_<'a>>, Vec<String>) {
    split_boxes(b.payload)
}

fn is_container(kind: &str) -> bool {
    matches!(
        kind,
        "moov" | "trak" | "mdia" | "minf" | "stbl" | "edts" | "udta" | "mvex" | "dinf" | "moof" | "traf"
    )
}

// ---------------------------------------------------------------- header parsers

fn parse_mvhd(p: &[u8]) -> Option<(u32, u64)> {
    // version(1) flags(3) creation modification timescale duration
    let version = *p.first()?;
    if version == 1 {
        if p.len() < 32 {
            return None;
        }
        let timescale = be32(&p[20..]);
        let duration = be64(&p[24..]);
        Some((timescale, duration))
    } else {
        if p.len() < 20 {
            return None;
        }
        let timescale = be32(&p[12..]);
        let duration = be32(&p[16..]) as u64;
        Some((timescale, duration))
    }
}

fn parse_mdhd(p: &[u8]) -> Option<(u32, u64, String)> {
    let version = *p.first()?;
    let (ts_off, dur_off, dur_len) = if version == 1 { (20usize, 24usize, 8usize) } else { (12usize, 16usize, 4usize) };
    if p.len() < dur_off + dur_len + 2 {
        return None;
    }
    let timescale = be32(&p[ts_off..]);
    let duration = if dur_len == 8 { be64(&p[dur_off..]) } else { be32(&p[dur_off..]) as u64 };
    // packed ISO-639-2/T: three 5-bit letters, each offset by 0x60
    let packed = be16(&p[dur_off + dur_len..]);
    let lang: String = [(packed >> 10) & 0x1f, (packed >> 5) & 0x1f, packed & 0x1f]
        .iter()
        .map(|c| (0x60 + *c) as u8 as char)
        .collect();
    let language = if lang.chars().all(|c| c.is_ascii_lowercase()) { lang } else { "und".into() };
    Some((timescale, duration, language))
}

fn parse_hdlr(p: &[u8]) -> Option<String> {
    if p.len() < 12 {
        return None;
    }
    Some(fourcc(&p[8..12]))
}

fn parse_tkhd(p: &[u8]) -> Option<(u32, u32, u32, i32)> {
    // Layout (after the 4-byte version+flags):
    //   v0: creation 4, modification 4, track_id 4, reserved 4, duration 4,
    //       reserved 8, layer 2, alt_group 2, volume 2, reserved 2, matrix 36, width 4, height 4
    //   v1: the two timestamps and the duration are 8 bytes wide (+12 bytes overall)
    let version = *p.first()?;
    let (id_off, matrix_off, size_off) = if version == 1 { (20usize, 52usize, 88usize) } else { (12usize, 40usize, 76usize) };
    if p.len() < size_off + 8 {
        return None;
    }
    let track_id = be32(&p[id_off..]);
    let matrix = &p[matrix_off..matrix_off + 36];
    let a = be32(&matrix[0..]) as i32;
    let b = be32(&matrix[4..]) as i32;
    let c = be32(&matrix[12..]) as i32;
    let d = be32(&matrix[16..]) as i32;
    let one = 0x0001_0000i32;
    let rotation = if a == 0 && b == one && c == -one && d == 0 {
        90
    } else if a == -one && b == 0 && c == 0 && d == -one {
        180
    } else if a == 0 && b == -one && c == one && d == 0 {
        270
    } else {
        0
    };
    // width/height are 16.16 fixed point display dimensions
    let width = be32(&p[size_off..]) >> 16;
    let height = be32(&p[size_off + 4..]) >> 16;
    Some((track_id, width, height, rotation))
}

/// `stts` → (sample count, total delta in media timescale units).
fn parse_stts(p: &[u8]) -> Option<(u32, u64)> {
    if p.len() < 8 {
        return None;
    }
    let entries = be32(&p[4..]) as usize;
    let mut samples = 0u32;
    let mut total = 0u64;
    for i in 0..entries {
        let off = 8 + i * 8;
        if off + 8 > p.len() {
            break;
        }
        let count = be32(&p[off..]);
        let delta = be32(&p[off + 4..]);
        samples = samples.saturating_add(count);
        // Hostile input: count and delta are both u32, so their product can exceed u64.
        // Saturate rather than overflowing (which would panic in a debug build).
        total = total.saturating_add((count as u64).saturating_mul(delta as u64));
    }
    Some((samples, total))
}

/// First entry of `stsd`: (format fourcc, width, height, channels, sample rate).
fn parse_stsd(p: &[u8], kind: &str) -> Option<(String, Option<u32>, Option<u32>, Option<u16>, Option<u32>)> {
    if p.len() < 16 {
        return None;
    }
    let format = fourcc(&p[12..16]);
    let entry = &p[16..];
    if kind == "vide" {
        // sample entry: 6 reserved + 2 data_ref + 2+2+12 pre_defined + 2 width + 2 height
        if entry.len() < 28 {
            return Some((format, None, None, None, None));
        }
        let width = be16(&entry[24..]) as u32;
        let height = be16(&entry[26..]) as u32;
        Some((format, Some(width), Some(height), None, None))
    } else if kind == "soun" {
        if entry.len() < 20 {
            return Some((format, None, None, None, None));
        }
        let channels = be16(&entry[16..]);
        let sample_size = be16(&entry[18..]);
        let rate = if entry.len() >= 28 { be32(&entry[24..]) >> 16 } else { 0 };
        Some((format, None, None, Some(channels), Some(rate)))
            .map(|t| (t.0, t.1, t.2, t.3, t.4))
            .map(|t| {
                let _ = sample_size;
                t
            })
    } else {
        Some((format, None, None, None, None))
    }
}

fn track_kind(handler: &str) -> String {
    match handler {
        "vide" => "video".into(),
        "soun" => "audio".into(),
        "sbtl" | "subt" | "text" => "subtitle".into(),
        "hint" => "hint".into(),
        "meta" => "metadata".into(),
        other => format!("handler:{}", other),
    }
}

fn parse_trak(b: &Box_<'_>, warnings: &mut Vec<String>) -> Option<Track> {
    let (moov, _) = children(b);
    let mut track = Track::default();

    if let Some(tkhd) = find(&moov, "tkhd") {
        match parse_tkhd(tkhd.payload) {
            Some((id, w, h, rot)) => {
                track.id = id;
                track.rotation = rot;
                if w > 0 {
                    track.width = Some(w);
                }
                if h > 0 {
                    track.height = Some(h);
                }
            }
            None => warnings.push("a track header (tkhd) was too short to read".into()),
        }
    } else {
        warnings.push("a track has no tkhd header".into());
    }

    let Some(mdia) = find(&moov, "mdia") else {
        warnings.push("a track has no mdia box".into());
        return Some(track);
    };
    let (media, _) = children(mdia);
    if let Some(mdhd) = find(&media, "mdhd") {
        if let Some((timescale, duration, language)) = parse_mdhd(mdhd.payload) {
            track.timescale = timescale;
            track.language = language;
            if timescale > 0 {
                track.duration_ms = duration.saturating_mul(1000) / timescale as u64;
            }
        }
    }
    let handler = find(&media, "hdlr").and_then(|h| parse_hdlr(h.payload)).unwrap_or_default();
    track.kind = track_kind(&handler);

    if let Some(minf) = find(&media, "minf") {
        if let Some(stbl) = find(&children(minf).0, "stbl") {
            let (table, _) = children(stbl);
            if let Some(stsd) = find(&table, "stsd") {
                if let Some((codec, w, h, channels, rate)) = parse_stsd(stsd.payload, &handler) {
                    track.codec = codec;
                    if track.kind == "video" {
                        if track.width.is_none() {
                            track.width = w;
                        }
                        if track.height.is_none() {
                            track.height = h;
                        }
                    } else if track.kind == "audio" {
                        track.channels = channels;
                        track.sample_rate = rate;
                    }
                }
            } else {
                warnings.push("a track has no sample description (stsd)".into());
            }
            if let Some(stts) = find(&table, "stts") {
                if let Some((samples, total_delta)) = parse_stts(stts.payload) {
                    track.sample_count = samples;
                    // frame rate = samples / media duration in seconds
                    if track.timescale > 0 && total_delta > 0 {
                        let seconds = total_delta as f64 / track.timescale as f64;
                        if seconds > 0.0 {
                            track.fps = Some((samples as f64 / seconds * 100.0).round() / 100.0);
                        }
                    }
                }
            }
            if track.kind == "video" && table.iter().any(|b| b.kind == "stss") {
                // `stss` present means some frames are not sync frames: real B/P frames
                track.sample_format = Some(1);
            }
        }
    }
    Some(track)
}

/// Detect the container from magic bytes and structural evidence — never from a file name.
pub fn detect(data: &[u8]) -> &'static str {
    if data.len() >= 4 && data[..4] == [0x1A, 0x45, 0xDF, 0xA3] {
        return "matroska"; // also WebM
    }
    if data.len() >= 2 && data[0] == 0x47 && (data.len() < 189 || data.get(188) == Some(&0x47)) {
        return "mpeg-ts";
    }
    if data.len() >= 8 {
        let kind = fourcc(&data[4..8]);
        if matches!(
            kind.as_str(),
            "ftyp" | "styp" | "moov" | "mdat" | "free" | "skip" | "wide" | "pnot"
        ) {
            return "mp4";
        }
    }
    if data.len() >= 4 && &data[..4] == b"RIFF" && data.len() >= 12 && &data[8..12] == b"AVI " {
        return "avi";
    }
    "unknown"
}

/// Read the container structure of a media file.
pub fn inspect(data: &[u8]) -> MediaInfo {
    let mut info = MediaInfo {
        container: detect(data).to_string(),
        ..Default::default()
    };
    if matches!(info.container.as_str(), "matroska" | "mpeg-ts" | "avi") {
        info.warnings.push(format!(
            "{} container: this build parses ISO base media (MP4/M4A/MOV) only; \
             duration, streams and frame rate are unknown",
            info.container
        ));
        return info;
    }
    if info.container != "mp4" {
        info.warnings.push(
            "not a recognised media container (no ftyp/moov box, no Matroska or MPEG-TS magic)".into(),
        );
        return info;
    }

    let (top, mut warnings) = split_boxes(data);

    if let Some(ftyp) = find(&top, "ftyp") {
        if ftyp.payload.len() >= 8 {
            info.brands.push(fourcc(&ftyp.payload[0..4]));
            let mut i = 8;
            while i + 4 <= ftyp.payload.len() {
                info.brands.push(fourcc(&ftyp.payload[i..i + 4]));
                i += 4;
            }
            info.brands.retain(|b| !b.is_empty() && b != "....");
            info.brands.dedup();
        }
    }

    let Some(moov) = find(&top, "moov") else {
        warnings.push("no moov box: the file is incomplete or was saved without its index".into());
        info.warnings = warnings;
        return info;
    };
    let (moov_boxes, more) = children(moov);
    warnings.extend(more);

    if find(&moov_boxes, "mvex").is_some() {
        info.fragmented = true;
        warnings.push(
            "fragmented MP4 (moov/mvex): duration and frame rate may only be known after the fragments are read"
                .into(),
        );
    }

    match find(&moov_boxes, "mvhd").and_then(|b| parse_mvhd(b.payload)) {
        Some((timescale, duration)) => {
            info.timescale = timescale;
            if timescale > 0 {
                info.duration_ms = duration.saturating_mul(1000) / timescale as u64;
            }
            if duration == 0 && !info.fragmented {
                warnings.push("movie header reports a duration of zero".into());
            }
        }
        None => warnings.push("moov has no readable mvhd (movie header)".into()),
    }

    for trak in find_all(&moov_boxes, "trak").into_iter().take(MAX_TRACKS) {
        if let Some(track) = parse_trak(trak, &mut warnings) {
            info.tracks.push(track);
        }
    }
    if info.tracks.is_empty() {
        warnings.push("no tracks were found in the movie header".into());
    }
    for t in &info.tracks {
        if t.kind == "video" && (t.width.unwrap_or(0) == 0 || t.height.unwrap_or(0) == 0) {
            warnings.push(format!("video track {} declares no display size", t.id));
        }
    }
    info.warnings = warnings;
    let _ = is_container;
    let _ = MAX_DEPTH;
    info
}

impl MediaInfo {
    pub fn to_json(&self) -> Value {
        obj! {
            "container" => self.container.as_str(),
            "duration_ms" => self.duration_ms,
            "timescale" => self.timescale as u64,
            "fragmented" => self.fragmented,
            "brands" => Value::Arr(self.brands.iter().map(|b| Value::from(b.as_str())).collect()),
            "tracks" => Value::Arr(self.tracks.iter().map(|t| obj! {
                "id" => t.id as u64,
                "kind" => t.kind.as_str(),
                "codec" => t.codec.as_str(),
                "language" => t.language.as_str(),
                "timescale" => t.timescale as u64,
                "duration_ms" => t.duration_ms,
                "sample_count" => t.sample_count as u64,
                "fps" => match t.fps { Some(f) => Value::from(f), None => Value::Null },
                "width" => t.width.map(|w| Value::from(w as u64)).unwrap_or(Value::Null),
                "height" => t.height.map(|h| Value::from(h as u64)).unwrap_or(Value::Null),
                "rotation" => t.rotation as i64,
                "channels" => t.channels.map(|c| Value::from(c as u64)).unwrap_or(Value::Null),
                "sample_rate" => t.sample_rate.map(|r| Value::from(r as u64)).unwrap_or(Value::Null),
            }).collect()),
            "warnings" => Value::Arr(self.warnings.iter().map(|w| Value::from(w.as_str())).collect()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------- box builders (this is how the fixtures are made)

    fn boxed(kind: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + payload.len());
        out.extend_from_slice(&((8 + payload.len()) as u32).to_be_bytes());
        out.extend_from_slice(kind.as_bytes());
        out.extend_from_slice(payload);
        out
    }

    fn large_boxed(kind: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&1u32.to_be_bytes());
        out.extend_from_slice(kind.as_bytes());
        out.extend_from_slice(&(16u64 + payload.len() as u64).to_be_bytes());
        out.extend_from_slice(payload);
        out
    }

    fn full(version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![version];
        out.extend_from_slice(&flags.to_be_bytes()[1..]);
        out.extend_from_slice(payload);
        out
    }

    fn mvhd(timescale: u32, duration: u32) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&0u32.to_be_bytes()); // creation
        p.extend_from_slice(&0u32.to_be_bytes()); // modification
        p.extend_from_slice(&timescale.to_be_bytes());
        p.extend_from_slice(&duration.to_be_bytes());
        p.extend_from_slice(&[0u8; 80]); // rate, volume, matrix, etc.
        boxed("mvhd", &full(0, 0, &p))
    }

    fn matrix(rotation: i32) -> [u8; 36] {
        let one = 0x0001_0000i32;
        let (a, b, c, d) = match rotation {
            90 => (0, one, -one, 0),
            180 => (-one, 0, 0, -one),
            270 => (0, -one, one, 0),
            _ => (one, 0, 0, one),
        };
        let mut m = [0u8; 36];
        for (i, v) in [a, b, 0, c, d, 0, 0, 0, 0x4000_0000].iter().enumerate() {
            m[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
        }
        m
    }

    fn tkhd(id: u32, w: u32, h: u32, rotation: i32) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&0u32.to_be_bytes()); // creation
        p.extend_from_slice(&0u32.to_be_bytes()); // modification
        p.extend_from_slice(&id.to_be_bytes());
        p.extend_from_slice(&0u32.to_be_bytes()); // reserved
        p.extend_from_slice(&1000u32.to_be_bytes()); // duration
        p.extend_from_slice(&[0u8; 8]); // reserved
        p.extend_from_slice(&0u16.to_be_bytes()); // layer
        p.extend_from_slice(&0u16.to_be_bytes()); // alternate group
        p.extend_from_slice(&0u16.to_be_bytes()); // volume
        p.extend_from_slice(&0u16.to_be_bytes()); // reserved
        p.extend_from_slice(&matrix(rotation));
        p.extend_from_slice(&((w << 16).to_be_bytes()));
        p.extend_from_slice(&((h << 16).to_be_bytes()));
        boxed("tkhd", &full(0, 3, &p))
    }

    fn mdhd(timescale: u32, duration: u32) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&0u32.to_be_bytes()); // creation
        p.extend_from_slice(&0u32.to_be_bytes()); // modification
        p.extend_from_slice(&timescale.to_be_bytes());
        p.extend_from_slice(&duration.to_be_bytes());
        // "eng" packed as 5-bit letters
        let lang: u16 = (b'e' as u16 - 0x60) << 10 | (b'n' as u16 - 0x60) << 5 | (b'g' as u16 - 0x60);
        p.extend_from_slice(&lang.to_be_bytes());
        p.extend_from_slice(&0u16.to_be_bytes());
        boxed("mdhd", &full(0, 0, &p))
    }

    fn hdlr(kind: &str) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&0u32.to_be_bytes()); // pre_defined
        p.extend_from_slice(kind.as_bytes());
        p.extend_from_slice(&[0u8; 12]);
        p.push(0);
        boxed("hdlr", &full(0, 0, &p))
    }

    fn video_stsd(codec: &str, w: u16, h: u16) -> Vec<u8> {
        let mut entry = Vec::new();
        entry.extend_from_slice(&[0u8; 6]); // reserved
        entry.extend_from_slice(&1u16.to_be_bytes()); // data ref index
        entry.extend_from_slice(&[0u8; 16]); // pre_defined + reserved
        entry.extend_from_slice(&w.to_be_bytes());
        entry.extend_from_slice(&h.to_be_bytes());
        entry.extend_from_slice(&[0u8; 50]); // resolutions, compressor name, depth
        let mut sample = Vec::new();
        sample.extend_from_slice(&((8 + entry.len()) as u32).to_be_bytes());
        sample.extend_from_slice(codec.as_bytes());
        sample.extend_from_slice(&entry);
        let mut p = Vec::new();
        p.extend_from_slice(&1u32.to_be_bytes()); // entry count
        p.extend_from_slice(&sample);
        boxed("stsd", &full(0, 0, &p))
    }

    fn audio_stsd(codec: &str, channels: u16, rate: u32) -> Vec<u8> {
        let mut entry = Vec::new();
        entry.extend_from_slice(&[0u8; 6]);
        entry.extend_from_slice(&1u16.to_be_bytes());
        entry.extend_from_slice(&0u16.to_be_bytes()); // version
        entry.extend_from_slice(&0u16.to_be_bytes()); // revision
        entry.extend_from_slice(&0u32.to_be_bytes()); // vendor
        entry.extend_from_slice(&channels.to_be_bytes());
        entry.extend_from_slice(&16u16.to_be_bytes()); // sample size
        entry.extend_from_slice(&0u16.to_be_bytes());
        entry.extend_from_slice(&0u16.to_be_bytes());
        entry.extend_from_slice(&((rate << 16).to_be_bytes()));
        let mut sample = Vec::new();
        sample.extend_from_slice(&((8 + entry.len()) as u32).to_be_bytes());
        sample.extend_from_slice(codec.as_bytes());
        sample.extend_from_slice(&entry);
        let mut p = Vec::new();
        p.extend_from_slice(&1u32.to_be_bytes());
        p.extend_from_slice(&sample);
        boxed("stsd", &full(0, 0, &p))
    }

    fn stts(entries: &[(u32, u32)]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for (count, delta) in entries {
            p.extend_from_slice(&count.to_be_bytes());
            p.extend_from_slice(&delta.to_be_bytes());
        }
        boxed("stts", &full(0, 0, &p))
    }

    fn trak(
        stsd: Vec<u8>,
        handler: &str,
        timescale: u32,
        duration: u32,
        fps: u32,
        w: u32,
        h: u32,
        rot: i32,
    ) -> Vec<u8> {
        // one stts entry: `frames` samples of `timescale / fps` media units each
        let delta = (timescale / fps).max(1);
        let frames = duration / delta;
        let stbl = boxed("stbl", &[stsd, stts(&[(frames, delta)])].concat());
        let minf = boxed("minf", &stbl);
        let mdia = boxed("mdia", &[mdhd(timescale, duration), hdlr(handler), minf].concat());
        boxed("trak", &[tkhd(1, w, h, rot), mdia].concat())
    }

    fn sample_mp4(rotation: i32) -> Vec<u8> {
        let ftyp = boxed("ftyp", b"isomisomiso2mp41");
        // 6 s movie, video at 10 fps, stereo 48 kHz audio
        let video = trak(video_stsd("avc1", 320, 240), "vide", 30_000, 180_000, 10, 320, 240, rotation);
        let audio = trak(audio_stsd("mp4a", 2, 48_000), "soun", 48_000, 288_000, 48_000, 0, 0, 0);
        let moov = boxed("moov", &[mvhd(1000, 6000), video, audio].concat());
        let mdat = boxed("mdat", &[0u8; 16]);
        [ftyp, moov, mdat].concat()
    }

    // ---------------- tests

    #[test]
    fn reads_duration_streams_size_and_frame_rate() {
        let info = inspect(&sample_mp4(0));
        assert_eq!(info.container, "mp4");
        assert_eq!(info.duration_ms, 6000, "6 s movie from mvhd");
        assert!(info.brands.iter().any(|b| b == "isom"));
        assert_eq!(info.tracks.len(), 2);

        let video = info.tracks.iter().find(|t| t.kind == "video").expect("video track");
        assert_eq!(video.codec, "avc1");
        assert_eq!(video.width, Some(320));
        assert_eq!(video.height, Some(240));
        assert_eq!(video.fps, Some(10.0), "180 samples over 6 s of media time");
        assert_eq!(video.duration_ms, 6000);
        assert_eq!(video.language, "eng");
        assert_eq!(video.rotation, 0);
        assert_eq!(video.sample_count, 60, "6 s at 10 fps is 60 samples");

        let audio = info.tracks.iter().find(|t| t.kind == "audio").expect("audio track");
        assert_eq!(audio.codec, "mp4a");
        assert_eq!(audio.channels, Some(2));
        assert_eq!(audio.sample_rate, Some(48_000));
        assert!(info.warnings.is_empty(), "{:?}", info.warnings);
    }

    #[test]
    fn reads_display_rotation_from_the_track_matrix() {
        assert_eq!(inspect(&sample_mp4(90)).tracks[0].rotation, 90);
        assert_eq!(inspect(&sample_mp4(180)).tracks[0].rotation, 180);
        assert_eq!(inspect(&sample_mp4(270)).tracks[0].rotation, 270);
    }

    #[test]
    fn json_reports_absent_values_as_null_not_zero() {
        let info = inspect(&sample_mp4(0));
        let v = info.to_json();
        let tracks = v.get("tracks").and_then(|t| t.as_arr()).cloned().unwrap_or_default();
        let audio = tracks.iter().find(|t| t.str_of("kind") == Some("audio")).unwrap();
        assert!(audio.get("width").map(|w| w.is_null()).unwrap_or(false), "audio has no width");
        let video = tracks.iter().find(|t| t.str_of("kind") == Some("video")).unwrap();
        assert!(video.get("channels").map(|c| c.is_null()).unwrap_or(false), "video has no channels");
        assert_eq!(video.get("fps").and_then(|f| f.as_f64()), Some(10.0));
    }

    #[test]
    fn truncated_file_is_reported_not_guessed() {
        let full = sample_mp4(0);
        let cut = &full[..full.len() - 40];
        let info = inspect(cut);
        assert!(
            info.warnings.iter().any(|w| w.contains("truncated") || w.contains("no moov")),
            "{:?}",
            info.warnings
        );
    }

    #[test]
    fn missing_moov_is_a_warning_and_no_tracks() {
        let only_ftyp = boxed("ftyp", b"isomisom");
        let info = inspect(&only_ftyp);
        assert_eq!(info.container, "mp4");
        assert!(info.tracks.is_empty());
        assert!(info.warnings.iter().any(|w| w.contains("no moov")), "{:?}", info.warnings);
    }

    #[test]
    fn fragmented_mp4_is_flagged() {
        let mut moov_payload = mvhd(1000, 0);
        moov_payload.extend(boxed("mvex", &[]));
        let file = [boxed("ftyp", b"isomiso5"), boxed("moov", &moov_payload)].concat();
        let info = inspect(&file);
        assert!(info.fragmented);
        assert!(info.warnings.iter().any(|w| w.contains("fragmented")), "{:?}", info.warnings);
    }

    #[test]
    fn other_containers_are_named_but_not_parsed() {
        let matroska = [0x1Au8, 0x45, 0xDF, 0xA3, 0, 0, 0, 0];
        let info = inspect(&matroska);
        assert_eq!(info.container, "matroska");
        assert!(info.tracks.is_empty(), "no invented tracks");
        assert!(info.warnings[0].contains("ISO base media"), "{:?}", info.warnings);

        assert_eq!(detect(b"not media at all"), "unknown");
        let unknown = inspect(b"hello world, definitely not a video");
        assert_eq!(unknown.container, "unknown");
        assert!(!unknown.warnings.is_empty());
    }

    #[test]
    fn absurd_sample_tables_saturate_instead_of_overflowing() {
        // Two entries of (0xFFFFFFFF samples, 0xFFFFFFFF delta) cannot be represented in u64;
        // in a debug build the multiply would panic. `parse_stts` must clamp and keep going.
        let mut p = vec![0, 0, 0, 0, 0, 0, 0, 2];
        for _ in 0..2 {
            p.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        }
        let (samples, total) = parse_stts(&p).expect("well-formed header");
        assert_eq!(samples, u32::MAX, "sample count saturates at the u32 ceiling");
        assert_eq!(total, u64::MAX, "total duration saturates instead of wrapping to a small number");
        // A truncated entry list stops at the last complete entry rather than reading past
        // the buffer; here the first entry straddles the cut, so nothing is counted.
        assert_eq!(parse_stts(&p[..12]), Some((0, 0)));
        // One full entry is readable even when the second is cut in half.
        assert_eq!(
            parse_stts(&p[..16]),
            Some((u32::MAX, (u32::MAX as u64) * (u32::MAX as u64)))
        );
    }

    #[test]
    fn sixty_four_bit_box_sizes_are_understood() {
        let payload = [mvhd(1000, 2500)].concat();
        let file = [
            boxed("ftyp", b"isomiso5"),
            large_boxed("moov", &payload),
        ]
        .concat();
        let info = inspect(&file);
        assert_eq!(info.duration_ms, 2500, "{:?}", info.warnings);
    }

    #[test]
    fn a_real_encoder_output_is_read_correctly() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sample.mp4");
        let Ok(bytes) = std::fs::read(&p) else {
            eprintln!("sample.mp4 fixture absent; skipping the real-encoder check");
            return;
        };
        let info = inspect(&bytes);
        assert_eq!(info.container, "mp4");
        let video = info.tracks.iter().find(|t| t.kind == "video").expect("video track");
        // the fixture is generated by the command recorded in tests/fixtures/make_media.py
        assert_eq!(video.codec, "avc1");
        assert_eq!(video.width, Some(320));
        assert_eq!(video.height, Some(240));
        assert_eq!(video.fps, Some(10.0), "{:?}", video);
        assert!((5_900..=6_200).contains(&info.duration_ms), "duration {}", info.duration_ms);
    }
}
