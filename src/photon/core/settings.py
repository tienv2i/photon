"""JSON config in ~/.config/photon."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

# Default config
_defaults: dict[str, Any] = {
    "window": {
        "width": 1280,
        "height": 800,
        "x": 100,
        "y": 100,
    },
    "sidebar": {
        "left": {"visible": False, "width": 240},
        "right": {"visible": False, "width": 280},
    },
    "recent_files": [],
    "last_folder": "",
}

# Current config (loaded from file)
_config: dict[str, Any] = {}

# Config file path
_config_path: Path | None = None


def setup_config(config_path: Path | None = None) -> Path:
    """Setup config.

    Args:
        config_path: Path to config file (default: ~/.config/photon/config.json)

    Returns:
        Path to config file
    """
    global _config_path, _config

    if config_path is None:
        config_dir = Path.home() / ".config" / "photon"
        config_dir.mkdir(mode=0o700, exist_ok=True)
        config_path = config_dir / "config.json"

    _config_path = config_path

    # Load config
    _config = _defaults.copy()
    if config_path.exists():
        try:
            with open(config_path, encoding="utf-8") as f:
                user_config = json.load(f)
                _config.update(user_config)
        except (OSError, json.JSONDecodeError):
            # Use defaults if can't load
            pass

    return config_path


def get(key: str, default: Any = None) -> Any:
    """Get config value by dot-separated key.

    Args:
        key: Dot-separated key (e.g., "window.width")
        default: Default value if key not found

    Returns:
        Config value
    """
    parts = key.split(".")
    value = _config

    for part in parts:
        if isinstance(value, dict) and part in value:
            value = value[part]
        else:
            return default

    return value


def set(key: str, value: Any) -> None:
    """Set config value by dot-separated key.

    Args:
        key: Dot-separated key (e.g., "window.width")
        value: Value to set
    """
    global _config

    parts = key.split(".")
    config = _config

    for part in parts[:-1]:
        if part not in config or not isinstance(config[part], dict):
            config[part] = {}
        config = config[part]

    config[parts[-1]] = value

    # Save to file
    _save()


def _save() -> None:
    """Save config to file."""
    if _config_path is None:
        return

    try:
        with open(_config_path, "w", encoding="utf-8") as f:
            json.dump(_config, f, indent=2, ensure_ascii=False)
    except OSError:
        # Silently fail if can't write
        pass
