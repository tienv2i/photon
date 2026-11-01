"""Top toolbar (Open, Show Edit Tools)."""

from __future__ import annotations

from PySide6.QtCore import Signal
from PySide6.QtGui import QAction
from PySide6.QtWidgets import QToolBar, QWidget

from photon.i18n import t


class Toolbar(QToolBar):
    """Top toolbar with Open and Show Edit Tools buttons.

    Windows Photo Viewer-inspired: simple, minimal buttons.
    """

    # Signals
    openClicked = Signal()
    editToolsClicked = Signal()

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(t("app.title"), parent)
        self.setObjectName("toolbar")
        self.setMovable(False)
        self.setFloatable(False)

        self._setup_actions()

    def _setup_actions(self) -> None:
        """Setup toolbar actions."""
        # Open button
        open_action = QAction(t("toolbar.open"), self)
        open_action.setObjectName("toolbar_open")
        open_action.setToolTip(t("toolbar.open.tooltip"))
        open_action.setShortcut("Ctrl+O")
        open_action.triggered.connect(self.openClicked.emit)
        self.addAction(open_action)

        # Separator
        self.addSeparator()

        # Show Edit Tools button
        edit_action = QAction(t("toolbar.edit_tools"), self)
        edit_action.setObjectName("toolbar_edit_tools")
        edit_action.setToolTip(t("toolbar.edit_tools.tooltip"))
        edit_action.setShortcut("F9")
        edit_action.triggered.connect(self.editToolsClicked.emit)
        self.addAction(edit_action)

    def set_button_enabled(self, button: str, enabled: bool) -> None:
        """Enable/disable a toolbar button.

        Args:
            button: Button name ("open" or "edit_tools")
            enabled: True to enable, False to disable
        """
        for action in self.actions():
            if action.objectName() == f"toolbar_{button}":
                action.setEnabled(enabled)
                break

    def retranslate(self) -> None:
        """Update translations (call after language change)."""
        for action in self.actions():
            if action.objectName() == "toolbar_open":
                action.setText(t("toolbar.open"))
                action.setToolTip(t("toolbar.open.tooltip"))
            elif action.objectName() == "toolbar_edit_tools":
                action.setText(t("toolbar.edit_tools"))
                action.setToolTip(t("toolbar.edit_tools.tooltip"))
