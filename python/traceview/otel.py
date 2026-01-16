"""
OpenTelemetry configuration helpers.

Provides utilities for configuring OpenTelemetry to work with traceview,
including environment variable setup and manual configuration.
"""

from __future__ import annotations

import os


def configure_environment(
    endpoint: str = "http://localhost:4318",
    session_id: str | None = None,
    service_name: str = "traceview-client",
) -> dict[str, str]:
    """
    Configure OpenTelemetry via environment variables.

    This sets the standard OTLP environment variables so that any
    OpenTelemetry-instrumented code will automatically send traces
    to the traceview server.

    This is useful when you can't modify code but want traces to
    go to traceview.

    Args:
        endpoint: Base URL for the OTLP endpoint.
        session_id: Optional session ID to group traces.
        service_name: Service name for the resource.

    Returns:
        Dictionary of environment variables that were set.

    Example:
        from traceview.otel import configure_environment

        # Set environment variables before importing instrumented code
        configure_environment(session_id="my-session")

        # Now any OpenTelemetry-instrumented code will send to traceview
        import my_instrumented_app
        my_instrumented_app.run()
    """
    env_vars = {
        "OTEL_EXPORTER_OTLP_ENDPOINT": endpoint,
        "OTEL_EXPORTER_OTLP_PROTOCOL": "http/protobuf",
        "OTEL_SERVICE_NAME": service_name,
    }

    if session_id:
        existing_attrs = os.environ.get("OTEL_RESOURCE_ATTRIBUTES", "")
        session_attr = f"session.id={session_id}"
        if existing_attrs:
            env_vars["OTEL_RESOURCE_ATTRIBUTES"] = f"{existing_attrs},{session_attr}"
        else:
            env_vars["OTEL_RESOURCE_ATTRIBUTES"] = session_attr

    for key, value in env_vars.items():
        os.environ[key] = value

    return env_vars


def get_tracer(name: str = __name__):
    """
    Get an OpenTelemetry tracer.

    Convenience function that handles the import and returns a tracer.
    Requires opentelemetry-api to be installed.

    Args:
        name: Name for the tracer, typically __name__.

    Returns:
        OpenTelemetry Tracer instance.

    Example:
        from traceview.otel import get_tracer

        tracer = get_tracer(__name__)
        with tracer.start_as_current_span("my-operation"):
            do_work()
    """
    from opentelemetry import trace

    return trace.get_tracer(name)
