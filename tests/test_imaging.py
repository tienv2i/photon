"""Tests for imaging operations."""

from __future__ import annotations

from pathlib import Path

import pytest
from PIL import Image

from photon.imaging.decode import decode_image
from photon.imaging.encode import encode_image
from photon.imaging.ops import flip, resize, rotate


@pytest.fixture
def test_image(tmp_path: Path) -> Path:
    """Create a test image."""
    path = tmp_path / "test.png"
    img = Image.new("RGB", (100, 50), color="red")
    img.save(path)
    return path


def test_probe_image_reads_header_only(test_image: Path) -> None:
    """probe_image returns the same metadata keys as a full decode."""
    from photon.imaging.decode import probe_image

    meta = probe_image(test_image)
    assert meta["width"] == 100
    assert meta["height"] == 50
    assert meta["format"] == "PNG"
    assert meta["size_bytes"] > 0


def test_probe_image_missing(tmp_path: Path) -> None:
    """probe_image raises the same error type as decode_image."""
    from photon.core.errors import FileNotFoundError_
    from photon.imaging.decode import probe_image

    with pytest.raises(FileNotFoundError_):
        probe_image(tmp_path / "nope.png")


def test_exif_orientation_applied_everywhere(tmp_path: Path) -> None:
    """A rotated-by-EXIF photo must report and save the displayed size."""
    from photon.automation.api import AutomationAPI
    from photon.imaging.decode import probe_image

    src = tmp_path / "phone.jpg"
    img = Image.new("RGB", (400, 200), "red")
    exif = img.getexif()
    exif[0x0112] = 6  # orientation: rotate 90° clockwise for display
    img.save(src, exif=exif, quality=95)

    _, meta = decode_image(src)
    probed = probe_image(src)
    assert (meta["width"], meta["height"]) == (200, 400)
    assert (probed["width"], probed["height"]) == (200, 400)

    api = AutomationAPI()
    api.file_open(str(src))
    out = tmp_path / "saved.jpg"
    result = api.file_save_copy(path=str(out))
    assert (result["width"], result["height"]) == (200, 400)


def test_decode_image(test_image: Path) -> None:
    """Test decoding image."""
    img, meta = decode_image(test_image)

    assert img.size == (100, 50)
    assert meta["width"] == 100
    assert meta["height"] == 50
    assert meta["format"] == "PNG"


def test_decode_nonexistent(tmp_path: Path) -> None:
    """Test decoding nonexistent file."""
    from photon.core.errors import FileNotFoundError_

    with pytest.raises(FileNotFoundError_):
        decode_image(tmp_path / "nonexistent.jpg")


def test_encode_image(test_image: Path, tmp_path: Path) -> None:
    """Test encoding image."""
    img, _ = decode_image(test_image)

    output = tmp_path / "output.jpg"
    result = encode_image(img, output, quality=85)

    assert output.exists()
    assert result["format"] == "JPEG"
    assert result["quality"] == 85


def test_rotate() -> None:
    """Test rotation."""
    img = Image.new("RGB", (100, 50), color="red")

    rotated = rotate(img, 90)
    assert rotated.size == (50, 100)

    rotated = rotate(img, 180)
    assert rotated.size == (100, 50)


def test_flip() -> None:
    """Test flip."""
    img = Image.new("RGB", (100, 50), color="red")

    flipped_h = flip(img, "h")
    assert flipped_h.size == (100, 50)

    flipped_v = flip(img, "v")
    assert flipped_v.size == (100, 50)


def test_resize() -> None:
    """Test resize."""
    img = Image.new("RGB", (100, 50), color="red")

    # Resize with max_width
    resized = resize(img, max_width=50)
    assert resized.width == 50
    assert resized.height == 25  # Aspect ratio preserved

    # Resize with scale
    resized = resize(img, scale=0.5)
    assert resized.size == (50, 25)
