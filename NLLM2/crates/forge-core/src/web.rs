//! Static hosting for the single-page web app.
//!
//! Serves files only from inside the configured web root (path traversal is
//! refused) and falls back to `index.html` for client-side routes.

use crate::http::{mime_for, Response};
use crate::obj;
use crate::security::safe_join;
use std::path::Path;

pub fn serve_static(root: &Path, path: &str) -> Response {
    let rel = if path == "/" || path.is_empty() {
        "index.html".to_string()
    } else {
        path.trim_start_matches('/').to_string()
    };

    if let Some(full) = safe_join(root, &rel) {
        if full.is_file() {
            return match std::fs::read(&full) {
                Ok(bytes) => {
                    let mut res = Response::new(200, mime_for(&rel), bytes);
                    res = res.with_header("Cache-Control", "no-cache");
                    res
                }
                Err(_) => Response::text(500, "read error"),
            };
        }
    }

    // Client-side routing: extension-less unknown paths get the app shell.
    if !rel.contains('.') {
        if let Some(index) = safe_join(root, "index.html") {
            if let Ok(bytes) = std::fs::read(&index) {
                return Response::new(200, "text/html; charset=utf-8", bytes);
            }
        }
    }

    Response::json(404, &obj! { "error" => "not found", "code" => "static_404" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/web")
    }

    #[test]
    fn serves_the_shell_and_assets() {
        let root = app_dir();
        let index = serve_static(&root, "/");
        assert_eq!(index.status, 200);
        assert!(String::from_utf8_lossy(&index.body).contains("Forge"));
        assert_eq!(serve_static(&root, "/app.js").status, 200);
        assert_eq!(serve_static(&root, "/styles.css").content_type, "text/css; charset=utf-8");
    }

    #[test]
    fn deep_links_fall_back_to_shell() {
        let r = serve_static(&app_dir(), "/nb/abc123");
        assert_eq!(r.status, 200);
        assert!(String::from_utf8_lossy(&r.body).contains("app.js"));
    }

    #[test]
    fn traversal_and_missing_assets_are_refused() {
        let root = app_dir();
        for p in ["/../Cargo.toml", "/../../etc/passwd", "/%2e%2e/Cargo.toml"] {
            let r = serve_static(&root, p);
            assert_ne!(r.status, 200, "must not serve {}", p);
        }
        let missing = serve_static(&root, "/nope.js");
        assert_eq!(missing.status, 404);
    }
}
