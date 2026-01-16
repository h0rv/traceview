//! Traceview - A distributed tracing viewer.

// Silence false positives from `unused_crate_dependencies` lint.
// These crates are either:
// - Used via procedural macros (serde, thiserror)
// - Used in submodules but reported as unused at crate root
// - Used by the binary (main.rs) but shared in Cargo.toml
use base64 as _;
use chrono as _;
use clap as _;
use futures as _;
use futures_core as _;
use iocraft as _;
use maud as _;
use opentelemetry_proto as _;
use pin_project_lite as _;
use prost as _;
use serde as _;
use serde_json as _;
use sqlx as _;
use thiserror as _;
use tokio_stream as _;
use tower_http as _;
use tracing_subscriber as _;

// Dev-dependencies - silence lint via cfg(test) since they're only available in tests
#[cfg(test)]
mod _dev_deps {
    use futures_util as _;
    use reqwest as _;
    use tempfile as _;
    use tower as _;
}

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use iocraft::prelude::*;

pub mod api;
pub mod db;
pub mod error;
pub mod ingest;
pub mod models;
pub mod sse;
pub mod tui;
pub mod views;

pub use api::{AppState, SharedState, create_router};
pub use db::{BatchWriter, Database};
pub use error::{Result, TraceviewError};
pub use ingest::{OtlpTraceData, convert_otlp, convert_otlp_proto, extract_session_name};
pub use models::{Session, Span, SpanEvent, SpanKind};
pub use sse::{SpanStream, span_sse};
pub use views::{
    app_layout, base_layout, session_detail, sessions_list, sidebar_session_list, span_html,
};

/// Run the OTLP server with web UI.
///
/// This starts the HTTP server that:
/// - Accepts OTLP trace data at /v1/traces
/// - Provides a web UI for viewing traces
/// - Exposes JSON APIs for trace data
pub async fn run_server(
    db: Database,
    port: u16,
    batch_size: usize,
    batch_interval_ms: u64,
) -> Result<()> {
    // Create batch writer (spawn as background task)
    let (batch_writer, _span_tx) =
        BatchWriter::new(db.clone(), batch_size, Duration::from_millis(batch_interval_ms));
    tokio::spawn(async move {
        if let Err(e) = batch_writer.run().await {
            tracing::error!("Batch writer error: {}", e);
        }
    });

    // Create app state
    let state = Arc::new(AppState { db });

    // Create router
    let app = create_router(state);

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Starting server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Run the TUI viewer.
///
/// This starts the terminal user interface for viewing traces.
/// The TUI runs in fullscreen mode and blocks until the user exits.
pub async fn run_tui(db: Arc<Database>) -> Result<()> {
    element!(tui::App(db: Some(db))).fullscreen().await?;
    Ok(())
}
