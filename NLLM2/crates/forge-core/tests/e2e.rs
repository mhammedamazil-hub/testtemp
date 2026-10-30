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

    // public URL: honest 501 instead of pretending it worked
    let (st, body) = s.json(
        "POST",
        &format!("/api/notebooks/{}/sources/url", nb_id),
        Some(obj(&[("url", Value::from("http://93.184.216.34/page"))])),
    );
    assert_eq!(st, 501);
    assert_eq!(body.str_of("code"), Some("url_ingest_unavailable"));

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
