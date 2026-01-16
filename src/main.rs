//! tv - Unified CLI for traceview.
//!
//! A single binary that can:
//! - Start the OTLP server with web UI (`tv serve`)
//! - Run the TUI viewer (`tv ui`)
//! - Run both together (default: `tv`)

// Silence false positives from `unused_crate_dependencies` lint.
// These crates are used in the library but lint checks binary separately.
use axum as _;
use base64 as _;
use chrono as _;
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

// Dev-dependencies (only available in test builds)
#[cfg(test)]
mod _dev_deps {
    use futures_util as _;
    use reqwest as _;
    use tempfile as _;
    use tower as _;
}

use std::sync::Arc;

use clap::{Parser, Subcommand};

use traceview::{Database, run_server, run_tui};

/// tv - Traceview OTLP trace viewer
#[derive(Parser)]
#[command(name = "tv", about = "Traceview - OTLP trace viewer for GenAI applications", version)]
struct Cli {
    /// HTTP port for OTLP ingest and web UI
    #[arg(short, long, default_value_t = 4318, env = "TV_PORT")]
    port: u16,

    /// Path to SQLite database
    #[arg(short, long, default_value = "traces.db", env = "TV_DB_PATH")]
    db: String,

    /// Batch size for span inserts
    #[arg(long, default_value_t = 1000)]
    batch_size: usize,

    /// Batch interval in milliseconds
    #[arg(long, default_value_t = 100)]
    batch_interval_ms: u64,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start OTLP server and web UI only (no TUI)
    Serve,
    /// Start TUI viewer only (no server)
    Ui,
}

#[tokio::main]
async fn main() -> traceview::Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into()),
        )
        .init();

    let cli = Cli::parse();

    // Open/create database
    let db = Database::new(&cli.db).await?;

    match cli.command {
        Some(Command::Serve) => {
            // Server only mode
            println!("Server running at http://localhost:{}", cli.port);
            run_server(db, cli.port, cli.batch_size, cli.batch_interval_ms).await
        }
        Some(Command::Ui) => {
            // TUI only mode
            run_tui(Arc::new(db)).await
        }
        None => {
            // Default: Both together
            println!("Server running at http://localhost:{}", cli.port);

            let server_db = db.clone();
            let port = cli.port;
            let batch_size = cli.batch_size;
            let batch_interval = cli.batch_interval_ms;

            // Spawn server in background
            let server_handle = tokio::spawn(async move {
                if let Err(e) = run_server(server_db, port, batch_size, batch_interval).await {
                    tracing::error!("Server error: {}", e);
                }
            });

            // Run TUI in foreground (blocks until quit)
            let result = run_tui(Arc::new(db)).await;

            // TUI exited, shutdown server
            server_handle.abort();
            result
        }
    }
}
