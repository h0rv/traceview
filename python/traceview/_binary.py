"""
Binary location discovery for the traceview Rust binary.

The binary is packaged inside the wheel by maturin and placed in the
package's data directory. This module handles cross-platform binary
discovery.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path
from shutil import which


def get_binary_path() -> Path:
    """
    Find the path to the traceview binary.

    The binary location depends on how the package was installed:
    1. When installed from wheel: in the package's bin/scripts directory
    2. When running from source: use cargo build output

    Returns:
        Path to the 'tv' binary.

    Raises:
        FileNotFoundError: If the binary cannot be found.
    """
    binary_name = "tv.exe" if sys.platform == "win32" else "tv"

    # Strategy 1: Binary in the same directory as this module (maturin places it here)
    module_dir = Path(__file__).parent
    binary_in_module = module_dir / binary_name
    if binary_in_module.exists():
        return binary_in_module

    # Strategy 2: Check the scripts/bin directory of the virtual environment
    if sys.prefix != sys.base_prefix:
        if sys.platform == "win32":
            venv_binary = Path(sys.prefix) / "Scripts" / binary_name
        else:
            venv_binary = Path(sys.prefix) / "bin" / binary_name
        if venv_binary.exists():
            return venv_binary

    # Strategy 3: Check if 'tv' is on PATH
    path_binary = which(binary_name)
    if path_binary:
        return Path(path_binary)

    # Strategy 4: Development mode - try to find cargo target
    cargo_target = _find_cargo_binary()
    if cargo_target and cargo_target.exists():
        return cargo_target

    raise FileNotFoundError(
        f"Could not find traceview binary '{binary_name}'. "
        "Please ensure traceview is properly installed via pip."
    )


def _find_cargo_binary() -> Path | None:
    """
    Find the binary from cargo target directory (development mode).

    Looks for the binary in common cargo target locations relative
    to the source tree.
    """
    current = Path(__file__).parent
    for _ in range(5):
        cargo_toml = current / "Cargo.toml"
        if cargo_toml.exists():
            binary_name = "tv.exe" if sys.platform == "win32" else "tv"
            for profile in ["release", "debug"]:
                target_binary = current / "target" / profile / binary_name
                if target_binary.exists():
                    return target_binary
            break
        current = current.parent
    return None


def run_binary(
    *args: str,
    capture_output: bool = False,
    check: bool = False,
    timeout: float | None = None,
) -> subprocess.CompletedProcess[bytes]:
    """
    Run the traceview binary with the given arguments.

    This is a convenience wrapper around subprocess.run.

    Args:
        *args: Arguments to pass to the binary.
        capture_output: If True, capture stdout and stderr.
        check: If True, raise CalledProcessError on non-zero exit.
        timeout: Timeout in seconds.

    Returns:
        CompletedProcess instance with return code and output.

    Example:
        result = run_binary("serve", "--port", "8080")
        print(result.returncode)
    """
    binary = get_binary_path()
    return subprocess.run(
        [str(binary), *args],
        capture_output=capture_output,
        check=check,
        timeout=timeout,
    )
