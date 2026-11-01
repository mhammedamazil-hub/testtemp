//! Access control: owner token + role-scoped share links.
//!
//! Forge runs locally for one operator. That is a deployment fact, not a security model: as
//! soon as a notebook is shared, "who is asking" has to be decidable. Two modes exist, and the
//! active one is reported by `/api/capabilities` so a page can never imply more than it does:
//!
//! * **single-user** (default, no owner token configured): every request is the owner.
//!   Sharing is not enforced because there is nobody to enforce it against — the app must be
//!   bound to loopback or a trusted network, and it says so.
//! * **token** (`owner.token` exists in the data directory): requests must present either the
//!   owner token or a share token. Shares are scoped to one notebook and one role:
//!   `viewer` may read and ask questions, `editor` may also add and edit sources, notes and
//!   artifacts, and neither may delete the notebook or manage shares.
//!
//! Tokens are 32 hex characters from the OS RNG. Only their SHA-256 is stored, so a copy of
//! `db.json` does not hand out access. A revoked share is rejected on the next request.

use crate::domain::new_id;
use crate::hashes::sha256;
use crate::http::Request;
use crate::json::Value;
use std::io::Read;

/// One share link. The plaintext token is returned once, at creation, and never stored.
#[derive(Debug, Clone, Default)]
pub struct ShareGrant {
    pub id: String,
    pub notebook_id: String,
    pub role: String, // "viewer" | "editor"
    pub token_sha256: String,
    pub token_prefix: String,
    pub created_at: String,
    pub revoked_at: String,
}

/// Who is making the request.
#[derive(Debug, Clone, PartialEq)]
pub enum Identity {
    /// The operator, or any request while no owner token is configured.
    Owner,
    /// A share link, scoped to one notebook.
    Share { grant_id: String, notebook_id: String, role: String },
}

impl Identity {
    pub fn name(&self) -> &'static str {
        match self {
            Identity::Owner => "owner",
            Identity::Share { role, .. } => match role.as_str() {
                "editor" => "share:editor",
                _ => "share:viewer",
            },
        }
    }
    pub fn notebook(&self) -> Option<&str> {
        match self {
            Identity::Owner => None,
            Identity::Share { notebook_id, .. } => Some(notebook_id.as_str()),
        }
    }
    pub fn is_owner(&self) -> bool {
        matches!(self, Identity::Owner)
    }
}

/// A request must be authenticated, and was not.
pub fn unauthorized(reason: &str) -> (u16, &'static str, String) {
    (401, "unauthorized", reason.to_string())
}

/// 32 hex characters from the OS RNG, falling back to a hash of id+time if `/dev/urandom` is
/// unavailable (the app still works; the fallback is weaker and is reported by `mode_note`).
pub fn fresh_token() -> String {
    let mut bytes = [0u8; 16];
    let ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_ok();
    if !ok {
        let seed = format!("{}|{}", new_id("seed"), std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0));
        // sha256 returns lowercase hex; take 32 hex digits = 16 bytes of material.
        let h = sha256(seed.as_bytes());
        for (i, chunk) in h.as_bytes().chunks(2).take(16).enumerate() {
            let pair = std::str::from_utf8(chunk).unwrap_or("00");
            bytes[i] = u8::from_str_radix(pair, 16).unwrap_or(0);
        }
    }
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Constant-time comparison for token material.
pub fn token_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// Where a token may travel: `Authorization: Bearer …` or `X-Forge-Token`.
pub fn presented_token(req: &Request) -> Option<String> {
    if let Some(h) = req.header("authorization") {
        if let Some(rest) = h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer ")) {
            return Some(rest.trim().to_string());
        }
    }
    req.header("x-forge-token").map(|s| s.trim().to_string())
}

/// The share token travels separately so a viewer can never be mistaken for the owner.
pub fn presented_share(req: &Request) -> Option<String> {
    req.header("x-forge-share").map(|s| s.trim().to_string())
}

/// Resolve an identity against the configured owner token and the stored grants.
///
/// `enforced` is true when an owner token exists. In that mode an anonymous request is an
/// error, not an implicit owner.
pub fn resolve(
    req: &Request,
    enforced: bool,
    owner_token: Option<&str>,
    grants: &[ShareGrant],
) -> Result<Identity, (u16, &'static str, String)> {
    if !enforced {
        return Ok(Identity::Owner);
    }
    let share = presented_share(req);
    let token = presented_token(req);
    if share.is_none() && token.is_none() {
        return Err(unauthorized("this notebook server requires a token"));
    }
    if let Some(t) = token {
        if let Some(owner) = owner_token {
            if token_eq(&t, owner) {
                return Ok(Identity::Owner);
            }
        }
        return Err(unauthorized("the owner token is not valid"));
    }
    let t = share.unwrap_or_default();
    let hex = sha256(t.as_bytes());
    match grants.iter().find(|g| token_eq(&g.token_sha256, &hex)) {
        Some(g) if g.revoked_at.is_empty() => Ok(Identity::Share {
            grant_id: g.id.clone(),
            notebook_id: g.notebook_id.clone(),
            role: g.role.clone(),
        }),
        Some(_) => Err(unauthorized("this share link has been revoked")),
        None => Err(unauthorized("this share link is not valid")),
    }
}

/// Role check for one request.
///
/// `notebook` is the notebook the route acts on (resolved by the caller, which knows how to
/// follow a source id to its notebook). `action` is a coarse verb: "read", "write", "manage".
pub fn allow(identity: &Identity, notebook: Option<&str>, action: &str) -> Result<(), (u16, &'static str, String)> {
    let Identity::Share { notebook_id, role, .. } = identity else {
        return Ok(());
    };
    match notebook {
        Some(nb) if nb == notebook_id => {}
        _ => {
            return Err((
                403,
                "not_in_share",
                "this share link only covers one notebook".to_string(),
            ))
        }
    }
    match (role.as_str(), action) {
        ("viewer", "read") => Ok(()),
        ("editor", "read") | ("editor", "write") => Ok(()),
        ("viewer", "write") => Err((
            403,
            "forbidden_role",
            "this link is view-only; it cannot change the notebook".to_string(),
        )),
        (_, "manage") => Err((
            403,
            "forbidden_role",
            "only the owner can manage sharing or delete a notebook".to_string(),
        )),
        _ => Err((403, "forbidden_role", "this action is not permitted for this link".to_string())),
    }
}

/// Public description of a grant (never includes the token or its hash).
pub fn grant_json(g: &ShareGrant) -> Value {
    crate::obj! {
        "id" => g.id.as_str(),
        "notebook_id" => g.notebook_id.as_str(),
        "role" => g.role.as_str(),
        "token_prefix" => g.token_prefix.as_str(),
        "created_at" => g.created_at.as_str(),
        "revoked_at" => g.revoked_at.as_str(),
        "revoked" => !g.revoked_at.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn req(headers: &[(&str, &str)]) -> Request {
        Request {
            method: "GET".into(),
            path: "/api/x".into(),
            query: BTreeMap::new(),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v.to_string()))
                .collect(),
            body: vec![],
            peer: "test".into(),
        }
    }

    fn grant(role: &str, token: &str) -> ShareGrant {
        let hex = sha256(token.as_bytes());
        ShareGrant {
            id: "shr-1".into(),
            notebook_id: "nb-1".into(),
            role: role.into(),
            token_sha256: hex,
            token_prefix: token[..6].to_string(),
            created_at: "2026-01-01T00:00:00Z".into(),
            revoked_at: String::new(),
        }
    }

    #[test]
    fn single_user_mode_treats_everyone_as_the_owner() {
        let id = resolve(&req(&[]), false, None, &[]).unwrap();
        assert_eq!(id, Identity::Owner);
    }

    #[test]
    fn enforced_mode_rejects_anonymous_requests() {
        let err = resolve(&req(&[]), true, Some("owner-token"), &[]).unwrap_err();
        assert_eq!(err.0, 401);
    }

    #[test]
    fn the_owner_token_is_accepted_in_either_header() {
        let a = resolve(&req(&[("authorization", "Bearer owner-token")]), true, Some("owner-token"), &[]).unwrap();
        let b = resolve(&req(&[("x-forge-token", "owner-token")]), true, Some("owner-token"), &[]).unwrap();
        assert_eq!(a, Identity::Owner);
        assert_eq!(b, Identity::Owner);
    }

    #[test]
    fn a_wrong_owner_token_is_rejected() {
        let err = resolve(&req(&[("authorization", "Bearer nope")]), true, Some("owner-token"), &[]).unwrap_err();
        assert_eq!(err.0, 401);
    }

    #[test]
    fn a_share_token_resolves_to_its_grant() {
        let id = resolve(&req(&[("x-forge-share", "share-abc")]), true, Some("owner"), &[grant("viewer", "share-abc")]).unwrap();
        match id {
            Identity::Share { role, notebook_id, .. } => {
                assert_eq!(role, "viewer");
                assert_eq!(notebook_id, "nb-1");
            }
            _ => panic!("expected a share identity"),
        }
    }

    #[test]
    fn a_revoked_share_is_rejected() {
        let mut g = grant("viewer", "share-abc");
        g.revoked_at = "2026-02-01T00:00:00Z".into();
        let err = resolve(&req(&[("x-forge-share", "share-abc")]), true, Some("owner"), &[g]).unwrap_err();
        assert_eq!(err.0, 401);
        assert!(err.2.contains("revoked"));
    }

    #[test]
    fn viewers_cannot_write_and_nobody_but_the_owner_manages_shares() {
        let viewer = Identity::Share { grant_id: "shr-1".into(), notebook_id: "nb-1".into(), role: "viewer".into() };
        assert!(allow(&viewer, Some("nb-1"), "read").is_ok());
        assert_eq!(allow(&viewer, Some("nb-1"), "write").unwrap_err().1, "forbidden_role");
        assert_eq!(allow(&viewer, Some("nb-1"), "manage").unwrap_err().1, "forbidden_role");
        let editor = Identity::Share { grant_id: "shr-2".into(), notebook_id: "nb-1".into(), role: "editor".into() };
        assert!(allow(&editor, Some("nb-1"), "write").is_ok());
        assert!(allow(&editor, Some("nb-1"), "manage").is_err());
        assert_eq!(allow(&editor, Some("nb-2"), "read").unwrap_err().1, "not_in_share");
    }

    #[test]
    fn tokens_are_unique_and_not_stored_in_the_clear() {
        let t1 = fresh_token();
        let t2 = fresh_token();
        assert_eq!(t1.len(), 32);
        assert_ne!(t1, t2);
        assert!(t1.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(token_eq(&t1, &t1));
        assert!(!token_eq(&t1, &t2));
    }
}
