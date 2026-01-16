//! Help overlay component.

use iocraft::prelude::*;

/// Props for the Help component.
#[derive(Default, Props)]
pub struct HelpProps {
    /// Whether to show the help overlay.
    pub visible: bool,
}

/// Help overlay component.
#[component]
pub fn Help(props: &HelpProps) -> impl Into<AnyElement<'static>> {
    if !props.visible {
        return element! { View() };
    }

    let keybindings = [
        (
            "Navigation",
            vec![
                ("j / Down", "Move down"),
                ("k / Up", "Move up"),
                ("gg", "Go to first"),
                ("G", "Go to last"),
                ("Ctrl+d", "Page down"),
                ("Ctrl+u", "Page up"),
            ],
        ),
        (
            "Actions",
            vec![
                ("Enter", "Select / Expand"),
                ("Space", "Toggle expand"),
                ("Tab", "Switch panel"),
                ("h / Left", "Focus sessions"),
                ("l / Right", "Focus spans"),
            ],
        ),
        ("General", vec![("?", "Toggle help"), ("q / Esc", "Quit / Close"), ("r", "Refresh")]),
    ];

    element! {
        View(
            position: Position::Absolute,
            width: 100pct,
            height: 100pct,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        ) {
            View(
                flex_direction: FlexDirection::Column,
                border_style: BorderStyle::Round,
                border_color: Color::Blue,
                padding: 2,
                background_color: Color::Black,
            ) {
                View(margin_bottom: 1, justify_content: JustifyContent::Center) {
                    Text(content: "tv - Traceview TUI", weight: Weight::Bold, color: Color::Blue)
                }

                #(keybindings.iter().map(|(section, bindings)| {
                    element! {
                        View(flex_direction: FlexDirection::Column, margin_bottom: 1, key: *section) {
                            Text(content: format!("{}:", section), weight: Weight::Bold, color: Color::Yellow)
                            #(bindings.iter().map(|(key, desc)| {
                                element! {
                                    View(key: *key) {
                                        Text(content: format!("  {:12} {}", key, desc), color: Color::Grey)
                                    }
                                }
                            }))
                        }
                    }
                }))

                View(margin_top: 1, justify_content: JustifyContent::Center) {
                    Text(content: "Press ? or Esc to close", color: Color::DarkGrey)
                }
            }
        }
    }
}
