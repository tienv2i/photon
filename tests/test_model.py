"""Tests for PhotoDoc model."""

from __future__ import annotations

from pathlib import Path

import pytest

from photon.core.errors import FileNotFoundError_
from photon.core.model import PhotoDoc


def test_open_nonexistent(tmp_path: Path) -> None:
    """Test opening nonexistent file."""
    doc = PhotoDoc()
    with pytest.raises(FileNotFoundError_):
        doc.open(tmp_path / "nonexistent.jpg")


def test_rotate() -> None:
    """Test rotation."""
    doc = PhotoDoc()

    doc.rotate(90)
    assert doc.rotation == 90
    assert doc.stack_len == 1

    doc.rotate(90)
    assert doc.rotation == 180
    assert doc.stack_len == 2

    doc.rotate(-90)
    assert doc.rotation == 90
    assert doc.stack_len == 3


def test_flip() -> None:
    """Test flip."""
    doc = PhotoDoc()

    doc.flip("h")
    assert doc.flip_h is True
    assert doc.flip_v is False

    doc.flip("v")
    assert doc.flip_h is True
    assert doc.flip_v is True

    doc.flip("h")
    assert doc.flip_h is False


def test_zoom() -> None:
    """Test zoom."""
    doc = PhotoDoc()

    doc.zoom_to(2.0)
    assert doc.zoom == 2.0

    doc.zoom_to(0.5)
    assert doc.zoom == 0.5

    # Test clamping
    doc.zoom_to(100.0)
    assert doc.zoom == 10.0

    doc.zoom_to(0.01)
    assert doc.zoom == 0.1


def test_undo_redo() -> None:
    """Test undo/redo."""
    doc = PhotoDoc()

    doc.rotate(90)
    doc.rotate(90)
    assert doc.stack_len == 2

    assert doc.undo() is True
    assert doc.stack_len == 1

    assert doc.undo() is True
    assert doc.stack_len == 0

    assert doc.undo() is False  # Empty stack

    assert doc.redo() is True
    assert doc.stack_len == 1

    assert doc.redo() is True
    assert doc.stack_len == 2

    assert doc.redo() is False  # Empty stack


def test_undo_restores_state() -> None:
    """Undo must revert the actual rotation/flip state, not just the stack."""
    doc = PhotoDoc()

    doc.rotate(90)
    doc.flip("h")
    assert doc.rotation == 90
    assert doc.flip_h is True

    assert doc.undo() is True
    assert doc.rotation == 90
    assert doc.flip_h is False

    assert doc.undo() is True
    assert doc.rotation == 0
    assert doc.flip_h is False

    assert doc.redo() is True
    assert doc.rotation == 90

    assert doc.redo() is True
    assert doc.flip_h is True


def test_resize_undo_redo() -> None:
    """Resize is part of the edit history."""
    doc = PhotoDoc()

    doc.resize(1920)
    assert doc.resize_max == 1920

    doc.undo()
    assert doc.resize_max is None

    doc.redo()
    assert doc.resize_max == 1920


def test_open_clears_edit_history(tmp_path: Path) -> None:
    """Opening a new image must not carry edits from the previous one."""
    path = tmp_path / "a.png"
    path.write_bytes(b"not-an-image")

    doc = PhotoDoc()
    doc.rotate(90)
    doc.resize(800)

    doc.open(path)

    assert doc.rotation == 0
    assert doc.resize_max is None
    assert doc.stack_len == 0
    assert doc.undo() is False


def test_reset() -> None:
    """Test reset."""
    doc = PhotoDoc()

    doc.rotate(90)
    doc.flip("h")
    doc.zoom_to(2.0)

    doc.reset()

    assert doc.rotation == 0
    assert doc.flip_h is False
    assert doc.flip_v is False
    assert doc.zoom == 1.0
    assert doc.stack_len == 0
