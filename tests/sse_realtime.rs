//! Real-time SSE test that verifies spans actually appear in the SSE stream.
//!
//! This test specifically checks that:
//! 1. SSE connection is established
//! 2. Trace is sent via OTLP
//! 3. SSE stream receives the span data

#![allow(unused_crate_dependencies)]
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used, clippy::single_match)]

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::time::timeout;

use traceview::api::{AppState, SharedState, create_router};
use traceview::db::Database;

async fn start_server() -> (String, SharedState) {
    let db = Database::new_in_memory().await.expect("DB failed");
    let state = Arc::new(AppState { db });
    let app = create_router(Arc::clone(&state));

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("Bind failed");
    let addr = listener.local_addr().expect("Addr failed");
    let url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    (url, state)
}

fn otlp_payload(session_id: &str, content: &str) -> String {
    format!(
        r#"{{
        "resourceSpans": [{{
            "resource": {{
                "attributes": [{{ "key": "session.id", "value": {{"stringValue": "{session_id}"}} }}]
            }},
            "scopeSpans": [{{
                "scope": {{"name": "test"}},
                "spans": [{{
                    "traceId": "abcd1234abcd1234abcd1234abcd1234",
                    "spanId": "1234567890abcdef",
                    "name": "chat",
                    "startTimeUnixNano": "1700000000000000000",
                    "endTimeUnixNano": "1700000001000000000",
                    "attributes": [],
                    "events": [{{
                        "name": "gen_ai.user.message",
                        "timeUnixNano": "1700000000100000000",
                        "attributes": [{{ "key": "gen_ai.content", "value": {{"stringValue": "{content}"}} }}]
                    }}]
                }}]
            }}]
        }}]
    }}"#
    )
}

/// THE MAIN TEST: Verify SSE receives spans sent after connection.
///
/// This test:
/// 1. Starts server
/// 2. Creates a session
/// 3. Connects to SSE stream for that session
/// 4. Sends a trace via OTLP
/// 5. Verifies SSE stream received the span data
#[tokio::test]
async fn test_sse_receives_span_after_connection() {
    let (base_url, state) = start_server().await;
    let session_id = "realtime-test";

    // Create session first
    let session = traceview::models::Session {
        id: session_id.to_string(),
        name: Some("Realtime Test".to_string()),
        created_at: 1000,
        updated_at: 1000,
    };
    state.db.upsert_session(&session).await.expect("Session creation failed");

    // Channel to receive SSE data from the background task
    let (tx, mut rx) = mpsc::channel::<String>(10);

    // Connect to SSE in background task
    let sse_url = format!("{}/sessions/{}/stream", base_url, session_id);
    let sse_task = tokio::spawn(async move {
        let client = reqwest::Client::new();
        let response = client
            .get(&sse_url)
            .header("accept", "text/event-stream")
            .send()
            .await
            .expect("SSE connection failed");

        println!("SSE connected, status: {}", response.status());

        // Stream the bytes
        let mut stream = response.bytes_stream();
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    let text = String::from_utf8_lossy(&chunk);
                    println!("SSE chunk received: {:?}", text);
                    if tx.send(text.to_string()).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    println!("SSE stream error: {}", e);
                    break;
                }
            }
        }
    });

    // Wait for SSE connection to establish
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Now send a trace via OTLP
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/traces", base_url))
        .header("content-type", "application/json")
        .body(otlp_payload(session_id, "Hello SSE!"))
        .send()
        .await
        .expect("OTLP send failed");

    assert!(resp.status().is_success(), "OTLP failed: {}", resp.status());
    println!("OTLP trace sent successfully");

    // Wait for SSE to receive the span
    let mut received_span = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);

    while tokio::time::Instant::now() < deadline {
        match timeout(Duration::from_millis(100), rx.recv()).await {
            Ok(Some(data)) => {
                println!("Received SSE data: {}", data);
                if data.contains("span") && data.contains("html") {
                    println!("SUCCESS: Received span data via SSE!");
                    received_span = true;
                    break;
                }
            }
            Ok(None) => {
                println!("SSE channel closed");
                break;
            }
            Err(_) => {
                // Timeout, keep waiting
            }
        }
    }

    // Cleanup
    sse_task.abort();

    // ASSERT: We should have received the span
    assert!(received_span, "FAILURE: SSE did not receive span data! The streaming is broken.");
}

/// Test that directly checks if the broadcast channel in the API state works.
#[tokio::test]
async fn test_api_broadcast_channel_works() {
    let (base_url, state) = start_server().await;

    // Subscribe to the state's database broadcast
    let mut rx = state.db.subscribe();

    // Send a trace via HTTP
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/traces", base_url))
        .header("content-type", "application/json")
        .body(otlp_payload("broadcast-test", "Testing broadcast"))
        .send()
        .await
        .expect("Request failed");

    assert!(resp.status().is_success());

    // Check if we received on the broadcast channel
    match timeout(Duration::from_secs(2), rx.recv()).await {
        Ok(Ok(span)) => {
            println!("Broadcast received span: {} in session {}", span.id, span.session_id);
            assert_eq!(span.session_id, "broadcast-test");
        }
        Ok(Err(e)) => {
            panic!("Broadcast channel error: {:?}", e);
        }
        Err(_) => {
            panic!("TIMEOUT: Broadcast channel did not receive span from HTTP ingest!");
        }
    }
}

/// Test SSE with firehose endpoint (all sessions)
#[tokio::test]
async fn test_firehose_receives_spans() {
    let (base_url, _state) = start_server().await;

    let (tx, mut rx) = mpsc::channel::<String>(10);

    // Connect to firehose SSE
    let firehose_url = format!("{}/stream", base_url);
    let sse_task = tokio::spawn(async move {
        let client = reqwest::Client::new();
        let response = client
            .get(&firehose_url)
            .header("accept", "text/event-stream")
            .send()
            .await
            .expect("Firehose connection failed");

        let mut stream = response.bytes_stream();
        while let Some(chunk_result) = stream.next().await {
            if let Ok(chunk) = chunk_result {
                let text = String::from_utf8_lossy(&chunk);
                if tx.send(text.to_string()).await.is_err() {
                    break;
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send trace
    let client = reqwest::Client::new();
    client
        .post(format!("{}/v1/traces", base_url))
        .header("content-type", "application/json")
        .body(otlp_payload("firehose-test", "Firehose message"))
        .send()
        .await
        .expect("OTLP failed");

    // Wait for SSE data
    let mut received = false;
    for _ in 0..30 {
        match timeout(Duration::from_millis(100), rx.recv()).await {
            Ok(Some(data)) => {
                println!("Firehose received: {}", data);
                if data.contains("span") {
                    received = true;
                    break;
                }
            }
            _ => {}
        }
    }

    sse_task.abort();
    assert!(received, "Firehose SSE did not receive span!");
}
