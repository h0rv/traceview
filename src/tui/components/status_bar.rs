//! Status bar component showing key hints.

use iocraft::prelude::*;

use crate::tui::state::Focus;

/// Props for the StatusBar component.
#[derive(Default, Props)]
pub struct StatusBarProps {
    /// Current focus.
    pub focus: Focus,
    /// Whether sidebar is visible.
    pub sidebar_visible: bool,
}

/// Status bar component.
#[component]
pub fn StatusBar(props: &StatusBarProps) -> impl Into<AnyElement<'static>> {
    let hints = match (props.focus, props.sidebar_visible) {
        (Focus::SessionList, _) => "j/k:nav  Enter:select  Tab:spans  b:hide  q:quit",
        (Focus::SpanList, true) => "j/k:nav  Enter:expand  Tab:sessions  b:hide  q:quit",
        (Focus::SpanList, false) => "j/k:nav  Enter:expand  b:show  q:quit",
    };

    element! {
        View(
            width: 100pct,
            border_style: BorderStyle::Round,
            border_color: Color::DarkGrey,
            padding_left: 1,
            padding_right: 1,
            justify_content: JustifyContent::SpaceBetween,
        ) {
            Text(content: hints, color: Color::Grey)
            Text(content: "?:help", color: Color::Grey)
        }
    }
}
