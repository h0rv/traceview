#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "httpx",
# ]
# ///
"""
Send dummy OTLP traces to traceview for testing.

No API keys required - just generates fake GenAI traces.

Usage:
    # Start traceview first
    cargo run -- --port 6969

    # Send traces
    uv run scripts/send_traces.py

    # Send more traces
    uv run scripts/send_traces.py --count 5
"""

import sys
import time
import uuid
import random
from typing import Any

import httpx

TRACEVIEW_URL = "http://localhost:6969/v1/traces"


def generate_span_id() -> str:
    """Generate a 16-char hex span ID."""
    return uuid.uuid4().hex[:16]


def generate_trace_id() -> str:
    """Generate a 32-char hex trace ID."""
    return uuid.uuid4().hex


def ns_timestamp(offset_ms: int = 0) -> str:
    """Get current time in nanoseconds as string, with optional offset."""
    return str(int((time.time() + offset_ms / 1000) * 1_000_000_000))


def make_attr(key: str, value: Any) -> dict:
    """Create an OTLP attribute."""
    if isinstance(value, str):
        return {"key": key, "value": {"stringValue": value}}
    elif isinstance(value, int):
        return {"key": key, "value": {"intValue": str(value)}}
    elif isinstance(value, bool):
        return {"key": key, "value": {"boolValue": value}}
    else:
        return {"key": key, "value": {"stringValue": str(value)}}


def make_event(name: str, timestamp: str, attrs: list[dict]) -> dict:
    """Create an OTLP event."""
    return {
        "name": name,
        "timeUnixNano": timestamp,
        "attributes": attrs,
    }


def generate_conversation() -> dict:
    """Generate a fake GenAI conversation trace."""
    trace_id = generate_trace_id()
    session_id = f"session-{uuid.uuid4().hex[:8]}"

    # Conversation scenarios
    scenarios = [
        {
            "user_msg": "What's the weather like in San Francisco?",
            "thinking": "The user wants weather information for San Francisco. I should use the weather tool to get current conditions.",
            "tool_name": "get_weather",
            "tool_input": '{"city": "San Francisco"}',
            "tool_output": "Weather in San Francisco: 62°F, partly cloudy, humidity 78%",
            "assistant_msg": "The current weather in San Francisco is 62°F and partly cloudy with 78% humidity. It's a typical day for the Bay Area!",
        },
        {
            "user_msg": "Calculate a 20% tip on a $85 dinner bill",
            "thinking": "The user wants to calculate a tip. I need to compute 20% of $85. Let me use the calculator tool.",
            "tool_name": "calculate_tip",
            "tool_input": '{"bill": 85.00, "percent": 20}',
            "tool_output": "Tip: $17.00, Total: $102.00",
            "assistant_msg": "For an $85 dinner bill with a 20% tip:\n- Tip amount: $17.00\n- Total with tip: $102.00",
        },
        {
            "user_msg": "Search for the latest news about AI",
            "thinking": "The user wants to find recent AI news. I'll use the web search tool to find relevant articles.",
            "tool_name": "web_search",
            "tool_input": '{"query": "latest AI news 2024"}',
            "tool_output": "Found 3 results: 1) New breakthrough in LLM reasoning... 2) AI regulation updates... 3) OpenAI announces...",
            "assistant_msg": "Here are the latest AI news highlights:\n1. New breakthrough in LLM reasoning capabilities\n2. Updates on AI regulation in the EU and US\n3. Major announcements from leading AI companies",
        },
        {
            "user_msg": "What time is it in Tokyo?",
            "thinking": "The user wants the current time in Tokyo. I should get the time and convert to JST timezone.",
            "tool_name": "get_time",
            "tool_input": '{"timezone": "Asia/Tokyo"}',
            "tool_output": "2024-01-15 14:32:00 JST",
            "assistant_msg": "The current time in Tokyo (JST) is 2:32 PM on January 15th, 2024.",
        },
        {
            "user_msg": "Explain quantum computing in simple terms",
            "thinking": "This is a conceptual question that doesn't require tools. I'll explain quantum computing using accessible analogies.",
            "tool_name": None,  # No tool call for this one
            "tool_input": None,
            "tool_output": None,
            "assistant_msg": "Quantum computing uses quantum bits (qubits) that can be both 0 and 1 simultaneously, unlike regular bits. Think of it like being able to check all paths in a maze at once instead of one at a time. This makes quantum computers potentially very powerful for certain types of problems!",
        },
    ]

    scenario = random.choice(scenarios)
    model = random.choice(["claude-3-opus-20240229", "claude-3-sonnet-20240229", "gpt-4-turbo", "claude-sonnet-4-20250514"])

    # Build spans and events
    spans = []

    # Root span (chat completion)
    root_span_id = generate_span_id()
    root_start = ns_timestamp(0)
    root_end = ns_timestamp(random.randint(1000, 3000))

    events = []

    # User message event
    events.append(make_event(
        "gen_ai.user.message",
        ns_timestamp(10),
        [make_attr("gen_ai.content", scenario["user_msg"])]
    ))

    # Thinking event (extended thinking)
    events.append(make_event(
        "gen_ai.thinking",
        ns_timestamp(100),
        [make_attr("gen_ai.content", scenario["thinking"])]
    ))

    # Tool call and result if applicable
    if scenario["tool_name"]:
        tool_call_id = f"call_{uuid.uuid4().hex[:8]}"

        # Tool call event
        events.append(make_event(
            "gen_ai.tool.message",
            ns_timestamp(200),
            [
                make_attr("gen_ai.tool.name", scenario["tool_name"]),
                make_attr("gen_ai.tool.call.id", tool_call_id),
                make_attr("gen_ai.content", scenario["tool_input"]),
                make_attr("tool_calls", "true"),
            ]
        ))

        # Tool result event
        events.append(make_event(
            "gen_ai.tool.message",
            ns_timestamp(500),
            [
                make_attr("gen_ai.tool.name", scenario["tool_name"]),
                make_attr("gen_ai.tool.call.id", tool_call_id),
                make_attr("gen_ai.content", scenario["tool_output"]),
            ]
        ))

    # Assistant message event
    events.append(make_event(
        "gen_ai.assistant.message",
        ns_timestamp(800),
        [make_attr("gen_ai.content", scenario["assistant_msg"])]
    ))

    # Choice event
    events.append(make_event(
        "gen_ai.choice",
        ns_timestamp(850),
        [
            make_attr("index", 0),
            make_attr("gen_ai.finish_reason", "end_turn"),
        ]
    ))

    # Build root span
    input_tokens = random.randint(50, 500)
    output_tokens = random.randint(100, 800)

    root_span = {
        "traceId": trace_id,
        "spanId": root_span_id,
        "name": "chat",
        "startTimeUnixNano": root_start,
        "endTimeUnixNano": root_end,
        "attributes": [
            make_attr("gen_ai.system", "anthropic" if "claude" in model else "openai"),
            make_attr("gen_ai.operation.name", "chat"),
            make_attr("gen_ai.request.model", model),
            make_attr("gen_ai.response.model", model),
            make_attr("gen_ai.usage.input_tokens", input_tokens),
            make_attr("gen_ai.usage.output_tokens", output_tokens),
            make_attr("gen_ai.response.finish_reasons", "end_turn"),
            make_attr("session.id", session_id),
        ],
        "events": events,
    }
    spans.append(root_span)

    return {
        "resourceSpans": [{
            "resource": {
                "attributes": [
                    make_attr("service.name", "demo-agent"),
                    make_attr("service.version", "1.0.0"),
                    make_attr("session.id", session_id),
                ]
            },
            "scopeSpans": [{
                "scope": {"name": "pydantic-ai"},
                "spans": spans,
            }]
        }]
    }


def send_traces(count: int = 3) -> None:
    """Send multiple conversation traces to traceview."""
    print(f"🚀 Sending {count} conversation traces to {TRACEVIEW_URL}...")

    for i in range(count):
        trace_data = generate_conversation()

        try:
            response = httpx.post(
                TRACEVIEW_URL,
                json=trace_data,
                headers={"Content-Type": "application/json"},
                timeout=10,
            )
            response.raise_for_status()

            # Extract some info for display
            spans = trace_data["resourceSpans"][0]["scopeSpans"][0]["spans"]
            if spans:
                span = spans[0]
                session_id = next(
                    (a["value"].get("stringValue", "") for a in span["attributes"] if a["key"] == "session.id"),
                    "unknown"
                )
                model = next(
                    (a["value"].get("stringValue", "") for a in span["attributes"] if a["key"] == "gen_ai.request.model"),
                    "unknown"
                )
                user_msg = ""
                for event in span.get("events", []):
                    if event["name"] == "gen_ai.user.message":
                        user_msg = next(
                            (a["value"].get("stringValue", "")[:50] for a in event["attributes"] if a["key"] == "gen_ai.content"),
                            ""
                        )
                        break

                print(f"  ✓ [{i+1}/{count}] Session: {session_id}, Model: {model}")
                print(f"              User: {user_msg}...")
        except httpx.HTTPStatusError as e:
            print(f"  ✗ [{i+1}/{count}] HTTP error: {e.response.status_code}")
        except httpx.RequestError as e:
            print(f"  ✗ [{i+1}/{count}] Request failed: {e}")
            print(f"     Is traceview running? Start with: cargo run -- --port 6969")
            return

        time.sleep(0.2)  # Small delay between traces

    print(f"\n✅ Done! View traces at http://localhost:6969/")


def main() -> None:
    """Main entry point."""
    count = 3

    # Parse --count argument
    if "--count" in sys.argv:
        try:
            idx = sys.argv.index("--count")
            count = int(sys.argv[idx + 1])
        except (IndexError, ValueError):
            print("Usage: send_traces.py [--count N]")
            sys.exit(1)

    send_traces(count)


if __name__ == "__main__":
    main()
