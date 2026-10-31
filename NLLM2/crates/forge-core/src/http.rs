//! Small HTTP/1.1 server built on `std::net` (no hyper/axum: crates.io blocked).
//!
//! Scope is deliberate: one request per connection, `Connection: close`, hard limits on
//! header size and body size. Responses are either buffered (`Content-Length`) or streamed
//! with **chunked transfer encoding** (`Reply::Stream`), which is what the chat stream uses.
//! There is no request chunked decoding: request bodies must carry a `Content-Length`.

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

    fn common_headers() -> String {
        let mut h = String::new();
        h.push_str("X-Content-Type-Options: nosniff\r\n");
        h.push_str("Cache-Control: no-store\r\n");
        h.push_str("Referrer-Policy: no-referrer\r\n");
        h.push_str(
            "Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'\r\n",
        );
        h
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.status,
            Response::reason(self.status),
            self.content_type,
            self.body.len()
        );
        head.push_str(&Response::common_headers());
        for (k, v) in &self.headers {
            head.push_str(&format!("{}: {}\r\n", k, v));
        }
        head.push_str("\r\n");
        let mut out = head.into_bytes();
        out.extend_from_slice(&self.body);
        out
    }
}


/// What a handler returns. Most routes are `Full` (buffered, `Content-Length`); the chat
/// stream is `Stream` (framed, `Transfer-Encoding: chunked`).
pub enum Reply {
    Full(Response),
    Stream(StreamReply),
}

impl From<Response> for Reply {
    fn from(r: Response) -> Reply {
        Reply::Full(r)
    }
}

/// A chunked response that is written frame by frame.
///
/// `on_finish` is called exactly once with `complete = true` only if the terminating
/// zero-length chunk was accepted by the socket. That is what lets the API persist a chat
/// turn **after** delivery: a stream the client aborted never becomes part of the
/// conversation, so "Stop" has a real effect rather than a cosmetic one.
pub struct StreamReply {
    pub status: u16,
    pub content_type: String,
    pub headers: Vec<(String, String)>,
    pub frames: Vec<Vec<u8>>,
    /// Diagnostic pacing between frames (see `maybe_pace`). `None` means "as fast as the
    /// socket accepts", which is what the UI uses.
    pub pace: Option<Duration>,
    pub on_finish: Option<Box<dyn FnOnce(bool) + Send + Sync>>,
}

impl StreamReply {
    pub fn new(status: u16, content_type: &str, frames: Vec<Vec<u8>>) -> StreamReply {
        StreamReply {
            status,
            content_type: content_type.to_string(),
            headers: Vec::new(),
            frames,
            pace: None,
            on_finish: None,
        }
    }

    /// Diagnostic pacing between frames, in milliseconds. Off by default; it exists so a test
    /// (or a person debugging the UI) can observe progressive delivery on a fast local answer.
    /// It adds delay and nothing else — no content depends on it.
    pub fn maybe_pace(mut self, pace: Option<Duration>) -> StreamReply {
        self.pace = pace;
        self
    }

    pub fn with_header(mut self, k: &str, v: &str) -> StreamReply {
        self.headers.push((k.to_string(), v.to_string()));
        self
    }

    pub fn on_finish(mut self, f: impl FnOnce(bool) + Send + Sync + 'static) -> StreamReply {
        self.on_finish = Some(Box::new(f));
        self
    }

    fn head(&self) -> Vec<u8> {
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n",
            self.status,
            Response::reason(self.status),
            self.content_type
        );
        head.push_str(&Response::common_headers());
        // Proxies must not buffer a stream; nothing between us and the browser is allowed
        // to sit on frames until the answer is finished.
        head.push_str("X-Accel-Buffering: no\r\n");
        for (k, v) in &self.headers {
            head.push_str(&format!("{}: {}\r\n", k, v));
        }
        head.push_str("\r\n");
        head.into_bytes()
    }
}

/// True when the peer has closed its side of the connection.
///
/// `read` on a socket whose peer sent FIN returns `Ok(0)`; a timeout (the normal case, since
/// the request body is already consumed) returns an error and means "still connected".
fn peer_gone(stream: &TcpStream) -> bool {
    let mut probe = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let _ = probe.set_read_timeout(Some(Duration::from_millis(1)));
    let mut byte = [0u8; 1];
    matches!(probe.read(&mut byte), Ok(0))
}

/// Write a chunked response. Returns true only if every frame and the terminator were
/// accepted. Aborts (peer gone, EPIPE, write timeout) stop the loop immediately.
pub fn write_stream(stream: &mut TcpStream, mut sr: StreamReply) -> bool {
    let mut complete = stream.write_all(&sr.head()).is_ok() && stream.flush().is_ok();
    if complete {
        for (idx, frame) in sr.frames.iter().enumerate() {
            if idx > 0 {
                if let Some(p) = sr.pace {
                    std::thread::sleep(p);
                }
            }
            if peer_gone(stream) {
                complete = false;
                break;
            }
            let mut buf = format!("{:X}\r\n", frame.len()).into_bytes();
            buf.extend_from_slice(frame);
            buf.extend_from_slice(b"\r\n");
            if stream.write_all(&buf).is_err() || stream.flush().is_err() {
                complete = false;
                break;
            }
        }
        if complete && (stream.write_all(b"0\r\n\r\n").is_err() || stream.flush().is_err()) {
            complete = false;
        }
    }
    if let Some(f) = sr.on_finish.take() {
        f(complete);
    }
    complete
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
    F: Fn(&Request) -> Reply + Send + Sync + 'static,
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

fn handle_conn(mut stream: TcpStream, handler: &dyn Fn(&Request) -> Reply, max_body: usize) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".into());
    match read_request(&mut stream, &peer, max_body) {
        Ok(req) => match handler(&req) {
            Reply::Full(response) => {
                let _ = stream.write_all(&response.encode());
                let _ = stream.flush();
            }
            Reply::Stream(sr) => {
                write_stream(&mut stream, sr);
            }
        },
        Err((code, msg)) => {
            let response = Response::json(
                code,
                &crate::obj! { "error" => msg.as_str(), "status" => code as u64 },
            );
            let _ = stream.write_all(&response.encode());
            let _ = stream.flush();
        }
    }
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
    fn a_complete_stream_uses_chunked_framing_and_reports_delivery() {
        use std::io::Read as _;
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel::<bool>();
        let client = std::thread::spawn(move || {
            let mut s = std::net::TcpStream::connect(addr).unwrap();
            s.write_all(b"GET /api/notebooks/x/chat/stream HTTP/1.1\r\nHost: x\r\n\r\n")
                .unwrap();
            let mut raw = Vec::new();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let _ = s.read_to_end(&mut raw);
            raw
        });

        let mut server = listener.accept().unwrap().0;
        let _ = read_request(&mut server, "test", 1024);
        let ok = write_stream(
            &mut server,
            StreamReply::new(
                200,
                "application/x-ndjson",
                vec![b"{\"event\":\"start\"}\n".to_vec(), b"{\"event\":\"done\"}\n".to_vec()],
            )
            .on_finish(move |c| {
                let _ = tx.send(c);
            }),
        );
        assert!(ok, "an accepted stream must report completion");
        assert!(rx.recv().unwrap(), "on_finish must see complete = true");
        std::mem::drop(server); // close, so the client's read_to_end returns

        let raw = client.join().unwrap();
        let text = String::from_utf8_lossy(&raw).to_string();
        assert!(text.contains("Transfer-Encoding: chunked"));
        assert!(!text.contains("Content-Length"));
        // Each frame is its own chunk: hex length, CRLF, payload, CRLF.
        let start = format!("{:X}\r\n{{\"event\":\"start\"}}\n\r\n", "{\"event\":\"start\"}\n".len());
        let done = format!("{:X}\r\n{{\"event\":\"done\"}}\n\r\n", "{\"event\":\"done\"}\n".len());
        assert!(text.contains(&start), "raw: {}", text);
        assert!(text.contains(&done), "raw: {}", text);
        assert!(text.ends_with("0\r\n\r\n"));
    }

    #[test]
    fn an_aborted_stream_is_not_reported_as_delivered() {
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let s = std::net::TcpStream::connect(addr).unwrap();
            std::mem::drop(s); // hang up without reading a byte
        });

        let mut server = listener.accept().unwrap().0;
        server.set_write_timeout(Some(Duration::from_millis(200))).unwrap();
        client.join().unwrap();
        // let the FIN land before the writer starts probing
        std::thread::sleep(Duration::from_millis(50));

        let (tx, rx) = mpsc::channel::<bool>();
        let ok = write_stream(
            &mut server,
            StreamReply::new(200, "application/x-ndjson", vec![b"{\"event\":\"x\"}\n".to_vec()])
                .on_finish(move |c| {
                    let _ = tx.send(c);
                }),
        );
        assert!(!ok, "a hung-up client must not count as delivery");
        assert!(!rx.recv().unwrap(), "on_finish must see complete = false");
    }

    #[test]
    fn mime_types() {
        assert_eq!(mime_for("a.html"), "text/html; charset=utf-8");
        assert_eq!(mime_for("a.js"), "text/javascript; charset=utf-8");
        assert_eq!(mime_for("a.weird"), "application/octet-stream");
    }
}
