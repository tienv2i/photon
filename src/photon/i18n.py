"""i18n: translation system with JSON locale files."""

from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any

# Current language
_lang = "en"

# Loaded translations: {lang: {key: text}}
_translations: dict[str, dict[str, str]] = {}

# Locale directory
_locale_dir = Path(__file__).parent / "locales"


def setup_i18n(lang: str | None = None) -> str:
    """Setup i18n.

    Args:
        lang: Language code (e.g., "en", "vi"). If None, use PHOTON_LANG env or "en".

    Returns:
        Active language code
    """
    global _lang

    if lang is None:
        lang = os.environ.get("PHOTON_LANG", "en")

    _lang = lang
    _load_locale(lang)

    return _lang


def t(key: str, **kwargs: Any) -> str:
    """Translate key to current language.

    Args:
        key: Translation key (e.g., "toolbar.open")
        **kwargs: Format arguments

    Returns:
        Translated string, or key if not found
    """
    # Try current language
    if _lang in _translations and key in _translations[_lang]:
        text = _translations[_lang][key]
    # Fallback to English
    elif "en" in _translations and key in _translations["en"]:
        text = _translations["en"][key]
    else:
        # Return key itself (helps identify missing translations)
        return key

    # Format with kwargs
    if kwargs:
        try:
            return text.format(**kwargs)
        except (KeyError, IndexError):
            return text

    return text


def get_lang() -> str:
    """Get current language code."""
    return _lang


def set_lang(lang: str) -> None:
    """Set current language and reload.

    Args:
        lang: Language code
    """
    global _lang
    _lang = lang
    _load_locale(lang)


def available_langs() -> list[str]:
    """List available language codes."""
    if not _locale_dir.exists():
        return ["en"]

    return sorted(f.stem for f in _locale_dir.glob("*.json"))


def _load_locale(lang: str) -> None:
    """Load locale file.

    Args:
        lang: Language code
    """
    if lang in _translations:
        return  # Already loaded

    locale_file = _locale_dir / f"{lang}.json"

    if locale_file.exists():
        try:
            with open(locale_file, encoding="utf-8") as f:
                _translations[lang] = json.load(f)
        except (OSError, json.JSONDecodeError):
            _translations[lang] = {}
    else:
        _translations[lang] = {}

    # Always load English as fallback
    if lang != "en" and "en" not in _translations:
        _load_locale("en")
