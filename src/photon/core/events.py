"""EventBus: in-memory signal + file journal."""

from __future__ import annotations

import contextlib
from collections.abc import Callable
from typing import Any

from photon.core.log import log_event

# Subscribers: {event_name: [callback, ...]}
_subscribers: dict[str, list[Callable[..., None]]] = {}

# Wildcard subscribers (receive all events)
_wildcard_subscribers: list[Callable[..., None]] = []


class EventBus:
    """Event bus for in-process communication.

    All UI actions, automation commands, and system events emit through here.
    """

    @staticmethod
    def emit(event: str, meta: dict[str, Any] | None = None, **kwargs: Any) -> None:
        """Emit an event.

        Args:
            event: Event name (e.g., "view.rotate", "toolbar.clicked")
            meta: Metadata dict
            **kwargs: Additional metadata (merged into meta)
        """
        if meta is None:
            meta = {}
        if kwargs:
            meta.update(kwargs)

        # Log to ring buffer + file
        log_event(event, meta=meta)

        # Notify subscribers
        for callback in _subscribers.get(event, []):
            with contextlib.suppress(Exception):
                callback(event, meta)

        # Notify wildcard subscribers
        for callback in _wildcard_subscribers:
            with contextlib.suppress(Exception):
                callback(event, meta)

    @staticmethod
    def on(event: str, callback: Callable[..., None]) -> None:
        """Subscribe to an event.

        Args:
            event: Event name, or "*" for all events
            callback: Function to call with (event, meta)
        """
        if event == "*":
            _wildcard_subscribers.append(callback)
        else:
            if event not in _subscribers:
                _subscribers[event] = []
            _subscribers[event].append(callback)

    @staticmethod
    def off(event: str, callback: Callable[..., None]) -> None:
        """Unsubscribe from an event.

        Args:
            event: Event name, or "*" for all events
            callback: Function to unsubscribe
        """
        if event == "*":
            if callback in _wildcard_subscribers:
                _wildcard_subscribers.remove(callback)
        else:
            if event in _subscribers and callback in _subscribers[event]:
                _subscribers[event].remove(callback)

    @staticmethod
    def clear() -> None:
        """Clear all subscribers (for testing)."""
        _subscribers.clear()
        _wildcard_subscribers.clear()
