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
                style { (PreEscaped(app_css())) }
            }
            body class="app-layout" {
                // Main container with sidebar and content
                div class="app-container" {
                    // Left sidebar
                    aside class="app-sidebar" {
                        div class="sidebar-header" {
                            a href="/" class="sidebar-logo" { "Traceview" }
                        }
                        // Search box
                        div class="search-container" {
                            input type="search"
                                id="global-search"
                                class="search-input"
                                placeholder="Search... (⌘K)"
                                autocomplete="off";
                            div id="search-results" class="search-results hidden" {}
                        }
                        div class="sidebar-section-label" { "Sessions" }
                        ul class="sidebar-list" id="session-list" {
                            (sidebar_content)
                        }
                        div class="sidebar-footer" {
                            button id="theme-toggle" class="theme-toggle" title="Toggle theme" {
                                (PreEscaped(r#"<svg class="icon-sun" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>"#))
                                (PreEscaped(r#"<svg class="icon-moon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>"#))
                            }
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

/// Complete custom CSS for Traceview - no external dependencies.
fn app_css() -> &'static str {
    r#"
/* ==========================================================================
   Design Tokens
   ========================================================================== */
:root {
    /* Light mode (default) */
    --bg: #ffffff;
    --bg-surface: #fafafa;
    --bg-elevated: #ffffff;
    --text: #0a0a0a;
    --text-secondary: #525252;
    --text-muted: #737373;
    --border: #e5e5e5;
    --border-subtle: #f0f0f0;

    /* Interactive */
    --interactive: #171717;
    --interactive-hover: #262626;
    --interactive-text: #ffffff;

    /* Accent for highlights */
    --accent: #171717;
    --accent-subtle: rgba(23, 23, 23, 0.08);

    /* Semantic colors */
    --color-user: #2563eb;
    --color-assistant: #16a34a;
    --color-tool: #d97706;
    --color-thinking: #7c3aed;
    --color-error: #dc2626;
    --color-system: #737373;

    /* Spacing scale */
    --space-1: 0.25rem;
    --space-2: 0.5rem;
    --space-3: 0.75rem;
    --space-4: 1rem;
    --space-5: 1.25rem;
    --space-6: 1.5rem;

    /* Typography */
    --font-sans: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
    --font-mono: "SF Mono", SFMono-Regular, ui-monospace, Menlo, Monaco, Consolas, monospace;
    --text-xs: 0.75rem;
    --text-sm: 0.8125rem;
    --text-base: 0.875rem;
    --text-lg: 1rem;

    /* Misc */
    --radius: 6px;
    --radius-sm: 4px;
    --transition: 150ms ease;
}

[data-theme="dark"] {
    --bg: #0a0a0a;
    --bg-surface: #111111;
    --bg-elevated: #171717;
    --text: #fafafa;
    --text-secondary: #a3a3a3;
    --text-muted: #737373;
    --border: #262626;
    --border-subtle: #1f1f1f;

    --interactive: #fafafa;
    --interactive-hover: #e5e5e5;
    --interactive-text: #0a0a0a;

    --accent: #fafafa;
    --accent-subtle: rgba(250, 250, 250, 0.08);
}

/* ==========================================================================
   Reset & Base
   ========================================================================== */
*, *::before, *::after { box-sizing: border-box; }

html, body {
    height: 100%;
    margin: 0;
    padding: 0;
}

body {
    font-family: var(--font-sans);
    font-size: var(--text-base);
    line-height: 1.5;
    color: var(--text);
    background: var(--bg);
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
}

h1, h2, h3, h4, h5, h6 {
    margin: 0 0 var(--space-3) 0;
    font-weight: 600;
    line-height: 1.25;
    color: var(--text);
}

h2 { font-size: var(--text-lg); }

p { margin: 0 0 var(--space-3) 0; }

a {
    color: var(--text);
    text-decoration: none;
}

a:hover { color: var(--text-secondary); }

code {
    font-family: var(--font-mono);
    font-size: 0.9em;
    background: var(--bg-surface);
    padding: 0.15em 0.4em;
    border-radius: var(--radius-sm);
}

pre {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
}

ul { margin: 0; padding: 0; list-style: none; }

/* ==========================================================================
   Buttons
   ========================================================================== */
button {
    font-family: inherit;
    font-size: var(--text-sm);
    font-weight: 500;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-elevated);
    color: var(--text);
    cursor: pointer;
    transition: all var(--transition);
}

button:hover {
    background: var(--bg-surface);
    border-color: var(--text-muted);
}

button.active {
    background: var(--interactive);
    color: var(--interactive-text);
    border-color: var(--interactive);
}

button.active:hover {
    background: var(--interactive-hover);
    border-color: var(--interactive-hover);
}

.icon-btn {
    padding: var(--space-2);
    min-width: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
}

/* ==========================================================================
   App Layout
   ========================================================================== */
.app-layout {
    height: 100vh;
    overflow: hidden;
}

.app-container {
    display: grid;
    grid-template-columns: 240px 1fr;
    height: 100%;
    overflow: hidden;
}

/* ==========================================================================
   Sidebar
   ========================================================================== */
.app-sidebar {
    background: var(--bg-surface);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow: hidden;
}

.sidebar-header {
    padding: var(--space-4);
}

.sidebar-logo {
    font-weight: 600;
    font-size: var(--text-lg);
    color: var(--text);
    letter-spacing: -0.02em;
}

.sidebar-logo:hover {
    color: var(--text-secondary);
}

.sidebar-section-label {
    padding: var(--space-3) var(--space-4) var(--space-2);
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
}

.sidebar-list {
    flex: 1;
    overflow-y: auto;
}

.sidebar-footer {
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--border);
}

/* Search */
.search-container {
    padding: 0 var(--space-4) var(--space-3);
    position: relative;
}

.search-input {
    width: 100%;
    padding: var(--space-2) var(--space-3);
    font-family: inherit;
    font-size: var(--text-sm);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-elevated);
    color: var(--text);
    outline: none;
    transition: border-color var(--transition), box-shadow var(--transition);
}

.search-input::placeholder {
    color: var(--text-muted);
}

.search-input:focus {
    border-color: var(--interactive);
    box-shadow: 0 0 0 3px var(--accent-subtle);
}

.search-results {
    position: absolute;
    top: 100%;
    left: var(--space-4);
    right: var(--space-4);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
    max-height: 400px;
    overflow-y: auto;
    z-index: 100;
}

.search-results.hidden {
    display: none;
}

.search-section-label {
    padding: var(--space-2) var(--space-3);
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
    background: var(--bg-surface);
    border-bottom: 1px solid var(--border-subtle);
}

.search-result-item {
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border-subtle);
    cursor: pointer;
    transition: background var(--transition);
}

.search-result-item:last-child {
    border-bottom: none;
}

.search-result-item:hover {
    background: var(--accent-subtle);
}

.search-result-title {
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--text);
    margin-bottom: var(--space-1);
}

.search-result-snippet {
    font-size: var(--text-xs);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.search-result-snippet mark {
    background: rgba(255, 220, 0, 0.3);
    color: inherit;
    padding: 0 2px;
    border-radius: 2px;
}

[data-theme="dark"] .search-result-snippet mark {
    background: rgba(255, 220, 0, 0.2);
}

.search-no-results {
    padding: var(--space-4);
    text-align: center;
    color: var(--text-muted);
    font-size: var(--text-sm);
}

.search-loading {
    padding: var(--space-3);
    text-align: center;
    color: var(--text-muted);
    font-size: var(--text-sm);
}

.theme-toggle {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2);
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
}

.theme-toggle:hover {
    color: var(--text);
    background: transparent;
}

.theme-toggle svg {
    display: block;
}

/* Show sun in dark mode, moon in light mode */
[data-theme="light"] .icon-sun { display: none; }
[data-theme="light"] .icon-moon { display: block; }
[data-theme="dark"] .icon-sun { display: block; }
[data-theme="dark"] .icon-moon { display: none; }

.session-item {
    border-bottom: 1px solid var(--border-subtle);
}

.session-item a {
    display: block;
    padding: var(--space-3) var(--space-4);
    transition: background var(--transition);
}

.session-item a:hover {
    background: var(--accent-subtle);
}

.session-item.active a {
    background: var(--accent-subtle);
    border-left: 2px solid var(--accent);
    padding-left: calc(var(--space-4) - 2px);
}

.session-item-name {
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-bottom: var(--space-1);
}

.session-item-meta {
    display: flex;
    gap: var(--space-3);
    font-size: var(--text-xs);
    color: var(--text-muted);
}

.event-count {
    background: var(--border);
    padding: 0.1rem 0.5rem;
    border-radius: 100px;
    font-weight: 500;
}

/* ==========================================================================
   Main Area
   ========================================================================== */
.app-main {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--bg);
}

.app-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-2) var(--space-4);
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border);
    gap: var(--space-4);
}

.filter-tabs, .view-toggle {
    display: flex;
    gap: var(--space-1);
}

.filter-tabs button, .view-toggle button {
    font-size: var(--text-xs);
    padding: var(--space-2) var(--space-3);
}

.view-toggle button {
    padding: var(--space-2);
}

.view-toggle button svg {
    display: block;
}

.content-area {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-4);
}

/* ==========================================================================
   Views
   ========================================================================== */
.conversation-view.hidden,
.timeline-view.hidden {
    display: none;
}

/* ==========================================================================
   Timeline
   ========================================================================== */
.timeline-view {
    display: flex;
    flex-direction: column;
}

.timeline-header {
    display: flex;
    height: 32px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-elevated);
    position: sticky;
    top: 0;
}

.timeline-tick {
    flex: 1;
    border-left: 1px solid var(--border);
    font-size: var(--text-xs);
    font-family: var(--font-mono);
    color: var(--text-muted);
    padding: var(--space-2);
    display: flex;
    align-items: center;
}

.timeline-tick:first-child { border-left: none; }

.timeline-body { flex: 1; }

.timeline-row {
    display: flex;
    align-items: center;
    height: 36px;
    border-bottom: 1px solid var(--border-subtle);
    transition: background var(--transition);
}

.timeline-row:hover {
    background: var(--accent-subtle);
}

.timeline-row-label {
    width: 120px;
    flex-shrink: 0;
    padding: 0 var(--space-3);
    font-size: var(--text-xs);
    font-weight: 500;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.timeline-row-bar-container {
    flex: 1;
    position: relative;
    height: 100%;
}

.timeline-bar {
    position: absolute;
    height: 22px;
    top: 7px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    min-width: 4px;
    transition: opacity var(--transition);
}

.timeline-bar:hover { opacity: 0.75; }

.timeline-bar[data-kind="user"] { background: var(--color-user); }
.timeline-bar[data-kind="assistant"] { background: var(--color-assistant); }
.timeline-bar[data-kind="thinking"] { background: var(--color-thinking); }
.timeline-bar[data-kind="tool_call"] { background: var(--color-tool); }
.timeline-bar[data-kind="tool_result"] { background: var(--color-tool); opacity: 0.7; }
.timeline-bar[data-kind="system"] { background: var(--color-system); }

/* ==========================================================================
   Spans
   ========================================================================== */
.span {
    margin-bottom: var(--space-2);
    padding: var(--space-3);
    border-radius: var(--radius);
    border-left: 3px solid var(--border);
    background: var(--bg-surface);
}

.span[data-kind="user"] {
    border-left-color: var(--color-user);
    background: rgba(37, 99, 235, 0.06);
}

.span[data-kind="assistant"] {
    border-left-color: var(--color-assistant);
    background: rgba(22, 163, 74, 0.06);
}

.span[data-kind="tool_call"],
.span[data-kind="tool_result"] {
    border-left-color: var(--color-tool);
    background: rgba(217, 119, 6, 0.06);
}

.span[data-kind="thinking"] {
    border-left-color: var(--color-thinking);
    background: rgba(124, 58, 237, 0.06);
}

.span[data-kind="system"] {
    border-left-color: var(--color-system);
    background: var(--bg-surface);
    font-style: italic;
}

.span-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--space-1);
}

.span-kind {
    font-size: var(--text-xs);
    text-transform: uppercase;
    font-weight: 600;
    letter-spacing: 0.05em;
    color: var(--text-secondary);
}

.span-meta {
    font-size: var(--text-xs);
    color: var(--text-muted);
    font-family: var(--font-mono);
}

.span-timestamp {
    font-size: var(--text-xs);
    color: var(--text-muted);
}

.span-content {
    margin-top: var(--space-2);
    white-space: pre-wrap;
    font-size: var(--text-sm);
    line-height: 1.6;
}

/* ==========================================================================
   Expandable Spans
   ========================================================================== */
.expandable-span {
    cursor: pointer;
    transition: background var(--transition);
}

.expandable-span:hover {
    background: var(--accent-subtle);
}

.expandable-span.expanded {
    background: var(--bg-elevated);
}

.span-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
}

.span-header-left {
    display: flex;
    align-items: center;
    gap: var(--space-2);
}

.expand-icon {
    font-size: var(--text-xs);
    color: var(--text-muted);
    transition: transform var(--transition);
    flex-shrink: 0;
}

.expandable-span.expanded .expand-icon {
    transform: rotate(90deg);
}

.span-preview {
    margin-top: var(--space-2);
    font-size: var(--text-sm);
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.expandable-span.expanded .span-preview {
    display: none;
}

.span-detail {
    margin-top: var(--space-3);
    padding-top: var(--space-3);
    border-top: 1px solid var(--border-subtle);
}

.span-detail.hidden {
    display: none;
}

.detail-section {
    margin-bottom: var(--space-3);
}

.detail-section:last-child {
    margin-bottom: 0;
}

.detail-label {
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
    margin-bottom: var(--space-2);
}

.detail-content {
    font-size: var(--text-sm);
    line-height: 1.6;
    white-space: pre-wrap;
    color: var(--text);
}

.detail-code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    background: var(--bg-surface);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    overflow-x: auto;
    margin: 0;
    color: var(--text-secondary);
}

/* Metadata grid */
.metadata-grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--space-1) var(--space-3);
    font-size: var(--text-xs);
}

.meta-row {
    display: contents;
}

.meta-key {
    color: var(--text-muted);
}

.meta-value {
    font-family: var(--font-mono);
    color: var(--text-secondary);
}

/* Tool span specific */
.tool-span .tool-name {
    font-size: var(--text-sm);
    background: none;
    padding: 0;
}

.tool-args-preview {
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 300px;
}

.tool-calls-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
}

.tool-call-item {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
}

.tool-call-item code {
    background: var(--bg-surface);
    padding: 0.1em 0.3em;
    border-radius: var(--radius-sm);
}

.tool-call-item .tool-args {
    color: var(--text-muted);
}

.no-content {
    color: var(--text-muted);
    font-style: italic;
}

/* ==========================================================================
   Session Header & Token Summary
   ========================================================================== */
.session-header {
    margin-bottom: var(--space-4);
}

.session-header h2 {
    font-family: var(--font-mono);
    font-size: var(--text-base);
    font-weight: 500;
    color: var(--text);
}

.token-summary {
    display: flex;
    gap: var(--space-4);
    flex-wrap: wrap;
    margin-bottom: var(--space-4);
}

.token-stat {
    font-size: var(--text-sm);
    color: var(--text-muted);
}

.token-total {
    font-weight: 600;
    color: var(--color-assistant);
}

/* ==========================================================================
   States & Utilities
   ========================================================================== */
.span.wrapper-span { display: none; }

.empty-state {
    text-align: center;
    padding: var(--space-6);
    color: var(--text-muted);
}

/* Highlight animation */
@keyframes span-highlight {
    from { box-shadow: 0 0 0 2px var(--accent); }
    to { box-shadow: none; }
}

.span.highlight {
    animation: span-highlight 1.5s ease-out;
}

/* New session animation */
@keyframes fade-highlight {
    from { background: var(--accent-subtle); }
    to { background: transparent; }
}

.session-item-new {
    animation: fade-highlight 2s ease-out;
}

/* Welcome message */
.welcome-message {
    max-width: 400px;
    margin: var(--space-6) auto;
    text-align: center;
}

.welcome-message h2 {
    font-size: var(--text-lg);
    margin-bottom: var(--space-2);
}

.welcome-message p {
    color: var(--text-muted);
    font-size: var(--text-sm);
}

/* Scrollbar styling */
::-webkit-scrollbar {
    width: 8px;
    height: 8px;
}

::-webkit-scrollbar-track {
    background: transparent;
}

::-webkit-scrollbar-thumb {
    background: var(--border);
    border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
    background: var(--text-muted);
}
"#
}

/// Returns the JavaScript for theme toggle functionality.
fn theme_script() -> &'static str {
    r#"
(function() {
    var savedTheme = localStorage.getItem('traceview-theme') || 'light';
    document.documentElement.dataset.theme = savedTheme;
})();

document.getElementById('theme-toggle')?.addEventListener('click', function() {
    var current = document.documentElement.dataset.theme;
    var next = current === 'dark' ? 'light' : 'dark';
    document.documentElement.dataset.theme = next;
    localStorage.setItem('traceview-theme', next);
});
"#
}

/// Returns the JavaScript for app interactions including SSE, filters, and timeline.
fn app_script() -> &'static str {
    r#"
(function() {
    // Span expand/collapse functionality
    document.addEventListener('click', function(e) {
        var span = e.target.closest('.span[data-expandable="true"]');
        if (!span) return;

        // Don't toggle if clicking a link or button inside
        if (e.target.closest('a, button, code')) return;

        span.classList.toggle('expanded');
        var detail = span.querySelector('.span-detail');
        if (detail) {
            detail.classList.toggle('hidden');
        }
    });

    // Search functionality
    var searchInput = document.getElementById('global-search');
    var searchResults = document.getElementById('search-results');
    var searchTimeout = null;

    if (searchInput && searchResults) {
        // Debounced search
        searchInput.addEventListener('input', function() {
            var query = this.value.trim();
            clearTimeout(searchTimeout);

            if (query.length < 2) {
                searchResults.classList.add('hidden');
                return;
            }

            searchResults.innerHTML = '<div class="search-loading">Searching...</div>';
            searchResults.classList.remove('hidden');

            searchTimeout = setTimeout(function() {
                fetch('/api/search?q=' + encodeURIComponent(query) + '&limit=20')
                    .then(function(res) { return res.json(); })
                    .then(function(data) {
                        renderSearchResults(data, query);
                    })
                    .catch(function(err) {
                        console.error('Search error:', err);
                        searchResults.innerHTML = '<div class="search-no-results">Search error</div>';
                    });
            }, 200);
        });

        // Keyboard shortcuts
        document.addEventListener('keydown', function(e) {
            // Cmd/Ctrl+K to focus search
            if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
                e.preventDefault();
                searchInput.focus();
                searchInput.select();
            }
            // Escape to close search
            if (e.key === 'Escape' && !searchResults.classList.contains('hidden')) {
                searchResults.classList.add('hidden');
                searchInput.blur();
            }
        });

        // Close search when clicking outside
        document.addEventListener('click', function(e) {
            if (!e.target.closest('.search-container')) {
                searchResults.classList.add('hidden');
            }
        });

        // Focus shows results if there's a query
        searchInput.addEventListener('focus', function() {
            if (this.value.trim().length >= 2) {
                searchResults.classList.remove('hidden');
            }
        });

        function renderSearchResults(data, query) {
            var html = '';
            var hasResults = false;

            if (data.sessions && data.sessions.length > 0) {
                hasResults = true;
                html += '<div class="search-section-label">Sessions</div>';
                data.sessions.forEach(function(match) {
                    var name = match.session.name || match.session.id;
                    html += '<a href="/sessions/' + match.session.id + '" class="search-result-item">';
                    html += '<div class="search-result-title">' + escapeHtml(name) + '</div>';
                    html += '<div class="search-result-snippet">' + highlightMatch(match.snippet, query) + '</div>';
                    html += '</a>';
                });
            }

            if (data.spans && data.spans.length > 0) {
                hasResults = true;
                html += '<div class="search-section-label">Spans</div>';
                data.spans.forEach(function(match) {
                    var label = match.span.kind || 'span';
                    html += '<a href="/sessions/' + match.span.session_id + '#span-' + match.span.id + '" class="search-result-item">';
                    html += '<div class="search-result-title">' + escapeHtml(label) + '</div>';
                    html += '<div class="search-result-snippet">' + highlightMatch(match.snippet, query) + '</div>';
                    html += '</a>';
                });
            }

            if (!hasResults) {
                html = '<div class="search-no-results">No results for "' + escapeHtml(query) + '"</div>';
            }

            searchResults.innerHTML = html;
        }

        function escapeHtml(text) {
            var div = document.createElement('div');
            div.textContent = text;
            return div.innerHTML;
        }

        function highlightMatch(text, query) {
            var escaped = escapeHtml(text);
            var regex = new RegExp('(' + query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + ')', 'gi');
            return escaped.replace(regex, '<mark>$1</mark>');
        }
    }

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
        assert!(html_str.contains("Toggle theme"));
        assert!(html_str.contains("icon-sun"));
        assert!(html_str.contains("icon-moon"));
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
        assert!(css.contains("grid-template-columns: 240px 1fr"));
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
