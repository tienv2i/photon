"""XDG Desktop Portal file dialogs via D-Bus.

Uses org.freedesktop.portal.Desktop / FileDialog:
- OpenFile(parent_window, options) -> handle token
- SaveFile(parent_window, options) -> handle token
- Response signal with selected paths

Portal D-Bus calls run on a background thread so the UI thread is never blocked
>50 ms. The ``Response`` signal is listened to asynchronously. Falls back to
QFileDialog if the portal is unavailable.

Debug logging: every portal request/response is emitted via ``log_event`` so it
can be inspected with ``photon-ctl log.tail`` / ``log.errors``.
"""

from __future__ import annotations

import contextlib
import os
import threading
import uuid
from typing import Any
from urllib.parse import unquote

from photon.core.errors import PhotonError
from photon.core.events import EventBus
from photon.core.log import log_event
from photon.i18n import t


def _log_debug(event: str, meta: dict[str, Any]) -> None:
    """Emit a debug log event (safe even if logging not yet configured)."""
    log_event(event, meta=meta, status="ok", level="DEBUG")


def _unwrap(value: Any) -> Any:
    """Unwrap a jeepney Variant value.

    jeepney decodes ``a{sv}`` entries as ``(signature, value)`` tuples, e.g.
    ``('as', ['file:///tmp/a.png'])``. Plain values are returned unchanged so
    this also works with hand-built dicts in tests.
    """
    if isinstance(value, tuple) and len(value) == 2 and isinstance(value[0], str):
        return value[1]
    return value


class PortalError(PhotonError):
    """Portal D-Bus error."""

    def __init__(self, message: str):
        super().__init__("PHOTON-PORTAL-401", message)


class PortalFileDialog:
    """XDG Desktop Portal FileDialog via D-Bus.

    Portal D-Bus calls are executed on a background thread. The ``Response``
    signal is listened to asynchronously so the UI thread is never blocked.
    Falls back to QFileDialog if the portal is unavailable.
    """

    def __init__(self) -> None:
        self._handle_counter = 0
        self._pending: dict[str, dict[str, Any]] = {}
        self._lock = threading.Lock()
        self._dbus_available: bool | None = None

    # ------------------------------------------------------------------ public

    def open_file(
        self,
        parent_window: str = "",
        title: str | None = None,
        filters: list[str] | None = None,
    ) -> str:
        """Open file dialog via portal (XDG preferred, Qt fallback).

        Args:
            parent_window: Parent window handle (unused by portal).
            title: Dialog title (default: localized).
            filters: File filters (e.g., ["*.jpg", "*.png"]).

        Returns:
            Handle token for polling result.
        """
        if title is None:
            title = t("dialog.open.title")

        with self._lock:
            self._handle_counter += 1
            handle = f"photon-dialog-{self._handle_counter}"

        self._pending[handle] = {
            "type": "open",
            "title": title,
            "filters": list(filters or []),
            "status": "pending",
            "paths": [],
        }

        _log_debug(
            "portal.dialog.requested",
            {"handle": handle, "type": "open", "title": title, "filters": list(filters or [])},
        )

        # Try XDG portal first (on a worker thread)
        if self._check_dbus():
            threading.Thread(
                target=self._open_via_portal_async,
                args=(handle, title, list(filters or [])),
                daemon=True,
                name=f"photon-portal-open-{handle}",
            ).start()
            return handle

        # Fallback to QFileDialog
        self._open_via_qt(handle, title, filters)
        return handle

    def save_file(
        self,
        parent_window: str = "",
        title: str | None = None,
        filters: list[str] | None = None,
        current_name: str = "",
    ) -> str:
        """Save file dialog via portal (XDG preferred, Qt fallback)."""
        if title is None:
            title = t("dialog.save.title")

        with self._lock:
            self._handle_counter += 1
            handle = f"photon-dialog-{self._handle_counter}"

        self._pending[handle] = {
            "type": "save",
            "title": title,
            "filters": list(filters or []),
            "current_name": current_name,
            "status": "pending",
            "paths": [],
        }

        _log_debug(
            "portal.dialog.requested",
            {"handle": handle, "type": "save", "title": title, "filters": list(filters or [])},
        )

        if self._check_dbus():
            threading.Thread(
                target=self._save_via_portal_async,
                args=(handle, title, list(filters or []), current_name),
                daemon=True,
                name=f"photon-portal-save-{handle}",
            ).start()
            return handle

        self._save_via_qt(handle, title, filters, current_name)
        return handle

    def get_result(self, handle: str) -> dict[str, Any]:
        """Get dialog result.

        Args:
            handle: Handle token from open_file/save_file.

        Returns:
            {"status": "pending"|"done"|"cancelled", "paths": [...]}

        Raises:
            PortalError: If handle not found.
        """
        with self._lock:
            entry = self._pending.get(handle)
            if entry is None:
                raise PortalError(f"Unknown dialog handle: {handle}")
            return {
                "status": entry["status"],
                "paths": list(entry["paths"]),
            }

    # --------------------------------------------------------------- dbus check

    def _check_dbus(self) -> bool:
        """Check if D-Bus session bus and portal are available."""
        if self._dbus_available is not None:
            return self._dbus_available

        bus_addr = os.environ.get("DBUS_SESSION_BUS_ADDRESS")
        if not bus_addr:
            _log_debug(
                "portal.dbus.unavailable",
                {"reason": "no DBUS_SESSION_BUS_ADDRESS"},
            )
            self._dbus_available = False
            return False

        try:
            from jeepney import DBusAddress, new_method_call
            from jeepney.io.blocking import open_dbus_connection

            conn = open_dbus_connection(bus=bus_addr)
            dbus_addr = DBusAddress(
                "/org/freedesktop/DBus",
                bus_name="org.freedesktop.DBus",
                interface="org.freedesktop.DBus",
            )
            reply = conn.send_and_get_reply(
                new_method_call(
                    dbus_addr,
                    "NameHasOwner",
                    "s",
                    ("org.freedesktop.portal.Desktop",),
                )
            )
            has_owner = bool(reply.body[0]) if reply.body else False
            if not has_owner:
                reply_act = conn.send_and_get_reply(
                    new_method_call(dbus_addr, "ListActivatableNames", "", ())
                )
                activatable = (
                    "org.freedesktop.portal.Desktop" in reply_act.body[0]
                    if reply_act.body
                    else False
                )
                self._dbus_available = activatable
            else:
                self._dbus_available = True
            conn.close()

            if self._dbus_available:
                _log_debug("portal.dbus.available", {})
            else:
                _log_debug("portal.dbus.unavailable", {"reason": "portal not available"})
        except Exception as e:  # noqa: BLE001
            _log_debug("portal.dbus.unavailable", {"reason": str(e)})
            self._dbus_available = False

        return self._dbus_available

    # ----------------------------------------------------------- portal async

    def _open_via_portal_async(self, handle: str, title: str, filters: list[str]) -> None:
        """Run OpenFile portal call + Response listener on a worker thread."""
        from jeepney import DBusAddress, new_method_call
        from jeepney.bus_messages import message_bus
        from jeepney.io.blocking import open_dbus_connection
        from jeepney.low_level import HeaderFields, MessageType

        portal_addr = DBusAddress(
            "/org/freedesktop/portal/desktop",
            bus_name="org.freedesktop.portal.Desktop",
            interface="org.freedesktop.portal.FileChooser",
        )

        token = f"photon_{uuid.uuid4().hex[:8]}"

        options: dict[str, Any] = {
            "handle_token": ("s", token),
        }
        if filters:
            filter_patterns = [(0, f) for f in filters]
            options["filters"] = (
                "a(sa(us))",
                [(title or "Images", filter_patterns)],
            )

        _log_debug(
            "portal.openfile.call",
            {"handle": handle, "token": token},
        )

        conn = None
        try:
            conn = open_dbus_connection()

            # Subscribe to Response signal
            conn.send_and_get_reply(
                message_bus.AddMatch(
                    "type='signal',interface='org.freedesktop.portal.Request',member='Response'"
                )
            )

            msg = new_method_call(
                portal_addr,
                "OpenFile",
                "ssa{sv}",
                ("", title, options),
            )
            reply = conn.send_and_get_reply(msg)
            request_handle = reply.body[0]

            _log_debug(
                "portal.openfile.sent",
                {"handle": handle, "request_handle": request_handle},
            )

            # Listen for the Response signal on the request path
            while True:
                try:
                    msg = conn.receive()
                except Exception:
                    break

                if (
                    msg.header.message_type == MessageType.signal
                    and msg.header.fields.get(HeaderFields.path) == request_handle
                    and msg.header.fields.get(HeaderFields.member) == "Response"
                ):
                    body: tuple[Any, ...] = msg.body or ()
                    response_code: int = body[0] if len(body) > 0 else 0
                    results: Any = body[1] if len(body) > 1 else {}

                    paths: list[str] = []
                    if isinstance(results, dict):
                        uris = _unwrap(results.get("uris", []))
                        if not isinstance(uris, list):
                            uris = [uris] if isinstance(uris, str) else []
                        for uri in uris:
                            if isinstance(uri, str) and uri.startswith("file://"):
                                paths.append(unquote(uri[len("file://") :]))

                    with self._lock:
                        entry = self._pending.get(handle)
                        if entry is not None:
                            entry["status"] = "done" if response_code == 0 else "cancelled"
                            entry["paths"] = paths

                    _log_debug(
                        "portal.openfile.response",
                        {
                            "handle": handle,
                            "response_code": response_code,
                            "paths": paths,
                        },
                    )
                    status = entry["status"] if entry is not None else "cancelled"
                    EventBus.emit(
                        "dialog.result",
                        {"handle": handle, "status": status, "paths": paths},
                    )
                    break
        except Exception as e:  # noqa: BLE001
            log_event(
                "portal.openfile.failed",
                meta={"handle": handle, "error": str(e)},
                status="error",
                level="INFO",
            )
            # Let MainWindow fallback to QFileDialog on the GUI thread
            with self._lock:
                entry = self._pending.get(handle)
                if entry is not None:
                    entry["status"] = "pending"
            EventBus.emit(
                "dialog.portal_failed",
                {"handle": handle, "type": "open", "title": entry["title"] if entry else ""},
            )
        finally:
            if conn is not None:
                with contextlib.suppress(Exception):
                    conn.close()

    def _save_via_portal_async(
        self, handle: str, title: str, filters: list[str], current_name: str
    ) -> None:
        """Run SaveFile portal call + Response listener on a worker thread."""
        from jeepney import DBusAddress, new_method_call
        from jeepney.bus_messages import message_bus
        from jeepney.io.blocking import open_dbus_connection
        from jeepney.low_level import HeaderFields, MessageType

        portal_addr = DBusAddress(
            "/org/freedesktop/portal/desktop",
            bus_name="org.freedesktop.portal.Desktop",
            interface="org.freedesktop.portal.FileChooser",
        )

        token = f"photon_{uuid.uuid4().hex[:8]}"

        options: dict[str, Any] = {
            "handle_token": ("s", token),
        }
        if current_name:
            options["current_name"] = ("s", current_name)

        if filters:
            filter_patterns = [(0, f) for f in filters]
            options["filters"] = (
                "a(sa(us))",
                [(title or "Images", filter_patterns)],
            )

        _log_debug(
            "portal.savefile.call",
            {"handle": handle, "token": token},
        )

        conn = None
        try:
            conn = open_dbus_connection()

            conn.send_and_get_reply(
                message_bus.AddMatch(
                    "type='signal',interface='org.freedesktop.portal.Request',member='Response'"
                )
            )

            msg = new_method_call(
                portal_addr,
                "SaveFile",
                "ssa{sv}",
                ("", title, options),
            )
            reply = conn.send_and_get_reply(msg)
            request_handle = reply.body[0]

            _log_debug(
                "portal.savefile.sent",
                {"handle": handle, "request_handle": request_handle},
            )

            while True:
                try:
                    msg = conn.receive()
                except Exception:
                    break

                if (
                    msg.header.message_type == MessageType.signal
                    and msg.header.fields.get(HeaderFields.path) == request_handle
                    and msg.header.fields.get(HeaderFields.member) == "Response"
                ):
                    body: tuple[Any, ...] = msg.body or ()
                    response_code = body[0] if len(body) > 0 else 0
                    results = body[1] if len(body) > 1 else {}

                    paths: list[str] = []
                    if isinstance(results, dict):
                        uris = _unwrap(results.get("uris"))
                        if not isinstance(uris, list):
                            uris = [uris] if isinstance(uris, str) else []
                        for u in uris:
                            if isinstance(u, str) and u.startswith("file://"):
                                paths.append(unquote(u[len("file://") :]))
                        if not paths and "uri" in results:
                            u = _unwrap(results["uri"])
                            if isinstance(u, str) and u.startswith("file://"):
                                paths.append(unquote(u[len("file://") :]))

                    with self._lock:
                        entry = self._pending.get(handle)
                        if entry is not None:
                            entry["status"] = "done" if response_code == 0 else "cancelled"
                            entry["paths"] = paths

                    _log_debug(
                        "portal.savefile.response",
                        {
                            "handle": handle,
                            "response_code": response_code,
                            "paths": paths,
                        },
                    )
                    status = entry["status"] if entry is not None else "cancelled"
                    EventBus.emit(
                        "dialog.result",
                        {"handle": handle, "status": status, "paths": paths},
                    )
                    break
        except Exception as e:  # noqa: BLE001
            log_event(
                "portal.savefile.failed",
                meta={"handle": handle, "error": str(e)},
                status="error",
                level="INFO",
            )
            # Let MainWindow fallback to QFileDialog on the GUI thread
            with self._lock:
                entry = self._pending.get(handle)
                if entry is not None:
                    entry["status"] = "pending"
            EventBus.emit(
                "dialog.portal_failed",
                {
                    "handle": handle,
                    "type": "save",
                    "title": entry["title"] if entry is not None else "",
                    "current_name": entry.get("current_name", "") if entry is not None else "",
                },
            )
        finally:
            if conn is not None:
                with contextlib.suppress(Exception):
                    conn.close()

    # --------------------------------------------------------------- qt fallback

    def _open_via_qt(self, handle: str, title: str, filters: list[str] | None) -> None:
        """Open file dialog via QFileDialog (fallback)."""
        from PySide6.QtWidgets import QApplication, QFileDialog

        if QApplication.instance() is None:
            with self._lock:
                self._pending[handle]["status"] = "cancelled"
            EventBus.emit("dialog.result", {"handle": handle, "status": "cancelled", "paths": []})
            return

        if filters:
            filter_str = f"{t('dialog.filter.images')} ({' '.join(filters)})"
        else:
            filter_str = "Images (*.jpg *.jpeg *.png *.webp *.gif *.tiff *.bmp);;All files (*)"

        parent = None
        for widget in QApplication.topLevelWidgets():
            if widget.objectName() == "main_window":
                parent = widget
                break

        paths, _ = QFileDialog.getOpenFileNames(parent, title, "", filter_str)

        if paths:
            with self._lock:
                self._pending[handle]["status"] = "done"
                self._pending[handle]["paths"] = list(paths)
        else:
            with self._lock:
                self._pending[handle]["status"] = "cancelled"

        EventBus.emit(
            "dialog.result",
            {
                "handle": handle,
                "status": self._pending[handle]["status"],
                "paths": self._pending[handle]["paths"],
            },
        )

    def _save_via_qt(
        self,
        handle: str,
        title: str,
        filters: list[str] | None,
        current_name: str,
    ) -> None:
        """Save file dialog via QFileDialog (fallback)."""
        from PySide6.QtWidgets import QApplication, QFileDialog

        if QApplication.instance() is None:
            with self._lock:
                self._pending[handle]["status"] = "cancelled"
            EventBus.emit("dialog.result", {"handle": handle, "status": "cancelled", "paths": []})
            return

        if filters:
            filter_str = f"{t('dialog.filter.images')} ({' '.join(filters)})"
        else:
            filter_str = "PNG (*.png);;JPEG (*.jpg *.jpeg);;WebP (*.webp)"

        parent = None
        for widget in QApplication.topLevelWidgets():
            if widget.objectName() == "main_window":
                parent = widget
                break

        path, _ = QFileDialog.getSaveFileName(parent, title, current_name, filter_str)

        if path:
            with self._lock:
                self._pending[handle]["status"] = "done"
                self._pending[handle]["paths"] = [path]
        else:
            with self._lock:
                self._pending[handle]["status"] = "cancelled"

        EventBus.emit(
            "dialog.result",
            {
                "handle": handle,
                "status": self._pending[handle]["status"],
                "paths": self._pending[handle]["paths"],
            },
        )


# Global instance
portal_dialog = PortalFileDialog()
