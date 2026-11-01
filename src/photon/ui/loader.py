"""Background image loading for the canvas (QThreadPool).

Decoding a large photo with Pillow takes tens to hundreds of milliseconds.
Doing that on the GUI thread freezes the window and violates the project rule
"never block the UI thread for more than 50 ms". Workers decode into a
``QImage`` (safe off the GUI thread); only the ``QPixmap`` conversion and the
signal emission happen on the GUI thread.
"""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import QObject, QRunnable, QThreadPool, Signal
from PySide6.QtGui import QImage

from photon.core.events import EventBus


class ImageLoadSignals(QObject):
    """Signals emitted by :class:`ImageLoadTask`."""

    loaded = Signal(str, QImage)  # path, image
    failed = Signal(str, str)  # path, error


class ImageLoadTask(QRunnable):
    """Decode an image file on a worker thread."""

    def __init__(self, path: Path, signals: ImageLoadSignals) -> None:
        super().__init__()
        self._path = Path(path)
        self._signals = signals

    def run(self) -> None:
        """Decode the file into a QImage and emit the result."""
        try:
            image = load_qimage(self._path)
        except Exception as e:  # noqa: BLE001 - reported through the signal
            self._signals.failed.emit(str(self._path), str(e))
            return
        self._signals.loaded.emit(str(self._path), image)


def load_qimage(path: Path) -> QImage:
    """Decode ``path`` into a QImage (runs on a worker thread).

    Args:
        path: Image file to decode

    Returns:
        Owning QImage (a copy, so the Pillow buffer can be freed)

    Raises:
        DecodeError: If the file cannot be decoded
    """
    from PIL import Image, ImageOps

    from photon.core.errors import DecodeError

    try:
        with Image.open(path) as img:
            img.load()
            # Honor the camera's orientation metadata; without this, phone
            # photos show up rotated and "save copy" writes them that way.
            image = ImageOps.exif_transpose(img) or img
            image = image.convert("RGBA")
            width, height = image.size
            data = image.tobytes("raw", "RGBA")
    except Exception as e:
        raise DecodeError(str(path), str(e)) from e

    qimage = QImage(
        data,
        width,
        height,
        width * 4,
        QImage.Format.Format_RGBA8888,
    )
    # Copy so the QImage does not reference the temporary Python buffer.
    return qimage.copy()


class ImageLoader(QObject):
    """Queue image decodes on the global thread pool."""

    loaded = Signal(str, QImage)
    failed = Signal(str, str)

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._signals = ImageLoadSignals()
        self._signals.loaded.connect(self._on_loaded)
        self._signals.failed.connect(self._on_failed)
        self._pending_path: str | None = None

    def load(self, path: Path) -> None:
        """Start decoding ``path`` in the background.

        Only the newest request is kept: loading a folder quickly would
        otherwise paint stale images over the current one.

        Args:
            path: Image file to decode
        """
        self._pending_path = str(path)
        QThreadPool.globalInstance().start(ImageLoadTask(path, self._signals))

    def _on_loaded(self, path: str, image: QImage) -> None:
        if path != self._pending_path:
            return  # superseded by a newer request
        EventBus.emit("image.decoded_async", {"path": path})
        self.loaded.emit(path, image)

    def _on_failed(self, path: str, error: str) -> None:
        if path != self._pending_path:
            return
        self.failed.emit(path, error)
