"""Tests for scenario assertions (no app required)."""

from __future__ import annotations

from typing import Any

from photon.automation.scenario import ScenarioRunner


def _check(assertion: dict[str, Any], data: dict[str, Any]) -> str | None:
    runner = ScenarioRunner.__new__(ScenarioRunner)
    return runner._check_assertion(assertion, data)


def test_contains_operator() -> None:
    data = {"format": "JPEG", "bottom": ["bottom_zoom_in", "bottom_next"]}
    assert _check({"property": "format", "contains": "JPEG"}, data) is None
    assert _check({"property": "bottom", "contains": "bottom_next"}, data) is None
    assert _check({"property": "bottom", "contains": "bottom_delete"}, data) is not None


def test_not_contains_operator() -> None:
    data = {"bottom": ["bottom_zoom_in", "bottom_next"]}
    assert _check({"property": "bottom", "not_contains": "bottom_delete"}, data) is None
    error = _check({"property": "bottom", "not_contains": "bottom_next"}, data)
    assert error is not None
    assert "NOT to contain" in error


def test_list_equality_supports_toolbar_order() -> None:
    order = ["bottom_zoom_in", "bottom_zoom_out", "bottom_fit"]
    data = {"bottom": list(order)}
    assert _check({"property": "bottom", "equals": order}, data) is None
    assert _check({"property": "bottom", "equals": order[:-1]}, data) is not None


def test_not_equals_operator() -> None:
    data = {"events": [{"event": "x"}]}
    assert _check({"property": "events", "not_equals": []}, data) is None
    assert _check({"property": "events", "not_equals": [{"event": "x"}]}, data) is not None
