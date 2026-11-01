"""Unix domain socket JSON-line server."""

from __future__ import annotations

import contextlib
import json
import os
import socket
import threading
from pathlib import Path
from typing import Any

from photon.automation.api import AutomationAPI
from photon.automation.commands import CommandRegistry
from photon.core import log


class AutomationServer:
    """Unix domain socket server for automation commands.

    Protocol: JSON lines
    Request: {"cmd": "file.open", "args": {"path": "/tmp/a.jpg"}}
    Response: {"ok": true, "data": {...}} or {"ok": false, "error": "PHOTON-XXX-NNN"}
    """

    def __init__(self, socket_path: str, signal_file: str | None = None) -> None:
        self.socket_path = socket_path
        self.signal_file = signal_file

        self.api = AutomationAPI()
        self.registry = CommandRegistry(self.api)

        self._server_socket: socket.socket | None = None
        self._running = False
        self._thread: threading.Thread | None = None
        self.pid = os.getpid()

        # Track start time
        import time

        self.api._start_time = time.time()

    @property
    def socket_path(self) -> str:
        """Socket path."""
        return self._socket_path

    @socket_path.setter
    def socket_path(self, value: str) -> None:
        self._socket_path = value

    def start(self) -> None:
        """Start server (non-blocking)."""
        if self._running:
            return

        # Remove old socket if exists
        socket_file = Path(self.socket_path)
        with contextlib.suppress(OSError):
            socket_file.unlink(missing_ok=True)

        # Create socket
        self._server_socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._server_socket.bind(self.socket_path)
        self._server_socket.listen(5)
        os.chmod(self.socket_path, 0o600)

        self._running = True
        self._thread = threading.Thread(target=self._accept_loop, daemon=True)
        self._thread.start()

        log.log_event("automation.server_started", {"socket": self.socket_path})

        # Start signal file watcher if configured
        if self.signal_file:
            threading.Thread(target=self._watch_signal_file, daemon=True).start()

    def stop(self) -> None:
        """Stop server."""
        if not self._running:
            return

        self._running = False

        if self._server_socket:
            with contextlib.suppress(OSError):
                self._server_socket.close()

        # Remove socket file (another shutdown path may have removed it already)
        socket_file = Path(self.socket_path)
        with contextlib.suppress(OSError):
            socket_file.unlink(missing_ok=True)

        log.log_event("automation.server_stopped", {})

    def _accept_loop(self) -> None:
        """Accept client connections."""
        while self._running:
            try:
                client, _ = self._server_socket.accept()  # type: ignore
                threading.Thread(target=self._handle_client, args=(client,), daemon=True).start()
            except OSError:
                # Socket closed
                break

    def _handle_client(self, client: socket.socket) -> None:
        """Handle a client connection (JSON lines)."""
        buffer = b""

        try:
            while self._running:
                data = client.recv(4096)
                if not data:
                    break

                buffer += data

                # Process complete lines
                while b"\n" in buffer:
                    line, buffer = buffer.split(b"\n", 1)
                    if not line.strip():
                        continue

                    response = self._process_line(line.decode("utf-8"))
                    client.sendall((json.dumps(response) + "\n").encode("utf-8"))
        except (OSError, ConnectionResetError):
            pass
        finally:
            with contextlib.suppress(OSError):
                client.close()

    def _process_line(self, line: str) -> dict[str, Any]:
        """Process a single JSON command line.

        Commands that touch Qt widgets are executed on the GUI thread.
        Automation connections are handled on worker threads, and mutating
        widgets from there corrupts Qt state (crashes, stale reads), so the
        call is marshalled with a blocking queued invocation. ``wait`` is
        excluded because it sleeps and must not block the UI.

        Args:
            line: JSON string

        Returns:
            Response dict
        """
        import time

        start = time.time()

        try:
            request = json.loads(line)
            cmd = request.get("cmd", "")
            args = request.get("args", {})

            result = self._execute(cmd, args)
            duration_ms = (time.time() - start) * 1000

            log.log_event(
                f"cmd.{cmd}",
                meta={"args": args},
                duration_ms=duration_ms,
            )

            return {"ok": True, "data": result}

        except json.JSONDecodeError as e:
            error = {"ok": False, "error": "PHOTON-CTL-204", "message": f"Invalid JSON: {e}"}
            log.log_event("cmd.invalid_json", meta={"error": str(e)}, status="error", level="ERROR")
            return error

        except Exception as e:
            # Get error code if PhotonError
            code = getattr(e, "code", "PHOTON-CTL-500")
            message = getattr(e, "message", str(e))

            log.log_event(
                "cmd.error",
                meta={"cmd": cmd if "cmd" in dir() else "?", "error": str(e)},
                status="error",
                level="ERROR",
            )

            return {"ok": False, "error": code, "message": message}

    def _execute(self, cmd: str, args: dict[str, Any]) -> dict[str, Any]:
        """Run a command, marshalling UI-touching work to the GUI thread."""
        window = self.api._window

        # Headless: nothing to marshal. ``wait`` sleeps, so keep it off the
        # GUI thread even when a window exists.
        if window is None or cmd == "wait":
            return self.registry.execute(cmd, args)

        try:
            from PySide6.QtCore import Q_ARG, QMetaObject, Qt
        except ImportError:  # pragma: no cover - GUI build always has Qt
            return self.registry.execute(cmd, args)

        window._automation_response = None
        window._automation_error = None
        QMetaObject.invokeMethod(
            window,
            "_execute_automation_command",
            Qt.ConnectionType.BlockingQueuedConnection,
            Q_ARG(str, cmd),
            Q_ARG("QVariantMap", args),
        )

        error = getattr(window, "_automation_error", None)
        if error is not None:
            raise error

        response = getattr(window, "_automation_response", None)
        return response if response is not None else {}

    def _watch_signal_file(self) -> None:
        """Watch signal file for JSON commands (alternative to socket)."""
        signal_path = Path(self.signal_file)  # type: ignore

        # Create empty file if not exists
        if not signal_path.exists():
            signal_path.touch()

        # Track file position
        position = 0

        while self._running:
            try:
                with open(signal_path, encoding="utf-8") as f:
                    f.seek(position)
                    for line in f:
                        line = line.strip()
                        if line:
                            self._process_line(line)
                    position = f.tell()
            except OSError:
                pass

            import time

            time.sleep(0.1)


# Global server instance (set by app.py)
_server: AutomationServer | None = None


def get_server() -> AutomationServer | None:
    """Get global server instance."""
    return _server


def set_server(server: AutomationServer) -> None:
    """Set global server instance."""
    global _server
    _server = server
