"""UI tests: clamped panning and the side-by-side compare pane."""

from __future__ import annotations

from pathlib import Path

import pytest
from PIL import Image

from photon.ui.canvas import Canvas


@pytest.fixture(autouse=True)
def _enable_compare_feature(monkeypatch: pytest.MonkeyPatch) -> None:
    """These tests exercise the compare/frames feature, hidden by default."""
    monkeypatch.setattr("photon.core.features.COMPARE_ENABLED", True)


@pytest.fixture
def gradient_image(tmp_path: Path) -> Path:
    """Small 100x50 image, red on the left half and blue on the right."""
    path = tmp_path / "gradient.png"
    img = Image.new("RGB", (100, 50), "red")
    for x in range(50, 100):
        for y in range(50):
            img.putpixel((x, y), (0, 0, 255))
    img.save(path)
    return path


def _canvas_with_image(qtbot, path: Path, size: tuple[int, int] = (200, 200)) -> Canvas:
    canvas = Canvas()
    qtbot.addWidget(canvas)
    canvas.resize(*size)
    canvas.show()
    qtbot.waitExposed(canvas)
    canvas.set_image(path)
    return canvas


# --- Clamped panning -------------------------------------------------------


def test_fitted_image_cannot_be_panned(qtbot, gradient_image: Path) -> None:
    """An image that fits the viewport stays centred: no pan at all."""
    canvas = _canvas_with_image(qtbot, gradient_image)
    canvas.fit_to_window()

    assert canvas.offset_bounds() == (0, 0)

    canvas.pan(120, 90)
    assert (canvas.offset_x, canvas.offset_y) == (0, 0)


def test_pan_is_clamped_to_image_bounds(qtbot, gradient_image: Path) -> None:
    """Panning further than the image edge must stop at the edge."""
    canvas = _canvas_with_image(qtbot, gradient_image)
    canvas.set_zoom(8.0)  # 800x400 in a 200x200 viewport

    max_x, max_y = canvas.offset_bounds()
    assert max_x == (800 - 200) // 2
    assert max_y == (400 - 200) // 2

    canvas.pan(10_000, 10_000)
    assert (canvas.offset_x, canvas.offset_y) == (max_x, max_y)

    canvas.pan(-10_000, -10_000)
    assert (canvas.offset_x, canvas.offset_y) == (-max_x, -max_y)


def test_zoom_out_pulls_offset_back_in_bounds(qtbot, gradient_image: Path) -> None:
    """Zooming out must clamp an offset that is now outside the image."""
    canvas = _canvas_with_image(qtbot, gradient_image)
    canvas.set_zoom(4.0)
    canvas.pan(10_000, 10_000)
    assert canvas.offset_x > 0

    canvas.set_zoom(0.5)

    assert canvas.offset_bounds() == (0, 0)
    assert (canvas.offset_x, canvas.offset_y) == (0, 0)


def test_drag_pan_stops_at_bounds(qtbot, gradient_image: Path) -> None:
    """A real mouse drag cannot throw the image off screen."""
    from PySide6.QtCore import QPoint, Qt
    from PySide6.QtTest import QTest

    canvas = _canvas_with_image(qtbot, gradient_image)
    canvas.set_zoom(8.0)
    max_x, _ = canvas.offset_bounds()

    start = QPoint(100, 100)
    QTest.mousePress(canvas, Qt.MouseButton.LeftButton, pos=start)
    QTest.mouseMove(canvas, QPoint(2_000, 2_000))
    QTest.mouseRelease(canvas, Qt.MouseButton.LeftButton, pos=QPoint(2_000, 2_000))

    assert canvas.offset_x == max_x


def test_rotation_recomputes_bounds(qtbot, gradient_image: Path) -> None:
    """A 90° rotation swaps the pannable axis, not just the painting."""
    canvas = _canvas_with_image(qtbot, gradient_image)
    canvas.set_zoom(8.0)  # 800x400

    assert canvas.offset_bounds() == (300, 100)

    canvas.set_rotation(90)  # 400x800 on screen
    assert canvas.offset_bounds() == (100, 300)


# --- Compare pane ----------------------------------------------------------


def test_compare_toggle_shows_and_hides_pane(qtbot, gradient_image: Path) -> None:
    """The Compare button/shortcut toggles a real pane next to the canvas."""
    from photon.ui.window import MainWindow

    window = MainWindow()
    qtbot.addWidget(window)
    window.resize(900, 600)
    window.show()
    qtbot.waitExposed(window)

    assert window.compare_pane.isVisible() is False

    window._on_toggle_compare()
    assert window.compare_pane.isVisible() is True
    # The pane was seeded with the main canvas' image (identical pixels).
    assert window.compare_pane.has_image is False  # no image had been opened

    window.open_path(gradient_image)
    assert window.wait_for_image_load(5000) is True

    window._on_toggle_compare()  # hide
    assert window.compare_pane.isVisible() is False

    window._on_toggle_compare()  # show again
    assert window.compare_pane.isVisible() is True
    assert window.compare_pane.has_image is True


def test_compare_pane_seeds_reference_image(qtbot, gradient_image: Path) -> None:
    """Opening compare with a loaded image fills the pane with that image."""
    from photon.ui.window import MainWindow

    window = MainWindow()
    qtbot.addWidget(window)
    window.resize(900, 600)
    window.show()
    qtbot.waitExposed(window)
    window.open_path(gradient_image)
    assert window.wait_for_image_load(5000) is True

    window._on_compare(True)

    pane = window.compare_pane
    assert pane.has_image is True
    assert pane.path == str(gradient_image)
    assert (pane.state()["width"], pane.state()["height"]) == (100, 50)


def test_compare_pane_keeps_image_when_hidden(qtbot, tmp_path: Path, gradient_image: Path) -> None:
    """Hiding the pane must not throw the reference image away."""
    from photon.ui.window import MainWindow

    other = tmp_path / "reference.png"
    Image.new("RGB", (60, 30), "green").save(other)

    window = MainWindow()
    qtbot.addWidget(window)
    window.resize(900, 600)
    window.show()
    qtbot.waitExposed(window)
    window.open_path(gradient_image)
    window.wait_for_image_load(5000)

    window._on_compare(True)
    window.load_compare_file(other)
    assert window.wait_for_compare_load(5000) is True
    assert window.compare_pane.path == str(other)

    window._on_compare(False)
    window._on_compare(True)

    assert window.compare_pane.path == str(other)
    assert window.compare_pane.has_image is True


def test_compare_pane_mirrors_zoom_and_pan(qtbot, gradient_image: Path) -> None:
    """Frames are independent: zooming the main canvas must not move the pane."""
    from photon.ui.window import MainWindow

    window = MainWindow()
    qtbot.addWidget(window)
    window.resize(900, 600)
    window.show()
    qtbot.waitExposed(window)
    window.open_path(gradient_image)
    window.wait_for_image_load(5000)
    window._on_compare(True)

    window.canvas.set_zoom(3.0)
    window.canvas.pan(40, -20)
    qtbot.wait(20)

    pane_canvas = window.compare_pane.canvas
    assert pane_canvas.zoom == pytest.approx(1.0)
    assert pane_canvas.offset_x == 0
    assert pane_canvas.offset_y == 0


def test_click_activates_frame(qtbot, gradient_image: Path) -> None:
    """Clicking a frame makes it the target of the toolbar."""
    from PySide6.QtCore import QPoint, Qt
    from PySide6.QtTest import QTest

    from photon.ui.window import MainWindow

    window = MainWindow()
    qtbot.addWidget(window)
    window.resize(900, 600)
    window.show()
    qtbot.waitExposed(window)
    window.open_path(gradient_image)
    window.wait_for_image_load(5000)
    window._on_compare(True)

    assert window.active_frame == "main"
    assert window.canvas.active is True
    assert window.compare_pane.canvas.active is False

    QTest.mousePress(
        window.compare_pane.canvas,
        Qt.MouseButton.LeftButton,
        pos=QPoint(20, 20),
    )
    assert window.active_frame == "compare"
    assert window.compare_pane.canvas.active is True
    assert window.canvas.active is False

    QTest.mousePress(window.canvas, Qt.MouseButton.LeftButton, pos=QPoint(20, 20))
    assert window.active_frame == "main"
    assert window.canvas.active is True
    assert window.compare_pane.canvas.active is False


def test_toolbar_commands_only_touch_active_frame(qtbot, gradient_image: Path) -> None:
    """Zoom/rotate/fit/reset go to the active frame, never to both."""
    from photon.ui.window import MainWindow

    window = MainWindow()
    qtbot.addWidget(window)
    window.resize(900, 600)
    window.show()
    qtbot.waitExposed(window)
    window.open_path(gradient_image)
    window.wait_for_image_load(5000)
    window._on_compare(True)

    main = window.canvas
    pane = window.compare_pane.canvas
    main_zoom = main.zoom
    main_rotation = main.rotation

    # --- Compare frame active: only the pane changes ---
    window.set_active_frame("compare")
    window._on_zoom_in()
    assert pane.zoom > 1.0
    assert main.zoom == pytest.approx(main_zoom)

    window._on_rotate(90)
    assert pane.rotation == 90
    assert main.rotation == main_rotation
    assert window.doc.rotation == main_rotation  # no document edit

    window._on_flip("h")
    assert pane.flip_h is True
    assert main.flip_h is False

    window._on_reset()
    assert (pane.zoom, pane.rotation, pane.flip_h) == (1.0, 0, False)
    assert main.zoom == pytest.approx(main_zoom)

    # --- Main frame active: the pane must not move ---
    window.set_active_frame("main")
    pane_zoom = pane.zoom
    window._on_zoom_in()
    assert main.zoom > main_zoom
    assert pane.zoom == pytest.approx(pane_zoom)

    window._on_rotate(90)
    assert main.rotation == (main_rotation + 90) % 360
    assert pane.rotation == 0
    assert window.doc.rotation == main.rotation  # main rotation is a doc edit


def test_fit_only_touches_active_frame(qtbot, gradient_image: Path) -> None:
    """Fit applies to the active frame and leaves the other zoom untouched."""
    from photon.ui.window import MainWindow

    window = MainWindow()
    qtbot.addWidget(window)
    window.resize(900, 600)
    window.show()
    qtbot.waitExposed(window)
    window.open_path(gradient_image)
    window.wait_for_image_load(5000)
    window._on_compare(True)

    pane = window.compare_pane.canvas
    pane.set_zoom(4.0)
    window.set_active_frame("main")

    window._on_fit()

    assert pane.zoom == pytest.approx(4.0)
    assert window.canvas.fit_mode is True


def test_hiding_compare_focuses_main(qtbot, gradient_image: Path) -> None:
    """Hiding the compare frame must not leave commands pointed at it."""
    from photon.ui.window import MainWindow

    window = MainWindow()
    qtbot.addWidget(window)
    window.resize(900, 600)
    window.show()
    qtbot.waitExposed(window)
    window.open_path(gradient_image)
    window.wait_for_image_load(5000)
    window._on_compare(True)
    window.set_active_frame("compare")

    window._on_compare(False)

    assert window.active_frame == "main"
    assert window.canvas.active is True


def test_compare_pane_ignores_edits(qtbot, gradient_image: Path) -> None:
    """Rotation/flip belong to the document; the reference stays unrotated."""
    from photon.ui.window import MainWindow

    window = MainWindow()
    qtbot.addWidget(window)
    window.resize(900, 600)
    window.show()
    qtbot.waitExposed(window)
    window.open_path(gradient_image)
    window.wait_for_image_load(5000)
    window._on_compare(True)

    window.canvas.set_rotation(90)
    window.canvas.set_flip(True, False)

    pane_canvas = window.compare_pane.canvas
    assert pane_canvas.rotation == 0
    assert pane_canvas.flip_h is False


def test_compare_pane_renders_different_images(qtbot, tmp_path: Path, gradient_image: Path) -> None:
    """Both halves of the window must actually paint different pixels."""
    from photon.ui.window import MainWindow

    reference = tmp_path / "reference.png"
    Image.new("RGB", (100, 50), "green").save(reference)

    window = MainWindow()
    qtbot.addWidget(window)
    window.resize(900, 600)
    window.show()
    qtbot.waitExposed(window)
    window.open_path(gradient_image)
    window.wait_for_image_load(5000)

    window._on_compare(True)
    window.load_compare_file(reference)
    assert window.wait_for_compare_load(5000) is True
    qtbot.wait(50)

    main_canvas = window.canvas
    # Sample a quarter of the way into the image (the centre column sits on
    # the red/blue boundary, which is ambiguous).
    main_x = main_canvas.width() // 2 - int(25 * main_canvas.zoom)
    main_colour = main_canvas.grab().toImage().pixelColor(main_x, main_canvas.height() // 2)

    pane_centre = window.compare_pane.canvas
    pane_colour = (
        pane_centre.grab().toImage().pixelColor(pane_centre.width() // 2, pane_centre.height() // 2)
    )

    assert main_colour.red() > 150  # left half of the red/blue image
    assert main_colour.blue() < 100
    # PIL's "green" is (0, 128, 0), so check the green channel dominates.
    assert pane_colour.green() > 100
    assert pane_colour.red() < 100
    assert pane_colour.blue() < 100


def test_compare_pane_accepts_file_drop(qtbot, tmp_path: Path, gradient_image: Path) -> None:
    """Dropping a file on the pane loads it as the reference image."""
    from PySide6.QtCore import QMimeData, QPointF, Qt, QUrl
    from PySide6.QtGui import QDragEnterEvent, QDropEvent

    from photon.ui.window import MainWindow

    dropped = tmp_path / "dropped.png"
    Image.new("RGB", (80, 40), "yellow").save(dropped)

    window = MainWindow()
    qtbot.addWidget(window)
    window.resize(900, 600)
    window.show()
    qtbot.waitExposed(window)
    window.open_path(gradient_image)
    window.wait_for_image_load(5000)
    window._on_compare(True)

    mime = QMimeData()
    mime.setUrls([QUrl.fromLocalFile(str(dropped))])

    pane_canvas = window.compare_pane.canvas
    enter = QDragEnterEvent(
        QPointF(10, 10),
        Qt.DropAction.CopyAction,
        mime,
        Qt.MouseButton.LeftButton,
        Qt.KeyboardModifier.NoModifier,
    )
    pane_canvas.dragEnterEvent(enter)
    assert enter.isAccepted() is True

    drop = QDropEvent(
        QPointF(10, 10),
        Qt.DropAction.CopyAction,
        mime,
        Qt.MouseButton.LeftButton,
        Qt.KeyboardModifier.NoModifier,
    )
    pane_canvas.dropEvent(drop)
    assert window.wait_for_compare_load(5000) is True
    assert window.compare_pane.path == str(dropped)


def test_compare_pane_reports_decode_failure(qtbot, tmp_path: Path, gradient_image: Path) -> None:
    """A corrupt reference file must not wipe the image already in the pane."""
    from photon.ui.window import MainWindow

    bad = tmp_path / "broken.png"
    bad.write_bytes(b"this is not an image")

    window = MainWindow()
    qtbot.addWidget(window)
    window.resize(900, 600)
    window.show()
    qtbot.waitExposed(window)
    window.open_path(gradient_image)
    window.wait_for_image_load(5000)
    window._on_compare(True)

    window.load_compare_file(bad)
    assert window.wait_for_compare_load(5000) is True

    assert window._compare_error is not None
    # The pane keeps the previous (valid) reference image.
    assert window.compare_pane.path == str(gradient_image)


def test_compare_pane_does_not_steal_main_drag_drop(qtbot, gradient_image: Path) -> None:
    """The main canvas still ignores drops: only the pane is a drop target."""
    canvas = _canvas_with_image(qtbot, gradient_image)

    assert canvas.acceptDrops() is False


# --- Automation: frame targeting -------------------------------------------


def _api_with_window(qtbot, image: Path):
    from photon.automation.api import AutomationAPI
    from photon.ui.window import MainWindow

    window = MainWindow()
    qtbot.addWidget(window)
    window.resize(900, 600)
    window.show()
    qtbot.waitExposed(window)
    window.open_path(image)
    window.wait_for_image_load(5000)
    window._on_compare(True)

    api = AutomationAPI()
    api.set_window(window)
    return api, window


def test_api_view_commands_default_to_active_frame(qtbot, gradient_image: Path) -> None:
    """view.* without ``frame`` follows the active frame (toolbar behaviour)."""
    api, window = _api_with_window(qtbot, gradient_image)
    pane = window.compare_pane.canvas
    main_zoom = window.canvas.zoom

    window.set_active_frame("compare")
    assert api.view_zoom(factor=2.0) == {"zoom": pytest.approx(2.0), "frame": "compare"}
    assert pane.zoom == pytest.approx(2.0)
    assert window.canvas.zoom == pytest.approx(main_zoom)

    window.set_active_frame("main")
    result = api.view_rotate(degrees=90)
    assert result["frame"] == "main"
    assert window.canvas.rotation == 90
    assert pane.rotation == 0
    assert window.doc.rotation == 90


def test_api_view_commands_accept_explicit_frame(qtbot, gradient_image: Path) -> None:
    """``frame`` overrides the active frame without switching focus."""
    api, window = _api_with_window(qtbot, gradient_image)
    pane = window.compare_pane.canvas
    main = window.canvas

    assert window.active_frame == "main"

    result = api.view_rotate(degrees=90, frame="compare")
    assert result == {"rotation": 90, "frame": "compare"}
    assert pane.rotation == 90
    assert main.rotation == 0
    assert window.doc.rotation == 0
    assert window.active_frame == "main"  # focus unchanged

    result = api.view_flip(axis="h", frame="compare")
    assert result["frame"] == "compare"
    assert result["flip_h"] is True
    assert pane.flip_h is True
    assert main.flip_h is False

    api.view_reset(frame="compare")
    assert (pane.zoom, pane.rotation, pane.flip_h) == (1.0, 0, False)
    assert main.rotation == 0


def test_api_frame_state_reports_both_frames(qtbot, gradient_image: Path) -> None:
    """frame.state exposes the active frame and both view states."""
    api, window = _api_with_window(qtbot, gradient_image)

    state = api.frame_state()
    assert state["active"] == "main"
    assert state["frames"]["main"]["active"] is True
    assert state["frames"]["main"]["width"] == 100
    assert state["frames"]["compare"]["active"] is False

    assert api.frame_activate(frame="compare") == {"active": "compare"}
    state = api.frame_state()
    assert state["active"] == "compare"
    assert state["frames"]["compare"]["active"] is True
    assert state["frames"]["main"]["active"] is False


def test_api_frame_activate_rejects_unknown_frame(qtbot, gradient_image: Path) -> None:
    """A typo must fail loudly instead of silently targeting the main frame."""
    from photon.core.errors import InvalidArgumentsError

    api, _window = _api_with_window(qtbot, gradient_image)

    with pytest.raises(InvalidArgumentsError):
        api.frame_activate(frame="nope")
    with pytest.raises(InvalidArgumentsError):
        api.view_zoom(factor=2.0, frame="nope")
