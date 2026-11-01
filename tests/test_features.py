"""Feature switches: compare + active frame are hidden by default.

The code stays in place (so the feature can be turned back on), but the UI must
not expose it and automation must fail with a clear error instead of silently
doing something surprising.
"""

from __future__ import annotations

from pathlib import Path

import pytest
from PIL import Image

from photon.automation.api import AutomationAPI
from photon.core.errors import FeatureDisabledError
from photon.core.features import compare_enabled
from photon.ui.window import MainWindow


@pytest.fixture
def sample_image(tmp_path: Path) -> Path:
    path = tmp_path / "sample.png"
    Image.new("RGB", (120, 60), "red").save(path)
    return path


@pytest.fixture
def window(qtbot, sample_image: Path) -> MainWindow:
    win = MainWindow()
    qtbot.addWidget(win)
    win.resize(900, 600)
    win.show()
    qtbot.waitExposed(win)
    win.open_path(sample_image)
    win.wait_for_image_load(5000)
    return win


def test_compare_is_disabled_by_default() -> None:
    """The switch is off in the shipped configuration."""
    assert compare_enabled() is False


def test_compare_button_is_hidden(window: MainWindow) -> None:
    """No Compare button in the viewer toolbar while the feature is hidden."""
    button = window.bottom_toolbar._buttons["bottom_compare"]
    assert button.isVisible() is False
    # It still exists so the state structure stays stable for automation.
    assert window.bottom_toolbar._buttons["bottom_zoom_in"].isVisible() is True


def test_pane_cannot_be_opened(window: MainWindow) -> None:
    """Shortcut/button/toggle all keep the pane closed."""
    window._on_toggle_compare()
    window._on_compare(True)
    window.bottom_toolbar.compareRequested.emit(True)

    assert window.compare_pane.isVisible() is False
    assert window._compare_active is False


def test_no_focus_border_without_compare(window: MainWindow) -> None:
    """Single-frame mode has no active-frame highlight and no frame events."""
    received: list[dict[str, object]] = []
    from photon.core.events import EventBus

    EventBus.on("frame.activated", lambda name, meta: received.append(meta))

    assert window.canvas.active is False
    assert window.compare_pane.canvas.active is False

    # Clicking the main canvas must not turn the focus border on.
    window.canvas.activated.emit()
    assert window.canvas.active is False
    assert received == []


def test_set_active_frame_rejects_compare(window: MainWindow) -> None:
    """A caller cannot focus the hidden frame."""
    assert window.set_active_frame("main") == "main"

    with pytest.raises(ValueError):
        window.set_active_frame("compare")

    assert window.active_frame == "main"


def test_view_commands_refuse_compare_frame(window: MainWindow) -> None:
    """view.* {"frame": "compare"} must fail loudly, not touch the main frame."""
    api = AutomationAPI()
    api.set_window(window)

    with pytest.raises(FeatureDisabledError):
        api.view_zoom(factor=2.0, frame="compare")

    # The active frame path keeps working as before.
    result = api.view_zoom(factor=2.0)
    assert result == {"zoom": pytest.approx(2.0), "frame": "main"}


def test_compare_and_frame_commands_report_disabled(window: MainWindow) -> None:
    """Every hidden feature command returns PHOTON-CTL-204."""
    api = AutomationAPI()
    api.set_window(window)

    calls = [
        lambda: api.view_compare(enabled=True),
        lambda: api.compare_state(),
        lambda: api.compare_set_image(path="/tmp/does-not-matter.png"),
        lambda: api.compare_clear(),
        lambda: api.frame_activate(frame="main"),
        lambda: api.frame_state(),
    ]

    for call in calls:
        with pytest.raises(FeatureDisabledError) as excinfo:
            call()
        assert excinfo.value.code == "PHOTON-CTL-204"


def test_window_state_still_reports_a_main_only_view(window: MainWindow) -> None:
    """window.state stays usable and honest about the hidden feature."""
    api = AutomationAPI()
    api.set_window(window)

    state = api.window_state()
    assert state["compare"] is False
    assert state["active_frame"] == "main"
    assert state["canvas"]["active"] is False
    assert state["compare_pane"]["visible"] is False


def test_compare_feature_can_be_re_enabled(
    qtbot, monkeypatch: pytest.MonkeyPatch, sample_image: Path
) -> None:
    """Flipping the switch brings the button, pane and focus back."""
    monkeypatch.setattr("photon.core.features.COMPARE_ENABLED", True)

    win = MainWindow()
    qtbot.addWidget(win)
    win.resize(900, 600)
    win.show()
    qtbot.waitExposed(win)
    win.open_path(sample_image)
    win.wait_for_image_load(5000)

    assert win.bottom_toolbar._buttons["bottom_compare"].isVisible() is True

    win._on_compare(True)
    assert win.compare_pane.isVisible() is True
    assert win.canvas.active is True
