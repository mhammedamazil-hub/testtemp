//! Small HTTP/1.1 server built on `std::net` (no hyper/axum: crates.io blocked).
//!
//! Scope is deliberate: one request per connection, `Connection: close`,
//! hard limits on header size and body size, no chunked encoding. That is
//! sufficient for a local single-user app and is fully covered by tests.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

pub const MAX_HEADER_BYTES: usize = 64 * 1024;
pub const DEFAULT_MAX_BODY: usize = 32 * 1024 * 1024; // 32 MiB uploads (PDF etc.)

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
    pub peer: String,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_ascii_lowercase()).map(|s| s.as_str())
    }
    pub fn body_str(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
    pub fn query_get(&self, k: &str) -> Option<&str> {
        self.query.get(k).map(|s| s.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
}

impl Response {
    pub fn new(status: u16, content_type: &str, body: impl Into<Vec<u8>>) -> Response {
        Response { status, content_type: content_type.to_string(), body: body.into(), headers: Vec::new() }
    }

    pub fn json(status: u16, v: &crate::json::Value) -> Response {
        Response::new(status, "application/json; charset=utf-8", crate::json::to_string(v).into_bytes())
    }

    pub fn text(status: u16, s: impl Into<String>) -> Response {
        Response::new(status, "text/plain; charset=utf-8", s.into().into_bytes())
    }

    pub fn html(s: impl Into<String>) -> Response {
        Response::new(200, "text/html; charset=utf-8", s.into().into_bytes())
    }

    pub fn with_header(mut self, k: &str, v: &str) -> Response {
        self.headers.push((k.to_string(), v.to_string()));
        self
    }

    fn reason(status: u16) -> &'static str {
        match status {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            304 => "Not Modified",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            413 => "Payload Too Large",
            415 => "Unsupported Media Type",
            422 => "Unprocessable Entity",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            _ => "Unknown",
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.status,
            Response::reason(self.status),
            self.content_type,
            self.body.len()
        );
        head.push_str("X-Content-Type-Options: nosniff\r\n");
        head.push_str("Cache-Control: no-store\r\n");
        head.push_str("Referrer-Policy: no-referrer\r\n");
        // No inline script is used by the app; inline *style attributes* are used
        // by the renderer, hence style-src 'unsafe-inline'.
        head.push_str(
            "Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'\r\n",
        );
        for (k, v) in &self.headers {
            head.push_str(&format!("{}: {}\r\n", k, v));
        }
        head.push_str("\r\n");
        let mut out = head.into_bytes();
        out.extend_from_slice(&self.body);
        out
    }
}

pub fn parse_request(raw: &[u8], peer: &str, max_body: usize) -> Result<Request, (u16, String)> {
    // Split head from body at the first CRLFCRLF.
    let sep = find(raw, b"\r\n\r\n").ok_or((400u16, "no header terminator".to_string()))?;
    let head = String::from_utf8_lossy(&raw[..sep]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().ok_or((400u16, "empty request".to_string()))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or((400u16, "no method".to_string()))?.to_string();
    let target = parts.next().ok_or((400u16, "no target".to_string()))?.to_string();
    let version = parts.next().unwrap_or("HTTP/1.1").to_string();
    if !version.starts_with("HTTP/1.") {
        return Err((400, format!("unsupported version {}", version)));
    }

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), parse_query(q)),
        None => (target, BTreeMap::new()),
    };

    let mut headers = BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (k, v) = line.split_once(':').ok_or((400u16, "bad header".to_string()))?;
        headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
    }

    let mut body = raw[sep + 4..].to_vec();
    if let Some(len_s) = headers.get("content-length") {
        let len: usize = len_s.parse().map_err(|_| (400u16, "bad content-length".to_string()))?;
        if len > max_body {
            return Err((413, format!("body {} exceeds limit {}", len, max_body)));
        }
        if body.len() > len {
            body.truncate(len);
        }
    }

    Ok(Request { method, path, query, headers, body, peer: peer.to_string() })
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

fn parse_query(q: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(percent_decode(k), percent_decode(v));
    }
    map
}

pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Read one request from a stream honouring the declared content length.
pub fn read_request(stream: &mut TcpStream, peer: &str, max_body: usize) -> Result<Request, (u16, String)> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| (400u16, e.to_string()))?);
    let mut head = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| (400u16, e.to_string()))?;
        if n == 0 {
            return Err((400, "connection closed before headers".to_string()));
        }
        head.extend_from_slice(line.as_bytes());
        if head.len() > MAX_HEADER_BYTES {
            return Err((413, "headers too large".to_string()));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
    }

    let head_str = String::from_utf8_lossy(&head).to_string();
    let mut lines = head_str.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    if method.is_empty() {
        return Err((400, "empty request line".to_string()));
    }
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), parse_query(q)),
        None => (target, BTreeMap::new()),
    };
    let mut headers = BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let len: usize = headers
        .get("content-length")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if len > max_body {
        return Err((413, format!("body {} exceeds limit {}", len, max_body)));
    }
    let mut body = vec![0u8; len];
    if len > 0 {
        reader
            .read_exact(&mut body)
            .map_err(|e| (400u16, format!("short body: {}", e)))?;
    }
    Ok(Request { method, path, query, headers, body, peer: peer.to_string() })
}

/// Serve forever, one thread per connection.
pub fn serve<F>(listener: TcpListener, handler: F, max_body: usize) -> std::io::Result<()>
where
    F: Fn(&Request) -> Response + Send + Sync + 'static,
{
    let handler = Arc::new(handler);
    for incoming in listener.incoming() {
        let stream = match incoming {
            Ok(s) => s,
            Err(_) => continue,
        };
        let h = Arc::clone(&handler);
        std::thread::spawn(move || {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
            handle_conn(stream, &*h, max_body);
        });
    }
    Ok(())
}

fn handle_conn(mut stream: TcpStream, handler: &dyn Fn(&Request) -> Response, max_body: usize) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".into());
    let response = match read_request(&mut stream, &peer, max_body) {
        Ok(req) => handler(&req),
        Err((code, msg)) => Response::json(
            code,
            &crate::obj! { "error" => msg.as_str(), "status" => code as u64 },
        ),
    };
    let _ = stream.write_all(&response.encode());
    let _ = stream.flush();
}

pub fn mime_for(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".html") || lower.ends_with(".htm") {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "text/javascript; charset=utf-8"
    } else if lower.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if lower.ends_with(".json") || lower.ends_with(".webmanifest") {
        "application/json; charset=utf-8"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else if lower.ends_with(".woff2") {
        "font/woff2"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_carry_security_headers() {
        let raw = String::from_utf8(Response::json(200, &crate::json::Value::Null).encode()).unwrap();
        assert!(raw.contains("X-Content-Type-Options: nosniff"));
        assert!(raw.contains("Content-Security-Policy: default-src 'self'"));
        assert!(raw.contains("Referrer-Policy: no-referrer"));
        assert!(!raw.to_lowercase().contains("access-control-allow-origin: *"));
    }

    #[test]
    fn parses_query_and_percent_encoding() {
        let raw = b"GET /api/x?a=1&b=hello%20world&c=%E2%9C%93 HTTP/1.1\r\nHost: x\r\n\r\n";
        let req = parse_request(raw, "127.0.0.1:1", 1024).unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/api/x");
        assert_eq!(req.query_get("a"), Some("1"));
        assert_eq!(req.query_get("b"), Some("hello world"));
        assert_eq!(req.query_get("c"), Some("✓"));
    }

    #[test]
    fn encodes_response_with_content_length() {
        let r = Response::text(200, "hello");
        let raw = String::from_utf8(r.encode()).unwrap();
        assert!(raw.starts_with("HTTP/1.1 200 OK"));
        assert!(raw.contains("Content-Length: 5"));
        assert!(raw.ends_with("hello"));
    }

    #[test]
    fn rejects_oversized_body() {
        let raw = b"POST / HTTP/1.1\r\nContent-Length: 999999\r\n\r\n";
        let err = parse_request(raw, "p", 1024).unwrap_err();
        assert_eq!(err.0, 413);
    }

    #[test]
    fn rejects_malformed() {
        assert!(parse_request(b"garbage", "p", 100).is_err());
        assert!(parse_request(b"GET / HTTP/1.1\r\nBadHeader\r\n\r\n", "p", 100).is_err());
    }

    #[test]
    fn mime_types() {
        assert_eq!(mime_for("a.html"), "text/html; charset=utf-8");
        assert_eq!(mime_for("a.js"), "text/javascript; charset=utf-8");
        assert_eq!(mime_for("a.weird"), "application/octet-stream");
    }
}
