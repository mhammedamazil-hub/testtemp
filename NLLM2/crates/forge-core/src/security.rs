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
