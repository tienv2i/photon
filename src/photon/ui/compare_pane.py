"""Side-by-side compare pane: a second, independent image frame.

The pane is a :class:`~photon.ui.canvas.Canvas` subclass, so zoom, pan, fit,
rotation and drag handling behave exactly like the main view. It is *not*
linked to the main canvas: each frame owns its own zoom/rotation/flip/pan, and
the toolbar (and ``view.*`` automation commands) act on whichever frame is
active. Clicking a frame makes it active (accent border), so a comparison can
show the reference at a different zoom or orientation without disturbing the
main image.
"""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import QRect, Qt, Signal
from PySide6.QtGui import QColor, QPainter, QPaintEvent, QPixmap
from PySide6.QtWidgets import QVBoxLayout, QWidget

from photon.core.events import EventBus
from photon.ui.canvas import Canvas


class ComparePane(QWidget):
    """Right-hand frame that holds a reference image for comparison."""

    #: Emitted with the path when the user drops a file on the pane.
    imageDropped = Signal(str)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self.setObjectName("compare_pane")

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        self.canvas = CompareCanvas(self)
        self.canvas.setObjectName("compare_canvas")
        layout.addWidget(self.canvas, 1)

        self.canvas.fileDropped.connect(self._on_dropped)

    # --- Content ---

    def set_image(self, path: Path, pixmap: QPixmap) -> None:
        """Place the reference image (already decoded by the caller).

        Decoding is intentionally not done here: both call sites decode on a
        worker thread (``ImageLoader``) or reuse the already-loaded pixmap, so
        the GUI thread never blocks on Pillow.

        Args:
            path: Path the pixmap came from (shown in the caption)
            pixmap: Decoded reference image
        """
        path = Path(path)
        self.canvas.set_pixmap(pixmap, path)
        self.canvas.fit_to_window()

        EventBus.emit("compare.image_set", {"path": str(path)})

    def clear(self) -> None:
        """Remove the reference image."""
        self.canvas.clear()
        EventBus.emit("compare.image_cleared", {})

    def _on_dropped(self, path: str) -> None:
        self.imageDropped.emit(path)

    # --- State ---

    @property
    def has_image(self) -> bool:
        """Whether the pane currently shows an image."""
        return self.canvas.pixmap is not None

    @property
    def path(self) -> str | None:
        """Path of the reference image, if any."""
        path = self.canvas.image_path
        return str(path) if path is not None else None

    @property
    def name(self) -> str | None:
        """File name of the reference image, if any."""
        path = self.canvas.image_path
        return path.name if path is not None else None

    def state(self) -> dict[str, object]:
        """Snapshot used by automation (``compare.state`` / ``window.state``)."""
        return {"visible": self.isVisible(), **self.canvas.state()}


class CompareCanvas(Canvas):
    """Canvas variant with its own placeholder text and file-name caption."""

    empty_text_key = "canvas.compare_empty"

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent, accept_drops=True)

    def paintEvent(self, event: QPaintEvent) -> None:
        """Draw the image/border, then the reference file name."""
        super().paintEvent(event)

        path = self.image_path
        if path is None:
            return

        painter = QPainter(self)
        font = painter.font()
        font.setPointSize(9)
        painter.setFont(font)

        metrics = painter.fontMetrics()
        text = metrics.elidedText(
            path.name,
            Qt.TextElideMode.ElideMiddle,
            max(40, self.width() - 28),
        )
        text_size = metrics.size(Qt.TextFlag.TextSingleLine, text)
        chip = QRect(8, 6, text_size.width() + 12, text_size.height() + 6)

        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(QColor(0, 0, 0, 150))
        painter.drawRoundedRect(chip, 4, 4)

        painter.setPen(QColor(235, 235, 235))
        painter.drawText(chip, Qt.AlignmentFlag.AlignCenter, text)
