//! CLI entry point for traceview.
//!
//! This binary provides the main server executable that:
//! - Accepts OTLP trace data via HTTP
//! - Stores spans in SQLite
//! - Provides JSON API and SSE streams for viewing traces

// Silence false positives from `unused_crate_dependencies` lint.
// These crates are used in the library but lint checks binary separately.
use base64 as _;
use chrono as _;
use futures_core as _;
use maud as _;
use pin_project_lite as _;
use serde as _;
use serde_json as _;
use sqlx as _;
use thiserror as _;
use tokio_stream as _;
use tower_http as _;

// Dev-dependencies
#[cfg(test)]
use reqwest as _;
#[cfg(test)]
use tempfile as _;
#[cfg(test)]
use tower as _;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use std::path::PathBuf;

use traceview::{AppState, BatchWriter, Database, create_router};

/// Command line arguments for traceview server.
#[derive(Parser)]
#[command(name = "traceview", about = "OTLP trace viewer for GenAI applications")]
struct Args {
    /// Database file path
    #[arg(short, long, default_value = "./traces.db")]
    db_path: PathBuf,

    /// Port to listen on
    #[arg(short, long, default_value_t = 6969)]
    port: u16,

    /// Batch size for span inserts
    #[arg(long, default_value_t = 1000)]
    batch_size: usize,

    /// Batch interval in milliseconds
    #[arg(long, default_value_t = 100)]
    batch_interval_ms: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into()),
        )
        .init();

    // Parse args
    let args = Args::parse();

    // Convert db_path to string, with fallback
    let db_path_str = args.db_path.to_str().unwrap_or("./traces.db");

    // Create database
    let db = Database::new(db_path_str).await?;

    // Create batch writer (spawn as background task)
    let (batch_writer, _span_tx) = BatchWriter::new(
        db.clone(),
        args.batch_size,
        Duration::from_millis(args.batch_interval_ms),
    );
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
    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!("Starting traceview on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
