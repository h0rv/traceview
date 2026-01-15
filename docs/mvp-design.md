## Design Document: `traceview`

### Core Requirements
- Ingest OTLP/HTTP traces at high volume
- Store locally in SQLite
- Stream new traces to UI via SSE
- Render agent interactions (user/assistant/thinking/tool calls)
- Single binary, minimal deps

---

### Major Design Decisions

**1. Ingestion Pipeline — Backpressure & Batching**

OTEL can blast thousands of spans/second. Can't insert one at a time.

```
OTLP POST → parse → mpsc channel (bounded) → batch writer task
                                                    ↓
                                              SQLite (batched inserts)
                                                    ↓
                                              broadcast channel → SSE
```

- Bounded channel = backpressure at ingestion if DB can't keep up
- Dedicated writer task batches inserts every 100ms or 1000 spans
- WAL mode for concurrent read/write

**2. SQLite Schema — Denormalized for Read Speed**

```sql
-- Sessions (agent conversations)
sessions (
    id TEXT PRIMARY KEY,
    name TEXT,
    created_at INTEGER,
    updated_at INTEGER
)

-- Flat span table, denormalized
spans (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    parent_span_id TEXT,
    trace_id TEXT NOT NULL,
    
    -- GenAI specific
    kind TEXT NOT NULL,  -- 'user' | 'assistant' | 'thinking' | 'tool_call' | 'tool_result' | 'span'
    model TEXT,
    
    -- Content
    content TEXT,
    metadata TEXT,  -- JSON blob for attributes
    
    -- Timing
    start_time INTEGER NOT NULL,
    end_time INTEGER,
    duration_ms INTEGER,
    
    -- Tokens (if present)
    input_tokens INTEGER,
    output_tokens INTEGER
)

-- Indexes for common queries
CREATE INDEX idx_spans_session ON spans(session_id, start_time);
CREATE INDEX idx_spans_trace ON spans(trace_id, start_time);
CREATE INDEX idx_spans_kind ON spans(kind);
CREATE INDEX idx_sessions_updated ON sessions(updated_at DESC);
```

Why denormalized:
- Single query gets everything for a session
- No JOINs = faster reads
- GenAI attributes extracted to columns for filtering

**3. OTLP Parsing — GenAI Attribute Extraction**

Map OTEL semconv to our `kind`:

```rust
// Detect span/event type from attributes
fn classify_span(attrs: &Attributes) -> SpanKind {
    match attrs.get("gen_ai.operation.name") {
        Some("chat") => SpanKind::Assistant,
        Some("tool_call") => SpanKind::ToolCall,
        _ => {}
    }
    
    // Check event names
    match event_name {
        "gen_ai.user.message" => SpanKind::User,
        "gen_ai.assistant.message" => SpanKind::Assistant,
        "gen_ai.tool.message" => SpanKind::ToolResult,
        // Anthropic-specific
        "gen_ai.thinking" => SpanKind::Thinking,
        _ => SpanKind::Span,
    }
}
```

**4. Session Detection**

OTEL doesn't have a native "session" concept. Options:
- Use `trace_id` as session (default)
- Look for custom attribute `session.id` or `gen_ai.conversation.id`
- Group traces by time proximity

```rust
fn extract_session_id(span: &OtelSpan) -> String {
    span.attributes.get("session.id")
        .or_else(|| span.attributes.get("gen_ai.conversation.id"))
        .unwrap_or(&span.trace_id)
        .clone()
}
```

**5. SSE Architecture — Per-Session Subscriptions**

```rust
// Global state
struct AppState {
    db: SqlitePool,
    
    // All new spans broadcast here
    span_tx: broadcast::Sender<SpanEvent>,
}

// SSE endpoint
GET /sessions/:id/stream → filter broadcast for that session
GET /stream → all spans (firehose)
```

Client subscribes to session, receives only relevant updates. Avoids blasting entire firehose to every client.

**6. UI Structure**

```
/                       → session list
/sessions/:id           → session detail view
/sessions/:id/stream    → SSE for that session
/api/sessions           → JSON list
/api/sessions/:id/spans → JSON spans
/v1/traces              → OTLP ingest
```

HTML structure for session view:
```html
<main id="trace-container">
  <!-- Existing spans rendered server-side -->
  <div class="span" data-kind="user">...</div>
  <div class="span" data-kind="assistant">...</div>
</main>

<script>
  const es = new EventSource('/sessions/xxx/stream');
  es.onmessage = e => {
    document.getElementById('trace-container')
      .insertAdjacentHTML('beforeend', e.data);
  };
</script>
```

**7. Scaling Considerations**

| Concern | Solution |
|---------|----------|
| Write throughput | Batched inserts, WAL mode |
| Read under load | Denormalized schema, proper indexes |
| Memory (large traces) | Pagination, virtual scroll hint in UI |
| SSE connection limits | Per-session filtering, bounded buffer |
| DB size growth | Optional retention policy (delete spans > N days) |
| Burst traffic | Bounded channel backpressure |

**8. Configuration**

```rust
struct Config {
    db_path: PathBuf,           // default: ./traces.db
    port: u16,                   // default: 6969
    batch_size: usize,           // default: 1000
    batch_interval_ms: u64,      // default: 100
    retention_days: Option<u32>, // default: None (keep forever)
}
```

---

### File Structure

```
traceview/
├── Cargo.toml
├── src/
│   ├── main.rs           # CLI, config, startup
│   ├── db.rs             # SQLite schema, queries, batch writer
│   ├── ingest.rs         # OTLP parsing, GenAI classification
│   ├── api.rs            # Axum routes (ingest, query, SSE)
│   ├── sse.rs            # SSE broadcast logic
│   ├── views/
│   │   ├── mod.rs
│   │   ├── layout.rs     # Base HTML layout
│   │   ├── sessions.rs   # Session list view
│   │   └── trace.rs      # Session detail view
│   └── models.rs         # Span, Session, SpanKind types
```
