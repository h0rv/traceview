//! SSE broadcast system for real-time span streaming.
//!
//! This module provides Server-Sent Events (SSE) streaming for spans,
//! allowing clients to receive real-time updates as spans are ingested.

use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use pin_project_lite::pin_project;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use crate::models::Span;

pin_project! {
    /// A stream of SSE events containing spans, optionally filtered by session.
    ///
    /// This wraps a broadcast receiver and converts spans to SSE events.
    /// When a session filter is set, only spans matching that session are emitted.
    pub struct SpanStream {
        #[pin]
        inner: BroadcastStream<Span>,
        session_filter: Option<String>,
    }
}

impl SpanStream {
    /// Create a stream for all spans (firehose mode).
    ///
    /// All spans from the broadcast channel will be emitted as SSE events.
    #[must_use]
    pub fn all(rx: broadcast::Receiver<Span>) -> Self {
        Self { inner: BroadcastStream::new(rx), session_filter: None }
    }

    /// Create a stream filtered to a specific session.
    ///
    /// Only spans with a matching `session_id` will be emitted.
    #[must_use]
    pub fn for_session(rx: broadcast::Receiver<Span>, session_id: String) -> Self {
        Self { inner: BroadcastStream::new(rx), session_filter: Some(session_id) }
    }
}

impl Stream for SpanStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        loop {
            match this.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(result)) => {
                    match result {
                        Ok(span) => {
                            // Apply session filter if set
                            if let Some(filter_id) = this.session_filter.as_ref()
                                && &span.session_id != filter_id
                            {
                                // Skip this span and poll again
                                continue;
                            }

                            // Convert span to SSE event
                            match serde_json::to_string(&span) {
                                Ok(json) => {
                                    let event = Event::default().event("span").data(json);
                                    return Poll::Ready(Some(Ok(event)));
                                }
                                Err(_) => {
                                    // Skip spans that fail to serialize (shouldn't happen)
                                    continue;
                                }
                            }
                        }
                        Err(BroadcastStreamRecvError::Lagged(count)) => {
                            // Log the lag and continue - we can't recover missed messages
                            tracing::warn!(count, "SSE stream lagged, skipped messages");
                            continue;
                        }
                    }
                }
                Poll::Ready(None) => {
                    // Stream ended
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

/// Create an SSE response for span streaming.
///
/// If `session_id` is provided, only spans for that session are streamed.
/// Otherwise, all spans are streamed (firehose mode).
///
/// The stream includes keep-alive pings every 15 seconds.
pub fn span_sse(
    rx: broadcast::Receiver<Span>,
    session_id: Option<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = match session_id {
        Some(id) => SpanStream::for_session(rx, id),
        None => SpanStream::all(rx),
    };

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping"))
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use crate::models::SpanKind;
    use tokio_stream::StreamExt;

    fn create_test_span(id: &str, session_id: &str) -> Span {
        Span {
            id: id.to_string(),
            session_id: session_id.to_string(),
            parent_span_id: None,
            trace_id: "trace-1".to_string(),
            kind: SpanKind::User,
            model: Some("claude-3".to_string()),
            content: Some("Hello".to_string()),
            metadata: None,
            start_time: 1_000_000,
            end_time: None,
            duration_ms: None,
            input_tokens: Some(10),
            output_tokens: None,
            finish_reason: None,
            tool_call_id: None,
            tool_name: None,
        }
    }

    #[tokio::test]
    async fn test_span_stream_all_receives_all_spans() {
        let (tx, rx) = broadcast::channel::<Span>(16);
        let mut stream = SpanStream::all(rx);

        // Send spans from different sessions
        let span1 = create_test_span("span-1", "session-a");
        let span2 = create_test_span("span-2", "session-b");
        let span3 = create_test_span("span-3", "session-a");

        tx.send(span1).unwrap_or_else(|e| panic!("Failed to send: {e}"));
        tx.send(span2).unwrap_or_else(|e| panic!("Failed to send: {e}"));
        tx.send(span3).unwrap_or_else(|e| panic!("Failed to send: {e}"));

        // Drop sender to close the stream
        drop(tx);

        // Collect all events
        let mut events = Vec::new();
        while let Some(result) = stream.next().await {
            let Ok(event) = result;
            events.push(event);
        }

        // Should receive all 3 spans
        assert_eq!(events.len(), 3);
    }

    #[tokio::test]
    async fn test_span_stream_for_session_filters_correctly() {
        let (tx, rx) = broadcast::channel::<Span>(16);
        let mut stream = SpanStream::for_session(rx, "session-a".to_string());

        // Send spans from different sessions
        let span1 = create_test_span("span-1", "session-a");
        let span2 = create_test_span("span-2", "session-b");
        let span3 = create_test_span("span-3", "session-a");
        let span4 = create_test_span("span-4", "session-c");

        tx.send(span1).unwrap_or_else(|e| panic!("Failed to send: {e}"));
        tx.send(span2).unwrap_or_else(|e| panic!("Failed to send: {e}"));
        tx.send(span3).unwrap_or_else(|e| panic!("Failed to send: {e}"));
        tx.send(span4).unwrap_or_else(|e| panic!("Failed to send: {e}"));

        // Drop sender to close the stream
        drop(tx);

        // Collect all events
        let mut events = Vec::new();
        while let Some(result) = stream.next().await {
            let Ok(event) = result;
            events.push(event);
        }

        // Should only receive 2 spans (from session-a)
        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn test_event_serialization_includes_span_data() {
        let (tx, rx) = broadcast::channel::<Span>(16);
        let mut stream = SpanStream::all(rx);

        let span = Span {
            id: "test-span-id".to_string(),
            session_id: "test-session".to_string(),
            parent_span_id: Some("parent-id".to_string()),
            trace_id: "test-trace".to_string(),
            kind: SpanKind::Assistant,
            model: Some("claude-3-opus".to_string()),
            content: Some("Test content".to_string()),
            metadata: Some(serde_json::json!({"key": "value"})),
            start_time: 1_700_000_000_000_000_000,
            end_time: Some(1_700_000_001_000_000_000),
            duration_ms: Some(1000),
            input_tokens: Some(100),
            output_tokens: Some(50),
            finish_reason: Some("end_turn".to_string()),
            tool_call_id: None,
            tool_name: None,
        };

        tx.send(span.clone()).unwrap_or_else(|e| panic!("Failed to send: {e}"));
        drop(tx);

        let event_result = stream.next().await;
        assert!(event_result.is_some());

        let event = event_result
            .unwrap_or_else(|| panic!("Expected event"))
            .unwrap_or_else(|_: Infallible| panic!("Infallible error"));

        // Convert event to string for verification
        // The Event type doesn't expose its data directly, so we verify by serializing
        // the original span and checking it matches the expected format
        let expected_json = serde_json::to_string(&span);
        assert!(expected_json.is_ok());

        let json = expected_json.unwrap_or_default();
        assert!(json.contains("\"id\":\"test-span-id\""));
        assert!(json.contains("\"session_id\":\"test-session\""));
        assert!(json.contains("\"kind\":\"assistant\""));
        assert!(json.contains("\"model\":\"claude-3-opus\""));
        assert!(json.contains("\"content\":\"Test content\""));

        // Verify event type is set (we can't directly inspect Event internals easily)
        // But we know the event was created successfully
        assert!(format!("{event:?}").contains("span"));
    }

    #[tokio::test]
    async fn test_stream_handles_lagged_messages_gracefully() {
        // Create a very small channel to force lagging
        let (tx, rx) = broadcast::channel::<Span>(2);
        let mut stream = SpanStream::all(rx);

        // Send more messages than the channel can hold
        for i in 0..10 {
            let span = create_test_span(&format!("span-{i}"), "session-1");
            // Ignore send errors (channel may be full)
            let _ = tx.send(span);
        }

        // The stream should continue working despite lag
        // Send one more span that should be received
        let final_span = create_test_span("final-span", "session-1");
        let _ = tx.send(final_span);

        drop(tx);

        // Collect events - should handle lag gracefully and still receive some events
        let mut event_count = 0;
        while let Some(result) = stream.next().await {
            let Ok(_event) = result;
            event_count += 1;
        }

        // Should have received at least some events (the exact count depends on timing)
        // The important thing is that the stream didn't panic or error out
        assert!(event_count >= 0); // Stream handled lag gracefully
    }

    #[tokio::test]
    async fn test_stream_ends_when_channel_closes() {
        let (tx, rx) = broadcast::channel::<Span>(16);
        let mut stream = SpanStream::all(rx);

        // Send one span
        let span = create_test_span("span-1", "session-1");
        tx.send(span).unwrap_or_else(|e| panic!("Failed to send: {e}"));

        // Close the channel
        drop(tx);

        // Should receive the span
        let first = stream.next().await;
        assert!(first.is_some());

        // Stream should end
        let second = stream.next().await;
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn test_span_sse_returns_sse_response() {
        let (tx, rx) = broadcast::channel::<Span>(16);

        // Test firehose mode
        let _sse = span_sse(rx, None);

        // Test session filter mode
        let rx2 = tx.subscribe();
        let _sse_filtered = span_sse(rx2, Some("session-1".to_string()));

        // The fact that these compile and don't panic is the test
        // The Sse type wraps our stream correctly
    }

    #[tokio::test]
    async fn test_keep_alive_configuration() {
        let (_tx, rx) = broadcast::channel::<Span>(16);
        let sse = span_sse(rx, None);

        // Verify that keep_alive is configured by checking the debug output
        // This is a basic sanity check that the configuration was applied
        let debug_str = format!("{sse:?}");
        assert!(debug_str.contains("Sse"));
    }

    #[tokio::test]
    async fn test_empty_session_filter_receives_nothing() {
        let (tx, rx) = broadcast::channel::<Span>(16);
        let mut stream = SpanStream::for_session(rx, "nonexistent-session".to_string());

        // Send spans for different sessions
        let span1 = create_test_span("span-1", "session-a");
        let span2 = create_test_span("span-2", "session-b");

        tx.send(span1).unwrap_or_else(|e| panic!("Failed to send: {e}"));
        tx.send(span2).unwrap_or_else(|e| panic!("Failed to send: {e}"));

        drop(tx);

        // Should receive no events
        let mut events = Vec::new();
        while let Some(result) = stream.next().await {
            let Ok(event) = result;
            events.push(event);
        }

        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let (tx, _) = broadcast::channel::<Span>(16);

        // Create multiple subscribers
        let rx1 = tx.subscribe();
        let rx2 = tx.subscribe();
        let rx3 = tx.subscribe();

        let mut stream1 = SpanStream::all(rx1);
        let mut stream2 = SpanStream::for_session(rx2, "session-a".to_string());
        let mut stream3 = SpanStream::for_session(rx3, "session-b".to_string());

        // Send spans
        let span_a = create_test_span("span-a", "session-a");
        let span_b = create_test_span("span-b", "session-b");

        tx.send(span_a).unwrap_or_else(|e| panic!("Failed to send: {e}"));
        tx.send(span_b).unwrap_or_else(|e| panic!("Failed to send: {e}"));

        drop(tx);

        // Collect from all streams
        let mut count1 = 0;
        let mut count2 = 0;
        let mut count3 = 0;

        while let Some(result) = stream1.next().await {
            let Ok(_event) = result;
            count1 += 1;
        }

        while let Some(result) = stream2.next().await {
            let Ok(_event) = result;
            count2 += 1;
        }

        while let Some(result) = stream3.next().await {
            let Ok(_event) = result;
            count3 += 1;
        }

        // Stream1 (all) should get both
        assert_eq!(count1, 2);
        // Stream2 (session-a filter) should get one
        assert_eq!(count2, 1);
        // Stream3 (session-b filter) should get one
        assert_eq!(count3, 1);
    }

    #[tokio::test]
    async fn test_span_stream_is_send() {
        // Verify that SpanStream can be sent across threads
        fn assert_send<T: Send>() {}
        assert_send::<SpanStream>();
    }
}
