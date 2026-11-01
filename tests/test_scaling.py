"""Tests for scaling."""

from __future__ import annotations

import os

from photon.scaling import apply_stylesheet, detect_theme, setup_scaling


def test_setup_scaling() -> None:
    """Test scaling setup."""
    config = setup_scaling()

    assert "dpi_scale" in config
    assert "theme" in config
    assert config["theme"] in ("light", "dark")


def test_detect_theme() -> None:
    """Test theme detection."""
    # Save original
    original_gtk = os.environ.get("GTK_THEME")
    original_colorfgbg = os.environ.get("COLORFGBG")
    original_xdg = os.environ.get("XDG_CURRENT_DESKTOP")

    try:
        # Test dark theme via GTK
        os.environ["GTK_THEME"] = "Adwaita:dark"
        assert detect_theme() == "dark"

        # Test light theme via GTK (must not contain "dark")
        # Also clear COLORFGBG to avoid interference
        os.environ["GTK_THEME"] = "Adwaita-light"
        original_colorfgbg_val = os.environ.pop("COLORFGBG", None)
        assert detect_theme() == "light"
        if original_colorfgbg_val is not None:
            os.environ["COLORFGBG"] = original_colorfgbg_val

        # Test dark via COLORFGBG (clear GTK first)
        del os.environ["GTK_THEME"]
        os.environ["COLORFGBG"] = "15;0"  # Light fg, dark bg
        assert detect_theme() == "dark"

        # Test light via COLORFGBG
        os.environ["COLORFGBG"] = "0;15"  # Dark fg, light bg
        assert detect_theme() == "light"

    finally:
        # Restore
        if original_gtk is not None:
            os.environ["GTK_THEME"] = original_gtk
        elif "GTK_THEME" in os.environ:
            del os.environ["GTK_THEME"]

        if original_colorfgbg is not None:
            os.environ["COLORFGBG"] = original_colorfgbg
        elif "COLORFGBG" in os.environ:
            del os.environ["COLORFGBG"]

        if original_xdg is not None:
            os.environ["XDG_CURRENT_DESKTOP"] = original_xdg
        elif "XDG_CURRENT_DESKTOP" in os.environ:
            del os.environ["XDG_CURRENT_DESKTOP"]


def test_apply_stylesheet_light() -> None:
    """Test light theme stylesheet."""
    css = apply_stylesheet(theme="light")

    assert "background-color: #f5f5f5" in css
    assert "color: #333333" in css
    assert "QToolBar" in css


def test_apply_stylesheet_dark() -> None:
    """Test dark theme stylesheet."""
    css = apply_stylesheet(theme="dark")

    assert "background-color: #2b2b2b" in css
    assert "color: #e0e0e0" in css
    assert "QToolBar" in css


def test_apply_stylesheet_font_scale() -> None:
    """Test font scaling in stylesheet."""
    css = apply_stylesheet(theme="light", font_scale=1.5)

    # Base font is 10pt, scaled by 1.5 = 15pt
    assert "font-size: 15pt" in css
