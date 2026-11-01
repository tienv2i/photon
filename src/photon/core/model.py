"""PhotoDoc: image path, transform stack, undo/redo."""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from photon.core.errors import FileNotFoundError_
from photon.core.events import EventBus


@dataclass
class Transform:
    """A single transform operation.

    ``before``/``after`` are snapshots of the editable view state. Storing
    both makes undo/redo exact instead of trying to invert each operation
    (flip toggles and repeated resizes are easy to get wrong that way).
    """

    kind: str  # "rotate", "flip", "resize"
    params: dict[str, Any] = field(default_factory=dict)
    before: dict[str, Any] = field(default_factory=dict)
    after: dict[str, Any] = field(default_factory=dict)


class PhotoDoc:
    """Document model for an image.

    Tracks:
    - File path and metadata
    - Transform stack (undo/redo)
    - Current view state (zoom, rotation, flip)
    """

    def __init__(self) -> None:
        self.path: Path | None = None
        self.width: int = 0
        self.height: int = 0
        self.format: str = ""
        self.mode: str = ""
        self.size_bytes: int = 0

        # View state
        self.zoom: float = 1.0
        self.rotation: int = 0  # 0, 90, 180, 270
        self.flip_h: bool = False
        self.flip_v: bool = False
        self.resize_max: int | None = None  # longest side after resize

        # Transform stack for undo/redo
        self._undo_stack: list[Transform] = []
        self._redo_stack: list[Transform] = []

        # Loading state
        self.loading: bool = False

    @property
    def index(self) -> int:
        """Current image index in folder (0-based)."""
        if self.path is None:
            return -1
        # TODO: Track folder index
        return 0

    @property
    def total(self) -> int:
        """Total images in folder."""
        # TODO: Track folder total
        return 1

    def open(self, path: Path) -> dict[str, Any]:
        """Open an image file.

        Args:
            path: Path to image file

        Returns:
            Metadata dict

        Raises:
            FileNotFoundError_: If file doesn't exist
        """
        if not path.exists():
            raise FileNotFoundError_(str(path))

        self.path = path
        self.loading = True

        # A new document starts with no edits; stale undo history from the
        # previous image would otherwise "undo" the wrong file's transforms.
        self._reset_transforms()

        # Emit event
        EventBus.emit("file.open", {"path": str(path)})

        # TODO: Actual decode (will be done in imaging/decode.py)
        # For now, just return placeholder
        self.loading = False

        return {
            "path": str(path),
            "width": self.width,
            "height": self.height,
            "format": self.format,
            "size_bytes": self.size_bytes,
        }

    def _snapshot(self) -> dict[str, Any]:
        """Snapshot the editable state (rotation/flip/resize)."""
        return {
            "rotation": self.rotation,
            "flip_h": self.flip_h,
            "flip_v": self.flip_v,
            "resize_max": self.resize_max,
        }

    def _restore(self, state: dict[str, Any]) -> None:
        """Restore the editable state from a snapshot."""
        self.rotation = state["rotation"]
        self.flip_h = state["flip_h"]
        self.flip_v = state["flip_v"]
        self.resize_max = state["resize_max"]

    def _push(self, transform: Transform) -> None:
        """Record a transform with before/after snapshots."""
        self._undo_stack.append(transform)
        self._redo_stack.clear()

    def rotate(self, degrees: int) -> None:
        """Rotate image (90, -90, 180).

        Args:
            degrees: Rotation in degrees
        """
        if degrees not in (90, -90, 180):
            raise ValueError(f"Invalid rotation: {degrees}")

        before = self._snapshot()
        self.rotation = (self.rotation + degrees) % 360
        self._push(Transform("rotate", {"degrees": degrees}, before, self._snapshot()))

        EventBus.emit("view.rotate", {"degrees": degrees, "rotation": self.rotation})

    def flip(self, axis: str) -> None:
        """Flip image horizontally or vertically.

        Args:
            axis: "h" or "v"
        """
        if axis not in ("h", "v"):
            raise ValueError(f"Invalid axis: {axis}")

        before = self._snapshot()
        if axis == "h":
            self.flip_h = not self.flip_h
        else:
            self.flip_v = not self.flip_v
        self._push(Transform("flip", {"axis": axis}, before, self._snapshot()))

        EventBus.emit("view.flip", {"axis": axis, "flip_h": self.flip_h, "flip_v": self.flip_v})

    def resize(self, max_side: int) -> None:
        """Limit the longest side when saving (non-destructive preview).

        Args:
            max_side: Maximum width/height in pixels (must be > 0)

        Raises:
            ValueError: If ``max_side`` is not positive
        """
        if max_side <= 0:
            raise ValueError(f"Invalid resize max: {max_side}")

        before = self._snapshot()
        self.resize_max = max_side
        self._push(Transform("resize", {"max": max_side}, before, self._snapshot()))

        EventBus.emit("edit.resize", {"max": max_side})

    def zoom_to(self, factor: float) -> None:
        """Set zoom level.

        Args:
            factor: Zoom factor (1.0 = 100%)
        """
        self.zoom = max(0.1, min(10.0, factor))
        EventBus.emit("view.zoom", {"zoom": self.zoom})

    def undo(self) -> bool:
        """Undo last transform.

        Returns:
            True if undone, False if stack empty
        """
        if not self._undo_stack:
            return False

        transform = self._undo_stack.pop()
        self._redo_stack.append(transform)

        self._restore(transform.before)
        EventBus.emit(
            "edit.undo",
            {"transform": transform.kind, "rotation": self.rotation},
        )
        return True

    def redo(self) -> bool:
        """Redo last undone transform.

        Returns:
            True if redone, False if stack empty
        """
        if not self._redo_stack:
            return False

        transform = self._redo_stack.pop()
        self._undo_stack.append(transform)

        self._restore(transform.after)
        EventBus.emit(
            "edit.redo",
            {"transform": transform.kind, "rotation": self.rotation},
        )
        return True

    @property
    def stack_len(self) -> int:
        """Length of undo stack."""
        return len(self._undo_stack)

    @property
    def transforms(self) -> list[Transform]:
        """Applied transforms in the order the user performed them."""
        return list(self._undo_stack)

    def _reset_transforms(self) -> None:
        """Clear edit state and undo/redo history (keeps zoom)."""
        self.rotation = 0
        self.flip_h = False
        self.flip_v = False
        self.resize_max = None
        self._undo_stack.clear()
        self._redo_stack.clear()

    def reset(self) -> None:
        """Reset all transforms and view state."""
        self.zoom = 1.0
        self._reset_transforms()

        EventBus.emit("view.reset", {})
