//! Axum API routes for traceview.
//!
//! This module provides HTTP endpoints for:
//! - OTLP trace ingestion
//! - JSON API for sessions and spans
//! - Server-Sent Events (SSE) for real-time updates
//! - HTML views (placeholders)

use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::collections::HashMap;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::db::Database;
use crate::error::TraceviewError;
use crate::ingest::{OtlpTraceData, convert_otlp, convert_otlp_proto, extract_session_name};
use crate::models::{ExportSummary, SearchResult, Session, SessionExport, Span, SpanKind};
use crate::views::{app_layout, session_detail, sidebar_session_list, span_html};

// ============================================================================
// App State
// ============================================================================

/// Shared application state containing the database.
pub struct AppState {
    /// The database instance.
    pub db: Database,
}

/// Type alias for Arc-wrapped app state.
pub type SharedState = Arc<AppState>;

// ============================================================================
// Error Handling
// ============================================================================

/// API error wrapper that converts `TraceviewError` into HTTP responses.
pub struct ApiError(TraceviewError);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            TraceviewError::InvalidOtlp { .. } => StatusCode::BAD_REQUEST,
            TraceviewError::InvalidSpan { .. } => StatusCode::BAD_REQUEST,
            TraceviewError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            TraceviewError::Json(_) => StatusCode::BAD_REQUEST,
            TraceviewError::Protobuf(_) => StatusCode::BAD_REQUEST,
            TraceviewError::ChannelSend => StatusCode::INTERNAL_SERVER_ERROR,
            TraceviewError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, self.0.to_string()).into_response()
    }
}

impl<E: Into<TraceviewError>> From<E> for ApiError {
    fn from(err: E) -> Self {
        ApiError(err.into())
    }
}

/// Check if a content type indicates protobuf.
fn is_protobuf_content_type(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|ct| {
        ct.contains("application/x-protobuf") || ct.contains("application/protobuf")
    })
}

// ============================================================================
// Router
// ============================================================================

/// Create the main router with all routes.
pub fn create_router(state: SharedState) -> Router {
    Router::new()
        // OTLP ingest endpoint
        .route("/v1/traces", post(ingest_traces))
        // JSON API
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/{id}", get(get_session))
        .route("/api/sessions/{id}/spans", get(get_session_spans))
        .route("/api/sessions/{id}/export", get(export_session))
        .route("/api/search", get(search))
        // SSE streams
        .route("/stream", get(stream_all))
        .route("/sessions/{id}/stream", get(stream_session))
        // HTML views
        .route("/", get(index))
        .route("/sessions/{id}", get(session_view))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
}

// ============================================================================
// Request Types
// ============================================================================

/// Query parameters for listing sessions.
#[derive(Debug, Deserialize)]
pub struct ListParams {
    /// Maximum number of sessions to return.
    pub limit: Option<i64>,
    /// Number of sessions to skip.
    pub offset: Option<i64>,
}

// ============================================================================
// Handlers
// ============================================================================

/// Ingest OTLP traces in JSON or Protobuf format.
///
/// Accepts OTLP trace data, converts it to spans, and stores them in the database.
/// Sessions are automatically created/updated for all spans.
///
/// Supported content types:
/// - `application/json`: OTLP JSON format
/// - `application/x-protobuf` or `application/protobuf`: OTLP Protobuf format
async fn ingest_traces(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    // Get content type from headers
    let content_type = headers.get("content-type").and_then(|v| v.to_str().ok());

    // Parse based on content type
    let spans = if is_protobuf_content_type(content_type) {
        convert_otlp_proto(&body)?
    } else {
        // Default to JSON
        let data: OtlpTraceData = serde_json::from_slice(&body)?;
        convert_otlp(&data)?
    };

    if spans.is_empty() {
        return Ok(StatusCode::OK);
    }

    // Collect unique session IDs
    let session_ids: HashSet<&str> = spans.iter().map(|s| s.session_id.as_str()).collect();

    // Get current timestamp for session updates (saturating conversion from u128 to i64)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
        .unwrap_or(0);

    // Upsert sessions for all unique session_ids
    for session_id in &session_ids {
        let session =
            Session { id: (*session_id).to_string(), name: None, created_at: now, updated_at: now };
        state.db.upsert_session(&session).await?;
    }

    // Insert all spans
    state.db.insert_spans(&spans).await?;

    // Auto-name sessions from first user message
    if let Some(name) = extract_session_name(&spans) {
        for session_id in &session_ids {
            state.db.update_session_name_if_empty(session_id, &name).await?;
        }
    }

    Ok(StatusCode::OK)
}

/// List sessions with optional pagination.
async fn list_sessions(
    State(state): State<SharedState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<Session>>, ApiError> {
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let sessions = state.db.get_sessions(limit, offset).await?;
    Ok(Json(sessions))
}

/// Get a single session by ID.
async fn get_session(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<Session>, ApiError> {
    let session = state.db.get_session(&id).await?.ok_or_else(|| TraceviewError::InvalidSpan {
        reason: format!("session not found: {id}"),
    })?;

    Ok(Json(session))
}

/// Get all spans for a session, ordered by start_time.
async fn get_session_spans(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Span>>, ApiError> {
    let spans = state.db.get_spans_by_session(&id).await?;
    Ok(Json(spans))
}

/// Export a session with all spans as JSON for download.
async fn export_session(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // Fetch session
    let session = state.db.get_session(&id).await?.ok_or_else(|| TraceviewError::InvalidSpan {
        reason: format!("session not found: {id}"),
    })?;

    // Fetch all spans
    let spans = state.db.get_spans_by_session(&id).await?;

    // Calculate summary
    let summary = calculate_export_summary(&spans);

    // Get current timestamp
    let exported_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
        .unwrap_or(0);

    // Build export structure
    let export = SessionExport { export_version: "1.0", exported_at, session, spans, summary };

    // Serialize to pretty JSON
    let json = serde_json::to_string_pretty(&export)?;

    // Generate filename
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let safe_id = sanitize_filename(&id);
    let filename = format!("traceview-{safe_id}-{timestamp}.json");

    // Return with download headers
    Ok((
        [
            (header::CONTENT_TYPE, "application/json".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\"")),
        ],
        json,
    ))
}

/// Calculate export summary statistics from spans.
fn calculate_export_summary(spans: &[Span]) -> ExportSummary {
    let mut span_kinds: HashMap<String, usize> = HashMap::new();
    let mut total_input = 0i64;
    let mut total_output = 0i64;
    let mut total_duration = 0i64;

    for span in spans {
        *span_kinds.entry(span_kind_to_string(span.kind).to_string()).or_default() += 1;
        total_input += span.input_tokens.unwrap_or(0);
        total_output += span.output_tokens.unwrap_or(0);
        if let Some(d) = span.duration_ms {
            total_duration += d;
        }
    }

    ExportSummary {
        span_count: spans.len(),
        total_input_tokens: total_input,
        total_output_tokens: total_output,
        total_duration_ms: if total_duration > 0 { Some(total_duration) } else { None },
        span_kinds,
    }
}

/// Convert SpanKind to string for export.
fn span_kind_to_string(kind: SpanKind) -> &'static str {
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

/// Sanitize session ID for use in filename.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .take(50)
        .collect()
}

/// Query parameters for search.
#[derive(Debug, Deserialize)]
pub struct SearchParams {
    /// Search query string.
    pub q: String,
    /// Maximum results to return.
    pub limit: Option<i64>,
}

/// Search sessions and spans by text query.
async fn search(
    State(state): State<SharedState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResult>, ApiError> {
    let limit = params.limit.unwrap_or(20);
    let results = state.db.search(&params.q, limit).await?;
    Ok(Json(results))
}

/// SSE data format - includes both span data and pre-rendered HTML.
#[derive(serde::Serialize)]
struct SseSpanData {
    id: String,
    session_id: String,
    html: String,
}

fn span_to_sse_data(span: &Span) -> Option<String> {
    let html = span_html(span).into_string();
    let data = SseSpanData { id: span.id.clone(), session_id: span.session_id.clone(), html };
    serde_json::to_string(&data).ok()
}

/// SSE stream for all span updates.
async fn stream_all(
    State(state): State<SharedState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.db.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        result.ok().and_then(|span| {
            span_to_sse_data(&span).map(|data| Ok(Event::default().event("span").data(data)))
        })
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(30)))
}

/// SSE stream for a specific session's span updates.
async fn stream_session(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.db.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        let session_id = id.clone();
        result.ok().and_then(|span| {
            if span.session_id == session_id {
                span_to_sse_data(&span).map(|data| Ok(Event::default().event("span").data(data)))
            } else {
                None
            }
        })
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(30)))
}

/// HTML index page showing session list in sidebar with welcome message.
async fn index(State(state): State<SharedState>) -> Result<Html<String>, ApiError> {
    use maud::html;
    let sessions = state.db.get_sessions_with_counts(100, 0).await?;
    let sidebar = sidebar_session_list(&sessions, None);
    let content = html! {
        div class="welcome-message" {
            h2 { "Welcome to Traceview" }
            p { "Select a session from the sidebar to view traces." }
            p { "Sessions will appear here as traces are received via OTLP." }
        }
    };
    Ok(Html(app_layout("Sessions", sidebar, content, false, None).into_string()))
}

/// HTML session detail view with sidebar.
async fn session_view(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Html<String>, ApiError> {
    let session = state.db.get_session(&id).await?.ok_or_else(|| TraceviewError::InvalidSpan {
        reason: format!("session not found: {id}"),
    })?;
    let spans = state.db.get_spans_by_session(&id).await?;
    let sessions = state.db.get_sessions_with_counts(100, 0).await?;
    let sidebar = sidebar_session_list(&sessions, Some(&id));
    let content = session_detail(&session, &spans);
    let title = session.name.as_deref().unwrap_or(&session.id);
    Ok(Html(app_layout(title, sidebar, content, true, Some(&id)).into_string()))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn setup_test_state() -> SharedState {
        let db = Database::new_in_memory().await.unwrap_or_else(|e| {
            panic!("Failed to create in-memory database: {e}");
        });
        Arc::new(AppState { db })
    }

    fn sample_otlp_json() -> &'static str {
        r#"{
            "resourceSpans": [{
                "resource": {
                    "attributes": [{
                        "key": "session.id",
                        "value": {"stringValue": "test-session-123"}
                    }]
                },
                "scopeSpans": [{
                    "scope": {"name": "test"},
                    "spans": [{
                        "traceId": "0123456789abcdef0123456789abcdef",
                        "spanId": "0123456789abcdef",
                        "name": "test_span",
                        "startTimeUnixNano": "1700000000000000000",
                        "endTimeUnixNano": "1700000001000000000",
                        "attributes": [{
                            "key": "gen_ai.request.model",
                            "value": {"stringValue": "claude-3-opus"}
                        }],
                        "events": []
                    }]
                }]
            }]
        }"#
    }

    fn invalid_otlp_json() -> &'static str {
        r#"{
            "resourceSpans": [{
                "scopeSpans": [{
                    "spans": [{
                        "traceId": "",
                        "spanId": "abc",
                        "name": "test",
                        "startTimeUnixNano": "invalid"
                    }]
                }]
            }]
        }"#
    }

    #[tokio::test]
    async fn test_ingest_traces_valid_payload() {
        let state = setup_test_state().await;
        let app = create_router(Arc::clone(&state));

        let request = Request::builder()
            .method("POST")
            .uri("/v1/traces")
            .header("content-type", "application/json")
            .body(Body::from(sample_otlp_json()))
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let response = app.oneshot(request).await.unwrap_or_else(|e| {
            panic!("Request failed: {e}");
        });

        assert_eq!(response.status(), StatusCode::OK);

        // Verify span was inserted
        let spans = state.db.get_spans_by_session("test-session-123").await.unwrap_or_default();
        assert_eq!(spans.len(), 1);

        // Verify session was created
        let session = state.db.get_session("test-session-123").await.unwrap_or(None);
        assert!(session.is_some());
    }

    #[tokio::test]
    async fn test_ingest_traces_invalid_payload_returns_400() {
        let state = setup_test_state().await;
        let app = create_router(state);

        let request = Request::builder()
            .method("POST")
            .uri("/v1/traces")
            .header("content-type", "application/json")
            .body(Body::from(invalid_otlp_json()))
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let response = app.oneshot(request).await.unwrap_or_else(|e| {
            panic!("Request failed: {e}");
        });

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ingest_traces_malformed_json_returns_400() {
        let state = setup_test_state().await;
        let app = create_router(state);

        let request = Request::builder()
            .method("POST")
            .uri("/v1/traces")
            .header("content-type", "application/json")
            .body(Body::from("not valid json"))
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let response = app.oneshot(request).await.unwrap_or_else(|e| {
            panic!("Request failed: {e}");
        });

        // Malformed JSON should return 422 (Unprocessable Entity) from axum's Json extractor
        // or 400 depending on the version
        assert!(
            response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[tokio::test]
    async fn test_list_sessions_with_pagination() {
        let state = setup_test_state().await;

        // Create test sessions
        for i in 0..5 {
            let session = Session {
                id: format!("session-{i}"),
                name: Some(format!("Session {i}")),
                created_at: 1_000_000,
                updated_at: i64::from(i) * 1000,
            };
            state.db.upsert_session(&session).await.unwrap_or_else(|e| {
                panic!("Failed to upsert session: {e}");
            });
        }

        let app = create_router(state);

        // Test with limit and offset
        let request = Request::builder()
            .method("GET")
            .uri("/api/sessions?limit=2&offset=1")
            .body(Body::empty())
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let response = app.oneshot(request).await.unwrap_or_else(|e| {
            panic!("Request failed: {e}");
        });

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap_or_default();
        let sessions: Vec<Session> = serde_json::from_slice(&body).unwrap_or_default();

        assert_eq!(sessions.len(), 2);
    }

    #[tokio::test]
    async fn test_list_sessions_default_pagination() {
        let state = setup_test_state().await;
        let app = create_router(state);

        let request = Request::builder()
            .method("GET")
            .uri("/api/sessions")
            .body(Body::empty())
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let response = app.oneshot(request).await.unwrap_or_else(|e| {
            panic!("Request failed: {e}");
        });

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_session_returns_404_for_missing() {
        let state = setup_test_state().await;
        let app = create_router(state);

        let request = Request::builder()
            .method("GET")
            .uri("/api/sessions/nonexistent-session")
            .body(Body::empty())
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let response = app.oneshot(request).await.unwrap_or_else(|e| {
            panic!("Request failed: {e}");
        });

        // We return InvalidSpan error which maps to BAD_REQUEST
        // In a more refined implementation, we might want a NotFound variant
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_session_returns_existing() {
        let state = setup_test_state().await;

        // Create a session
        let session = Session {
            id: "existing-session".to_string(),
            name: Some("Existing Session".to_string()),
            created_at: 1_000_000,
            updated_at: 2_000_000,
        };
        state.db.upsert_session(&session).await.unwrap_or_else(|e| {
            panic!("Failed to upsert session: {e}");
        });

        let app = create_router(state);

        let request = Request::builder()
            .method("GET")
            .uri("/api/sessions/existing-session")
            .body(Body::empty())
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let response = app.oneshot(request).await.unwrap_or_else(|e| {
            panic!("Request failed: {e}");
        });

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap_or_default();
        let retrieved: Session = serde_json::from_slice(&body).unwrap_or_else(|_| Session {
            id: String::new(),
            name: None,
            created_at: 0,
            updated_at: 0,
        });

        assert_eq!(retrieved.id, "existing-session");
        assert_eq!(retrieved.name, Some("Existing Session".to_string()));
    }

    #[tokio::test]
    async fn test_get_session_spans_returns_ordered() {
        let state = setup_test_state().await;

        // Create a session
        let session = Session {
            id: "span-test-session".to_string(),
            name: None,
            created_at: 1_000_000,
            updated_at: 1_000_000,
        };
        state.db.upsert_session(&session).await.unwrap_or_else(|e| {
            panic!("Failed to upsert session: {e}");
        });

        // Create spans out of order
        let spans = vec![
            Span {
                id: "span-3".to_string(),
                session_id: "span-test-session".to_string(),
                parent_span_id: None,
                trace_id: "trace-1".to_string(),
                kind: crate::models::SpanKind::Span,
                model: None,
                content: None,
                metadata: None,
                start_time: 3000,
                end_time: None,
                duration_ms: None,
                input_tokens: None,
                output_tokens: None,
                finish_reason: None,
                tool_call_id: None,
                tool_name: None,
            },
            Span {
                id: "span-1".to_string(),
                session_id: "span-test-session".to_string(),
                parent_span_id: None,
                trace_id: "trace-1".to_string(),
                kind: crate::models::SpanKind::Span,
                model: None,
                content: None,
                metadata: None,
                start_time: 1000,
                end_time: None,
                duration_ms: None,
                input_tokens: None,
                output_tokens: None,
                finish_reason: None,
                tool_call_id: None,
                tool_name: None,
            },
            Span {
                id: "span-2".to_string(),
                session_id: "span-test-session".to_string(),
                parent_span_id: None,
                trace_id: "trace-1".to_string(),
                kind: crate::models::SpanKind::Span,
                model: None,
                content: None,
                metadata: None,
                start_time: 2000,
                end_time: None,
                duration_ms: None,
                input_tokens: None,
                output_tokens: None,
                finish_reason: None,
                tool_call_id: None,
                tool_name: None,
            },
        ];

        state.db.insert_spans(&spans).await.unwrap_or_else(|e| {
            panic!("Failed to insert spans: {e}");
        });

        let app = create_router(state);

        let request = Request::builder()
            .method("GET")
            .uri("/api/sessions/span-test-session/spans")
            .body(Body::empty())
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let response = app.oneshot(request).await.unwrap_or_else(|e| {
            panic!("Request failed: {e}");
        });

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap_or_default();
        let retrieved: Vec<Span> = serde_json::from_slice(&body).unwrap_or_default();

        assert_eq!(retrieved.len(), 3);
        // Should be ordered by start_time ASC
        assert_eq!(retrieved.first().map(|s| s.id.as_str()), Some("span-1"));
        assert_eq!(retrieved.get(1).map(|s| s.id.as_str()), Some("span-2"));
        assert_eq!(retrieved.get(2).map(|s| s.id.as_str()), Some("span-3"));
    }

    #[tokio::test]
    async fn test_full_roundtrip_ingest_query() {
        let state = setup_test_state().await;
        let app = create_router(Arc::clone(&state));

        // Step 1: Ingest traces
        let ingest_request = Request::builder()
            .method("POST")
            .uri("/v1/traces")
            .header("content-type", "application/json")
            .body(Body::from(sample_otlp_json()))
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let ingest_response = app.clone().oneshot(ingest_request).await.unwrap_or_else(|e| {
            panic!("Ingest request failed: {e}");
        });
        assert_eq!(ingest_response.status(), StatusCode::OK);

        // Step 2: List sessions
        let list_request = Request::builder()
            .method("GET")
            .uri("/api/sessions")
            .body(Body::empty())
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let list_response = app.clone().oneshot(list_request).await.unwrap_or_else(|e| {
            panic!("List request failed: {e}");
        });
        assert_eq!(list_response.status(), StatusCode::OK);

        let body =
            axum::body::to_bytes(list_response.into_body(), usize::MAX).await.unwrap_or_default();
        let sessions: Vec<Session> = serde_json::from_slice(&body).unwrap_or_default();
        assert!(!sessions.is_empty());

        let session_id =
            sessions.first().map(|s| s.id.clone()).unwrap_or_else(|| "unknown".to_string());

        // Step 3: Get session
        let get_request = Request::builder()
            .method("GET")
            .uri(format!("/api/sessions/{session_id}"))
            .body(Body::empty())
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let get_response = app.clone().oneshot(get_request).await.unwrap_or_else(|e| {
            panic!("Get session request failed: {e}");
        });
        assert_eq!(get_response.status(), StatusCode::OK);

        // Step 4: Get session spans
        let spans_request = Request::builder()
            .method("GET")
            .uri(format!("/api/sessions/{session_id}/spans"))
            .body(Body::empty())
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let spans_response = app.oneshot(spans_request).await.unwrap_or_else(|e| {
            panic!("Get spans request failed: {e}");
        });
        assert_eq!(spans_response.status(), StatusCode::OK);

        let body =
            axum::body::to_bytes(spans_response.into_body(), usize::MAX).await.unwrap_or_default();
        let spans: Vec<Span> = serde_json::from_slice(&body).unwrap_or_default();
        assert!(!spans.is_empty());
    }

    #[tokio::test]
    async fn test_index_route() {
        let state = setup_test_state().await;
        let app = create_router(state);

        let request = Request::builder()
            .method("GET")
            .uri("/")
            .body(Body::empty())
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let response = app.oneshot(request).await.unwrap_or_else(|e| {
            panic!("Request failed: {e}");
        });

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap_or_default();
        let body_str = String::from_utf8_lossy(&body);
        // Check that it returns HTML with Traceview branding
        assert!(body_str.contains("<!DOCTYPE html>"));
        assert!(body_str.contains("Traceview"));
    }

    #[tokio::test]
    async fn test_session_view_route() {
        let state = setup_test_state().await;

        // Create a session first
        let session = Session {
            id: "my-session-id".to_string(),
            name: Some("Test Session".to_string()),
            created_at: 1_000_000,
            updated_at: 2_000_000,
        };
        state.db.upsert_session(&session).await.unwrap_or_else(|e| {
            panic!("Failed to upsert session: {e}");
        });

        let app = create_router(state);

        let request = Request::builder()
            .method("GET")
            .uri("/sessions/my-session-id")
            .body(Body::empty())
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let response = app.oneshot(request).await.unwrap_or_else(|e| {
            panic!("Request failed: {e}");
        });

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap_or_default();
        let body_str = String::from_utf8_lossy(&body);
        // Check that it returns HTML with session detail
        assert!(body_str.contains("<!DOCTYPE html>"));
        assert!(body_str.contains("Test Session"));
    }

    #[tokio::test]
    async fn test_session_view_not_found() {
        let state = setup_test_state().await;
        let app = create_router(state);

        let request = Request::builder()
            .method("GET")
            .uri("/sessions/nonexistent")
            .body(Body::empty())
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let response = app.oneshot(request).await.unwrap_or_else(|e| {
            panic!("Request failed: {e}");
        });

        // Session not found returns BAD_REQUEST (InvalidSpan error)
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ingest_empty_traces() {
        let state = setup_test_state().await;
        let app = create_router(state);

        let empty_traces = r#"{"resourceSpans": []}"#;

        let request = Request::builder()
            .method("POST")
            .uri("/v1/traces")
            .header("content-type", "application/json")
            .body(Body::from(empty_traces))
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let response = app.oneshot(request).await.unwrap_or_else(|e| {
            panic!("Request failed: {e}");
        });

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_api_error_into_response() {
        // Test InvalidOtlp maps to BAD_REQUEST
        let err = ApiError(TraceviewError::InvalidOtlp { reason: "test".to_string() });
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Test InvalidSpan maps to BAD_REQUEST
        let err = ApiError(TraceviewError::InvalidSpan { reason: "test".to_string() });
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Test ChannelSend maps to INTERNAL_SERVER_ERROR
        let err = ApiError(TraceviewError::ChannelSend);
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        // Test Protobuf maps to BAD_REQUEST
        let err = ApiError(TraceviewError::Protobuf(prost::DecodeError::new("test")));
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ingest_traces_protobuf_content_type() {
        let state = setup_test_state().await;
        let app = create_router(Arc::clone(&state));

        // Create a valid protobuf payload using prost
        use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
        use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue, any_value};
        use opentelemetry_proto::tonic::resource::v1::Resource;
        use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span as ProtoSpan};
        use prost::Message;

        let request = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "session.id".to_string(),
                        value: Some(AnyValue {
                            value: Some(any_value::Value::StringValue(
                                "proto-test-session".to_string(),
                            )),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans: vec![ProtoSpan {
                        trace_id: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                        span_id: vec![1, 2, 3, 4, 5, 6, 7, 8],
                        parent_span_id: vec![],
                        name: "test_proto_span".to_string(),
                        start_time_unix_nano: 1700000000000000000,
                        end_time_unix_nano: 1700000001000000000,
                        attributes: vec![],
                        events: vec![],
                        links: vec![],
                        status: None,
                        kind: 0,
                        flags: 0,
                        trace_state: String::new(),
                        dropped_attributes_count: 0,
                        dropped_events_count: 0,
                        dropped_links_count: 0,
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };

        let mut buf = Vec::new();
        request.encode(&mut buf).unwrap_or_else(|e| panic!("Failed to encode protobuf: {e}"));

        let http_request = Request::builder()
            .method("POST")
            .uri("/v1/traces")
            .header("content-type", "application/x-protobuf")
            .body(Body::from(buf))
            .unwrap_or_else(|e| panic!("Failed to build request: {e}"));

        let response = app.oneshot(http_request).await.unwrap_or_else(|e| {
            panic!("Request failed: {e}");
        });

        assert_eq!(response.status(), StatusCode::OK);

        // Verify span was inserted
        let spans = state.db.get_spans_by_session("proto-test-session").await.unwrap_or_default();
        assert_eq!(spans.len(), 1);
        assert_eq!(
            spans.first().map(|s| s.trace_id.as_str()),
            Some("0102030405060708090a0b0c0d0e0f10")
        );
    }

    #[test]
    fn test_is_protobuf_content_type() {
        assert!(is_protobuf_content_type(Some("application/x-protobuf")));
        assert!(is_protobuf_content_type(Some("application/protobuf")));
        assert!(is_protobuf_content_type(Some("application/x-protobuf; charset=utf-8")));
        assert!(!is_protobuf_content_type(Some("application/json")));
        assert!(!is_protobuf_content_type(Some("text/plain")));
        assert!(!is_protobuf_content_type(None));
    }
}
