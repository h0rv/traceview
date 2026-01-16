//! Main TUI application component.

use std::collections::HashSet;
use std::sync::Arc;

use iocraft::prelude::*;

use crate::db::Database;
use crate::models::{Session, Span};
use crate::tui::components::{Help, SessionList, SpanView, StatusBar};
use crate::tui::state::Focus;

/// Props for the App component.
#[derive(Default, Props)]
pub struct AppProps {
    /// Database connection.
    pub db: Option<Arc<Database>>,
}

/// Main TUI application component.
#[component]
pub fn App(props: &AppProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    // Get terminal size for fullscreen layout
    let (width, height) = hooks.use_terminal_size();

    // State - let type inference work
    let mut sessions = hooks.use_state(Vec::<(Session, i64)>::new);
    let mut current_session = hooks.use_state(|| Option::<Session>::None);
    let mut spans = hooks.use_state(Vec::<Span>::new);
    let mut focus = hooks.use_state(|| Focus::SessionList);
    let mut session_idx = hooks.use_state(|| 0usize);
    let mut span_idx = hooks.use_state(|| 0usize);
    let mut expanded_spans = hooks.use_state(HashSet::<String>::new);
    let mut show_help = hooks.use_state(|| false);
    let mut show_sidebar = hooks.use_state(|| true);
    let mut should_exit = hooks.use_state(|| false);
    let mut pending_g = hooks.use_state(|| false);

    // Load sessions on mount
    let db = props.db.clone();
    hooks.use_future(async move {
        if let Some(db) = db
            && let Ok(loaded) = db.get_sessions_with_counts(100, 0).await
        {
            sessions.set(loaded);
        }
    });

    // Handle keyboard events
    hooks.use_terminal_events({
        let db = props.db.clone();
        move |event| {
            let Some(ref db) = db else { return };
            if let TerminalEvent::Key(KeyEvent { code, kind, modifiers, .. }) = event {
                // Only handle key press, not release
                if kind != KeyEventKind::Press {
                    return;
                }

                // Handle 'g' prefix for gg
                if pending_g.get() {
                    pending_g.set(false);
                    if code == KeyCode::Char('g') {
                        // gg - go to first
                        match focus.get() {
                            Focus::SessionList => session_idx.set(0),
                            Focus::SpanList => span_idx.set(0),
                        }
                    }
                    return;
                }

                match code {
                    // Quit
                    KeyCode::Char('q') => {
                        if show_help.get() {
                            show_help.set(false);
                        } else {
                            should_exit.set(true);
                        }
                    }
                    KeyCode::Esc => {
                        if show_help.get() {
                            show_help.set(false);
                        } else {
                            should_exit.set(true);
                        }
                    }

                    // Help
                    KeyCode::Char('?') => {
                        show_help.set(!show_help.get());
                    }

                    // Toggle sidebar
                    KeyCode::Char('b') => {
                        show_sidebar.set(!show_sidebar.get());
                        // If hiding sidebar and focused on it, switch to spans
                        if !show_sidebar.get() && focus.get() == Focus::SessionList {
                            focus.set(Focus::SpanList);
                        }
                    }

                    // Navigation
                    KeyCode::Char('j') | KeyCode::Down => match focus.get() {
                        Focus::SessionList => {
                            let max = sessions.read().len().saturating_sub(1);
                            session_idx.set(session_idx.get().saturating_add(1).min(max));
                        }
                        Focus::SpanList => {
                            let max = spans.read().len().saturating_sub(1);
                            span_idx.set(span_idx.get().saturating_add(1).min(max));
                        }
                    },
                    KeyCode::Char('k') | KeyCode::Up => match focus.get() {
                        Focus::SessionList => {
                            session_idx.set(session_idx.get().saturating_sub(1));
                        }
                        Focus::SpanList => {
                            span_idx.set(span_idx.get().saturating_sub(1));
                        }
                    },

                    // Go to first (g prefix)
                    KeyCode::Char('g') => {
                        pending_g.set(true);
                    }

                    // Go to last
                    KeyCode::Char('G') => match focus.get() {
                        Focus::SessionList => {
                            let max = sessions.read().len().saturating_sub(1);
                            session_idx.set(max);
                        }
                        Focus::SpanList => {
                            let max = spans.read().len().saturating_sub(1);
                            span_idx.set(max);
                        }
                    },

                    // Page down
                    KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => {
                        match focus.get() {
                            Focus::SessionList => {
                                let max = sessions.read().len().saturating_sub(1);
                                session_idx.set(session_idx.get().saturating_add(10).min(max));
                            }
                            Focus::SpanList => {
                                let max = spans.read().len().saturating_sub(1);
                                span_idx.set(span_idx.get().saturating_add(10).min(max));
                            }
                        }
                    }

                    // Page up
                    KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
                        match focus.get() {
                            Focus::SessionList => {
                                session_idx.set(session_idx.get().saturating_sub(10));
                            }
                            Focus::SpanList => {
                                span_idx.set(span_idx.get().saturating_sub(10));
                            }
                        }
                    }

                    // Switch focus
                    KeyCode::Tab => {
                        if show_sidebar.get() {
                            focus.set(match focus.get() {
                                Focus::SessionList => Focus::SpanList,
                                Focus::SpanList => Focus::SessionList,
                            });
                        }
                    }
                    KeyCode::Char('h') | KeyCode::Left => {
                        if show_sidebar.get() {
                            focus.set(Focus::SessionList);
                        }
                    }
                    KeyCode::Char('l') | KeyCode::Right => {
                        focus.set(Focus::SpanList);
                    }

                    // Select/Expand
                    KeyCode::Enter => {
                        match focus.get() {
                            Focus::SessionList => {
                                // Select session and load spans
                                let sessions_read = sessions.read();
                                if let Some((session, _)) = sessions_read.get(session_idx.get()) {
                                    let session = session.clone();
                                    let session_id = session.id.clone();
                                    current_session.set(Some(session));
                                    span_idx.set(0);
                                    expanded_spans.set(HashSet::new());
                                    focus.set(Focus::SpanList);

                                    // Load spans synchronously
                                    drop(sessions_read);
                                    if let Ok(loaded) = futures::executor::block_on(
                                        db.get_spans_by_session(&session_id),
                                    ) {
                                        spans.set(loaded);
                                    }
                                }
                            }
                            Focus::SpanList => {
                                // Toggle expand
                                let spans_read = spans.read();
                                if let Some(span) = spans_read.get(span_idx.get()) {
                                    let id = span.id.clone();
                                    drop(spans_read);
                                    let mut expanded = expanded_spans.read().clone();
                                    if expanded.contains(&id) {
                                        expanded.remove(&id);
                                    } else {
                                        expanded.insert(id);
                                    }
                                    expanded_spans.set(expanded);
                                }
                            }
                        }
                    }
                    KeyCode::Char(' ') => {
                        // Toggle expand (spans only)
                        if matches!(focus.get(), Focus::SpanList) {
                            let spans_read = spans.read();
                            if let Some(span) = spans_read.get(span_idx.get()) {
                                let id = span.id.clone();
                                drop(spans_read);
                                let mut expanded = expanded_spans.read().clone();
                                if expanded.contains(&id) {
                                    expanded.remove(&id);
                                } else {
                                    expanded.insert(id);
                                }
                                expanded_spans.set(expanded);
                            }
                        }
                    }

                    // Refresh
                    KeyCode::Char('r') => {
                        if let Ok(loaded) =
                            futures::executor::block_on(db.get_sessions_with_counts(100, 0))
                        {
                            sessions.set(loaded);
                        }
                    }

                    _ => {}
                }
            }
        }
    });

    // Check if we should exit
    if should_exit.get() {
        hooks.use_future(async move {
            std::process::exit(0);
        });
    }

    // Get data for rendering
    let sessions_vec: Vec<(Session, i64)> = sessions.read().clone();
    let current_opt: Option<Session> = current_session.read().clone();
    let spans_vec: Vec<Span> = spans.read().clone();
    let expanded_set: HashSet<String> = expanded_spans.read().clone();
    let sidebar_visible = show_sidebar.get();

    element! {
        View(flex_direction: FlexDirection::Column, width, height) {
            // Main content
            View(flex_grow: 1.0, flex_direction: FlexDirection::Row) {
                #(sidebar_visible.then(|| {
                    element! {
                        SessionList(
                            sessions: sessions_vec.clone(),
                            selected_idx: session_idx.get(),
                            focused: focus.get() == Focus::SessionList,
                            height: height,
                        )
                    }
                }))
                SpanView(
                    session: current_opt.clone(),
                    spans: spans_vec.clone(),
                    selected_idx: span_idx.get(),
                    expanded_ids: expanded_set.clone(),
                    focused: focus.get() == Focus::SpanList,
                    height: height,
                )
            }

            // Status bar
            StatusBar(focus: focus.get(), sidebar_visible: sidebar_visible)

            // Help overlay
            Help(visible: show_help.get())
        }
    }
}
