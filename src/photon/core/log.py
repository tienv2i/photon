"""Structured logging, ring buffer, crash hook."""

from __future__ import annotations

import json
import sys
import traceback
from collections import deque
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

# Ring buffer for last N events (queryable via automation)
_ring_buffer: deque[dict[str, Any]] = deque(maxlen=2000)

# Log file path (set by setup_logging)
_log_path: Path | None = None

# Current log level
_level = "INFO"
_levels = {"DEBUG": 0, "INFO": 1, "WARNING": 2, "ERROR": 3}


def setup_logging(level: str = "INFO", log_path: Path | None = None) -> Path:
    """Setup logging.

    Args:
        level: Log level (DEBUG, INFO, WARNING, ERROR)
        log_path: Path to log file (default: ~/.cache/photon/photon.log)

    Returns:
        Path to log file
    """
    global _log_path, _level

    _level = level
    if log_path is None:
        cache_dir = Path.home() / ".cache" / "photon"
        cache_dir.mkdir(mode=0o700, exist_ok=True)
        log_path = cache_dir / "photon.log"

    _log_path = log_path

    # Install crash hook
    sys.excepthook = _crash_hook  # type: ignore[assignment]

    return log_path


def log_event(
    event: str,
    meta: dict[str, Any] | None = None,
    status: str = "ok",
    duration_ms: float | None = None,
    level: str = "INFO",
) -> dict[str, Any]:
    """Log an event.

    Args:
        event: Event name (e.g., "view.rotate", "toolbar.clicked")
        meta: Metadata to include
        status: Status ("ok", "error", "pending")
        duration_ms: Duration in milliseconds
        level: Log level

    Returns:
        The logged event dict
    """
    entry: dict[str, Any] = {
        "event": event,
        "timestamp": datetime.now(UTC).isoformat(),
        "level": level,
        "status": status,
    }

    if meta:
        entry["meta"] = meta

    if duration_ms is not None:
        entry["duration_ms"] = round(duration_ms, 2)

    # Add to ring buffer
    _ring_buffer.append(entry)

    # Write to file if level >= current level
    if _levels.get(level, 1) >= _levels.get(_level, 1):
        _write_to_file(entry)

    return entry


def get_recent_events(n: int = 50) -> list[dict[str, Any]]:
    """Get recent events from ring buffer.

    Args:
        n: Number of events to return (most recent last)

    Returns:
        List of event dicts
    """
    events = list(_ring_buffer)
    return events[-n:] if n < len(events) else events


def get_errors(n: int = 10) -> list[dict[str, Any]]:
    """Get recent error events.

    Args:
        n: Number of errors to return

    Returns:
        List of error event dicts
    """
    errors = [e for e in _ring_buffer if e.get("level") == "ERROR" or e.get("status") == "error"]
    return errors[-n:] if n < len(errors) else errors


def _write_to_file(entry: dict[str, Any]) -> None:
    """Write event to log file (JSON lines)."""
    if _log_path is None:
        return

    try:
        with open(_log_path, "a", encoding="utf-8") as f:
            f.write(json.dumps(entry, ensure_ascii=False) + "\n")
    except OSError:
        # Silently fail if can't write to log file
        pass


def _crash_hook(exc_type: type, exc_value: Exception, exc_tb: Any) -> None:
    """Global exception hook — dump traceback to crash log."""
    # Log to ring buffer
    log_event(
        "app.crashed",
        meta={
            "type": exc_type.__name__,
            "message": str(exc_value),
            "traceback": "".join(traceback.format_exception(exc_type, exc_value, exc_tb)),
        },
        status="error",
        level="ERROR",
    )

    # Write to crash log file
    cache_dir = Path.home() / ".cache" / "photon"
    cache_dir.mkdir(mode=0o700, exist_ok=True)
    crash_path = cache_dir / f"crash-{datetime.now().strftime('%Y%m%d-%H%M%S')}.log"

    try:
        with open(crash_path, "w", encoding="utf-8") as f:
            f.write(f"Exception: {exc_type.__name__}: {exc_value}\n\n")
            f.write("".join(traceback.format_exception(exc_type, exc_value, exc_tb)))
    except OSError:
        pass

    # Call original excepthook
    sys.__excepthook__(exc_type, exc_value, exc_tb)
