//! Source discovery from pages the user chose.
//!
//! Forge does **not** query a search engine. There is no search API key, no scraping of a
//! search results page, and no invented result list: doing any of those would either break a
//! provider's terms or mean making up citations. What this module does is narrower and honest:
//! given seed URLs the user picked, it reads the links those pages actually contain, resolves
//! them, and offers them as *candidates with provenance* — every candidate records the page it
//! was found on. The user then decides which candidates become sources.
//!
//! A "search multiple independent domains" step is therefore reported as: N candidate hosts
//! discovered from M seeds, with the exact anchor text and referring URL for each.

use crate::ingest::text;

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// Absolute http(s) URL, fragment stripped.
    pub url: String,
    /// Host (with a non-default port when present), used to report how many independent
    /// origins were reached without ever calling that a "search result".
    pub host: String,
    /// The link text, as it appears on the page.
    pub anchor: String,
    /// The page the link was found on.
    pub from_url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Seed {
    pub url: String,
    pub status: u16,
    pub title: String,
    pub error: String,
    pub links_seen: usize,
    pub links_kept: usize,
}

/// Extensions that are never worth importing as a *page*: they would be refused by the upload
/// rules anyway (media) or need a viewer Forge does not have.
fn skipped_extension(url: &str) -> Option<&'static str> {
    let path = url.split(['?', '#']).next().unwrap_or("");
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    const MEDIA: &[&str] = &[
        "mp4", "m4v", "mov", "mkv", "webm", "avi", "mp3", "m4a", "wav", "flac", "aac", "ogg",
    ];
    const BINARY: &[&str] = &["exe", "dll", "apk", "iso", "dmg", "zip", "gz", "tar", "7z", "rar"];
    const IMAGE: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp"];
    if MEDIA.contains(&ext.as_str()) {
        return Some("media");
    }
    if BINARY.contains(&ext.as_str()) {
        return Some("archive");
    }
    if IMAGE.contains(&ext.as_str()) {
        return Some("image");
    }
    None
}

/// Resolve `href` against `base`, or `None` when it cannot be a page Forge may fetch.
pub fn resolve(base: &str, href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() || href.starts_with('#') {
        return None;
    }
    let lower = href.to_ascii_lowercase();
    if lower.starts_with("mailto:")
        || lower.starts_with("javascript:")
        || lower.starts_with("tel:")
        || lower.starts_with("data:")
    {
        return None;
    }
    let absolute = if lower.starts_with("http://") || lower.starts_with("https://") {
        href.to_string()
    } else if let Some(rest) = href.strip_prefix("//") {
        let scheme = base.split("://").next().unwrap_or("https");
        format!("{}://{}", scheme, rest)
    } else if href.starts_with('/') {
        let (scheme, host) = split_origin(base)?;
        format!("{}://{}{}", scheme, host, href)
    } else {
        let (scheme, host) = split_origin(base)?;
        let path = base.split(['?', '#']).next().unwrap_or(base);
        let dir = match path.rfind('/') {
            Some(i) if i > scheme.len() + 3 => &path[..=i],
            _ => &format!("{}://{}/", scheme, host),
        };
        format!("{}{}", dir, href)
    };
    // strip the fragment: it never changes which document is fetched
    let clean = absolute.split('#').next().unwrap_or("").to_string();
    if clean.is_empty() {
        return None;
    }
    Some(clean)
}

fn split_origin(url: &str) -> Option<(String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    if host.is_empty() {
        return None;
    }
    Some((scheme.to_ascii_lowercase(), host.to_ascii_lowercase()))
}

pub fn host_of(url: &str) -> String {
    split_origin(url).map(|(_, h)| h).unwrap_or_default()
}

/// Candidates from one page. Order is document order; duplicates (including links back to the
/// page itself) are dropped, and the result is capped.
pub fn candidates_from(seed_url: &str, html: &str, limit: usize) -> (Vec<Candidate>, usize, Vec<String>) {
    let mut warnings = Vec::new();
    let links = text::html_links(html);
    let seen_links = links.len();
    let mut out: Vec<Candidate> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let seed_canon = seed_url.trim_end_matches('/').to_ascii_lowercase();
    seen.insert(seed_canon);
    let mut skipped = 0usize;
    for (href, anchor) in links {
        if out.len() >= limit {
            break;
        }
        let Some(url) = resolve(seed_url, &href) else { continue };
        let lower = url.to_ascii_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            continue;
        }
        if let Some(kind) = skipped_extension(&url) {
            let _ = kind; // counted below, never silently dropped
            skipped += 1;
            continue;
        }
        let key = url.trim_end_matches('/').to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        let host = host_of(&url);
        if host.is_empty() {
            continue;
        }
        out.push(Candidate { url, host, anchor, from_url: seed_url.to_string() });
    }
    if skipped > 0 {
        warnings.push(format!(
            "{} link(s) pointed at media, archives or images and were not offered as page candidates",
            skipped
        ));
    }
    if out.is_empty() && seen_links > 0 {
        warnings.push("this page has links, but none of them could be offered as a page source".to_string());
    }
    if seen_links == 0 {
        warnings.push("this page contains no links".to_string());
    }
    (out, seen_links, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_forms_against_the_seed() {
        let base = "https://example.org/guides/index.html";
        assert_eq!(resolve(base, "/about").as_deref(), Some("https://example.org/about"));
        assert_eq!(resolve(base, "deep.html").as_deref(), Some("https://example.org/guides/deep.html"));
        assert_eq!(resolve(base, "//cdn.example.net/x").as_deref(), Some("https://cdn.example.net/x"));
        assert_eq!(resolve(base, "https://other.test/a#frag").as_deref(), Some("https://other.test/a"));
        assert_eq!(resolve(base, "#section"), None);
        assert_eq!(resolve(base, "mailto:x@y"), None);
        assert_eq!(resolve(base, "javascript:void(0)"), None);
    }

    #[test]
    fn candidates_carry_provenance_and_skip_media() {
        let html = r##"
            <h1>Reading list</h1>
            <a href="https://alpha.test/paper">Alpha paper</a>
            <a href="/local/page">Local page</a>
            <a href="https://beta.test/talk.mp4">A talk</a>
            <a href="https://alpha.test/paper">Alpha paper (again)</a>
            <a href="#top">Back to top</a>
        "##;
        let (cands, seen, warnings) = candidates_from("https://example.org/list", html, 10);
        assert_eq!(seen, 5);
        assert_eq!(cands.len(), 2, "the duplicate and the media link must not appear: {:?}", cands);
        assert_eq!(cands[0].url, "https://alpha.test/paper");
        assert_eq!(cands[0].host, "alpha.test");
        assert_eq!(cands[0].anchor, "Alpha paper");
        assert_eq!(cands[0].from_url, "https://example.org/list");
        assert!(cands.iter().any(|c| c.url == "https://example.org/local/page"));
        assert!(warnings.iter().any(|w| w.contains("media")), "{:?}", warnings);
    }

    #[test]
    fn host_reports_the_origin_including_a_port() {
        assert_eq!(host_of("http://127.0.0.1:8080/a/b"), "127.0.0.1:8080");
        assert_eq!(host_of("https://Example.ORG/x"), "example.org");
    }
}
