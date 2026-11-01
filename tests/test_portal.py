"""Tests for XDG Desktop Portal file dialog."""

from __future__ import annotations

from unittest.mock import MagicMock, patch

from photon.portal.filedialog import PortalFileDialog


def test_portal_file_dialog_init() -> None:
    """Test PortalFileDialog initialisation."""
    dialog = PortalFileDialog()
    assert dialog._handle_counter == 0
    assert dialog._pending == {}


def test_portal_check_dbus() -> None:
    """Test D-Bus availability check."""
    dialog = PortalFileDialog()
    result = dialog._check_dbus()
    assert isinstance(result, bool)


def test_portal_open_file_creates_handle() -> None:
    """Test open_file registers pending handle.

    The Qt fallback is stubbed: with a QApplication alive it opens a real
    modal dialog and would block the whole test session.
    """
    dialog = PortalFileDialog()
    dialog._dbus_available = False

    with patch.object(PortalFileDialog, "_open_via_qt", autospec=True):
        handle = dialog.open_file(title="Open Test", filters=["*.png", "*.jpg"])

    assert handle.startswith("photon-dialog-")

    res = dialog.get_result(handle)
    assert res["status"] == "pending"


def test_portal_save_file_creates_handle() -> None:
    """Test save_file registers pending handle (Qt fallback stubbed)."""
    dialog = PortalFileDialog()
    dialog._dbus_available = False

    with patch.object(PortalFileDialog, "_save_via_qt", autospec=True):
        handle = dialog.save_file(title="Save Test", filters=["*.png"], current_name="sample.png")

    assert handle.startswith("photon-dialog-")

    res = dialog.get_result(handle)
    assert res["status"] == "pending"


def test_portal_open_async_success_parsing() -> None:
    """Test worker thread portal response parsing with url decoding."""
    from jeepney.low_level import HeaderFields, MessageType

    dialog = PortalFileDialog()
    handle = "photon-dialog-test-1"
    dialog._pending[handle] = {
        "type": "open",
        "title": "Open",
        "filters": ["*.png"],
        "status": "pending",
        "paths": [],
    }

    mock_conn = MagicMock()
    mock_reply = MagicMock()
    mock_reply.body = ("/org/freedesktop/portal/desktop/request/1_99/photon_test",)
    mock_conn.send_and_get_reply.return_value = mock_reply

    # Construct mock signal message
    mock_signal = MagicMock()
    mock_signal.header.message_type = MessageType.signal
    mock_signal.header.fields = {
        HeaderFields.path: "/org/freedesktop/portal/desktop/request/1_99/photon_test",
        HeaderFields.member: "Response",
    }
    # Real jeepney decodes a{sv} values as (signature, value) tuples.
    mock_signal.body = (0, {"uris": ("as", ["file:///path/to/My%20Image.png"])})
    mock_conn.receive.side_effect = [mock_signal]

    with patch("jeepney.io.blocking.open_dbus_connection", return_value=mock_conn):
        dialog._open_via_portal_async(handle, "Open", ["*.png"])

    res = dialog.get_result(handle)
    assert res["status"] == "done"
    assert res["paths"] == ["/path/to/My Image.png"]


def test_portal_save_async_success_parsing() -> None:
    """Test worker thread portal save response parsing."""
    from jeepney.low_level import HeaderFields, MessageType

    dialog = PortalFileDialog()
    handle = "photon-dialog-test-2"
    dialog._pending[handle] = {
        "type": "save",
        "title": "Save",
        "filters": ["*.png"],
        "current_name": "out.png",
        "status": "pending",
        "paths": [],
    }

    mock_conn = MagicMock()
    mock_reply = MagicMock()
    mock_reply.body = ("/org/freedesktop/portal/desktop/request/1_99/photon_save",)
    mock_conn.send_and_get_reply.return_value = mock_reply

    mock_signal = MagicMock()
    mock_signal.header.message_type = MessageType.signal
    mock_signal.header.fields = {
        HeaderFields.path: "/org/freedesktop/portal/desktop/request/1_99/photon_save",
        HeaderFields.member: "Response",
    }
    mock_signal.body = (0, {"uris": ("as", ["file:///path/to/saved%20image.png"])})
    mock_conn.receive.side_effect = [mock_signal]

    with patch("jeepney.io.blocking.open_dbus_connection", return_value=mock_conn):
        dialog._save_via_portal_async(handle, "Save", ["*.png"], "out.png")

    res = dialog.get_result(handle)
    assert res["status"] == "done"
    assert res["paths"] == ["/path/to/saved image.png"]


def test_portal_unwrap_variant_shape() -> None:
    """``a{sv}`` values arrive as (signature, value) tuples from jeepney."""
    from photon.portal.filedialog import _unwrap

    assert _unwrap(("as", ["file:///a.png"])) == ["file:///a.png"]
    assert _unwrap(("s", "file:///a.png")) == "file:///a.png"
    # Plain values (as used in hand-built dicts) pass through unchanged.
    assert _unwrap(["file:///a.png"]) == ["file:///a.png"]
    assert _unwrap(None) is None


def test_portal_open_async_single_uri_key() -> None:
    """Portal may return a scalar ``uris`` value; it must still be parsed."""
    from jeepney.low_level import HeaderFields, MessageType

    dialog = PortalFileDialog()
    handle = "photon-dialog-test-3"
    dialog._pending[handle] = {
        "type": "open",
        "title": "Open",
        "filters": ["*.png"],
        "status": "pending",
        "paths": [],
    }

    mock_conn = MagicMock()
    mock_reply = MagicMock()
    mock_reply.body = ("/org/freedesktop/portal/desktop/request/1_99/photon_one",)
    mock_conn.send_and_get_reply.return_value = mock_reply

    mock_signal = MagicMock()
    mock_signal.header.message_type = MessageType.signal
    mock_signal.header.fields = {
        HeaderFields.path: "/org/freedesktop/portal/desktop/request/1_99/photon_one",
        HeaderFields.member: "Response",
    }
    mock_signal.body = (0, {"uris": ("s", "file:///tmp/single.png")})
    mock_conn.receive.side_effect = [mock_signal]

    with patch("jeepney.io.blocking.open_dbus_connection", return_value=mock_conn):
        dialog._open_via_portal_async(handle, "Open", ["*.png"])

    res = dialog.get_result(handle)
    assert res["status"] == "done"
    assert res["paths"] == ["/tmp/single.png"]
