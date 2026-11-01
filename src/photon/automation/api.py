"""AutomationAPI: single entry point for all actions."""

from __future__ import annotations

import os
import time
from pathlib import Path
from typing import Any

from photon.core import log
from photon.core.errors import (
    FeatureDisabledError,
    FileNotFoundError_,
    InvalidArgumentsError,
    PhotonError,
)
from photon.core.events import EventBus
from photon.core.features import compare_enabled
from photon.core.model import PhotoDoc
from photon.core.trash import move_to_trash
from photon.imaging.decode import decode_image
from photon.portal.filedialog import portal_dialog


def _invoke_on_gui_thread(window: Any, method: str) -> None:
    """Run ``method`` on the window's GUI thread.

    Automation commands normally execute on the GUI thread already (the server
    marshals them); this helper keeps direct calls safe when a command runs on
    a worker thread. Qt widgets must only be created/touched from the thread
    that owns them. Falls back to a direct call when Qt is unavailable.
    """
    try:
        from PySide6.QtCore import QMetaObject, Qt, QThread
    except ImportError:  # pragma: no cover - Qt always present in GUI builds
        getattr(window, method)()
        return

    if QThread.currentThread() == window.thread():
        getattr(window, method)()
        return

    QMetaObject.invokeMethod(
        window,
        method,
        Qt.ConnectionType.QueuedConnection,
    )


class AutomationAPI:
    """Automation API — every UI action and CLI command calls methods here.

    This is the SINGLE code path. Toolbar buttons, keyboard shortcuts,
    and automation commands all delegate to these methods.
    """

    def __init__(self) -> None:
        self.doc = PhotoDoc()
        self._start_time: float = time.time()

        # Window reference (set by app when GUI starts)
        self._window: Any = None

        # Folder navigation (used only when running without a window)
        self._local_folder_files: list[Path] = []
        self._local_current_index: int = -1

    # Folder state lives in MainWindow once a window is attached, so the UI
    # and automation can never disagree about the current folder/index.
    @property
    def _folder_files(self) -> list[Path]:
        if self._window is not None:
            return list(self._window._folder_files)
        return self._local_folder_files

    @_folder_files.setter
    def _folder_files(self, files: list[Path]) -> None:
        if self._window is not None:
            self._window._folder_files = files
        else:
            self._local_folder_files = files

    @property
    def _current_index(self) -> int:
        if self._window is not None:
            return int(self._window._current_index)
        return self._local_current_index

    @_current_index.setter
    def _current_index(self, index: int) -> None:
        if self._window is not None:
            self._window._current_index = index
        else:
            self._local_current_index = index

    def resolve_getter(self, path: str) -> Any:
        """Resolve a dotted getter path against live app state.

        Supported roots: ``view`` (zoom/rotation), ``viewer`` (loading),
        ``file`` (path/width/height/format), ``window`` (width/height/
        fullscreen/compare/fit), ``app`` (version/headless/pid).

        Args:
            path: e.g. ``view.zoom`` or ``file.width``

        Returns:
            Current value.

        Raises:
            KeyError: If the root or attribute is unknown.
        """
        root, _, attr = path.partition(".")
        table: dict[str, dict[str, Any]] = {
            "view": {
                "zoom": self.doc.zoom,
                "rotation": self.doc.rotation,
                "flip_h": self.doc.flip_h,
                "flip_v": self.doc.flip_v,
                "resize_max": self.doc.resize_max,
            },
            "viewer": {
                "loading": self.doc.loading,
            },
            "file": {
                "path": str(self.doc.path) if self.doc.path else None,
                "width": self.doc.width,
                "height": self.doc.height,
                "format": self.doc.format,
            },
            "app": {
                "version": "0.1.0",
                "headless": _is_headless(),
                "pid": _get_pid(),
            },
        }
        if root == "canvas" and self._window is not None:
            canvas = self._window.canvas
            canvas_values = {
                "active": canvas.active,
                "width": canvas.width(),
                "height": canvas.height(),
                "zoom": canvas.zoom,
                "rotation": canvas.rotation,
                "flip_h": canvas.flip_h,
                "flip_v": canvas.flip_v,
                "offset_x": canvas.offset_x,
                "offset_y": canvas.offset_y,
                "fit": canvas.fit_mode,
            }
            if attr not in canvas_values:
                raise KeyError(f"unknown getter: {path}")
            return canvas_values[attr]
        if root == "compare" and self._window is not None:
            pane = self._window.compare_pane
            compare_values = {
                "active": pane.canvas.active,
                "enabled": bool(getattr(self._window, "_compare_active", False)),
                "visible": pane.isVisible(),
                "path": pane.path,
                "zoom": pane.canvas.zoom,
                "rotation": pane.canvas.rotation,
                "flip_h": pane.canvas.flip_h,
                "flip_v": pane.canvas.flip_v,
                "offset_x": pane.canvas.offset_x,
                "offset_y": pane.canvas.offset_y,
            }
            if attr not in compare_values:
                raise KeyError(f"unknown getter: {path}")
            return compare_values[attr]
        if root == "frame" and self._window is not None:
            frame_values = {
                "active": self._window.active_frame,
                "main": self._window.active_frame == "main",
                "compare": self._window.active_frame == "compare",
            }
            if attr not in frame_values:
                raise KeyError(f"unknown getter: {path}")
            return frame_values[attr]
        if root == "window" and self._window is not None:
            size = self._window.size()
            window_values = {
                "width": size.width(),
                "height": size.height(),
                "fullscreen": self._window.isFullScreen(),
                "compare": bool(getattr(self._window, "_compare_active", False)),
                "active_frame": self._window.active_frame,
                "fit": getattr(self._window.canvas, "fit_mode", False),
            }
            if attr not in window_values:
                raise KeyError(f"unknown getter: {path}")
            return window_values[attr]
        section = table.get(root)
        if section is None or attr not in section:
            raise KeyError(f"unknown getter: {path}")
        return section[attr]

    def set_window(self, window: Any) -> None:
        """Set window reference for UI operations.

        Shares the window's ``PhotoDoc`` and folder state so automation
        commands observe exactly what the UI shows (a separate model would
        silently report stale ``path``/``width`` values).

        Args:
            window: MainWindow instance
        """
        self._window = window
        if getattr(window, "doc", None) is not None:
            self.doc = window.doc
        # Folder list/index are proxied to the window (see properties above).

    # --- App ---

    def app_info(self, **kwargs: Any) -> dict[str, Any]:
        """Get app info."""
        import platform

        return {
            "version": "0.1.0",
            "pid": _get_pid(),
            "platform": platform.platform(),
            "headless": _is_headless(),
            "socket": _get_socket_path(),
            "uptime_s": round(time.time() - self._start_time, 1),
        }

    def app_doctor(self, **kwargs: Any) -> dict[str, Any]:
        """Health check."""
        cache_dir = Path.home() / ".cache" / "photon"
        log_path = cache_dir / "photon.log"
        socket_path = cache_dir / "photon.sock"

        errors = log.get_errors(n=5)

        return {
            "socket_ok": socket_path.exists(),
            "log_ok": log_path.exists(),
            "cache_ok": cache_dir.exists(),
            "recent_errors": errors,
        }

    def app_selftest(self, **kwargs: Any) -> dict[str, Any]:
        """Run internal self-tests."""
        passed = []
        failed = []

        # Test decode
        try:
            from PIL import Image

            test_path = Path("/tmp/photon-selftest.png")
            Image.new("RGB", (10, 10), color="red").save(test_path)

            img, meta = decode_image(test_path)
            if img.size == (10, 10):
                passed.append("decode")
            else:
                failed.append("decode")
            test_path.unlink()
        except Exception as e:
            failed.append(f"decode: {e}")

        return {"passed": passed, "failed": failed}

    def app_quit(self, **kwargs: Any) -> dict[str, Any]:
        """Quit app."""
        EventBus.emit("app.quit", {"reason": "command"})
        return {"quitting": True}

    # --- File ---

    def file_open(self, path: str = "", **kwargs: Any) -> dict[str, Any]:
        """Open image file.

        Args:
            path: Path to image file

        Returns:
            File metadata dict
        """
        if not path:
            raise InvalidArgumentsError("file.open", "path is required")

        p = Path(path)

        # If directory, open folder
        if p.is_dir():
            return self._open_folder(p)

        # Open single file
        return self._open_file(p)

    def _open_folder(self, folder: Path) -> dict[str, Any]:
        """Open folder and load image list.

        Args:
            folder: Path to folder

        Returns:
            Folder info dict
        """
        # Scan for image files
        image_extensions = {".jpg", ".jpeg", ".png", ".webp", ".gif", ".tiff", ".bmp"}
        files = sorted(
            f for f in folder.iterdir() if f.is_file() and f.suffix.lower() in image_extensions
        )

        if not files:
            return {"path": str(folder), "count": 0, "files": []}

        self._folder_files = files
        self._current_index = 0

        # Open first image
        result = self._open_file(files[0])

        return {
            "path": str(folder),
            "count": len(files),
            "files": [str(f) for f in files],
            "current": result,
        }

    def _open_file(self, path: Path) -> dict[str, Any]:
        """Open single image file.

        Args:
            path: Path to image file

        Returns:
            File metadata dict
        """
        # Decode image
        img, meta = decode_image(path)

        # Update doc (open() also clears the previous image's edit history)
        self.doc.open(path)
        self.doc.width = meta["width"]
        self.doc.height = meta["height"]
        self.doc.format = meta["format"]
        self.doc.size_bytes = meta["size_bytes"]

        # Update folder index if in folder mode
        if self._folder_files:
            try:
                self._current_index = self._folder_files.index(path)
            except ValueError:
                self._current_index = -1

        # Update window if available
        if self._window:
            self._window.open_path(path)
            # Automation must observe a finished load; the GUI thread keeps
            # painting inside the nested loop, so this is not a freeze.
            self._window.wait_for_image_load()

        EventBus.emit(
            "file.opened",
            {
                "path": str(path),
                "width": meta["width"],
                "height": meta["height"],
                "index": self._current_index,
                "total": len(self._folder_files),
            },
        )

        return meta

    def file_next(self, **kwargs: Any) -> dict[str, Any]:
        """Next image in folder.

        Returns:
            Navigation result
        """
        if not self._folder_files:
            return {"index": -1, "total": 0, "path": None}

        if self._current_index < len(self._folder_files) - 1:
            self._current_index += 1
            result = self._open_file(self._folder_files[self._current_index])
            EventBus.emit("file.next", {"index": self._current_index})
            return {
                "index": self._current_index,
                "total": len(self._folder_files),
                "path": str(self._folder_files[self._current_index]),
                "file": result,
            }
        else:
            # Already at end
            return {
                "index": self._current_index,
                "total": len(self._folder_files),
                "path": str(self._folder_files[self._current_index]),
            }

    def file_prev(self, **kwargs: Any) -> dict[str, Any]:
        """Previous image in folder.

        Returns:
            Navigation result
        """
        if not self._folder_files:
            return {"index": -1, "total": 0, "path": None}

        if self._current_index > 0:
            self._current_index -= 1
            result = self._open_file(self._folder_files[self._current_index])
            EventBus.emit("file.prev", {"index": self._current_index})
            return {
                "index": self._current_index,
                "total": len(self._folder_files),
                "path": str(self._folder_files[self._current_index]),
                "file": result,
            }
        else:
            # Already at start
            return {
                "index": self._current_index,
                "total": len(self._folder_files),
                "path": str(self._folder_files[self._current_index]),
            }

    def file_info(self, **kwargs: Any) -> dict[str, Any]:
        """Get current file info.

        Returns:
            File metadata dict
        """
        if self.doc.path is None:
            return {"path": None}

        return {
            "path": str(self.doc.path),
            "width": self.doc.width,
            "height": self.doc.height,
            "format": self.doc.format,
            "size_bytes": self.doc.size_bytes,
            "index": self._current_index,
            "total": len(self._folder_files),
        }

    def file_save_copy(
        self,
        path: str = "",
        format: str | None = None,
        quality: int = 90,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Save copy of current image (never overwrites original).

        Args:
            path: Output path (default: <stem> (edited).<ext>)
            format: Output format (jpg, png, webp)
            quality: Quality for lossy formats (1-100)

        Returns:
            Save result
        """
        if self.doc.path is None:
            raise InvalidArgumentsError("file.save_copy", "no image open")

        from photon.imaging import ops
        from photon.imaging.encode import encode_image

        img, _ = decode_image(self.doc.path)

        # Bake the edit stack into the saved copy, in the same order the
        # user applied it. Saving without this silently discarded all edits.
        for transform in self.doc.transforms:
            if transform.kind == "rotate":
                img = ops.rotate(img, int(transform.params["degrees"]))
            elif transform.kind == "flip":
                img = ops.flip(img, str(transform.params["axis"]))
            elif transform.kind == "resize":
                max_side = int(transform.params["max"])
                img = ops.resize(img, max_width=max_side, max_height=max_side)

        # Default save path: <stem> (edited).<ext>
        if not path:
            original = self.doc.path
            path = str(original.parent / f"{original.stem} (edited){original.suffix}")

        result = encode_image(img, Path(path), format=format, quality=quality)
        return result

    def file_delete(self, **kwargs: Any) -> dict[str, Any]:
        """Delete current image by moving it to the freedesktop trash.

        Returns:
            Delete result with the trashed path
        """
        if self.doc.path is None:
            raise InvalidArgumentsError("file.delete", "no image open")

        path = self.doc.path
        trashed_to = move_to_trash(path)

        log.log_event("file.trashed", {"path": str(path), "trashed_to": str(trashed_to)})
        EventBus.emit("file.delete", {"path": str(path), "trashed_to": str(trashed_to)})

        # Keep UI/folder navigation in sync so the deleted file is not
        # reopened by file.next / file.prev.
        if self._window is not None:
            self._window.on_file_trashed(path)
        else:
            self.doc.path = None
            self.doc.reset()

        return {"trashed_to": str(trashed_to), "path": str(path)}

    # --- View ---

    # --- Frames ----------------------------------------------------------

    def _frame_target(self, frame: str | None, command: str) -> tuple[str, Any]:
        """Resolve a frame argument to ``(name, canvas)``.

        ``None`` (or ``"active"``) means the currently active frame, which is
        what the toolbar buttons act on. ``canvas`` is None when the API runs
        without a window (headless unit tests).

        Args:
            frame: "main", "compare", "active" or None
            command: Command name, used for error messages

        Returns:
            Tuple of (frame name, canvas or None)

        Raises:
            InvalidArgumentsError: When the frame name is unknown
        """
        name = frame or "active"
        if name == "active":
            name = self._window.active_frame if self._window is not None else "main"
        if name == "compare" and not compare_enabled():
            raise FeatureDisabledError("compare")
        if self._window is None:
            return name, None
        try:
            canvas = self._window.canvas_for(name)
        except ValueError:
            raise InvalidArgumentsError(command, f"unknown frame: {frame!r}") from None
        return name, canvas

    def frame_activate(self, frame: str = "", **kwargs: Any) -> dict[str, Any]:
        """Make a frame active; toolbar/``view.*`` commands follow it.

        Args:
            frame: "main" or "compare"

        Returns:
            {"active": "main"|"compare"}
        """
        if not frame:
            raise InvalidArgumentsError("frame.activate", "frame is required")
        if not compare_enabled():
            raise FeatureDisabledError("compare")
        if self._window is None:
            return {"active": frame, "error": "no window"}

        try:
            return {"active": self._window.set_active_frame(frame)}
        except ValueError:
            raise InvalidArgumentsError("frame.activate", f"unknown frame: {frame!r}") from None

    def frame_state(self, **kwargs: Any) -> dict[str, Any]:
        """State of both frames and which one is active.

        Returns:
            {"active": "main"|"compare", "frames": {"main": {...}, "compare": {...}}}
        """
        if not compare_enabled():
            raise FeatureDisabledError("compare")
        if self._window is None:
            return {"active": "main", "frames": {}}

        return {
            "active": self._window.active_frame,
            "frames": {
                "main": self._window.canvas.state(),
                "compare": self._window.compare_pane.state(),
            },
        }

    # --- View ------------------------------------------------------------

    def view_zoom(
        self,
        factor: float | None = None,
        level: str | None = None,
        frame: str | None = None,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Zoom a frame (default: the active one).

        Args:
            factor: Zoom factor (1.0 = 100%)
            level: "in" or "out" (relative zoom)
            frame: "main", "compare" or None for the active frame

        Returns:
            New zoom level and the frame it was applied to
        """
        name, canvas = self._frame_target(frame, "view.zoom")

        if canvas is None:
            if level == "in":
                factor = self.doc.zoom * 1.25
            elif level == "out":
                factor = self.doc.zoom / 1.25
            if factor is None:
                raise InvalidArgumentsError("view.zoom", "factor or level required")
            self.doc.zoom_to(factor)
            return {"zoom": self.doc.zoom, "frame": name}

        if level == "in":
            self._window._on_zoom_in(canvas)
        elif level == "out":
            self._window._on_zoom_out(canvas)
        elif factor is not None:
            self._window._apply_zoom(factor, canvas)
        else:
            raise InvalidArgumentsError("view.zoom", "factor or level required")

        zoom = self.doc.zoom if name == "main" else canvas.zoom
        return {"zoom": zoom, "frame": name}

    def view_pan(
        self,
        dx: int = 0,
        dy: int = 0,
        frame: str | None = None,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Pan a frame by a pixel delta (clamped to the image bounds).

        Uses the same :meth:`Canvas.pan` path as a mouse drag, so automation
        can assert that the image cannot be thrown off screen.

        Args:
            dx: Horizontal delta in pixels
            dy: Vertical delta in pixels
            frame: "main", "compare" or None for the active frame

        Returns:
            Current pan offset, the maximum allowed offset and the frame name
        """
        name, canvas = self._frame_target(frame, "view.pan")
        if canvas is None:
            return {"offset_x": 0, "offset_y": 0, "frame": name, "error": "no window"}

        canvas.pan(dx, dy)
        max_x, max_y = canvas.offset_bounds()
        return {
            "offset_x": canvas.offset_x,
            "offset_y": canvas.offset_y,
            "max_x": max_x,
            "max_y": max_y,
            "at_limit": abs(canvas.offset_x) >= max_x and abs(canvas.offset_y) >= max_y,
            "frame": name,
        }

    def view_fit(self, frame: str | None = None, **kwargs: Any) -> dict[str, Any]:
        """Fit a frame (default: the active one) to its viewport.

        Args:
            frame: "main", "compare" or None for the active frame

        Returns:
            New zoom level and the frame it was applied to
        """
        name, canvas = self._frame_target(frame, "view.fit")
        if canvas is None:
            self.doc.zoom_to(1.0)
            return {"zoom": self.doc.zoom, "frame": name}

        self._window._apply_fit(canvas)
        zoom = self.doc.zoom if name == "main" else canvas.zoom
        return {"zoom": zoom, "frame": name}

    def view_actual(self, frame: str | None = None, **kwargs: Any) -> dict[str, Any]:
        """Show a frame (default: the active one) at 100%.

        Args:
            frame: "main", "compare" or None for the active frame

        Returns:
            New zoom level and the frame it was applied to
        """
        name, canvas = self._frame_target(frame, "view.actual")
        if canvas is None:
            self.doc.zoom_to(1.0)
            return {"zoom": self.doc.zoom, "frame": name}

        self._window._on_actual(canvas)
        zoom = self.doc.zoom if name == "main" else canvas.zoom
        return {"zoom": zoom, "frame": name}

    def view_rotate(
        self,
        degrees: int = 90,
        frame: str | None = None,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Rotate a frame (default: the active one).

        On the main frame this is a document edit (undoable); on the compare
        frame it only changes how the reference is displayed.

        Args:
            degrees: Rotation delta (90, -90, 180)
            frame: "main", "compare" or None for the active frame

        Returns:
            New rotation and the frame it was applied to
        """
        name, canvas = self._frame_target(frame, "view.rotate")
        if canvas is None:
            self.doc.rotate(degrees)
            return {"rotation": self.doc.rotation, "frame": name}

        self._window._on_rotate(degrees, canvas)
        rotation = self.doc.rotation if name == "main" else canvas.rotation
        return {"rotation": rotation, "frame": name}

    def view_flip(
        self,
        axis: str = "h",
        frame: str | None = None,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Flip a frame (default: the active one).

        Args:
            axis: "h", "v" or "hv"
            frame: "main", "compare" or None for the active frame

        Returns:
            Flip state and the frame it was applied to
        """
        name, canvas = self._frame_target(frame, "view.flip")
        if canvas is None:
            for single in axis:
                self.doc.flip(single)
            return {
                "flip_h": self.doc.flip_h,
                "flip_v": self.doc.flip_v,
                "frame": name,
            }

        self._window._on_flip(axis, canvas)
        if name == "main":
            return {
                "flip_h": self.doc.flip_h,
                "flip_v": self.doc.flip_v,
                "frame": name,
            }
        return {"flip_h": canvas.flip_h, "flip_v": canvas.flip_v, "frame": name}

    def view_reset(self, frame: str | None = None, **kwargs: Any) -> dict[str, Any]:
        """Reset zoom/rotation/flip of a frame (default: the active one).

        Args:
            frame: "main", "compare" or None for the active frame

        Returns:
            Reset state and the frame it was applied to
        """
        name, canvas = self._frame_target(frame, "view.reset")
        if canvas is None:
            self.doc.reset()
            return {
                "zoom": self.doc.zoom,
                "rotation": self.doc.rotation,
                "frame": name,
            }

        self._window._on_reset(canvas)
        if name == "main":
            return {
                "zoom": self.doc.zoom,
                "rotation": self.doc.rotation,
                "frame": name,
            }
        return {"zoom": canvas.zoom, "rotation": canvas.rotation, "frame": name}

    def view_fullscreen(
        self,
        enabled: bool | None = None,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Toggle or set full screen mode.

        Args:
            enabled: True/False to force a state, None to toggle

        Returns:
            {"fullscreen": bool}
        """
        if self._window is None:
            return {"fullscreen": False, "error": "no window"}

        current = bool(self._window.isFullScreen())
        target = (not current) if enabled is None else bool(enabled)
        if target != current:
            _invoke_on_gui_thread(self._window, "_on_toggle_fullscreen")

        EventBus.emit("view.fullscreen", {"fullscreen": target})
        return {"fullscreen": target}

    def view_compare(
        self,
        enabled: bool | None = None,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Toggle or set the side-by-side compare pane.

        The pane sits right of the main canvas and holds a reference image.
        Hiding it keeps the reference image so it can be shown again.

        Args:
            enabled: True/False to force a state, None to toggle

        Returns:
            {"active": bool}
        """
        if not compare_enabled():
            raise FeatureDisabledError("compare")

        if self._window is None:
            return {"active": False, "error": "no window"}

        current = bool(getattr(self._window, "_compare_active", False))
        target = (not current) if enabled is None else bool(enabled)
        if target != current:
            # The window emits ``view.compare`` from ``_on_compare`` so the
            # button click and this command share one notification path.
            _invoke_on_gui_thread(self._window, "_on_toggle_compare")

        return {"active": target}

    def compare_set_image(self, path: str = "", **kwargs: Any) -> dict[str, Any]:
        """Place a reference image in the compare pane.

        Decoding happens on a worker thread (the same ``ImageLoader`` path the
        drag & drop UI uses), then the pane is shown so the comparison is
        visible immediately.

        Args:
            path: Image file to show on the right-hand side

        Returns:
            Compare pane state
        """
        if not compare_enabled():
            raise FeatureDisabledError("compare")

        if not path:
            raise InvalidArgumentsError("compare.set_image", "path is required")
        if self._window is None:
            return {"path": None, "error": "no window"}

        target = Path(path)
        if not target.exists():
            raise FileNotFoundError_(str(target))

        if not self._window._compare_active:
            _invoke_on_gui_thread(self._window, "_on_toggle_compare")

        self._window.load_compare_file(target)
        self._window.wait_for_compare_load()

        error = self._window._compare_error
        if error is not None:
            # The loader already produced a fully formatted PhotonError
            # message ("PHOTON-DEC-101: Failed to decode image: ..."); re-raise
            # it without nesting the code/message twice.
            code, sep, message = error.partition(": ")
            raise PhotonError(code if sep else "PHOTON-DEC-101", message or error)

        return self._window.compare_pane.state()

    def compare_clear(self, **kwargs: Any) -> dict[str, Any]:
        """Remove the reference image from the compare pane."""
        if not compare_enabled():
            raise FeatureDisabledError("compare")
        if self._window is None:
            return {"path": None, "error": "no window"}

        self._window.compare_pane.clear()
        return self._window.compare_pane.state()

    def compare_state(self, **kwargs: Any) -> dict[str, Any]:
        """Return the compare frame state (visible, active, image, view)."""
        if not compare_enabled():
            raise FeatureDisabledError("compare")
        if self._window is None:
            return {"visible": False, "error": "no window"}

        return self._window.compare_pane.state()

    # --- Edit ---

    def edit_set(
        self,
        rotate: int | None = None,
        flip: str | None = None,
        resize: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Apply edit transform.

        Args:
            rotate: Rotation degrees
            flip: Flip axis ("h" or "v")
            resize: ``{"max": 1920}`` to limit the longest side on save

        Returns:
            Stack length and current transform state
        """
        if rotate is not None:
            self.doc.rotate(rotate)
        if flip is not None:
            self.doc.flip(flip)
        if resize is not None:
            max_side = resize.get("max")
            if not isinstance(max_side, int):
                raise InvalidArgumentsError("edit.set", "resize.max must be an integer")
            self.doc.resize(max_side)

        self._sync_canvas_transforms()
        return self._transform_state()

    def edit_undo(self, **kwargs: Any) -> dict[str, Any]:
        """Undo last edit.

        Returns:
            Undo result
        """
        undone = self.doc.undo()
        self._sync_canvas_transforms()
        return {"undone": undone, **self._transform_state()}

    def edit_redo(self, **kwargs: Any) -> dict[str, Any]:
        """Redo last undone edit.

        Returns:
            Redo result
        """
        redone = self.doc.redo()
        self._sync_canvas_transforms()
        return {"redone": redone, **self._transform_state()}

    def edit_reset(self, **kwargs: Any) -> dict[str, Any]:
        """Reset all edits.

        Returns:
            Reset result
        """
        self.doc.reset()
        self._sync_canvas_transforms()
        return {"stack_len": 0, **self._transform_state()}

    def _transform_state(self) -> dict[str, Any]:
        """Current non-destructive edit state (shared by edit.* results)."""
        return {
            "stack_len": self.doc.stack_len,
            "rotation": self.doc.rotation,
            "flip_h": self.doc.flip_h,
            "flip_v": self.doc.flip_v,
            "resize_max": self.doc.resize_max,
        }

    def _sync_canvas_transforms(self) -> None:
        """Mirror doc rotation/flip onto the canvas (no-op when headless)."""
        if self._window is None:
            return
        canvas = self._window.canvas
        canvas.set_rotation(self.doc.rotation)
        canvas.set_flip(self.doc.flip_h, self.doc.flip_v)

    # --- Sidebar ---

    def sidebar_show(
        self,
        side: str = "right",
        visible: bool = True,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Show/hide sidebar(s).

        Args:
            side: "left", "right", or "both"/"edit_tools" for both panels
            visible: True to show, False to hide

        Returns:
            Sidebar state
        """
        # Update config
        from photon.core.settings import set as config_set

        visible = bool(visible)

        if side in ("both", "edit_tools"):
            config_set("sidebar.left.visible", visible)
            config_set("sidebar.right.visible", visible)
            if self._window:
                self._window.set_edit_tools_visible(visible)

            EventBus.emit("sidebar.show", {"side": "both", "visible": visible})
            return {"side": "both", "visible": visible, "left": visible, "right": visible}

        if side not in ("left", "right"):
            raise InvalidArgumentsError("sidebar.show", f"unknown side: {side!r}")

        config_set(f"sidebar.{side}.visible", visible)

        # Update window if available
        if self._window:
            if side == "left":
                self._window.sidebar_left.setVisible(visible)
            elif side == "right":
                self._window.sidebar_right.setVisible(visible)

        EventBus.emit("sidebar.show", {"side": side, "visible": visible})
        return {"side": side, "visible": visible}

    def sidebar_state(self, side: str = "right", **kwargs: Any) -> dict[str, Any]:
        """Get sidebar state.

        Args:
            side: "left", "right", or "both"/"edit_tools" for both panels

        Returns:
            Sidebar state
        """
        from photon.core.settings import get as config_get

        def _visible(name: str) -> bool:
            if self._window:
                widget = self._window.sidebar_left if name == "left" else self._window.sidebar_right
                return not widget.isHidden()
            return bool(config_get(f"sidebar.{name}.visible", False))

        if side in ("both", "edit_tools"):
            left = _visible("left")
            right = _visible("right")
            return {
                "side": "both",
                "visible": left and right,
                "left": left,
                "right": right,
                "width": {
                    "left": config_get("sidebar.left.width", 240),
                    "right": config_get("sidebar.right.width", 280),
                },
                "content": "empty",
            }

        if side not in ("left", "right"):
            raise InvalidArgumentsError("sidebar.state", f"unknown side: {side!r}")

        return {
            "side": side,
            "visible": _visible(side),
            "width": config_get(f"sidebar.{side}.width", 240),
            "content": "empty",
        }

    def sidebar_set_content(
        self,
        side: str = "right",
        widget: str = "edit_tools",
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Set sidebar content (future).

        Args:
            side: "left" or "right"
            widget: Widget name

        Returns:
            Result
        """
        EventBus.emit("sidebar.set_content", {"side": side, "widget": widget})
        return {"side": side, "widget": widget}

    # --- Toolbar ---

    def toolbar_click(self, button: str = "", **kwargs: Any) -> dict[str, Any]:
        """Simulate toolbar button click (same code path as real click).

        Actions are marshalled onto the GUI thread: automation commands run on
        a worker thread, and creating widgets/QTimers from there produces
        "QObject: Cannot create children for a parent in a different thread".

        Args:
            button: Button name ("open" or "edit_tools")

        Returns:
            Click result
        """
        EventBus.emit("toolbar.clicked", {"button": button})

        # Trigger actual action on the GUI thread
        if button == "open" and self._window:
            _invoke_on_gui_thread(self._window, "_on_open_clicked")
        elif button == "edit_tools" and self._window:
            _invoke_on_gui_thread(self._window, "_on_edit_tools_clicked")

        return {"clicked": button}

    def toolbar_state(self, **kwargs: Any) -> dict[str, Any]:
        """Get toolbar state.

        Returns:
            Toolbar state, including the bottom viewer toolbar button order
            and checked states (empty when no window is attached).
        """
        state: dict[str, Any] = {"buttons": ["open", "edit_tools"]}

        toolbar = getattr(self._window, "bottom_toolbar", None) if self._window else None
        if toolbar is not None:
            left, right = toolbar.centering_margins()
            state["bottom"] = toolbar.button_names()
            state["bottom_visible"] = [
                name for name in toolbar.button_names() if toolbar.is_button_visible(name)
            ]
            state["bottom_checked"] = [
                name for name in toolbar.button_names() if toolbar.is_button_checked(name)
            ]
            state["centered"] = toolbar.is_centered()
            state["margins"] = {"left": left, "right": right}
        return state

    # --- Dialog ---

    def dialog_open(
        self,
        type: str = "open",
        filters: list[str] | None = None,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Open file dialog via portal.

        Args:
            type: "open" or "save"
            filters: File filters

        Returns:
            Dialog handle
        """
        if type == "open":
            handle = portal_dialog.open_file(filters=filters)
        elif type == "save":
            handle = portal_dialog.save_file(filters=filters)
        else:
            raise InvalidArgumentsError("dialog.open", f"unknown type: {type}")

        return {"handle": handle}

    def dialog_result(self, handle: str = "", **kwargs: Any) -> dict[str, Any]:
        """Get dialog result.

        Args:
            handle: Dialog handle

        Returns:
            Dialog result
        """
        result = portal_dialog.get_result(handle)
        return result

    # --- UI (debug) ---

    def shot(self, name: str = "shot", widget: str | None = None, **kwargs: Any) -> dict[str, Any]:
        """Take screenshot of app or widget.

        Args:
            name: Screenshot name
            widget: Widget objectName (default: main window)

        Returns:
            Screenshot metadata
        """
        if self._window is None:
            return {"path": None, "error": "no window (headless mode)"}

        # Find target widget
        target = self._window
        if widget:
            target = self._window.findChild(type(self._window), widget)
            if target is None:
                # Try any QWidget
                from PySide6.QtWidgets import QWidget

                target = self._window.findChild(QWidget, widget)
                if target is None:
                    return {"path": None, "error": f"widget not found: {widget}"}

        # Grab widget
        pixmap = target.grab()

        # Save to file
        shots_dir = Path.home() / ".cache" / "photon" / "shots"
        shots_dir.mkdir(parents=True, exist_ok=True)
        path = shots_dir / f"{name}.png"

        pixmap.save(str(path))

        # Check if blank (all same color)
        image = pixmap.toImage()
        is_blank = True
        first_pixel = image.pixel(0, 0)
        for x in range(0, image.width(), 10):  # Sample every 10 pixels
            for y in range(0, image.height(), 10):
                if image.pixel(x, y) != first_pixel:
                    is_blank = False
                    break
            if not is_blank:
                break

        result = {
            "path": str(path),
            "width": pixmap.width(),
            "height": pixmap.height(),
            "blank": is_blank,
            "dpr": pixmap.devicePixelRatio(),
        }

        EventBus.emit("ui.shot", result)
        return result

    def ui_events(self, n: int = 30, **kwargs: Any) -> dict[str, Any]:
        """Get recent UI events.

        Args:
            n: Number of events

        Returns:
            Recent events
        """
        events = log.get_recent_events(n=n)
        return {"events": events}

    def ui_click(self, widget: str = "", x: int = 0, y: int = 0, **kwargs: Any) -> dict[str, Any]:
        """Click widget by objectName.

        Args:
            widget: Widget objectName
            x: X offset within widget
            y: Y offset within widget

        Returns:
            Click result
        """
        if self._window is None:
            return {"clicked": None, "error": "no window (headless mode)"}

        from PySide6.QtCore import QCoreApplication, QPoint, Qt
        from PySide6.QtGui import QMouseEvent
        from PySide6.QtWidgets import QWidget

        # Find widget
        target = self._window.findChild(QWidget, widget)
        if target is None:
            return {"clicked": None, "error": f"widget not found: {widget}"}

        # Calculate global position
        global_pos = target.mapToGlobal(QPoint(x, y))

        # Post events instead of calling handlers directly: automation runs on
        # a worker thread and Qt widgets may only be touched from the GUI
        # thread (direct calls corrupt Qt state / crash the process).
        press = QMouseEvent(
            QMouseEvent.Type.MouseButtonPress,
            QPoint(x, y),
            global_pos,
            Qt.MouseButton.LeftButton,
            Qt.MouseButton.LeftButton,
            Qt.KeyboardModifier.NoModifier,
        )
        QCoreApplication.postEvent(target, press)

        release = QMouseEvent(
            QMouseEvent.Type.MouseButtonRelease,
            QPoint(x, y),
            global_pos,
            Qt.MouseButton.LeftButton,
            Qt.MouseButton.LeftButton,
            Qt.KeyboardModifier.NoModifier,
        )
        QCoreApplication.postEvent(target, release)

        EventBus.emit("ui.click", {"widget": widget, "x": x, "y": y})
        return {"clicked": widget}

    def ui_key(self, key: str = "", **kwargs: Any) -> dict[str, Any]:
        """Send keyboard shortcut.

        Args:
            key: Key sequence (e.g., "Ctrl+O", "F9")

        Returns:
            Key result
        """
        if self._window is None:
            return {"sent": None, "error": "no window (headless mode)"}
        if not key:
            return {"sent": None, "error": "empty key"}

        # Find matching action, then trigger it on the GUI thread.
        from PySide6.QtCore import Q_ARG, QMetaObject, Qt
        from PySide6.QtGui import QAction, QKeySequence

        wanted = QKeySequence(key)

        # Actions added to the window itself plus actions owned by child
        # widgets (toolbar buttons carry Ctrl+O and F9). The toolbar actions
        # are not in window.actions(), so a plain window scan would report
        # "no action for key" for shortcuts the user can really press.
        candidates = list(self._window.actions())
        seen = {id(action) for action in candidates}
        for action in self._window.findChildren(QAction):
            if id(action) not in seen:
                candidates.append(action)
                seen.add(id(action))

        for action in candidates:
            # Compare normalized sequences: "Delete" and "Del" (or "Ctrl++"
            # and "Ctrl+=") are the same shortcut, so raw string equality
            # silently dropped keystrokes that the user really pressed.
            if wanted.matches(action.shortcut()) == QKeySequence.SequenceMatch.ExactMatch:
                if action in self._window.actions():
                    index = self._window.actions().index(action)
                    QMetaObject.invokeMethod(
                        self._window,
                        "_trigger_action",
                        Qt.ConnectionType.QueuedConnection,
                        Q_ARG(int, index),
                    )
                else:
                    QMetaObject.invokeMethod(
                        action,
                        "trigger",
                        Qt.ConnectionType.QueuedConnection,
                    )
                EventBus.emit("ui.key", {"key": key})
                return {"sent": key}

        return {"sent": None, "error": f"no action for key: {key}"}

    def window_state(self, **kwargs: Any) -> dict[str, Any]:
        """Get window state.

        Returns:
            Window state
        """
        if self._window is None:
            return {"visible": False, "fullscreen": False, "title": None}

        return {
            "visible": self._window.isVisible(),
            "fullscreen": self._window.isFullScreen(),
            "compare": bool(getattr(self._window, "_compare_active", False)),
            "active_frame": self._window.active_frame,
            "title": self._window.windowTitle(),
            "canvas": {
                "width": self._window.canvas.width(),
                "height": self._window.canvas.height(),
                "active": self._window.canvas.active,
                "zoom": self._window.canvas.zoom,
                "rotation": self._window.canvas.rotation,
                "flip_h": self._window.canvas.flip_h,
                "flip_v": self._window.canvas.flip_v,
                "offset_x": self._window.canvas.offset_x,
                "offset_y": self._window.canvas.offset_y,
                "fit": self._window.canvas.fit_mode,
            },
            "compare_pane": self._window.compare_pane.state(),
            "geometry": {
                "x": self._window.x(),
                "y": self._window.y(),
                "width": self._window.width(),
                "height": self._window.height(),
            },
        }

    def window_resize(self, width: int = 1280, height: int = 800, **kwargs: Any) -> dict[str, Any]:
        """Resize window.

        Args:
            width: Window width
            height: Window height

        Returns:
            New size
        """
        if self._window:
            self._window.resize(width, height)

        EventBus.emit("window.resize", {"width": width, "height": height})
        return {"width": width, "height": height}

    # --- Log ---

    def log_tail(self, n: int = 40, **kwargs: Any) -> dict[str, Any]:
        """Get recent log lines.

        Args:
            n: Number of lines

        Returns:
            Log lines
        """
        events = log.get_recent_events(n=n)
        lines = [f"event={e['event']} status={e['status']}" for e in events]
        return {"lines": lines}

    def log_errors(
        self,
        n: int = 10,
        exclude: list[str] | None = None,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Get recent errors.

        Args:
            n: Number of errors
            exclude: Event names to skip (e.g. ``["cmd.error"]`` to ignore the
                expected failures a scenario triggers on purpose)

        Returns:
            Errors
        """
        errors = log.get_errors(n=n)
        if exclude:
            errors = [e for e in errors if e.get("event") not in exclude]
        return {"errors": errors}


# --- Helpers ---


def _get_pid() -> int:
    """Get current process PID."""

    return os.getpid()


def _is_headless() -> bool:
    """Check if running headless."""

    return os.environ.get("PHOTON_HEADLESS", "").lower() in ("1", "true")


def _get_socket_path() -> str:
    """Get socket path."""

    return os.environ.get("PHOTON_SOCKET", str(Path.home() / ".cache" / "photon" / "photon.sock"))
