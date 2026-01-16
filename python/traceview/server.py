"""
Server context manager for running traceview as a local development server.

Provides a clean API for starting and stopping the traceview server,
with optional OpenTelemetry auto-configuration.
"""

from __future__ import annotations

import atexit
import os
import signal
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import TYPE_CHECKING

from ._binary import get_binary_path

if TYPE_CHECKING:
    from collections.abc import Callable
    from types import TracebackType


class TraceviewError(Exception):
    """Error raised when traceview server operations fail."""

    pass


@dataclass
class TraceviewConfig:
    """Configuration for the traceview server."""

    port: int = 4318
    db_path: str = "traces.db"
    batch_size: int = 1000
    batch_interval_ms: int = 100
    startup_timeout: float = 5.0
    configure_otel: bool = False
    session_id: str | None = None

    @property
    def endpoint(self) -> str:
        """OTLP HTTP endpoint URL."""
        return f"http://localhost:{self.port}"

    @property
    def traces_endpoint(self) -> str:
        """Full URL for trace ingestion."""
        return f"{self.endpoint}/v1/traces"

    @property
    def ui_url(self) -> str:
        """URL for the web UI."""
        return self.endpoint


class Traceview:
    """
    Context manager for running a local traceview server.

    Starts the traceview server in a subprocess and optionally configures
    OpenTelemetry to send traces to it. The server is automatically stopped
    when the context manager exits.

    Example:
        with Traceview(port=4318, configure_otel=True) as tv:
            print(f"Server running at {tv.ui_url}")
            # Your traced code here

    Example with custom session ID:
        with Traceview(configure_otel=True, session_id="my-session") as tv:
            # All traces will be tagged with session.id="my-session"
            pass
    """

    def __init__(
        self,
        port: int = 4318,
        db_path: str = "traces.db",
        batch_size: int = 1000,
        batch_interval_ms: int = 100,
        startup_timeout: float = 5.0,
        configure_otel: bool = False,
        session_id: str | None = None,
    ):
        """
        Initialize traceview server configuration.

        Args:
            port: Port for the HTTP server. Default: 4318 (standard OTLP HTTP port).
            db_path: Path to SQLite database file. Default: "traces.db".
            batch_size: Batch size for span inserts. Default: 1000.
            batch_interval_ms: Batch interval in milliseconds. Default: 100.
            startup_timeout: Seconds to wait for server to start. Default: 5.0.
            configure_otel: If True, configure OpenTelemetry to send traces here.
            session_id: Optional session ID to tag all traces with.
        """
        self.config = TraceviewConfig(
            port=port,
            db_path=db_path,
            batch_size=batch_size,
            batch_interval_ms=batch_interval_ms,
            startup_timeout=startup_timeout,
            configure_otel=configure_otel,
            session_id=session_id,
        )
        self._process: subprocess.Popen[bytes] | None = None
        self._otel_cleanup: Callable[[], None] | None = None

    @property
    def endpoint(self) -> str:
        """OTLP HTTP endpoint URL."""
        return self.config.endpoint

    @property
    def traces_endpoint(self) -> str:
        """Full URL for trace ingestion."""
        return self.config.traces_endpoint

    @property
    def ui_url(self) -> str:
        """URL for the web UI."""
        return self.config.ui_url

    @property
    def is_running(self) -> bool:
        """Check if the server process is running."""
        return self._process is not None and self._process.poll() is None

    def start(self) -> None:
        """
        Start the traceview server.

        Raises:
            TraceviewError: If the server fails to start or port is in use.
        """
        if self.is_running:
            return

        if not self._is_port_available():
            raise TraceviewError(
                f"Port {self.config.port} is already in use. "
                "Either stop the existing process or use a different port."
            )

        binary = get_binary_path()
        cmd = [
            str(binary),
            "serve",
            "--port",
            str(self.config.port),
            "--db",
            self.config.db_path,
            "--batch-size",
            str(self.config.batch_size),
            "--batch-interval-ms",
            str(self.config.batch_interval_ms),
        ]

        self._process = subprocess.Popen(
            cmd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=(sys.platform != "win32"),
            creationflags=(
                subprocess.CREATE_NEW_PROCESS_GROUP if sys.platform == "win32" else 0
            ),
        )

        atexit.register(self._cleanup)

        if not self._wait_for_ready():
            self.stop()
            raise TraceviewError(
                f"Server failed to start within {self.config.startup_timeout}s. "
                "Check if another process is using the port."
            )

        if self.config.configure_otel:
            self._setup_otel()

    def stop(self) -> None:
        """
        Stop the traceview server.

        Gracefully terminates the server process. If it doesn't stop
        within a timeout, forcefully kills it.
        """
        if self._otel_cleanup:
            self._otel_cleanup()
            self._otel_cleanup = None

        if self._process is None:
            return

        try:
            if sys.platform == "win32":
                self._process.terminate()
            else:
                os.killpg(os.getpgid(self._process.pid), signal.SIGTERM)

            try:
                self._process.wait(timeout=2.0)
            except subprocess.TimeoutExpired:
                if sys.platform == "win32":
                    self._process.kill()
                else:
                    os.killpg(os.getpgid(self._process.pid), signal.SIGKILL)
                self._process.wait(timeout=1.0)
        except (ProcessLookupError, OSError):
            pass
        finally:
            self._process = None
            atexit.unregister(self._cleanup)

    def _cleanup(self) -> None:
        """Cleanup handler for atexit."""
        self.stop()

    def _is_port_available(self) -> bool:
        """Check if the configured port is available."""
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            try:
                s.bind(("127.0.0.1", self.config.port))
                return True
            except OSError:
                return False

    def _wait_for_ready(self) -> bool:
        """
        Wait for the server to become ready.

        Polls the server endpoint until it responds or timeout is reached.
        """
        start = time.monotonic()
        while time.monotonic() - start < self.config.startup_timeout:
            if self._process is None or self._process.poll() is not None:
                return False

            try:
                req = urllib.request.Request(self.config.endpoint, method="HEAD")
                with urllib.request.urlopen(req, timeout=0.5):
                    return True
            except (urllib.error.URLError, ConnectionError, TimeoutError):
                time.sleep(0.1)

        return False

    def _setup_otel(self) -> None:
        """
        Configure OpenTelemetry to send traces to this server.

        This sets up a TracerProvider with an OTLP HTTP exporter pointing
        to the local traceview server. If a session_id is configured,
        it adds a resource attribute for session grouping.
        """
        try:
            from opentelemetry import trace
            from opentelemetry.exporter.otlp.proto.http.trace_exporter import (
                OTLPSpanExporter,
            )
            from opentelemetry.sdk.resources import SERVICE_NAME, Resource
            from opentelemetry.sdk.trace import TracerProvider
            from opentelemetry.sdk.trace.export import BatchSpanProcessor
        except ImportError as e:
            raise TraceviewError(
                "OpenTelemetry packages not installed. "
                "Install with: pip install traceview[otel]"
            ) from e

        resource_attrs = {
            SERVICE_NAME: "traceview-client",
        }
        if self.config.session_id:
            resource_attrs["session.id"] = self.config.session_id

        resource = Resource.create(resource_attrs)

        exporter = OTLPSpanExporter(endpoint=self.config.traces_endpoint)

        provider = TracerProvider(resource=resource)
        processor = BatchSpanProcessor(exporter)
        provider.add_span_processor(processor)

        trace.set_tracer_provider(provider)

        def cleanup() -> None:
            provider.force_flush()
            provider.shutdown()

        self._otel_cleanup = cleanup

    def __enter__(self) -> Traceview:
        """Start the server when entering context."""
        self.start()
        return self

    def __exit__(
        self,
        _exc_type: type[BaseException] | None,
        _exc_val: BaseException | None,
        _exc_tb: TracebackType | None,
    ) -> None:
        """Stop the server when exiting context."""
        self.stop()


_instance: Traceview | None = None


def init(
    port: int = 4318,
    configure_otel: bool = True,
    session_id: str | None = None,
    db_path: str = "traces.db",
) -> Traceview:
    """
    Start traceview server in background. Auto-stops on process exit.

    Example:
        import traceview
        traceview.init()
        # traces now sent to http://localhost:4318
    """
    global _instance
    if _instance is not None and _instance.is_running:
        return _instance
    _instance = Traceview(
        port=port,
        db_path=db_path,
        configure_otel=configure_otel,
        session_id=session_id,
    )
    _instance.start()
    return _instance
