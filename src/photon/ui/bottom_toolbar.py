"""Bottom viewer toolbar: zoom, navigation, full screen, compare, rotate.

Windows Photo Viewer-inspired floating pill toolbar, centred at the bottom.

Button order (left to right):
    [Zoom in] [Zoom out] [Fit] [Back] [Full screen] [Next] [Compare]
    [Rotate left] [Rotate right]
"""

from __future__ import annotations

from PySide6.QtCore import Signal
from PySide6.QtWidgets import (
    QFrame,
    QHBoxLayout,
    QToolButton,
    QWidget,
)

from photon.core.features import compare_enabled
from photon.i18n import t


class BottomToolbar(QFrame):
    """Bottom toolbar with viewer controls.

    Layout:
        [Zoom in] [Zoom out] [Fit] [Back] [Full screen] [Next] [Compare]
        [Rotate left] [Rotate right]

    The button group is horizontally centred; there is intentionally no delete
    button. Buttons emit signals that are connected to MainWindow handlers.
    """

    # Signals
    zoomInRequested = Signal()
    zoomOutRequested = Signal()
    zoomFitRequested = Signal()
    prevRequested = Signal()
    fullscreenRequested = Signal()
    nextRequested = Signal()
    compareRequested = Signal(bool)  # True = compare (original) view
    rotateLeftRequested = Signal()
    rotateRightRequested = Signal()

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self.setObjectName("bottom_toolbar")
        self.setFrameStyle(QFrame.Shape.StyledPanel | QFrame.Shadow.Raised)
        self.setFixedHeight(40)

        self._buttons: dict[str, QToolButton] = {}

        self._setup_ui()

    def _setup_ui(self) -> None:
        """Setup toolbar UI (centred button group)."""
        layout = QHBoxLayout(self)
        layout.setContentsMargins(8, 4, 8, 4)
        layout.setSpacing(4)

        # Stretch on both sides keeps the button group centred.
        layout.addStretch(1)

        # --- Zoom in ---
        btn_zoom_in = self._create_button(
            "bottom_zoom_in",
            t("action.zoom_in"),
            t("shortcut.zoom_in"),
            "+",
        )
        btn_zoom_in.clicked.connect(self.zoomInRequested.emit)
        layout.addWidget(btn_zoom_in)

        # --- Zoom out ---
        btn_zoom_out = self._create_button(
            "bottom_zoom_out",
            t("action.zoom_out"),
            t("shortcut.zoom_out"),
            "−",
        )
        btn_zoom_out.clicked.connect(self.zoomOutRequested.emit)
        layout.addWidget(btn_zoom_out)

        # --- Fit to window (default view after opening) ---
        btn_fit = self._create_button(
            "bottom_fit",
            t("action.fit"),
            t("shortcut.fit"),
            "Fit",
            checkable=True,
        )
        btn_fit.clicked.connect(self.zoomFitRequested.emit)
        layout.addWidget(btn_fit)

        layout.addSpacing(8)

        # --- Back (previous image) ---
        btn_prev = self._create_button(
            "bottom_prev",
            t("action.prev"),
            t("shortcut.prev"),
            "◀",
        )
        btn_prev.clicked.connect(self.prevRequested.emit)
        layout.addWidget(btn_prev)

        # --- Full screen ---
        btn_fullscreen = self._create_button(
            "bottom_fullscreen",
            t("action.fullscreen"),
            t("shortcut.fullscreen"),
            "⛶",
            checkable=True,
        )
        btn_fullscreen.clicked.connect(self.fullscreenRequested.emit)
        layout.addWidget(btn_fullscreen)

        # --- Next image ---
        btn_next = self._create_button(
            "bottom_next",
            t("action.next"),
            t("shortcut.next"),
            "▶",
        )
        btn_next.clicked.connect(self.nextRequested.emit)
        layout.addWidget(btn_next)

        layout.addSpacing(8)

        # --- Compare (original vs edited) ---
        # Hidden while ``features.compare_enabled()`` is False: the button is
        # still created (so automation/state keep a stable structure) but not
        # shown, and MainWindow refuses to open the pane.
        btn_compare = self._create_button(
            "bottom_compare",
            t("action.compare"),
            t("shortcut.compare"),
            "◫",
            checkable=True,
        )
        btn_compare.toggled.connect(self.compareRequested.emit)
        layout.addWidget(btn_compare)
        btn_compare.setVisible(compare_enabled())

        layout.addSpacing(8)

        # --- Rotate left (CCW) ---
        btn_rotate_left = self._create_button(
            "bottom_rotate_left",
            t("action.rotate_ccw"),
            t("shortcut.rotate_ccw"),
            "↺",
        )
        btn_rotate_left.clicked.connect(self.rotateLeftRequested.emit)
        layout.addWidget(btn_rotate_left)

        # --- Rotate right (CW) ---
        btn_rotate_right = self._create_button(
            "bottom_rotate_right",
            t("action.rotate_cw"),
            t("shortcut.rotate_cw"),
            "↻",
        )
        btn_rotate_right.clicked.connect(self.rotateRightRequested.emit)
        layout.addWidget(btn_rotate_right)

        layout.addStretch(1)

    def _create_button(
        self,
        object_name: str,
        tooltip: str,
        shortcut_text: str,
        text: str,
        checkable: bool = False,
    ) -> QToolButton:
        """Create a toolbar button.

        Args:
            object_name: Widget objectName for automation
            tooltip: Button tooltip
            shortcut_text: Shortcut text (e.g., "Ctrl++")
            text: Button text/icon
            checkable: Whether button is checkable

        Returns:
            QToolButton instance
        """
        btn = QToolButton(self)
        btn.setObjectName(object_name)
        btn.setText(text)
        btn.setToolTip(f"{tooltip} ({shortcut_text})")
        btn.setCheckable(checkable)
        btn.setAutoRaise(True)
        self._buttons[object_name] = btn
        return btn

    def set_zoom_factor(self, zoom: float, fit_active: bool = False) -> None:
        """Update button states based on zoom factor and fit mode.

        Args:
            zoom: Current zoom factor (1.0 = 100%)
            fit_active: Whether the canvas is tracking fit-to-window
        """
        fit_btn = self._buttons.get("bottom_fit")
        if fit_btn:
            fit_btn.setChecked(fit_active)

    def set_compare_active(self, active: bool) -> None:
        """Reflect compare state without re-emitting the signal.

        Args:
            active: Whether the original (compare) view is shown
        """
        btn = self._buttons.get("bottom_compare")
        if btn is not None and btn.isChecked() != active:
            btn.blockSignals(True)
            btn.setChecked(active)
            btn.blockSignals(False)

    def set_fullscreen_active(self, active: bool) -> None:
        """Reflect full screen state without re-emitting the signal.

        Args:
            active: Whether the window is full screen
        """
        btn = self._buttons.get("bottom_fullscreen")
        if btn is not None and btn.isChecked() != active:
            btn.blockSignals(True)
            btn.setChecked(active)
            btn.blockSignals(False)

    def button_names(self) -> list[str]:
        """Return objectNames in visual order (used by automation/tests)."""
        return list(self._buttons.keys())

    def is_button_checked(self, name: str) -> bool:
        """Return whether a button is checked (False if it is not checkable)."""
        btn = self._buttons.get(name)
        return bool(btn is not None and btn.isChecked())

    def is_button_visible(self, name: str) -> bool:
        """Return whether a button is currently shown (feature switches)."""
        btn = self._buttons.get(name)
        return bool(btn is not None and not btn.isHidden())

    def centering_margins(self) -> tuple[int, int]:
        """Return (left, right) pixel gaps around the button group."""
        if not self._buttons:
            return (0, 0)

        buttons = [b for b in self._buttons.values() if not b.isHidden()]
        if not buttons:
            return (0, 0)
        buttons.sort(key=lambda b: b.x())
        left = buttons[0].x()
        right = self.width() - (buttons[-1].x() + buttons[-1].width())
        return (left, right)

    def is_centered(self, tolerance: int = 4) -> bool:
        """Return True when the button group is horizontally centred."""
        left, right = self.centering_margins()
        return abs(left - right) <= tolerance

    def retranslate(self) -> None:
        """Update translations (call after language change)."""
        for name, btn in self._buttons.items():
            if name == "bottom_zoom_in":
                btn.setText("+")
                btn.setToolTip(f"{t('action.zoom_in')} ({t('shortcut.zoom_in')})")
            elif name == "bottom_zoom_out":
                btn.setText("−")
                btn.setToolTip(f"{t('action.zoom_out')} ({t('shortcut.zoom_out')})")
            elif name == "bottom_fit":
                btn.setText("Fit")
                btn.setToolTip(f"{t('action.fit')} ({t('shortcut.fit')})")
            elif name == "bottom_prev":
                btn.setText("◀")
                btn.setToolTip(f"{t('action.prev')} ({t('shortcut.prev')})")
            elif name == "bottom_fullscreen":
                btn.setText("⛶")
                btn.setToolTip(f"{t('action.fullscreen')} ({t('shortcut.fullscreen')})")
            elif name == "bottom_next":
                btn.setText("▶")
                btn.setToolTip(f"{t('action.next')} ({t('shortcut.next')})")
            elif name == "bottom_compare":
                btn.setText("◫")
                btn.setToolTip(f"{t('action.compare')} ({t('shortcut.compare')})")
            elif name == "bottom_rotate_left":
                btn.setText("↺")
                btn.setToolTip(f"{t('action.rotate_ccw')} ({t('shortcut.rotate_ccw')})")
            elif name == "bottom_rotate_right":
                btn.setText("↻")
                btn.setToolTip(f"{t('action.rotate_cw')} ({t('shortcut.rotate_cw')})")
