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
        html lang="en" data-theme="light" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " - Traceview" }
                // Pico.css - minimal classless CSS framework
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css";
                style { (PreEscaped(minimal_css())) }
            }
            body {
                nav class="container-fluid" {
                    ul {
                        li { a href="/" { strong { "Traceview" } } }
                    }
                    ul {
                        li {
                            button id="theme-toggle" class="outline contrast" title="Toggle dark mode" { "\u{1F319}" }
                        }
                    }
                }
                main class="container" { (content) }
                script { (PreEscaped(theme_script())) }
                script { (PreEscaped(sse_script())) }
            }
        }
    }
}

/// Minimal custom CSS to complement Pico.css
fn minimal_css() -> &'static str {
    r#"
/* Token summary bar */
.token-summary { display: flex; gap: 1rem; flex-wrap: wrap; margin-bottom: 1rem; }
.token-stat { font-size: 0.875rem; color: var(--pico-muted-color); }
.token-total { font-weight: 600; color: var(--pico-primary); }

/* Filter buttons - use Pico's role="group" */
[role="group"] button.active { background: var(--pico-primary); color: var(--pico-primary-inverse); }

/* Spans styling */
.span { margin-bottom: 0.5rem; padding: 0.75rem; border-radius: var(--pico-border-radius); border-left: 3px solid var(--pico-muted-border-color); }
.span[data-kind="user"] { border-left-color: var(--pico-primary); background: var(--pico-primary-focus); }
.span[data-kind="assistant"] { border-left-color: #22c55e; background: rgba(34, 197, 94, 0.1); }
.span[data-kind="tool_call"], .span[data-kind="tool_result"] { border-left-color: #f59e0b; background: rgba(245, 158, 11, 0.1); }
.span[data-kind="thinking"] { border-left-color: #8b5cf6; background: rgba(139, 92, 246, 0.1); }
.span[data-kind="system"] { border-left-color: #6b7280; background: rgba(107, 114, 128, 0.1); font-style: italic; }

/* Span header */
.span-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.25rem; }
.span-kind { font-size: 0.75rem; text-transform: uppercase; font-weight: 600; opacity: 0.7; }
.span-meta { font-size: 0.75rem; color: var(--pico-muted-color); }

/* Content area */
.span-content { margin-top: 0.5rem; white-space: pre-wrap; font-size: 0.875rem; }

/* Tool details - collapsible */
details.tool-details { margin: 0; }
details.tool-details summary { cursor: pointer; padding: 0.5rem; margin: -0.5rem; }
details.tool-details summary::-webkit-details-marker { display: none; }
details.tool-details summary::before { content: "▶ "; font-size: 0.75rem; }
details.tool-details[open] summary::before { content: "▼ "; }
.tool-content { margin-top: 0.5rem; padding: 0.5rem; background: var(--pico-code-background-color); border-radius: var(--pico-border-radius); }
.tool-content pre { margin: 0; white-space: pre-wrap; font-size: 0.8rem; }

/* Copy button */
.copy-btn { font-size: 0.75rem; padding: 0.25rem 0.5rem; margin-left: 0.5rem; }

/* Session list */
.session-item { border-bottom: 1px solid var(--pico-muted-border-color); }
.session-item:last-child { border-bottom: none; }
.session-meta { font-size: 0.75rem; color: var(--pico-muted-color); display: block; }

/* Animation for new items */
@keyframes highlight { from { background: var(--pico-primary-focus); } to { background: transparent; } }
.session-item-new { animation: highlight 2s ease-out; }

/* Hide wrapper spans with no content */
.span.wrapper-span { display: none; }
"#
}

/// Returns the JavaScript for theme toggle functionality.
fn theme_script() -> &'static str {
    r#"
(function() {
    var savedTheme = localStorage.getItem('traceview-theme') || 'light';
    document.documentElement.dataset.theme = savedTheme;
    var btn = document.getElementById('theme-toggle');
    if (btn) {
        btn.textContent = savedTheme === 'dark' ? '\u2600\uFE0F' : '\uD83C\uDF19';
    }
})();

document.getElementById('theme-toggle')?.addEventListener('click', function() {
    var current = document.documentElement.dataset.theme;
    var next = current === 'dark' ? 'light' : 'dark';
    document.documentElement.dataset.theme = next;
    localStorage.setItem('traceview-theme', next);
    this.textContent = next === 'dark' ? '\u2600\uFE0F' : '\uD83C\uDF19';
});
"#
}

/// Returns the JavaScript for SSE connection handling and UI interactions.
fn sse_script() -> &'static str {
    r#"
(function() {
    // Copy to clipboard functionality
    document.addEventListener('click', function(e) {
        if (e.target.classList.contains('copy-btn')) {
            var targetId = e.target.dataset.copyTarget;
            var targetEl = document.getElementById(targetId);
            var content = targetEl ? targetEl.textContent : null;
            if (content) {
                navigator.clipboard.writeText(content).then(function() {
                    var original = e.target.textContent;
                    e.target.textContent = '\u2713';
                    setTimeout(function() { e.target.textContent = original; }, 1500);
                });
            }
        }
    });

    // Filter functionality for session detail page
    document.querySelectorAll('.filter-btn').forEach(function(btn) {
        btn.addEventListener('click', function() {
            var filter = this.dataset.filter;
            document.querySelectorAll('.span').forEach(function(span) {
                var kind = span.dataset.kind;
                var hasError = span.dataset.hasError === 'true';
                var show = true;
                if (filter === 'tools') {
                    show = kind === 'tool_call' || kind === 'tool_result';
                } else if (filter === 'errors') {
                    show = hasError;
                }
                span.style.display = show ? '' : 'none';
            });
            document.querySelectorAll('.filter-btn').forEach(function(b) {
                b.classList.remove('active');
            });
            this.classList.add('active');
        });
    });

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
        assert!(html_str.contains("<html lang=\"en\" data-theme=\"light\">"));
        assert!(html_str.contains("<title>Test Page - Traceview</title>"));
        assert!(html_str.contains("Test content"));
    }

    #[test]
    fn test_base_layout_includes_header() {
        let content = html! {};
        let result = base_layout("Home", content);
        let html_str = result.into_string();

        // Pico.css uses <nav> for header navigation
        assert!(html_str.contains("<nav"));
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

        // We use Pico.css from CDN plus minimal custom styles
        assert!(html_str.contains("pico.min.css"));
        assert!(html_str.contains("<style>"));
        assert!(html_str.contains(".span"));
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

    #[test]
    fn test_layout_includes_clipboard_script() {
        let content = html! {};
        let result = base_layout("Test", content);
        let html_str = result.into_string();

        // Verify clipboard functionality is included
        assert!(html_str.contains("copy-btn"));
        assert!(html_str.contains("navigator.clipboard.writeText"));
        assert!(html_str.contains("copyTarget"));
    }

    #[test]
    fn test_layout_includes_filter_script() {
        let content = html! {};
        let result = base_layout("Test", content);
        let html_str = result.into_string();

        // Verify filter functionality is included
        assert!(html_str.contains("filter-btn"));
        assert!(html_str.contains("dataset.filter"));
        assert!(html_str.contains("dataset.hasError"));
        assert!(html_str.contains("tool_call"));
        assert!(html_str.contains("tool_result"));
    }

    #[test]
    fn test_base_layout_includes_theme_toggle() {
        let content = html! {};
        let result = base_layout("Test", content);
        let html_str = result.into_string();

        // Verify theme toggle button is included
        assert!(html_str.contains("id=\"theme-toggle\""));
        assert!(html_str.contains("class=\"outline contrast\""));
        assert!(html_str.contains("Toggle dark mode"));
    }

    #[test]
    fn test_base_layout_includes_theme_script() {
        let content = html! {};
        let result = base_layout("Test", content);
        let html_str = result.into_string();

        // Verify theme script is included
        assert!(html_str.contains("traceview-theme"));
        assert!(html_str.contains("localStorage"));
        assert!(html_str.contains("dataset.theme"));
    }
}
