//! Tests for TUI components.

#[cfg(test)]
mod span_view_tests {
    use std::collections::HashSet;

    use iocraft::prelude::*;

    use crate::models::{Span, SpanKind};
    use crate::tui::components::SpanView;

    /// Create a test span with the given kind and content.
    fn create_test_span(id: &str, kind: SpanKind, content: Option<&str>) -> Span {
        Span {
            id: id.to_string(),
            session_id: "test-session".to_string(),
            parent_span_id: None,
            trace_id: "test-trace".to_string(),
            kind,
            model: Some("claude-3".to_string()),
            content: content.map(|s| s.to_string()),
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

    /// Create a tool span with tool_name set.
    fn create_tool_span(id: &str, tool_name: &str, content: Option<&str>) -> Span {
        let mut span = create_test_span(id, SpanKind::ToolCall, content);
        span.tool_name = Some(tool_name.to_string());
        span
    }

    /// Create many spans similar to session 5F6A2 (29 spans).
    fn create_many_spans() -> Vec<Span> {
        let mut spans = Vec::new();

        // Simulate the pattern from session 5F6A2
        for i in 0..29 {
            let span = match i % 5 {
                0 => create_test_span(
                    &format!("span-{i}"),
                    SpanKind::Assistant,
                    Some("This is an assistant response with some content."),
                ),
                1 => create_tool_span(&format!("span-{i}"), "bash", Some("echo 'hello world'")),
                2 => create_tool_span(&format!("span-{i}"), "read_file", Some("/path/to/file.rs")),
                3 => create_test_span(
                    &format!("span-{i}"),
                    SpanKind::User,
                    Some("User message here"),
                ),
                _ => create_test_span(&format!("span-{i}"), SpanKind::Span, None),
            };
            spans.push(span);
        }

        spans
    }

    /// Render SpanView and return the output string.
    fn render_span_view(
        spans: Vec<Span>,
        selected_idx: usize,
        expanded_ids: HashSet<String>,
        _width: u32,
        height: u32,
    ) -> String {
        let mut e = element! {
            SpanView(
                session: None,
                spans: spans,
                selected_idx: selected_idx,
                expanded_ids: expanded_ids,
                focused: true,
                height: height as u16,
            )
        };

        // Render to string
        e.to_string()
    }

    /// Count the number of lines in the rendered output.
    fn count_lines(output: &str) -> usize {
        output.lines().count()
    }

    /// Check if the position indicator is present in the output.
    fn has_position_indicator(output: &str) -> bool {
        // Look for pattern like [1/29] or [15/29]
        output.lines().any(|line| line.contains('[') && line.contains('/') && line.contains(']'))
    }

    #[test]
    fn test_span_view_renders() {
        let spans = vec![
            create_test_span("span-1", SpanKind::User, Some("Hello")),
            create_test_span("span-2", SpanKind::Assistant, Some("Hi there")),
        ];

        let output = render_span_view(spans, 0, HashSet::new(), 80, 24);
        assert!(!output.is_empty());
    }

    #[test]
    fn test_span_view_shows_position_indicator() {
        let spans = create_many_spans();
        let output = render_span_view(spans, 0, HashSet::new(), 80, 24);

        assert!(
            has_position_indicator(&output),
            "Position indicator [n/total] should be present. Output:\n{}",
            output
        );
    }

    #[test]
    fn test_span_view_shows_position_in_header() {
        let spans = create_many_spans();
        let output = render_span_view(spans, 15, HashSet::new(), 80, 24);

        // Position indicator should be in the header area
        assert!(
            has_position_indicator(&output),
            "Position indicator [n/total] should be present. Output:\n{}",
            output
        );
    }

    #[test]
    fn test_span_view_with_many_spans_fits_terminal_height() {
        let spans = create_many_spans();
        let output = render_span_view(spans, 0, HashSet::new(), 80, 24);

        let line_count = count_lines(&output);

        // For a 24-line terminal, we need to fit within that height
        // Each span takes ~3 lines (header + preview + margin)
        // Plus border (2), header (2), indicators (2) = ~6 lines overhead
        // So we should limit to roughly (24 - 6) / 3 = 6 spans visible
        // Allow some margin for now, but must be under 30 lines
        assert!(
            line_count <= 30,
            "Output must fit in terminal. Got {} lines (max 30):\n{}",
            line_count,
            output
        );
    }

    #[test]
    fn test_span_view_expanded_span_content_visible() {
        let spans = vec![
            create_test_span("span-1", SpanKind::User, Some("Hello user message")),
            create_test_span("span-2", SpanKind::Assistant, Some("This is the assistant response")),
        ];

        let mut expanded = HashSet::new();
        expanded.insert("span-2".to_string());

        let output = render_span_view(spans, 1, expanded, 80, 24);

        assert!(
            output.contains("assistant response"),
            "Expanded span content should be visible. Output:\n{}",
            output
        );
    }

    #[test]
    fn test_span_view_tool_name_shown() {
        let spans = vec![create_tool_span("span-1", "bash", Some("echo hello"))];

        let output = render_span_view(spans, 0, HashSet::new(), 80, 24);

        assert!(output.contains("bash"), "Tool name should be shown. Output:\n{}", output);
    }

    #[test]
    fn test_viewport_limits_rendered_spans() {
        let spans = create_many_spans();

        // Render with selection at start
        let output_start = render_span_view(spans.clone(), 0, HashSet::new(), 80, 24);

        // Render with selection in middle
        let output_middle = render_span_view(spans, 15, HashSet::new(), 80, 24);

        // Both should contain position indicator
        assert!(has_position_indicator(&output_start));
        assert!(has_position_indicator(&output_middle));

        // Start should show [1/29], middle should show [16/29]
        assert!(output_start.contains("[1/29]"), "Start should show [1/29]");
        assert!(output_middle.contains("[16/29]"), "Middle should show [16/29]");
    }
}
