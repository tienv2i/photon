"""Tests for file operations."""

from __future__ import annotations

from pathlib import Path

import pytest
from PIL import Image

from photon.automation.api import AutomationAPI
from photon.automation.commands import CommandRegistry


@pytest.fixture
def test_image(tmp_path: Path) -> Path:
    """Create a test image."""
    path = tmp_path / "test.png"
    img = Image.new("RGB", (100, 50), color="red")
    img.save(path)
    return path


@pytest.fixture
def test_folder(tmp_path: Path) -> Path:
    """Create a test folder with multiple images."""
    folder = tmp_path / "images"
    folder.mkdir()

    # Create 3 test images
    for i in range(3):
        path = folder / f"img_{i:03d}.png"
        img = Image.new("RGB", (100, 50), color=(i * 80, 0, 0))
        img.save(path)

    return folder


def test_file_open_single(test_image: Path) -> None:
    """Test opening single file."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    result = registry.execute("file.open", {"path": str(test_image)})

    assert result["path"] == str(test_image)
    assert result["width"] == 100
    assert result["height"] == 50
    assert result["format"] == "PNG"


def test_file_open_folder(test_folder: Path) -> None:
    """Test opening folder."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    result = registry.execute("file.open", {"path": str(test_folder)})

    assert result["path"] == str(test_folder)
    assert result["count"] == 3
    assert len(result["files"]) == 3
    assert result["current"]["path"].endswith("img_000.png")


def test_file_next(test_folder: Path) -> None:
    """Test navigating to next file."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    # Open folder
    registry.execute("file.open", {"path": str(test_folder)})

    # Next
    result = registry.execute("file.next")
    assert result["index"] == 1
    assert result["path"].endswith("img_001.png")

    # Next
    result = registry.execute("file.next")
    assert result["index"] == 2
    assert result["path"].endswith("img_002.png")

    # Next (already at end)
    result = registry.execute("file.next")
    assert result["index"] == 2
    assert result["path"].endswith("img_002.png")


def test_file_prev(test_folder: Path) -> None:
    """Test navigating to previous file."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    # Open folder
    registry.execute("file.open", {"path": str(test_folder)})

    # Next twice
    registry.execute("file.next")
    registry.execute("file.next")

    # Prev
    result = registry.execute("file.prev")
    assert result["index"] == 1
    assert result["path"].endswith("img_001.png")

    # Prev
    result = registry.execute("file.prev")
    assert result["index"] == 0
    assert result["path"].endswith("img_000.png")

    # Prev (already at start)
    result = registry.execute("file.prev")
    assert result["index"] == 0
    assert result["path"].endswith("img_000.png")


def test_file_info(test_image: Path) -> None:
    """Test getting file info."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    # Open file
    registry.execute("file.open", {"path": str(test_image)})

    # Get info
    result = registry.execute("file.info")

    assert result["path"] == str(test_image)
    assert result["width"] == 100
    assert result["height"] == 50
    assert result["format"] == "PNG"
    assert result["size_bytes"] > 0


def test_file_save_copy(test_image: Path, tmp_path: Path) -> None:
    """Test saving copy of file."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    # Open file
    registry.execute("file.open", {"path": str(test_image)})

    # Save copy
    output = tmp_path / "output.jpg"
    result = registry.execute("file.save_copy", {"path": str(output), "quality": 85})

    assert output.exists()
    assert result["format"] == "JPEG"
    assert result["quality"] == 85


def test_file_save_copy_default_path(test_image: Path) -> None:
    """Test saving copy with default path."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    # Open file
    registry.execute("file.open", {"path": str(test_image)})

    # Save copy (no path specified)
    result = registry.execute("file.save_copy")

    # Should create "test (edited).png" in same directory
    expected = test_image.parent / "test (edited).png"
    assert expected.exists()
    assert result["saved_to"] == str(expected)

    # Cleanup
    expected.unlink()


def test_view_zoom() -> None:
    """Test zooming."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    # Zoom to specific factor
    result = registry.execute("view.zoom", {"factor": 2.0})
    assert result["zoom"] == 2.0

    # Zoom in (relative)
    result = registry.execute("view.zoom", {"level": "in"})
    assert result["zoom"] == 2.5  # 2.0 * 1.25

    # Zoom out (relative)
    result = registry.execute("view.zoom", {"level": "out"})
    assert result["zoom"] == 2.0  # 2.5 / 1.25


def test_view_rotate() -> None:
    """Test rotating."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    # Rotate 90
    result = registry.execute("view.rotate", {"degrees": 90})
    assert result["rotation"] == 90

    # Rotate 90 again
    result = registry.execute("view.rotate", {"degrees": 90})
    assert result["rotation"] == 180

    # Rotate -90
    result = registry.execute("view.rotate", {"degrees": -90})
    assert result["rotation"] == 90


def test_view_reset() -> None:
    """Test resetting view."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    # Apply transforms
    registry.execute("view.zoom", {"factor": 2.0})
    registry.execute("view.rotate", {"degrees": 90})

    # Reset
    result = registry.execute("view.reset")

    assert result["zoom"] == 1.0
    assert result["rotation"] == 0


def test_edit_undo_redo() -> None:
    """Test undo/redo."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    # Apply edit
    result = registry.execute("edit.set", {"rotate": 90})
    assert result["stack_len"] == 1

    # Undo
    result = registry.execute("edit.undo")
    assert result["stack_len"] == 0
    assert result["undone"] is True

    # Redo
    result = registry.execute("edit.redo")
    assert result["stack_len"] == 1
    assert result["redone"] is True


def test_sidebar_show_state() -> None:
    """Test sidebar show and state."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    # Show sidebar
    result = registry.execute("sidebar.show", {"side": "right", "visible": True})
    assert result["side"] == "right"
    assert result["visible"] is True

    # Get state
    result = registry.execute("sidebar.state", {"side": "right"})
    assert result["side"] == "right"
    assert result["visible"] is True


def test_toolbar_click() -> None:
    """Test toolbar click."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    result = registry.execute("toolbar.click", {"button": "open"})
    assert result["clicked"] == "open"


def test_dialog_open_result() -> None:
    """Test dialog open and result."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    # Open dialog
    result = registry.execute("dialog.open", {"type": "open"})
    handle = result["handle"]
    assert handle.startswith("photon-dialog-")

    # Get result (should be done or pending)
    result = registry.execute("dialog.result", {"handle": handle})
    assert result["status"] in ("pending", "done", "cancelled")


def test_log_tail_errors() -> None:
    """Test log tail and errors."""
    api = AutomationAPI()
    registry = CommandRegistry(api)

    # Perform some actions
    registry.execute("view.zoom", {"factor": 2.0})
    registry.execute("view.rotate", {"degrees": 90})

    # Get log tail
    result = registry.execute("log.tail", {"n": 10})
    assert len(result["lines"]) > 0

    # Get errors
    result = registry.execute("log.errors", {"n": 5})
    assert "errors" in result
