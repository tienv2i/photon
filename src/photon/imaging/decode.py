"""Pillow-based image decode (ZERO Qt imports)."""

from __future__ import annotations

from pathlib import Path
from typing import Any

from PIL import Image, ImageOps

from photon.core.errors import DecodeError, FileNotFoundError_, UnsupportedFormatError
from photon.core.events import EventBus

# Supported formats
SUPPORTED_FORMATS = {".jpg", ".jpeg", ".png", ".webp", ".gif", ".tiff", ".bmp"}


def decode_image(
    path: Path,
    apply_exif_orientation: bool = True,
) -> tuple[Image.Image, dict[str, Any]]:
    """Decode image file.

    Args:
        path: Path to image file
        apply_exif_orientation: Rotate pixels according to the EXIF
            ``Orientation`` tag. The viewer honors it, so any code that saves
            pixels must use the same setting or phone photos come out sideways.

    Returns:
        Tuple of (PIL Image, metadata dict). ``meta["width"]``/``["height"]``
        always describe the returned image (i.e. post-orientation).

    Raises:
        FileNotFoundError_: If file doesn't exist
        UnsupportedFormatError: If format not supported
        DecodeError: If decode fails
    """
    if not path.exists():
        raise FileNotFoundError_(str(path))

    if path.suffix.lower() not in SUPPORTED_FORMATS:
        raise UnsupportedFormatError(path.suffix)

    try:
        img = Image.open(path)
        img.load()  # Force decode to catch errors early

        exif = {}
        if hasattr(img, "_getexif") and img._getexif():
            exif = dict(img._getexif())

        if apply_exif_orientation:
            img = ImageOps.exif_transpose(img) or img

        # Extract metadata
        meta = {
            "path": str(path),
            "width": img.width,
            "height": img.height,
            "format": img.format or path.suffix.upper().lstrip("."),
            "mode": img.mode,
            "size_bytes": path.stat().st_size,
            "exif": exif,
        }

        EventBus.emit("image.decoded", meta)

        return img, meta

    except Exception as e:
        raise DecodeError(str(path), str(e)) from e


def probe_image(path: Path) -> dict[str, Any]:
    """Read image metadata without decoding the pixel data.

    Viewers need width/height/format before painting; a full ``decode_image``
    costs tens of milliseconds on large photos and used to run twice per file
    open (once here, once for the pixmap). Header probing keeps the UI thread
    responsive.

    Args:
        path: Path to image file

    Returns:
        Metadata dict (same keys as ``decode_image``), with width/height
        already accounting for EXIF orientation.

    Raises:
        FileNotFoundError_: If file doesn't exist
        UnsupportedFormatError: If format not supported
        DecodeError: If the header cannot be read
    """
    if not path.exists():
        raise FileNotFoundError_(str(path))

    if path.suffix.lower() not in SUPPORTED_FORMATS:
        raise UnsupportedFormatError(path.suffix)

    try:
        with Image.open(path) as img:
            width, height = img.width, img.height
            # Orientations 5-8 swap the axes; report the size the viewer
            # will actually draw so status bar / fit math stay correct.
            exif = {}
            if hasattr(img, "_getexif") and img._getexif():
                exif = dict(img._getexif())
                if exif.get(0x0112) in (5, 6, 7, 8):
                    width, height = height, width
            meta = {
                "path": str(path),
                "width": width,
                "height": height,
                "format": img.format or path.suffix.upper().lstrip("."),
                "mode": img.mode,
                "size_bytes": path.stat().st_size,
                "exif": exif,
            }
        EventBus.emit("image.probed", meta)
        return meta
    except Exception as e:
        raise DecodeError(str(path), str(e)) from e


def create_thumbnail(img: Image.Image, max_size: int = 256) -> Image.Image:
    """Create thumbnail of image.

    Args:
        img: Source image
        max_size: Maximum size (width or height)

    Returns:
        Thumbnail image
    """
    thumb = img.copy()
    thumb.thumbnail((max_size, max_size), Image.Resampling.LANCZOS)
    return thumb


def get_exif(path: Path) -> dict[str, Any]:
    """Extract EXIF metadata from image.

    Args:
        path: Path to image file

    Returns:
        EXIF dict (empty if no EXIF)
    """
    try:
        img = Image.open(path)
        if hasattr(img, "_getexif") and img._getexif():
            return dict(img._getexif())
    except Exception:
        pass
    return {}
