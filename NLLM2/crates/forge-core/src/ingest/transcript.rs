//! Timed transcripts (WebVTT and SubRip).
//!
//! This is the *only* video-adjacent ingest path in Forge: the user supplies a caption file
//! (exported from a video they are entitled to, or a transcript they obtained themselves) and
//! Forge indexes the cue text with an exact millisecond offset. Forge never downloads a video,
//! never calls a platform API and never pulls captions on its own — see
//! `docs/VIDEO_SOURCES.md`. A citation from here is therefore "spoken at 00:42", not "shown at
//! 00:42": there is no frame analysis.

use super::Block;
use crate::domain::Locator;

#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    /// Offset of the start of the cue on the media timeline.
    pub start_ms: u32,
    /// Offset of the end of the cue.
    pub end_ms: u32,
    /// Optional cue identifier (VTT) or speaker label we could recover.
    pub speaker: String,
    pub text: String,
}

/// Parse either format, sniffing the content rather than trusting the extension.
pub fn parse(raw: &str) -> Result<Vec<Cue>, String> {
    let cleaned = raw.replace("\r\n", "\n").replace('\r', "\n");
    let is_vtt = cleaned.trim_start().starts_with("WEBVTT");
    // The header (and any metadata lines under it) ends at the first blank line; cues start
    // after that. Some exporters omit the blank line, so the header line is dropped by itself.
    let body = if is_vtt {
        match cleaned.split_once("\n\n") {
            Some((_header, rest)) => rest.to_string(),
            None => cleaned.trim_start().trim_start_matches("WEBVTT").to_string(),
        }
    } else {
        cleaned
    };
    let cues = if is_vtt { parse_vtt(&body)? } else { parse_srt(&body)? };
    if cues.is_empty() {
        return Err("no timed cues found (expected WebVTT or SubRip timestamps)".to_string());
    }
    Ok(cues)
}

fn parse_vtt(raw: &str) -> Result<Vec<Cue>, String> {
    let mut out = Vec::new();
    let mut speaker = String::new();
    for block in raw.split("\n\n") {
        let block = block.trim();
        if block.is_empty() || block.starts_with("WEBVTT") || block.starts_with("NOTE") || block.starts_with("STYLE") {
            continue;
        }
        let mut lines = block.lines();
        let mut first = lines.next().unwrap_or("");
        // A cue identifier line may precede the timing line.
        if !first.contains("-->") {
            speaker = cue_identifier(first);
            let Some(next) = lines.next() else { continue };
            first = next;
        }
        let Some((start, end)) = parse_timing(first) else { continue };
        let text = join_lines(lines);
        if text.is_empty() {
            continue;
        }
        out.push(Cue { start_ms: start, end_ms: end, speaker: speaker.clone(), text });
        speaker.clear();
    }
    // A WEBVTT file whose first block is the header plus the first cue is handled above
    // because the header line is skipped as its own block.
    Ok(out)
}

fn parse_srt(raw: &str) -> Result<Vec<Cue>, String> {
    let mut out = Vec::new();
    for block in raw.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        let mut lines: Vec<&str> = block.lines().collect();
        // Optional numeric index (ignored: the timestamp is the locator that matters).
        if !lines[0].contains("-->") && lines[0].trim().chars().all(|c| c.is_ascii_digit()) {
            lines.remove(0);
        }
        let Some(head) = lines.first() else { continue };
        let Some((start, end)) = parse_timing(head) else { continue };
        let text = join_lines(lines[1..].iter().copied());
        if text.is_empty() {
            continue;
        }
        out.push(Cue { start_ms: start, end_ms: end, speaker: String::new(), text });
    }
    Ok(out)
}

/// `00:01:02.500 --> 00:01:05.000 align:start position:0%` in either format.
fn parse_timing(line: &str) -> Option<(u32, u32)> {
    let (left, right) = line.split_once("-->")?;
    let start = parse_offset(left)?;
    // Cue settings follow the end timestamp, separated by whitespace.
    let end_token = right.split_whitespace().next()?;
    let end = parse_offset(end_token)?;
    Some((start, end))
}

fn parse_offset(s: &str) -> Option<u32> {
    let s = s.trim();
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, sec] => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?, *sec),
        [m, sec] => (0u32, m.parse::<u32>().ok()?, *sec),
        _ => return None,
    };
    let (sec, frac) = match sec.split_once(['.', ',']) {
        Some((a, b)) => (a.parse::<u32>().ok()?, b),
        None => (sec.parse::<u32>().ok()?, ""),
    };
    let ms = match frac.len() {
        0 => 0,
        1 => frac.parse::<u32>().ok()? * 100,
        2 => frac.parse::<u32>().ok()? * 10,
        _ => frac[..3].parse::<u32>().ok()?,
    };
    Some(h * 3_600_000 + m * 60_000 + sec * 1_000 + ms)
}

fn cue_identifier(line: &str) -> String {
    let t = line.trim();
    // Only keep short, human-ish identifiers; a long line here is not a cue id.
    if t.len() <= 40 && !t.contains("--") {
        t.trim_matches(|c: char| c == '"' || c == '\'').to_string()
    } else {
        String::new()
    }
}

fn join_lines<'a>(lines: impl Iterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for line in lines {
        let line = strip_markup(line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(line);
    }
    out.trim().to_string()
}

/// Remove WebVTT inline markup (`<v Roger>`, `<i>`, `<c.green>`, `{\an8}`).
fn strip_markup(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut depth = 0usize;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Spoken text with its offset, ready for the shared chunker.
pub fn blocks(raw: &str) -> Result<(Vec<Block>, Vec<String>), String> {
    let cues = parse(raw)?;
    let mut warnings = Vec::new();
    let mut blocks = Vec::new();
    for cue in &cues {
        let mut text = cue.text.clone();
        if !cue.speaker.is_empty() && !text.contains(&cue.speaker) {
            text = format!("{}: {}", cue.speaker, text);
        }
        blocks.push(Block { text, locator: Locator::timestamp(cue.start_ms) });
    }
    if cues.iter().any(|c| c.end_ms <= c.start_ms) {
        warnings.push("some cues have a non-positive duration; their start offsets were kept".to_string());
    }
    Ok((blocks, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vtt_cues_with_millisecond_offsets() {
        let raw = "WEBVTT\n\nintro-1\n00:00:01.500 --> 00:00:04.000 align:start\n<v Roger>Forge is a research notebook.\n\n00:00:04.000 --> 00:00:07.250\nIt cites the evidence.\n";
        let cues = parse(raw).unwrap();
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].start_ms, 1_500);
        assert_eq!(cues[0].speaker, "intro-1");
        assert_eq!(cues[0].text, "Forge is a research notebook.");
        assert_eq!(cues[1].start_ms, 4_000);
        assert_eq!(cues[1].end_ms, 7_250);
    }

    #[test]
    fn parses_srt_with_comma_decimals_and_indices() {
        let raw = "1\n00:00:02,000 --> 00:00:05,500\nThe retention window is thirty days.\n\n2\n00:00:05,500 --> 00:00:09,000\nLegal holds extend it.\n";
        let cues = parse(raw).unwrap();
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].start_ms, 2_000);
        assert_eq!(cues[0].text, "The retention window is thirty days.");
    }

    #[test]
    fn rejects_text_without_timestamps() {
        let err = parse("just a paragraph\n\nand another").unwrap_err();
        assert!(err.contains("no timed cues"), "{}", err);
    }

    #[test]
    fn blocks_carry_timestamp_locators() {
        let raw = "WEBVTT\n\n00:00:42.000 --> 00:00:45.000\nThe pipeline writes a checkpoint before it indexes.\n";
        let (blocks, warnings) = blocks(raw).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(blocks[0].locator.kind, "timestamp");
        assert_eq!(blocks[0].locator.at_ms, Some(42_000));
        assert_eq!(blocks[0].locator.human(), "00:42");
    }
}
