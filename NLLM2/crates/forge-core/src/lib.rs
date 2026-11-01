//! Forge core: a source-grounded research notebook engine.
//!
//! Zero dependencies by necessity (crates.io is unreachable — docs/DECISIONS.md D1).
//! Everything here is hand-built on the standard library and covered by tests.

pub mod api;
pub mod artifacts;
pub mod datatable;
pub mod domain;
pub mod grounded;
pub mod guide;
pub mod hashes;
pub mod fetch;
pub mod github;
pub mod http;
pub mod inflate;
pub mod ingest;
pub mod json;
pub mod media;
pub mod auth;
pub mod contradictions;
pub mod discovery;
pub mod export;
pub mod notebooklm;
pub mod providers;
pub mod render;
pub mod retrieve;
pub mod security;
pub mod store;
pub mod tables;
pub mod untrusted;
pub mod web;

pub use domain::*;
