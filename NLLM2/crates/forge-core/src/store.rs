//! Durable state: an in-memory `Db` written through to JSON on disk.
//!
//! Why not SQLite: crates.io is unreachable (docs/DECISIONS.md D1), and the
//! Rust standard library has no database. A single JSON document with atomic
//! replace (write temp + rename) is what we can verify end-to-end. See
//! docs/DECISIONS.md D2 for the scale limit this implies.

use crate::domain::*;
use crate::json::{self, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Default, Clone)]
pub struct Db {
    pub notebooks: Vec<Notebook>,
    pub sources: Vec<Source>,
    pub chunks: Vec<Chunk>,
    pub conversations: Vec<Conversation>,
    pub notes: Vec<Note>,
    pub artifacts: Vec<Artifact>,
}

pub struct Store {
    pub dir: PathBuf,
    inner: Mutex<Db>,
}

impl Store {
    pub fn open(dir: impl AsRef<Path>) -> std::io::Result<Store> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        fs::create_dir_all(dir.join("backups"))?;
        let db = match fs::read_to_string(dir.join("db.json")) {
            Ok(text) => decode_db(&text).unwrap_or_default(),
            Err(_) => Db::default(),
        };
        let s = Store { dir, inner: Mutex::new(db) };
        if !s.path().exists() {
            s.save()?;
        }
        Ok(s)
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join("db.json")
    }

    pub fn read<R>(&self, f: impl FnOnce(&Db) -> R) -> R {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        f(&guard)
    }

    /// Mutate + persist. The write is atomic (tmp + rename) so a crash cannot
    /// leave a half-written database.
    pub fn write<R>(&self, f: impl FnOnce(&mut Db) -> R) -> std::io::Result<R> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let out = f(&mut guard);
        let text = json::to_pretty(&encode_db(&guard));
        let tmp = self.dir.join("db.json.tmp");
        fs::write(&tmp, text.as_bytes())?;
        fs::rename(&tmp, self.path())?;
        Ok(out)
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.write(|_| ())
    }

    /// Rolling backup used by the export/backup workflow and manual recovery.
    pub fn backup(&self) -> std::io::Result<PathBuf> {
        let name = format!("backups/db-{}.json", now_iso().replace([':', 'T', 'Z'], "-"));
        let dest = self.dir.join(&name);
        let text = self.read(|db| json::to_pretty(&encode_db(db)));
        fs::write(&dest, text.as_bytes())?;
        Ok(dest)
    }
}

fn encode_db(db: &Db) -> Value {
    let mut m = BTreeMap::new();
    m.insert("schema".into(), Value::from("forge.db/1"));
    m.insert("notebooks".into(), Value::Arr(db.notebooks.iter().map(nb_json).collect()));
    m.insert("sources".into(), Value::Arr(db.sources.iter().map(source_json).collect()));
    m.insert("chunks".into(), Value::Arr(db.chunks.iter().map(chunk_json).collect()));
    m.insert(
        "conversations".into(),
        Value::Arr(db.conversations.iter().map(conv_json).collect()),
    );
    m.insert("notes".into(), Value::Arr(db.notes.iter().map(note_json).collect()));
    m.insert("artifacts".into(), Value::Arr(db.artifacts.iter().map(artifact_json).collect()));
    Value::Obj(m)
}

fn decode_db(text: &str) -> Option<Db> {
    let v = json::parse(text).ok()?;
    let arr = |key: &str| v.get(key).and_then(|x| x.as_arr()).cloned().unwrap_or_default();
    Some(Db {
        notebooks: arr("notebooks").iter().map(nb_from).collect(),
        sources: arr("sources").iter().map(source_from).collect(),
        chunks: arr("chunks").iter().map(chunk_from).collect(),
        conversations: arr("conversations").iter().map(conv_from).collect(),
        notes: arr("notes").iter().map(note_from).collect(),
        artifacts: arr("artifacts").iter().map(artifact_from).collect(),
    })
}

fn nb_json(n: &Notebook) -> Value {
    let mut m = BTreeMap::new();
    m.insert("id".into(), Value::from(n.id.as_str()));
    m.insert("title".into(), Value::from(n.title.as_str()));
    m.insert("description".into(), Value::from(n.description.as_str()));
    m.insert("instructions".into(), Value::from(n.instructions.as_str()));
    m.insert("tags".into(), json_str_list(&n.tags));
    m.insert("archived".into(), Value::Bool(n.archived));
    m.insert("created_at".into(), Value::from(n.created_at.as_str()));
    m.insert("updated_at".into(), Value::from(n.updated_at.as_str()));
    Value::Obj(m)
}

fn nb_from(v: &Value) -> Notebook {
    Notebook {
        id: s(v, "id"),
        title: s(v, "title"),
        description: s(v, "description"),
        instructions: s(v, "instructions"),
        tags: parse_str_list(v.get("tags")),
        archived: v.get("archived").and_then(|b| b.as_bool()).unwrap_or(false),
        created_at: s(v, "created_at"),
        updated_at: s(v, "updated_at"),
    }
}

fn source_json(sr: &Source) -> Value {
    let mut m = BTreeMap::new();
    m.insert("id".into(), Value::from(sr.id.as_str()));
    m.insert("notebook_id".into(), Value::from(sr.notebook_id.as_str()));
    m.insert("title".into(), Value::from(sr.title.as_str()));
    m.insert("kind".into(), Value::from(sr.kind.as_str()));
    m.insert("origin".into(), Value::from(sr.origin.as_str()));
    m.insert("byte_len".into(), Value::from(sr.byte_len));
    m.insert("status".into(), Value::from(sr.status.as_str()));
    m.insert("error".into(), Value::from(sr.error.as_str()));
    m.insert("warnings".into(), json_str_list(&sr.warnings));
    m.insert("added_at".into(), Value::from(sr.added_at.as_str()));
    m.insert("page_count".into(), Value::from(sr.page_count as u64));
    m.insert("chunk_count".into(), Value::from(sr.chunk_count as u64));
    m.insert("content_hash".into(), Value::from(sr.content_hash.as_str()));
    m.insert("canonical_url".into(), Value::from(sr.canonical_url.as_str()));
    m.insert("fetched_at".into(), Value::from(sr.fetched_at.as_str()));
    m.insert("labels".into(), json_str_list(&sr.labels));
    let mut meta = BTreeMap::new();
    for (k, v) in &sr.meta {
        meta.insert(k.clone(), Value::from(v.as_str()));
    }
    m.insert("meta".into(), Value::Obj(meta));
    Value::Obj(m)
}

fn source_from(v: &Value) -> Source {
    Source {
        id: s(v, "id"),
        notebook_id: s(v, "notebook_id"),
        title: s(v, "title"),
        kind: s(v, "kind"),
        origin: s(v, "origin"),
        byte_len: v.get("byte_len").and_then(|x| x.as_f64()).unwrap_or(0.0) as usize,
        status: s(v, "status"),
        error: s(v, "error"),
        warnings: parse_str_list(v.get("warnings")),
        added_at: s(v, "added_at"),
        page_count: v.get("page_count").and_then(|x| x.as_f64()).unwrap_or(0.0) as u32,
        chunk_count: v.get("chunk_count").and_then(|x| x.as_f64()).unwrap_or(0.0) as u32,
        content_hash: s(v, "content_hash"),
        canonical_url: s(v, "canonical_url"),
        fetched_at: s(v, "fetched_at"),
        labels: parse_str_list(v.get("labels")),
        meta: v
            .get("meta")
            .and_then(|m| m.as_obj())
            .map(|m| {
                m.iter()
                    .map(|(k, val)| (k.clone(), val.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn chunk_json(c: &Chunk) -> Value {
    let mut m = BTreeMap::new();
    m.insert("id".into(), Value::from(c.id.as_str()));
    m.insert("source_id".into(), Value::from(c.source_id.as_str()));
    m.insert("notebook_id".into(), Value::from(c.notebook_id.as_str()));
    m.insert("ordinal".into(), Value::from(c.ordinal as u64));
    m.insert("text".into(), Value::from(c.text.as_str()));
    m.insert("locator".into(), c.locator.to_json());
    Value::Obj(m)
}

fn chunk_from(v: &Value) -> Chunk {
    Chunk {
        id: s(v, "id"),
        source_id: s(v, "source_id"),
        notebook_id: s(v, "notebook_id"),
        ordinal: v.get("ordinal").and_then(|x| x.as_f64()).unwrap_or(0.0) as u32,
        text: s(v, "text"),
        locator: v.get("locator").map(Locator::from_json).unwrap_or(Locator::paragraph(0, 0, 0)),
    }
}

fn conv_json(c: &Conversation) -> Value {
    let mut m = BTreeMap::new();
    m.insert("id".into(), Value::from(c.id.as_str()));
    m.insert("notebook_id".into(), Value::from(c.notebook_id.as_str()));
    m.insert("title".into(), Value::from(c.title.as_str()));
    m.insert("messages".into(), Value::Arr(c.messages.iter().map(|m| m.to_json()).collect()));
    m.insert("created_at".into(), Value::from(c.created_at.as_str()));
    m.insert("updated_at".into(), Value::from(c.updated_at.as_str()));
    Value::Obj(m)
}

fn conv_from(v: &Value) -> Conversation {
    Conversation {
        id: s(v, "id"),
        notebook_id: s(v, "notebook_id"),
        title: s(v, "title"),
        messages: v
            .get("messages")
            .and_then(|x| x.as_arr())
            .map(|a| a.iter().map(Message::from_json).collect())
            .unwrap_or_default(),
        created_at: s(v, "created_at"),
        updated_at: s(v, "updated_at"),
    }
}

fn note_json(n: &Note) -> Value {
    let mut m = BTreeMap::new();
    m.insert("id".into(), Value::from(n.id.as_str()));
    m.insert("notebook_id".into(), Value::from(n.notebook_id.as_str()));
    m.insert("title".into(), Value::from(n.title.as_str()));
    m.insert("body".into(), Value::from(n.body.as_str()));
    m.insert("source_ids".into(), json_str_list(&n.source_ids));
    m.insert("created_at".into(), Value::from(n.created_at.as_str()));
    m.insert("updated_at".into(), Value::from(n.updated_at.as_str()));
    Value::Obj(m)
}

fn note_from(v: &Value) -> Note {
    Note {
        id: s(v, "id"),
        notebook_id: s(v, "notebook_id"),
        title: s(v, "title"),
        body: s(v, "body"),
        source_ids: parse_str_list(v.get("source_ids")),
        created_at: s(v, "created_at"),
        updated_at: s(v, "updated_at"),
    }
}

fn artifact_json(a: &Artifact) -> Value {
    let mut m = BTreeMap::new();
    m.insert("id".into(), Value::from(a.id.as_str()));
    m.insert("notebook_id".into(), Value::from(a.notebook_id.as_str()));
    m.insert("kind".into(), Value::from(a.kind.as_str()));
    m.insert("title".into(), Value::from(a.title.as_str()));
    m.insert("created_at".into(), Value::from(a.created_at.as_str()));
    m.insert("source_ids".into(), json_str_list(&a.source_ids));
    m.insert("stale".into(), Value::Bool(a.stale));
    m.insert("payload".into(), a.payload.clone());
    Value::Obj(m)
}

fn artifact_from(v: &Value) -> Artifact {
    Artifact {
        id: s(v, "id"),
        notebook_id: s(v, "notebook_id"),
        kind: s(v, "kind"),
        title: s(v, "title"),
        created_at: s(v, "created_at"),
        source_ids: parse_str_list(v.get("source_ids")),
        stale: v.get("stale").and_then(|b| b.as_bool()).unwrap_or(false),
        payload: v.get("payload").cloned().unwrap_or(Value::Null),
    }
}

fn s(v: &Value, key: &str) -> String {
    v.str_of(key).unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("forge-test-{}-{}", tag, new_id("t")));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn persists_across_reopen() {
        let dir = tmpdir("persist");
        {
            let st = Store::open(&dir).unwrap();
            st.write(|db| {
                db.notebooks.push(Notebook {
                    id: "nb-1".into(),
                    title: "Research".into(),
                    description: String::new(),
                    instructions: String::new(),
                    tags: vec!["x".into()],
                    archived: false,
                    created_at: now_iso(),
                    updated_at: now_iso(),
                });
            })
            .unwrap();
        }
        let st2 = Store::open(&dir).unwrap();
        assert_eq!(st2.read(|db| db.notebooks.len()), 1);
        assert_eq!(st2.read(|db| db.notebooks[0].title.clone()), "Research");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn atomic_write_leaves_no_tmp_file() {
        let dir = tmpdir("atomic");
        let st = Store::open(&dir).unwrap();
        st.write(|db| db.notebooks.clear()).unwrap();
        assert!(!dir.join("db.json.tmp").exists());
        assert!(dir.join("db.json").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupt_db_does_not_panic() {
        let dir = tmpdir("corrupt");
        fs::write(dir.join("db.json"), "{not json").unwrap();
        let st = Store::open(&dir).unwrap(); // falls back to empty
        assert_eq!(st.read(|db| db.notebooks.len()), 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn backup_writes_a_copy() {
        let dir = tmpdir("backup");
        let st = Store::open(&dir).unwrap();
        let p = st.backup().unwrap();
        assert!(p.exists());
        fs::remove_dir_all(&dir).ok();
    }
}
