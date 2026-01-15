//! Traceview - A distributed tracing viewer.

// Silence false positives from `unused_crate_dependencies` lint.
// These crates are either:
// - Used via procedural macros (serde, thiserror)
// - Used in submodules but reported as unused at crate root
// - Used by the binary (main.rs) but shared in Cargo.toml
use base64 as _;
use chrono as _;
use clap as _;
use futures_core as _;
use maud as _;
use pin_project_lite as _;
use serde as _;
use serde_json as _;
use sqlx as _;
use thiserror as _;
use tokio_stream as _;
use tower_http as _;
use tracing_subscriber as _;

// Dev-dependencies used only in tests
#[cfg(test)]
use reqwest as _;
#[cfg(test)]
use tempfile as _;
#[cfg(test)]
use tower as _;

pub mod api;
pub mod db;
pub mod error;
pub mod ingest;
pub mod models;
pub mod sse;
pub mod views;

pub use api::{AppState, SharedState, create_router};
pub use db::{BatchWriter, Database};
pub use error::{Result, TraceviewError};
pub use ingest::{OtlpTraceData, convert_otlp};
pub use models::{Session, Span, SpanEvent, SpanKind};
pub use sse::{SpanStream, span_sse};
pub use views::{base_layout, session_detail, sessions_list, span_html};
