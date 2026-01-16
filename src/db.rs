//! Database layer with batch writer for traceview.
//!
//! This module provides SQLite storage for spans and sessions with:
//! - WAL mode for concurrent read/write
//! - Batch writing for efficient ingestion
//! - Broadcast channel for real-time SSE updates

use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tokio::sync::{broadcast, mpsc};
use tokio::time::interval;

use crate::error::Result;
use crate::models::{Session, Span, SpanKind};

/// Default broadcast channel capacity for span updates.
const BROADCAST_CAPACITY: usize = 1024;

/// Database handle providing storage and real-time span notifications.
#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
    span_tx: broadcast::Sender<Span>,
}

impl Database {
    /// Create a new database connection with the given file path.
    ///
    /// This will create the database file if it doesn't exist and run migrations.
    pub async fn new(db_path: &str) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new().max_connections(5).connect_with(options).await?;

        let (span_tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        let db = Self { pool, span_tx };
        db.run_migrations().await?;
        Ok(db)
    }

    /// Create a new in-memory database for testing.
    pub async fn new_in_memory() -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await?;

        let (span_tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        let db = Self { pool, span_tx };
        db.run_migrations().await?;
        Ok(db)
    }

    /// Run database migrations to create tables and indexes.
    async fn run_migrations(&self) -> Result<()> {
        // Create sessions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                name TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create spans table with all fields from Span model
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS spans (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                parent_span_id TEXT,
                trace_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                model TEXT,
                content TEXT,
                metadata TEXT,
                start_time INTEGER NOT NULL,
                end_time INTEGER,
                duration_ms INTEGER,
                input_tokens INTEGER,
                output_tokens INTEGER,
                finish_reason TEXT,
                tool_call_id TEXT,
                tool_name TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create indexes for efficient queries
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_spans_session_start ON spans (session_id, start_time)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_spans_trace_start ON spans (trace_id, start_time)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_spans_kind ON spans (kind)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions (updated_at DESC)",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Subscribe to receive span insertions for SSE.
    pub fn subscribe(&self) -> broadcast::Receiver<Span> {
        self.span_tx.subscribe()
    }

    /// Insert a single span and broadcast it to subscribers.
    pub async fn insert_span(&self, span: &Span) -> Result<()> {
        let kind_str = serialize_span_kind(span.kind);
        let metadata_json = span.metadata.as_ref().map(serde_json::to_string).transpose()?;

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO spans (
                id, session_id, parent_span_id, trace_id, kind, model, content,
                metadata, start_time, end_time, duration_ms, input_tokens,
                output_tokens, finish_reason, tool_call_id, tool_name
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&span.id)
        .bind(&span.session_id)
        .bind(&span.parent_span_id)
        .bind(&span.trace_id)
        .bind(kind_str)
        .bind(&span.model)
        .bind(&span.content)
        .bind(&metadata_json)
        .bind(span.start_time)
        .bind(span.end_time)
        .bind(span.duration_ms)
        .bind(span.input_tokens)
        .bind(span.output_tokens)
        .bind(&span.finish_reason)
        .bind(&span.tool_call_id)
        .bind(&span.tool_name)
        .execute(&self.pool)
        .await?;

        // Broadcast to subscribers, ignoring if no receivers
        let _ = self.span_tx.send(span.clone());

        Ok(())
    }

    /// Insert multiple spans in a transaction for efficiency.
    pub async fn insert_spans(&self, spans: &[Span]) -> Result<()> {
        if spans.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for span in spans {
            let kind_str = serialize_span_kind(span.kind);
            let metadata_json = span.metadata.as_ref().map(serde_json::to_string).transpose()?;

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO spans (
                    id, session_id, parent_span_id, trace_id, kind, model, content,
                    metadata, start_time, end_time, duration_ms, input_tokens,
                    output_tokens, finish_reason, tool_call_id, tool_name
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&span.id)
            .bind(&span.session_id)
            .bind(&span.parent_span_id)
            .bind(&span.trace_id)
            .bind(kind_str)
            .bind(&span.model)
            .bind(&span.content)
            .bind(&metadata_json)
            .bind(span.start_time)
            .bind(span.end_time)
            .bind(span.duration_ms)
            .bind(span.input_tokens)
            .bind(span.output_tokens)
            .bind(&span.finish_reason)
            .bind(&span.tool_call_id)
            .bind(&span.tool_name)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        // Broadcast all spans after commit
        for span in spans {
            let _ = self.span_tx.send(span.clone());
        }

        Ok(())
    }

    /// Get a session by ID.
    pub async fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let row = sqlx::query("SELECT id, name, created_at, updated_at FROM sessions WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(r) => Ok(Some(Session {
                id: r.get("id"),
                name: r.get("name"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })),
            None => Ok(None),
        }
    }

    /// Get sessions with pagination, ordered by updated_at descending.
    pub async fn get_sessions(&self, limit: i64, offset: i64) -> Result<Vec<Session>> {
        let rows = sqlx::query(
            "SELECT id, name, created_at, updated_at FROM sessions ORDER BY updated_at DESC LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let sessions = rows
            .into_iter()
            .map(|r| Session {
                id: r.get("id"),
                name: r.get("name"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect();

        Ok(sessions)
    }

    /// Get sessions with span counts for sidebar display.
    ///
    /// Returns sessions ordered by updated_at descending along with
    /// the count of spans in each session.
    pub async fn get_sessions_with_counts(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(Session, i64)>> {
        let rows = sqlx::query(
            r#"
            SELECT s.id, s.name, s.created_at, s.updated_at,
                   COUNT(sp.id) as span_count
            FROM sessions s
            LEFT JOIN spans sp ON s.id = sp.session_id
            GROUP BY s.id
            ORDER BY s.updated_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let sessions = rows
            .into_iter()
            .map(|r| {
                let session = Session {
                    id: r.get("id"),
                    name: r.get("name"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                };
                let count: i64 = r.get("span_count");
                (session, count)
            })
            .collect();

        Ok(sessions)
    }

    /// Get all spans for a session, ordered by start_time.
    pub async fn get_spans_by_session(&self, session_id: &str) -> Result<Vec<Span>> {
        let rows = sqlx::query(
            r#"
            SELECT id, session_id, parent_span_id, trace_id, kind, model, content,
                   metadata, start_time, end_time, duration_ms, input_tokens,
                   output_tokens, finish_reason, tool_call_id, tool_name
            FROM spans
            WHERE session_id = ?
            ORDER BY start_time ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        let mut spans = Vec::with_capacity(rows.len());
        for r in rows {
            let metadata_str: Option<String> = r.get("metadata");
            let metadata = metadata_str.map(|s| serde_json::from_str(&s)).transpose()?;

            let kind_str: String = r.get("kind");
            let kind = deserialize_span_kind(&kind_str);

            spans.push(Span {
                id: r.get("id"),
                session_id: r.get("session_id"),
                parent_span_id: r.get("parent_span_id"),
                trace_id: r.get("trace_id"),
                kind,
                model: r.get("model"),
                content: r.get("content"),
                metadata,
                start_time: r.get("start_time"),
                end_time: r.get("end_time"),
                duration_ms: r.get("duration_ms"),
                input_tokens: r.get("input_tokens"),
                output_tokens: r.get("output_tokens"),
                finish_reason: r.get("finish_reason"),
                tool_call_id: r.get("tool_call_id"),
                tool_name: r.get("tool_name"),
            });
        }

        Ok(spans)
    }

    /// Update a session's name only if it is currently NULL.
    ///
    /// This is used for auto-naming sessions from the first user message.
    /// If the session already has a name, this operation does nothing.
    pub async fn update_session_name_if_empty(&self, session_id: &str, name: &str) -> Result<()> {
        sqlx::query("UPDATE sessions SET name = ? WHERE id = ? AND name IS NULL")
            .bind(name)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Create or update a session.
    pub async fn upsert_session(&self, session: &Session) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO sessions (id, name, created_at, updated_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&session.id)
        .bind(&session.name)
        .bind(session.created_at)
        .bind(session.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Search sessions and spans by text query.
    ///
    /// Uses LIKE queries for simplicity. Searches session names and span content.
    pub async fn search(&self, query: &str, limit: i64) -> Result<crate::models::SearchResult> {
        use crate::models::{SearchResult, SessionMatch, SpanMatch};

        let search_pattern = format!("%{query}%");

        // Search sessions by name
        let session_rows =
            sqlx::query("SELECT id, name, created_at, updated_at FROM sessions WHERE name LIKE ? ORDER BY updated_at DESC LIMIT ?")
                .bind(&search_pattern)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?;

        let sessions: Vec<SessionMatch> = session_rows
            .into_iter()
            .map(|r| {
                let name: Option<String> = r.get("name");
                SessionMatch {
                    session: Session {
                        id: r.get("id"),
                        name: name.clone(),
                        created_at: r.get("created_at"),
                        updated_at: r.get("updated_at"),
                    },
                    snippet: name.unwrap_or_default(),
                }
            })
            .collect();

        // Search spans by content or tool_name
        let span_rows = sqlx::query(
            r#"
            SELECT id, session_id, parent_span_id, trace_id, kind, model, content,
                   metadata, start_time, end_time, duration_ms, input_tokens,
                   output_tokens, finish_reason, tool_call_id, tool_name
            FROM spans
            WHERE content LIKE ? OR tool_name LIKE ?
            ORDER BY start_time DESC
            LIMIT ?
            "#,
        )
        .bind(&search_pattern)
        .bind(&search_pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut spans = Vec::with_capacity(span_rows.len());
        for r in span_rows {
            let metadata_str: Option<String> = r.get("metadata");
            let metadata = metadata_str.map(|s| serde_json::from_str(&s)).transpose()?;
            let kind_str: String = r.get("kind");
            let kind = deserialize_span_kind(&kind_str);
            let content: Option<String> = r.get("content");
            let tool_name: Option<String> = r.get("tool_name");

            // Create snippet from content or tool_name
            let snippet = content
                .as_ref()
                .map(|c| extract_snippet(c, query))
                .or_else(|| tool_name.clone())
                .unwrap_or_default();

            spans.push(SpanMatch {
                span: Span {
                    id: r.get("id"),
                    session_id: r.get("session_id"),
                    parent_span_id: r.get("parent_span_id"),
                    trace_id: r.get("trace_id"),
                    kind,
                    model: r.get("model"),
                    content,
                    metadata,
                    start_time: r.get("start_time"),
                    end_time: r.get("end_time"),
                    duration_ms: r.get("duration_ms"),
                    input_tokens: r.get("input_tokens"),
                    output_tokens: r.get("output_tokens"),
                    finish_reason: r.get("finish_reason"),
                    tool_call_id: r.get("tool_call_id"),
                    tool_name,
                },
                snippet,
            });
        }

        Ok(SearchResult { sessions, spans })
    }
}

/// Extract a snippet around the query match in the content.
fn extract_snippet(content: &str, query: &str) -> String {
    let lower_content = content.to_lowercase();
    let lower_query = query.to_lowercase();

    if let Some(pos) = lower_content.find(&lower_query) {
        let start = pos.saturating_sub(40);
        let end = (pos + query.len() + 40).min(content.len());

        let mut snippet = String::new();
        if start > 0 {
            snippet.push_str("...");
        }
        snippet.push_str(&content[start..end]);
        if end < content.len() {
            snippet.push_str("...");
        }
        snippet
    } else {
        // No match found, return truncated content
        if content.len() > 80 { format!("{}...", &content[..80]) } else { content.to_string() }
    }
}

/// Serialize SpanKind to string for database storage.
fn serialize_span_kind(kind: SpanKind) -> &'static str {
    match kind {
        SpanKind::User => "user",
        SpanKind::Assistant => "assistant",
        SpanKind::System => "system",
        SpanKind::Thinking => "thinking",
        SpanKind::ToolCall => "tool_call",
        SpanKind::ToolResult => "tool_result",
        SpanKind::Choice => "choice",
        SpanKind::Span => "span",
    }
}

/// Deserialize SpanKind from database string.
fn deserialize_span_kind(s: &str) -> SpanKind {
    match s {
        "user" => SpanKind::User,
        "assistant" => SpanKind::Assistant,
        "system" => SpanKind::System,
        "thinking" => SpanKind::Thinking,
        "tool_call" => SpanKind::ToolCall,
        "tool_result" => SpanKind::ToolResult,
        "choice" => SpanKind::Choice,
        _ => SpanKind::Span,
    }
}

/// Batch writer that accumulates spans and writes them periodically or when batch is full.
pub struct BatchWriter {
    rx: mpsc::Receiver<Span>,
    db: Database,
    batch_size: usize,
    batch_interval: Duration,
}

impl BatchWriter {
    /// Create a new batch writer and return the sender for submitting spans.
    pub fn new(
        db: Database,
        batch_size: usize,
        batch_interval: Duration,
    ) -> (Self, mpsc::Sender<Span>) {
        let (tx, rx) = mpsc::channel(batch_size * 2);
        let writer = Self { rx, db, batch_size, batch_interval };
        (writer, tx)
    }

    /// Run the batch writer, consuming self.
    ///
    /// This will flush accumulated spans either when the batch is full
    /// or when the interval timer fires.
    pub async fn run(mut self) -> Result<()> {
        let mut batch: Vec<Span> = Vec::with_capacity(self.batch_size);
        let mut ticker = interval(self.batch_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                span_opt = self.rx.recv() => {
                    match span_opt {
                        Some(span) => {
                            batch.push(span);
                            if batch.len() >= self.batch_size {
                                self.flush(&mut batch).await?;
                            }
                        }
                        None => {
                            // Channel closed, flush remaining and exit
                            if !batch.is_empty() {
                                self.flush(&mut batch).await?;
                            }
                            return Ok(());
                        }
                    }
                }
                _ = ticker.tick() => {
                    if !batch.is_empty() {
                        self.flush(&mut batch).await?;
                    }
                }
            }
        }
    }

    async fn flush(&self, batch: &mut Vec<Span>) -> Result<()> {
        self.db.insert_spans(batch).await?;
        batch.clear();
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use crate::error::TraceviewError;
    use std::sync::Arc;
    use tokio::sync::Barrier;

    fn create_test_span(id: &str, session_id: &str, start_time: i64) -> Span {
        Span {
            id: id.to_string(),
            session_id: session_id.to_string(),
            parent_span_id: None,
            trace_id: "trace-1".to_string(),
            kind: SpanKind::User,
            model: Some("claude-3".to_string()),
            content: Some("Hello".to_string()),
            metadata: None,
            start_time,
            end_time: None,
            duration_ms: None,
            input_tokens: Some(10),
            output_tokens: None,
            finish_reason: None,
            tool_call_id: None,
            tool_name: None,
        }
    }

    fn create_test_session(id: &str) -> Session {
        Session {
            id: id.to_string(),
            name: Some("Test Session".to_string()),
            created_at: 1_000_000,
            updated_at: 2_000_000,
        }
    }

    #[tokio::test]
    async fn test_in_memory_database_creation() {
        let db = Database::new_in_memory().await;
        assert!(db.is_ok());
    }

    #[tokio::test]
    async fn test_insert_single_span() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        let span = create_test_span("span-1", "session-1", 1000);
        let result = db.insert_span(&span).await;
        assert!(result.is_ok());

        // Verify span was inserted
        let spans = db.get_spans_by_session("session-1").await;
        assert!(spans.is_ok());
        let spans = spans.unwrap_or_default();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans.first().map(|s| s.id.as_str()), Some("span-1"));
    }

    #[tokio::test]
    async fn test_batch_insert_multiple_spans() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        let spans = vec![
            create_test_span("span-1", "session-1", 1000),
            create_test_span("span-2", "session-1", 2000),
            create_test_span("span-3", "session-1", 3000),
        ];

        let result = db.insert_spans(&spans).await;
        assert!(result.is_ok());

        let retrieved = db.get_spans_by_session("session-1").await;
        assert!(retrieved.is_ok());
        let retrieved = retrieved.unwrap_or_default();
        assert_eq!(retrieved.len(), 3);
    }

    #[tokio::test]
    async fn test_get_session() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        let session = create_test_session("session-1");
        db.upsert_session(&session).await.unwrap_or_else(|e| {
            panic!("Failed to upsert session: {e}");
        });

        let retrieved = db.get_session("session-1").await;
        assert!(retrieved.is_ok());
        let retrieved = retrieved.unwrap_or(None);
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap_or_else(|| create_test_session(""));
        assert_eq!(retrieved.id, "session-1");
        assert_eq!(retrieved.name, Some("Test Session".to_string()));
    }

    #[tokio::test]
    async fn test_get_session_not_found() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        let result = db.get_session("nonexistent").await;
        assert!(result.is_ok());
        assert!(result.unwrap_or(Some(create_test_session(""))).is_none());
    }

    #[tokio::test]
    async fn test_get_sessions_pagination() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        // Insert multiple sessions with different updated_at times
        for i in 0..5 {
            let session = Session {
                id: format!("session-{i}"),
                name: Some(format!("Session {i}")),
                created_at: 1_000_000,
                updated_at: i64::from(i) * 1000,
            };
            db.upsert_session(&session).await.unwrap_or_else(|e| {
                panic!("Failed to upsert session: {e}");
            });
        }

        // Get first page
        let page1 = db.get_sessions(2, 0).await;
        assert!(page1.is_ok());
        let page1 = page1.unwrap_or_default();
        assert_eq!(page1.len(), 2);
        // Should be ordered by updated_at DESC
        assert_eq!(page1.first().map(|s| s.id.as_str()), Some("session-4"));

        // Get second page
        let page2 = db.get_sessions(2, 2).await;
        assert!(page2.is_ok());
        let page2 = page2.unwrap_or_default();
        assert_eq!(page2.len(), 2);
    }

    #[tokio::test]
    async fn test_get_spans_by_session_ordering() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        // Insert spans out of order
        let spans = vec![
            create_test_span("span-3", "session-1", 3000),
            create_test_span("span-1", "session-1", 1000),
            create_test_span("span-2", "session-1", 2000),
        ];

        db.insert_spans(&spans).await.unwrap_or_else(|e| {
            panic!("Failed to insert spans: {e}");
        });

        let retrieved = db.get_spans_by_session("session-1").await;
        assert!(retrieved.is_ok());
        let retrieved = retrieved.unwrap_or_default();

        // Should be ordered by start_time ASC
        assert_eq!(retrieved.first().map(|s| s.id.as_str()), Some("span-1"));
        assert_eq!(retrieved.get(1).map(|s| s.id.as_str()), Some("span-2"));
        assert_eq!(retrieved.get(2).map(|s| s.id.as_str()), Some("span-3"));
    }

    #[tokio::test]
    async fn test_upsert_session_creates() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        let session = create_test_session("new-session");
        let result = db.upsert_session(&session).await;
        assert!(result.is_ok());

        let retrieved = db.get_session("new-session").await;
        assert!(retrieved.is_ok());
        assert!(retrieved.unwrap_or(None).is_some());
    }

    #[tokio::test]
    async fn test_upsert_session_updates() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        // Create initial session
        let session1 = Session {
            id: "session-1".to_string(),
            name: Some("Original Name".to_string()),
            created_at: 1_000_000,
            updated_at: 1_000_000,
        };
        db.upsert_session(&session1).await.unwrap_or_else(|e| {
            panic!("Failed to upsert session: {e}");
        });

        // Update the session
        let session2 = Session {
            id: "session-1".to_string(),
            name: Some("Updated Name".to_string()),
            created_at: 1_000_000,
            updated_at: 2_000_000,
        };
        db.upsert_session(&session2).await.unwrap_or_else(|e| {
            panic!("Failed to upsert session: {e}");
        });

        let retrieved = db.get_session("session-1").await;
        assert!(retrieved.is_ok());
        let retrieved = retrieved.unwrap_or(None).unwrap_or_else(|| create_test_session(""));
        assert_eq!(retrieved.name, Some("Updated Name".to_string()));
        assert_eq!(retrieved.updated_at, 2_000_000);
        // created_at should remain unchanged
        assert_eq!(retrieved.created_at, 1_000_000);
    }

    #[tokio::test]
    async fn test_broadcast_receiver_receives_spans() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        let mut rx = db.subscribe();
        let span = create_test_span("span-1", "session-1", 1000);

        db.insert_span(&span).await.unwrap_or_else(|e| {
            panic!("Failed to insert span: {e}");
        });

        let received = rx.try_recv();
        assert!(received.is_ok());
        let received = received.unwrap_or_else(|_| create_test_span("", "", 0));
        assert_eq!(received.id, "span-1");
    }

    #[tokio::test]
    async fn test_broadcast_batch_insert() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        let mut rx = db.subscribe();
        let spans = vec![
            create_test_span("span-1", "session-1", 1000),
            create_test_span("span-2", "session-1", 2000),
        ];

        db.insert_spans(&spans).await.unwrap_or_else(|e| {
            panic!("Failed to insert spans: {e}");
        });

        // Should receive both spans
        let r1 = rx.try_recv();
        let r2 = rx.try_recv();
        assert!(r1.is_ok());
        assert!(r2.is_ok());
    }

    #[tokio::test]
    async fn test_concurrent_read_write() {
        let db = Arc::new(Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        }));

        let barrier = Arc::new(Barrier::new(3));

        // Writer task
        let db_writer = Arc::clone(&db);
        let barrier_writer = Arc::clone(&barrier);
        let writer_handle = tokio::spawn(async move {
            barrier_writer.wait().await;
            for i in 0..10 {
                let span = create_test_span(&format!("span-{i}"), "session-1", i64::from(i) * 1000);
                if let Err(e) = db_writer.insert_span(&span).await {
                    return Err(TraceviewError::InvalidSpan { reason: e.to_string() });
                }
            }
            Ok::<_, TraceviewError>(())
        });

        // Reader task 1
        let db_reader1 = Arc::clone(&db);
        let barrier_reader1 = Arc::clone(&barrier);
        let reader1_handle = tokio::spawn(async move {
            barrier_reader1.wait().await;
            for _ in 0..10 {
                let _ = db_reader1.get_spans_by_session("session-1").await;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            Ok::<_, TraceviewError>(())
        });

        // Reader task 2
        let db_reader2 = Arc::clone(&db);
        let barrier_reader2 = barrier;
        let reader2_handle = tokio::spawn(async move {
            barrier_reader2.wait().await;
            for _ in 0..10 {
                let _ = db_reader2.get_sessions(10, 0).await;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            Ok::<_, TraceviewError>(())
        });

        // All tasks should complete without error
        let (w_result, r1_result, r2_result) =
            tokio::join!(writer_handle, reader1_handle, reader2_handle);

        assert!(w_result.is_ok());
        assert!(r1_result.is_ok());
        assert!(r2_result.is_ok());
    }

    #[tokio::test]
    async fn test_span_with_metadata() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        let mut span = create_test_span("span-meta", "session-1", 1000);
        span.metadata = Some(serde_json::json!({"custom_key": "custom_value", "count": 42}));

        db.insert_span(&span).await.unwrap_or_else(|e| {
            panic!("Failed to insert span: {e}");
        });

        let spans = db.get_spans_by_session("session-1").await.unwrap_or_default();
        let retrieved = spans.first();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap_or(&span);
        assert!(retrieved.metadata.is_some());
        let meta = retrieved.metadata.as_ref().unwrap_or(&serde_json::json!(null));
        assert_eq!(meta.get("custom_key").and_then(|v| v.as_str()), Some("custom_value"));
    }

    #[tokio::test]
    async fn test_all_span_kinds() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        let kinds = [
            SpanKind::User,
            SpanKind::Assistant,
            SpanKind::System,
            SpanKind::Thinking,
            SpanKind::ToolCall,
            SpanKind::ToolResult,
            SpanKind::Choice,
            SpanKind::Span,
        ];

        for (i, kind) in kinds.iter().enumerate() {
            let mut span = create_test_span(
                &format!("span-{i}"),
                "session-1",
                i64::try_from(i).unwrap_or(0) * 1000,
            );
            span.kind = *kind;
            db.insert_span(&span).await.unwrap_or_else(|e| {
                panic!("Failed to insert span with kind {kind:?}: {e}");
            });
        }

        let spans = db.get_spans_by_session("session-1").await.unwrap_or_default();
        assert_eq!(spans.len(), 8);

        // Verify kinds roundtrip correctly
        for (i, kind) in kinds.iter().enumerate() {
            let span = spans.get(i);
            assert!(span.is_some());
            assert_eq!(span.map(|s| s.kind), Some(*kind));
        }
    }

    #[tokio::test]
    async fn test_batch_writer_flushes_on_size() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        let (writer, tx) = BatchWriter::new(db.clone(), 3, Duration::from_secs(60));

        // Spawn the writer
        let writer_handle = tokio::spawn(async move { writer.run().await });

        // Send 3 spans (batch_size)
        for i in 0..3 {
            let span = create_test_span(&format!("span-{i}"), "session-1", i64::from(i) * 1000);
            tx.send(span).await.unwrap_or_else(|e| {
                panic!("Failed to send span: {e}");
            });
        }

        // Give time for flush
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Check spans were written
        let spans = db.get_spans_by_session("session-1").await.unwrap_or_default();
        assert_eq!(spans.len(), 3);

        // Close channel and wait for writer to finish
        drop(tx);
        let result = writer_handle.await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_batch_writer_flushes_on_interval() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        let (writer, tx) = BatchWriter::new(db.clone(), 100, Duration::from_millis(50));

        let writer_handle = tokio::spawn(async move { writer.run().await });

        // Send 2 spans (less than batch_size)
        for i in 0..2 {
            let span = create_test_span(&format!("span-{i}"), "session-1", i64::from(i) * 1000);
            tx.send(span).await.unwrap_or_else(|e| {
                panic!("Failed to send span: {e}");
            });
        }

        // Wait for interval to trigger flush
        tokio::time::sleep(Duration::from_millis(100)).await;

        let spans = db.get_spans_by_session("session-1").await.unwrap_or_default();
        assert_eq!(spans.len(), 2);

        drop(tx);
        let result = writer_handle.await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_batch_writer_flushes_on_close() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        let (writer, tx) = BatchWriter::new(db.clone(), 100, Duration::from_secs(60));

        let writer_handle = tokio::spawn(async move { writer.run().await });

        // Send 1 span
        let span = create_test_span("span-0", "session-1", 1000);
        tx.send(span).await.unwrap_or_else(|e| {
            panic!("Failed to send span: {e}");
        });

        // Close channel immediately
        drop(tx);

        // Wait for writer to finish
        let result = writer_handle.await;
        assert!(result.is_ok());

        // Verify span was flushed on close
        let spans = db.get_spans_by_session("session-1").await.unwrap_or_default();
        assert_eq!(spans.len(), 1);
    }

    #[tokio::test]
    async fn test_empty_batch_insert() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        let result = db.insert_spans(&[]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_span_replace_on_conflict() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        // Insert initial span
        let span1 = Span {
            id: "span-1".to_string(),
            session_id: "session-1".to_string(),
            parent_span_id: None,
            trace_id: "trace-1".to_string(),
            kind: SpanKind::User,
            model: None,
            content: Some("Original content".to_string()),
            metadata: None,
            start_time: 1000,
            end_time: None,
            duration_ms: None,
            input_tokens: None,
            output_tokens: None,
            finish_reason: None,
            tool_call_id: None,
            tool_name: None,
        };
        db.insert_span(&span1).await.unwrap_or_else(|e| {
            panic!("Failed to insert span: {e}");
        });

        // Insert span with same ID but different content
        let span2 = Span {
            id: "span-1".to_string(),
            session_id: "session-1".to_string(),
            parent_span_id: None,
            trace_id: "trace-1".to_string(),
            kind: SpanKind::User,
            model: None,
            content: Some("Updated content".to_string()),
            metadata: None,
            start_time: 1000,
            end_time: Some(2000),
            duration_ms: Some(1),
            input_tokens: None,
            output_tokens: None,
            finish_reason: None,
            tool_call_id: None,
            tool_name: None,
        };
        db.insert_span(&span2).await.unwrap_or_else(|e| {
            panic!("Failed to insert span: {e}");
        });

        // Verify only one span exists with updated content
        let spans = db.get_spans_by_session("session-1").await.unwrap_or_default();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans.first().and_then(|s| s.content.as_deref()), Some("Updated content"));
        assert_eq!(spans.first().and_then(|s| s.end_time), Some(2000));
    }

    #[tokio::test]
    async fn test_update_session_name_if_empty() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        // Create a session with no name
        let session = Session {
            id: "session-auto-name".to_string(),
            name: None,
            created_at: 1_000_000,
            updated_at: 1_000_000,
        };
        db.upsert_session(&session).await.unwrap_or_else(|e| {
            panic!("Failed to upsert session: {e}");
        });

        // Update the name
        db.update_session_name_if_empty("session-auto-name", "Auto-generated name")
            .await
            .unwrap_or_else(|e| {
                panic!("Failed to update session name: {e}");
            });

        // Verify the name was set
        let retrieved = db.get_session("session-auto-name").await.unwrap_or(None);
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap_or_else(|| create_test_session(""));
        assert_eq!(retrieved.name, Some("Auto-generated name".to_string()));
    }

    #[tokio::test]
    async fn test_update_session_name_preserves_existing() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        // Create a session with an existing name
        let session = Session {
            id: "session-existing-name".to_string(),
            name: Some("Existing Name".to_string()),
            created_at: 1_000_000,
            updated_at: 1_000_000,
        };
        db.upsert_session(&session).await.unwrap_or_else(|e| {
            panic!("Failed to upsert session: {e}");
        });

        // Try to update the name - should NOT change since name is already set
        db.update_session_name_if_empty("session-existing-name", "New auto name")
            .await
            .unwrap_or_else(|e| {
                panic!("Failed to update session name: {e}");
            });

        // Verify the original name is preserved
        let retrieved = db.get_session("session-existing-name").await.unwrap_or(None);
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap_or_else(|| create_test_session(""));
        assert_eq!(retrieved.name, Some("Existing Name".to_string()));
    }

    #[tokio::test]
    async fn test_update_session_name_nonexistent_session() {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });

        // Should not error when updating a nonexistent session
        let result = db.update_session_name_if_empty("nonexistent-session", "Some name").await;
        assert!(result.is_ok());
    }
}
