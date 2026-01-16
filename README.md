# tv - Local Trace Viewer for AI Agents

A fast, local trace viewer for AI agent development. Single binary, zero config, works with any OpenTelemetry-instrumented AI framework.

```bash
uvx trace-view
```

## Why tv?

When building AI agents with tools like pydantic-ai, LangChain, or custom frameworks, you need to see what's happening:
- What prompts are being sent?
- What tools are being called and with what arguments?
- Where is the latency coming from?
- What's the token usage?

Production observability tools (Datadog, Jaeger) are overkill for local development. `tv` gives you a purpose-built viewer that understands AI agent traces.

## Features

**Dual Interface** - Web UI for detailed analysis, TUI for quick terminal-based viewing

**GenAI-Aware** - Automatically classifies spans into user messages, assistant responses, tool calls, and thinking blocks (including Claude's extended thinking)

**Zero Config** - Point your OTEL exporter at `localhost:4318` and spans appear automatically. Sessions are auto-grouped by process.

**Real-time Updates** - Spans stream in live via SSE as your agent runs

**Token Tracking** - See input/output token counts and totals per session

**Timeline View** - Visual representation of span timing for performance analysis

**Search** - Full-text search across span content

**Export** - Export sessions to JSON for analysis or sharing

## Installation

```bash
# Run directly (no install)
uvx trace-view

# Or install globally
uv tool install trace-view

# Or add to project
uv add trace-view

# From source
cargo install traceview
```

## Quick Start

```python
import traceview

traceview.init()  # starts server at http://localhost:4318, configures OTEL

# your traced code runs normally
run_my_agent()
```

```bash
uv add "trace-view[otel]"
python my_agent.py
```

Or run standalone: `uvx trace-view`

## CLI Usage

```bash
# Default: Web UI + TUI together
tv

# Server only (background/headless)
tv serve

# TUI only (connect to existing db)
tv ui

# Custom port and database
tv --port 8080 --db my_traces.db

# Environment variables work too
TV_PORT=8080 TV_DB_PATH=my_traces.db tv serve
```

## Keyboard Shortcuts (TUI)

| Key | Action |
|-----|--------|
| `j` / `k` | Move down / up |
| `gg` / `G` | Jump to first / last |
| `Ctrl+d` / `Ctrl+u` | Page down / up |
| `Enter` / `Space` | Expand span |
| `Tab` | Switch panel |
| `h` / `l` | Focus sessions / spans |
| `?` | Show help |
| `q` | Quit |

## Web UI Features

- **Filter tabs**: All | Tools | Thoughts | Errors
- **View modes**: Conversation (default) | Timeline
- **Search**: Full-text search across spans
- **Export**: Download sessions as JSON
- **Dark/Light mode** toggle
- **Real-time updates** via SSE
- **Token summary** per session

## How Sessions Work

`tv` automatically groups spans into sessions using this priority:
1. `session.id` attribute (explicit)
2. `gen_ai.conversation.id` (OTEL GenAI convention)
3. `service.instance.id` (auto-generated per process)
4. Falls back to `trace_id`

This means all spans from a single script execution automatically group together without any configuration.

## Span Classification

`tv` maps OTEL GenAI semantic conventions to span types:

| Span Kind | Source |
|-----------|--------|
| User | `gen_ai.user.message` events |
| Assistant | `gen_ai.assistant.message` events |
| Thinking | `gen_ai.thinking` (Anthropic extended thinking) |
| Tool Call | `gen_ai.tool.message` with `tool_calls` |
| Tool Result | `gen_ai.tool.message` without `tool_calls` |
| System | `gen_ai.system.message` events |

## Architecture

- **Single binary** - No external dependencies
- **SQLite storage** - WAL mode for concurrent read/write
- **OTLP ingest** - HTTP/JSON and HTTP/protobuf support
- **Batch writer** - Configurable batching for high-throughput ingest
- **SSE streaming** - Real-time updates without polling

## Development

```bash
# Install dependencies (use --no-editable for binary)
uv sync --no-editable

# Run the binary
.venv/bin/tv

# Build Rust directly
cargo build

# Run tests
cargo test

# Before committing
cargo cl        # strict clippy
cargo t         # tests
cargo fmt --check

# Python checks
uv run ruff check python/
uv run pyright python/
```

## License

MIT

## Roadmap

- [x] Full-text search
- [x] Export to JSON
- [x] Python package
- [ ] Pre-built binaries
- [ ] Cost tracking
- [ ] Span diff mode
