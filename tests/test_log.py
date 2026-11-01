"""Tests for core logging."""

from __future__ import annotations

from pathlib import Path

from photon.core.log import get_errors, get_recent_events, log_event, setup_logging


def test_setup_logging(tmp_path: Path) -> None:
    """Test logging setup."""
    log_path = setup_logging(level="DEBUG", log_path=tmp_path / "test.log")
    assert log_path.exists() or log_path.parent.exists()


def test_log_event(tmp_path: Path) -> None:
    """Test logging an event."""
    setup_logging(level="DEBUG", log_path=tmp_path / "test.log")

    entry = log_event("test.event", meta={"key": "value"}, status="ok")

    assert entry["event"] == "test.event"
    assert entry["meta"]["key"] == "value"
    assert entry["status"] == "ok"
    assert "timestamp" in entry


def test_ring_buffer(tmp_path: Path) -> None:
    """Test ring buffer."""
    setup_logging(level="DEBUG", log_path=tmp_path / "test.log")

    # Log multiple events
    for i in range(10):
        log_event(f"event.{i}", meta={"i": i})

    events = get_recent_events(n=5)
    assert len(events) == 5
    assert events[-1]["event"] == "event.9"


def test_get_errors(tmp_path: Path) -> None:
    """Test getting errors."""
    setup_logging(level="DEBUG", log_path=tmp_path / "test.log")

    log_event("normal.event", status="ok")
    log_event("error.event", status="error", level="ERROR")

    errors = get_errors()
    assert len(errors) >= 1
    assert errors[-1]["event"] == "error.event"
