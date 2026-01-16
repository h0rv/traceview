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

import random
import sys
import time
import uuid
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


def generate_conversation(fixed_session_id: str | None = None) -> dict:
    """Generate a fake GenAI conversation trace.

    Args:
        fixed_session_id: If provided, use this session ID instead of generating a new one
    """
    trace_id = generate_trace_id()
    session_id = fixed_session_id or f"session-{uuid.uuid4().hex[:8]}"

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
    model = random.choice(
        [
            "claude-3-opus-20240229",
            "claude-3-sonnet-20240229",
            "gpt-4-turbo",
            "claude-sonnet-4-20250514",
        ]
    )

    # Build spans and events
    spans = []

    # Root span (chat completion)
    root_span_id = generate_span_id()
    root_start = ns_timestamp(0)
    root_end = ns_timestamp(random.randint(1000, 3000))

    events = []

    # User message event
    events.append(
        make_event(
            "gen_ai.user.message",
            ns_timestamp(10),
            [make_attr("gen_ai.content", scenario["user_msg"])],
        )
    )

    # Thinking event (extended thinking)
    events.append(
        make_event(
            "gen_ai.thinking",
            ns_timestamp(100),
            [make_attr("gen_ai.content", scenario["thinking"])],
        )
    )

    # Tool call and result if applicable
    if scenario["tool_name"]:
        tool_call_id = f"call_{uuid.uuid4().hex[:8]}"

        # Tool call event
        events.append(
            make_event(
                "gen_ai.tool.message",
                ns_timestamp(200),
                [
                    make_attr("gen_ai.tool.name", scenario["tool_name"]),
                    make_attr("gen_ai.tool.call.id", tool_call_id),
                    make_attr("gen_ai.content", scenario["tool_input"]),
                    make_attr("tool_calls", "true"),
                ],
            )
        )

        # Tool result event
        events.append(
            make_event(
                "gen_ai.tool.message",
                ns_timestamp(500),
                [
                    make_attr("gen_ai.tool.name", scenario["tool_name"]),
                    make_attr("gen_ai.tool.call.id", tool_call_id),
                    make_attr("gen_ai.content", scenario["tool_output"]),
                ],
            )
        )

    # Assistant message event
    events.append(
        make_event(
            "gen_ai.assistant.message",
            ns_timestamp(800),
            [make_attr("gen_ai.content", scenario["assistant_msg"])],
        )
    )

    # Choice event
    events.append(
        make_event(
            "gen_ai.choice",
            ns_timestamp(850),
            [
                make_attr("index", 0),
                make_attr("gen_ai.finish_reason", "end_turn"),
            ],
        )
    )

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
        "resourceSpans": [
            {
                "resource": {
                    "attributes": [
                        make_attr("service.name", "demo-agent"),
                        make_attr("service.version", "1.0.0"),
                        make_attr("session.id", session_id),
                    ]
                },
                "scopeSpans": [
                    {
                        "scope": {"name": "pydantic-ai"},
                        "spans": spans,
                    }
                ],
            }
        ]
    }


def send_single_event(session_id: str, event_type: str, content: str, **kwargs) -> None:
    """Send a single event/span to simulate streaming LLM output."""
    trace_id = generate_trace_id()
    span_id = generate_span_id()
    now_ns = ns_timestamp(0)

    # Map event type to OTEL event name
    event_name_map = {
        "user": "gen_ai.user.message",
        "assistant": "gen_ai.assistant.message",
        "thinking": "gen_ai.thinking",
        "tool_call": "gen_ai.tool.message",
        "tool_result": "gen_ai.tool.message",
        "system": "gen_ai.system.message",
    }

    event_name = event_name_map.get(event_type, "gen_ai.assistant.message")

    # Build event attributes
    event_attrs = [make_attr("gen_ai.content", content)]
    if event_type == "tool_call":
        event_attrs.append(make_attr("tool_calls", "true"))
        if "tool_name" in kwargs:
            event_attrs.append(make_attr("gen_ai.tool.name", kwargs["tool_name"]))
            event_attrs.append(
                make_attr("gen_ai.tool.call.id", f"call_{uuid.uuid4().hex[:8]}")
            )
    elif event_type == "tool_result" and "tool_name" in kwargs:
        event_attrs.append(make_attr("gen_ai.tool.name", kwargs["tool_name"]))

    # Build the trace
    trace_data = {
        "resourceSpans": [
            {
                "resource": {"attributes": [make_attr("session.id", session_id)]},
                "scopeSpans": [
                    {
                        "scope": {"name": "demo"},
                        "spans": [
                            {
                                "traceId": trace_id,
                                "spanId": span_id,
                                "name": event_type,
                                "startTimeUnixNano": now_ns,
                                "endTimeUnixNano": ns_timestamp(100),
                                "attributes": [
                                    make_attr("gen_ai.system", "anthropic"),
                                    make_attr(
                                        "gen_ai.request.model",
                                        kwargs.get("model", "claude-sonnet-4-20250514"),
                                    ),
                                ],
                                "events": [
                                    {
                                        "name": event_name,
                                        "timeUnixNano": now_ns,
                                        "attributes": event_attrs,
                                    }
                                ],
                            }
                        ],
                    }
                ],
            }
        ]
    }

    httpx.post(
        TRACEVIEW_URL,
        json=trace_data,
        headers={"Content-Type": "application/json"},
        timeout=10,
    )


def simulate_conversation(session_id: str) -> None:
    """Simulate a realistic LLM conversation with streaming delays."""
    print(f"\n💬 Simulating conversation in session: {session_id}")

    # Pick a random scenario
    scenarios = [
        {
            "user": "What's the weather like in San Francisco today?",
            "thinking": "The user wants current weather information for San Francisco. I should use the weather tool to get accurate, up-to-date information rather than relying on my training data.",
            "tool_name": "get_weather",
            "tool_input": '{"city": "San Francisco", "units": "fahrenheit"}',
            "tool_output": "Current conditions in San Francisco: 64°F, partly cloudy, humidity 72%, wind 12 mph W",
            "assistant": "The weather in San Francisco today is quite pleasant! It's currently 64°F (18°C) with partly cloudy skies. The humidity is at 72% and there's a light westerly wind at 12 mph. Perfect weather for a walk along the Embarcadero!",
        },
        {
            "user": "Calculate a 20% tip on a $127.50 dinner bill, and also show me 15% and 25%",
            "thinking": "The user wants tip calculations at multiple percentages. I'll calculate all three to give them options. This is straightforward math: multiply the bill by each percentage.",
            "tool_name": "calculator",
            "tool_input": '{"expression": "127.50 * 0.20"}',
            "tool_output": "25.50",
            "assistant": "Here are the tip calculations for your $127.50 bill:\n\n• **15% tip:** $19.13 → Total: $146.63\n• **20% tip:** $25.50 → Total: $153.00\n• **25% tip:** $31.88 → Total: $159.38\n\nThe 20% tip of $25.50 bringing your total to $153.00 is a nice round number!",
        },
        {
            "user": "Explain how transformers work in machine learning",
            "thinking": "This is a technical explanation request about transformer architecture. I should explain the key concepts: attention mechanism, self-attention, positional encoding, and how they enable parallel processing. I'll keep it accessible while being accurate.",
            "tool_name": None,
            "assistant": "Transformers are a neural network architecture that revolutionized NLP. Here's how they work:\n\n**Self-Attention**: The key innovation. Each word looks at every other word in the sequence to understand context. \"The cat sat on the mat\" - 'sat' attends to 'cat' to know who's sitting.\n\n**Positional Encoding**: Since transformers process all words in parallel (unlike RNNs), they add position information so the model knows word order.\n\n**Multi-Head Attention**: Multiple attention patterns run in parallel, letting the model focus on different types of relationships simultaneously.\n\n**Feed-Forward Layers**: After attention, each position goes through the same neural network independently.\n\nThe magic is parallelization - transformers can process entire sequences at once, making them much faster to train than sequential models.",
        },
    ]

    scenario = random.choice(scenarios)

    # User message
    print(f"  👤 User: {scenario['user'][:60]}...")
    send_single_event(session_id, "user", scenario["user"])
    time.sleep(random.uniform(0.3, 0.6))

    # Thinking (simulated delay for "reasoning")
    print("  🧠 Thinking...")
    send_single_event(session_id, "thinking", scenario["thinking"])
    time.sleep(random.uniform(0.8, 1.5))

    # Tool call if applicable
    if scenario.get("tool_name"):
        print(f"  🔧 Tool call: {scenario['tool_name']}")
        send_single_event(
            session_id,
            "tool_call",
            scenario["tool_input"],
            tool_name=scenario["tool_name"],
        )
        time.sleep(random.uniform(0.4, 0.8))

        # Tool result
        print("  📥 Tool result received")
        send_single_event(
            session_id,
            "tool_result",
            scenario["tool_output"],
            tool_name=scenario["tool_name"],
        )
        time.sleep(random.uniform(0.2, 0.4))

    # Assistant response (simulate token streaming with chunks)
    print("  🤖 Assistant responding...")
    response = scenario["assistant"]

    # Split response into chunks to simulate streaming
    chunk_size = random.randint(50, 100)
    chunks = [response[i : i + chunk_size] for i in range(0, len(response), chunk_size)]

    for j, _chunk in enumerate(chunks):
        # Send partial response
        partial = response[: ((j + 1) * chunk_size)]
        send_single_event(session_id, "assistant", partial)
        time.sleep(random.uniform(0.1, 0.3))  # Token generation delay

    print(f"  ✅ Response complete ({len(response)} chars)")


def send_traces(
    count: int = 3, fixed_session_id: str | None = None, streaming: bool = True
) -> None:
    """Send multiple conversation traces to traceview.

    Args:
        count: Number of traces/conversations to send
        fixed_session_id: If provided, all traces go to this session (for testing live updates)
        streaming: If True, simulate realistic LLM timing with delays
    """
    if streaming and fixed_session_id:
        # Use the new streaming simulation
        print(f"🎬 Simulating {count} streaming conversation(s)...")
        for i in range(count):
            print(f"\n{'=' * 50}")
            print(f"Conversation {i + 1}/{count}")
            print("=" * 50)
            simulate_conversation(fixed_session_id)
            if i < count - 1:
                time.sleep(1.0)  # Pause between conversations
        print(f"\n✅ Done! View at http://localhost:6969/sessions/{fixed_session_id}")
        return

    # Original batch mode
    if fixed_session_id:
        print(f"🚀 Sending {count} traces to session '{fixed_session_id}'...")
    else:
        print(f"🚀 Sending {count} conversation traces to {TRACEVIEW_URL}...")

    for i in range(count):
        trace_data = generate_conversation(fixed_session_id)

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
                    (
                        a["value"].get("stringValue", "")
                        for a in span["attributes"]
                        if a["key"] == "session.id"
                    ),
                    "unknown",
                )
                model = next(
                    (
                        a["value"].get("stringValue", "")
                        for a in span["attributes"]
                        if a["key"] == "gen_ai.request.model"
                    ),
                    "unknown",
                )
                user_msg = ""
                for event in span.get("events", []):
                    if event["name"] == "gen_ai.user.message":
                        user_msg = next(
                            (
                                a["value"].get("stringValue", "")[:50]
                                for a in event["attributes"]
                                if a["key"] == "gen_ai.content"
                            ),
                            "",
                        )
                        break

                print(f"  ✓ [{i + 1}/{count}] Session: {session_id}, Model: {model}")
                print(f"              User: {user_msg}...")
        except httpx.HTTPStatusError as e:
            print(f"  ✗ [{i + 1}/{count}] HTTP error: {e.response.status_code}")
        except httpx.RequestError as e:
            print(f"  ✗ [{i + 1}/{count}] Request failed: {e}")
            print("     Is traceview running? Start with: cargo run -- --port 6969")
            return

        time.sleep(0.2)  # Small delay between traces

    print("\n✅ Done! View traces at http://localhost:6969/")


def main() -> None:
    """Main entry point."""
    count = 3
    session_id = None
    streaming = False
    new_session = False

    # Parse arguments
    args = sys.argv[1:]
    i = 0
    while i < len(args):
        if args[i] == "--count" and i + 1 < len(args):
            try:
                count = int(args[i + 1])
            except ValueError:
                print(
                    "Usage: send_traces.py [--count N] [--session ID] [--stream] [--new]"
                )
                sys.exit(1)
            i += 2
        elif args[i] == "--session" and i + 1 < len(args):
            session_id = args[i + 1]
            i += 2
        elif args[i] == "--stream":
            streaming = True
            i += 1
        elif args[i] == "--new":
            new_session = True
            i += 1
        elif args[i] in ("-h", "--help"):
            print("""
send_traces.py - Send demo traces to traceview

Usage:
    uv run scripts/send_traces.py [options]

Options:
    --count N      Number of conversations to send (default: 3)
    --session ID   Send to a specific session ID
    --stream       Simulate realistic LLM timing with delays
    --new          Create a new session and stream to it (combines --stream with new session)
    -h, --help     Show this help

Examples:
    # Send 3 quick batch traces (new sessions each)
    uv run scripts/send_traces.py

    # Create a new session and stream a realistic conversation
    uv run scripts/send_traces.py --new

    # Stream multiple conversations to a new session
    uv run scripts/send_traces.py --new --count 3

    # Stream to an existing session
    uv run scripts/send_traces.py --session session-abc123 --stream --count 2
""")
            sys.exit(0)
        else:
            i += 1

    # --new creates a new session and enables streaming
    if new_session:
        session_id = f"session-{uuid.uuid4().hex[:8]}"
        streaming = True
        print(f"📝 Created new session: {session_id}")

    send_traces(count, session_id, streaming)


if __name__ == "__main__":
    main()
