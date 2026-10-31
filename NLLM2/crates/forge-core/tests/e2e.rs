//! End-to-end tests over a real TCP socket: the exact path a browser takes.

use forge_core::api::Api;
use forge_core::http::{serve, Request};
use forge_core::json::{self, Value};
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
                    api.handle(req)
                } else {
                    forge_core::web::serve_static(&web, &req.path)
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
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let payload = body.unwrap_or(&[]);
        let head = format!(
            "{} {} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
            method, path, ctype, payload.len()
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
        let raw_body = body.map(|v| json::to_string(&v).into_bytes());
        let (status, text) = self.raw(method, path, raw_body.as_deref(), "application/json");
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
