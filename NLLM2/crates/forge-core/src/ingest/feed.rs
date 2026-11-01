//! RSS 2.0, Atom 1.0 and RSS 1.0 (RDF) feeds.
//!
//! A feed is a *list of dated items*, and that structure is what makes it citable: an answer
//! drawn from a feed must be able to say which item it came from and when that item was
//! published, not just "somewhere in this XML". So every block is one item, and the locator
//! carries the position and the title:
//!
//! ```text
//! Locator { kind: "item", page: Some(3), section: Some("3 · The retention model"), .. }
//! label: "item 3 · The retention model"
//! ```
//!
//! Parsing is deliberately hand-written: Forge has no dependencies, and a feed is a flat list
//! of elements — it does not need a general XML engine. What it does need is to *not lie*:
//! an item with no date is left without a date (never stamped with the fetch time), an item
//! with no body is skipped rather than cited as an empty quote, and a document that is not a
//! feed is refused with a reason instead of being indexed as prose.

use super::{text, Block, Parsed};
use crate::domain::Locator;

/// Beyond this an item is truncated; feeds are summaries, not books.
const ITEM_MAX_CHARS: usize = 4000;
/// A feed with more items than this keeps the newest-looking prefix and says so.
const ITEM_MAX: usize = 200;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Item {
    pub title: String,
    pub link: String,
    pub published: String,
    pub author: String,
    pub summary: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Feed {
    pub title: String,
    pub link: String,
    /// `rss`, `atom` or `rdf` — recorded so the source can say what it actually was.
    pub flavour: String,
    pub items: Vec<Item>,
    pub warnings: Vec<String>,
}

/// Cheap sniff used before parsing, and by `fetch` to classify a response.
pub fn looks_like_feed(bytes: &[u8]) -> bool {
    let head: String = String::from_utf8_lossy(bytes).chars().take(4096).collect();
    let mut lower = head.to_ascii_lowercase();
    if let Some(at) = lower.find("<rss") {
        lower.truncate(at + 64);
        return true;
    }
    let lower = head.to_ascii_lowercase();
    lower.contains("<feed") && (lower.contains("www.w3.org/2005/atom") || lower.contains("<entry"))
        || lower.contains("<rdf:rdf")
        || (lower.contains("<rss") && lower.contains("version"))
}

/// Return the text of `<tag ...>…</tag>`, CDATA unwrapped. `None` when absent.
fn raw_element<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{}", tag);
    let start = xml.find(&open)?;
    let after = &xml[start..];
    // reject `<itemize>` when looking for `<item`
    let close_of_open = after.find('>')?;
    let name_end = after[1..close_of_open].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == ':' || *c == '-' || *c == '_').count();
    if !after[..1 + name_end].eq_ignore_ascii_case(&open) {
        return None;
    }
    if after[..close_of_open].ends_with('/') {
        return Some(""); // self-closing
    }
    let close_tag = format!("</{}>", tag).to_ascii_lowercase();
    let body_start = start + close_of_open + 1;
    let body = &xml[body_start..];
    let end = body.to_ascii_lowercase().find(&close_tag)?;
    Some(&body[..end])
}

/// All occurrences of a repeating element (`item` / `entry`) inside `xml`.
fn elements<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = xml;
    let open = format!("<{}", tag);
    let close_tag = format!("</{}>", tag);
    while let Some(start) = rest.find(&open) {
        let after = &rest[start..];
        // `<item>` and `<item foo="1">` only — not `<itemize>`
        let name_len = after[1..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == ':' || *c == '-' || *c == '_').count();
        if !after[..1 + name_len].eq_ignore_ascii_case(&open) {
            rest = &after[1..];
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        if after[..gt].ends_with('/') {
            rest = &after[gt + 1..];
            continue;
        }
        let body = &after[gt + 1..];
        match body.to_ascii_lowercase().find(&close_tag) {
            Some(end) => {
                out.push(&body[..end]);
                rest = &body[end + close_tag.len()..];
            }
            // An unclosed item means the document is broken; stop rather than guess the rest.
            None => break,
        }
    }
    out
}

/// Unwrap CDATA and trim. Entities are **not** decoded here: decoding first would turn an
/// escaped `&lt;b&gt;` into markup that `strip_tags` would then delete as if it were a tag.
fn cdata_or_text(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(stripped) = trimmed.strip_prefix("<![CDATA[") {
        stripped.strip_suffix("]]>").unwrap_or(stripped).trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// An Atom `<link href="..."/>` or an RSS `<link>text</link>`.
fn link_of(item: &str) -> String {
    // Atom: prefer rel="alternate", otherwise the first href that is not self/enclosure.
    let mut fallback = String::new();
    let mut rest = item;
    while let Some(at) = rest.find("<link") {
        let after = &rest[at..];
        let Some(gt) = after.find('>') else { break };
        let tag = &after[..gt];
        let href = attr(tag, "href").unwrap_or_default();
        let rel = attr(tag, "rel").unwrap_or_else(|| "alternate".to_string());
        if !href.is_empty() && rel == "alternate" && fallback.is_empty() {
            fallback = href;
        } else if !href.is_empty() && fallback.is_empty() && rel != "self" && rel != "enclosure" {
            fallback = href;
        }
        rest = &after[gt + 1..];
    }
    if !fallback.is_empty() {
        return fallback;
    }
    // RSS: element text.
    if let Some(raw) = raw_element(item, "link") {
        let value = text::decode_entities(&cdata_or_text(raw)).trim().to_string();
        if !value.is_empty() {
            return value;
        }
    }
    // RSS 1.0 often repeats the URL as the element's rdf:resource.
    let lower = item.to_ascii_lowercase();
    if let Some(at) = lower.find("<link") {
        let tag = &item[at..];
        if let Some(gt) = tag.find('>') {
            if let Some(v) = attr(&tag[..gt], "rdf:resource") {
                return v;
            }
        }
    }
    String::new()
}

fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{}=", name.to_ascii_lowercase());
    let mut from = 0usize;
    while let Some(rel) = lower[from..].find(&needle) {
        let at = from + rel;
        // must be preceded by whitespace or '<', so `href` does not match `xhref`
        let prev = lower[..at].chars().last();
        if matches!(prev, Some(c) if c.is_whitespace() || c == '<') {
            let rest = &tag[at + needle.len()..];
            let quote = rest.chars().next()?;
            if quote == '"' || quote == '\'' {
                let value = &rest[1..];
                let end = value.find(quote)?;
                return Some(text::decode_entities(&value[..end]));
            }
            let end = rest.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(rest.len());
            return Some(text::decode_entities(rest[..end].trim()));
        }
        from = at + needle.len();
    }
    None
}

/// First present element among `names`.
fn first(item: &str, names: &[&str]) -> String {
    for name in names {
        if let Some(raw) = raw_element(item, name) {
            let value = cdata_or_text(raw);
            if !value.is_empty() {
                return value;
            }
        }
    }
    String::new()
}

/// Tags that are markup in a feed body. Anything else — `<y>`, `<3`, a URL with `<` in it —
/// is text, and is kept: over-stripping would silently delete the author's words.
const KNOWN_TAGS: &[&str] = &[
    "a", "abbr", "b", "blockquote", "br", "caption", "cite", "code", "dd", "del", "div", "dl",
    "dt", "em", "figcaption", "figure", "h1", "h2", "h3", "h4", "h5", "h6", "hr", "i", "iframe",
    "img", "ins", "li", "ol", "p", "pre", "q", "s", "small", "span", "strong", "sub", "sup",
    "table", "tbody", "td", "tfoot", "th", "thead", "tr", "u", "ul", "video", "audio", "source",
    "script", "style", "svg", "path", "name", "email", "uri", "label",
];

/// Tags whose removal separates words; whitespace is collapsed afterwards, so the extra space
/// only matters where two paragraphs would otherwise run together.
const BLOCK_TAGS: &[&str] = &[
    "p", "div", "br", "li", "ul", "ol", "h1", "h2", "h3", "h4", "h5", "h6", "blockquote", "pre",
    "tr", "td", "th", "table", "thead", "tbody", "tfoot", "dd", "dt", "dl", "figure",
    "figcaption", "hr", "caption",
];

fn strip_known_markup(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let rest = &text[i + 1..];
            let name: String = rest
                .chars()
                .skip_while(|c| *c == '/')
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            let lower = name.to_ascii_lowercase();
            let is_tag = KNOWN_TAGS.contains(&lower.as_str());
            if is_tag && !lower.is_empty() {
                match rest.find('>') {
                    Some(end) => {
                        if BLOCK_TAGS.contains(&lower.as_str()) {
                            out.push(' ');
                        }
                        i += 1 + end + 1;
                        continue;
                    }
                    None => break,
                }
            }
        }
        let ch = text[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn tidy_summary(raw: &str) -> String {
    // Feed bodies are usually HTML. Strip real markup first, *then* decode entities, and then
    // strip markup that was escaped into the text (`&lt;p&gt;` inside `<content type="html">`).
    // A tag name that is not on the known list is left alone, so `x <y>` survives.
    let plain = if raw.contains('<') { strip_known_markup(raw) } else { raw.to_string() };
    let plain = text::decode_entities(&plain);
    let plain = strip_known_markup(&plain);
    plain
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(ITEM_MAX_CHARS)
        .collect()
}

/// Parse a feed. `Err` means "this is not a feed", with the reason the user should see.
pub fn parse(bytes: &[u8]) -> Result<Feed, String> {
    let xml = super::decode_text(bytes);
    if xml.trim().is_empty() {
        return Err("the feed is empty".to_string());
    }
    let lower = xml.to_ascii_lowercase();

    let mut feed = Feed::default();
    let (flavour, root_title, raw_items) = if lower.contains("<rss") && lower.contains("<channel") {
        ("rss", first(&xml, &["title"]), elements(&xml, "item"))
    } else if lower.contains("<feed") {
        ("atom", first(&xml, &["title"]), elements(&xml, "entry"))
    } else if lower.contains("<rdf:rdf") {
        ("rdf", first(&xml, &["title"]), elements(&xml, "item"))
    } else {
        return Err(
            "not a feed: no <rss>, <feed> or <rdf:RDF> root element was found".to_string(),
        );
    };
    feed.flavour = flavour.to_string();
    feed.title = tidy_summary(&root_title);
    feed.link = if flavour == "atom" { link_of(&xml) } else { first(&xml, &["link"]) };
    if feed.link.contains('<') {
        feed.link = link_of(&xml);
    }

    if raw_items.is_empty() {
        return Err("the feed parses but contains no items".to_string());
    }

    for raw in raw_items {
        let title = tidy_summary(&first(raw, &["title"]));
        // `content:encoded` first: it is the body, `description` is often only a teaser.
        let summary = tidy_summary(&first(raw, &["content:encoded", "content", "summary", "description"]));
        let published = text::decode_entities(&first(raw, &["pubDate", "published", "updated", "dc:date", "date"])).trim().to_string();
        // Atom nests the name: `<author><name>A. Menon</name></author>`. tidy_summary strips
        // the wrapper, so both shapes give the same string.
        let author = tidy_summary(&first(raw, &["dc:creator", "creator", "author", "name"]));
        let link = link_of(raw);
        if summary.is_empty() && title.is_empty() {
            feed.warnings.push("an item had neither a title nor a body and was skipped".to_string());
            continue;
        }
        if published.is_empty() {
            // Deliberately not filled in with the fetch time: the publish date is the source's
            // fact, and inventing one would let an answer cite a date the feed never claimed.
            feed.warnings.push(format!(
                "item \"{}\" has no publish date; it is indexed without one",
                shorten(&title, 60)
            ));
        }
        feed.items.push(Item { title, link, published, author, summary });
    }

    if feed.items.is_empty() {
        return Err("every item in the feed was empty".to_string());
    }
    if feed.items.len() > ITEM_MAX {
        feed.warnings.push(format!(
            "the feed lists {} items; the first {} were indexed",
            feed.items.len(),
            ITEM_MAX
        ));
        feed.items.truncate(ITEM_MAX);
    }
    Ok(feed)
}

/// One block per item, in feed order (feeds are ordered by the publisher, so that order is
/// kept rather than re-sorted: reordering would change which item is "item 1").
pub fn blocks(feed: &Feed) -> Vec<Block> {
    let mut out = Vec::new();
    for (idx, item) in feed.items.iter().enumerate() {
        let n = idx as u32 + 1;
        let heading = if item.title.is_empty() {
            format!("item {}", n)
        } else {
            item.title.clone()
        };
        let mut body = String::new();
        body.push_str(&heading);
        let mut meta: Vec<String> = Vec::new();
        if !item.published.is_empty() {
            meta.push(item.published.clone());
        }
        if !item.author.is_empty() {
            meta.push(item.author.clone());
        }
        if !item.link.is_empty() {
            meta.push(item.link.clone());
        }
        if !meta.is_empty() {
            body.push('\n');
            body.push_str(&meta.join(" · "));
        }
        if !item.summary.is_empty() {
            body.push('\n');
            body.push_str(&item.summary);
        }
        let section = if item.title.is_empty() { format!("{}", n) } else { format!("{} · {}", n, item.title) };
        out.push(Block {
            text: body,
            locator: Locator::item(n, &shorten(&section, 120)),
        });
    }
    out
}

/// The `Parsed` form used by the ingestion pipeline.
pub fn feed(bytes: &[u8]) -> Result<Parsed, String> {
    let parsed = parse(bytes)?;
    let mut warnings = parsed.warnings.clone();
    warnings.push(format!(
        "feed read as {}: {} item(s){}",
        match parsed.flavour.as_str() {
            "atom" => "Atom 1.0",
            "rdf" => "RSS 1.0 (RDF)",
            _ => "RSS 2.0",
        },
        parsed.items.len(),
        if parsed.title.is_empty() { String::new() } else { format!(" from \"{}\"", parsed.title) }
    ));
    let blocks = blocks(&parsed);
    Ok(Parsed { blocks: super::chunk_blocks(blocks), warnings, page_count: parsed.items.len() as u32, tables: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
  <title>Retention Weekly</title>
  <link>https://example.test/</link>
  <description>notes on retention</description>
  <item>
    <title>Churn fell after the onboarding rewrite</title>
    <link>https://example.test/posts/onboarding</link>
    <pubDate>Tue, 03 Mar 2026 09:00:00 GMT</pubDate>
    <dc:creator>R. Okafor</dc:creator>
    <description><![CDATA[<p>Monthly churn fell from 4.1% to 2.9% in the quarter after the rewrite.</p>]]></description>
  </item>
  <item>
    <title>Legal hold overrides the thirty day window</title>
    <link>https://example.test/posts/legal-hold</link>
    <pubDate>Wed, 04 Mar 2026 12:30:00 GMT</pubDate>
    <description>A legal hold suspends deletion regardless of the retention window.</description>
  </item>
</channel></rss>"#;

    const ATOM: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Forge release notes</title>
  <link href="https://example.test/atom"/>
  <entry>
    <title>Version 3 stores item dates</title>
    <link rel="alternate" href="https://example.test/atom/3"/>
    <published>2026-02-01T10:00:00Z</published>
    <author><name>A. Menon</name></author>
    <summary>Feeds are now read as dated items.</summary>
  </entry>
  <entry>
    <title>Version 2 added EPUB</title>
    <link rel="alternate" href="https://example.test/atom/2"/>
    <updated>2026-01-15T08:00:00Z</updated>
    <summary>EPUB files are read in spine order.</summary>
  </entry>
</feed>"#;

    #[test]
    fn rss_items_keep_their_own_dates_authors_and_bodies() {
        let feed = parse(RSS.as_bytes()).expect("rss parses");
        assert_eq!(feed.flavour, "rss");
        assert_eq!(feed.title, "Retention Weekly");
        assert_eq!(feed.items.len(), 2);
        assert_eq!(feed.items[0].published, "Tue, 03 Mar 2026 09:00:00 GMT");
        assert_eq!(feed.items[0].author, "R. Okafor");
        // CDATA is unwrapped and the HTML inside it is stripped, not indexed as markup.
        assert_eq!(
            feed.items[0].summary,
            "Monthly churn fell from 4.1% to 2.9% in the quarter after the rewrite."
        );
        assert_eq!(feed.items[1].link, "https://example.test/posts/legal-hold");
    }

    #[test]
    fn atom_entries_use_the_alternate_link_and_both_date_elements() {
        let feed = parse(ATOM.as_bytes()).expect("atom parses");
        assert_eq!(feed.flavour, "atom");
        assert_eq!(feed.items.len(), 2);
        assert_eq!(feed.items[0].link, "https://example.test/atom/3");
        assert_eq!(feed.items[0].published, "2026-02-01T10:00:00Z");
        assert_eq!(feed.items[0].author, "A. Menon");
        // `<updated>` is a date too, and is used when `<published>` is absent.
        assert_eq!(feed.items[1].published, "2026-01-15T08:00:00Z");
        assert!(feed.warnings.is_empty(), "{:?}", feed.warnings);
    }

    #[test]
    fn an_item_without_a_date_is_left_without_one_and_says_so() {
        let xml = r#"<rss version="2.0"><channel><title>t</title>
            <item><title>undated</title><description>body text</description></item>
            </channel></rss>"#;
        let feed = parse(xml.as_bytes()).expect("parses");
        assert_eq!(feed.items[0].published, "");
        assert!(feed.warnings.iter().any(|w| w.contains("no publish date")), "{:?}", feed.warnings);
        let blocks = blocks(&feed);
        assert!(!blocks[0].text.contains("©"), "no invented metadata");
    }

    #[test]
    fn locators_point_at_the_item_and_read_as_item_n_title() {
        let feed = parse(ATOM.as_bytes()).expect("parses");
        let blocks = blocks(&feed);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].locator.kind, "item");
        assert_eq!(blocks[1].locator.page, Some(2));
        assert_eq!(blocks[1].locator.human(), "item 2 · Version 2 added EPUB");
        assert!(blocks[1].text.starts_with("Version 2 added EPUB\n2026-01-15T08:00:00Z · "));
    }

    #[test]
    fn a_document_that_is_not_a_feed_is_refused_with_a_reason() {
        let e = parse(b"<html><body><h1>not a feed</h1></body></html>").unwrap_err();
        assert!(e.contains("not a feed"), "{}", e);
        let e = parse(b"<rss version=\"2.0\"><channel><title>empty</title></channel></rss>").unwrap_err();
        assert!(e.contains("no items"), "{}", e);
    }

    #[test]
    fn rdf_feeds_are_recognised_and_look_like_feed_does_not_overreach() {
        let rdf = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
              xmlns="http://purl.org/rss/1.0/"><channel><title>Old feed</title>
              <link>https://example.test/</link></channel>
              <item rdf:about="https://example.test/a"><title>A</title>
              <link>https://example.test/a</link><description>first</description></item></rdf:RDF>"#;
        let feed = parse(rdf.as_bytes()).expect("rdf parses");
        assert_eq!(feed.flavour, "rdf");
        assert_eq!(feed.items[0].link, "https://example.test/a");

        assert!(looks_like_feed(RSS.as_bytes()));
        assert!(looks_like_feed(ATOM.as_bytes()));
        assert!(!looks_like_feed(b"<html><body>hi</body></html>"));
        assert!(!looks_like_feed(b"{\"json\":true}"));
    }

    #[test]
    fn escaped_html_in_a_content_element_is_markup_not_prose() {
        // Real shape from a GitHub releases Atom feed: the body is escaped HTML.
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Releases</title>
            <entry><title>v1.2</title><published>2026-01-02T03:04:05Z</published>
            <content type="html">&lt;p&gt;Fixes the retry loop.&lt;/p&gt;&lt;p&gt;Adds EPUB.&lt;/p&gt;</content>
            </entry></feed>"#;
        let feed = parse(xml.as_bytes()).expect("parses");
        assert_eq!(feed.items[0].summary, "Fixes the retry loop. Adds EPUB.", "{:?}", feed.items[0].summary);
        assert!(!feed.items[0].summary.contains('<'), "no markup leaks into the evidence text");
    }

    #[test]
    fn a_less_than_that_is_not_a_tag_is_kept() {
        assert_eq!(tidy_summary("x <y> and 3 < 4"), "x <y> and 3 < 4");
    }

    #[test]
    fn entity_encoded_titles_are_decoded() {
        let xml = r#"<rss version="2.0"><channel><title>A &amp; B</title>
            <item><title>Q&amp;A &#8212; retention</title><description>x &lt;y&gt;</description></item>
            </channel></rss>"#;
        let feed = parse(xml.as_bytes()).expect("parses");
        assert_eq!(feed.title, "A & B");
        assert_eq!(feed.items[0].summary, "x <y>");
    }

    #[test]
    fn the_pipeline_reports_what_it_read() {
        let parsed = feed(RSS.as_bytes()).expect("parses");
        assert_eq!(parsed.page_count, 2);
        assert!(parsed.warnings[0].starts_with("feed read as RSS 2.0: 2 item(s)"), "{:?}", parsed.warnings);
        assert_eq!(parsed.blocks.len(), 2);
    }
}
