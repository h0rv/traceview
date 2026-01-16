//! Application state for the TUI.

use std::collections::HashSet;

use crate::models::{Session, Span};

/// Which panel has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    SessionList,
    SpanList,
}

/// Application state for the TUI.
#[derive(Debug, Default)]
pub struct AppState {
    /// Sessions with their span counts.
    pub sessions: Vec<(Session, i64)>,
    /// Currently selected session.
    pub current_session: Option<Session>,
    /// Spans for the current session.
    pub spans: Vec<Span>,
    /// Which panel has focus.
    pub focus: Focus,
    /// Selected index in session list.
    pub session_idx: usize,
    /// Selected index in span list.
    pub span_idx: usize,
    /// Set of expanded span IDs.
    pub expanded_spans: HashSet<String>,
    /// Whether to show help overlay.
    pub show_help: bool,
}

impl AppState {
    /// Move selection up in the current list.
    pub fn move_up(&mut self) {
        match self.focus {
            Focus::SessionList => {
                self.session_idx = self.session_idx.saturating_sub(1);
            }
            Focus::SpanList => {
                self.span_idx = self.span_idx.saturating_sub(1);
            }
        }
    }

    /// Move selection down in the current list.
    pub fn move_down(&mut self) {
        match self.focus {
            Focus::SessionList => {
                if !self.sessions.is_empty() {
                    self.session_idx = (self.session_idx + 1).min(self.sessions.len() - 1);
                }
            }
            Focus::SpanList => {
                if !self.spans.is_empty() {
                    self.span_idx = (self.span_idx + 1).min(self.spans.len() - 1);
                }
            }
        }
    }

    /// Go to first item in current list.
    pub fn go_to_first(&mut self) {
        match self.focus {
            Focus::SessionList => self.session_idx = 0,
            Focus::SpanList => self.span_idx = 0,
        }
    }

    /// Go to last item in current list.
    pub fn go_to_last(&mut self) {
        match self.focus {
            Focus::SessionList => {
                if !self.sessions.is_empty() {
                    self.session_idx = self.sessions.len() - 1;
                }
            }
            Focus::SpanList => {
                if !self.spans.is_empty() {
                    self.span_idx = self.spans.len() - 1;
                }
            }
        }
    }

    /// Toggle focus between panels.
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::SessionList => Focus::SpanList,
            Focus::SpanList => Focus::SessionList,
        };
    }

    /// Toggle expansion of the currently selected span.
    pub fn toggle_current_span(&mut self) {
        if let Some(span) = self.spans.get(self.span_idx) {
            let id = span.id.clone();
            if self.expanded_spans.contains(&id) {
                self.expanded_spans.remove(&id);
            } else {
                self.expanded_spans.insert(id);
            }
        }
    }

    /// Check if a span is expanded.
    pub fn is_expanded(&self, span_id: &str) -> bool {
        self.expanded_spans.contains(span_id)
    }
}
