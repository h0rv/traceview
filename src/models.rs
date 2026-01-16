//! Core types for traceview, mapping to OTEL GenAI semantic conventions.
//!
//! This module defines the primary data structures used to represent spans,
//! sessions, and events from OpenTelemetry GenAI instrumentation.
//!
//! References:
//! - OTEL GenAI Semantic Conventions: https://opentelemetry.io/docs/specs/semconv/gen-ai/

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The kind of span, mapping to OTEL GenAI semantic conventions.
///
/// Each variant corresponds to specific OTEL GenAI events or message types:
/// - `User`: Maps to `gen_ai.user.message` event
/// - `Assistant`: Maps to `gen_ai.assistant.message` event
/// - `System`: Maps to `gen_ai.system.message` event
/// - `Thinking`: Maps to extended_thinking content block (Anthropic extension)
/// - `ToolCall`: Maps to `gen_ai.tool.message` with tool_calls attribute
/// - `ToolResult`: Maps to `gen_ai.tool.message` response
/// - `Choice`: Maps to `gen_ai.choice` event
/// - `Span`: Generic span (default)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    /// Maps to `gen_ai.user.message` event - user input to the model
    User,
    /// Maps to `gen_ai.assistant.message` event - model response
    Assistant,
    /// Maps to `gen_ai.system.message` event - system prompt
    System,
    /// Maps to extended_thinking content block - Anthropic extension for reasoning
    Thinking,
    /// Maps to `gen_ai.tool.message` with tool_calls - tool invocation request
    ToolCall,
    /// Maps to `gen_ai.tool.message` response - tool execution result
    ToolResult,
    /// Maps to `gen_ai.choice` event - model choice/completion
    Choice,
    /// Generic span - default for unclassified spans
    #[default]
    Span,
}

/// A span representing a unit of work in a trace, mapped to OTEL GenAI conventions.
///
/// Spans form a tree structure via `parent_span_id` and are grouped by `trace_id`.
/// Sessions group related traces together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    /// Unique identifier for this span.
    /// Maps to OTEL `span_id`.
    pub id: String,

    /// The session this span belongs to.
    /// Application-specific grouping of traces.
    pub session_id: String,

    /// Parent span ID, if this span has a parent.
    /// Maps to OTEL `parent_span_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,

    /// The trace this span belongs to.
    /// Maps to OTEL `trace_id`.
    pub trace_id: String,

    /// The kind of span, indicating its role in the conversation.
    pub kind: SpanKind,

    /// The model used for this span.
    /// Maps to `gen_ai.request.model` or `gen_ai.response.model`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// The content of the span (message text, tool output, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    /// Additional metadata as JSON.
    /// Can contain any OTEL attributes not explicitly modeled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,

    /// Start time in Unix nanoseconds.
    /// Maps to OTEL span start time.
    pub start_time: i64,

    /// End time in Unix nanoseconds, if the span has completed.
    /// Maps to OTEL span end time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<i64>,

    /// Duration in milliseconds, computed from start and end times.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,

    /// Number of input tokens consumed.
    /// Maps to `gen_ai.usage.input_tokens`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<i64>,

    /// Number of output tokens generated.
    /// Maps to `gen_ai.usage.output_tokens`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<i64>,

    /// The reason the model stopped generating.
    /// Maps to `gen_ai.response.finish_reasons`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,

    /// Unique identifier for a tool call.
    /// Maps to `gen_ai.tool.call.id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,

    /// Name of the tool being called.
    /// Maps to `gen_ai.tool.name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

/// A session grouping related traces together.
///
/// Sessions provide a logical grouping for traces, typically representing
/// a conversation or interaction sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique identifier for this session.
    pub id: String,

    /// Human-readable name for the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Creation time in Unix nanoseconds.
    pub created_at: i64,

    /// Last update time in Unix nanoseconds.
    pub updated_at: i64,
}

/// An event from OTLP span events, carrying attributes.
///
/// Events represent points in time within a span's lifetime,
/// often used for GenAI message events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    /// The name of the event (e.g., "gen_ai.user.message").
    pub name: String,

    /// Event timestamp in Unix nanoseconds.
    pub timestamp: i64,

    /// Event attributes as key-value pairs.
    /// Values are JSON to support various OTEL attribute types.
    pub attributes: HashMap<String, serde_json::Value>,
}

// ============================================================================
// Export Types
// ============================================================================

/// Complete session export structure for JSON download.
#[derive(Debug, Clone, Serialize)]
pub struct SessionExport {
    /// Export format version.
    pub export_version: &'static str,

    /// Timestamp when the export was generated (Unix nanoseconds).
    pub exported_at: i64,

    /// The session data.
    pub session: Session,

    /// All spans belonging to the session, ordered by start_time.
    pub spans: Vec<Span>,

    /// Summary statistics.
    pub summary: ExportSummary,
}

/// Summary statistics for an exported session.
#[derive(Debug, Clone, Serialize)]
pub struct ExportSummary {
    /// Total number of spans.
    pub span_count: usize,

    /// Total input tokens across all spans.
    pub total_input_tokens: i64,

    /// Total output tokens across all spans.
    pub total_output_tokens: i64,

    /// Total duration in milliseconds (sum of all span durations).
    pub total_duration_ms: Option<i64>,

    /// Count of spans by kind.
    pub span_kinds: HashMap<String, usize>,
}

// ============================================================================
// Search Types
// ============================================================================

/// Search result containing matching sessions and spans.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    /// Matching sessions.
    pub sessions: Vec<SessionMatch>,

    /// Matching spans.
    pub spans: Vec<SpanMatch>,
}

/// A session match from search.
#[derive(Debug, Clone, Serialize)]
pub struct SessionMatch {
    /// The session data.
    pub session: Session,

    /// Snippet showing the match context.
    pub snippet: String,
}

/// A span match from search.
#[derive(Debug, Clone, Serialize)]
pub struct SpanMatch {
    /// The span data.
    pub span: Span,

    /// Snippet showing the match context.
    pub snippet: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    mod span_kind_tests {
        use super::*;

        #[test]
        fn test_default_is_span() {
            let kind = SpanKind::default();
            assert_eq!(kind, SpanKind::Span);
        }

        #[test]
        fn test_serialize_all_variants() {
            let test_cases = [
                (SpanKind::User, "\"user\""),
                (SpanKind::Assistant, "\"assistant\""),
                (SpanKind::System, "\"system\""),
                (SpanKind::Thinking, "\"thinking\""),
                (SpanKind::ToolCall, "\"tool_call\""),
                (SpanKind::ToolResult, "\"tool_result\""),
                (SpanKind::Choice, "\"choice\""),
                (SpanKind::Span, "\"span\""),
            ];

            for (kind, expected_json) in test_cases {
                let serialized = serde_json::to_string(&kind);
                assert!(serialized.is_ok(), "Failed to serialize {kind:?}");
                assert_eq!(serialized.unwrap_or_default(), expected_json);
            }
        }

        #[test]
        fn test_deserialize_all_variants() {
            let test_cases = [
                ("\"user\"", SpanKind::User),
                ("\"assistant\"", SpanKind::Assistant),
                ("\"system\"", SpanKind::System),
                ("\"thinking\"", SpanKind::Thinking),
                ("\"tool_call\"", SpanKind::ToolCall),
                ("\"tool_result\"", SpanKind::ToolResult),
                ("\"choice\"", SpanKind::Choice),
                ("\"span\"", SpanKind::Span),
            ];

            for (json, expected_kind) in test_cases {
                let deserialized: Result<SpanKind, _> = serde_json::from_str(json);
                assert!(deserialized.is_ok(), "Failed to deserialize {json}");
                assert_eq!(deserialized.unwrap_or_default(), expected_kind);
            }
        }

        #[test]
        fn test_roundtrip_serialization() {
            let kinds = [
                SpanKind::User,
                SpanKind::Assistant,
                SpanKind::System,
                SpanKind::Thinking,
                SpanKind::ToolCall,
                SpanKind::ToolResult,
                SpanKind::Choice,
                SpanKind::Span,
            ];

            for kind in kinds {
                let serialized = serde_json::to_string(&kind);
                assert!(serialized.is_ok());
                let deserialized: Result<SpanKind, _> =
                    serde_json::from_str(&serialized.unwrap_or_default());
                assert!(deserialized.is_ok());
                assert_eq!(deserialized.unwrap_or_default(), kind);
            }
        }

        #[test]
        fn test_copy_trait() {
            let kind = SpanKind::User;
            let copied = kind; // Copy, not move
            assert_eq!(kind, copied);
        }

        #[test]
        fn test_invalid_deserialization() {
            let invalid_json = "\"invalid_kind\"";
            let result: Result<SpanKind, _> = serde_json::from_str(invalid_json);
            assert!(result.is_err());
        }
    }

    mod span_tests {
        use super::*;

        fn create_test_span() -> Span {
            Span {
                id: "span-123".to_string(),
                session_id: "session-456".to_string(),
                parent_span_id: Some("span-parent".to_string()),
                trace_id: "trace-789".to_string(),
                kind: SpanKind::User,
                model: Some("claude-3-opus".to_string()),
                content: Some("Hello, world!".to_string()),
                metadata: Some(serde_json::json!({"custom": "value"})),
                start_time: 1_700_000_000_000_000_000,
                end_time: Some(1_700_000_001_000_000_000),
                duration_ms: Some(1000),
                input_tokens: Some(100),
                output_tokens: Some(50),
                finish_reason: Some("end_turn".to_string()),
                tool_call_id: None,
                tool_name: None,
            }
        }

        #[test]
        fn test_span_creation() {
            let span = create_test_span();
            assert_eq!(span.id, "span-123");
            assert_eq!(span.session_id, "session-456");
            assert_eq!(span.kind, SpanKind::User);
        }

        #[test]
        fn test_span_field_access() {
            let span = create_test_span();

            assert_eq!(span.parent_span_id, Some("span-parent".to_string()));
            assert_eq!(span.trace_id, "trace-789");
            assert_eq!(span.model, Some("claude-3-opus".to_string()));
            assert_eq!(span.content, Some("Hello, world!".to_string()));
            assert_eq!(span.start_time, 1_700_000_000_000_000_000);
            assert_eq!(span.end_time, Some(1_700_000_001_000_000_000));
            assert_eq!(span.duration_ms, Some(1000));
            assert_eq!(span.input_tokens, Some(100));
            assert_eq!(span.output_tokens, Some(50));
            assert_eq!(span.finish_reason, Some("end_turn".to_string()));
            assert!(span.tool_call_id.is_none());
            assert!(span.tool_name.is_none());
        }

        #[test]
        fn test_span_with_tool_fields() {
            let span = Span {
                id: "span-tool".to_string(),
                session_id: "session-1".to_string(),
                parent_span_id: None,
                trace_id: "trace-1".to_string(),
                kind: SpanKind::ToolCall,
                model: None,
                content: Some(r#"{"query": "search term"}"#.to_string()),
                metadata: None,
                start_time: 1_700_000_000_000_000_000,
                end_time: None,
                duration_ms: None,
                input_tokens: None,
                output_tokens: None,
                finish_reason: None,
                tool_call_id: Some("call-abc123".to_string()),
                tool_name: Some("web_search".to_string()),
            };

            assert_eq!(span.kind, SpanKind::ToolCall);
            assert_eq!(span.tool_call_id, Some("call-abc123".to_string()));
            assert_eq!(span.tool_name, Some("web_search".to_string()));
        }

        #[test]
        fn test_span_serialization() {
            let span = create_test_span();
            let serialized = serde_json::to_string(&span);
            assert!(serialized.is_ok());

            let json = serialized.unwrap_or_default();
            assert!(json.contains("\"id\":\"span-123\""));
            assert!(json.contains("\"kind\":\"user\""));
            assert!(json.contains("\"model\":\"claude-3-opus\""));
        }

        #[test]
        fn test_span_deserialization() {
            let json = r#"{
                "id": "span-1",
                "session_id": "session-1",
                "trace_id": "trace-1",
                "kind": "assistant",
                "start_time": 1700000000000000000
            }"#;

            let span: Result<Span, _> = serde_json::from_str(json);
            assert!(span.is_ok());

            let span = span.unwrap_or_else(|_| create_test_span());
            assert_eq!(span.id, "span-1");
            assert_eq!(span.kind, SpanKind::Assistant);
            assert!(span.parent_span_id.is_none());
            assert!(span.model.is_none());
        }

        #[test]
        fn test_span_optional_fields_skip_serialization() {
            let span = Span {
                id: "minimal".to_string(),
                session_id: "session".to_string(),
                parent_span_id: None,
                trace_id: "trace".to_string(),
                kind: SpanKind::Span,
                model: None,
                content: None,
                metadata: None,
                start_time: 0,
                end_time: None,
                duration_ms: None,
                input_tokens: None,
                output_tokens: None,
                finish_reason: None,
                tool_call_id: None,
                tool_name: None,
            };

            let json = serde_json::to_string(&span).unwrap_or_default();

            // These optional fields should not appear in output
            assert!(!json.contains("parent_span_id"));
            assert!(!json.contains("model"));
            assert!(!json.contains("content"));
            assert!(!json.contains("metadata"));
            assert!(!json.contains("end_time"));
            assert!(!json.contains("duration_ms"));
            assert!(!json.contains("input_tokens"));
            assert!(!json.contains("output_tokens"));
            assert!(!json.contains("finish_reason"));
            assert!(!json.contains("tool_call_id"));
            assert!(!json.contains("tool_name"));
        }

        #[test]
        fn test_span_roundtrip() {
            let original = create_test_span();
            let serialized = serde_json::to_string(&original);
            assert!(serialized.is_ok());

            let deserialized: Result<Span, _> =
                serde_json::from_str(&serialized.unwrap_or_default());
            assert!(deserialized.is_ok());

            let restored = deserialized.unwrap_or_else(|_| create_test_span());
            assert_eq!(original.id, restored.id);
            assert_eq!(original.session_id, restored.session_id);
            assert_eq!(original.kind, restored.kind);
            assert_eq!(original.start_time, restored.start_time);
        }
    }

    mod session_tests {
        use super::*;

        #[test]
        fn test_session_creation() {
            let session = Session {
                id: "session-abc".to_string(),
                name: Some("Test Session".to_string()),
                created_at: 1_700_000_000_000_000_000,
                updated_at: 1_700_000_001_000_000_000,
            };

            assert_eq!(session.id, "session-abc");
            assert_eq!(session.name, Some("Test Session".to_string()));
        }

        #[test]
        fn test_session_timestamps() {
            let created = 1_700_000_000_000_000_000_i64;
            let updated = 1_700_000_100_000_000_000_i64;

            let session = Session {
                id: "session-1".to_string(),
                name: None,
                created_at: created,
                updated_at: updated,
            };

            assert_eq!(session.created_at, created);
            assert_eq!(session.updated_at, updated);
            assert!(session.updated_at > session.created_at);
        }

        #[test]
        fn test_session_serialization() {
            let session = Session {
                id: "session-1".to_string(),
                name: Some("My Session".to_string()),
                created_at: 1_700_000_000_000_000_000,
                updated_at: 1_700_000_001_000_000_000,
            };

            let json = serde_json::to_string(&session);
            assert!(json.is_ok());

            let json_str = json.unwrap_or_default();
            assert!(json_str.contains("\"id\":\"session-1\""));
            assert!(json_str.contains("\"name\":\"My Session\""));
        }

        #[test]
        fn test_session_without_name_skips_field() {
            let session =
                Session { id: "session-1".to_string(), name: None, created_at: 0, updated_at: 0 };

            let json = serde_json::to_string(&session).unwrap_or_default();
            assert!(!json.contains("name"));
        }

        #[test]
        fn test_session_deserialization() {
            let json = r#"{
                "id": "session-deserialize",
                "name": "Deserialized Session",
                "created_at": 1700000000000000000,
                "updated_at": 1700000001000000000
            }"#;

            let session: Result<Session, _> = serde_json::from_str(json);
            assert!(session.is_ok());

            let session = session.unwrap_or_else(|_| Session {
                id: String::new(),
                name: None,
                created_at: 0,
                updated_at: 0,
            });

            assert_eq!(session.id, "session-deserialize");
            assert_eq!(session.name, Some("Deserialized Session".to_string()));
        }

        #[test]
        fn test_session_roundtrip() {
            let original = Session {
                id: "roundtrip-session".to_string(),
                name: Some("Roundtrip Test".to_string()),
                created_at: 1_700_000_000_000_000_000,
                updated_at: 1_700_000_500_000_000_000,
            };

            let serialized = serde_json::to_string(&original);
            assert!(serialized.is_ok());

            let deserialized: Result<Session, _> =
                serde_json::from_str(&serialized.unwrap_or_default());
            assert!(deserialized.is_ok());

            let restored = deserialized.unwrap_or_else(|_| Session {
                id: String::new(),
                name: None,
                created_at: 0,
                updated_at: 0,
            });

            assert_eq!(original.id, restored.id);
            assert_eq!(original.name, restored.name);
            assert_eq!(original.created_at, restored.created_at);
            assert_eq!(original.updated_at, restored.updated_at);
        }
    }

    mod span_event_tests {
        use super::*;

        #[test]
        fn test_span_event_creation() {
            let mut attributes = HashMap::new();
            attributes.insert("gen_ai.content".to_string(), serde_json::json!("Hello"));
            attributes.insert("gen_ai.role".to_string(), serde_json::json!("user"));

            let event = SpanEvent {
                name: "gen_ai.user.message".to_string(),
                timestamp: 1_700_000_000_000_000_000,
                attributes,
            };

            assert_eq!(event.name, "gen_ai.user.message");
            assert_eq!(event.timestamp, 1_700_000_000_000_000_000);
            assert_eq!(event.attributes.len(), 2);
        }

        #[test]
        fn test_span_event_attribute_access() {
            let mut attributes = HashMap::new();
            attributes.insert("gen_ai.tool.name".to_string(), serde_json::json!("calculator"));
            attributes.insert("gen_ai.tool.call.id".to_string(), serde_json::json!("call-123"));

            let event =
                SpanEvent { name: "gen_ai.tool.message".to_string(), timestamp: 0, attributes };

            let tool_name = event.attributes.get("gen_ai.tool.name");
            assert!(tool_name.is_some());
            assert_eq!(tool_name.unwrap_or(&serde_json::json!(null)), "calculator");
        }

        #[test]
        fn test_span_event_serialization() {
            let mut attributes = HashMap::new();
            attributes.insert("key".to_string(), serde_json::json!("value"));

            let event = SpanEvent {
                name: "test.event".to_string(),
                timestamp: 1_700_000_000_000_000_000,
                attributes,
            };

            let json = serde_json::to_string(&event);
            assert!(json.is_ok());

            let json_str = json.unwrap_or_default();
            assert!(json_str.contains("\"name\":\"test.event\""));
            assert!(json_str.contains("\"timestamp\":1700000000000000000"));
        }

        #[test]
        fn test_span_event_empty_attributes() {
            let event = SpanEvent {
                name: "empty.event".to_string(),
                timestamp: 0,
                attributes: HashMap::new(),
            };

            assert!(event.attributes.is_empty());

            let json = serde_json::to_string(&event);
            assert!(json.is_ok());
        }

        #[test]
        fn test_span_event_complex_attributes() {
            let mut attributes = HashMap::new();
            attributes.insert("nested".to_string(), serde_json::json!({"inner": {"deep": true}}));
            attributes.insert("array".to_string(), serde_json::json!([1, 2, 3]));
            attributes.insert("number".to_string(), serde_json::json!(42));
            attributes.insert("boolean".to_string(), serde_json::json!(true));

            let event = SpanEvent { name: "complex.event".to_string(), timestamp: 0, attributes };

            let json = serde_json::to_string(&event);
            assert!(json.is_ok());

            let deserialized: Result<SpanEvent, _> =
                serde_json::from_str(&json.unwrap_or_default());
            assert!(deserialized.is_ok());
        }
    }
}
