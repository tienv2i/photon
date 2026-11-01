"""photon-ctl command line tool."""

from __future__ import annotations

import argparse
import json
import socket
import sys
from pathlib import Path
from typing import Any


def send_command(
    socket_path: str,
    cmd: str,
    args: dict[str, Any] | None = None,
    timeout: float = 10.0,
) -> dict[str, Any]:
    """Send command to running photon instance.

    Args:
        socket_path: Unix socket path
        cmd: Command name
        args: Command arguments
        timeout: Socket timeout in seconds

    Returns:
        Response dict

    Raises:
        ConnectionRefusedError: If app not running
        socket.timeout: If response timeout
    """
    request = json.dumps({"cmd": cmd, "args": args or {}}) + "\n"

    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.settimeout(timeout)

    try:
        sock.connect(socket_path)
        sock.sendall(request.encode("utf-8"))

        # Read response (until newline)
        buffer = b""
        while b"\n" not in buffer:
            data = sock.recv(4096)
            if not data:
                break
            buffer += data

        if not buffer:
            return {"ok": False, "error": "PHOTON-CTL-205", "message": "Empty response"}

        return json.loads(buffer.decode("utf-8"))

    finally:
        sock.close()


def get_default_socket() -> str:
    """Get default socket path."""
    import os

    return os.environ.get("PHOTON_SOCKET", str(Path.home() / ".cache" / "photon" / "photon.sock"))


def main() -> int:
    """Main entry point for photon-ctl."""
    parser = argparse.ArgumentParser(
        prog="photon-ctl",
        description="Control running photon instance",
    )
    parser.add_argument("command", nargs="?", help="Command to execute (e.g., app.info)")
    parser.add_argument("args", nargs="?", help="JSON arguments")
    parser.add_argument("--socket", default=None, help="Socket path")
    parser.add_argument("--script", default=None, help="Run YAML scenario file")
    parser.add_argument("--out", default=None, help="Output directory for scenario results")
    parser.add_argument("--timeout", type=float, default=10.0, help="Command timeout (seconds)")

    args = parser.parse_args()

    socket_path = args.socket or get_default_socket()

    # Scenario mode
    if args.script:
        return _run_scenario(args.script, socket_path, args.out)

    # Command mode
    if not args.command:
        parser.print_help()
        return 1

    cmd_args: dict[str, Any] = {}
    if args.args:
        try:
            cmd_args = json.loads(args.args)
        except json.JSONDecodeError as e:
            print(f"Error: Invalid JSON args: {e}", file=sys.stderr)
            return 1

    try:
        response = send_command(socket_path, args.command, cmd_args, timeout=args.timeout)
        print(json.dumps(response, indent=2, ensure_ascii=False))
        return 0 if response.get("ok") else 1

    except (ConnectionRefusedError, FileNotFoundError):
        print(f"Error: Cannot connect to photon at {socket_path}", file=sys.stderr)
        print("Is photon running? Start with: uv run photon", file=sys.stderr)
        return 2

    except TimeoutError:
        print(f"Error: Timeout waiting for response ({args.timeout}s)", file=sys.stderr)
        return 3


def _run_scenario(script_path: str, socket_path: str, out_dir: str | None) -> int:
    """Run YAML scenario.

    Args:
        script_path: Path to scenario YAML
        socket_path: Socket path
        out_dir: Output directory for results

    Returns:
        Exit code (0 = all passed, 1 = failures)
    """
    from photon.automation.scenario import run_scenario

    try:
        results = run_scenario(script_path, socket_path, out_dir)
    except FileNotFoundError:
        print(f"Error: Scenario not found: {script_path}", file=sys.stderr)
        return 1
    except Exception as e:  # noqa: BLE001
        print(f"Error: Scenario failed to run: {e}", file=sys.stderr)
        return 1

    return 0 if results.get("failed", 1) == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
