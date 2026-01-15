//! Base HTML layout for traceview pages.

use maud::{DOCTYPE, Markup, PreEscaped, html};

/// Renders the three-panel app layout with sidebar and main content.
///
/// # Arguments
/// * `title` - The page title (will be appended with " - Traceview")
/// * `sidebar_content` - Markup for the session list sidebar
/// * `main_content` - The main content markup to render
/// * `show_toolbar` - Whether to show the filter/view toolbar
/// * `session_id` - Current session ID for SSE (if viewing a session)
#[allow(clippy::needless_pass_by_value)]
pub fn app_layout(
    title: &str,
    sidebar_content: Markup,
    main_content: Markup,
    show_toolbar: bool,
    session_id: Option<&str>,
) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" data-theme="light" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " - Traceview" }
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css";
                style { (PreEscaped(app_css())) }
            }
            body class="app-layout" {
                // Top header bar
                nav class="app-header" {
                    div class="app-logo" {
                        a href="/" { strong { "Traceview" } }
                    }
                    div class="app-actions" {
                        button id="theme-toggle" class="outline contrast" title="Toggle dark mode" { "\u{1F319}" }
                    }
                }

                // Main container with sidebar and content
                div class="app-container" {
                    // Left sidebar
                    aside class="app-sidebar" {
                        div class="sidebar-header" {
                            span { "Sessions" }
                        }
                        ul class="sidebar-list" id="session-list" {
                            (sidebar_content)
                        }
                    }

                    // Main content area
                    main class="app-main" {
                        @if show_toolbar {
                            div class="app-toolbar" {
                                // Filter tabs
                                div class="filter-tabs" role="group" {
                                    button class="filter-btn active" data-filter="all" { "All" }
                                    button class="filter-btn" data-filter="tools" { "Tools" }
                                    button class="filter-btn" data-filter="thoughts" { "Thoughts" }
                                    button class="filter-btn" data-filter="errors" { "Errors" }
                                }

                                // View toggle
                                div class="view-toggle" role="group" {
                                    button class="view-btn active" data-view="conversation" title="Conversation View" {
                                        (PreEscaped(r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>"#))
                                    }
                                    button class="view-btn" data-view="timeline" title="Timeline View" {
                                        (PreEscaped(r#"<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="4" width="18" height="4" rx="1"/><rect x="3" y="10" width="12" height="4" rx="1"/><rect x="3" y="16" width="15" height="4" rx="1"/></svg>"#))
                                    }
                                }
                            }
                        }

                        div class="content-area" {
                            // Conversation view (default)
                            div id="conversation-view" class="conversation-view" {
                                @if let Some(sid) = session_id {
                                    div id="spans-container" data-session-id=(sid) {
                                        (main_content)
                                    }
                                } @else {
                                    (main_content)
                                }
                            }

                            // Timeline view (hidden by default)
                            div id="timeline-view" class="timeline-view hidden" {
                                div class="timeline-header" id="timeline-header" {}
                                div class="timeline-body" id="timeline-body" {}
                            }
                        }
                    }
                }

                script { (PreEscaped(theme_script())) }
                script { (PreEscaped(app_script())) }
            }
        }
    }
}

/// Backwards-compatible simple layout (wraps app_layout with empty sidebar).
///
/// # Arguments
/// * `title` - The page title (will be appended with " - Traceview")
/// * `content` - The main content markup to render
#[allow(clippy::needless_pass_by_value)]
pub fn base_layout(title: &str, content: Markup) -> Markup {
    app_layout(title, html! {}, content, false, None)
}

/// CSS for the three-panel app layout.
fn app_css() -> &'static str {
    r#"
/* Three-panel app layout */
html, body { height: 100%; margin: 0; }
.app-layout {
    display: grid;
    grid-template-rows: auto 1fr;
    height: 100vh;
    overflow: hidden;
}

.app-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem 1rem;
    background: var(--pico-card-background-color);
    border-bottom: 1px solid var(--pico-muted-border-color);
}

.app-logo a {
    text-decoration: none;
    color: var(--pico-color);
}

.app-actions button {
    margin: 0;
    padding: 0.4rem 0.6rem;
}

.app-container {
    display: grid;
    grid-template-columns: 280px 1fr;
    overflow: hidden;
}

/* Sidebar */
.app-sidebar {
    background: var(--pico-card-background-color);
    border-right: 1px solid var(--pico-muted-border-color);
    display: flex;
    flex-direction: column;
    overflow: hidden;
}

.sidebar-header {
    padding: 0.75rem 1rem;
    font-weight: 600;
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--pico-muted-color);
    border-bottom: 1px solid var(--pico-muted-border-color);
}

.sidebar-list {
    flex: 1;
    overflow-y: auto;
    margin: 0;
    padding: 0;
    list-style: none;
}

.session-item {
    border-bottom: 1px solid var(--pico-muted-border-color);
}

.session-item a {
    display: block;
    padding: 0.75rem 1rem;
    text-decoration: none;
    color: var(--pico-color);
    transition: background 0.15s;
}

.session-item a:hover {
    background: var(--pico-primary-focus);
}

.session-item.active a {
    background: var(--pico-primary-focus);
    border-left: 3px solid var(--pico-primary);
}

.session-item-name {
    font-size: 0.9rem;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-bottom: 0.25rem;
}

.session-item-meta {
    display: flex;
    gap: 0.75rem;
    font-size: 0.75rem;
    color: var(--pico-muted-color);
}

.event-count {
    background: var(--pico-muted-border-color);
    padding: 0.1rem 0.4rem;
    border-radius: 10px;
    font-size: 0.7rem;
}

/* Main area */
.app-main {
    display: flex;
    flex-direction: column;
    overflow: hidden;
}

.app-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem 1rem;
    background: var(--pico-card-background-color);
    border-bottom: 1px solid var(--pico-muted-border-color);
}

.filter-tabs, .view-toggle {
    display: flex;
    gap: 0.25rem;
}

.filter-tabs button, .view-toggle button {
    margin: 0;
    padding: 0.4rem 0.75rem;
    font-size: 0.85rem;
}

.view-toggle button {
    padding: 0.4rem 0.5rem;
    display: flex;
    align-items: center;
    justify-content: center;
}

[role="group"] button.active {
    background: var(--pico-primary);
    color: var(--pico-primary-inverse);
}

.content-area {
    flex: 1;
    overflow-y: auto;
    padding: 1rem;
}

/* Conversation view */
.conversation-view { }
.conversation-view.hidden { display: none; }
.timeline-view.hidden { display: none; }

/* Timeline view */
.timeline-view {
    display: flex;
    flex-direction: column;
}

.timeline-header {
    display: flex;
    height: 28px;
    border-bottom: 1px solid var(--pico-muted-border-color);
    background: var(--pico-card-background-color);
    position: sticky;
    top: 0;
}

.timeline-tick {
    flex: 1;
    border-left: 1px solid var(--pico-muted-border-color);
    font-size: 0.7rem;
    color: var(--pico-muted-color);
    padding: 0.25rem 0.35rem;
}

.timeline-tick:first-child { border-left: none; }

.timeline-body {
    flex: 1;
}

.timeline-row {
    display: flex;
    align-items: center;
    height: 32px;
    border-bottom: 1px solid var(--pico-muted-border-color);
}

.timeline-row:hover {
    background: var(--pico-primary-focus);
}

.timeline-row-label {
    width: 140px;
    flex-shrink: 0;
    padding: 0 0.5rem;
    font-size: 0.75rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: 0.35rem;
}

.timeline-row-bar-container {
    flex: 1;
    position: relative;
    height: 100%;
}

.timeline-bar {
    position: absolute;
    height: 20px;
    top: 6px;
    border-radius: 3px;
    cursor: pointer;
    min-width: 4px;
}

.timeline-bar:hover { opacity: 0.8; }

/* Timeline bar colors by kind */
.timeline-bar[data-kind="user"] { background: var(--pico-primary); }
.timeline-bar[data-kind="assistant"] { background: #22c55e; }
.timeline-bar[data-kind="thinking"] { background: #8b5cf6; }
.timeline-bar[data-kind="tool_call"] { background: #f59e0b; }
.timeline-bar[data-kind="tool_result"] { background: #10b981; }
.timeline-bar[data-kind="system"] { background: #6b7280; }

/* Span styling */
.span {
    margin-bottom: 0.5rem;
    padding: 0.75rem;
    border-radius: var(--pico-border-radius);
    border-left: 3px solid var(--pico-muted-border-color);
    position: relative;
}

.span[data-kind="user"] { border-left-color: var(--pico-primary); background: var(--pico-primary-focus); }
.span[data-kind="assistant"] { border-left-color: #22c55e; background: rgba(34, 197, 94, 0.1); }
.span[data-kind="tool_call"], .span[data-kind="tool_result"] { border-left-color: #f59e0b; background: rgba(245, 158, 11, 0.1); }
.span[data-kind="thinking"] { border-left-color: #8b5cf6; background: rgba(139, 92, 246, 0.1); }
.span[data-kind="system"] { border-left-color: #6b7280; background: rgba(107, 114, 128, 0.1); font-style: italic; }

.span-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.25rem; }
.span-kind { font-size: 0.7rem; text-transform: uppercase; font-weight: 600; letter-spacing: 0.05em; }
.span-meta { font-size: 0.75rem; color: var(--pico-muted-color); }
.span-timestamp { font-size: 0.7rem; color: var(--pico-muted-color); }
.span-content { margin-top: 0.5rem; white-space: pre-wrap; font-size: 0.875rem; }

/* Tool details - collapsible */
details.tool-details { margin: 0; }
details.tool-details summary { cursor: pointer; padding: 0.5rem; margin: -0.5rem; }
details.tool-details summary::-webkit-details-marker { display: none; }
details.tool-details summary::before { content: "▶ "; font-size: 0.75rem; }
details.tool-details[open] summary::before { content: "▼ "; }
.tool-content { margin-top: 0.5rem; padding: 0.5rem; background: var(--pico-code-background-color); border-radius: var(--pico-border-radius); }
.tool-content pre { margin: 0; white-space: pre-wrap; font-size: 0.8rem; }

/* Token summary */
.token-summary { display: flex; gap: 1rem; flex-wrap: wrap; margin-bottom: 1rem; }
.token-stat { font-size: 0.875rem; color: var(--pico-muted-color); }
.token-total { font-weight: 600; color: var(--pico-primary); }

/* Session header in content area */
.session-header { margin-bottom: 1rem; }
.session-header h2 { margin-bottom: 0.5rem; }

/* Hide wrapper spans */
.span.wrapper-span { display: none; }

/* Empty state */
.empty-state {
    text-align: center;
    padding: 2rem;
    color: var(--pico-muted-color);
}

/* Highlight animation for span scroll */
@keyframes span-highlight {
    from { box-shadow: 0 0 0 3px var(--pico-primary); }
    to { box-shadow: none; }
}
.span.highlight { animation: span-highlight 1.5s ease-out; }

/* Animation for new items */
@keyframes highlight { from { background: var(--pico-primary-focus); } to { background: transparent; } }
.session-item-new { animation: highlight 2s ease-out; }
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

/// Returns the JavaScript for app interactions including SSE, filters, and timeline.
fn app_script() -> &'static str {
    r#"
(function() {
    // Filter functionality
    document.querySelectorAll('.filter-btn').forEach(function(btn) {
        btn.addEventListener('click', function() {
            var filter = this.dataset.filter;
            document.querySelectorAll('.span').forEach(function(span) {
                var kind = span.dataset.kind;
                var hasError = span.dataset.hasError === 'true';
                var show = true;
                if (filter === 'tools') {
                    show = kind === 'tool_call' || kind === 'tool_result';
                } else if (filter === 'thoughts') {
                    show = kind === 'thinking';
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

    // View toggle functionality
    document.querySelectorAll('.view-btn').forEach(function(btn) {
        btn.addEventListener('click', function() {
            var view = this.dataset.view;

            document.querySelectorAll('.view-btn').forEach(function(b) {
                b.classList.remove('active');
            });
            this.classList.add('active');

            var conversationView = document.getElementById('conversation-view');
            var timelineView = document.getElementById('timeline-view');

            if (view === 'conversation') {
                conversationView.classList.remove('hidden');
                timelineView.classList.add('hidden');
            } else if (view === 'timeline') {
                conversationView.classList.add('hidden');
                timelineView.classList.remove('hidden');
                if (!timelineView.dataset.rendered) {
                    renderTimeline();
                    timelineView.dataset.rendered = 'true';
                }
            }
        });
    });

    // Timeline rendering
    function renderTimeline() {
        var spans = Array.from(document.querySelectorAll('.span[data-start-time]')).map(function(el) {
            return {
                id: el.dataset.spanId,
                kind: el.dataset.kind,
                startTime: parseInt(el.dataset.startTime, 10),
                endTime: parseInt(el.dataset.endTime, 10) || (Date.now() * 1000000),
                label: el.querySelector('.span-kind')?.textContent || el.dataset.kind
            };
        }).filter(function(s) { return !isNaN(s.startTime); });

        if (spans.length === 0) {
            document.getElementById('timeline-body').innerHTML = '<div class="empty-state">No timing data available</div>';
            return;
        }

        var minTime = Math.min.apply(null, spans.map(function(s) { return s.startTime; }));
        var maxTime = Math.max.apply(null, spans.map(function(s) { return s.endTime; }));
        var totalDuration = maxTime - minTime;
        if (totalDuration === 0) totalDuration = 1;

        // Render time axis
        var header = document.getElementById('timeline-header');
        header.innerHTML = '';
        var tickCount = 8;
        for (var i = 0; i <= tickCount; i++) {
            var tickTime = minTime + (totalDuration * i / tickCount);
            var tick = document.createElement('div');
            tick.className = 'timeline-tick';
            tick.textContent = formatTimelineTick(tickTime, minTime);
            header.appendChild(tick);
        }

        // Render rows
        var body = document.getElementById('timeline-body');
        body.innerHTML = '';
        spans.forEach(function(span) {
            var row = document.createElement('div');
            row.className = 'timeline-row';

            var label = document.createElement('div');
            label.className = 'timeline-row-label';
            label.textContent = span.label;

            var barContainer = document.createElement('div');
            barContainer.className = 'timeline-row-bar-container';

            var bar = document.createElement('div');
            bar.className = 'timeline-bar';
            bar.dataset.kind = span.kind;

            var startPercent = ((span.startTime - minTime) / totalDuration) * 100;
            var widthPercent = ((span.endTime - span.startTime) / totalDuration) * 100;
            bar.style.left = startPercent + '%';
            bar.style.width = Math.max(widthPercent, 0.3) + '%';

            bar.addEventListener('click', function() {
                var targetSpan = document.querySelector('[data-span-id="' + span.id + '"]');
                if (targetSpan) {
                    document.querySelector('[data-view="conversation"]').click();
                    setTimeout(function() {
                        targetSpan.scrollIntoView({ behavior: 'smooth', block: 'center' });
                        targetSpan.classList.add('highlight');
                        setTimeout(function() { targetSpan.classList.remove('highlight'); }, 1500);
                    }, 100);
                }
            });

            barContainer.appendChild(bar);
            row.appendChild(label);
            row.appendChild(barContainer);
            body.appendChild(row);
        });
    }

    function formatTimelineTick(nanos, baseNanos) {
        var relativeMs = (nanos - baseNanos) / 1000000;
        if (relativeMs < 1000) {
            return relativeMs.toFixed(0) + 'ms';
        } else if (relativeMs < 60000) {
            return (relativeMs / 1000).toFixed(1) + 's';
        } else {
            var mins = Math.floor(relativeMs / 60000);
            var secs = Math.floor((relativeMs % 60000) / 1000);
            return mins + 'm' + secs + 's';
        }
    }

    // Session detail page - SSE for spans
    var spansContainer = document.getElementById('spans-container');
    if (spansContainer && spansContainer.dataset.sessionId) {
        var sessionId = spansContainer.dataset.sessionId;
        console.log('Connecting to SSE for session:', sessionId);
        var eventSource = new EventSource('/sessions/' + sessionId + '/stream');

        eventSource.addEventListener('span', function(event) {
            try {
                var data = JSON.parse(event.data);
                if (data.html) {
                    var emptyState = spansContainer.querySelector('.empty-state');
                    if (emptyState) emptyState.remove();

                    var temp = document.createElement('div');
                    temp.innerHTML = data.html;
                    var newSpan = temp.firstChild;
                    if (newSpan) {
                        var existingSpan = document.querySelector('[data-span-id="' + data.id + '"]');
                        if (existingSpan) {
                            existingSpan.replaceWith(newSpan);
                        } else {
                            spansContainer.appendChild(newSpan);
                            newSpan.scrollIntoView({ behavior: 'smooth', block: 'end' });
                        }
                        // Re-render timeline if visible
                        var timelineView = document.getElementById('timeline-view');
                        if (timelineView && !timelineView.classList.contains('hidden')) {
                            renderTimeline();
                        }
                    }
                }
            } catch (e) {
                console.error('Error parsing SSE data:', e);
            }
        });

        eventSource.onerror = function() {
            console.error('SSE error, will auto-reconnect');
        };

        window.addEventListener('beforeunload', function() {
            eventSource.close();
        });
    }

    // Sidebar SSE for new sessions
    var sessionList = document.getElementById('session-list');
    if (sessionList && !spansContainer) {
        console.log('Connecting to SSE firehose for sidebar');
        var eventSource = new EventSource('/stream');

        var knownSessions = new Set();
        sessionList.querySelectorAll('.session-item').forEach(function(item) {
            var sid = item.dataset.sessionId;
            if (sid) knownSessions.add(sid);
        });

        eventSource.addEventListener('span', function(event) {
            try {
                var data = JSON.parse(event.data);
                if (data.session_id && !knownSessions.has(data.session_id)) {
                    knownSessions.add(data.session_id);

                    var emptyState = sessionList.querySelector('.empty-state');
                    if (emptyState) emptyState.remove();

                    var newItem = document.createElement('li');
                    newItem.className = 'session-item session-item-new';
                    newItem.dataset.sessionId = data.session_id;
                    newItem.innerHTML = '<a href="/sessions/' + data.session_id + '"><div class="session-item-name">' + data.session_id + '</div><div class="session-item-meta"><span class="session-time">Just now</span><span class="event-count">1 events</span></div></a>';
                    sessionList.insertBefore(newItem, sessionList.firstChild);

                    setTimeout(function() {
                        newItem.classList.remove('session-item-new');
                    }, 2000);
                } else if (data.session_id) {
                    // Update event count for existing session
                    var item = sessionList.querySelector('.session-item[data-session-id="' + data.session_id + '"]');
                    if (item) {
                        var countEl = item.querySelector('.event-count');
                        if (countEl) {
                            var match = countEl.textContent.match(/(\d+)/);
                            var count = match ? parseInt(match[1], 10) + 1 : 1;
                            countEl.textContent = count + ' events';
                        }
                    }
                }
            } catch (e) {
                console.error('Error parsing SSE data:', e);
            }
        });

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
    fn test_app_layout_renders_valid_html() {
        let sidebar = html! { li { "Test session" } };
        let content = html! { p { "Test content" } };
        let result = app_layout("Test Page", sidebar, content, true, Some("test-session"));
        let html_str = result.into_string();

        assert!(html_str.contains("<!DOCTYPE html>"));
        assert!(html_str.contains("<html lang=\"en\" data-theme=\"light\">"));
        assert!(html_str.contains("<title>Test Page - Traceview</title>"));
        assert!(html_str.contains("Test content"));
        assert!(html_str.contains("Test session"));
    }

    #[test]
    fn test_app_layout_includes_sidebar() {
        let sidebar = html! { li { "Session 1" } };
        let content = html! {};
        let result = app_layout("Test", sidebar, content, false, None);
        let html_str = result.into_string();

        assert!(html_str.contains("app-sidebar"));
        assert!(html_str.contains("sidebar-list"));
        assert!(html_str.contains("Session 1"));
    }

    #[test]
    fn test_app_layout_shows_toolbar_when_enabled() {
        let result = app_layout("Test", html! {}, html! {}, true, None);
        let html_str = result.into_string();

        assert!(html_str.contains("app-toolbar"));
        assert!(html_str.contains("filter-tabs"));
        assert!(html_str.contains("view-toggle"));
        assert!(html_str.contains("data-filter=\"all\""));
        assert!(html_str.contains("data-filter=\"tools\""));
        assert!(html_str.contains("data-filter=\"thoughts\""));
        assert!(html_str.contains("data-view=\"conversation\""));
        assert!(html_str.contains("data-view=\"timeline\""));
    }

    #[test]
    fn test_app_layout_hides_toolbar_when_disabled() {
        let result = app_layout("Test", html! {}, html! {}, false, None);
        let html_str = result.into_string();

        // Toolbar HTML element should not be present (CSS class still exists in stylesheet)
        assert!(!html_str.contains("<div class=\"app-toolbar\">"));
        assert!(!html_str.contains("<div class=\"filter-tabs\""));
    }

    #[test]
    fn test_app_layout_includes_session_id_for_sse() {
        let result = app_layout("Test", html! {}, html! {}, true, Some("my-session-123"));
        let html_str = result.into_string();

        assert!(html_str.contains("data-session-id=\"my-session-123\""));
        assert!(html_str.contains("spans-container"));
    }

    #[test]
    fn test_base_layout_backwards_compatible() {
        let content = html! { p { "Legacy content" } };
        let result = base_layout("Legacy Page", content);
        let html_str = result.into_string();

        assert!(html_str.contains("Legacy Page - Traceview"));
        assert!(html_str.contains("Legacy content"));
        assert!(html_str.contains("app-layout"));
    }

    #[test]
    fn test_app_layout_includes_theme_toggle() {
        let result = app_layout("Test", html! {}, html! {}, false, None);
        let html_str = result.into_string();

        assert!(html_str.contains("id=\"theme-toggle\""));
        assert!(html_str.contains("Toggle dark mode"));
    }

    #[test]
    fn test_app_layout_includes_timeline_view() {
        let result = app_layout("Test", html! {}, html! {}, true, None);
        let html_str = result.into_string();

        assert!(html_str.contains("timeline-view"));
        assert!(html_str.contains("timeline-header"));
        assert!(html_str.contains("timeline-body"));
    }

    #[test]
    fn test_app_layout_escapes_title() {
        let result = app_layout("<script>alert('xss')</script>", html! {}, html! {}, false, None);
        let html_str = result.into_string();

        assert!(!html_str.contains("<script>alert"));
        assert!(html_str.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_app_css_includes_grid_layout() {
        let css = app_css();
        assert!(css.contains(".app-layout"));
        assert!(css.contains("grid-template-columns: 280px 1fr"));
        assert!(css.contains(".app-sidebar"));
        assert!(css.contains(".app-main"));
    }

    #[test]
    fn test_app_script_includes_filter_logic() {
        let script = app_script();
        assert!(script.contains("filter-btn"));
        assert!(script.contains("dataset.filter")); // JS accesses data-filter via dataset.filter
        assert!(script.contains("thoughts"));
        assert!(script.contains("thinking"));
    }

    #[test]
    fn test_app_script_includes_timeline_logic() {
        let script = app_script();
        assert!(script.contains("renderTimeline"));
        assert!(script.contains("timeline-bar"));
        assert!(script.contains("data-start-time"));
    }
}
