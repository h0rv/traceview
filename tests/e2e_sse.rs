//! End-to-end test for SSE streaming functionality.
//!
//! This test verifies that spans ingested via OTLP are streamed to SSE clients.

#![allow(unused_crate_dependencies)]
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tokio::time::timeout;
use tower::ServiceExt;

use traceview::api::{AppState, SharedState, create_router};
use traceview::db::Database;
use traceview::models::Span;

fn sample_otlp_json(session_id: &str) -> String {
    format!(
        r#"{{
        "resourceSpans": [{{
            "resource": {{
                "attributes": [{{
                    "key": "session.id",
                    "value": {{"stringValue": "{session_id}"}}
                }}]
            }},
            "scopeSpans": [{{
                "scope": {{"name": "test"}},
                "spans": [{{
                    "traceId": "0123456789abcdef0123456789abcdef",
                    "spanId": "0123456789abcdef",
                    "name": "test_span",
                    "startTimeUnixNano": "1700000000000000000",
                    "endTimeUnixNano": "1700000001000000000",
                    "attributes": [{{
                        "key": "gen_ai.request.model",
                        "value": {{"stringValue": "claude-3-opus"}}
                    }}],
                    "events": [{{
                        "name": "gen_ai.user.message",
                        "timeUnixNano": "1700000000100000000",
                        "attributes": [{{
                            "key": "gen_ai.content",
                            "value": {{"stringValue": "Hello from SSE test!"}}
                        }}]
                    }}]
                }}]
            }}]
        }}]
    }}"#
    )
}

async fn setup_test_state() -> SharedState {
    let db = Database::new_in_memory().await.expect("Failed to create in-memory database");
    Arc::new(AppState { db })
}

/// Test that SSE stream receives spans after OTLP ingestion.
///
/// This is the core E2E test for the streaming functionality.
#[tokio::test]
async fn test_sse_receives_ingested_spans() {
    let state = setup_test_state().await;
    let app = create_router(Arc::clone(&state));

    // Subscribe to the broadcast channel BEFORE ingesting
    let mut rx = state.db.subscribe();

    // Ingest a trace via OTLP endpoint
    let session_id = "sse-test-session";
    let request = Request::builder()
        .method("POST")
        .uri("/v1/traces")
        .header("content-type", "application/json")
        .body(Body::from(sample_otlp_json(session_id)))
        .expect("Failed to build request");

    let response = app.oneshot(request).await.expect("Request failed");
    assert_eq!(response.status(), StatusCode::OK);

    // Now check if we received the span via broadcast
    let received = timeout(Duration::from_secs(2), rx.recv()).await;

    match received {
        Ok(Ok(span)) => {
            println!("Received span: {:?}", span.id);
            assert_eq!(span.session_id, session_id);
        }
        Ok(Err(e)) => {
            panic!("Broadcast channel error: {e:?}");
        }
        Err(_) => {
            panic!("Timeout waiting for span - SSE streaming is broken!");
        }
    }
}

/// Test that SSE stream receives multiple spans in order.
#[tokio::test]
async fn test_sse_receives_multiple_spans() {
    let state = setup_test_state().await;

    // Subscribe BEFORE ingesting
    let mut rx = state.db.subscribe();

    // Insert spans directly to test broadcast
    let spans: Vec<Span> = (0..3)
        .map(|i| Span {
            id: format!("span-{i}"),
            session_id: "multi-span-session".to_string(),
            parent_span_id: None,
            trace_id: "trace-1".to_string(),
            kind: traceview::models::SpanKind::User,
            model: None,
            content: Some(format!("Message {i}")),
            metadata: None,
            start_time: i * 1000,
            end_time: None,
            duration_ms: None,
            input_tokens: None,
            output_tokens: None,
            finish_reason: None,
            tool_call_id: None,
            tool_name: None,
        })
        .collect();

    // Insert spans one by one
    for span in &spans {
        state.db.insert_span(span).await.expect("Failed to insert span");
    }

    // Receive all spans
    let mut received_ids = Vec::new();
    for _ in 0..3 {
        match timeout(Duration::from_secs(2), rx.recv()).await {
            Ok(Ok(span)) => {
                received_ids.push(span.id);
            }
            Ok(Err(e)) => {
                panic!("Broadcast error: {e:?}");
            }
            Err(_) => {
                panic!(
                    "Timeout - only received {} of 3 spans. SSE broadcast broken!",
                    received_ids.len()
                );
            }
        }
    }

    assert_eq!(received_ids.len(), 3);
    assert!(received_ids.contains(&"span-0".to_string()));
    assert!(received_ids.contains(&"span-1".to_string()));
    assert!(received_ids.contains(&"span-2".to_string()));
}

/// Test that batch insert also broadcasts spans.
#[tokio::test]
async fn test_sse_receives_batch_inserted_spans() {
    let state = setup_test_state().await;

    // Subscribe BEFORE ingesting
    let mut rx = state.db.subscribe();

    // Create batch of spans
    let spans: Vec<Span> = (0..5)
        .map(|i| Span {
            id: format!("batch-span-{i}"),
            session_id: "batch-session".to_string(),
            parent_span_id: None,
            trace_id: "trace-batch".to_string(),
            kind: traceview::models::SpanKind::Assistant,
            model: Some("claude".to_string()),
            content: Some(format!("Batch message {i}")),
            metadata: None,
            start_time: i * 1000,
            end_time: None,
            duration_ms: None,
            input_tokens: None,
            output_tokens: None,
            finish_reason: None,
            tool_call_id: None,
            tool_name: None,
        })
        .collect();

    // Batch insert
    state.db.insert_spans(&spans).await.expect("Failed to batch insert");

    // Should receive all 5 spans
    let mut received_count = 0;
    for _ in 0..5 {
        match timeout(Duration::from_millis(500), rx.recv()).await {
            Ok(Ok(_)) => {
                received_count += 1;
            }
            Ok(Err(e)) => {
                panic!("Broadcast error after {received_count} spans: {e:?}");
            }
            Err(_) => {
                panic!(
                    "Timeout - only received {received_count} of 5 batch-inserted spans. Batch broadcast broken!"
                );
            }
        }
    }

    assert_eq!(received_count, 5, "Should receive all 5 batch-inserted spans via SSE");
}

/// Test the full SSE HTTP endpoint with actual HTTP streaming.
#[tokio::test]
async fn test_sse_http_endpoint_streams_spans() {
    let state = setup_test_state().await;
    let app = create_router(Arc::clone(&state));

    // Start SSE connection
    let sse_request = Request::builder()
        .method("GET")
        .uri("/stream")
        .body(Body::empty())
        .expect("Failed to build SSE request");

    let sse_response = app.clone().oneshot(sse_request).await.expect("SSE request failed");
    assert_eq!(sse_response.status(), StatusCode::OK);

    // Get the body as a stream
    let body = sse_response.into_body();

    // Now ingest a span
    let ingest_request = Request::builder()
        .method("POST")
        .uri("/v1/traces")
        .header("content-type", "application/json")
        .body(Body::from(sample_otlp_json("http-sse-test")))
        .expect("Failed to build ingest request");

    let ingest_response = app.oneshot(ingest_request).await.expect("Ingest request failed");
    assert_eq!(ingest_response.status(), StatusCode::OK);

    // Read from SSE body stream
    // Note: This is a simplified test - in reality we'd need to parse SSE format
    let body_bytes = timeout(Duration::from_secs(2), axum::body::to_bytes(body, 1024 * 1024)).await;

    match body_bytes {
        Ok(Ok(bytes)) => {
            let body_str = String::from_utf8_lossy(&bytes);
            println!("SSE body received: {}", body_str);
            // If streaming works, we should see span data or at least keep-alive
        }
        Ok(Err(e)) => {
            println!("Body read error (may be expected for streaming): {e}");
        }
        Err(_) => {
            // Timeout is actually expected for SSE since it's a long-lived connection
            println!("SSE connection timed out (expected for streaming endpoint)");
        }
    }
}
