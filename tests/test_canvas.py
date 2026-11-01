"""UI tests: canvas transforms must actually reach the screen."""

from __future__ import annotations

from pathlib import Path

import pytest
from PIL import Image

from photon.ui.canvas import Canvas


@pytest.fixture
def split_image(tmp_path: Path) -> Path:
    """Image with a red left half and a blue right half."""
    path = tmp_path / "split.png"
    img = Image.new("RGB", (100, 50), "red")
    for x in range(50, 100):
        for y in range(50):
            img.putpixel((x, y), (0, 0, 255))
    img.save(path)
    return path


def test_flip_h_mirrors_rendering(qtbot, split_image: Path) -> None:
    """set_flip must mirror what is painted, not only report state."""
    canvas = Canvas()
    qtbot.addWidget(canvas)
    canvas.resize(200, 200)
    canvas.set_image(split_image)
    canvas.set_zoom(1.0)
    canvas.show()
    qtbot.waitExposed(canvas)
    qtbot.wait(50)

    # 30px left of centre is inside the red half.
    before = canvas.grab().toImage().pixelColor(70, 100)
    assert before.red() > 200

    canvas.set_flip(True, False)
    canvas.repaint()

    after = canvas.grab().toImage().pixelColor(70, 100)
    assert after.blue() > 200


def test_rotate_shortcut_updates_canvas(qtbot, split_image: Path) -> None:
    """The R shortcut must go through the same path as automation."""
    from PySide6.QtGui import QKeySequence

    from photon.ui.window import MainWindow

    window = MainWindow()
    qtbot.addWidget(window)
    window.open_path(split_image)

    rotate_action = next(
        action for action in window.actions() if action.shortcut() == QKeySequence("R")
    )
    rotate_action.trigger()

    assert window.doc.rotation == 90
    assert window.canvas.rotation == 90


def test_undo_shortcut_reverts_canvas(qtbot, split_image: Path) -> None:
    """Ctrl+Z must repaint the canvas, not just pop the undo stack."""
    from PySide6.QtGui import QKeySequence

    from photon.ui.window import MainWindow

    window = MainWindow()
    qtbot.addWidget(window)
    window.open_path(split_image)

    def trigger(seq: str) -> None:
        action = next(a for a in window.actions() if a.shortcut() == QKeySequence(seq))
        action.trigger()

    trigger("R")
    trigger("Ctrl+Z")

    assert window.doc.rotation == 0
    assert window.canvas.rotation == 0


def test_image_loads_in_background(qtbot, split_image: Path) -> None:
    """file.open must paint the image without decoding on the GUI thread."""
    from photon.ui.window import MainWindow

    window = MainWindow()
    qtbot.addWidget(window)
    window.open_path(split_image)
    assert window.wait_for_image_load(5000) is True
    assert window.doc.loading is False
    assert window.canvas.fit_mode is True
    assert window.canvas.zoom <= 1.0


def test_delete_advances_to_next_image(qtbot, tmp_path: Path, monkeypatch) -> None:
    """Trashing the current file must move to a surviving neighbour."""
    from PIL import Image

    from photon.ui.window import MainWindow

    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path / "data"))

    folder = tmp_path / "imgs"
    folder.mkdir()
    for name in ("a.png", "b.png"):
        Image.new("RGB", (20, 10), "red").save(folder / name)

    window = MainWindow()
    qtbot.addWidget(window)
    window.open_path(folder)
    window.wait_for_image_load(5000)
    assert window.doc.path is not None
    assert window.doc.path.name == "a.png"

    window._on_delete_clicked()

    assert not (folder / "a.png").exists()
    assert window.doc.path is not None
    assert window.doc.path.name == "b.png"
    assert [p.name for p in window._folder_files] == ["b.png"]
