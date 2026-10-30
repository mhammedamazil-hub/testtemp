//! HTTP API: the single boundary between the web UI and the Rust core.
//!
//! Every handler validates input, enforces notebook scoping and returns honest
//! error codes. No handler invents data: unsupported operations return 4xx with
//! a machine-readable reason.

use crate::artifacts;
use crate::domain::*;
use crate::grounded;
use crate::http::{percent_decode, Request, Response};
use crate::ingest;
use crate::json::{self, Value};
use crate::providers::{self, ProviderConfig};
use crate::obj;
use crate::security;
use crate::store::Store;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct Api {
    pub store: Store,
}

fn err(status: u16, code: &str, msg: &str) -> Response {
    Response::json(
        status,
        &obj! { "error" => msg, "code" => code, "status" => status as u64 },
    )
}

fn ok(v: Value) -> Response {
    Response::json(200, &v)
}

fn created(v: Value) -> Response {
    Response::json(201, &v)
}

impl Api {
    pub fn new(store: Store) -> Api {
        Api { store }
    }

    pub fn handle(&self, req: &Request) -> Response {
        let path = req.path.trim_end_matches('/').to_string();
        let path = if path.is_empty() { "/".to_string() } else { path };
        let segs: Vec<&str> = path.trim_start_matches('/').split('/').filter(|s| !s.is_empty()).collect();

        let result = match (req.method.as_str(), segs.as_slice()) {
            ("GET", ["api", "health"]) => Ok(self.health()),
            ("GET", ["api", "capabilities"]) => Ok(ok(self.capabilities())),
            ("GET", ["api", "notebooks"]) => Ok(ok(self.list_notebooks())),
            ("POST", ["api", "notebooks"]) => self.create_notebook(req),
            ("GET", ["api", "notebooks", id]) => Ok(self.get_notebook(id)),
            ("PATCH", ["api", "notebooks", id]) => self.update_notebook(id, req),
            ("DELETE", ["api", "notebooks", id]) => Ok(self.delete_notebook(id)),
            ("GET", ["api", "notebooks", id, "sources"]) => Ok(ok(self.list_sources(id))),
            ("POST", ["api", "notebooks", id, "sources", "text"]) => self.add_text_source(id, req),
            ("POST", ["api", "notebooks", id, "sources", "upload"]) => self.add_upload_source(id, req),
            ("POST", ["api", "notebooks", id, "sources", "url"]) => self.add_url_source(id, req),
            ("GET", ["api", "notebooks", id, "chunks"]) => Ok(ok(self.list_chunks(id))),
            ("POST", ["api", "notebooks", id, "chat"]) => self.chat(id, req),
            ("GET", ["api", "notebooks", id, "conversations"]) => Ok(ok(self.list_conversations(id))),
            ("GET", ["api", "conversations", cid]) => Ok(self.get_conversation(cid)),
            ("POST", ["api", "notebooks", id, "notes"]) => self.create_note(id, req),
            ("GET", ["api", "notebooks", id, "notes"]) => Ok(ok(self.list_notes(id))),
            ("PATCH", ["api", "notes", nid]) => self.update_note(nid, req),
            ("DELETE", ["api", "notes", nid]) => Ok(self.delete_note(nid)),
            ("POST", ["api", "notebooks", id, "artifacts"]) => self.create_artifact(id, req),
            ("GET", ["api", "notebooks", id, "artifacts"]) => Ok(ok(self.list_artifacts(id))),
            ("GET", ["api", "artifacts", aid]) => Ok(self.get_artifact(aid)),
            ("GET", ["api", "sources", sid]) => Ok(self.get_source(sid)),
            ("GET", ["api", "sources", sid, "chunks"]) => Ok(self.source_chunks(sid, req)),
            ("DELETE", ["api", "sources", sid]) => Ok(self.delete_source(sid)),
            ("GET", ["api", "notebooks", id, "export"]) => Ok(self.export_notebook(id)),
            ("POST", ["api", "import"]) => self.import_notebook(req),
            ("GET", ["api", "search"]) => Ok(self.search(req)),
            ("GET", ["api", "providers"]) => Ok(self.list_providers()),
            ("POST", ["api", "providers"]) => self.add_provider(req),
            ("DELETE", ["api", "providers", pid]) => Ok(self.delete_provider(pid)),
            ("POST", ["api", "backup"]) => self.create_backup(),
            ("GET", ["api", "backups"]) => Ok(self.list_backups()),
            _ => Err(err(404, "not_found", &format!("no route for {} {}", req.method, req.path))),
        };

        match result {
            Ok(r) => r,
            Err(r) => r,
        }
    }

    // ------------------------------------------------------------ health
    fn health(&self) -> Response {
        let (nb, src, chunks, conv, notes, arts) = self.store.read(|db| {
            (db.notebooks.len(), db.sources.len(), db.chunks.len(), db.conversations.len(), db.notes.len(), db.artifacts.len())
        });
        ok(obj! {
            "status" => "ok",
            "version" => VERSION,
            "schema" => "forge.db/1",
            "counts" => obj! {
                "notebooks" => nb as u64,
                "sources" => src as u64,
                "chunks" => chunks as u64,
                "conversations" => conv as u64,
                "notes" => notes as u64,
                "artifacts" => arts as u64,
            },
        })
    }

    /// Honest, machine-readable capability matrix (drives UI badges).
    fn capabilities(&self) -> Value {
        obj! {
            "version" => VERSION,
            "sources" => obj! {
                "text" => obj!{ "status" => "supported", "locator" => "paragraph" },
                "markdown" => obj!{ "status" => "supported", "locator" => "paragraph+section" },
                "html" => obj!{ "status" => "supported", "locator" => "paragraph+section" },
                "csv" => obj!{ "status" => "supported", "locator" => "row" },
                "pdf" => obj!{ "status" => "supported", "locator" => "page", "note" => "text-layer PDFs; no OCR" },
                "url" => obj!{ "status" => "blocked", "reason" => "https requires TLS (no crate available); http fetch is SSRF-guarded but this sandbox has no outbound route" },
                "docx" => obj!{ "status" => "supported", "locator" => "paragraph", "note" => "WordprocessingML text; images ignored" },
                "pptx" => obj!{ "status" => "supported", "locator" => "slide", "note" => "slide text; speaker notes not indexed" },
                "xlsx" => obj!{ "status" => "supported", "locator" => "row", "note" => "first worksheet, shared strings resolved" },
                "audio" => obj!{ "status" => "unsupported", "reason" => "no ASR engine available offline" },
                "video" => obj!{ "status" => "unsupported", "reason" => "no ASR/FFmpeg pipeline in this build" },
                "image_ocr" => obj!{ "status" => "unsupported", "reason" => "no OCR engine available" },
            },
            "providers" => self.provider_status(),
            "answering" => obj! {
                "engine" => "extractive",
                "grounded" => true,
                "abstains" => true,
                "citations_verified" => true,
                "llm" => obj!{ "status" => "not_configured", "reason" => "no TLS client and no provider key; requires an https-capable build" },
            },
            "artifacts" => obj! {
                "flashcards" => "supported",
                "quiz" => "supported",
                "report" => "supported",
                "mindmap" => "supported",
                "study_guide" => "supported",
                "faq" => "supported",
                "summary" => "supported",
                "audio_overview" => "unsupported",
                "audio_script" => "supported",
                "video_overview" => "unsupported",
                "slides" => "unsupported",
                "infographic" => "unsupported",
            },
        }
    }

    // --------------------------------------------------------- notebooks
    fn list_notebooks(&self) -> Value {
        self.store.read(|db| {
            let items: Vec<Value> = db
                .notebooks
                .iter()
                .map(|n| {
                    let sources = db.sources.iter().filter(|s| s.notebook_id == n.id).count();
                    let notes = db.notes.iter().filter(|x| x.notebook_id == n.id).count();
                    let artifacts = db.artifacts.iter().filter(|x| x.notebook_id == n.id).count();
                    obj! {
                        "id" => n.id.as_str(),
                        "title" => n.title.as_str(),
                        "description" => n.description.as_str(),
                        "tags" => json_str_list(&n.tags),
                        "archived" => n.archived,
                        "created_at" => n.created_at.as_str(),
                        "updated_at" => n.updated_at.as_str(),
                        "source_count" => sources as u64,
                        "note_count" => notes as u64,
                        "artifact_count" => artifacts as u64,
                    }
                })
                .collect();
            obj! { "notebooks" => Value::Arr(items) }
        })
    }

    fn create_notebook(&self, req: &Request) -> Result<Response, Response> {
        let body = parse_body(req)?;
        let title = body.str_of("title").unwrap_or("").trim().to_string();
        if title.is_empty() {
            return Err(err(422, "invalid_title", "title is required"));
        }
        let now = now_iso();
        let nb = Notebook {
            id: new_id("nb"),
            title,
            description: body.str_of("description").unwrap_or("").to_string(),
            instructions: body.str_of("instructions").unwrap_or("").to_string(),
            tags: parse_str_list(body.get("tags")),
            archived: false,
            created_at: now.clone(),
            updated_at: now,
        };
        let id = nb.id.clone();
        self.store
            .write(|db| db.notebooks.push(nb.clone()))
            .map_err(|e| err(500, "storage", &e.to_string()))?;
        Ok(created(obj! { "id" => id, "title" => nb.title.clone() }))
    }

    fn get_notebook(&self, id: &str) -> Response {
        self.store.read(|db| match db.notebooks.iter().find(|n| n.id == id) {
            None => err(404, "not_found", "notebook not found"),
            Some(n) => ok(obj! {
                "id" => n.id.as_str(),
                "title" => n.title.as_str(),
                "description" => n.description.as_str(),
                "instructions" => n.instructions.as_str(),
                "tags" => json_str_list(&n.tags),
                "archived" => n.archived,
                "created_at" => n.created_at.as_str(),
                "updated_at" => n.updated_at.as_str(),
                "sources" => Value::Arr(db.sources.iter().filter(|s| s.notebook_id == id).map(source_json).collect()),
                "notes" => Value::Arr(db.notes.iter().filter(|x| x.notebook_id == id).map(note_json).collect()),
                "artifacts" => Value::Arr(db.artifacts.iter().filter(|x| x.notebook_id == id).map(artifact_json).collect()),
            }),
        })
    }

    fn update_notebook(&self, id: &str, req: &Request) -> Result<Response, Response> {
        let body = parse_body(req)?;
        let found = self
            .store
            .write(|db| {
                if let Some(n) = db.notebooks.iter_mut().find(|n| n.id == id) {
                    if let Some(t) = body.str_of("title") {
                        if !t.trim().is_empty() {
                            n.title = t.trim().to_string();
                        }
                    }
                    if let Some(d) = body.str_of("description") {
                        n.description = d.to_string();
                    }
                    if let Some(i) = body.str_of("instructions") {
                        n.instructions = i.to_string();
                    }
                    if body.get("tags").is_some() {
                        n.tags = parse_str_list(body.get("tags"));
                    }
                    if let Some(a) = body.get("archived").and_then(|v| v.as_bool()) {
                        n.archived = a;
                    }
                    n.updated_at = now_iso();
                    true
                } else {
                    false
                }
            })
            .map_err(|e| err(500, "storage", &e.to_string()))?;
        if found {
            Ok(ok(obj! { "id" => id, "updated" => true }))
        } else {
            Err(err(404, "not_found", "notebook not found"))
        }
    }

    fn delete_notebook(&self, id: &str) -> Response {
        let removed = self
            .store
            .write(|db| {
                let before = db.notebooks.len();
                db.notebooks.retain(|n| n.id != id);
                let existed = db.notebooks.len() != before;
                db.sources.retain(|s| s.notebook_id != id);
                db.chunks.retain(|c| c.notebook_id != id);
                db.conversations.retain(|c| c.notebook_id != id);
                db.notes.retain(|n| n.notebook_id != id);
                db.artifacts.retain(|a| a.notebook_id != id);
                existed
            })
            .unwrap_or(false);
        if removed {
            ok(obj! { "deleted" => true })
        } else {
            err(404, "not_found", "notebook not found")
        }
    }

    // ----------------------------------------------------------- sources
    fn list_sources(&self, nb: &str) -> Value {
        self.store.read(|db| {
            obj! {
                "sources" => Value::Arr(db.sources.iter().filter(|s| s.notebook_id == nb).map(source_json).collect())
            }
        })
    }

    fn get_source(&self, sid: &str) -> Response {
        self.store.read(|db| match db.sources.iter().find(|s| s.id == sid) {
            None => err(404, "not_found", "source not found"),
            Some(s) => ok(source_json(s)),
        })
    }

    fn source_chunks(&self, sid: &str, req: &Request) -> Response {
        let page = req.query_get("page").and_then(|p| p.parse::<u32>().ok());
        let payload: Value = self.store.read(|db| {
            let mut items: Vec<&Chunk> = db.chunks.iter().filter(|c| c.source_id == sid).collect();
            items.sort_by_key(|c| c.ordinal);
            let total = items.len();
            if let Some(p) = page {
                items.retain(|c| c.locator.page == Some(p));
            }
            obj! {
                "source_id" => sid,
                "total" => total as u64,
                "chunks" => Value::Arr(items.iter().map(|c| obj! {
                    "id" => c.id.as_str(),
                    "ordinal" => c.ordinal as u64,
                    "text" => c.text.as_str(),
                    "locator" => c.locator.to_json(),
                    "locator_label" => c.locator.human(),
                }).collect()),
            }
        });
        ok(payload)
    }

    fn delete_source(&self, sid: &str) -> Response {
        let removed = self
            .store
            .write(|db| {
                let before = db.sources.len();
                db.sources.retain(|s| s.id != sid);
                let existed = db.sources.len() != before;
                db.chunks.retain(|c| c.source_id != sid);
                existed
            })
            .unwrap_or(false);
        if removed {
            ok(obj! { "deleted" => true })
        } else {
            err(404, "not_found", "source not found")
        }
    }

    fn add_text_source(&self, nb: &str, req: &Request) -> Result<Response, Response> {
        let body = parse_body(req)?;
        let text = body.str_of("text").unwrap_or("").to_string();
        if text.trim().is_empty() {
            return Err(err(422, "empty_source", "text is empty"));
        }
        if text.len() > security::MAX_TEXT_BYTES {
            return Err(err(413, "too_large", "pasted text exceeds limit"));
        }
        let title = body
            .str_of("title")
            .map(|s| s.to_string())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("Pasted note {}", now_iso()));
        self.ingest_bytes(nb, &title, "text", "paste", text.as_bytes(), "text")
    }

    fn add_upload_source(&self, nb: &str, req: &Request) -> Result<Response, Response> {
        if req.body.len() > security::MAX_SOURCE_BYTES {
            return Err(err(413, "too_large", "upload exceeds size limit"));
        }
        if req.body.is_empty() {
            return Err(err(422, "empty_source", "empty upload"));
        }
        let filename = req
            .query_get("filename")
            .map(percent_decode)
            .unwrap_or_else(|| "upload.txt".to_string());
        let safe_name = security::clean_filename(&filename);
        let kind = SourceKind::from_filename(&safe_name).as_str().to_string();
        self.ingest_bytes(nb, &safe_name, &kind, &safe_name, &req.body, "upload")
    }

    fn add_url_source(&self, _nb: &str, req: &Request) -> Result<Response, Response> {
        let body = parse_body(req)?;
        let url = body.str_of("url").unwrap_or("").trim().to_string();
        match security::check_url(&url) {
            security::UrlVerdict::Rejected(reason) => {
                return Err(err(422, "url_rejected", &format!("URL refused: {}", reason)))
            }
            security::UrlVerdict::Allowed(clean) => {
                // Honest limitation: no TLS client exists in this build, and the
                // sandbox has no outbound route for arbitrary hosts.
                return Err(err(
                    501,
                    "url_ingest_unavailable",
                    &format!(
                        "URL ingesting is not available in this build (validated {} but HTTPS needs a TLS client and no external fetch route exists here). Paste the page text or download the file and upload it instead.",
                        clean
                    ),
                ));
            }
        }
    }

    fn ingest_bytes(
        &self,
        nb: &str,
        title: &str,
        kind: &str,
        origin: &str,
        bytes: &[u8],
        mode: &str,
    ) -> Result<Response, Response> {
        if mode == "upload" && kind == "url" {
            return Err(err(422, "invalid_kind", "url sources must use the url endpoint"));
        }
        let notebook_exists = self.store.read(|db| db.notebooks.iter().any(|n| n.id == nb));
        if !notebook_exists {
            return Err(err(404, "not_found", "notebook not found"));
        }
        let source_id = new_id("src");
        let parsed = match ingest::extract(kind, title, bytes) {
            Ok(p) => p,
            Err(e) => {
                // Record the failure honestly so the UI can offer a retry.
                let failed = Source {
                    id: source_id.clone(),
                    notebook_id: nb.to_string(),
                    title: title.to_string(),
                    kind: kind.to_string(),
                    origin: origin.to_string(),
                    byte_len: bytes.len(),
                    status: "failed".into(),
                    error: e.clone(),
                    warnings: vec![],
                    added_at: now_iso(),
                    page_count: 0,
                    chunk_count: 0,
                };
                let _ = self.store.write(|db| db.sources.push(failed));
                return Err(err(422, "extract_failed", &e));
            }
        };

        if parsed.blocks.is_empty() {
            let msg = "no readable text could be extracted from this file";
            let failed = Source {
                id: source_id.clone(),
                notebook_id: nb.to_string(),
                title: title.to_string(),
                kind: kind.to_string(),
                origin: origin.to_string(),
                byte_len: bytes.len(),
                status: "failed".into(),
                error: msg.into(),
                warnings: parsed.warnings.clone(),
                added_at: now_iso(),
                page_count: parsed.page_count,
                chunk_count: 0,
            };
            let _ = self.store.write(|db| db.sources.push(failed));
            return Err(err(422, "empty_extraction", msg));
        }

        let chunk_count = parsed.blocks.len();
        let source = Source {
            id: source_id.clone(),
            notebook_id: nb.to_string(),
            title: title.to_string(),
            kind: kind.to_string(),
            origin: origin.to_string(),
            byte_len: bytes.len(),
            status: "ready".into(),
            error: String::new(),
            warnings: parsed.warnings.clone(),
            added_at: now_iso(),
            page_count: parsed.page_count,
            chunk_count: chunk_count as u32,
        };
        let chunks: Vec<Chunk> = parsed
            .blocks
            .into_iter()
            .enumerate()
            .map(|(i, b)| Chunk {
                id: new_id("chk"),
                source_id: source_id.clone(),
                notebook_id: nb.to_string(),
                ordinal: i as u32,
                text: b.text,
                locator: b.locator,
            })
            .collect();
        let sid_for_warn = source_id.clone();
        let for_response: Vec<Value> = chunks
            .iter()
            .take(50)
            .map(|c| obj! {
                "id" => c.id.as_str(),
                "text" => c.text.as_str(),
                "locator" => c.locator.to_json(),
            })
            .collect();
        self.store
            .write(|db| {
                db.sources.push(source);
                db.chunks.extend(chunks);
            })
            .map_err(|e| err(500, "storage", &e.to_string()))?;

        Ok(created(obj! {
            "source_id" => source_id.clone(),
            "title" => title,
            "kind" => kind,
            "status" => "ready",
            "chunk_count" => chunk_count as u64,
            "page_count" => self.store.read(|db| db.sources.iter().find(|s| s.id == source_id).map(|s| s.page_count).unwrap_or(0)) as u64,
            "warnings" => Value::Arr(parsed_warnings(&self.store, &sid_for_warn)),
            "preview" => Value::Arr(for_response),
        }))
    }

    fn list_chunks(&self, nb: &str) -> Value {
        self.store.read(|db| {
            obj! {
                "chunks" => Value::Arr(db.chunks.iter().filter(|c| c.notebook_id == nb).take(200).map(|c| obj! {
                    "id" => c.id.as_str(),
                    "source_id" => c.source_id.as_str(),
                    "text" => c.text.as_str(),
                    "locator" => c.locator.to_json(),
                }).collect())
            }
        })
    }

    // -------------------------------------------------------------- chat
    fn chat(&self, nb: &str, req: &Request) -> Result<Response, Response> {
        let body = parse_body(req)?;
        let question = body.str_of("question").unwrap_or("").trim().to_string();
        if question.is_empty() {
            return Err(err(422, "empty_question", "question is required"));
        }
        if question.len() > 4000 {
            return Err(err(422, "question_too_long", "question exceeds 4000 characters"));
        }

        let exists = self.store.read(|db| db.notebooks.iter().any(|n| n.id == nb));
        if !exists {
            return Err(err(404, "not_found", "notebook not found"));
        }

        let selected: Vec<String> = parse_str_list(body.get("source_ids"));
        let instructions = self.store.read(|db| {
            db.notebooks
                .iter()
                .find(|n| n.id == nb)
                .map(|n| n.instructions.clone())
                .unwrap_or_default()
        });

        let (chunks, sources) = self.store.read(|db| {
            let sources: Vec<Source> = db
                .sources
                .iter()
                .filter(|s| s.notebook_id == nb && s.status == "ready")
                .filter(|s| selected.is_empty() || selected.contains(&s.id))
                .cloned()
                .collect();
            let chunks: Vec<Chunk> = db
                .chunks
                .iter()
                .filter(|c| c.notebook_id == nb)
                .filter(|c| selected.is_empty() || selected.contains(&c.source_id))
                .cloned()
                .collect();
            (chunks, sources)
        });

        if chunks.is_empty() {
            return Err(err(
                409,
                "no_sources",
                "no ready sources are selected; import a source first",
            ));
        }

        let configured = self.load_providers().into_iter().next();
        let key = configured
            .as_ref()
            .and_then(|c| providers::read_secret(&self.store.dir, &c.id));
        let (engine, engine_note) =
            providers::select(configured.as_ref(), key.as_deref());
        let answer = engine
            .answer(&question, &chunks, &sources, 4)
            .unwrap_or_else(|_| grounded::answer(&question, &chunks, &sources, 4));
        let problems = grounded::validate_citations(&answer.citations, &chunks);

        let now = now_iso();
        let user_msg = Message {
            id: new_id("msg"),
            role: "user".into(),
            text: question.clone(),
            citations: vec![],
            created_at: now.clone(),
            grounded: false,
            abstained: false,
            note: String::new(),
        };
        let assistant_msg = Message {
            id: new_id("msg"),
            role: "assistant".into(),
            text: answer.text.clone(),
            citations: answer.citations.clone(),
            created_at: now.clone(),
            grounded: true,
            abstained: answer.abstained,
            note: if problems.is_empty() {
                String::new()
            } else {
                format!("citation validation warnings: {}", problems.join("; "))
            },
        };

        let conv_id = body
            .str_of("conversation_id")
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());
        let mut out_conv = String::new();
        self.store
            .write(|db| {
                let cid = match conv_id {
                    Some(c) if db.conversations.iter().any(|x| x.id == c && x.notebook_id == nb) => c,
                    _ => {
                        let c = new_id("conv");
                        db.conversations.push(Conversation {
                            id: c.clone(),
                            notebook_id: nb.to_string(),
                            title: question.chars().take(60).collect(),
                            messages: vec![],
                            created_at: now.clone(),
                            updated_at: now.clone(),
                        });
                        c
                    }
                };
                if let Some(conv) = db.conversations.iter_mut().find(|c| c.id == cid) {
                    conv.messages.push(user_msg);
                    conv.messages.push(assistant_msg.clone());
                    conv.updated_at = now.clone();
                }
                out_conv = cid;
            })
            .map_err(|e| err(500, "storage", &e.to_string()))?;

        Ok(ok(obj! {
            "conversation_id" => out_conv,
            "message_id" => assistant_msg.id.clone(),
            "answer" => assistant_msg.text.clone(),
            "citations" => Value::Arr(assistant_msg.citations.iter().map(|c| c.to_json()).collect()),
            "abstained" => assistant_msg.abstained,
            "grounded" => true,
            "engine" => answer.engine.clone(),
            "engine_note" => engine_note.clone(),
            "coverage" => answer.coverage as u64,
            "citation_problems" => Value::Arr(problems.iter().map(|p| Value::from(p.as_str())).collect()),
            "sources_considered" => sources.len() as u64,
            "chunks_considered" => chunks.len() as u64,
            "note" => assistant_msg.note.clone(),
            "notebook_instructions" => instructions,
        }))
    }

    fn list_conversations(&self, nb: &str) -> Value {
        self.store.read(|db| {
            obj! {
                "conversations" => Value::Arr(db.conversations.iter().filter(|c| c.notebook_id == nb).map(|c| obj! {
                    "id" => c.id.as_str(),
                    "title" => c.title.as_str(),
                    "message_count" => c.messages.len() as u64,
                    "updated_at" => c.updated_at.as_str(),
                }).collect())
            }
        })
    }

    fn get_conversation(&self, cid: &str) -> Response {
        self.store.read(|db| match db.conversations.iter().find(|c| c.id == cid) {
            None => err(404, "not_found", "conversation not found"),
            Some(c) => ok(obj! {
                "id" => c.id.as_str(),
                "notebook_id" => c.notebook_id.as_str(),
                "title" => c.title.as_str(),
                "messages" => Value::Arr(c.messages.iter().map(|m| m.to_json()).collect()),
            }),
        })
    }

    // ------------------------------------------------------------- notes
    fn create_note(&self, nb: &str, req: &Request) -> Result<Response, Response> {
        let body = parse_body(req)?;
        let text = body.str_of("body").unwrap_or("").to_string();
        if text.trim().is_empty() {
            return Err(err(422, "empty_note", "note body is required"));
        }
        let now = now_iso();
        let note = Note {
            id: new_id("note"),
            notebook_id: nb.to_string(),
            title: body
                .str_of("title")
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| text.lines().next().unwrap_or("Note").chars().take(60).collect()),
            body: text,
            source_ids: parse_str_list(body.get("source_ids")),
            created_at: now.clone(),
            updated_at: now,
        };
        let id = note.id.clone();
        let nb_ok = self.store.read(|db| db.notebooks.iter().any(|n| n.id == nb));
        if !nb_ok {
            return Err(err(404, "not_found", "notebook not found"));
        }
        self.store
            .write(|db| db.notes.push(note))
            .map_err(|e| err(500, "storage", &e.to_string()))?;
        Ok(created(obj! { "id" => id }))
    }

    fn list_notes(&self, nb: &str) -> Value {
        self.store.read(|db| {
            obj! {
                "notes" => Value::Arr(db.notes.iter().filter(|n| n.notebook_id == nb).map(note_json).collect())
            }
        })
    }

    fn update_note(&self, nid: &str, req: &Request) -> Result<Response, Response> {
        let body = parse_body(req)?;
        let found = self
            .store
            .write(|db| {
                if let Some(n) = db.notes.iter_mut().find(|n| n.id == nid) {
                    if let Some(t) = body.str_of("title") {
                        n.title = t.to_string();
                    }
                    if let Some(b) = body.str_of("body") {
                        n.body = b.to_string();
                    }
                    n.updated_at = now_iso();
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);
        if found {
            Ok(ok(obj! { "updated" => true }))
        } else {
            Err(err(404, "not_found", "note not found"))
        }
    }

    fn delete_note(&self, nid: &str) -> Response {
        let removed = self
            .store
            .write(|db| {
                let before = db.notes.len();
                db.notes.retain(|n| n.id != nid);
                db.notes.len() != before
            })
            .unwrap_or(false);
        if removed {
            ok(obj! { "deleted" => true })
        } else {
            err(404, "not_found", "note not found")
        }
    }

    // --------------------------------------------------------- artifacts
    fn create_artifact(&self, nb: &str, req: &Request) -> Result<Response, Response> {
        let body = parse_body(req)?;
        let kind = body.str_of("kind").unwrap_or("").to_string();
        let topic = body.str_of("topic").unwrap_or("").to_string();
        let selected: Vec<String> = parse_str_list(body.get("source_ids"));

        let (chunks, sources) = self.store.read(|db| {
            let sources: Vec<Source> = db
                .sources
                .iter()
                .filter(|s| s.notebook_id == nb && s.status == "ready")
                .filter(|s| selected.is_empty() || selected.contains(&s.id))
                .cloned()
                .collect();
            let chunks: Vec<Chunk> = db
                .chunks
                .iter()
                .filter(|c| c.notebook_id == nb)
                .filter(|c| selected.is_empty() || selected.contains(&c.source_id))
                .cloned()
                .collect();
            (chunks, sources)
        });
        if chunks.is_empty() {
            return Err(err(409, "no_sources", "select at least one ready source"));
        }
        let (title, payload) = artifacts::generate(&kind, &chunks, &sources, &topic)
            .map_err(|e| err(422, "bad_kind", &e))?;
        let art = Artifact {
            id: new_id("art"),
            notebook_id: nb.to_string(),
            kind: kind.clone(),
            title,
            created_at: now_iso(),
            source_ids: sources.iter().map(|s| s.id.clone()).collect(),
            payload,
        };
        let id = art.id.clone();
        let payload_out = art.payload.clone();
        let title_out = art.title.clone();
        self.store
            .write(|db| db.artifacts.push(art))
            .map_err(|e| err(500, "storage", &e.to_string()))?;
        Ok(created(obj! {
            "id" => id,
            "kind" => kind,
            "title" => title_out,
            "payload" => payload_out,
        }))
    }

    fn list_artifacts(&self, nb: &str) -> Value {
        self.store.read(|db| {
            obj! {
                "artifacts" => Value::Arr(db.artifacts.iter().filter(|a| a.notebook_id == nb).map(|a| obj! {
                    "id" => a.id.as_str(),
                    "kind" => a.kind.as_str(),
                    "title" => a.title.as_str(),
                    "created_at" => a.created_at.as_str(),
                }).collect())
            }
        })
    }

    fn get_artifact(&self, aid: &str) -> Response {
        self.store.read(|db| match db.artifacts.iter().find(|a| a.id == aid) {
            None => err(404, "not_found", "artifact not found"),
            Some(a) => ok(obj! {
                "id" => a.id.as_str(),
                "notebook_id" => a.notebook_id.as_str(),
                "kind" => a.kind.as_str(),
                "title" => a.title.as_str(),
                "created_at" => a.created_at.as_str(),
                "payload" => a.payload.clone(),
            }),
        })
    }

    // ------------------------------------------------------ export/import
    fn export_notebook(&self, nb: &str) -> Response {
        let found = self.store.read(|db| db.notebooks.iter().any(|n| n.id == nb));
        if !found {
            return err(404, "not_found", "notebook not found");
        }
        let payload = self.store.read(|db| {
            let notebook = db.notebooks.iter().find(|n| n.id == nb).unwrap();
            obj! {
                "format" => "forge.notebook/1",
                "exported_at" => now_iso(),
                "notebook" => nb_json(notebook),
                "sources" => Value::Arr(db.sources.iter().filter(|s| s.notebook_id == nb).map(source_json).collect()),
                "chunks" => Value::Arr(db.chunks.iter().filter(|c| c.notebook_id == nb).map(chunk_json).collect()),
                "conversations" => Value::Arr(db.conversations.iter().filter(|c| c.notebook_id == nb).map(conv_json).collect()),
                "notes" => Value::Arr(db.notes.iter().filter(|n| n.notebook_id == nb).map(note_json).collect()),
                "artifacts" => Value::Arr(db.artifacts.iter().filter(|a| a.notebook_id == nb).map(artifact_json).collect()),
            }
        });
        Response::json(200, &payload).with_header(
            "Content-Disposition",
            &format!("attachment; filename=\"{}.forge.json\"", nb),
        )
    }

    fn import_notebook(&self, req: &Request) -> Result<Response, Response> {
        let body = parse_body(req)?;
        let fmt = body.str_of("format").unwrap_or("");
        if fmt != "forge.notebook/1" {
            return Err(err(422, "bad_format", "expected format forge.notebook/1"));
        }
        let nb_v = body.get("notebook").ok_or_else(|| err(422, "bad_format", "notebook missing"))?;
        let new_nb_id = new_id("nb");
        let now = now_iso();
        let mut id_map: BTreeMap<String, String> = BTreeMap::new();

        let notebook = Notebook {
            id: new_nb_id.clone(),
            title: format!("{} (imported)", nb_v.str_of("title").unwrap_or("Notebook")),
            description: nb_v.str_of("description").unwrap_or("").to_string(),
            instructions: nb_v.str_of("instructions").unwrap_or("").to_string(),
            tags: parse_str_list(nb_v.get("tags")),
            archived: false,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        id_map.insert(nb_v.str_of("id").unwrap_or("").to_string(), new_nb_id.clone());

        let old_sources: Vec<&Value> = body.get("sources").and_then(|v| v.as_arr()).map(|a| a.iter().collect()).unwrap_or_default();
        let mut sources: Vec<Source> = Vec::new();
        for s in &old_sources {
            let old = s.str_of("id").unwrap_or("").to_string();
            let new = new_id("src");
            id_map.insert(old.clone(), new.clone());
            sources.push(Source {
                id: new,
                notebook_id: new_nb_id.clone(),
                title: s.str_of("title").unwrap_or("source").to_string(),
                kind: s.str_of("kind").unwrap_or("text").to_string(),
                origin: s.str_of("origin").unwrap_or("").to_string(),
                byte_len: s.get("byte_len").and_then(|x| x.as_f64()).unwrap_or(0.0) as usize,
                status: s.str_of("status").unwrap_or("ready").to_string(),
                error: s.str_of("error").unwrap_or("").to_string(),
                warnings: parse_str_list(s.get("warnings")),
                added_at: now.clone(),
                page_count: s.get("page_count").and_then(|x| x.as_f64()).unwrap_or(0.0) as u32,
                chunk_count: s.get("chunk_count").and_then(|x| x.as_f64()).unwrap_or(0.0) as u32,
            });
        }

        let mut chunks: Vec<Chunk> = Vec::new();
        if let Some(arr) = body.get("chunks").and_then(|v| v.as_arr()) {
            for c in arr {
                let old_sid = c.str_of("source_id").unwrap_or("");
                let Some(new_sid) = id_map.get(old_sid).cloned() else { continue };
                chunks.push(Chunk {
                    id: new_id("chk"),
                    source_id: new_sid,
                    notebook_id: new_nb_id.clone(),
                    ordinal: c.get("ordinal").and_then(|x| x.as_f64()).unwrap_or(0.0) as u32,
                    text: c.str_of("text").unwrap_or("").to_string(),
                    locator: c.get("locator").map(Locator::from_json).unwrap_or(Locator::paragraph(0, 0, 0)),
                });
            }
        }

        let mut notes: Vec<Note> = Vec::new();
        if let Some(arr) = body.get("notes").and_then(|v| v.as_arr()) {
            for n in arr {
                notes.push(Note {
                    id: new_id("note"),
                    notebook_id: new_nb_id.clone(),
                    title: n.str_of("title").unwrap_or("note").to_string(),
                    body: n.str_of("body").unwrap_or("").to_string(),
                    source_ids: parse_str_list(n.get("source_ids"))
                        .into_iter()
                        .filter_map(|s| id_map.get(&s).cloned())
                        .collect(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                });
            }
        }

        let mut conversations: Vec<Conversation> = Vec::new();
        if let Some(arr) = body.get("conversations").and_then(|v| v.as_arr()) {
            for c in arr {
                let mut messages = Vec::new();
                if let Some(ms) = c.get("messages").and_then(|v| v.as_arr()) {
                    for m in ms {
                        messages.push(Message::from_json(m));
                    }
                }
                conversations.push(Conversation {
                    id: new_id("conv"),
                    notebook_id: new_nb_id.clone(),
                    title: c.str_of("title").unwrap_or("conversation").to_string(),
                    messages,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                });
            }
        }

        let mut artifacts: Vec<Artifact> = Vec::new();
        if let Some(arr) = body.get("artifacts").and_then(|v| v.as_arr()) {
            for a in arr {
                artifacts.push(Artifact {
                    id: new_id("art"),
                    notebook_id: new_nb_id.clone(),
                    kind: a.str_of("kind").unwrap_or("report").to_string(),
                    title: a.str_of("title").unwrap_or("artifact").to_string(),
                    created_at: now.clone(),
                    source_ids: parse_str_list(a.get("source_ids"))
                        .into_iter()
                        .filter_map(|s| id_map.get(&s).cloned())
                        .collect(),
                    payload: a.get("payload").cloned().unwrap_or(Value::Null),
                });
            }
        }

        let counts = (
            sources.len(),
            chunks.len(),
            notes.len(),
            conversations.len(),
            artifacts.len(),
        );
        let nb_out = new_nb_id.clone();
        self.store
            .write(|db| {
                db.notebooks.push(notebook);
                db.sources.extend(sources);
                db.chunks.extend(chunks);
                db.notes.extend(notes);
                db.conversations.extend(conversations);
                db.artifacts.extend(artifacts);
            })
            .map_err(|e| err(500, "storage", &e.to_string()))?;
        Ok(created(obj! {
            "notebook_id" => nb_out,
            "imported" => obj! {
                "sources" => counts.0 as u64,
                "chunks" => counts.1 as u64,
                "notes" => counts.2 as u64,
                "conversations" => counts.3 as u64,
                "artifacts" => counts.4 as u64,
            },
        }))
    }

    fn search(&self, req: &Request) -> Response {
        let q = req.query_get("q").unwrap_or("").to_string();
        let nb = req.query_get("notebook").map(|s| s.to_string());
        if q.trim().is_empty() {
            return err(422, "empty_query", "q parameter is required");
        }
        let chunks: Vec<Chunk> = self.store.read(|db| {
            db.chunks
                .iter()
                .filter(|c| nb.as_ref().map(|n| &c.notebook_id == n).unwrap_or(true))
                .cloned()
                .collect()
        });
        let hits = crate::retrieve::search(&chunks, &q, 20);
        let items: Vec<Value> = hits
            .iter()
            .map(|h| {
                let c = &chunks[h.chunk_index];
                obj! {
                    "chunk_id" => c.id.as_str(),
                    "source_id" => c.source_id.as_str(),
                    "notebook_id" => c.notebook_id.as_str(),
                    "score" => h.score,
                    "text" => c.text.chars().take(400).collect::<String>(),
                    "locator" => c.locator.to_json(),
                }
            })
            .collect();
        ok(obj! { "query" => q, "count" => items.len() as u64, "results" => Value::Arr(items) })
    }
}

// ------------------------------------------------------- providers / backup
impl Api {
    fn providers_path(&self) -> PathBuf {
        self.store.dir.join("providers.json")
    }

    pub fn load_providers(&self) -> Vec<ProviderConfig> {
        let Ok(text) = std::fs::read_to_string(self.providers_path()) else {
            return Vec::new();
        };
        let Ok(v) = json::parse(&text) else { return Vec::new() };
        let Some(arr) = v.get("providers").and_then(|x| x.as_arr()) else {
            return Vec::new();
        };
        arr.iter()
            .map(|p| ProviderConfig {
                id: p.str_of("id").unwrap_or("").to_string(),
                kind: p.str_of("kind").unwrap_or("extractive").to_string(),
                base_url: p.str_of("base_url").unwrap_or("").to_string(),
                model: p.str_of("model").unwrap_or("").to_string(),
                created_at: p.str_of("created_at").unwrap_or("").to_string(),
            })
            .collect()
    }

    fn save_providers(&self, list: &[ProviderConfig]) -> std::io::Result<()> {
        let body = obj! {
            "providers" => Value::Arr(list.iter().map(|p| obj! {
                "id" => p.id.as_str(),
                "kind" => p.kind.as_str(),
                "base_url" => p.base_url.as_str(),
                "model" => p.model.as_str(),
                "created_at" => p.created_at.as_str(),
            }).collect())
        };
        std::fs::write(self.providers_path(), json::to_string(&body))
    }

    fn provider_status(&self) -> Value {
        let list = self.load_providers();
        obj! {
            "configured" => list.len() as u64,
            "usable" => 0u64,
            "note" => "provider keys are stored on disk with 0600 permissions and never returned by the API; this build cannot make TLS requests, so the local extractive engine answers",
        }
    }

    fn list_providers(&self) -> Response {
        let list = self.load_providers();
        ok(obj! {
            "providers" => Value::Arr(list.iter().map(|p| {
                let key_set = providers::read_secret(&self.store.dir, &p.id).is_some();
                p.to_json(key_set)
            }).collect()),
            "note" => "keys are write-only: the API reports key_set but never the key itself",
        })
    }

    fn add_provider(&self, req: &Request) -> Result<Response, Response> {
        let body = parse_body(req)?;
        let kind = body.str_of("kind").unwrap_or("").to_string();
        let base_url = body.str_of("base_url").unwrap_or("").trim().to_string();
        let model = body.str_of("model").unwrap_or("").trim().to_string();
        let api_key = body.str_of("api_key").unwrap_or("").to_string();
        providers::validate(&kind, &base_url, &model, &api_key)
            .map_err(|e| err(422, "invalid_provider", &e))?;
        let mut list = self.load_providers();
        let cfg = ProviderConfig {
            id: new_id("prov"),
            kind,
            base_url,
            model,
            created_at: now_iso(),
        };
        if !api_key.trim().is_empty() {
            providers::write_secret(&self.store.dir, &cfg.id, &api_key)
                .map_err(|e| err(500, "storage", &e.to_string()))?;
        }
        let out = cfg.to_json(true);
        list.push(cfg);
        self.save_providers(&list)
            .map_err(|e| err(500, "storage", &e.to_string()))?;
        Ok(created(out))
    }

    fn delete_provider(&self, pid: &str) -> Response {
        let mut list = self.load_providers();
        let before = list.len();
        list.retain(|p| p.id != pid);
        if list.len() == before {
            return err(404, "not_found", "provider not found");
        }
        providers::delete_secret(&self.store.dir, pid);
        let _ = self.save_providers(&list);
        ok(obj! { "deleted" => true })
    }

    fn create_backup(&self) -> Result<Response, Response> {
        self.store.save().map_err(|e| err(500, "storage", &e.to_string()))?;
        let path = self
            .store
            .backup()
            .map_err(|e| err(500, "backup", &e.to_string()))?;
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Ok(created(obj! {
            "name" => path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            "bytes" => size,
        }))
    }

    fn list_backups(&self) -> Response {
        let dir = self.store.dir.join("backups");
        let mut items: Vec<Value> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .filter_map(|e| {
                        let meta = e.metadata().ok()?;
                        Some(obj! {
                            "name" => e.file_name().to_string_lossy().to_string(),
                            "bytes" => meta.len(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        items.sort_by_key(|v| v.str_of("name").unwrap_or("").to_string());
        ok(obj! { "backups" => Value::Arr(items) })
    }
}

fn parsed_warnings(store: &Store, sid: &str) -> Vec<Value> {
    store
        .read(|db| {
            db.sources
                .iter()
                .find(|s| s.id == sid)
                .map(|s| s.warnings.iter().map(|w| Value::from(w.as_str())).collect())
                .unwrap_or_default()
        })
}

fn parse_body(req: &Request) -> Result<Value, Response> {
    if req.body.is_empty() {
        return Ok(Value::obj());
    }
    let text = req.body_str();
    json::parse(&text).map_err(|e| err(400, "bad_json", &format!("invalid JSON body: {}", e)))
}

// --- serialisers shared with the store (kept local to avoid leaking internals)

fn nb_json(n: &Notebook) -> Value {
    obj! {
        "id" => n.id.as_str(),
        "title" => n.title.as_str(),
        "description" => n.description.as_str(),
        "instructions" => n.instructions.as_str(),
        "tags" => json_str_list(&n.tags),
        "archived" => n.archived,
        "created_at" => n.created_at.as_str(),
        "updated_at" => n.updated_at.as_str(),
    }
}

fn source_json(s: &Source) -> Value {
    obj! {
        "id" => s.id.as_str(),
        "notebook_id" => s.notebook_id.as_str(),
        "title" => s.title.as_str(),
        "kind" => s.kind.as_str(),
        "origin" => s.origin.as_str(),
        "status" => s.status.as_str(),
        "error" => s.error.as_str(),
        "warnings" => json_str_list(&s.warnings),
        "byte_len" => s.byte_len as u64,
        "page_count" => s.page_count as u64,
        "chunk_count" => s.chunk_count as u64,
        "added_at" => s.added_at.as_str(),
    }
}

fn note_json(n: &Note) -> Value {
    obj! {
        "id" => n.id.as_str(),
        "notebook_id" => n.notebook_id.as_str(),
        "title" => n.title.as_str(),
        "body" => n.body.as_str(),
        "source_ids" => json_str_list(&n.source_ids),
        "created_at" => n.created_at.as_str(),
        "updated_at" => n.updated_at.as_str(),
    }
}

fn artifact_json(a: &Artifact) -> Value {
    obj! {
        "id" => a.id.as_str(),
        "notebook_id" => a.notebook_id.as_str(),
        "kind" => a.kind.as_str(),
        "title" => a.title.as_str(),
        "created_at" => a.created_at.as_str(),
        "source_ids" => json_str_list(&a.source_ids),
        "payload" => a.payload.clone(),
    }
}

fn chunk_json(c: &Chunk) -> Value {
    obj! {
        "id" => c.id.as_str(),
        "source_id" => c.source_id.as_str(),
        "ordinal" => c.ordinal as u64,
        "text" => c.text.as_str(),
        "locator" => c.locator.to_json(),
    }
}

fn conv_json(c: &Conversation) -> Value {
    obj! {
        "id" => c.id.as_str(),
        "notebook_id" => c.notebook_id.as_str(),
        "title" => c.title.as_str(),
        "created_at" => c.created_at.as_str(),
        "updated_at" => c.updated_at.as_str(),
        "messages" => Value::Arr(c.messages.iter().map(|m| m.to_json()).collect()),
    }
}
