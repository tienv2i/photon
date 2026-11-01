"""Left/right dockable panels (empty, ready for edit tools)."""

from __future__ import annotations

from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QHideEvent, QShowEvent
from PySide6.QtWidgets import QLabel, QSizePolicy, QVBoxLayout, QWidget

from photon.i18n import t


class Sidebar(QWidget):
    """Dockable sidebar panel.

    Left sidebar: "Edit tools" — list of editing tools.
    Right sidebar: "Tool options" — options for the active edit tool.

    Both can be toggled visible/hidden at runtime.
    """

    # Signals
    visibilityChanged = Signal(bool)  # visible

    def __init__(self, side: str = "left", parent: QWidget | None = None) -> None:
        super().__init__(parent)

        self.side = side  # "left" or "right"
        self.setObjectName(f"sidebar_{side}")

        # Fixed width
        self.setFixedWidth(240 if side == "left" else 280)
        self.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Expanding)

        # Background color
        self.setAutoFillBackground(True)

        self._setup_ui()

    def _setup_ui(self) -> None:
        """Setup sidebar UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(8)

        # Title label
        title_key = "sidebar.right" if self.side == "right" else "sidebar.left"
        title_label = QLabel(t(title_key))
        title_label.setObjectName(f"sidebar_{self.side}_title")
        title_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        title_label.setStyleSheet("font-weight: bold; font-size: 12pt;")
        layout.addWidget(title_label)

        # Placeholder label
        placeholder = QLabel(t("sidebar.placeholder"))
        placeholder.setObjectName(f"sidebar_{self.side}_placeholder")
        placeholder.setAlignment(Qt.AlignmentFlag.AlignCenter)
        placeholder.setWordWrap(True)
        placeholder.setStyleSheet("color: gray;")
        layout.addWidget(placeholder)

        # Stretch to push content to top
        layout.addStretch()

    def set_content(self, widget: QWidget) -> None:
        """Replace sidebar content with a widget.

        Args:
            widget: Widget to embed
        """
        # Clear existing
        layout = self.layout()
        if layout is not None:
            while layout.count():
                item = layout.takeAt(0)
                w = item.widget() if item else None
                if w is not None:
                    w.deleteLater()

        # Add new widget
        if layout is not None:
            layout.addWidget(widget)

    def retranslate(self) -> None:
        """Update translations (call after language change)."""
        title_key = "sidebar.right" if self.side == "right" else "sidebar.left"

        layout = self.layout()
        if layout is None:
            return

        for i in range(layout.count()):
            item = layout.itemAt(i)
            widget = item.widget() if item else None
            if widget is None:
                continue

            if widget.objectName() == f"sidebar_{self.side}_title":
                label = widget
                if isinstance(label, QLabel):
                    label.setText(t(title_key))
            elif widget.objectName() == f"sidebar_{self.side}_placeholder":
                label = widget
                if isinstance(label, QLabel):
                    label.setText(t("sidebar.placeholder"))

    def showEvent(self, event: QShowEvent) -> None:
        """Handle show."""
        super().showEvent(event)
        self.visibilityChanged.emit(True)

    def hideEvent(self, event: QHideEvent) -> None:
        """Handle hide."""
        super().hideEvent(event)
        self.visibilityChanged.emit(False)
