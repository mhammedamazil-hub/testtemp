//! Model-provider seam (bring-your-own-key) and the always-available local engine.
//!
//! Design contract:
//! * `Answerer` is the only way answers are produced.
//! * The **extractive** answerer always exists and needs no key or network.
//! * A remote provider can be *configured*, but it is only *used* when it reports
//!   itself available. In this build HTTPS is unavailable, so a configured
//!   provider is reported as configured-but-unusable instead of being silently
//!   ignored or faked. The fallback is stated in every response (`engine`,
//!   `engine_note`).
//!
//! API keys are never returned by any endpoint; only `key_set` is exposed.

use crate::domain::{Chunk, Source};
use crate::grounded::{self, Answer};
use crate::json::Value;
use crate::obj;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderKind {
    Extractive,
    OpenAiCompatible,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::Extractive => "extractive",
            ProviderKind::OpenAiCompatible => "openai_compatible",
        }
    }
    pub fn parse(s: &str) -> Option<ProviderKind> {
        match s {
            "extractive" => Some(ProviderKind::Extractive),
            "openai_compatible" | "openai" | "byok" => Some(ProviderKind::OpenAiCompatible),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub id: String,
    pub kind: String,
    pub base_url: String,
    pub model: String,
    pub created_at: String,
}

impl ProviderConfig {
    pub fn to_json(&self, key_set: bool) -> Value {
        obj! {
            "id" => self.id.as_str(),
            "kind" => self.kind.as_str(),
            "base_url" => self.base_url.as_str(),
            "model" => self.model.as_str(),
            "created_at" => self.created_at.as_str(),
            "key_set" => key_set,
            // Honest availability, computed rather than assumed.
            "available" => false,
            "reason" => "this build has no TLS client, so https providers cannot be contacted",
        }
    }
}

/// Validate user-supplied provider settings. https is mandatory: sending an API
/// key in cleartext is never acceptable, even to a "local" model.
pub fn validate(kind: &str, base_url: &str, model: &str, api_key: &str) -> Result<(), String> {
    let k = ProviderKind::parse(kind).ok_or_else(|| format!("unknown provider kind '{}'", kind))?;
    if k == ProviderKind::Extractive {
        return Ok(());
    }
    if !base_url.starts_with("https://") {
        return Err("base_url must start with https:// (refusing to send a key in cleartext)".into());
    }
    if base_url.len() > 512 {
        return Err("base_url is too long".into());
    }
    if model.trim().is_empty() {
        return Err("model name is required".into());
    }
    if api_key.trim().len() < 8 {
        return Err("api key looks too short".into());
    }
    Ok(())
}

pub trait Answerer {
    fn name(&self) -> &'static str;
    fn available(&self) -> bool;
    fn reason(&self) -> String;
    fn answer(&self, question: &str, chunks: &[Chunk], sources: &[Source], max: usize) -> Result<Answer, String>;
}

/// The default engine: composes an answer only from retrieved source sentences.
pub struct ExtractiveAnswerer;

impl Answerer for ExtractiveAnswerer {
    fn name(&self) -> &'static str {
        "extractive"
    }
    fn available(&self) -> bool {
        true
    }
    fn reason(&self) -> String {
        "local extractive engine: answers are sentences quoted from your sources".into()
    }
    fn answer(&self, question: &str, chunks: &[Chunk], sources: &[Source], max: usize) -> Result<Answer, String> {
        Ok(grounded::answer(question, chunks, sources, max))
    }
}

/// Remote OpenAI-compatible chat endpoint. Kept behind the same trait so a
/// future build with TLS can switch engines without touching the API layer.
pub struct RemoteAnswerer {
    pub cfg: ProviderConfig,
    pub api_key: String,
}

impl Answerer for RemoteAnswerer {
    fn name(&self) -> &'static str {
        "openai_compatible"
    }
    fn available(&self) -> bool {
        // Availability is a property of the build, not of the configuration.
        false
    }
    fn reason(&self) -> String {
        "provider configured, but this build cannot make TLS requests; using the local extractive engine".into()
    }
    fn answer(&self, _q: &str, _c: &[Chunk], _s: &[Source], _m: usize) -> Result<Answer, String> {
        Err("TLS/HTTPS is not available in this build; remote answering is impossible".into())
    }
}

/// Pick the engine to use. Never returns "none": there is always a local engine.
pub fn select(cfg: Option<&ProviderConfig>, key: Option<&str>) -> (Box<dyn Answerer>, String) {
    if let Some(c) = cfg {
        let remote = RemoteAnswerer { cfg: c.clone(), api_key: key.unwrap_or("").to_string() };
        if remote.available() {
            return (Box::new(remote), String::new());
        }
        let note = remote.reason();
        return (Box::new(ExtractiveAnswerer), note);
    }
    let e = ExtractiveAnswerer;
    (Box::new(e), String::new())
}

/// Secrets live outside the document database, with 0600 permissions.
pub fn secrets_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("secrets.json")
}

pub fn write_secret(data_dir: &std::path::Path, id: &str, key: &str) -> std::io::Result<()> {
    let path = secrets_path(data_dir);
    let mut map = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| crate::json::parse(&s).ok())
        .and_then(|v| match v {
            Value::Obj(m) => Some(m),
            _ => None,
        })
        .unwrap_or_default();
    map.insert(id.to_string(), Value::from(key));
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, crate::json::to_string(&Value::Obj(map)))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, &path)
}

pub fn read_secret(data_dir: &std::path::Path, id: &str) -> Option<String> {
    let s = std::fs::read_to_string(secrets_path(data_dir)).ok()?;
    let v = crate::json::parse(&s).ok()?;
    v.str_of(id).map(|x| x.to_string())
}

pub fn delete_secret(data_dir: &std::path::Path, id: &str) -> Option<String> {
    let path = secrets_path(data_dir);
    let s = std::fs::read_to_string(&path).ok()?;
    let mut v = crate::json::parse(&s).ok()?;
    let removed = v.str_of(id).map(|x| x.to_string());
    if removed.is_some() {
        if let Value::Obj(ref mut m) = v {
            m.remove(id);
        }
        let _ = std::fs::write(&path, crate::json::to_string(&v));
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_cleartext_endpoints_and_short_keys() {
        assert!(validate("openai_compatible", "http://api.example.com", "gpt", "sk-12345678").is_err());
        assert!(validate("openai_compatible", "https://api.example.com", "", "sk-12345678").is_err());
        assert!(validate("openai_compatible", "https://api.example.com", "m", "short").is_err());
        assert!(validate("openai_compatible", "https://api.example.com", "m", "sk-12345678").is_ok());
        assert!(validate("magic", "https://x", "m", "sk-12345678").is_err());
    }

    #[test]
    fn extractive_engine_is_always_selected() {
        let (a, note) = select(None, None);
        assert_eq!(a.name(), "extractive");
        assert!(a.available());
        assert!(note.is_empty());
    }

    #[test]
    fn configured_remote_provider_states_its_limitation() {
        let cfg = ProviderConfig {
            id: "p1".into(),
            kind: "openai_compatible".into(),
            base_url: "https://api.example.com".into(),
            model: "m".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let (a, note) = select(Some(&cfg), Some("sk-12345678"));
        assert_eq!(a.name(), "extractive", "must fall back, never pretend");
        assert!(note.contains("TLS"));
        let json = cfg.to_json(true);
        assert_eq!(json.get("available").and_then(|v| v.as_bool()), Some(false));
        assert!(!crate::json::to_string(&json).contains("sk-12345678"));
    }

    #[test]
    fn secrets_are_private_and_removable() {
        let dir = std::env::temp_dir().join(format!("forge-secret-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write_secret(&dir, "p1", "sk-secret-value").unwrap();
        assert_eq!(read_secret(&dir, "p1").as_deref(), Some("sk-secret-value"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(secrets_path(&dir)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "secrets file must be owner-only");
        }
        assert_eq!(delete_secret(&dir, "p1").as_deref(), Some("sk-secret-value"));
        assert!(read_secret(&dir, "p1").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
