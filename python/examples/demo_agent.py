"""
Demo agent that generates traces for traceview.

Requires: pip install pydantic-ai logfire
"""

import asyncio
import random
import sys
from datetime import datetime

import traceview

tv = traceview.init()
print(f"Traceview: {tv.endpoint}")

from pydantic_ai import Agent, RunContext

# Optional: logfire instrumentation for pydantic-ai
try:
    import logfire
    from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter
    from opentelemetry.sdk.trace.export import BatchSpanProcessor

    exporter = OTLPSpanExporter(endpoint=tv.traces_endpoint)
    logfire.configure(
        send_to_logfire=False,
        additional_span_processors=[BatchSpanProcessor(exporter)],
    )
    logfire.instrument_pydantic_ai()
except Exception:
    pass


async def get_weather(_ctx: RunContext[None], city: str) -> str:
    """Get the current weather for a city."""
    await asyncio.sleep(0.3)
    conditions = ["sunny", "cloudy", "rainy", "partly cloudy", "windy"]
    temp = random.randint(45, 95)
    return f"Weather in {city}: {temp}F, {random.choice(conditions)}"


async def calculate_tip(
    _ctx: RunContext[None], bill_amount: float, tip_percent: float
) -> str:
    """Calculate the tip amount for a bill."""
    tip = bill_amount * (tip_percent / 100)
    total = bill_amount + tip
    return f"Bill: ${bill_amount:.2f}, Tip ({tip_percent}%): ${tip:.2f}, Total: ${total:.2f}"


async def get_current_time(_ctx: RunContext[None], timezone: str = "UTC") -> str:
    """Get the current time in a specific timezone."""
    now = datetime.now()
    return f"Current time ({timezone}): {now.strftime('%Y-%m-%d %H:%M:%S')}"


agent = Agent(
    "anthropic:claude-sonnet-4-20250514",
    system_prompt="You are a helpful assistant with tools for weather, tips, and time.",
    tools=[get_weather, calculate_tip, get_current_time],
)


async def main() -> None:
    prompts = [
        "What's the weather in San Francisco and New York?",
        "Calculate a 20% tip on $127.50",
    ]

    if len(sys.argv) > 1:
        prompts = [" ".join(sys.argv[1:])]

    for prompt in prompts:
        print(f"\nUser: {prompt}")
        result = await agent.run(prompt)
        print(f"Assistant: {result.output}")


if __name__ == "__main__":
    asyncio.run(main())
