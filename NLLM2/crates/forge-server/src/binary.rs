//! Server-side routing for binary inspection.
//!
//! Kept in `forge-server` rather than `forge-core::api` because `bininspect`
//! depends on `forge-core` (for its ZIP reader and JSON writer), so wiring the
//! other direction would create a dependency cycle.

use forge_core::http::{Request, Response};
use forge_core::json::{self, Value};
use forge_core::obj;

/// Largest artifact we will analyse in one request (256 MiB).
const MAX_BYTES: usize = 256 * 1024 * 1024;

fn err(status: u16, code: &str, msg: &str) -> Response {
    let body = obj! {
        "error" => msg,
        "code" => code,
        "status" => status as u64
    };
    Response::json(status, &body)
}

fn ok(v: Value) -> Response {
    Response::json(200, &obj! { "ok" => true, "report" => v })
}

/// `POST /api/binary/inspect`
///
/// Accepts either:
///   * `application/octet-stream` — the raw artifact as the request body, with an
///     optional `?name=foo.apk` query parameter used for the report's `file` field; or
///   * `application/json` — `{"filename": "foo.apk", "data_base64": "..."}`.
///
/// Returns a structural report; never executes or unpacks the artifact to disk.
pub fn handle(req: &Request) -> Response {
    if req.method != "POST" {
        return err(405, "method_not_allowed", "use POST with the artifact as the body");
    }
    if req.body.is_empty() {
        return err(400, "empty_body", "request body is empty");
    }
    if req.body.len() > MAX_BYTES {
        return err(
            413,
            "too_large",
            &format!("artifact exceeds {} bytes", MAX_BYTES),
        );
    }

    let content_type = req.header("content-type").unwrap_or("");
    let (name, data) = if content_type.starts_with("application/json") {
        match parse_json_upload(&req.body) {
            Ok(v) => v,
            Err(e) => return err(400, "bad_json", &e),
        }
    } else {
        let name = req
            .query
            .get("name")
            .or_else(|| req.query.get("filename"))
            .cloned()
            .unwrap_or_else(|| "artifact.bin".to_string());
        (name, req.body.clone())
    };

    if data.is_empty() {
        return err(400, "empty_body", "no artifact bytes decoded");
    }

    ok(bininspect::report::analyze(&name, &data, 40))
}

fn parse_json_upload(body: &[u8]) -> Result<(String, Vec<u8>), String> {
    let v = json::parse(std::str::from_utf8(body).map_err(|e| e.to_string())?)
        .map_err(|e| format!("invalid JSON: {}", e))?;
    let name = v
        .get("filename")
        .and_then(|f| f.as_str())
        .unwrap_or("artifact.bin")
        .to_string();
    let b64 = v
        .get("data_base64")
        .and_then(|d| d.as_str())
        .ok_or_else(|| "missing data_base64".to_string())?;
    let data = base64_decode(b64)?;
    Ok((name, data))
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let clean: Vec<u8> = s
        .bytes()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    if clean.len() % 4 != 0 {
        return Err("base64 length must be a multiple of 4".into());
    }
    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    for chunk in clean.chunks(4) {
        let pad = chunk.iter().filter(|c| **c == b'=').count();
        let mut acc: u32 = 0;
        for (i, c) in chunk.iter().enumerate() {
            let v = if *c == b'=' {
                0
            } else {
                val(*c).ok_or_else(|| format!("invalid base64 character {:?}", *c as char))?
            } as u32;
            acc |= v << (18 - 6 * i);
        }
        out.push((acc >> 16) as u8);
        if pad < 2 {
            out.push((acc >> 8) as u8);
        }
        if pad < 1 {
            out.push(acc as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_core::http::Request;
    use std::collections::BTreeMap;

    fn fixture(rel: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures")
                .join(rel),
        )
        .expect("fixture")
    }

    fn req_post() -> Request {
        Request {
            method: "POST".to_string(),
            path: "/api/binary/inspect".to_string(),
            query: BTreeMap::new(),
            headers: BTreeMap::new(),
            body: Vec::new(),
            peer: "test".to_string(),
        }
    }

    fn post_raw(name: &str, data: Vec<u8>, ct: &str) -> Request {
        let mut req = req_post();
        req.body = data;
        req.headers.insert("content-type".into(), ct.to_string());
        req.query.insert("name".to_string(), name.to_string());
        req
    }

    fn body_json(resp: &Response) -> Value {
        json::parse(std::str::from_utf8(&resp.body).expect("utf8")).expect("json")
    }

    #[test]
    fn inspects_an_apk_posted_as_bytes() {
        let resp = handle(&post_raw("x.apk", fixture("apk/sample.apk"), "application/octet-stream"));
        assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
        let v = body_json(&resp);
        let r = v.get("report").expect("report");
        assert_eq!(r.str_of("format"), Some("apk_or_zip"));
        assert_eq!(r.get("apk").and_then(|a| a.str_of("package")), Some("com.example.forgetest"));
        assert!(r.str_of("sha256").unwrap_or("").len() == 64);
    }

    #[test]
    fn inspects_a_pe_posted_as_bytes() {
        let resp = handle(&post_raw("x.exe", fixture("sample.exe"), "application/octet-stream"));
        assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
        let v = body_json(&resp);
        let r = v.get("report").expect("report");
        assert_eq!(r.str_of("format"), Some("pe"));
        assert_eq!(r.get("pe").and_then(|p| p.str_of("machine")), Some("x86-64"));
    }

    #[test]
    fn accepts_a_json_body_with_base64() {
        let data = fixture("apk/sample.apk");
        let b64 = base64_encode(&data);
        let payload = json::to_string(&obj! { "filename" => "up.apk", "data_base64" => b64.as_str() });
        let mut req = req_post();
        req.body = payload.into_bytes();
        req.headers.insert("content-type".into(), "application/json".to_string());
        let resp = handle(&req);
        assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
        assert_eq!(body_json(&resp).get("report").and_then(|r| r.str_of("format")), Some("apk_or_zip"));
    }

    #[test]
    fn rejects_wrong_method_empty_and_garbage() {
        let mut get = req_post();
        get.method = "GET".to_string();
        assert_eq!(handle(&get).status, 405);
        assert_eq!(handle(&req_post()).status, 400);
        let resp = handle(&post_raw("junk", vec![0u8; 64], "application/octet-stream"));
        let v = body_json(&resp);
        assert_eq!(resp.status, 200, "unknown files still report");
        assert_eq!(v.get("report").and_then(|r| r.str_of("format")), Some("unknown"));
    }

    #[test]
    fn base64_roundtrip() {
        for sample in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            let enc = base64_encode(sample);
            assert_eq!(base64_decode(&enc).unwrap(), sample, "sample {:?}", sample);
        }
        assert!(base64_decode("not base64!!").is_err());
    }

    fn base64_encode(data: &[u8]) -> String {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for c in data.chunks(3) {
            let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
            let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
            out.push(T[(n >> 18) as usize & 63] as char);
            out.push(T[(n >> 12) as usize & 63] as char);
            out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
            out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
        }
        out
    }
}
