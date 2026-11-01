"""Command registry: <domain>.<verb> -> AutomationAPI method."""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from photon.automation.api import AutomationAPI
from photon.core.errors import UnknownCommandError


class CommandRegistry:
    """Registry mapping command names to API methods."""

    def __init__(self, api: AutomationAPI) -> None:
        self.api = api
        self._commands: dict[str, Callable[..., dict[str, Any]]] = {}
        self._register_all()

    def _register_all(self) -> None:
        """Register all commands."""
        api = self.api

        # App
        self._commands["app.info"] = api.app_info
        self._commands["app.doctor"] = api.app_doctor
        self._commands["app.selftest"] = api.app_selftest
        self._commands["app.quit"] = api.app_quit

        # File
        self._commands["file.open"] = api.file_open
        self._commands["file.next"] = api.file_next
        self._commands["file.prev"] = api.file_prev
        self._commands["file.info"] = api.file_info
        self._commands["file.save_copy"] = api.file_save_copy
        self._commands["file.delete"] = api.file_delete

        # View
        self._commands["view.zoom"] = api.view_zoom
        self._commands["view.pan"] = api.view_pan
        self._commands["frame.activate"] = api.frame_activate
        self._commands["frame.state"] = api.frame_state
        self._commands["view.fit"] = api.view_fit
        self._commands["view.actual"] = api.view_actual
        self._commands["view.rotate"] = api.view_rotate
        self._commands["view.flip"] = api.view_flip
        self._commands["view.reset"] = api.view_reset
        self._commands["view.fullscreen"] = api.view_fullscreen
        self._commands["view.compare"] = api.view_compare

        # Compare pane
        self._commands["compare.set_image"] = api.compare_set_image
        self._commands["compare.clear"] = api.compare_clear
        self._commands["compare.state"] = api.compare_state

        # Edit
        self._commands["edit.set"] = api.edit_set
        self._commands["edit.undo"] = api.edit_undo
        self._commands["edit.redo"] = api.edit_redo
        self._commands["edit.reset"] = api.edit_reset

        # Sidebar
        self._commands["sidebar.show"] = api.sidebar_show
        self._commands["sidebar.state"] = api.sidebar_state
        self._commands["sidebar.set_content"] = api.sidebar_set_content

        # Toolbar
        self._commands["toolbar.click"] = api.toolbar_click
        self._commands["toolbar.state"] = api.toolbar_state

        # Dialog
        self._commands["dialog.open"] = api.dialog_open
        self._commands["dialog.result"] = api.dialog_result

        # UI
        self._commands["shot"] = api.shot
        self._commands["ui.events"] = api.ui_events
        self._commands["ui.click"] = api.ui_click
        self._commands["ui.key"] = api.ui_key
        self._commands["window.state"] = api.window_state
        self._commands["window.resize"] = api.window_resize

        # Log
        self._commands["log.tail"] = api.log_tail
        self._commands["log.errors"] = api.log_errors

        # Wait (documented in AGENTS.md/docs, must exist)
        self._commands["wait"] = self._wait

    def _wait(
        self,
        condition: Any = None,
        seconds: float | None = None,
        timeout: float = 10.0,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Wait for a condition (getter comparison) or a fixed duration.

        Args:
            condition: expression like ``view.zoom == 1.0`` (uses log events)
            seconds: fixed seconds to sleep (ignored if condition given)
            timeout: maximum seconds to wait for a condition

        Returns:
            {"waited": seconds, "condition": ..., "met": bool}
        """
        import re
        import time

        started = time.time()

        # Fixed delay form
        if condition is None:
            delay = float(seconds) if seconds is not None else 0.0
            time.sleep(max(0.0, delay))
            return {"waited": round(time.time() - started, 3), "condition": None, "met": True}

        expr = str(condition).strip()

        # Numeric form: wait 5 (seconds)
        if re.fullmatch(r"\d+(\.\d+)?", expr):
            time.sleep(float(expr))
            return {
                "waited": round(time.time() - started, 3),
                "condition": expr,
                "met": True,
            }

        # Getter comparison form: "view.zoom == 1.0" resolved against live
        # app state (not log text) so it reflects actual usage.
        deadline = time.time() + max(0.0, float(timeout))
        m = re.fullmatch(r"([\w.]+)\s*(==|!=)\s*(.+)", expr)

        resolver = getattr(self.api, "resolve_getter", None)
        if resolver is None:
            return {
                "waited": round(time.time() - started, 3),
                "condition": expr,
                "met": False,
                "error": "getter resolution unavailable",
            }

        matched = False
        last_value: Any = None
        if m:
            path, op, raw_expected = m.group(1), m.group(2), m.group(3).strip().strip("'\"")
            while time.time() < deadline:
                try:
                    last_value = resolver(path)
                except KeyError:
                    return {
                        "waited": round(time.time() - started, 3),
                        "condition": expr,
                        "met": False,
                        "error": f"unknown getter: {path}",
                    }
                if isinstance(last_value, bool):
                    expected: Any = raw_expected.lower() == "true"
                else:
                    try:
                        expected = float(raw_expected) if "." in raw_expected else int(raw_expected)
                    except ValueError:
                        expected = raw_expected
                ok = last_value == expected if op == "==" else last_value != expected
                if ok:
                    matched = True
                    break
                time.sleep(0.1)
        else:
            return {
                "waited": round(time.time() - started, 3),
                "condition": expr,
                "met": False,
                "error": "unsupported condition format",
            }

        return {
            "waited": round(time.time() - started, 3),
            "condition": expr,
            "met": matched,
            "value": last_value,
        }

    def execute(self, cmd: str, args: dict[str, Any] | None = None) -> dict[str, Any]:
        """Execute a command.

        Args:
            cmd: Command name (e.g., "file.open")
            args: Command arguments

        Returns:
            Result dict from API method

        Raises:
            UnknownCommandError: If command not registered
        """
        if cmd not in self._commands:
            raise UnknownCommandError(cmd)

        if args is None:
            args = {}

        return self._commands[cmd](**args)

    @property
    def commands(self) -> list[str]:
        """List of registered command names."""
        return sorted(self._commands.keys())
