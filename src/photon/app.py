"""Application lifecycle & entrypoint (GUI / headless)."""

from __future__ import annotations

import argparse
import contextlib
import os
import signal
import sys
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from photon.automation.server import AutomationServer


def run(argv: list[str] | None = None) -> int:
    """Run photon application.

    Args:
        argv: Command line arguments (defaults to sys.argv[1:])

    Returns:
        Exit code
    """
    parser = argparse.ArgumentParser(
        prog="photon",
        description="Automation-first image viewer/editor",
    )
    parser.add_argument("path", nargs="?", help="Image file or folder to open")
    parser.add_argument(
        "--headless",
        action="store_true",
        help="Run without GUI (automation server only)",
    )
    parser.add_argument(
        "--socket",
        type=str,
        default=None,
        help="Unix socket path (default: ~/.cache/photon/photon.sock)",
    )
    parser.add_argument(
        "--signal-file",
        type=str,
        default=None,
        help="Watch this file for JSON commands (alternative to socket)",
    )
    parser.add_argument(
        "--lang",
        type=str,
        default=None,
        help="Language code (en, vi). Default: PHOTON_LANG env or 'en'",
    )
    parser.add_argument(
        "--log-level",
        choices=["DEBUG", "INFO", "WARNING", "ERROR"],
        default="INFO",
        help="Log level (default: INFO)",
    )

    args = parser.parse_args(argv)

    # Import here to avoid Qt import in headless-only mode
    from photon.automation.server import AutomationServer
    from photon.core.log import setup_logging
    from photon.i18n import setup_i18n

    # Setup logging
    setup_logging(level=args.log_level)

    # Setup i18n
    setup_i18n(args.lang)

    # Determine socket path
    socket_path = args.socket or _default_socket_path()

    # Create automation server
    server = AutomationServer(socket_path=socket_path, signal_file=args.signal_file)

    # Make the CLI flag authoritative for state reported by `app.info`; without
    # this, `--headless` still reported headless=false unless PHOTON_HEADLESS
    # happened to be exported in the environment.
    if args.headless:
        os.environ["PHOTON_HEADLESS"] = "1"
        return _run_headless(server, args)
    else:
        os.environ.pop("PHOTON_HEADLESS", None)
        return _run_gui(server, args)


def _run_headless(server: AutomationServer, args: argparse.Namespace) -> int:
    """Run headless mode (no GUI, automation server only)."""
    import time

    from photon.core.events import EventBus

    print(f"[photon] Headless mode, socket: {server.socket_path}", file=sys.stderr)
    print(f"[photon] PID: {server.pid}", file=sys.stderr)

    # Start server
    server.start()

    # Setup signal handlers
    def shutdown(signum: int, frame: object) -> None:
        with contextlib.suppress(BrokenPipeError, OSError):
            print(f"\n[photon] Received signal {signum}, shutting down...", file=sys.stderr)
        server.stop()
        EventBus.emit("app.quit", {"reason": "signal", "signum": signum})
        sys.exit(0)

    signal.signal(signal.SIGINT, shutdown)
    signal.signal(signal.SIGTERM, shutdown)

    # Main loop
    try:
        while True:
            time.sleep(1.0)
    except KeyboardInterrupt:
        shutdown(signal.SIGINT, None)

    return 0


def _run_gui(server: AutomationServer, args: argparse.Namespace) -> int:
    """Run GUI mode with Qt."""
    from PySide6.QtCore import QTimer
    from PySide6.QtWidgets import QApplication

    from photon.core.events import EventBus
    from photon.scaling import apply_stylesheet, detect_theme, setup_scaling
    from photon.ui.window import MainWindow

    # Setup scaling (before QApplication)
    scaling_config = setup_scaling()

    # Create Qt application
    app = QApplication(sys.argv[:1])
    app.setApplicationName("Photon")
    app.setOrganizationName("photon")

    # Apply stylesheet
    theme = detect_theme()
    stylesheet = apply_stylesheet(theme=theme, font_scale=scaling_config.get("font_scale", 1.0))
    app.setStyleSheet(stylesheet)

    # Start automation server
    server.start()

    # Create main window
    window = MainWindow()
    window.show()

    # Set window reference in API
    server.api.set_window(window)
    window._automation_runner = server.registry.execute

    # Open initial file if provided
    if args.path:
        window.open_path(Path(args.path))

    # Setup Ctrl+C handler (Qt blocks SIGINT by default)
    def handle_sigint() -> None:
        print("\n[photon] Ctrl+C received, shutting down...", file=sys.stderr)
        app.quit()

    # Use QTimer to poll for SIGINT (Qt event loop blocks signal handlers)
    signal.signal(signal.SIGINT, lambda s, f: QTimer.singleShot(0, handle_sigint))
    timer = QTimer()
    timer.timeout.connect(lambda: None)  # Keep event loop alive
    timer.start(100)

    # Cleanup on quit
    def cleanup() -> None:
        EventBus.emit("app.quit", {"reason": "user"})
        server.stop()

    app.aboutToQuit.connect(cleanup)

    # Run event loop
    return app.exec()


def _default_socket_path() -> str:
    """Get default socket path."""
    cache_dir = Path.home() / ".cache" / "photon"
    cache_dir.mkdir(mode=0o700, exist_ok=True)
    return str(cache_dir / "photon.sock")


if __name__ == "__main__":
    sys.exit(run())
