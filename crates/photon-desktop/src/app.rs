//! `iced 0.14` daemon + multi-window GUI adapter (`photon`) using `photon-ui`.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::widget::{
    checkbox, column, container, image as iced_image, operation as widget_operation, row, rule,
    scrollable, slider, text,
};
use iced::{
    Alignment, Element, Fill, Length, Point, Rectangle, Size, Subscription, Task,
    Theme as IcedTheme, event, mouse, window,
};
use image::RgbaImage;
use photon_core::CoreHandle;
use photon_i18n::{tr, tr_ui_text};
use photon_panels::SidebarOrderState;
use photon_protocol::{
    ApplyTarget, BrowserLeftPanel, BrowserRightPanel, BrowserSortField, BrowserThumbSize,
    BrowserViewMode, ClientKind, ColorAdjustments, Command, CommandResponse, EditorRightPanel,
    EditorToolTab, Envelope, EventKind, FlipAxis, Language, LayerEffect, LiquifyMode,
    LiveStrokeKind, MoveDirection, Query, ResizeFilter, ResizeMode, SortOrder,
    SystemOpenWindowMode, Theme, UiStateSnapshot, UiText, ViewerRightPanel, ViewerViewMode,
    WebtoonFitMode, Workspace,
};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::dialog::{
    ConfirmDialogSpec, DialogEvent, DialogManager, DialogSpec, DialogWindowState,
    ManagedDialogSpec, OverlayMode, PromptDialogSpec, ToastKind, ToastSpec, alert_dialog_spec,
    confirm_dialog_spec, dialog_style_for_theme, managed_dialog_stack, prompt_dialog_spec,
    with_toast_banner,
};
use photon_ui::icons::Icon;
use photon_ui::menu::{
    MenuEntry, MenuItem, menu_panel_entries, with_anchored_menu, with_bottom_right_menu_at,
    with_context_menu, with_top_right_menu, with_top_right_menu_at,
};
use photon_ui::panel::{section_card, status_bar};
use photon_ui::theme::{
    PaletteTokens, TEXT_SIZE_CONTROL, app_shell_style, hairline_rule_style, slider_style,
};
use photon_ui::tooltip::{TooltipPosition, with_tooltip_pos};

use crate::batch_dialog::{
    BatchDialogKind, BatchDialogMsg, BatchDialogState, build_batch_convert_dialog,
    build_batch_crop_dialog, build_batch_resize_dialog,
};
use crate::browser_view::{
    browser_filter_input_id, build_browser_context_menu_entries, build_browser_help_menu_entries,
    build_browser_sort_menu, build_browser_toolbar, build_browser_view,
    build_browser_window_menu_entries, build_quick_access_context_menu,
};
use crate::editor_view::{
    AdjustmentPopupTab, ColorBalanceZone, ColorPickerTarget, CompareMode, CurvesChannel,
    DimGridAction, DimensionInputState, EditorTool, EditorUiState, build_color_picker_dialog,
    build_editor_add_layer_menu, build_editor_canvas_context_menu, build_editor_layer_context_menu,
    build_editor_toolbar, build_editor_view, build_export_dialog, crop_uv_to_signed,
    default_curves, format_hex, parse_hex_rgb,
};
use crate::inspector::{ViewerUiPresentation, build_ui_tree};
use crate::panel_splitter::{DockDragState, DockSplitterKind};
use crate::shader_canvas::{
    CanvasEvent, CpuImageFrame, default_viewport_bg_rgb, dual_spread_geometry,
};
use crate::viewer_view::{
    build_viewer_context_menu, build_viewer_toolbar, build_viewer_view, dual_spread_slots,
    dual_spread_slots_for, step_dual_spread, viewer_counter_summary,
    viewer_reader_options_right_offset, viewer_status_summary, webtoon_index_at_scroll,
    webtoon_offset_for_index, webtoon_scroll_id,
};

static CORE_HANDLE: OnceLock<CoreHandle> = OnceLock::new();
static GPU_BACKEND_NAME: OnceLock<String> = OnceLock::new();
static OPENED_WITH_FILES: OnceLock<bool> = OnceLock::new();

pub fn init_globals(handle: CoreHandle, gpu_backend: String, opened_with_files: bool) {
    let _ = CORE_HANDLE.set(handle);
    let _ = GPU_BACKEND_NAME.set(gpu_backend);
    let _ = OPENED_WITH_FILES.set(opened_with_files);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowRole {
    Main,
    AuxiliaryViewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveMenu {
    BrowserFile,
    BrowserView,
    BrowserTools,
    BrowserPlugins,
    BrowserWindow,
    BrowserHelp,
    BrowserSort,
    ReaderOptions,
    EditorFile,
    EditorEdit,
    EditorImage,
    EditorFilter,
    EditorView,
    EditorWindow,
    EditorHelp,
    EditorAddLayer,
    WorkspacePanels,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DesktopDialogId {
    Settings,
    NewFolder,
    ConfirmResetLayout,
    ConfirmUnsavedLeave,
    Resize,
    Export,
    ColorPicker,
    BatchResize,
    BatchCrop,
    BatchConvert,
    RenameItem,
    ConfirmDeletePermanently,
    Shortcuts,
    About,
    Alert,
    AdjustColor,
    AdjustHsv,
    AdjustColorBalance,
    AdjustCurves,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ActiveContextMenu {
    BrowserEntry {
        x: f32,
        y: f32,
        path: String,
    },
    BrowserQuickAccess {
        x: f32,
        y: f32,
        path: String,
    },
    Viewer {
        x: f32,
        y: f32,
        target_path: Option<String>,
    },
    EditorCanvas {
        x: f32,
        y: f32,
    },
    EditorLayer {
        x: f32,
        y: f32,
        layer_id: u64,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum InAppDialog {
    NewFolder {
        parent: String,
        name: String,
        error: Option<String>,
    },
    ConfirmResetLayout,
    ConfirmUnsavedLeave {
        close_editor: bool,
    },
    Resize {
        dims: DimensionInputState,
        filter: ResizeFilter,
        mode: ResizeMode,
        fill: Option<[u8; 3]>,
        error: Option<String>,
    },
    Export {
        path: String,
        quality: u8,
        max_edge: String,
        metadata: bool,
        error: Option<String>,
    },
    ColorPicker {
        target: ColorPickerTarget,
        r: u8,
        g: u8,
        b: u8,
        hex: String,
        transparent: bool,
        previous_viewport_bg: Option<[u8; 3]>,
    },
    Batch(BatchDialogState),
    RenameItem {
        path: String,
        name: String,
        error: Option<String>,
    },
    ConfirmDeletePermanently {
        paths: Vec<String>,
    },
    Alert {
        id: DesktopDialogId,
        title: String,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct ToastState {
    pub id: u64,
    pub message: String,
    pub kind: ToastKind,
}

pub struct PhotonGui {
    pub(crate) core: CoreHandle,
    pub(crate) snapshot: Arc<UiStateSnapshot>,
    windows: BTreeMap<window::Id, WindowRole>,
    main_window: Option<window::Id>,
    pub(crate) window_size: Size,
    resize_save_seq: u64,
    suppress_resize_until: Option<Instant>,
    pub(crate) folder_input: String,
    pub(crate) browser_filter: String,
    pub(crate) browser_filter_bar_open: bool,
    pub(crate) browser_filter_case_sensitive: bool,
    pub(crate) browser_filter_regex: bool,
    last_click: Option<(String, Instant)>,
    pub(crate) thumbnails: HashMap<String, iced_image::Handle>,
    thumbnails_loading: BTreeSet<String>,
    pub(crate) editor_layer_thumbs: HashMap<u64, iced_image::Handle>,
    pub(crate) editor_mask_thumbs: HashMap<u64, iced_image::Handle>,
    pub(crate) zoom: f32,
    pub(crate) pan_x: f32,
    pub(crate) pan_y: f32,
    pub(crate) pending_adj: ColorAdjustments,
    pub(crate) viewer_adj: ColorAdjustments,
    transfer_adj_to_editor: Option<ColorAdjustments>,
    pub(crate) viewer_frame: Option<CpuImageFrame>,
    pub(crate) viewer_pair_frame: Option<CpuImageFrame>,
    pub(crate) editor_frame: Option<CpuImageFrame>,
    pub(crate) editor_original_frame: Option<CpuImageFrame>,
    pub(crate) editor_ui: EditorUiState,
    pub(crate) viewer_mode: ViewerViewMode,
    pub(crate) viewer_manga_rtl: bool,
    pub(crate) viewer_manga_cover_separate: bool,
    pub(crate) viewer_manga_page_gap: u16,
    pub(crate) viewer_webtoon_fit_mode: WebtoonFitMode,
    pub(crate) browser_inspector_open: bool,
    pub(crate) browser_preview_open: bool,
    pub(crate) browser_info_open: bool,
    pub(crate) browser_quick_access_open: bool,
    pub(crate) browser_explorer_open: bool,
    pub(crate) browser_quick_access_collapsed: bool,
    pub(crate) browser_explorer_collapsed: bool,
    pub(crate) browser_preview_collapsed: bool,
    pub(crate) browser_info_collapsed: bool,
    pub(crate) browser_left_panels: SidebarOrderState<BrowserLeftPanel>,
    pub(crate) browser_right_panels: SidebarOrderState<BrowserRightPanel>,
    pub(crate) viewer_info_open: bool,
    pub(crate) viewer_adjustments_open: bool,
    pub(crate) viewer_right_panels: SidebarOrderState<ViewerRightPanel>,
    pub(crate) viewer_filmstrip_open: bool,
    pub(crate) viewer_color_popover_open: bool,
    pub(crate) dock_drag: DockDragState,
    pub(crate) browser_left_width_drag: Option<f32>,
    pub(crate) browser_right_width_drag: Option<f32>,
    pub(crate) viewer_right_width_preview: Option<f32>,
    pub(crate) editor_tool_tab: EditorToolTab,
    pub(crate) editor_history_open: bool,
    pub(crate) editor_layers_open: bool,
    pub(crate) editor_history_collapsed: bool,
    pub(crate) editor_layers_collapsed: bool,
    pub(crate) editor_panels: SidebarOrderState<EditorRightPanel>,
    pub(crate) editor_panel_order: Vec<EditorRightPanel>,
    pub(crate) dragging_editor_panel: Option<EditorRightPanel>,
    pub(crate) editor_panel_drop_target: Option<EditorRightPanel>,
    pending_leave_to_browser: bool,
    pending_layout_commands: usize,
    last_core_layout: photon_protocol::LayoutState,
    pending_screenshots: BTreeMap<window::Id, (String, String)>,
    settings_open: bool,
    dialog: Option<InAppDialog>,
    pub(crate) dialog_manager: DialogManager<DesktopDialogId>,
    toast: Option<ToastState>,
    toast_seq: u64,
    pub(crate) path_editing: bool,
    pub(crate) active_menu: Option<ActiveMenu>,
    pub(crate) context_menu: Option<ActiveContextMenu>,
    pub(crate) last_cursor: Option<Point>,
    pub(crate) viewer_quarter_turns: u8,
    pub(crate) webtoon_zoom: f32,
    webtoon_scroll_y: f32,
    pub(crate) webtoon_visible_index: Option<usize>,
    pub(crate) image_sizes: HashMap<String, (u32, u32)>,
    pub(crate) webtoon_pages: HashMap<String, iced_image::Handle>,
    webtoon_loading: BTreeSet<String>,
    webtoon_order: VecDeque<String>,
    pub(crate) is_fullscreen: bool,
    pub(crate) crop_uv: Option<[f32; 4]>,
    pub(crate) viewport_background: Option<[u8; 3]>,
    pub(crate) expanded_folders: BTreeSet<String>,
    pub(crate) folder_children: HashMap<String, Vec<(String, String)>>,
}

#[derive(Debug, Clone)]
pub enum Message {
    StateUpdated(Arc<UiStateSnapshot>),
    CoreEvent(photon_protocol::Event),
    KeyEvent(keyboard::Event),
    WindowOpened(window::Id),
    WindowMonitorSizeCheck(window::Id, Option<Size>, bool),
    WindowClosed(window::Id),
    WindowResized(window::Id, Size),
    PersistWindowSize(u64, u32, u32),
    PersistWindowSizeIfWindowed(u64, u32, u32, bool),
    Dispatch(Command),
    CommandCompleted(Result<CommandResponse, String>),
    LayoutCommandCompleted(Result<CommandResponse, String>),
    PickFolderDialog,
    FolderPicked(Option<PathBuf>),
    PickFavoriteFolderDialog,
    FavoriteFolderPicked(Option<PathBuf>),
    PickImagesDialog,
    ImagesPicked(Vec<PathBuf>),
    PickSaveAsDialog,
    SaveAsPicked(Option<PathBuf>),
    OpenNewFolderDialog,
    NewFolderNameChanged(String),
    ConfirmNewFolder,
    RequestResetLayout,
    ConfirmResetLayout,
    RequestEditorLeave {
        close_editor: bool,
    },
    RequestEditorLeaveToBrowser,
    DiscardAndLeaveEditor(bool),
    SaveAndLeaveEditor(bool),
    CloseDialog,
    OpenResizeDialog,
    ResizeDimAction(DimGridAction),
    ResizeFilterChanged(ResizeFilter),
    ResizeModeChanged(ResizeMode),
    ConfirmResize,
    ToggleCropMode,
    SetCropPreset(Option<f32>),
    ApplyCrop,
    CancelCrop,
    DismissToast(Option<u64>),
    ToggleMenu(ActiveMenu),
    HoverMenu(ActiveMenu),
    CloseMenu,
    OpenShortcutsDialog,
    OpenAboutDialog,
    WindowCursorMoved(Point),
    WindowMouseUp,
    DialogEvent(DialogEvent<DesktopDialogId>),
    ToggleBrowserInspector,
    BrowserOpenSelectedInEditor,
    ToggleBrowserFilterBar,
    ToggleBrowserFilterCaseSensitive,
    ToggleBrowserFilterRegex,
    ClearBrowserFilter,
    BeginDragBrowserPanel(BrowserLeftPanel),
    HoverBrowserPanelDrop(BrowserLeftPanel),
    DropBrowserPanel(BrowserLeftPanel),
    SwapBrowserLeftPanels,
    ToggleBrowserPanelVisible(BrowserLeftPanel),
    ToggleBrowserLeftPanelCollapse(BrowserLeftPanel),
    CloseBrowserLeftPanel(BrowserLeftPanel),
    BeginDragBrowserRightPanel(BrowserRightPanel),
    HoverBrowserRightPanelDrop(BrowserRightPanel),
    DropBrowserRightPanel(BrowserRightPanel),
    SwapBrowserRightPanels,
    ToggleBrowserRightPanelVisible(BrowserRightPanel),
    ToggleBrowserRightPanelCollapse(BrowserRightPanel),
    CloseBrowserRightPanel(BrowserRightPanel),
    PanelPopupToggleBrowserLeft(BrowserLeftPanel),
    PanelPopupToggleBrowserRight(BrowserRightPanel),
    PanelPopupToggleViewerAdj,
    PanelPopupToggleViewerInfo,
    PanelPopupToggleViewerFilmstrip,
    PanelPopupToggleEditorTools,
    PanelPopupToggleEditorPanel(EditorRightPanel),
    BeginDragViewerPanel(ViewerRightPanel),
    HoverViewerPanelDrop(ViewerRightPanel),
    DropViewerPanel(ViewerRightPanel),
    SwapViewerRightPanels,
    ToggleViewerPanelCollapse(ViewerRightPanel),
    CloseViewerPanel(ViewerRightPanel),
    SelectEditorToolTab(EditorToolTab),
    ToggleEditorToolsVisible,
    CloseEditorToolsPanel,
    ToggleEditorPanelVisible(EditorRightPanel),
    ToggleEditorPanelCollapse(EditorRightPanel),
    CloseEditorPanel(EditorRightPanel),
    BeginDragEditorPanel(EditorRightPanel),
    HoverEditorPanelDrop(EditorRightPanel),
    DropEditorPanel(EditorRightPanel),
    SwapEditorPanelOrder,
    EditorCancelLayerRename,
    OpenViewerContextMenu {
        path: Option<String>,
        pos: Option<[f32; 2]>,
    },
    OpenLayerContextMenu {
        layer_id: u64,
    },
    OpenBrowserContextMenu {
        path: String,
        x: f32,
        y: f32,
    },
    OpenQuickAccessContextMenu {
        path: String,
        x: f32,
        y: f32,
    },
    CloseContextMenu,
    ContextMenuDispatch(Command),
    CopyImageToClipboard(Option<String>),
    CopyImageCompleted(bool),
    RevealInFolder(String),
    ViewerCanvas(Option<String>, CanvasEvent),
    ViewerZoomActual,
    ViewerRotatePreview(i8),
    ToggleFullscreen,
    ToggleFilmstrip,
    CloseFilmstrip,
    ToggleViewerInspector,
    CopyViewerPath,
    ToggleViewerColorPopover,
    SetViewerAdjExposure(f32),
    SetViewerAdjGamma(f32),
    SetViewerAdjBrightness(f32),
    SetViewerAdjContrast(f32),
    SetViewerAdjSaturation(f32),
    SetViewerAdjHighlights(f32),
    SetViewerAdjShadows(f32),
    ResetViewerAdj,
    ViewerAdjOpenInEditor,
    ViewerSplitterDrag(f32),
    ViewerSplitterRelease(f32),
    TogglePathEdit(bool),
    FolderInputChanged(String),
    SubmitFolderInput,
    BrowserFilterChanged(String),
    SetBrowserViewMode(BrowserViewMode),
    SetBrowserSortField(BrowserSortField),
    SetBrowserSortOrder(SortOrder),
    SetBrowserThumbSize(BrowserThumbSize),
    OpenBatchDialog(BatchDialogKind),
    BatchDialog(BatchDialogMsg),
    BatchOutputFolderPicked(Option<PathBuf>),
    BrowserSplitterDrag(f32),
    BrowserSplitterRelease(f32),
    BrowserRightSplitterDrag(f32),
    BrowserRightSplitterRelease(f32),
    BrowserCopyPath(String),
    BrowserStartRename(String),
    BrowserRenameChanged(String),
    BrowserConfirmRename,
    BrowserCopyTo(Vec<String>),
    BrowserMoveTo(Vec<String>),
    BrowserCopyMoveDestPicked {
        is_move: bool,
        paths: Vec<String>,
        dest: Option<PathBuf>,
    },
    BrowserConfirmDeletePermanently(Vec<String>),
    BrowserExecuteDeletePermanently(Vec<String>),
    DockSplitterDrag(DockSplitterKind, f32),
    DockSplitterRelease(DockSplitterKind, f32),
    SetViewerViewMode(ViewerViewMode),
    SetDefaultViewerViewMode(ViewerViewMode),
    DualStepSpread(isize),
    SetMangaRtl(bool),
    SetMangaSeparateCover(bool),
    SetMangaPageGap(u16),
    SetWebtoonFitMode(WebtoonFitMode),
    SetSystemOpenWindowMode(SystemOpenWindowMode),
    WebtoonScrolled(scrollable::Viewport),
    WebtoonPagesLoaded(Vec<(String, u32, u32, Vec<u8>)>),
    BrowserEntryActivated {
        path: String,
        is_dir: bool,
        is_image: bool,
    },
    ThumbnailsLoaded(Vec<(String, u32, u32, Vec<u8>)>),
    ToggleFolderExpand(String),
    FolderChildrenLoaded(String, Vec<(String, String)>),
    ZoomStep(f32),
    Canvas(CanvasEvent),
    SetBrightness(f32),
    SetContrast(f32),
    SetSaturation(f32),
    SetExposure(f32),
    SetGamma(f32),
    SetHighlights(f32),
    SetShadows(f32),
    ApplyColorAdjustments,
    EditorResetPendingAdjustments,
    EditorSelectTool(EditorTool),
    EditorOpenAdjustPopup(AdjustmentPopupTab),
    EditorCloseAdjustPopup {
        apply: bool,
    },
    EditorSetLiquifyMode(LiquifyMode),
    EditorSetLiquifyRadius(f32),
    EditorSetLiquifyStrength(f32),
    EditorCycleCompareMode,
    EditorSetCompareMode(CompareMode),
    EditorSwapColors,
    EditorResetColors,
    OpenColorPicker(ColorPickerTarget),
    ColorPickerSetRgb(u8, u8, u8),
    ColorPickerHexChanged(String),
    ColorPickerToggleTransparent(bool),
    ResetViewportBackground,
    ConfirmColorPicker,
    EditorSetBrushRadius(f32),
    EditorSetBrushOpacity(f32),
    EditorSetBrushHardness(f32),
    EditorSetBrushFlow(f32),
    EditorSetBrushSpacing(f32),
    EditorSetBrushTip(photon_protocol::BrushTip),
    EditorSetApplyTarget(ApplyTarget),
    EditorSetHsvHue(f32),
    EditorSetHsvSat(f32),
    EditorSetHsvVal(f32),
    EditorApplyHsv,
    EditorSetBalanceZone(ColorBalanceZone),
    EditorSetBalanceAxis(usize, f32),
    EditorTogglePreserveLuminosity(bool),
    EditorApplyColorBalance,
    EditorSelectCurvesChannel(CurvesChannel),
    EditorCurvesChanged(Vec<[f32; 2]>),
    EditorApplyCurves,
    EditorSetRotateAngle(f64),
    EditorApplyRotateAngle,
    EditorCanvasDimAction(DimGridAction),
    EditorSetCanvasAnchor(photon_protocol::CanvasAnchor),
    EditorApplyCanvasSize,
    OpenExportDialog,
    PickExportDialog,
    ExportPathPicked(Option<PathBuf>),
    ExportPathChanged(String),
    ExportQualityChanged(u8),
    ExportMaxEdgeChanged(String),
    ExportMetadataToggled(bool),
    ConfirmExport,
    EditorStartLayerRename(u64, String),
    EditorLayerRenameChanged(String),
    EditorCommitLayerRename,
    ViewerFrameLoaded(Option<CpuImageFrame>),
    ViewerPairFrameLoaded(Option<CpuImageFrame>),
    EditorFrameLoaded(Option<CpuImageFrame>),
    EditorOriginalFrameLoaded(Option<CpuImageFrame>),
    EditorThumbnailsLoaded(u64, Vec<photon_image::LayerThumbnail>),
    TriggerWindowScreenshot(window::Id),
    ScreenshotReady(window::Id, window::Screenshot),
    OpenSettings,
    CloseSettings,
    Noop,
}

pub fn boot() -> (PhotonGui, Task<Message>) {
    let core = CORE_HANDLE
        .get()
        .expect("CoreHandle must be initialized before iced::daemon")
        .clone();
    let snapshot = core.latest_snapshot();
    let folder_input = snapshot.meta.current_folder.clone().unwrap_or_default();
    let opened_with_files = OPENED_WITH_FILES.get().copied().unwrap_or(false);
    // On first launch window_width/height will be the protocol default (1240×800).
    // Clamp to new minimum so the Viewer toolbar is never clipped.
    let remembered_size = Size::new(
        (snapshot.meta.layout.window_width as f32).clamp(960.0, 3840.0),
        (snapshot.meta.layout.window_height as f32).clamp(540.0, 2160.0),
    );
    let launch_mode = snapshot.meta.layout.system_open_window_mode;
    let start_fullscreen = opened_with_files && launch_mode == SystemOpenWindowMode::Fullscreen;
    let mut suppress_resize_until = None;
    let init_size = if opened_with_files {
        match launch_mode {
            SystemOpenWindowMode::FitImage => {
                let target_img = snapshot
                    .meta
                    .viewer_path
                    .as_deref()
                    .or_else(|| snapshot.meta.editor.as_ref().map(|e| e.path.as_str()));
                if let Some((iw, ih)) =
                    target_img.and_then(|p| photon_image::probe_dimensions(Path::new(p)))
                {
                    suppress_resize_until = Some(Instant::now() + Duration::from_millis(1200));
                    window_size_for_image(&snapshot, iw, ih)
                } else {
                    remembered_size
                }
            }
            SystemOpenWindowMode::Fullscreen => {
                suppress_resize_until = Some(Instant::now() + Duration::from_millis(1200));
                remembered_size
            }
            SystemOpenWindowMode::Remembered => remembered_size,
        }
    } else {
        // Ignore initial WM map/configure resize jitter so remembered size isn't overwritten.
        suppress_resize_until = Some(Instant::now() + Duration::from_millis(600));
        remembered_size
    };

    let (id, open_task) = window::open(window::Settings {
        size: init_size,
        min_size: Some(Size::new(960.0, 540.0)),
        ..Default::default()
    });

    let mut windows = BTreeMap::new();
    windows.insert(id, WindowRole::Main);

    let mut editor_ui = EditorUiState::default();
    if let Some(ed) = &snapshot.meta.editor {
        editor_ui.sync_document_dims(ed.width, ed.height);
    }
    let init_layout = &snapshot.meta.layout;
    let viewer_mode = init_layout.viewer_view_mode;
    let viewer_manga_rtl = init_layout.manga_rtl;
    let viewer_manga_cover_separate = init_layout.manga_cover_separate;
    let viewer_manga_page_gap = init_layout.manga_page_gap;
    let viewer_webtoon_fit_mode = init_layout.webtoon_fit_mode;
    let browser_preview_open =
        init_layout.panel("browser.preview") || init_layout.panel("browser.inspector");
    let browser_info_open =
        init_layout.panel("browser.info") || init_layout.panel("browser.inspector");
    let browser_inspector_open = browser_preview_open || browser_info_open;
    let browser_quick_access_open = init_layout.panel("browser.quick_access");
    let browser_explorer_open = init_layout.panel("browser.explorer");
    let browser_left_panels = SidebarOrderState::new(
        init_layout.browser_left_panel_order.clone(),
        &BrowserLeftPanel::ALL,
    );
    let browser_right_panels = SidebarOrderState::new(
        init_layout.browser_right_panel_order.clone(),
        &BrowserRightPanel::ALL,
    );
    let viewer_info_open =
        init_layout.panel("viewer.info") || init_layout.panel("viewer.inspector");
    let viewer_adjustments_open = init_layout.panel("viewer.adjustments");
    let viewer_right_panels = SidebarOrderState::new(
        init_layout.viewer_right_panel_order.clone(),
        &ViewerRightPanel::ALL,
    );
    let viewer_filmstrip_open = init_layout.panel("viewer.filmstrip");
    let editor_tool_tab = init_layout.editor_tool_tab;
    let editor_history_open = init_layout.panel("editor.history");
    let editor_layers_open = init_layout.panel("editor.layers");
    let editor_panels = SidebarOrderState::new(
        init_layout.editor_right_panel_order.clone(),
        &EditorRightPanel::ALL,
    );
    let editor_panel_order = editor_panels.order().to_vec();
    let last_core_layout = init_layout.clone();

    let mut app = PhotonGui {
        core,
        snapshot,
        windows,
        main_window: Some(id),
        window_size: init_size,
        resize_save_seq: 0,
        suppress_resize_until,
        folder_input,
        browser_filter: String::new(),
        browser_filter_bar_open: false,
        browser_filter_case_sensitive: false,
        browser_filter_regex: false,
        last_click: None,
        thumbnails: HashMap::new(),
        thumbnails_loading: BTreeSet::new(),
        editor_layer_thumbs: HashMap::new(),
        editor_mask_thumbs: HashMap::new(),
        zoom: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
        pending_adj: ColorAdjustments::default(),
        viewer_adj: ColorAdjustments::default(),
        transfer_adj_to_editor: None,
        viewer_frame: None,
        viewer_pair_frame: None,
        editor_frame: None,
        editor_original_frame: None,
        editor_ui,
        viewer_mode,
        viewer_manga_rtl,
        viewer_manga_cover_separate,
        viewer_manga_page_gap,
        viewer_webtoon_fit_mode,
        browser_inspector_open,
        browser_preview_open,
        browser_info_open,
        browser_quick_access_open,
        browser_explorer_open,
        browser_quick_access_collapsed: false,
        browser_explorer_collapsed: false,
        browser_preview_collapsed: false,
        browser_info_collapsed: false,
        browser_left_panels,
        browser_right_panels,
        viewer_info_open,
        viewer_adjustments_open,
        viewer_right_panels,
        viewer_filmstrip_open,
        viewer_color_popover_open: viewer_adjustments_open,
        dock_drag: DockDragState::default(),
        browser_left_width_drag: None,
        browser_right_width_drag: None,
        viewer_right_width_preview: None,
        editor_tool_tab,
        editor_history_open,
        editor_layers_open,
        editor_history_collapsed: false,
        editor_layers_collapsed: false,
        editor_panels,
        editor_panel_order,
        dragging_editor_panel: None,
        editor_panel_drop_target: None,
        pending_leave_to_browser: false,
        pending_layout_commands: 0,
        last_core_layout,
        pending_screenshots: BTreeMap::new(),
        settings_open: false,
        dialog: None,
        dialog_manager: DialogManager::new(),
        toast: None,
        toast_seq: 0,
        path_editing: false,
        active_menu: None,
        context_menu: None,
        last_cursor: None,
        viewer_quarter_turns: 0,
        webtoon_zoom: 1.0,
        webtoon_scroll_y: 0.0,
        webtoon_visible_index: None,
        image_sizes: HashMap::new(),
        webtoon_pages: HashMap::new(),
        webtoon_loading: BTreeSet::new(),
        webtoon_order: VecDeque::new(),
        is_fullscreen: start_fullscreen,
        crop_uv: None,
        viewport_background: None,
        expanded_folders: BTreeSet::new(),
        folder_children: HashMap::new(),
    };

    probe_viewer_image_sizes(&mut app);
    let thumb_tasks = schedule_missing_thumbnails(&mut app);
    let webtoon_tasks = if app.viewer_mode == ViewerViewMode::Webtoon {
        let center = app.snapshot.meta.viewer_index.unwrap_or(0);
        schedule_webtoon_pages_near(&mut app, center)
    } else {
        Task::none()
    };
    let tree_load_tasks = sync_expanded_tree_tasks(&mut app, true);
    let load_task = reload_active_frames(
        &app.core,
        &app.snapshot,
        &app.image_sizes,
        &app.viewer_ui_presentation(),
    );
    let tree_task = sync_ui_tree_task(&app);
    let fs_task = if start_fullscreen {
        window::set_mode(id, window::Mode::Fullscreen)
    } else {
        Task::none()
    };
    (
        app,
        Task::batch([
            open_task.map(Message::WindowOpened),
            fs_task,
            load_task,
            tree_task,
            tree_load_tasks,
            thumb_tasks,
            webtoon_tasks,
        ]),
    )
}

pub fn title(state: &PhotonGui, window_id: window::Id) -> String {
    let lang = state.snapshot.meta.language;
    let base = tr(lang, "app-title");
    let meta = &state.snapshot.meta;
    let role = state
        .windows
        .get(&window_id)
        .copied()
        .unwrap_or(WindowRole::Main);
    let ws = match role {
        WindowRole::AuxiliaryViewer => Workspace::Viewer,
        WindowRole::Main => meta.workspace,
    };
    match ws {
        Workspace::Browser => {
            if let Some(folder) = meta.current_folder.as_deref() {
                let name = Path::new(folder)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(folder);
                format!("{name} — {base}")
            } else {
                format!("{base} — {}", tr(lang, "workspace-browser"))
            }
        }
        Workspace::Viewer => match state.viewer_mode {
            ViewerViewMode::Webtoon => {
                let count = meta.viewer_count;
                let cur = state
                    .webtoon_visible_index
                    .or(meta.viewer_index)
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let title_left = meta
                    .current_folder
                    .as_deref()
                    .and_then(|f| Path::new(f).file_name())
                    .and_then(|n| n.to_str())
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| tr(lang, "action-viewer-manhwa"));
                format!("{title_left} ({cur}/{count}) — {base}")
            }
            ViewerViewMode::Double => {
                let imgs = &state.snapshot.viewer_images;
                let (left_slot, right_slot) = dual_spread_slots(state);
                let name_for = |idx: usize| {
                    imgs.get(idx)
                        .and_then(|p| Path::new(p).file_name())
                        .and_then(|n| n.to_str())
                        .unwrap_or("-")
                };
                match (left_slot, right_slot) {
                    (Some((li, _)), Some((ri, _))) => {
                        let a = name_for(li);
                        let b = name_for(ri);
                        format!("{a} & {b} ({}·{}/{}) — {base}", li + 1, ri + 1, imgs.len())
                    }
                    (Some((li, _)), None) => {
                        format!("{} ({}/{}) — {base}", name_for(li), li + 1, imgs.len())
                    }
                    _ => format!("{base} — {}", tr(lang, "workspace-viewer")),
                }
            }
            ViewerViewMode::Single => {
                if let Some(name) = meta
                    .viewer_path
                    .as_deref()
                    .and_then(|p| Path::new(p).file_name())
                    .and_then(|n| n.to_str())
                {
                    let idx_part = if meta.viewer_count > 0 {
                        format!(
                            " ({}/{})",
                            meta.viewer_index.unwrap_or(0) + 1,
                            meta.viewer_count
                        )
                    } else {
                        String::new()
                    };
                    if let Some(frame) = &state.viewer_frame {
                        format!(
                            "{name} · {}×{}{idx_part} — {base}",
                            frame.width, frame.height
                        )
                    } else {
                        format!("{name}{idx_part} — {base}")
                    }
                } else {
                    format!("{base} — {}", tr(lang, "workspace-viewer"))
                }
            }
        },
        Workspace::Editor => {
            if let Some(doc) = &meta.editor {
                let dirty = if doc.modified { " *" } else { "" };
                format!(
                    "{}{} · {}×{} — {base}",
                    doc.name, dirty, doc.width, doc.height
                )
            } else {
                format!("{base} — {}", tr(lang, "workspace-editor"))
            }
        }
    }
}

pub fn theme(state: &PhotonGui, _window: window::Id) -> IcedTheme {
    match state.snapshot.meta.theme {
        Theme::Dark => IcedTheme::Dark,
        Theme::Light => IcedTheme::Light,
    }
}

pub fn subscription(_state: &PhotonGui) -> Subscription<Message> {
    let subs = vec![
        Subscription::run(state_watch_stream),
        Subscription::run(event_broadcast_stream),
        keyboard::listen().map(Message::KeyEvent),
        event::listen_with(|ev, _status, _window| match ev {
            iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(Message::WindowCursorMoved(position))
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                Some(Message::WindowMouseUp)
            }
            _ => None,
        }),
        window::close_events().map(Message::WindowClosed),
        window::resize_events().map(|(id, size)| Message::WindowResized(id, size)),
    ];
    Subscription::batch(subs)
}

fn state_watch_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(16, async |mut output| {
        use iced::futures::SinkExt;
        let Some(core) = CORE_HANDLE.get() else {
            return;
        };
        let mut rx = core.subscribe_state();
        while rx.changed().await.is_ok() {
            let snap = rx.borrow().clone();
            let _ = output.send(Message::StateUpdated(snap)).await;
        }
    })
}

fn event_broadcast_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(64, async |mut output| {
        use iced::futures::SinkExt;
        let Some(core) = CORE_HANDLE.get() else {
            return;
        };
        let mut rx = core.subscribe_events();
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let _ = output.send(Message::CoreEvent(ev)).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

pub fn update(state: &mut PhotonGui, message: Message) -> Task<Message> {
    match message {
        Message::Noop => Task::none(),
        Message::ToggleMenu(menu) => {
            state.context_menu = None;
            state.active_menu = if state.active_menu == Some(menu) {
                None
            } else {
                Some(menu)
            };
            Task::none()
        }
        Message::HoverMenu(menu) => {
            if state.active_menu.is_some() && state.active_menu != Some(menu) {
                state.active_menu = Some(menu);
            }
            Task::none()
        }
        Message::CloseMenu => {
            state.active_menu = None;
            Task::none()
        }
        Message::OpenShortcutsDialog => {
            state.active_menu = None;
            state.context_menu = None;
            let lang = state.snapshot.meta.language;
            state.dialog = Some(InAppDialog::Alert {
                id: DesktopDialogId::Shortcuts,
                title: tr(lang, "action-shortcuts"),
                message: "Ctrl+1/2/3: Switch Workspace\nCtrl+O / Ctrl+Shift+O: Open Folder / Files\nCtrl+Z / Ctrl+Y: Undo / Redo · Ctrl+S: Save · Ctrl+Shift+E: Export\nCtrl+R: Resize · F / 0: Fit · 1: 100% · F11: Fullscreen\nEditor Tools: V (Pointer), H (Hand), Z (Zoom), B (Brush), E (Eraser), L (Liquify), I (Eyedropper), C (Crop), X (Swap Colors), [ / ] (Brush Size)".to_owned(),
            });
            open_dialog_window(state, DesktopDialogId::Shortcuts);
            Task::none()
        }
        Message::OpenAboutDialog => {
            state.active_menu = None;
            state.context_menu = None;
            let lang = state.snapshot.meta.language;
            let backend = GPU_BACKEND_NAME.get().map(String::as_str).unwrap_or("wgpu");
            state.dialog = Some(InAppDialog::Alert {
                id: DesktopDialogId::About,
                title: tr(lang, "action-about"),
                message: format!(
                    "{} v{}\nUnified Browser · Viewer · Editor\nGPU Backend: {backend}",
                    tr(lang, "app-title"),
                    env!("CARGO_PKG_VERSION")
                ),
            });
            open_dialog_window(state, DesktopDialogId::About);
            Task::none()
        }
        Message::ViewerZoomActual => {
            state.context_menu = None;
            if state.viewer_mode == ViewerViewMode::Webtoon {
                state.viewer_webtoon_fit_mode = WebtoonFitMode::Original;
                state.webtoon_zoom = 1.0;
                return sync_ui_tree_task(state);
            }
            if let Some(frame) = &state.viewer_frame {
                let (base_w, base_h) = if state.viewer_mode == ViewerViewMode::Double
                    && let Some(pair) = &state.viewer_pair_frame
                {
                    let (sw, sh, _, _) = dual_spread_geometry(
                        (frame.width, frame.height),
                        (pair.width, pair.height),
                        f32::from(state.viewer_manga_page_gap),
                    );
                    (sw, sh)
                } else {
                    (frame.width as f32, frame.height as f32)
                };
                let odd = (state.viewer_quarter_turns % 2) == 1;
                let (iw, ih) = if odd {
                    (base_h, base_w)
                } else {
                    (base_w, base_h)
                };
                let vp_w = (state.window_size.width - 40.0).max(120.0);
                let vp_h = (state.window_size.height - 130.0).max(120.0);
                let fit_scale = (vp_w / iw.max(1.0)).min(vp_h / ih.max(1.0));
                state.zoom = (1.0 / fit_scale.max(0.05)).clamp(0.1, 16.0);
                state.pan_x = 0.0;
                state.pan_y = 0.0;
            }
            sync_ui_tree_task(state)
        }
        Message::ViewerRotatePreview(delta) => {
            state.context_menu = None;
            let next = (i16::from(state.viewer_quarter_turns) + i16::from(delta)).rem_euclid(4);
            state.viewer_quarter_turns = next as u8;
            sync_ui_tree_task(state)
        }
        Message::ToggleFullscreen => {
            state.context_menu = None;
            state.is_fullscreen = !state.is_fullscreen;
            if let Some(win_id) = state.main_window {
                let mode = if state.is_fullscreen {
                    window::Mode::Fullscreen
                } else {
                    window::Mode::Windowed
                };
                return Task::batch([window::set_mode(win_id, mode), sync_ui_tree_task(state)]);
            }
            sync_ui_tree_task(state)
        }
        Message::CloseFilmstrip => {
            state.context_menu = None;
            state.viewer_filmstrip_open = false;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::ToggleFilmstrip => {
            state.context_menu = None;
            state.viewer_filmstrip_open = !state.viewer_filmstrip_open;
            let thumb_task = if state.viewer_filmstrip_open {
                schedule_missing_thumbnails(state)
            } else {
                Task::none()
            };
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([thumb_task, layout_task, sync_ui_tree_task(state)])
        }
        Message::ToggleViewerInspector => {
            state.context_menu = None;
            state.viewer_info_open = !state.viewer_info_open;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::ToggleBrowserInspector => {
            state.active_menu = None;
            state.context_menu = None;
            let next = !(state.browser_preview_open || state.browser_info_open);
            state.browser_preview_open = next;
            state.browser_info_open = next;
            state.browser_inspector_open = next;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::BrowserOpenSelectedInEditor => {
            state.active_menu = None;
            state.context_menu = None;
            let selected = state.snapshot.meta.selection.iter().find(|path| {
                state
                    .snapshot
                    .entries
                    .iter()
                    .any(|e| &e.path == *path && e.is_image)
            });
            if let Some(path) = selected.cloned() {
                dispatch_to_core(state.core.clone(), Command::EditorOpen { path })
            } else {
                Task::none()
            }
        }
        Message::CopyViewerPath => {
            state.context_menu = None;
            if let Some(path) = state.snapshot.meta.viewer_path.clone() {
                let toast_task = show_toast(
                    state,
                    tr(state.snapshot.meta.language, "toast-path-copied"),
                    ToastKind::Success,
                );
                return Task::batch([iced::clipboard::write(path), toast_task]);
            }
            Task::none()
        }
        Message::OpenSettings => {
            state.active_menu = None;
            state.context_menu = None;
            state.settings_open = true;
            open_dialog_window(state, DesktopDialogId::Settings);
            Task::none()
        }
        Message::CloseSettings => {
            state.settings_open = false;
            state.dialog_manager.close(&DesktopDialogId::Settings);
            Task::none()
        }
        Message::LayoutCommandCompleted(res) => {
            state.pending_layout_commands = state.pending_layout_commands.saturating_sub(1);
            update(state, Message::CommandCompleted(res))
        }
        Message::KeyEvent(ev) => handle_key_event(state, ev),
        Message::WindowOpened(id) => {
            tracing::debug!(target: "photon::desktop", ?id, "window opened");
            let mut tasks = vec![sync_ui_tree_task(state)];
            if state.main_window == Some(id) && !state.is_fullscreen {
                let check_task = Task::batch([window::monitor_size(id).then(move |mon| {
                    window::is_maximized(id)
                        .map(move |maxed| Message::WindowMonitorSizeCheck(id, mon, maxed))
                })]);
                tasks.push(check_task);
            }
            Task::batch(tasks)
        }
        Message::WindowMonitorSizeCheck(id, Some(mon), is_maximized) => {
            if state.main_window == Some(id) && !state.is_fullscreen && !is_maximized {
                let max_w = (mon.width * 0.94).max(960.0).min(mon.width);
                let max_h = (mon.height * 0.90).max(540.0).min(mon.height);
                if state.window_size.width > max_w || state.window_size.height > max_h {
                    let clamped = Size::new(
                        state.window_size.width.min(max_w).max(960.0),
                        state.window_size.height.min(max_h).max(540.0),
                    );
                    state.window_size = clamped;
                    state.suppress_resize_until = Some(Instant::now() + Duration::from_millis(600));
                    return window::resize(id, clamped);
                }
            }
            Task::none()
        }
        Message::WindowMonitorSizeCheck(_, None, _) => Task::none(),
        Message::WindowClosed(id) => {
            state.windows.remove(&id);
            if state.main_window == Some(id) {
                state.main_window = state.windows.keys().next().copied();
            }
            if state.windows.is_empty() {
                return iced::exit();
            }
            sync_ui_tree_task(state)
        }
        Message::WindowResized(id, size) => {
            if state.main_window == Some(id) {
                state.window_size = size;
                if state
                    .suppress_resize_until
                    .is_some_and(|until| Instant::now() < until)
                {
                    return sync_ui_tree_task(state);
                }
                state.suppress_resize_until = None;
                let w = (size.width.round() as u32).clamp(960, 3840);
                let h = (size.height.round() as u32).clamp(540, 2160);
                let layout = &state.snapshot.meta.layout;
                if w != layout.window_width || h != layout.window_height {
                    state.resize_save_seq = state.resize_save_seq.wrapping_add(1);
                    let seq = state.resize_save_seq;
                    let save_task = Task::perform(
                        async move {
                            tokio::time::sleep(Duration::from_millis(400)).await;
                            Message::PersistWindowSize(seq, w, h)
                        },
                        |m| m,
                    );
                    return Task::batch([sync_ui_tree_task(state), save_task]);
                }
                return sync_ui_tree_task(state);
            }
            Task::none()
        }
        Message::PersistWindowSize(seq, w, h) => {
            if seq != state.resize_save_seq
                || state.is_fullscreen
                || state
                    .suppress_resize_until
                    .is_some_and(|until| Instant::now() < until)
            {
                return Task::none();
            }
            let Some(main_id) = state.main_window else {
                return Task::none();
            };
            window::is_maximized(main_id)
                .map(move |is_max| Message::PersistWindowSizeIfWindowed(seq, w, h, is_max))
        }
        Message::PersistWindowSizeIfWindowed(seq, w, h, is_maximized) => {
            if is_maximized || seq != state.resize_save_seq || state.is_fullscreen {
                return Task::none();
            }
            let layout = &state.snapshot.meta.layout;
            if layout.window_width == w && layout.window_height == h {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.window_width = w;
                layout.window_height = h;
            })
        }
        Message::StateUpdated(snap) => {
            let prev_ws = state.snapshot.meta.workspace;
            let viewer_changed = snap.meta.viewer_path != state.snapshot.meta.viewer_path;
            let root_changed = snap.meta.root_folder != state.snapshot.meta.root_folder;
            let tree_changed = snap.meta.tree_revision != state.snapshot.meta.tree_revision;
            let folder_changed = snap.meta.current_folder != state.snapshot.meta.current_folder;
            let listing_changed = snap.meta.listing_epoch != state.snapshot.meta.listing_epoch;
            let editor_changed = snap
                .meta
                .editor
                .as_ref()
                .map(|e| (e.path.clone(), e.pixel_revision))
                != state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|e| (e.path.clone(), e.pixel_revision));
            let editor_orig_changed = snap
                .meta
                .editor
                .as_ref()
                .map(|e| (e.path.clone(), e.original_revision))
                != state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|e| (e.path.clone(), e.original_revision));
            let editor_dims_changed = snap
                .meta
                .editor
                .as_ref()
                .map(|e| (e.path.clone(), e.width, e.height))
                != state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|e| (e.path.clone(), e.width, e.height));
            if let Some(folder) = &snap.meta.current_folder {
                state.folder_input = folder.clone();
            }
            if folder_changed {
                state.browser_filter.clear();
                state.last_click = None;
            }
            if let Some(ed) = &snap.meta.editor {
                if state.editor_ui.adjust_popup_tab.is_none() || editor_changed {
                    state.pending_adj = ed.adjustments;
                }
                if editor_dims_changed {
                    state.editor_ui.sync_document_dims(ed.width, ed.height);
                }
            }
            if state.pending_layout_commands == 0 && snap.meta.layout != state.last_core_layout {
                state.browser_preview_open = snap.meta.layout.panel("browser.preview")
                    || (snap.meta.layout.panel("browser.inspector")
                        && !snap.meta.layout.panel("browser.info"));
                state.browser_info_open = snap.meta.layout.panel("browser.info")
                    || (snap.meta.layout.panel("browser.inspector")
                        && !snap.meta.layout.panel("browser.preview"));
                state.browser_inspector_open =
                    state.browser_preview_open || state.browser_info_open;
                state.browser_quick_access_open = snap.meta.layout.panel("browser.quick_access");
                state.browser_explorer_open = snap.meta.layout.panel("browser.explorer");
                state.browser_left_panels.set_order(
                    snap.meta.layout.browser_left_panel_order.clone(),
                    &BrowserLeftPanel::ALL,
                );
                state.browser_right_panels.set_order(
                    snap.meta.layout.browser_right_panel_order.clone(),
                    &BrowserRightPanel::ALL,
                );
                state.viewer_info_open = snap.meta.layout.panel("viewer.info")
                    || snap.meta.layout.panel("viewer.inspector");
                state.viewer_adjustments_open = snap.meta.layout.panel("viewer.adjustments");
                state.viewer_color_popover_open = state.viewer_adjustments_open;
                state.viewer_right_panels.set_order(
                    snap.meta.layout.viewer_right_panel_order.clone(),
                    &ViewerRightPanel::ALL,
                );
                state.viewer_filmstrip_open = snap.meta.layout.panel("viewer.filmstrip");
                state.editor_tool_tab = snap.meta.layout.editor_tool_tab;
                state.editor_history_open = snap.meta.layout.panel("editor.history");
                state.editor_layers_open = snap.meta.layout.panel("editor.layers");
                state.editor_panels.set_order(
                    snap.meta.layout.editor_right_panel_order.clone(),
                    &EditorRightPanel::ALL,
                );
                sync_editor_panel_compat(state);
            }
            let incoming_core_layout = snap.meta.layout.clone();
            if snap.meta.workspace != prev_ws {
                state.active_menu = None;
                state.context_menu = None;
                state.crop_uv = None;
                state.editor_ui.renaming_layer_id = None;
                state.editor_ui.adjust_popup_tab = None;
                close_all_adjustment_dialogs(state);
                state.browser_left_panels.cancel_drag();
                state.browser_right_panels.cancel_drag();
                state.viewer_right_panels.cancel_drag();
                state.editor_panels.cancel_drag();
                sync_editor_panel_compat(state);
                state.viewer_color_popover_open = false;
                if snap.meta.workspace == Workspace::Viewer {
                    state.reset_viewer_presentation_from_layout(&snap.meta.layout);
                    state.viewer_adj = ColorAdjustments::default();
                } else {
                    state.viewer_quarter_turns = 0;
                }
                if snap.meta.workspace == Workspace::Editor {
                    state.zoom = 1.0;
                    state.pan_x = 0.0;
                    state.pan_y = 0.0;
                }
            }
            if snap.meta.editor.is_some()
                && let Some(adj) = state.transfer_adj_to_editor.take()
            {
                state.pending_adj = adj;
                let tab = AdjustmentPopupTab::ColorAndLight;
                state.editor_ui.adjust_popup_tab = Some(tab);
                close_all_adjustment_dialogs(state);
                open_dialog_window(state, adjustment_dialog_id(tab));
            }
            if editor_changed {
                state.crop_uv = None;
            }
            state.last_core_layout = incoming_core_layout;
            state.snapshot = snap;
            if state.pending_layout_commands > 0 {
                let mut merged = state.snapshot.meta.layout.clone();
                apply_live_layout_fields(state, &mut merged);
                Arc::make_mut(&mut state.snapshot).meta.layout = merged;
            }
            probe_viewer_image_sizes(state);

            let mut tasks = vec![sync_ui_tree_task(state)];
            if root_changed {
                state.expanded_folders.clear();
                state.folder_children.clear();
            }
            if root_changed || tree_changed || folder_changed {
                tasks.push(sync_expanded_tree_tasks(
                    state,
                    root_changed || tree_changed,
                ));
            }
            if listing_changed || folder_changed {
                state.webtoon_pages.clear();
                state.webtoon_loading.clear();
                state.webtoon_order.clear();
                state.webtoon_scroll_y = 0.0;
                state.webtoon_visible_index = None;
                tasks.push(schedule_missing_thumbnails(state));
            }
            if viewer_changed {
                state.zoom = 1.0;
                state.pan_x = 0.0;
                state.pan_y = 0.0;
                state.viewer_quarter_turns = 0;
                state.webtoon_visible_index = state.snapshot.meta.viewer_index;
                let vmode = state.viewer_mode;
                if vmode == ViewerViewMode::Double {
                    let (left_slot, right_slot) = dual_spread_slots(state);
                    let lp = left_slot.map(|(_, p)| p.to_owned());
                    let rp = right_slot.map(|(_, p)| p.to_owned());
                    if let Some(lp) = lp {
                        tasks.push(load_viewer_frame_task(lp));
                    } else {
                        state.viewer_frame = None;
                    }
                    if let Some(rp) = rp {
                        tasks.push(load_viewer_pair_frame_task(Some(rp)));
                    } else {
                        state.viewer_pair_frame = None;
                    }
                } else {
                    if let Some(path) = state.snapshot.meta.viewer_path.clone() {
                        tasks.push(load_viewer_frame_task(path));
                    } else {
                        state.viewer_frame = None;
                    }
                    state.viewer_pair_frame = None;
                }
                if vmode == ViewerViewMode::Webtoon {
                    let center = state.snapshot.meta.viewer_index.unwrap_or(0);
                    tasks.push(schedule_webtoon_pages_near(state, center));
                    if let Some(y) = webtoon_offset_for_index(state, center) {
                        tasks.push(widget_operation::scroll_to(
                            webtoon_scroll_id(),
                            scrollable::AbsoluteOffset {
                                x: None,
                                y: Some(y),
                            },
                        ));
                    }
                }
            } else if state.viewer_mode == ViewerViewMode::Double {
                let (left_slot, right_slot) = dual_spread_slots(state);
                let left_mismatch = match (left_slot, &state.viewer_frame) {
                    (Some((_, lp)), Some(f)) => f.key != format!("viewer:{lp}"),
                    (Some(_), None) | (None, Some(_)) => true,
                    (None, None) => false,
                };
                let right_mismatch = match (right_slot, &state.viewer_pair_frame) {
                    (Some((_, rp)), Some(f)) => f.key != format!("viewer:{rp}"),
                    (Some(_), None) | (None, Some(_)) => true,
                    (None, None) => false,
                };
                if left_mismatch || right_mismatch {
                    tasks.push(reload_dual_frames_task(state));
                }
            } else {
                state.viewer_pair_frame = None;
            }
            if state.snapshot.meta.workspace == Workspace::Viewer
                && snap_viewer_mode_needs_thumbs(
                    &state.snapshot,
                    state.viewer_mode,
                    state.viewer_filmstrip_open,
                )
            {
                tasks.push(schedule_missing_thumbnails(state));
            }
            if editor_changed {
                if state.snapshot.meta.editor.is_some() {
                    tasks.push(load_editor_frame_task(state.core.clone()));
                    tasks.push(load_editor_thumbnails_task(state.core.clone()));
                } else {
                    state.editor_frame = None;
                    state.editor_layer_thumbs.clear();
                    state.editor_mask_thumbs.clear();
                }
            }
            if editor_orig_changed
                || (state.snapshot.meta.editor.is_some() && state.editor_original_frame.is_none())
            {
                if state.snapshot.meta.editor.is_some() {
                    tasks.push(load_editor_original_frame_task(state.core.clone()));
                } else {
                    state.editor_original_frame = None;
                }
            }
            Task::batch(tasks)
        }
        Message::CoreEvent(ev) => match ev.kind {
            EventKind::ScreenshotRequested {
                request_id,
                output_path,
            } => {
                if let Some(win_id) = state.main_window {
                    let path = output_path.unwrap_or_else(|| "/tmp/photon-shot.png".to_owned());
                    state.pending_screenshots.insert(win_id, (request_id, path));
                    // Wait briefly so any pending StateUpdated relayout + RedrawRequested
                    // finishes populating iced_wgpu's layers before capturing.
                    return Task::perform(
                        tokio::time::sleep(std::time::Duration::from_millis(40)),
                        move |_| Message::TriggerWindowScreenshot(win_id),
                    );
                }
                Task::none()
            }
            EventKind::UiActionRequested { target_id, .. } => {
                handle_semantic_ui_action(state, &target_id)
            }
            _ => Task::none(),
        },
        Message::TriggerWindowScreenshot(win_id) => {
            window::screenshot(win_id).map(move |shot| Message::ScreenshotReady(win_id, shot))
        }
        Message::ScreenshotReady(win_id, shot) => {
            if let Some((request_id, out_path)) = state.pending_screenshots.remove(&win_id) {
                let core = state.core.clone();
                return Task::perform(
                    async move {
                        let w = shot.size.width;
                        let h = shot.size.height;
                        if let Some(parent) = Path::new(&out_path).parent()
                            && !parent.as_os_str().is_empty()
                        {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        if let Some(img) = RgbaImage::from_raw(w, h, shot.rgba.to_vec()) {
                            let _ = img.save(&out_path);
                            core.emit_event(EventKind::ScreenshotCaptured {
                                request_id,
                                path: out_path,
                                width: w,
                                height: h,
                            })
                            .await;
                        }
                    },
                    |_| Message::Noop,
                );
            }
            Task::none()
        }
        Message::Dispatch(cmd) => {
            state.active_menu = None;
            dispatch_to_core(state.core.clone(), cmd)
        }
        Message::CommandCompleted(Ok(resp)) => {
            let lang = state.snapshot.meta.language;
            if !resp.ok {
                let msg = resp
                    .ui
                    .as_ref()
                    .map(|u| tr_ui_text(lang, u))
                    .or_else(|| resp.error.as_ref().map(|e| e.message.clone()))
                    .unwrap_or_else(|| tr(lang, "dialog-error-title"));
                return show_toast(state, msg, ToastKind::Error);
            }
            if let Some(ui) = &resp.ui
                && matches!(
                    ui.code.as_str(),
                    "editor_saved"
                        | "editor_exported"
                        | "created_folder"
                        | "config_reset_done"
                        | "favorite_added"
                        | "favorite_removed"
                        | "trashed_items"
                        | "deleted_items"
                        | "copied_items"
                        | "moved_items"
                        | "renamed_item"
                        | "translate_done"
                        | "batch_done"
                )
            {
                return show_toast(state, tr_ui_text(lang, ui), ToastKind::Success);
            }
            Task::none()
        }
        Message::CommandCompleted(Err(err)) => {
            state.dialog = Some(InAppDialog::Alert {
                id: DesktopDialogId::Alert,
                title: tr(state.snapshot.meta.language, "dialog-error-title"),
                message: err,
            });
            open_dialog_window(state, DesktopDialogId::Alert);
            Task::none()
        }
        Message::PickFolderDialog => {
            state.active_menu = None;
            let title = tr(state.snapshot.meta.language, "action-open-folder");
            let start = state
                .snapshot
                .meta
                .current_folder
                .as_deref()
                .map(PathBuf::from);
            Task::perform(
                async move { photon_dialogs::pick_folder(&title, start).await },
                Message::FolderPicked,
            )
        }
        Message::FolderPicked(Some(path)) => dispatch_to_core(
            state.core.clone(),
            Command::OpenFolder {
                path: path.to_string_lossy().into_owned(),
            },
        ),
        Message::FolderPicked(None) => Task::none(),
        Message::PickFavoriteFolderDialog => {
            let title = tr(state.snapshot.meta.language, "action-pin-quick-access");
            let start = state
                .snapshot
                .meta
                .current_folder
                .as_deref()
                .map(PathBuf::from);
            Task::perform(
                async move { photon_dialogs::pick_folder(&title, start).await },
                Message::FavoriteFolderPicked,
            )
        }
        Message::FavoriteFolderPicked(Some(path)) => dispatch_to_core(
            state.core.clone(),
            Command::AddFavorite {
                path: path.to_string_lossy().into_owned(),
            },
        ),
        Message::FavoriteFolderPicked(None) => Task::none(),
        Message::PickImagesDialog => {
            state.active_menu = None;
            let title = tr(state.snapshot.meta.language, "action-open-file");
            let start = state
                .snapshot
                .meta
                .current_folder
                .as_deref()
                .map(PathBuf::from);
            Task::perform(
                async move { photon_dialogs::open_images(&title, start).await },
                Message::ImagesPicked,
            )
        }
        Message::ImagesPicked(paths) if !paths.is_empty() => dispatch_to_core(
            state.core.clone(),
            Command::OpenFiles {
                paths: paths
                    .into_iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect(),
            },
        ),
        Message::ImagesPicked(_) => Task::none(),
        Message::PickSaveAsDialog => {
            state.active_menu = None;
            let title = tr(state.snapshot.meta.language, "action-save-as");
            let (start, default_name) = match &state.snapshot.meta.editor {
                Some(doc) => {
                    let p = Path::new(&doc.save_path);
                    (p.parent().map(Path::to_path_buf), Some(doc.name.clone()))
                }
                None => (None, None),
            };
            Task::perform(
                async move {
                    photon_dialogs::save_image_as(&title, start, default_name.as_deref()).await
                },
                Message::SaveAsPicked,
            )
        }
        Message::SaveAsPicked(Some(path)) => dispatch_to_core(
            state.core.clone(),
            Command::EditorSaveAs {
                path: path.to_string_lossy().into_owned(),
            },
        ),
        Message::SaveAsPicked(None) => Task::none(),
        Message::OpenNewFolderDialog => {
            state.active_menu = None;
            let parent = state
                .snapshot
                .meta
                .current_folder
                .clone()
                .unwrap_or_else(|| {
                    photon_storage::paths::home_dir()
                        .to_string_lossy()
                        .into_owned()
                });
            state.dialog = Some(InAppDialog::NewFolder {
                parent,
                name: String::new(),
                error: None,
            });
            open_dialog_window(state, DesktopDialogId::NewFolder);
            Task::none()
        }
        Message::NewFolderNameChanged(val) => {
            if let Some(InAppDialog::NewFolder { name, error, .. }) = &mut state.dialog {
                *name = val;
                *error = None;
            }
            Task::none()
        }
        Message::ConfirmNewFolder => {
            let Some(InAppDialog::NewFolder { parent, name, .. }) = state.dialog.clone() else {
                return Task::none();
            };
            let clean = name.trim().to_owned();
            if clean.is_empty() || clean.contains('/') || clean.contains('\\') {
                if let Some(InAppDialog::NewFolder { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "dialog-error-title"));
                }
                return Task::none();
            }
            state.dialog = None;
            state.dialog_manager.close(&DesktopDialogId::NewFolder);
            dispatch_to_core(
                state.core.clone(),
                Command::NewFolder {
                    parent,
                    name: clean,
                },
            )
        }
        Message::RequestResetLayout => {
            state.dialog = Some(InAppDialog::ConfirmResetLayout);
            open_dialog_window(state, DesktopDialogId::ConfirmResetLayout);
            Task::none()
        }
        Message::ConfirmResetLayout => {
            state.dialog = None;
            state
                .dialog_manager
                .close(&DesktopDialogId::ConfirmResetLayout);
            dispatch_to_core(
                state.core.clone(),
                Command::ResetConfig {
                    reset_layout: true,
                    reset_settings: false,
                },
            )
        }
        Message::RequestEditorLeave { close_editor } => {
            state.active_menu = None;
            state.pending_leave_to_browser = false;
            if state
                .snapshot
                .meta
                .editor
                .as_ref()
                .is_some_and(|d| d.modified)
            {
                state.dialog = Some(InAppDialog::ConfirmUnsavedLeave { close_editor });
                open_dialog_window(state, DesktopDialogId::ConfirmUnsavedLeave);
                return Task::none();
            }
            let cmd = if close_editor {
                Command::EditorClose
            } else {
                Command::WorkspaceBack
            };
            dispatch_to_core(state.core.clone(), cmd)
        }
        Message::RequestEditorLeaveToBrowser => {
            state.active_menu = None;
            if state
                .snapshot
                .meta
                .editor
                .as_ref()
                .is_some_and(|d| d.modified)
            {
                state.pending_leave_to_browser = true;
                state.dialog = Some(InAppDialog::ConfirmUnsavedLeave {
                    close_editor: false,
                });
                open_dialog_window(state, DesktopDialogId::ConfirmUnsavedLeave);
                return Task::none();
            }
            state.pending_leave_to_browser = false;
            dispatch_to_core(
                state.core.clone(),
                Command::SetWorkspace {
                    workspace: Workspace::Browser,
                },
            )
        }
        Message::DiscardAndLeaveEditor(close_editor) => {
            let to_browser = std::mem::take(&mut state.pending_leave_to_browser);
            state.dialog = None;
            state
                .dialog_manager
                .close(&DesktopDialogId::ConfirmUnsavedLeave);
            let core = state.core.clone();
            Task::perform(
                async move {
                    if !close_editor {
                        let _ = core
                            .dispatch(Envelope::new(
                                format!("gui-{}", std::process::id()),
                                ClientKind::Gui,
                                Command::EditorClose,
                            ))
                            .await;
                    }
                    let cmd = if close_editor {
                        Command::EditorClose
                    } else if to_browser {
                        Command::SetWorkspace {
                            workspace: Workspace::Browser,
                        }
                    } else {
                        Command::WorkspaceBack
                    };
                    core.dispatch(Envelope::new(
                        format!("gui-{}", std::process::id()),
                        ClientKind::Gui,
                        cmd,
                    ))
                    .await
                },
                Message::CommandCompleted,
            )
        }
        Message::SaveAndLeaveEditor(close_editor) => {
            let to_browser = std::mem::take(&mut state.pending_leave_to_browser);
            state.dialog = None;
            state
                .dialog_manager
                .close(&DesktopDialogId::ConfirmUnsavedLeave);
            let core = state.core.clone();
            Task::perform(
                async move {
                    let save_resp = core
                        .dispatch(Envelope::new(
                            format!("gui-{}", std::process::id()),
                            ClientKind::Gui,
                            Command::EditorSave,
                        ))
                        .await;
                    match save_resp {
                        Ok(resp) if resp.ok => {
                            let leave_cmd = if close_editor {
                                Command::EditorClose
                            } else if to_browser {
                                Command::SetWorkspace {
                                    workspace: Workspace::Browser,
                                }
                            } else {
                                Command::WorkspaceBack
                            };
                            core.dispatch(Envelope::new(
                                format!("gui-{}", std::process::id()),
                                ClientKind::Gui,
                                leave_cmd,
                            ))
                            .await
                        }
                        other => other,
                    }
                },
                Message::CommandCompleted,
            )
        }
        Message::CloseDialog => {
            state.pending_leave_to_browser = false;
            if let Some(d) = state.dialog.take() {
                if let InAppDialog::ColorPicker {
                    target: ColorPickerTarget::ViewportBackground,
                    previous_viewport_bg,
                    ..
                } = &d
                {
                    state.viewport_background = *previous_viewport_bg;
                }
                state.dialog_manager.close(&dialog_id_for_in_app(&d));
            }
            Task::none()
        }
        Message::OpenResizeDialog => {
            state.active_menu = None;
            state.context_menu = None;
            if let Some(doc) = &state.snapshot.meta.editor {
                state.dialog = Some(InAppDialog::Resize {
                    dims: DimensionInputState::from_dims(doc.width, doc.height),
                    filter: ResizeFilter::Lanczos3,
                    mode: ResizeMode::Stretch,
                    fill: None,
                    error: None,
                });
                open_dialog_window(state, DesktopDialogId::Resize);
            }
            Task::none()
        }
        Message::ResizeDimAction(action) => {
            if let Some(InAppDialog::Resize { dims, error, .. }) = &mut state.dialog {
                dims.apply(action);
                *error = None;
            }
            Task::none()
        }
        Message::ResizeFilterChanged(f) => {
            if let Some(InAppDialog::Resize { filter, .. }) = &mut state.dialog {
                *filter = f;
            }
            Task::none()
        }
        Message::ResizeModeChanged(m) => {
            if let Some(InAppDialog::Resize { mode, .. }) = &mut state.dialog {
                *mode = m;
            }
            Task::none()
        }
        Message::ConfirmResize => {
            let Some(InAppDialog::Resize {
                dims,
                filter,
                mode,
                fill,
                ..
            }) = state.dialog.clone()
            else {
                return Task::none();
            };
            let w = dims.width_px;
            let h = dims.height_px;
            if (1..=32_768).contains(&w) && (1..=32_768).contains(&h) {
                state.dialog = None;
                state.dialog_manager.close(&DesktopDialogId::Resize);
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorResize {
                        width: w,
                        height: h,
                        filter,
                        mode,
                        fill,
                        fill_alpha: if fill.is_some() { 255 } else { 0 },
                    },
                )
            } else {
                if let Some(InAppDialog::Resize { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "dialog-error-title"));
                }
                Task::none()
            }
        }
        Message::ToggleCropMode => {
            state.active_menu = None;
            state.context_menu = None;
            if state.crop_uv.is_some() {
                state.crop_uv = None;
                if state.editor_ui.tool == EditorTool::Crop {
                    state.editor_ui.tool = EditorTool::Pointer;
                }
            } else if state.snapshot.meta.editor.is_some() {
                state.editor_ui.tool = EditorTool::Crop;
                state.crop_uv = Some([0.1, 0.1, 0.9, 0.9]);
            }
            sync_ui_tree_task(state)
        }
        Message::SetCropPreset(ratio) => {
            state.editor_ui.crop_aspect = ratio;
            if let Some(doc) = &state.snapshot.meta.editor {
                let iw = doc.width.max(1) as f32;
                let ih = doc.height.max(1) as f32;
                let uv = match ratio {
                    None => [0.1, 0.1, 0.9, 0.9],
                    Some(target_ratio) => {
                        let img_ratio = iw / ih;
                        if img_ratio > target_ratio {
                            let h = 0.8_f32;
                            let w = (h * target_ratio / img_ratio).min(0.95);
                            let dx = (1.0 - w) * 0.5;
                            [dx, 0.1, 1.0 - dx, 0.9]
                        } else {
                            let w = 0.8_f32;
                            let h = (w * img_ratio / target_ratio).min(0.95);
                            let dy = (1.0 - h) * 0.5;
                            [0.1, dy, 0.9, 1.0 - dy]
                        }
                    }
                };
                state.crop_uv = Some(uv);
            }
            Task::none()
        }
        Message::ApplyCrop => {
            let Some(uv) = state.crop_uv.take() else {
                return Task::none();
            };
            let Some(doc) = &state.snapshot.meta.editor else {
                return Task::none();
            };
            let (x, y, w, h) = crop_uv_to_signed(uv, doc.width, doc.height);
            if state.editor_ui.tool == EditorTool::Crop {
                state.editor_ui.tool = EditorTool::Pointer;
            }
            let fill = state.editor_ui.crop_fill;
            dispatch_to_core(
                state.core.clone(),
                Command::EditorCrop {
                    x,
                    y,
                    width: w,
                    height: h,
                    fill,
                    fill_alpha: if fill.is_some() { 255 } else { 0 },
                },
            )
        }
        Message::CancelCrop => {
            state.crop_uv = None;
            if state.editor_ui.tool == EditorTool::Crop {
                state.editor_ui.tool = EditorTool::Pointer;
            }
            sync_ui_tree_task(state)
        }
        Message::DismissToast(id) => {
            if id.is_none() || state.toast.as_ref().is_some_and(|t| Some(t.id) == id) {
                state.toast = None;
            }
            Task::none()
        }
        Message::TogglePathEdit(editing) => {
            state.active_menu = None;
            state.path_editing = editing;
            if !editing && let Some(folder) = &state.snapshot.meta.current_folder {
                state.folder_input = folder.clone();
            }
            Task::none()
        }
        Message::FolderInputChanged(val) => {
            state.folder_input = val;
            Task::none()
        }
        Message::SubmitFolderInput => {
            state.path_editing = false;
            let path = state.folder_input.trim().to_owned();
            if path.is_empty() {
                Task::none()
            } else {
                dispatch_to_core(state.core.clone(), Command::OpenFolder { path })
            }
        }
        Message::BrowserFilterChanged(val) => {
            state.browser_filter = val;
            Task::none()
        }
        Message::ToggleBrowserFilterBar => {
            state.browser_filter_bar_open = !state.browser_filter_bar_open;
            if state.browser_filter_bar_open {
                Task::batch([
                    widget_operation::focus(browser_filter_input_id()),
                    sync_ui_tree_task(state),
                ])
            } else {
                sync_ui_tree_task(state)
            }
        }
        Message::ToggleBrowserFilterCaseSensitive => {
            state.browser_filter_case_sensitive = !state.browser_filter_case_sensitive;
            Task::none()
        }
        Message::ToggleBrowserFilterRegex => {
            state.browser_filter_regex = !state.browser_filter_regex;
            Task::none()
        }
        Message::ClearBrowserFilter => {
            if !state.browser_filter.is_empty() {
                state.browser_filter.clear();
                widget_operation::focus(browser_filter_input_id())
            } else {
                state.browser_filter_bar_open = false;
                sync_ui_tree_task(state)
            }
        }
        Message::SetBrowserViewMode(mode) => {
            state.active_menu = None;
            if state.snapshot.meta.layout.browser_view_mode == mode {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.browser_view_mode = mode;
            })
        }
        Message::SetViewerViewMode(mode) => {
            // Close any open menus / context menus
            state.active_menu = None;
            state.context_menu = None;

            // If already in the requested mode, do nothing
            if state.viewer_mode == mode {
                return Task::none();
            }

            let prev_mode = state.viewer_mode;
            state.viewer_mode = mode;

            // Reset single-image view state whenever leaving/entering a mode
            state.zoom = 1.0;
            state.pan_x = 0.0;
            state.pan_y = 0.0;

            let mut tasks = vec![sync_ui_tree_task(state)];

            match mode {
                ViewerViewMode::Single => {
                    // Clear dual-spread pair
                    state.viewer_pair_frame = None;
                    // Reload current single image
                    if let Some(path) = state.snapshot.meta.viewer_path.clone() {
                        tasks.push(load_viewer_frame_task(path));
                    }
                }
                ViewerViewMode::Double => {
                    // Coming from Webtoon: clear webtoon state
                    if prev_mode == ViewerViewMode::Webtoon {
                        state.webtoon_visible_index = None;
                    }
                    // Load both spread slots
                    tasks.push(reload_dual_frames_task(state));
                }
                ViewerViewMode::Webtoon => {
                    // Clear dual-spread pair frame
                    state.viewer_pair_frame = None;
                    let center = state.snapshot.meta.viewer_index.unwrap_or(0);
                    state.webtoon_visible_index = Some(center);
                    tasks.push(schedule_webtoon_pages_near(state, center));
                    if let Some(y) = webtoon_offset_for_index(state, center) {
                        tasks.push(widget_operation::scroll_to(
                            webtoon_scroll_id(),
                            scrollable::AbsoluteOffset {
                                x: None,
                                y: Some(y),
                            },
                        ));
                    }
                }
            }
            Task::batch(tasks)
        }
        Message::SetDefaultViewerViewMode(mode) => {
            state.viewer_mode = mode;
            if state.snapshot.meta.layout.viewer_view_mode == mode {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.viewer_view_mode = mode;
            })
        }
        Message::DualStepSpread(delta) => {
            if let Some(path) = step_dual_spread(state, delta) {
                dispatch_to_core(state.core.clone(), Command::ViewerOpen { path })
            } else {
                Task::none()
            }
        }
        Message::SetMangaRtl(rtl) => {
            state.viewer_manga_rtl = rtl;
            if state.viewer_mode == ViewerViewMode::Double {
                let (left_slot, right_slot) = dual_spread_slots(state);
                let swapped = match (
                    left_slot,
                    right_slot,
                    &state.viewer_frame,
                    &state.viewer_pair_frame,
                ) {
                    (Some((_, lp)), Some((_, rp)), Some(lf), Some(rf))
                        if lf.key == format!("viewer:{rp}") && rf.key == format!("viewer:{lp}") =>
                    {
                        let old_l = state.viewer_frame.take();
                        let old_r = state.viewer_pair_frame.take();
                        state.viewer_frame = old_r;
                        state.viewer_pair_frame = old_l;
                        true
                    }
                    _ => false,
                };
                if swapped {
                    Task::none()
                } else {
                    reload_dual_frames_task(state)
                }
            } else {
                Task::none()
            }
        }
        Message::SetMangaSeparateCover(sep) => {
            state.viewer_manga_cover_separate = sep;
            if state.viewer_mode == ViewerViewMode::Double {
                reload_dual_frames_task(state)
            } else {
                Task::none()
            }
        }
        Message::SetMangaPageGap(gap) => {
            state.viewer_manga_page_gap = gap.min(64);
            Task::none()
        }
        Message::SetWebtoonFitMode(fit) => {
            state.webtoon_zoom = 1.0;
            state.viewer_webtoon_fit_mode = fit;
            Task::none()
        }
        Message::SetSystemOpenWindowMode(mode) => commit_layout_with(state, |layout| {
            layout.system_open_window_mode = mode;
        }),
        Message::WebtoonScrolled(vp) => {
            let y = vp.absolute_offset().y;
            let vh = vp.bounds().height;
            state.webtoon_scroll_y = y;
            if let Some(idx) = webtoon_index_at_scroll(state, y + vh * 0.45) {
                let changed = state.webtoon_visible_index != Some(idx);
                state.webtoon_visible_index = Some(idx);
                if changed {
                    return schedule_webtoon_pages_near(state, idx);
                }
            }
            Task::none()
        }
        Message::WebtoonPagesLoaded(items) => {
            for (path, w, h, rgba) in items {
                state.webtoon_loading.remove(&path);
                if !state.webtoon_pages.contains_key(&path) {
                    state.webtoon_order.push_back(path.clone());
                }
                state
                    .webtoon_pages
                    .insert(path, iced_image::Handle::from_rgba(w, h, rgba));
                while state.webtoon_order.len() > 48 {
                    if let Some(old) = state.webtoon_order.pop_front() {
                        state.webtoon_pages.remove(&old);
                    }
                }
            }
            Task::none()
        }
        Message::BrowserEntryActivated {
            path,
            is_dir,
            is_image,
        } => {
            let now = Instant::now();
            let double_click = state.last_click.as_ref().is_some_and(|(prev, ts)| {
                prev == &path && now.duration_since(*ts) <= Duration::from_millis(450)
            });
            if double_click {
                state.last_click = None;
                if is_dir {
                    return dispatch_to_core(state.core.clone(), Command::Navigate { path });
                }
                if is_image {
                    return dispatch_to_core(state.core.clone(), Command::ViewerOpen { path });
                }
                return Task::none();
            }
            state.last_click = Some((path.clone(), now));
            dispatch_to_core(state.core.clone(), Command::Select { paths: vec![path] })
        }
        Message::ThumbnailsLoaded(items) => {
            for (path, w, h, rgba) in items {
                state.thumbnails_loading.remove(&path);
                state
                    .thumbnails
                    .insert(path, iced_image::Handle::from_rgba(w, h, rgba));
            }
            Task::none()
        }
        Message::ToggleFolderExpand(path) => {
            if state.expanded_folders.contains(&path) {
                state.expanded_folders.remove(&path);
                Task::none()
            } else {
                state.expanded_folders.insert(path.clone());
                load_folder_children_task(state.core.clone(), path)
            }
        }
        Message::FolderChildrenLoaded(path, children) => {
            state.folder_children.insert(path, children);
            Task::none()
        }
        Message::ZoomStep(factor) => {
            state.context_menu = None;
            if state.snapshot.meta.workspace == Workspace::Viewer
                && state.viewer_mode == ViewerViewMode::Webtoon
            {
                state.webtoon_zoom = (state.webtoon_zoom * factor).clamp(0.4, 3.0);
                return Task::none();
            }
            state.zoom = (state.zoom * factor).clamp(0.1, 16.0);
            sync_ui_tree_task(state)
        }
        Message::Canvas(ev) => {
            match ev {
                CanvasEvent::ContextMenuRequested { pos: [x, y], .. } => {
                    if state.snapshot.meta.workspace == Workspace::Editor
                        && state.snapshot.meta.editor.is_some()
                    {
                        state.active_menu = None;
                        state.context_menu = Some(ActiveContextMenu::EditorCanvas { x, y });
                        return sync_ui_tree_task(state);
                    }
                    return Task::none();
                }
                CanvasEvent::ZoomBy {
                    factor,
                    cursor,
                    bounds,
                } => {
                    let old_zoom = state.zoom;
                    let new_zoom = (old_zoom * factor).clamp(0.1, 16.0);
                    let ratio = new_zoom / old_zoom.max(0.05);
                    if let Some([cx, cy]) = cursor {
                        let half_w = bounds[0] * 0.5;
                        let half_h = bounds[1] * 0.5;
                        state.pan_x = (cx - half_w) - ((cx - half_w) - state.pan_x) * ratio;
                        state.pan_y = (cy - half_h) - ((cy - half_h) - state.pan_y) * ratio;
                    }
                    state.zoom = new_zoom;
                }
                CanvasEvent::PanBy { dx, dy } => {
                    state.pan_x += dx;
                    state.pan_y += dy;
                }
                CanvasEvent::CropUpdated(uv) => {
                    state.crop_uv = Some(uv);
                    return Task::none();
                }
                CanvasEvent::StrokeBegin([x, y]) => {
                    state.editor_ui.stroke_active = true;
                    let stroke = if state.editor_ui.tool == EditorTool::Liquify {
                        LiveStrokeKind::Liquify {
                            mode: state.editor_ui.liquify_mode,
                            radius: state.editor_ui.liquify_radius,
                            strength: state.editor_ui.liquify_strength,
                        }
                    } else {
                        LiveStrokeKind::Paint {
                            color: state.editor_ui.fg_color,
                            radius: state.editor_ui.brush_radius,
                            opacity: state.editor_ui.brush_opacity,
                            erase: state.editor_ui.tool == EditorTool::Eraser,
                            brush: state.editor_ui.brush_settings,
                        }
                    };
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorLiveStrokeBegin { stroke, x, y },
                    );
                }
                CanvasEvent::StrokeStep(points) => {
                    if state.editor_ui.stroke_active {
                        return dispatch_to_core(
                            state.core.clone(),
                            Command::EditorLiveStrokeStep { points },
                        );
                    }
                    return Task::none();
                }
                CanvasEvent::StrokeEnd(commit) => {
                    if state.editor_ui.stroke_active {
                        state.editor_ui.stroke_active = false;
                        return dispatch_to_core(
                            state.core.clone(),
                            Command::EditorLiveStrokeEnd { commit },
                        );
                    }
                    return Task::none();
                }
                CanvasEvent::SampleColor([px, py]) => {
                    if let Some(frame) = &state.editor_frame
                        && px < frame.width
                        && py < frame.height
                    {
                        let idx = ((py as usize) * (frame.width as usize) + (px as usize)) * 4;
                        if idx + 3 < frame.rgba.len() {
                            let rgba = [
                                frame.rgba[idx],
                                frame.rgba[idx + 1],
                                frame.rgba[idx + 2],
                                frame.rgba[idx + 3],
                            ];
                            state.editor_ui.sampled_color = Some(rgba);
                            state.editor_ui.fg_color = [rgba[0], rgba[1], rgba[2]];
                        }
                    }
                    return Task::none();
                }
                CanvasEvent::CompareSplitChanged(frac) => {
                    state.editor_ui.compare_split_x = frac.clamp(0.02, 0.98);
                    return Task::none();
                }
                CanvasEvent::ResetFit => {
                    state.context_menu = None;
                    if state.snapshot.meta.workspace == Workspace::Viewer
                        && state.viewer_mode == ViewerViewMode::Webtoon
                    {
                        state.viewer_webtoon_fit_mode = WebtoonFitMode::Auto;
                        state.webtoon_zoom = 1.0;
                        return sync_ui_tree_task(state);
                    }
                    state.zoom = 1.0;
                    state.pan_x = 0.0;
                    state.pan_y = 0.0;
                }
            }
            sync_ui_tree_task(state)
        }
        Message::SetBrightness(v) => {
            state.pending_adj.brightness = v;
            Task::none()
        }
        Message::SetContrast(v) => {
            state.pending_adj.contrast = v;
            Task::none()
        }
        Message::SetSaturation(v) => {
            state.pending_adj.saturation = v;
            Task::none()
        }
        Message::SetExposure(v) => {
            state.pending_adj.exposure = v;
            Task::none()
        }
        Message::SetGamma(v) => {
            state.pending_adj.gamma = v.clamp(0.2, 3.0);
            Task::none()
        }
        Message::SetHighlights(v) => {
            state.pending_adj.highlights = v;
            Task::none()
        }
        Message::SetShadows(v) => {
            state.pending_adj.shadows = v;
            Task::none()
        }
        Message::ApplyColorAdjustments => dispatch_to_core(
            state.core.clone(),
            Command::EditorAdjustColor {
                brightness: state.pending_adj.brightness,
                contrast: state.pending_adj.contrast,
                saturation: state.pending_adj.saturation,
                exposure: state.pending_adj.exposure,
                highlights: state.pending_adj.highlights,
                shadows: state.pending_adj.shadows,
                gamma: state.pending_adj.gamma,
                target: state.editor_ui.apply_target,
            },
        ),
        Message::EditorResetPendingAdjustments => {
            state.pending_adj = ColorAdjustments::default();
            state.editor_ui.reset_adjustments();
            Task::none()
        }
        Message::EditorOpenAdjustPopup(tab) => {
            state.active_menu = None;
            state.context_menu = None;
            if let Some(ed) = &state.snapshot.meta.editor {
                if let Some(active) = ed.layers.iter().find(|l| l.id == ed.active_layer_id)
                    && let Some(effect) = &active.effect
                {
                    match effect {
                        LayerEffect::ColorLight(adj) => state.pending_adj = *adj,
                        LayerEffect::Hsv(hsv) => state.editor_ui.hsv = *hsv,
                        LayerEffect::ColorBalance(cb) => state.editor_ui.balance = *cb,
                        LayerEffect::Curves(c) => state.editor_ui.curves = c.clone(),
                        _ => {}
                    }
                }
                close_all_adjustment_dialogs(state);
                state.editor_ui.adjust_popup_tab = Some(tab);
                open_dialog_window(state, adjustment_dialog_id(tab));
                return sync_ui_tree_task(state);
            }
            Task::none()
        }
        Message::EditorCloseAdjustPopup { apply } => {
            let tab = state.editor_ui.adjust_popup_tab.take();
            close_all_adjustment_dialogs(state);
            if !apply {
                state.pending_adj = state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|e| e.adjustments)
                    .unwrap_or_default();
                state.editor_ui.reset_adjustments();
                return sync_ui_tree_task(state);
            }
            let matching_effect_update = state.snapshot.meta.editor.as_ref().and_then(|ed| {
                let active = ed.layers.iter().find(|l| l.id == ed.active_layer_id)?;
                let eff = active.effect.as_ref()?;
                let next = match (tab, eff) {
                    (Some(AdjustmentPopupTab::ColorAndLight), LayerEffect::ColorLight(_)) => {
                        LayerEffect::ColorLight(state.pending_adj)
                    }
                    (Some(AdjustmentPopupTab::Hsv), LayerEffect::Hsv(_)) => {
                        LayerEffect::Hsv(state.editor_ui.hsv)
                    }
                    (Some(AdjustmentPopupTab::ColorBalance), LayerEffect::ColorBalance(_)) => {
                        LayerEffect::ColorBalance(state.editor_ui.balance)
                    }
                    (Some(AdjustmentPopupTab::Curves), LayerEffect::Curves(_)) => {
                        LayerEffect::Curves(state.editor_ui.curves.clone())
                    }
                    _ => return None,
                };
                Some(Command::EditorLayerUpdateEffect {
                    id: active.id,
                    effect: next,
                })
            });
            let cmd_task = if let Some(cmd) = matching_effect_update {
                dispatch_to_core(state.core.clone(), cmd)
            } else {
                match tab {
                    Some(AdjustmentPopupTab::ColorAndLight) => {
                        update(state, Message::ApplyColorAdjustments)
                    }
                    Some(AdjustmentPopupTab::Hsv) => update(state, Message::EditorApplyHsv),
                    Some(AdjustmentPopupTab::ColorBalance) => {
                        update(state, Message::EditorApplyColorBalance)
                    }
                    Some(AdjustmentPopupTab::Curves) => update(state, Message::EditorApplyCurves),
                    None => Task::none(),
                }
            };
            state.pending_adj = ColorAdjustments::default();
            state.editor_ui.reset_adjustments();
            Task::batch([cmd_task, sync_ui_tree_task(state)])
        }
        Message::EditorSetLiquifyMode(m) => {
            state.editor_ui.liquify_mode = m;
            Task::none()
        }
        Message::EditorSetLiquifyRadius(r) => {
            state.editor_ui.liquify_radius = r.clamp(5.0, 150.0);
            Task::none()
        }
        Message::EditorSetLiquifyStrength(s) => {
            state.editor_ui.liquify_strength = s.clamp(0.05, 1.0);
            Task::none()
        }
        Message::EditorSelectTool(tool) => {
            state.active_menu = None;
            state.context_menu = None;
            state.editor_ui.tool = tool;
            if tool == EditorTool::Crop {
                if state.crop_uv.is_none() && state.snapshot.meta.editor.is_some() {
                    state.crop_uv = Some([0.1, 0.1, 0.9, 0.9]);
                }
            } else {
                state.crop_uv = None;
            }
            if tool == EditorTool::CanvasSize
                && let Some(ed) = &state.snapshot.meta.editor
            {
                state.editor_ui.sync_document_dims(ed.width, ed.height);
            }
            sync_ui_tree_task(state)
        }
        Message::EditorCycleCompareMode => {
            state.context_menu = None;
            state.editor_ui.compare_mode = match state.editor_ui.compare_mode {
                CompareMode::Off => CompareMode::Split,
                CompareMode::Split => CompareMode::SideBySide,
                CompareMode::SideBySide => CompareMode::HoldOriginal,
                CompareMode::HoldOriginal => CompareMode::Off,
            };
            sync_ui_tree_task(state)
        }
        Message::EditorSetCompareMode(mode) => {
            state.active_menu = None;
            state.context_menu = None;
            state.editor_ui.compare_mode = if state.editor_ui.compare_mode == mode {
                CompareMode::Off
            } else {
                mode
            };
            sync_ui_tree_task(state)
        }
        Message::EditorSwapColors => {
            let old_fg = state.editor_ui.fg_color;
            if let Some(bg) = state.editor_ui.bg_color {
                state.editor_ui.fg_color = bg;
            }
            state.editor_ui.bg_color = Some(old_fg);
            Task::none()
        }
        Message::EditorResetColors => {
            state.editor_ui.fg_color = [0, 0, 0];
            state.editor_ui.bg_color = Some([255, 255, 255]);
            Task::none()
        }
        Message::OpenColorPicker(target) => {
            let dark = state.snapshot.meta.theme == Theme::Dark;
            let (rgb, transparent) = match target {
                ColorPickerTarget::Foreground => (state.editor_ui.fg_color, false),
                ColorPickerTarget::Background => match state.editor_ui.bg_color {
                    Some(c) => (c, false),
                    None => ([255, 255, 255], true),
                },
                ColorPickerTarget::CanvasBackground => match state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .and_then(|e| e.canvas_background)
                {
                    Some(c) => (c, false),
                    None => ([255, 255, 255], true),
                },
                ColorPickerTarget::CropFill => match state.editor_ui.crop_fill {
                    Some(c) => (c, false),
                    None => ([255, 255, 255], true),
                },
                ColorPickerTarget::CanvasSizeFill => match state.editor_ui.canvas_fill {
                    Some(c) => (c, false),
                    None => ([255, 255, 255], true),
                },
                ColorPickerTarget::RotateFill => match state.editor_ui.rotate_fill {
                    Some(c) => (c, false),
                    None => ([255, 255, 255], true),
                },
                ColorPickerTarget::ViewportBackground => {
                    let c = state
                        .viewport_background
                        .unwrap_or_else(|| default_viewport_bg_rgb(dark));
                    (c, false)
                }
            };
            state.dialog = Some(InAppDialog::ColorPicker {
                target,
                r: rgb[0],
                g: rgb[1],
                b: rgb[2],
                hex: format_hex(rgb),
                transparent,
                previous_viewport_bg: state.viewport_background,
            });
            open_dialog_window(state, DesktopDialogId::ColorPicker);
            Task::none()
        }
        Message::ColorPickerSetRgb(nr, ng, nb) => {
            if let Some(InAppDialog::ColorPicker {
                target,
                r,
                g,
                b,
                hex,
                transparent,
                ..
            }) = &mut state.dialog
            {
                *r = nr;
                *g = ng;
                *b = nb;
                *hex = format_hex([nr, ng, nb]);
                *transparent = false;
                if *target == ColorPickerTarget::ViewportBackground {
                    state.viewport_background = Some([nr, ng, nb]);
                }
            }
            Task::none()
        }
        Message::ColorPickerHexChanged(val) => {
            if let Some(InAppDialog::ColorPicker {
                target,
                r,
                g,
                b,
                hex,
                transparent,
                ..
            }) = &mut state.dialog
            {
                *hex = val.clone();
                if let Some([nr, ng, nb]) = parse_hex_rgb(&val) {
                    *r = nr;
                    *g = ng;
                    *b = nb;
                    *transparent = false;
                    if *target == ColorPickerTarget::ViewportBackground {
                        state.viewport_background = Some([nr, ng, nb]);
                    }
                }
            }
            Task::none()
        }
        Message::ColorPickerToggleTransparent(t) => {
            if let Some(InAppDialog::ColorPicker {
                target,
                transparent,
                ..
            }) = &mut state.dialog
                && *target != ColorPickerTarget::ViewportBackground
                && *target != ColorPickerTarget::Foreground
            {
                *transparent = t;
            }
            Task::none()
        }
        Message::ResetViewportBackground => {
            let dark = state.snapshot.meta.theme == Theme::Dark;
            let def = default_viewport_bg_rgb(dark);
            state.viewport_background = None;
            if let Some(InAppDialog::ColorPicker {
                target: ColorPickerTarget::ViewportBackground,
                r,
                g,
                b,
                hex,
                transparent,
                ..
            }) = &mut state.dialog
            {
                *r = def[0];
                *g = def[1];
                *b = def[2];
                *hex = format_hex(def);
                *transparent = false;
            }
            Task::none()
        }
        Message::ConfirmColorPicker => {
            let Some(InAppDialog::ColorPicker {
                target,
                r,
                g,
                b,
                transparent,
                ..
            }) = state.dialog.take()
            else {
                return Task::none();
            };
            state.dialog_manager.close(&DesktopDialogId::ColorPicker);
            let picked = if transparent { None } else { Some([r, g, b]) };
            match target {
                ColorPickerTarget::Foreground => {
                    state.editor_ui.fg_color = [r, g, b];
                    Task::none()
                }
                ColorPickerTarget::Background => {
                    state.editor_ui.bg_color = picked;
                    Task::none()
                }
                ColorPickerTarget::CropFill => {
                    state.editor_ui.crop_fill = picked;
                    Task::none()
                }
                ColorPickerTarget::CanvasSizeFill => {
                    state.editor_ui.canvas_fill = picked;
                    Task::none()
                }
                ColorPickerTarget::RotateFill => {
                    state.editor_ui.rotate_fill = picked;
                    Task::none()
                }
                ColorPickerTarget::ViewportBackground => {
                    let dark = state.snapshot.meta.theme == Theme::Dark;
                    if state.viewport_background.is_none()
                        && [r, g, b] == default_viewport_bg_rgb(dark)
                    {
                        state.viewport_background = None;
                    } else {
                        state.viewport_background = Some([r, g, b]);
                    }
                    Task::none()
                }
                ColorPickerTarget::CanvasBackground => dispatch_to_core(
                    state.core.clone(),
                    Command::EditorSetCanvasBackground {
                        color: picked,
                        transparent,
                    },
                ),
            }
        }
        Message::EditorSetBrushRadius(v) => {
            state.editor_ui.brush_radius = v.clamp(1.0, 200.0);
            Task::none()
        }
        Message::EditorSetBrushOpacity(v) => {
            state.editor_ui.brush_opacity = v.clamp(0.01, 1.0);
            Task::none()
        }
        Message::EditorSetBrushHardness(v) => {
            state.editor_ui.brush_settings.hardness = v.clamp(0.0, 1.0);
            Task::none()
        }
        Message::EditorSetBrushFlow(v) => {
            state.editor_ui.brush_settings.flow = v.clamp(0.05, 1.0);
            Task::none()
        }
        Message::EditorSetBrushSpacing(v) => {
            state.editor_ui.brush_settings.spacing = v.clamp(0.05, 2.0);
            Task::none()
        }
        Message::EditorSetBrushTip(tip) => {
            state.editor_ui.brush_settings.tip = tip;
            Task::none()
        }
        Message::EditorSetApplyTarget(target) => {
            state.editor_ui.apply_target = target;
            Task::none()
        }
        Message::EditorSetHsvHue(v) => {
            state.editor_ui.hsv.hue = v;
            Task::none()
        }
        Message::EditorSetHsvSat(v) => {
            state.editor_ui.hsv.saturation = v;
            Task::none()
        }
        Message::EditorSetHsvVal(v) => {
            state.editor_ui.hsv.value = v;
            Task::none()
        }
        Message::EditorApplyHsv => {
            let hsv = state.editor_ui.hsv;
            let target = state.editor_ui.apply_target;
            dispatch_to_core(
                state.core.clone(),
                Command::EditorAdjustHsv {
                    hue: hsv.hue,
                    saturation: hsv.saturation,
                    value: hsv.value,
                    target,
                },
            )
        }
        Message::EditorSetBalanceZone(zone) => {
            state.editor_ui.balance_zone = zone;
            Task::none()
        }
        Message::EditorSetBalanceAxis(axis, val) => {
            if axis < 3 {
                match state.editor_ui.balance_zone {
                    ColorBalanceZone::Shadows => state.editor_ui.balance.shadows[axis] = val,
                    ColorBalanceZone::Midtones => state.editor_ui.balance.midtones[axis] = val,
                    ColorBalanceZone::Highlights => state.editor_ui.balance.highlights[axis] = val,
                }
            }
            Task::none()
        }
        Message::EditorTogglePreserveLuminosity(keep) => {
            state.editor_ui.balance.preserve_luminosity = keep;
            Task::none()
        }
        Message::EditorApplyColorBalance => {
            let b = state.editor_ui.balance;
            dispatch_to_core(
                state.core.clone(),
                Command::EditorColorBalance {
                    shadows: b.shadows,
                    midtones: b.midtones,
                    highlights: b.highlights,
                    preserve_luminosity: b.preserve_luminosity,
                    target: state.editor_ui.apply_target,
                },
            )
        }
        Message::EditorSelectCurvesChannel(ch) => {
            state.editor_ui.curves_channel = ch;
            Task::none()
        }
        Message::EditorCurvesChanged(pts) => {
            match state.editor_ui.curves_channel {
                CurvesChannel::Rgb => state.editor_ui.curves.rgb = pts,
                CurvesChannel::Red => state.editor_ui.curves.red = pts,
                CurvesChannel::Green => state.editor_ui.curves.green = pts,
                CurvesChannel::Blue => state.editor_ui.curves.blue = pts,
            }
            Task::none()
        }
        Message::EditorApplyCurves => {
            let c = state.editor_ui.curves.clone();
            state.editor_ui.curves = default_curves();
            dispatch_to_core(
                state.core.clone(),
                Command::EditorAdjustCurves {
                    rgb: c.rgb,
                    red: c.red,
                    green: c.green,
                    blue: c.blue,
                    target: state.editor_ui.apply_target,
                },
            )
        }
        Message::EditorSetRotateAngle(deg) => {
            state.editor_ui.rotate_angle = deg.clamp(-180.0, 180.0);
            Task::none()
        }
        Message::EditorApplyRotateAngle => {
            let degrees = state.editor_ui.rotate_angle;
            let fill = state.editor_ui.rotate_fill;
            state.editor_ui.rotate_angle = 0.0;
            state.editor_ui.tool = EditorTool::Pointer;
            dispatch_to_core(
                state.core.clone(),
                Command::EditorRotateAngle {
                    degrees,
                    fill,
                    fill_alpha: if fill.is_some() { 255 } else { 0 },
                    target: state.editor_ui.apply_target,
                },
            )
        }
        Message::EditorCanvasDimAction(action) => {
            state.editor_ui.canvas_dims.apply(action);
            Task::none()
        }
        Message::EditorSetCanvasAnchor(anchor) => {
            state.editor_ui.canvas_anchor = anchor;
            Task::none()
        }
        Message::EditorApplyCanvasSize => {
            let w = state.editor_ui.canvas_dims.width_px;
            let h = state.editor_ui.canvas_dims.height_px;
            if !(1..=32_768).contains(&w) || !(1..=32_768).contains(&h) {
                return show_toast(
                    state,
                    tr(state.snapshot.meta.language, "dialog-error-title"),
                    ToastKind::Error,
                );
            }
            let fill = state.editor_ui.canvas_fill;
            dispatch_to_core(
                state.core.clone(),
                Command::EditorCanvasSize {
                    width: w,
                    height: h,
                    anchor: state.editor_ui.canvas_anchor,
                    fill,
                    fill_alpha: if fill.is_some() { 255 } else { 0 },
                },
            )
        }
        Message::OpenExportDialog => {
            state.active_menu = None;
            if let Some(doc) = &state.snapshot.meta.editor {
                let p = Path::new(&doc.path);
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
                let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("jpg");
                let default_out = p
                    .parent()
                    .map(|dir| dir.join(format!("{stem}-export.{ext}")))
                    .unwrap_or_else(|| PathBuf::from(format!("{stem}-export.{ext}")));
                state.dialog = Some(InAppDialog::Export {
                    path: default_out.to_string_lossy().into_owned(),
                    quality: 92,
                    max_edge: String::new(),
                    metadata: true,
                    error: None,
                });
                open_dialog_window(state, DesktopDialogId::Export);
            }
            Task::none()
        }
        Message::PickExportDialog => {
            let title = tr(state.snapshot.meta.language, "action-export");
            let current = match &state.dialog {
                Some(InAppDialog::Export { path, .. }) => PathBuf::from(path),
                _ => PathBuf::new(),
            };
            let start = current.parent().map(Path::to_path_buf);
            let name = current
                .file_name()
                .and_then(|n| n.to_str())
                .map(ToOwned::to_owned);
            Task::perform(
                async move { photon_dialogs::save_image_as(&title, start, name.as_deref()).await },
                Message::ExportPathPicked,
            )
        }
        Message::ExportPathPicked(Some(picked)) => {
            if let Some(InAppDialog::Export { path, error, .. }) = &mut state.dialog {
                *path = picked.to_string_lossy().into_owned();
                *error = None;
            }
            Task::none()
        }
        Message::ExportPathPicked(None) => Task::none(),
        Message::ExportPathChanged(val) => {
            if let Some(InAppDialog::Export { path, error, .. }) = &mut state.dialog {
                *path = val;
                *error = None;
            }
            Task::none()
        }
        Message::ExportQualityChanged(q) => {
            if let Some(InAppDialog::Export { quality, .. }) = &mut state.dialog {
                *quality = q.clamp(10, 100);
            }
            Task::none()
        }
        Message::ExportMaxEdgeChanged(val) => {
            if let Some(InAppDialog::Export {
                max_edge, error, ..
            }) = &mut state.dialog
            {
                *max_edge = val;
                *error = None;
            }
            Task::none()
        }
        Message::ExportMetadataToggled(keep) => {
            if let Some(InAppDialog::Export { metadata, .. }) = &mut state.dialog {
                *metadata = keep;
            }
            Task::none()
        }
        Message::ConfirmExport => {
            let Some(InAppDialog::Export {
                path,
                quality,
                max_edge,
                metadata,
                ..
            }) = state.dialog.clone()
            else {
                return Task::none();
            };
            let clean_path = path.trim().to_owned();
            if clean_path.is_empty() {
                if let Some(InAppDialog::Export { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "dialog-error-title"));
                }
                return Task::none();
            }
            let parsed_edge = if max_edge.trim().is_empty() {
                None
            } else if let Ok(v) = max_edge.trim().parse::<u32>()
                && v > 0
            {
                Some(v)
            } else {
                if let Some(InAppDialog::Export { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "dialog-error-title"));
                }
                return Task::none();
            };
            state.dialog = None;
            state.dialog_manager.close(&DesktopDialogId::Export);
            dispatch_to_core(
                state.core.clone(),
                Command::EditorExport {
                    path: clean_path,
                    quality: Some(quality),
                    metadata,
                    max_edge: parsed_edge,
                },
            )
        }
        Message::EditorStartLayerRename(id, current_name) => {
            state.context_menu = None;
            state.editor_ui.renaming_layer_id = Some(id);
            state.editor_ui.layer_rename_input = current_name;
            Task::none()
        }
        Message::EditorLayerRenameChanged(val) => {
            state.editor_ui.layer_rename_input = val;
            Task::none()
        }
        Message::EditorCommitLayerRename => {
            let Some(id) = state.editor_ui.renaming_layer_id.take() else {
                return Task::none();
            };
            let name = state.editor_ui.layer_rename_input.trim().to_owned();
            if name.is_empty() {
                return Task::none();
            }
            dispatch_to_core(
                state.core.clone(),
                Command::EditorLayerSetProps {
                    id,
                    name: Some(name),
                    visible: None,
                    locked: None,
                    opacity: None,
                    blend_mode: None,
                    offset_x: None,
                    offset_y: None,
                },
            )
        }
        Message::EditorCancelLayerRename => {
            state.editor_ui.renaming_layer_id = None;
            state.editor_ui.layer_rename_input.clear();
            Task::none()
        }
        Message::BeginDragBrowserPanel(panel) => {
            state.browser_left_panels.begin_drag(panel);
            Task::none()
        }
        Message::HoverBrowserPanelDrop(panel) => {
            state.browser_left_panels.hover_drop(panel);
            Task::none()
        }
        Message::DropBrowserPanel(target) => {
            if state.browser_left_panels.commit_drop(Some(target)) {
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::SwapBrowserLeftPanels => {
            if state.browser_left_panels.swap_pair() {
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::ToggleBrowserPanelVisible(panel) => {
            state.active_menu = None;
            match panel {
                BrowserLeftPanel::QuickAccess => {
                    state.browser_quick_access_open = !state.browser_quick_access_open;
                }
                BrowserLeftPanel::Explorer => {
                    state.browser_explorer_open = !state.browser_explorer_open;
                }
            }
            let sidebar_open = state.browser_quick_access_open || state.browser_explorer_open;
            let layout_task = commit_layout_with(state, |layout| {
                layout
                    .panels
                    .insert("browser.sidebar".to_owned(), sidebar_open);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::ToggleBrowserLeftPanelCollapse(panel) => {
            match panel {
                BrowserLeftPanel::QuickAccess => {
                    state.browser_quick_access_collapsed = !state.browser_quick_access_collapsed;
                }
                BrowserLeftPanel::Explorer => {
                    state.browser_explorer_collapsed = !state.browser_explorer_collapsed;
                }
            }
            sync_ui_tree_task(state)
        }
        Message::CloseBrowserLeftPanel(panel) => {
            state.context_menu = None;
            match panel {
                BrowserLeftPanel::QuickAccess => state.browser_quick_access_open = false,
                BrowserLeftPanel::Explorer => state.browser_explorer_open = false,
            }
            let sidebar_open = state.browser_quick_access_open || state.browser_explorer_open;
            let layout_task = commit_layout_with(state, |layout| {
                layout
                    .panels
                    .insert("browser.sidebar".to_owned(), sidebar_open);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::BeginDragBrowserRightPanel(panel) => {
            state.browser_right_panels.begin_drag(panel);
            Task::none()
        }
        Message::HoverBrowserRightPanelDrop(panel) => {
            state.browser_right_panels.hover_drop(panel);
            Task::none()
        }
        Message::DropBrowserRightPanel(target) => {
            if state.browser_right_panels.commit_drop(Some(target)) {
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::SwapBrowserRightPanels => {
            if state.browser_right_panels.swap_pair() {
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::ToggleBrowserRightPanelVisible(panel) => {
            state.active_menu = None;
            toggle_browser_right_panel(state, panel)
        }
        Message::ToggleBrowserRightPanelCollapse(panel) => {
            match panel {
                BrowserRightPanel::Preview => {
                    state.browser_preview_collapsed = !state.browser_preview_collapsed;
                }
                BrowserRightPanel::Info => {
                    state.browser_info_collapsed = !state.browser_info_collapsed;
                }
            }
            sync_ui_tree_task(state)
        }
        Message::CloseBrowserRightPanel(panel) => {
            match panel {
                BrowserRightPanel::Preview => state.browser_preview_open = false,
                BrowserRightPanel::Info => state.browser_info_open = false,
            }
            state.browser_inspector_open = state.browser_preview_open || state.browser_info_open;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::PanelPopupToggleBrowserLeft(panel) => {
            match panel {
                BrowserLeftPanel::QuickAccess => {
                    state.browser_quick_access_open = !state.browser_quick_access_open;
                }
                BrowserLeftPanel::Explorer => {
                    state.browser_explorer_open = !state.browser_explorer_open;
                }
            }
            let sidebar_open = state.browser_quick_access_open || state.browser_explorer_open;
            let layout_task = commit_layout_with(state, |layout| {
                layout
                    .panels
                    .insert("browser.sidebar".to_owned(), sidebar_open);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::PanelPopupToggleBrowserRight(panel) => toggle_browser_right_panel(state, panel),
        Message::PanelPopupToggleViewerAdj => {
            state.viewer_adjustments_open = !state.viewer_adjustments_open;
            state.viewer_color_popover_open = state.viewer_adjustments_open;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::PanelPopupToggleViewerInfo => {
            state.viewer_info_open = !state.viewer_info_open;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::PanelPopupToggleViewerFilmstrip => {
            state.viewer_filmstrip_open = !state.viewer_filmstrip_open;
            let thumb_task = if state.viewer_filmstrip_open {
                schedule_missing_thumbnails(state)
            } else {
                Task::none()
            };
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([thumb_task, layout_task, sync_ui_tree_task(state)])
        }
        Message::PanelPopupToggleEditorTools => {
            let next = !state.snapshot.meta.layout.panel("editor.tools");
            let layout_task = commit_layout_with(state, |layout| {
                layout.panels.insert("editor.tools".to_owned(), next);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::PanelPopupToggleEditorPanel(panel) => {
            match panel {
                EditorRightPanel::ToolOptionsHistory => {
                    state.editor_history_open = !state.editor_history_open;
                }
                EditorRightPanel::Layers => {
                    state.editor_layers_open = !state.editor_layers_open;
                }
            }
            let inspector_open = state.editor_history_open || state.editor_layers_open;
            let layout_task = commit_layout_with(state, |layout| {
                layout
                    .panels
                    .insert("editor.inspector".to_owned(), inspector_open);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::BeginDragViewerPanel(panel) => {
            state.viewer_right_panels.begin_drag(panel);
            Task::none()
        }
        Message::HoverViewerPanelDrop(panel) => {
            state.viewer_right_panels.hover_drop(panel);
            Task::none()
        }
        Message::DropViewerPanel(target) => {
            if state.viewer_right_panels.commit_drop(Some(target)) {
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::SwapViewerRightPanels => {
            if state.viewer_right_panels.swap_pair() {
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::ToggleViewerPanelCollapse(panel) => {
            state.context_menu = None;
            match panel {
                ViewerRightPanel::ViewAdjustment => {
                    state.viewer_adjustments_open = !state.viewer_adjustments_open;
                    state.viewer_color_popover_open = state.viewer_adjustments_open;
                }
                ViewerRightPanel::Info => {
                    state.viewer_info_open = !state.viewer_info_open;
                }
            }
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::CloseViewerPanel(panel) => {
            state.context_menu = None;
            match panel {
                ViewerRightPanel::ViewAdjustment => {
                    state.viewer_adjustments_open = false;
                    state.viewer_color_popover_open = false;
                }
                ViewerRightPanel::Info => {
                    state.viewer_info_open = false;
                }
            }
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::SelectEditorToolTab(tab) => {
            state.editor_tool_tab = tab;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::ToggleEditorToolsVisible => {
            state.active_menu = None;
            let next = !state.snapshot.meta.layout.panel("editor.tools");
            let layout_task = commit_layout_with(state, |layout| {
                layout.panels.insert("editor.tools".to_owned(), next);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::CloseEditorToolsPanel => {
            state.context_menu = None;
            let layout_task = commit_layout_with(state, |layout| {
                layout.panels.insert("editor.tools".to_owned(), false);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::CloseEditorPanel(panel) => {
            state.context_menu = None;
            match panel {
                EditorRightPanel::ToolOptionsHistory => state.editor_history_open = false,
                EditorRightPanel::Layers => state.editor_layers_open = false,
            }
            let inspector_open = state.editor_history_open || state.editor_layers_open;
            let layout_task = commit_layout_with(state, |layout| {
                layout
                    .panels
                    .insert("editor.inspector".to_owned(), inspector_open);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::ToggleEditorPanelVisible(panel) => {
            state.active_menu = None;
            match panel {
                EditorRightPanel::ToolOptionsHistory => {
                    state.editor_history_open = !state.editor_history_open;
                }
                EditorRightPanel::Layers => {
                    state.editor_layers_open = !state.editor_layers_open;
                }
            }
            let inspector_open = state.editor_history_open || state.editor_layers_open;
            let layout_task = commit_layout_with(state, |layout| {
                layout
                    .panels
                    .insert("editor.inspector".to_owned(), inspector_open);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::ToggleEditorPanelCollapse(panel) => {
            match panel {
                EditorRightPanel::ToolOptionsHistory => {
                    state.editor_history_collapsed = !state.editor_history_collapsed;
                }
                EditorRightPanel::Layers => {
                    state.editor_layers_collapsed = !state.editor_layers_collapsed;
                }
            }
            sync_ui_tree_task(state)
        }
        Message::BeginDragEditorPanel(panel) => {
            state.editor_panels.begin_drag(panel);
            sync_editor_panel_compat(state);
            Task::none()
        }
        Message::HoverEditorPanelDrop(panel) => {
            state.editor_panels.hover_drop(panel);
            sync_editor_panel_compat(state);
            Task::none()
        }
        Message::DropEditorPanel(target) => {
            let changed = state.editor_panels.commit_drop(Some(target));
            sync_editor_panel_compat(state);
            if !changed {
                return Task::none();
            }
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::SwapEditorPanelOrder => {
            if state.editor_panels.swap_pair() {
                sync_editor_panel_compat(state);
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::ViewerFrameLoaded(frame) => {
            state.viewer_frame = frame;
            Task::none()
        }
        Message::ViewerPairFrameLoaded(frame) => {
            state.viewer_pair_frame = frame;
            Task::none()
        }
        Message::EditorFrameLoaded(frame) => {
            state.editor_frame = frame;
            Task::none()
        }
        Message::EditorOriginalFrameLoaded(frame) => {
            state.editor_original_frame = frame;
            Task::none()
        }
        Message::EditorThumbnailsLoaded(_rev, thumbs) => {
            let mut layer_map = HashMap::with_capacity(thumbs.len());
            let mut mask_map = HashMap::new();
            for item in thumbs {
                layer_map.insert(
                    item.layer_id,
                    iced_image::Handle::from_rgba(item.width, item.height, item.layer_rgba),
                );
                if let Some(mask_rgba) = item.mask_rgba {
                    mask_map.insert(
                        item.layer_id,
                        iced_image::Handle::from_rgba(item.width, item.height, mask_rgba),
                    );
                }
            }
            state.editor_layer_thumbs = layer_map;
            state.editor_mask_thumbs = mask_map;
            Task::none()
        }
        Message::WindowCursorMoved(pt) => {
            state.last_cursor = Some(pt);
            if state.dialog_manager.is_dragging() {
                state.dialog_manager.update_gesture(pt, state.window_size);
            }
            Task::none()
        }
        Message::WindowMouseUp => {
            if state.dialog_manager.is_dragging() {
                state.dialog_manager.end_gesture();
            }
            let mut order_changed = false;
            if state.browser_left_panels.is_dragging()
                && state.browser_left_panels.commit_drop(None)
            {
                order_changed = true;
            }
            if state.browser_right_panels.is_dragging()
                && state.browser_right_panels.commit_drop(None)
            {
                order_changed = true;
            }
            if state.viewer_right_panels.is_dragging()
                && state.viewer_right_panels.commit_drop(None)
            {
                order_changed = true;
            }
            if state.editor_panels.is_dragging() {
                if state.editor_panels.commit_drop(None) {
                    order_changed = true;
                }
                sync_editor_panel_compat(state);
            }
            if order_changed {
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::DialogEvent(ev) => {
            state.dialog_manager.handle_event(ev, state.window_size);
            Task::none()
        }
        Message::OpenViewerContextMenu { path, pos } => {
            state.active_menu = None;
            let resolved_pos = pos
                .or_else(|| state.last_cursor.map(|p| [p.x, p.y]))
                .unwrap_or([
                    state.window_size.width * 0.5,
                    state.window_size.height * 0.5,
                ]);
            let resolved_path = path.or_else(|| {
                if state.viewer_mode == ViewerViewMode::Webtoon {
                    state
                        .webtoon_visible_index
                        .or(state.snapshot.meta.viewer_index)
                        .and_then(|i| state.snapshot.viewer_images.get(i).cloned())
                        .or_else(|| state.snapshot.meta.viewer_path.clone())
                } else {
                    state.snapshot.meta.viewer_path.clone()
                }
            });
            state.context_menu = Some(ActiveContextMenu::Viewer {
                x: resolved_pos[0],
                y: resolved_pos[1],
                target_path: resolved_path,
            });
            sync_ui_tree_task(state)
        }
        Message::OpenLayerContextMenu { layer_id } => {
            state.active_menu = None;
            if state.snapshot.meta.editor.is_some() {
                let p = state.last_cursor.unwrap_or(Point::new(
                    (state.window_size.width - 220.0).max(20.0),
                    220.0,
                ));
                state.context_menu = Some(ActiveContextMenu::EditorLayer {
                    x: p.x,
                    y: p.y,
                    layer_id,
                });
                return sync_ui_tree_task(state);
            }
            Task::none()
        }
        Message::CloseContextMenu => {
            if state.context_menu.take().is_some() {
                sync_ui_tree_task(state)
            } else {
                Task::none()
            }
        }
        Message::ContextMenuDispatch(cmd) => {
            state.context_menu = None;
            dispatch_to_core(state.core.clone(), cmd)
        }
        Message::CopyImageToClipboard(target_path) => {
            state.context_menu = None;
            let frame_rgba =
                if state.snapshot.meta.workspace == Workspace::Editor && target_path.is_none() {
                    state
                        .editor_frame
                        .as_ref()
                        .map(|f| (f.width, f.height, f.rgba.clone()))
                } else if let Some(ref p) = target_path {
                    let viewer_key = format!("viewer:{p}");
                    state
                        .viewer_frame
                        .as_ref()
                        .filter(|f| f.key == viewer_key)
                        .map(|f| (f.width, f.height, f.rgba.clone()))
                } else {
                    None
                };
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        copy_image_png_to_clipboard(target_path.as_deref(), frame_rgba)
                    })
                    .await
                    .unwrap_or(false)
                },
                Message::CopyImageCompleted,
            )
        }
        Message::CopyImageCompleted(ok) => {
            let lang = state.snapshot.meta.language;
            if ok {
                show_toast(state, tr(lang, "toast-image-copied"), ToastKind::Success)
            } else {
                show_toast(
                    state,
                    tr(lang, "toast-clipboard-unavailable"),
                    ToastKind::Error,
                )
            }
        }
        Message::RevealInFolder(path) => {
            state.context_menu = None;
            reveal_path_in_folder(&path);
            Task::none()
        }
        Message::ViewerCanvas(pane_path, ev) => match ev {
            CanvasEvent::ContextMenuRequested { pos, .. } => update(
                state,
                Message::OpenViewerContextMenu {
                    path: pane_path,
                    pos: Some(pos),
                },
            ),
            other => update(state, Message::Canvas(other)),
        },
        Message::DockSplitterDrag(kind, val) => {
            state.dock_drag.set_drag(kind, val);
            Task::none()
        }
        Message::DockSplitterRelease(kind, val) => {
            if let Some(next_layout) =
                state
                    .dock_drag
                    .commit_release(&state.snapshot.meta.layout, kind, val)
            {
                return commit_layout_with(state, |layout| {
                    layout.editor_right_width = next_layout.editor_right_width;
                    layout.editor_right_split_pct = next_layout.editor_right_split_pct;
                });
            }
            Task::none()
        }
        Message::BrowserSplitterDrag(w) => {
            state.browser_left_width_drag = Some(w.clamp(160.0, 520.0));
            Task::none()
        }
        Message::BrowserSplitterRelease(w) => {
            state.browser_left_width_drag = None;
            let clamped = w.round().clamp(160.0, 520.0) as u32;
            if state.snapshot.meta.layout.left_width == clamped {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.left_width = clamped;
            })
        }
        Message::BrowserRightSplitterDrag(w) => {
            state.browser_right_width_drag = Some(w.clamp(200.0, 600.0));
            Task::none()
        }
        Message::BrowserRightSplitterRelease(w) => {
            state.browser_right_width_drag = None;
            let clamped = w.round().clamp(200.0, 600.0) as u32;
            if state.snapshot.meta.layout.right_width == clamped {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.right_width = clamped;
            })
        }
        Message::ViewerSplitterDrag(w) => {
            state.viewer_right_width_preview = Some(w.clamp(200.0, 600.0));
            Task::none()
        }
        Message::ViewerSplitterRelease(w) => {
            state.viewer_right_width_preview = None;
            let clamped = w.round().clamp(200.0, 600.0) as u32;
            if state.snapshot.meta.layout.right_width == clamped {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.right_width = clamped;
            })
        }
        Message::SetBrowserSortField(field) => {
            state.active_menu = None;
            commit_layout_with(state, |layout| {
                if layout.browser_sort_field == field {
                    layout.browser_sort_order = match layout.browser_sort_order {
                        SortOrder::Asc => SortOrder::Desc,
                        SortOrder::Desc => SortOrder::Asc,
                    };
                } else {
                    layout.browser_sort_field = field;
                }
            })
        }
        Message::SetBrowserSortOrder(order) => {
            state.active_menu = None;
            if state.snapshot.meta.layout.browser_sort_order == order {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.browser_sort_order = order;
            })
        }
        Message::SetBrowserThumbSize(size) => {
            if state.snapshot.meta.layout.browser_thumb_size == size {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.browser_thumb_size = size;
            })
        }
        Message::OpenBatchDialog(kind) => {
            state.active_menu = None;
            state.context_menu = None;
            let meta = &state.snapshot.meta;
            let sel_images: Vec<&str> = meta
                .selection
                .iter()
                .filter(|p| {
                    state
                        .snapshot
                        .entries
                        .iter()
                        .any(|e| &e.path == *p && e.is_image)
                })
                .map(String::as_str)
                .collect();
            let ref_dims = sel_images
                .first()
                .copied()
                .or_else(|| {
                    state
                        .snapshot
                        .entries
                        .iter()
                        .find(|e| e.is_image)
                        .map(|e| e.path.as_str())
                })
                .and_then(|p| photon_image::probe_dimensions(Path::new(p)));
            state.dialog = Some(InAppDialog::Batch(BatchDialogState::new(
                kind,
                !sel_images.is_empty(),
                meta.current_folder.as_deref(),
                ref_dims,
            )));
            open_dialog_window(state, kind.dialog_id());
            Task::none()
        }
        Message::BatchDialog(msg) => {
            if matches!(msg, BatchDialogMsg::PickOutputFolder) {
                let title = tr(state.snapshot.meta.language, "batch-output-folder");
                let start = match &state.dialog {
                    Some(InAppDialog::Batch(b)) if !b.output_folder.is_empty() => {
                        Some(PathBuf::from(&b.output_folder))
                    }
                    _ => state
                        .snapshot
                        .meta
                        .current_folder
                        .as_deref()
                        .map(PathBuf::from),
                };
                return Task::perform(
                    async move { photon_dialogs::pick_folder(&title, start).await },
                    Message::BatchOutputFolderPicked,
                );
            }
            if matches!(msg, BatchDialogMsg::Confirm) {
                let (cmd_res, dialog_id) = match &mut state.dialog {
                    Some(InAppDialog::Batch(b)) => {
                        (b.build_command(&state.snapshot), b.kind.dialog_id())
                    }
                    _ => return Task::none(),
                };
                match cmd_res {
                    Ok(cmd) => {
                        state.dialog = None;
                        state.dialog_manager.close(&dialog_id);
                        return dispatch_to_core(state.core.clone(), cmd);
                    }
                    Err(err_key) => {
                        if let Some(InAppDialog::Batch(b)) = &mut state.dialog {
                            b.error = Some(tr(state.snapshot.meta.language, &err_key));
                        }
                        return Task::none();
                    }
                }
            }
            if let Some(InAppDialog::Batch(b)) = &mut state.dialog {
                b.apply_msg(msg);
            }
            Task::none()
        }
        Message::BatchOutputFolderPicked(Some(path)) => {
            if let Some(InAppDialog::Batch(b)) = &mut state.dialog {
                b.apply_msg(BatchDialogMsg::OutputFolderPicked(Some(
                    path.to_string_lossy().into_owned(),
                )));
            }
            Task::none()
        }
        Message::BatchOutputFolderPicked(None) => Task::none(),
        Message::OpenBrowserContextMenu { path, x, y } => {
            state.active_menu = None;
            let pos = state.last_cursor.unwrap_or(Point::new(x, y));
            state.context_menu = Some(ActiveContextMenu::BrowserEntry {
                x: pos.x,
                y: pos.y,
                path,
            });
            sync_ui_tree_task(state)
        }
        Message::OpenQuickAccessContextMenu { path, x, y } => {
            state.active_menu = None;
            let pos = state.last_cursor.unwrap_or(Point::new(x, y));
            state.context_menu = Some(ActiveContextMenu::BrowserQuickAccess {
                x: pos.x,
                y: pos.y,
                path,
            });
            sync_ui_tree_task(state)
        }
        Message::BrowserCopyPath(path) => {
            state.context_menu = None;
            let toast_task = show_toast(
                state,
                tr(state.snapshot.meta.language, "toast-path-copied"),
                ToastKind::Success,
            );
            Task::batch([iced::clipboard::write(path), toast_task])
        }
        Message::BrowserStartRename(path) => {
            state.context_menu = None;
            let name = Path::new(&path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_owned();
            state.dialog = Some(InAppDialog::RenameItem {
                path,
                name,
                error: None,
            });
            open_dialog_window(state, DesktopDialogId::RenameItem);
            Task::none()
        }
        Message::BrowserRenameChanged(val) => {
            if let Some(InAppDialog::RenameItem { name, error, .. }) = &mut state.dialog {
                *name = val;
                *error = None;
            }
            Task::none()
        }
        Message::BrowserConfirmRename => {
            let Some(InAppDialog::RenameItem { path, name, .. }) = state.dialog.clone() else {
                return Task::none();
            };
            let clean = name.trim().to_owned();
            if clean.is_empty() || clean.contains('/') || clean.contains('\\') {
                if let Some(InAppDialog::RenameItem { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "dialog-error-title"));
                }
                return Task::none();
            }
            state.dialog = None;
            state.dialog_manager.close(&DesktopDialogId::RenameItem);
            dispatch_to_core(
                state.core.clone(),
                Command::RenamePath {
                    path,
                    new_name: clean,
                },
            )
        }
        Message::BrowserCopyTo(paths) => {
            state.context_menu = None;
            let title = tr(state.snapshot.meta.language, "action-copy-to");
            let start = state
                .snapshot
                .meta
                .current_folder
                .as_deref()
                .map(PathBuf::from);
            Task::perform(
                async move { photon_dialogs::pick_folder(&title, start).await },
                move |dest| Message::BrowserCopyMoveDestPicked {
                    is_move: false,
                    paths,
                    dest,
                },
            )
        }
        Message::BrowserMoveTo(paths) => {
            state.context_menu = None;
            let title = tr(state.snapshot.meta.language, "action-move-to");
            let start = state
                .snapshot
                .meta
                .current_folder
                .as_deref()
                .map(PathBuf::from);
            Task::perform(
                async move { photon_dialogs::pick_folder(&title, start).await },
                move |dest| Message::BrowserCopyMoveDestPicked {
                    is_move: true,
                    paths,
                    dest,
                },
            )
        }
        Message::BrowserCopyMoveDestPicked {
            is_move,
            paths,
            dest: Some(dest),
        } => {
            let destination = dest.to_string_lossy().into_owned();
            let cmd = if is_move {
                Command::MoveTo { paths, destination }
            } else {
                Command::CopyTo { paths, destination }
            };
            dispatch_to_core(state.core.clone(), cmd)
        }
        Message::BrowserCopyMoveDestPicked { dest: None, .. } => Task::none(),
        Message::BrowserConfirmDeletePermanently(paths) => {
            state.context_menu = None;
            state.dialog = Some(InAppDialog::ConfirmDeletePermanently { paths });
            state
                .dialog_manager
                .bring_to_front(&DesktopDialogId::ConfirmDeletePermanently);
            Task::none()
        }
        Message::BrowserExecuteDeletePermanently(paths) => {
            state.dialog = None;
            state
                .dialog_manager
                .close(&DesktopDialogId::ConfirmDeletePermanently);
            dispatch_to_core(state.core.clone(), Command::DeletePermanently { paths })
        }
        Message::ToggleViewerColorPopover => {
            state.context_menu = None;
            state.viewer_adjustments_open = !state.viewer_adjustments_open;
            state.viewer_color_popover_open = state.viewer_adjustments_open;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::SetViewerAdjExposure(v) => {
            state.viewer_adj.exposure = v;
            Task::none()
        }
        Message::SetViewerAdjGamma(v) => {
            state.viewer_adj.gamma = v.clamp(0.2, 3.0);
            Task::none()
        }
        Message::SetViewerAdjBrightness(v) => {
            state.viewer_adj.brightness = v;
            Task::none()
        }
        Message::SetViewerAdjContrast(v) => {
            state.viewer_adj.contrast = v;
            Task::none()
        }
        Message::SetViewerAdjSaturation(v) => {
            state.viewer_adj.saturation = v;
            Task::none()
        }
        Message::SetViewerAdjHighlights(v) => {
            state.viewer_adj.highlights = v;
            Task::none()
        }
        Message::SetViewerAdjShadows(v) => {
            state.viewer_adj.shadows = v;
            Task::none()
        }
        Message::ResetViewerAdj => {
            state.viewer_adj = ColorAdjustments::default();
            Task::none()
        }
        Message::ViewerAdjOpenInEditor => {
            state.context_menu = None;
            state.viewer_color_popover_open = false;
            if let Some(path) = state.snapshot.meta.viewer_path.clone() {
                state.transfer_adj_to_editor = Some(state.viewer_adj);
                return dispatch_to_core(state.core.clone(), Command::EditorOpen { path });
            }
            Task::none()
        }
    }
}

fn handle_key_event(state: &mut PhotonGui, ev: keyboard::Event) -> Task<Message> {
    let keyboard::Event::KeyPressed { key, modifiers, .. } = ev else {
        return Task::none();
    };
    if modifiers.logo() {
        return Task::none();
    }
    if modifiers.alt() {
        if modifiers.control()
            && matches!(key.as_ref(), Key::Character("c") | Key::Character("C"))
            && state.snapshot.meta.workspace == Workspace::Viewer
        {
            return update(state, Message::CopyViewerPath);
        }
        return Task::none();
    }

    if matches!(key, Key::Named(Named::Escape)) {
        if let Some(d) = state.dialog.take() {
            if let InAppDialog::ColorPicker {
                target: ColorPickerTarget::ViewportBackground,
                previous_viewport_bg,
                ..
            } = &d
            {
                state.viewport_background = *previous_viewport_bg;
            }
            state.dialog_manager.close(&dialog_id_for_in_app(&d));
            return Task::none();
        }
        if state.settings_open {
            state.settings_open = false;
            state.dialog_manager.close(&DesktopDialogId::Settings);
            return Task::none();
        }
        if state.context_menu.take().is_some() {
            return sync_ui_tree_task(state);
        }
        if state.active_menu.is_some() {
            state.active_menu = None;
            return Task::none();
        }
        if state.path_editing {
            state.path_editing = false;
            return Task::none();
        }
        if state.editor_ui.adjust_popup_tab.is_some() {
            return update(state, Message::EditorCloseAdjustPopup { apply: false });
        }
        if state.is_fullscreen {
            return update(state, Message::ToggleFullscreen);
        }
        if state.crop_uv.is_some() {
            state.crop_uv = None;
            return sync_ui_tree_task(state);
        }
        match state.snapshot.meta.workspace {
            Workspace::Editor => {
                return update(
                    state,
                    Message::RequestEditorLeave {
                        close_editor: false,
                    },
                );
            }
            Workspace::Viewer => {
                return dispatch_to_core(state.core.clone(), Command::WorkspaceBack);
            }
            Workspace::Browser => {
                if state.browser_filter_bar_open {
                    if !state.browser_filter.is_empty() {
                        state.browser_filter.clear();
                        return widget_operation::focus(browser_filter_input_id());
                    }
                    state.browser_filter_bar_open = false;
                    return sync_ui_tree_task(state);
                }
            }
        }
    }

    if matches!(key, Key::Named(Named::F11)) {
        return update(state, Message::ToggleFullscreen);
    }

    if state.settings_open
        || state.dialog.is_some()
        || (state.snapshot.meta.workspace == Workspace::Editor
            && state.editor_ui.adjust_popup_tab.is_some())
    {
        return Task::none();
    }

    if matches!(key, Key::Named(Named::F5)) && state.snapshot.meta.workspace == Workspace::Browser {
        return dispatch_to_core(state.core.clone(), Command::Refresh);
    }

    if modifiers.contains(Modifiers::CTRL) {
        match key.as_ref() {
            Key::Character(",") => {
                state.active_menu = None;
                state.settings_open = true;
                return Task::none();
            }
            Key::Character("o") if modifiers.shift() => {
                return update(state, Message::PickImagesDialog);
            }
            Key::Character("o") => {
                return update(state, Message::PickFolderDialog);
            }
            Key::Character("n") if state.snapshot.meta.workspace == Workspace::Browser => {
                return update(state, Message::OpenNewFolderDialog);
            }
            Key::Character("s")
                if modifiers.shift() && state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::PickSaveAsDialog);
            }
            Key::Character("e")
                if modifiers.shift() && state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::OpenExportDialog);
            }
            Key::Character("c")
                if modifiers.shift() && state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::CopyImageToClipboard(None));
            }
            Key::Character("c")
                if modifiers.shift() && state.snapshot.meta.workspace == Workspace::Viewer =>
            {
                return update(
                    state,
                    Message::CopyImageToClipboard(state.snapshot.meta.viewer_path.clone()),
                );
            }
            Key::Character("r") if state.snapshot.meta.workspace == Workspace::Editor => {
                return update(state, Message::OpenResizeDialog);
            }
            Key::Character("l") if state.snapshot.meta.workspace == Workspace::Browser => {
                state.path_editing = !state.path_editing;
                return Task::none();
            }
            Key::Character("i") | Key::Character("I")
                if state.snapshot.meta.workspace == Workspace::Browser =>
            {
                return update(state, Message::ToggleBrowserFilterBar);
            }
            Key::Character("1") => {
                return dispatch_to_core(
                    state.core.clone(),
                    Command::SetWorkspace {
                        workspace: Workspace::Browser,
                    },
                );
            }
            Key::Character("2") => {
                return dispatch_to_core(
                    state.core.clone(),
                    Command::SetWorkspace {
                        workspace: Workspace::Viewer,
                    },
                );
            }
            Key::Character("3") => {
                return dispatch_to_core(
                    state.core.clone(),
                    Command::SetWorkspace {
                        workspace: Workspace::Editor,
                    },
                );
            }
            Key::Character("z") if modifiers.shift() => {
                return dispatch_to_core(state.core.clone(), Command::EditorRedo);
            }
            Key::Character("z") => {
                return dispatch_to_core(state.core.clone(), Command::EditorUndo);
            }
            Key::Character("y") => {
                return dispatch_to_core(state.core.clone(), Command::EditorRedo);
            }
            Key::Character("s") => {
                return dispatch_to_core(state.core.clone(), Command::EditorSave);
            }
            _ => {}
        }
    }

    match state.snapshot.meta.workspace {
        Workspace::Editor if state.editor_ui.renaming_layer_id.is_none() => match key.as_ref() {
            Key::Character("v") | Key::Character("V") => {
                update(state, Message::EditorSelectTool(EditorTool::Pointer))
            }
            Key::Character("h") | Key::Character("H") => {
                update(state, Message::EditorSelectTool(EditorTool::Hand))
            }
            Key::Character("z") | Key::Character("Z") => {
                update(state, Message::EditorSelectTool(EditorTool::Zoom))
            }
            Key::Character("b") | Key::Character("B") => {
                update(state, Message::EditorSelectTool(EditorTool::Brush))
            }
            Key::Character("e") | Key::Character("E") => {
                update(state, Message::EditorSelectTool(EditorTool::Eraser))
            }
            Key::Character("l") | Key::Character("L") => {
                update(state, Message::EditorSelectTool(EditorTool::Liquify))
            }
            Key::Character("i") | Key::Character("I") => {
                update(state, Message::EditorSelectTool(EditorTool::Eyedropper))
            }
            Key::Character("c") | Key::Character("C") => {
                update(state, Message::EditorSelectTool(EditorTool::Crop))
            }
            Key::Character("x") | Key::Character("X") => update(state, Message::EditorSwapColors),
            Key::Character("[") => update(
                state,
                Message::EditorSetBrushRadius((state.editor_ui.brush_radius - 2.0).max(1.0)),
            ),
            Key::Character("]") => update(
                state,
                Message::EditorSetBrushRadius((state.editor_ui.brush_radius + 2.0).min(200.0)),
            ),
            _ => Task::none(),
        },
        Workspace::Viewer => match key.as_ref() {
            Key::Named(Named::ArrowLeft) => {
                if state.viewer_mode == ViewerViewMode::Double {
                    let delta = if state.viewer_manga_rtl { 1 } else { -1 };
                    update(state, Message::DualStepSpread(delta))
                } else {
                    dispatch_to_core(state.core.clone(), Command::ViewerPrevious)
                }
            }
            Key::Named(Named::ArrowRight) => {
                if state.viewer_mode == ViewerViewMode::Double {
                    let delta = if state.viewer_manga_rtl { -1 } else { 1 };
                    update(state, Message::DualStepSpread(delta))
                } else {
                    dispatch_to_core(state.core.clone(), Command::ViewerNext)
                }
            }
            Key::Named(Named::PageDown) => {
                if state.viewer_mode == ViewerViewMode::Webtoon {
                    let step = (state.window_size.height - 140.0).max(240.0) * 0.85;
                    widget_operation::scroll_to(
                        webtoon_scroll_id(),
                        scrollable::AbsoluteOffset {
                            x: None,
                            y: Some(state.webtoon_scroll_y + step),
                        },
                    )
                } else if state.viewer_mode == ViewerViewMode::Double {
                    update(state, Message::DualStepSpread(1))
                } else {
                    dispatch_to_core(state.core.clone(), Command::ViewerNext)
                }
            }
            Key::Named(Named::PageUp) => {
                if state.viewer_mode == ViewerViewMode::Webtoon {
                    let step = (state.window_size.height - 140.0).max(240.0) * 0.85;
                    widget_operation::scroll_to(
                        webtoon_scroll_id(),
                        scrollable::AbsoluteOffset {
                            x: None,
                            y: Some((state.webtoon_scroll_y - step).max(0.0)),
                        },
                    )
                } else if state.viewer_mode == ViewerViewMode::Double {
                    update(state, Message::DualStepSpread(-1))
                } else {
                    dispatch_to_core(state.core.clone(), Command::ViewerPrevious)
                }
            }
            Key::Named(Named::ArrowDown) if state.viewer_mode == ViewerViewMode::Webtoon => {
                widget_operation::scroll_to(
                    webtoon_scroll_id(),
                    scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(state.webtoon_scroll_y + 260.0),
                    },
                )
            }
            Key::Named(Named::ArrowUp) if state.viewer_mode == ViewerViewMode::Webtoon => {
                widget_operation::scroll_to(
                    webtoon_scroll_id(),
                    scrollable::AbsoluteOffset {
                        x: None,
                        y: Some((state.webtoon_scroll_y - 260.0).max(0.0)),
                    },
                )
            }
            Key::Named(Named::Backspace) => {
                dispatch_to_core(state.core.clone(), Command::WorkspaceBack)
            }
            Key::Character("s") | Key::Character("S") => {
                update(state, Message::SetViewerViewMode(ViewerViewMode::Single))
            }
            Key::Character("d") | Key::Character("D") => {
                update(state, Message::SetViewerViewMode(ViewerViewMode::Double))
            }
            Key::Character("m") | Key::Character("M") => {
                update(state, Message::SetViewerViewMode(ViewerViewMode::Webtoon))
            }
            Key::Character("i") | Key::Character("I") => {
                update(state, Message::ToggleViewerInspector)
            }
            Key::Character("f") | Key::Character("F") | Key::Character("0") => {
                update(state, Message::Canvas(CanvasEvent::ResetFit))
            }
            Key::Character("+") | Key::Character("=") => update(state, Message::ZoomStep(1.25)),
            Key::Character("-") => update(state, Message::ZoomStep(0.8)),
            Key::Named(Named::Space) if state.viewer_mode == ViewerViewMode::Webtoon => {
                let step = (state.window_size.height - 140.0).max(240.0) * 0.85;
                widget_operation::scroll_to(
                    webtoon_scroll_id(),
                    scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(state.webtoon_scroll_y + step),
                    },
                )
            }
            Key::Character("r") if modifiers.shift() => {
                update(state, Message::ViewerRotatePreview(-1))
            }
            Key::Character("r") | Key::Character("R") => {
                update(state, Message::ViewerRotatePreview(1))
            }
            Key::Character("1") => update(state, Message::ViewerZoomActual),
            _ => Task::none(),
        },
        _ => Task::none(),
    }
}

pub fn view(state: &PhotonGui, window_id: window::Id) -> Element<'_, Message> {
    let role = state
        .windows
        .get(&window_id)
        .copied()
        .unwrap_or(WindowRole::Main);
    let lang = state.snapshot.meta.language;
    let ws = if role == WindowRole::AuxiliaryViewer {
        Workspace::Viewer
    } else {
        state.snapshot.meta.workspace
    };

    let top_bar = build_toolbar(state, role, ws);
    let body: Element<'_, Message> = match ws {
        Workspace::Browser => build_browser_view(state),
        Workspace::Viewer => build_viewer_view(state),
        Workspace::Editor => build_editor_view(state),
    };

    let status_left = match ws {
        Workspace::Viewer => viewer_status_summary(state, lang),
        Workspace::Editor => editor_status_summary(state, lang),
        Workspace::Browser => {
            let s = tr_ui_text(lang, &state.snapshot.meta.status);
            if s.is_empty() { tr(lang, "ready") } else { s }
        }
    };
    let backend = GPU_BACKEND_NAME.get().map(String::as_str).unwrap_or("wgpu");
    let right_meta = if ws == Workspace::Viewer {
        let counter_prefix = viewer_counter_summary(state)
            .map(|c| format!("{c} · "))
            .unwrap_or_default();
        if state.snapshot.meta.safe_mode {
            format!("{counter_prefix}{}", tr(lang, "label-safe-mode"))
        } else {
            let mode_label = match state.viewer_mode {
                ViewerViewMode::Single => tr(lang, "action-viewer-single"),
                ViewerViewMode::Double => tr(lang, "action-viewer-double"),
                ViewerViewMode::Webtoon => tr(lang, "action-viewer-manhwa"),
            };
            if state.viewer_mode == ViewerViewMode::Webtoon {
                let fit_label = match state.viewer_webtoon_fit_mode {
                    WebtoonFitMode::Auto => tr(lang, "action-fit-auto"),
                    WebtoonFitMode::Width => tr(lang, "action-fit-width"),
                    WebtoonFitMode::Height => tr(lang, "action-fit-height"),
                    WebtoonFitMode::Original => tr(lang, "action-fit-original"),
                };
                format!(
                    "{counter_prefix}{:.0}% ({fit_label}) · {mode_label} · {backend}",
                    state.webtoon_zoom * 100.0
                )
            } else {
                format!(
                    "{counter_prefix}{:.0}% · {mode_label} · {backend}",
                    state.zoom * 100.0
                )
            }
        }
    } else if state.snapshot.meta.safe_mode {
        tr(lang, "label-safe-mode")
    } else if ws == Workspace::Editor {
        format!("{:.0}% · {backend}", state.zoom * 100.0)
    } else {
        backend.to_owned()
    };
    let status = status_bar(status_left, right_meta);
    let shell_col = column![top_bar, container(body).height(Fill).width(Fill), status];

    let shell = container(shell_col.height(Fill).width(Fill))
        .width(Fill)
        .height(Fill)
        .style(app_shell_style);

    let with_menu = match build_active_menu(state, role, ws) {
        Some((Some(left_offset), menu_el)) => {
            with_anchored_menu(shell, true, left_offset, 44.0, menu_el, Message::CloseMenu)
        }
        Some((None, menu_el)) => match state.active_menu {
            Some(ActiveMenu::WorkspacePanels) => {
                with_top_right_menu_at(shell, true, 44.0, 44.0, menu_el, Message::CloseMenu)
            }
            Some(ActiveMenu::ReaderOptions) => with_top_right_menu_at(
                shell,
                true,
                viewer_reader_options_right_offset(state, role),
                44.0,
                menu_el,
                Message::CloseMenu,
            ),
            Some(ActiveMenu::EditorAddLayer) => {
                with_bottom_right_menu_at(shell, true, 14.0, 60.0, menu_el, Message::CloseMenu)
            }
            _ => with_top_right_menu(shell, true, menu_el, Message::CloseMenu),
        },
        None => shell.into(),
    };

    let max_menu_h = (state.window_size.height - 16.0).max(160.0);
    fn clamp_menu_height<'a>(
        menu_el: Element<'a, Message>,
        est_h: f32,
        max_h: f32,
    ) -> (Element<'a, Message>, f32) {
        if est_h > max_h {
            (
                scrollable(menu_el).height(Length::Fixed(max_h)).into(),
                max_h,
            )
        } else {
            (menu_el, est_h)
        }
    }

    let with_ctx = match &state.context_menu {
        Some(ActiveContextMenu::BrowserEntry { x, y, path }) => {
            let (menu_el, est_h) = clamp_menu_height(
                build_browser_context_menu_entries(state, path),
                480.0,
                max_menu_h,
            );
            with_context_menu(
                with_menu,
                true,
                *x,
                *y,
                state.window_size,
                225.0,
                est_h,
                menu_el,
                Message::CloseContextMenu,
            )
        }
        Some(ActiveContextMenu::BrowserQuickAccess { x, y, path }) => {
            let (menu_el, est_h) = clamp_menu_height(
                build_quick_access_context_menu(state, path),
                220.0,
                max_menu_h,
            );
            with_context_menu(
                with_menu,
                true,
                *x,
                *y,
                state.window_size,
                220.0,
                est_h,
                menu_el,
                Message::CloseContextMenu,
            )
        }
        Some(ActiveContextMenu::Viewer { x, y, target_path }) => {
            let (menu_el, est_h) = clamp_menu_height(
                build_viewer_context_menu(state, target_path.as_deref()),
                615.0,
                max_menu_h,
            );
            with_context_menu(
                with_menu,
                true,
                *x,
                *y,
                state.window_size,
                225.0,
                est_h,
                menu_el,
                Message::CloseContextMenu,
            )
        }
        Some(ActiveContextMenu::EditorCanvas { x, y }) => {
            if let Some(raw_menu) = build_editor_canvas_context_menu(state) {
                let (menu_el, est_h) = clamp_menu_height(raw_menu, 575.0, max_menu_h);
                with_context_menu(
                    with_menu,
                    true,
                    *x,
                    *y,
                    state.window_size,
                    225.0,
                    est_h,
                    menu_el,
                    Message::CloseContextMenu,
                )
            } else {
                with_menu
            }
        }
        Some(ActiveContextMenu::EditorLayer { x, y, layer_id }) => {
            if let Some(raw_menu) = build_editor_layer_context_menu(state, *layer_id) {
                let (menu_el, est_h) = clamp_menu_height(raw_menu, 380.0, max_menu_h);
                with_context_menu(
                    with_menu,
                    true,
                    *x,
                    *y,
                    state.window_size,
                    210.0,
                    est_h,
                    menu_el,
                    Message::CloseContextMenu,
                )
            } else {
                with_menu
            }
        }
        None => with_menu,
    };

    let toast_spec = state.toast.as_ref().map(|t| ToastSpec {
        message: t.message.clone(),
        kind: t.kind,
        on_dismiss: Message::DismissToast(Some(t.id)),
    });
    let with_toast = with_toast_banner(with_ctx, toast_spec);

    let theme = theme(state, window_id);
    let mut dialogs: Vec<ManagedDialogSpec<'_, DesktopDialogId, Message>> = Vec::new();
    if role == WindowRole::Main
        && ws == Workspace::Editor
        && let Some(active_tab) = state.editor_ui.adjust_popup_tab
    {
        dialogs.push(build_editor_adjustment_dialog(state, active_tab, &theme));
    }
    if state.settings_open && role == WindowRole::Main {
        let settings_body = build_settings_body(state);
        let settings_footer = row![
            button::labeled(
                tr(lang, "action-reset-layout"),
                Some(Icon::Reset),
                ButtonVariant::Secondary,
                Some(Message::RequestResetLayout),
            ),
            iced::widget::space::horizontal(),
            button::labeled(
                tr(lang, "action-close"),
                None,
                ButtonVariant::Primary,
                Some(Message::CloseSettings),
            ),
        ]
        .align_y(Alignment::Center);

        dialogs.push(dialog_spec_to_managed(
            DesktopDialogId::Settings,
            DialogSpec {
                open: true,
                title: tr(lang, "settings-title"),
                width: 560.0,
                body: settings_body,
                footer: settings_footer.into(),
                on_close: Message::CloseSettings,
                on_noop: Message::Noop,
            },
            default_window_state_for(DesktopDialogId::Settings),
            OverlayMode::default(),
            lang,
            &theme,
        ));
    }

    if role == WindowRole::Main
        && let Some(active_dialog) = &state.dialog
    {
        dialogs.push(build_in_app_dialog(state, active_dialog, &theme));
    }

    managed_dialog_stack(
        with_toast,
        &state.dialog_manager,
        state.window_size,
        dialogs,
    )
}

fn build_toolbar(state: &PhotonGui, role: WindowRole, ws: Workspace) -> Element<'_, Message> {
    match ws {
        Workspace::Browser => build_browser_toolbar(state),
        Workspace::Viewer => build_viewer_toolbar(state, role),
        Workspace::Editor => build_editor_toolbar(state, role),
    }
}

pub(crate) fn toolbar_divider<'a, M: 'a>() -> Element<'a, M> {
    container(rule::vertical(1).style(hairline_rule_style))
        .height(Length::Fixed(18.0))
        .into()
}

pub(crate) fn toolbar_right_utilities(state: &PhotonGui) -> row::Row<'_, Message> {
    let lang = state.snapshot.meta.language;
    let panels_open = state.active_menu == Some(ActiveMenu::WorkspacePanels);

    row![with_tooltip_pos(
        button::icon_only(
            Icon::Sliders,
            ButtonVariant::Tab {
                active: panels_open,
            },
            Some(Message::ToggleMenu(ActiveMenu::WorkspacePanels)),
        ),
        tr(lang, "tooltip-panels-appearance-menu"),
        TooltipPosition::Bottom,
    ),]
    .spacing(4)
    .align_y(Alignment::Center)
}

fn build_workspace_panels_menu(state: &PhotonGui, ws: Workspace) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let layout = &state.snapshot.meta.layout;
    let is_dark = state.snapshot.meta.theme == Theme::Dark;

    let mut entries = vec![MenuEntry::Item(
        MenuItem::new(
            tr(
                lang,
                if is_dark {
                    "action-switch-light-theme"
                } else {
                    "action-switch-dark-theme"
                },
            ),
            Message::Dispatch(Command::ToggleTheme),
        )
        .icon(if is_dark { Icon::Sun } else { Icon::Moon }),
    )];

    entries.push(MenuEntry::Separator);
    match ws {
        Workspace::Browser => {
            entries.push(MenuEntry::Header(tr(lang, "panels-group-browser")));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-quick-access"),
                    Message::PanelPopupToggleBrowserLeft(BrowserLeftPanel::QuickAccess),
                )
                .icon(Icon::Star)
                .checked(state.browser_quick_access_open),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-explorer"),
                    Message::PanelPopupToggleBrowserLeft(BrowserLeftPanel::Explorer),
                )
                .icon(Icon::Folder)
                .checked(state.browser_explorer_open),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-preview"),
                    Message::PanelPopupToggleBrowserRight(BrowserRightPanel::Preview),
                )
                .icon(Icon::Image)
                .checked(state.browser_preview_open),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-info"),
                    Message::PanelPopupToggleBrowserRight(BrowserRightPanel::Info),
                )
                .icon(Icon::Info)
                .checked(state.browser_info_open),
            ));
        }
        Workspace::Viewer => {
            entries.push(MenuEntry::Header(tr(lang, "panels-group-viewer")));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-view-adjustments"),
                    Message::PanelPopupToggleViewerAdj,
                )
                .icon(Icon::Palette)
                .checked(state.viewer_adjustments_open),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(tr(lang, "panel-info"), Message::PanelPopupToggleViewerInfo)
                    .icon(Icon::Info)
                    .checked(state.viewer_info_open),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "action-filmstrip"),
                    Message::PanelPopupToggleViewerFilmstrip,
                )
                .icon(Icon::Filmstrip)
                .checked(state.viewer_filmstrip_open),
            ));
        }
        Workspace::Editor => {
            entries.push(MenuEntry::Header(tr(lang, "panels-group-editor")));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-tools"),
                    Message::PanelPopupToggleEditorTools,
                )
                .icon(Icon::Pointer)
                .checked(layout.panel("editor.tools")),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-tool-options-history"),
                    Message::PanelPopupToggleEditorPanel(EditorRightPanel::ToolOptionsHistory),
                )
                .icon(Icon::History)
                .checked(state.editor_history_open),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-layers"),
                    Message::PanelPopupToggleEditorPanel(EditorRightPanel::Layers),
                )
                .icon(Icon::Layers)
                .checked(state.editor_layers_open),
            ));
        }
    }

    entries.push(MenuEntry::Separator);
    entries.push(MenuEntry::Item(
        MenuItem::new(tr(lang, "action-reset-layout"), Message::RequestResetLayout)
            .icon(Icon::Reset),
    ));
    entries.push(MenuEntry::Separator);
    entries.push(MenuEntry::Item(
        MenuItem::new(tr(lang, "action-settings"), Message::OpenSettings)
            .icon(Icon::Settings)
            .shortcut("Ctrl+,"),
    ));

    menu_panel_entries(entries, 256.0)
}

fn build_active_menu(
    state: &PhotonGui,
    _role: WindowRole,
    ws: Workspace,
) -> Option<(Option<f32>, Element<'_, Message>)> {
    let menu = state.active_menu?;
    if menu == ActiveMenu::WorkspacePanels {
        return Some((None, build_workspace_panels_menu(state, ws)));
    }
    if ws == Workspace::Editor && menu == ActiveMenu::EditorAddLayer {
        return Some((None, build_editor_add_layer_menu(state)));
    }
    let lang = state.snapshot.meta.language;
    let layout = &state.snapshot.meta.layout;
    let ed = state.snapshot.meta.editor.as_ref();

    let has_selected_image = state.snapshot.meta.selection.iter().any(|path| {
        state
            .snapshot
            .entries
            .iter()
            .any(|e| &e.path == path && e.is_image)
    });

    let (offset, entries) = match (ws, menu) {
        (Workspace::Browser, ActiveMenu::BrowserFile) => (
            Some(12.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-open-folder-dialog"),
                        Message::PickFolderDialog,
                    )
                    .icon(Icon::FolderOpen)
                    .shortcut("Ctrl+O"),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-open-file-dialog"),
                        Message::PickImagesDialog,
                    )
                    .icon(Icon::Image)
                    .shortcut("Ctrl+Shift+O"),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-new-folder"), Message::OpenNewFolderDialog)
                        .icon(Icon::Folder)
                        .shortcut("Ctrl+N"),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-edit-path"), Message::TogglePathEdit(true))
                        .shortcut("Ctrl+L"),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-file-edit"),
                        Message::BrowserOpenSelectedInEditor,
                    )
                    .icon(Icon::Editor)
                    .enabled(has_selected_image),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-refresh"),
                        Message::Dispatch(Command::Refresh),
                    )
                    .icon(Icon::Refresh)
                    .shortcut("F5"),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-settings"), Message::OpenSettings)
                        .icon(Icon::Settings)
                        .shortcut("Ctrl+,"),
                ),
            ],
        ),
        (Workspace::Browser, ActiveMenu::BrowserView) => (
            Some(56.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-view-grid"),
                        Message::SetBrowserViewMode(BrowserViewMode::Grid),
                    )
                    .icon(Icon::Grid)
                    .checked(layout.browser_view_mode == BrowserViewMode::Grid),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-view-list"),
                        Message::SetBrowserViewMode(BrowserViewMode::List),
                    )
                    .icon(Icon::List)
                    .checked(layout.browser_view_mode == BrowserViewMode::List),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-view-details"),
                        Message::SetBrowserViewMode(BrowserViewMode::Details),
                    )
                    .icon(Icon::Table)
                    .checked(layout.browser_view_mode == BrowserViewMode::Details),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-toggle-sidebar"),
                        Message::Dispatch(Command::ToggleLeftPanel),
                    )
                    .checked(layout.panel("browser.sidebar")),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-view-detail-panel"),
                        Message::ToggleBrowserInspector,
                    )
                    .checked(state.browser_inspector_open),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-theme-toggle"),
                        Message::Dispatch(Command::ToggleTheme),
                    )
                    .icon(if state.snapshot.meta.theme == Theme::Dark {
                        Icon::Sun
                    } else {
                        Icon::Moon
                    }),
                ),
            ],
        ),
        (Workspace::Browser, ActiveMenu::BrowserTools) => {
            let has_images = state.snapshot.entries.iter().any(|e| e.is_image);
            (
                Some(104.0),
                vec![
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "menu-tools-batch-resize"),
                            Message::OpenBatchDialog(BatchDialogKind::Resize),
                        )
                        .icon(Icon::Resize)
                        .enabled(has_images),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "menu-tools-batch-crop"),
                            Message::OpenBatchDialog(BatchDialogKind::Crop),
                        )
                        .icon(Icon::Crop)
                        .enabled(has_images),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "menu-tools-batch-convert"),
                            Message::OpenBatchDialog(BatchDialogKind::Convert),
                        )
                        .icon(Icon::Export)
                        .enabled(has_images),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "menu-tools-edit"),
                            Message::BrowserOpenSelectedInEditor,
                        )
                        .icon(Icon::Editor)
                        .enabled(has_selected_image),
                    ),
                ],
            )
        }
        (Workspace::Browser, ActiveMenu::BrowserPlugins) => (
            Some(156.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "menu-plugins-ai-disabled"), Message::Noop)
                        .enabled(false),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "menu-plugins-disabled"), Message::Noop).enabled(false),
                ),
            ],
        ),
        (Workspace::Browser, ActiveMenu::BrowserWindow) => {
            (Some(214.0), build_browser_window_menu_entries(state))
        }
        (Workspace::Browser, ActiveMenu::BrowserHelp) => {
            (Some(276.0), build_browser_help_menu_entries(lang))
        }
        (Workspace::Browser, ActiveMenu::BrowserSort) => {
            return Some((None, build_browser_sort_menu(state)));
        }
        (Workspace::Editor, ActiveMenu::EditorFile) => (
            Some(12.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-save"),
                        Message::Dispatch(Command::EditorSave),
                    )
                    .icon(Icon::Save)
                    .shortcut("Ctrl+S")
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-save-as"), Message::PickSaveAsDialog)
                        .icon(Icon::Save)
                        .shortcut("Ctrl+Shift+S")
                        .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-export"), Message::OpenExportDialog)
                        .icon(Icon::Export)
                        .shortcut("Ctrl+Shift+E")
                        .enabled(ed.is_some()),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-file-open-viewer"),
                        Message::RequestEditorLeave {
                            close_editor: false,
                        },
                    )
                    .icon(Icon::Viewer)
                    .shortcut("Esc"),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-file-open-browser"),
                        Message::RequestEditorLeaveToBrowser,
                    )
                    .icon(Icon::Folder),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-close"),
                        Message::RequestEditorLeave { close_editor: true },
                    )
                    .icon(Icon::Close)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-settings"), Message::OpenSettings)
                        .icon(Icon::Settings)
                        .shortcut("Ctrl+,"),
                ),
            ],
        ),
        (Workspace::Editor, ActiveMenu::EditorEdit) => (
            Some(54.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-undo"),
                        Message::Dispatch(Command::EditorUndo),
                    )
                    .icon(Icon::Undo)
                    .shortcut("Ctrl+Z")
                    .enabled(ed.is_some_and(|d| d.can_undo)),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-redo"),
                        Message::Dispatch(Command::EditorRedo),
                    )
                    .icon(Icon::Redo)
                    .shortcut("Ctrl+Y")
                    .enabled(ed.is_some_and(|d| d.can_redo)),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-reset"),
                        Message::Dispatch(Command::EditorReset),
                    )
                    .icon(Icon::Reset)
                    .enabled(ed.is_some_and(|d| d.modified)),
                ),
            ],
        ),
        (Workspace::Editor, ActiveMenu::EditorImage) => (
            Some(96.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-rotate-left"),
                        Message::Dispatch(Command::EditorRotate {
                            degrees: -90,
                            target: state.editor_ui.apply_target,
                        }),
                    )
                    .icon(Icon::RotateLeft)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-rotate-right"),
                        Message::Dispatch(Command::EditorRotate {
                            degrees: 90,
                            target: state.editor_ui.apply_target,
                        }),
                    )
                    .icon(Icon::RotateRight)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "tool-rotate-angle"),
                        Message::EditorSelectTool(EditorTool::RotateAngle),
                    )
                    .icon(Icon::RotateAngle)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-flip-h"),
                        Message::Dispatch(Command::EditorFlip {
                            axis: FlipAxis::Horizontal,
                            target: state.editor_ui.apply_target,
                        }),
                    )
                    .icon(Icon::FlipH)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-flip-v"),
                        Message::Dispatch(Command::EditorFlip {
                            axis: FlipAxis::Vertical,
                            target: state.editor_ui.apply_target,
                        }),
                    )
                    .icon(Icon::FlipV)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-crop"), Message::ToggleCropMode)
                        .icon(Icon::Crop)
                        .checked(state.crop_uv.is_some())
                        .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-resize"), Message::OpenResizeDialog)
                        .icon(Icon::Resize)
                        .shortcut("Ctrl+R")
                        .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "tool-canvas-size"),
                        Message::EditorSelectTool(EditorTool::CanvasSize),
                    )
                    .icon(Icon::CanvasSize)
                    .enabled(ed.is_some()),
                ),
            ],
        ),
        (Workspace::Editor, ActiveMenu::EditorFilter) => (
            Some(150.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-filter-color"),
                        Message::EditorOpenAdjustPopup(AdjustmentPopupTab::ColorAndLight),
                    )
                    .icon(Icon::Sliders)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-filter-hsv"),
                        Message::EditorOpenAdjustPopup(AdjustmentPopupTab::Hsv),
                    )
                    .icon(Icon::Palette)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-filter-balance"),
                        Message::EditorOpenAdjustPopup(AdjustmentPopupTab::ColorBalance),
                    )
                    .icon(Icon::ColorBalance)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-filter-curves"),
                        Message::EditorOpenAdjustPopup(AdjustmentPopupTab::Curves),
                    )
                    .icon(Icon::Curves)
                    .enabled(ed.is_some()),
                ),
            ],
        ),
        (Workspace::Editor, ActiveMenu::EditorView) => (
            Some(198.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-zoom-in"), Message::ZoomStep(1.25))
                        .icon(Icon::ZoomIn),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-zoom-out"), Message::ZoomStep(0.8))
                        .icon(Icon::ZoomOut),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-fit"),
                        Message::Canvas(CanvasEvent::ResetFit),
                    )
                    .icon(Icon::Fit),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-compare-original"),
                        Message::EditorCycleCompareMode,
                    )
                    .icon(Icon::Compare)
                    .checked(state.editor_ui.compare_mode != CompareMode::Off)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "label-compare-split"),
                        Message::EditorSetCompareMode(CompareMode::Split),
                    )
                    .checked(state.editor_ui.compare_mode == CompareMode::Split)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "label-compare-side-by-side"),
                        Message::EditorSetCompareMode(CompareMode::SideBySide),
                    )
                    .icon(Icon::ViewDouble)
                    .checked(state.editor_ui.compare_mode == CompareMode::SideBySide)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "label-compare-hold"),
                        Message::EditorSetCompareMode(CompareMode::HoldOriginal),
                    )
                    .checked(state.editor_ui.compare_mode == CompareMode::HoldOriginal)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-theme-toggle"),
                        Message::Dispatch(Command::ToggleTheme),
                    )
                    .icon(if state.snapshot.meta.theme == Theme::Dark {
                        Icon::Sun
                    } else {
                        Icon::Moon
                    }),
                ),
            ],
        ),
        (Workspace::Editor, ActiveMenu::EditorWindow) => (
            Some(248.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "panel-tools"), Message::ToggleEditorToolsVisible)
                        .checked(layout.panel("editor.tools")),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "panel-tool-options-history"),
                        Message::ToggleEditorPanelVisible(EditorRightPanel::ToolOptionsHistory),
                    )
                    .checked(state.editor_history_open),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "panel-layers"),
                        Message::ToggleEditorPanelVisible(EditorRightPanel::Layers),
                    )
                    .checked(state.editor_layers_open),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-fullscreen"), Message::ToggleFullscreen)
                        .icon(if state.is_fullscreen {
                            Icon::FullscreenExit
                        } else {
                            Icon::Fullscreen
                        })
                        .shortcut("F11")
                        .checked(state.is_fullscreen),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-reset-layout"), Message::RequestResetLayout)
                        .icon(Icon::Reset),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-settings"), Message::OpenSettings)
                        .icon(Icon::Settings)
                        .shortcut("Ctrl+,"),
                ),
            ],
        ),
        (Workspace::Editor, ActiveMenu::EditorHelp) => (
            Some(312.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-shortcuts"), Message::OpenShortcutsDialog)
                        .icon(Icon::Info)
                        .shortcut("?"),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-about"), Message::OpenAboutDialog)
                        .icon(Icon::Editor),
                ),
            ],
        ),
        (Workspace::Viewer, ActiveMenu::ReaderOptions) => (
            None,
            vec![
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-viewer-single"),
                        Message::SetViewerViewMode(ViewerViewMode::Single),
                    )
                    .icon(Icon::ViewSingle)
                    .shortcut("S")
                    .checked(state.viewer_mode == ViewerViewMode::Single),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-viewer-double"),
                        Message::SetViewerViewMode(ViewerViewMode::Double),
                    )
                    .icon(Icon::ViewDouble)
                    .shortcut("D")
                    .checked(state.viewer_mode == ViewerViewMode::Double),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-viewer-manhwa"),
                        Message::SetViewerViewMode(ViewerViewMode::Webtoon),
                    )
                    .icon(Icon::ViewManhwa)
                    .shortcut("M")
                    .checked(state.viewer_mode == ViewerViewMode::Webtoon),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-dir-ltr"), Message::SetMangaRtl(false))
                        .checked(!state.viewer_manga_rtl),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-dir-rtl"), Message::SetMangaRtl(true))
                        .checked(state.viewer_manga_rtl),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "label-separate-cover"),
                        Message::SetMangaSeparateCover(!state.viewer_manga_cover_separate),
                    )
                    .checked(state.viewer_manga_cover_separate),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        format!("{}: 0 px", tr(lang, "label-page-gap")),
                        Message::SetMangaPageGap(0),
                    )
                    .checked(state.viewer_manga_page_gap == 0),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        format!("{}: 8 px", tr(lang, "label-page-gap")),
                        Message::SetMangaPageGap(8),
                    )
                    .checked(state.viewer_manga_page_gap == 8),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        format!("{}: 16 px", tr(lang, "label-page-gap")),
                        Message::SetMangaPageGap(16),
                    )
                    .checked(state.viewer_manga_page_gap == 16),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        format!(
                            "{}: {}",
                            tr(lang, "label-fit-mode"),
                            tr(lang, "action-fit-auto")
                        ),
                        Message::SetWebtoonFitMode(WebtoonFitMode::Auto),
                    )
                    .checked(state.viewer_webtoon_fit_mode == WebtoonFitMode::Auto),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        format!(
                            "{}: {}",
                            tr(lang, "label-fit-mode"),
                            tr(lang, "action-fit-width")
                        ),
                        Message::SetWebtoonFitMode(WebtoonFitMode::Width),
                    )
                    .checked(state.viewer_webtoon_fit_mode == WebtoonFitMode::Width),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        format!(
                            "{}: {}",
                            tr(lang, "label-fit-mode"),
                            tr(lang, "action-fit-height")
                        ),
                        Message::SetWebtoonFitMode(WebtoonFitMode::Height),
                    )
                    .checked(state.viewer_webtoon_fit_mode == WebtoonFitMode::Height),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        format!(
                            "{}: {}",
                            tr(lang, "label-fit-mode"),
                            tr(lang, "action-fit-original")
                        ),
                        Message::SetWebtoonFitMode(WebtoonFitMode::Original),
                    )
                    .checked(state.viewer_webtoon_fit_mode == WebtoonFitMode::Original),
                ),
            ],
        ),
        _ => return None,
    };

    Some((offset, menu_panel_entries(entries, 230.0)))
}

fn setting_row<'a>(
    label: String,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        container(text(label).size(TEXT_SIZE_CONTROL)).width(Length::Fill),
        control.into(),
    ]
    .spacing(16)
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

fn build_settings_body(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let meta = &state.snapshot.meta;
    let is_dark = meta.theme == Theme::Dark;
    let sys_open_mode = meta.layout.system_open_window_mode;
    let vmode = meta.layout.viewer_view_mode;

    let appearance_card = section_card(
        tr(lang, "settings-section-general"),
        column![
            setting_row(
                tr(lang, "settings-theme"),
                row![
                    button::labeled(
                        tr(lang, "settings-theme-dark"),
                        Some(Icon::Moon),
                        ButtonVariant::Tab { active: is_dark },
                        Some(Message::Dispatch(Command::SetTheme { theme: Theme::Dark })),
                    ),
                    button::labeled(
                        tr(lang, "settings-theme-light"),
                        Some(Icon::Sun),
                        ButtonVariant::Tab { active: !is_dark },
                        Some(Message::Dispatch(Command::SetTheme {
                            theme: Theme::Light,
                        })),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            setting_row(
                tr(lang, "settings-language"),
                row![
                    button::labeled(
                        tr(lang, "lang-system"),
                        None,
                        ButtonVariant::Tab {
                            active: lang == Language::System
                        },
                        Some(Message::Dispatch(Command::SetLanguage {
                            language: Language::System,
                        })),
                    ),
                    button::labeled(
                        "EN",
                        None,
                        ButtonVariant::Tab {
                            active: lang == Language::English
                        },
                        Some(Message::Dispatch(Command::SetLanguage {
                            language: Language::English,
                        })),
                    ),
                    button::labeled(
                        "VI",
                        None,
                        ButtonVariant::Tab {
                            active: lang == Language::Vietnamese
                        },
                        Some(Message::Dispatch(Command::SetLanguage {
                            language: Language::Vietnamese,
                        })),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
        ]
        .spacing(10),
    );

    let viewer_card = section_card(
        tr(lang, "settings-section-viewer"),
        column![
            setting_row(
                tr(lang, "settings-system-open"),
                row![
                    button::labeled(
                        tr(lang, "settings-open-fit"),
                        None,
                        ButtonVariant::Tab {
                            active: sys_open_mode == SystemOpenWindowMode::FitImage,
                        },
                        Some(Message::SetSystemOpenWindowMode(
                            SystemOpenWindowMode::FitImage,
                        )),
                    ),
                    button::labeled(
                        tr(lang, "settings-open-fullscreen"),
                        None,
                        ButtonVariant::Tab {
                            active: sys_open_mode == SystemOpenWindowMode::Fullscreen,
                        },
                        Some(Message::SetSystemOpenWindowMode(
                            SystemOpenWindowMode::Fullscreen,
                        )),
                    ),
                    button::labeled(
                        tr(lang, "settings-open-remembered"),
                        None,
                        ButtonVariant::Tab {
                            active: sys_open_mode == SystemOpenWindowMode::Remembered,
                        },
                        Some(Message::SetSystemOpenWindowMode(
                            SystemOpenWindowMode::Remembered,
                        )),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            setting_row(
                tr(lang, "settings-default-viewer-mode"),
                row![
                    button::labeled(
                        tr(lang, "action-viewer-single"),
                        Some(Icon::ViewSingle),
                        ButtonVariant::Tab {
                            active: vmode == ViewerViewMode::Single,
                        },
                        Some(Message::SetDefaultViewerViewMode(ViewerViewMode::Single)),
                    ),
                    button::labeled(
                        tr(lang, "action-viewer-double"),
                        Some(Icon::ViewDouble),
                        ButtonVariant::Tab {
                            active: vmode == ViewerViewMode::Double,
                        },
                        Some(Message::SetDefaultViewerViewMode(ViewerViewMode::Double)),
                    ),
                    button::labeled(
                        tr(lang, "action-viewer-manhwa"),
                        Some(Icon::ViewManhwa),
                        ButtonVariant::Tab {
                            active: vmode == ViewerViewMode::Webtoon,
                        },
                        Some(Message::SetDefaultViewerViewMode(ViewerViewMode::Webtoon)),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
        ]
        .spacing(10),
    );

    let behavior_card = section_card(
        tr(lang, "settings-section-behavior"),
        column![
            setting_row(
                tr(lang, "settings-remember-folder"),
                row![
                    button::labeled(
                        tr(lang, "action-on"),
                        None,
                        ButtonVariant::Tab {
                            active: meta.remember_last_folder,
                        },
                        Some(Message::Dispatch(Command::SetSettings {
                            theme: meta.theme,
                            language: meta.language,
                            remember_last_folder: true,
                            hardware_acceleration: meta.hardware_acceleration,
                        })),
                    ),
                    button::labeled(
                        tr(lang, "action-off"),
                        None,
                        ButtonVariant::Tab {
                            active: !meta.remember_last_folder,
                        },
                        Some(Message::Dispatch(Command::SetSettings {
                            theme: meta.theme,
                            language: meta.language,
                            remember_last_folder: false,
                            hardware_acceleration: meta.hardware_acceleration,
                        })),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            setting_row(
                tr(lang, "settings-hw-accel"),
                row![
                    button::labeled(
                        tr(lang, "action-on"),
                        None,
                        ButtonVariant::Tab {
                            active: meta.hardware_acceleration,
                        },
                        Some(Message::Dispatch(Command::SetSettings {
                            theme: meta.theme,
                            language: meta.language,
                            remember_last_folder: meta.remember_last_folder,
                            hardware_acceleration: true,
                        })),
                    ),
                    button::labeled(
                        tr(lang, "action-off"),
                        None,
                        ButtonVariant::Tab {
                            active: !meta.hardware_acceleration,
                        },
                        Some(Message::Dispatch(Command::SetSettings {
                            theme: meta.theme,
                            language: meta.language,
                            remember_last_folder: meta.remember_last_folder,
                            hardware_acceleration: false,
                        })),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
        ]
        .spacing(10),
    );

    let current_folder = meta.current_folder.as_deref();
    let can_pin_current =
        current_folder.is_some_and(|cur| !meta.favorites.iter().any(|fav| fav == cur));
    let mut fav_list_col = column![].spacing(6);
    if meta.favorites.is_empty() {
        fav_list_col = fav_list_col.push(
            text(tr(lang, "settings-quick-access-empty"))
                .size(TEXT_SIZE_CONTROL)
                .style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.text_muted),
                    }
                }),
        );
    } else {
        let fav_len = meta.favorites.len();
        for (idx, fav_path) in meta.favorites.iter().enumerate() {
            let folder_name = Path::new(fav_path)
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(fav_path.as_str())
                .to_owned();
            fav_list_col = fav_list_col.push(
                row![
                    container(
                        column![
                            text(folder_name).size(TEXT_SIZE_CONTROL),
                            text(fav_path.clone()).size(11).style(|theme| {
                                let t = PaletteTokens::for_theme(theme);
                                text::Style {
                                    color: Some(t.text_muted),
                                }
                            }),
                        ]
                        .spacing(1),
                    )
                    .width(Length::Fill),
                    button::compact_icon_tipped(
                        Icon::ArrowUp,
                        ButtonVariant::Ghost,
                        (idx > 0).then(|| {
                            Message::Dispatch(Command::MoveFavorite {
                                path: fav_path.clone(),
                                direction: MoveDirection::Up,
                            })
                        }),
                        tr(lang, "action-move-up"),
                    ),
                    button::compact_icon_tipped(
                        Icon::ArrowDown,
                        ButtonVariant::Ghost,
                        (idx + 1 < fav_len).then(|| {
                            Message::Dispatch(Command::MoveFavorite {
                                path: fav_path.clone(),
                                direction: MoveDirection::Down,
                            })
                        }),
                        tr(lang, "action-move-down"),
                    ),
                    button::compact_icon_tipped(
                        Icon::Trash,
                        ButtonVariant::Ghost,
                        Some(Message::Dispatch(Command::RemoveFavorite {
                            path: fav_path.clone(),
                        })),
                        tr(lang, "action-unpin-quick-access"),
                    ),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            );
        }
    }
    fav_list_col = fav_list_col.push(
        row![
            button::labeled(
                tr(lang, "settings-quick-access-add"),
                Some(Icon::Plus),
                ButtonVariant::Secondary,
                Some(Message::PickFavoriteFolderDialog),
            ),
            button::labeled(
                tr(lang, "action-pin-quick-access"),
                Some(Icon::Star),
                ButtonVariant::Ghost,
                can_pin_current.then(|| Message::Dispatch(Command::AddFavorite {
                    path: current_folder.unwrap_or_default().to_owned(),
                })),
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    let quick_access_card = section_card(tr(lang, "panel-quick-access"), fav_list_col);

    column![
        appearance_card,
        viewer_card,
        behavior_card,
        quick_access_card
    ]
    .spacing(10)
    .into()
}

pub(crate) fn default_window_state_for(id: DesktopDialogId) -> DialogWindowState {
    match id {
        DesktopDialogId::Settings => DialogWindowState::new(600.0, 580.0)
            .min_size(460.0, 360.0)
            .max_size(960.0, 900.0),
        DesktopDialogId::NewFolder | DesktopDialogId::RenameItem => {
            DialogWindowState::new(440.0, 185.0).min_size(320.0, 160.0)
        }
        DesktopDialogId::ConfirmResetLayout | DesktopDialogId::ConfirmDeletePermanently => {
            DialogWindowState::new(440.0, 168.0).min_size(320.0, 148.0)
        }
        DesktopDialogId::ConfirmUnsavedLeave => {
            DialogWindowState::new(480.0, 170.0).min_size(340.0, 150.0)
        }
        DesktopDialogId::Resize => DialogWindowState::new(500.0, 400.0).min_size(380.0, 320.0),
        DesktopDialogId::Export => DialogWindowState::new(500.0, 335.0).min_size(380.0, 280.0),
        DesktopDialogId::ColorPicker => DialogWindowState::new(440.0, 390.0).min_size(360.0, 320.0),
        DesktopDialogId::BatchResize => DialogWindowState::new(580.0, 640.0)
            .min_size(460.0, 420.0)
            .max_size(960.0, 920.0),
        DesktopDialogId::BatchCrop => DialogWindowState::new(560.0, 500.0)
            .min_size(440.0, 380.0)
            .max_size(960.0, 920.0),
        DesktopDialogId::BatchConvert => DialogWindowState::new(520.0, 495.0)
            .min_size(420.0, 360.0)
            .max_size(960.0, 920.0),
        DesktopDialogId::Shortcuts => DialogWindowState::new(480.0, 225.0).min_size(340.0, 180.0),
        DesktopDialogId::About | DesktopDialogId::Alert => {
            DialogWindowState::new(440.0, 180.0).min_size(320.0, 150.0)
        }
        DesktopDialogId::AdjustColor => DialogWindowState::new(460.0, 480.0)
            .min_size(360.0, 320.0)
            .max_size(760.0, 860.0),
        DesktopDialogId::AdjustHsv => DialogWindowState::new(440.0, 305.0)
            .min_size(340.0, 240.0)
            .max_size(760.0, 720.0),
        DesktopDialogId::AdjustColorBalance => DialogWindowState::new(460.0, 365.0)
            .min_size(360.0, 280.0)
            .max_size(760.0, 760.0),
        DesktopDialogId::AdjustCurves => DialogWindowState::new(460.0, 255.0)
            .min_size(360.0, 220.0)
            .max_size(760.0, 760.0),
    }
}

const ALL_ADJUSTMENT_DIALOG_IDS: [DesktopDialogId; 4] = [
    DesktopDialogId::AdjustColor,
    DesktopDialogId::AdjustHsv,
    DesktopDialogId::AdjustColorBalance,
    DesktopDialogId::AdjustCurves,
];

const fn adjustment_dialog_id(tab: AdjustmentPopupTab) -> DesktopDialogId {
    match tab {
        AdjustmentPopupTab::ColorAndLight => DesktopDialogId::AdjustColor,
        AdjustmentPopupTab::Hsv => DesktopDialogId::AdjustHsv,
        AdjustmentPopupTab::ColorBalance => DesktopDialogId::AdjustColorBalance,
        AdjustmentPopupTab::Curves => DesktopDialogId::AdjustCurves,
    }
}

fn close_all_adjustment_dialogs(state: &mut PhotonGui) {
    for id in ALL_ADJUSTMENT_DIALOG_IDS {
        state.dialog_manager.close(&id);
    }
}

fn sync_editor_panel_compat(state: &mut PhotonGui) {
    state.editor_panel_order = state.editor_panels.order().to_vec();
    state.dragging_editor_panel = state.editor_panels.dragging();
    state.editor_panel_drop_target = state.editor_panels.drop_target();
}

fn toggle_browser_right_panel(state: &mut PhotonGui, panel: BrowserRightPanel) -> Task<Message> {
    match panel {
        BrowserRightPanel::Preview => state.browser_preview_open = !state.browser_preview_open,
        BrowserRightPanel::Info => state.browser_info_open = !state.browser_info_open,
    }
    state.browser_inspector_open = state.browser_preview_open || state.browser_info_open;
    let layout_task = commit_layout_with(state, |_| {});
    Task::batch([layout_task, sync_ui_tree_task(state)])
}

fn apply_live_layout_fields(state: &PhotonGui, layout: &mut photon_protocol::LayoutState) {
    let any_browser_right = state.browser_preview_open || state.browser_info_open;
    layout
        .panels
        .insert("browser.inspector".to_owned(), any_browser_right);
    layout
        .panels
        .insert("browser.preview".to_owned(), state.browser_preview_open);
    layout
        .panels
        .insert("browser.info".to_owned(), state.browser_info_open);
    layout.panels.insert(
        "browser.quick_access".to_owned(),
        state.browser_quick_access_open,
    );
    layout
        .panels
        .insert("browser.explorer".to_owned(), state.browser_explorer_open);
    layout
        .panels
        .insert("viewer.inspector".to_owned(), state.viewer_info_open);
    layout
        .panels
        .insert("viewer.info".to_owned(), state.viewer_info_open);
    layout.panels.insert(
        "viewer.adjustments".to_owned(),
        state.viewer_adjustments_open,
    );
    layout
        .panels
        .insert("viewer.filmstrip".to_owned(), state.viewer_filmstrip_open);
    layout
        .panels
        .insert("editor.history".to_owned(), state.editor_history_open);
    layout
        .panels
        .insert("editor.layers".to_owned(), state.editor_layers_open);
    layout.browser_left_panel_order = state.browser_left_panels.order().to_vec();
    layout.browser_right_panel_order = state.browser_right_panels.order().to_vec();
    layout.viewer_right_panel_order = state.viewer_right_panels.order().to_vec();
    layout.editor_tool_tab = state.editor_tool_tab;
    layout.editor_right_panel_order = state.editor_panels.order().to_vec();
}

fn editor_status_summary(state: &PhotonGui, lang: Language) -> String {
    if let Some(ed) = &state.snapshot.meta.editor {
        let modified_mark = if ed.modified { " *" } else { "" };
        format!("{}{} · {}×{}", ed.name, modified_mark, ed.width, ed.height)
    } else {
        let s = tr_ui_text(lang, &state.snapshot.meta.status);
        if s.is_empty() { tr(lang, "ready") } else { s }
    }
}

fn open_dialog_window(state: &mut PhotonGui, id: DesktopDialogId) {
    state
        .dialog_manager
        .open_with(id, default_window_state_for(id), false);
}

fn commit_layout_with(
    state: &mut PhotonGui,
    mutate: impl FnOnce(&mut photon_protocol::LayoutState),
) -> Task<Message> {
    let mut layout = state.snapshot.meta.layout.clone();
    apply_live_layout_fields(state, &mut layout);
    mutate(&mut layout);
    Arc::make_mut(&mut state.snapshot).meta.layout = layout.clone();
    state.pending_layout_commands += 1;
    let core = state.core.clone();
    Task::perform(
        async move {
            let env = Envelope::new(
                format!("gui-{}", std::process::id()),
                ClientKind::Gui,
                Command::SetLayout { layout },
            );
            core.dispatch(env).await
        },
        Message::LayoutCommandCompleted,
    )
}

fn dialog_id_for_in_app(dialog: &InAppDialog) -> DesktopDialogId {
    match dialog {
        InAppDialog::NewFolder { .. } => DesktopDialogId::NewFolder,
        InAppDialog::ConfirmResetLayout => DesktopDialogId::ConfirmResetLayout,
        InAppDialog::ConfirmUnsavedLeave { .. } => DesktopDialogId::ConfirmUnsavedLeave,
        InAppDialog::Resize { .. } => DesktopDialogId::Resize,
        InAppDialog::Export { .. } => DesktopDialogId::Export,
        InAppDialog::ColorPicker { .. } => DesktopDialogId::ColorPicker,
        InAppDialog::Batch(b) => b.kind.dialog_id(),
        InAppDialog::RenameItem { .. } => DesktopDialogId::RenameItem,
        InAppDialog::ConfirmDeletePermanently { .. } => DesktopDialogId::ConfirmDeletePermanently,
        InAppDialog::Alert { id, .. } => *id,
    }
}

pub(crate) fn resolved_editor_canvas_bounds(state: &PhotonGui) -> Rectangle {
    let layout = &state.snapshot.meta.layout;
    let left_w = if layout.panel("editor.tools") {
        97.0
    } else {
        0.0
    };
    let right_w = if layout.panel("editor.inspector") {
        6.0 + state.dock_drag.effective_editor_right(layout)
    } else {
        0.0
    };
    let top_h = 47.0;
    let bottom_h = 31.0;
    let w = (state.window_size.width - left_w - right_w).max(160.0);
    let h = (state.window_size.height - top_h - bottom_h).max(120.0);
    Rectangle::new(Point::new(left_w, top_h), Size::new(w, h))
}

fn dialog_spec_to_managed<'a>(
    id: DesktopDialogId,
    spec: DialogSpec<'a, Message>,
    defaults: DialogWindowState,
    overlay: OverlayMode,
    lang: Language,
    theme: &IcedTheme,
) -> ManagedDialogSpec<'a, DesktopDialogId, Message> {
    let close_btn = with_tooltip_pos(
        button::icon_only(
            Icon::Close,
            ButtonVariant::Ghost,
            Some(spec.on_close.clone()),
        ),
        tr(lang, "tooltip-dialog-close"),
        TooltipPosition::Left,
    );
    ManagedDialogSpec {
        id,
        open: spec.open,
        title: spec.title,
        defaults,
        resizable: true,
        draggable: true,
        overlay,
        body: spec.body,
        footer: Some(spec.footer),
        close_button: Some(close_btn),
        style: dialog_style_for_theme(theme),
        on_event: Box::new(Message::DialogEvent),
        on_close: spec.on_close,
        on_noop: spec.on_noop,
    }
}

fn build_editor_adjustment_dialog<'a>(
    state: &'a PhotonGui,
    active_tab: AdjustmentPopupTab,
    theme: &IcedTheme,
) -> ManagedDialogSpec<'a, DesktopDialogId, Message> {
    let lang = state.snapshot.meta.language;
    let ui = &state.editor_ui;
    let adj = &state.pending_adj;

    let target_row = row![
        text(tr(lang, "label-apply-target")).size(11),
        iced::widget::space::horizontal(),
        button::labeled(
            tr(lang, "target-active-layer"),
            None,
            ButtonVariant::Tab {
                active: ui.apply_target == ApplyTarget::ActiveLayer,
            },
            Some(Message::EditorSetApplyTarget(ApplyTarget::ActiveLayer)),
        ),
        button::labeled(
            tr(lang, "target-whole-image"),
            None,
            ButtonVariant::Tab {
                active: ui.apply_target == ApplyTarget::WholeImage,
            },
            Some(Message::EditorSetApplyTarget(ApplyTarget::WholeImage)),
        ),
        button::labeled(
            tr(lang, "target-new-layer"),
            None,
            ButtonVariant::Tab {
                active: ui.apply_target == ApplyTarget::NewLayer,
            },
            Some(Message::EditorSetApplyTarget(ApplyTarget::NewLayer)),
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let kv = |label: String, val: String| {
        row![
            text(label).size(12),
            iced::widget::space::horizontal(),
            text(val).size(12),
        ]
        .align_y(Alignment::Center)
    };

    let tab_body: Element<'a, Message> = match active_tab {
        AdjustmentPopupTab::ColorAndLight => column![
            kv(tr(lang, "label-exposure"), format!("{:.2}", adj.exposure)),
            slider(-2.0..=2.0, adj.exposure, Message::SetExposure)
                .step(0.05_f32)
                .style(slider_style),
            kv(tr(lang, "label-gamma"), format!("{:.2}", adj.gamma)),
            slider(0.2..=3.0, adj.gamma, Message::SetGamma)
                .step(0.05_f32)
                .style(slider_style),
            kv(
                tr(lang, "label-highlights"),
                format!("{:.2}", adj.highlights)
            ),
            slider(-1.0..=1.0, adj.highlights, Message::SetHighlights)
                .step(0.05_f32)
                .style(slider_style),
            kv(tr(lang, "label-shadows"), format!("{:.2}", adj.shadows)),
            slider(-1.0..=1.0, adj.shadows, Message::SetShadows)
                .step(0.05_f32)
                .style(slider_style),
            kv(
                tr(lang, "label-brightness"),
                format!("{:.2}", adj.brightness)
            ),
            slider(-0.5..=0.5, adj.brightness, Message::SetBrightness)
                .step(0.02_f32)
                .style(slider_style),
            kv(tr(lang, "label-contrast"), format!("{:.2}", adj.contrast)),
            slider(-0.5..=0.5, adj.contrast, Message::SetContrast)
                .step(0.02_f32)
                .style(slider_style),
            kv(
                tr(lang, "label-saturation"),
                format!("{:.2}", adj.saturation)
            ),
            slider(-1.0..=1.0, adj.saturation, Message::SetSaturation)
                .step(0.05_f32)
                .style(slider_style),
        ]
        .spacing(5)
        .into(),
        AdjustmentPopupTab::Hsv => column![
            kv(tr(lang, "label-hue"), format!("{:.0}°", ui.hsv.hue)),
            slider(-180.0..=180.0, ui.hsv.hue, Message::EditorSetHsvHue)
                .step(1.0_f32)
                .style(slider_style),
            kv(
                tr(lang, "label-saturation"),
                format!("{:.2}", ui.hsv.saturation)
            ),
            slider(-1.0..=1.0, ui.hsv.saturation, Message::EditorSetHsvSat)
                .step(0.05_f32)
                .style(slider_style),
            kv(tr(lang, "label-value"), format!("{:.2}", ui.hsv.value)),
            slider(-1.0..=1.0, ui.hsv.value, Message::EditorSetHsvVal)
                .step(0.05_f32)
                .style(slider_style),
        ]
        .spacing(6)
        .into(),
        AdjustmentPopupTab::ColorBalance => {
            let zone_vals = match ui.balance_zone {
                ColorBalanceZone::Shadows => ui.balance.shadows,
                ColorBalanceZone::Midtones => ui.balance.midtones,
                ColorBalanceZone::Highlights => ui.balance.highlights,
            };
            column![
                row![
                    button::labeled(
                        tr(lang, "label-shadows"),
                        None,
                        ButtonVariant::Tab {
                            active: ui.balance_zone == ColorBalanceZone::Shadows,
                        },
                        Some(Message::EditorSetBalanceZone(ColorBalanceZone::Shadows)),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "label-midtones"),
                        None,
                        ButtonVariant::Tab {
                            active: ui.balance_zone == ColorBalanceZone::Midtones,
                        },
                        Some(Message::EditorSetBalanceZone(ColorBalanceZone::Midtones)),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "label-highlights"),
                        None,
                        ButtonVariant::Tab {
                            active: ui.balance_zone == ColorBalanceZone::Highlights,
                        },
                        Some(Message::EditorSetBalanceZone(ColorBalanceZone::Highlights)),
                    )
                    .width(Fill),
                ]
                .spacing(4),
                kv("Cyan / Red".to_owned(), format!("{:.2}", zone_vals[0])),
                slider(-1.0..=1.0, zone_vals[0], |v| {
                    Message::EditorSetBalanceAxis(0, v)
                })
                .step(0.02_f32)
                .style(slider_style),
                kv("Magenta / Green".to_owned(), format!("{:.2}", zone_vals[1])),
                slider(-1.0..=1.0, zone_vals[1], |v| {
                    Message::EditorSetBalanceAxis(1, v)
                })
                .step(0.02_f32)
                .style(slider_style),
                kv("Yellow / Blue".to_owned(), format!("{:.2}", zone_vals[2])),
                slider(-1.0..=1.0, zone_vals[2], |v| {
                    Message::EditorSetBalanceAxis(2, v)
                })
                .step(0.02_f32)
                .style(slider_style),
                checkbox(ui.balance.preserve_luminosity)
                    .label(tr(lang, "label-preserve-luminosity"))
                    .text_size(TEXT_SIZE_CONTROL)
                    .on_toggle(Message::EditorTogglePreserveLuminosity),
            ]
            .spacing(6)
            .into()
        }
        AdjustmentPopupTab::Curves => {
            let pts_count = match ui.curves_channel {
                CurvesChannel::Rgb => ui.curves.rgb.len(),
                CurvesChannel::Red => ui.curves.red.len(),
                CurvesChannel::Green => ui.curves.green.len(),
                CurvesChannel::Blue => ui.curves.blue.len(),
            };
            column![
                row![
                    button::labeled(
                        tr(lang, "channel-rgb"),
                        None,
                        ButtonVariant::Tab {
                            active: ui.curves_channel == CurvesChannel::Rgb,
                        },
                        Some(Message::EditorSelectCurvesChannel(CurvesChannel::Rgb)),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "channel-red"),
                        None,
                        ButtonVariant::Tab {
                            active: ui.curves_channel == CurvesChannel::Red,
                        },
                        Some(Message::EditorSelectCurvesChannel(CurvesChannel::Red)),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "channel-green"),
                        None,
                        ButtonVariant::Tab {
                            active: ui.curves_channel == CurvesChannel::Green,
                        },
                        Some(Message::EditorSelectCurvesChannel(CurvesChannel::Green)),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "channel-blue"),
                        None,
                        ButtonVariant::Tab {
                            active: ui.curves_channel == CurvesChannel::Blue,
                        },
                        Some(Message::EditorSelectCurvesChannel(CurvesChannel::Blue)),
                    )
                    .width(Fill),
                ]
                .spacing(4),
                text(format!("{pts_count} control points"))
                    .size(11)
                    .style(|th| text::Style {
                        color: Some(PaletteTokens::for_theme(th).text_muted),
                    }),
                row![
                    button::labeled(
                        "Linear",
                        None,
                        ButtonVariant::Secondary,
                        Some(Message::EditorCurvesChanged(vec![[0.0, 0.0], [1.0, 1.0]])),
                    ),
                    button::labeled(
                        "S-Curve",
                        None,
                        ButtonVariant::Secondary,
                        Some(Message::EditorCurvesChanged(vec![
                            [0.0, 0.0],
                            [0.25, 0.18],
                            [0.75, 0.82],
                            [1.0, 1.0],
                        ])),
                    ),
                    button::labeled(
                        "Fade",
                        None,
                        ButtonVariant::Secondary,
                        Some(Message::EditorCurvesChanged(vec![
                            [0.0, 0.08],
                            [0.5, 0.52],
                            [1.0, 0.96],
                        ])),
                    ),
                ]
                .spacing(6),
            ]
            .spacing(8)
            .into()
        }
    };

    let body = column![target_row, tab_body].spacing(10);
    let footer = row![
        with_tooltip_pos(
            button::labeled(
                tr(lang, "action-reset"),
                Some(Icon::Reset),
                ButtonVariant::Ghost,
                Some(Message::EditorResetPendingAdjustments),
            ),
            tr(lang, "action-reset"),
            TooltipPosition::Top,
        ),
        iced::widget::space::horizontal(),
        with_tooltip_pos(
            button::labeled(
                tr(lang, "action-cancel"),
                None,
                ButtonVariant::Secondary,
                Some(Message::EditorCloseAdjustPopup { apply: false }),
            ),
            tr(lang, "tooltip-dialog-close"),
            TooltipPosition::Top,
        ),
        with_tooltip_pos(
            button::labeled(
                tr(lang, "action-apply"),
                Some(Icon::Check),
                ButtonVariant::Primary,
                Some(Message::EditorCloseAdjustPopup { apply: true }),
            ),
            tr(lang, "action-apply"),
            TooltipPosition::Top,
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let close_btn = with_tooltip_pos(
        button::icon_only(
            Icon::Close,
            ButtonVariant::Ghost,
            Some(Message::EditorCloseAdjustPopup { apply: false }),
        ),
        tr(lang, "tooltip-dialog-close"),
        TooltipPosition::Left,
    );

    let id = adjustment_dialog_id(active_tab);
    let title = match active_tab {
        AdjustmentPopupTab::ColorAndLight => tr(lang, "dialog-adjust-color"),
        AdjustmentPopupTab::Hsv => tr(lang, "dialog-adjust-hsv"),
        AdjustmentPopupTab::ColorBalance => tr(lang, "dialog-adjust-balance"),
        AdjustmentPopupTab::Curves => tr(lang, "dialog-adjust-curves"),
    };

    ManagedDialogSpec {
        id,
        open: true,
        title,
        defaults: default_window_state_for(id),
        resizable: true,
        draggable: true,
        overlay: OverlayMode::Cutout {
            cutout: resolved_editor_canvas_bounds(state),
            dim_alpha: 0.42,
            lock_outside: true,
        },
        body: body.into(),
        footer: Some(footer.into()),
        close_button: Some(close_btn),
        style: dialog_style_for_theme(theme),
        on_event: Box::new(Message::DialogEvent),
        on_close: Message::EditorCloseAdjustPopup { apply: false },
        on_noop: Message::Noop,
    }
}

fn build_in_app_dialog<'a>(
    state: &'a PhotonGui,
    dialog: &'a InAppDialog,
    theme: &IcedTheme,
) -> ManagedDialogSpec<'a, DesktopDialogId, Message> {
    let lang = state.snapshot.meta.language;
    let id = dialog_id_for_in_app(dialog);
    let overlay = OverlayMode::default();
    match dialog {
        InAppDialog::NewFolder {
            parent,
            name,
            error,
        } => dialog_spec_to_managed(
            id,
            prompt_dialog_spec(
                PromptDialogSpec::new(
                    tr(lang, "dialog-new-folder-title"),
                    tr(lang, "dialog-new-folder-placeholder"),
                    name.clone(),
                    tr(lang, "dialog-create"),
                    tr(lang, "dialog-cancel"),
                    Message::NewFolderNameChanged,
                    Message::ConfirmNewFolder,
                    Message::CloseDialog,
                    Message::Noop,
                )
                .label(parent.clone())
                .error(error.clone()),
            ),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::ConfirmResetLayout => dialog_spec_to_managed(
            id,
            confirm_dialog_spec(ConfirmDialogSpec {
                title: tr(lang, "dialog-reset-title"),
                message: tr(lang, "dialog-reset-body"),
                confirm_label: tr(lang, "action-reset-layout"),
                cancel_label: tr(lang, "dialog-cancel"),
                danger: true,
                secondary_action: None,
                on_confirm: Message::ConfirmResetLayout,
                on_cancel: Message::CloseDialog,
                on_noop: Message::Noop,
            }),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::ConfirmUnsavedLeave { close_editor } => {
            let doc_name = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .map(|d| d.name.clone())
                .unwrap_or_default();
            let body = tr_ui_text(
                lang,
                &UiText::with("dialog_unsaved_body", [("name", doc_name)]),
            );
            dialog_spec_to_managed(
                id,
                confirm_dialog_spec(ConfirmDialogSpec {
                    title: tr(lang, "dialog-unsaved-title"),
                    message: body,
                    confirm_label: tr(lang, "dialog-save-and-leave"),
                    cancel_label: tr(lang, "dialog-cancel"),
                    danger: true,
                    secondary_action: Some((
                        tr(lang, "dialog-discard"),
                        Message::DiscardAndLeaveEditor(*close_editor),
                    )),
                    on_confirm: Message::SaveAndLeaveEditor(*close_editor),
                    on_cancel: Message::CloseDialog,
                    on_noop: Message::Noop,
                }),
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::Resize {
            dims,
            filter,
            mode,
            error,
            ..
        } => {
            let mut body_col = column![
                dims.view(lang, Message::ResizeDimAction),
                column![
                    text(tr(lang, "label-resize-mode")).size(12),
                    row![
                        button::labeled(
                            tr(lang, "mode-stretch"),
                            None,
                            ButtonVariant::Tab {
                                active: *mode == ResizeMode::Stretch,
                            },
                            Some(Message::ResizeModeChanged(ResizeMode::Stretch)),
                        ),
                        button::labeled(
                            tr(lang, "mode-cover"),
                            None,
                            ButtonVariant::Tab {
                                active: *mode == ResizeMode::Cover,
                            },
                            Some(Message::ResizeModeChanged(ResizeMode::Cover)),
                        ),
                        button::labeled(
                            tr(lang, "mode-pad"),
                            None,
                            ButtonVariant::Tab {
                                active: *mode == ResizeMode::Pad,
                            },
                            Some(Message::ResizeModeChanged(ResizeMode::Pad)),
                        ),
                    ]
                    .spacing(4),
                ]
                .spacing(6),
                column![
                    text(tr(lang, "label-filter")).size(12),
                    row![
                        button::labeled(
                            "Lanczos3",
                            None,
                            ButtonVariant::Tab {
                                active: *filter == ResizeFilter::Lanczos3,
                            },
                            Some(Message::ResizeFilterChanged(ResizeFilter::Lanczos3)),
                        ),
                        button::labeled(
                            "CatmullRom",
                            None,
                            ButtonVariant::Tab {
                                active: *filter == ResizeFilter::CatmullRom,
                            },
                            Some(Message::ResizeFilterChanged(ResizeFilter::CatmullRom)),
                        ),
                        button::labeled(
                            "Triangle",
                            None,
                            ButtonVariant::Tab {
                                active: *filter == ResizeFilter::Triangle,
                            },
                            Some(Message::ResizeFilterChanged(ResizeFilter::Triangle)),
                        ),
                        button::labeled(
                            "Nearest",
                            None,
                            ButtonVariant::Tab {
                                active: *filter == ResizeFilter::Nearest,
                            },
                            Some(Message::ResizeFilterChanged(ResizeFilter::Nearest)),
                        ),
                    ]
                    .spacing(4),
                ]
                .spacing(6),
            ]
            .spacing(12);

            if let Some(err) = error {
                body_col = body_col.push(text(err.clone()).size(12).style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.danger),
                    }
                }));
            }

            let footer = row![
                button::labeled(
                    tr(lang, "dialog-cancel"),
                    None,
                    ButtonVariant::Ghost,
                    Some(Message::CloseDialog),
                ),
                iced::widget::space::horizontal(),
                button::labeled(
                    tr(lang, "action-resize"),
                    Some(Icon::Check),
                    ButtonVariant::Primary,
                    Some(Message::ConfirmResize),
                ),
            ]
            .align_y(Alignment::Center);

            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-resize-title"),
                    width: 460.0,
                    body: body_col.into(),
                    footer: footer.into(),
                    on_close: Message::CloseDialog,
                    on_noop: Message::Noop,
                },
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::Batch(batch_state) => match batch_state.kind {
            BatchDialogKind::Resize => build_batch_resize_dialog(state, batch_state, theme),
            BatchDialogKind::Crop => build_batch_crop_dialog(state, batch_state, theme),
            BatchDialogKind::Convert => build_batch_convert_dialog(state, batch_state, theme),
        },
        InAppDialog::RenameItem { path, name, error } => dialog_spec_to_managed(
            id,
            prompt_dialog_spec(
                PromptDialogSpec::new(
                    tr(lang, "dialog-rename-title"),
                    tr(lang, "action-rename"),
                    name.clone(),
                    tr(lang, "action-rename"),
                    tr(lang, "dialog-cancel"),
                    Message::BrowserRenameChanged,
                    Message::BrowserConfirmRename,
                    Message::CloseDialog,
                    Message::Noop,
                )
                .label(path.clone())
                .error(error.clone()),
            ),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::ConfirmDeletePermanently { paths } => dialog_spec_to_managed(
            id,
            confirm_dialog_spec(ConfirmDialogSpec {
                title: tr(lang, "dialog-delete-title"),
                message: format!("{} ({})", tr(lang, "dialog-delete-body"), paths.len()),
                confirm_label: tr(lang, "action-delete-permanently"),
                cancel_label: tr(lang, "dialog-cancel"),
                danger: true,
                secondary_action: None,
                on_confirm: Message::BrowserExecuteDeletePermanently(paths.clone()),
                on_cancel: Message::CloseDialog,
                on_noop: Message::Noop,
            }),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::Export {
            path,
            quality,
            max_edge,
            metadata,
            error,
        } => dialog_spec_to_managed(
            id,
            build_export_dialog(lang, path, *quality, max_edge, *metadata, error.as_deref()),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::ColorPicker {
            target,
            r,
            g,
            b,
            hex,
            transparent,
            ..
        } => dialog_spec_to_managed(
            id,
            build_color_picker_dialog(lang, *target, *r, *g, *b, hex, *transparent),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::Alert { title, message, .. } => dialog_spec_to_managed(
            id,
            alert_dialog_spec(
                title.clone(),
                message.clone(),
                tr(lang, "action-close"),
                Message::CloseDialog,
                Message::Noop,
            ),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
    }
}

fn show_toast(state: &mut PhotonGui, message: String, kind: ToastKind) -> Task<Message> {
    state.toast_seq += 1;
    let id = state.toast_seq;
    state.toast = Some(ToastState { id, message, kind });
    Task::perform(tokio::time::sleep(Duration::from_secs(3)), move |_| {
        Message::DismissToast(Some(id))
    })
}

fn dispatch_to_core(core: CoreHandle, command: Command) -> Task<Message> {
    Task::perform(
        async move {
            let env = Envelope::new(
                format!("gui-{}", std::process::id()),
                ClientKind::Gui,
                command,
            );
            core.dispatch(env).await
        },
        Message::CommandCompleted,
    )
}

impl PhotonGui {
    fn viewer_ui_presentation(&self) -> ViewerUiPresentation {
        ViewerUiPresentation {
            view_mode: self.viewer_mode,
            info_open: self.viewer_info_open,
            adjustments_open: self.viewer_adjustments_open,
            filmstrip_open: self.viewer_filmstrip_open,
            is_fullscreen: self.is_fullscreen,
            color_popover_open: self.viewer_color_popover_open,
        }
    }

    fn reset_viewer_presentation_from_layout(&mut self, layout: &photon_protocol::LayoutState) {
        self.viewer_mode = layout.viewer_view_mode;
        self.viewer_manga_rtl = layout.manga_rtl;
        self.viewer_manga_cover_separate = layout.manga_cover_separate;
        self.viewer_manga_page_gap = layout.manga_page_gap;
        self.viewer_webtoon_fit_mode = layout.webtoon_fit_mode;
        self.viewer_info_open = layout.panel("viewer.info") || layout.panel("viewer.inspector");
        self.viewer_adjustments_open = layout.panel("viewer.adjustments");
        self.viewer_color_popover_open = self.viewer_adjustments_open;
        self.viewer_right_panels.set_order(
            layout.viewer_right_panel_order.clone(),
            &ViewerRightPanel::ALL,
        );
        self.viewer_filmstrip_open = layout.panel("viewer.filmstrip");
        self.viewer_quarter_turns = 0;
        self.webtoon_zoom = 1.0;
    }
}

fn sync_ui_tree_task(state: &PhotonGui) -> Task<Message> {
    let core = state.core.clone();
    let backend = GPU_BACKEND_NAME.get().map(String::as_str).unwrap_or("wgpu");
    let viewer_ui = state.viewer_ui_presentation();
    let tree = build_ui_tree(
        &state.snapshot,
        state.windows.len(),
        state.window_size,
        state.zoom,
        state.pan_x,
        state.pan_y,
        backend,
        &viewer_ui,
        &state.editor_ui,
        state.context_menu.as_ref(),
    );
    Task::perform(
        async move {
            core.update_ui_tree(tree).await;
        },
        |_| Message::Noop,
    )
}

fn handle_semantic_ui_action(state: &mut PhotonGui, target_id: &str) -> Task<Message> {
    match target_id {
        "nav.back" => {
            if state.snapshot.meta.workspace == Workspace::Editor {
                update(
                    state,
                    Message::RequestEditorLeave {
                        close_editor: false,
                    },
                )
            } else {
                dispatch_to_core(state.core.clone(), Command::WorkspaceBack)
            }
        }
        "nav.workspace.browser" => dispatch_to_core(
            state.core.clone(),
            Command::SetWorkspace {
                workspace: Workspace::Browser,
            },
        ),
        "nav.workspace.viewer" => dispatch_to_core(
            state.core.clone(),
            Command::SetWorkspace {
                workspace: Workspace::Viewer,
            },
        ),
        "nav.workspace.editor" => dispatch_to_core(
            state.core.clone(),
            Command::SetWorkspace {
                workspace: Workspace::Editor,
            },
        ),
        "toolbar.refresh" => dispatch_to_core(state.core.clone(), Command::Refresh),
        "toolbar.theme_toggle" => dispatch_to_core(state.core.clone(), Command::ToggleTheme),
        "toolbar.panels_menu" => update(state, Message::ToggleMenu(ActiveMenu::WorkspacePanels)),
        "toolbar.settings" => update(state, Message::OpenSettings),
        "browser.filter_toggle" => update(state, Message::ToggleBrowserFilterBar),
        "browser.view_mode" => commit_layout_with(state, |layout| {
            layout.browser_view_mode = match layout.browser_view_mode {
                BrowserViewMode::Grid => BrowserViewMode::List,
                BrowserViewMode::List => BrowserViewMode::Details,
                BrowserViewMode::Details => BrowserViewMode::Grid,
            };
        }),
        "browser.menu.file" => update(state, Message::ToggleMenu(ActiveMenu::BrowserFile)),
        "browser.menu.file.edit" | "browser.menu.tools.edit" => {
            update(state, Message::BrowserOpenSelectedInEditor)
        }
        "browser.menu.view" => update(state, Message::ToggleMenu(ActiveMenu::BrowserView)),
        "browser.menu.view.detail_panel" => update(state, Message::ToggleBrowserInspector),
        "browser.menu.tools" => update(state, Message::ToggleMenu(ActiveMenu::BrowserTools)),
        "browser.menu.tools.batch_resize" => {
            update(state, Message::OpenBatchDialog(BatchDialogKind::Resize))
        }
        "browser.menu.tools.batch_crop" => {
            update(state, Message::OpenBatchDialog(BatchDialogKind::Crop))
        }
        "browser.menu.tools.batch_convert" => {
            update(state, Message::OpenBatchDialog(BatchDialogKind::Convert))
        }
        "browser.menu.plugins" => update(state, Message::ToggleMenu(ActiveMenu::BrowserPlugins)),
        "browser.menu.window" => update(state, Message::ToggleMenu(ActiveMenu::BrowserWindow)),
        "browser.menu.window.quick_access" => update(
            state,
            Message::ToggleBrowserPanelVisible(BrowserLeftPanel::QuickAccess),
        ),
        "browser.menu.window.explorer" => update(
            state,
            Message::ToggleBrowserPanelVisible(BrowserLeftPanel::Explorer),
        ),
        "browser.menu.window.detail" => update(state, Message::ToggleBrowserInspector),
        "browser.menu.window.preview" => update(
            state,
            Message::ToggleBrowserRightPanelVisible(BrowserRightPanel::Preview),
        ),
        "browser.menu.window.info" => update(
            state,
            Message::ToggleBrowserRightPanelVisible(BrowserRightPanel::Info),
        ),
        "browser.right_panel.reorder" => update(state, Message::SwapBrowserRightPanels),
        "browser.menu.help" => update(state, Message::ToggleMenu(ActiveMenu::BrowserHelp)),
        "browser.panel.reorder" => update(state, Message::SwapBrowserLeftPanels),
        "viewer.prev" => {
            if state.viewer_mode == ViewerViewMode::Double {
                update(state, Message::DualStepSpread(-1))
            } else {
                dispatch_to_core(state.core.clone(), Command::ViewerPrevious)
            }
        }
        "viewer.next" => {
            if state.viewer_mode == ViewerViewMode::Double {
                update(state, Message::DualStepSpread(1))
            } else {
                dispatch_to_core(state.core.clone(), Command::ViewerNext)
            }
        }
        "viewer.mode_single" => update(state, Message::SetViewerViewMode(ViewerViewMode::Single)),
        "viewer.mode_double" => update(state, Message::SetViewerViewMode(ViewerViewMode::Double)),
        "viewer.mode_manhwa" => update(state, Message::SetViewerViewMode(ViewerViewMode::Webtoon)),
        "viewer.reader_options" => update(state, Message::ToggleMenu(ActiveMenu::ReaderOptions)),
        "viewer.zoom_in" => update(state, Message::ZoomStep(1.25)),
        "viewer.zoom_out" => update(state, Message::ZoomStep(0.8)),
        "viewer.fit" => update(state, Message::Canvas(CanvasEvent::ResetFit)),
        "viewer.edit" => dispatch_to_core(
            state.core.clone(),
            Command::SetWorkspace {
                workspace: Workspace::Editor,
            },
        ),
        "viewer.info_toggle" => update(state, Message::ToggleViewerInspector),
        "viewer.panel.reorder" => update(state, Message::SwapViewerRightPanels),
        "viewer.actual_size" => update(state, Message::ViewerZoomActual),
        "viewer.rotate_left" => update(state, Message::ViewerRotatePreview(-1)),
        "viewer.rotate_right" => update(state, Message::ViewerRotatePreview(1)),
        "viewer.filmstrip_toggle" => update(state, Message::ToggleFilmstrip),
        "viewer.fullscreen" => update(state, Message::ToggleFullscreen),
        "viewer.color_popover.toggle" => update(state, Message::ToggleViewerColorPopover),
        "viewer.color_popover.reset" => update(state, Message::ResetViewerAdj),
        "viewer.color_popover.open_editor" => update(state, Message::ViewerAdjOpenInEditor),
        "editor.tool_tab.options" => {
            update(state, Message::SelectEditorToolTab(EditorToolTab::Options))
        }
        "editor.tool_tab.history" => {
            update(state, Message::SelectEditorToolTab(EditorToolTab::History))
        }
        "editor.panel.reorder" => update(state, Message::SwapEditorPanelOrder),
        "editor.tool.pointer" => update(state, Message::EditorSelectTool(EditorTool::Pointer)),
        "editor.tool.hand" => update(state, Message::EditorSelectTool(EditorTool::Hand)),
        "editor.tool.zoom" => update(state, Message::EditorSelectTool(EditorTool::Zoom)),
        "editor.tool.brush" => update(state, Message::EditorSelectTool(EditorTool::Brush)),
        "editor.tool.eraser" => update(state, Message::EditorSelectTool(EditorTool::Eraser)),
        "editor.tool.liquify" => update(state, Message::EditorSelectTool(EditorTool::Liquify)),
        "editor.tool.eyedropper" => {
            update(state, Message::EditorSelectTool(EditorTool::Eyedropper))
        }
        "editor.tool.crop" => update(state, Message::EditorSelectTool(EditorTool::Crop)),
        "editor.tool.canvas_size" => {
            update(state, Message::EditorSelectTool(EditorTool::CanvasSize))
        }
        "editor.tool.rotate_angle" => {
            update(state, Message::EditorSelectTool(EditorTool::RotateAngle))
        }
        "editor.adjust_popup.open" | "editor.adjust_popup.tab.color" | "editor.adjust.color" => {
            update(
                state,
                Message::EditorOpenAdjustPopup(AdjustmentPopupTab::ColorAndLight),
            )
        }
        "editor.adjust_popup.tab.hsv" | "editor.adjust.hsv" => update(
            state,
            Message::EditorOpenAdjustPopup(AdjustmentPopupTab::Hsv),
        ),
        "editor.adjust_popup.tab.balance" | "editor.adjust.balance" => update(
            state,
            Message::EditorOpenAdjustPopup(AdjustmentPopupTab::ColorBalance),
        ),
        "editor.adjust_popup.tab.curves" | "editor.adjust.curves" => update(
            state,
            Message::EditorOpenAdjustPopup(AdjustmentPopupTab::Curves),
        ),
        "editor.adjust_popup.reset" => update(state, Message::EditorResetPendingAdjustments),
        "editor.adjust_popup.cancel" => {
            update(state, Message::EditorCloseAdjustPopup { apply: false })
        }
        "editor.adjust_popup.apply" => {
            update(state, Message::EditorCloseAdjustPopup { apply: true })
        }
        "editor.menu.filter" => update(state, Message::ToggleMenu(ActiveMenu::EditorFilter)),
        "editor.menu.file.open_viewer" => update(
            state,
            Message::RequestEditorLeave {
                close_editor: false,
            },
        ),
        "editor.menu.file.open_browser" => update(state, Message::RequestEditorLeaveToBrowser),
        "editor.menu.window" => update(state, Message::ToggleMenu(ActiveMenu::EditorWindow)),
        "editor.menu.window.tools" => update(state, Message::ToggleEditorToolsVisible),
        "editor.menu.window.history" => update(
            state,
            Message::ToggleEditorPanelVisible(EditorRightPanel::ToolOptionsHistory),
        ),
        "editor.menu.window.layers" => update(
            state,
            Message::ToggleEditorPanelVisible(EditorRightPanel::Layers),
        ),
        "editor.menu.help" => update(state, Message::ToggleMenu(ActiveMenu::EditorHelp)),
        "editor.layer.mask.add" | "editor.layer.mask.add_reveal" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerMaskAdd {
                        id: e.active_layer_id,
                        reveal_all: true,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.layer.mask.add_hide" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerMaskAdd {
                        id: e.active_layer_id,
                        reveal_all: false,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.layer.mask.edit" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .and_then(|e| {
                e.layers
                    .iter()
                    .find(|l| l.id == e.active_layer_id)
                    .map(|l| {
                        dispatch_to_core(
                            state.core.clone(),
                            Command::EditorLayerMaskSetEditing {
                                id: l.id,
                                editing: !l.editing_mask,
                            },
                        )
                    })
            })
            .unwrap_or_else(Task::none),
        "editor.layer.mask.enable" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .and_then(|e| {
                e.layers
                    .iter()
                    .find(|l| l.id == e.active_layer_id)
                    .map(|l| {
                        dispatch_to_core(
                            state.core.clone(),
                            Command::EditorLayerMaskSetEnabled {
                                id: l.id,
                                enabled: !l.mask_enabled,
                            },
                        )
                    })
            })
            .unwrap_or_else(Task::none),
        "editor.layer.mask.invert" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerMaskInvert {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.layer.mask.apply" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerMaskApply {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.layer.mask.delete" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerMaskDelete {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.compare" => update(state, Message::EditorCycleCompareMode),
        "editor.export" => update(state, Message::OpenExportDialog),
        "editor.rotate_left" => dispatch_to_core(
            state.core.clone(),
            Command::EditorRotate {
                degrees: -90,
                target: state.editor_ui.apply_target,
            },
        ),
        "editor.rotate_right" => dispatch_to_core(
            state.core.clone(),
            Command::EditorRotate {
                degrees: 90,
                target: state.editor_ui.apply_target,
            },
        ),
        "editor.flip_h" => dispatch_to_core(
            state.core.clone(),
            Command::EditorFlip {
                axis: FlipAxis::Horizontal,
                target: state.editor_ui.apply_target,
            },
        ),
        "editor.flip_v" => dispatch_to_core(
            state.core.clone(),
            Command::EditorFlip {
                axis: FlipAxis::Vertical,
                target: state.editor_ui.apply_target,
            },
        ),
        "editor.layer.add" => {
            let count = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .map(|e| e.layers.len() + 1)
                .unwrap_or(1);
            dispatch_to_core(
                state.core.clone(),
                Command::EditorLayerAdd {
                    name: format!("Layer {count}"),
                },
            )
        }
        "editor.layer.duplicate" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerDuplicate {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.layer.delete" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerDelete {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.layer.merge_down" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerMergeDown {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.layer.flatten" => dispatch_to_core(state.core.clone(), Command::EditorLayerFlatten),
        "editor.crop" => update(state, Message::ToggleCropMode),
        "editor.resize" => update(state, Message::OpenResizeDialog),
        "editor.undo" => dispatch_to_core(state.core.clone(), Command::EditorUndo),
        "editor.redo" => dispatch_to_core(state.core.clone(), Command::EditorRedo),
        "editor.reset" => dispatch_to_core(state.core.clone(), Command::EditorReset),
        "editor.save" => dispatch_to_core(state.core.clone(), Command::EditorSave),
        "viewer.context_menu" => {
            if matches!(state.context_menu, Some(ActiveContextMenu::Viewer { .. })) {
                update(state, Message::CloseContextMenu)
            } else {
                update(
                    state,
                    Message::OpenViewerContextMenu {
                        path: state.snapshot.meta.viewer_path.clone(),
                        pos: Some([
                            state.window_size.width * 0.5,
                            state.window_size.height * 0.5,
                        ]),
                    },
                )
            }
        }
        "viewer.context_menu.edit" => {
            let p = match &state.context_menu {
                Some(ActiveContextMenu::Viewer { target_path, .. }) => target_path
                    .clone()
                    .or_else(|| state.snapshot.meta.viewer_path.clone()),
                _ => state.snapshot.meta.viewer_path.clone(),
            };
            state.context_menu = None;
            p.map(|path| dispatch_to_core(state.core.clone(), Command::EditorOpen { path }))
                .unwrap_or_else(Task::none)
        }
        "viewer.context_menu.copy_image" => {
            let p = match &state.context_menu {
                Some(ActiveContextMenu::Viewer { target_path, .. }) => target_path
                    .clone()
                    .or_else(|| state.snapshot.meta.viewer_path.clone()),
                _ => state.snapshot.meta.viewer_path.clone(),
            };
            update(state, Message::CopyImageToClipboard(p))
        }
        "viewer.context_menu.copy_path" => update(state, Message::CopyViewerPath),
        "viewer.context_menu.reveal_folder" => {
            let p = match &state.context_menu {
                Some(ActiveContextMenu::Viewer { target_path, .. }) => target_path
                    .clone()
                    .or_else(|| state.snapshot.meta.viewer_path.clone()),
                _ => state.snapshot.meta.viewer_path.clone(),
            };
            p.map(|path| update(state, Message::RevealInFolder(path)))
                .unwrap_or_else(Task::none)
        }
        "viewer.context_menu.fit" => update(state, Message::Canvas(CanvasEvent::ResetFit)),
        "viewer.context_menu.actual_size" => update(state, Message::ViewerZoomActual),
        "viewer.context_menu.zoom_in" => update(state, Message::ZoomStep(1.25)),
        "viewer.context_menu.zoom_out" => update(state, Message::ZoomStep(0.8)),
        "viewer.context_menu.rotate_left" => update(state, Message::ViewerRotatePreview(-1)),
        "viewer.context_menu.rotate_right" => update(state, Message::ViewerRotatePreview(1)),
        "viewer.context_menu.mode_single" => {
            update(state, Message::SetViewerViewMode(ViewerViewMode::Single))
        }
        "viewer.context_menu.mode_double" => {
            update(state, Message::SetViewerViewMode(ViewerViewMode::Double))
        }
        "viewer.context_menu.mode_manhwa" => {
            update(state, Message::SetViewerViewMode(ViewerViewMode::Webtoon))
        }
        "viewer.context_menu.fullscreen" => update(state, Message::ToggleFullscreen),
        "viewer.context_menu.info" => update(state, Message::ToggleViewerInspector),
        "editor.context_menu" => {
            if matches!(
                state.context_menu,
                Some(ActiveContextMenu::EditorCanvas { .. })
            ) {
                update(state, Message::CloseContextMenu)
            } else if state.snapshot.meta.editor.is_some() {
                state.active_menu = None;
                state.context_menu = Some(ActiveContextMenu::EditorCanvas {
                    x: state.window_size.width * 0.5,
                    y: state.window_size.height * 0.5,
                });
                sync_ui_tree_task(state)
            } else {
                Task::none()
            }
        }
        "editor.context_menu.undo" => {
            update(state, Message::ContextMenuDispatch(Command::EditorUndo))
        }
        "editor.context_menu.redo" => {
            update(state, Message::ContextMenuDispatch(Command::EditorRedo))
        }
        "editor.context_menu.copy_image" => update(state, Message::CopyImageToClipboard(None)),
        "editor.context_menu.duplicate_layer" => {
            if let Some(id) = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .map(|e| e.active_layer_id)
            {
                update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorLayerDuplicate { id }),
                )
            } else {
                Task::none()
            }
        }
        "editor.context_menu.rotate_left" => update(
            state,
            Message::ContextMenuDispatch(Command::EditorRotate {
                degrees: -90,
                target: ApplyTarget::WholeImage,
            }),
        ),
        "editor.context_menu.rotate_right" => update(
            state,
            Message::ContextMenuDispatch(Command::EditorRotate {
                degrees: 90,
                target: ApplyTarget::WholeImage,
            }),
        ),
        "editor.context_menu.flip_h" => update(
            state,
            Message::ContextMenuDispatch(Command::EditorFlip {
                axis: FlipAxis::Horizontal,
                target: ApplyTarget::WholeImage,
            }),
        ),
        "editor.context_menu.flip_v" => update(
            state,
            Message::ContextMenuDispatch(Command::EditorFlip {
                axis: FlipAxis::Vertical,
                target: ApplyTarget::WholeImage,
            }),
        ),
        "editor.context_menu.crop" => update(state, Message::ToggleCropMode),
        "editor.context_menu.resize" => update(state, Message::OpenResizeDialog),
        "editor.context_menu.canvas_size" => {
            update(state, Message::EditorSelectTool(EditorTool::CanvasSize))
        }
        "editor.context_menu.fit" => update(state, Message::Canvas(CanvasEvent::ResetFit)),
        "editor.context_menu.compare" => update(state, Message::EditorCycleCompareMode),
        "editor.context_menu.compare_side_by_side" => update(
            state,
            Message::EditorSetCompareMode(CompareMode::SideBySide),
        ),
        "editor.context_menu.reset" => {
            update(state, Message::ContextMenuDispatch(Command::EditorReset))
        }
        "editor.layer.context_menu" => {
            if matches!(
                state.context_menu,
                Some(ActiveContextMenu::EditorLayer { .. })
            ) {
                update(state, Message::CloseContextMenu)
            } else if let Some(ed) = &state.snapshot.meta.editor {
                state.active_menu = None;
                state.context_menu = Some(ActiveContextMenu::EditorLayer {
                    x: (state.window_size.width - 220.0).max(20.0),
                    y: 260.0,
                    layer_id: ed.active_layer_id,
                });
                sync_ui_tree_task(state)
            } else {
                Task::none()
            }
        }
        "editor.layer.context_menu.duplicate"
        | "editor.layer.context_menu.rename"
        | "editor.layer.context_menu.move_up"
        | "editor.layer.context_menu.move_down"
        | "editor.layer.context_menu.merge_down"
        | "editor.layer.context_menu.flatten"
        | "editor.layer.context_menu.toggle_visible"
        | "editor.layer.context_menu.toggle_locked"
        | "editor.layer.context_menu.delete" => {
            let Some(ed) = state.snapshot.meta.editor.clone() else {
                return Task::none();
            };
            let layer_id = match state.context_menu {
                Some(ActiveContextMenu::EditorLayer { layer_id, .. }) => layer_id,
                _ => ed.active_layer_id,
            };
            let Some(idx) = ed.layers.iter().position(|l| l.id == layer_id) else {
                state.context_menu = None;
                return Task::none();
            };
            let layer = ed.layers[idx].clone();
            match target_id {
                "editor.layer.context_menu.duplicate" => update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorLayerDuplicate { id: layer.id }),
                ),
                "editor.layer.context_menu.rename" => update(
                    state,
                    Message::EditorStartLayerRename(layer.id, layer.name.clone()),
                ),
                "editor.layer.context_menu.move_up" => {
                    if idx + 1 < ed.layers.len() {
                        update(
                            state,
                            Message::ContextMenuDispatch(Command::EditorLayerReorder {
                                id: layer.id,
                                new_index: idx + 1,
                            }),
                        )
                    } else {
                        state.context_menu = None;
                        Task::none()
                    }
                }
                "editor.layer.context_menu.move_down" => {
                    if idx > 0 {
                        update(
                            state,
                            Message::ContextMenuDispatch(Command::EditorLayerReorder {
                                id: layer.id,
                                new_index: idx - 1,
                            }),
                        )
                    } else {
                        state.context_menu = None;
                        Task::none()
                    }
                }
                "editor.layer.context_menu.merge_down" => update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorLayerMergeDown { id: layer.id }),
                ),
                "editor.layer.context_menu.flatten" => update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorLayerFlatten),
                ),
                "editor.layer.context_menu.toggle_visible" => update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorLayerSetProps {
                        id: layer.id,
                        name: None,
                        visible: Some(!layer.visible),
                        locked: None,
                        opacity: None,
                        blend_mode: None,
                        offset_x: None,
                        offset_y: None,
                    }),
                ),
                "editor.layer.context_menu.toggle_locked" => update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorLayerSetProps {
                        id: layer.id,
                        name: None,
                        visible: None,
                        locked: Some(!layer.locked),
                        opacity: None,
                        blend_mode: None,
                        offset_x: None,
                        offset_y: None,
                    }),
                ),
                "editor.layer.context_menu.delete" => update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorLayerDelete { id: layer.id }),
                ),
                _ => Task::none(),
            }
        }
        _ => Task::none(),
    }
}

fn sync_expanded_tree_tasks(state: &mut PhotonGui, force_reload_expanded: bool) -> Task<Message> {
    let Some(root_str) = state.snapshot.meta.root_folder.clone() else {
        return Task::none();
    };
    let mut to_load = Vec::new();
    if state.expanded_folders.insert(root_str.clone()) || force_reload_expanded {
        to_load.push(root_str.clone());
    }
    if let Some(cur) = &state.snapshot.meta.current_folder {
        let root_path = Path::new(&root_str);
        let mut cur_path = PathBuf::from(cur);
        while cur_path.starts_with(root_path) {
            if let Some(parent) = cur_path.parent() {
                let p_str = parent.to_string_lossy().into_owned();
                if parent.starts_with(root_path)
                    && (state.expanded_folders.insert(p_str.clone())
                        || !state.folder_children.contains_key(&p_str))
                    && !to_load.contains(&p_str)
                {
                    to_load.push(p_str);
                }
                if parent == root_path {
                    break;
                }
                cur_path = parent.to_path_buf();
            } else {
                break;
            }
        }
    }
    if force_reload_expanded {
        for expanded in &state.expanded_folders {
            if !to_load.contains(expanded) {
                to_load.push(expanded.clone());
            }
        }
    }
    Task::batch(
        to_load
            .into_iter()
            .map(|p| load_folder_children_task(state.core.clone(), p)),
    )
}

fn load_folder_children_task(core: CoreHandle, path: String) -> Task<Message> {
    Task::perform(
        async move {
            let res = core.query(Query::ListFolders { path: path.clone() }).await;
            let mut children = Vec::new();
            if let Ok(Ok(val)) = res
                && let Some(arr) = val.get("folders").and_then(|v| v.as_array())
            {
                for item in arr {
                    if let (Some(name), Some(child_path)) = (
                        item.get("name").and_then(|v| v.as_str()),
                        item.get("path").and_then(|v| v.as_str()),
                    ) {
                        children.push((name.to_owned(), child_path.to_owned()));
                    }
                }
            }
            (path, children)
        },
        |(path, children)| Message::FolderChildrenLoaded(path, children),
    )
}

fn schedule_missing_thumbnails(state: &mut PhotonGui) -> Task<Message> {
    if state.thumbnails.len() > 256 {
        let keep: BTreeSet<&str> = state
            .snapshot
            .entries
            .iter()
            .map(|e| e.path.as_str())
            .chain(state.snapshot.viewer_images.iter().map(String::as_str))
            .collect();
        state.thumbnails.retain(|k, _| keep.contains(k.as_str()));
    }

    let mut pending: Vec<String> = state
        .snapshot
        .entries
        .iter()
        .filter(|e| e.is_image)
        .map(|e| e.path.clone())
        .chain(state.snapshot.viewer_images.iter().cloned())
        .filter(|p| !state.thumbnails.contains_key(p) && state.thumbnails_loading.insert(p.clone()))
        .collect();
    pending.sort();
    pending.dedup();

    if pending.is_empty() {
        return Task::none();
    }

    let tasks = pending
        .chunks(8)
        .map(|chunk| {
            let paths = chunk.to_vec();
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let mut out = Vec::new();
                        for path in paths {
                            if let Ok(thumb) =
                                photon_image::generate_thumbnail(Path::new(&path), 240)
                            {
                                let (w, h) = thumb.dimensions();
                                out.push((path, w, h, thumb.into_raw()));
                            }
                        }
                        out
                    })
                    .await
                    .unwrap_or_default()
                },
                Message::ThumbnailsLoaded,
            )
        })
        .collect::<Vec<_>>();

    Task::batch(tasks)
}

/// Compute KogaView-style fitted window dimensions when opened directly on an image file.
fn window_size_for_image(snap: &UiStateSnapshot, img_w: u32, img_h: u32) -> Size {
    if img_w == 0 || img_h == 0 {
        return Size::new(
            (snap.meta.layout.window_width as f32).clamp(960.0, 1728.0),
            (snap.meta.layout.window_height as f32).clamp(540.0, 972.0),
        );
    }
    let layout = &snap.meta.layout;
    let is_editor = snap.meta.workspace == Workspace::Editor;
    let info_open = if is_editor {
        layout.panel("editor.inspector")
    } else {
        layout.panel("viewer.inspector")
    };
    let tools_open = is_editor && layout.panel("editor.tools");
    let filmstrip_open = !is_editor
        && layout.viewer_view_mode != ViewerViewMode::Webtoon
        && snap.meta.viewer_count > 1
        && layout.panel("viewer.filmstrip");

    let chrome_w = if info_open { 246.0 } else { 0.0 } + if tools_open { 68.0 } else { 0.0 } + 16.0;
    let chrome_h = 50.0 + 28.0 + if filmstrip_open { 68.0 } else { 0.0 } + 16.0;

    // Conservative 90% work-area cap for typical 1080p+ desktops without needing screen probe.
    let max_w = 1728.0_f32;
    let max_h = 972.0_f32;
    let avail_w = (max_w - chrome_w).max(200.0);
    let avail_h = (max_h - chrome_h).max(200.0);

    let iw = img_w as f32;
    let ih = img_h as f32;
    let mut scale = (avail_w / iw).min(avail_h / ih).min(1.0);
    if scale < 1.0 {
        scale *= 0.98;
    }

    let w = (iw * scale + chrome_w).round().clamp(960.0, max_w);
    let h = (ih * scale + chrome_h).round().clamp(540.0, max_h);
    Size::new(w, h)
}

fn probe_viewer_image_sizes(state: &mut PhotonGui) {
    for path in &state.snapshot.viewer_images {
        if !state.image_sizes.contains_key(path)
            && let Some(dims) = photon_image::probe_dimensions(Path::new(path))
        {
            state.image_sizes.insert(path.clone(), dims);
        }
    }
}

fn schedule_webtoon_pages_near(state: &mut PhotonGui, center_idx: usize) -> Task<Message> {
    let total = state.snapshot.viewer_images.len();
    if total == 0 {
        return Task::none();
    }
    let (start, end) = if total <= 24 {
        (0, total)
    } else {
        (center_idx.saturating_sub(4), (center_idx + 12).min(total))
    };
    let pending: Vec<String> = state.snapshot.viewer_images[start..end]
        .iter()
        .filter(|p| {
            !state.webtoon_pages.contains_key(*p) && state.webtoon_loading.insert((*p).clone())
        })
        .cloned()
        .collect();
    if pending.is_empty() {
        return Task::none();
    }
    let tasks = pending
        .chunks(4)
        .map(|chunk| {
            let paths = chunk.to_vec();
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let mut out = Vec::new();
                        for path in paths {
                            if let Ok(rgba) = photon_image::decode_rgba(Path::new(&path)) {
                                let (w, h) = rgba.dimensions();
                                if w > 2200 || h > 4096 {
                                    let thumb =
                                        image::DynamicImage::ImageRgba8(rgba).thumbnail(2200, 4096);
                                    let r = thumb.to_rgba8();
                                    let (nw, nh) = r.dimensions();
                                    out.push((path, nw, nh, r.into_raw()));
                                } else {
                                    out.push((path, w, h, rgba.into_raw()));
                                }
                            }
                        }
                        out
                    })
                    .await
                    .unwrap_or_default()
                },
                Message::WebtoonPagesLoaded,
            )
        })
        .collect::<Vec<_>>();
    Task::batch(tasks)
}

fn snap_viewer_mode_needs_thumbs(
    snap: &UiStateSnapshot,
    viewer_mode: ViewerViewMode,
    filmstrip_open: bool,
) -> bool {
    viewer_mode == ViewerViewMode::Webtoon || (snap.viewer_images.len() > 1 && filmstrip_open)
}

fn reload_dual_frames_task(state: &PhotonGui) -> Task<Message> {
    let (left_slot, right_slot) = dual_spread_slots_for(
        &state.snapshot.viewer_images,
        &state.image_sizes,
        state.snapshot.meta.viewer_index,
        state.viewer_manga_cover_separate,
        state.viewer_manga_rtl,
    );
    let mut tasks = Vec::new();
    match left_slot {
        Some((_, lp)) => {
            if state
                .viewer_frame
                .as_ref()
                .is_none_or(|f| f.key != format!("viewer:{lp}"))
            {
                tasks.push(load_viewer_frame_task(lp.to_owned()));
            }
        }
        None => tasks.push(Task::done(Message::ViewerFrameLoaded(None))),
    }
    match right_slot {
        Some((_, rp)) => {
            if state
                .viewer_pair_frame
                .as_ref()
                .is_none_or(|f| f.key != format!("viewer:{rp}"))
            {
                tasks.push(load_viewer_pair_frame_task(Some(rp.to_owned())));
            }
        }
        None => tasks.push(load_viewer_pair_frame_task(None)),
    }
    Task::batch(tasks)
}

fn reload_active_frames(
    core: &CoreHandle,
    snap: &UiStateSnapshot,
    image_sizes: &HashMap<String, (u32, u32)>,
    viewer_ui: &ViewerUiPresentation,
) -> Task<Message> {
    let mut tasks = Vec::new();
    if viewer_ui.view_mode == ViewerViewMode::Double {
        let (left_slot, right_slot) = dual_spread_slots_for(
            &snap.viewer_images,
            image_sizes,
            snap.meta.viewer_index,
            snap.meta.layout.manga_cover_separate,
            snap.meta.layout.manga_rtl,
        );
        if let Some((_, lp)) = left_slot {
            tasks.push(load_viewer_frame_task(lp.to_owned()));
        }
        tasks.push(load_viewer_pair_frame_task(
            right_slot.map(|(_, rp)| rp.to_owned()),
        ));
    } else if let Some(path) = &snap.meta.viewer_path {
        tasks.push(load_viewer_frame_task(path.clone()));
    }
    if snap.meta.editor.is_some() {
        tasks.push(load_editor_frame_task(core.clone()));
        tasks.push(load_editor_original_frame_task(core.clone()));
        tasks.push(load_editor_thumbnails_task(core.clone()));
    }
    Task::batch(tasks)
}

fn decode_viewer_frame_sync(path: &Path) -> Option<CpuImageFrame> {
    let rgba = photon_image::decode_rgba(path).ok()?;
    let (w, h) = rgba.dimensions();
    Some(CpuImageFrame {
        key: format!("viewer:{}", path.display()),
        revision: 1,
        width: w,
        height: h,
        rgba: Arc::new(rgba.into_raw()),
    })
}

fn load_viewer_frame_task(path: String) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || decode_viewer_frame_sync(Path::new(&path)))
                .await
                .ok()
                .flatten()
        },
        Message::ViewerFrameLoaded,
    )
}

fn load_viewer_pair_frame_task(path: Option<String>) -> Task<Message> {
    let Some(path) = path else {
        return Task::done(Message::ViewerPairFrameLoaded(None));
    };
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || decode_viewer_frame_sync(Path::new(&path)))
                .await
                .ok()
                .flatten()
        },
        Message::ViewerPairFrameLoaded,
    )
}

fn load_editor_frame_task(core: CoreHandle) -> Task<Message> {
    Task::perform(
        async move {
            let (rev, w, h, bytes) = core.read_editor_pixels().await?;
            Some(CpuImageFrame {
                key: "editor:active".to_owned(),
                revision: rev,
                width: w,
                height: h,
                rgba: Arc::new(bytes),
            })
        },
        Message::EditorFrameLoaded,
    )
}

fn load_editor_original_frame_task(core: CoreHandle) -> Task<Message> {
    Task::perform(
        async move {
            let (rev, w, h, bytes) = core.read_editor_original_pixels().await?;
            Some(CpuImageFrame {
                key: "editor:original".to_owned(),
                revision: rev,
                width: w,
                height: h,
                rgba: Arc::new(bytes),
            })
        },
        Message::EditorOriginalFrameLoaded,
    )
}

fn load_editor_thumbnails_task(core: CoreHandle) -> Task<Message> {
    Task::perform(
        async move { core.read_editor_thumbnails().await.unwrap_or_default() },
        |(rev, thumbs)| Message::EditorThumbnailsLoaded(rev, thumbs),
    )
}

fn copy_image_png_to_clipboard(
    path: Option<&str>,
    cached_rgba: Option<(u32, u32, Arc<Vec<u8>>)>,
) -> bool {
    use image::ImageEncoder;
    use std::io::Write;
    use std::process::{Command as ProcessCommand, Stdio};

    let (w, h, raw): (u32, u32, Vec<u8>) = if let Some((w, h, arc)) = cached_rgba {
        (w, h, arc.as_ref().clone())
    } else if let Some(p) = path {
        let Ok(rgba) = photon_image::decode_rgba(Path::new(p)) else {
            return false;
        };
        let (w, h) = rgba.dimensions();
        (w, h, rgba.into_raw())
    } else {
        return false;
    };

    let mut png_bytes = Vec::new();
    if image::codecs::png::PngEncoder::new(&mut png_bytes)
        .write_image(&raw, w, h, image::ExtendedColorType::Rgba8)
        .is_err()
    {
        return false;
    }

    let try_pipe = |program: &str, args: &[&str]| -> bool {
        let Ok(mut child) = ProcessCommand::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(&png_bytes);
        }
        child.wait().is_ok_and(|s| s.success())
    };

    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && try_pipe("wl-copy", &["--type", "image/png"])
    {
        return true;
    }
    try_pipe("wl-copy", &["--type", "image/png"])
        || try_pipe("xclip", &["-selection", "clipboard", "-t", "image/png"])
}

fn reveal_path_in_folder(path: &str) {
    use std::process::{Command as ProcessCommand, Stdio};

    let p = Path::new(path);
    let target_dir = p
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let _ = ProcessCommand::new("xdg-open")
        .arg(target_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}
