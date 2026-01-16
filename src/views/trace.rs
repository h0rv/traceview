//! Session detail and span rendering views for traceview.

use chrono::{TimeZone, Utc};
use maud::{Markup, html};
use serde_json::Value;

use crate::models::{Session, Span, SpanKind};

// =============================================================================
// Message Parsing (from OTEL GenAI metadata)
// =============================================================================

/// A parsed message from OTEL GenAI format.
#[derive(Debug, Clone)]
struct ParsedMessage {
    /// Role: "system", "user", or "assistant"
    role: String,
    /// Text content of the message
    content: String,
    /// Tool calls embedded in assistant messages
    tool_calls: Vec<ParsedToolCall>,
}

/// A tool call parsed from assistant message.
#[derive(Debug, Clone)]
struct ParsedToolCall {
    name: String,
    arguments: String,
}

/// Check if a span has embedded messages in metadata.
fn has_embedded_messages(span: &Span) -> bool {
    span.metadata.as_ref().is_some_and(|m| {
        m.get("gen_ai.input.messages").is_some() || m.get("gen_ai.output.messages").is_some()
    })
}

/// Parse messages from span metadata (gen_ai.input.messages and gen_ai.output.messages).
fn parse_messages_from_metadata(span: &Span) -> Vec<ParsedMessage> {
    let mut messages = Vec::new();
    let Some(metadata) = &span.metadata else {
        return messages;
    };

    // Parse input messages (system, user)
    if let Some(input) = metadata.get("gen_ai.input.messages") {
        messages.extend(parse_message_array(input));
    }

    // Parse output messages (assistant response)
    if let Some(output) = metadata.get("gen_ai.output.messages") {
        messages.extend(parse_message_array(output));
    }

    messages
}

/// Parse a JSON value containing an array of messages.
fn parse_message_array(value: &Value) -> Vec<ParsedMessage> {
    // Value is typically a JSON string containing an array
    let Some(json_str) = value.as_str() else {
        return Vec::new();
    };

    let arr: Vec<Value> = match serde_json::from_str(json_str) {
        Ok(a) => a,
        Err(_) => return Vec::new(),
    };

    arr.iter()
        .filter_map(|msg| {
            let role = msg.get("role")?.as_str()?.to_string();
            let parts = msg.get("parts")?.as_array()?;

            let mut content_parts = Vec::new();
            let mut tool_calls = Vec::new();

            for part in parts {
                match part.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(text) = part.get("content").and_then(|c| c.as_str()) {
                            content_parts.push(text.to_string());
                        }
                    }
                    Some("tool_call") => {
                        let name = part
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let arguments = part
                            .get("arguments")
                            .map(|v| {
                                if v.is_string() {
                                    v.as_str().unwrap_or("").to_string()
                                } else {
                                    serde_json::to_string(v).unwrap_or_default()
                                }
                            })
                            .unwrap_or_default();
                        tool_calls.push(ParsedToolCall { name, arguments });
                    }
                    _ => {}
                }
            }

            Some(ParsedMessage { role, content: content_parts.join("\n"), tool_calls })
        })
        .collect()
}

/// Truncate text to a maximum length, adding ellipsis if needed.
fn truncate_text(text: &str, max_len: usize) -> String {
    let text = text.trim();
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}…", &text[..max_len.saturating_sub(1)])
    }
}

/// Extract cost from span metadata.
fn extract_cost(span: &Span) -> Option<f64> {
    span.metadata.as_ref()?.get("operation.cost")?.as_f64()
}

/// Format model name to be more compact (strip provider prefix if present).
fn format_model_name(model: &str) -> &str {
    // Strip common prefixes like "anthropic:" or "openai:"
    model.split(':').next_back().unwrap_or(model)
}

/// The role of a conversation turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnRole {
    /// User message turn.
    User,
    /// Assistant response turn, includes any tool calls as nested.
    Assistant,
    /// System message turn.
    System,
}

/// A conversation turn grouping related spans.
#[derive(Debug)]
struct ConversationTurn {
    /// The role of this turn.
    role: TurnRole,
    /// The spans that belong to this turn.
    spans: Vec<Span>,
}

/// Summary of token usage across all spans in a session.
#[derive(Debug, Default, PartialEq, Eq)]
struct TokenSummary {
    total_input: i64,
    total_output: i64,
    span_count: usize,
}

impl TokenSummary {
    /// Returns the total number of tokens (input + output).
    fn total(&self) -> i64 {
        self.total_input + self.total_output
    }
}

/// Calculates token summary statistics from a slice of spans.
///
/// Spans with `None` values for input or output tokens are treated as 0.
fn calculate_token_summary(spans: &[Span]) -> TokenSummary {
    spans.iter().fold(TokenSummary::default(), |mut acc, span| {
        acc.total_input += span.input_tokens.unwrap_or(0);
        acc.total_output += span.output_tokens.unwrap_or(0);
        acc.span_count += 1;
        acc
    })
}

/// Renders the token summary as a compact HTML bar.
fn token_summary_html(summary: &TokenSummary) -> Markup {
    html! {
        div class="token-summary" {
            span class="token-stat" { (summary.span_count) " spans" }
            span class="token-stat" { (format_tokens(summary.total_input)) " input" }
            span class="token-stat" { (format_tokens(summary.total_output)) " output" }
            span class="token-stat token-total" { (format_tokens(summary.total())) " total" }
        }
    }
}

/// Formats a token count with thousand separators for readability.
fn format_tokens(count: i64) -> String {
    if count < 1000 {
        count.to_string()
    } else if count < 1_000_000 {
        format!("{:.1}k", count as f64 / 1000.0)
    } else {
        format!("{:.2}M", count as f64 / 1_000_000.0)
    }
}

/// Groups spans into conversation turns based on their kind.
///
/// User spans start a new turn, Assistant/Tool spans belong to assistant turns.
/// Wrapper spans (SpanKind::Span and SpanKind::Choice) are filtered out.
fn group_into_turns(spans: &[Span]) -> Vec<ConversationTurn> {
    let mut turns: Vec<ConversationTurn> = Vec::new();

    for span in spans {
        // Skip wrapper spans
        if span.kind == SpanKind::Span || span.kind == SpanKind::Choice {
            continue;
        }

        match span.kind {
            SpanKind::User => {
                // Start new user turn
                turns.push(ConversationTurn { role: TurnRole::User, spans: vec![span.clone()] });
            }
            SpanKind::System => {
                // System messages get their own turn
                turns.push(ConversationTurn { role: TurnRole::System, spans: vec![span.clone()] });
            }
            SpanKind::Assistant
            | SpanKind::ToolCall
            | SpanKind::ToolResult
            | SpanKind::Thinking => {
                // Add to current assistant turn or create new one
                if let Some(last) = turns.last_mut()
                    && matches!(last.role, TurnRole::Assistant)
                {
                    last.spans.push(span.clone());
                    continue;
                }
                turns.push(ConversationTurn {
                    role: TurnRole::Assistant,
                    spans: vec![span.clone()],
                });
            }
            // Span and Choice are already filtered above
            SpanKind::Span | SpanKind::Choice => {}
        }
    }
    turns
}

/// Renders a conversation turn as HTML.
fn turn_html(turn: &ConversationTurn) -> Markup {
    let turn_class = match turn.role {
        TurnRole::User => "turn turn-user",
        TurnRole::Assistant => "turn turn-assistant",
        TurnRole::System => "turn turn-system",
    };

    html! {
        div class=(turn_class) {
            @for span in &turn.spans {
                @if span.kind == SpanKind::Assistant && has_embedded_messages(span) {
                    // Expand assistant span to show individual messages
                    (render_expanded_messages(span))
                } @else {
                    (span_html(span))
                }
            }
        }
    }
}

/// Renders expanded messages from an assistant span's metadata.
fn render_expanded_messages(span: &Span) -> Markup {
    let messages = parse_messages_from_metadata(span);

    html! {
        @for msg in &messages {
            (render_message_span(&msg, span))
        }
    }
}

/// Renders a single parsed message as a span-like element.
fn render_message_span(msg: &ParsedMessage, parent_span: &Span) -> Markup {
    let kind_str = msg.role.to_uppercase();
    let preview = truncate_text(&msg.content, 150);
    let data_kind = msg.role.as_str();

    html! {
        div class="span expandable-span"
            data-kind=(data_kind)
            data-expandable="true" {

            div class="span-header" {
                div class="span-header-left" {
                    span class="span-kind" { (kind_str) }
                    span class="expand-icon" { "▸" }
                }

                // Show model/tokens only for assistant
                @if msg.role == "assistant" {
                    div class="span-meta" {
                        @if let Some(model) = &parent_span.model {
                            small { (format_model_name(model)) }
                        }
                        @if let Some(duration_ms) = parent_span.duration_ms {
                            small { " · " (format_duration(duration_ms)) }
                        }
                        @if let Some(input) = parent_span.input_tokens {
                            small { " · " (input) " in" }
                        }
                        @if let Some(output) = parent_span.output_tokens {
                            small { " / " (output) " out" }
                        }
                    }
                }
            }

            // Preview (shown when collapsed)
            @if !preview.is_empty() {
                div class="span-preview" { (preview) }
            }

            // Full content (hidden by default, shown on expand)
            div class="span-detail hidden" {
                div class="detail-content" { (msg.content) }

                // Tool calls for assistant messages
                @if !msg.tool_calls.is_empty() {
                    div class="detail-section" {
                        div class="detail-label" { "Tool Calls" }
                        div class="tool-calls-list" {
                            @for tc in &msg.tool_calls {
                                div class="tool-call-item" {
                                    code { (tc.name) }
                                    span class="tool-args" { "(" (truncate_text(&tc.arguments, 60)) ")" }
                                }
                            }
                        }
                    }
                }

                // Metadata only for assistant
                @if msg.role == "assistant" {
                    (render_metadata_section(parent_span))
                }
            }
        }
    }
}

/// Renders the metadata section for a span.
fn render_metadata_section(span: &Span) -> Markup {
    html! {
        div class="detail-section metadata-section" {
            div class="detail-label" { "Metadata" }
            div class="metadata-grid" {
                @if let Some(model) = &span.model {
                    div class="meta-row" {
                        span class="meta-key" { "Model" }
                        span class="meta-value" { (model) }
                    }
                }
                @if let Some(input) = span.input_tokens {
                    div class="meta-row" {
                        span class="meta-key" { "Input tokens" }
                        span class="meta-value" { (input) }
                    }
                }
                @if let Some(output) = span.output_tokens {
                    div class="meta-row" {
                        span class="meta-key" { "Output tokens" }
                        span class="meta-value" { (output) }
                    }
                }
                @if let Some(cost) = extract_cost(span) {
                    div class="meta-row" {
                        span class="meta-key" { "Cost" }
                        span class="meta-value" { (format!("${cost:.4}")) }
                    }
                }
                @if let Some(reason) = &span.finish_reason {
                    div class="meta-row" {
                        span class="meta-key" { "Finish reason" }
                        span class="meta-value" { (reason) }
                    }
                }
            }
        }
    }
}

/// Renders the full conversation view from spans.
fn conversation_view(spans: &[Span]) -> Markup {
    let turns = group_into_turns(spans);
    html! {
        div class="conversation" {
            @for turn in &turns {
                (turn_html(turn))
            }
        }
    }
}

/// Checks if a span has an error finish reason.
fn is_error_span(span: &Span) -> bool {
    span.finish_reason
        .as_deref()
        .is_some_and(|r| r == "error" || r.contains("error") || r == "content_filter")
}

/// Renders the session detail page with all spans.
///
/// # Arguments
/// * `session` - The session to display
/// * `spans` - Slice of spans belonging to this session
pub fn session_detail(session: &Session, spans: &[Span]) -> Markup {
    let display_name = session.name.as_deref().unwrap_or(&session.id);
    let summary = calculate_token_summary(spans);

    html! {
        // Session header with name and token summary
        div class="session-header" {
            div class="session-header-top" {
                h2 { (display_name) }
                a href={ "/api/sessions/" (session.id) "/export" }
                    class="export-btn"
                    title="Download session as JSON"
                    download {
                    // Download icon SVG
                    (maud::PreEscaped(r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>"#))
                    " Export"
                }
            }
            (token_summary_html(&summary))
            div class="session-info" {
                p { "Session ID: " code { (session.id) } }
                p { "Created: " (format_timestamp(session.created_at)) }
                p { "Updated: " (format_timestamp(session.updated_at)) }
            }
        }

        // Spans content (container/filters provided by app_layout)
        @if spans.is_empty() {
            div class="empty-state" {
                p { "No spans recorded yet." }
                p { "Spans will appear here as they are received." }
            }
        } @else {
            (conversation_view(spans))
        }
    }
}

/// Renders a single span as HTML.
///
/// This function is used for both initial render and SSE updates.
/// Tool calls and tool results use collapsible details/summary elements.
///
/// # Arguments
/// * `span` - The span to render
pub fn span_html(span: &Span) -> Markup {
    let kind_str = span_kind_to_string(span.kind);

    // Tool calls and tool results use collapsible details/summary
    if matches!(span.kind, SpanKind::ToolCall | SpanKind::ToolResult) {
        return tool_span_html(span, kind_str);
    }

    let has_error = is_error_span(span);

    html! {
        div class="span"
            data-kind=(kind_str)
            data-span-id=(span.id)
            data-start-time=(span.start_time)
            data-end-time=(span.end_time.map(|t| t.to_string()).unwrap_or_default())
            data-has-error=(if has_error { "true" } else { "false" }) {

            div class="span-header" {
                span class="span-kind" { (kind_str) }
                div class="span-meta" {
                    @if let Some(model) = &span.model {
                        small { (model) }
                    }
                    @if let Some(duration_ms) = span.duration_ms {
                        small { " · " (format_duration(duration_ms)) }
                    }
                    @if let Some(input) = span.input_tokens {
                        small { " · " (input) " in" }
                    }
                    @if let Some(output) = span.output_tokens {
                        small { " / " (output) " out" }
                    }
                }
            }

            @if let Some(content) = &span.content {
                div class="span-content" { (content) }
            }
        }
    }
}

/// Extracts tool arguments and response from span metadata.
fn extract_tool_content(span: &Span) -> Option<(Option<String>, Option<String>)> {
    let metadata = span.metadata.as_ref()?;
    let args = metadata.get("tool_arguments").and_then(|v| {
        if v.is_string() {
            v.as_str().map(|s| s.to_string())
        } else {
            serde_json::to_string_pretty(v).ok()
        }
    });
    let response = metadata.get("tool_response").and_then(|v| {
        if v.is_string() {
            v.as_str().map(|s| s.to_string())
        } else {
            serde_json::to_string_pretty(v).ok()
        }
    });
    if args.is_some() || response.is_some() { Some((args, response)) } else { None }
}

/// Renders a tool span (ToolCall or ToolResult) with expandable details.
fn tool_span_html(span: &Span, kind_str: &str) -> Markup {
    let tool_name = span.tool_name.as_deref().unwrap_or("unknown");

    // Hide wrapper spans with "unknown" tool name - these are pydantic-ai's "running N tools" spans
    if tool_name == "unknown" {
        return html! {
            div class="span wrapper-span"
                data-kind=(kind_str)
                data-span-id=(span.id)
                data-start-time=(span.start_time)
                data-end-time=(span.end_time.map(|t| t.to_string()).unwrap_or_default())
                data-has-error="false" {}
        };
    }

    let has_error = is_error_span(span);

    // Extract tool content from metadata or use span content
    let (args, response) = extract_tool_content(span).unwrap_or((None, None));
    let args_preview = args.as_ref().map(|a| truncate_text(a, 60));

    html! {
        div class="span tool-span expandable-span"
            data-kind=(kind_str)
            data-span-id=(span.id)
            data-start-time=(span.start_time)
            data-end-time=(span.end_time.map(|t| t.to_string()).unwrap_or_default())
            data-has-error=(if has_error { "true" } else { "false" })
            data-expandable="true" {

            div class="span-header" {
                div class="span-header-left" {
                    span class="expand-icon" { "▸" }
                    code class="tool-name" { (tool_name) }
                    @if let Some(preview) = &args_preview {
                        span class="tool-args-preview" { "(" (preview) ")" }
                    }
                }
                div class="span-meta" {
                    @if let Some(duration_ms) = span.duration_ms {
                        small { (format_duration(duration_ms)) }
                    }
                }
            }

            // Expandable detail
            div class="span-detail hidden" {
                @if let Some(args) = &args {
                    div class="detail-section" {
                        div class="detail-label" { "Arguments" }
                        pre class="detail-code" { (args) }
                    }
                }
                @if let Some(response) = &response {
                    div class="detail-section" {
                        div class="detail-label" { "Response" }
                        pre class="detail-code" { (response) }
                    }
                }
                @if args.is_none() && response.is_none() {
                    @if let Some(content) = &span.content {
                        div class="detail-section" {
                            div class="detail-label" { "Content" }
                            pre class="detail-code" { (content) }
                        }
                    } @else {
                        div class="detail-section" {
                            small class="no-content" { "No content available" }
                        }
                    }
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

        // session_detail now returns spans directly without container (app_layout provides it)
        assert!(html_str.contains("data-kind=\"user\""));
        assert!(html_str.contains("data-kind=\"assistant\""));
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
        // Tool name shown in code element with class
        assert!(html_str.contains("class=\"tool-name\""));
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
        // Token counts shown as "N in / M out" in simplified structure
        assert!(html_str.contains("in"));
        assert!(html_str.contains("out"));
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

        // Simplified structure doesn't show explicit "In progress..." text
        // Just verify span renders without duration
        assert!(html_str.contains("data-kind=\"assistant\""));
        assert!(!html_str.contains("1.00s")); // No duration shown
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

    #[test]
    fn test_span_timing_displays_duration() {
        let span = create_test_span(SpanKind::Assistant);
        let result = span_html(&span);
        let html_str = result.into_string();

        // Duration shown in span-meta section
        assert!(html_str.contains("span-meta"));
        assert!(html_str.contains("1.00s"));
    }

    #[test]
    fn test_span_timing_displays_pending_for_in_progress() {
        let mut span = create_test_span(SpanKind::Assistant);
        span.duration_ms = None;
        span.end_time = None;
        let result = span_html(&span);
        let html_str = result.into_string();

        // Simplified structure doesn't show timing for in-progress spans
        assert!(html_str.contains("span-meta"));
        assert!(!html_str.contains("1.00s")); // No duration
    }

    #[test]
    fn test_calculate_token_summary_basic() {
        let spans = vec![create_test_span(SpanKind::User), create_test_span(SpanKind::Assistant)];
        let summary = calculate_token_summary(&spans);

        assert_eq!(summary.total_input, 200);
        assert_eq!(summary.total_output, 100);
        assert_eq!(summary.span_count, 2);
        assert_eq!(summary.total(), 300);
    }

    #[test]
    fn test_calculate_token_summary_with_null_tokens() {
        let mut span1 = create_test_span(SpanKind::User);
        span1.input_tokens = None;
        span1.output_tokens = Some(50);

        let mut span2 = create_test_span(SpanKind::Assistant);
        span2.input_tokens = Some(100);
        span2.output_tokens = None;

        let spans = vec![span1, span2];
        let summary = calculate_token_summary(&spans);

        assert_eq!(summary.total_input, 100);
        assert_eq!(summary.total_output, 50);
        assert_eq!(summary.span_count, 2);
    }

    #[test]
    fn test_calculate_token_summary_empty_spans() {
        let spans: Vec<Span> = vec![];
        let summary = calculate_token_summary(&spans);

        assert_eq!(summary.total_input, 0);
        assert_eq!(summary.total_output, 0);
        assert_eq!(summary.span_count, 0);
        assert_eq!(summary.total(), 0);
    }

    #[test]
    fn test_token_summary_html_renders() {
        let summary = TokenSummary { total_input: 1500, total_output: 500, span_count: 5 };
        let result = token_summary_html(&summary);
        let html_str = result.into_string();

        assert!(html_str.contains("token-summary"));
        assert!(html_str.contains("token-stat"));
        assert!(html_str.contains("5 spans"));
        assert!(html_str.contains("1.5k input"));
        assert!(html_str.contains("500 output"));
        assert!(html_str.contains("2.0k total"));
    }

    #[test]
    fn test_format_tokens_small() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(999), "999");
    }

    #[test]
    fn test_format_tokens_thousands() {
        assert_eq!(format_tokens(1000), "1.0k");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(999_999), "1000.0k");
    }

    #[test]
    fn test_format_tokens_millions() {
        assert_eq!(format_tokens(1_000_000), "1.00M");
        assert_eq!(format_tokens(2_500_000), "2.50M");
    }

    #[test]
    fn test_session_detail_includes_token_summary() {
        let session = create_test_session();
        let spans = vec![create_test_span(SpanKind::User), create_test_span(SpanKind::Assistant)];
        let result = session_detail(&session, &spans);
        let html_str = result.into_string();

        assert!(html_str.contains("token-summary"));
        assert!(html_str.contains("2 spans"));
        assert!(html_str.contains("200 input"));
        assert!(html_str.contains("100 output"));
        assert!(html_str.contains("300 total"));
    }

    #[test]
    fn test_span_html_tool_call_uses_expandable_structure() {
        let mut span = create_test_span(SpanKind::ToolCall);
        span.tool_name = Some("read_file".to_string());
        span.tool_call_id = Some("call-123".to_string());
        span.content = Some("file contents here".to_string());

        let result = span_html(&span);
        let html_str = result.into_string();

        // Verify expandable span structure
        assert!(html_str.contains("expandable-span"));
        assert!(html_str.contains("span-detail"));
        assert!(html_str.contains("expand-icon"));

        // Verify tool name is present
        assert!(html_str.contains("read_file"));
        assert!(html_str.contains("tool-name"));

        // Verify content is in detail section
        assert!(html_str.contains("file contents here"));

        // Verify duration is shown
        assert!(html_str.contains("1.00s"));

        // Verify data attributes still work
        assert!(html_str.contains("data-kind=\"tool_call\""));
        assert!(html_str.contains("data-expandable=\"true\""));
    }

    #[test]
    fn test_span_html_tool_result_uses_expandable_structure() {
        let mut span = create_test_span(SpanKind::ToolResult);
        span.tool_name = Some("bash".to_string());
        span.content = Some("command output".to_string());
        span.duration_ms = Some(250);

        let result = span_html(&span);
        let html_str = result.into_string();

        // Verify expandable span structure
        assert!(html_str.contains("expandable-span"));
        assert!(html_str.contains("span-detail"));
        assert!(html_str.contains("expand-icon"));

        // Verify tool name is present
        assert!(html_str.contains("bash"));
        assert!(html_str.contains("tool-name"));

        // Verify content is in detail section
        assert!(html_str.contains("command output"));

        // Verify duration
        assert!(html_str.contains("250ms"));

        // Verify data attributes
        assert!(html_str.contains("data-kind=\"tool_result\""));
        assert!(html_str.contains("data-expandable=\"true\""));
    }

    #[test]
    fn test_tool_span_hidden_when_no_tool_name() {
        let span = create_test_span(SpanKind::ToolCall);
        // tool_name is None by default in create_test_span

        let result = span_html(&span);
        let html_str = result.into_string();

        // Wrapper spans with "unknown" tool name are hidden
        assert!(html_str.contains("wrapper-span"));
    }

    #[test]
    fn test_tool_span_in_progress_no_duration() {
        let mut span = create_test_span(SpanKind::ToolCall);
        span.tool_name = Some("web_search".to_string());
        span.duration_ms = None;

        let result = span_html(&span);
        let html_str = result.into_string();

        // Simplified structure just doesn't show duration for in-progress
        assert!(html_str.contains("web_search"));
        assert!(!html_str.contains("1.00s"));
    }

    #[test]
    fn test_span_html_renders_content() {
        let span = create_test_span(SpanKind::User);
        let result = span_html(&span);
        let html_str = result.into_string();

        // Content should be rendered in span-content div
        assert!(html_str.contains("span-content"));
        assert!(html_str.contains("Test content"));
    }

    #[test]
    fn test_span_html_no_content_wrapper_when_no_content() {
        let mut span = create_test_span(SpanKind::User);
        span.content = None;
        let result = span_html(&span);
        let html_str = result.into_string();

        // Should NOT have span-content div when no content
        assert!(!html_str.contains("span-content"));
    }

    #[test]
    fn test_tool_span_renders_content() {
        let mut span = create_test_span(SpanKind::ToolCall);
        span.tool_name = Some("read_file".to_string());
        span.content = Some("file contents".to_string());

        let result = span_html(&span);
        let html_str = result.into_string();

        // Content should be in span-detail section
        assert!(html_str.contains("span-detail"));
        assert!(html_str.contains("file contents"));
    }

    #[test]
    fn test_tool_span_has_detail_section_always() {
        let mut span = create_test_span(SpanKind::ToolCall);
        span.tool_name = Some("read_file".to_string());
        span.content = None;

        let result = span_html(&span);
        let html_str = result.into_string();

        // span-detail div always present for consistent structure
        assert!(html_str.contains("span-detail"));
        assert!(html_str.contains("No content available"));
    }

    #[test]
    fn test_session_detail_does_not_include_filter_controls() {
        // Filter controls are now in app_layout, not session_detail
        let session = create_test_session();
        let spans = vec![create_test_span(SpanKind::User)];
        let result = session_detail(&session, &spans);
        let html_str = result.into_string();

        // session_detail should NOT include filter controls (app_layout provides them)
        assert!(!html_str.contains("filter-btn"));
        assert!(!html_str.contains("data-filter="));
    }

    #[test]
    fn test_span_has_data_kind_attribute() {
        let span = create_test_span(SpanKind::User);
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("data-kind=\"user\""));
    }

    #[test]
    fn test_span_has_data_has_error_attribute() {
        let span = create_test_span(SpanKind::User);
        let result = span_html(&span);
        let html_str = result.into_string();

        // Default span has finish_reason="end_turn", which is not an error
        assert!(html_str.contains("data-has-error=\"false\""));
    }

    #[test]
    fn test_span_has_error_true_for_error_finish_reason() {
        let mut span = create_test_span(SpanKind::User);
        span.finish_reason = Some("error".to_string());
        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("data-has-error=\"true\""));
    }

    #[test]
    fn test_is_error_span_detects_error_finish_reason() {
        let mut span = create_test_span(SpanKind::User);

        // Test exact "error" match
        span.finish_reason = Some("error".to_string());
        assert!(is_error_span(&span));

        // Test contains "error"
        span.finish_reason = Some("rate_limit_error".to_string());
        assert!(is_error_span(&span));

        // Test "content_filter" match
        span.finish_reason = Some("content_filter".to_string());
        assert!(is_error_span(&span));

        // Test non-error finish reasons
        span.finish_reason = Some("end_turn".to_string());
        assert!(!is_error_span(&span));

        span.finish_reason = Some("stop".to_string());
        assert!(!is_error_span(&span));

        // Test None finish_reason
        span.finish_reason = None;
        assert!(!is_error_span(&span));
    }

    #[test]
    fn test_tool_span_has_data_has_error_attribute() {
        let mut span = create_test_span(SpanKind::ToolCall);
        span.tool_name = Some("bash".to_string());
        span.finish_reason = Some("error".to_string());

        let result = span_html(&span);
        let html_str = result.into_string();

        assert!(html_str.contains("data-has-error=\"true\""));
    }

    fn create_test_span_with_id(kind: SpanKind, id: &str) -> Span {
        let mut span = create_test_span(kind);
        span.id = id.to_string();
        span
    }

    #[test]
    fn test_group_into_turns_basic_flow() {
        let spans = vec![
            create_test_span_with_id(SpanKind::User, "user-1"),
            create_test_span_with_id(SpanKind::Assistant, "assistant-1"),
        ];

        let turns = group_into_turns(&spans);

        assert_eq!(turns.len(), 2);
        assert_eq!(turns.first().map(|t| t.role), Some(TurnRole::User));
        assert_eq!(turns.first().map(|t| t.spans.len()), Some(1));
        assert_eq!(turns.get(1).map(|t| t.role), Some(TurnRole::Assistant));
        assert_eq!(turns.get(1).map(|t| t.spans.len()), Some(1));
    }

    #[test]
    fn test_group_into_turns_with_tools() {
        let mut tool_call = create_test_span_with_id(SpanKind::ToolCall, "tool-call-1");
        tool_call.tool_name = Some("bash".to_string());
        let mut tool_result = create_test_span_with_id(SpanKind::ToolResult, "tool-result-1");
        tool_result.tool_name = Some("bash".to_string());

        let spans = vec![
            create_test_span_with_id(SpanKind::User, "user-1"),
            create_test_span_with_id(SpanKind::Assistant, "assistant-1"),
            tool_call,
            tool_result,
            create_test_span_with_id(SpanKind::Assistant, "assistant-2"),
        ];

        let turns = group_into_turns(&spans);

        assert_eq!(turns.len(), 2);
        assert_eq!(turns.first().map(|t| t.role), Some(TurnRole::User));
        assert_eq!(turns.get(1).map(|t| t.role), Some(TurnRole::Assistant));
        // Assistant turn should contain: assistant-1, tool_call, tool_result, assistant-2
        assert_eq!(turns.get(1).map(|t| t.spans.len()), Some(4));
    }

    #[test]
    fn test_group_into_turns_skips_wrapper_spans() {
        let spans = vec![
            create_test_span_with_id(SpanKind::Span, "wrapper-1"),
            create_test_span_with_id(SpanKind::User, "user-1"),
            create_test_span_with_id(SpanKind::Choice, "choice-1"),
            create_test_span_with_id(SpanKind::Assistant, "assistant-1"),
            create_test_span_with_id(SpanKind::Span, "wrapper-2"),
        ];

        let turns = group_into_turns(&spans);

        assert_eq!(turns.len(), 2);
        // Verify wrapper spans are not included
        for turn in &turns {
            for span in &turn.spans {
                assert!(span.kind != SpanKind::Span);
                assert!(span.kind != SpanKind::Choice);
            }
        }
    }

    #[test]
    fn test_group_into_turns_empty_spans() {
        let spans: Vec<Span> = vec![];
        let turns = group_into_turns(&spans);
        assert!(turns.is_empty());
    }

    #[test]
    fn test_group_into_turns_only_wrapper_spans() {
        let spans = vec![
            create_test_span_with_id(SpanKind::Span, "wrapper-1"),
            create_test_span_with_id(SpanKind::Choice, "choice-1"),
        ];

        let turns = group_into_turns(&spans);
        assert!(turns.is_empty());
    }

    #[test]
    fn test_group_into_turns_system_gets_own_turn() {
        let spans = vec![
            create_test_span_with_id(SpanKind::System, "system-1"),
            create_test_span_with_id(SpanKind::User, "user-1"),
            create_test_span_with_id(SpanKind::Assistant, "assistant-1"),
        ];

        let turns = group_into_turns(&spans);

        assert_eq!(turns.len(), 3);
        assert_eq!(turns.first().map(|t| t.role), Some(TurnRole::System));
        assert_eq!(turns.get(1).map(|t| t.role), Some(TurnRole::User));
        assert_eq!(turns.get(2).map(|t| t.role), Some(TurnRole::Assistant));
    }

    #[test]
    fn test_group_into_turns_thinking_in_assistant() {
        let spans = vec![
            create_test_span_with_id(SpanKind::User, "user-1"),
            create_test_span_with_id(SpanKind::Thinking, "thinking-1"),
            create_test_span_with_id(SpanKind::Assistant, "assistant-1"),
        ];

        let turns = group_into_turns(&spans);

        assert_eq!(turns.len(), 2);
        assert_eq!(turns.get(1).map(|t| t.role), Some(TurnRole::Assistant));
        // Thinking and Assistant should be in the same turn
        assert_eq!(turns.get(1).map(|t| t.spans.len()), Some(2));
    }

    #[test]
    fn test_conversation_view_renders_turns() {
        let spans = vec![
            create_test_span_with_id(SpanKind::User, "user-1"),
            create_test_span_with_id(SpanKind::Assistant, "assistant-1"),
        ];

        let result = conversation_view(&spans);
        let html_str = result.into_string();

        assert!(html_str.contains("class=\"conversation\""));
        assert!(html_str.contains("class=\"turn turn-user\""));
        assert!(html_str.contains("class=\"turn turn-assistant\""));
    }

    #[test]
    fn test_conversation_view_empty_spans() {
        let spans: Vec<Span> = vec![];
        let result = conversation_view(&spans);
        let html_str = result.into_string();

        assert!(html_str.contains("class=\"conversation\""));
        // Should have empty conversation div
        assert!(!html_str.contains("class=\"turn"));
    }

    #[test]
    fn test_turn_html_user_turn() {
        let turn = ConversationTurn {
            role: TurnRole::User,
            spans: vec![create_test_span(SpanKind::User)],
        };

        let result = turn_html(&turn);
        let html_str = result.into_string();

        assert!(html_str.contains("class=\"turn turn-user\""));
        assert!(html_str.contains("data-kind=\"user\""));
    }

    #[test]
    fn test_turn_html_assistant_turn() {
        let turn = ConversationTurn {
            role: TurnRole::Assistant,
            spans: vec![create_test_span(SpanKind::Assistant)],
        };

        let result = turn_html(&turn);
        let html_str = result.into_string();

        assert!(html_str.contains("class=\"turn turn-assistant\""));
        assert!(html_str.contains("data-kind=\"assistant\""));
    }

    #[test]
    fn test_turn_html_system_turn() {
        let turn = ConversationTurn {
            role: TurnRole::System,
            spans: vec![create_test_span(SpanKind::System)],
        };

        let result = turn_html(&turn);
        let html_str = result.into_string();

        assert!(html_str.contains("class=\"turn turn-system\""));
        assert!(html_str.contains("data-kind=\"system\""));
    }

    #[test]
    fn test_turn_html_multiple_spans() {
        let mut tool_call = create_test_span_with_id(SpanKind::ToolCall, "tool-1");
        tool_call.tool_name = Some("read_file".to_string());

        let turn = ConversationTurn {
            role: TurnRole::Assistant,
            spans: vec![create_test_span_with_id(SpanKind::Assistant, "assistant-1"), tool_call],
        };

        let result = turn_html(&turn);
        let html_str = result.into_string();

        assert!(html_str.contains("class=\"turn turn-assistant\""));
        assert!(html_str.contains("data-kind=\"assistant\""));
        assert!(html_str.contains("data-kind=\"tool_call\""));
    }

    #[test]
    fn test_session_detail_uses_conversation_view() {
        let session = create_test_session();
        let spans = vec![
            create_test_span_with_id(SpanKind::User, "user-1"),
            create_test_span_with_id(SpanKind::Assistant, "assistant-1"),
        ];

        let result = session_detail(&session, &spans);
        let html_str = result.into_string();

        // session_detail returns conversation view directly (container provided by app_layout)
        assert!(html_str.contains("class=\"conversation\""));
        assert!(html_str.contains("class=\"turn turn-user\""));
        assert!(html_str.contains("class=\"turn turn-assistant\""));
    }
}
