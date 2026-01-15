//! OTLP ingestion with OTEL GenAI semantic convention compliance.
//!
//! This module handles parsing OTLP/HTTP JSON format and converting it to our
//! internal Span model, following the OpenTelemetry GenAI semantic conventions.
//!
//! References:
//! - OTEL GenAI Semantic Conventions: https://opentelemetry.io/docs/specs/semconv/gen-ai/
//! - OTLP JSON Format: https://opentelemetry.io/docs/specs/otlp/

use crate::error::{Result, TraceviewError};
use crate::models::{Span, SpanKind};
use serde::Deserialize;
use std::collections::HashMap;

// ============================================================================
// OTLP JSON Structures
// ============================================================================

/// Root structure for OTLP trace data in JSON format.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtlpTraceData {
    pub resource_spans: Vec<ResourceSpans>,
}

/// A collection of spans from a single resource.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSpans {
    pub resource: Option<Resource>,
    pub scope_spans: Vec<ScopeSpans>,
}

/// Resource attributes describing the entity producing telemetry.
#[derive(Debug, Deserialize)]
pub struct Resource {
    pub attributes: Vec<KeyValue>,
}

/// A collection of spans from a single instrumentation scope.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeSpans {
    pub scope: Option<Scope>,
    pub spans: Vec<OtlpSpan>,
}

/// Instrumentation scope information.
#[derive(Debug, Deserialize)]
pub struct Scope {
    pub name: Option<String>,
}

/// An OTLP span in JSON format.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtlpSpan {
    /// Trace ID as base64 or hex string.
    pub trace_id: String,
    /// Span ID as base64 or hex string.
    pub span_id: String,
    /// Parent span ID, if any.
    pub parent_span_id: Option<String>,
    /// The operation name.
    pub name: String,
    /// Start time in Unix nanoseconds (as string because JSON doesn't support u64).
    pub start_time_unix_nano: String,
    /// End time in Unix nanoseconds.
    pub end_time_unix_nano: Option<String>,
    /// Span attributes.
    #[serde(default)]
    pub attributes: Vec<KeyValue>,
    /// Span events.
    #[serde(default)]
    pub events: Vec<OtlpEvent>,
}

/// An OTLP event in JSON format.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtlpEvent {
    /// Event name (e.g., "gen_ai.user.message").
    pub name: String,
    /// Event time in Unix nanoseconds.
    pub time_unix_nano: String,
    /// Event attributes.
    #[serde(default)]
    pub attributes: Vec<KeyValue>,
}

/// A key-value pair for OTLP attributes.
#[derive(Debug, Deserialize)]
pub struct KeyValue {
    pub key: String,
    pub value: AttributeValue,
}

/// An OTLP attribute value, supporting various types.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeValue {
    pub string_value: Option<String>,
    /// OTLP sends integers as strings in JSON.
    pub int_value: Option<String>,
    pub bool_value: Option<bool>,
    pub array_value: Option<ArrayValue>,
    pub double_value: Option<f64>,
}

/// An array of attribute values.
#[derive(Debug, Deserialize)]
pub struct ArrayValue {
    pub values: Vec<AttributeValue>,
}

// ============================================================================
// Classification Functions
// ============================================================================

/// Classify an OTLP event into a SpanKind based on OTEL GenAI semantic conventions.
///
/// Event name mapping:
/// - `gen_ai.user.message` -> User
/// - `gen_ai.assistant.message` -> Assistant
/// - `gen_ai.system.message` -> System
/// - `gen_ai.tool.message` -> ToolCall (if has tool_calls) or ToolResult
/// - `gen_ai.choice` -> Choice
/// - `gen_ai.thinking` or `thinking` -> Thinking
pub fn classify_event(event_name: &str, attrs: &HashMap<String, serde_json::Value>) -> SpanKind {
    match event_name {
        "gen_ai.user.message" => SpanKind::User,
        "gen_ai.assistant.message" => SpanKind::Assistant,
        "gen_ai.system.message" => SpanKind::System,
        "gen_ai.choice" => SpanKind::Choice,
        "gen_ai.thinking" | "thinking" => SpanKind::Thinking,
        "gen_ai.tool.message" => classify_tool_message(attrs),
        _ => SpanKind::Span,
    }
}

/// Helper to classify tool messages as either ToolCall or ToolResult.
fn classify_tool_message(attrs: &HashMap<String, serde_json::Value>) -> SpanKind {
    // Check if this is a tool call (has tool_calls attribute) or tool result
    let has_tool_call_id = attrs.contains_key("gen_ai.tool.call.id")
        || attrs.contains_key("tool_calls")
        || attrs.contains_key("gen_ai.tool.calls");

    if !has_tool_call_id {
        // No tool_calls attribute means this is a tool result
        return SpanKind::ToolResult;
    }

    // If it has a tool name but also has content that looks like a result,
    // it's likely a tool result
    let has_result_content = attrs.contains_key("gen_ai.tool.result")
        || attrs.contains_key("tool_result")
        || attrs.contains_key("content");

    if !has_result_content {
        return SpanKind::ToolCall;
    }

    // Check if this is explicitly a tool call with arguments
    let has_arguments =
        attrs.contains_key("gen_ai.tool.call.arguments") || attrs.contains_key("arguments");

    if has_arguments { SpanKind::ToolCall } else { SpanKind::ToolResult }
}

/// Classify an OTLP span into a SpanKind based on OTEL GenAI semantic conventions.
///
/// Checks `gen_ai.operation.name` attribute:
/// - `chat` -> Assistant
/// - `execute_tool` -> ToolCall
/// - `invoke_agent` -> Span
///
/// Also checks span name patterns for additional classification.
pub fn classify_span(span_name: &str, attrs: &HashMap<String, serde_json::Value>) -> SpanKind {
    // First check gen_ai.operation.name attribute
    if let Some(op_name) = attrs.get("gen_ai.operation.name").and_then(|v| v.as_str()) {
        match op_name {
            "chat" => return SpanKind::Assistant,
            "execute_tool" => return SpanKind::ToolCall,
            "invoke_agent" | "agent" => return SpanKind::Span,
            _ => {}
        }
    }

    // Check span name patterns
    classify_by_span_name(span_name)
}

/// Helper to classify span by name patterns.
fn classify_by_span_name(span_name: &str) -> SpanKind {
    let name_lower = span_name.to_lowercase();

    if name_lower.contains("chat") || name_lower.contains("completion") {
        return SpanKind::Assistant;
    }

    if name_lower.contains("tool") {
        return if name_lower.contains("result") || name_lower.contains("response") {
            SpanKind::ToolResult
        } else {
            SpanKind::ToolCall
        };
    }

    if name_lower.contains("thinking") || name_lower.contains("reasoning") {
        return SpanKind::Thinking;
    }

    SpanKind::Span
}

// ============================================================================
// Session Extraction
// ============================================================================

/// Extract a session ID from span attributes.
///
/// Looks for these attributes in order:
/// 1. `session.id` - Explicit session identifier
/// 2. `gen_ai.conversation.id` - OTEL GenAI conversation ID
/// 3. `service.instance.id` - Auto-generated per-process ID (groups all spans from one process)
/// 4. Falls back to `trace_id`
///
/// Using `service.instance.id` as a fallback means that all spans from a single
/// process invocation (e.g., one run of an agent script) are automatically grouped
/// together without requiring any user configuration.
pub fn extract_session_id(attrs: &HashMap<String, serde_json::Value>, trace_id: &str) -> String {
    // Try session.id first (explicit user-defined session)
    if let Some(s) = attrs.get("session.id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        return s.to_string();
    }

    // Try gen_ai.conversation.id (OTEL GenAI semantic convention)
    if let Some(s) =
        attrs.get("gen_ai.conversation.id").and_then(|v| v.as_str()).filter(|s| !s.is_empty())
    {
        return s.to_string();
    }

    // Try service.instance.id (auto-generated by OTEL SDK, unique per process)
    // This groups all spans from one process invocation together automatically
    if let Some(s) =
        attrs.get("service.instance.id").and_then(|v| v.as_str()).filter(|s| !s.is_empty())
    {
        return s.to_string();
    }

    // Fall back to trace_id
    trace_id.to_string()
}

// ============================================================================
// Attribute Conversion
// ============================================================================

/// Convert an AttributeValue to a serde_json::Value.
fn attribute_value_to_json(attr_value: &AttributeValue) -> serde_json::Value {
    if let Some(ref s) = attr_value.string_value {
        return serde_json::Value::String(s.clone());
    }
    if let Some(ref i) = attr_value.int_value {
        // OTLP sends integers as strings
        if let Ok(n) = i.parse::<i64>() {
            return serde_json::Value::Number(n.into());
        }
        return serde_json::Value::String(i.clone());
    }
    if let Some(b) = attr_value.bool_value {
        return serde_json::Value::Bool(b);
    }
    if let Some(d) = attr_value.double_value
        && let Some(n) = serde_json::Number::from_f64(d)
    {
        return serde_json::Value::Number(n);
    }
    if let Some(ref arr) = attr_value.array_value {
        let values: Vec<serde_json::Value> =
            arr.values.iter().map(attribute_value_to_json).collect();
        return serde_json::Value::Array(values);
    }
    serde_json::Value::Null
}

/// Convert a vector of KeyValue pairs to a HashMap.
pub fn attrs_to_map(attrs: &[KeyValue]) -> HashMap<String, serde_json::Value> {
    let mut map = HashMap::new();
    for kv in attrs {
        map.insert(kv.key.clone(), attribute_value_to_json(&kv.value));
    }
    map
}

// ============================================================================
// ID Decoding
// ============================================================================

/// Decode a trace or span ID from either base64 or hex format.
fn decode_id(id: &str) -> Result<String> {
    if id.is_empty() {
        return Err(TraceviewError::InvalidOtlp { reason: "empty ID".to_string() });
    }

    // Check if it's already a valid hex string (32 chars for trace_id, 16 for span_id)
    if id.chars().all(|c| c.is_ascii_hexdigit()) && (id.len() == 32 || id.len() == 16) {
        return Ok(id.to_lowercase());
    }

    // Try to decode as base64
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    match STANDARD.decode(id) {
        Ok(bytes) => {
            // Convert bytes to hex string
            Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
        }
        Err(_) => {
            // If base64 decode fails, return the original string
            // It might be a custom format
            Ok(id.to_string())
        }
    }
}

// ============================================================================
// Main Conversion
// ============================================================================

/// Convert OTLP trace data to our internal Span model.
///
/// This function:
/// 1. Parses all spans from the OTLP data
/// 2. Converts events within spans to child spans
/// 3. Extracts OTEL GenAI attributes (model, tokens, finish_reason, etc.)
/// 4. Classifies spans based on semantic conventions
pub fn convert_otlp(data: &OtlpTraceData) -> Result<Vec<Span>> {
    let mut spans = Vec::new();

    for resource_spans in &data.resource_spans {
        let resource_attrs = resource_spans
            .resource
            .as_ref()
            .map(|r| attrs_to_map(&r.attributes))
            .unwrap_or_default();

        for scope_spans in &resource_spans.scope_spans {
            for otlp_span in &scope_spans.spans {
                convert_single_span(otlp_span, &resource_attrs, &mut spans)?;
            }
        }
    }

    Ok(spans)
}

// ============================================================================
// Protobuf Conversion
// ============================================================================

use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::AnyValue;
use opentelemetry_proto::tonic::common::v1::any_value::Value as ProtoValue;
use prost::Message;

/// Convert OTLP protobuf trace data to our internal Span model.
///
/// This handles the binary protobuf format that most OTEL SDKs use by default.
pub fn convert_otlp_proto(data: &[u8]) -> Result<Vec<Span>> {
    let request = ExportTraceServiceRequest::decode(data)?;
    let mut spans = Vec::new();

    for resource_spans in request.resource_spans {
        let resource_attrs =
            resource_spans.resource.map(|r| proto_attrs_to_map(&r.attributes)).unwrap_or_default();

        for scope_spans in resource_spans.scope_spans {
            for proto_span in scope_spans.spans {
                convert_proto_span(&proto_span, &resource_attrs, &mut spans)?;
            }
        }
    }

    Ok(spans)
}

/// Convert protobuf attributes to a HashMap.
fn proto_attrs_to_map(
    attrs: &[opentelemetry_proto::tonic::common::v1::KeyValue],
) -> HashMap<String, serde_json::Value> {
    let mut map = HashMap::new();
    for kv in attrs {
        if let Some(ref value) = kv.value {
            map.insert(kv.key.clone(), proto_value_to_json(value));
        }
    }
    map
}

/// Convert a protobuf AnyValue to JSON.
fn proto_value_to_json(value: &AnyValue) -> serde_json::Value {
    match &value.value {
        Some(ProtoValue::StringValue(s)) => serde_json::Value::String(s.clone()),
        Some(ProtoValue::IntValue(i)) => serde_json::Value::Number((*i).into()),
        Some(ProtoValue::BoolValue(b)) => serde_json::Value::Bool(*b),
        Some(ProtoValue::DoubleValue(d)) => serde_json::Number::from_f64(*d)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Some(ProtoValue::ArrayValue(arr)) => {
            let values: Vec<serde_json::Value> =
                arr.values.iter().map(proto_value_to_json).collect();
            serde_json::Value::Array(values)
        }
        Some(ProtoValue::KvlistValue(kvlist)) => {
            let obj: serde_json::Map<String, serde_json::Value> = kvlist
                .values
                .iter()
                .filter_map(|kv| {
                    kv.value.as_ref().map(|v| (kv.key.clone(), proto_value_to_json(v)))
                })
                .collect();
            serde_json::Value::Object(obj)
        }
        Some(ProtoValue::BytesValue(b)) => {
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            serde_json::Value::String(STANDARD.encode(b))
        }
        None => serde_json::Value::Null,
    }
}

/// Convert a protobuf span to our Span model.
fn convert_proto_span(
    proto_span: &opentelemetry_proto::tonic::trace::v1::Span,
    resource_attrs: &HashMap<String, serde_json::Value>,
    spans: &mut Vec<Span>,
) -> Result<()> {
    // Convert IDs from bytes to hex
    let trace_id = bytes_to_hex(&proto_span.trace_id);
    let span_id = bytes_to_hex(&proto_span.span_id);
    let parent_span_id = if proto_span.parent_span_id.is_empty() {
        None
    } else {
        Some(bytes_to_hex(&proto_span.parent_span_id))
    };

    // Convert span attributes
    let span_attrs = proto_attrs_to_map(&proto_span.attributes);

    // Merge resource and span attributes
    let mut merged_attrs = resource_attrs.clone();
    merged_attrs.extend(span_attrs);

    // Extract session ID
    let session_id = extract_session_id(&merged_attrs, &trace_id);

    // Parse timestamps (protobuf uses u64 nanoseconds directly)
    let start_time = i64::try_from(proto_span.start_time_unix_nano)
        .map_err(|_| TraceviewError::InvalidOtlp { reason: "timestamp overflow".to_string() })?;
    let end_time = if proto_span.end_time_unix_nano > 0 {
        Some(i64::try_from(proto_span.end_time_unix_nano).map_err(|_| {
            TraceviewError::InvalidOtlp { reason: "timestamp overflow".to_string() }
        })?)
    } else {
        None
    };

    // Calculate duration
    let duration_ms = end_time.map(|e| (e - start_time) / 1_000_000);

    // Classify the span
    let kind = classify_span(&proto_span.name, &merged_attrs);

    // Extract GenAI attributes
    let model = extract_string_attr(
        &merged_attrs,
        &["gen_ai.response.model", "gen_ai.request.model", "model"],
    );
    let input_tokens =
        extract_i64_attr(&merged_attrs, &["gen_ai.usage.input_tokens", "input_tokens"]);
    let output_tokens =
        extract_i64_attr(&merged_attrs, &["gen_ai.usage.output_tokens", "output_tokens"]);
    let finish_reason = extract_finish_reason(&merged_attrs);
    let content = extract_content(&merged_attrs);
    let tool_call_id = extract_string_attr(&merged_attrs, &["gen_ai.tool.call.id", "tool_call_id"]);
    let tool_name = extract_string_attr(&merged_attrs, &["gen_ai.tool.name", "tool_name"]);

    // Build metadata
    let metadata = build_metadata(&merged_attrs);

    // Create the main span
    let span = Span {
        id: span_id.clone(),
        session_id: session_id.clone(),
        parent_span_id,
        trace_id: trace_id.clone(),
        kind,
        model,
        content,
        metadata,
        start_time,
        end_time,
        duration_ms,
        input_tokens,
        output_tokens,
        finish_reason,
        tool_call_id,
        tool_name,
    };
    spans.push(span);

    // Convert events to child spans
    for (event_idx, event) in proto_span.events.iter().enumerate() {
        let event_span =
            convert_proto_event_to_span(event, event_idx, &trace_id, &span_id, &session_id)?;
        spans.push(event_span);
    }

    Ok(())
}

/// Convert a protobuf event to a Span.
fn convert_proto_event_to_span(
    event: &opentelemetry_proto::tonic::trace::v1::span::Event,
    event_idx: usize,
    trace_id: &str,
    parent_span_id: &str,
    session_id: &str,
) -> Result<Span> {
    let event_attrs = proto_attrs_to_map(&event.attributes);

    // Generate a unique ID for the event span
    let event_span_id = format!("{parent_span_id}-event-{event_idx}");

    // Parse timestamp
    let timestamp = i64::try_from(event.time_unix_nano)
        .map_err(|_| TraceviewError::InvalidOtlp { reason: "timestamp overflow".to_string() })?;

    // Classify the event
    let kind = classify_event(&event.name, &event_attrs);

    // Extract GenAI attributes
    let model = extract_string_attr(
        &event_attrs,
        &["gen_ai.response.model", "gen_ai.request.model", "model"],
    );
    let input_tokens =
        extract_i64_attr(&event_attrs, &["gen_ai.usage.input_tokens", "input_tokens"]);
    let output_tokens =
        extract_i64_attr(&event_attrs, &["gen_ai.usage.output_tokens", "output_tokens"]);
    let finish_reason = extract_finish_reason(&event_attrs);
    let content = extract_content(&event_attrs);
    let tool_call_id = extract_string_attr(&event_attrs, &["gen_ai.tool.call.id", "tool_call_id"]);
    let tool_name = extract_string_attr(&event_attrs, &["gen_ai.tool.name", "tool_name"]);

    // Build metadata
    let metadata = build_metadata(&event_attrs);

    Ok(Span {
        id: event_span_id,
        session_id: session_id.to_string(),
        parent_span_id: Some(parent_span_id.to_string()),
        trace_id: trace_id.to_string(),
        kind,
        model,
        content,
        metadata,
        start_time: timestamp,
        end_time: Some(timestamp),
        duration_ms: Some(0),
        input_tokens,
        output_tokens,
        finish_reason,
        tool_call_id,
        tool_name,
    })
}

/// Convert bytes to hex string.
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Convert a single OTLP span and its events to our Span model.
fn convert_single_span(
    otlp_span: &OtlpSpan,
    resource_attrs: &HashMap<String, serde_json::Value>,
    spans: &mut Vec<Span>,
) -> Result<()> {
    // Decode IDs
    let trace_id = decode_id(&otlp_span.trace_id)?;
    let span_id = decode_id(&otlp_span.span_id)?;
    let parent_span_id = match &otlp_span.parent_span_id {
        Some(p) if !p.is_empty() => Some(decode_id(p)?),
        _ => None,
    };

    // Convert span attributes
    let span_attrs = attrs_to_map(&otlp_span.attributes);

    // Merge resource and span attributes for session extraction
    let mut merged_attrs = resource_attrs.clone();
    merged_attrs.extend(span_attrs);

    // Extract session ID
    let session_id = extract_session_id(&merged_attrs, &trace_id);

    // Parse timestamps
    let start_time = parse_timestamp(&otlp_span.start_time_unix_nano)?;
    let end_time = otlp_span.end_time_unix_nano.as_ref().map(|t| parse_timestamp(t)).transpose()?;

    // Calculate duration
    let duration_ms = end_time.map(|e| (e - start_time) / 1_000_000);

    // Classify the span
    let kind = classify_span(&otlp_span.name, &merged_attrs);

    // Extract GenAI attributes
    let model = extract_string_attr(
        &merged_attrs,
        &["gen_ai.response.model", "gen_ai.request.model", "model"],
    );
    let input_tokens =
        extract_i64_attr(&merged_attrs, &["gen_ai.usage.input_tokens", "input_tokens"]);
    let output_tokens =
        extract_i64_attr(&merged_attrs, &["gen_ai.usage.output_tokens", "output_tokens"]);
    let finish_reason = extract_finish_reason(&merged_attrs);
    let content = extract_content(&merged_attrs);
    let tool_call_id = extract_string_attr(&merged_attrs, &["gen_ai.tool.call.id", "tool_call_id"]);
    let tool_name = extract_string_attr(&merged_attrs, &["gen_ai.tool.name", "tool_name"]);

    // Build metadata from remaining attributes
    let metadata = build_metadata(&merged_attrs);

    // Create the main span
    let span = Span {
        id: span_id.clone(),
        session_id: session_id.clone(),
        parent_span_id,
        trace_id: trace_id.clone(),
        kind,
        model,
        content,
        metadata,
        start_time,
        end_time,
        duration_ms,
        input_tokens,
        output_tokens,
        finish_reason,
        tool_call_id,
        tool_name,
    };
    spans.push(span);

    // Convert events to child spans
    for (event_idx, event) in otlp_span.events.iter().enumerate() {
        let event_span = convert_event_to_span(event, event_idx, &trace_id, &span_id, &session_id)?;
        spans.push(event_span);
    }

    Ok(())
}

/// Convert an OTLP event to a Span.
fn convert_event_to_span(
    event: &OtlpEvent,
    event_idx: usize,
    trace_id: &str,
    parent_span_id: &str,
    session_id: &str,
) -> Result<Span> {
    let event_attrs = attrs_to_map(&event.attributes);

    // Generate a unique ID for the event span
    let event_span_id = format!("{parent_span_id}-event-{event_idx}");

    // Parse timestamp
    let timestamp = parse_timestamp(&event.time_unix_nano)?;

    // Classify the event
    let kind = classify_event(&event.name, &event_attrs);

    // Extract GenAI attributes
    let model = extract_string_attr(
        &event_attrs,
        &["gen_ai.response.model", "gen_ai.request.model", "model"],
    );
    let input_tokens =
        extract_i64_attr(&event_attrs, &["gen_ai.usage.input_tokens", "input_tokens"]);
    let output_tokens =
        extract_i64_attr(&event_attrs, &["gen_ai.usage.output_tokens", "output_tokens"]);
    let finish_reason = extract_finish_reason(&event_attrs);
    let content = extract_content(&event_attrs);
    let tool_call_id = extract_string_attr(&event_attrs, &["gen_ai.tool.call.id", "tool_call_id"]);
    let tool_name = extract_string_attr(&event_attrs, &["gen_ai.tool.name", "tool_name"]);

    // Build metadata
    let metadata = build_metadata(&event_attrs);

    Ok(Span {
        id: event_span_id,
        session_id: session_id.to_string(),
        parent_span_id: Some(parent_span_id.to_string()),
        trace_id: trace_id.to_string(),
        kind,
        model,
        content,
        metadata,
        start_time: timestamp,
        end_time: Some(timestamp), // Events are instantaneous
        duration_ms: Some(0),
        input_tokens,
        output_tokens,
        finish_reason,
        tool_call_id,
        tool_name,
    })
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse a timestamp string to i64 nanoseconds.
fn parse_timestamp(ts: &str) -> Result<i64> {
    ts.parse::<i64>()
        .map_err(|_| TraceviewError::InvalidOtlp { reason: format!("invalid timestamp: {ts}") })
}

/// Extract a string attribute from a map, trying multiple keys in order.
fn extract_string_attr(
    attrs: &HashMap<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    for key in keys {
        if let Some(s) = attrs.get(*key).and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            return Some(s.to_string());
        }
    }
    None
}

/// Extract an i64 attribute from a map, trying multiple keys in order.
fn extract_i64_attr(attrs: &HashMap<String, serde_json::Value>, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(value) = attrs.get(*key) {
            if let Some(n) = value.as_i64() {
                return Some(n);
            }
            // Try parsing from string (OTLP sends integers as strings)
            if let Some(n) = value.as_str().and_then(|s| s.parse::<i64>().ok()) {
                return Some(n);
            }
        }
    }
    None
}

/// Extract finish reason from attributes.
fn extract_finish_reason(attrs: &HashMap<String, serde_json::Value>) -> Option<String> {
    // Try gen_ai.response.finish_reasons (array) first
    if let Some(reasons) = attrs.get("gen_ai.response.finish_reasons") {
        if let Some(arr) = reasons.as_array()
            && let Some(s) = arr.first().and_then(|v| v.as_str())
        {
            return Some(s.to_string());
        }
        // Also handle single string value
        if let Some(s) = reasons.as_str() {
            return Some(s.to_string());
        }
    }

    // Try finish_reason singular
    attrs.get("finish_reason").and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// Extract content from attributes.
fn extract_content(attrs: &HashMap<String, serde_json::Value>) -> Option<String> {
    let content_keys = [
        "gen_ai.content",
        "gen_ai.prompt",
        "gen_ai.completion",
        "content",
        "message.content",
        "text",
    ];

    for key in content_keys {
        if let Some(value) = attrs.get(key) {
            if let Some(s) = value.as_str().filter(|s| !s.is_empty()) {
                return Some(s.to_string());
            }
            // Handle array of content blocks
            if let Some(arr) = value.as_array() {
                let texts: Vec<String> = arr
                    .iter()
                    .filter_map(|v| {
                        // Try direct string
                        if let Some(s) = v.as_str() {
                            return Some(s.to_string());
                        }
                        // Try object with text field
                        v.as_object()
                            .and_then(|obj| obj.get("text"))
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect();
                if !texts.is_empty() {
                    return Some(texts.join("\n"));
                }
            }
        }
    }

    None
}

/// Build metadata JSON from attributes, excluding already-extracted fields.
fn build_metadata(attrs: &HashMap<String, serde_json::Value>) -> Option<serde_json::Value> {
    // Fields that are already extracted into dedicated span fields
    let extracted_fields = [
        "gen_ai.response.model",
        "gen_ai.request.model",
        "model",
        "gen_ai.usage.input_tokens",
        "input_tokens",
        "gen_ai.usage.output_tokens",
        "output_tokens",
        "gen_ai.response.finish_reasons",
        "finish_reason",
        "gen_ai.tool.call.id",
        "tool_call_id",
        "gen_ai.tool.name",
        "tool_name",
        "gen_ai.content",
        "gen_ai.prompt",
        "gen_ai.completion",
        "content",
        "message.content",
        "text",
        "session.id",
        "gen_ai.conversation.id",
    ];

    let metadata: HashMap<String, serde_json::Value> = attrs
        .iter()
        .filter(|(k, _)| !extracted_fields.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    if metadata.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(metadata.into_iter().collect()))
    }
}

// ============================================================================
// Session Name Extraction
// ============================================================================

/// Extract a session name from the first user message in a list of spans.
///
/// Looks for the first span with `SpanKind::User` and extracts its content
/// to use as the session name. The name is trimmed and truncated to 50 characters
/// with "..." appended if longer.
///
/// Returns `None` if:
/// - No span with `SpanKind::User` is found
/// - The user span has no content
/// - The content is empty after trimming
pub fn extract_session_name(spans: &[Span]) -> Option<String> {
    spans
        .iter()
        .find(|s| s.kind == SpanKind::User)
        .and_then(|s| s.content.as_ref())
        .map(|c| {
            let trimmed = c.trim();
            if trimmed.chars().count() > 50 {
                format!("{}...", trimmed.chars().take(47).collect::<String>())
            } else {
                trimmed.to_string()
            }
        })
        .filter(|s| !s.is_empty())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    mod classify_event_tests {
        use super::*;

        #[test]
        fn test_user_message() {
            let attrs = HashMap::new();
            assert_eq!(classify_event("gen_ai.user.message", &attrs), SpanKind::User);
        }

        #[test]
        fn test_assistant_message() {
            let attrs = HashMap::new();
            assert_eq!(classify_event("gen_ai.assistant.message", &attrs), SpanKind::Assistant);
        }

        #[test]
        fn test_system_message() {
            let attrs = HashMap::new();
            assert_eq!(classify_event("gen_ai.system.message", &attrs), SpanKind::System);
        }

        #[test]
        fn test_choice() {
            let attrs = HashMap::new();
            assert_eq!(classify_event("gen_ai.choice", &attrs), SpanKind::Choice);
        }

        #[test]
        fn test_thinking_genai() {
            let attrs = HashMap::new();
            assert_eq!(classify_event("gen_ai.thinking", &attrs), SpanKind::Thinking);
        }

        #[test]
        fn test_thinking_anthropic() {
            let attrs = HashMap::new();
            assert_eq!(classify_event("thinking", &attrs), SpanKind::Thinking);
        }

        #[test]
        fn test_tool_message_with_tool_calls() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.tool.call.id".to_string(), serde_json::json!("call-123"));
            assert_eq!(classify_event("gen_ai.tool.message", &attrs), SpanKind::ToolCall);
        }

        #[test]
        fn test_tool_message_without_tool_calls() {
            let attrs = HashMap::new();
            assert_eq!(classify_event("gen_ai.tool.message", &attrs), SpanKind::ToolResult);
        }

        #[test]
        fn test_tool_message_with_result() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.tool.call.id".to_string(), serde_json::json!("call-123"));
            attrs.insert("gen_ai.tool.result".to_string(), serde_json::json!("result"));
            assert_eq!(classify_event("gen_ai.tool.message", &attrs), SpanKind::ToolResult);
        }

        #[test]
        fn test_tool_message_with_arguments() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.tool.call.id".to_string(), serde_json::json!("call-123"));
            attrs.insert("content".to_string(), serde_json::json!("some content"));
            attrs
                .insert("gen_ai.tool.call.arguments".to_string(), serde_json::json!(r#"{"x": 1}"#));
            assert_eq!(classify_event("gen_ai.tool.message", &attrs), SpanKind::ToolCall);
        }

        #[test]
        fn test_unknown_event() {
            let attrs = HashMap::new();
            assert_eq!(classify_event("unknown.event", &attrs), SpanKind::Span);
        }
    }

    mod classify_span_tests {
        use super::*;

        #[test]
        fn test_chat_operation() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.operation.name".to_string(), serde_json::json!("chat"));
            assert_eq!(classify_span("some_span", &attrs), SpanKind::Assistant);
        }

        #[test]
        fn test_execute_tool_operation() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.operation.name".to_string(), serde_json::json!("execute_tool"));
            assert_eq!(classify_span("some_span", &attrs), SpanKind::ToolCall);
        }

        #[test]
        fn test_invoke_agent_operation() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.operation.name".to_string(), serde_json::json!("invoke_agent"));
            assert_eq!(classify_span("some_span", &attrs), SpanKind::Span);
        }

        #[test]
        fn test_chat_span_name() {
            let attrs = HashMap::new();
            assert_eq!(classify_span("chat_completion", &attrs), SpanKind::Assistant);
        }

        #[test]
        fn test_tool_span_name() {
            let attrs = HashMap::new();
            assert_eq!(classify_span("tool_execution", &attrs), SpanKind::ToolCall);
        }

        #[test]
        fn test_tool_result_span_name() {
            let attrs = HashMap::new();
            assert_eq!(classify_span("tool_result", &attrs), SpanKind::ToolResult);
        }

        #[test]
        fn test_thinking_span_name() {
            let attrs = HashMap::new();
            assert_eq!(classify_span("thinking_block", &attrs), SpanKind::Thinking);
        }

        #[test]
        fn test_unknown_span() {
            let attrs = HashMap::new();
            assert_eq!(classify_span("unknown_operation", &attrs), SpanKind::Span);
        }
    }

    mod extract_session_id_tests {
        use super::*;

        #[test]
        fn test_session_id_attribute() {
            let mut attrs = HashMap::new();
            attrs.insert("session.id".to_string(), serde_json::json!("session-123"));
            assert_eq!(extract_session_id(&attrs, "trace-456"), "session-123");
        }

        #[test]
        fn test_conversation_id_attribute() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.conversation.id".to_string(), serde_json::json!("conv-789"));
            assert_eq!(extract_session_id(&attrs, "trace-456"), "conv-789");
        }

        #[test]
        fn test_session_id_priority_over_conversation() {
            let mut attrs = HashMap::new();
            attrs.insert("session.id".to_string(), serde_json::json!("session-123"));
            attrs.insert("gen_ai.conversation.id".to_string(), serde_json::json!("conv-789"));
            assert_eq!(extract_session_id(&attrs, "trace-456"), "session-123");
        }

        #[test]
        fn test_fallback_to_trace_id() {
            let attrs = HashMap::new();
            assert_eq!(extract_session_id(&attrs, "trace-456"), "trace-456");
        }

        #[test]
        fn test_empty_session_id_falls_back() {
            let mut attrs = HashMap::new();
            attrs.insert("session.id".to_string(), serde_json::json!(""));
            assert_eq!(extract_session_id(&attrs, "trace-456"), "trace-456");
        }

        #[test]
        fn test_service_instance_id_fallback() {
            // service.instance.id is auto-generated by OTEL SDK
            let mut attrs = HashMap::new();
            attrs.insert(
                "service.instance.id".to_string(),
                serde_json::json!("85bf30db5d0c4798a6906415cde9bf29"),
            );
            assert_eq!(extract_session_id(&attrs, "trace-456"), "85bf30db5d0c4798a6906415cde9bf29");
        }

        #[test]
        fn test_session_id_priority_over_service_instance() {
            let mut attrs = HashMap::new();
            attrs.insert("session.id".to_string(), serde_json::json!("explicit-session"));
            attrs.insert("service.instance.id".to_string(), serde_json::json!("auto-instance-id"));
            assert_eq!(extract_session_id(&attrs, "trace-456"), "explicit-session");
        }

        #[test]
        fn test_conversation_id_priority_over_service_instance() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.conversation.id".to_string(), serde_json::json!("conv-123"));
            attrs.insert("service.instance.id".to_string(), serde_json::json!("auto-instance-id"));
            assert_eq!(extract_session_id(&attrs, "trace-456"), "conv-123");
        }
    }

    mod attrs_to_map_tests {
        use super::*;

        #[test]
        fn test_string_value() {
            let attrs = vec![KeyValue {
                key: "test".to_string(),
                value: AttributeValue {
                    string_value: Some("hello".to_string()),
                    int_value: None,
                    bool_value: None,
                    array_value: None,
                    double_value: None,
                },
            }];
            let map = attrs_to_map(&attrs);
            assert_eq!(map.get("test"), Some(&serde_json::json!("hello")));
        }

        #[test]
        fn test_int_value() {
            let attrs = vec![KeyValue {
                key: "count".to_string(),
                value: AttributeValue {
                    string_value: None,
                    int_value: Some("42".to_string()),
                    bool_value: None,
                    array_value: None,
                    double_value: None,
                },
            }];
            let map = attrs_to_map(&attrs);
            assert_eq!(map.get("count"), Some(&serde_json::json!(42)));
        }

        #[test]
        fn test_bool_value() {
            let attrs = vec![KeyValue {
                key: "flag".to_string(),
                value: AttributeValue {
                    string_value: None,
                    int_value: None,
                    bool_value: Some(true),
                    array_value: None,
                    double_value: None,
                },
            }];
            let map = attrs_to_map(&attrs);
            assert_eq!(map.get("flag"), Some(&serde_json::json!(true)));
        }

        #[test]
        fn test_array_value() {
            let attrs = vec![KeyValue {
                key: "items".to_string(),
                value: AttributeValue {
                    string_value: None,
                    int_value: None,
                    bool_value: None,
                    array_value: Some(ArrayValue {
                        values: vec![
                            AttributeValue {
                                string_value: Some("a".to_string()),
                                int_value: None,
                                bool_value: None,
                                array_value: None,
                                double_value: None,
                            },
                            AttributeValue {
                                string_value: Some("b".to_string()),
                                int_value: None,
                                bool_value: None,
                                array_value: None,
                                double_value: None,
                            },
                        ],
                    }),
                    double_value: None,
                },
            }];
            let map = attrs_to_map(&attrs);
            assert_eq!(map.get("items"), Some(&serde_json::json!(["a", "b"])));
        }

        #[test]
        fn test_empty_attrs() {
            let attrs: Vec<KeyValue> = vec![];
            let map = attrs_to_map(&attrs);
            assert!(map.is_empty());
        }
    }

    mod decode_id_tests {
        use super::*;

        #[test]
        fn test_hex_trace_id() {
            let result = decode_id("0123456789abcdef0123456789abcdef");
            assert!(result.is_ok());
            let value = result.unwrap_or_default();
            assert_eq!(value, "0123456789abcdef0123456789abcdef");
        }

        #[test]
        fn test_hex_span_id() {
            let result = decode_id("0123456789abcdef");
            assert!(result.is_ok());
            let value = result.unwrap_or_default();
            assert_eq!(value, "0123456789abcdef");
        }

        #[test]
        fn test_base64_id() {
            // Base64 for bytes [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]
            let result = decode_id("ASNFZ4mrze8=");
            assert!(result.is_ok());
            let value = result.unwrap_or_default();
            assert_eq!(value, "0123456789abcdef");
        }

        #[test]
        fn test_empty_id_error() {
            let result = decode_id("");
            assert!(result.is_err());
        }

        #[test]
        fn test_uppercase_hex_normalized() {
            let result = decode_id("0123456789ABCDEF");
            assert!(result.is_ok());
            let value = result.unwrap_or_default();
            assert_eq!(value, "0123456789abcdef");
        }
    }

    mod convert_otlp_tests {
        use super::*;

        fn sample_otlp_json() -> &'static str {
            r#"{
                "resourceSpans": [{
                    "resource": {
                        "attributes": [{
                            "key": "service.name",
                            "value": {"stringValue": "test-service"}
                        }, {
                            "key": "session.id",
                            "value": {"stringValue": "session-abc"}
                        }]
                    },
                    "scopeSpans": [{
                        "scope": {
                            "name": "opentelemetry-genai"
                        },
                        "spans": [{
                            "traceId": "0123456789abcdef0123456789abcdef",
                            "spanId": "0123456789abcdef",
                            "name": "chat_completion",
                            "startTimeUnixNano": "1700000000000000000",
                            "endTimeUnixNano": "1700000001000000000",
                            "attributes": [{
                                "key": "gen_ai.request.model",
                                "value": {"stringValue": "claude-3-opus"}
                            }, {
                                "key": "gen_ai.usage.input_tokens",
                                "value": {"intValue": "100"}
                            }, {
                                "key": "gen_ai.usage.output_tokens",
                                "value": {"intValue": "50"}
                            }, {
                                "key": "gen_ai.response.finish_reasons",
                                "value": {"arrayValue": {"values": [{"stringValue": "end_turn"}]}}
                            }],
                            "events": [{
                                "name": "gen_ai.user.message",
                                "timeUnixNano": "1700000000100000000",
                                "attributes": [{
                                    "key": "gen_ai.content",
                                    "value": {"stringValue": "Hello, how are you?"}
                                }]
                            }, {
                                "name": "gen_ai.assistant.message",
                                "timeUnixNano": "1700000000900000000",
                                "attributes": [{
                                    "key": "gen_ai.content",
                                    "value": {"stringValue": "I'm doing well, thank you!"}
                                }]
                            }]
                        }]
                    }]
                }]
            }"#
        }

        fn parse_sample_otlp() -> Option<OtlpTraceData> {
            serde_json::from_str(sample_otlp_json()).ok()
        }

        #[test]
        fn test_parse_otlp_json() {
            let data = parse_sample_otlp();
            assert!(data.is_some());
            let data = data.unwrap_or_else(|| OtlpTraceData { resource_spans: vec![] });
            assert_eq!(data.resource_spans.len(), 1);
            let resource_span = data.resource_spans.first();
            assert!(resource_span.is_some());
            let empty_vec = vec![];
            let scope_spans = resource_span.map(|r| &r.scope_spans).unwrap_or(&empty_vec);
            assert_eq!(scope_spans.len(), 1);
        }

        #[test]
        fn test_convert_otlp_basic() {
            let data = parse_sample_otlp();
            assert!(data.is_some());
            let spans = data.as_ref().and_then(|d| convert_otlp(d).ok());
            assert!(spans.is_some());
            let spans = spans.unwrap_or_default();

            // Should have 1 main span + 2 event spans
            assert_eq!(spans.len(), 3);
        }

        #[test]
        fn test_convert_otlp_main_span() {
            let data = parse_sample_otlp();
            assert!(data.is_some());
            let spans = data.as_ref().and_then(|d| convert_otlp(d).ok()).unwrap_or_default();
            let main_span = spans.first();
            assert!(main_span.is_some());
            let main_span = main_span.unwrap_or_else(|| {
                // This shouldn't happen, but satisfy the borrow checker
                static DEFAULT: std::sync::OnceLock<Span> = std::sync::OnceLock::new();
                DEFAULT.get_or_init(|| Span {
                    id: String::new(),
                    session_id: String::new(),
                    parent_span_id: None,
                    trace_id: String::new(),
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
                })
            });

            assert_eq!(main_span.trace_id, "0123456789abcdef0123456789abcdef");
            assert_eq!(main_span.id, "0123456789abcdef");
            assert_eq!(main_span.session_id, "session-abc");
            assert_eq!(main_span.model, Some("claude-3-opus".to_string()));
            assert_eq!(main_span.input_tokens, Some(100));
            assert_eq!(main_span.output_tokens, Some(50));
            assert_eq!(main_span.finish_reason, Some("end_turn".to_string()));
            assert_eq!(main_span.kind, SpanKind::Assistant);
        }

        #[test]
        fn test_convert_otlp_event_spans() {
            let data = parse_sample_otlp();
            assert!(data.is_some());
            let spans = data.as_ref().and_then(|d| convert_otlp(d).ok()).unwrap_or_default();

            // User message event
            let user_event = spans.get(1);
            assert!(user_event.is_some());
            if let Some(user_event) = user_event {
                assert_eq!(user_event.kind, SpanKind::User);
                assert_eq!(user_event.content, Some("Hello, how are you?".to_string()));
                assert_eq!(user_event.parent_span_id, Some("0123456789abcdef".to_string()));
            }

            // Assistant message event
            let assistant_event = spans.get(2);
            assert!(assistant_event.is_some());
            if let Some(assistant_event) = assistant_event {
                assert_eq!(assistant_event.kind, SpanKind::Assistant);
                assert_eq!(assistant_event.content, Some("I'm doing well, thank you!".to_string()));
            }
        }

        #[test]
        fn test_convert_otlp_duration() {
            let data = parse_sample_otlp();
            assert!(data.is_some());
            let spans = data.as_ref().and_then(|d| convert_otlp(d).ok()).unwrap_or_default();
            let main_span = spans.first();
            assert!(main_span.is_some());
            if let Some(main_span) = main_span {
                assert_eq!(main_span.duration_ms, Some(1000)); // 1 second
            }
        }

        #[test]
        fn test_convert_otlp_empty() {
            let data = OtlpTraceData { resource_spans: vec![] };
            let spans = convert_otlp(&data).unwrap_or_default();
            assert!(spans.is_empty());
        }
    }

    mod attribute_extraction_tests {
        use super::*;

        #[test]
        fn test_extract_model() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.response.model".to_string(), serde_json::json!("claude-3-opus"));

            let model =
                extract_string_attr(&attrs, &["gen_ai.response.model", "gen_ai.request.model"]);
            assert_eq!(model, Some("claude-3-opus".to_string()));
        }

        #[test]
        fn test_extract_model_fallback() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.request.model".to_string(), serde_json::json!("gpt-4"));

            let model =
                extract_string_attr(&attrs, &["gen_ai.response.model", "gen_ai.request.model"]);
            assert_eq!(model, Some("gpt-4".to_string()));
        }

        #[test]
        fn test_extract_tokens() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.usage.input_tokens".to_string(), serde_json::json!(100));
            attrs.insert("gen_ai.usage.output_tokens".to_string(), serde_json::json!(50));

            let input = extract_i64_attr(&attrs, &["gen_ai.usage.input_tokens"]);
            let output = extract_i64_attr(&attrs, &["gen_ai.usage.output_tokens"]);

            assert_eq!(input, Some(100));
            assert_eq!(output, Some(50));
        }

        #[test]
        fn test_extract_finish_reason_array() {
            let mut attrs = HashMap::new();
            attrs.insert(
                "gen_ai.response.finish_reasons".to_string(),
                serde_json::json!(["end_turn"]),
            );

            let reason = extract_finish_reason(&attrs);
            assert_eq!(reason, Some("end_turn".to_string()));
        }

        #[test]
        fn test_extract_finish_reason_string() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.response.finish_reasons".to_string(), serde_json::json!("stop"));

            let reason = extract_finish_reason(&attrs);
            assert_eq!(reason, Some("stop".to_string()));
        }

        #[test]
        fn test_extract_content() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.content".to_string(), serde_json::json!("Hello world"));

            let content = extract_content(&attrs);
            assert_eq!(content, Some("Hello world".to_string()));
        }

        #[test]
        fn test_extract_content_array() {
            let mut attrs = HashMap::new();
            attrs.insert(
                "gen_ai.content".to_string(),
                serde_json::json!([
                    {"text": "Hello"},
                    {"text": "World"}
                ]),
            );

            let content = extract_content(&attrs);
            assert_eq!(content, Some("Hello\nWorld".to_string()));
        }
    }

    mod tool_classification_tests {
        use super::*;

        #[test]
        fn test_tool_call_with_id_and_arguments() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.tool.call.id".to_string(), serde_json::json!("call-123"));
            attrs.insert("gen_ai.tool.name".to_string(), serde_json::json!("search"));
            attrs.insert(
                "gen_ai.tool.call.arguments".to_string(),
                serde_json::json!(r#"{"query": "test"}"#),
            );
            attrs.insert("content".to_string(), serde_json::json!("ignored"));

            let kind = classify_event("gen_ai.tool.message", &attrs);
            assert_eq!(kind, SpanKind::ToolCall);
        }

        #[test]
        fn test_tool_result_with_content() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.tool.call.id".to_string(), serde_json::json!("call-123"));
            attrs.insert("gen_ai.tool.result".to_string(), serde_json::json!("Result data"));

            let kind = classify_event("gen_ai.tool.message", &attrs);
            assert_eq!(kind, SpanKind::ToolResult);
        }

        #[test]
        fn test_tool_result_without_call_id() {
            let mut attrs = HashMap::new();
            attrs.insert("gen_ai.tool.name".to_string(), serde_json::json!("calculator"));

            let kind = classify_event("gen_ai.tool.message", &attrs);
            assert_eq!(kind, SpanKind::ToolResult);
        }
    }

    mod thinking_tests {
        use super::*;

        #[test]
        fn test_thinking_event_genai() {
            let attrs = HashMap::new();
            let kind = classify_event("gen_ai.thinking", &attrs);
            assert_eq!(kind, SpanKind::Thinking);
        }

        #[test]
        fn test_thinking_event_anthropic() {
            let attrs = HashMap::new();
            let kind = classify_event("thinking", &attrs);
            assert_eq!(kind, SpanKind::Thinking);
        }

        #[test]
        fn test_thinking_span_name() {
            let attrs = HashMap::new();
            let kind = classify_span("extended_thinking", &attrs);
            assert_eq!(kind, SpanKind::Thinking);
        }

        #[test]
        fn test_reasoning_span_name() {
            let attrs = HashMap::new();
            let kind = classify_span("chain_of_reasoning", &attrs);
            assert_eq!(kind, SpanKind::Thinking);
        }
    }

    mod base64_hex_tests {
        use super::*;

        #[test]
        fn test_standard_hex_trace_id() {
            let result = decode_id("4bf92f3577b34da6a3ce929d0e0e4736");
            assert!(result.is_ok());
            let value = result.unwrap_or_default();
            assert_eq!(value, "4bf92f3577b34da6a3ce929d0e0e4736");
        }

        #[test]
        fn test_standard_hex_span_id() {
            let result = decode_id("00f067aa0ba902b7");
            assert!(result.is_ok());
            let value = result.unwrap_or_default();
            assert_eq!(value, "00f067aa0ba902b7");
        }

        #[test]
        fn test_base64_trace_id() {
            // This is the base64 encoding of the 16 bytes for trace_id
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            let bytes: [u8; 16] = [
                0x4b, 0xf9, 0x2f, 0x35, 0x77, 0xb3, 0x4d, 0xa6, 0xa3, 0xce, 0x92, 0x9d, 0x0e, 0x0e,
                0x47, 0x36,
            ];
            let b64 = STANDARD.encode(bytes);

            let result = decode_id(&b64);
            assert!(result.is_ok());
            let value = result.unwrap_or_default();
            assert_eq!(value, "4bf92f3577b34da6a3ce929d0e0e4736");
        }

        #[test]
        fn test_base64_span_id() {
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            let bytes: [u8; 8] = [0x00, 0xf0, 0x67, 0xaa, 0x0b, 0xa9, 0x02, 0xb7];
            let b64 = STANDARD.encode(bytes);

            let result = decode_id(&b64);
            assert!(result.is_ok());
            let value = result.unwrap_or_default();
            assert_eq!(value, "00f067aa0ba902b7");
        }
    }

    mod extract_session_name_tests {
        use super::*;

        fn create_test_span(kind: SpanKind, content: Option<&str>) -> Span {
            Span {
                id: "span-1".to_string(),
                session_id: "session-1".to_string(),
                parent_span_id: None,
                trace_id: "trace-1".to_string(),
                kind,
                model: None,
                content: content.map(|s| s.to_string()),
                metadata: None,
                start_time: 1000,
                end_time: None,
                duration_ms: None,
                input_tokens: None,
                output_tokens: None,
                finish_reason: None,
                tool_call_id: None,
                tool_name: None,
            }
        }

        #[test]
        fn test_extract_session_name_from_user_message() {
            let spans = vec![
                create_test_span(SpanKind::System, Some("You are a helpful assistant.")),
                create_test_span(SpanKind::User, Some("Hello, how are you?")),
                create_test_span(SpanKind::Assistant, Some("I'm doing well!")),
            ];

            let name = extract_session_name(&spans);
            assert_eq!(name, Some("Hello, how are you?".to_string()));
        }

        #[test]
        fn test_extract_session_name_truncation() {
            let long_message =
                "This is a very long user message that exceeds the fifty character limit we set";
            let spans = vec![create_test_span(SpanKind::User, Some(long_message))];

            let name = extract_session_name(&spans);
            assert!(name.is_some());
            let name = name.unwrap_or_default();
            // Should be 47 chars + "..."
            assert_eq!(name.chars().count(), 50);
            assert!(name.ends_with("..."));
            assert!(name.starts_with("This is a very long user message that exceeds t"));
        }

        #[test]
        fn test_extract_session_name_no_user_message() {
            let spans = vec![
                create_test_span(SpanKind::System, Some("You are a helpful assistant.")),
                create_test_span(SpanKind::Assistant, Some("I'm ready to help!")),
            ];

            let name = extract_session_name(&spans);
            assert!(name.is_none());
        }

        #[test]
        fn test_extract_session_name_empty_content() {
            let spans = vec![create_test_span(SpanKind::User, Some(""))];

            let name = extract_session_name(&spans);
            assert!(name.is_none());
        }

        #[test]
        fn test_extract_session_name_whitespace_only() {
            let spans = vec![create_test_span(SpanKind::User, Some("   \n\t  "))];

            let name = extract_session_name(&spans);
            assert!(name.is_none());
        }

        #[test]
        fn test_extract_session_name_none_content() {
            let spans = vec![create_test_span(SpanKind::User, None)];

            let name = extract_session_name(&spans);
            assert!(name.is_none());
        }

        #[test]
        fn test_extract_session_name_trims_whitespace() {
            let spans = vec![create_test_span(SpanKind::User, Some("  Hello world  \n"))];

            let name = extract_session_name(&spans);
            assert_eq!(name, Some("Hello world".to_string()));
        }

        #[test]
        fn test_extract_session_name_first_user_message() {
            let spans = vec![
                create_test_span(SpanKind::User, Some("First message")),
                create_test_span(SpanKind::User, Some("Second message")),
            ];

            let name = extract_session_name(&spans);
            assert_eq!(name, Some("First message".to_string()));
        }

        #[test]
        fn test_extract_session_name_unicode_truncation() {
            // Test that truncation handles unicode correctly (using chars not bytes)
            let unicode_message = "Hello, \u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}!";
            let spans = vec![create_test_span(SpanKind::User, Some(unicode_message))];

            let name = extract_session_name(&spans);
            assert!(name.is_some());
            let name = name.unwrap_or_default();
            // Should be exactly 50 chars
            assert_eq!(name.chars().count(), 50);
            assert!(name.ends_with("..."));
        }

        #[test]
        fn test_extract_session_name_exactly_50_chars() {
            // Exactly 50 characters - should not be truncated
            let exact_50 = "12345678901234567890123456789012345678901234567890";
            assert_eq!(exact_50.chars().count(), 50);
            let spans = vec![create_test_span(SpanKind::User, Some(exact_50))];

            let name = extract_session_name(&spans);
            assert_eq!(name, Some(exact_50.to_string()));
        }

        #[test]
        fn test_extract_session_name_51_chars_truncated() {
            // 51 characters - should be truncated
            let char_51 = "123456789012345678901234567890123456789012345678901";
            assert_eq!(char_51.chars().count(), 51);
            let spans = vec![create_test_span(SpanKind::User, Some(char_51))];

            let name = extract_session_name(&spans);
            assert!(name.is_some());
            let name = name.unwrap_or_default();
            assert_eq!(name.chars().count(), 50);
            assert!(name.ends_with("..."));
        }

        #[test]
        fn test_extract_session_name_empty_spans() {
            let spans: Vec<Span> = vec![];
            let name = extract_session_name(&spans);
            assert!(name.is_none());
        }
    }
}
