"""Zoom/pan/fit image display."""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import QMimeData, QPoint, QRectF, Qt, Signal
from PySide6.QtGui import (
    QColor,
    QDragEnterEvent,
    QDragMoveEvent,
    QDropEvent,
    QMouseEvent,
    QPainter,
    QPaintEvent,
    QPen,
    QPixmap,
    QResizeEvent,
    QWheelEvent,
)
from PySide6.QtWidgets import QWidget

from photon.core.events import EventBus
from photon.i18n import t


class Canvas(QWidget):
    """Image canvas with zoom and pan.

    Features:
    - Display image with zoom/pan
    - Fit-to-window
    - Mouse wheel zoom (anchored at the cursor)
    - Click-drag pan, clamped so the image never leaves the viewport
    - Optional drag & drop of image files
    """

    # Signals
    zoomChanged = Signal(float)  # zoom factor
    viewChanged = Signal()  # zoom, pan, rotation or flip changed
    activated = Signal()  # the user clicked this canvas (make it the active frame)
    fileDropped = Signal(str)  # path of a dropped image

    #: Translation key used for the "no image" placeholder.
    empty_text_key = "canvas.no_image"

    def __init__(self, parent: QWidget | None = None, accept_drops: bool = False) -> None:
        super().__init__(parent)

        self.setObjectName("canvas")
        self.setMinimumSize(200, 200)
        self.setMouseTracking(True)

        # Image
        self._pixmap: QPixmap | None = None
        self._image_path: Path | None = None

        # Frame selection: the active frame receives the toolbar commands.
        self._active: bool = False

        # View state
        self._zoom: float = 1.0
        self._rotation: int = 0  # 0, 90, 180, 270
        self._flip_h: bool = False
        self._flip_v: bool = False
        self._offset: QPoint = QPoint(0, 0)
        self._panning: bool = False
        self._pan_start: QPoint = QPoint(0, 0)
        # When True the zoom tracks the window size (fit-to-window mode).
        self._fit_mode: bool = False

        # Background color
        self.setAutoFillBackground(True)

        if accept_drops:
            self.setAcceptDrops(True)

    def set_image(self, path: Path) -> None:
        """Load and display image.

        Args:
            path: Path to image file
        """
        pixmap = QPixmap(str(path))
        if pixmap.isNull():
            EventBus.emit("canvas.load_failed", {"path": str(path)})
            return

        self.set_pixmap(pixmap, path)

    def set_pixmap(self, pixmap: QPixmap, path: Path) -> None:
        """Display an already-decoded pixmap.

        Used by the background loader so the GUI thread never decodes.

        Args:
            pixmap: Decoded image
            path: Source file (for events/debugging)
        """
        self._pixmap = pixmap
        self._image_path = path
        self._zoom = 1.0
        self._rotation = 0
        self._flip_h = False
        self._flip_v = False
        self._offset = QPoint(0, 0)

        self.update()
        self.viewChanged.emit()
        EventBus.emit(
            "canvas.image_set",
            {"path": str(path), "size": (pixmap.width(), pixmap.height())},
        )

    def clear(self) -> None:
        """Remove the displayed image and reset the view."""
        self._pixmap = None
        self._image_path = None
        self._zoom = 1.0
        self._rotation = 0
        self._flip_h = False
        self._flip_v = False
        self._offset = QPoint(0, 0)
        self._fit_mode = False
        self.update()
        self.viewChanged.emit()
        EventBus.emit("canvas.cleared", {})

    @property
    def pixmap(self) -> QPixmap | None:
        """Currently displayed pixmap (None when empty)."""
        return self._pixmap

    @property
    def image_path(self) -> Path | None:
        """Path of the displayed image (None when empty)."""
        return self._image_path

    def set_zoom(self, zoom: float) -> None:
        """Set zoom level (disables fit-to-window tracking).

        Args:
            zoom: Zoom factor (1.0 = 100%)
        """
        self._fit_mode = False
        self._zoom = max(0.1, min(10.0, zoom))
        # Zooming out can leave the pan offset outside the image bounds.
        self._clamp_offset()
        self.zoomChanged.emit(self._zoom)
        self.update()
        self.viewChanged.emit()

        EventBus.emit("canvas.zoom", {"zoom": self._zoom})

    def set_rotation(self, degrees: int) -> None:
        """Rotate the displayed image.

        Args:
            degrees: Absolute rotation in degrees (0, 90, 180, 270)
        """
        self._rotation = degrees % 360
        self._offset = QPoint(0, 0)

        if self._fit_mode:
            # Rotation changes the effective bounds, so refit.
            self.fit_to_window()
        else:
            self._clamp_offset()
            self.update()
            self.viewChanged.emit()

        EventBus.emit("canvas.rotate", {"rotation": self._rotation})

    def effective_size(self) -> tuple[float, float]:
        """Size of the drawn image in viewport pixels (post zoom/rotation).

        Returns:
            (width, height); (0, 0) when no image is loaded. Flips do not
            change the bounding box.
        """
        if self._pixmap is None:
            return (0.0, 0.0)

        width = self._pixmap.width() * self._zoom
        height = self._pixmap.height() * self._zoom
        if self._rotation % 180 != 0:
            width, height = height, width
        return (width, height)

    def offset_bounds(self) -> tuple[int, int]:
        """Maximum absolute pan offset (x, y) before the image leaves the view.

        The image may only be dragged until one of its edges reaches the
        matching viewport edge. An image that fits inside an axis cannot be
        panned along that axis at all, so it stays centred instead of being
        flung off screen.

        Returns:
            (max_x, max_y) in pixels, both >= 0.
        """
        eff_w, eff_h = self.effective_size()
        if eff_w <= 0 or eff_h <= 0:
            return (0, 0)

        max_x = max(0.0, (eff_w - self.width()) / 2.0)
        max_y = max(0.0, (eff_h - self.height()) / 2.0)
        return (int(max_x), int(max_y))

    def _clamp_offset(self) -> None:
        """Keep the pan offset inside :meth:`offset_bounds`."""
        max_x, max_y = self.offset_bounds()
        x = max(-max_x, min(max_x, self._offset.x()))
        y = max(-max_y, min(max_y, self._offset.y()))
        self._offset = QPoint(x, y)

    def pan(self, dx: int, dy: int) -> None:
        """Pan by a delta, clamped to the image bounds.

        Args:
            dx: Horizontal delta in pixels (positive = right)
            dy: Vertical delta in pixels (positive = down)
        """
        self._offset += QPoint(dx, dy)
        self._clamp_offset()
        self.update()
        self.viewChanged.emit()
        EventBus.emit(
            "canvas.pan",
            {"dx": self._offset.x(), "dy": self._offset.y()},
        )

    def center(self) -> None:
        """Reset the pan offset so the image is centred."""
        self._offset = QPoint(0, 0)
        self.update()
        self.viewChanged.emit()

    @property
    def offset_x(self) -> int:
        """Current horizontal pan offset in pixels."""
        return self._offset.x()

    @property
    def offset_y(self) -> int:
        """Current vertical pan offset in pixels."""
        return self._offset.y()

    def set_flip(self, horizontal: bool, vertical: bool) -> None:
        """Mirror the displayed image.

        Args:
            horizontal: Mirror left/right
            vertical: Mirror top/bottom
        """
        self._flip_h = horizontal
        self._flip_v = vertical
        self.update()
        self.viewChanged.emit()

        EventBus.emit("canvas.flip", {"flip_h": self._flip_h, "flip_v": self._flip_v})

    @property
    def zoom(self) -> float:
        """Current zoom factor."""
        return self._zoom

    @property
    def active(self) -> bool:
        """Whether this canvas is the active frame (receives toolbar commands)."""
        return self._active

    def set_active(self, active: bool) -> None:
        """Mark this canvas as the active frame (draws an accent border).

        Args:
            active: True to highlight and receive toolbar commands
        """
        if active == self._active:
            return
        self._active = active
        self.update()

    def reset_view(self) -> None:
        """Reset rotation, flips and zoom to the defaults."""
        self._rotation = 0
        self._flip_h = False
        self._flip_v = False
        self._offset = QPoint(0, 0)
        self._fit_mode = False
        self._zoom = 1.0
        self.update()
        self.zoomChanged.emit(self._zoom)
        self.viewChanged.emit()

    def state(self) -> dict[str, object]:
        """Snapshot of the view state (used by automation ``frame.state``)."""
        pixmap = self._pixmap
        return {
            "active": self._active,
            "path": str(self._image_path) if self._image_path is not None else None,
            "width": pixmap.width() if pixmap is not None else 0,
            "height": pixmap.height() if pixmap is not None else 0,
            "zoom": self._zoom,
            "rotation": self._rotation,
            "flip_h": self._flip_h,
            "flip_v": self._flip_v,
            "offset_x": self._offset.x(),
            "offset_y": self._offset.y(),
            "fit": self._fit_mode,
        }

    @property
    def rotation(self) -> int:
        """Current rotation in degrees."""
        return self._rotation

    @property
    def flip_h(self) -> bool:
        """Whether the image is mirrored horizontally."""
        return self._flip_h

    @property
    def flip_v(self) -> bool:
        """Whether the image is mirrored vertically."""
        return self._flip_v

    @property
    def fit_mode(self) -> bool:
        """Whether zoom tracks the window size (fit-to-window)."""
        return self._fit_mode

    def fit_to_window(self) -> None:
        """Fit image to window size and keep tracking window resizes."""
        self._fit_mode = True

        if self._pixmap is None:
            return

        # Calculate zoom to fit
        pixmap_w = self._pixmap.width()
        pixmap_h = self._pixmap.height()

        if pixmap_w == 0 or pixmap_h == 0:
            return

        # Rotated bounds: 90/270 swap width and height.
        if self._rotation % 180 == 0:
            eff_w, eff_h = pixmap_w, pixmap_h
        else:
            eff_w, eff_h = pixmap_h, pixmap_w

        zoom_w = self.width() / eff_w
        zoom_h = self.height() / eff_h
        self._zoom = max(0.05, min(zoom_w, zoom_h, 1.0))  # Don't upscale small images

        # Center
        self._offset = QPoint(0, 0)

        self.zoomChanged.emit(self._zoom)
        self.update()
        self.viewChanged.emit()

        EventBus.emit("canvas.fit", {"zoom": self._zoom})

    def paintEvent(self, event: QPaintEvent) -> None:
        """Paint image (or the placeholder text when empty), plus frame border."""
        painter = QPainter(self)

        if self._pixmap is None:
            painter.setPen(Qt.GlobalColor.gray)
            painter.drawText(
                self.rect(),
                Qt.AlignmentFlag.AlignCenter,
                t(self.empty_text_key),
            )
        else:
            painter.setRenderHint(QPainter.RenderHint.SmoothPixmapTransform)

            # Calculate scaled size
            scaled_w = self._pixmap.width() * self._zoom
            scaled_h = self._pixmap.height() * self._zoom

            # Rotate around the viewport centre, then draw centred + offset
            painter.translate(
                self.width() / 2 + self._offset.x(),
                self.height() / 2 + self._offset.y(),
            )
            painter.rotate(self._rotation)
            # Flips apply in screen space, i.e. after rotation, matching the
            # order used when the transform stack is baked into a saved copy.
            painter.scale(-1.0 if self._flip_h else 1.0, -1.0 if self._flip_v else 1.0)
            target = QRectF(-scaled_w / 2, -scaled_h / 2, scaled_w, scaled_h)
            painter.drawPixmap(target, self._pixmap, QRectF(self._pixmap.rect()))

        self._paint_frame_border(painter)

    def _paint_frame_border(self, painter: QPainter) -> None:
        """Draw the active-frame highlight (2 px accent) or a subtle divider."""
        # The image branch leaves a translate/rotate/scale on the painter; the
        # border must be drawn in widget coordinates.
        painter.resetTransform()
        painter.setBrush(Qt.BrushStyle.NoBrush)
        if self._active:
            pen = QPen(QColor(74, 158, 255))
            pen.setWidth(2)
            painter.setPen(pen)
            painter.drawRect(self.rect().adjusted(1, 1, -2, -2))
        else:
            painter.setPen(QColor(70, 70, 70))
            painter.drawRect(self.rect().adjusted(0, 0, -1, -1))

    def wheelEvent(self, event: QWheelEvent) -> None:
        """Handle mouse wheel zoom, anchored at the cursor."""
        delta = event.angleDelta().y()
        if delta == 0:
            event.ignore()
            return

        factor = 1.1 if delta > 0 else 1 / 1.1
        # Interacting with a frame makes it the target of the toolbar.
        self.activated.emit()

        # Keep the point under the cursor stationary while zooming.
        cursor = event.position()
        centre_x = self.width() / 2 + self._offset.x()
        centre_y = self.height() / 2 + self._offset.y()

        old_zoom = self._zoom
        new_zoom = max(0.1, min(10.0, old_zoom * factor))
        if new_zoom == old_zoom:
            event.accept()
            return

        ratio = new_zoom / old_zoom
        self._offset = QPoint(
            int(round(cursor.x() - centre_x * ratio + self.width() / 2)),
            int(round(cursor.y() - centre_y * ratio + self.height() / 2)),
        )
        self._fit_mode = False
        self._zoom = new_zoom
        self._clamp_offset()
        self.zoomChanged.emit(self._zoom)
        self.update()
        self.viewChanged.emit()

        EventBus.emit("canvas.zoom", {"zoom": self._zoom})
        event.accept()

    def mousePressEvent(self, event: QMouseEvent) -> None:
        """Start panning on left/middle click (cursor changes to a hand)."""
        if event.button() in (Qt.MouseButton.LeftButton, Qt.MouseButton.MiddleButton):
            # Any click selects this frame: toolbar commands follow the click.
            self.activated.emit()
            self._panning = True
            self._pan_start = event.position().toPoint()
            self.setCursor(Qt.CursorShape.ClosedHandCursor)
            event.accept()

    def mouseMoveEvent(self, event: QMouseEvent) -> None:
        """Pan the image, clamped so it cannot be dragged off screen."""
        if not self._panning:
            return

        pos = event.position().toPoint()
        delta = pos - self._pan_start
        self._pan_start = pos
        self.pan(delta.x(), delta.y())
        event.accept()

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:
        """End pan."""
        if self._panning:
            self._panning = False
            self.setCursor(Qt.CursorShape.ArrowCursor)
            event.accept()

    def mouseDoubleClickEvent(self, event: QMouseEvent) -> None:
        """Double click toggles between fit and 100%."""
        if event.button() != Qt.MouseButton.LeftButton:
            return

        if self._fit_mode:
            self.set_zoom(1.0)
        else:
            self.fit_to_window()
        event.accept()

    def resizeEvent(self, event: QResizeEvent) -> None:
        """Handle resize: keep the image fitted while in fit mode."""
        super().resizeEvent(event)
        if self._fit_mode and self._pixmap is not None:
            self.fit_to_window()
        elif self._pixmap is not None:
            self._clamp_offset()
            self.viewChanged.emit()

    # --- Drag & drop (used by the compare pane) ---

    def dragEnterEvent(self, event: QDragEnterEvent) -> None:
        """Accept a drag when it carries an image file."""
        if self._dropped_image(event.mimeData()) is not None:
            event.acceptProposedAction()

    def dragMoveEvent(self, event: QDragMoveEvent) -> None:
        """Keep accepting the drag while it moves over the canvas."""
        if self._dropped_image(event.mimeData()) is not None:
            event.acceptProposedAction()

    def dropEvent(self, event: QDropEvent) -> None:
        """Emit ``fileDropped`` for the first image in the payload."""
        path = self._dropped_image(event.mimeData())
        if path is None:
            return
        event.acceptProposedAction()
        self.fileDropped.emit(str(path))

    @staticmethod
    def _dropped_image(mime: QMimeData) -> Path | None:
        """Return the first droppable image path from ``mime``, if any."""
        if not mime.hasUrls():
            return None

        from photon.imaging.decode import SUPPORTED_FORMATS

        for url in mime.urls():
            if not url.isLocalFile():
                continue
            path = Path(url.toLocalFile())
            if path.is_file() and path.suffix.lower() in SUPPORTED_FORMATS:
                return path
        return None
