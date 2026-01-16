//! Span view component for displaying trace spans.

use std::collections::HashSet;

use iocraft::prelude::*;
use serde_json::Value;

use crate::models::{Session, Span, SpanKind};

/// Lines of overhead: border (2) + header (1) + header margin (1) + padding implicit.
const OVERHEAD_LINES: usize = 4;
/// Max lines to show for expanded content.
const MAX_EXPANDED_LINES: usize = 12;

/// Props for the SpanView component.
#[derive(Default, Props)]
pub struct SpanViewProps {
    /// Current session being viewed.
    pub session: Option<Session>,
    /// Spans to display.
    pub spans: Vec<Span>,
    /// Currently selected index.
    pub selected_idx: usize,
    /// Set of expanded span IDs.
    pub expanded_ids: HashSet<String>,
    /// Whether this panel has focus.
    pub focused: bool,
    /// Available height in terminal lines.
    pub height: u16,
}

/// Span view component.
#[component]
pub fn SpanView(props: &SpanViewProps) -> impl Into<AnyElement<'static>> {
    let border_color = if props.focused { Color::Blue } else { Color::DarkGrey };

    // Calculate available lines for spans (subtract overhead + status bar)
    let available_lines = (props.height as usize).saturating_sub(OVERHEAD_LINES + 3);

    // Dynamically calculate which spans fit by measuring actual content
    let (start, end) = calculate_visible_range(
        &props.spans,
        props.selected_idx,
        &props.expanded_ids,
        available_lines,
    );

    // Build position indicator
    let total = props.spans.len();
    let position =
        if total > 0 { format!("[{}/{}]", props.selected_idx + 1, total) } else { String::new() };

    // Use fixed height based on terminal size minus status bar (~3 lines)
    let view_height = props.height.saturating_sub(3);

    element! {
        View(
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Round,
            border_color: border_color,
            flex_grow: 1.0,
            height: view_height,
            max_height: view_height,
            padding_left: 1,
            padding_right: 1,
            overflow: Overflow::Hidden,
        ) {
            // Header with session name and position indicator
            View(justify_content: JustifyContent::SpaceBetween, margin_bottom: 1) {
                #(props.session.as_ref().map(|s| {
                    let name = s.name.as_deref().unwrap_or(&s.id);
                    let display_name = truncate_str(name, 40);
                    element! {
                        Text(content: display_name, weight: Weight::Bold, color: Color::White)
                    }
                }))
                #(props.session.is_none().then(|| {
                    element! {
                        Text(content: "Select a session", color: Color::White)
                    }
                }))
                #(if !position.is_empty() {
                    Some(element! {
                        Text(content: position, color: Color::DarkGrey)
                    })
                } else {
                    None
                })
            }

            // Visible spans (viewport only)
            View(flex_direction: FlexDirection::Column, flex_grow: 1.0, overflow: Overflow::Hidden) {
                #(props.spans.iter().enumerate().skip(start).take(end - start).map(|(idx, span)| {
                    render_span_item(span, idx, props)
                }))
            }
        }
    }
}

/// Calculate visible range by measuring actual content lines.
/// Returns (start, end) indices of spans to render.
fn calculate_visible_range(
    spans: &[Span],
    selected_idx: usize,
    expanded_ids: &HashSet<String>,
    available_lines: usize,
) -> (usize, usize) {
    if spans.is_empty() {
        return (0, 0);
    }

    let total = spans.len();
    let selected_idx = selected_idx.min(total.saturating_sub(1));

    // Measure lines for each span
    let span_lines: Vec<usize> = spans
        .iter()
        .map(|span| measure_span_lines(span, expanded_ids.contains(&span.id)))
        .collect();

    // Start from selected and expand outward to fill available space
    let mut start = selected_idx;
    let mut end = selected_idx + 1;
    let mut used_lines = span_lines.get(selected_idx).copied().unwrap_or(3);

    // Try to add spans before and after selected, alternating
    loop {
        let can_add_before = start > 0;
        let can_add_after = end < total;

        if !can_add_before && !can_add_after {
            break;
        }

        let mut made_progress = false;

        // Try adding one before
        if can_add_before {
            let lines_needed = span_lines.get(start - 1).copied().unwrap_or(3);
            if used_lines + lines_needed <= available_lines {
                start -= 1;
                used_lines += lines_needed;
                made_progress = true;
            }
        }

        // Try adding one after
        if can_add_after {
            let lines_needed = span_lines.get(end).copied().unwrap_or(3);
            if used_lines + lines_needed <= available_lines {
                end += 1;
                used_lines += lines_needed;
                made_progress = true;
            }
        }

        // If we couldn't add anything, stop
        if !made_progress {
            break;
        }
    }

    (start, end)
}

/// Measure how many terminal lines a span will take when rendered.
fn measure_span_lines(span: &Span, is_expanded: bool) -> usize {
    // Header: 1 line
    // Margin: 1 line
    let mut lines = 2;

    if is_expanded {
        // Get content and count actual lines (truncated)
        if let Some(content) = get_span_content(span) {
            let content_lines = content.lines().count().min(MAX_EXPANDED_LINES);
            lines += content_lines;
            // Truncation indicator if needed
            if content.lines().count() > MAX_EXPANDED_LINES {
                lines += 1;
            }
        }

        // Tool metadata for tool spans
        if matches!(span.kind, SpanKind::ToolCall | SpanKind::ToolResult)
            && let Some(metadata) = &span.metadata
        {
            if metadata.get("tool_arguments").is_some() {
                lines += 7; // label + up to 6 lines content
            }
            if metadata.get("tool_response").is_some() {
                lines += 7;
            }
        }

        // Token info lines
        if span.input_tokens.is_some() {
            lines += 1;
        }
        if span.output_tokens.is_some() {
            lines += 1;
        }
        if span.finish_reason.is_some() {
            lines += 1;
        }
    } else {
        // Collapsed: just preview line
        lines += 1;
    }

    lines
}

fn render_span_item(span: &Span, idx: usize, props: &SpanViewProps) -> AnyElement<'static> {
    let is_selected = idx == props.selected_idx;
    let is_expanded = props.expanded_ids.contains(&span.id);
    let kind_color = span_kind_color(&span.kind);
    let kind_str = span_kind_to_str(&span.kind);

    // Build header line (no prefix)
    let mut header = kind_str.to_string();

    // Show tool name for tool spans
    if let Some(tool_name) = &span.tool_name {
        header.push_str(&format!(" [{}]", tool_name));
    }

    if let Some(model) = &span.model {
        let short_model = model.split(':').next_back().unwrap_or(model);
        header.push_str(&format!(" | {}", short_model));
    }
    if let Some(duration) = span.duration_ms {
        header.push_str(&format!(" | {}", format_duration(duration)));
    }

    // Add token info to header
    if span.input_tokens.is_some() || span.output_tokens.is_some() {
        let input = span.input_tokens.unwrap_or(0);
        let output = span.output_tokens.unwrap_or(0);
        header.push_str(&format!(" | {}in/{}out", input, output));
    }

    // Use background color for selection (like session list)
    let bg_color = if is_selected && props.focused { Some(Color::DarkGrey) } else { None };

    let header_color = if is_selected { Color::White } else { kind_color };

    let header_weight = if is_selected { Weight::Bold } else { Weight::Normal };

    // Get content to display
    let display_content = get_span_content(span);
    let preview = display_content.as_ref().map(|c| truncate_str(c, 80)).unwrap_or_default();

    // Truncate expanded content to max lines
    let (expanded_content, truncated_info) = if is_expanded {
        display_content.as_ref().map(|c| truncate_lines(c, MAX_EXPANDED_LINES)).unwrap_or_default()
    } else {
        (String::new(), None)
    };

    // Use white for expanded content, lighter for preview
    let content_color = Color::White;
    let preview_color = Color::Grey;

    element! {
        View(flex_direction: FlexDirection::Column, margin_bottom: 1, key: idx, background_color: bg_color) {
            // Header row
            Text(content: header, color: header_color, weight: header_weight)

            // Show content
            #(if is_expanded && !expanded_content.is_empty() {
                // Truncated content when expanded
                Some(element! {
                    View(margin_left: 2, flex_direction: FlexDirection::Column) {
                        Text(content: expanded_content.clone(), color: content_color)
                        #(truncated_info.map(|info| {
                            element! {
                                Text(content: info, color: Color::DarkGrey)
                            }
                        }))
                    }
                })
            } else if !preview.is_empty() {
                // Preview when collapsed
                Some(element! {
                    View(margin_left: 2) {
                        Text(content: preview, color: preview_color)
                    }
                })
            } else {
                None
            })

            // Extra metadata when expanded (only for tool spans)
            #(is_expanded.then(|| {
                render_expanded_metadata(span)
            }))
        }
    }
    .into()
}

fn render_expanded_metadata(span: &Span) -> AnyElement<'static> {
    // Get tool arguments and response from metadata
    let (tool_args, tool_response) = extract_tool_content(span);

    // Truncate tool content
    let (args_text, args_info) = tool_args.map(|a| truncate_lines(&a, 6)).unwrap_or_default();
    let (resp_text, resp_info) = tool_response.map(|r| truncate_lines(&r, 6)).unwrap_or_default();

    element! {
        View(margin_left: 2, flex_direction: FlexDirection::Column) {
            // Tool arguments
            #((!args_text.is_empty()).then(|| {
                element! {
                    View(flex_direction: FlexDirection::Column) {
                        Text(content: "Arguments:", color: Color::Yellow, weight: Weight::Bold)
                        Text(content: args_text.clone(), color: Color::White)
                        #(args_info.as_ref().map(|info| {
                            element! {
                                Text(content: info.clone(), color: Color::DarkGrey)
                            }
                        }))
                    }
                }
            }))

            // Tool response
            #((!resp_text.is_empty()).then(|| {
                element! {
                    View(flex_direction: FlexDirection::Column) {
                        Text(content: "Response:", color: Color::Yellow, weight: Weight::Bold)
                        Text(content: resp_text.clone(), color: Color::White)
                        #(resp_info.as_ref().map(|info| {
                            element! {
                                Text(content: info.clone(), color: Color::DarkGrey)
                            }
                        }))
                    }
                }
            }))

            // Token info
            #(span.input_tokens.map(|t| {
                element! {
                    Text(content: format!("Input tokens: {}", t), color: Color::Grey)
                }
            }))
            #(span.output_tokens.map(|t| {
                element! {
                    Text(content: format!("Output tokens: {}", t), color: Color::Grey)
                }
            }))
            #(span.finish_reason.as_ref().map(|r| {
                element! {
                    Text(content: format!("Finish: {}", r), color: Color::Grey)
                }
            }))
        }
    }
    .into()
}

/// Get the content to display for a span.
/// Tries span.content first, then extracts from metadata.
fn get_span_content(span: &Span) -> Option<String> {
    // First try direct content
    if let Some(content) = &span.content
        && !content.is_empty()
    {
        return Some(content.clone());
    }

    let Some(metadata) = &span.metadata else {
        return None;
    };

    // For tool spans, try to extract tool-specific content
    if matches!(span.kind, SpanKind::ToolCall | SpanKind::ToolResult) {
        let (args, response) = extract_tool_content(span);
        if let Some(resp) = response {
            return Some(resp);
        }
        if let Some(args) = args {
            return Some(args);
        }
    }

    // For User/Assistant/System spans, extract from gen_ai messages
    if matches!(span.kind, SpanKind::User | SpanKind::Assistant | SpanKind::System) {
        // Try gen_ai.output.messages for assistant responses
        if let Some(output) = metadata.get("gen_ai.output.messages")
            && let Some(content) = extract_message_content(output, "assistant")
        {
            return Some(content);
        }
        // Try gen_ai.input.messages for user/system messages
        let role = match span.kind {
            SpanKind::User => "user",
            SpanKind::System => "system",
            _ => "user",
        };
        if let Some(input) = metadata.get("gen_ai.input.messages")
            && let Some(content) = extract_message_content(input, role)
        {
            return Some(content);
        }
    }

    None
}

/// Extract text content from gen_ai messages JSON.
fn extract_message_content(messages: &Value, target_role: &str) -> Option<String> {
    // Messages can be a JSON string or array
    let messages_array = if let Some(s) = messages.as_str() {
        serde_json::from_str::<Value>(s).ok()?
    } else {
        messages.clone()
    };

    let arr = messages_array.as_array()?;

    // Find the last message matching the target role
    for msg in arr.iter().rev() {
        let role = msg.get("role")?.as_str()?;
        if role != target_role {
            continue;
        }

        // Extract text from parts
        if let Some(parts) = msg.get("parts").and_then(|p| p.as_array()) {
            let texts: Vec<String> = parts
                .iter()
                .filter_map(|part| {
                    let part_type = part.get("type")?.as_str()?;
                    if part_type == "text" {
                        part.get("content")?.as_str().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect();

            if !texts.is_empty() {
                return Some(texts.join("\n"));
            }
        }

        // Try direct content field
        if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
            return Some(content.to_string());
        }
    }

    None
}

/// Extract tool arguments and response from span metadata.
fn extract_tool_content(span: &Span) -> (Option<String>, Option<String>) {
    let Some(metadata) = &span.metadata else {
        return (None, None);
    };

    let args = metadata.get("tool_arguments").and_then(value_to_string);
    let response = metadata.get("tool_response").and_then(value_to_string);

    (args, response)
}

/// Convert a JSON value to a string for display.
fn value_to_string(v: &Value) -> Option<String> {
    if v.is_string() {
        v.as_str().map(|s| s.to_string())
    } else {
        serde_json::to_string_pretty(v).ok()
    }
}

fn span_kind_color(kind: &SpanKind) -> Color {
    match kind {
        SpanKind::User => Color::Cyan,
        SpanKind::Assistant => Color::Green,
        SpanKind::System => Color::Yellow,
        SpanKind::Thinking => Color::Magenta,
        SpanKind::ToolCall => Color::Blue,
        SpanKind::ToolResult => Color::Blue,
        SpanKind::Choice => Color::White,
        SpanKind::Span => Color::Grey,
    }
}

fn span_kind_to_str(kind: &SpanKind) -> &'static str {
    match kind {
        SpanKind::User => "USER",
        SpanKind::Assistant => "ASSISTANT",
        SpanKind::System => "SYSTEM",
        SpanKind::Thinking => "THINKING",
        SpanKind::ToolCall => "TOOL_CALL",
        SpanKind::ToolResult => "TOOL_RESULT",
        SpanKind::Choice => "CHOICE",
        SpanKind::Span => "SPAN",
    }
}

fn format_duration(ms: i64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        let secs = ms as f64 / 1000.0;
        format!("{:.1}s", secs)
    } else {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) / 1000;
        format!("{}m{}s", mins, secs)
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    // Get first line only for preview
    let first_line = s.lines().next().unwrap_or(s);
    if first_line.len() <= max_len {
        first_line.to_string()
    } else {
        format!("{}...", &first_line[..max_len.saturating_sub(3)])
    }
}

/// Truncate content to max lines, returning (content, truncation_info).
fn truncate_lines(s: &str, max_lines: usize) -> (String, Option<String>) {
    let lines: Vec<&str> = s.lines().collect();
    let total = lines.len();

    if total <= max_lines {
        (s.to_string(), None)
    } else {
        let truncated = lines.get(..max_lines).map(|slice| slice.join("\n")).unwrap_or_default();
        let remaining = total.saturating_sub(max_lines);
        let info = format!("... [{} more lines]", remaining);
        (truncated, Some(info))
    }
}
