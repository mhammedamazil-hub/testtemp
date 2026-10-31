//! HTTP API: the single boundary between the web UI and the Rust core.
//!
//! Every handler validates input, enforces notebook scoping and returns honest
//! error codes. No handler invents data: unsupported operations return 4xx with
//! a machine-readable reason.

use crate::artifacts;
use crate::domain::*;
use crate::fetch::{self, FetchedMeta};
use crate::grounded;
use crate::hashes;
use crate::http::{percent_decode, Request, Response};
use crate::ingest;
use crate::json::{self, Value};
use crate::notebooklm;
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

/// A duplicate is not an error the user caused; report which source already exists.
fn err_dup(existing_id: &str, title: &str, url: &str) -> Response {
    let body = obj! {
        "error" => format!("this URL is already a source in this notebook: {}", title),
        "code" => "duplicate_source",
        "status" => 409u64,
        "existing_source_id" => existing_id,
        "url" => url,
    };
    Response::json(409, &body)
}

fn clip(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

/// Copy fetch provenance onto a source record.
fn apply_fetch_meta(src: &mut Source, fetched: &Option<FetchedMeta>) {
    if let Some(f) = fetched {
        src.canonical_url = f.canonical_url.clone();
        src.fetched_at = f.fetched_at.clone();
        src.meta.insert("http_status".into(), f.status.to_string());
        src.meta.insert("content_type".into(), f.content_type.clone());
        if f.redirects > 0 {
            src.meta.insert("redirects".into(), f.redirects.to_string());
        }
    }
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
            ("POST", ["api", "notes", nid, "to-source"]) => self.note_to_source(nid),
            ("POST", ["api", "notebooks", id, "artifacts"]) => self.create_artifact(id, req),
            ("GET", ["api", "notebooks", id, "artifacts"]) => Ok(ok(self.list_artifacts(id))),
            ("GET", ["api", "artifacts", aid]) => Ok(self.get_artifact(aid)),
            ("GET", ["api", "sources", sid]) => Ok(self.get_source(sid)),
            ("GET", ["api", "sources", sid, "chunks"]) => Ok(self.source_chunks(sid, req)),
            ("GET", ["api", "sources", sid, "tables"]) => Ok(self.source_tables(sid, req)),
            ("GET", ["api", "sources", sid, "guide"]) => Ok(self.source_guide(sid)),
            ("DELETE", ["api", "sources", sid]) => Ok(self.delete_source(sid)),
            ("PATCH", ["api", "sources", sid]) => self.update_source(sid, req),
            ("POST", ["api", "sources", sid, "refresh"]) => self.refresh_source(sid),
            ("GET", ["api", "notebooks", id, "export"]) => Ok(self.export_notebook(id)),
            ("POST", ["api", "import"]) => self.import_notebook(req),
            ("GET", ["api", "search"]) => Ok(self.search(req)),
            ("GET", ["api", "providers"]) => Ok(self.list_providers()),
            ("POST", ["api", "providers"]) => self.add_provider(req),
            ("DELETE", ["api", "providers", pid]) => Ok(self.delete_provider(pid)),
            ("POST", ["api", "backup"]) => self.create_backup(),
            ("GET", ["api", "backups"]) => Ok(self.list_backups()),
            ("GET", ["api", "notebooklm", "status"]) => Ok(ok(notebooklm::status())),
            ("POST", ["api", "notebooklm", "preview"]) => self.notebooklm_preview(req),
            _ => Err(err(404, "not_found", &format!("no route for {} {}", req.method, req.path))),
        };

        match result {
            Ok(r) => r,
            Err(r) => r,
        }
    }

    // ------------------------------------------------------------ health
    fn health(&self) -> Response {
        let (nb, src, chunks, conv, notes, arts, tables) = self.store.read(|db| {
            (db.notebooks.len(), db.sources.len(), db.chunks.len(), db.conversations.len(), db.notes.len(), db.artifacts.len(), db.tables.len())
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
                "tables" => tables as u64,
            },
        })
    }

    /// Honest, machine-readable capability matrix (drives UI badges).
    fn capabilities(&self) -> Value {
        obj! {
            "version" => VERSION,
            "tables" => obj! {
                "status" => "supported",
                "operations" => "sum, average, min, max, count; optional grouping by a column",
                "locator" => "cell (A1 range, e.g. Sales!C2:C6)",
                "note" => "computed over numeric cells only: empty cells and placeholders are excluded (never counted as zero), a column that mixes units is refused, and a cached column total is excluded to avoid double counting",
            },
            "sources" => obj! {
                "text" => obj!{ "status" => "supported", "locator" => "paragraph" },
                "markdown" => obj!{ "status" => "supported", "locator" => "paragraph+section" },
                "html" => obj!{ "status" => "supported", "locator" => "paragraph+section" },
                "csv" => obj!{ "status" => "supported", "locator" => "row+cell", "note" => "RFC 4180 quoting, delimiter sniffed; the grid is kept for aggregation" },
                "pdf" => obj!{ "status" => "supported", "locator" => "page", "note" => "text-layer PDFs; no OCR" },
                "url" => obj!{
                    "status" => if fetch::transport_available() { "supported" } else { "unsupported" },
                    "locator" => "section+paragraph",
                    "note" => "fetched with curl (no TLS crate is available), SSRF-guarded and DNS-pinned at every redirect hop",
                },
                "youtube" => obj!{ "status" => "unsupported", "reason" => "needs a transcript extractor (yt-dlp) and an ASR pipeline; neither is installed" },
                "audio_transcript" => obj!{ "status" => "unsupported", "reason" => "no ASR engine installed (see docs/RESEARCH_BLOCKERS.md)" },
                "docx" => obj!{ "status" => "supported", "locator" => "paragraph", "note" => "WordprocessingML text; images ignored" },
                "pptx" => obj!{ "status" => "supported", "locator" => "slide", "note" => "slide text; speaker notes not indexed" },
                "xlsx" => obj!{ "status" => "supported", "locator" => "row+cell", "note" => "every worksheet, shared/inline strings, cached formulas flagged; hidden sheets stored but never cited" },
                "audio" => obj!{ "status" => "unsupported", "reason" => "no ASR engine available offline" },
                "video" => obj!{ "status" => "unsupported", "reason" => "no ASR/FFmpeg pipeline in this build" },
                "image_ocr" => obj!{ "status" => "unsupported", "reason" => "no OCR engine available" },
            },
            "providers" => self.provider_status(),
            "notebooklm_bridge" => obj! {
                "status" => "protocol_implemented_target_unreachable",
                "rpc_implemented" => 14u64,
                "reason" => "wire protocol rebuilt from public reverse engineering and unit-tested; no route to notebooklm.google.com and no TLS client in this build",
                "endpoint" => "/api/notebooklm/status",
            },
            "web_fetch" => obj! {
                "transport" => if fetch::transport_available() { "curl" } else { "none" },
                "private_networks" => if security::allow_private() { "ALLOWED by FORGE_FETCH_ALLOW_PRIVATE (test mode)" } else { "blocked" },
                "max_bytes" => fetch::MAX_FETCH_BYTES as u64,
                "redirects" => fetch::MAX_HOPS as u64,
                "timeout_seconds" => fetch::FETCH_TIMEOUT.as_secs(),
                "dns" => "resolved by Forge, pinned into curl with --resolve, re-validated on every redirect",
            },
            "binary_analysis" => obj! {
                "apk_manifest" => obj!{ "status" => "supported", "note" => "binary AndroidManifest.xml decoded: package, sdk, permissions, exported components" },
                "apk_dex_inventory" => obj!{ "status" => "supported", "note" => "classes/methods/strings + crypto, network, dynamic-code, root-check and cleartext-URL references" },
                "apk_java_decompilation" => obj!{ "status" => "unsupported", "reason" => "no JVM, apktool, jadx or dex2jar in this image; DEX is analysed structurally, not translated to Java" },
                "pe_headers" => obj!{ "status" => "supported", "note" => "machine, subsystem, sections, per-section entropy, imports, exports, overlay" },
                "pe_dotnet" => obj!{ "status" => "supported", "note" => "CLR header, BSJB metadata version, #Strings and #US user strings" },
                "disassembly" => obj!{ "status" => "unsupported", "reason" => "instruction-level disassembly needs Ghidra/IDA/objdump-class engine; not installed here" },
                "endpoint" => "/api/binary/inspect",
            },
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
                "sources" => Value::Arr(db.sources.iter().filter(|s| s.notebook_id == id).map(|s| source_json(s, &db.tables)).collect()),
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
                db.tables.retain(|t| t.notebook_id != id);
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
                "sources" => Value::Arr(db.sources.iter().filter(|s| s.notebook_id == nb).map(|s| source_json(s, &db.tables)).collect())
            }
        })
    }

    fn get_source(&self, sid: &str) -> Response {
        self.store.read(|db| match db.sources.iter().find(|s| s.id == sid) {
            None => err(404, "not_found", "source not found"),
            Some(s) => ok(source_json(s, &db.tables)),
        })
    }

    /// Rename a source or change its labels. Never touches extracted content.
    fn update_source(&self, sid: &str, req: &Request) -> Result<Response, Response> {
        let body = parse_body(req)?;
        let title = body.str_of("title").map(|t| t.trim().to_string());
        let labels: Option<Vec<String>> = body.get("labels").map(|_| parse_str_list(body.get("labels")));
        if title.is_none() && labels.is_none() {
            return Err(err(422, "nothing_to_update", "send title and/or labels"));
        }
        if let Some(t) = &title {
            if t.is_empty() {
                return Err(err(422, "empty_title", "title cannot be empty"));
            }
            if t.chars().count() > 200 {
                return Err(err(422, "title_too_long", "title is limited to 200 characters"));
            }
        }
        let found = self.store.write(|db| {
            let Some(src) = db.sources.iter_mut().find(|s| s.id == sid) else {
                return None;
            };
            if let Some(t) = title {
                src.title = t;
            }
            if let Some(mut l) = labels {
                l.retain(|x| !x.trim().is_empty());
                l.dedup();
                l.truncate(20);
                src.labels = l;
            }
            Some(source_json(src, &db.tables))
        })
        .map_err(|e| err(500, "storage", &e.to_string()))?;
        match found {
            None => Err(err(404, "not_found", "source not found")),
            Some(v) => Ok(ok(v)),
        }
    }

    /// Re-fetch a URL source and re-index it only if the content actually changed.
    ///
    /// Artifacts that were generated before the change are flagged as potentially
    /// stale (their citations still resolve, but the source text moved).
    fn refresh_source(&self, sid: &str) -> Result<Response, Response> {
        let current = self.store.read(|db| db.sources.iter().find(|s| s.id == sid).cloned());
        let Some(src) = current else {
            return Err(err(404, "not_found", "source not found"));
        };
        if src.canonical_url.is_empty() {
            return Err(err(
                422,
                "not_refreshable",
                "only sources imported from a URL can be refreshed; uploads and pastes are fixed snapshots",
            ));
        }
        let fetched = fetch::fetch(&src.canonical_url).map_err(|e| {
            let status = match e {
                fetch::FetchError::Rejected(_) => 422,
                fetch::FetchError::NoTransport(_) => 501,
                fetch::FetchError::Limit(_) => 413,
                fetch::FetchError::Failed(_) => 502,
            };
            err(status, e.code(), &e.message())
        })?;

        let new_hash = hashes::sha256(&fetched.body);
        if new_hash == src.content_hash {
            let v = self.store.write(|db| {
                let s = db.sources.iter_mut().find(|s| s.id == sid)?;
                s.fetched_at = now_iso();
                s.meta.insert("http_status".into(), fetched.status.to_string());
                Some(source_json(s, &db.tables))
            })
            .ok()
            .flatten();
            return Ok(ok(obj! {
                "changed" => false,
                "source" => v.unwrap_or(Value::Null),
                "message" => "content is unchanged; nothing was re-indexed",
            }));
        }

        let parsed = ingest::extract(&src.kind, &src.title, &fetched.body)
            .map_err(|e| err(422, "extract_failed", &e))?;
        if parsed.blocks.is_empty() {
            return Err(err(422, "empty_extraction", "the refreshed page has no readable text; the old source was kept"));
        }
        let chunk_count = parsed.blocks.len();
        let kind = src.kind.clone();
        let chunks: Vec<Chunk> = parsed
            .blocks
            .into_iter()
            .enumerate()
            .map(|(i, b)| Chunk {
                id: new_id("chk"),
                source_id: sid.to_string(),
                notebook_id: src.notebook_id.clone(),
                ordinal: i as u32,
                text: b.text,
                locator: b.locator,
            })
            .collect();
        let stale = self.store.write(|db| {
            db.chunks.retain(|c| c.source_id != sid);
            db.chunks.extend(chunks);
            let mut stale: Vec<String> = Vec::new();
            if let Some(s) = db.sources.iter_mut().find(|s| s.id == sid) {
                s.content_hash = new_hash.clone();
                s.fetched_at = now_iso();
                s.chunk_count = chunk_count as u32;
                s.page_count = parsed.page_count;
                s.byte_len = fetched.body.len();
                s.warnings = parsed.warnings.clone();
                s.meta.insert("http_status".into(), fetched.status.to_string());
                let nb = s.notebook_id.clone();
                let id = s.id.clone();
                for a in db.artifacts.iter_mut() {
                    if a.notebook_id == nb {
                        let used = a.source_ids.iter().any(|x| x == &id)
                            || !a.source_ids.is_empty();
                        if used {
                            a.stale = true;
                            stale.push(a.id.clone());
                        }
                    }
                }
            }
            stale
        });
        Ok(ok(obj! {
            "changed" => true,
            "kind" => kind.as_str(),
            "chunk_count" => chunk_count as u64,
            "stale_artifacts" => Value::Arr(stale.unwrap_or_default().iter().map(|s| Value::from(s.as_str())).collect()),
        }))
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

    /// Structured tables of one source, with real cell references.
    ///
    /// `?rows=N` caps how many rows are returned per sheet (default 200); `row_count`
    /// always reports the true size so a truncated view cannot be mistaken for the data.
    fn source_tables(&self, sid: &str, req: &Request) -> Response {
        let limit = req
            .query_get("rows")
            .and_then(|r| r.parse::<usize>().ok())
            .map(|n| n.clamp(1, 2000))
            .unwrap_or(200);
        self.store.read(|db| {
            let known = db.sources.iter().any(|s| s.id == sid);
            if !known {
                return err(404, "not_found", "source not found");
            }
            let mut tables: Vec<&crate::tables::Table> =
                db.tables.iter().filter(|t| t.source_id == sid).collect();
            tables.sort_by(|a, b| a.sheet.cmp(&b.sheet));
            ok(obj! {
                "source_id" => sid,
                "count" => tables.len() as u64,
                "tables" => Value::Arr(tables.iter().map(|t| t.json(limit)).collect()),
            })
        })
    }

    /// Source Guide: an extractive orientation to one source.
    ///
    /// Every sentence is quoted from a chunk and carries that chunk's id and locator.
    /// `generated_by` is always `extractive` — no model is called, so no paraphrase is
    /// presented as one.
    fn source_guide(&self, sid: &str) -> Response {
        let built = self.store.read(|db| {
            let Some(source) = db.sources.iter().find(|s| s.id == sid) else {
                return Err(err(404, "not_found", "source not found"));
            };
            let chunks: Vec<Chunk> = db.chunks.iter().filter(|c| c.source_id == sid).cloned().collect();
            let tables: Vec<crate::tables::Table> =
                db.tables.iter().filter(|t| t.source_id == sid).cloned().collect();
            match crate::guide::build(source, &chunks, &tables) {
                Ok(g) => Ok(ok(g.to_json())),
                Err(e) => Err(err(
                    409,
                    "guide_unavailable",
                    &format!("{} — a guide needs indexed passages to quote", e),
                )),
            }
        });
        built.unwrap_or_else(|e| e)
    }

    fn delete_source(&self, sid: &str) -> Response {
        let removed = self
            .store
            .write(|db| {
                let before = db.sources.len();
                db.sources.retain(|s| s.id != sid);
                let existed = db.sources.len() != before;
                db.chunks.retain(|c| c.source_id != sid);
                db.tables.retain(|t| t.source_id != sid);
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

    fn add_url_source(&self, nb: &str, req: &Request) -> Result<Response, Response> {
        let body = parse_body(req)?;
        let raw = body.str_of("url").unwrap_or("").trim().to_string();
        if raw.is_empty() {
            return Err(err(422, "missing_url", "url is required"));
        }
        let parsed = security::parse_url(&raw).map_err(|e| err(422, "url_rejected", &e))?;
        let canonical = parsed.canonical();

        // Duplicate detection happens before any network traffic.
        if let Some(existing) = self.store.read(|db| {
            db.sources
                .iter()
                .find(|s| s.notebook_id == nb && !s.canonical_url.is_empty() && s.canonical_url == canonical)
                .map(|s| (s.id.clone(), s.title.clone()))
        }) {
            return Err(err_dup(&existing.0, &existing.1, &canonical));
        }

        let fetched = fetch::fetch(&raw).map_err(|e| {
            let status = match e {
                fetch::FetchError::Rejected(_) => 422,
                fetch::FetchError::NoTransport(_) => 501,
                fetch::FetchError::Limit(_) => 413,
                fetch::FetchError::Failed(_) => 502,
            };
            err(status, e.code(), &e.message())
        })?;

        // Canonical URL may have changed after redirects; re-check for a duplicate.
        if fetched.canonical_url != canonical {
            if let Some(existing) = self.store.read(|db| {
                db.sources
                    .iter()
                    .find(|s| s.notebook_id == nb && s.canonical_url == fetched.canonical_url)
                    .map(|s| (s.id.clone(), s.title.clone()))
            }) {
                return Err(err_dup(&existing.0, &existing.1, &fetched.canonical_url));
            }
        }

        let kind = fetched.ingest_kind();
        let title = self.derive_title(&fetched, kind, parsed.host.as_str());
        self.ingest_source(
            Some(FetchedMeta {
                canonical_url: fetched.canonical_url.clone(),
                fetched_at: now_iso(),
                status: fetched.status,
                content_type: fetched.media_type(),
                redirects: fetched.redirects.len(),
            }),
            BTreeMap::new(),
            nb,
            &title,
            kind,
            "url",
            &fetched.body,
        )
    }

    /// Title for a fetched source: page title, then meta description, then the host.
    fn derive_title(&self, fetched: &fetch::Fetched, kind: &str, host: &str) -> String {
        if kind == "html" {
            let raw = ingest::decode_text(&fetched.body);
            if let Some(t) = ingest::text::html_title(&raw) {
                return clip(&t, 120);
            }
            if let Some(d) = ingest::text::html_description(&raw) {
                return clip(&d, 120);
            }
        }
        let path = fetched
            .final_url
            .split("://")
            .nth(1)
            .unwrap_or(&fetched.final_url)
            .to_string();
        let name = path.rsplit('/').next().unwrap_or("").to_string();
        if !name.is_empty() && name.contains('.') {
            format!("{} — {}", clip(&name, 60), host)
        } else {
            path
        }
    }

    fn ingest_bytes(
        &self,
        nb: &str,
        title: &str,
        kind: &str,
        origin: &str,
        bytes: &[u8],
        _mode: &str,
    ) -> Result<Response, Response> {
        self.ingest_source(None, BTreeMap::new(), nb, title, kind, origin, bytes)
    }

    fn ingest_source(
        &self,
        fetched: Option<FetchedMeta>,
        extra_meta: BTreeMap<String, String>,
        nb: &str,
        title: &str,
        kind: &str,
        origin: &str,
        bytes: &[u8],
    ) -> Result<Response, Response> {
        if origin == "upload" && kind == "url" {
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
                let mut failed = Source {
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
                    content_hash: hashes::sha256(bytes),
                    ..Default::default()
                };
                apply_fetch_meta(&mut failed, &fetched);
                let _ = self.store.write(|db| db.sources.push(failed));
                return Err(err(422, "extract_failed", &e));
            }
        };

        if parsed.blocks.is_empty() {
            let msg = "no readable text could be extracted from this file";
            let mut failed = Source {
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
                content_hash: hashes::sha256(bytes),
                ..Default::default()
            };
            apply_fetch_meta(&mut failed, &fetched);
            let _ = self.store.write(|db| db.sources.push(failed));
            return Err(err(422, "empty_extraction", msg));
        }

        let chunk_count = parsed.blocks.len();
        let mut source = Source {
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
            content_hash: hashes::sha256(bytes),
            ..Default::default()
        };
        apply_fetch_meta(&mut source, &fetched);
        for (k, v) in &extra_meta {
            source.meta.insert(k.clone(), v.clone());
        }
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

        // Structured grids are stored verbatim alongside the chunk text: an answer that
        // totals a column must be able to re-read the same cells after a restart.
        let mut tables = parsed.tables;
        for t in tables.iter_mut() {
            t.source_id = source_id.clone();
            t.notebook_id = nb.to_string();
            t.id = format!("tbl_{}", crate::domain::short_hash(&format!("{}|{}", t.source_id, t.sheet)));
        }
        let table_summary: Vec<Value> = tables
            .iter()
            .map(|t| obj! {
                "id" => t.id.as_str(),
                "sheet" => t.sheet.as_str(),
                "hidden" => t.hidden,
                "columns" => Value::Arr(t.headers.iter().map(|h| Value::from(h.as_str())).collect()),
                "row_count" => t.rows.len() as u64,
            })
            .collect();
        let table_count = tables.len() as u64;
        let table_row_count: u64 = tables.iter().map(|t| t.rows.len() as u64).sum();
        if !tables.is_empty() {
            self.store
                .write(|db| db.tables.extend(tables))
                .map_err(|e| err(500, "storage", &e.to_string()))?;
        }

        let fetched_url = fetched.as_ref().map(|f| f.canonical_url.clone()).unwrap_or_default();
        let fetched_status = fetched.as_ref().map(|f| f.status as u64).unwrap_or(0);
        let mut meta_json = BTreeMap::new();
        for (k, v) in &extra_meta {
            meta_json.insert(k.clone(), Value::from(v.as_str()));
        }
        Ok(created(obj! {
            "source_id" => source_id.clone(),
            "title" => title,
            "kind" => kind,
            "meta" => Value::Obj(meta_json),
            "url" => fetched_url,
            "http_status" => fetched_status,
            "status" => "ready",
            "chunk_count" => chunk_count as u64,
            "page_count" => self.store.read(|db| db.sources.iter().find(|s| s.id == source_id).map(|s| s.page_count).unwrap_or(0)) as u64,
            "warnings" => Value::Arr(parsed_warnings(&self.store, &sid_for_warn)),
            "preview" => Value::Arr(for_response),
            "table_count" => table_count,
            "table_row_count" => table_row_count,
            "tables" => Value::Arr(table_summary),
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

        // Presentation controls. Both are validated: an unknown value is rejected rather
        // than silently replaced by the default, so the UI can never claim a style it got.
        let style_raw = body.str_of("style").unwrap_or("").to_string();
        let length_raw = body.str_of("length").unwrap_or("").to_string();
        let Some(style) = grounded::Style::parse(&style_raw) else {
            return Err(err(
                422,
                "bad_style",
                &format!("unknown style \"{}\" (use evidence, concise or detailed)", style_raw),
            ));
        };
        let Some(length) = grounded::Length::parse(&length_raw) else {
            return Err(err(
                422,
                "bad_length",
                &format!("unknown length \"{}\" (use brief, standard or thorough)", length_raw),
            ));
        };
        let answer_opts = grounded::AnswerOptions { style, length };

        let selected: Vec<String> = parse_str_list(body.get("source_ids"));
        let instructions = self.store.read(|db| {
            db.notebooks
                .iter()
                .find(|n| n.id == nb)
                .map(|n| n.instructions.clone())
                .unwrap_or_default()
        });

        let (chunks, sources, tables) = self.store.read(|db| {
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
            let tables: Vec<crate::tables::Table> = db
                .tables
                .iter()
                .filter(|t| t.notebook_id == nb && !t.hidden)
                .filter(|t| selected.is_empty() || selected.contains(&t.source_id))
                .cloned()
                .collect();
            (chunks, sources, tables)
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

        // A structured question ("total revenue by region") is answered by computing over
        // real cells. This runs before text retrieval because the text path can only ever
        // quote a row, not add one up.
        let computed = crate::tables::try_answer(&question, &tables, &sources);
        let (answer, problems) = match computed {
            Some(a) => {
                let problems = crate::tables::validate(&a, &tables);
                (a, problems)
            }
            None => {
                // Our own engine is called directly so style/length are honoured; a
                // provider (when one is reachable) only receives the citation budget.
                let a = if engine.name() == "extractive" {
                    grounded::answer_with(&question, &chunks, &sources, answer_opts)
                } else {
                    engine
                        .answer(&question, &chunks, &sources, answer_opts.length.max_citations())
                        .unwrap_or_else(|_| {
                            grounded::answer_with(&question, &chunks, &sources, answer_opts)
                        })
                };
                let problems = grounded::validate_citations(&a.citations, &chunks);
                (a, problems)
            }
        };

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
            "settings" => obj! {
                "style" => answer_opts.style.as_str(),
                "length" => answer_opts.length.as_str(),
                "max_citations" => answer_opts.length.max_citations() as u64,
            },
            "coverage" => answer.coverage as u64,
            "citation_problems" => Value::Arr(problems.iter().map(|p| Value::from(p.as_str())).collect()),
            "sources_considered" => sources.len() as u64,
            "chunks_considered" => chunks.len() as u64,
            "tables_considered" => tables.len() as u64,
            "computed" => answer.engine == "table",
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
    /// Turn a note into a source so it can be selected for chat and artifacts.
    ///
    /// The reference product lists notes among the sources that participate in Q&A;
    /// Forge implements that by conversion rather than by a second retrieval path, so
    /// a note and a pasted source behave identically (citations, viewer, export).
    /// Converting twice is refused rather than silently duplicating.
    fn note_to_source(&self, nid: &str) -> Result<Response, Response> {
        let note = self
            .store
            .read(|db| db.notes.iter().find(|n| n.id == nid).cloned());
        let Some(note) = note else {
            return Err(err(404, "not_found", "note not found"));
        };
        if note.body.trim().is_empty() {
            return Err(err(422, "empty_note", "the note has no content to convert"));
        }
        let already = self.store.read(|db| {
            db.sources
                .iter()
                .find(|s| s.meta_of("converted_from") == note.id)
                .map(|s| (s.id.clone(), s.title.clone()))
        });
        if let Some((sid, title)) = already {
            return Err(err(
                409,
                "already_a_source",
                &format!("this note is already the source \"{}\" ({})", title, sid),
            ));
        }
        let title = if note.title.trim().is_empty() {
            format!("Note from {}", &note.updated_at.chars().take(10).collect::<String>())
        } else {
            note.title.clone()
        };
        let mut meta = BTreeMap::new();
        meta.insert("converted_from".to_string(), note.id.clone());
        meta.insert("converted_at".to_string(), now_iso());
        let resp = self.ingest_source(
            None,
            meta,
            &note.notebook_id,
            &clip(&title, 120),
            "text",
            "note",
            note.body.as_bytes(),
        )?;
        Ok(resp)
    }

    fn create_artifact(&self, nb: &str, req: &Request) -> Result<Response, Response> {
        let body = parse_body(req)?;
        let kind = body.str_of("kind").unwrap_or("").to_string();
        let topic = body.str_of("topic").unwrap_or("").to_string();
        // Presentation controls. Both are validated: an unknown value is rejected rather
        // than silently replaced by the default, so the UI can never claim a style it got.
        let style_raw = body.str_of("style").unwrap_or("").to_string();
        let length_raw = body.str_of("length").unwrap_or("").to_string();
        let Some(style) = grounded::Style::parse(&style_raw) else {
            return Err(err(
                422,
                "bad_style",
                &format!("unknown style \"{}\" (use evidence, concise or detailed)", style_raw),
            ));
        };
        let Some(length) = grounded::Length::parse(&length_raw) else {
            return Err(err(
                422,
                "bad_length",
                &format!("unknown length \"{}\" (use brief, standard or thorough)", length_raw),
            ));
        };
        let answer_opts = grounded::AnswerOptions { style, length };

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
            stale: false,
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
                    "source_ids" => json_str_list(&a.source_ids),
                    "stale" => a.stale,
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
                "sources" => Value::Arr(db.sources.iter().filter(|s| s.notebook_id == nb).map(|s| source_json(s, &db.tables)).collect()),
                "chunks" => Value::Arr(db.chunks.iter().filter(|c| c.notebook_id == nb).map(chunk_json).collect()),
                "conversations" => Value::Arr(db.conversations.iter().filter(|c| c.notebook_id == nb).map(conv_json).collect()),
                "notes" => Value::Arr(db.notes.iter().filter(|n| n.notebook_id == nb).map(note_json).collect()),
                "artifacts" => Value::Arr(db.artifacts.iter().filter(|a| a.notebook_id == nb).map(artifact_json).collect()),
                // Grids travel with the notebook: without them a re-imported spreadsheet
                // source would keep its rows as text but lose every cell reference.
                "tables" => Value::Arr(db.tables.iter().filter(|t| t.notebook_id == nb).map(crate::store::table_json).collect()),
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
                content_hash: s.str_of("content_hash").unwrap_or("").to_string(),
                canonical_url: s.str_of("canonical_url").unwrap_or("").to_string(),
                fetched_at: s.str_of("fetched_at").unwrap_or("").to_string(),
                labels: parse_str_list(s.get("labels")),
                meta: s
                    .get("meta")
                    .and_then(|m| m.as_obj())
                    .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
                    .unwrap_or_default(),
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
            stale: false,
                });
            }
        }

        let mut tables: Vec<crate::tables::Table> = Vec::new();
        if let Some(arr) = body.get("tables").and_then(|v| v.as_arr()) {
            for t in arr {
                let Some(mut table) = crate::store::table_from(t) else { continue };
                let Some(new_sid) = id_map.get(&table.source_id).cloned() else { continue };
                table.source_id = new_sid;
                table.notebook_id = new_nb_id.clone();
                table.id = format!(
                    "tbl_{}",
                    crate::domain::short_hash(&format!("{}|{}", table.source_id, table.sheet))
                );
                tables.push(table);
            }
        }

        let counts = (
            sources.len(),
            chunks.len(),
            notes.len(),
            conversations.len(),
            artifacts.len(),
            tables.len(),
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
                db.tables.extend(tables);
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
                "tables" => counts.5 as u64,
            },
        }))
    }

    /// Build the real NotebookLM wire request for an action and return it.
    ///
    /// This is deliberately a *preview*: it proves the protocol construction is
    /// implemented and inspectable, and refuses to claim any request was sent.
    fn notebooklm_preview(&self, req: &Request) -> Result<Response, Response> {
        let body = parse_body(req)?;
        let action = body.str_of("action").unwrap_or("").to_string();
        let notebook_id = body.str_of("notebook_id").unwrap_or("nb-example").to_string();
        let creds = notebooklm::Credentials {
            cookies: "<your Cookie header>".into(),
            at: "<your at token>".into(),
        };

        let (rpc, params) = match action.as_str() {
            "add_url" => {
                let url = body.str_of("url").unwrap_or("https://example.com/article");
                match security::check_url(url) {
                    security::UrlVerdict::Rejected(why) => {
                        return Err(err(422, "url_rejected", &format!("URL refused: {}", why)))
                    }
                    security::UrlVerdict::Allowed(clean) => (
                        notebooklm::RPC_ADD_SOURCE,
                        notebooklm::add_url_source(&notebook_id, &clean),
                    ),
                }
            }
            "add_text" => (
                notebooklm::RPC_ADD_SOURCE,
                notebooklm::add_text_source(
                    &notebook_id,
                    body.str_of("title").unwrap_or("Pasted note"),
                    body.str_of("text").unwrap_or(""),
                ),
            ),
            "add_youtube" => (
                notebooklm::RPC_ADD_SOURCE,
                notebooklm::add_youtube_source(
                    &notebook_id,
                    body.str_of("url").unwrap_or("https://youtu.be/example"),
                ),
            ),
            "register_file" => (
                notebooklm::RPC_ADD_SOURCE_FILE,
                notebooklm::register_file_source(
                    &notebook_id,
                    body.str_of("filename").unwrap_or("document.pdf"),
                ),
            ),
            "artifact" => {
                let kind = match body.str_of("kind").unwrap_or("audio") {
                    "audio" => notebooklm::ArtifactKind::Audio,
                    "report" => notebooklm::ArtifactKind::Report,
                    "video" => notebooklm::ArtifactKind::Video,
                    "quiz" => notebooklm::ArtifactKind::QuizFlashcards,
                    "mindmap" => notebooklm::ArtifactKind::MindMap,
                    "slides" => notebooklm::ArtifactKind::SlideDeck,
                    "infographic" => notebooklm::ArtifactKind::Infographic,
                    "table" => notebooklm::ArtifactKind::DataTable,
                    other => {
                        return Err(err(422, "bad_kind", &format!("unknown artifact kind '{}'", other)))
                    }
                };
                let ids: Vec<String> = parse_str_list(body.get("source_ids"));
                let ids = if ids.is_empty() { vec!["<source-id>".to_string()] } else { ids };
                let mut ar = notebooklm::ArtifactRequest::new(kind, &notebook_id, &ids);
                ar.instructions = body.str_of("instructions").unwrap_or("");
                if body.str_of("variant") == Some("quiz") {
                    ar.study_variant = notebooklm::StudyVariant::Quiz;
                }
                (notebooklm::RPC_CREATE_ARTIFACT, ar.params())
            }
            "ask" => {
                let wire = notebooklm::chat_request(
                    &notebook_id,
                    body.str_of("question").unwrap_or("What do the sources say?"),
                    &creds,
                );
                return Ok(ok(obj! {
                    "action" => "ask",
                    "sent" => false,
                    "rpc" => "GenerateFreeFormStreamed",
                    "method" => wire.method,
                    "url" => wire.url,
                    "headers" => Value::Arr(wire.headers.iter().map(|(k, v)| obj! {
                        "name" => k.as_str(),
                        "value" => v.as_str(),
                    }).collect()),
                    "body" => wire.body,
                    "why_not_sent" => notebooklm::UNAVAILABLE,
                }));
            }
            other => {
                return Err(err(
                    422,
                    "bad_action",
                    &format!(
                        "unknown action '{}'; use add_url, add_text, add_youtube, register_file, artifact or ask",
                        other
                    ),
                ))
            }
        };

        let wire = notebooklm::batchexecute_request(rpc, &params, &creds);
        Ok(ok(obj! {
            "action" => action.as_str(),
            "sent" => false,
            "rpc" => rpc.name,
            "rpc_id" => rpc.id,
            "corroboration" => rpc.corroboration as u64,
            "method" => wire.method,
            "url" => wire.url,
            "headers" => Value::Arr(wire.headers.iter().map(|(k, v)| obj! {
                "name" => k.as_str(),
                "value" => v.as_str(),
            }).collect()),
            "body" => wire.body,
            "params" => params,
            "why_not_sent" => notebooklm::UNAVAILABLE,
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

fn source_json(s: &Source, tables: &[crate::tables::Table]) -> Value {
    let mine: Vec<&crate::tables::Table> = tables.iter().filter(|t| t.source_id == s.id).collect();
    let table_rows: u64 = mine.iter().map(|t| t.rows.len() as u64).sum();
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
        "content_hash" => s.content_hash.as_str(),
        "canonical_url" => s.canonical_url.as_str(),
        "fetched_at" => s.fetched_at.as_str(),
        "labels" => json_str_list(&s.labels),
        "meta" => {
            let mut m = BTreeMap::new();
            for (k, v) in &s.meta {
                m.insert(k.clone(), Value::from(v.as_str()));
            }
            Value::Obj(m)
        },
        "table_count" => mine.len() as u64,
        "table_row_count" => table_rows,
        "table_sheets" => Value::Arr(mine.iter().map(|t| obj! {
            "id" => t.id.as_str(),
            "sheet" => t.sheet.as_str(),
            "row_count" => t.rows.len() as u64,
            "hidden" => t.hidden,
        }).collect()),
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
        "stale" => a.stale,
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
