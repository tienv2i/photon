"""Pillow-based image operations (ZERO Qt imports)."""

from __future__ import annotations

from PIL import Image, ImageOps

from photon.core.events import EventBus


def rotate(img: Image.Image, degrees: int) -> Image.Image:
    """Rotate image.

    Args:
        img: Source image
        degrees: Rotation (90, -90, 180)

    Returns:
        Rotated image
    """
    # PIL rotates counter-clockwise, so negate for clockwise
    result = img.rotate(-degrees, expand=True)
    EventBus.emit("image.rotated", {"degrees": degrees})
    return result


def flip(img: Image.Image, axis: str) -> Image.Image:
    """Flip image.

    Args:
        img: Source image
        axis: "h" (horizontal) or "v" (vertical)

    Returns:
        Flipped image
    """
    if axis == "h":
        result = ImageOps.mirror(img)
    elif axis == "v":
        result = ImageOps.flip(img)
    else:
        raise ValueError(f"Invalid axis: {axis}")

    EventBus.emit("image.flipped", {"axis": axis})
    return result


def resize(
    img: Image.Image,
    max_width: int | None = None,
    max_height: int | None = None,
    scale: float | None = None,
) -> Image.Image:
    """Resize image.

    Args:
        img: Source image
        max_width: Maximum width (keeps aspect ratio)
        max_height: Maximum height (keeps aspect ratio)
        scale: Scale factor (e.g., 0.5 for 50%)

    Returns:
        Resized image
    """
    if scale is not None:
        new_w = int(img.width * scale)
        new_h = int(img.height * scale)
        result = img.resize((new_w, new_h), Image.Resampling.LANCZOS)
    elif max_width or max_height:
        # Calculate scale to fit within bounds
        w_scale = max_width / img.width if max_width else float("inf")
        h_scale = max_height / img.height if max_height else float("inf")
        scale = min(w_scale, h_scale, 1.0)  # Don't upscale

        new_w = int(img.width * scale)
        new_h = int(img.height * scale)
        result = img.resize((new_w, new_h), Image.Resampling.LANCZOS)
    else:
        result = img.copy()

    EventBus.emit(
        "image.resized",
        {"from": (img.width, img.height), "to": (result.width, result.height)},
    )
    return result


def crop(
    img: Image.Image,
    left: int,
    top: int,
    right: int,
    bottom: int,
) -> Image.Image:
    """Crop image.

    Args:
        img: Source image
        left: Left coordinate
        top: Top coordinate
        right: Right coordinate
        bottom: Bottom coordinate

    Returns:
        Cropped image
    """
    result = img.crop((left, top, right, bottom))
    EventBus.emit(
        "image.cropped",
        {"box": (left, top, right, bottom), "size": (result.width, result.height)},
    )
    return result
