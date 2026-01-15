//! Base HTML layout for traceview pages.

use maud::{DOCTYPE, Markup, PreEscaped, html};

/// Renders the base HTML layout wrapping the provided content.
///
/// # Arguments
/// * `title` - The page title (will be appended with " - Traceview")
/// * `content` - The main content markup to render
#[allow(clippy::needless_pass_by_value)]
pub fn base_layout(title: &str, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " - Traceview" }
                style { (PreEscaped(include_str!("styles.css"))) }
            }
            body {
                header {
                    h1 { a href="/" { "Traceview" } }
                }
                main { (content) }
                script { (PreEscaped(sse_script())) }
            }
        }
    }
}

/// Returns the JavaScript for SSE connection handling.
fn sse_script() -> &'static str {
    r#"
(function() {
    // Session detail page - stream spans for this session
    const spansContainer = document.getElementById('spans-container');
    if (spansContainer) {
        const sessionId = spansContainer.dataset.sessionId;
        if (sessionId) {
            console.log('Connecting to SSE for session:', sessionId);
            const eventSource = new EventSource('/sessions/' + sessionId + '/stream');

            eventSource.addEventListener('span', function(event) {
                console.log('SSE span received:', event.data);
                try {
                    const data = JSON.parse(event.data);
                    if (data.html) {
                        // Remove empty state if present
                        const emptyState = spansContainer.querySelector('.empty-state');
                        if (emptyState) {
                            emptyState.remove();
                        }

                        const temp = document.createElement('div');
                        temp.innerHTML = data.html;
                        const newSpan = temp.firstChild;
                        if (newSpan) {
                            const existingSpan = document.querySelector('[data-span-id="' + data.id + '"]');
                            if (existingSpan) {
                                existingSpan.replaceWith(newSpan);
                            } else {
                                spansContainer.appendChild(newSpan);
                                newSpan.scrollIntoView({ behavior: 'smooth', block: 'end' });
                            }
                        }
                    }
                } catch (e) {
                    console.error('Error parsing SSE data:', e);
                }
            });

            eventSource.onopen = function() {
                console.log('SSE connection established for session:', sessionId);
            };

            eventSource.onerror = function(event) {
                console.error('SSE error, will auto-reconnect:', event);
            };

            window.addEventListener('beforeunload', function() {
                eventSource.close();
            });
        }
    }

    // Session list page - stream all spans to update session list
    const sessionList = document.getElementById('session-list');
    if (sessionList) {
        console.log('Connecting to SSE firehose for session list');
        const eventSource = new EventSource('/stream');

        // Track sessions we've seen to add new ones
        const knownSessions = new Set();
        sessionList.querySelectorAll('.session-item').forEach(function(item) {
            const link = item.querySelector('a');
            if (link) {
                const href = link.getAttribute('href');
                if (href) {
                    const match = href.match(/\/sessions\/(.+)$/);
                    if (match) knownSessions.add(match[1]);
                }
            }
        });

        eventSource.addEventListener('span', function(event) {
            try {
                const data = JSON.parse(event.data);
                if (data.session_id && !knownSessions.has(data.session_id)) {
                    console.log('New session detected:', data.session_id);
                    knownSessions.add(data.session_id);

                    // Remove empty state if present
                    const emptyState = sessionList.querySelector('.empty-state');
                    if (emptyState) {
                        emptyState.remove();
                    }

                    // Add new session to the list
                    const newItem = document.createElement('li');
                    newItem.className = 'session-item session-item-new';
                    newItem.innerHTML = '<a href="/sessions/' + data.session_id + '">' + data.session_id + '</a><span class="session-meta">Just now</span>';
                    sessionList.insertBefore(newItem, sessionList.firstChild);

                    // Flash animation
                    setTimeout(function() {
                        newItem.classList.remove('session-item-new');
                    }, 2000);
                }
            } catch (e) {
                console.error('Error parsing SSE data:', e);
            }
        });

        eventSource.onopen = function() {
            console.log('SSE firehose connected for session list');
        };

        eventSource.onerror = function(event) {
            console.error('SSE firehose error:', event);
        };

        window.addEventListener('beforeunload', function() {
            eventSource.close();
        });
    }
})();
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_layout_renders_valid_html() {
        let content = html! { p { "Test content" } };
        let result = base_layout("Test Page", content);
        let html_str = result.into_string();

        assert!(html_str.contains("<!DOCTYPE html>"));
        assert!(html_str.contains("<html lang=\"en\">"));
        assert!(html_str.contains("<title>Test Page - Traceview</title>"));
        assert!(html_str.contains("Test content"));
    }

    #[test]
    fn test_base_layout_includes_header() {
        let content = html! {};
        let result = base_layout("Home", content);
        let html_str = result.into_string();

        assert!(html_str.contains("<header>"));
        assert!(html_str.contains("Traceview"));
        assert!(html_str.contains("href=\"/\""));
    }

    #[test]
    fn test_base_layout_includes_sse_script() {
        let content = html! {};
        let result = base_layout("Test", content);
        let html_str = result.into_string();

        assert!(html_str.contains("<script>"));
        assert!(html_str.contains("EventSource"));
    }

    #[test]
    fn test_base_layout_includes_styles() {
        let content = html! {};
        let result = base_layout("Test", content);
        let html_str = result.into_string();

        assert!(html_str.contains("<style>"));
        assert!(html_str.contains("box-sizing"));
    }

    #[test]
    fn test_base_layout_escapes_title() {
        let content = html! {};
        let result = base_layout("<script>alert('xss')</script>", content);
        let html_str = result.into_string();

        // The title should be escaped
        assert!(!html_str.contains("<script>alert"));
        assert!(html_str.contains("&lt;script&gt;"));
    }
}
