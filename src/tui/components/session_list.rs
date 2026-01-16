//! Session list sidebar component.

use iocraft::prelude::*;

use crate::models::Session;

/// Lines of overhead: border (2) + header (1) + header margin (1).
const OVERHEAD_LINES: usize = 4;

/// Props for the SessionList component.
#[derive(Default, Props)]
pub struct SessionListProps {
    /// Sessions with span counts.
    pub sessions: Vec<(Session, i64)>,
    /// Currently selected index.
    pub selected_idx: usize,
    /// Whether this panel has focus.
    pub focused: bool,
    /// Available height in terminal lines.
    pub height: u16,
}

/// Session list sidebar component.
#[component]
pub fn SessionList(props: &SessionListProps) -> impl Into<AnyElement<'static>> {
    let border_color = if props.focused { Color::Blue } else { Color::DarkGrey };

    // Calculate viewport - each session is 1 line, account for status bar (~3 lines)
    let available = (props.height as usize).saturating_sub(OVERHEAD_LINES + 3);
    let viewport_size = available.max(1);
    let (start, end) = calculate_viewport(props.selected_idx, props.sessions.len(), viewport_size);

    // Use fixed height based on terminal size minus status bar (~3 lines)
    let view_height = props.height.saturating_sub(3);

    element! {
        View(
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Round,
            border_color: border_color,
            width: 22,
            flex_shrink: 0.0,
            height: view_height,
            max_height: view_height,
            padding_left: 1,
            padding_right: 1,
            overflow: Overflow::Hidden,
        ) {
            View(margin_bottom: 1) {
                Text(content: "Sessions", weight: Weight::Bold, color: Color::White)
            }

            View(flex_direction: FlexDirection::Column, flex_grow: 1.0, overflow: Overflow::Hidden) {
                #(props.sessions.iter().enumerate().skip(start).take(end - start).map(|(idx, (session, count))| {
                    let is_selected = idx == props.selected_idx;
                    let name = session.name.as_deref().unwrap_or(&session.id);
                    let display_name = truncate_str(name, 16);
                    let content = format!("{} ({})", display_name, count);

                    // Use background color for selection instead of ">" prefix
                    let bg_color = if is_selected && props.focused {
                        Some(Color::Blue)
                    } else if is_selected {
                        Some(Color::DarkGrey)
                    } else {
                        None
                    };

                    let text_color = if is_selected {
                        Color::White
                    } else {
                        Color::Grey
                    };

                    let weight = if is_selected {
                        Weight::Bold
                    } else {
                        Weight::Normal
                    };

                    element! {
                        View(background_color: bg_color, key: idx) {
                            Text(content: content, color: text_color, weight: weight)
                        }
                    }
                }))
            }
        }
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Calculate viewport start and end indices.
fn calculate_viewport(selected: usize, total: usize, viewport_size: usize) -> (usize, usize) {
    if total <= viewport_size {
        return (0, total);
    }

    // Keep selected item roughly centered
    let half = viewport_size / 2;
    let start = if selected <= half {
        0
    } else if selected >= total.saturating_sub(half) {
        total.saturating_sub(viewport_size)
    } else {
        selected.saturating_sub(half)
    };

    let end = (start + viewport_size).min(total);
    (start, end)
}
