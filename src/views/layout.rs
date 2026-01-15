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
    const container = document.getElementById('spans-container');
    if (!container) return;

    const sessionId = container.dataset.sessionId;
    if (!sessionId) return;

    const eventSource = new EventSource('/api/sessions/' + sessionId + '/stream');

    eventSource.onmessage = function(event) {
        const data = JSON.parse(event.data);
        if (data.html) {
            const temp = document.createElement('div');
            temp.innerHTML = data.html;
            const newSpan = temp.firstChild;
            if (newSpan) {
                const existingSpan = document.querySelector('[data-span-id="' + data.id + '"]');
                if (existingSpan) {
                    existingSpan.replaceWith(newSpan);
                } else {
                    container.appendChild(newSpan);
                }
            }
        }
    };

    eventSource.onerror = function(event) {
        console.error('SSE connection error:', event);
        eventSource.close();
    };

    window.addEventListener('beforeunload', function() {
        eventSource.close();
    });
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
