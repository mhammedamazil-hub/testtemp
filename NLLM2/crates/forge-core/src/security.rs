//! Security helpers: URL/SSRF policy, path safety, size limits, output escaping.

use std::net::{IpAddr, ToSocketAddrs};

pub const MAX_SOURCE_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_URL_LEN: usize = 2048;

#[derive(Debug, PartialEq)]
pub enum UrlVerdict {
    Allowed(String),
    Rejected(String),
}

/// Validate a URL for ingestion.
///
/// Policy: only http/https; never localhost, link-local, private, CGNAT,
/// multicast or unspecified addresses; hostnames are resolved and ALL resolved
/// addresses must be public (defeats DNS-rebinding to an internal service).
pub fn check_url(raw: &str) -> UrlVerdict {
    let raw = raw.trim();
    if raw.is_empty() {
        return UrlVerdict::Rejected("empty url".into());
    }
    if raw.len() > MAX_URL_LEN {
        return UrlVerdict::Rejected("url too long".into());
    }
    let lower = raw.to_ascii_lowercase();
    let (scheme, rest) = match lower.split_once("://") {
        Some(v) => v,
        None => return UrlVerdict::Rejected("missing scheme (http/https)".into()),
    };
    if scheme != "http" && scheme != "https" {
        return UrlVerdict::Rejected(format!("scheme '{}' is not allowed", scheme));
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() {
        return UrlVerdict::Rejected("missing host".into());
    }
    let host_port = authority.rsplit('@').next().unwrap_or(authority); // strip userinfo
    let host = if host_port.starts_with('[') {
        // IPv6 literal
        host_port
            .split(']')
            .next()
            .unwrap_or("")
            .trim_start_matches('[')
            .to_string()
    } else {
        host_port.split(':').next().unwrap_or("").to_string()
    };
    if host.is_empty() {
        return UrlVerdict::Rejected("missing host".into());
    }
    let host_lower = host.to_ascii_lowercase();
    if host_lower == "localhost" || host_lower.ends_with(".localhost") || host_lower.ends_with(".local") {
        return UrlVerdict::Rejected("localhost/.local hosts are blocked".into());
    }
    if host_lower == "metadata" || host_lower.contains("metadata.google.internal") {
        return UrlVerdict::Rejected("cloud metadata hosts are blocked".into());
    }
    if let Ok(ip) = host_lower.parse::<IpAddr>() {
        if !is_public_ip(&ip) {
            return UrlVerdict::Rejected(format!("address {} is not public", ip));
        }
        return UrlVerdict::Allowed(lower);
    }
    // resolve and check every address
    match (host_lower.as_str(), 80u16).to_socket_addrs() {
        Ok(addrs) => {
            let ips: Vec<IpAddr> = addrs.map(|a| a.ip()).collect();
            if ips.is_empty() {
                return UrlVerdict::Rejected("host did not resolve".into());
            }
            for ip in ips {
                if !is_public_ip(&ip) {
                    return UrlVerdict::Rejected(format!(
                        "host resolves to non-public address {}",
                        ip
                    ));
                }
            }
            UrlVerdict::Allowed(lower)
        }
        Err(e) => UrlVerdict::Rejected(format!("dns resolution failed: {}", e)),
    }
}

/// The pieces of an http(s) URL that matter for a safe fetch.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedUrl {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    /// Path plus query, always starting with `/`.
    pub path_query: String,
}

impl ParsedUrl {
    pub fn authority(&self) -> String {
        let default = if self.scheme == "https" { 443 } else { 80 };
        if self.port == default {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// Rebuild the URL we will actually request.
    pub fn to_url(&self) -> String {
        format!("{}://{}{}", self.scheme, self.authority(), self.path_query)
    }

    /// A normalised identity for de-duplication: lower-cased scheme/host, default
    /// port dropped, fragment removed, empty path → "/", trailing "." removed.
    pub fn canonical(&self) -> String {
        let mut host = self.host.trim_end_matches('.').to_ascii_lowercase();
        if host.is_empty() {
            host = self.host.clone();
        }
        let default = if self.scheme == "https" { 443 } else { 80 };
        let auth = if self.port == default {
            host
        } else {
            format!("{}:{}", host, self.port)
        };
        let mut path = self.path_query.clone();
        if path.is_empty() {
            path = "/".to_string();
        }
        format!("{}://{}{}", self.scheme, auth, path)
    }
}

/// Parse an absolute http(s) URL without pulling in a URL crate.
///
/// Rejects: non-http(s) schemes, missing host, embedded credentials
/// (`user:pass@host`) — the spec forbids forwarding userinfo — and control
/// characters. Percent-encoding is preserved verbatim.
pub fn parse_url(raw: &str) -> Result<ParsedUrl, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("empty url".into());
    }
    if raw.len() > MAX_URL_LEN {
        return Err("url too long".into());
    }
    if raw.chars().any(|c| c.is_control()) {
        return Err("url contains control characters".into());
    }
    let lower = raw.to_ascii_lowercase();
    let (scheme, _rest) = lower
        .split_once("://")
        .ok_or_else(|| "missing scheme (http/https)".to_string())?;
    if scheme != "http" && scheme != "https" {
        return Err(format!("scheme '{}' is not allowed", scheme));
    }
    let default_port: u16 = if scheme == "https" { 443 } else { 80 };

    // strip fragment; keep path+query from the *original* string to preserve case
    let no_frag = raw.split('#').next().unwrap_or("");
    let after_scheme = &no_frag[scheme.len() + 3..];
    let authority_end = after_scheme
        .find(|c| c == '/' || c == '?')
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];
    let path_query = match after_scheme[authority_end..].is_empty() {
        true => "/".to_string(),
        false => {
            let pq = &after_scheme[authority_end..];
            if pq.starts_with('/') { pq.to_string() } else { format!("/{}", pq) }
        }
    };
    if authority.is_empty() {
        return Err("missing host".into());
    }
    if authority.contains('@') {
        return Err("credentials embedded in the URL are refused".into());
    }

    let (host, port) = if authority.starts_with('[') {
        let close = authority.find(']').ok_or_else(|| "malformed IPv6 host".to_string())?;
        let host = authority[1..close].to_string();
        let rest = &authority[close + 1..];
        let port = if let Some(p) = rest.strip_prefix(':') {
            p.parse::<u16>().map_err(|_| "bad port".to_string())?
        } else {
            default_port
        };
        (host, port)
    } else if let Some((h, p)) = authority.rsplit_once(':') {
        (h.to_string(), p.parse::<u16>().map_err(|_| "bad port".to_string())?)
    } else {
        (authority.to_string(), default_port)
    };
    if host.is_empty() {
        return Err("missing host".into());
    }
    if port == 0 {
        return Err("port 0 is not allowed".into());
    }
    Ok(ParsedUrl { scheme: scheme.to_string(), host, port, path_query })
}

/// Why a name could not be turned into a usable address. Separating these matters:
/// a blocked address is a policy decision (4xx), an unresolvable name is a network
/// condition (5xx) and must not be reported as "refused by policy".
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveError {
    Blocked(String),
    Dns(String),
}

/// Resolve a host to public addresses only. Every returned address is
/// guaranteed to pass `is_public_ip`, so a fetcher can pin to one of them.
///
/// `FORGE_FETCH_ALLOW_PRIVATE=1` disables the public-only rule. It exists so the
/// test suite can drive a loopback fixture server; the capability page reports
/// when it is on so it cannot be enabled silently.
pub fn resolve_public(host: &str, port: u16) -> Result<Vec<IpAddr>, ResolveError> {
    let literal = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = literal.parse::<IpAddr>() {
        return if is_public_ip(&ip) || allow_private() {
            Ok(vec![ip])
        } else {
            Err(ResolveError::Blocked(format!("address {} is not public", ip)))
        };
    }
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|e| ResolveError::Dns(format!("cannot resolve {}: {}", host, e)))?;
    let ips: Vec<IpAddr> = addrs.map(|a| a.ip()).collect();
    if ips.is_empty() {
        return Err(ResolveError::Dns(format!("{} did not resolve", host)));
    }
    if !allow_private() {
        for ip in &ips {
            if !is_public_ip(ip) {
                return Err(ResolveError::Blocked(format!(
                    "{} resolves to non-public address {}",
                    host, ip
                )));
            }
        }
    }
    Ok(ips)
}

/// True when the operator explicitly opted into private-network fetching.
pub fn allow_private() -> bool {
    std::env::var("FORGE_FETCH_ALLOW_PRIVATE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn is_public_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_unspecified()
                || (o[0] == 100 && (64..=127).contains(&o[1])) // CGNAT 100.64/10
                || (o[0] == 169 && o[1] == 254) // link-local / metadata
                || o[0] == 0)
        }
        IpAddr::V6(v6) => {
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 // unique local
                || (v6.segments()[0] & 0xffc0) == 0xfe80) // link local
        }
    }
}

/// Resolve a name inside a root directory, refusing traversal and absolute paths.
pub fn safe_join(root: &std::path::Path, rel: &str) -> Option<std::path::PathBuf> {
    if rel.contains('\0') {
        return None;
    }
    let cleaned = rel.trim_start_matches('/');
    let mut out = root.to_path_buf();
    for part in cleaned.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.contains('\\') || part.contains(':') {
            return None;
        }
        out.push(part);
    }
    // canonicalise to be sure (the target must exist for this check)
    let canon = out.canonicalize().ok()?;
    let root_canon = root.canonicalize().ok()?;
    if canon.starts_with(&root_canon) {
        Some(canon)
    } else {
        None
    }
}

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Restrict an uploaded filename to something safe for display only.
pub fn clean_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ' | '(' | ')'))
        .take(120)
        .collect();
    if cleaned.trim().is_empty() {
        "upload".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_ssrf_targets() {
        for bad in [
            "http://localhost/admin",
            "http://127.0.0.1:8080/",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.5/",
            "http://192.168.1.1/",
            "http://[::1]/",
            "http://100.64.1.1/",
            "ftp://example.com/x",
            "file:///etc/passwd",
            "http://metadata.google.internal/",
            "not-a-url",
        ] {
            assert!(
                matches!(check_url(bad), UrlVerdict::Rejected(_)),
                "should reject {}",
                bad
            );
        }
    }

    #[test]
    fn allows_public_http() {
        // 93.184.216.34 is a public address; no DNS needed for the literal.
        assert!(matches!(check_url("http://93.184.216.34/page"), UrlVerdict::Allowed(_)));
    }

    #[test]
    fn ip_classification() {
        assert!(!is_public_ip(&"127.0.0.1".parse().unwrap()));
        assert!(!is_public_ip(&"10.1.2.3".parse().unwrap()));
        assert!(!is_public_ip(&"172.16.0.1".parse().unwrap()));
        assert!(is_public_ip(&"8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn path_traversal_is_refused() {
        let root = std::env::temp_dir().join(format!("forge-sec-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("ok.txt"), b"x").unwrap();
        assert!(safe_join(&root, "ok.txt").is_some());
        assert!(safe_join(&root, "../etc/passwd").is_none());
        assert!(safe_join(&root, "/etc/passwd").is_none());
        assert!(safe_join(&root, "a/../../etc/passwd").is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn html_escaping_prevents_markup() {
        assert_eq!(escape_html("<script>x</script>"), "&lt;script&gt;x&lt;/script&gt;");
    }

    #[test]
    fn filenames_are_sanitised() {
        assert_eq!(clean_filename("../../etc/passwd"), "passwd");
        assert_eq!(clean_filename("a/b/c.pdf"), "c.pdf");
        assert_eq!(clean_filename(""), "upload");
    }
}
