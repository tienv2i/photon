"""System DPI scaling and theme detection."""

from __future__ import annotations

import contextlib
import os
from typing import Any


def setup_scaling() -> dict[str, Any]:
    """Setup system scaling (must be called before QApplication).

    Detects:
    - DPI scale factor
    - System theme (light/dark)
    - Font scaling

    Returns:
        Scaling config dict
    """
    config: dict[str, Any] = {
        "dpi_scale": 1.0,
        "theme": "light",
        "font_size": 10,
    }

    # Check Qt scaling env vars first
    if os.environ.get("QT_ENABLE_HIGHDPI_SCALING"):
        # Qt handles it
        pass
    else:
        # Enable Qt high-DPI scaling
        os.environ.setdefault("QT_ENABLE_HIGHDPI_SCALING", "1")
        os.environ.setdefault("QT_AUTO_SCREEN_SCALE_FACTOR", "1")

    # Detect system theme
    config["theme"] = detect_theme()

    # Detect font scale from environment
    if os.environ.get("PHOTON_FONT_SCALE"):
        with contextlib.suppress(ValueError):
            config["font_scale"] = float(os.environ["PHOTON_FONT_SCALE"])

    return config


def detect_theme() -> str:
    """Detect system theme (light/dark).

    Returns:
        "light" or "dark"
    """
    # Check GTK/GNOME theme
    gtk_theme = os.environ.get("GTK_THEME", "").lower()
    if "dark" in gtk_theme:
        return "dark"

    # Check KDE color scheme
    kde_color = os.environ.get("KDE_COLOR_SCHEME", "").lower()
    if "dark" in kde_color:
        return "dark"

    # Check XDG desktop
    xdg_current = os.environ.get("XDG_CURRENT_DESKTOP", "").lower()
    if "gnome" in xdg_current or "kde" in xdg_current:
        # Try to detect from settings (simplified)
        # TODO: Query actual theme from gsettings/kreadconfig
        pass

    # Check color scheme env
    if os.environ.get("COLORFGBG"):
        # Format: fg;bg (e.g., "15;0" = light on dark)
        try:
            parts = os.environ["COLORFGBG"].split(";")
            bg = int(parts[1])
            if bg < 10:  # Dark background
                return "dark"
        except (ValueError, IndexError):
            pass

    # Default to light
    return "light"


def get_scale_factor() -> float:
    """Get DPI scale factor.

    Returns:
        Scale factor (1.0 = 100%)
    """
    # Check env override
    if os.environ.get("PHOTON_SCALE"):
        try:
            return float(os.environ["PHOTON_SCALE"])
        except ValueError:
            pass

    # Let Qt handle it (returns actual DPI after QApplication init)
    from PySide6.QtWidgets import QApplication

    if QApplication.instance():
        screen = QApplication.primaryScreen()
        if screen:
            return screen.devicePixelRatio()

    return 1.0


def apply_stylesheet(theme: str = "light", font_scale: float = 1.0) -> str:
    """Generate Qt stylesheet for theme.

    Args:
        theme: "light" or "dark"
        font_scale: Font scale factor

    Returns:
        Qt stylesheet string
    """
    base_font = int(10 * font_scale)

    if theme == "dark":
        return f"""
        QWidget {{
            background-color: #2b2b2b;
            color: #e0e0e0;
            font-size: {base_font}pt;
        }}
        QToolBar {{
            background-color: #3c3c3c;
            border-bottom: 1px solid #555;
            spacing: 6px;
            padding: 4px;
        }}
        QToolBar QToolButton {{
            background-color: transparent;
            border: 1px solid transparent;
            border-radius: 4px;
            padding: 6px 12px;
        }}
        QToolBar QToolButton:hover {{
            background-color: #505050;
            border-color: #666;
        }}
        QToolBar QToolButton:pressed {{
            background-color: #404040;
        }}
        QStatusBar {{
            background-color: #3c3c3c;
            border-top: 1px solid #555;
        }}
        #sidebar_left, #sidebar_right {{
            background-color: #333333;
            border-right: 1px solid #555;
        }}
        #canvas {{
            background-color: #1e1e1e;
        }}
        """
    else:
        return f"""
        QWidget {{
            background-color: #f5f5f5;
            color: #333333;
            font-size: {base_font}pt;
        }}
        QToolBar {{
            background-color: #e8e8e8;
            border-bottom: 1px solid #ccc;
            spacing: 6px;
            padding: 4px;
        }}
        QToolBar QToolButton {{
            background-color: transparent;
            border: 1px solid transparent;
            border-radius: 4px;
            padding: 6px 12px;
        }}
        QToolBar QToolButton:hover {{
            background-color: #d0d0d0;
            border-color: #bbb;
        }}
        QToolBar QToolButton:pressed {{
            background-color: #c0c0c0;
        }}
        QStatusBar {{
            background-color: #e8e8e8;
            border-top: 1px solid #ccc;
        }}
        #sidebar_left, #sidebar_right {{
            background-color: #eeeeee;
            border-right: 1px solid #ccc;
        }}
        #canvas {{
            background-color: #ffffff;
        }}
        """
