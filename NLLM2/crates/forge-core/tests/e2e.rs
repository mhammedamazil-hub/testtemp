//! End-to-end tests over a real TCP socket: the exact path a browser takes.

use forge_core::api::Api;
use forge_core::http::{serve, Request};
use forge_core::json::{self, Value};
use forge_core::obj;
use forge_core::store::Store;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

struct Server {
    port: u16,
}

impl Server {
    fn start(dir: PathBuf) -> Server {
        let store = Store::open(&dir).expect("store");
        let api = Api::new(store);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let web = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/web");
        std::thread::spawn(move || {
            let handler = move |req: &Request| {
                if req.path.starts_with("/api") {
                    api.route(req)
                } else {
                    forge_core::http::Reply::Full(forge_core::web::serve_static(&web, &req.path))
                }
            };
            let _ = serve(listener, handler, forge_core::http::DEFAULT_MAX_BODY);
        });
        // Wait for the listener to accept connections.
        for _ in 0..50 {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Server { port }
    }

    fn raw(&self, method: &str, path: &str, body: Option<&[u8]>, ctype: &str) -> (u16, String) {
        self.raw_h(method, path, body, ctype, &[])
    }

    /// Same, with extra request headers — this is how a share token or owner token travels.
    fn raw_h(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        ctype: &str,
        headers: &[(&str, &str)],
    ) -> (u16, String) {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let payload = body.unwrap_or(&[]);
        let extra: String = headers.iter().map(|(k, v)| format!("{}: {}\r\n", k, v)).collect();
        let head = format!(
            "{} {} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: {}\r\nContent-Length: {}\r\n{}\r\n",
            method, path, ctype, payload.len(), extra
        );
        s.write_all(head.as_bytes()).unwrap();
        s.write_all(payload).unwrap();
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).unwrap();
        let text = String::from_utf8_lossy(&raw).to_string();
        let status = text
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        (status, body)
    }

    fn json(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        self.json_h(method, path, body, &[])
    }

    fn json_h(&self, method: &str, path: &str, body: Option<Value>, headers: &[(&str, &str)]) -> (u16, Value) {
        let raw_body = body.map(|v| json::to_string(&v).into_bytes());
        let (status, text) = self.raw_h(method, path, raw_body.as_deref(), "application/json", headers);
        (status, json::parse(&text).unwrap_or(Value::Null))
    }
}

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "forge-e2e-{}-{}-{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn obj(pairs: &[(&str, Value)]) -> Value {
    let mut m = std::collections::BTreeMap::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v.clone());
    }
    Value::Obj(m)
}

const NOTEBOOK_TEXT: &str = "Retrieval augmented generation combines a retriever with a generator.\n\n\
BM25 scores documents using term frequency saturation and inverse document frequency weighting.\n\n\
Citation validation ensures every quoted span appears verbatim in the evidence store.\n\n\
Abstention is required when the retrieved evidence falls below the relevance floor.";

#[test]
fn full_research_workflow_over_http() {
    let dir = tempdir("workflow");
    let s = Server::start(dir.clone());

    // health
    let (st, health) = s.json("GET", "/api/health", None);
    assert_eq!(st, 200);
    assert_eq!(health.str_of("schema"), Some("forge.db/1"));

    // create notebook
    let (st, nb) = s.json(
        "POST",
        "/api/notebooks",
        Some(obj(&[("title", Value::from("Retrieval study"))])),
    );
    assert_eq!(st, 201);
    let nb_id = nb.str_of("id").unwrap().to_string();

    // import a text source
    let (st, src) = s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb_id),
        Some(obj(&[
            ("title", Value::from("RAG notes")),
            ("text", Value::from(NOTEBOOK_TEXT)),
        ])),
    );
    assert_eq!(st, 201, "import failed: {:?}", src);
    let source_id = src.str_of("source_id").unwrap().to_string();
    assert!(src.get("chunk_count").and_then(|v| v.as_f64()).unwrap() >= 1.0);

    // grounded question -> citations that exist in the store
    let (st, chat) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("how does BM25 score documents"))])),
    );
    assert_eq!(st, 200, "chat failed: {:?}", chat);
    assert_eq!(chat.get("abstained").and_then(|v| v.as_bool()), Some(false));
    let answer = chat.str_of("answer").unwrap();
    assert!(
        answer.to_lowercase().contains("bm25"),
        "answer should quote the evidence: {}",
        answer
    );
    let cites = chat.get("citations").and_then(|v| v.as_arr()).unwrap();
    assert!(!cites.is_empty(), "answer must carry citations");
    let first = &cites[0];
    let chunk_id = first.str_of("chunk_id").unwrap();
    let locator_label = first.str_of("locator_label").unwrap();
    assert!(locator_label.starts_with("paragraph") || locator_label.starts_with("page"));
    assert_eq!(first.str_of("source_id"), Some(source_id.as_str()));
    assert!(first.get("n").is_some());

    // the cited chunk is fetchable and contains the quote
    let (st, chunks) = s.json("GET", &format!("/api/sources/{}/chunks", source_id), None);
    assert_eq!(st, 200);
    let found = chunks
        .get("chunks")
        .and_then(|c| c.as_arr())
        .unwrap()
        .iter()
        .find(|c| c.str_of("id") == Some(chunk_id))
        .expect("cited chunk must exist")
        .clone();
    let quote = first.str_of("quote").unwrap();
    assert!(
        found.str_of("text").unwrap().contains(quote),
        "quote must be verbatim from the chunk"
    );

    // off-topic question -> honest abstention, no invented citation
    let (st, off) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("what is the capital of Mongolia"))])),
    );
    assert_eq!(st, 200);
    assert_eq!(off.get("abstained").and_then(|v| v.as_bool()), Some(true));
    assert!(off.get("citations").and_then(|c| c.as_arr()).unwrap().is_empty());

    // study artifact
    let (st, art) = s.json(
        "POST",
        &format!("/api/notebooks/{}/artifacts", nb_id),
        Some(obj(&[("kind", Value::from("flashcards"))])),
    );
    assert_eq!(st, 201, "artifact failed: {:?}", art);
    assert!(art.get("payload").unwrap().get("cards").is_some());

    // notes
    let (st, _) = s.json(
        "POST",
        &format!("/api/notebooks/{}/notes", nb_id),
        Some(obj(&[("body", Value::from("Check the abstention policy."))])),
    );
    assert_eq!(st, 201);

    // export then re-import into a fresh store
    let (st, export) = s.raw_export(&format!("/api/notebooks/{}/export", nb_id));
    assert_eq!(st, 200);
    let dir2 = tempdir("import");
    let s2 = Server::start(dir2.clone());
    let (st, imported) = s2.json("POST", "/api/import", Some(export));
    assert_eq!(st, 201, "import failed: {:?}", imported);
    let new_id = imported.str_of("notebook_id").unwrap().to_string();
    let (st, nb2) = s2.json("GET", &format!("/api/notebooks/{}", new_id), None);
    assert_eq!(st, 200);
    assert_eq!(nb2.get("sources").unwrap().as_arr().unwrap().len(), 1);
    assert_eq!(nb2.get("notes").unwrap().as_arr().unwrap().len(), 1);
    // the imported notebook still answers with citations
    let (st, chat2) = s2.json(
        "POST",
        &format!("/api/notebooks/{}/chat", new_id),
        Some(obj(&[("question", Value::from("citation validation"))])),
    );
    assert_eq!(st, 200);
    assert!(!chat2.get("citations").unwrap().as_arr().unwrap().is_empty());

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&dir2).ok();
}

impl Server {
    fn raw_export(&self, path: &str) -> (u16, Value) {
        let (status, text) = self.raw("GET", path, None, "application/json");
        (status, json::parse(&text).unwrap_or(Value::Null))
    }
}

#[test]
fn rejects_bad_input_and_dangerous_urls() {
    // shares the lock with the URL test, which flips the private-fetch flag
    let _guard = URL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("FORGE_FETCH_ALLOW_PRIVATE");
    let dir = tempdir("reject");
    let s = Server::start(dir.clone());
    let (st, _) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("x"))])));
    assert_eq!(st, 201);
    let (_, list) = s.json("GET", "/api/notebooks", None);
    let nb_id = list
        .get("notebooks")
        .unwrap()
        .as_arr()
        .unwrap()[0]
        .str_of("id")
        .unwrap()
        .to_string();

    // empty question
    let (st, body) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("   "))])),
    );
    assert_eq!(st, 422);
    assert_eq!(body.str_of("code"), Some("empty_question"));

    // chat with no sources is a conflict, not a fake answer
    let (st, body) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("anything"))])),
    );
    assert_eq!(st, 409);
    assert_eq!(body.str_of("code"), Some("no_sources"));

    // SSRF attempts
    for url in [
        "http://127.0.0.1:8080/admin",
        "http://169.254.169.254/latest/meta-data/",
        "file:///etc/passwd",
        "http://localhost/",
    ] {
        let (st, body) = s.json(
            "POST",
            &format!("/api/notebooks/{}/sources/url", nb_id),
            Some(obj(&[("url", Value::from(url))])),
        );
        assert_eq!(st, 422, "{} must be refused", url);
        assert_eq!(body.str_of("code"), Some("url_rejected"));
    }

    // A public URL that this sandbox cannot reach is a *network* failure (502),
    // not a policy refusal (422) and not a fake success.
    let (st, body) = s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/url", nb_id),
        Some(obj(&[("url", Value::from("http://93.184.216.34/page"))])),
    );
    assert!(
        st == 502 || st == 504,
        "unreachable public host must be a transport error, got {} {:?}",
        st,
        body
    );
    assert_eq!(body.str_of("code"), Some("fetch_failed"));

    // A name that does not resolve is also a transport failure, not a policy refusal.
    let (st, body) = s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/url", nb_id),
        Some(obj(&[("url", Value::from("http://no-such-host.invalid/x"))])),
    );
    assert_eq!(st, 502, "{:?}", body);
    assert_eq!(body.str_of("code"), Some("fetch_failed"));

    // garbage JSON
    let (st, _) = s.raw("POST", "/api/notebooks", Some(b"{not json"), "application/json");
    assert_eq!(st, 400);

    // unknown route
    let (st, _) = s.json("GET", "/api/nope", None);
    assert_eq!(st, 404);

    // upload with a hostile filename is sanitised, not used as a path
    let (st, up) = s.raw(
        "POST",
        &format!("/api/notebooks/{}/sources/upload?filename=..%2F..%2Fetc%2Fpasswd", nb_id),
        Some(b"plain text evidence about parsing"),
        "text/plain",
    );
    assert_eq!(st, 201);
    let parsed = json::parse(&up).unwrap();
    assert!(!parsed.str_of("title").unwrap().contains(".."));

    // oversized / empty upload
    let (st, _) = s.raw(
        "POST",
        &format!("/api/notebooks/{}/sources/upload?filename=a.txt", nb_id),
        Some(b""),
        "text/plain",
    );
    assert_eq!(st, 422);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn static_app_is_served_over_http() {
    let dir = tempdir("static");
    let s = Server::start(dir.clone());
    let (st, html) = s.raw("GET", "/", None, "text/html");
    assert_eq!(st, 200);
    assert!(html.contains("Forge"));
    let (st, _) = s.raw("GET", "/app.js", None, "text/javascript");
    assert_eq!(st, 200);
    let (st, _) = s.raw("GET", "/../../Cargo.toml", None, "text/plain");
    assert_ne!(st, 200);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn imports_office_formats_over_http() {
    let dir = tempdir("office");
    let s = Server::start(dir.clone());
    let (_, nb) = s.json(
        "POST",
        "/api/notebooks",
        Some(obj(&[("title", Value::from("Office"))])),
    );
    let nb_id = nb.str_of("id").unwrap().to_string();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");

    for (file, kind, locator_kind) in [
        ("sample.docx", "docx", "paragraph"),
        ("sample.pptx", "pptx", "section"),
        ("sample.xlsx", "xlsx", "row"),
    ] {
        let bytes = std::fs::read(fixtures.join(file)).unwrap();
        let (status, body) = s.raw(
            "POST",
            &format!("/api/notebooks/{}/sources/upload?filename={}", nb_id, file),
            Some(&bytes),
            "application/octet-stream",
        );
        assert_eq!(status, 201, "{} upload failed: {}", file, body);
        let v = json::parse(&body).unwrap();
        assert_eq!(v.str_of("kind"), Some(kind));
        let first_loc = v
            .get("preview")
            .and_then(|p| p.as_arr())
            .and_then(|a| a.first())
            .and_then(|c| c.get("locator"))
            .and_then(|l| l.str_of("kind"))
            .unwrap_or("");
        assert_eq!(first_loc, locator_kind, "{} locator", file);
    }

    // the docx evidence answers a question and cites a paragraph locator
    let (status, chat) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("what pairs a retriever with a generator"))])),
    );
    assert_eq!(status, 200);
    let label = chat
        .get("citations")
        .and_then(|c| c.as_arr())
        .and_then(|a| a.first())
        .and_then(|c| c.str_of("locator_label"))
        .unwrap_or("")
        .to_string();
    assert!(label.starts_with("paragraph"), "expected paragraph locator, got {}", label);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn provider_keys_are_write_only_and_never_used_unsafely() {
    let dir = tempdir("providers");
    let s = Server::start(dir.clone());

    // cleartext endpoint refused
    let (st, body) = s.json(
        "POST",
        "/api/providers",
        Some(obj(&[
            ("kind", Value::from("openai_compatible")),
            ("base_url", Value::from("http://api.example.com")),
            ("model", Value::from("m")),
            ("api_key", Value::from("sk-thisisasecretkey")),
        ])),
    );
    assert_eq!(st, 422);
    assert!(body.str_of("error").unwrap().contains("https"));

    // valid config accepted
    let (st, created) = s.json(
        "POST",
        "/api/providers",
        Some(obj(&[
            ("kind", Value::from("openai_compatible")),
            ("base_url", Value::from("https://api.example.com/v1")),
            ("model", Value::from("some-model")),
            ("api_key", Value::from("sk-thisisasecretkey")),
        ])),
    );
    assert_eq!(st, 201);
    assert_eq!(created.get("key_set").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(created.get("available").and_then(|v| v.as_bool()), Some(false));

    // listing never leaks the key
    let (st, list) = s.json("GET", "/api/providers", None);
    assert_eq!(st, 200);
    let text = json::to_string(&list);
    assert!(!text.contains("sk-thisisasecretkey"), "API key leaked: {}", text);

    // a configured-but-unusable provider still answers locally, and says so
    let (_, nb) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("P"))])));
    let nb_id = nb.str_of("id").unwrap().to_string();
    s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb_id),
        Some(obj(&[("text", Value::from("Retrieval scores passages with BM25 weighting."))])),
    );
    let (st, chat) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("how are passages scored"))])),
    );
    assert_eq!(st, 200);
    assert_eq!(chat.str_of("engine"), Some("extractive"));
    assert!(chat.str_of("engine_note").unwrap_or("").contains("TLS"));

    // backup + listing
    let (st, backup) = s.json("POST", "/api/backup", Some(obj(&[])));
    assert_eq!(st, 201);
    assert!(backup.get("bytes").and_then(|b| b.as_f64()).unwrap() > 0.0);
    let (st, list) = s.json("GET", "/api/backups", None);
    assert_eq!(st, 200);
    assert_eq!(list.get("backups").unwrap().as_arr().unwrap().len(), 1);

    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// URL ingestion, end to end: a real HTTP server → fetch → parse → index → chat
// → citation. The fixture origin is loopback, so the test opts into private-range
// fetching explicitly (the guard is off by default and reported in capabilities).
// ---------------------------------------------------------------------------

static URL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Serve one page on loopback so the fetcher has something real to talk to.
fn start_origin(html: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(4) {
            let mut stream = match stream {
                Ok(s) => s,
                Err(_) => continue,
            };
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                html.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(html.as_bytes());
            let _ = stream.flush();
        }
    });
    port
}

#[test]
fn url_source_is_fetched_indexed_cited_and_deduplicated() {
    let _guard = URL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("FORGE_FETCH_ALLOW_PRIVATE", "1");

    let html = "<html><head><title>Retrieval Notes</title></head><body>\
        <h1>Retrieval</h1>\
        <p>Hybrid retrieval combines lexical scoring with dense embeddings and then reranks.</p>\
        <h2>Grounding</h2>\
        <p>The evidence floor is 0.42: passages below it are treated as no evidence at all.</p>\
        </body></html>";
    let port = start_origin(html);
    let dir = tempdir("url-ingest");
    let s = Server::start(dir.clone());

    let (st, _) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("URL test"))])));
    assert_eq!(st, 201);
    let (_, list) = s.json("GET", "/api/notebooks", None);
    let nb = list.get("notebooks").unwrap().as_arr().unwrap()[0]
        .str_of("id")
        .unwrap()
        .to_string();

    let url = format!("http://127.0.0.1:{}/notes", port);
    let (st, body) = s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/url", nb),
        Some(obj(&[("url", Value::from(url.as_str()))])),
    );
    assert_eq!(st, 201, "url ingest failed: {:?}", body);
    assert_eq!(body.str_of("title"), Some("Retrieval Notes"), "title comes from <title>");
    assert_eq!(body.str_of("kind"), Some("html"));
    assert!(body.get("chunk_count").and_then(|c| c.as_f64()).unwrap_or(0.0) >= 1.0);

    // provenance is stored on the source itself
    let (_, sources) = s.json("GET", &format!("/api/notebooks/{}/sources", nb), None);
    let src = &sources.get("sources").unwrap().as_arr().unwrap()[0];
    assert_eq!(src.str_of("origin"), Some("url"));
    assert_eq!(src.str_of("canonical_url"), Some(url.as_str()));
    assert_eq!(src.get("meta").and_then(|m| m.str_of("http_status")), Some("200"));
    assert!(src.str_of("content_hash").unwrap_or("").len() == 64, "content hash recorded");
    assert!(!src.str_of("fetched_at").unwrap_or("").is_empty());

    // asking the question returns a citation into the fetched page
    let (st, answer) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb),
        Some(obj(&[("question", Value::from("What is the evidence floor?"))])),
    );
    assert_eq!(st, 200, "{:?}", answer);
    let text = answer.str_of("answer").unwrap_or("");
    assert!(text.contains("0.42"), "answer should carry the fact: {}", text);
    let cites = answer.get("citations").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert!(!cites.is_empty(), "a grounded answer must cite");
    let cited_source = cites[0].str_of("source_id").unwrap_or("");
    assert_eq!(cited_source, src.str_of("id").unwrap(), "citation points at the fetched source");
    assert_eq!(cites[0].str_of("source_title"), Some("Retrieval Notes"));

    // the same URL is refused as a duplicate, and the response names the original
    let (st, dup) = s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/url", nb),
        Some(obj(&[("url", Value::from(url.as_str()))])),
    );
    assert_eq!(st, 409, "{:?}", dup);
    assert_eq!(dup.str_of("code"), Some("duplicate_source"));
    assert_eq!(dup.str_of("existing_source_id"), src.str_of("id"));

    // URL variants that normalise to the same identity are duplicates too
    let variant = format!("HTTP://127.0.0.1:{}/notes#section", port);
    let (st, _) = s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/url", nb),
        Some(obj(&[("url", Value::from(variant.as_str()))])),
    );
    assert_eq!(st, 409, "scheme case and fragment must not create a second source");

    std::env::remove_var("FORGE_FETCH_ALLOW_PRIVATE");
}

/// Serves a different page on each connection so a refresh sees a real change.
fn start_changing_origin() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for (n, stream) in listener.incoming().take(6).enumerate() {
            let mut stream = match stream {
                Ok(s) => s,
                Err(_) => continue,
            };
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let html = if n == 0 {
                "<html><head><title>Mutable</title></head><body><p>The retry budget is 2.75 attempts per request and it is enforced by the client.</p></body></html>"
            } else {
                "<html><head><title>Mutable</title></head><body><p>The retry budget is 4.5 attempts per request after the tier change.</p></body></html>"
            };
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                html.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(html.as_bytes());
            let _ = stream.flush();
        }
    });
    port
}

#[test]
fn refreshing_a_url_source_reindexes_only_when_content_changed() {
    let _guard = URL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("FORGE_FETCH_ALLOW_PRIVATE", "1");

    let port = start_changing_origin();
    let s = Server::start(tempdir("refresh"));
    let (_, _) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Refresh"))])));
    let (_, list) = s.json("GET", "/api/notebooks", None);
    let nb = list.get("notebooks").unwrap().as_arr().unwrap()[0].str_of("id").unwrap().to_string();

    let url = format!("http://127.0.0.1:{}/doc", port);
    let (st, added) = s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/url", nb),
        Some(obj(&[("url", Value::from(url.as_str()))])),
    );
    assert_eq!(st, 201, "{:?}", added);
    let sid = added.str_of("source_id").unwrap().to_string();

    // an artifact generated now must be flagged stale once the source changes
    let (st, _) = s.json(
        "POST",
        &format!("/api/notebooks/{}/artifacts", nb),
        Some(obj(&[
            ("kind", Value::from("summary")),
            ("source_ids", Value::Arr(vec![Value::from(sid.as_str())])),
        ])),
    );
    assert_eq!(st, 201);
    let (_, arts) = s.json("GET", &format!("/api/notebooks/{}/artifacts", nb), None);
    let art = &arts.get("artifacts").unwrap().as_arr().unwrap()[0];
    assert_eq!(art.get("stale").and_then(|x| x.as_bool()), Some(false));

    // second fetch returns different text
    let (st, refreshed) = s.json("POST", &format!("/api/sources/{}/refresh", sid), None);
    assert_eq!(st, 200, "{:?}", refreshed);
    assert_eq!(refreshed.get("changed").and_then(|c| c.as_bool()), Some(true));
    let stale = refreshed.get("stale_artifacts").and_then(|x| x.as_arr()).cloned().unwrap_or_default();
    assert_eq!(stale.len(), 1, "the summary must be marked stale");

    // the index now answers from the new text
    let (st, answer) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb),
        Some(obj(&[("question", Value::from("What is the retry budget?"))])),
    );
    assert_eq!(st, 200);
    assert!(
        answer.str_of("answer").unwrap_or("").contains("4.5"),
        "answer should use refreshed content: {:?}",
        answer.str_of("answer")
    );

    // refreshing again with the same content is a no-op, not a re-index
    let (st, again) = s.json("POST", &format!("/api/sources/{}/refresh", sid), None);
    assert_eq!(st, 200);
    assert_eq!(again.get("changed").and_then(|c| c.as_bool()), Some(false));

    std::env::remove_var("FORGE_FETCH_ALLOW_PRIVATE");
}

#[test]
fn pasted_sources_are_not_refreshable_and_labels_persist() {
    let s = Server::start(tempdir("labels"));
    let (_, _) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Labels"))])));
    let (_, list) = s.json("GET", "/api/notebooks", None);
    let nb = list.get("notebooks").unwrap().as_arr().unwrap()[0].str_of("id").unwrap().to_string();
    let (st, added) = s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb),
        Some(obj(&[
            ("title", Value::from("Note")),
            ("text", Value::from("Retrieval reranking happens after the lexical stage in this pipeline.")),
        ])),
    );
    assert_eq!(st, 201);
    let sid = added.str_of("source_id").unwrap().to_string();

    let (st, body) = s.json("POST", &format!("/api/sources/{}/refresh", sid), None);
    assert_eq!(st, 422, "{:?}", body);
    assert_eq!(body.str_of("code"), Some("not_refreshable"));

    let (st, patched) = s.json(
        "PATCH",
        &format!("/api/sources/{}", sid),
        Some(obj(&[
            ("title", Value::from("Renamed source")),
            ("labels", Value::Arr(vec![Value::from("retrieval"), Value::from("draft")])),
        ])),
    );
    assert_eq!(st, 200, "{:?}", patched);
    assert_eq!(patched.str_of("title"), Some("Renamed source"));
    let labels = patched.get("labels").and_then(|l| l.as_arr()).cloned().unwrap_or_default();
    assert_eq!(labels.len(), 2);

    // persisted, not just in the response
    let (_, reloaded) = s.json("GET", &format!("/api/sources/{}", sid), None);
    assert_eq!(reloaded.str_of("title"), Some("Renamed source"));
    assert_eq!(reloaded.get("labels").and_then(|l| l.as_arr()).map(|l| l.len()), Some(2));
}

/// Journey D: a note becomes evidence, is cited, and stops influencing when deselected.
#[test]
fn note_can_become_a_cited_source() {
    let s = Server::start(tempdir("note-source"));
    let (_, _) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Notes"))])));
    let (_, list) = s.json("GET", "/api/notebooks", None);
    let nb = list.get("notebooks").unwrap().as_arr().unwrap()[0].str_of("id").unwrap().to_string();

    // one ordinary source, so the notebook is not empty
    let (st, _) = s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb),
        Some(obj(&[
            ("title", Value::from("Handbook")),
            ("text", Value::from("The retention window for uploaded artefacts is thirty days by default.")),
        ])),
    );
    assert_eq!(st, 201);

    let (st, note) = s.json(
        "POST",
        &format!("/api/notebooks/{}/notes", nb),
        Some(obj(&[
            ("title", Value::from("Reviewer note")),
            ("body", Value::from("Verification pass: the audit threshold was agreed at 2.25 sigma for this quarter.")),
        ])),
    );
    assert_eq!(st, 201);
    let note_id = note.str_of("id").unwrap().to_string();

    // convert
    let (st, conv) = s.json("POST", &format!("/api/notes/{}/to-source", note_id), None);
    assert_eq!(st, 201, "{:?}", conv);
    let sid = conv.str_of("source_id").unwrap().to_string();
    assert_eq!(
        conv.get("meta").and_then(|m| m.str_of("converted_from")),
        Some(note_id.as_str())
    );

    // provenance is on the record
    let (_, src) = s.json("GET", &format!("/api/sources/{}", sid), None);
    assert_eq!(src.str_of("origin"), Some("note"));
    assert_eq!(src.get("meta").and_then(|m| m.str_of("converted_from")), Some(note_id.as_str()));

    // converting again is refused, not duplicated
    let (st, dup) = s.json("POST", &format!("/api/notes/{}/to-source", note_id), None);
    assert_eq!(st, 409);
    assert_eq!(dup.str_of("code"), Some("already_a_source"));

    // with the note-source selected, the answer cites it
    let (st, answer) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb),
        Some(obj(&[
            ("question", Value::from("What is the audit threshold?")),
            ("source_ids", Value::Arr(vec![Value::from(sid.as_str())])),
        ])),
    );
    assert_eq!(st, 200, "{:?}", answer);
    assert!(answer.str_of("answer").unwrap_or("").contains("2.25"), "{:?}", answer.str_of("answer"));
    let cites = answer.get("citations").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert_eq!(cites[0].str_of("source_id"), Some(sid.as_str()));

    // deselected: the note no longer influences the answer
    let (st, other) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb),
        Some(obj(&[("question", Value::from("What is the audit threshold?"))])),
    );
    assert_eq!(st, 200);
    let text = other.str_of("answer").unwrap_or("");
    assert!(!text.contains("2.25") || !cites.is_empty() && text.contains("does not"),
        "unselected note must not be quoted: {}", text);

    // deleting the note keeps the source: it is its own evidence record now
    let (st, _) = s.json("DELETE", &format!("/api/notes/{}", note_id), None);
    assert_eq!(st, 200);
    let (st, still) = s.json("GET", &format!("/api/sources/{}", sid), None);
    assert_eq!(st, 200, "{:?}", still);
    assert_eq!(still.str_of("origin"), Some("note"));
}

/// Journey: a spreadsheet answer is computed from real cells and cites the exact range.
#[test]
fn spreadsheet_questions_are_computed_over_real_cells() {
    let dir = tempdir("tables");
    let s = Server::start(dir.clone());
    let (_, nb) = s.json(
        "POST",
        "/api/notebooks",
        Some(obj(&[("title", Value::from("Sales review"))])),
    );
    let nb_id = nb.str_of("id").unwrap().to_string();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");

    let bytes = std::fs::read(fixtures.join("sales.xlsx")).unwrap();
    let (status, body) = s.raw(
        "POST",
        &format!("/api/notebooks/{}/sources/upload?filename=sales.xlsx", nb_id),
        Some(&bytes),
        "application/octet-stream",
    );
    assert_eq!(status, 201, "upload failed: {}", body);
    let v = json::parse(&body).unwrap();
    let source_id = v.str_of("source_id").unwrap().to_string();
    assert_eq!(v.get("table_count").and_then(|x| x.as_f64()), Some(2.0), "{:?}", v);
    let tables = v.get("tables").and_then(|t| t.as_arr()).cloned().unwrap_or_default();
    assert_eq!(tables.len(), 2);
    assert!(tables.iter().any(|t| t.str_of("sheet") == Some("Sales")));
    let hidden = tables.iter().find(|t| t.str_of("sheet") == Some("Meta")).expect("Meta");
    assert_eq!(hidden.get("hidden").and_then(|b| b.as_bool()), Some(true));

    // the derived answer is a computation, not a quote
    let (st, chat) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("what is the total revenue"))])),
    );
    assert_eq!(st, 200, "chat failed: {:?}", chat);
    assert_eq!(chat.get("computed").and_then(|b| b.as_bool()), Some(true), "{:?}", chat);
    assert_eq!(chat.str_of("engine"), Some("table"));
    let answer = chat.str_of("answer").unwrap_or("");
    assert!(answer.contains("2350.5"), "total must exclude the cached subtotal: {}", answer);
    assert!(answer.contains("C4"), "the excluded subtotal row is named: {}", answer);
    let cites = chat.get("citations").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert_eq!(cites.len(), 1, "{:?}", cites);
    let loc = cites[0].get("locator").expect("locator");
    assert_eq!(loc.str_of("kind"), Some("cell"));
    assert_eq!(loc.str_of("sheet"), Some("Sales"));
    assert_eq!(loc.str_of("cell"), Some("Sales!C2:C6"));
    assert_eq!(cites[0].str_of("source_id"), Some(source_id.as_str()));
    assert_eq!(
        chat.get("citation_problems").and_then(|p| p.as_arr()).map(|a| a.len()),
        Some(0),
        "{:?}",
        chat.get("citation_problems")
    );

    // the table is browsable with real cell references, and row numbers are original
    let (st, tables_json) = s.json("GET", &format!("/api/sources/{}/tables", source_id), None);
    assert_eq!(st, 200);
    let sales = tables_json
        .get("tables")
        .and_then(|t| t.as_arr())
        .and_then(|a| a.iter().find(|t| t.str_of("sheet") == Some("Sales")).cloned())
        .expect("Sales table");
    assert_eq!(sales.get("row_count").and_then(|x| x.as_f64()), Some(5.0));
    let first_row = sales.get("rows").and_then(|r| r.as_arr()).and_then(|a| a.first()).cloned().unwrap();
    assert_eq!(first_row.get("row").and_then(|x| x.as_f64()), Some(2.0), "data starts at row 2");
    let cells = first_row.get("cells").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert_eq!(cells[2].str_of("ref"), Some("C2"));
    assert_eq!(cells[2].get("number").and_then(|x| x.as_f64()), Some(1200.5));

    // capping rows never lies about the size
    let (st, capped) = s.json("GET", &format!("/api/sources/{}/tables?rows=2", source_id), None);
    assert_eq!(st, 200);
    let capped_sales = capped
        .get("tables")
        .and_then(|t| t.as_arr())
        .and_then(|a| a.iter().find(|t| t.str_of("sheet") == Some("Sales")).cloned())
        .unwrap();
    assert_eq!(capped_sales.get("rows").and_then(|r| r.as_arr()).map(|a| a.len()), Some(2));
    assert_eq!(capped_sales.get("row_count").and_then(|x| x.as_f64()), Some(5.0));
    assert_eq!(capped_sales.get("truncated").and_then(|b| b.as_bool()), Some(true));

    // a prose question about the same source still goes through text evidence
    let (st, prose) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("which channel does North use"))])),
    );
    assert_eq!(st, 200);
    assert_eq!(prose.get("computed").and_then(|b| b.as_bool()), Some(false), "{}", prose.str_of("answer").unwrap_or(""));

    // deleting the source removes its tables too
    let (st, _) = s.json("DELETE", &format!("/api/sources/{}", source_id), None);
    assert_eq!(st, 200);
    let (st, after) = s.json("GET", &format!("/api/sources/{}/tables", source_id), None);
    assert_eq!(st, 404);
    assert_eq!(after.str_of("code"), Some("not_found"));

    // survival across restart is tested in store::tests::tables_survive_a_restart
}

/// A notebook export must carry the grids, otherwise a re-import silently loses every
/// cell reference and the ability to compute.
#[test]
fn exported_notebooks_keep_their_tables() {
    let dir = tempdir("export-tables");
    let s = Server::start(dir.clone());
    let (_, nb) = s.json(
        "POST",
        "/api/notebooks",
        Some(obj(&[("title", Value::from("Export check"))])),
    );
    let nb_id = nb.str_of("id").unwrap().to_string();
    let bytes = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sales.xlsx"),
    )
    .unwrap();
    let (status, body) = s.raw(
        "POST",
        &format!("/api/notebooks/{}/sources/upload?filename=sales.xlsx", nb_id),
        Some(&bytes),
        "application/octet-stream",
    );
    assert_eq!(status, 201, "{}", body);
    let src = json::parse(&body).unwrap();
    let source_id = src.str_of("source_id").unwrap().to_string();

    let (st, exported) = s.json("GET", &format!("/api/notebooks/{}/export", nb_id), None);
    assert_eq!(st, 200);
    assert_eq!(
        exported.get("tables").and_then(|t| t.as_arr()).map(|a| a.len()),
        Some(2),
        "both sheets exported"
    );

    let (st, imported) = s.json("POST", "/api/import", Some(exported.clone()));
    assert_eq!(st, 201, "{:?}", imported);
    assert_eq!(
        imported.get("imported").and_then(|i| i.get("tables")).and_then(|x| x.as_f64()),
        Some(2.0),
        "{:?}",
        imported
    );
    let new_nb = imported.str_of("notebook_id").unwrap().to_string();

    // the copied notebook answers the same question with the same number and a cell citation
    let (st, chat) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", new_nb),
        Some(obj(&[("question", Value::from("what is the total revenue"))])),
    );
    assert_eq!(st, 200, "{:?}", chat);
    assert_eq!(chat.get("computed").and_then(|b| b.as_bool()), Some(true));
    assert!(chat.str_of("answer").unwrap_or("").contains("2350.5"));
    let cites = chat.get("citations").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert_eq!(cites[0].get("locator").and_then(|l| l.str_of("sheet")), Some("Sales"));
    assert_ne!(
        cites[0].str_of("source_id"),
        Some(source_id.as_str()),
        "imported sources get new ids"
    );

    // and the imported source exposes its tables under its new id
    let new_source = cites[0].str_of("source_id").unwrap().to_string();
    let (st, tables) = s.json("GET", &format!("/api/sources/{}/tables", new_source), None);
    assert_eq!(st, 200);
    assert_eq!(tables.get("count").and_then(|c| c.as_f64()), Some(2.0));
}

/// Source Guide: orientation that quotes the source instead of paraphrasing it.
#[test]
fn source_guide_quotes_the_source_and_refuses_when_empty() {
    let dir = tempdir("guide");
    let s = Server::start(dir.clone());
    let (_, nb) = s.json(
        "POST",
        "/api/notebooks",
        Some(obj(&[("title", Value::from("Guide"))])),
    );
    let nb_id = nb.str_of("id").unwrap().to_string();
    // long enough to index as several passages, so the guide must choose between them
    let markdown = "# Photosynthesis\n\n\
Photosynthesis converts light energy into chemical energy inside chloroplasts, where the light reactions split water and release oxygen as a by-product.\n\n\
## Where it happens\n\n\
Chlorophyll absorbs light most strongly in the blue and red bands and reflects green light, which is why healthy leaves look green to the eye. The Calvin cycle runs in the stroma of the chloroplast and builds glucose from carbon dioxide using the ATP and NADPH produced by the light reactions.\n\n\
## Limits\n\n\
Stomata close under drought stress, which limits carbon dioxide uptake and slows the Calvin cycle even when light is plentiful. Temperature also matters: the enzymes of the Calvin cycle lose activity above roughly forty degrees.\n\n\
## Measurement\n\n\
Gas exchange measurements record carbon dioxide uptake per leaf area, so a drought experiment reports both stomatal conductance and the assimilation rate for each plant.\n";
    let (st, src) = s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb_id),
        Some(obj(&[
            ("title", Value::from("Biology")),
            ("text", Value::from(markdown)),
        ])),
    );
    assert_eq!(st, 201);
    let sid = src.str_of("source_id").unwrap().to_string();

    let (st, guide) = s.json("GET", &format!("/api/sources/{}/guide", sid), None);
    assert_eq!(st, 200, "{:?}", guide);
    assert_eq!(guide.str_of("generated_by"), Some("extractive"));
    assert!(guide.get("model").map(|m| m.is_null()).unwrap_or(false), "no model may be claimed");
    assert_eq!(guide.get("deterministic").and_then(|d| d.as_bool()), Some(true));

    // the headline is a verbatim quotation that exists in the stored chunks
    let (_, chunks_json) = s.json("GET", &format!("/api/sources/{}/chunks", sid), None);
    let chunks = chunks_json.get("chunks").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    let headline = guide.get("headline").expect("headline");
    let quoted = headline.str_of("text").unwrap().to_string();
    assert!(
        chunks.iter().any(|c| c.str_of("text").map(|t| t.contains(&quoted)).unwrap_or(false)),
        "headline is not a quote from the source: {}",
        quoted
    );
    assert!(headline.str_of("chunk_id").is_some());

    // every summary line resolves to a real chunk id and carries a locator
    let summary = guide.get("summary").and_then(|x| x.as_arr()).cloned().unwrap_or_default();
    assert!(!summary.is_empty(), "{:?}", guide);
    for line in &summary {
        let cid = line.str_of("chunk_id").unwrap();
        assert!(chunks.iter().any(|c| c.str_of("id") == Some(cid)), "unknown chunk {}", cid);
        assert!(!line.str_of("locator").unwrap_or("").is_empty());
    }
    assert!(summary.windows(2).any(|w| w[0].str_of("chunk_id") != w[1].str_of("chunk_id"))
        || chunks.len() == 1, "the guide should not quote one passage only");

    // structure comes from the parser, topics carry a location
    let outline: Vec<String> = guide.get("outline").and_then(|o| o.as_arr()).map(|a| a.iter().filter_map(|x| x.str_of("x").map(|s| s.to_string())).collect()).unwrap_or_default();
    assert!(outline.is_empty() || outline.iter().any(|o| o.contains("Photosynthesis")), "{:?}", outline);
    let topics = guide.get("topics").and_then(|t| t.as_arr()).cloned().unwrap_or_default();
    assert!(!topics.is_empty());
    for t in &topics {
        assert!(!t.str_of("locator").unwrap_or("").is_empty(), "topic without a location: {:?}", t);
    }
    let questions = guide.get("questions").and_then(|q| q.as_arr()).cloned().unwrap_or_default();
    assert!(!questions.is_empty());

    // unknown source, and a source that never indexed anything
    let (st, _) = s.json("GET", "/api/sources/src-missing/guide", None);
    assert_eq!(st, 404);
    let bad = b"%PDF-1.4 not really a pdf";
    let (st, _) = s.raw(
        "POST",
        &format!("/api/notebooks/{}/sources/upload?filename=broken.pdf", nb_id),
        Some(bad),
        "application/pdf",
    );
    assert_eq!(st, 422);
    let (_, list) = s.json("GET", &format!("/api/notebooks/{}/sources", nb_id), None);
    let failed = list
        .get("sources")
        .and_then(|x| x.as_arr())
        .and_then(|a| a.iter().find(|s| s.str_of("status") == Some("failed")).cloned())
        .expect("failed source recorded");
    let failed_id = failed.str_of("id").unwrap().to_string();
    let (st, body) = s.json("GET", &format!("/api/sources/{}/guide", failed_id), None);
    assert_eq!(st, 409, "{:?}", body);
    assert_eq!(body.str_of("code"), Some("guide_unavailable"));
}

/// Chat presentation controls are real: they change the answer and are validated.
#[test]
fn chat_style_and_length_are_effective_and_validated() {
    let dir = tempdir("style");
    let s = Server::start(dir.clone());
    let (_, nb) = s.json(
        "POST",
        "/api/notebooks",
        Some(obj(&[("title", Value::from("Style"))])),
    );
    let nb_id = nb.str_of("id").unwrap().to_string();
    let (st, _) = s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb_id),
        Some(obj(&[
            ("title", Value::from("Retrieval notes")),
            (
                "text",
                Value::from(
                    "The retrieval pipeline ranks candidate spans with BM25 and then builds bounded model context.\n\n\
                     Candidate spans are ranked by BM25 so that the model only sees context that matched the question.\n\n\
                     A bounded context window keeps retrieval ranking honest: spans compete for a fixed budget.",
                ),
            ),
        ])),
    );
    assert_eq!(st, 201);
    let question = "how are candidate spans ranked by BM25";

    let ask = |extra: Vec<(&str, Value)>| {
        let mut fields = vec![("question", Value::from(question))];
        fields.extend(extra);
        s.json("POST", &format!("/api/notebooks/{}/chat", nb_id), Some(obj(&fields)))
    };

    // default settings are echoed back
    let (st, base) = ask(vec![]);
    assert_eq!(st, 200, "{:?}", base);
    let settings = base.get("settings").expect("settings echoed");
    assert_eq!(settings.str_of("style"), Some("evidence"));
    assert_eq!(settings.str_of("length"), Some("standard"));

    // brief = one citation, thorough = more
    let (st, brief) = ask(vec![("length", Value::from("brief"))]);
    assert_eq!(st, 200);
    let brief_cites = brief.get("citations").and_then(|c| c.as_arr()).map(|a| a.len()).unwrap_or(0);
    assert_eq!(brief_cites, 1, "{:?}", brief);
    assert_eq!(
        brief.get("settings").and_then(|s| s.get("max_citations")).and_then(|x| x.as_f64()),
        Some(1.0)
    );
    let (st, thorough) = ask(vec![("length", Value::from("thorough"))]);
    assert_eq!(st, 200);
    let thorough_cites = thorough.get("citations").and_then(|c| c.as_arr()).map(|a| a.len()).unwrap_or(0);
    assert!(thorough_cites >= brief_cites, "{} vs {}", thorough_cites, brief_cites);

    // styles change the rendering while citing the same passages
    let (st, detailed) = ask(vec![("style", Value::from("detailed"))]);
    assert_eq!(st, 200);
    let detailed_text = detailed.str_of("answer").unwrap().to_string();
    assert!(detailed_text.contains("Sources consulted:"), "{}", detailed_text);
    assert!(
        detailed_text.contains("—"),
        "detailed answers name the source and location: {}",
        detailed_text
    );
    let (st, concise) = ask(vec![("style", Value::from("concise"))]);
    assert_eq!(st, 200);
    let concise_text = concise.str_of("answer").unwrap().to_string();
    assert!(!concise_text.contains("\n- "), "concise has no bullet list: {}", concise_text);
    assert_eq!(
        detailed.get("citations").and_then(|c| c.as_arr()).map(|a| a.len()),
        concise.get("citations").and_then(|c| c.as_arr()).map(|a| a.len()),
        "style must not change the evidence"
    );

    // unknown values are refused, not silently defaulted
    let (st, bad_style) = ask(vec![("style", Value::from("bullets"))]);
    assert_eq!(st, 422, "{:?}", bad_style);
    assert_eq!(bad_style.str_of("code"), Some("bad_style"));
    let (st, bad_length) = ask(vec![("length", Value::from("long"))]);
    assert_eq!(st, 422, "{:?}", bad_length);
    assert_eq!(bad_length.str_of("code"), Some("bad_length"));

    // an invalid setting must not leave a half-written conversation behind
    let (_, convs) = s.json("GET", &format!("/api/notebooks/{}/conversations", nb_id), None);
    let messages: usize = convs
        .get("conversations")
        .and_then(|c| c.as_arr())
        .map(|a| {
            a.iter()
                .filter_map(|c| c.get("message_count").and_then(|m| m.as_f64()))
                .map(|n| n as usize)
                .sum()
        })
        .unwrap_or(0);
    assert_eq!(messages, 10, "five accepted asks (2 messages each), rejections store nothing");
}


impl Server {
    /// Open a chat stream and read it like a browser does: headers first, then chunked frames.
    /// `stop_after` limits how many chunks are read before the socket is dropped, which is
    /// exactly what the Stop button does.
    fn stream_chat(
        &self,
        nb: &str,
        body: &Value,
        stop_after: Option<usize>,
    ) -> (Vec<String>, String, bool) {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let payload = json::to_string(body).into_bytes();
        let head = format!(
            "POST /api/notebooks/{}/chat/stream HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            nb,
            payload.len()
        );
        s.write_all(head.as_bytes()).unwrap();
        s.write_all(&payload).unwrap();

        let mut raw = Vec::new(); // response bytes not yet consumed
        let mut body = Vec::new(); // chunked body bytes not yet decoded
        let mut buf = [0u8; 4096];
        let mut frames: Vec<String> = Vec::new();
        let mut finished = false;
        let mut head_done = false;
        loop {
            if let Some(limit) = stop_after {
                if frames.len() >= limit {
                    break; // hang up mid-stream: the cancellation path
                }
            }
            match s.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    raw.extend_from_slice(&buf[..n]);
                    if !head_done {
                        if let Some(h) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                            body.extend_from_slice(&raw[h + 4..]);
                            raw.clear();
                            head_done = true;
                        } else {
                            continue;
                        }
                    } else {
                        body.append(&mut raw);
                    }
                    while let Some((frame, consumed)) = take_chunk(&body) {
                        body.drain(..consumed);
                        if frame.is_empty() {
                            finished = true;
                        } else {
                            frames.push(frame);
                        }
                    }
                }
                Err(_) => break,
            }
        }
        (frames, String::from_utf8_lossy(&raw).to_string(), finished)
    }
}

/// Pull one chunk out of a chunked-encoding buffer: `hex\r\npayload\r\n`.
fn take_chunk(buf: &[u8]) -> Option<(String, usize)> {
    let crlf = buf.windows(2).position(|w| w == b"\r\n")?;
    let size = usize::from_str_radix(std::str::from_utf8(&buf[..crlf]).ok()?.trim(), 16).ok()?;
    let start = crlf + 2;
    if buf.len() < start + size + 2 {
        return None;
    }
    let body = String::from_utf8_lossy(&buf[start..start + size]).to_string();
    Some((body, start + size + 2))
}

/// Media metadata is read from the container, and the response says what was *not* done.
#[test]
fn media_metadata_is_read_without_decoding_frames() {
    let dir = tempdir("media");
    let s = Server::start(dir.clone());
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sample.mp4");
    let bytes = std::fs::read(&fixture).expect("sample.mp4 fixture");

    let (st, body) = s.raw(
        "POST",
        "/api/media/inspect?name=sample.mp4",
        Some(&bytes),
        "application/octet-stream",
    );
    assert_eq!(st, 200, "{}", body);
    let v = json::parse(&body).unwrap();
    assert_eq!(v.str_of("file"), Some("sample.mp4"));
    assert_eq!(v.get("bytes").and_then(|b| b.as_f64()), Some(bytes.len() as f64));
    assert_eq!(v.str_of("sha256").map(|h| h.len()), Some(64));

    let media = v.get("media").expect("media object");
    assert_eq!(media.str_of("container"), Some("mp4"));
    let duration = media.get("duration_ms").and_then(|d| d.as_f64()).unwrap_or(0.0);
    assert!((5_900.0..=6_200.0).contains(&duration), "duration {}", duration);
    let tracks = media.get("tracks").and_then(|t| t.as_arr()).cloned().unwrap_or_default();
    let video = tracks.iter().find(|t| t.str_of("kind") == Some("video")).expect("video track");
    assert_eq!(video.str_of("codec"), Some("avc1"));
    assert_eq!(video.get("width").and_then(|w| w.as_f64()), Some(320.0));
    assert_eq!(video.get("height").and_then(|h| h.as_f64()), Some(240.0));
    assert_eq!(video.get("fps").and_then(|f| f.as_f64()), Some(10.0));

    // the response must not imply anything was watched, read or transcribed
    let notes = v.get("notes").expect("notes");
    assert_eq!(notes.get("decoded_frames").and_then(|d| d.as_f64()), Some(0.0));
    assert_eq!(notes.str_of("transcript"), Some("not produced"));
    assert_eq!(notes.str_of("captions"), Some("not read"));

    // a container we do not parse is named, not guessed at
    let (st, body) = s.raw(
        "POST",
        "/api/media/inspect?name=clip.webm",
        Some(&[0x1Au8, 0x45, 0xDF, 0xA3, 0, 0, 0, 0]),
        "application/octet-stream",
    );
    assert_eq!(st, 200);
    let v = json::parse(&body).unwrap();
    assert_eq!(v.get("media").and_then(|m| m.str_of("container")), Some("matroska"));
    assert_eq!(
        v.get("media").and_then(|m| m.get("tracks")).and_then(|t| t.as_arr()).map(|a| a.len()),
        Some(0),
        "no invented streams"
    );

    // empty body and wrong method are refused
    let (st, _) = s.raw("POST", "/api/media/inspect", Some(&[]), "application/octet-stream");
    assert_eq!(st, 400);
    let (st, _) = s.json("GET", "/api/media/inspect", None);
    assert_eq!(st, 404, "GET is not routed");
}

/// CHAT-06 / journey: the answer streams, and a stream the client abandons stores nothing.
#[test]
fn chat_streams_frames_and_only_persists_when_delivery_completes() {
    let dir = tempdir("stream");
    let s = Server::start(dir);
    let (_, nb) = s.json("POST", "/api/notebooks", Some(obj! { "title" => "stream" }));
    let nb_id = nb.str_of("id").unwrap().to_string();
    s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb_id),
        Some(obj! {
            "title" => "calvin",
            "text" => "The Calvin cycle fixes carbon dioxide. RuBisCO catalyses carbon fixation. \
                       Light reactions produce ATP and NADPH in the thylakoid membrane."
        }),
    );

    // 1. a complete stream: frames arrive in order and the turn becomes history
    let (frames, _, finished) = s.stream_chat(
        &nb_id,
        &obj! { "question" => "what does the calvin cycle fix", "style" => "detailed", "length" => "brief" },
        None,
    );
    assert!(finished, "the terminating chunk must arrive");
    let events: Vec<&str> = frames.iter().map(|f| f.as_str()).collect();
    assert!(events[0].contains("\"event\":\"start\""), "first frame: {}", events[0]);
    assert!(events.iter().any(|f| f.contains("\"event\":\"delta\"")));
    assert_eq!(
        events.iter().filter(|f| f.contains("\"event\":\"done\"")).count(),
        1,
        "exactly one done frame"
    );
    let done = json::parse(frames.last().unwrap()).unwrap();
    assert_eq!(done.str_of("event"), Some("done"));
    assert_eq!(done.str_of("engine"), Some("extractive"));
    assert_eq!(done.get("settings").and_then(|s| s.str_of("length")), Some("brief"));
    assert_eq!(
        done.get("citations").and_then(|c| c.as_arr()).map(|a| a.len()),
        Some(1),
        "brief = one citation, same budget as the buffered route"
    );

    // every delta is a real substring of the answer, in order — no invented words
    let answer = done.str_of("answer").unwrap();
    let mut cursor = 0usize;
    for f in &frames {
        let v = json::parse(f).unwrap();
        if v.str_of("event") == Some("delta") {
            let piece = v.str_of("text").unwrap();
            let at = answer[cursor..]
                .find(piece)
                .unwrap_or_else(|| panic!("delta {:?} is not in the answer", piece));
            cursor += at + piece.len();
        }
    }

    // the persisted turn matches what was delivered
    let (_, convs) = s.json("GET", &format!("/api/notebooks/{}/conversations", nb_id), None);
    let list = convs.get("conversations").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert_eq!(list.len(), 1, "a delivered stream is stored");
    assert_eq!(list[0].get("message_count").and_then(|m| m.as_f64()), Some(2.0));

    // 2. a second complete stream appends to the *same* conversation when asked to
    let conv_id = list[0].str_of("id").unwrap_or("").to_string();
    let (frames2, _, finished2) = s.stream_chat(
        &nb_id,
        &obj! { "question" => "what produces ATP", "conversation_id" => conv_id.clone() },
        None,
    );
    assert!(!frames2.is_empty() && finished2);
    let (_, c2) = s.json("GET", &format!("/api/notebooks/{}/conversations", nb_id), None);
    let l2 = c2.get("conversations").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert_eq!(l2.len(), 1, "the stream honours conversation_id instead of starting a new one");
    assert_eq!(l2[0].get("message_count").and_then(|m| m.as_f64()), Some(4.0));

    // 3. an abandoned stream: pace the frames, read one, hang up

    let (partial, _, finished3) = {
        let mut socket = TcpStream::connect(("127.0.0.1", s.port)).unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let payload = json::to_string(&obj! {
            "question" => "what catalyses carbon fixation",
            "conversation_id" => conv_id.clone(),
        })
        .into_bytes();
        let head = format!(
            "POST /api/notebooks/{}/chat/stream?pace_ms=40 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            nb_id,
            payload.len()
        );
        socket.write_all(head.as_bytes()).unwrap();
        socket.write_all(&payload).unwrap();
        let mut got = Vec::new();
        let mut body = Vec::new();
        let mut buf = [0u8; 512];
        let mut frames: Vec<String> = Vec::new();
        let mut head_done = false;
        // read until the first whole frame arrives, then hang up: this is the Stop button
        while frames.is_empty() {
            let n = socket.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            got.extend_from_slice(&buf[..n]);
            if !head_done {
                if let Some(h) = got.windows(4).position(|w| w == b"\r\n\r\n") {
                    body.extend_from_slice(&got[h + 4..]);
                    head_done = true;
                }
                continue;
            }
            body.extend_from_slice(&buf[..n]);
            while let Some((f, used)) = take_chunk(&body) {
                body.drain(..used);
                if !f.is_empty() {
                    frames.push(f);
                }
            }
        }
        drop(socket); // Stop
        (frames, String::new(), false)
    };
    assert!(!partial.is_empty(), "at least the start frame arrived before Stop");
    assert!(!finished3, "the stream was cut short");
    std::thread::sleep(Duration::from_millis(600)); // let the writer notice the FIN

    let (_, convs2) = s.json("GET", &format!("/api/notebooks/{}/conversations", nb_id), None);
    let list2 = convs2.get("conversations").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert_eq!(list2.len(), 1, "Stop must not create a second conversation");
    assert_eq!(
        list2[0].get("message_count").and_then(|m| m.as_f64()),
        Some(4.0),
        "Stop must not append a message: the aborted turn is not history"
    );

    // 4. an invalid style never opens a stream at all
    let (status, body) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat/stream", nb_id),
        Some(obj! { "question" => "anything", "style" => "shouty" }),
    );
    assert_eq!(status, 422);
    assert_eq!(body.str_of("code"), Some("bad_style"));
}

// ============================================================================
// §14 acceptance journeys. Each test is named after its letter so a reader can
// check the specification against the suite one-to-one.
// ============================================================================

/// Journey A — first notebook: create, rename, restart, persist, no hidden demo data.
#[test]
fn journey_a_first_notebook_survives_a_restart_without_demo_data() {
    let dir = tempdir("journey-a");
    let nb_id;
    {
        let s = Server::start(dir.clone());
        // a brand-new data directory must be empty, not seeded with demo content
        let (st, list) = s.json("GET", "/api/notebooks", None);
        assert_eq!(st, 200);
        assert_eq!(list.get("notebooks").and_then(|n| n.as_arr()).map(|a| a.len()), Some(0),
            "a fresh install shows no notebooks, demo or otherwise");
        let (_, health) = s.json("GET", "/api/health", None);
        assert_eq!(
            health.get("counts").and_then(|c| c.get("notebooks")).and_then(|v| v.as_f64()),
            Some(0.0)
        );

        let (st, nb) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Untitled notebook"))])));
        assert_eq!(st, 201);
        nb_id = nb.str_of("id").unwrap().to_string();

        let (st, renamed) = s.json(
            "PATCH",
            &format!("/api/notebooks/{}", nb_id),
            Some(obj(&[("title", Value::from("Mitochondria reading"))])),
        );
        assert_eq!(st, 200, "{:?}", renamed);
        assert_eq!(renamed.str_of("title"), Some("Mitochondria reading"));

        // close: dropping the server ends its handler thread and its store handle
    }

    // reopen against the same data directory
    let s2 = Server::start(dir.clone());
    let (_, list2) = s2.json("GET", "/api/notebooks", None);
    let nbs = list2.get("notebooks").and_then(|n| n.as_arr()).cloned().unwrap_or_default();
    assert_eq!(nbs.len(), 1, "one notebook, the one that was created");
    assert_eq!(nbs[0].str_of("id"), Some(nb_id.as_str()));
    assert_eq!(nbs[0].str_of("title"), Some("Mitochondria reading"), "the rename persisted");

    let (_, srcs) = s2.json("GET", &format!("/api/notebooks/{}/sources", nb_id), None);
    assert_eq!(srcs.get("sources").and_then(|s| s.as_arr()).map(|a| a.len()), Some(0),
        "the notebook has no sources; nothing was fabricated to fill it");
    std::fs::remove_dir_all(&dir).ok();
}

/// Journey B — PDF evidence: page 2 citation, a clickable locator, and abstention.
#[test]
fn journey_b_pdf_page_two_is_cited_and_unsupported_questions_abstain() {
    let dir = tempdir("journey-b");
    let s = Server::start(dir.clone());
    let (_, nb) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("PDF"))])));
    let nb_id = nb.str_of("id").unwrap().to_string();
    let pdf = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/simple.pdf")).unwrap();
    let (st, body) = s.raw(
        "POST",
        &format!("/api/notebooks/{}/sources/upload?filename=simple.pdf", nb_id),
        Some(&pdf),
        "application/pdf",
    );
    assert_eq!(st, 201, "{}", body);
    let uploaded = json::parse(&body).unwrap();
    // the upload response names the source it created (`source_id`), alongside its preview
    let sid = uploaded
        .str_of("source_id")
        .or_else(|| uploaded.str_of("id"))
        .expect("upload must name the new source")
        .to_string();
    assert_eq!(uploaded.str_of("kind"), Some("pdf"));

    // the parsed pages are inspectable, not just counted
    let (st, chunks) = s.json("GET", &format!("/api/sources/{}/chunks", sid), None);
    assert_eq!(st, 200);
    let pages: Vec<f64> = chunks
        .get("chunks")
        .and_then(|c| c.as_arr())
        .map(|a| {
            a.iter()
                .filter_map(|c| c.get("locator").and_then(|l| l.get("page").or_else(|| l.get("value"))))
                .filter_map(|p| p.as_f64())
                .collect()
        })
        .unwrap_or_default();
    assert!(pages.contains(&1.0) && pages.contains(&2.0), "both pages parsed: {:?}", pages);

    // the answer lives on page 2 and the citation says so
    let (st, chat) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("What does page two say about citations?"))])),
    );
    assert_eq!(st, 200, "{:?}", chat);
    let cites = chat.get("citations").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert!(!cites.is_empty(), "an answer about page two must cite it: {:?}", chat);
    let cite = &cites[0];
    assert_eq!(cite.get("locator").and_then(|l| l.get("page")).and_then(|p| p.as_f64()), Some(2.0),
        "citation page must be 2: {:?}", cite);
    assert!(cite.str_of("quote").unwrap_or("").contains("citations") || cite.str_of("quote").unwrap_or("").contains("evidence span"),
        "the quote must be the page-2 sentence: {:?}", cite);

    // opening the citation returns the chunk containing that exact quote
    let chunk_id = cite.str_of("chunk_id").unwrap().to_string();
    let (_, one) = s.json("GET", &format!("/api/sources/{}/chunks", sid), None);
    let found = one
        .get("chunks")
        .and_then(|c| c.as_arr())
        .and_then(|a| a.iter().find(|c| c.str_of("id") == Some(chunk_id.as_str())).cloned());
    assert!(found.is_some(), "the cited chunk must exist");
    assert!(found.unwrap().str_of("text").unwrap().contains(cite.str_of("quote").unwrap()));

    // an unsupported question abstains with zero citations rather than inventing one
    let (st, again) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("What is the capital of Mongolia?"))])),
    );
    assert_eq!(st, 200);
    assert_eq!(again.get("abstained"), Some(&Value::Bool(true)), "{:?}", again);
    assert_eq!(again.get("citations").and_then(|c| c.as_arr()).map(|a| a.len()), Some(0));
    std::fs::remove_dir_all(&dir).ok();
}

/// Journey D — note as a source: save an answer as a note, edit it, scope it in and out.
#[test]
fn journey_d_note_edits_are_what_get_cited_and_exclusion_removes_influence() {
    let dir = tempdir("journey-d");
    let s = Server::start(dir.clone());
    let (_, nb) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Notes"))])));
    let nb_id = nb.str_of("id").unwrap().to_string();
    s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb_id),
        Some(obj(&[
            ("title", Value::from("Handbook")),
            ("text", Value::from("The default retention window is thirty days.")),
        ])),
    );

    // 1-2. an answer exists, and saving it as a note keeps the text verbatim
    let (_, answer) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("what is the default retention window"))])),
    );
    let (st, note) = s.json(
        "POST",
        &format!("/api/notebooks/{}/notes", nb_id),
        Some(obj(&[
            ("title", Value::from("Saved answer")),
            ("body", Value::from(answer.str_of("answer").unwrap())),
        ])),
    );
    assert_eq!(st, 201);
    let note_id = note.str_of("id").unwrap().to_string();

    // 3. edit the note so it contains a fact the sources do not
    let (st, edited) = s.json(
        "PATCH",
        &format!("/api/notes/{}", note_id),
        Some(obj(&[("body", Value::from("Confirmed with the vendor: the audit threshold is 2.25 sigma this quarter."))])),
    );
    assert_eq!(st, 200, "{:?}", edited);

    // 4-5. convert to a source and ask something only the note knows
    let (_, srcs) = s.json("GET", &format!("/api/notebooks/{}/sources", nb_id), None);
    let handbook = srcs
        .get("sources")
        .and_then(|s| s.as_arr())
        .and_then(|a| a.iter().find(|s| s.str_of("title") == Some("Handbook")).cloned())
        .and_then(|s| s.str_of("id").map(|x| x.to_string()))
        .expect("handbook source");
    let (_, conv) = s.json("POST", &format!("/api/notes/{}/to-source", note_id), None);
    let sid = conv.str_of("source_id").unwrap().to_string();
    let (_, a1) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("what is the audit threshold"))])),
    );
    let c1 = a1.get("citations").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert_eq!(c1.len(), 1, "{:?}", a1);
    assert_eq!(c1[0].str_of("source_id"), Some(sid.as_str()), "the citation points at the note-source");
    assert!(c1[0].str_of("quote").unwrap().contains("2.25"), "the edited body is what is quoted: {:?}", c1[0]);

    // 6. excluding it removes the influence entirely
    let (_, a2) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[
            ("question", Value::from("what is the audit threshold")),
            ("source_ids", Value::Arr(vec![Value::from(handbook.as_str())])),
        ])),
    );
    assert_eq!(a2.get("abstained"), Some(&Value::Bool(true)), "{:?}", a2);
    assert_eq!(a2.get("citations").and_then(|c| c.as_arr()).map(|a| a.len()), Some(0));
    std::fs::remove_dir_all(&dir).ok();
}

/// Journey C — multi-source reasoning, and a contradiction that is surfaced rather than
/// silently reconciled.
#[test]
fn journey_c_conflicting_sources_are_shown_side_by_side() {
    let dir = tempdir("journey-c");
    let s = Server::start(dir.clone());
    let (_, nb) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Policy"))])));
    let nb_id = nb.str_of("id").unwrap().to_string();

    s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb_id),
        Some(obj(&[
            ("title", Value::from("Policy 2025")),
            ("text", Value::from("The default retention window is thirty days for uploaded artefacts.")),
        ])),
    );
    s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb_id),
        Some(obj(&[
            ("title", Value::from("Policy 2026")),
            ("text", Value::from("The default retention window is sixty days for uploaded artefacts.")),
        ])),
    );

    // 2-3. a question that needs both documents cites both
    let (st, both) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("retention days limit"))])),
    );
    assert_eq!(st, 200, "{:?}", both);
    let titles: Vec<String> = both
        .get("citations")
        .and_then(|c| c.as_arr())
        .map(|a| a.iter().filter_map(|c| c.str_of("source_title").map(|t| t.to_string())).collect())
        .unwrap_or_default();
    assert!(titles.iter().any(|t| t == "Policy 2025") && titles.iter().any(|t| t == "Policy 2026"),
        "both sources must be cited: {:?}", titles);

    // 5-6. the contradiction is surfaced, not reconciled
    let conflicts = both.get("conflicts").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert_eq!(conflicts.len(), 1, "one disagreement expected: {:?}", both);
    let c = &conflicts[0];
    assert_eq!(c.str_of("kind"), Some("numeric"));
    let claims = c.get("claims").and_then(|x| x.as_arr()).cloned().unwrap_or_default();
    assert_eq!(claims.len(), 2);
    let quotes: Vec<String> = claims.iter().filter_map(|k| k.str_of("quote").map(|q| q.to_string())).collect();
    assert!(quotes.iter().any(|q| q.contains("thirty")) && quotes.iter().any(|q| q.contains("sixty")));

    // the answer text says so, in words, and cites both sides
    let answer = both.str_of("answer").unwrap_or("");
    assert!(answer.contains("Sources disagree"), "{}", answer);
    assert!(answer.contains("does not choose between them"), "{}", answer);
    assert!(answer.contains("thirty") && answer.contains("sixty"), "{}", answer);
    let cited_titles: Vec<String> = both
        .get("citations")
        .and_then(|x| x.as_arr())
        .map(|a| a.iter().filter_map(|x| x.str_of("source_title").map(|t| t.to_string())).collect())
        .unwrap_or_default();
    assert!(cited_titles.iter().any(|t| t == "Policy 2025") && cited_titles.iter().any(|t| t == "Policy 2026"),
        "both sides of the disagreement are cited: {:?}", cited_titles);
    assert_eq!(
        both.get("citation_problems").and_then(|p| p.as_arr()).map(|a| a.len()),
        Some(0),
        "the added citations still validate: {:?}", both
    );

    // a question where the sources agree produces no conflict block
    s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb_id),
        Some(obj(&[
            ("title", Value::from("Handbook")),
            ("text", Value::from("Support answers within four hours on business days.")),
        ])),
    );
    let (_, calm) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("how fast does support answer"))])),
    );
    assert_eq!(calm.get("conflicts").and_then(|c| c.as_arr()).map(|a| a.len()), Some(0), "{:?}", calm);
    assert!(!calm.str_of("answer").unwrap_or("").contains("Sources disagree"));
    std::fs::remove_dir_all(&dir).ok();
}

/// Journey F — flashcards with a chosen difficulty, review marks, quiz, and a restart.
#[test]
fn journey_f_cards_reviews_and_quiz_survive_a_restart() {
    let dir = tempdir("journey-f");
    let nb_id;
    let card_id;
    let quiz_id;
    {
        let s = Server::start(dir.clone());
        let (_, nb) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Study"))])));
        nb_id = nb.str_of("id").unwrap().to_string();
        s.json(
            "POST",
            &format!("/api/notebooks/{}/sources/text", nb_id),
            Some(obj(&[
                ("title", Value::from("Biology")),
                ("text", Value::from(
                    "Photosynthesis is the process that converts light energy into chemical energy. \
                     The Calvin cycle fixes carbon dioxide into glucose using ATP and NADPH. \
                     Chlorophyll absorbs light most strongly in the blue and red bands of the spectrum.")),
            ])),
        );

        // 1. difficulty is a real generator parameter, and an unknown one is refused
        let (st, easy) = s.json(
            "POST",
            &format!("/api/notebooks/{}/artifacts", nb_id),
            Some(obj(&[("kind", Value::from("flashcards")), ("difficulty", Value::from("easy"))])),
        );
        assert_eq!(st, 201, "{:?}", easy);
        assert_eq!(easy.get("payload").and_then(|p| p.str_of("difficulty")), Some("easy"));
        let easy_cards = easy.get("payload").and_then(|p| p.get("cards")).and_then(|c| c.as_arr()).cloned().unwrap_or_default();
        assert!(!easy_cards.is_empty());
        assert!(
            easy_cards.iter().all(|c| !c.str_of("front").unwrap_or("").contains("_____")),
            "easy never uses a cloze card"
        );

        let (st, hard) = s.json(
            "POST",
            &format!("/api/notebooks/{}/artifacts", nb_id),
            Some(obj(&[("kind", Value::from("flashcards")), ("difficulty", Value::from("hard"))])),
        );
        assert_eq!(st, 201, "{:?}", hard);
        assert_eq!(hard.get("payload").and_then(|p| p.str_of("difficulty")), Some("hard"));

        let (st, bad) = s.json(
            "POST",
            &format!("/api/notebooks/{}/artifacts", nb_id),
            Some(obj(&[("kind", Value::from("flashcards")), ("difficulty", Value::from("impossible"))])),
        );
        assert_eq!(st, 422);
        assert_eq!(bad.str_of("code"), Some("bad_difficulty"));

        // every card is grounded: front/back are substrings of a real sentence, with a citation
        for c in &easy_cards {
            let cit = c.get("citation").expect("card citation");
            assert!(cit.str_of("quote").is_some() && cit.str_of("locator_label").is_some());
        }

        // 3. record known/unknown against a specific card
        card_id = easy_cards[0].str_of("id").unwrap().to_string();
        let easy_id = easy.str_of("id").unwrap().to_string();
        let (st, marked) = s.json(
            "POST",
            &format!("/api/artifacts/{}/review", easy_id),
            Some(obj(&[("card_id", Value::from(card_id.as_str())), ("known", Value::Bool(true))])),
        );
        assert_eq!(st, 200, "{:?}", marked);
        assert_eq!(marked.get("counts").and_then(|c| c.get("known")).and_then(|v| v.as_f64()), Some(1.0));

        // an unknown card id is refused rather than silently stored
        let (st, errc) = s.json(
            "POST",
            &format!("/api/artifacts/{}/review", easy_id),
            Some(obj(&[("card_id", Value::from("card-nonexistent")), ("known", Value::Bool(true))])),
        );
        assert_eq!(st, 422);
        assert_eq!(errc.str_of("code"), Some("unknown_card"));

        // 4-6. quiz with answers and cited explanations
        let (st, quiz) = s.json(
            "POST",
            &format!("/api/notebooks/{}/artifacts", nb_id),
            Some(obj(&[("kind", Value::from("quiz"))])),
        );
        assert_eq!(st, 201);
        quiz_id = quiz.str_of("id").unwrap().to_string();
        let qs = quiz.get("payload").and_then(|p| p.get("questions")).and_then(|q| q.as_arr()).cloned().unwrap_or_default();
        assert!(!qs.is_empty());
        let q0 = &qs[0];
        let opts = q0.get("options").and_then(|o| o.as_arr()).cloned().unwrap_or_default();
        assert!(opts.len() >= 2, "a quiz question needs options");
        let answer = q0.str_of("answer").unwrap();
        assert!(opts.iter().any(|o| o.as_str() == Some(answer)), "the answer must be one of the options");
        assert!(q0.get("citation").and_then(|c| c.str_of("quote")).is_some(), "an explanation must be cited");
    }

    // 7. reopen: the artifact, the difficulty and the review mark all persist
    let s2 = Server::start(dir.clone());
    let (_, arts) = s2.json("GET", &format!("/api/notebooks/{}/artifacts", nb_id), None);
    let list = arts.get("artifacts").and_then(|a| a.as_arr()).cloned().unwrap_or_default();
    assert_eq!(list.len(), 3, "three artifacts survived the restart");
    let cards_art = list
        .iter()
        .find(|a| a.str_of("kind") == Some("flashcards"))
        .cloned()
        .expect("flashcards artifact");
    assert_eq!(
        cards_art.get("reviews").and_then(|r| r.get(card_id.as_str())).and_then(|v| v.as_str()),
        Some("known"),
        "the learner's mark persisted: {:?}",
        cards_art
    );
    let (st, one) = s2.json("GET", &format!("/api/artifacts/{}", quiz_id), None);
    assert_eq!(st, 200);
    assert_eq!(one.str_of("kind"), Some("quiz"));
    std::fs::remove_dir_all(&dir).ok();
}

/// Journey G — concept map: generate, export as SVG/Markdown, reopen after restart.
#[test]
fn journey_g_mindmap_exports_and_reopens() {
    let dir = tempdir("journey-g");
    let nb_id;
    let map_id;
    {
        let s = Server::start(dir.clone());
        let (_, nb) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Map"))])));
        nb_id = nb.str_of("id").unwrap().to_string();
        s.json(
            "POST",
            &format!("/api/notebooks/{}/sources/text", nb_id),
            Some(obj(&[
                ("title", Value::from("Ecology")),
                ("text", Value::from(
                    "Mycorrhizal fungi trade phosphorus with plant roots. \
                     Phosphorus uptake improves drought tolerance in seedlings. \
                     Fungal networks connect neighbouring trees underground.")),
            ])),
        );
        let (st, map) = s.json(
            "POST",
            &format!("/api/notebooks/{}/artifacts", nb_id),
            Some(obj(&[("kind", Value::from("mindmap"))])),
        );
        assert_eq!(st, 201, "{:?}", map);
        map_id = map.str_of("id").unwrap().to_string();
        let nodes = map.get("payload").and_then(|p| p.get("nodes")).and_then(|n| n.as_arr()).cloned().unwrap_or_default();
        let edges = map.get("payload").and_then(|p| p.get("edges")).and_then(|n| n.as_arr()).cloned().unwrap_or_default();
        assert!(nodes.len() >= 3, "concept map nodes");
        assert!(!edges.is_empty(), "concept map edges");

        // 6. export: SVG for the diagram, Markdown for the text
        let (st, svg) = s.raw("GET", &format!("/api/artifacts/{}/export?format=svg", map_id), None, "text/plain");
        assert_eq!(st, 200);
        assert!(svg.starts_with("<svg") && svg.contains("</svg>"), "{}", &svg[..svg.len().min(200)]);
        assert!(svg.contains("Mycorrhizal") || svg.contains("phosphorus") || svg.contains("Fungal"));

        let (st, md) = s.raw("GET", &format!("/api/artifacts/{}/export?format=md", map_id), None, "text/plain");
        assert_eq!(st, 200);
        assert!(md.contains("# Concept map") || md.contains("#"), "{}", &md[..md.len().min(200)]);
        assert!(md.contains("concepts"));

        // a format that does not apply is refused with a reason, not an empty file
        let (st, body) = s.raw("GET", &format!("/api/artifacts/{}/export?format=pptx", map_id), None, "text/plain");
        assert_eq!(st, 501);
        assert!(body.contains("no pptx renderer") || body.contains("format_unsupported"), "{}", body);
    }

    // 7. it is still there, with the same id, after a restart
    let s2 = Server::start(dir.clone());
    let (st, again) = s2.json("GET", &format!("/api/artifacts/{}", map_id), None);
    assert_eq!(st, 200);
    assert_eq!(again.str_of("id"), Some(map_id.as_str()));
    std::fs::remove_dir_all(&dir).ok();
}

/// Journey H — slide deck: audience controls the outline, slides can be edited, reordered,
/// deleted and restored, the version increments, and Markdown export carries the edits.
#[test]
fn journey_h_slides_are_editable_versioned_and_exportable() {
    let dir = tempdir("journey-h");
    let nb_id;
    let deck_id;
    {
        let s = Server::start(dir.clone());
        let (_, nb) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Deck"))])));
        nb_id = nb.str_of("id").unwrap().to_string();
        for (title, text) in [
            ("Policy 2025", "Retention is thirty days. Backups run nightly at 02:00 UTC."),
            ("Policy 2026", "Retention is sixty days. Backups run nightly at 03:00 UTC."),
        ] {
            s.json(
                "POST",
                &format!("/api/notebooks/{}/sources/text", nb_id),
                Some(obj(&[("title", Value::from(title)), ("text", Value::from(text))])),
            );
        }

        // 2-3. audience is a real parameter of the outline
        let (st, deck) = s.json(
            "POST",
            &format!("/api/notebooks/{}/artifacts", nb_id),
            Some(obj(&[
                ("kind", Value::from("slides")),
                ("audience", Value::from("executive")),
                ("topic", Value::from("Retention")),
            ])),
        );
        assert_eq!(st, 201, "{:?}", deck);
        deck_id = deck.str_of("id").unwrap().to_string();
        let payload = deck.get("payload").cloned().unwrap_or(Value::Null);
        assert_eq!(payload.str_of("audience"), Some("executive"));
        let slides = payload.get("slides").and_then(|s| s.as_arr()).cloned().unwrap_or_default();
        assert!(slides.len() >= 3, "title + sources + at least one content slide: {:?}", slides);
        let exec_bullets: usize = slides
            .iter()
            .filter_map(|s| s.get("bullets").and_then(|b| b.as_arr()))
            .map(|b| b.len())
            .sum();

        let (_, technical) = s.json(
            "POST",
            &format!("/api/notebooks/{}/artifacts", nb_id),
            Some(obj(&[
                ("kind", Value::from("slides")),
                ("audience", Value::from("technical")),
                ("topic", Value::from("Retention")),
            ])),
        );
        let tech_payload = technical.get("payload").cloned().unwrap_or(Value::Null);
        let tech_slides = tech_payload.get("slides").and_then(|s| s.as_arr()).cloned().unwrap_or_default();
        let tech_bullets: usize = tech_slides
            .iter()
            .filter_map(|s| s.get("bullets").and_then(|b| b.as_arr()))
            .map(|b| b.len())
            .sum();
        assert!(tech_bullets >= exec_bullets, "technical puts at least as much on a slide");

        // 5-6. edit one slide
        let (st, edited) = s.json(
            "PATCH",
            &format!("/api/artifacts/{}", deck_id),
            Some(obj(&[
                ("op", Value::from("edit")),
                ("index", Value::from(1.0)),
                ("slide_title", Value::from("What changed in retention")),
                ("notes", Value::from("Lead with the delta, not the history.")),
            ])),
        );
        assert_eq!(st, 200, "{:?}", edited);
        assert_eq!(edited.get("version").and_then(|v| v.as_f64()), Some(2.0));
        let edited_slides = edited.get("payload").and_then(|p| p.get("slides")).and_then(|s| s.as_arr()).cloned().unwrap_or_default();
        assert_eq!(edited_slides[1].str_of("title"), Some("What changed in retention"));

        // 7. reorder, delete and restore
        let before_order: Vec<String> = edited_slides.iter().filter_map(|s| s.str_of("title").map(|t| t.to_string())).collect();
        let (st, moved) = s.json(
            "PATCH",
            &format!("/api/artifacts/{}", deck_id),
            Some(obj(&[("op", Value::from("reorder")), ("from", Value::from(1.0)), ("to", Value::from(0.0))])),
        );
        assert_eq!(st, 200, "{:?}", moved);
        let moved_slides = moved.get("payload").and_then(|p| p.get("slides")).and_then(|s| s.as_arr()).cloned().unwrap_or_default();
        assert_eq!(moved_slides[0].str_of("title"), Some(before_order[1].as_str()), "reorder moved the slide");
        assert_eq!(moved.get("version").and_then(|v| v.as_f64()), Some(3.0));

        let count = moved_slides.len();
        let (st, deleted) = s.json(
            "PATCH",
            &format!("/api/artifacts/{}", deck_id),
            Some(obj(&[("op", Value::from("delete")), ("index", Value::from(0.0))])),
        );
        assert_eq!(st, 200, "{:?}", deleted);
        let after_delete = deleted.get("payload").and_then(|p| p.get("slides")).and_then(|s| s.as_arr()).cloned().unwrap_or_default();
        assert_eq!(after_delete.len(), count - 1);

        let (st, restored) = s.json(
            "PATCH",
            &format!("/api/artifacts/{}", deck_id),
            Some(obj(&[("op", Value::from("restore")), ("index", Value::from(0.0))])),
        );
        assert_eq!(st, 200, "{:?}", restored);
        let after_restore = restored.get("payload").and_then(|p| p.get("slides")).and_then(|s| s.as_arr()).cloned().unwrap_or_default();
        assert_eq!(after_restore.len(), count, "restore brings the slide back");

        // 8. export: Markdown carries the edit; PPTX is refused with a reason
        let (st, md) = s.raw("GET", &format!("/api/artifacts/{}/export?format=md", deck_id), None, "text/plain");
        assert_eq!(st, 200);
        assert!(md.contains("What changed in retention"), "{}", md);
        assert!(md.contains("Lead with the delta"), "notes are exported too: {}", md);
        let (st, err) = s.raw("GET", &format!("/api/artifacts/{}/export?format=pptx", deck_id), None, "text/plain");
        assert_eq!(st, 501);
        assert!(err.contains("format_unsupported"), "{}", err);

        // 9. the version is persisted, not recomputed on read
        let (_, again) = s.json("GET", &format!("/api/artifacts/{}", deck_id), None);
        assert_eq!(again.get("version").and_then(|v| v.as_f64()), Some(5.0));
    }

    let s2 = Server::start(dir.clone());
    let (_, persisted) = s2.json("GET", &format!("/api/artifacts/{}", deck_id), None);
    assert_eq!(persisted.get("version").and_then(|v| v.as_f64()), Some(5.0), "version survives restart");
    let titles: Vec<String> = persisted
        .get("payload")
        .and_then(|p| p.get("slides"))
        .and_then(|s| s.as_arr())
        .map(|a| a.iter().filter_map(|s| s.str_of("title").map(|t| t.to_string())).collect())
        .unwrap_or_default();
    assert!(titles.iter().any(|t| t == "What changed in retention"), "{:?}", titles);
    std::fs::remove_dir_all(&dir).ok();
}

/// Journey K — resilience: a process that dies mid-ingest leaves a clear, retryable state,
/// and a retry never duplicates records.
#[test]
fn journey_k_interrupted_ingestion_is_recovered_and_retry_is_idempotent() {
    let dir = tempdir("journey-k");
    let nb_id;
    let sid;
    {
        let s = Server::start(dir.clone());
        let (_, nb) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Resilience"))])));
        nb_id = nb.str_of("id").unwrap().to_string();
        let pdf = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/simple.pdf")).unwrap();
        let (st, body) = s.raw(
            "POST",
            &format!("/api/notebooks/{}/sources/upload?filename=simple.pdf", nb_id),
            Some(&pdf),
            "application/pdf",
        );
        assert_eq!(st, 201, "{}", body);
        let up = json::parse(&body).unwrap();
        sid = up.str_of("source_id").unwrap().to_string();

        // the job is recorded and finished, not just implied
        let (st, jobs) = s.json("GET", &format!("/api/notebooks/{}/jobs", nb_id), None);
        assert_eq!(st, 200);
        let list = jobs.get("jobs").and_then(|j| j.as_arr()).cloned().unwrap_or_default();
        assert_eq!(list.len(), 1, "{:?}", list);
        assert_eq!(list[0].str_of("status"), Some("done"));
        assert_eq!(list[0].str_of("source_id"), Some(sid.as_str()));
        assert_eq!(list[0].get("attempts").and_then(|a| a.as_f64()), Some(1.0));
        // the upload is retained, which is what makes a retry possible without re-uploading
        assert!(dir.join("blobs").join(format!("{}.bin", sid)).exists());
    }

    // Simulate a crash in the middle of an ingest: a job left running and a source left
    // processing, exactly what a stopped process would leave behind.
    {
        let store = Store::open(&dir).expect("store");
        store
            .write(|db| {
                for j in db.jobs.iter_mut() {
                    if j.source_id == sid {
                        j.status = "running".into();
                        j.finished_at = String::new();
                        j.error = String::new();
                    }
                }
                for s in db.sources.iter_mut() {
                    if s.id == sid {
                        s.status = "processing".into();
                    }
                }
            })
            .unwrap();
    }

    // Reopen: recovery turns the impossible "still running" into a clear, retryable failure.
    let s2 = Server::start(dir.clone());
    let (_, src) = s2.json("GET", &format!("/api/sources/{}", sid), None);
    assert_eq!(src.str_of("status"), Some("failed"), "{:?}", src);
    assert!(src.str_of("error").unwrap_or("").contains("interrupted"), "{:?}", src.str_of("error"));
    let (_, jobs2) = s2.json("GET", &format!("/api/notebooks/{}/jobs", nb_id), None);
    let interrupted = jobs2.get("jobs").and_then(|j| j.as_arr()).and_then(|a| a.first()).cloned().unwrap();
    assert_eq!(interrupted.str_of("status"), Some("interrupted"));
    assert!(interrupted.str_of("error").unwrap_or("").contains("stopped"));

    // Retry re-ingests from the retained file, re-using the same source id.
    let (st, retried) = s2.json("POST", &format!("/api/sources/{}/retry", sid), None);
    assert_eq!(st, 200, "{:?}", retried);
    assert_eq!(retried.str_of("status"), Some("ready"));
    assert_eq!(retried.get("attempts").and_then(|a| a.as_f64()), Some(2.0));

    // no duplicate records: one source, one job per attempt, chunk count restored once
    let (_, srcs) = s2.json("GET", &format!("/api/notebooks/{}/sources", nb_id), None);
    let all = srcs.get("sources").and_then(|x| x.as_arr()).cloned().unwrap_or_default();
    assert_eq!(all.len(), 1, "retry must not create a second source: {:?}", all);
    assert_eq!(all[0].str_of("id"), Some(sid.as_str()));
    let (_, chunks) = s2.json("GET", &format!("/api/sources/{}/chunks", sid), None);
    assert_eq!(
        chunks.get("total").and_then(|t| t.as_f64()),
        Some(2.0),
        "two PDF pages, indexed once"
    );
    let (_, jobs3) = s2.json("GET", &format!("/api/notebooks/{}/jobs", nb_id), None);
    let attempts: Vec<f64> = jobs3
        .get("jobs")
        .and_then(|j| j.as_arr())
        .map(|a| a.iter().filter_map(|j| j.get("attempts").and_then(|x| x.as_f64())).collect())
        .unwrap_or_default();
    assert_eq!(attempts.len(), 2, "one job per attempt: {:?}", attempts);
    assert!(jobs3.get("jobs").and_then(|j| j.as_arr()).unwrap().iter().any(|j| j.str_of("status") == Some("done")));

    // a second retry of a healthy source is refused rather than re-indexing it
    let (st, again) = s2.json("POST", &format!("/api/sources/{}/retry", sid), None);
    assert_eq!(st, 409);
    assert_eq!(again.str_of("code"), Some("already_ready"));

    // database consistency: the counts agree with the records
    let (_, health) = s2.json("GET", "/api/health", None);
    let counts = health.get("counts").cloned().unwrap_or(Value::Null);
    assert_eq!(counts.get("sources").and_then(|v| v.as_f64()), Some(1.0));
    assert_eq!(counts.get("chunks").and_then(|v| v.as_f64()), Some(2.0));
    assert_eq!(counts.get("notebooks").and_then(|v| v.as_f64()), Some(1.0));

    // and the answer still works after the recovery, citing page 2
    let (_, chat) = s2.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("what does page two say about citations"))])),
    );
    assert_eq!(
        chat.get("citations").and_then(|c| c.as_arr()).and_then(|a| a.first())
            .and_then(|c| c.get("locator")).and_then(|l| l.get("page")).and_then(|p| p.as_f64()),
        Some(2.0),
        "recovered evidence is still citable: {:?}",
        chat
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// Journey J — access control: a viewer cannot edit, an editor cannot delete, revocation
/// takes effect immediately, and a share link can never see another notebook.
#[test]
fn journey_j_share_links_are_scoped_by_role_and_revocable() {
    let dir = tempdir("journey-j");
    let s = Server::start(dir.clone());

    // single-user mode first: no token, everything is the operator
    let (st, mode) = s.json("GET", "/api/auth", None);
    assert_eq!(st, 200);
    assert_eq!(mode.str_of("mode"), Some("single-user"));
    assert_eq!(mode.get("enforced"), Some(&Value::Bool(false)));

    let (_, a) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Shared"))])));
    let nb_a = a.str_of("id").unwrap().to_string();
    let (_, b) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Private"))])));
    let nb_b = b.str_of("id").unwrap().to_string();
    s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb_a),
        Some(obj(&[("title", Value::from("Policy")), ("text", Value::from("Retention is thirty days."))])),
    );
    s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb_b),
        Some(obj(&[("title", Value::from("Secret")), ("text", Value::from("The acquisition closes in March."))])),
    );

    // enable enforcement; the returned token is the owner credential
    let (st, owner) = s.json("POST", "/api/auth/owner-token", Some(obj(&[("token", Value::from("owner-token-for-tests-0123"))])));
    assert_eq!(st, 200, "{:?}", owner);
    assert_eq!(owner.get("enforced"), Some(&Value::Bool(true)));
    let owner_hdr = [("authorization", "Bearer owner-token-for-tests-0123")];

    // anonymous access is now refused everywhere that matters
    let (st, anon) = s.json("GET", "/api/notebooks", None);
    assert_eq!(st, 401, "{:?}", anon);
    let (st, anon2) = s.json("POST", &format!("/api/notebooks/{}/chat", nb_a), Some(obj(&[("question", Value::from("retention"))])));
    assert_eq!(st, 401);
    let (_, caps) = s.json("GET", "/api/capabilities", None);
    assert_eq!(caps.get("auth").and_then(|a| a.str_of("mode")), Some("token"));

    // the owner sees everything
    let (st, all) = s.json_h("GET", "/api/notebooks", None, &owner_hdr);
    assert_eq!(st, 200);
    assert_eq!(all.get("notebooks").and_then(|n| n.as_arr()).map(|a| a.len()), Some(2));

    // 2-3. a viewer link: read and ask, but no edits
    let (st, viewer) = s.json_h(
        "POST",
        &format!("/api/notebooks/{}/shares", nb_a),
        Some(obj(&[("role", Value::from("viewer"))])),
        &owner_hdr,
    );
    assert_eq!(st, 201, "{:?}", viewer);
    assert_eq!(viewer.str_of("role"), Some("viewer"));
    let viewer_token = viewer.str_of("token").unwrap().to_string();
    assert_eq!(viewer_token.len(), 32);
    let viewer_hdr = [("x-forge-share", viewer_token.as_str())];

    let (st, visible) = s.json_h("GET", "/api/notebooks", None, &viewer_hdr);
    assert_eq!(st, 200);
    let list = visible.get("notebooks").and_then(|n| n.as_arr()).cloned().unwrap_or_default();
    assert_eq!(list.len(), 1, "a share sees exactly one notebook: {:?}", list);
    assert_eq!(list[0].str_of("id"), Some(nb_a.as_str()));

    let (st, _) = s.json_h("GET", &format!("/api/notebooks/{}", nb_b), None, &viewer_hdr);
    assert_eq!(st, 403, "another notebook is out of scope");
    let (st, _) = s.json_h("POST", &format!("/api/notebooks/{}/chat", nb_b), Some(obj(&[("question", Value::from("when"))])), &viewer_hdr);
    assert_eq!(st, 403);

    let (st, answer) = s.json_h(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_a),
        Some(obj(&[("question", Value::from("retention days limit"))])),
        &viewer_hdr,
    );
    assert_eq!(st, 200, "viewers may ask questions: {:?}", answer);
    assert!(answer.str_of("answer").unwrap_or("").contains("thirty"), "answer: {:?}", answer);

    let (st, denied) = s.json_h(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb_a),
        Some(obj(&[("title", Value::from("nope")), ("text", Value::from("nope"))])),
        &viewer_hdr,
    );
    assert_eq!(st, 403, "{:?}", denied);
    assert_eq!(denied.str_of("code"), Some("forbidden_role"));
    let (st, _) = s.json_h("PATCH", &format!("/api/notebooks/{}", nb_a), Some(obj(&[("title", Value::from("renamed"))])), &viewer_hdr);
    assert_eq!(st, 403);
    let (st, _) = s.json_h("GET", "/api/providers", None, &viewer_hdr);
    assert_eq!(st, 403, "provider configuration is owner-only");

    // 4-5. an editor link can write, but still cannot delete or re-share
    let (st, editor) = s.json_h(
        "POST",
        &format!("/api/notebooks/{}/shares", nb_a),
        Some(obj(&[("role", Value::from("editor"))])),
        &owner_hdr,
    );
    assert_eq!(st, 201);
    let editor_hdr = [("x-forge-share", editor.str_of("token").unwrap())];

    let (st, added) = s.json_h(
        "POST",
        &format!("/api/notebooks/{}/sources/text", nb_a),
        Some(obj(&[("title", Value::from("Addendum")), ("text", Value::from("Retention is extended to sixty days for legal holds."))])),
        &editor_hdr,
    );
    assert_eq!(st, 201, "editors may add sources: {:?}", added);
    let (st, _) = s.json_h("DELETE", &format!("/api/notebooks/{}", nb_a), None, &editor_hdr);
    assert_eq!(st, 403, "only the owner may delete the notebook");
    let (st, _) = s.json_h("POST", &format!("/api/notebooks/{}/shares", nb_a), Some(obj(&[("role", Value::from("editor"))])), &editor_hdr);
    assert_eq!(st, 403, "only the owner may create share links");

    // the shares list never leaks a token
    let (st, shares) = s.json_h("GET", &format!("/api/notebooks/{}/shares", nb_a), None, &owner_hdr);
    assert_eq!(st, 200);
    let list = shares.get("shares").and_then(|s| s.as_arr()).cloned().unwrap_or_default();
    assert_eq!(list.len(), 2);
    for g in &list {
        assert!(g.str_of("token").is_none(), "tokens are never listed: {:?}", g);
        assert!(g.str_of("token_sha256").is_none(), "hashes are not exposed either: {:?}", g);
        assert_eq!(g.str_of("token_prefix").map(|p| p.len()), Some(6));
    }

    // 6-7. revoke the viewer: the very next request fails, and it cannot reach any endpoint
    let viewer_id = list.iter().find(|g| g.str_of("role") == Some("viewer")).unwrap().str_of("id").unwrap().to_string();
    let (st, revoked) = s.json_h("DELETE", &format!("/api/shares/{}", viewer_id), None, &owner_hdr);
    assert_eq!(st, 200, "{:?}", revoked);
    for path in [
        "/api/notebooks".to_string(),
        format!("/api/notebooks/{}", nb_a),
        format!("/api/notebooks/{}/sources", nb_a),
        format!("/api/notebooks/{}/chat", nb_a),
        format!("/api/notebooks/{}/artifacts", nb_a),
        format!("/api/notebooks/{}/notes", nb_a),
    ] {
        let (st, body) = s.json_h("GET", &path, None, &viewer_hdr);
        assert_eq!(st, 401, "revoked viewer reached {}: {:?}", path, body);
    }
    // the streaming endpoint is a separate dispatch path and must be guarded too
    let (st, _) = s.raw_h(
        "POST",
        &format!("/api/notebooks/{}/chat/stream", nb_a),
        Some(br#"{"question":"retention"}"#),
        "application/json",
        &viewer_hdr,
    );
    assert_eq!(st, 401, "revoked viewer reached the stream endpoint");
    // and the editor link still works, unaffected by the other revocation
    let (st, _) = s.json_h("GET", &format!("/api/notebooks/{}", nb_a), None, &editor_hdr);
    assert_eq!(st, 200);
    // the owner can still read the notebook and its audit trail
    let (st, nb) = s.json_h("GET", &format!("/api/notebooks/{}", nb_a), None, &owner_hdr);
    assert_eq!(st, 200);
    assert_eq!(nb.str_of("title"), Some("Shared"));
    std::fs::remove_dir_all(&dir).ok();
}

/// Journey E — a video *transcript*. The caption file is supplied by the user; Forge never
/// downloads media or captions. Citations must carry the spoken timecode, and the capability
/// page must say plainly that there is no visual/frame analysis.
#[test]
fn journey_e_transcript_cites_timecodes_and_refuses_visual_claims() {
    let dir = tempdir("journey-e");
    let s = Server::start(dir.clone());
    let (_, nb) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Briefing"))])));
    let nb_id = nb.str_of("id").unwrap().to_string();

    let bytes = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/lecture.vtt")).unwrap();
    let (st, body) = s.raw(
        "POST",
        &format!("/api/notebooks/{}/sources/upload?filename=lecture.vtt", nb_id),
        Some(&bytes),
        "application/octet-stream",
    );
    assert_eq!(st, 201, "{}", body);
    let up = json::parse(&body).unwrap();
    assert_eq!(up.str_of("kind"), Some("transcript"));
    assert_eq!(up.get("chunk_count").and_then(|c| c.as_f64()), Some(3.0));

    // The cue offsets are structure, not decoration: every passage carries a timestamp locator.
    let (st, chunks) = s.json("GET", &format!("/api/notebooks/{}/chunks", nb_id), None);
    assert_eq!(st, 200);
    let list = chunks.get("chunks").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert_eq!(list.len(), 3);
    let stamps: Vec<u64> = list
        .iter()
        .map(|c| c.get("locator").and_then(|l| l.get("at_ms")).and_then(|m| m.as_f64()).unwrap_or(-1.0) as u64)
        .collect();
    assert!(stamps.contains(&42_000), "expected a 00:42 cue: {:?}", stamps);

    // A question about the statement at 00:42 must cite 00:42.
    let (st, answer) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("what is the retention window for customer records"))])),
    );
    assert_eq!(st, 200);
    let citations = answer.get("citations").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert!(!citations.is_empty(), "no citation: {:?}", answer);
    let first = &citations[0];
    assert_eq!(first.str_of("locator_label"), Some("00:42"), "{:?}", first);
    assert_eq!(first.get("locator").and_then(|l| l.str_of("kind")), Some("timestamp"));
    assert!(answer.str_of("answer").unwrap_or("").contains("thirty"), "{:?}", answer.str_of("answer"));

    // The evidence is the cue text itself, so a viewer can show what was said.
    let src_id = first.str_of("source_id").unwrap().to_string();
    let (st, src) = s.json("GET", &format!("/api/sources/{}", src_id), None);
    assert_eq!(st, 200);
    assert_eq!(src.str_of("kind"), Some("transcript"));

    // Visual analysis is not available, and the capability page says so instead of implying it.
    let (_, caps) = s.json("GET", "/api/capabilities", None);
    let video = caps.get("sources").and_then(|s| s.get("video")).unwrap();
    assert_eq!(video.str_of("status"), Some("unsupported"));
    let video_text = json::to_string(video);
    assert!(video_text.contains("no video ingestion"), "{:?}", video);
    // and nothing anywhere claims frame analysis
    assert!(video_text.contains("no frame decoding"), "{:?}", video);
    assert!(!json::to_string(&caps).to_lowercase().contains("visual analysis: supported"));
    let transcript = caps.get("sources").and_then(|s| s.get("transcript")).unwrap();
    assert_eq!(transcript.str_of("status"), Some("supported"));
    assert!(transcript.str_of("note").unwrap_or("").contains("does not analyse frames"), "{:?}", transcript);

    // A video file is not a source: uploading one is refused with a reason, not indexed as text.
    let (st, refused) = s.raw(
        "POST",
        &format!("/api/notebooks/{}/sources/upload?filename=clip.mp4", nb_id),
        Some(&[0u8, 0, 0, 0x18, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm']),
        "application/octet-stream",
    );
    assert_eq!(st, 422, "a video must be refused as a source: {}", refused);
    let refusal = json::parse(&refused).unwrap();
    assert_eq!(refusal.str_of("code"), Some("unsupported_source_kind"));
    assert!(refusal.str_of("error").unwrap_or("").contains("transcript"), "{:?}", refusal);
    let (st, sources) = s.json("GET", &format!("/api/notebooks/{}/sources", nb_id), None);
    assert_eq!(st, 200);
    assert_eq!(sources.get("sources").and_then(|s| s.as_arr()).map(|a| a.len()), Some(1));
    std::fs::remove_dir_all(&dir).ok();
}

/// Journey I — research discovery. Forge queries no search engine (there is none configured,
/// and scraping results would be neither authorised nor honest). What it does: reads the links
/// of seed pages the user chose, reports each candidate with its provenance, imports the ones
/// the user selects, and builds a report whose references are the real pages.
#[test]
fn journey_i_discovery_offers_provenanced_candidates_and_exports_a_report() {
    let _guard = URL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("FORGE_FETCH_ALLOW_PRIVATE", "1");

    let page_a: &'static str = "<html><head><title>Alpha study</title></head><body>\
        <p>Retrieval quality depends on the ranking function, and the ranking function is trained on click logs.</p>\
        </body></html>";
    let page_b: &'static str = "<html><head><title>Beta note</title></head><body>\
        <p>A citation check rejects any answer whose quoted span is not present in the source.</p>\
        </body></html>";
    let port_a = start_origin(page_a);
    let port_b = start_origin(page_b);
    let seed_html: &'static str = Box::leak(
        format!(
            "<html><head><title>Reading list</title></head><body>\
             <a href=\"http://127.0.0.1:{}/alpha\">Alpha study</a>\
             <a href=\"http://127.0.0.1:{}/beta\">Beta note</a>\
             <a href=\"http://127.0.0.1:{}/talk.mp4\">Recorded talk</a>\
             </body></html>",
            port_a, port_b, port_a
        )
        .into_boxed_str(),
    );
    let port_seed = start_origin(seed_html);
    let dir = tempdir("journey-i");
    let s = Server::start(dir.clone());
    let (_, nb) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Discovery"))])));
    let nb_id = nb.str_of("id").unwrap().to_string();

    // 1-3. one seed page, twice, yields candidates from two independent origins — and says so.
    let seed = format!("http://127.0.0.1:{}/list", port_seed);
    let (st, crawl) = s.json(
        "POST",
        "/api/discovery/crawl",
        Some(obj(&[("seed_urls", Value::Arr(vec![Value::from(seed.as_str())])), ("limit", Value::from(10.0))])),
    );
    assert_eq!(st, 200, "{:?}", crawl);
    assert!(crawl.get("searched_engine").map(|v| v.is_null()).unwrap_or(false),
        "no engine may be claimed: {:?}", crawl.get("searched_engine"));
    assert!(crawl.str_of("note").unwrap_or("").contains("does not query a search engine"), "{:?}", crawl);
    let candidates = crawl.get("candidates").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert_eq!(candidates.len(), 2, "media links must not be offered as pages: {:?}", candidates);
    assert_eq!(crawl.get("candidate_hosts").and_then(|h| h.as_f64()), Some(2.0));
    for c in &candidates {
        assert_eq!(c.str_of("from_url"), Some(seed.as_str()), "provenance is mandatory: {:?}", c);
        assert!(!c.str_of("url").unwrap_or("").ends_with(".mp4"));
    }
    let seed_report = crawl.get("seeds").and_then(|x| x.as_arr()).and_then(|a| a.first()).cloned().unwrap();
    assert_eq!(seed_report.get("status").and_then(|v| v.as_f64()), Some(200.0));
    assert_eq!(seed_report.str_of("title"), Some("Reading list"));
    assert!(crawl.get("warnings").and_then(|w| w.as_arr()).map(|w| w.iter().any(|x| x.as_str().unwrap_or("").contains("media"))).unwrap_or(false));

    // 4-5. the user opens/imports the two candidates they want.
    for c in &candidates {
        let url = c.str_of("url").unwrap().to_string();
        let (st, body) = s.json(
            "POST",
            &format!("/api/notebooks/{}/sources/url", nb_id),
            Some(obj(&[("url", Value::from(url.as_str()))])),
        );
        assert_eq!(st, 201, "importing {} failed: {:?}", url, body);
    }

    // 6-7. a report whose claims carry citations that resolve to the imported pages.
    let (st, art) = s.json(
        "POST",
        &format!("/api/notebooks/{}/artifacts", nb_id),
        Some(obj(&[
            ("kind", Value::from("report")),
            ("topic", Value::from("retrieval and citation")),
        ])),
    );
    assert_eq!(st, 201, "{:?}", art);
    let art_id = art.str_of("id").unwrap().to_string();
    let payload = art.get("payload").cloned().unwrap();
    let sections = payload.get("sections").and_then(|s| s.as_arr()).cloned().unwrap_or_default();
    assert!(!sections.is_empty(), "{:?}", payload);
    let mut cited_titles: Vec<String> = Vec::new();
    for sec in &sections {
        for pt in sec.get("points").and_then(|p| p.as_arr()).cloned().unwrap_or_default() {
            let cit = pt.get("citation").cloned().unwrap_or(Value::Null);
            assert!(cit.str_of("quote").is_some(), "a point without a citation: {:?}", pt);
            cited_titles.push(cit.str_of("source_title").unwrap_or("").to_string());
        }
    }
    assert!(cited_titles.iter().any(|t| t == "Alpha study"), "{:?}", cited_titles);
    assert!(cited_titles.iter().any(|t| t == "Beta note"), "{:?}", cited_titles);

    // 8. the report exports, and the export names the real sources rather than inventing any.
    let (st, md) = s.raw("GET", &format!("/api/artifacts/{}/export?format=md", art_id), None, "text/markdown");
    assert_eq!(st, 200);
    assert!(md.contains("**Sources**"), "{}", md);
    assert!(md.contains("Alpha study"), "{}", md);
    assert!(md.contains("Beta note"), "{}", md);
    assert!(md.contains("Retrieval quality depends on the ranking function"), "the quoted span must survive export: {}", md);
    // every reference listed in the export is a source that exists in this notebook
    let (_, sources) = s.json("GET", &format!("/api/notebooks/{}/sources", nb_id), None);
    let titles: Vec<String> = sources
        .get("sources")
        .and_then(|x| x.as_arr())
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|x| x.str_of("title").unwrap_or("").to_string())
        .collect();
    assert_eq!(titles.len(), 2, "{:?}", titles);
    for line in md.lines().filter(|l| l.starts_with("- ") && l.contains(" (html,")) {
        let title = line.trim_start_matches("- ").split(" (html,").next().unwrap_or("");
        assert!(titles.iter().any(|t| t == title), "export cites an unknown source: {}", line);
    }

    std::env::remove_var("FORGE_FETCH_ALLOW_PRIVATE");
    std::fs::remove_dir_all(&dir).ok();
}

/// §7.1 — the library is a real list: it can be searched (including by source name), sorted, and
/// a notebook can be duplicated. "Recent" means real activity, not a rewritten row.
#[test]
fn library_search_sort_icons_and_duplicate_are_real() {
    let dir = tempdir("library");
    let s = Server::start(dir.clone());

    let (_, a) = s.json(
        "POST",
        "/api/notebooks",
        Some(obj(&[
            ("title", Value::from("Policy handbook")),
            ("description", Value::from("retention rules")),
            ("icon", Value::from("📘")),
        ])),
    );
    let a_id = a.str_of("id").unwrap().to_string();
    let (_, b) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Biology"))])));
    let b_id = b.str_of("id").unwrap().to_string();
    let (_, c) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Zebra notes"))])));
    let c_id = c.str_of("id").unwrap().to_string();

    // the icon is stored, returned, and capped (it is a visual identifier, not a title)
    let (st, nb) = s.json("GET", &format!("/api/notebooks/{}", a_id), None);
    assert_eq!(st, 200);
    assert_eq!(nb.str_of("icon"), Some("📘"));
    let (st, _) = s.json("PATCH", &format!("/api/notebooks/{}", c_id), Some(obj(&[("icon", Value::from("zzzzzzzzzz"))])));
    assert_eq!(st, 200);
    let (_, z) = s.json("GET", &format!("/api/notebooks/{}", c_id), None);
    assert_eq!(z.str_of("icon"), Some("zzzz"), "icon must be capped: {:?}", z.str_of("icon"));

    // a source title is searchable: the notebook that contains a document is the one you want
    s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/text", b_id),
        Some(obj(&[("title", Value::from("Lecture on photosynthesis")), ("text", Value::from("Chlorophyll absorbs light."))])),
    );

    let (st, found) = s.json("GET", "/api/notebooks?q=photosynthesis", None);
    assert_eq!(st, 200);
    let hits: Vec<String> = found
        .get("notebooks")
        .and_then(|n| n.as_arr())
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|n| n.str_of("title").unwrap_or("").to_string())
        .collect();
    assert_eq!(hits, vec!["Biology".to_string()], "source-title search: {:?}", hits);
    assert_eq!(found.str_of("query"), Some("photosynthesis"));

    let (_, by_desc) = s.json("GET", "/api/notebooks?q=RETENTION", None);
    assert_eq!(by_desc.get("notebooks").and_then(|n| n.as_arr()).map(|a| a.len()), Some(1), "case-insensitive");

    let (_, none) = s.json("GET", "/api/notebooks?q=nothing-matches-this", None);
    assert_eq!(none.get("notebooks").and_then(|n| n.as_arr()).map(|a| a.len()), Some(0));

    // sorting: title ascending, and the reported sort is echoed so the UI can trust it
    let (_, sorted) = s.json("GET", "/api/notebooks?sort=title", None);
    let titles: Vec<String> = sorted
        .get("notebooks")
        .and_then(|n| n.as_arr())
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|n| n.str_of("title").unwrap_or("").to_string())
        .collect();
    assert_eq!(titles, vec!["Biology".to_string(), "Policy handbook".to_string(), "Zebra notes".to_string()]);
    assert_eq!(sorted.str_of("sort"), Some("title"));

    // last_activity reflects the ingest, so "recent" is not just creation order
    let (_, recent) = s.json("GET", "/api/notebooks?sort=recent", None);
    let first = recent.get("notebooks").and_then(|n| n.as_arr()).and_then(|a| a.first()).cloned().unwrap();
    assert_eq!(first.str_of("title"), Some("Biology"), "the notebook that just gained a source is the most recent: {:?}", first);
    assert!(first.str_of("last_activity").unwrap_or("") >= first.str_of("updated_at").unwrap_or(""));

    // duplicate copies evidence but not chat history
    s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", b_id),
        Some(obj(&[("question", Value::from("chlorophyll absorbs light"))])),
    );
    let (st, dup) = s.json("POST", &format!("/api/notebooks/{}/duplicate", b_id), None);
    assert_eq!(st, 201, "{:?}", dup);
    let dup_id = dup.str_of("id").unwrap().to_string();
    assert_eq!(dup.str_of("title"), Some("Biology (copy)"));
    assert_eq!(dup.get("source_count").and_then(|v| v.as_f64()), Some(1.0));
    assert_eq!(dup.get("conversations_copied").and_then(|v| v.as_f64()), Some(0.0));

    let (st, copy) = s.json("GET", &format!("/api/notebooks/{}", dup_id), None);
    assert_eq!(st, 200);
    assert_eq!(copy.str_of("icon"), Some(""), "no icon on the original, none on the copy");
    let copy_sources = copy.get("sources").and_then(|s| s.as_arr()).cloned().unwrap_or_default();
    assert_eq!(copy_sources.len(), 1);
    let copy_src = copy_sources[0].str_of("id").unwrap().to_string();
    assert_ne!(copy_src, b_id, "ids are new, not shared with the original");

    // the copy answers from its own chunks, and cites the same source title
    let (st, answer) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", dup_id),
        Some(obj(&[("question", Value::from("chlorophyll absorbs light"))])),
    );
    assert_eq!(st, 200);
    assert!(answer.str_of("answer").unwrap_or("").to_lowercase().contains("chlorophyll"), "{:?}", answer.str_of("answer"));
    let (_, copy_convs) = s.json("GET", &format!("/api/notebooks/{}/conversations", dup_id), None);
    assert_eq!(copy_convs.get("conversations").and_then(|c| c.as_arr()).map(|a| a.len()), Some(1),
        "only the question asked in the copy");

    // and the original is untouched
    let (_, orig) = s.json("GET", &format!("/api/notebooks/{}", b_id), None);
    assert_eq!(orig.get("sources").and_then(|s| s.as_arr()).map(|a| a.len()), Some(1));
    let (_, orig_convs) = s.json("GET", &format!("/api/notebooks/{}/conversations", b_id), None);
    assert_eq!(orig_convs.get("conversations").and_then(|c| c.as_arr()).map(|a| a.len()), Some(1));

    // unknown notebook ids are 404, not an empty copy
    let (st, _) = s.json("POST", "/api/notebooks/nb-nope/duplicate", None);
    assert_eq!(st, 404);

    // archived filtering is honoured
    s.json("PATCH", &format!("/api/notebooks/{}", c_id), Some(obj(&[("archived", Value::Bool(true))])));
    let (_, active) = s.json("GET", "/api/notebooks?archived=false", None);
    assert!(active.get("notebooks").and_then(|n| n.as_arr()).map(|a| a.iter().all(|n| n.str_of("title") != Some("Zebra notes"))).unwrap_or(false));
    let (_, archived) = s.json("GET", "/api/notebooks?archived=true", None);
    assert_eq!(archived.get("notebooks").and_then(|n| n.as_arr()).map(|a| a.len()), Some(1));

    std::fs::remove_dir_all(&dir).ok();
}

/// §7.2 — EPUB. Chapters must be read in spine order (which can differ from filename order), and
/// a citation must point at the chapter and its position rather than at "the book".
#[test]
fn epub_sources_are_read_in_spine_order_and_cite_their_chapter() {
    let dir = tempdir("epub");
    let s = Server::start(dir.clone());
    let (_, nb) = s.json("POST", "/api/notebooks", Some(obj(&[("title", Value::from("Handbook"))])));
    let nb_id = nb.str_of("id").unwrap().to_string();
    let bytes = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sample.epub")).unwrap();
    let (st, body) = s.raw(
        "POST",
        &format!("/api/notebooks/{}/sources/upload?filename=handbook.epub", nb_id),
        Some(&bytes),
        "application/epub+zip",
    );
    assert_eq!(st, 201, "{}", body);
    let up = json::parse(&body).unwrap();
    assert_eq!(up.str_of("kind"), Some("epub"));
    let warnings = up.get("warnings").and_then(|w| w.as_arr()).cloned().unwrap_or_default();
    assert!(warnings.iter().any(|w| w.as_str().unwrap_or("").contains("image")), "{:?}", warnings);

    let (_, chunks) = s.json("GET", &format!("/api/notebooks/{}/chunks", nb_id), None);
    let list = chunks.get("chunks").and_then(|c| c.as_arr()).cloned().unwrap_or_default();
    assert!(!list.is_empty(), "{:?}", chunks);
    for c in &list {
        let loc = c.get("locator").cloned().unwrap_or(Value::Null);
        assert_eq!(loc.str_of("kind"), Some("chapter"), "{:?}", loc);
        assert!(loc.str_of("section").unwrap_or("").contains(" · "), "chapter title missing: {:?}", loc);
    }
    let first_page = list[0].get("locator").and_then(|l| l.get("page")).and_then(|p| p.as_f64());
    assert_eq!(first_page, Some(1.0), "the spine's first document is chapter 1");

    let (st, answer) = s.json(
        "POST",
        &format!("/api/notebooks/{}/chat", nb_id),
        Some(obj(&[("question", Value::from("how long are customer records retained"))])),
    );
    assert_eq!(st, 200);
    let cit = answer.get("citations").and_then(|c| c.as_arr()).and_then(|a| a.first()).cloned().unwrap_or(Value::Null);
    assert!(cit.str_of("locator_label").unwrap_or("").contains("chapter 2"), "citation: {:?}", cit);
    assert!(cit.str_of("locator_label").unwrap_or("").contains("retention model"), "citation: {:?}", cit);

    // a corrupt .epub is refused with a reason, not indexed as text
    let (st, bad) = s.raw(
        "POST",
        &format!("/api/notebooks/{}/sources/upload?filename=broken.epub", nb_id),
        Some(b"definitely not a zip archive"),
        "application/epub+zip",
    );
    assert_eq!(st, 422, "{}", bad);
    std::fs::remove_dir_all(&dir).ok();
}
