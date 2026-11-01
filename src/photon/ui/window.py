"""Main window: toolbar, sidebars, canvas."""

from __future__ import annotations

from pathlib import Path
from typing import Any

from PySide6.QtCore import Q_ARG, QEvent, QMetaObject, Qt, QTimer, Signal, Slot
from PySide6.QtGui import QAction, QCloseEvent, QDragEnterEvent, QDropEvent, QKeySequence
from PySide6.QtWidgets import (
    QFileDialog,
    QHBoxLayout,
    QLabel,
    QMainWindow,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

from photon.core.events import EventBus
from photon.core.features import compare_enabled
from photon.core.model import PhotoDoc
from photon.core.settings import get as config_get
from photon.core.settings import set as config_set
from photon.i18n import t
from photon.imaging.decode import probe_image
from photon.portal.filedialog import portal_dialog
from photon.ui.bottom_toolbar import BottomToolbar
from photon.ui.canvas import Canvas
from photon.ui.compare_pane import ComparePane
from photon.ui.loader import ImageLoader
from photon.ui.sidebar import Sidebar
from photon.ui.toolbar import Toolbar

#: Frame names used by the active-frame logic and automation commands.
FRAME_MAIN = "main"
FRAME_COMPARE = "compare"
FRAMES: tuple[str, ...] = (FRAME_MAIN, FRAME_COMPARE)


class MainWindow(QMainWindow):
    """Main application window.

    Layout (Windows Photo Viewer-inspired):
    - Top: Toolbar (Open, Show Edit Tools)
    - Middle: Left Sidebar | Canvas | Right Sidebar
    - Bottom: BottomToolbar (zoom, navigation, full screen, compare, rotate)
    - Bottom: Status bar (zoom, dimensions, filename)
    """

    # Signals
    fileOpened = Signal(str)  # path
    sidebarToggled = Signal(str, bool)  # side, visible

    def __init__(self) -> None:
        super().__init__()

        # Window setup
        self.setWindowTitle(t("app.title"))
        self.setObjectName("main_window")
        self.resize(
            config_get("window.width", 1280),
            config_get("window.height", 800),
        )

        # Enable drag & drop
        self.setAcceptDrops(True)

        # Document model
        self.doc = PhotoDoc()

        # Folder navigation
        self._folder_files: list[Path] = []
        self._current_index: int = -1

        # Dialog polling timer
        self._dialog_timer: QTimer | None = None

        # Compare pane state (the pane itself lives in _setup_ui; it keeps its
        # reference image so hiding/showing does not lose the comparison)
        self._compare_active: bool = False

        # Active frame: toolbar/shortcuts act only on this canvas ("main" or
        # "compare"). Clicking a frame changes it (see Canvas.activated).
        self._active_frame: str = FRAME_MAIN

        # Automation bridge (set by app once the server exists)
        self._automation_runner: Any = None
        self._automation_response: Any = None
        self._automation_error: Exception | None = None

        # Background decoder: large photos must not block the GUI thread
        self._image_loader = ImageLoader(self)
        self._image_loader.loaded.connect(self._on_image_loaded)
        self._image_loader.failed.connect(self._on_image_failed)
        self._load_generation: int = 0

        # Separate loader so the compare pane can hold another image
        self._compare_loader = ImageLoader(self)
        self._compare_loader.loaded.connect(self._on_compare_loaded)
        self._compare_loader.failed.connect(self._on_compare_failed)
        self._compare_loading: bool = False
        self._compare_generation: int = 0
        self._compare_error: str | None = None

        # Setup UI
        self._setup_ui()
        self._setup_toolbar()
        self._setup_bottom_toolbar()
        self._setup_shortcuts()
        self._setup_statusbar()

        # Restore sidebar state
        self._restore_sidebar_state()

        # Highlight the main frame and sync status/toolbar with it
        self._refresh_active_frame()

        # Connect events
        EventBus.on("file.open", self._on_file_open)
        EventBus.on("dialog.result", self._on_dialog_result)
        EventBus.on("dialog.portal_failed", self._on_dialog_portal_failed)

    def _setup_ui(self) -> None:
        """Setup central widget layout."""
        central = QWidget()
        central.setObjectName("central_widget")
        self.setCentralWidget(central)

        self._main_layout = QVBoxLayout(central)
        self._main_layout.setContentsMargins(0, 0, 0, 0)
        self._main_layout.setSpacing(0)

        # Central row: Left Sidebar | Canvas | Right Sidebar
        self._central_layout = QHBoxLayout()
        self._central_layout.setContentsMargins(0, 0, 0, 0)
        self._central_layout.setSpacing(0)

        # Left sidebar
        self.sidebar_left = Sidebar(side="left")
        self.sidebar_left.setObjectName("sidebar_left")
        self._central_layout.addWidget(self.sidebar_left)

        # Canvas (center)
        self.canvas = Canvas()
        self.canvas.setObjectName("canvas")
        self._central_layout.addWidget(self.canvas, stretch=1)

        # Compare pane (right of the canvas, hidden until the compare button
        # is pressed). Holds a reference image for side-by-side comparison.
        self.compare_pane = ComparePane()
        self.compare_pane.setVisible(False)
        self.compare_pane.imageDropped.connect(self._on_compare_dropped)
        self._central_layout.addWidget(self.compare_pane, stretch=1)

        # Each frame owns its view state; the toolbar acts on the active one.
        self._frame_canvases: dict[str, Canvas] = {
            FRAME_MAIN: self.canvas,
            FRAME_COMPARE: self.compare_pane.canvas,
        }
        self.canvas.activated.connect(lambda: self._on_frame_activated(FRAME_MAIN))
        self.compare_pane.canvas.activated.connect(lambda: self._on_frame_activated(FRAME_COMPARE))
        self.compare_pane.canvas.zoomChanged.connect(self._on_compare_canvas_zoom)

        # Right sidebar
        self.sidebar_right = Sidebar(side="right")
        self.sidebar_right.setObjectName("sidebar_right")
        self._central_layout.addWidget(self.sidebar_right)

        self._main_layout.addLayout(self._central_layout, stretch=1)

    def _setup_toolbar(self) -> None:
        """Setup top toolbar."""
        self.toolbar = Toolbar()
        self.toolbar.setObjectName("toolbar")
        self.addToolBar(self.toolbar)

        # Connect toolbar signals
        self.toolbar.openClicked.connect(self._on_open_clicked)
        self.toolbar.editToolsClicked.connect(self._on_edit_tools_clicked)

    def _setup_bottom_toolbar(self) -> None:
        """Setup bottom viewer toolbar (centred, no delete button)."""
        self.bottom_toolbar = BottomToolbar()
        self.bottom_toolbar.setObjectName("bottom_toolbar")

        # Connect bottom toolbar signals
        self.bottom_toolbar.zoomInRequested.connect(self._on_zoom_in)
        self.bottom_toolbar.zoomOutRequested.connect(self._on_zoom_out)
        self.bottom_toolbar.zoomFitRequested.connect(self._on_fit)
        self.bottom_toolbar.prevRequested.connect(self._on_prev)
        self.bottom_toolbar.fullscreenRequested.connect(self._on_toggle_fullscreen)
        self.bottom_toolbar.nextRequested.connect(self._on_next)
        self.bottom_toolbar.compareRequested.connect(self._on_compare)
        self.bottom_toolbar.rotateLeftRequested.connect(lambda: self._on_rotate(-90))
        self.bottom_toolbar.rotateRightRequested.connect(lambda: self._on_rotate(90))

        # Keep status bar + toolbar in sync with the canvas
        self.canvas.zoomChanged.connect(self._on_canvas_zoom)

        # Add to main vertical layout (below canvas, above status bar)
        self.bottom_toolbar.setSizePolicy(QSizePolicy.Policy.Preferred, QSizePolicy.Policy.Fixed)
        self._main_layout.addWidget(self.bottom_toolbar)

    def _setup_shortcuts(self) -> None:
        """Setup keyboard shortcuts."""
        # Open (Ctrl+O)
        open_action = QAction(self)
        open_action.setShortcut(QKeySequence("Ctrl+O"))
        open_action.triggered.connect(self._on_open_clicked)
        self.addAction(open_action)

        # F9 belongs to the top toolbar's "Show Edit Tools" action, which
        # toggles both sidebars together. Do not register a second F9 action
        # here: two window shortcuts for the same key are ambiguous and Qt
        # then fires neither of them.

        # Toggle left sidebar (F8)
        toggle_left = QAction(self)
        toggle_left.setShortcut(QKeySequence("F8"))
        toggle_left.triggered.connect(lambda: self.toggle_sidebar("left"))
        self.addAction(toggle_left)

        # Toggle full screen (F11)
        fullscreen = QAction(self)
        fullscreen.setShortcut(QKeySequence("F11"))
        fullscreen.triggered.connect(self._on_toggle_fullscreen)
        self.addAction(fullscreen)

        # Toggle compare with original (C); hidden while the feature is off
        compare = QAction(self)
        compare.setShortcut(QKeySequence("C"))
        compare.setEnabled(compare_enabled())
        compare.setVisible(compare_enabled())
        compare.triggered.connect(self._on_toggle_compare)
        self.addAction(compare)

        # Zoom in (Ctrl++)
        zoom_in = QAction(self)
        zoom_in.setShortcut(QKeySequence("Ctrl++"))
        zoom_in.triggered.connect(self._on_zoom_in)
        self.addAction(zoom_in)

        # Zoom out (Ctrl+-)
        zoom_out = QAction(self)
        zoom_out.setShortcut(QKeySequence("Ctrl+-"))
        zoom_out.triggered.connect(self._on_zoom_out)
        self.addAction(zoom_out)

        # Fit to window (0)
        fit = QAction(self)
        fit.setShortcut(QKeySequence("0"))
        fit.triggered.connect(self._on_fit)
        self.addAction(fit)

        # Actual size (1)
        actual = QAction(self)
        actual.setShortcut(QKeySequence("1"))
        actual.triggered.connect(self._on_actual)
        self.addAction(actual)

        # Rotate CW (R)
        rotate_cw = QAction(self)
        rotate_cw.setShortcut(QKeySequence("R"))
        rotate_cw.triggered.connect(lambda: self._on_rotate(90))
        self.addAction(rotate_cw)

        # Rotate CCW (Shift+R)
        rotate_ccw = QAction(self)
        rotate_ccw.setShortcut(QKeySequence("Shift+R"))
        rotate_ccw.triggered.connect(lambda: self._on_rotate(-90))
        self.addAction(rotate_ccw)

        # Undo / Redo
        undo = QAction(self)
        undo.setShortcut(QKeySequence("Ctrl+Z"))
        undo.triggered.connect(self._on_undo)
        self.addAction(undo)

        redo = QAction(self)
        redo.setShortcut(QKeySequence("Ctrl+Y"))
        redo.triggered.connect(self._on_redo)
        self.addAction(redo)

        # Next image (Right arrow)
        next_img = QAction(self)
        next_img.setShortcut(QKeySequence("Right"))
        next_img.triggered.connect(self._on_next)
        self.addAction(next_img)

        # Prev image (Left arrow)
        prev_img = QAction(self)
        prev_img.setShortcut(QKeySequence("Left"))
        prev_img.triggered.connect(self._on_prev)
        self.addAction(prev_img)

        # Delete current image (moves it to the trash)
        delete_action = QAction(self)
        delete_action.setShortcut(QKeySequence("Delete"))
        delete_action.triggered.connect(self._on_delete_clicked)
        self.addAction(delete_action)

        # Quit (Ctrl+Q)
        quit_action = QAction(self)
        quit_action.setShortcut(QKeySequence("Ctrl+Q"))
        quit_action.triggered.connect(self.close)
        self.addAction(quit_action)

    def _setup_statusbar(self) -> None:
        """Setup status bar."""
        self.statusBar().setObjectName("statusbar")

        # Zoom label
        self.label_zoom = QLabel(t("status.zoom", zoom=100))
        self.label_zoom.setObjectName("status_zoom")
        self.statusBar().addPermanentWidget(self.label_zoom)

        # Dimensions label
        self.label_dims = QLabel("")
        self.label_dims.setObjectName("status_dims")
        self.statusBar().addPermanentWidget(self.label_dims)

        # Filename label
        self.label_filename = QLabel("")
        self.label_filename.setObjectName("status_filename")
        self.statusBar().addPermanentWidget(self.label_filename)

    def _restore_sidebar_state(self) -> None:
        """Restore sidebar visibility from config."""
        left_visible = config_get("sidebar.left.visible", False)
        right_visible = config_get("sidebar.right.visible", False)

        self.sidebar_left.setVisible(left_visible)
        self.sidebar_right.setVisible(right_visible)

    def open_path(self, path: Path) -> None:
        """Open image file or folder.

        Args:
            path: Path to file or folder
        """
        if path.is_dir():
            self._open_folder(path)
        elif path.is_file():
            self._open_file(path)

    def _open_folder(self, folder: Path) -> None:
        """Open folder and load image list.

        Args:
            folder: Path to folder
        """
        # Scan for image files
        image_extensions = {".jpg", ".jpeg", ".png", ".webp", ".gif", ".tiff", ".bmp"}
        files = sorted(
            f for f in folder.iterdir() if f.is_file() and f.suffix.lower() in image_extensions
        )

        if not files:
            EventBus.emit("folder.empty", {"path": str(folder)})
            return

        self._folder_files = files
        self._current_index = 0

        # Open first image
        self._open_file(files[0])

        EventBus.emit(
            "folder.opened",
            {"path": str(folder), "count": len(files)},
        )

    def _open_file(self, path: Path) -> None:
        """Open single image file.

        Args:
            path: Path to image file
        """
        try:
            # Probe the header only: full decoding happens on a worker thread
            # (see ImageLoader) so a large photo never freezes the window.
            meta = probe_image(path)

            # Update doc (open() also clears the previous image's edit history)
            self.doc.open(path)
            self.doc.width = meta["width"]
            self.doc.height = meta["height"]
            self.doc.format = meta["format"]
            self.doc.size_bytes = meta["size_bytes"]

            # Start every image unrotated, unflipped and fitted to the window
            self.canvas.set_flip(False, False)
            self.doc.loading = True
            self._image_loader.load(path)

            # Update folder index if in folder mode
            if self._folder_files:
                try:
                    self._current_index = self._folder_files.index(path)
                except ValueError:
                    self._current_index = -1

            # Update UI
            self.fileOpened.emit(str(path))
            if self._active_frame == FRAME_MAIN:
                self._update_status_info(self.canvas)
            self.setWindowTitle(f"{path.name} - {t('app.title')}")

            # Save to recent files
            self._add_recent_file(path)

            # Refresh view labels with the freshly decoded dimensions
            self._on_file_open("file.open", {"path": str(path)})

            EventBus.emit(
                "file.opened",
                {
                    "path": str(path),
                    "width": self.doc.width,
                    "height": self.doc.height,
                    "index": self._current_index,
                    "total": len(self._folder_files),
                },
            )

        except Exception as e:
            EventBus.emit(
                "file.open_failed",
                {"path": str(path), "error": str(e)},
            )

    def _on_image_loaded(self, path: str, image: Any) -> None:
        """Display an image decoded on a worker thread."""
        from PySide6.QtGui import QPixmap

        if self.doc.path is None or Path(path) != self.doc.path:
            return  # stale result for an image the user already left

        pixmap = QPixmap.fromImage(image)
        self.canvas.set_pixmap(pixmap, Path(path))
        self.doc.loading = False
        # A newly opened image is what the user wants to work on: focus the
        # main frame, then fit it.
        self.set_active_frame(FRAME_MAIN)
        self._apply_fit(self.canvas)
        self._load_generation += 1

    def _on_image_failed(self, path: str, error: str) -> None:
        """Report a failed background decode."""
        self.doc.loading = False
        self._load_generation += 1
        EventBus.emit("file.open_failed", {"path": path, "error": error})

    def wait_for_image_load(self, timeout_ms: int = 10_000) -> bool:
        """Block until the pending image has been painted.

        Uses a nested event loop, so the window keeps painting and shortcuts
        keep working instead of freezing. Automation commands call this so
        ``file.open`` is deterministic even though decoding is asynchronous.

        Args:
            timeout_ms: Maximum wait in milliseconds

        Returns:
            True if the image finished loading, False on timeout
        """
        if not self.doc.loading:
            return True

        from PySide6.QtCore import QEventLoop, QTimer

        loop = QEventLoop()
        deadline = QTimer()
        deadline.setSingleShot(True)
        deadline.timeout.connect(loop.quit)

        start_generation = self._load_generation
        poll = QTimer()

        def _check() -> None:
            if not self.doc.loading or self._load_generation != start_generation:
                loop.quit()

        poll.timeout.connect(_check)
        poll.start(10)
        deadline.start(timeout_ms)
        loop.exec()

        poll.stop()
        deadline.stop()
        return not self.doc.loading

    def wait_for_compare_load(self, timeout_ms: int = 10_000) -> bool:
        """Block until the pending compare-pane image has been painted.

        Mirrors :meth:`wait_for_image_load` so ``compare.set_image`` is
        deterministic for automation and scenarios.

        Args:
            timeout_ms: Maximum wait in milliseconds

        Returns:
            True if the reference image finished loading, False on timeout
        """
        if not self._compare_loading:
            return True

        from PySide6.QtCore import QEventLoop, QTimer

        loop = QEventLoop()
        deadline = QTimer()
        deadline.setSingleShot(True)
        deadline.timeout.connect(loop.quit)

        start_generation = self._compare_generation
        poll = QTimer()

        def _check() -> None:
            if not self._compare_loading or self._compare_generation != start_generation:
                loop.quit()

        poll.timeout.connect(_check)
        poll.start(10)
        deadline.start(timeout_ms)
        loop.exec()

        poll.stop()
        deadline.stop()
        return not self._compare_loading

    def _add_recent_file(self, path: Path) -> None:
        """Add file to recent files list.

        Args:
            path: Path to file
        """
        recent = config_get("recent_files", [])
        path_str = str(path)

        # Remove if already exists
        if path_str in recent:
            recent.remove(path_str)

        # Add to front
        recent.insert(0, path_str)

        # Keep only last 10
        config_set("recent_files", recent[:10])

    def toggle_sidebar(self, side: str) -> None:
        """Toggle a single sidebar's visibility.

        Args:
            side: "left" or "right"
        """
        if side == "left":
            visible = not self.sidebar_left.isVisible()
            self.sidebar_left.setVisible(visible)
            config_set("sidebar.left.visible", visible)
        elif side == "right":
            visible = not self.sidebar_right.isVisible()
            self.sidebar_right.setVisible(visible)
            config_set("sidebar.right.visible", visible)
        else:
            return

        self.sidebarToggled.emit(side, visible)
        EventBus.emit("sidebar.toggled", {"side": side, "visible": visible})

    def set_edit_tools_visible(self, visible: bool) -> None:
        """Show or hide **both** sidebars together (edit-tools layout).

        "Show Edit Tools" reveals the left and right panels as one unit: the
        left panel lists the edit tools, the right panel shows options for the
        active tool. Both sides are persisted so the layout survives a restart.

        Args:
            visible: True to show both panels, False to hide both
        """
        visible = bool(visible)
        self.sidebar_left.setVisible(visible)
        self.sidebar_right.setVisible(visible)
        config_set("sidebar.left.visible", visible)
        config_set("sidebar.right.visible", visible)

        for side in ("left", "right"):
            self.sidebarToggled.emit(side, visible)
            EventBus.emit("sidebar.toggled", {"side": side, "visible": visible})
        EventBus.emit("edit_tools.toggled", {"visible": visible})

    def edit_tools_visible(self) -> bool:
        """Whether both edit-tool sidebars are currently shown."""
        return not self.sidebar_left.isHidden() and not self.sidebar_right.isHidden()

    def toggle_edit_tools(self) -> bool:
        """Toggle both sidebars as one unit (Show Edit Tools button / F9).

        If either panel is hidden, both are shown; only when both are visible
        does the toggle hide them again.

        Returns:
            The new visibility (True = both panels shown)
        """
        visible = not self.edit_tools_visible()
        self.set_edit_tools_visible(visible)
        return visible

    # --- Event handlers ---

    @Slot()
    def _on_open_clicked(self) -> None:
        """Handle Open button click."""
        EventBus.emit("toolbar.clicked", {"button": "open"})

        # Open file dialog
        handle = portal_dialog.open_file(
            filters=["*.jpg", "*.jpeg", "*.png", "*.webp", "*.gif", "*.tiff", "*.bmp"]
        )

        # Start polling for result
        self._start_dialog_polling(handle)

    def _start_dialog_polling(self, handle: str) -> None:
        """Start polling dialog result.

        Args:
            handle: Dialog handle
        """
        if self._dialog_timer:
            self._dialog_timer.stop()

        self._dialog_timer = QTimer(self)
        self._dialog_timer.timeout.connect(lambda: self._poll_dialog(handle))
        self._dialog_timer.start(100)  # Poll every 100ms

    def _poll_dialog(self, handle: str) -> None:
        """Poll dialog result.

        Args:
            handle: Dialog handle
        """
        try:
            result = portal_dialog.get_result(handle)

            if result["status"] != "pending" and self._dialog_timer:
                # Dialog finished (done or cancelled)
                self._dialog_timer.stop()

                # Result will be handled by _on_dialog_result event
        except Exception:
            # Handle not found, stop polling
            if self._dialog_timer:
                self._dialog_timer.stop()

    @Slot(str)
    def _open_selected_path(self, path_str: str) -> None:
        """Open selected file on the GUI thread."""
        self.open_path(Path(path_str))

    @Slot(int)
    def _trigger_action(self, index: int) -> None:
        """Trigger one of the window's QActions on the GUI thread."""
        actions = self.actions()
        if 0 <= index < len(actions):
            actions[index].trigger()

    @Slot(str, "QVariantMap")
    def _execute_automation_command(self, cmd: str, args: dict[str, Any]) -> None:
        """Execute an automation command on the GUI thread.

        The automation server runs on worker threads; this slot is invoked with
        a blocking queued connection so widget mutations happen on the thread
        that owns them and the caller sees the resulting state.
        """
        runner = self._automation_runner
        if runner is None:
            self._automation_response = {}
            return

        try:
            self._automation_response = runner(cmd, dict(args))
        except Exception as e:  # noqa: BLE001 - re-raised on the caller thread
            self._automation_error = e

    def _on_dialog_result(self, event: str, meta: dict[str, Any]) -> None:
        """Handle dialog.result event.

        Args:
            event: Event name
            meta: Event metadata
        """
        handle = meta.get("handle", "")
        status = meta.get("status", "")

        if status == "done":
            result = portal_dialog.get_result(handle)
            paths = result.get("paths", [])

            if paths:
                # Open first selected file safely on GUI thread
                QMetaObject.invokeMethod(
                    self,
                    "_open_selected_path",
                    Qt.ConnectionType.QueuedConnection,
                    Q_ARG(str, paths[0]),
                )

    def _on_dialog_portal_failed(self, event: str, meta: dict[str, Any]) -> None:
        """Portal unavailable — schedule QFileDialog fallback on the GUI thread."""
        handle = meta.get("handle", "")
        dtype = meta.get("type", "open")
        title = meta.get("title", "")
        current_name = meta.get("current_name", "")

        with portal_dialog._lock:
            entry = portal_dialog._pending.get(handle)
        if entry is None:
            return

        # Stash params and invoke the real fallback on the GUI thread so
        # QFileDialog (which must run on the main thread) never blocks
        # the UI or crashes the event loop.
        self._fallback_handle = handle
        self._fallback_dtype = dtype
        self._fallback_title = title
        self._fallback_current_name = current_name
        with portal_dialog._lock:
            self._fallback_filters = list(entry.get("filters", []))
        QMetaObject.invokeMethod(
            self,
            "_do_portal_fallback",
            Qt.ConnectionType.QueuedConnection,
        )

    @Slot()
    def _do_portal_fallback(self) -> None:
        """Run QFileDialog fallback on the GUI thread."""
        handle = getattr(self, "_fallback_handle", "")
        dtype = getattr(self, "_fallback_dtype", "open")
        title = getattr(self, "_fallback_title", "")
        current_name = getattr(self, "_fallback_current_name", "")
        filters = getattr(self, "_fallback_filters", [])

        # Build filter string
        if filters:
            filter_str = f"{t('dialog.filter.images')} ({' '.join(filters)})"
        else:
            filter_str = "Images (*.jpg *.jpeg *.png *.webp *.gif *.tiff *.bmp);;All files (*)"

        parent = self

        if dtype == "save":
            path, _ = QFileDialog.getSaveFileName(parent, title, current_name, filter_str)
            paths = [path] if path else []
        else:
            paths, _ = QFileDialog.getOpenFileNames(parent, title, "", filter_str)

        with portal_dialog._lock:
            entry = portal_dialog._pending.get(handle)
            if entry is None:
                return
            if paths:
                entry["status"] = "done"
                entry["paths"] = list(paths)
            else:
                entry["status"] = "cancelled"

        EventBus.emit(
            "dialog.result",
            {"handle": handle, "status": entry["status"], "paths": entry["paths"]},
        )

    @Slot()
    def _on_edit_tools_clicked(self) -> None:
        """Handle Show Edit Tools: show/hide both side panels together."""
        self.toggle_edit_tools()
        EventBus.emit("toolbar.clicked", {"button": "edit_tools"})

    def _on_file_open(self, event: str, meta: dict[str, Any]) -> None:
        """Handle file.open event.

        Args:
            event: Event name
            meta: Event metadata
        """
        # The status bar follows the active frame: a newly opened main image
        # must not overwrite the compare frame's info while it is focused.
        if self._active_frame == FRAME_MAIN:
            self._update_status_info(self.canvas)
            self._update_view_indicators(self.canvas)

    # --- Active frame -----------------------------------------------------

    @property
    def active_frame(self) -> str:
        """Name of the frame that receives toolbar/shortcut commands."""
        return self._active_frame

    @property
    def active_canvas(self) -> Canvas:
        """Canvas of the active frame."""
        return self._frame_canvases[self._active_frame]

    def canvas_for(self, frame: str) -> Canvas:
        """Return the canvas of ``frame`` (``"main"`` or ``"compare"``).

        Args:
            frame: Frame name

        Raises:
            ValueError: When the frame name is unknown
        """
        try:
            return self._frame_canvases[frame]
        except KeyError:
            raise ValueError(f"unknown frame: {frame!r}") from None

    def _on_frame_activated(self, frame: str) -> None:
        """Handle a click/wheel on a frame (``Canvas.activated``)."""
        if not compare_enabled():
            # Single-frame mode: nothing to focus, no active-frame events.
            return
        self.set_active_frame(frame)

    def set_active_frame(self, frame: str) -> str:
        """Make ``frame`` active: toolbar/shortcuts act on it from now on.

        Activating the compare frame also shows it, so the user never sends
        commands to an invisible frame.

        Args:
            frame: ``"main"`` or ``"compare"``

        Returns:
            The (validated) active frame name

        Raises:
            ValueError: When the frame name is unknown or the compare feature
                is currently disabled
        """
        self.canvas_for(frame)  # validate before touching any state

        if not compare_enabled():
            if frame != FRAME_MAIN:
                raise ValueError("compare feature is disabled")
            self._active_frame = FRAME_MAIN
            self._refresh_active_frame()
            return self._active_frame

        if frame == FRAME_COMPARE and not self._compare_active:
            self._on_compare(True)

        changed = frame != self._active_frame
        self._active_frame = frame
        self._refresh_active_frame()

        if changed:
            EventBus.emit("frame.activated", {"frame": frame})
        return self._active_frame

    def _refresh_active_frame(self) -> None:
        """Highlight the active frame and sync toolbar/status bar with it."""
        enabled = compare_enabled()
        for name, canvas in self._frame_canvases.items():
            canvas.set_active(enabled and name == self._active_frame)

        canvas = self.active_canvas
        self._update_view_indicators(canvas)
        self._update_status_info(canvas)

    def _update_view_indicators(self, canvas: Canvas) -> None:
        """Show ``canvas`` zoom/fit state in the status bar and toolbar."""
        label = getattr(self, "label_zoom", None)
        if label is not None:
            label.setText(t("status.zoom", zoom=int(round(canvas.zoom * 100))))
        toolbar = getattr(self, "bottom_toolbar", None)
        if toolbar is not None:
            toolbar.set_zoom_factor(canvas.zoom, canvas.fit_mode)

    def _update_status_info(self, canvas: Canvas) -> None:
        """Show the active frame's image size and file name in the status bar."""
        dims = getattr(self, "label_dims", None)
        filename = getattr(self, "label_filename", None)

        pixmap = canvas.pixmap
        if dims is not None:
            if pixmap is None:
                dims.setText("")
            else:
                dims.setText(t("status.dimensions", width=pixmap.width(), height=pixmap.height()))

        if filename is not None:
            path = canvas.image_path
            filename.setText(t("status.filename", filename=path.name) if path is not None else "")

    # --- View commands (always target the active frame) -------------------

    def _on_fit(self) -> None:
        """Handle fit-to-window."""
        self._apply_fit()

    def _apply_fit(self, canvas: Canvas | None = None) -> None:
        """Fit ``canvas`` (default: active frame) to its viewport."""
        target = canvas if canvas is not None else self.active_canvas
        target.fit_to_window()
        if target is self.canvas:
            self.doc.zoom = target.zoom
        if target is self.active_canvas:
            self._update_view_indicators(target)

    def _on_canvas_zoom(self, zoom: float) -> None:
        """Main canvas zoom changed: mirror it into the doc and the indicators."""
        self.doc.zoom = zoom
        if self._active_frame == FRAME_MAIN:
            self._update_view_indicators(self.canvas)

    def _on_compare_canvas_zoom(self, zoom: float) -> None:
        """Compare canvas zoom changed: refresh indicators only when active."""
        if self._active_frame == FRAME_COMPARE:
            self._update_view_indicators(self.compare_pane.canvas)

    def _target_canvas(self, canvas: Canvas | None) -> Canvas:
        """Return ``canvas`` or the active frame's canvas."""
        return canvas if canvas is not None else self.active_canvas

    def _apply_zoom(self, zoom: float, canvas: Canvas | None = None) -> None:
        """Apply ``zoom`` to a frame (clamped by the canvas)."""
        target = self._target_canvas(canvas)
        if target is self.canvas:
            self.doc.zoom_to(zoom)
            self.canvas.set_zoom(self.doc.zoom)
        else:
            target.set_zoom(zoom)

    def _on_zoom_in(self, canvas: Canvas | None = None) -> None:
        """Zoom a frame (default: active) in by one step."""
        target = self._target_canvas(canvas)
        self._apply_zoom(target.zoom * 1.25, target)

    def _on_zoom_out(self, canvas: Canvas | None = None) -> None:
        """Zoom a frame (default: active) out by one step."""
        target = self._target_canvas(canvas)
        self._apply_zoom(target.zoom / 1.25, target)

    def _on_actual(self, canvas: Canvas | None = None) -> None:
        """Show a frame's image at 100% (1:1)."""
        self._apply_zoom(1.0, canvas)

    def _on_rotate(self, degrees: int, canvas: Canvas | None = None) -> None:
        """Rotate a frame (default: active) by ``degrees``.

        On the main frame this is a document edit (undoable); on the compare
        frame it only changes how the reference is displayed.
        """
        target = self._target_canvas(canvas)
        if target is self.canvas:
            self.doc.rotate(degrees)
            self.canvas.set_rotation(self.doc.rotation)
        else:
            target.set_rotation(target.rotation + degrees)

    def _on_flip(self, axis: str = "h", canvas: Canvas | None = None) -> None:
        """Flip a frame (default: active) along ``axis`` ("h", "v" or "hv").

        Main frame: document edit. Compare frame: display-only.
        """
        target = self._target_canvas(canvas)
        if target is self.canvas:
            for single in axis:
                self.doc.flip(single)
            self.canvas.set_flip(self.doc.flip_h, self.doc.flip_v)
        else:
            flip_h = target.flip_h
            flip_v = target.flip_v
            if "h" in axis:
                flip_h = not flip_h
            if "v" in axis:
                flip_v = not flip_v
            target.set_flip(flip_h, flip_v)

    def _on_reset(self, canvas: Canvas | None = None) -> None:
        """Reset zoom/rotation/flip of a frame (default: active)."""
        target = self._target_canvas(canvas)
        if target is self.canvas:
            self.doc.reset()
            self.canvas.reset_view()
        else:
            target.reset_view()

    def _on_undo(self) -> None:
        """Undo the last edit and mirror the restored state on the canvas."""
        if self.doc.undo():
            self._sync_canvas_transforms()

    def _on_redo(self) -> None:
        """Redo the last undone edit and mirror the state on the canvas."""
        if self.doc.redo():
            self._sync_canvas_transforms()

    def _sync_canvas_transforms(self) -> None:
        """Push the document's rotation/flip state onto the canvas."""
        self.canvas.set_rotation(self.doc.rotation)
        self.canvas.set_flip(self.doc.flip_h, self.doc.flip_v)

    @Slot()
    def _on_toggle_fullscreen(self) -> None:
        """Toggle full screen mode."""
        if self.isFullScreen():
            self.showNormal()
        else:
            self.showFullScreen()
        self._sync_fullscreen_state()

    def _sync_fullscreen_state(self) -> None:
        """Sync the toolbar button with the real window state."""
        self.bottom_toolbar.set_fullscreen_active(self.isFullScreen())
        EventBus.emit("window.fullscreen", {"fullscreen": self.isFullScreen()})

    @Slot()
    def _on_toggle_compare(self) -> None:
        """Toggle compare view from the keyboard shortcut."""
        self._on_compare(not self._compare_active)

    def _on_compare(self, enabled: bool) -> None:
        """Show/hide the side-by-side compare pane.

        The pane sits to the right of the main canvas and holds a reference
        image. Turning compare on for the first time seeds the pane with the
        file as it is on disk (the canvas shows the same pixels plus the
        pending rotation/flip), so the user immediately gets "before" vs
        "after". Hiding the pane keeps its content, so re-opening restores the
        previous comparison instead of losing the reference image.

        Args:
            enabled: True to show the pane, False to hide it
        """
        if enabled and not compare_enabled():
            # Feature temporarily hidden: never open the pane, never focus it.
            self._compare_active = False
            self.compare_pane.setVisible(False)
            self.bottom_toolbar.set_compare_active(False)
            self._active_frame = FRAME_MAIN
            self._refresh_active_frame()
            return

        if enabled == self._compare_active:
            return

        self._compare_active = enabled
        self.compare_pane.setVisible(enabled)

        if enabled:
            # Seed the pane with the unedited image when it is still empty.
            image_path = self.canvas.image_path
            if (
                not self.compare_pane.has_image
                and image_path is not None
                and self.canvas.pixmap is not None
            ):
                self.compare_pane.set_image(image_path, self.canvas.pixmap)

        self.bottom_toolbar.set_compare_active(enabled)
        EventBus.emit("view.compare", {"active": enabled})

        # Hiding the active compare frame moves focus back to the main one.
        if not enabled and self._active_frame == FRAME_COMPARE:
            self.set_active_frame(FRAME_MAIN)

    def _on_compare_dropped(self, path: str) -> None:
        """Load an image dropped onto the compare pane."""
        self.load_compare_file(Path(path))

    def load_compare_file(self, path: Path) -> None:
        """Decode ``path`` in the background and show it in the compare pane.

        Args:
            path: Image file for the reference side
        """
        self._compare_loading = True
        self._compare_error = None
        self._compare_loader.load(path)

    def _on_compare_loaded(self, path: str, image: Any) -> None:
        """Display the reference image decoded in the background.

        Placing a reference image also focuses the compare frame, so the next
        toolbar action (zoom/rotate/fit) applies to the reference the user
        just dropped.
        """
        from PySide6.QtGui import QPixmap

        self._compare_loading = False
        self._compare_generation += 1
        self.compare_pane.set_image(Path(path), QPixmap.fromImage(image))
        if compare_enabled():
            self.set_active_frame(FRAME_COMPARE)

    def _on_compare_failed(self, path: str, error: str) -> None:
        """Report a failed compare-image decode."""
        self._compare_loading = False
        self._compare_generation += 1
        self._compare_error = error
        EventBus.emit("compare.load_failed", {"path": path, "error": error})

    def _on_window_state_change(self) -> None:
        """Sync toolbar state after a window state change (full screen)."""
        self.bottom_toolbar.set_fullscreen_active(self.isFullScreen())

    def changeEvent(self, event: QEvent) -> None:
        """React to window state changes such as entering full screen."""
        super().changeEvent(event)
        if event.type() == QEvent.Type.WindowStateChange and hasattr(self, "bottom_toolbar"):
            self._on_window_state_change()

    def _on_next(self) -> None:
        """Handle next image."""
        if not self._folder_files:
            return

        if self._current_index < len(self._folder_files) - 1:
            self._current_index += 1
            self._open_file(self._folder_files[self._current_index])
            EventBus.emit("file.next", {"index": self._current_index})

    def _on_prev(self) -> None:
        """Handle previous image."""
        if not self._folder_files:
            return

        if self._current_index > 0:
            self._current_index -= 1
            self._open_file(self._folder_files[self._current_index])
            EventBus.emit("file.prev", {"index": self._current_index})

    def _on_delete_clicked(self) -> None:
        """Delete the current image via the automation API (single code path).

        The UI must not implement its own delete: an earlier version only
        emitted an event and cleared the view, so pressing Delete looked like
        it worked while the file stayed on disk.
        """
        if self.doc.path is None:
            return

        runner = self._automation_runner
        if runner is not None:
            try:
                runner("file.delete", {})
                return
            except Exception:  # noqa: BLE001 - fall back to the direct path
                pass

        from photon.core.trash import move_to_trash

        path = self.doc.path
        move_to_trash(path)
        EventBus.emit("file.delete", {"path": str(path)})
        self.on_file_trashed(path)

    def on_file_trashed(self, path: Path) -> None:
        """React to ``file.delete`` trashing a file.

        Removes the file from the folder list and shows the next (or
        previous) image, so navigation never points at a deleted entry.

        Args:
            path: File that was moved to the trash
        """
        if path in self._folder_files:
            index = self._folder_files.index(path)
            del self._folder_files[index]

            if self._folder_files:
                self._current_index = min(index, len(self._folder_files) - 1)
                self._open_file(self._folder_files[self._current_index])
                return
            self._current_index = -1

        self._clear_view()

    def _clear_view(self) -> None:
        """Clear the document, canvas and status labels."""
        self.doc.path = None
        self.doc.width = 0
        self.doc.height = 0
        self.doc.format = ""
        self.doc.size_bytes = 0
        self.doc.reset()

        self.canvas.set_image(Path(""))  # Clear canvas
        if self._active_frame == FRAME_MAIN:
            self._update_status_info(self.canvas)
        self.setWindowTitle(t("app.title"))

    # --- Drag & Drop ---

    def dragEnterEvent(self, event: QDragEnterEvent) -> None:
        """Handle drag enter event."""
        if event.mimeData().hasUrls():
            # Check if any URL is an image file
            image_exts = {".jpg", ".jpeg", ".png", ".webp", ".gif", ".tiff", ".bmp"}
            for url in event.mimeData().urls():
                path = Path(url.toLocalFile())
                if path.suffix.lower() in image_exts:
                    event.acceptProposedAction()
                    return

    def dropEvent(self, event: QDropEvent) -> None:
        """Handle drop event."""
        for url in event.mimeData().urls():
            path = Path(url.toLocalFile())
            if path.exists():
                self.open_path(path)
                break

    def closeEvent(self, event: QCloseEvent) -> None:
        """Handle window close — save state."""
        config_set("window.width", self.width())
        config_set("window.height", self.height())
        config_set("window.x", self.x())
        config_set("window.y", self.y())

        EventBus.emit("window.closed", {"width": self.width(), "height": self.height()})

        super().closeEvent(event)
