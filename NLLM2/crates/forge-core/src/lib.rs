//! Forge core: a source-grounded research notebook engine.
//!
//! Zero dependencies by necessity (crates.io is unreachable — docs/DECISIONS.md D1).
//! Everything here is hand-built on the standard library and covered by tests.

pub mod api;
pub mod artifacts;
pub mod domain;
pub mod grounded;
pub mod http;
pub mod inflate;
pub mod ingest;
pub mod json;
pub mod providers;
pub mod retrieve;
pub mod security;
pub mod store;
pub mod web;

pub use domain::*;
