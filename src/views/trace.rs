//! Session detail and span rendering views for traceview.

use chrono::{TimeZone, Utc};
use maud::{Markup, html};

use crate::models::{Session, Span, SpanKind};

/// Renders the session detail page with all spans.
///
/// # Arguments
/// * `session` - The session to display
/// * `spans` - Slice of spans belonging to this session
pub fn session_detail(session: &Session, spans: &[Span]) -> Markup {
    let display_name = session.name.as_deref().unwrap_or(&session.id);

    html! {
        div class="session-header" {
            h2 { (display_name) }
            div class="session-info" {
                p { "Session ID: " code { (session.id) } }
                p { "Created: " (format_timestamp(session.created_at)) }
                p { "Updated: " (format_timestamp(session.updated_at)) }
            }
        }

        div id="spans-container" data-session-id=(session.id) {
            @if spans.is_empty() {
                div class="empty-state" {
                    p { "No spans recorded yet." }
                    p { "Spans will appear here as they are received." }
                }
            } @else {
                @for span in spans {
                    (span_html(span))
                }
            }
        }
    }
}

/// Renders a single span as HTML.
///
/// This function is used for both initial render and SSE updates.
///
/// # Arguments
/// * `span` - The span to render
pub fn span_html(span: &Span) -> Markup {
    let kind_str = span_kind_to_string(span.kind);

    html! {
        div class="span"
            data-kind=(kind_str)
            data-span-id=(span.id) {

            div class="span-header" {
                span class="span-kind" { (kind_str) }
                div class="span-meta" {
                    @if let Some(model) = &span.model {
                        span { "Model: " (model) }
                        " | "
                    }
                    @if let Some(duration_ms) = span.duration_ms {
                        span { (format_duration(duration_ms)) }
                    } @else {
                        span { "In progress..." }
                    }
                }
            }

            @if let Some(tool_name) = &span.tool_name {
                div class="span-tool" {
                    strong { "Tool: " }
                    code { (tool_name) }
                    @if let Some(tool_call_id) = &span.tool_call_id {
                        " "
                        small { "(" (tool_call_id) ")" }
                    }
                }
            }

            @if let Some(content) = &span.content {
                div class="span-content" { (content) }
            }

            @if span.input_tokens.is_some() || span.output_tokens.is_some() {
                div class="span-tokens" {
                    @if let Some(input) = span.input_tokens {
                        span { "Input: " (input) " tokens" }
                    }
                    @if span.input_tokens.is_some() && span.output_tokens.is_some() {
                        " | "
                    }
                    @if let Some(output) = span.output_tokens {
                        span { "Output: " (output) " tokens" }
                    }
                }
            }

            @if let Some(finish_reason) = &span.finish_reason {
                div class="span-finish" {
                    small { "Finish reason: " (finish_reason) }
                }
            }
        }
    }
}

/// Converts a `SpanKind` to its string representation for use in data attributes.
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

/// Formats a duration in milliseconds to a human-readable string.
fn format_duration(ms: i64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        let secs = ms as f64 / 1000.0;
        format!("{secs:.2}s")
    } else {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) / 1000;
        format!("{mins}m {secs}s")
    }
}

/// Formats a Unix nanosecond timestamp into a human-readable string.
fn format_timestamp(nanos: i64) -> String {
    let secs = nanos / 1_000_000_000;
    // SAFETY: modulo 1_000_000_000 guarantees the value fits in u32 (max ~999_999_999)
    #[allow(clippy::cast_possible_truncation)]
    let nsecs = (nanos % 1_000_000_000).unsigned_abs() as u32;

    Utc.timestamp_opt(secs, nsecs)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "Unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_session() -> Session {
        Session {
            id: "test-session".to_string(),
            name: Some("Test Session".to_string()),
            created_at: 1_700_000_000_000_000_000,
            updated_at: 1_700_000_100_000_000_000,
        }
    }

    fn create_test_span(kind: SpanKind) -> Span {
        Span {
            id: "span-123".to_string(),
            session_id: "test-session".to_string(),
            parent_span_id: None,
            trace_id: "trace-456".to_string(),
            kind,
            model: Some("claude-3-opus".to_string()),
            content: Some("Test content".to_string()),
            metadata: None,
            start_time: 1_700_000_000_000_000_000,
            end_time: Some(1_700_000_001_000_000_000),
            duration_ms: Some(1000),
            input_tokens: Some(100),
            output_tokens: Some(50),
            finish_reason: Some("end_turn".to_string()),
            tool_call_id: None,
            tool_name: None,
        }
    }

    #[test]
    fn test_session_detail_renders_header() {
        let session = create_test_session();
        let spans = vec![];
        let result = session_detail(&session, &spans);
        let html_str = result.into_string();

        assert!(html_str.contains("Test Session"));
        assert!(html_str.contains("test-session"));
        assert!(html_str.contains("session-header"));
    }

    #[test]
    fn test_session_detail_renders_spans() {
        let session = create_test_session();
        let spans = vec![create_test_span(SpanKind::User), create_test_span(SpanKind::Assistant)];
        let result = session_detail(&session, &spans);
        let html_str = result.into_string();

        assert!(html_str.contains("spans-container"));
        assert!(html_str.contains("data-session-id=\"test-session\""));
    }

    #[test]
    fn test_session_detail_empty_spans() {
        let session = create_test_session();
        let spans: Vec<Span> = vec![];
        let result = session_detail(&session, &spans);
        let html_str = result.into_string();

        assert!(html_str.contains("No spans recorded"));
        assert!(html_str.contains("empty-state"));
    }

    #[test]
    fn test_span_html_user_kind() {
        let span = create_test_span(SpanKind::User);
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("data-kind=\"user\""));
        assert!(html_str.contains("span-kind"));
    }

    #[test]
    fn test_span_html_assistant_kind() {
        let span = create_test_span(SpanKind::Assistant);
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("data-kind=\"assistant\""));
    }

    #[test]
    fn test_span_html_thinking_kind() {
        let span = create_test_span(SpanKind::Thinking);
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("data-kind=\"thinking\""));
    }

    #[test]
    fn test_span_html_tool_call_kind() {
        let mut span = create_test_span(SpanKind::ToolCall);
        span.tool_name = Some("web_search".to_string());
        span.tool_call_id = Some("call-abc".to_string());
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("data-kind=\"tool_call\""));
        assert!(html_str.contains("web_search"));
        assert!(html_str.contains("call-abc"));
    }

    #[test]
    fn test_span_html_tool_result_kind() {
        let span = create_test_span(SpanKind::ToolResult);
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("data-kind=\"tool_result\""));
    }

    #[test]
    fn test_span_html_system_kind() {
        let span = create_test_span(SpanKind::System);
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("data-kind=\"system\""));
    }

    #[test]
    fn test_span_html_choice_kind() {
        let span = create_test_span(SpanKind::Choice);
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("data-kind=\"choice\""));
    }

    #[test]
    fn test_span_html_generic_span_kind() {
        let span = create_test_span(SpanKind::Span);
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("data-kind=\"span\""));
    }

    #[test]
    fn test_span_html_shows_content() {
        let span = create_test_span(SpanKind::User);
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("Test content"));
        assert!(html_str.contains("span-content"));
    }

    #[test]
    fn test_span_html_shows_tokens() {
        let span = create_test_span(SpanKind::Assistant);
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("100"));
        assert!(html_str.contains("50"));
        assert!(html_str.contains("tokens"));
    }

    #[test]
    fn test_span_html_shows_duration() {
        let span = create_test_span(SpanKind::User);
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("1.00s"));
    }

    #[test]
    fn test_span_html_shows_model() {
        let span = create_test_span(SpanKind::Assistant);
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("claude-3-opus"));
    }

    #[test]
    fn test_span_html_in_progress() {
        let mut span = create_test_span(SpanKind::Assistant);
        span.duration_ms = None;
        span.end_time = None;
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("In progress..."));
    }

    #[test]
    fn test_span_kind_to_string_all_variants() {
        assert_eq!(span_kind_to_string(SpanKind::User), "user");
        assert_eq!(span_kind_to_string(SpanKind::Assistant), "assistant");
        assert_eq!(span_kind_to_string(SpanKind::System), "system");
        assert_eq!(span_kind_to_string(SpanKind::Thinking), "thinking");
        assert_eq!(span_kind_to_string(SpanKind::ToolCall), "tool_call");
        assert_eq!(span_kind_to_string(SpanKind::ToolResult), "tool_result");
        assert_eq!(span_kind_to_string(SpanKind::Choice), "choice");
        assert_eq!(span_kind_to_string(SpanKind::Span), "span");
    }

    #[test]
    fn test_format_duration_milliseconds() {
        assert_eq!(format_duration(500), "500ms");
        assert_eq!(format_duration(999), "999ms");
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(1000), "1.00s");
        assert_eq!(format_duration(1500), "1.50s");
        assert_eq!(format_duration(59990), "59.99s"); // 59999 rounds to 60.00
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(60000), "1m 0s");
        assert_eq!(format_duration(90000), "1m 30s");
        assert_eq!(format_duration(125000), "2m 5s");
    }
}
