"""Tests for automation API."""

from __future__ import annotations

from pathlib import Path

from photon.automation.api import AutomationAPI
from photon.automation.commands import CommandRegistry


def test_api_app_info() -> None:
    """Test app.info command."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    result = registry.execute("app.info")

    assert "version" in result
    assert "pid" in result
    assert "platform" in result


def test_api_view_rotate() -> None:
    """Test view.rotate command."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    result = registry.execute("view.rotate", {"degrees": 90})

    assert result["rotation"] == 90


def test_api_sidebar_show() -> None:
    """Test sidebar.show command."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    result = registry.execute("sidebar.show", {"side": "right", "visible": True})

    assert result["side"] == "right"
    assert result["visible"] is True


def test_unknown_command() -> None:
    """Test unknown command raises error."""
    from photon.core.errors import UnknownCommandError

    api = AutomationAPI()
    registry = CommandRegistry(api)

    try:
        registry.execute("unknown.command")
        raise AssertionError("Should have raised UnknownCommandError")
    except UnknownCommandError as e:
        assert "unknown.command" in str(e)


def test_command_list() -> None:
    """Test listing commands."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    commands = registry.commands

    assert "app.info" in commands
    assert "file.open" in commands
    assert "view.rotate" in commands
    assert len(commands) > 20


def test_edit_undo_redo_returns_state() -> None:
    """edit.undo/redo expose the restored transform state."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    registry.execute("edit.set", {"rotate": 90, "flip": "h"})
    result = registry.execute("edit.undo")
    assert result["undone"] is True
    assert result["rotation"] == 90
    assert result["flip_h"] is False

    result = registry.execute("edit.redo")
    assert result["redone"] is True
    assert result["flip_h"] is True


def test_edit_set_resize() -> None:
    """edit.set accepts the documented resize.max option."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    result = registry.execute("edit.set", {"resize": {"max": 1920}})
    assert result["resize_max"] == 1920


def test_save_copy_applies_edits(tmp_path) -> None:
    """save_copy must bake rotation/flip/resize into the saved file."""
    from PIL import Image

    src = tmp_path / "src.png"
    img = Image.new("RGB", (100, 50), "red")
    img.save(src)

    api = AutomationAPI()
    api.file_open(str(src))
    api.edit_set(rotate=90)

    out = tmp_path / "out.png"
    result = api.file_save_copy(path=str(out))

    assert (result["width"], result["height"]) == (50, 100)


def test_save_copy_flips_pixels(tmp_path) -> None:
    """A horizontal flip in the edit stack must mirror saved pixels."""
    from PIL import Image

    src = tmp_path / "src.png"
    img = Image.new("RGB", (100, 50), "red")
    for x in range(50, 100):
        for y in range(50):
            img.putpixel((x, y), (0, 0, 255))
    img.save(src)

    api = AutomationAPI()
    api.file_open(str(src))
    api.edit_set(flip="h")

    out = tmp_path / "out.png"
    api.file_save_copy(path=str(out))

    with Image.open(out) as saved:
        left = saved.convert("RGB").getpixel((10, 25))
    assert left[2] > 200  # blue moved to the left half


def test_save_copy_resizes(tmp_path) -> None:
    """resize.max limits the longest side on save."""
    from PIL import Image

    src = tmp_path / "src.png"
    Image.new("RGB", (100, 50), "red").save(src)

    api = AutomationAPI()
    api.file_open(str(src))
    api.edit_set(resize={"max": 50})

    out = tmp_path / "out.png"
    result = api.file_save_copy(path=str(out))

    assert (result["width"], result["height"]) == (50, 25)


def test_file_delete_moves_to_trash(tmp_path, monkeypatch) -> None:
    """file.delete moves the file to the freedesktop trash."""
    from PIL import Image

    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path / "data"))

    src = tmp_path / "src.png"
    Image.new("RGB", (10, 10), "red").save(src)

    api = AutomationAPI()
    api.file_open(str(src))
    result = api.file_delete()

    trashed = Path(result["trashed_to"])
    assert not src.exists()
    assert trashed.exists()
    assert trashed.parent == tmp_path / "data" / "Trash" / "files"

    info = tmp_path / "data" / "Trash" / "info" / f"{trashed.name}.trashinfo"
    assert info.exists()
    assert "[Trash Info]" in info.read_text(encoding="utf-8")
    assert api.doc.path is None


def test_trash_does_not_clobber(tmp_path, monkeypatch) -> None:
    """Two files with the same name must not overwrite each other."""
    from photon.core.trash import move_to_trash

    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path / "data"))

    first = tmp_path / "a" / "pic.png"
    second = tmp_path / "b" / "pic.png"
    for path in (first, second):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"x")

    dest1 = move_to_trash(first)
    dest2 = move_to_trash(second)

    assert dest1.name == "pic.png"
    assert dest2.name == "pic.1.png"
    assert dest1.exists()
    assert dest2.exists()


def test_resolve_getter_known_roots() -> None:
    """Getter paths resolve against live doc state."""
    api = AutomationAPI()

    assert api.resolve_getter("view.zoom") == api.doc.zoom
    assert api.resolve_getter("viewer.loading") == api.doc.loading
    assert api.resolve_getter("file.path") is None  # nothing opened yet


def test_resolve_getter_unknown_raises() -> None:
    """Unknown getters raise KeyError instead of returning garbage."""
    import pytest

    api = AutomationAPI()
    with pytest.raises(KeyError):
        api.resolve_getter("nope.nada")


def test_wait_fixed_seconds() -> None:
    """`wait` supports fixed delays."""
    import time

    api = AutomationAPI()
    registry = CommandRegistry(api)

    t0 = time.monotonic()
    result = registry.execute("wait", {"seconds": 0.05})
    assert result["met"] is True
    assert time.monotonic() - t0 >= 0.05


def test_wait_getter_condition() -> None:
    """`wait` resolves getter comparisons against live state."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    result = registry.execute("wait", {"condition": "view.zoom == 1.0", "timeout": 2})
    assert result["met"] is True
    assert result["value"] == 1.0

    result = registry.execute("wait", {"condition": "bogus.attr == 1", "timeout": 0.2})
    assert result["met"] is False
    assert "unknown getter" in result["error"]


def test_wait_documented_in_commands() -> None:
    """AGENTS.md documents `wait`; it must be registered."""
    api = AutomationAPI()
    registry = CommandRegistry(api)
    assert "wait" in registry.commands
