"""Show Edit Tools reveals both side panels (left + right) as one unit."""

from __future__ import annotations

import copy

import pytest

from photon.automation.api import AutomationAPI
from photon.core import settings
from photon.core.errors import InvalidArgumentsError
from photon.ui.window import MainWindow


@pytest.fixture(autouse=True)
def _isolated_config(monkeypatch: pytest.MonkeyPatch) -> None:
    """Keep sidebar visibility from leaking into other tests / the user's config."""
    monkeypatch.setattr(settings, "_config", copy.deepcopy(settings._defaults), raising=False)
    monkeypatch.setattr(settings, "_config_path", None, raising=False)


@pytest.fixture
def window(qtbot) -> MainWindow:
    win = MainWindow()
    qtbot.addWidget(win)
    win.resize(900, 600)
    win.show()
    qtbot.waitExposed(win)
    return win


def test_both_sidebars_hidden_by_default(window: MainWindow) -> None:
    assert window.sidebar_left.isHidden() is True
    assert window.sidebar_right.isHidden() is True
    assert window.edit_tools_visible() is False


def test_show_edit_tools_toggles_both_sidebars(window: MainWindow) -> None:
    assert window.toggle_edit_tools() is True
    assert window.sidebar_left.isHidden() is False
    assert window.sidebar_right.isHidden() is False
    assert window.edit_tools_visible() is True
    assert settings.get("sidebar.left.visible") is True
    assert settings.get("sidebar.right.visible") is True

    assert window.toggle_edit_tools() is False
    assert window.sidebar_left.isHidden() is True
    assert window.sidebar_right.isHidden() is True
    assert settings.get("sidebar.left.visible") is False
    assert settings.get("sidebar.right.visible") is False


def test_one_visible_panel_means_the_next_click_shows_both(window: MainWindow) -> None:
    """The toggle is "all or nothing": a half-open layout opens fully first."""
    window.sidebar_right.setVisible(True)

    assert window.toggle_edit_tools() is True
    assert window.sidebar_left.isHidden() is False
    assert window.sidebar_right.isHidden() is False


def test_toolbar_action_shows_and_hides_both(window: MainWindow) -> None:
    """Real UI path: the top toolbar action, not a private method."""
    action = next(a for a in window.toolbar.actions() if a.objectName() == "toolbar_edit_tools")

    action.trigger()
    assert window.edit_tools_visible() is True

    action.trigger()
    assert window.edit_tools_visible() is False


def test_automation_toolbar_click_shows_both(window: MainWindow) -> None:
    api = AutomationAPI()
    api.set_window(window)

    result = api.toolbar_click(button="edit_tools")
    assert result["clicked"] == "edit_tools"

    state = api.sidebar_state(side="both")
    assert state["side"] == "both"
    assert state["left"] is True
    assert state["right"] is True
    assert state["visible"] is True

    api.toolbar_click(button="edit_tools")
    state = api.sidebar_state(side="both")
    assert state["visible"] is False
    assert state["left"] is False
    assert state["right"] is False


def test_sidebar_show_both(window: MainWindow) -> None:
    api = AutomationAPI()
    api.set_window(window)

    result = api.sidebar_show(side="both", visible=True)
    assert result == {"side": "both", "visible": True, "left": True, "right": True}
    assert window.sidebar_left.isHidden() is False
    assert window.sidebar_right.isHidden() is False

    result = api.sidebar_show(side="edit_tools", visible=False)
    assert result["visible"] is False
    assert window.sidebar_left.isHidden() is True
    assert window.sidebar_right.isHidden() is True


def test_sidebar_show_still_handles_one_side(window: MainWindow) -> None:
    api = AutomationAPI()
    api.set_window(window)

    assert api.sidebar_show(side="left", visible=True) == {"side": "left", "visible": True}
    assert window.sidebar_left.isHidden() is False
    assert window.sidebar_right.isHidden() is True
    assert api.sidebar_state(side="left")["visible"] is True
    assert api.sidebar_state(side="right")["visible"] is False


def test_f9_shortcut_toggles_both(window: MainWindow, qtbot) -> None:
    """F9 is the toolbar action's shortcut; it must not be ambiguous."""
    api = AutomationAPI()
    api.set_window(window)

    assert api.ui_key(key="F9") == {"sent": "F9"}
    qtbot.waitUntil(window.edit_tools_visible)

    assert api.ui_key(key="F9") == {"sent": "F9"}
    qtbot.waitUntil(lambda: not window.edit_tools_visible())


def test_empty_ui_key_does_not_trigger_an_action(window: MainWindow) -> None:
    api = AutomationAPI()
    api.set_window(window)

    result = api.ui_key(key="")
    assert result["sent"] is None
    assert result["error"] == "empty key"
    assert window.edit_tools_visible() is False


def test_unknown_sidebar_side_is_rejected(window: MainWindow) -> None:
    api = AutomationAPI()
    api.set_window(window)

    with pytest.raises(InvalidArgumentsError):
        api.sidebar_show(side="middle", visible=True)
    with pytest.raises(InvalidArgumentsError):
        api.sidebar_state(side="middle")
