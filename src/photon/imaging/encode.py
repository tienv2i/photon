"""Pillow-based image encode (ZERO Qt imports)."""

from __future__ import annotations

from pathlib import Path
from typing import Any

from PIL import Image

from photon.core.errors import EncodeError
from photon.core.events import EventBus


def encode_image(
    img: Image.Image,
    path: Path,
    format: str | None = None,
    quality: int = 90,
    **kwargs: Any,
) -> dict[str, Any]:
    """Encode and save image.

    Args:
        img: PIL Image to save
        path: Output path
        format: Format override (jpg, png, webp, etc.) — default from extension
        quality: Quality (1-100) for lossy formats
        **kwargs: Additional save options

    Returns:
        Metadata dict with saved path and size

    Raises:
        EncodeError: If encode fails
    """
    # Determine format from extension if not specified
    if format is None:
        ext = path.suffix.lower().lstrip(".")
        format = "JPEG" if ext in ("jpg", "jpeg") else ext.upper()

    # Normalize format name
    format_map = {"JPG": "JPEG", "TIF": "TIFF"}
    format = format_map.get(format.upper(), format.upper())

    try:
        # Convert RGBA to RGB for JPEG
        if format == "JPEG" and img.mode in ("RGBA", "P"):
            img = img.convert("RGB")

        # Save options
        save_opts: dict[str, Any] = {"format": format, **kwargs}

        # Quality only for lossy formats
        if format in ("JPEG", "WEBP"):
            save_opts["quality"] = quality

        # Save to file
        img.save(path, **save_opts)

        meta = {
            "saved_to": str(path),
            "format": format,
            "quality": quality,
            "size_bytes": path.stat().st_size,
            "width": img.width,
            "height": img.height,
        }

        EventBus.emit("image.saved", meta)

        return meta

    except Exception as e:
        raise EncodeError(str(path), str(e)) from e
