"""Tests for i18n."""

from __future__ import annotations

from photon.i18n import available_langs, get_lang, set_lang, setup_i18n, t


def test_setup_i18n_default() -> None:
    """Test i18n setup with default language."""
    lang = setup_i18n("en")
    assert lang == "en"
    assert get_lang() == "en"


def test_translate_english() -> None:
    """Test translation in English."""
    setup_i18n("en")

    assert t("toolbar.open") == "Open"
    assert t("app.title") == "Photon"
    assert t("sidebar.left") == "Edit tools"
    assert t("sidebar.right") == "Tool options"


def test_translate_vietnamese() -> None:
    """Test translation in Vietnamese."""
    setup_i18n("vi")

    assert t("toolbar.open") == "Mở"
    assert t("app.title") == "Photon"
    assert t("sidebar.left") == "Công cụ sửa"
    assert t("sidebar.right") == "Tùy chọn công cụ"

    # Reset to English
    setup_i18n("en")


def test_translate_with_format() -> None:
    """Test translation with format arguments."""
    setup_i18n("en")

    result = t("status.zoom", zoom=150)
    assert result == "150%"

    result = t("status.dimensions", width=1920, height=1080)
    assert result == "1920 × 1080"


def test_translate_missing_key() -> None:
    """Test translation with missing key returns key itself."""
    setup_i18n("en")

    assert t("nonexistent.key") == "nonexistent.key"


def test_available_langs() -> None:
    """Test listing available languages."""
    langs = available_langs()
    assert "en" in langs
    assert "vi" in langs


def test_set_lang() -> None:
    """Test changing language."""
    setup_i18n("en")
    assert t("toolbar.open") == "Open"

    set_lang("vi")
    assert get_lang() == "vi"
    assert t("toolbar.open") == "Mở"

    # Reset
    set_lang("en")
