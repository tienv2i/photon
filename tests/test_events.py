"""Tests for EventBus."""

from __future__ import annotations

from photon.core.events import EventBus


def test_emit_and_on() -> None:
    """Test emitting and subscribing to events."""
    EventBus.clear()

    received = []

    def callback(event: str, meta: dict) -> None:
        received.append((event, meta))

    EventBus.on("test.event", callback)
    EventBus.emit("test.event", {"key": "value"})

    assert len(received) == 1
    assert received[0][0] == "test.event"
    assert received[0][1]["key"] == "value"

    EventBus.clear()


def test_wildcard_subscription() -> None:
    """Test wildcard subscription."""
    EventBus.clear()

    received = []

    def callback(event: str, meta: dict) -> None:
        received.append(event)

    EventBus.on("*", callback)

    EventBus.emit("event.a")
    EventBus.emit("event.b")

    assert len(received) == 2
    assert received == ["event.a", "event.b"]

    EventBus.clear()


def test_off() -> None:
    """Test unsubscribing."""
    EventBus.clear()

    received = []

    def callback(event: str, meta: dict) -> None:
        received.append(event)

    EventBus.on("test.event", callback)
    EventBus.emit("test.event")
    EventBus.off("test.event", callback)
    EventBus.emit("test.event")

    assert len(received) == 1

    EventBus.clear()
