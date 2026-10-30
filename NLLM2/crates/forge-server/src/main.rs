//! Forge server: wires the API to a TCP listener and serves the web app.

use forge_core::api::Api;
use forge_core::http::{serve, Response};
use forge_core::web::serve_static;
use forge_core::store::Store;
use std::net::TcpListener;
use std::path::PathBuf;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn main() {
    let addr = env_or("FORGE_ADDR", "0.0.0.0:8080");
    let data_dir = PathBuf::from(env_or("FORGE_DATA_DIR", "data"));
    let web_dir = PathBuf::from(env_or("FORGE_WEB_DIR", "apps/web"));

    let store = match Store::open(&data_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("fatal: cannot open data dir {:?}: {}", data_dir, e);
            std::process::exit(1);
        }
    };
    let api = Api::new(store);

    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("fatal: cannot bind {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    println!("Forge {} listening on http://{}", env!("CARGO_PKG_VERSION"), addr);
    println!("  data dir : {}", data_dir.display());
    println!("  web dir  : {}", web_dir.display());
    println!("  engine   : extractive grounded answering (no model provider configured)");

    let web = web_dir.canonicalize().unwrap_or(web_dir.clone());
    let web_for_handler = web.clone();

    let handler = move |req: &forge_core::http::Request| -> Response {
        if req.path.starts_with("/api") {
            return api.handle(req);
        }
        serve_static(&web_for_handler, &req.path)
    };

    if let Err(e) = serve(listener, handler, forge_core::http::DEFAULT_MAX_BODY) {
        eprintln!("server error: {}", e);
        std::process::exit(1);
    }
}
