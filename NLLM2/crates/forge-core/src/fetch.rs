//! Outbound URL fetching, with the SSRF policy enforced at every hop.
//!
//! Why `curl` and not a Rust client: this build has no TLS crate (crates.io is
//! unreachable — `docs/DECISIONS.md` D1), and the reference product's URL sources
//! are overwhelmingly https. `curl` is a mature, ubiquitous HTTP client, invoked
//! here as a subprocess with an explicit argument vector — never through a shell —
//! and never with credentials, cookies, or a proxy configuration.
//!
//! The security properties are ours, not curl's:
//!
//! 1. the URL is parsed by `security::parse_url` (scheme, userinfo, control chars);
//! 2. the host is resolved by us, and **every** address must be public;
//! 3. curl is pinned with `--resolve host:port:ip` so its own DNS lookup cannot
//!    disagree with ours (this is what defeats DNS rebinding);
//! 4. redirects are *not* followed by curl (`--max-redirs 0`); each `Location` is
//!    re-validated and re-pinned, with a hop cap;
//! 5. body size and time are capped, and `--proto`/`--proto-redir` restrict the
//!    scheme set even if a Location points somewhere unexpected.

use crate::security::{self, ParsedUrl};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Default ceilings for a single fetch.
pub const MAX_FETCH_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_HOPS: usize = 5;
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

pub const USER_AGENT: &str = "ForgeBot/0.1 (local research workspace; +https://example.invalid/forge)";

#[derive(Debug, Clone, PartialEq)]
pub enum FetchError {
    /// The URL was refused before any network activity.
    Rejected(String),
    /// No transport could be started (curl missing, spawn failure).
    NoTransport(String),
    /// The transport ran but the request failed or returned a non-success status.
    Failed(String),
    /// The response was too large or took too long.
    Limit(String),
}

impl FetchError {
    pub fn message(&self) -> String {
        match self {
            FetchError::Rejected(m) => format!("refused: {}", m),
            FetchError::NoTransport(m) => format!("no fetch transport: {}", m),
            FetchError::Failed(m) => format!("fetch failed: {}", m),
            FetchError::Limit(m) => format!("limit exceeded: {}", m),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            FetchError::Rejected(_) => "url_rejected",
            FetchError::NoTransport(_) => "fetch_transport_unavailable",
            FetchError::Failed(_) => "fetch_failed",
            FetchError::Limit(_) => "fetch_limit_exceeded",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Fetched {
    /// The URL that produced the body after following redirects.
    pub final_url: String,
    /// Normalised identity of `final_url`, used for duplicate detection.
    pub canonical_url: String,
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
    pub redirects: Vec<(u16, String)>,
}

impl Fetched {
    /// Media type without parameters, lower-cased (`text/html; charset=utf-8` → `text/html`).
    pub fn media_type(&self) -> String {
        self.content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
    }

    /// Best guess at the ingest kind for `ingest::extract`, from Content-Type first
    /// and the URL path second.
    pub fn ingest_kind(&self) -> &'static str {
        let mt = self.media_type();
        let path = self.final_url.split(['?', '#']).next().unwrap_or("").to_ascii_lowercase();
        let ends = |ext: &str| path.ends_with(&format!(".{}", ext));
        if mt == "application/pdf" || ends("pdf") || self.body.starts_with(b"%PDF") {
            return "pdf";
        }
        if mt == "text/html" || mt == "application/xhtml+xml" || ends("html") || ends("htm") {
            return "html";
        }
        if mt == "text/markdown" || ends("md") || ends("markdown") {
            return "markdown";
        }
        if mt == "text/csv" || ends("csv") {
            return "csv";
        }
        if mt == "text/tab-separated-values" || ends("tsv") {
            return "tsv";
        }
        // JSON, plain text, and anything unrecognised are treated as text: better to
        // index a readable body than to reject a source the user asked for.
        "text"
    }
}

/// Provenance for a source created by fetching a URL.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FetchedMeta {
    pub canonical_url: String,
    pub fetched_at: String,
    pub status: u16,
    pub content_type: String,
    pub redirects: usize,
}

/// Is a working HTTP transport present?
pub fn transport_available() -> bool {
    Command::new("curl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn temp_paths() -> (PathBuf, PathBuf) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let base = std::env::temp_dir();
    (
        base.join(format!("forge-fetch-{}-{}.body", pid, stamp)),
        base.join(format!("forge-fetch-{}-{}.hdr", pid, stamp)),
    )
}

/// One request, no redirect following. Returns (status, headers text, body).
fn request_once(url: &ParsedUrl, ip: std::net::IpAddr, max_bytes: usize) -> Result<(u16, String, Vec<u8>), FetchError> {
    let (body_path, hdr_path) = temp_paths();
    let pin = if url.host.contains(':') {
        format!("[{}]:{}:{}", url.host, url.port, ip)
    } else {
        format!("{}:{}:{}", url.host, url.port, ip)
    };
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--no-progress-meter",
            "--proto",
            "=http,https",
            "--proto-redir",
            "=http,https",
            "--max-redirs",
            "0",
            "--max-time",
            &FETCH_TIMEOUT.as_secs().to_string(),
            "--max-filesize",
            &max_bytes.to_string(),
            "--user-agent",
            USER_AGENT,
            "--header",
            "Accept: text/html,application/xhtml+xml,text/plain;q=0.9,*/*;q=0.5",
            "--resolve",
            &pin,
            "--dump-header",
            &hdr_path.to_string_lossy(),
            "--output",
            &body_path.to_string_lossy(),
            "--write-out",
            "%{http_code}",
            &url.to_url(),
        ])
        .output();

    let cleanup = || {
        let _ = std::fs::remove_file(&body_path);
        let _ = std::fs::remove_file(&hdr_path);
    };

    let out = match output {
        Ok(o) => o,
        Err(e) => {
            cleanup();
            return Err(FetchError::NoTransport(format!("cannot run curl: {}", e)));
        }
    };
    let headers = std::fs::read_to_string(&hdr_path).unwrap_or_default();
    let body = std::fs::read(&body_path).unwrap_or_default();
    cleanup();

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        // curl exit code 63 = max-filesize exceeded
        return if out.status.code() == Some(63) {
            Err(FetchError::Limit(format!(
                "response body exceeds {} bytes",
                max_bytes
            )))
        } else if out.status.code() == Some(28) {
            Err(FetchError::Limit(format!(
                "request exceeded {}s",
                FETCH_TIMEOUT.as_secs()
            )))
        } else {
            Err(FetchError::Failed(if stderr.is_empty() {
                format!("curl exited {:?}", out.status.code())
            } else {
                stderr
            }))
        };
    }
    let status: u16 = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0);
    Ok((status, headers, body))
}

fn header_value(headers: &str, name: &str) -> Option<String> {
    let want = format!("{}:", name.to_ascii_lowercase());
    // Last occurrence wins: with `--dump-header` a redirect chain can appear, and we
    // only ever issue single-hop requests, so the last block is the real one.
    headers
        .lines()
        .filter(|l| l.to_ascii_lowercase().starts_with(&want))
        .last()
        .map(|l| l[want.len()..].trim().to_string())
}

fn resolve_location(base: &ParsedUrl, location: &str) -> Result<ParsedUrl, FetchError> {
    let loc = location.trim();
    if loc.starts_with("http://") || loc.starts_with("https://") {
        return security::parse_url(loc).map_err(FetchError::Rejected);
    }
    if loc.starts_with("//") {
        return security::parse_url(&format!("{}:{}", base.scheme, loc)).map_err(FetchError::Rejected);
    }
    if loc.starts_with('/') {
        let (path, query) = match loc.split_once('?') {
            Some((p, q)) => (p, format!("?{}", q)),
            None => (loc, String::new()),
        };
        return Ok(ParsedUrl {
            scheme: base.scheme.clone(),
            host: base.host.clone(),
            port: base.port,
            path_query: format!("{}{}", path, query),
        });
    }
    // Relative to the current directory
    let base_dir = base.path_query.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    Ok(ParsedUrl {
        scheme: base.scheme.clone(),
        host: base.host.clone(),
        port: base.port,
        path_query: format!("{}/{}", base_dir, loc),
    })
}

/// Fetch a URL, validating policy at every hop.
pub fn fetch(raw_url: &str) -> Result<Fetched, FetchError> {
    fetch_with_limits(raw_url, MAX_FETCH_BYTES)
}

pub fn fetch_with_limits(raw_url: &str, max_bytes: usize) -> Result<Fetched, FetchError> {
    let mut url = security::parse_url(raw_url).map_err(FetchError::Rejected)?;
    let mut redirects: Vec<(u16, String)> = Vec::new();

    for hop in 0..=MAX_HOPS {
        if hop == MAX_HOPS {
            return Err(FetchError::Rejected(format!(
                "more than {} redirects",
                MAX_HOPS
            )));
        }
        let ips = security::resolve_public(&url.host, url.port).map_err(|e| match e {
            security::ResolveError::Blocked(m) => FetchError::Rejected(m),
            security::ResolveError::Dns(m) => FetchError::Failed(m),
        })?;
        let ip = ips[0];
        let (status, headers, body) = request_once(&url, ip, max_bytes)?;

        if (300..400).contains(&status) {
            let location = header_value(&headers, "location")
                .ok_or_else(|| FetchError::Failed(format!("HTTP {} without Location", status)))?;
            let next = resolve_location(&url, &location)?;
            redirects.push((status, next.to_url()));
            url = next;
            continue;
        }
        if !(200..300).contains(&status) {
            return Err(FetchError::Failed(format!(
                "HTTP {} for {}",
                status,
                url.to_url()
            )));
        }
        if body.len() > max_bytes {
            return Err(FetchError::Limit(format!(
                "body of {} bytes exceeds {}",
                body.len(),
                max_bytes
            )));
        }
        let content_type = header_value(&headers, "content-type").unwrap_or_default();
        return Ok(Fetched {
            final_url: url.to_url(),
            canonical_url: url.canonical(),
            status,
            content_type,
            body,
            redirects,
        });
    }
    unreachable!("loop always returns")
}

/// Remove a fragment and normalise a URL for duplicate detection without network access.
pub fn canonicalize(raw: &str) -> Option<String> {
    security::parse_url(raw).ok().map(|u| u.canonical())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // ---- one shared loopback fixture server -------------------------------
    //
    // Private-range fetching is off by default; the test opts in explicitly for the
    // duration of the test. Because the flag is process-global and tests run in
    // parallel, every test that needs the server goes through `with_server`, which
    // holds a lock.
    static SERVER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    static HITS: AtomicUsize = AtomicUsize::new(0);

    fn with_server<T>(f: impl FnOnce(u16) -> T) -> T {
        let _guard = SERVER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("FORGE_FETCH_ALLOW_PRIVATE", "1");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming().take(6) {
                let mut stream = match stream {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                HITS.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                let _ = reader.read_line(&mut first);
                let path = first.split_whitespace().nth(1).unwrap_or("/").to_string();
                // drain headers
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                }
                let (status, ctype, body): (u16, &str, Vec<u8>) = match path.as_str() {
                    "/page" => (
                        200,
                        "text/html; charset=utf-8",
                        b"<html><head><title>Fixture Page</title></head><body>\
                          <h1>Retrieval</h1><p>Hybrid retrieval combines lexical and dense signals.</p>\
                          <h2>Grounding</h2><p>Every claim must cite an evidence span.</p></body></html>"
                            .to_vec(),
                    ),
                    "/doc.pdf" => (200, "application/pdf", b"%PDF-1.4\n%fake\n".to_vec()),
                    "/redirect" => (302, "text/plain", b"".to_vec()),
                    "/loop" => (302, "text/plain", b"".to_vec()),
                    "/big" => (200, "text/plain", vec![b'x'; 3 * 1024 * 1024]),
                    "/missing" => (404, "text/plain", b"nope".to_vec()),
                    _ => (200, "text/plain", b"plain text body about widgets".to_vec()),
                };
                let location = match path.as_str() {
                    "/redirect" => "Location: /page\r\n",
                    "/loop" => "Location: /loop\r\n",
                    _ => "",
                };
                let head = format!(
                    "HTTP/1.1 {} X\r\nContent-Type: {}\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    status, ctype, location, body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
                let _ = stream.flush();
            }
        });
        let out = f(port);
        // let the thread finish its accept loop
        drop(handle);
        std::env::remove_var("FORGE_FETCH_ALLOW_PRIVATE");
        out
    }

    #[test]
    fn transport_is_present_in_this_image() {
        assert!(transport_available(), "curl must be available for URL ingestion");
    }

    #[test]
    fn fetches_html_and_classifies_it() {
        with_server(|port| {
            let got = fetch(&format!("http://127.0.0.1:{}/page", port)).expect("fetch");
            assert_eq!(got.status, 200);
            assert_eq!(got.media_type(), "text/html");
            assert_eq!(got.ingest_kind(), "html");
            assert!(String::from_utf8_lossy(&got.body).contains("Hybrid retrieval"));
            assert_eq!(got.canonical_url, format!("http://127.0.0.1:{}/page", port));
        });
    }

    #[test]
    fn follows_redirects_hop_by_hop_and_records_them() {
        with_server(|port| {
            let got = fetch(&format!("http://127.0.0.1:{}/redirect", port)).expect("fetch");
            assert_eq!(got.status, 200);
            assert!(got.final_url.ends_with("/page"), "final: {}", got.final_url);
            assert_eq!(got.redirects.len(), 1);
            assert_eq!(got.redirects[0].0, 302);
        });
    }

    #[test]
    fn refuses_a_redirect_loop() {
        with_server(|port| {
            let err = fetch(&format!("http://127.0.0.1:{}/loop", port)).unwrap_err();
            assert!(matches!(err, FetchError::Rejected(_)), "{:?}", err);
            assert!(err.message().contains("redirects"), "{}", err.message());
        });
    }

    #[test]
    fn enforces_the_size_cap() {
        with_server(|port| {
            let err = fetch_with_limits(&format!("http://127.0.0.1:{}/big", port), 1024 * 1024)
                .unwrap_err();
            assert!(matches!(err, FetchError::Limit(_)), "{:?}", err);
        });
    }

    #[test]
    fn reports_non_success_status() {
        with_server(|port| {
            let err = fetch(&format!("http://127.0.0.1:{}/missing", port)).unwrap_err();
            assert!(err.message().contains("404"), "{}", err.message());
        });
    }

    #[test]
    fn detects_pdf_by_magic_even_without_a_content_type() {
        with_server(|port| {
            let got = fetch(&format!("http://127.0.0.1:{}/doc.pdf", port)).expect("fetch");
            assert_eq!(got.ingest_kind(), "pdf");
        });
    }

    #[test]
    fn private_addresses_are_refused_when_the_override_is_off() {
        // holds SERVER_LOCK because it mutates the same process-global flag
        let _guard = SERVER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("FORGE_FETCH_ALLOW_PRIVATE");
        assert!(!security::allow_private());
        let err = fetch("http://127.0.0.1:9/nope").unwrap_err();
        assert!(matches!(err, FetchError::Rejected(_)), "{:?}", err);
        // and the classic metadata endpoint
        let err = fetch("http://169.254.169.254/latest/meta-data/").unwrap_err();
        assert!(matches!(err, FetchError::Rejected(_)));
    }

    #[test]
    fn rejects_bad_schemes_and_credentials() {
        assert!(matches!(fetch("file:///etc/passwd").unwrap_err(), FetchError::Rejected(_)));
        assert!(matches!(fetch("gopher://x/").unwrap_err(), FetchError::Rejected(_)));
        let err = fetch("https://user:pass@example.com/").unwrap_err();
        assert!(err.message().contains("credentials"), "{}", err.message());
    }

    #[test]
    fn canonicalization_matches_equivalent_urls() {
        let a = canonicalize("HTTP://Example.COM:80/a/b#frag").unwrap();
        let b = canonicalize("http://example.com/a/b").unwrap();
        assert_eq!(a, b, "scheme/host case, default port and fragment must normalise");
        let c = canonicalize("https://example.com").unwrap();
        assert_eq!(c, "https://example.com/");
        // query strings are preserved (they often select different content)
        assert_ne!(
            canonicalize("https://e.com/x?a=1").unwrap(),
            canonicalize("https://e.com/x?a=2").unwrap()
        );
    }

    #[test]
    fn reads_bodies_and_ignores_short_reads() {
        // a guard against regressing to a fixed-size read
        with_server(|port| {
            let got = fetch(&format!("http://127.0.0.1:{}/page", port)).expect("fetch");
            let mut buf = Vec::new();
            got.body.as_slice().read_to_end(&mut buf).unwrap();
            assert!(buf.len() > 100);
        });
    }
}
