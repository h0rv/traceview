"""
Traceview - Local OpenTelemetry trace viewer for GenAI applications.

Usage as a context manager:

    from traceview import Traceview

    with Traceview() as tv:
        # Server is running at http://localhost:4318
        # Your traced code here...
        pass

Usage with OpenTelemetry auto-configuration:

    from traceview import Traceview

    with Traceview(configure_otel=True) as tv:
        # OpenTelemetry is configured to send traces to traceview
        tracer = trace.get_tracer(__name__)
        with tracer.start_as_current_span("my-span"):
            pass

Command-line usage:

    $ tv serve        # Start server only
    $ tv ui           # Start TUI only
    $ tv              # Start both server and TUI
"""

from ._binary import get_binary_path, run_binary
from .server import Traceview, TraceviewError, init

__all__ = [
    "Traceview",
    "TraceviewError",
    "get_binary_path",
    "init",
    "run_binary",
]

__version__ = "0.1.0"
