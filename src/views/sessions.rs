//! Session list view for traceview.

use chrono::{DateTime, TimeZone, Utc};
use maud::{Markup, html};

use crate::models::Session;

/// Renders a list of sessions.
///
/// # Arguments
/// * `sessions` - Slice of sessions to render
pub fn sessions_list(sessions: &[Session]) -> Markup {
    html! {
        ul id="session-list" class="session-list" {
            @if sessions.is_empty() {
                div class="empty-state" {
                    p { "No sessions found." }
                    p { "Sessions will appear here when traces are received." }
                }
            } @else {
                @for session in sessions {
                    (session_item(session))
                }
            }
        }
    }
}

/// Renders a single session item in the list.
fn session_item(session: &Session) -> Markup {
    let display_name = session.name.as_deref().unwrap_or(&session.id);
    let created = format_timestamp(session.created_at);
    let updated = format_timestamp(session.updated_at);

    html! {
        li class="session-item" {
            a href={ "/sessions/" (session.id) } {
                (display_name)
            }
            div class="session-meta" {
                span { "Created: " (created) }
                " | "
                span { "Updated: " (updated) }
            }
        }
    }
}

/// Renders a compact session list for the sidebar.
///
/// Shows session name, relative time, and event count badge.
/// Highlights the currently selected session.
///
/// # Arguments
/// * `sessions` - Slice of (Session, span_count) tuples
/// * `current_session_id` - ID of the currently viewed session (if any)
pub fn sidebar_session_list(
    sessions: &[(Session, i64)],
    current_session_id: Option<&str>,
) -> Markup {
    html! {
        @if sessions.is_empty() {
            div class="empty-state" {
                p { "No sessions yet" }
            }
        } @else {
            @for (session, span_count) in sessions {
                @let is_active = current_session_id == Some(session.id.as_str());
                @let item_class = if is_active { "session-item active" } else { "session-item" };
                li class=(item_class) data-session-id=(session.id) {
                    a href={ "/sessions/" (session.id) } {
                        div class="session-item-name" {
                            (session.name.as_deref().unwrap_or(&session.id))
                        }
                        div class="session-item-meta" {
                            span class="session-time" { (format_relative_time(session.updated_at)) }
                            span class="event-count" { (span_count) " events" }
                        }
                    }
                }
            }
        }
    }
}

/// Formats a Unix nanosecond timestamp into a relative time string.
///
/// Returns strings like "Just now", "5m ago", "2h ago", "3d ago".
fn format_relative_time(nanos: i64) -> String {
    let secs = nanos / 1_000_000_000;
    let now_secs = chrono::Utc::now().timestamp();
    let diff_secs = now_secs.saturating_sub(secs);

    if diff_secs < 60 {
        "Just now".to_string()
    } else if diff_secs < 3600 {
        format!("{}m ago", diff_secs / 60)
    } else if diff_secs < 86400 {
        format!("{}h ago", diff_secs / 3600)
    } else {
        format!("{}d ago", diff_secs / 86400)
    }
}

/// Formats a Unix nanosecond timestamp into a human-readable string.
fn format_timestamp(nanos: i64) -> String {
    // Convert nanoseconds to seconds and nanoseconds remainder
    let secs = nanos / 1_000_000_000;
    // SAFETY: modulo 1_000_000_000 guarantees the value fits in u32 (max ~999_999_999)
    #[allow(clippy::cast_possible_truncation)]
    let nsecs = (nanos % 1_000_000_000).unsigned_abs() as u32;

    Utc.timestamp_opt(secs, nsecs)
        .single()
        .map(|dt: DateTime<Utc>| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "Unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_session(id: &str, name: Option<&str>) -> Session {
        Session {
            id: id.to_string(),
            name: name.map(String::from),
            created_at: 1_700_000_000_000_000_000,
            updated_at: 1_700_000_100_000_000_000,
        }
    }

    #[test]
    fn test_sessions_list_renders_all_sessions() {
        let sessions = vec![
            create_test_session("session-1", Some("First Session")),
            create_test_session("session-2", Some("Second Session")),
            create_test_session("session-3", None),
        ];

        let result = sessions_list(&sessions);
        let html_str = result.into_string();

        assert!(html_str.contains("First Session"));
        assert!(html_str.contains("Second Session"));
        assert!(html_str.contains("session-3")); // Uses ID when no name
        assert!(html_str.contains("/sessions/session-1"));
        assert!(html_str.contains("/sessions/session-2"));
        assert!(html_str.contains("/sessions/session-3"));
    }

    #[test]
    fn test_sessions_list_empty() {
        let sessions: Vec<Session> = vec![];
        let result = sessions_list(&sessions);
        let html_str = result.into_string();

        assert!(html_str.contains("No sessions found"));
        assert!(html_str.contains("empty-state"));
    }

    #[test]
    fn test_session_item_with_name() {
        let session = create_test_session("test-id", Some("Test Name"));
        let result = session_item(&session);
        let html_str = result.into_string();

        assert!(html_str.contains("Test Name"));
        assert!(html_str.contains("/sessions/test-id"));
        assert!(html_str.contains("session-item"));
    }

    #[test]
    fn test_session_item_without_name() {
        let session = create_test_session("my-session-id", None);
        let result = session_item(&session);
        let html_str = result.into_string();

        // Should display the ID when name is None
        assert!(html_str.contains("my-session-id"));
    }

    #[test]
    fn test_session_item_shows_timestamps() {
        let session = create_test_session("test", Some("Test"));
        let result = session_item(&session);
        let html_str = result.into_string();

        assert!(html_str.contains("Created:"));
        assert!(html_str.contains("Updated:"));
        assert!(html_str.contains("session-meta"));
    }

    #[test]
    fn test_format_timestamp_valid() {
        // 2023-11-14 22:13:20 UTC in nanoseconds
        let nanos = 1_700_000_000_000_000_000_i64;
        let result = format_timestamp(nanos);

        assert!(result.contains("2023"));
        assert!(result.contains("UTC"));
    }

    #[test]
    fn test_format_timestamp_zero() {
        let result = format_timestamp(0);
        assert!(result.contains("1970"));
    }
}
