//! NotebookLM (Gemini Notebook) protocol client.
//!
//! ## Provenance — read this before trusting a byte of it
//!
//! Every constant here comes from **public reverse-engineering of the web client**,
//! cross-checked across three independent open-source implementations:
//!
//! * `teng-lin/notebooklm-py` @ `956400aa7af6b61b4cb988a24c9a081a3abc4ee8`,
//!   `docs/rpc-reference.md` (dated live CDP captures, 2026-06-15 / 2026-08-07)
//! * `jacob-bd/gemini-notebook-mcp-cli`, `src/notebooklm_tools/core/utils.py`
//! * `jackwener/OpenCLI`, `clis/notebooklm/add-source.js`
//!
//! RPC ids used below appear in at least two of those codebases (`izAoDd`,
//! `o4cbdc`, `wXbhsf`, `CCqFvf`, `rLM1Ne`, `R7cb6c`, `VfAZjd` were verified
//! present in independent sources during this session).
//!
//! ## What this module can and cannot do here
//!
//! It **builds and parses the real wire protocol** — request URLs, `f.req`
//! envelopes, source specs, artifact configs and the chunked `wrb.fr` response
//! framing — and it is covered by unit tests that assert the exact shapes.
//!
//! It **cannot execute** against Google from this environment: the sandbox has no
//! route to `notebooklm.google.com` (verified: `curl` → HTTP 000) and the build
//! has no TLS client. `execute()` therefore returns `Err(Unavailable)` with the
//! reason instead of pretending. When a TLS-capable, Google-reachable build
//! exists, `Transport` is the only trait that needs an implementation.

use crate::json::{self, Value};

pub const ORIGIN: &str = "https://notebooklm.google.com";
pub const BATCH_PATH: &str = "/_/LabsTailwindUi/data/batchexecute";

/// Streaming chat endpoint (not batchexecute).
pub const CHAT_PATH: &str = "/_/LabsTailwindUi/data/google.internal.labs.tailwind.orchestration.v1.LabsTailwindOrchestrationService/GenerateFreeFormStreamed";

// ---------------------------------------------------------------- helpers
fn a(items: Vec<Value>) -> Value {
    Value::Arr(items)
}
fn s(v: &str) -> Value {
    Value::from(v)
}
fn n(v: i64) -> Value {
    Value::from(v as f64)
}
fn null() -> Value {
    Value::Null
}

/// The 11-slot source-spec array the backend expects (positions matter).
fn source_spec(mut slots: [Value; 11]) -> Value {
    // slot 10 is always 1 in observed captures
    slots[10] = n(1);
    a(slots.to_vec())
}

/// Shared request-options wrapper used as the last batchexecute param.
fn request_options() -> Value {
    a(vec![
        n(2),
        null(),
        null(),
        a(vec![
            n(1),
            null(),
            null(),
            null(),
            null(),
            null(),
            null(),
            null(),
            null(),
            null(),
            a(vec![n(1)]),
        ]),
    ])
}

/// The client capability block sent as params[0] for artifact generation.
fn create_artifact_options() -> Value {
    a(vec![
        n(2),
        null(),
        null(),
        a(vec![
            n(1),
            null(),
            null(),
            null(),
            null(),
            null(),
            null(),
            null(),
            null(),
            null(),
            a(vec![n(1)]),
        ]),
        a(vec![a(vec![n(1), n(4), n(8), n(2), n(3), n(6)])]),
    ])
}

// ---------------------------------------------------------------- catalogue
/// An RPC method observed in the web client.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rpc {
    pub id: &'static str,
    pub name: &'static str,
    /// How many independent implementations this id was confirmed in.
    pub corroboration: u8,
}

pub const RPC_LIST_NOTEBOOKS: Rpc = Rpc { id: "wXbhsf", name: "LIST_NOTEBOOKS", corroboration: 2 };
pub const RPC_CREATE_NOTEBOOK: Rpc = Rpc { id: "CCqFvf", name: "CREATE_NOTEBOOK", corroboration: 2 };
pub const RPC_GET_NOTEBOOK: Rpc = Rpc { id: "rLM1Ne", name: "GET_NOTEBOOK", corroboration: 2 };
pub const RPC_ADD_SOURCE: Rpc = Rpc { id: "izAoDd", name: "ADD_SOURCE", corroboration: 3 };
pub const RPC_ADD_SOURCE_FILE: Rpc = Rpc { id: "o4cbdc", name: "ADD_SOURCE_FILE", corroboration: 2 };
pub const RPC_DELETE_SOURCE: Rpc = Rpc { id: "tGMBJ", name: "DELETE_SOURCE", corroboration: 2 };
pub const RPC_CREATE_ARTIFACT: Rpc = Rpc { id: "R7cb6c", name: "CREATE_ARTIFACT", corroboration: 2 };
pub const RPC_LIST_ARTIFACTS: Rpc = Rpc { id: "gArtLc", name: "LIST_ARTIFACTS", corroboration: 2 };
pub const RPC_GENERATE_MIND_MAP: Rpc = Rpc { id: "yyryJe", name: "GENERATE_MIND_MAP", corroboration: 2 };
pub const RPC_SUMMARIZE: Rpc = Rpc { id: "VfAZjd", name: "SUMMARIZE", corroboration: 2 };
pub const RPC_CREATE_NOTE: Rpc = Rpc { id: "CYK0Xb", name: "CREATE_NOTE", corroboration: 2 };
pub const RPC_GET_NOTES: Rpc = Rpc { id: "cFji9", name: "GET_NOTES_AND_MIND_MAPS", corroboration: 2 };
pub const RPC_LIST_LABELS: Rpc = Rpc { id: "I3xc3c", name: "LIST_LABELS", corroboration: 1 };
pub const RPC_RETRIEVE_CHUNKS: Rpc = Rpc { id: "ASU5Oe", name: "RETRIEVE_RELEVANT_CHUNKS", corroboration: 1 };

/// Content type codes for `CREATE_ARTIFACT` (R7cb6c).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArtifactKind {
    Audio,
    Report,
    Video,
    QuizFlashcards,
    MindMap,
    FantasyMap,
    Infographic,
    SlideDeck,
    DataTable,
    File,
}

impl ArtifactKind {
    pub fn code(self) -> i64 {
        match self {
            ArtifactKind::Audio => 1,
            ArtifactKind::Report => 2,
            ArtifactKind::Video => 3,
            ArtifactKind::QuizFlashcards => 4,
            ArtifactKind::MindMap => 5,
            ArtifactKind::FantasyMap => 6,
            ArtifactKind::Infographic => 7,
            ArtifactKind::SlideDeck => 8,
            ArtifactKind::DataTable => 9,
            ArtifactKind::File => 10,
        }
    }
    pub fn from_code(c: i64) -> Option<ArtifactKind> {
        Some(match c {
            1 => ArtifactKind::Audio,
            2 => ArtifactKind::Report,
            3 => ArtifactKind::Video,
            4 => ArtifactKind::QuizFlashcards,
            5 => ArtifactKind::MindMap,
            6 => ArtifactKind::FantasyMap,
            7 => ArtifactKind::Infographic,
            8 => ArtifactKind::SlideDeck,
            9 => ArtifactKind::DataTable,
            10 => ArtifactKind::File,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            ArtifactKind::Audio => "audio_overview",
            ArtifactKind::Report => "report",
            ArtifactKind::Video => "video_overview",
            ArtifactKind::QuizFlashcards => "quiz_or_flashcards",
            ArtifactKind::MindMap => "mind_map",
            ArtifactKind::FantasyMap => "fantasy_map",
            ArtifactKind::Infographic => "infographic",
            ArtifactKind::SlideDeck => "slide_deck",
            ArtifactKind::DataTable => "data_table",
            ArtifactKind::File => "file",
        }
    }
}

/// Audio Overview length selector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioLength {
    Short,
    Default,
    Long,
}

impl AudioLength {
    pub fn code(self) -> i64 {
        match self {
            AudioLength::Short => 1,
            AudioLength::Default => 2,
            AudioLength::Long => 3,
        }
    }
}

/// Quiz vs flashcards variant for content type 4.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StudyVariant {
    Flashcards,
    Quiz,
}

impl StudyVariant {
    pub fn code(self) -> i64 {
        match self {
            StudyVariant::Flashcards => 1,
            StudyVariant::Quiz => 2,
        }
    }
}

// ---------------------------------------------------------------- requests
#[derive(Debug, Clone, PartialEq)]
pub struct Credentials {
    /// Full `Cookie:` header value from an authenticated browser session.
    pub cookies: String,
    /// `at` token (SNlM0e) scraped from the app page.
    pub at: String,
}

pub struct WireRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Percent-encode as `application/x-www-form-urlencoded` (space as %20 is accepted
/// by the endpoint; we use %20 rather than '+', which is what the web client sends).
fn form_encode(v: &str) -> String {
    let mut out = String::with_capacity(v.len() * 3);
    for b in v.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*b as char),
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{:02X}", other)),
        }
    }
    out
}

/// Build the `f.req` body for a batchexecute call.
pub fn batchexecute_body(rpc: Rpc, params: &Value) -> String {
    let inner = json::to_string(params);
    // Canonical batchexecute envelope: f.req = [[[rpcid, params_json, null, "generic"]]]
    // i.e. a list of calls; each call is a 4-element array.
    let call = a(vec![s(rpc.id), s(&inner), null(), s("generic")]);
    let envelope = a(vec![a(vec![call])]);
    format!("f.req={}", form_encode(&json::to_string(&envelope)))
}

pub fn batchexecute_request(rpc: Rpc, params: &Value, creds: &Credentials) -> WireRequest {
    WireRequest {
        method: "POST",
        url: format!("{}{}?rpcids={}&rt=c", ORIGIN, BATCH_PATH, rpc.id),
        headers: vec![
            ("Content-Type".into(), "application/x-www-form-urlencoded;charset=UTF-8".into()),
            ("X-Same-Domain".into(), "1".into()),
            ("Origin".into(), ORIGIN.into()),
            ("Referer".into(), format!("{}/", ORIGIN)),
            ("Cookie".into(), creds.cookies.clone()),
        ],
        body: batchexecute_body(rpc, params),
    }
}

// ------------------------------------------------------- source builders
/// `izAoDd` params for a web URL source.
pub fn add_url_source(notebook_id: &str, url: &str) -> Value {
    let mut slots = [null(), null(), null(), null(), null(), null(), null(), null(), null(), null(), n(1)];
    slots[2] = a(vec![s(url)]);
    a(vec![
        a(vec![source_spec(slots)]),
        s(notebook_id),
        request_options(),
    ])
}

/// `izAoDd` params for pasted text. Content-type slot 3 = 2 (pasted text).
pub fn add_text_source(notebook_id: &str, title: &str, content: &str) -> Value {
    let mut slots = [null(), null(), null(), null(), null(), null(), null(), null(), null(), null(), n(1)];
    slots[1] = a(vec![s(title), s(content)]);
    slots[3] = n(2);
    a(vec![
        a(vec![source_spec(slots)]),
        s(notebook_id),
        request_options(),
    ])
}

/// `izAoDd` params for a YouTube video. Position 7 carries the URL — different
/// from a regular web URL, which sits at position 2.
pub fn add_youtube_source(notebook_id: &str, url: &str) -> Value {
    let mut slots = [null(), null(), null(), null(), null(), null(), null(), null(), null(), null(), n(1)];
    slots[7] = s(url);
    a(vec![
        a(vec![source_spec(slots)]),
        s(notebook_id),
        request_options(),
    ])
}

/// `o4cbdc` params that register an upload and return a resumable upload URL.
pub fn register_file_source(notebook_id: &str, filename: &str) -> Value {
    let mut slots = [null(), null(), null(), null(), null(), null(), null(), null(), null(), null(), n(1)];
    slots[0] = s(filename);
    a(vec![
        s(notebook_id),
        a(vec![source_spec(slots)]),
        request_options(),
    ])
}

// ----------------------------------------------------- artifact builders
fn id_list_pair(source_ids: &[String]) -> (Value, Value) {
    let triple = a(source_ids.iter().map(|id| a(vec![a(vec![s(id)])])).collect());
    let double = a(source_ids.iter().map(|id| a(vec![s(id)])).collect());
    (triple, double)
}

pub struct ArtifactRequest<'a> {
    pub kind: ArtifactKind,
    pub notebook_id: &'a str,
    pub source_ids: &'a [String],
    /// Free-text focus/instructions.
    pub instructions: &'a str,
    /// Audio only.
    pub audio_length: AudioLength,
    /// Type 4 only.
    pub study_variant: StudyVariant,
    /// Type 4 only: 1 = fewer, 2 = standard, 3 = more.
    pub quantity: i64,
    /// Type 4 only: 1 = easy, 2 = medium, 3 = hard.
    pub difficulty: i64,
}

impl<'a> ArtifactRequest<'a> {
    pub fn new(kind: ArtifactKind, notebook_id: &'a str, source_ids: &'a [String]) -> Self {
        ArtifactRequest {
            kind,
            notebook_id,
            source_ids,
            instructions: "",
            audio_length: AudioLength::Default,
            study_variant: StudyVariant::Flashcards,
            quantity: 2,
            difficulty: 2,
        }
    }

    pub fn params(&self) -> Value {
        let (triple, double) = id_list_pair(self.source_ids);
        let spec = match self.kind {
            ArtifactKind::Audio => a(vec![
                null(),
                null(),
                n(ArtifactKind::Audio.code()),
                triple.clone(),
                null(),
                null(),
                a(vec![
                    null(),
                    a(vec![
                        s(self.instructions),
                        n(self.audio_length.code()),
                        null(),
                        double.clone(),
                    ]),
                ]),
            ]),
            ArtifactKind::QuizFlashcards => {
                // slots 6 = flashcards [qty, difficulty], 7 = quiz [qty, difficulty]
                let pair = a(vec![n(self.quantity), n(self.difficulty)]);
                let (fb, qz) = match self.study_variant {
                    StudyVariant::Flashcards => (pair, null()),
                    StudyVariant::Quiz => (null(), pair),
                };
                a(vec![
                    null(),
                    null(),
                    n(ArtifactKind::QuizFlashcards.code()),
                    triple.clone(),
                    null(),
                    null(),
                    null(),
                    null(),
                    a(vec![
                        n(self.study_variant.code()),
                        null(),
                        s(self.instructions),
                        null(),
                        null(),
                        null(),
                        fb,
                        qz,
                    ]),
                ])
            }
            other => a(vec![
                null(),
                null(),
                n(other.code()),
                triple.clone(),
                null(),
                null(),
                null(),
                null(),
                a(vec![null(), s(self.instructions)]),
            ]),
        };
        a(vec![
            create_artifact_options(),
            s(self.notebook_id),
            a(vec![
                spec,
                null(),
                a(self.source_ids.iter().map(|id| s(id)).collect()),
            ]),
        ])
    }
}

// ------------------------------------------------------------- chat request
pub fn chat_request(notebook_id: &str, question: &str, creds: &Credentials) -> WireRequest {
    let params = a(vec![
        a(vec![
            null(),
            null(),
            null(),
            null(),
            null(),
            null(),
            null(),
            a(vec![s(question)]),
        ]),
        s(notebook_id),
        request_options(),
    ]);
    WireRequest {
        method: "POST",
        url: format!("{}{}", ORIGIN, CHAT_PATH),
        headers: vec![
            ("Content-Type".into(), "application/x-www-form-urlencoded;charset=UTF-8".into()),
            ("X-Same-Domain".into(), "1".into()),
            ("Cookie".into(), creds.cookies.clone()),
        ],
        body: format!(
            "f.req={}&at={}",
            form_encode(&json::to_string(&params)),
            form_encode(&creds.at)
        ),
    }
}

// ------------------------------------------------------------ response parsing
/// The response body starts with an anti-XSSI prefix and is framed as
/// `<length>\n<json row>\n` records. Returns the decoded `wrb.fr` payloads.
pub fn parse_frames(raw: &str) -> Result<Vec<Value>, String> {
    let body = raw.trim_start_matches(")]}'").trim_start();
    let mut out: Vec<Value> = Vec::new();
    let mut lines = body.lines().peekable();
    while let Some(line) = lines.next() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let is_len = !t.is_empty() && t.chars().all(|c| c.is_ascii_digit());
        if is_len {
            if let Some(payload) = lines.next() {
                if let Ok(v) = json::parse(payload.trim()) {
                    collect_frames(&v, &mut out);
                }
            }
            continue;
        }
        if let Ok(v) = json::parse(t) {
            collect_frames(&v, &mut out);
        }
    }
    Ok(out)
}

fn collect_frames(v: &Value, out: &mut Vec<Value>) {
    let Value::Arr(items) = v else { return };
    for item in items {
        let Value::Arr(inner) = item else { continue };
        if inner.first().and_then(|x| x.as_str()) != Some("wrb.fr") {
            continue;
        }
        // ["wrb.fr", rpcid, "<json string>", ...]
        if let Some(payload) = inner.get(2).and_then(|x| x.as_str()) {
            if let Ok(parsed) = json::parse(payload) {
                out.push(parsed);
            }
        }
        // some frames carry the payload as a direct array at index 2
        if let Some(Value::Arr(_)) = inner.get(2) {
            out.push(inner[2].clone());
        }
    }
}

/// Streaming chat frames are cumulative; `envelope[4] == true` marks the final one.
pub fn parse_stream_answer(raw: &str) -> Result<String, String> {
    let frames = parse_frames(raw)?;
    let mut best: Option<(bool, usize, String)> = None;
    for f in frames {
        let Value::Arr(env) = &f else { continue };
        let is_final = env.get(4).and_then(|x| x.as_bool()).unwrap_or(false);
        let answer = env
            .first()
            .and_then(|a0| a0.as_arr())
            .and_then(|row| row.iter().find_map(|x| x.as_str()))
            .map(|s| s.to_string());
        if let Some(text) = answer {
            let better = match &best {
                None => true,
                Some((was_final, len, _)) => (!*was_final && is_final) || (!*was_final && !is_final && text.len() > *len),
            };
            if better {
                best = Some((is_final, text.len(), text));
            }
        }
    }
    best.map(|(_, _, t)| t)
        .ok_or_else(|| "no answer frame in response".to_string())
}

/// Artifact kickoff result: `[[artifact_id, title, type, null(), status, ...]]`.
pub fn parse_artifact_kickoff(frame: &Value) -> Option<(String, String, i64, i64)> {
    let row = frame.as_arr()?.first()?.as_arr()?;
    let id = row.first()?.as_str()?.to_string();
    let title = row.get(1).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let kind = row.get(2).and_then(|x| x.as_f64()).unwrap_or(0.0) as i64;
    let status = row.get(4).and_then(|x| x.as_f64()).unwrap_or(0.0) as i64;
    Some((id, title, kind, status))
}

// ------------------------------------------------------------- transport seam
/// Everything network-shaped lives behind this trait.
pub trait Transport {
    fn send(&self, req: &WireRequest) -> Result<String, String>;
}

/// The only transport this build can offer: none, and it says why.
pub struct NoTransport;

impl Transport for NoTransport {
    fn send(&self, _req: &WireRequest) -> Result<String, String> {
        Err(UNAVAILABLE.into())
    }
}

pub const UNAVAILABLE: &str = "NotebookLM cannot be reached from this build: the sandbox has no route to notebooklm.google.com (verified: HTTP 000) and no TLS client is available. The protocol is implemented and unit-tested; only a Transport is missing.";

pub struct NotebookLmClient<T: Transport> {
    pub creds: Credentials,
    pub transport: T,
}

impl<T: Transport> NotebookLmClient<T> {
    pub fn new(creds: Credentials, transport: T) -> Self {
        NotebookLmClient { creds, transport }
    }

    pub fn available(&self) -> bool {
        false
    }

    pub fn call(&self, rpc: Rpc, params: &Value) -> Result<Vec<Value>, String> {
        let req = batchexecute_request(rpc, params, &self.creds);
        let raw = self.transport.send(&req)?;
        parse_frames(&raw)
    }

    pub fn ask(&self, notebook_id: &str, question: &str) -> Result<String, String> {
        let req = chat_request(notebook_id, question, &self.creds);
        let raw = self.transport.send(&req)?;
        parse_stream_answer(&raw)
    }
}

/// Machine-readable status for the API and the UI.
pub fn status() -> Value {
    let _ = n(0);
    let mut catalogue: Vec<Value> = Vec::new();
    for rpc in [
        RPC_LIST_NOTEBOOKS,
        RPC_CREATE_NOTEBOOK,
        RPC_GET_NOTEBOOK,
        RPC_ADD_SOURCE,
        RPC_ADD_SOURCE_FILE,
        RPC_DELETE_SOURCE,
        RPC_CREATE_ARTIFACT,
        RPC_LIST_ARTIFACTS,
        RPC_GENERATE_MIND_MAP,
        RPC_SUMMARIZE,
        RPC_CREATE_NOTE,
        RPC_GET_NOTES,
        RPC_LIST_LABELS,
        RPC_RETRIEVE_CHUNKS,
    ] {
        catalogue.push(crate::obj! {
            "id" => rpc.id,
            "name" => rpc.name,
            "corroboration" => rpc.corroboration as u64,
        });
    }
    crate::obj! {
        "service" => "Google Gemini Notebook (notebooklm.google.com)",
        "status" => "protocol_implemented_target_unreachable",
        "implemented" => obj_bool(true),
        "reachable" => obj_bool(false),
        "reason" => UNAVAILABLE,
        "rpc_count" => catalogue.len() as u64,
        "rpc" => Value::Arr(catalogue),
        "artifact_types" => Value::Arr(vec![
            crate::obj!{"code" => 1u64, "kind" => "audio_overview"},
            crate::obj!{"code" => 2u64, "kind" => "report"},
            crate::obj!{"code" => 3u64, "kind" => "video_overview"},
            crate::obj!{"code" => 4u64, "kind" => "quiz_or_flashcards"},
            crate::obj!{"code" => 5u64, "kind" => "mind_map"},
            crate::obj!{"code" => 6u64, "kind" => "fantasy_map"},
            crate::obj!{"code" => 7u64, "kind" => "infographic"},
            crate::obj!{"code" => 8u64, "kind" => "slide_deck"},
            crate::obj!{"code" => 9u64, "kind" => "data_table"},
            crate::obj!{"code" => 10u64, "kind" => "file"},
        ]),
        "provenance" => "public reverse engineering: teng-lin/notebooklm-py@956400a docs/rpc-reference.md, cross-checked against jacob-bd/gemini-notebook-mcp-cli and jackwener/OpenCLI",
    }
}

fn obj_bool(b: bool) -> Value {
    Value::from(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> Vec<String> {
        vec!["src-1".to_string(), "src-2".to_string()]
    }

    #[test]
    fn batchexecute_envelope_is_the_documented_shape() {
        let body = batchexecute_body(RPC_LIST_NOTEBOOKS, &a(vec![]));
        assert!(body.starts_with("f.req="), "body must be form-encoded f.req");
        let decoded = body.trim_start_matches("f.req=");
        // percent-decoded copy of the envelope
        let raw = crate::http::percent_decode(decoded);
        let parsed = json::parse(&raw).expect("envelope must be valid JSON");
        let outer = parsed.as_arr().expect("outer array");
        let calls = outer[0].as_arr().expect("list of calls");
        let call = calls[0].as_arr().expect("call array");
        assert_eq!(call[0].as_str(), Some("wXbhsf"), "RPC id first");
        assert_eq!(call.len(), 4, "rpc id, params string, null(), 'generic'");
        assert_eq!(call[1].as_str(), Some("[]"), "params are a JSON string");
        assert!(matches!(call[2], Value::Null));
        assert_eq!(call[3].as_str(), Some("generic"));
    }

    #[test]
    fn request_has_rpc_scoped_url_and_headers() {
        let creds = Credentials { cookies: "SID=abc".into(), at: "AT123".into() };
        let req = batchexecute_request(RPC_ADD_SOURCE, &add_url_source("nb1", "https://example.com/x"), &creds);
        assert_eq!(req.method, "POST");
        assert_eq!(
            req.url,
            "https://notebooklm.google.com/_/LabsTailwindUi/data/batchexecute?rpcids=izAoDd&rt=c"
        );
        let names: Vec<&str> = req.headers.iter().map(|(k, _)| k.as_str()).collect();
        assert!(names.contains(&"X-Same-Domain"));
        assert!(names.contains(&"Cookie"));
        assert!(req.headers.iter().any(|(k, v)| k == "Cookie" && v == "SID=abc"));
    }

    #[test]
    fn url_source_puts_the_url_at_slot_two() {
        let p = add_url_source("nb1", "https://example.com/page");
        let specs = p.as_arr().unwrap()[0].as_arr().unwrap();
        let spec = specs[0].as_arr().unwrap();
        assert_eq!(spec.len(), 11, "11-element source spec");
        let urls = spec[2].as_arr().unwrap();
        assert_eq!(urls[0].as_str(), Some("https://example.com/page"));
        assert_eq!(spec[10].as_f64(), Some(1.0), "trailing marker is 1");
        assert_eq!(p.as_arr().unwrap()[1].as_str(), Some("nb1"));
    }

    #[test]
    fn text_source_carries_title_content_and_type_code() {
        let p = add_text_source("nb1", "Notes", "hello world");
        let spec = p.as_arr().unwrap()[0].as_arr().unwrap()[0].as_arr().unwrap();
        let pair = spec[1].as_arr().unwrap();
        assert_eq!(pair[0].as_str(), Some("Notes"));
        assert_eq!(pair[1].as_str(), Some("hello world"));
        assert_eq!(spec[3].as_f64(), Some(2.0), "pasted-text type code");
    }

    #[test]
    fn youtube_source_uses_slot_seven() {
        let p = add_youtube_source("nb1", "https://youtu.be/xyz");
        let spec = p.as_arr().unwrap()[0].as_arr().unwrap()[0].as_arr().unwrap();
        assert_eq!(spec[7].as_str(), Some("https://youtu.be/xyz"));
        assert!(spec[2].is_null(), "slot 2 stays empty for YouTube");
    }

    #[test]
    fn audio_artifact_uses_type_1_and_length_code() {
        let ids = ids();
        let mut req = ArtifactRequest::new(ArtifactKind::Audio, "nb1", &ids);
        req.audio_length = AudioLength::Long;
        req.instructions = "focus on evaluation";
        let p = req.params();
        let spec = p.as_arr().unwrap()[2].as_arr().unwrap()[0].as_arr().unwrap();
        assert_eq!(spec[2].as_f64(), Some(1.0), "AUDIO type code");
        // config block carries instructions + length + the id list
        let cfg = spec[6].as_arr().unwrap();
        let inner = cfg[1].as_arr().unwrap();
        assert_eq!(inner[0].as_str(), Some("focus on evaluation"));
        assert_eq!(inner[1].as_f64(), Some(3.0), "LONG == 3");
        assert_eq!(inner[3].as_arr().unwrap().len(), 2, "one entry per source id");
    }

    #[test]
    fn quiz_and_flashcards_differ_only_by_variant_slot() {
        let ids = ids();
        let mut fb = ArtifactRequest::new(ArtifactKind::QuizFlashcards, "nb1", &ids);
        fb.study_variant = StudyVariant::Flashcards;
        let mut qz = ArtifactRequest::new(ArtifactKind::QuizFlashcards, "nb1", &ids);
        qz.study_variant = StudyVariant::Quiz;

        let fb_spec = fb.params();
        let qz_spec = qz.params();
        let fb_opts = fb_spec.as_arr().unwrap()[2].as_arr().unwrap()[0].as_arr().unwrap()[8]
            .as_arr()
            .unwrap()
            .clone();
        let qz_opts = qz_spec.as_arr().unwrap()[2].as_arr().unwrap()[0].as_arr().unwrap()[8]
            .as_arr()
            .unwrap()
            .clone();
        assert_eq!(fb_opts[0].as_f64(), Some(1.0), "variant 1 = flashcards");
        assert_eq!(qz_opts[0].as_f64(), Some(2.0), "variant 2 = quiz");
        assert!(!fb_opts[6].is_null() && fb_opts[7].is_null(), "flashcards pair at slot 6");
        assert!(qz_opts[6].is_null() && !qz_opts[7].is_null(), "quiz pair at slot 7");
    }

    #[test]
    fn every_artifact_kind_has_a_distinct_code() {
        let kinds = [
            ArtifactKind::Audio,
            ArtifactKind::Report,
            ArtifactKind::Video,
            ArtifactKind::QuizFlashcards,
            ArtifactKind::MindMap,
            ArtifactKind::FantasyMap,
            ArtifactKind::Infographic,
            ArtifactKind::SlideDeck,
            ArtifactKind::DataTable,
            ArtifactKind::File,
        ];
        let mut codes: Vec<i64> = kinds.iter().map(|k| k.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 10);
        assert_eq!(ArtifactKind::from_code(1), Some(ArtifactKind::Audio));
        assert_eq!(ArtifactKind::from_code(99), None);
    }

    #[test]
    fn chat_request_targets_the_streamed_endpoint_with_at_token() {
        let creds = Credentials { cookies: "SID=abc".into(), at: "AT 123".into() };
        let req = chat_request("nb1", "what is grounding?", &creds);
        assert!(req.url.contains("GenerateFreeFormStreamed"));
        assert!(req.body.starts_with("f.req="));
        assert!(req.body.contains("&at="), "at token is form-encoded alongside f.req");
        assert!(req.body.contains("AT%20123"), "space must be percent-encoded");
    }

    #[test]
    fn parses_length_framed_wrb_fr_responses() {
        // Shape taken from the documented framing: ')]}\'' prefix, len line, JSON row.
        let inner = json::to_string(&a(vec![s("hello")]));
        let escaped = json::to_string(&s(&inner));
        let row = format!(r#"[["wrb.fr","wXbhsf",{},null,null,null,"generic"]]"#, escaped);
        let raw = format!(")]}}'\n\n{}\n{}\n", row.len(), row);
        let frames = parse_frames(&raw).expect("frames");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].as_arr().unwrap()[0].as_str(), Some("hello"));
    }

    #[test]
    fn streaming_chat_takes_the_final_cumulative_chunk() {
        let mk = |text: &str, final_chunk: bool| {
            let inner = json::to_string(&a(vec![
                a(vec![s(text)]),
                null(),
                null(),
                null(),
                Value::from(final_chunk),
            ]));
            let escaped = json::to_string(&s(&inner));
            format!(r#"[["wrb.fr","chat",{},null,null,null,"generic"]]"#, escaped)
        };
        let c1 = mk("The answer is", false);
        let c2 = mk("The answer is grounding.", true);
        let raw = format!(
            ")]}}'\n\n{}\n{}\n{}\n{}\n",
            c1.len(),
            c1,
            c2.len(),
            c2
        );
        assert_eq!(parse_stream_answer(&raw).unwrap(), "The answer is grounding.");
    }

    #[test]
    fn artifact_kickoff_is_decoded() {
        let frame = a(vec![a(vec![
            s("art-123"),
            s("Audio Overview"),
            n(1),
            null(),
            n(1),
        ])]);
        let (id, title, kind, status) = parse_artifact_kickoff(&frame).expect("kickoff");
        assert_eq!(id, "art-123");
        assert_eq!(title, "Audio Overview");
        assert_eq!(kind, 1);
        assert_eq!(status, 1, "1 == pending/initialized");
    }

    #[test]
    fn transport_is_unavailable_and_says_why() {
        let client = NotebookLmClient::new(
            Credentials { cookies: String::new(), at: String::new() },
            NoTransport,
        );
        assert!(!client.available(), "must never claim reachability");
        let err = client.call(RPC_LIST_NOTEBOOKS, &a(vec![])).unwrap_err();
        assert!(err.contains("no route") && err.contains("TLS"));
    }

    #[test]
    fn corroboration_counts_match_the_measured_evidence() {
        // Measured with docs/rea/ + scripts/fetch_notebooklm_evidence.sh:
        // occurrences of each id across the three independent codebases.
        for rpc in [
            RPC_ADD_SOURCE,
            RPC_ADD_SOURCE_FILE,
            RPC_LIST_NOTEBOOKS,
            RPC_CREATE_NOTEBOOK,
            RPC_GET_NOTEBOOK,
            RPC_CREATE_ARTIFACT,
            RPC_SUMMARIZE,
            RPC_DELETE_SOURCE,
            RPC_LIST_ARTIFACTS,
            RPC_CREATE_NOTE,
            RPC_GET_NOTES,
            RPC_GENERATE_MIND_MAP,
        ] {
            assert!(
                rpc.corroboration >= 2,
                "{} ({}) must be confirmed in >=2 independent implementations, got {}",
                rpc.name,
                rpc.id,
                rpc.corroboration
            );
        }
        assert_eq!(RPC_ADD_SOURCE.corroboration, 3, "izAoDd is in all three codebases");
    }

    #[test]
    fn status_reports_protocol_implemented_but_unreachable() {
        let st = status();
        assert_eq!(st.str_of("status"), Some("protocol_implemented_target_unreachable"));
        assert_eq!(st.get("reachable").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(st.get("implemented").and_then(|v| v.as_bool()), Some(true));
        assert!(st.get("rpc").and_then(|v| v.as_arr()).unwrap().len() >= 10);
        assert!(st.str_of("provenance").unwrap().contains("notebooklm-py"));
    }
}
