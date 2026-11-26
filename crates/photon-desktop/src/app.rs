//! `iced 0.14` daemon + multi-window GUI adapter (`photon`) using `photon-ui`.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::widget::{
    checkbox, column, container, image as iced_image, operation as widget_operation, row,
    scrollable, slider, text, text_input,
};
use iced::{
    Alignment, Color, Element, Fill, Length, Point, Rectangle, Size, Subscription, Task,
    Theme as IcedTheme, event, mouse, window,
};
use image::RgbaImage;
use photon_core::{CoreHandle, EditorTransformFrames};
use photon_i18n::{tr, tr_ui_text};
use photon_panels::{PanelTabItem, SidebarOrderState, panel_tab_bar};
use photon_protocol::{
    AiDevice, AiMaskStroke, AppLaunchWindowMode, ApplyTarget, BlendMode, BrowserLeftPanel,
    BrowserRightPanel, BrowserSortField, BrowserThumbSize, BrowserViewMode, CanvasAntiAliasingMode,
    ClientKind, ColorAdjustments, Command, CommandResponse, EditorRightPanel, EditorToolTab,
    Envelope, EventKind, FlipAxis, GradientColorMode, GradientRepeat, GradientStop, HealingMode,
    Language, LayerEffect, LayerTransformSpec, LiquifyMode, LiveStrokeKind, MoveDirection,
    PluginCapability, PluginInfo, PluginParamKind, PluginParamSpec, PluginParamValue, Query,
    ResizeFilter, ResizeMode, SelectionBooleanOp, SelectionModifyOp, SelectionShape, SortOrder,
    SystemOpenWindowMode, Theme, TranslateProvider, TranslateSettings, UiStateSnapshot, UiText,
    ViewerRightPanel, ViewerViewMode, ViewportDenoiseMode, WebtoonFitMode, Workspace,
};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::curve_editor::{CurveGraphEvent, curve_graph};
use photon_ui::dialog::{
    ConfirmDialogSpec, DialogEvent, DialogManager, DialogSpec, DialogWindowState,
    HeaderedManagedDialogSpec, ManagedDialogSpec, OverlayMode, PromptDialogSpec, ToastKind,
    ToastSpec, alert_dialog_spec, confirm_dialog_spec, dialog_style_for_theme,
    managed_dialog_stack_with_headers, prompt_dialog_spec, with_toast_banner,
};
use photon_ui::icons::Icon;
use photon_ui::menu::{
    MenuEntry, MenuItem, menu_panel_entries, with_anchored_menu, with_bottom_right_menu_at,
    with_context_menu, with_top_right_menu, with_top_right_menu_at,
};
use photon_ui::panel::{section_card, status_bar};
use photon_ui::theme::{
    PaletteTokens, TEXT_SIZE_CONTROL, app_shell_style, input_style, slider_style,
};
use photon_ui::tooltip::{TooltipPosition, with_tooltip_pos};

use crate::inspector::{ViewerUiPresentation, build_ui_tree};
use photon_browser::batch_dialog::{
    BatchDialogAction, BatchDialogKind, BatchDialogMsg, BatchDialogState,
    build_batch_convert_dialog, build_batch_crop_dialog, build_batch_resize_dialog,
};
use photon_browser::{
    BrowserMenuKind, BrowserMessage, BrowserViewState, browser_filter_input_id,
    build_browser_context_menu_entries, build_browser_help_menu_entries,
    build_browser_plugins_menu_entries, build_browser_sort_menu, build_browser_toolbar,
    build_browser_view, build_browser_window_menu_entries, build_quick_access_context_menu,
};
use photon_canvas::{
    CanvasEvent, CpuImageFrame, CpuMaskFrame, TransformPreview, default_viewport_bg_rgb,
    dual_spread_geometry,
};
use photon_editor::panel_splitter::{DockDragState, DockSplitterKind};
use photon_editor::{
    AdjustmentPopupTab, AlphaRefineMode, ColorBalanceZone, ColorPickerTarget, CompareMode,
    CurvesChannel, DimGridAction, DimensionInputState, EditorMenuKind, EditorMessage, EditorTool,
    EditorUiState, EditorViewState, WatermarkTab, build_color_picker_dialog,
    build_editor_add_layer_menu, build_editor_canvas_context_menu, build_editor_layer_context_menu,
    build_editor_plugins_menu_entries, build_editor_toolbar, build_editor_view,
    build_export_dialog, crop_uv_to_signed, default_curves, format_hex, parse_hex_rgb,
    rgb_to_color,
};
use photon_ui::editor::{anchor_grid_3x3, checker_swatch};
use photon_viewer::{
    ViewerMessage, ViewerTranslateMode, ViewerTranslateState, ViewerViewState,
    build_viewer_context_menu, build_viewer_quality_menu_entries, build_viewer_toolbar,
    build_viewer_view, dual_spread_slots, dual_spread_slots_for, step_dual_spread,
    viewer_counter_summary, viewer_reader_options_right_offset, viewer_status_summary,
    webtoon_index_at_scroll, webtoon_offset_for_index, webtoon_scroll_id,
};

static CORE_HANDLE: OnceLock<CoreHandle> = OnceLock::new();
static GPU_BACKEND_NAME: OnceLock<String> = OnceLock::new();
static OPENED_WITH_FILES: OnceLock<bool> = OnceLock::new();
static STANDALONE_VIEWER: OnceLock<bool> = OnceLock::new();

pub fn init_globals(
    handle: CoreHandle,
    gpu_backend: String,
    opened_with_files: bool,
    standalone_viewer: bool,
) {
    let _ = CORE_HANDLE.set(handle);
    let _ = GPU_BACKEND_NAME.set(gpu_backend);
    let _ = OPENED_WITH_FILES.set(opened_with_files);
    let _ = STANDALONE_VIEWER.set(standalone_viewer);
}

fn is_standalone_viewer() -> bool {
    STANDALONE_VIEWER.get().copied().unwrap_or(false)
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
    EditorPlugins,
    EditorSelect,
    EditorView,
    EditorWindow,
    EditorHelp,
    EditorAddLayer,
    WorkspaceSwitcher,
    WorkspacePanels,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DesktopDialogId {
    Settings,
    NewFolder,
    ConfirmResetLayout,
    ConfirmUnsavedLeave,
    Resize,
    CanvasSize,
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
    AdjustDenoise,
    SelectionModify,
    ManagePlugins,
    PluginParams,
    PluginModels,
    TranslateSettings,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionModifyKind {
    Feather,
    Expand,
    Contract,
    Border,
    Smooth,
    Grow,
    Similar,
}

impl SelectionModifyKind {
    const fn title_key(self) -> &'static str {
        match self {
            Self::Feather => "dialog-select-feather-title",
            Self::Expand => "dialog-select-expand-title",
            Self::Contract => "dialog-select-contract-title",
            Self::Border => "dialog-select-border-title",
            Self::Smooth => "dialog-select-smooth-title",
            Self::Grow => "dialog-select-grow-title",
            Self::Similar => "dialog-select-similar-title",
        }
    }

    const fn label_key(self) -> &'static str {
        match self {
            Self::Feather => "label-select-feather-radius",
            Self::Expand => "label-select-expand-by",
            Self::Contract => "label-select-contract-by",
            Self::Border => "label-select-border-width",
            Self::Smooth => "label-select-smooth-radius",
            Self::Grow => "label-select-grow-tolerance",
            Self::Similar => "label-select-similar-tolerance",
        }
    }

    const fn hint_key(self) -> &'static str {
        match self {
            Self::Feather => "hint-select-feather-range",
            Self::Expand | Self::Contract | Self::Border | Self::Smooth => "hint-select-px-range",
            Self::Grow | Self::Similar => "hint-select-tolerance-range",
        }
    }

    const fn error_key(self) -> &'static str {
        match self {
            Self::Feather => "error-select-feather-range",
            Self::Expand | Self::Contract | Self::Border | Self::Smooth => "error-select-px-range",
            Self::Grow | Self::Similar => "error-select-tolerance-range",
        }
    }

    const fn default_value(self) -> &'static str {
        match self {
            Self::Feather => "2.0",
            Self::Expand | Self::Contract | Self::Smooth => "2",
            Self::Border => "4",
            Self::Grow | Self::Similar => "24",
        }
    }

    fn parse_and_validate(self, raw: &str, lang: Language) -> Result<SelectionModifyOp, String> {
        let trimmed = raw.trim();
        let err = || tr(lang, self.error_key());
        match self {
            Self::Feather => {
                let v: f32 = trimmed.parse().map_err(|_| err())?;
                if v.is_finite() && (0.5..=250.0).contains(&v) {
                    Ok(SelectionModifyOp::Feather(v))
                } else {
                    Err(err())
                }
            }
            Self::Expand | Self::Contract | Self::Border | Self::Smooth => {
                let v: u32 = trimmed.parse().map_err(|_| err())?;
                if (1..=500).contains(&v) {
                    Ok(match self {
                        Self::Expand => SelectionModifyOp::Expand(v),
                        Self::Contract => SelectionModifyOp::Contract(v),
                        Self::Border => SelectionModifyOp::Border(v),
                        Self::Smooth => SelectionModifyOp::Smooth(v),
                        _ => unreachable!(),
                    })
                } else {
                    Err(err())
                }
            }
            Self::Grow | Self::Similar => {
                let v: u8 = trimmed.parse().map_err(|_| err())?;
                Ok(match self {
                    Self::Grow => SelectionModifyOp::Grow(v),
                    Self::Similar => SelectionModifyOp::Similar(v),
                    _ => unreachable!(),
                })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum InAppDialog {
    SelectionModify {
        kind: SelectionModifyKind,
        value: String,
        error: Option<String>,
    },
    NewFolder {
        parent: String,
        name: String,
        error: Option<String>,
    },
    ConfirmResetLayout,
    ConfirmUnsavedLeave {
        tab_id: Option<u64>,
        close_editor: bool,
    },
    Resize {
        dims: DimensionInputState,
        filter: ResizeFilter,
        mode: ResizeMode,
        fill: Option<[u8; 3]>,
        error: Option<String>,
    },
    CanvasSize {
        dims: DimensionInputState,
        anchor: photon_protocol::CanvasAnchor,
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
    AdjustDenoise {
        strength: f32,
        detail_preserve: f32,
        target: ApplyTarget,
    },
    #[allow(dead_code)]
    ManagePlugins {
        search: String,
        selected_id: Option<String>,
    },
    PluginParams {
        plugin_id: String,
        plugin_name: String,
        specs: Vec<PluginParamSpec>,
        values: BTreeMap<String, PluginParamValue>,
        target: ApplyTarget,
        show_target: bool,
    },
    PluginModels {
        plugin_id: String,
    },
    TranslateSettings,
    Alert {
        id: DesktopDialogId,
        title: String,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub enum BrowserClipboardPaste {
    Files(Vec<String>),
    ImagePng(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct ToastState {
    pub id: u64,
    pub message: String,
    pub kind: ToastKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SettingsTab {
    #[default]
    General,
    Viewer,
    QuickAccess,
    Plugins,
    Translate,
}

impl SettingsTab {
    const fn as_str(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Viewer => "viewer",
            Self::QuickAccess => "quick_access",
            Self::Plugins => "plugins",
            Self::Translate => "translate",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EditorTabUiCache {
    pub(crate) zoom: f32,
    pub(crate) pan_x: f32,
    pub(crate) pan_y: f32,
    pub(crate) crop_uv: Option<[f32; 4]>,
    pub(crate) tool: EditorTool,
    pub(crate) pending_adj: ColorAdjustments,
    pub(crate) adjust_popup_tab: Option<AdjustmentPopupTab>,
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
    pub(crate) editor_selection_mask: Option<CpuMaskFrame>,
    pub(crate) editor_histogram: Option<(u64, Arc<[u32; 256]>)>,
    pub(crate) transform_preview: Option<TransformPreview>,
    pub(crate) transform_layer_id: Option<u64>,
    pub(crate) marching_ants_phase: f32,
    pub(crate) editor_ui: EditorUiState,
    pub(crate) browser_file_clipboard: Vec<String>,
    pub(crate) last_synced_clipboard_png: Option<Vec<u8>>,
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
    settings_tab: SettingsTab,
    settings_confirm_factory_reset: bool,
    plugins_search: String,
    plugins_selected_id: Option<String>,
    translate_draft: TranslateSettings,
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
    pub(crate) viewer_viewport_background: Option<[u8; 3]>,
    ctrl_held: bool,
    pub(crate) expanded_folders: BTreeSet<String>,
    pub(crate) folder_children: HashMap<String, Vec<(String, String)>>,
    pub(crate) viewer_translate: ViewerTranslateState,
    pub(crate) editor_ai_stroke_pts: Vec<[f64; 2]>,
    last_ai_clipboard: Option<String>,
    last_ai_error: Option<String>,
    editor_tab_view_states: HashMap<u64, EditorTabUiCache>,
    pub(crate) quick_access_options_open: bool,
    pub(crate) quick_access_editing: bool,
    canvas_size_draft: Option<(
        DimensionInputState,
        photon_protocol::CanvasAnchor,
        Option<[u8; 3]>,
    )>,
    reopen_canvas_size_after_picker: bool,
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
    OpenSelectionModifyDialog(SelectionModifyKind),
    SelectionModifyValueChanged(String),
    ConfirmSelectionModify,
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
    OpenCanvasSizeDialog,
    CanvasSizeDimAction(DimGridAction),
    CanvasSizeAnchorChanged(photon_protocol::CanvasAnchor),
    ConfirmCanvasSize,
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
    SetAppLaunchWindowMode(AppLaunchWindowMode),
    SetAntiAliasingMode(CanvasAntiAliasingMode),
    SetViewportDenoiseMode(ViewportDenoiseMode),
    WebtoonScrolled(scrollable::Viewport),
    WebtoonPagesLoaded(Vec<(String, u32, u32, Vec<u8>)>),
    Browser(BrowserMessage),
    Viewer(ViewerMessage),
    Editor(EditorMessage),
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
    SetViewerViewportBackground(Option<[u8; 3]>),
    ModifiersChanged(Modifiers),
    ShutdownAfterSave,
    SaveCurrentUiLayout,
    ApplySafeDefaultsToDisk,
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
    EditorResetActiveCurvesChannel,
    EditorApplyCurves,
    EditorZoomActual,
    CanvasPanTo {
        pan_x: f32,
        pan_y: f32,
    },
    EditorSetFloodFillOpacity(f32),
    EditorToggleFloodFillSampleAll,
    EditorSetGradientOpacity(f32),
    EditorToggleGradientReverse,
    EditorToggleGradientDither,
    EditorSetGradientRepeat(GradientRepeat),
    EditorSetGradientBlendMode(BlendMode),
    EditorToggleGradientInteractive,
    EditorApplyGradientPreset(String),
    EditorSelectGradientStop(usize),
    EditorAddGradientStop(f64),
    EditorRemoveSelectedGradientStop,
    EditorSetGradientStopPos(usize, f64),
    EditorToggleGradientStopAlpha(usize),
    EditorSetGradientStopFg(usize),
    EditorApplyGradientDraft,
    EditorCancelGradientDraft,
    EditorSetHealingMode(HealingMode),
    EditorSetHealingRadius(f32),
    EditorSetHealingHardness(f32),
    EditorSetHealingStrength(f32),
    EditorToggleHealingSampleAll,
    EditorSetHealingSource(Option<[f32; 2]>),
    EditorSetDenoiseStrength(f32),
    EditorSetDenoiseDetail(f32),
    OpenDenoiseDialog,
    DenoiseDialogStrengthChanged(f32),
    DenoiseDialogDetailChanged(f32),
    DenoiseDialogTargetChanged(ApplyTarget),
    ConfirmDenoiseDialog,
    EditorSetTransformSpec(LayerTransformSpec),
    EditorTransformFramesLoaded(Option<EditorTransformFrames>),
    EditorTransformCommit,
    EditorTransformCancel,
    OpenManagePlugins,
    ManagePluginsSearchChanged(String),
    ManagePluginsSelect(String),
    PickPluginInstallFolder,
    PluginInstallFolderPicked(Option<PathBuf>),
    OpenPluginsFolder,
    RunPlugin(String),
    PluginParamChanged(String, PluginParamValue),
    PluginParamTargetChanged(ApplyTarget),
    ConfirmPluginParams,
    BrowserCopyFiles(Vec<String>),
    BrowserPasteFiles,
    BrowserPasteResolved(Option<BrowserClipboardPaste>),
    EditorCopyPixels {
        merged: bool,
    },
    EditorCutPixels,
    EditorPastePixels {
        into_selection: bool,
    },
    EditorPasteResolved {
        into_selection: bool,
        png_bytes: Option<Vec<u8>>,
        clipboard_empty: bool,
    },
    EditorPixelsCopiedSyncClipboard(Result<CommandResponse, String>),
    RecordSyncedClipboardPng(Vec<u8>),
    EditorSetRotateAngle(f64),
    EditorApplyRotateAngle,
    EditorSwitchTab(u64),
    EditorRequestCloseTab(u64),
    EditorOpenNewTabDialog,
    EditorNewTabFilesPicked(Vec<PathBuf>),
    OpenInViewerWindow(String),
    ToggleQuickAccessOptions,
    ToggleQuickAccessEditing,
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
    EditorSelectionMaskLoaded(Option<CpuMaskFrame>),
    MarchingAntsTick,
    EditorThumbnailsLoaded(u64, Vec<photon_image::LayerThumbnail>),
    EditorSetSelectOp(SelectionBooleanOp),
    EditorSetSelectFeather(f32),
    EditorToggleSelectAntiAlias,
    EditorSetWandTolerance(u8),
    EditorToggleWandContiguous,
    EditorToggleSelectSampleAll,
    EditorSetQuickSelectRadius(f32),
    EditorSetBrushSmoothingMode(photon_protocol::BrushSmoothingMode),
    EditorSetStabilizerDistance(f32),
    EditorSetShapeFillMode(photon_protocol::ShapeFillMode),
    EditorSetShapeStrokeWidth(f32),
    EditorSetFloodFillTolerance(u8),
    EditorToggleFloodFillContiguous,
    EditorSetGradientKind(photon_protocol::GradientKind),
    EditorSetGradientColorMode(photon_protocol::GradientColorMode),
    TriggerWindowScreenshot(window::Id),
    ScreenshotReady(window::Id, window::Screenshot),
    OpenSettings,
    CloseSettings,
    SelectSettingsTab(SettingsTab),
    RequestFactoryReset,
    PluginsSearchChanged(String),
    PluginsSelect(String),
    SetTranslateProvider(TranslateProvider),
    TranslateEndpointChanged(String),
    TranslateApiKeyChanged(String),
    TranslateSourceLangChanged(String),
    TranslateTargetLangChanged(String),
    SaveTranslateSettings,
    PickCustomModelFile(String),
    CustomModelFilePicked(String, Option<PathBuf>),
    OpenPluginModels(String),
    OpenTranslateSettings,
    CopyText(String),
    ViewerTranslate(ViewerMessage),
    EditorAi(EditorMessage),
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
    let remembered_size = Size::new(
        (snapshot.meta.layout.window_width as f32).clamp(680.0, 3840.0),
        (snapshot.meta.layout.window_height as f32).clamp(500.0, 2160.0),
    );
    let launch_mode = snapshot.meta.layout.system_open_window_mode;
    let start_fullscreen = opened_with_files && launch_mode == SystemOpenWindowMode::Fullscreen;
    let start_maximized = !opened_with_files
        && (snapshot.meta.layout.app_launch_window_mode == AppLaunchWindowMode::Maximized
            || (snapshot.meta.layout.app_launch_window_mode == AppLaunchWindowMode::Remembered
                && snapshot.meta.layout.window_maximized));
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
        // Ignore initial WM map/configure/maximize resize jitter so remembered size isn't overwritten.
        suppress_resize_until =
            Some(Instant::now() + Duration::from_millis(if start_maximized { 1200 } else { 600 }));
        remembered_size
    };

    let (id, open_task) = window::open(window::Settings {
        size: init_size,
        min_size: Some(Size::new(680.0, 500.0)),
        ..Default::default()
    });

    let mut windows = BTreeMap::new();
    windows.insert(
        id,
        if is_standalone_viewer() {
            WindowRole::AuxiliaryViewer
        } else {
            WindowRole::Main
        },
    );

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
    let viewer_viewport_background = init_layout.viewer_viewport_background;
    let initial_plugin_id = snapshot.meta.plugins.first().map(|p| p.id.clone());
    let initial_translate_draft = snapshot.meta.translate_settings.clone();

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
        editor_selection_mask: None,
        editor_histogram: None,
        transform_preview: None,
        transform_layer_id: None,
        marching_ants_phase: 0.0,
        editor_ui,
        browser_file_clipboard: Vec::new(),
        last_synced_clipboard_png: None,
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
        settings_tab: SettingsTab::General,
        settings_confirm_factory_reset: false,
        plugins_search: String::new(),
        plugins_selected_id: initial_plugin_id,
        translate_draft: initial_translate_draft.clone(),
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
        viewer_viewport_background,
        ctrl_held: false,
        expanded_folders: BTreeSet::new(),
        folder_children: HashMap::new(),
        viewer_translate: ViewerTranslateState {
            source_lang: initial_translate_draft.default_source_lang,
            target_lang: initial_translate_draft.default_target_lang,
            ..Default::default()
        },
        editor_ai_stroke_pts: Vec::new(),
        last_ai_clipboard: None,
        last_ai_error: None,
        editor_tab_view_states: HashMap::new(),
        quick_access_options_open: false,
        quick_access_editing: false,
        canvas_size_draft: None,
        reopen_canvas_size_after_picker: false,
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
    let max_task = if start_maximized && !start_fullscreen {
        window::maximize(id, true)
    } else {
        Task::none()
    };
    (
        app,
        Task::batch([
            open_task.map(Message::WindowOpened),
            fs_task,
            max_task,
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
                let (left_slot, right_slot) = dual_spread_slots(state.viewer_view_state(true));
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

pub fn subscription(state: &PhotonGui) -> Subscription<Message> {
    let mut subs = vec![
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
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => {
                Some(Message::ModifiersChanged(m))
            }
            _ => None,
        }),
        window::close_events().map(Message::WindowClosed),
        window::resize_events().map(|(id, size)| Message::WindowResized(id, size)),
    ];
    if state.snapshot.meta.workspace == Workspace::Editor
        && state
            .snapshot
            .meta
            .editor
            .as_ref()
            .is_some_and(|ed| ed.selection.has_selection)
    {
        subs.push(iced::time::every(Duration::from_millis(60)).map(|_| Message::MarchingAntsTick));
    }
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
                let (actual_zoom, min_z, max_z) =
                    actual_zoom_for_bounds(iw.max(1.0) as u32, ih.max(1.0) as u32, vp_w, vp_h);
                state.zoom = actual_zoom.clamp(min_z, max_z);
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
                open_in_editor_or_forward(state, path)
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
            state.settings_tab = SettingsTab::General;
            state.settings_confirm_factory_reset = false;
            state.translate_draft = state.snapshot.meta.translate_settings.clone();
            if state.plugins_selected_id.is_none() {
                state.plugins_selected_id =
                    state.snapshot.meta.plugins.first().map(|p| p.id.clone());
            }
            open_dialog_window(state, DesktopDialogId::Settings);
            Task::batch([
                dispatch_to_core(state.core.clone(), Command::SaveUiState { force: false }),
                sync_ui_tree_task(state),
            ])
        }
        Message::CloseSettings => {
            state.settings_open = false;
            state.settings_confirm_factory_reset = false;
            state.dialog_manager.close(&DesktopDialogId::Settings);
            Task::batch([
                dispatch_to_core(state.core.clone(), Command::SaveUiState { force: false }),
                sync_ui_tree_task(state),
            ])
        }
        Message::ModifiersChanged(m) => {
            state.ctrl_held = m.control();
            Task::none()
        }
        Message::ShutdownAfterSave => iced::exit(),
        Message::SaveCurrentUiLayout => {
            state.active_menu = None;
            let save_task =
                dispatch_to_core(state.core.clone(), Command::SaveUiState { force: true });
            let toast_task = show_toast(
                state,
                tr(state.snapshot.meta.language, "toast-ui-layout-saved"),
                ToastKind::Success,
            );
            Task::batch([save_task, toast_task])
        }
        Message::ApplySafeDefaultsToDisk => {
            state.active_menu = None;
            dispatch_to_core(
                state.core.clone(),
                Command::ResetConfig {
                    reset_layout: true,
                    reset_settings: true,
                },
            )
        }
        Message::SetViewerViewportBackground(bg) => {
            state.active_menu = None;
            state.viewer_viewport_background = bg;
            commit_layout_with(state, |layout| {
                layout.viewer_viewport_background = bg;
            })
        }
        Message::SelectSettingsTab(tab) => {
            state.settings_tab = tab;
            state.settings_confirm_factory_reset = false;
            sync_ui_tree_task(state)
        }
        Message::RequestFactoryReset => {
            if !state.settings_confirm_factory_reset {
                state.settings_confirm_factory_reset = true;
                return Task::none();
            }
            state.settings_confirm_factory_reset = false;
            dispatch_to_core(
                state.core.clone(),
                Command::ResetConfig {
                    reset_layout: true,
                    reset_settings: true,
                },
            )
        }
        Message::PluginsSearchChanged(val) => {
            state.plugins_search = val;
            Task::none()
        }
        Message::PluginsSelect(id) => {
            state.plugins_selected_id = Some(id);
            Task::none()
        }
        Message::SetTranslateProvider(provider) => {
            state.translate_draft.provider = provider;
            dispatch_to_core(
                state.core.clone(),
                Command::SetTranslateSettings {
                    settings: state.translate_draft.clone(),
                },
            )
        }
        Message::TranslateEndpointChanged(val) => {
            state.translate_draft.libre_endpoint = val;
            Task::none()
        }
        Message::TranslateApiKeyChanged(val) => {
            let trimmed = val.trim().to_owned();
            state.translate_draft.api_key = if trimmed.is_empty() { None } else { Some(val) };
            Task::none()
        }
        Message::TranslateSourceLangChanged(val) => {
            state.translate_draft.default_source_lang = val;
            Task::none()
        }
        Message::TranslateTargetLangChanged(val) => {
            state.translate_draft.default_target_lang = val;
            Task::none()
        }
        Message::SaveTranslateSettings => dispatch_to_core(
            state.core.clone(),
            Command::SetTranslateSettings {
                settings: state.translate_draft.clone(),
            },
        ),
        Message::PickCustomModelFile(model_id) => {
            let title = tr(state.snapshot.meta.language, "action-local-file");
            Task::perform(
                async move { photon_dialogs::open_file(&title, None, &[]).await },
                move |picked| Message::CustomModelFilePicked(model_id, picked),
            )
        }
        Message::CustomModelFilePicked(model_id, Some(path)) => dispatch_to_core(
            state.core.clone(),
            Command::AiSetCustomModel {
                model_id,
                path: path.to_string_lossy().into_owned(),
            },
        ),
        Message::CustomModelFilePicked(_, None) => Task::none(),
        Message::OpenPluginModels(plugin_id) => {
            state.active_menu = None;
            state.context_menu = None;
            if plugin_id == "photon.translate" {
                return update(state, Message::OpenTranslateSettings);
            }
            state.dialog = Some(InAppDialog::PluginModels { plugin_id });
            open_dialog_window(state, DesktopDialogId::PluginModels);
            Task::none()
        }
        Message::OpenTranslateSettings => {
            state.active_menu = None;
            state.context_menu = None;
            state.translate_draft = state.snapshot.meta.translate_settings.clone();
            state.dialog = Some(InAppDialog::TranslateSettings);
            open_dialog_window(state, DesktopDialogId::TranslateSettings);
            Task::none()
        }
        Message::CopyText(value) => {
            let toast_task = show_toast(
                state,
                tr(state.snapshot.meta.language, "toast-clipboard-copied"),
                ToastKind::Success,
            );
            Task::batch([iced::clipboard::write(value), toast_task])
        }
        Message::ViewerTranslate(vmsg) => handle_viewer_translate_message(state, vmsg),
        Message::EditorAi(emsg) => handle_editor_ai_message(state, emsg),
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
                let max_w = (mon.width * 0.94).max(680.0).min(mon.width);
                let max_h = (mon.height * 0.90).max(500.0).min(mon.height);
                if state.window_size.width > max_w || state.window_size.height > max_h {
                    let clamped = Size::new(
                        state.window_size.width.min(max_w).max(680.0),
                        state.window_size.height.min(max_h).max(500.0),
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
                if !is_standalone_viewer()
                    && (state.snapshot.meta.save_ui_state_on_exit || state.ctrl_held)
                {
                    let core = state.core.clone();
                    let force = state.ctrl_held;
                    return Task::perform(
                        async move {
                            let env = Envelope::new(
                                format!("gui-exit-{}", std::process::id()),
                                ClientKind::Gui,
                                Command::SaveUiState { force },
                            );
                            let _ = core.dispatch(env).await;
                        },
                        |_| Message::ShutdownAfterSave,
                    );
                }
                return iced::exit();
            }
            sync_ui_tree_task(state)
        }
        Message::WindowResized(id, size) => {
            if state.main_window == Some(id) {
                state.window_size = size;
                if is_standalone_viewer() {
                    return sync_ui_tree_task(state);
                }
                if state
                    .suppress_resize_until
                    .is_some_and(|until| Instant::now() < until)
                {
                    return sync_ui_tree_task(state);
                }
                state.suppress_resize_until = None;
                let w = (size.width.round() as u32).clamp(680, 3840);
                let h = (size.height.round() as u32).clamp(500, 2160);
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
            if seq != state.resize_save_seq || state.is_fullscreen {
                return Task::none();
            }
            let layout = &state.snapshot.meta.layout;
            if is_maximized {
                if layout.window_maximized {
                    return Task::none();
                }
                return commit_layout_with(state, |layout| {
                    layout.window_maximized = true;
                });
            }
            if !layout.window_maximized && layout.window_width == w && layout.window_height == h {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.window_maximized = false;
                layout.window_width = w;
                layout.window_height = h;
            })
        }
        Message::StateUpdated(snap) => {
            let prev_ws = state.snapshot.meta.workspace;
            let prev_tab_id = state.snapshot.meta.active_editor_tab_id;
            let next_tab_id = snap.meta.active_editor_tab_id;
            let focus_changed =
                snap.meta.focus_request_seq != state.snapshot.meta.focus_request_seq;
            let viewer_changed = snap.meta.viewer_path != state.snapshot.meta.viewer_path;
            let was_ai_running = state
                .snapshot
                .meta
                .ai_task
                .as_ref()
                .is_some_and(|t| t.running);
            let prev_ai_saved = state
                .snapshot
                .meta
                .ai_task
                .as_ref()
                .and_then(|t| t.saved_path.clone());
            let caption_changed = snap.meta.editor_caption != state.snapshot.meta.editor_caption;
            let translation_changed = snap.meta.translation != state.snapshot.meta.translation;
            let saved_tr_changed =
                snap.meta.viewer_saved_translation != state.snapshot.meta.viewer_saved_translation;
            let root_changed = snap.meta.root_folder != state.snapshot.meta.root_folder;
            let tree_changed = snap.meta.tree_revision != state.snapshot.meta.tree_revision;
            let folder_changed = snap.meta.current_folder != state.snapshot.meta.current_folder;
            let listing_changed = snap.meta.listing_epoch != state.snapshot.meta.listing_epoch;
            let editor_changed = snap
                .meta
                .editor
                .as_ref()
                .map(|e| (e.id, e.path.clone(), e.pixel_revision))
                != state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|e| (e.id, e.path.clone(), e.pixel_revision));
            let editor_orig_changed = snap
                .meta
                .editor
                .as_ref()
                .map(|e| (e.id, e.path.clone(), e.original_revision))
                != state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|e| (e.id, e.path.clone(), e.original_revision));
            let editor_selection_changed = snap.meta.editor.as_ref().map(|e| {
                (
                    e.id,
                    e.path.clone(),
                    e.selection.selection_revision,
                    e.selection.has_selection,
                )
            }) != state.snapshot.meta.editor.as_ref().map(|e| {
                (
                    e.id,
                    e.path.clone(),
                    e.selection.selection_revision,
                    e.selection.has_selection,
                )
            });
            let editor_active_layer_changed = snap
                .meta
                .editor
                .as_ref()
                .map(|e| (e.id, e.path.clone(), e.active_layer_id))
                != state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|e| (e.id, e.path.clone(), e.active_layer_id));
            let editor_dims_changed = snap
                .meta
                .editor
                .as_ref()
                .map(|e| (e.id, e.path.clone(), e.width, e.height))
                != state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|e| (e.id, e.path.clone(), e.width, e.height));
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
            if !state.settings_open
                || snap.meta.translate_settings != state.snapshot.meta.translate_settings
            {
                state.translate_draft = snap.meta.translate_settings.clone();
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
                state.viewer_viewport_background = snap.meta.layout.viewer_viewport_background;
                sync_editor_panel_compat(state);
            }
            let open_tab_ids: BTreeSet<u64> = snap.meta.editor_tabs.iter().map(|t| t.id).collect();
            state
                .editor_tab_view_states
                .retain(|id, _| open_tab_ids.contains(id));

            if (prev_tab_id != next_tab_id
                || (prev_ws == Workspace::Editor && snap.meta.workspace != Workspace::Editor))
                && prev_ws == Workspace::Editor
                && let Some(old_id) = prev_tab_id
                && open_tab_ids.contains(&old_id)
            {
                state.editor_tab_view_states.insert(
                    old_id,
                    EditorTabUiCache {
                        zoom: state.zoom,
                        pan_x: state.pan_x,
                        pan_y: state.pan_y,
                        crop_uv: state.crop_uv,
                        tool: state.editor_ui.tool,
                        pending_adj: state.pending_adj,
                        adjust_popup_tab: state.editor_ui.adjust_popup_tab,
                    },
                );
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
                    if let Some(cached) = next_tab_id
                        .and_then(|id| state.editor_tab_view_states.get(&id))
                        .cloned()
                    {
                        state.zoom = cached.zoom;
                        state.pan_x = cached.pan_x;
                        state.pan_y = cached.pan_y;
                        state.crop_uv = cached.crop_uv;
                        state.editor_ui.tool = cached.tool;
                        state.pending_adj = cached.pending_adj;
                        state.editor_ui.adjust_popup_tab = cached.adjust_popup_tab;
                        if let Some(tab) = cached.adjust_popup_tab {
                            open_dialog_window(state, adjustment_dialog_id(tab));
                        }
                    } else {
                        state.zoom = 1.0;
                        state.pan_x = 0.0;
                        state.pan_y = 0.0;
                    }
                } else {
                    state.transform_preview = None;
                    state.transform_layer_id = None;
                    state.editor_ui.transform_spec = LayerTransformSpec::default();
                }
            } else if prev_tab_id != next_tab_id && snap.meta.workspace == Workspace::Editor {
                state.active_menu = None;
                state.context_menu = None;
                state.editor_ui.renaming_layer_id = None;
                close_all_adjustment_dialogs(state);
                state.transform_preview = None;
                state.transform_layer_id = None;
                state.editor_ui.transform_spec = LayerTransformSpec::default();
                if let Some(cached) = next_tab_id
                    .and_then(|id| state.editor_tab_view_states.get(&id))
                    .cloned()
                {
                    state.zoom = cached.zoom;
                    state.pan_x = cached.pan_x;
                    state.pan_y = cached.pan_y;
                    state.crop_uv = cached.crop_uv;
                    state.editor_ui.tool = cached.tool;
                    state.pending_adj = cached.pending_adj;
                    state.editor_ui.adjust_popup_tab = cached.adjust_popup_tab;
                    if let Some(tab) = cached.adjust_popup_tab {
                        open_dialog_window(state, adjustment_dialog_id(tab));
                    }
                } else {
                    state.zoom = 1.0;
                    state.pan_x = 0.0;
                    state.pan_y = 0.0;
                    state.crop_uv = None;
                    state.editor_ui.adjust_popup_tab = None;
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
            if caption_changed || editor_changed {
                state.editor_ui.ai_caption_draft = state
                    .snapshot
                    .meta
                    .editor_caption
                    .clone()
                    .unwrap_or_default();
            }
            if viewer_changed {
                state.viewer_translate.picking_region = false;
                state.viewer_translate.editing_block_idx = None;
                state.viewer_translate.blocks.clear();
                state.viewer_translate.saved_sidecar = false;
                state.viewer_translate.last_path = state.snapshot.meta.viewer_path.clone();
            }
            if (saved_tr_changed || viewer_changed)
                && let Some(saved) = &state.snapshot.meta.viewer_saved_translation
                && Some(saved.path.as_str()) == state.snapshot.meta.viewer_path.as_deref()
            {
                state.viewer_translate.blocks = saved.blocks.clone();
                state.viewer_translate.saved_sidecar = true;
            } else if saved_tr_changed && state.snapshot.meta.viewer_saved_translation.is_none() {
                state.viewer_translate.saved_sidecar = false;
            }
            if translation_changed
                && let Some(tr_res) = &state.snapshot.meta.translation
                && Some(tr_res.path.as_str()) == state.snapshot.meta.viewer_path.as_deref()
            {
                state.viewer_translate.blocks = tr_res.blocks.clone();
                state.viewer_translate.saved_sidecar =
                    state.snapshot.meta.viewer_saved_translation.is_some();
            }
            if let Some(task) = state.snapshot.meta.ai_task.clone()
                && !task.running
            {
                if let Some(err) = &task.error
                    && state.last_ai_error.as_ref() != Some(err)
                {
                    state.last_ai_error = Some(err.clone());
                    tasks.push(show_toast(state, err.clone(), ToastKind::Error));
                } else {
                    if task.error.is_none() {
                        state.last_ai_error = None;
                    }
                    if let Some(clip) = &task.clipboard_text
                        && !clip.is_empty()
                        && (was_ai_running || state.last_ai_clipboard.as_ref() != Some(clip))
                    {
                        state.last_ai_clipboard = Some(clip.clone());
                        tasks.push(iced::clipboard::write(clip.clone()));
                        tasks.push(show_toast(
                            state,
                            tr(state.snapshot.meta.language, "toast-clipboard-copied"),
                            ToastKind::Success,
                        ));
                    }
                    if task.saved_path.is_some()
                        && task.saved_path != prev_ai_saved
                        && state.snapshot.meta.workspace == Workspace::Viewer
                        && let Some(vp) = state.snapshot.meta.viewer_path.clone()
                    {
                        tasks.push(load_viewer_frame_task(vp));
                    }
                }
            } else if state
                .snapshot
                .meta
                .ai_task
                .as_ref()
                .is_some_and(|t| t.running)
            {
                state.last_ai_error = None;
            }
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
                    let (left_slot, right_slot) = dual_spread_slots(state.viewer_view_state(true));
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
                    if let Some(y) = webtoon_offset_for_index(state.viewer_view_state(true), center)
                    {
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
                let (left_slot, right_slot) = dual_spread_slots(state.viewer_view_state(true));
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
            if editor_active_layer_changed && state.editor_ui.tool == EditorTool::Transform {
                state.transform_layer_id = state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|ed| ed.active_layer_id);
                state.editor_ui.transform_spec = LayerTransformSpec::default();
            }
            if editor_changed {
                if state.snapshot.meta.editor.is_some() {
                    tasks.push(load_editor_frame_task(state.core.clone()));
                    tasks.push(load_editor_thumbnails_task(state.core.clone()));
                    if state.editor_ui.tool == EditorTool::Transform {
                        tasks.push(load_editor_transform_frames_task(
                            state.core.clone(),
                            state.transform_layer_id,
                        ));
                    }
                } else {
                    state.editor_frame = None;
                    state.editor_histogram = None;
                    state.transform_preview = None;
                    state.transform_layer_id = None;
                    state.editor_layer_thumbs.clear();
                    state.editor_mask_thumbs.clear();
                }
            } else if (editor_selection_changed || editor_active_layer_changed)
                && state.snapshot.meta.editor.is_some()
                && state.editor_ui.tool == EditorTool::Transform
            {
                tasks.push(load_editor_transform_frames_task(
                    state.core.clone(),
                    state.transform_layer_id,
                ));
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
            if editor_selection_changed {
                if state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .is_some_and(|ed| ed.selection.has_selection)
                {
                    tasks.push(load_editor_selection_mask_task(state.core.clone()));
                } else {
                    state.editor_selection_mask = None;
                }
            }
            if viewer_changed && is_standalone_viewer() {
                let active_canonical_path = state
                    .snapshot
                    .meta
                    .viewer_path
                    .as_deref()
                    .map(canonicalize_path_str)
                    .unwrap_or_default();
                let open_canonical_paths = if active_canonical_path.as_os_str().is_empty() {
                    Vec::new()
                } else {
                    vec![active_canonical_path.clone()]
                };
                tasks.push(Task::perform(
                    async move {
                        let app_paths = photon_storage::AppPaths::current();
                        let pid = std::process::id();
                        let info = photon_ipc::ViewerInstanceInfo {
                            pid,
                            socket_path: photon_ipc::viewer_socket_path(&app_paths, pid),
                            active_canonical_path,
                            open_canonical_paths,
                        };
                        let _ = photon_ipc::update_viewer_instance(&app_paths, &info);
                    },
                    |_| Message::Noop,
                ));
            }
            if focus_changed && let Some(win_id) = state.main_window {
                tasks.push(window::minimize(win_id, false));
                tasks.push(window::gain_focus(win_id));
                tasks.push(window::request_user_attention(
                    win_id,
                    Some(window::UserAttention::Informational),
                ));
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
                        | "plugins_refreshed"
                        | "plugin_enabled"
                        | "plugin_disabled"
                        | "plugin_installed"
                        | "plugin_uninstalled"
                        | "plugin_executed"
                        | "toast_copied"
                        | "toast_cut"
                        | "toast_pasted"
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
        Message::ImagesPicked(paths) if !paths.is_empty() => {
            let path_strings: Vec<String> = paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            if is_standalone_viewer() {
                if let Some(first) = path_strings.first().cloned() {
                    let core = state.core.clone();
                    return Task::perform(
                        async move {
                            let app_paths = photon_storage::AppPaths::current();
                            let canon = canonicalize_path_str(&first);
                            let _lock = photon_ipc::acquire_viewer_path_lock(
                                &app_paths,
                                &canon,
                                Duration::from_secs(2),
                            );
                            if let Some(matched) =
                                photon_ipc::find_live_viewer_for_path(&app_paths, &canon)
                            {
                                if matched.info.pid != std::process::id() {
                                    let env = Envelope::new(
                                        format!("gui-{}", std::process::id()),
                                        ClientKind::Gui,
                                        Command::FocusWindow,
                                    );
                                    let _ = photon_ipc::send_once(
                                        &matched.info.socket_path,
                                        &photon_ipc::IpcRequest::Dispatch { envelope: env },
                                    )
                                    .await;
                                    return None;
                                }
                            }
                            let env = Envelope::new(
                                format!("gui-{}", std::process::id()),
                                ClientKind::Gui,
                                Command::OpenFiles {
                                    paths: path_strings,
                                },
                            );
                            Some(core.dispatch(env).await)
                        },
                        |res| match res {
                            Some(r) => Message::CommandCompleted(r),
                            None => Message::Noop,
                        },
                    );
                }
                Task::none()
            } else {
                dispatch_to_core(
                    state.core.clone(),
                    Command::OpenFiles {
                        paths: path_strings,
                    },
                )
            }
        }
        Message::ImagesPicked(_) => Task::none(),
        Message::EditorOpenNewTabDialog => {
            state.active_menu = None;
            state.context_menu = None;
            let title = tr(state.snapshot.meta.language, "tooltip-editor-new-tab");
            let start = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .and_then(|d| Path::new(&d.path).parent().map(Path::to_path_buf))
                .or_else(|| {
                    state
                        .snapshot
                        .meta
                        .current_folder
                        .as_deref()
                        .map(PathBuf::from)
                });
            Task::perform(
                async move { photon_dialogs::open_images(&title, start).await },
                Message::EditorNewTabFilesPicked,
            )
        }
        Message::EditorNewTabFilesPicked(paths) if !paths.is_empty() => {
            let core = state.core.clone();
            Task::perform(
                async move {
                    let mut last_resp: Option<Result<CommandResponse, String>> = None;
                    for path in paths {
                        let env = Envelope::new(
                            format!("gui-{}", std::process::id()),
                            ClientKind::Gui,
                            Command::EditorOpen {
                                path: path.to_string_lossy().into_owned(),
                            },
                        );
                        last_resp = Some(core.dispatch(env).await);
                    }
                    last_resp
                },
                |res| match res {
                    Some(r) => Message::CommandCompleted(r),
                    None => Message::Noop,
                },
            )
        }
        Message::EditorNewTabFilesPicked(_) => Task::none(),
        Message::OpenInViewerWindow(path) => open_or_focus_viewer_process(vec![path]),
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
        Message::OpenSelectionModifyDialog(kind) => {
            state.active_menu = None;
            state.dialog = Some(InAppDialog::SelectionModify {
                kind,
                value: kind.default_value().to_owned(),
                error: None,
            });
            open_dialog_window(state, DesktopDialogId::SelectionModify);
            Task::none()
        }
        Message::SelectionModifyValueChanged(val) => {
            if let Some(InAppDialog::SelectionModify { value, error, .. }) = &mut state.dialog {
                *value = val;
                *error = None;
            }
            Task::none()
        }
        Message::ConfirmSelectionModify => {
            let Some(InAppDialog::SelectionModify { kind, value, .. }) = state.dialog.clone()
            else {
                return Task::none();
            };
            match kind.parse_and_validate(&value, state.snapshot.meta.language) {
                Ok(op) => {
                    state.dialog = None;
                    state
                        .dialog_manager
                        .close(&DesktopDialogId::SelectionModify);
                    dispatch_to_core(state.core.clone(), Command::EditorSelectModify { op })
                }
                Err(msg) => {
                    if let Some(InAppDialog::SelectionModify { error, .. }) = &mut state.dialog {
                        *error = Some(msg);
                    }
                    Task::none()
                }
            }
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
            if !close_editor {
                state.pending_leave_to_browser = false;
                return dispatch_to_core(state.core.clone(), Command::WorkspaceBack);
            }
            state.pending_leave_to_browser = false;
            if let Some(doc) = &state.snapshot.meta.editor {
                if doc.modified {
                    state.dialog = Some(InAppDialog::ConfirmUnsavedLeave {
                        tab_id: Some(doc.id),
                        close_editor: true,
                    });
                    open_dialog_window(state, DesktopDialogId::ConfirmUnsavedLeave);
                    return Task::none();
                }
                return dispatch_to_core(
                    state.core.clone(),
                    Command::EditorCloseTab {
                        tab_id: Some(doc.id),
                    },
                );
            }
            dispatch_to_core(state.core.clone(), Command::WorkspaceBack)
        }
        Message::RequestEditorLeaveToBrowser => {
            state.active_menu = None;
            state.pending_leave_to_browser = false;
            dispatch_to_core(
                state.core.clone(),
                Command::SetWorkspace {
                    workspace: Workspace::Browser,
                },
            )
        }
        Message::EditorSwitchTab(tab_id) => {
            state.active_menu = None;
            state.context_menu = None;
            dispatch_to_core(state.core.clone(), Command::EditorSwitchTab { tab_id })
        }
        Message::EditorRequestCloseTab(id) => {
            state.active_menu = None;
            state.context_menu = None;
            let is_modified = state
                .snapshot
                .meta
                .editor_tabs
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.modified)
                .unwrap_or_else(|| {
                    state
                        .snapshot
                        .meta
                        .editor
                        .as_ref()
                        .is_some_and(|d| d.id == id && d.modified)
                });
            if is_modified {
                state.dialog = Some(InAppDialog::ConfirmUnsavedLeave {
                    tab_id: Some(id),
                    close_editor: true,
                });
                open_dialog_window(state, DesktopDialogId::ConfirmUnsavedLeave);
                return Task::none();
            }
            dispatch_to_core(
                state.core.clone(),
                Command::EditorCloseTab { tab_id: Some(id) },
            )
        }
        Message::DiscardAndLeaveEditor(close_editor) => {
            let to_browser = std::mem::take(&mut state.pending_leave_to_browser);
            let tab_id = match state.dialog.take() {
                Some(InAppDialog::ConfirmUnsavedLeave { tab_id, .. }) => tab_id,
                _ => None,
            };
            state
                .dialog_manager
                .close(&DesktopDialogId::ConfirmUnsavedLeave);
            let core = state.core.clone();
            Task::perform(
                async move {
                    let close_cmd = match tab_id {
                        Some(tab_id) => Command::EditorCloseTab {
                            tab_id: Some(tab_id),
                        },
                        None => Command::EditorClose,
                    };
                    if !close_editor {
                        let _ = core
                            .dispatch(Envelope::new(
                                format!("gui-{}", std::process::id()),
                                ClientKind::Gui,
                                close_cmd.clone(),
                            ))
                            .await;
                    }
                    let cmd = if close_editor {
                        close_cmd
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
            let tab_id = match state.dialog.take() {
                Some(InAppDialog::ConfirmUnsavedLeave { tab_id, .. }) => tab_id,
                _ => None,
            };
            let needs_switch = tab_id
                .zip(state.snapshot.meta.active_editor_tab_id)
                .is_some_and(|(target, active)| target != active);
            state
                .dialog_manager
                .close(&DesktopDialogId::ConfirmUnsavedLeave);
            let core = state.core.clone();
            Task::perform(
                async move {
                    if needs_switch && let Some(tab_id) = tab_id {
                        let _ = core
                            .dispatch(Envelope::new(
                                format!("gui-{}", std::process::id()),
                                ClientKind::Gui,
                                Command::EditorSwitchTab { tab_id },
                            ))
                            .await;
                    }
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
                                match tab_id {
                                    Some(tab_id) => Command::EditorCloseTab {
                                        tab_id: Some(tab_id),
                                    },
                                    None => Command::EditorClose,
                                }
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
                if let InAppDialog::ColorPicker {
                    target: ColorPickerTarget::ViewerViewportBg,
                    previous_viewport_bg,
                    ..
                } = &d
                {
                    state.viewer_viewport_background = *previous_viewport_bg;
                }
                if let InAppDialog::ColorPicker {
                    target: ColorPickerTarget::CanvasSizeFill,
                    ..
                } = &d
                    && std::mem::take(&mut state.reopen_canvas_size_after_picker)
                    && let Some((dims, anchor, fill)) = state.canvas_size_draft.take()
                {
                    state.dialog_manager.close(&DesktopDialogId::ColorPicker);
                    state.dialog = Some(InAppDialog::CanvasSize {
                        dims,
                        anchor,
                        fill,
                        error: None,
                    });
                    open_dialog_window(state, DesktopDialogId::CanvasSize);
                    return Task::none();
                }
                state.canvas_size_draft = None;
                state.reopen_canvas_size_after_picker = false;
                state.dialog_manager.close(&dialog_id_for_in_app(&d));
            }
            Task::none()
        }
        Message::OpenCanvasSizeDialog => {
            state.active_menu = None;
            state.context_menu = None;
            state.canvas_size_draft = None;
            state.reopen_canvas_size_after_picker = false;
            if let Some(doc) = &state.snapshot.meta.editor {
                state.dialog = Some(InAppDialog::CanvasSize {
                    dims: DimensionInputState::from_dims(doc.width, doc.height),
                    anchor: photon_protocol::CanvasAnchor::Center,
                    fill: state.editor_ui.canvas_fill,
                    error: None,
                });
                open_dialog_window(state, DesktopDialogId::CanvasSize);
            }
            Task::none()
        }
        Message::CanvasSizeDimAction(action) => {
            if let Some(InAppDialog::CanvasSize { dims, error, .. }) = &mut state.dialog {
                dims.apply(action);
                *error = None;
            }
            Task::none()
        }
        Message::CanvasSizeAnchorChanged(new_anchor) => {
            if let Some(InAppDialog::CanvasSize { anchor, .. }) = &mut state.dialog {
                *anchor = new_anchor;
            }
            Task::none()
        }
        Message::ConfirmCanvasSize => {
            let Some(InAppDialog::CanvasSize {
                dims, anchor, fill, ..
            }) = state.dialog.clone()
            else {
                return Task::none();
            };
            let w = dims.width_px;
            let h = dims.height_px;
            if (1..=32_768).contains(&w) && (1..=32_768).contains(&h) {
                state.dialog = None;
                state.canvas_size_draft = None;
                state.reopen_canvas_size_after_picker = false;
                state.editor_ui.canvas_fill = fill;
                state.dialog_manager.close(&DesktopDialogId::CanvasSize);
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorCanvasSize {
                        width: w,
                        height: h,
                        anchor,
                        fill,
                        fill_alpha: if fill.is_some() { 255 } else { 0 },
                    },
                )
            } else {
                if let Some(InAppDialog::CanvasSize { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "dialog-error-title"));
                }
                Task::none()
            }
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
                    if let Some(y) = webtoon_offset_for_index(state.viewer_view_state(true), center)
                    {
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
            if let Some(path) = step_dual_spread(state.viewer_view_state(true), delta) {
                dispatch_to_core(state.core.clone(), Command::ViewerOpen { path })
            } else {
                Task::none()
            }
        }
        Message::SetMangaRtl(rtl) => {
            state.viewer_manga_rtl = rtl;
            if state.viewer_mode == ViewerViewMode::Double {
                let (left_slot, right_slot) = dual_spread_slots(state.viewer_view_state(true));
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
        Message::SetAppLaunchWindowMode(mode) => commit_layout_with(state, |layout| {
            layout.app_launch_window_mode = mode;
        }),
        Message::SetAntiAliasingMode(mode) => {
            state.active_menu = None;
            commit_layout_with(state, |layout| {
                layout.canvas_anti_aliasing = mode;
            })
        }
        Message::SetViewportDenoiseMode(mode) => {
            state.active_menu = None;
            commit_layout_with(state, |layout| {
                layout.viewport_denoise = mode;
            })
        }
        Message::WebtoonScrolled(vp) => {
            let y = vp.absolute_offset().y;
            let vh = vp.bounds().height;
            state.webtoon_scroll_y = y;
            if let Some(idx) = webtoon_index_at_scroll(state.viewer_view_state(true), y + vh * 0.45)
            {
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
        Message::Browser(msg) => update(state, map_browser_message(msg)),
        Message::Viewer(msg) => update(state, map_viewer_message(msg)),
        Message::Editor(msg) => update(state, map_editor_message(msg)),
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
                    return open_or_focus_viewer_process(vec![path]);
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
            let (min_z, max_z) = active_workspace_zoom_bounds(state, None);
            state.zoom = (state.zoom * factor).clamp(min_z, max_z);
            sync_ui_tree_task(state)
        }
        Message::EditorZoomActual => {
            state.context_menu = None;
            if let Some(frame) = &state.editor_frame {
                let rect = resolved_editor_canvas_bounds(state);
                let (actual, min_z, max_z) =
                    actual_zoom_for_bounds(frame.width, frame.height, rect.width, rect.height);
                state.zoom = actual.clamp(min_z, max_z);
                state.pan_x = 0.0;
                state.pan_y = 0.0;
            }
            sync_ui_tree_task(state)
        }
        Message::CanvasPanTo { pan_x, pan_y } => {
            state.pan_x = pan_x;
            state.pan_y = pan_y;
            Task::none()
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
                    let (min_z, max_z) = active_workspace_zoom_bounds(state, Some(bounds));
                    apply_canvas_zoom_at_cursor(
                        &mut state.zoom,
                        &mut state.pan_x,
                        &mut state.pan_y,
                        factor,
                        cursor,
                        bounds,
                        min_z,
                        max_z,
                    );
                }
                CanvasEvent::PanBy { dx, dy } => {
                    apply_canvas_pan(&mut state.pan_x, &mut state.pan_y, dx, dy);
                }
                CanvasEvent::CropUpdated(uv) => {
                    state.crop_uv = Some(uv);
                    return Task::none();
                }
                CanvasEvent::HealingSourcePicked([x, y]) => {
                    state.editor_ui.healing_source_px = Some([x as f32, y as f32]);
                    return Task::none();
                }
                CanvasEvent::StrokeBegin([x, y]) => {
                    if (state.editor_ui.tool == EditorTool::RemoveBg
                        && state.editor_ui.ai_rembg_refine != AlphaRefineMode::Off)
                        || (state.editor_ui.tool == EditorTool::Watermark
                            && state.editor_ui.ai_wm_tab == WatermarkTab::AiAuto
                            && state.editor_ui.ai_wm_detector == "brush")
                    {
                        state.editor_ui.stroke_active = true;
                        state.editor_ai_stroke_pts = vec![[x, y]];
                        return Task::none();
                    }
                    state.editor_ui.stroke_active = true;
                    let stroke = if state.editor_ui.tool == EditorTool::Liquify {
                        LiveStrokeKind::Liquify {
                            mode: state.editor_ui.liquify_mode,
                            radius: state.editor_ui.liquify_radius,
                            strength: state.editor_ui.liquify_strength,
                        }
                    } else if state.editor_ui.tool == EditorTool::HealingBrush {
                        LiveStrokeKind::Healing {
                            mode: state.editor_ui.healing_mode,
                            radius: state.editor_ui.healing_radius,
                            hardness: state.editor_ui.healing_hardness,
                            strength: state.editor_ui.healing_strength,
                            source: state
                                .editor_ui
                                .healing_source_px
                                .map(|[sx, sy]| [f64::from(sx), f64::from(sy)]),
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
                        if (state.editor_ui.tool == EditorTool::RemoveBg
                            && state.editor_ui.ai_rembg_refine != AlphaRefineMode::Off)
                            || (state.editor_ui.tool == EditorTool::Watermark
                                && state.editor_ui.ai_wm_tab == WatermarkTab::AiAuto
                                && state.editor_ui.ai_wm_detector == "brush")
                        {
                            state.editor_ai_stroke_pts.extend(points);
                            return Task::none();
                        }
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
                        if state.editor_ui.tool == EditorTool::RemoveBg
                            && state.editor_ui.ai_rembg_refine != AlphaRefineMode::Off
                        {
                            let pts = std::mem::take(&mut state.editor_ai_stroke_pts);
                            if commit && !pts.is_empty() {
                                let stroke = AiMaskStroke {
                                    radius: state.editor_ui.ai_rembg_refine_radius,
                                    points: pts
                                        .into_iter()
                                        .map(|[sx, sy]| [sx as f32, sy as f32])
                                        .collect(),
                                };
                                let (erase_strokes, restore_strokes) = if state
                                    .editor_ui
                                    .ai_rembg_refine
                                    == AlphaRefineMode::EraseBg
                                {
                                    (vec![stroke], Vec::new())
                                } else {
                                    (Vec::new(), vec![stroke])
                                };
                                return dispatch_to_core(
                                    state.core.clone(),
                                    Command::EditorRefineAlpha {
                                        erase_strokes,
                                        restore_strokes,
                                        softness: state.editor_ui.ai_rembg_refine_softness,
                                    },
                                );
                            }
                            return Task::none();
                        }
                        if state.editor_ui.tool == EditorTool::Watermark
                            && state.editor_ui.ai_wm_tab == WatermarkTab::AiAuto
                            && state.editor_ui.ai_wm_detector == "brush"
                        {
                            let pts = std::mem::take(&mut state.editor_ai_stroke_pts);
                            if commit && !pts.is_empty() {
                                state.editor_ui.ai_wm_mask_strokes.push(AiMaskStroke {
                                    radius: state.editor_ui.ai_wm_brush_radius,
                                    points: pts
                                        .into_iter()
                                        .map(|[sx, sy]| [sx as f32, sy as f32])
                                        .collect(),
                                });
                            }
                            return Task::none();
                        }
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
                CanvasEvent::SelectMarqueeCommit {
                    is_ellipse,
                    x,
                    y,
                    width,
                    height,
                    shift,
                    alt,
                } => {
                    if state.editor_ui.tool == EditorTool::Watermark
                        && let Some(ed) = &state.snapshot.meta.editor
                    {
                        let ew = ed.width.max(1) as f32;
                        let eh = ed.height.max(1) as f32;
                        state.editor_ui.ai_wm_region_x = ((x / ew) * 100.0).clamp(0.0, 99.0);
                        state.editor_ui.ai_wm_region_y = ((y / eh) * 100.0).clamp(0.0, 99.0);
                        state.editor_ui.ai_wm_region_w = ((width / ew) * 100.0).clamp(1.0, 100.0);
                        state.editor_ui.ai_wm_region_h = ((height / eh) * 100.0).clamp(1.0, 100.0);
                        return Task::none();
                    }
                    let op = resolve_select_op(state, shift, alt);
                    let shape = if is_ellipse {
                        SelectionShape::Ellipse {
                            cx: x + width * 0.5,
                            cy: y + height * 0.5,
                            rx: width * 0.5,
                            ry: height * 0.5,
                            feather: state.editor_ui.select_feather,
                            anti_alias: state.editor_ui.select_anti_alias,
                        }
                    } else {
                        SelectionShape::Rect {
                            x,
                            y,
                            width,
                            height,
                            feather: state.editor_ui.select_feather,
                            anti_alias: state.editor_ui.select_anti_alias,
                        }
                    };
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorSelectShape { shape, op },
                    );
                }
                CanvasEvent::SelectLineCommit {
                    is_row,
                    coord,
                    shift,
                    alt,
                } => {
                    let op = resolve_select_op(state, shift, alt);
                    let shape = if is_row {
                        SelectionShape::SingleRow { y: coord }
                    } else {
                        SelectionShape::SingleCol { x: coord }
                    };
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorSelectShape { shape, op },
                    );
                }
                CanvasEvent::SelectPolygonCommit { points, shift, alt } => {
                    let op = resolve_select_op(state, shift, alt);
                    let shape = SelectionShape::Polygon {
                        points,
                        feather: state.editor_ui.select_feather,
                        anti_alias: state.editor_ui.select_anti_alias,
                    };
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorSelectShape { shape, op },
                    );
                }
                CanvasEvent::SelectMagicWand { x, y, shift, alt } => {
                    let op = resolve_select_op(state, shift, alt);
                    let shape = SelectionShape::MagicWand {
                        x,
                        y,
                        tolerance: state.editor_ui.wand_tolerance,
                        contiguous: state.editor_ui.wand_contiguous,
                        sample_all_layers: state.editor_ui.select_sample_all,
                        anti_alias: state.editor_ui.select_anti_alias,
                    };
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorSelectShape { shape, op },
                    );
                }
                CanvasEvent::SelectQuickStroke { points, shift, alt } => {
                    let op = resolve_select_op(state, shift, alt);
                    let shape = SelectionShape::QuickSelectStroke {
                        points,
                        radius: state.editor_ui.quick_select_radius,
                        sample_all_layers: state.editor_ui.select_sample_all,
                    };
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorSelectShape { shape, op },
                    );
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
                CanvasEvent::DrawShapeCommit {
                    kind,
                    points,
                    nodes,
                    closed,
                } => {
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorDrawShape {
                            kind,
                            points,
                            nodes,
                            fill_mode: state.editor_ui.shape_fill_mode,
                            closed,
                            color: Some(state.editor_ui.fg_color),
                            radius: Some(state.editor_ui.shape_stroke_width),
                            opacity: Some(state.editor_ui.brush_opacity),
                        },
                    );
                }
                CanvasEvent::BezierToSelectionCommit { nodes } => {
                    let points: Vec<[f32; 2]> =
                        photon_image::drawing::tessellate_bezier_path(&nodes, true, 0.5)
                            .into_iter()
                            .map(|[x, y]| [x as f32, y as f32])
                            .collect();
                    if points.len() >= 3 {
                        let shape = SelectionShape::Polygon {
                            points,
                            feather: state.editor_ui.select_feather,
                            anti_alias: state.editor_ui.select_anti_alias,
                        };
                        return dispatch_to_core(
                            state.core.clone(),
                            Command::EditorSelectShape {
                                shape,
                                op: state.editor_ui.select_op,
                            },
                        );
                    }
                    return Task::none();
                }
                CanvasEvent::FloodFillClick { x, y } => {
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorFloodFill {
                            x,
                            y,
                            tolerance: state.editor_ui.flood_fill_tolerance,
                            contiguous: state.editor_ui.flood_fill_contiguous,
                            color: Some(state.editor_ui.fg_color),
                            opacity: Some(state.editor_ui.flood_fill_opacity),
                            sample_all_layers: state.editor_ui.flood_fill_sample_all,
                        },
                    );
                }
                CanvasEvent::DrawGradientCommit { start, end } => {
                    state.editor_ui.gradient_draft = None;
                    return dispatch_to_core(
                        state.core.clone(),
                        build_draw_gradient_command(&state.editor_ui, start, end),
                    );
                }
                CanvasEvent::GradientDraftUpdated { start, end } => {
                    state.editor_ui.gradient_draft = Some((start, end));
                    return Task::none();
                }
                CanvasEvent::GradientDraftApply => {
                    return update(state, Message::EditorApplyGradientDraft);
                }
                CanvasEvent::GradientDraftCancel => {
                    return update(state, Message::EditorCancelGradientDraft);
                }
                CanvasEvent::TransformUpdated(spec) => {
                    state.editor_ui.transform_spec = spec;
                    if let Some(tp) = &mut state.transform_preview {
                        tp.spec = spec;
                    }
                    return Task::none();
                }
                CanvasEvent::TransformCommit => {
                    return update(state, Message::EditorTransformCommit);
                }
                CanvasEvent::TransformCancel => {
                    return update(state, Message::EditorTransformCancel);
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
            if tool.is_marquee_group() {
                state.editor_ui.marquee_subtool = tool;
            } else if tool.is_lasso_group() {
                state.editor_ui.lasso_subtool = tool;
            } else if tool.is_smart_group() {
                state.editor_ui.smart_subtool = tool;
            } else if tool.is_shape_group() {
                state.editor_ui.shape_subtool = tool;
            } else if tool.is_fill_group() {
                state.editor_ui.fill_subtool = tool;
            }
            if tool == EditorTool::Crop {
                if state.crop_uv.is_none() && state.snapshot.meta.editor.is_some() {
                    state.crop_uv = Some([0.1, 0.1, 0.9, 0.9]);
                }
            } else {
                state.crop_uv = None;
            }
            if tool != EditorTool::Gradient {
                state.editor_ui.gradient_draft = None;
            }
            if tool == EditorTool::Transform {
                state.editor_ui.transform_spec = LayerTransformSpec::default();
                state.transform_layer_id = state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|ed| ed.active_layer_id);
                if state.snapshot.meta.editor.is_some() {
                    return Task::batch([
                        load_editor_transform_frames_task(
                            state.core.clone(),
                            state.transform_layer_id,
                        ),
                        sync_ui_tree_task(state),
                    ]);
                }
            } else {
                state.transform_preview = None;
                state.transform_layer_id = None;
                state.editor_ui.transform_spec = LayerTransformSpec::default();
            }
            sync_ui_tree_task(state)
        }
        Message::EditorSetSelectOp(op) => {
            state.editor_ui.select_op = op;
            Task::none()
        }
        Message::EditorSetSelectFeather(v) => {
            state.editor_ui.select_feather = v.clamp(0.0, 250.0);
            Task::none()
        }
        Message::EditorToggleSelectAntiAlias => {
            state.editor_ui.select_anti_alias = !state.editor_ui.select_anti_alias;
            Task::none()
        }
        Message::EditorSetWandTolerance(v) => {
            state.editor_ui.wand_tolerance = v;
            Task::none()
        }
        Message::EditorToggleWandContiguous => {
            state.editor_ui.wand_contiguous = !state.editor_ui.wand_contiguous;
            Task::none()
        }
        Message::EditorToggleSelectSampleAll => {
            state.editor_ui.select_sample_all = !state.editor_ui.select_sample_all;
            Task::none()
        }
        Message::EditorSetQuickSelectRadius(v) => {
            state.editor_ui.quick_select_radius = v.clamp(2.0, 200.0);
            Task::none()
        }
        Message::EditorSetBrushSmoothingMode(m) => {
            state.editor_ui.brush_settings.smoothing_mode = m;
            Task::none()
        }
        Message::EditorSetStabilizerDistance(d) => {
            state.editor_ui.brush_settings.stabilizer_distance = d.clamp(4.0, 100.0);
            Task::none()
        }
        Message::EditorSetShapeFillMode(m) => {
            state.editor_ui.shape_fill_mode = m;
            Task::none()
        }
        Message::EditorSetShapeStrokeWidth(w) => {
            state.editor_ui.shape_stroke_width = w.clamp(1.0, 64.0);
            Task::none()
        }
        Message::EditorSetFloodFillTolerance(t) => {
            state.editor_ui.flood_fill_tolerance = t;
            Task::none()
        }
        Message::EditorToggleFloodFillContiguous => {
            state.editor_ui.flood_fill_contiguous = !state.editor_ui.flood_fill_contiguous;
            Task::none()
        }
        Message::EditorSetGradientKind(k) => {
            state.editor_ui.gradient_kind = k;
            Task::none()
        }
        Message::EditorSetGradientColorMode(c) => {
            state.editor_ui.gradient_color_mode = c;
            match c {
                GradientColorMode::FgToBg => {
                    state.editor_ui.gradient_preset_id = "fg_to_bg".to_owned();
                }
                GradientColorMode::FgToTransparent => {
                    state.editor_ui.gradient_preset_id = "fg_to_transparent".to_owned();
                }
                GradientColorMode::Custom => {
                    state.editor_ui.ensure_custom_gradient_stops();
                }
            }
            Task::none()
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
            state.active_menu = None;
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
                ColorPickerTarget::CanvasSizeFill => {
                    if let Some(InAppDialog::CanvasSize {
                        dims, anchor, fill, ..
                    }) = &state.dialog
                    {
                        state.canvas_size_draft = Some((dims.clone(), *anchor, *fill));
                        state.reopen_canvas_size_after_picker = true;
                        state.editor_ui.canvas_fill = *fill;
                    }
                    match state.editor_ui.canvas_fill {
                        Some(c) => (c, false),
                        None => ([255, 255, 255], true),
                    }
                }
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
                ColorPickerTarget::ViewerViewportBg => {
                    let c = state
                        .viewer_viewport_background
                        .unwrap_or_else(|| default_viewport_bg_rgb(dark));
                    (c, false)
                }
                ColorPickerTarget::GradientStop(idx) => {
                    let stops = state.editor_ui.raw_gradient_stops();
                    if let Some(stop) = stops.get(idx) {
                        (stop.color, stop.alpha <= 0.01)
                    } else {
                        (state.editor_ui.fg_color, false)
                    }
                }
            };
            state.dialog = Some(InAppDialog::ColorPicker {
                target,
                r: rgb[0],
                g: rgb[1],
                b: rgb[2],
                hex: format_hex(rgb),
                transparent,
                previous_viewport_bg: if target == ColorPickerTarget::ViewerViewportBg {
                    state.viewer_viewport_background
                } else {
                    state.viewport_background
                },
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
                } else if *target == ColorPickerTarget::ViewerViewportBg {
                    state.viewer_viewport_background = Some([nr, ng, nb]);
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
                    } else if *target == ColorPickerTarget::ViewerViewportBg {
                        state.viewer_viewport_background = Some([nr, ng, nb]);
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
                && *target != ColorPickerTarget::ViewerViewportBg
                && *target != ColorPickerTarget::Foreground
            {
                *transparent = t;
            }
            Task::none()
        }
        Message::ResetViewportBackground => {
            let dark = state.snapshot.meta.theme == Theme::Dark;
            let def = default_viewport_bg_rgb(dark);
            if let Some(InAppDialog::ColorPicker {
                target,
                r,
                g,
                b,
                hex,
                transparent,
                ..
            }) = &mut state.dialog
                && matches!(
                    target,
                    ColorPickerTarget::ViewportBackground | ColorPickerTarget::ViewerViewportBg
                )
            {
                if *target == ColorPickerTarget::ViewerViewportBg {
                    state.viewer_viewport_background = None;
                } else {
                    state.viewport_background = None;
                }
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
                    if std::mem::take(&mut state.reopen_canvas_size_after_picker)
                        && let Some((dims, anchor, _)) = state.canvas_size_draft.take()
                    {
                        state.dialog = Some(InAppDialog::CanvasSize {
                            dims,
                            anchor,
                            fill: picked,
                            error: None,
                        });
                        open_dialog_window(state, DesktopDialogId::CanvasSize);
                    }
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
                ColorPickerTarget::ViewerViewportBg => {
                    let bg = Some([r, g, b]);
                    state.viewer_viewport_background = bg;
                    commit_layout_with(state, |layout| {
                        layout.viewer_viewport_background = bg;
                    })
                }
                ColorPickerTarget::CanvasBackground => dispatch_to_core(
                    state.core.clone(),
                    Command::EditorSetCanvasBackground {
                        color: picked,
                        transparent,
                    },
                ),
                ColorPickerTarget::GradientStop(idx) => {
                    state.editor_ui.ensure_custom_gradient_stops();
                    if let Some(stop) = state.editor_ui.gradient_stops.get_mut(idx) {
                        stop.color = [r, g, b];
                        if transparent {
                            stop.alpha = 0.0;
                        } else if stop.alpha <= 0.01 {
                            stop.alpha = 1.0;
                        }
                    }
                    Task::none()
                }
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
        Message::EditorResetActiveCurvesChannel => {
            let linear = vec![[0.0, 0.0], [1.0, 1.0]];
            match state.editor_ui.curves_channel {
                CurvesChannel::Rgb => state.editor_ui.curves.rgb = linear,
                CurvesChannel::Red => state.editor_ui.curves.red = linear,
                CurvesChannel::Green => state.editor_ui.curves.green = linear,
                CurvesChannel::Blue => state.editor_ui.curves.blue = linear,
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
        Message::EditorSetFloodFillOpacity(v) => {
            state.editor_ui.flood_fill_opacity = v.clamp(0.01, 1.0);
            Task::none()
        }
        Message::EditorToggleFloodFillSampleAll => {
            state.editor_ui.flood_fill_sample_all = !state.editor_ui.flood_fill_sample_all;
            Task::none()
        }
        Message::EditorSetGradientOpacity(v) => {
            state.editor_ui.gradient_opacity = v.clamp(0.01, 1.0);
            Task::none()
        }
        Message::EditorToggleGradientReverse => {
            state.editor_ui.gradient_reverse = !state.editor_ui.gradient_reverse;
            Task::none()
        }
        Message::EditorToggleGradientDither => {
            state.editor_ui.gradient_dither = !state.editor_ui.gradient_dither;
            Task::none()
        }
        Message::EditorSetGradientRepeat(repeat) => {
            state.editor_ui.gradient_repeat = repeat;
            Task::none()
        }
        Message::EditorSetGradientBlendMode(blend_mode) => {
            state.editor_ui.gradient_blend_mode = blend_mode;
            Task::none()
        }
        Message::EditorToggleGradientInteractive => {
            state.editor_ui.gradient_interactive = !state.editor_ui.gradient_interactive;
            if !state.editor_ui.gradient_interactive {
                state.editor_ui.gradient_draft = None;
            }
            Task::none()
        }
        Message::EditorApplyGradientPreset(preset_id) => {
            state.editor_ui.apply_gradient_preset(&preset_id);
            Task::none()
        }
        Message::EditorSelectGradientStop(idx) => {
            let count = state.editor_ui.raw_gradient_stops().len();
            state.editor_ui.gradient_selected_stop = idx.min(count.saturating_sub(1));
            Task::none()
        }
        Message::EditorAddGradientStop(pos) => {
            state.editor_ui.ensure_custom_gradient_stops();
            if state.editor_ui.gradient_stops.len() < 12 {
                let p = pos.clamp(0.0, 1.0);
                let sorted = state.editor_ui.resolved_gradient_stops();
                let (color, alpha) = sample_gradient_stops(&sorted, p);
                state.editor_ui.gradient_stops.push(GradientStop {
                    pos: p,
                    color,
                    alpha,
                });
                state.editor_ui.gradient_selected_stop =
                    state.editor_ui.gradient_stops.len().saturating_sub(1);
            }
            Task::none()
        }
        Message::EditorRemoveSelectedGradientStop => {
            state.editor_ui.ensure_custom_gradient_stops();
            let idx = state.editor_ui.gradient_selected_stop;
            if state.editor_ui.gradient_stops.len() > 2
                && idx < state.editor_ui.gradient_stops.len()
            {
                state.editor_ui.gradient_stops.remove(idx);
                state.editor_ui.gradient_selected_stop = state
                    .editor_ui
                    .gradient_selected_stop
                    .min(state.editor_ui.gradient_stops.len().saturating_sub(1));
            }
            Task::none()
        }
        Message::EditorSetGradientStopPos(idx, pos) => {
            state.editor_ui.ensure_custom_gradient_stops();
            if let Some(stop) = state.editor_ui.gradient_stops.get_mut(idx) {
                stop.pos = pos.clamp(0.0, 1.0);
            }
            Task::none()
        }
        Message::EditorToggleGradientStopAlpha(idx) => {
            state.editor_ui.ensure_custom_gradient_stops();
            if let Some(stop) = state.editor_ui.gradient_stops.get_mut(idx) {
                stop.alpha = if stop.alpha > 0.5 { 0.0 } else { 1.0 };
            }
            Task::none()
        }
        Message::EditorSetGradientStopFg(idx) => {
            state.editor_ui.ensure_custom_gradient_stops();
            if let Some(stop) = state.editor_ui.gradient_stops.get_mut(idx) {
                stop.color = state.editor_ui.fg_color;
            }
            Task::none()
        }
        Message::EditorApplyGradientDraft => {
            let Some((start, end)) = state.editor_ui.gradient_draft.take() else {
                return Task::none();
            };
            dispatch_to_core(
                state.core.clone(),
                build_draw_gradient_command(&state.editor_ui, start, end),
            )
        }
        Message::EditorCancelGradientDraft => {
            state.editor_ui.gradient_draft = None;
            Task::none()
        }
        Message::EditorSetHealingMode(m) => {
            state.editor_ui.healing_mode = m;
            Task::none()
        }
        Message::EditorSetHealingRadius(r) => {
            state.editor_ui.healing_radius = r.clamp(1.0, 200.0);
            Task::none()
        }
        Message::EditorSetHealingHardness(h) => {
            state.editor_ui.healing_hardness = h.clamp(0.0, 1.0);
            Task::none()
        }
        Message::EditorSetHealingStrength(s) => {
            state.editor_ui.healing_strength = s.clamp(0.05, 1.0);
            Task::none()
        }
        Message::EditorToggleHealingSampleAll => {
            state.editor_ui.healing_sample_all = !state.editor_ui.healing_sample_all;
            Task::none()
        }
        Message::EditorSetHealingSource(src) => {
            state.editor_ui.healing_source_px = src;
            Task::none()
        }
        Message::EditorSetDenoiseStrength(v) => {
            state.editor_ui.denoise_strength = v.clamp(0.0, 1.0);
            Task::none()
        }
        Message::EditorSetDenoiseDetail(v) => {
            state.editor_ui.denoise_detail = v.clamp(0.0, 1.0);
            Task::none()
        }
        Message::OpenDenoiseDialog => {
            state.active_menu = None;
            state.context_menu = None;
            if state.snapshot.meta.editor.is_some() {
                state.dialog = Some(InAppDialog::AdjustDenoise {
                    strength: state.editor_ui.denoise_strength,
                    detail_preserve: state.editor_ui.denoise_detail,
                    target: state.editor_ui.apply_target,
                });
                open_dialog_window(state, DesktopDialogId::AdjustDenoise);
                return sync_ui_tree_task(state);
            }
            Task::none()
        }
        Message::DenoiseDialogStrengthChanged(v) => {
            let clamped = v.clamp(0.0, 1.0);
            state.editor_ui.denoise_strength = clamped;
            if let Some(InAppDialog::AdjustDenoise { strength, .. }) = &mut state.dialog {
                *strength = clamped;
            }
            Task::none()
        }
        Message::DenoiseDialogDetailChanged(v) => {
            let clamped = v.clamp(0.0, 1.0);
            state.editor_ui.denoise_detail = clamped;
            if let Some(InAppDialog::AdjustDenoise {
                detail_preserve, ..
            }) = &mut state.dialog
            {
                *detail_preserve = clamped;
            }
            Task::none()
        }
        Message::DenoiseDialogTargetChanged(t) => {
            state.editor_ui.apply_target = t;
            if let Some(InAppDialog::AdjustDenoise { target, .. }) = &mut state.dialog {
                *target = t;
            }
            Task::none()
        }
        Message::ConfirmDenoiseDialog => {
            let Some(InAppDialog::AdjustDenoise {
                strength,
                detail_preserve,
                target,
            }) = state.dialog.take()
            else {
                return Task::none();
            };
            state.dialog_manager.close(&DesktopDialogId::AdjustDenoise);
            dispatch_to_core(
                state.core.clone(),
                Command::EditorDenoise {
                    strength,
                    detail_preserve,
                    target,
                },
            )
        }
        Message::EditorSetTransformSpec(spec) => {
            state.editor_ui.transform_spec = spec;
            if let Some(tp) = &mut state.transform_preview {
                tp.spec = spec;
            }
            Task::none()
        }
        Message::EditorTransformFramesLoaded(maybe_frames) => {
            if state.editor_ui.tool != EditorTool::Transform {
                return Task::none();
            }
            let Some(frames) = maybe_frames else {
                state.transform_preview = None;
                return Task::none();
            };
            let rev_matches = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .is_some_and(|ed| ed.pixel_revision == frames.pixel_revision);
            if !rev_matches {
                return Task::none();
            }
            let sel_rev = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .map_or(0, |ed| ed.selection.selection_revision);
            let combined_rev = frames.pixel_revision ^ sel_rev.rotate_left(32);
            state.transform_layer_id = Some(frames.layer_id);
            state.transform_preview = Some(TransformPreview {
                base_frame: Some(CpuImageFrame {
                    key: format!("editor:transform-base:{}", frames.layer_id),
                    revision: combined_rev,
                    width: frames.canvas_width,
                    height: frames.canvas_height,
                    rgba: Arc::new(frames.base_rgba_excluding_active),
                }),
                layer_frame: CpuImageFrame {
                    key: format!("editor:transform-layer:{}", frames.layer_id),
                    revision: combined_rev,
                    width: frames.layer_width,
                    height: frames.layer_height,
                    rgba: Arc::new(frames.active_layer_rgba),
                },
                layer_offset: [frames.offset_x, frames.offset_y],
                layer_size: [frames.layer_width, frames.layer_height],
                spec: state.editor_ui.transform_spec,
            });
            Task::none()
        }
        Message::EditorTransformCommit => {
            let spec = state.editor_ui.transform_spec;
            let layer_id = state.transform_layer_id;
            state.editor_ui.transform_spec = LayerTransformSpec::default();
            state.transform_preview = None;
            state.transform_layer_id = None;
            if state.editor_ui.tool == EditorTool::Transform {
                state.editor_ui.tool = EditorTool::Pointer;
            }
            if !spec.is_identity() {
                return dispatch_to_core(
                    state.core.clone(),
                    Command::EditorTransformLayer {
                        layer_id,
                        transform: spec,
                    },
                );
            }
            sync_ui_tree_task(state)
        }
        Message::EditorTransformCancel => {
            state.editor_ui.transform_spec = LayerTransformSpec::default();
            state.transform_preview = None;
            state.transform_layer_id = None;
            if state.editor_ui.tool == EditorTool::Transform {
                state.editor_ui.tool = EditorTool::Pointer;
            }
            sync_ui_tree_task(state)
        }
        Message::OpenManagePlugins => {
            state.active_menu = None;
            state.context_menu = None;
            if matches!(state.dialog, Some(InAppDialog::ManagePlugins { .. })) {
                state.dialog = None;
                state.dialog_manager.close(&DesktopDialogId::ManagePlugins);
            }
            state.settings_open = true;
            state.settings_tab = SettingsTab::Plugins;
            state.settings_confirm_factory_reset = false;
            if state.plugins_selected_id.is_none() {
                state.plugins_selected_id =
                    state.snapshot.meta.plugins.first().map(|p| p.id.clone());
            }
            open_dialog_window(state, DesktopDialogId::Settings);
            sync_ui_tree_task(state)
        }
        Message::ManagePluginsSearchChanged(val) => {
            if let Some(InAppDialog::ManagePlugins { search, .. }) = &mut state.dialog {
                *search = val;
            }
            Task::none()
        }
        Message::ManagePluginsSelect(id) => {
            if let Some(InAppDialog::ManagePlugins { selected_id, .. }) = &mut state.dialog {
                *selected_id = Some(id);
            }
            Task::none()
        }
        Message::PickPluginInstallFolder => {
            let title = tr(state.snapshot.meta.language, "action-install-plugin");
            Task::perform(
                async move { photon_dialogs::pick_folder(&title, None).await },
                Message::PluginInstallFolderPicked,
            )
        }
        Message::PluginInstallFolderPicked(Some(path)) => dispatch_to_core(
            state.core.clone(),
            Command::PluginInstall {
                source_dir: path.to_string_lossy().into_owned(),
            },
        ),
        Message::PluginInstallFolderPicked(None) => Task::none(),
        Message::OpenPluginsFolder => {
            let dir = resolve_desktop_plugins_dir();
            let _ = std::fs::create_dir_all(&dir);
            open_folder_in_file_manager(&dir);
            Task::none()
        }
        Message::RunPlugin(id) => {
            state.active_menu = None;
            state.context_menu = None;
            let Some(plugin) = state
                .snapshot
                .meta
                .plugins
                .iter()
                .find(|p| p.id == id)
                .cloned()
            else {
                return Task::none();
            };
            if plugin.params.is_empty() {
                return dispatch_to_core(
                    state.core.clone(),
                    Command::PluginRun {
                        id,
                        params: BTreeMap::new(),
                        target: state.editor_ui.apply_target,
                    },
                );
            }
            let mut values = BTreeMap::new();
            for spec in &plugin.params {
                values.insert(spec.key.clone(), default_value_for_plugin_param(spec));
            }
            let show_target = state.snapshot.meta.workspace == Workspace::Editor
                && plugin
                    .capabilities
                    .contains(&PluginCapability::EditorFilter);
            state.dialog = Some(InAppDialog::PluginParams {
                plugin_id: plugin.id,
                plugin_name: plugin.name,
                specs: plugin.params,
                values,
                target: state.editor_ui.apply_target,
                show_target,
            });
            open_dialog_window(state, DesktopDialogId::PluginParams);
            sync_ui_tree_task(state)
        }
        Message::PluginParamChanged(key, val) => {
            if let Some(InAppDialog::PluginParams { values, .. }) = &mut state.dialog {
                values.insert(key, val);
            }
            Task::none()
        }
        Message::PluginParamTargetChanged(t) => {
            state.editor_ui.apply_target = t;
            if let Some(InAppDialog::PluginParams { target, .. }) = &mut state.dialog {
                *target = t;
            }
            Task::none()
        }
        Message::ConfirmPluginParams => {
            let Some(InAppDialog::PluginParams {
                plugin_id,
                values,
                target,
                ..
            }) = state.dialog.take()
            else {
                return Task::none();
            };
            state.dialog_manager.close(&DesktopDialogId::PluginParams);
            dispatch_to_core(
                state.core.clone(),
                Command::PluginRun {
                    id: plugin_id,
                    params: values,
                    target,
                },
            )
        }
        Message::BrowserCopyFiles(paths) => {
            state.active_menu = None;
            state.context_menu = None;
            let chosen = if paths.is_empty() {
                state.snapshot.meta.selection.clone()
            } else {
                paths
            };
            if chosen.is_empty() {
                return Task::none();
            }
            state.browser_file_clipboard = chosen.clone();
            state.last_synced_clipboard_png = None;
            let toast_task = show_toast(
                state,
                tr(state.snapshot.meta.language, "toast-copied"),
                ToastKind::Success,
            );
            let uri_task = Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || write_file_uris_to_clipboard(&chosen))
                        .await
                        .unwrap_or(false)
                },
                |_| Message::Noop,
            );
            Task::batch([toast_task, uri_task])
        }
        Message::BrowserPasteFiles => {
            state.active_menu = None;
            state.context_menu = None;
            if state.snapshot.meta.current_folder.is_none() {
                return Task::none();
            }
            let in_app_files = state.browser_file_clipboard.clone();
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        resolve_browser_clipboard_paste(in_app_files)
                    })
                    .await
                    .ok()
                    .flatten()
                },
                Message::BrowserPasteResolved,
            )
        }
        Message::BrowserPasteResolved(Some(paste)) => {
            let Some(folder) = state.snapshot.meta.current_folder.clone() else {
                return Task::none();
            };
            match paste {
                BrowserClipboardPaste::Files(paths) => dispatch_to_core(
                    state.core.clone(),
                    Command::CopyTo {
                        paths,
                        destination: folder,
                    },
                ),
                BrowserClipboardPaste::ImagePng(png_bytes) => dispatch_to_core(
                    state.core.clone(),
                    Command::PasteImageBytes {
                        folder,
                        filename_hint: "pasted".to_owned(),
                        png_bytes,
                    },
                ),
            }
        }
        Message::BrowserPasteResolved(None) => show_toast(
            state,
            tr(state.snapshot.meta.language, "toast-clipboard-empty"),
            ToastKind::Info,
        ),
        Message::EditorCopyPixels { merged } => {
            state.active_menu = None;
            state.context_menu = None;
            state.browser_file_clipboard.clear();
            let core = state.core.clone();
            Task::perform(
                async move {
                    let env = Envelope::new(
                        format!("gui-{}", std::process::id()),
                        ClientKind::Gui,
                        Command::EditorCopyPixels { merged },
                    );
                    core.dispatch(env).await
                },
                Message::EditorPixelsCopiedSyncClipboard,
            )
        }
        Message::EditorCutPixels => {
            state.active_menu = None;
            state.context_menu = None;
            state.browser_file_clipboard.clear();
            let core = state.core.clone();
            Task::perform(
                async move {
                    let env = Envelope::new(
                        format!("gui-{}", std::process::id()),
                        ClientKind::Gui,
                        Command::EditorCutPixels,
                    );
                    core.dispatch(env).await
                },
                Message::EditorPixelsCopiedSyncClipboard,
            )
        }
        Message::EditorPixelsCopiedSyncClipboard(res) => {
            let toast_task = update(state, Message::CommandCompleted(res.clone()));
            if res.as_ref().is_ok_and(|r| r.ok) {
                let core = state.core.clone();
                let sync_task = Task::perform(
                    async move {
                        let Ok(Ok(val)) = core.query(Query::EditorClipboardPng).await else {
                            return None;
                        };
                        let bytes: Vec<u8> = val
                            .get("pngBytes")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())?;
                        let png_for_hash = bytes.clone();
                        tokio::task::spawn_blocking(move || {
                            let _ = write_png_bytes_to_clipboard(&bytes);
                        })
                        .await
                        .ok()?;
                        Some(png_for_hash)
                    },
                    |maybe_png| {
                        if let Some(png) = maybe_png {
                            Message::RecordSyncedClipboardPng(png)
                        } else {
                            Message::Noop
                        }
                    },
                );
                return Task::batch([toast_task, sync_task]);
            }
            toast_task
        }
        Message::RecordSyncedClipboardPng(png) => {
            state.last_synced_clipboard_png = Some(png);
            Task::none()
        }
        Message::EditorPastePixels { into_selection } => {
            state.active_menu = None;
            state.context_menu = None;
            let has_internal = state.snapshot.meta.has_pixel_clipboard;
            let last_synced = state.last_synced_clipboard_png.clone();
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let os_png = read_clipboard_png_bytes();
                        match os_png {
                            Some(bytes) => {
                                if has_internal && last_synced.as_ref() == Some(&bytes) {
                                    (None, false)
                                } else {
                                    (Some(bytes), false)
                                }
                            }
                            None => {
                                if has_internal {
                                    (None, false)
                                } else {
                                    (None, true)
                                }
                            }
                        }
                    })
                    .await
                    .unwrap_or((None, !has_internal))
                },
                move |(png_bytes, clipboard_empty)| Message::EditorPasteResolved {
                    into_selection,
                    png_bytes,
                    clipboard_empty,
                },
            )
        }
        Message::EditorPasteResolved {
            into_selection,
            png_bytes,
            clipboard_empty,
        } => {
            if clipboard_empty {
                return show_toast(
                    state,
                    tr(state.snapshot.meta.language, "toast-clipboard-empty"),
                    ToastKind::Info,
                );
            }
            dispatch_to_core(
                state.core.clone(),
                Command::EditorPastePixels {
                    png_bytes,
                    into_selection,
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
        Message::ToggleQuickAccessOptions => {
            state.quick_access_options_open = !state.quick_access_options_open;
            Task::none()
        }
        Message::ToggleQuickAccessEditing => {
            state.quick_access_editing = !state.quick_access_editing;
            state.quick_access_options_open = false;
            Task::none()
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
        Message::PanelPopupToggleBrowserRight(panel) => {
            state.active_menu = None;
            toggle_browser_right_panel(state, panel)
        }
        Message::PanelPopupToggleViewerAdj => {
            state.active_menu = None;
            state.viewer_adjustments_open = !state.viewer_adjustments_open;
            state.viewer_color_popover_open = state.viewer_adjustments_open;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::PanelPopupToggleViewerInfo => {
            state.active_menu = None;
            state.viewer_info_open = !state.viewer_info_open;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::PanelPopupToggleViewerFilmstrip => {
            state.active_menu = None;
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
            state.active_menu = None;
            let next = !state.snapshot.meta.layout.panel("editor.tools");
            let layout_task = commit_layout_with(state, |layout| {
                layout.panels.insert("editor.tools".to_owned(), next);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::PanelPopupToggleEditorPanel(panel) => {
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
            state.editor_histogram = frame
                .as_ref()
                .map(|f| (f.revision, compute_luminance_histogram(f)));
            state.editor_frame = frame;
            Task::none()
        }
        Message::EditorOriginalFrameLoaded(frame) => {
            state.editor_original_frame = frame;
            Task::none()
        }
        Message::EditorSelectionMaskLoaded(mask) => {
            state.editor_selection_mask = mask;
            Task::none()
        }
        Message::MarchingAntsTick => {
            state.marching_ants_phase = (state.marching_ants_phase + 0.08) % 1.0;
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
            state.browser_file_clipboard.clear();
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
            CanvasEvent::SelectMarqueeCommit {
                x,
                y,
                width,
                height,
                ..
            } if state.viewer_translate.active && state.viewer_translate.picking_region => {
                state.viewer_translate.picking_region = false;
                let path = pane_path
                    .or_else(|| state.snapshot.meta.viewer_path.clone())
                    .unwrap_or_default();
                if path.is_empty() {
                    return Task::none();
                }
                let target_lang = if state.viewer_translate.mode == ViewerTranslateMode::OcrOnly {
                    String::new()
                } else {
                    state.viewer_translate.target_lang.clone()
                };
                dispatch_to_core(
                    state.core.clone(),
                    Command::ViewerTranslate {
                        path,
                        source_lang: state.viewer_translate.source_lang.clone(),
                        target_lang,
                        region: Some([
                            x.round().max(0.0) as i32,
                            y.round().max(0.0) as i32,
                            width.round().max(8.0) as i32,
                            height.round().max(8.0) as i32,
                        ]),
                        ocr_engine: state.viewer_translate.ocr_engine.as_str().to_owned(),
                    },
                )
            }
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
            open_dialog_window(state, batch_dialog_id(kind));
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
                        (b.build_command(&state.snapshot), batch_dialog_id(b.kind))
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
                if !is_standalone_viewer() {
                    state.transfer_adj_to_editor = Some(state.viewer_adj);
                }
                return open_in_editor_or_forward(state, path);
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
        if !modifiers.control() && state.snapshot.meta.workspace == Workspace::Editor {
            let tab_idx = match key.as_ref() {
                Key::Character("1") => Some(0),
                Key::Character("2") => Some(1),
                Key::Character("3") => Some(2),
                Key::Character("4") => Some(3),
                Key::Character("5") => Some(4),
                Key::Character("6") => Some(5),
                Key::Character("7") => Some(6),
                Key::Character("8") => Some(7),
                Key::Character("9") => Some(8),
                _ => None,
            };
            if let Some(idx) = tab_idx
                && let Some(tab) = state.snapshot.meta.editor_tabs.get(idx)
            {
                return update(state, Message::EditorSwitchTab(tab.id));
            }
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
            if let InAppDialog::ColorPicker {
                target: ColorPickerTarget::ViewerViewportBg,
                previous_viewport_bg,
                ..
            } = &d
            {
                state.viewer_viewport_background = *previous_viewport_bg;
            }
            if let InAppDialog::ColorPicker {
                target: ColorPickerTarget::CanvasSizeFill,
                ..
            } = &d
                && std::mem::take(&mut state.reopen_canvas_size_after_picker)
                && let Some((dims, anchor, fill)) = state.canvas_size_draft.take()
            {
                state.dialog_manager.close(&DesktopDialogId::ColorPicker);
                state.dialog = Some(InAppDialog::CanvasSize {
                    dims,
                    anchor,
                    fill,
                    error: None,
                });
                open_dialog_window(state, DesktopDialogId::CanvasSize);
                return Task::none();
            }
            state.canvas_size_draft = None;
            state.reopen_canvas_size_after_picker = false;
            state.dialog_manager.close(&dialog_id_for_in_app(&d));
            return Task::none();
        }
        if state.settings_open {
            state.settings_open = false;
            state.settings_confirm_factory_reset = false;
            state.dialog_manager.close(&DesktopDialogId::Settings);
            return sync_ui_tree_task(state);
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
        if state.snapshot.meta.workspace == Workspace::Editor
            && state.editor_ui.tool == EditorTool::Transform
        {
            return update(state, Message::EditorTransformCancel);
        }
        if state.snapshot.meta.workspace == Workspace::Editor
            && state.editor_ui.tool == EditorTool::Gradient
            && state.editor_ui.gradient_draft.is_some()
        {
            return update(state, Message::EditorCancelGradientDraft);
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
                if !is_standalone_viewer() {
                    return dispatch_to_core(state.core.clone(), Command::WorkspaceBack);
                }
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
                return update(state, Message::OpenSettings);
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
            Key::Character("c") | Key::Character("C")
                if modifiers.shift() && state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::EditorCopyPixels { merged: true });
            }
            Key::Character("c") | Key::Character("C")
                if state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::EditorCopyPixels { merged: false });
            }
            Key::Character("x") | Key::Character("X")
                if state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::EditorCutPixels);
            }
            Key::Character("v") | Key::Character("V")
                if state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(
                    state,
                    Message::EditorPastePixels {
                        into_selection: modifiers.shift(),
                    },
                );
            }
            Key::Character("t") | Key::Character("T")
                if state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::EditorSelectTool(EditorTool::Transform));
            }
            Key::Character("c") | Key::Character("C")
                if modifiers.shift() && state.snapshot.meta.workspace == Workspace::Browser =>
            {
                let selected_img = state
                    .snapshot
                    .meta
                    .selection
                    .iter()
                    .find(|p| {
                        state
                            .snapshot
                            .entries
                            .iter()
                            .any(|e| &e.path == *p && e.is_image)
                    })
                    .cloned();
                if selected_img.is_some() {
                    return update(state, Message::CopyImageToClipboard(selected_img));
                }
                return Task::none();
            }
            Key::Character("c") | Key::Character("C")
                if state.snapshot.meta.workspace == Workspace::Browser =>
            {
                return update(
                    state,
                    Message::BrowserCopyFiles(state.snapshot.meta.selection.clone()),
                );
            }
            Key::Character("v") | Key::Character("V")
                if state.snapshot.meta.workspace == Workspace::Browser =>
            {
                return update(state, Message::BrowserPasteFiles);
            }
            Key::Character("c")
                if modifiers.shift() && state.snapshot.meta.workspace == Workspace::Viewer =>
            {
                return update(
                    state,
                    Message::CopyImageToClipboard(state.snapshot.meta.viewer_path.clone()),
                );
            }
            Key::Character("r") | Key::Character("R")
                if modifiers.shift() && state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::OpenCanvasSizeDialog);
            }
            Key::Character("r") if state.snapshot.meta.workspace == Workspace::Editor => {
                return update(state, Message::OpenResizeDialog);
            }
            Key::Character("w") | Key::Character("W")
                if state.snapshot.meta.workspace == Workspace::Editor =>
            {
                if let Some(doc) = &state.snapshot.meta.editor {
                    return update(state, Message::EditorRequestCloseTab(doc.id));
                }
                return Task::none();
            }
            Key::Named(Named::Tab)
                if state.snapshot.meta.workspace == Workspace::Editor
                    && state.snapshot.meta.editor_tabs.len() > 1 =>
            {
                let tabs = &state.snapshot.meta.editor_tabs;
                let cur_idx = state
                    .snapshot
                    .meta
                    .active_editor_tab_id
                    .and_then(|id| tabs.iter().position(|t| t.id == id))
                    .unwrap_or(0);
                let next_idx = if modifiers.shift() {
                    if cur_idx == 0 {
                        tabs.len() - 1
                    } else {
                        cur_idx - 1
                    }
                } else {
                    (cur_idx + 1) % tabs.len()
                };
                if let Some(tab) = tabs.get(next_idx) {
                    return update(state, Message::EditorSwitchTab(tab.id));
                }
                return Task::none();
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
            Key::Character("q") | Key::Character("Q") => {
                if is_standalone_viewer() {
                    return iced::exit();
                }
                let core = state.core.clone();
                return Task::perform(
                    async move {
                        let env = Envelope::new(
                            format!("gui-ctrlq-{}", std::process::id()),
                            ClientKind::Gui,
                            Command::SaveUiState { force: true },
                        );
                        let _ = core.dispatch(env).await;
                    },
                    |_| Message::ShutdownAfterSave,
                );
            }
            Key::Character("1") if !is_standalone_viewer() => {
                return dispatch_to_core(
                    state.core.clone(),
                    Command::SetWorkspace {
                        workspace: Workspace::Browser,
                    },
                );
            }
            Key::Character("2") if !is_standalone_viewer() => {
                return dispatch_to_core(
                    state.core.clone(),
                    Command::SetWorkspace {
                        workspace: Workspace::Viewer,
                    },
                );
            }
            Key::Character("3") if !is_standalone_viewer() => {
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
            Key::Character("a") if state.snapshot.meta.workspace == Workspace::Editor => {
                return dispatch_to_core(state.core.clone(), Command::EditorSelectAll);
            }
            Key::Character("d")
                if state.snapshot.meta.workspace == Workspace::Editor && modifiers.shift() =>
            {
                return dispatch_to_core(state.core.clone(), Command::EditorReselect);
            }
            Key::Character("d") if state.snapshot.meta.workspace == Workspace::Editor => {
                return dispatch_to_core(state.core.clone(), Command::EditorDeselect);
            }
            Key::Character("i")
                if state.snapshot.meta.workspace == Workspace::Editor && modifiers.shift() =>
            {
                return dispatch_to_core(state.core.clone(), Command::EditorSelectInvert);
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
            Key::Character("m") | Key::Character("M") => update(
                state,
                Message::EditorSelectTool(state.editor_ui.marquee_subtool),
            ),
            Key::Character("w") | Key::Character("W") => update(
                state,
                Message::EditorSelectTool(state.editor_ui.smart_subtool),
            ),
            Key::Character("q") | Key::Character("Q") => {
                dispatch_to_core(state.core.clone(), Command::EditorSelectToggleQuickMask)
            }
            Key::Named(Named::Delete)
                if state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .is_some_and(|e| e.selection.has_selection) =>
            {
                dispatch_to_core(state.core.clone(), Command::EditorSelectionClearPixels)
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
            Key::Character("u") | Key::Character("U") => update(
                state,
                Message::EditorSelectTool(state.editor_ui.shape_subtool),
            ),
            Key::Character("p") | Key::Character("P") => {
                update(state, Message::EditorSelectTool(EditorTool::BezierPen))
            }
            Key::Character("f") | Key::Character("F") => {
                update(state, Message::EditorSelectTool(EditorTool::PaintBucket))
            }
            Key::Character("g") | Key::Character("G") => {
                update(state, Message::EditorSelectTool(EditorTool::Gradient))
            }
            Key::Character("j") | Key::Character("J") => {
                update(state, Message::EditorSelectTool(EditorTool::HealingBrush))
            }
            Key::Character("x") | Key::Character("X") => update(state, Message::EditorSwapColors),
            Key::Character("0") => update(state, Message::Canvas(CanvasEvent::ResetFit)),
            Key::Character("1") => update(state, Message::EditorZoomActual),
            Key::Character("+") | Key::Character("=") => update(state, Message::ZoomStep(1.25)),
            Key::Character("-") => update(state, Message::ZoomStep(0.8)),
            Key::Named(Named::Enter) if state.editor_ui.tool == EditorTool::Transform => {
                update(state, Message::EditorTransformCommit)
            }
            Key::Named(Named::Enter)
                if state.editor_ui.tool == EditorTool::Gradient
                    && state.editor_ui.gradient_draft.is_some() =>
            {
                update(state, Message::EditorApplyGradientDraft)
            }
            Key::Character("[") => {
                if state.editor_ui.tool == EditorTool::HealingBrush {
                    update(
                        state,
                        Message::EditorSetHealingRadius(
                            (state.editor_ui.healing_radius - 2.0).max(1.0),
                        ),
                    )
                } else {
                    update(
                        state,
                        Message::EditorSetBrushRadius(
                            (state.editor_ui.brush_radius - 2.0).max(1.0),
                        ),
                    )
                }
            }
            Key::Character("]") => {
                if state.editor_ui.tool == EditorTool::HealingBrush {
                    update(
                        state,
                        Message::EditorSetHealingRadius(
                            (state.editor_ui.healing_radius + 2.0).min(200.0),
                        ),
                    )
                } else {
                    update(
                        state,
                        Message::EditorSetBrushRadius(
                            (state.editor_ui.brush_radius + 2.0).min(200.0),
                        ),
                    )
                }
            }
            // Space is handled by photon-canvas for temporary Hand pan while held.
            Key::Named(Named::Space) => Task::none(),
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
                if is_standalone_viewer() {
                    Task::none()
                } else {
                    dispatch_to_core(state.core.clone(), Command::WorkspaceBack)
                }
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
            Key::Character("l") | Key::Character("L") => update(
                state,
                Message::ViewerTranslate(ViewerMessage::ToggleTranslateOverlay),
            ),
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
        Workspace::Browser => build_browser_view(state.browser_view_state()).map(Message::Browser),
        Workspace::Viewer => build_viewer_view(
            state.viewer_view_state(!is_standalone_viewer() && role == WindowRole::Main),
        )
        .map(Message::Viewer),
        Workspace::Editor => build_editor_view(state.editor_view_state()).map(Message::Editor),
    };

    let status_left = match ws {
        Workspace::Viewer => {
            viewer_status_summary(state.viewer_view_state(role == WindowRole::Main), lang)
        }
        Workspace::Editor => editor_status_summary(state, lang),
        Workspace::Browser => {
            let s = tr_ui_text(lang, &state.snapshot.meta.status);
            if s.is_empty() { tr(lang, "ready") } else { s }
        }
    };
    let backend = GPU_BACKEND_NAME.get().map(String::as_str).unwrap_or("wgpu");
    let right_meta = if ws == Workspace::Viewer {
        let counter_prefix =
            viewer_counter_summary(state.viewer_view_state(role == WindowRole::Main))
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
            Some(ActiveMenu::WorkspaceSwitcher) => {
                with_top_right_menu_at(shell, true, 48.0, 44.0, menu_el, Message::CloseMenu)
            }
            Some(ActiveMenu::WorkspacePanels) => {
                with_top_right_menu_at(shell, true, 44.0, 44.0, menu_el, Message::CloseMenu)
            }
            Some(ActiveMenu::ReaderOptions) => with_top_right_menu_at(
                shell,
                true,
                viewer_reader_options_right_offset(
                    state.viewer_view_state(role == WindowRole::Main),
                ),
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
                build_browser_context_menu_entries(state.browser_view_state(), path)
                    .map(Message::Browser),
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
                build_quick_access_context_menu(state.browser_view_state(), path)
                    .map(Message::Browser),
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
                build_viewer_context_menu(
                    state.viewer_view_state(role == WindowRole::Main),
                    target_path.as_deref(),
                )
                .map(Message::Viewer),
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
            if let Some(raw_menu) = build_editor_canvas_context_menu(state.editor_view_state()) {
                let (menu_el, est_h) =
                    clamp_menu_height(raw_menu.map(Message::Editor), 575.0, max_menu_h);
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
            if let Some(raw_menu) =
                build_editor_layer_context_menu(state.editor_view_state(), *layer_id)
            {
                let (menu_el, est_h) =
                    clamp_menu_height(raw_menu.map(Message::Editor), 380.0, max_menu_h);
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
    let mut dialogs: Vec<HeaderedManagedDialogSpec<'_, DesktopDialogId, Message>> = Vec::new();
    if role == WindowRole::Main
        && ws == Workspace::Editor
        && let Some(active_tab) = state.editor_ui.adjust_popup_tab
    {
        dialogs.push(build_editor_adjustment_dialog(state, active_tab, &theme).into());
    }
    if state.settings_open && role == WindowRole::Main {
        let settings_header_strip = build_settings_header_strip(state);
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

        dialogs.push(HeaderedManagedDialogSpec {
            spec: dialog_spec_to_managed(
                DesktopDialogId::Settings,
                DialogSpec {
                    open: true,
                    title: tr(lang, "settings-title"),
                    width: 660.0,
                    body: settings_body,
                    footer: settings_footer.into(),
                    on_close: Message::CloseSettings,
                    on_noop: Message::Noop,
                },
                default_window_state_for(DesktopDialogId::Settings),
                OverlayMode::default(),
                lang,
                &theme,
            ),
            header_strip: Some(settings_header_strip),
        });
    }

    if role == WindowRole::Main
        && let Some(active_dialog) = &state.dialog
    {
        dialogs.push(build_in_app_dialog(state, active_dialog, &theme).into());
    }

    managed_dialog_stack_with_headers(
        with_toast,
        &state.dialog_manager,
        state.window_size,
        dialogs,
    )
}

fn build_toolbar(state: &PhotonGui, role: WindowRole, ws: Workspace) -> Element<'_, Message> {
    match ws {
        Workspace::Browser => build_browser_toolbar(
            state.browser_view_state(),
            Element::from(toolbar_right_utilities(state))
                .map(|_| BrowserMessage::ToggleWorkspacePanels),
        )
        .map(Message::Browser),
        Workspace::Viewer => build_viewer_toolbar(
            state.viewer_view_state(role == WindowRole::Main),
            Element::from(toolbar_right_utilities(state))
                .map(|_| ViewerMessage::ToggleWorkspacePanels),
        )
        .map(Message::Viewer),
        Workspace::Editor => build_editor_toolbar(
            state.editor_view_state(),
            Element::from(toolbar_right_utilities(state))
                .map(|_| EditorMessage::ToggleWorkspacePanels),
        )
        .map(Message::Editor),
    }
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
            entries.push(MenuEntry::Separator);
            entries.push(MenuEntry::Header(tr(lang, "menu-viewer-backdrop")));
            let dark_bg = default_viewport_bg_rgb(true);
            let light_bg = default_viewport_bg_rgb(false);
            let cur_bg = state.viewer_viewport_background;
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "backdrop-theme-default"),
                    Message::SetViewerViewportBackground(None),
                )
                .checked(cur_bg.is_none()),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "backdrop-dark"),
                    Message::SetViewerViewportBackground(Some(dark_bg)),
                )
                .checked(cur_bg == Some(dark_bg)),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "backdrop-light"),
                    Message::SetViewerViewportBackground(Some(light_bg)),
                )
                .checked(cur_bg == Some(light_bg)),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "backdrop-custom"),
                    Message::OpenColorPicker(ColorPickerTarget::ViewerViewportBg),
                )
                .icon(Icon::Palette)
                .checked(cur_bg.is_some_and(|c| c != dark_bg && c != light_bg)),
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
        MenuItem::new(
            tr(lang, "action-save-ui-layout"),
            Message::SaveCurrentUiLayout,
        )
        .icon(Icon::Save),
    ));
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

fn build_workspace_switcher_menu(state: &PhotonGui, ws: Workspace) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let browser_msg = if ws == Workspace::Editor {
        Message::RequestEditorLeaveToBrowser
    } else {
        Message::Dispatch(Command::SetWorkspace {
            workspace: Workspace::Browser,
        })
    };
    let viewer_msg = if ws == Workspace::Editor {
        Message::RequestEditorLeave {
            close_editor: false,
        }
    } else {
        Message::Dispatch(Command::SetWorkspace {
            workspace: Workspace::Viewer,
        })
    };
    let editor_msg = if ws == Workspace::Editor {
        Message::Noop
    } else {
        Message::Dispatch(Command::SetWorkspace {
            workspace: Workspace::Editor,
        })
    };

    let entries = vec![
        MenuEntry::Item(
            MenuItem::new(tr(lang, "workspace-browser"), browser_msg)
                .icon(Icon::Browser)
                .checked(ws == Workspace::Browser),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "workspace-viewer"), viewer_msg)
                .icon(Icon::Viewer)
                .checked(ws == Workspace::Viewer),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "workspace-editor"), editor_msg)
                .icon(Icon::Editor)
                .checked(ws == Workspace::Editor),
        ),
    ];

    menu_panel_entries(entries, 180.0)
}

fn build_active_menu(
    state: &PhotonGui,
    _role: WindowRole,
    ws: Workspace,
) -> Option<(Option<f32>, Element<'_, Message>)> {
    let menu = state.active_menu?;
    if menu == ActiveMenu::WorkspaceSwitcher {
        return Some((None, build_workspace_switcher_menu(state, ws)));
    }
    if menu == ActiveMenu::WorkspacePanels {
        return Some((None, build_workspace_panels_menu(state, ws)));
    }
    if ws == Workspace::Editor && menu == ActiveMenu::EditorAddLayer {
        return Some((
            None,
            build_editor_add_layer_menu(state.editor_view_state()).map(Message::Editor),
        ));
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
            build_browser_plugins_menu_entries(state.browser_view_state())
                .into_iter()
                .map(|e| e.map(Message::Browser))
                .collect(),
        ),
        (Workspace::Browser, ActiveMenu::BrowserWindow) => (
            Some(214.0),
            build_browser_window_menu_entries(state.browser_view_state())
                .into_iter()
                .map(|e| e.map(Message::Browser))
                .collect(),
        ),
        (Workspace::Browser, ActiveMenu::BrowserHelp) => (
            Some(276.0),
            build_browser_help_menu_entries(lang)
                .into_iter()
                .map(|e| e.map(Message::Browser))
                .collect(),
        ),
        (Workspace::Browser, ActiveMenu::BrowserSort) => {
            return Some((
                None,
                build_browser_sort_menu(state.browser_view_state()).map(Message::Browser),
            ));
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
        (Workspace::Editor, ActiveMenu::EditorEdit) => {
            let has_sel = ed.is_some_and(|e| e.selection.has_selection);
            (
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
                        MenuItem::new(tr(lang, "action-cut"), Message::EditorCutPixels)
                            .icon(Icon::Cut)
                            .shortcut("Ctrl+X")
                            .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-copy"),
                            Message::EditorCopyPixels { merged: false },
                        )
                        .icon(Icon::Copy)
                        .shortcut("Ctrl+C")
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-copy-merged"),
                            Message::EditorCopyPixels { merged: true },
                        )
                        .icon(Icon::Copy)
                        .shortcut("Ctrl+Shift+C")
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-paste"),
                            Message::EditorPastePixels {
                                into_selection: false,
                            },
                        )
                        .icon(Icon::Paste)
                        .shortcut("Ctrl+V")
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-paste-into-selection"),
                            Message::EditorPastePixels {
                                into_selection: true,
                            },
                        )
                        .icon(Icon::Paste)
                        .enabled(has_sel),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-transform"),
                            Message::EditorSelectTool(EditorTool::Transform),
                        )
                        .icon(Icon::Transform)
                        .shortcut("Ctrl+T")
                        .enabled(ed.is_some()),
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
            )
        }
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
                    MenuItem::new(tr(lang, "action-image-size"), Message::OpenResizeDialog)
                        .icon(Icon::Resize)
                        .shortcut("Ctrl+R")
                        .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-canvas-size-dialog"),
                        Message::OpenCanvasSizeDialog,
                    )
                    .icon(Icon::CanvasSize)
                    .shortcut("Ctrl+Shift+R")
                    .enabled(ed.is_some()),
                ),
            ],
        ),
        (Workspace::Editor, ActiveMenu::EditorFilter) => (
            Some(208.0),
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
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "menu-filter-denoise"), Message::OpenDenoiseDialog)
                        .icon(Icon::Denoise)
                        .enabled(ed.is_some()),
                ),
            ],
        ),
        (Workspace::Editor, ActiveMenu::EditorPlugins) => (
            Some(260.0),
            build_editor_plugins_menu_entries(state.editor_view_state())
                .into_iter()
                .map(|e| e.map(Message::Editor))
                .collect(),
        ),
        (Workspace::Editor, ActiveMenu::EditorSelect) => {
            let has_sel = ed.is_some_and(|e| e.selection.has_selection);
            let can_reselect = ed.is_some_and(|e| e.selection.can_reselect);
            let quick_mask = ed.is_some_and(|e| e.selection.quick_mask_active);
            (
                Some(152.0),
                vec![
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-all"),
                            Message::Dispatch(Command::EditorSelectAll),
                        )
                        .shortcut("Ctrl+A")
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-deselect"),
                            Message::Dispatch(Command::EditorDeselect),
                        )
                        .shortcut("Ctrl+D")
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-reselect"),
                            Message::Dispatch(Command::EditorReselect),
                        )
                        .shortcut("Ctrl+Shift+D")
                        .enabled(can_reselect),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-invert"),
                            Message::Dispatch(Command::EditorSelectInvert),
                        )
                        .shortcut("Ctrl+Shift+I")
                        .enabled(has_sel),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-feather"),
                            Message::OpenSelectionModifyDialog(SelectionModifyKind::Feather),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-expand"),
                            Message::OpenSelectionModifyDialog(SelectionModifyKind::Expand),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-contract"),
                            Message::OpenSelectionModifyDialog(SelectionModifyKind::Contract),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-border"),
                            Message::OpenSelectionModifyDialog(SelectionModifyKind::Border),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-smooth"),
                            Message::OpenSelectionModifyDialog(SelectionModifyKind::Smooth),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-grow"),
                            Message::OpenSelectionModifyDialog(SelectionModifyKind::Grow),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-similar"),
                            Message::OpenSelectionModifyDialog(SelectionModifyKind::Similar),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-quick-mask"),
                            Message::Dispatch(Command::EditorSelectToggleQuickMask),
                        )
                        .icon(Icon::QuickMask)
                        .shortcut("Q")
                        .checked(quick_mask)
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-from-alpha"),
                            Message::Dispatch(Command::EditorSelectFromLayerAlpha {
                                op: state.editor_ui.select_op,
                            }),
                        )
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-to-mask"),
                            Message::Dispatch(Command::EditorSelectionToMask),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-duplicate-layer"),
                            Message::Dispatch(Command::EditorSelectionDuplicateLayer),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-clear-pixels"),
                            Message::Dispatch(Command::EditorSelectionClearPixels),
                        )
                        .shortcut("Del")
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-crop-to-selection"),
                            Message::Dispatch(Command::EditorCropToSelection),
                        )
                        .icon(Icon::Crop)
                        .enabled(has_sel),
                    ),
                ],
            )
        }
        (Workspace::Editor, ActiveMenu::EditorView) => (
            Some(316.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-zoom-in"), Message::ZoomStep(1.25))
                        .icon(Icon::ZoomIn)
                        .shortcut("+"),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-zoom-out"), Message::ZoomStep(0.8))
                        .icon(Icon::ZoomOut)
                        .shortcut("-"),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-fit"),
                        Message::Canvas(CanvasEvent::ResetFit),
                    )
                    .icon(Icon::Fit)
                    .shortcut("0"),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-actual-size"), Message::EditorZoomActual)
                        .shortcut("1"),
                ),
                MenuEntry::Separator,
                MenuEntry::Header(tr(lang, "settings-canvas-aa")),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "aa-mode-crisp"),
                        Message::SetAntiAliasingMode(CanvasAntiAliasingMode::Crisp),
                    )
                    .checked(layout.canvas_anti_aliasing == CanvasAntiAliasingMode::Crisp),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "aa-mode-smooth"),
                        Message::SetAntiAliasingMode(CanvasAntiAliasingMode::Smooth),
                    )
                    .checked(layout.canvas_anti_aliasing == CanvasAntiAliasingMode::Smooth),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "aa-mode-high-quality"),
                        Message::SetAntiAliasingMode(CanvasAntiAliasingMode::HighQuality),
                    )
                    .checked(layout.canvas_anti_aliasing == CanvasAntiAliasingMode::HighQuality),
                ),
                MenuEntry::Separator,
                MenuEntry::Header(tr(lang, "settings-viewport-denoise")),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "denoise-mode-off"),
                        Message::SetViewportDenoiseMode(ViewportDenoiseMode::Off),
                    )
                    .checked(layout.viewport_denoise == ViewportDenoiseMode::Off),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "denoise-mode-light"),
                        Message::SetViewportDenoiseMode(ViewportDenoiseMode::Light),
                    )
                    .checked(layout.viewport_denoise == ViewportDenoiseMode::Light),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "denoise-mode-medium"),
                        Message::SetViewportDenoiseMode(ViewportDenoiseMode::Medium),
                    )
                    .checked(layout.viewport_denoise == ViewportDenoiseMode::Medium),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "denoise-mode-strong"),
                        Message::SetViewportDenoiseMode(ViewportDenoiseMode::Strong),
                    )
                    .checked(layout.viewport_denoise == ViewportDenoiseMode::Strong),
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
            Some(364.0),
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
            Some(428.0),
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
        (Workspace::Viewer, ActiveMenu::ReaderOptions) => {
            let mut entries = vec![
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
                MenuEntry::Separator,
            ];
            entries.extend(
                build_viewer_quality_menu_entries(state.viewer_view_state(true))
                    .into_iter()
                    .map(|e| e.map(Message::Viewer)),
            );
            (None, entries)
        }
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

fn build_settings_header_strip(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let dlg_rect = state
        .dialog_manager
        .state(&DesktopDialogId::Settings)
        .cloned()
        .unwrap_or_else(|| default_window_state_for(DesktopDialogId::Settings))
        .resolved_rect(state.window_size);
    let compact = dlg_rect.width < 540.0;

    let tabs = vec![
        PanelTabItem::new(
            SettingsTab::General.as_str(),
            tr(lang, "settings-tab-general"),
            state.settings_tab == SettingsTab::General,
            Message::SelectSettingsTab(SettingsTab::General),
        )
        .icon(Icon::Settings)
        .tooltip(tr(lang, "settings-tab-general")),
        PanelTabItem::new(
            SettingsTab::Viewer.as_str(),
            tr(lang, "settings-tab-viewer"),
            state.settings_tab == SettingsTab::Viewer,
            Message::SelectSettingsTab(SettingsTab::Viewer),
        )
        .icon(Icon::Viewer)
        .tooltip(tr(lang, "settings-tab-viewer")),
        PanelTabItem::new(
            SettingsTab::QuickAccess.as_str(),
            tr(lang, "settings-tab-quick-access"),
            state.settings_tab == SettingsTab::QuickAccess,
            Message::SelectSettingsTab(SettingsTab::QuickAccess),
        )
        .icon(Icon::Star)
        .tooltip(tr(lang, "settings-tab-quick-access")),
        PanelTabItem::new(
            SettingsTab::Plugins.as_str(),
            tr(lang, "settings-tab-plugins"),
            state.settings_tab == SettingsTab::Plugins,
            Message::SelectSettingsTab(SettingsTab::Plugins),
        )
        .icon(Icon::Plug)
        .tooltip(tr(lang, "settings-tab-plugins")),
        PanelTabItem::new(
            SettingsTab::Translate.as_str(),
            tr(lang, "settings-tab-translate"),
            state.settings_tab == SettingsTab::Translate,
            Message::SelectSettingsTab(SettingsTab::Translate),
        )
        .icon(Icon::Translate)
        .tooltip(tr(lang, "settings-tab-translate")),
    ];

    panel_tab_bar(tabs, compact)
}

fn build_settings_body(state: &PhotonGui) -> Element<'_, Message> {
    match state.settings_tab {
        SettingsTab::General => build_settings_general_tab(state),
        SettingsTab::Viewer => build_settings_viewer_tab(state),
        SettingsTab::QuickAccess => build_settings_quick_access_tab(state),
        SettingsTab::Plugins => build_settings_plugins_tab(state),
        SettingsTab::Translate => build_settings_translate_tab(state),
    }
}

fn build_settings_general_tab(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let meta = &state.snapshot.meta;
    let is_dark = meta.theme == Theme::Dark;
    let app_launch_mode = meta.layout.app_launch_window_mode;
    let sys_open_mode = meta.layout.system_open_window_mode;

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

    let window_session_card = section_card(
        tr(lang, "settings-section-window-session"),
        column![
            setting_row(
                tr(lang, "settings-app-launch-window"),
                row![
                    button::labeled(
                        tr(lang, "settings-launch-maximized"),
                        None,
                        ButtonVariant::Tab {
                            active: app_launch_mode == AppLaunchWindowMode::Maximized,
                        },
                        Some(Message::SetAppLaunchWindowMode(
                            AppLaunchWindowMode::Maximized,
                        )),
                    ),
                    button::labeled(
                        tr(lang, "settings-launch-remembered"),
                        None,
                        ButtonVariant::Tab {
                            active: app_launch_mode == AppLaunchWindowMode::Remembered,
                        },
                        Some(Message::SetAppLaunchWindowMode(
                            AppLaunchWindowMode::Remembered,
                        )),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
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
            text(tr(lang, "settings-system-open-hint"))
                .size(11)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).text_muted),
                }),
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
                            save_ui_state_on_exit: meta.save_ui_state_on_exit,
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
                            save_ui_state_on_exit: meta.save_ui_state_on_exit,
                        })),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            setting_row(
                tr(lang, "settings-save-ui-on-exit"),
                row![
                    button::labeled(
                        tr(lang, "action-on"),
                        None,
                        ButtonVariant::Tab {
                            active: meta.save_ui_state_on_exit,
                        },
                        Some(Message::Dispatch(Command::SetSettings {
                            theme: meta.theme,
                            language: meta.language,
                            remember_last_folder: meta.remember_last_folder,
                            hardware_acceleration: meta.hardware_acceleration,
                            save_ui_state_on_exit: true,
                        })),
                    ),
                    button::labeled(
                        tr(lang, "action-off"),
                        None,
                        ButtonVariant::Tab {
                            active: !meta.save_ui_state_on_exit,
                        },
                        Some(Message::Dispatch(Command::SetSettings {
                            theme: meta.theme,
                            language: meta.language,
                            remember_last_folder: meta.remember_last_folder,
                            hardware_acceleration: meta.hardware_acceleration,
                            save_ui_state_on_exit: false,
                        })),
                    ),
                    button::labeled(
                        tr(lang, "action-save-ui-layout"),
                        Some(Icon::Save),
                        ButtonVariant::Secondary,
                        Some(Message::SaveCurrentUiLayout),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
        ]
        .spacing(10),
    );

    let mut recovery_col = column![].spacing(8);
    if meta.safe_mode {
        recovery_col = recovery_col
            .push(
                text(tr(lang, "settings-safe-mode-note"))
                    .size(11)
                    .style(|theme| text::Style {
                        color: Some(PaletteTokens::for_theme(theme).accent),
                    }),
            )
            .push(button::labeled(
                tr(lang, "settings-safe-mode-apply"),
                Some(Icon::Check),
                ButtonVariant::Primary,
                Some(Message::ApplySafeDefaultsToDisk),
            ));
    }
    recovery_col = recovery_col
        .push(
            row![
                button::labeled(
                    tr(lang, "action-reset-layout"),
                    Some(Icon::Reset),
                    ButtonVariant::Secondary,
                    Some(Message::RequestResetLayout),
                ),
                button::labeled(
                    if state.settings_confirm_factory_reset {
                        tr(lang, "settings-factory-reset-confirm")
                    } else {
                        tr(lang, "settings-factory-reset")
                    },
                    Some(Icon::Trash),
                    if state.settings_confirm_factory_reset {
                        ButtonVariant::Danger
                    } else {
                        ButtonVariant::Secondary
                    },
                    Some(Message::RequestFactoryReset),
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .push(
            text(tr(lang, "settings-reset-hint"))
                .size(11)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).text_muted),
                }),
        );

    let recovery_card = section_card(tr(lang, "settings-section-recovery"), recovery_col);

    column![appearance_card, window_session_card, recovery_card]
        .spacing(10)
        .into()
}

fn build_settings_viewer_tab(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let meta = &state.snapshot.meta;
    let vmode = meta.layout.viewer_view_mode;
    let aa_mode = meta.layout.canvas_anti_aliasing;
    let denoise_mode = meta.layout.viewport_denoise;
    let active_backend = GPU_BACKEND_NAME.get().map(String::as_str).unwrap_or("wgpu");
    let renderer_is_gpu = active_backend == "wgpu";
    let renderer_restart_needed = !meta.safe_mode && meta.hardware_acceleration != renderer_is_gpu;

    let viewer_card = section_card(
        tr(lang, "settings-section-viewer"),
        column![
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
            setting_row(
                tr(lang, "settings-canvas-aa"),
                row![
                    button::labeled(
                        tr(lang, "aa-mode-crisp"),
                        None,
                        ButtonVariant::Tab {
                            active: aa_mode == CanvasAntiAliasingMode::Crisp,
                        },
                        Some(Message::SetAntiAliasingMode(CanvasAntiAliasingMode::Crisp)),
                    ),
                    button::labeled(
                        tr(lang, "aa-mode-smooth"),
                        None,
                        ButtonVariant::Tab {
                            active: aa_mode == CanvasAntiAliasingMode::Smooth,
                        },
                        Some(Message::SetAntiAliasingMode(CanvasAntiAliasingMode::Smooth)),
                    ),
                    button::labeled(
                        tr(lang, "aa-mode-high-quality"),
                        None,
                        ButtonVariant::Tab {
                            active: aa_mode == CanvasAntiAliasingMode::HighQuality,
                        },
                        Some(Message::SetAntiAliasingMode(
                            CanvasAntiAliasingMode::HighQuality,
                        )),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            setting_row(
                tr(lang, "settings-viewport-denoise"),
                row![
                    button::labeled(
                        tr(lang, "denoise-mode-off"),
                        None,
                        ButtonVariant::Tab {
                            active: denoise_mode == ViewportDenoiseMode::Off,
                        },
                        Some(Message::SetViewportDenoiseMode(ViewportDenoiseMode::Off)),
                    ),
                    button::labeled(
                        tr(lang, "denoise-mode-light"),
                        None,
                        ButtonVariant::Tab {
                            active: denoise_mode == ViewportDenoiseMode::Light,
                        },
                        Some(Message::SetViewportDenoiseMode(ViewportDenoiseMode::Light)),
                    ),
                    button::labeled(
                        tr(lang, "denoise-mode-medium"),
                        None,
                        ButtonVariant::Tab {
                            active: denoise_mode == ViewportDenoiseMode::Medium,
                        },
                        Some(Message::SetViewportDenoiseMode(ViewportDenoiseMode::Medium)),
                    ),
                    button::labeled(
                        tr(lang, "denoise-mode-strong"),
                        None,
                        ButtonVariant::Tab {
                            active: denoise_mode == ViewportDenoiseMode::Strong,
                        },
                        Some(Message::SetViewportDenoiseMode(ViewportDenoiseMode::Strong)),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
        ]
        .spacing(10),
    );

    let mut rendering_col = column![
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
                        save_ui_state_on_exit: meta.save_ui_state_on_exit,
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
                        save_ui_state_on_exit: meta.save_ui_state_on_exit,
                    })),
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ),
        setting_row(
            tr(lang, "settings-active-renderer"),
            text(tr(
                lang,
                if renderer_is_gpu {
                    "renderer-gpu-wgpu"
                } else {
                    "renderer-software-tiny-skia"
                },
            ))
            .size(TEXT_SIZE_CONTROL)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_secondary),
            }),
        ),
    ]
    .spacing(10);
    if renderer_restart_needed {
        rendering_col = rendering_col.push(
            text(tr(lang, "settings-restart-required-renderer"))
                .size(11)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).accent),
                }),
        );
    }
    let rendering_card = section_card(tr(lang, "settings-section-rendering"), rendering_col);

    column![viewer_card, rendering_card].spacing(10).into()
}

fn build_settings_quick_access_tab(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let meta = &state.snapshot.meta;
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
    section_card(tr(lang, "panel-quick-access"), fav_list_col)
}

fn build_settings_plugins_tab(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let ai = &state.snapshot.meta.ai_settings;
    let dlg_rect = state
        .dialog_manager
        .state(&DesktopDialogId::Settings)
        .cloned()
        .unwrap_or_else(|| default_window_state_for(DesktopDialogId::Settings))
        .resolved_rect(state.window_size);
    let list_pane_h = (dlg_rect.height - 340.0).clamp(180.0, 320.0);

    let ai_card = section_card(
        tr(lang, "settings-section-plugins-ai"),
        column![
            setting_row(
                tr(lang, "settings-plugins-ai-device"),
                row![
                    button::labeled(
                        tr(lang, "ai-device-cpu"),
                        None,
                        ButtonVariant::Tab {
                            active: ai.device == AiDevice::Cpu,
                        },
                        Some(Message::Dispatch(Command::AiSetSettings {
                            device: AiDevice::Cpu,
                            auto_unload_secs: ai.auto_unload_secs,
                        })),
                    ),
                    button::labeled(
                        tr(lang, "ai-device-auto"),
                        None,
                        ButtonVariant::Tab {
                            active: ai.device == AiDevice::Auto,
                        },
                        Some(Message::Dispatch(Command::AiSetSettings {
                            device: AiDevice::Auto,
                            auto_unload_secs: ai.auto_unload_secs,
                        })),
                    ),
                    button::labeled(
                        tr(lang, "ai-device-gpu"),
                        None,
                        ButtonVariant::Tab {
                            active: ai.device == AiDevice::Gpu,
                        },
                        Some(Message::Dispatch(Command::AiSetSettings {
                            device: AiDevice::Gpu,
                            auto_unload_secs: ai.auto_unload_secs,
                        })),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            setting_row(
                format!(
                    "{} ({}s)",
                    tr(lang, "settings-plugins-auto-unload"),
                    ai.auto_unload_secs
                ),
                row![
                    slider(10.0..=600.0, ai.auto_unload_secs as f32, move |v| {
                        Message::Dispatch(Command::AiSetSettings {
                            device: ai.device,
                            auto_unload_secs: v.round() as u32,
                        })
                    })
                    .step(10.0_f32)
                    .style(slider_style)
                    .width(Length::Fixed(180.0)),
                    button::labeled(
                        tr(lang, "action-refresh-plugins"),
                        Some(Icon::Refresh),
                        ButtonVariant::Secondary,
                        Some(Message::Dispatch(Command::AiRefreshPlugins)),
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ),
        ]
        .spacing(8),
    );

    let plugins_card = section_card(
        tr(lang, "settings-section-installed-plugins"),
        build_plugins_manager_body(
            state,
            &state.plugins_search,
            state.plugins_selected_id.as_deref(),
            Message::PluginsSearchChanged,
            Message::PluginsSelect,
            list_pane_h,
        ),
    );

    column![ai_card, plugins_card].spacing(10).into()
}

fn build_settings_translate_tab(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let draft = &state.translate_draft;
    let api_key_val = draft.api_key.as_deref().unwrap_or("");

    let translate_card = section_card(
        tr(lang, "settings-section-translate"),
        column![
            setting_row(
                tr(lang, "settings-translate-provider"),
                row![
                    button::labeled(
                        tr(lang, "translate-provider-none"),
                        None,
                        ButtonVariant::Tab {
                            active: draft.provider == TranslateProvider::None,
                        },
                        Some(Message::SetTranslateProvider(TranslateProvider::None)),
                    ),
                    button::labeled(
                        tr(lang, "translate-provider-google"),
                        None,
                        ButtonVariant::Tab {
                            active: draft.provider == TranslateProvider::GoogleWeb,
                        },
                        Some(Message::SetTranslateProvider(TranslateProvider::GoogleWeb)),
                    ),
                    button::labeled(
                        tr(lang, "translate-provider-libre"),
                        None,
                        ButtonVariant::Tab {
                            active: draft.provider == TranslateProvider::LibreTranslate,
                        },
                        Some(Message::SetTranslateProvider(
                            TranslateProvider::LibreTranslate,
                        )),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
            setting_row(
                tr(lang, "settings-translate-libre-url"),
                text_input("http://localhost:5000", &draft.libre_endpoint)
                    .on_input(Message::TranslateEndpointChanged)
                    .on_submit(Message::SaveTranslateSettings)
                    .size(TEXT_SIZE_CONTROL)
                    .padding([6, 8])
                    .style(input_style)
                    .width(Length::Fixed(260.0)),
            ),
            setting_row(
                tr(lang, "settings-translate-api-key"),
                text_input(
                    &tr(lang, "settings-translate-api-key-placeholder"),
                    api_key_val,
                )
                .on_input(Message::TranslateApiKeyChanged)
                .on_submit(Message::SaveTranslateSettings)
                .size(TEXT_SIZE_CONTROL)
                .padding([6, 8])
                .style(input_style)
                .width(Length::Fixed(260.0)),
            ),
            setting_row(
                tr(lang, "settings-translate-source-lang"),
                text_input("eng", &draft.default_source_lang)
                    .on_input(Message::TranslateSourceLangChanged)
                    .on_submit(Message::SaveTranslateSettings)
                    .size(TEXT_SIZE_CONTROL)
                    .padding([6, 8])
                    .style(input_style)
                    .width(Length::Fixed(140.0)),
            ),
            setting_row(
                tr(lang, "settings-translate-target-lang"),
                text_input("vi", &draft.default_target_lang)
                    .on_input(Message::TranslateTargetLangChanged)
                    .on_submit(Message::SaveTranslateSettings)
                    .size(TEXT_SIZE_CONTROL)
                    .padding([6, 8])
                    .style(input_style)
                    .width(Length::Fixed(140.0)),
            ),
            row![
                text(tr(lang, "settings-translate-hint"))
                    .size(11)
                    .style(|theme| text::Style {
                        color: Some(PaletteTokens::for_theme(theme).text_muted),
                    })
                    .width(Length::Fill),
                button::labeled(
                    tr(lang, "settings-translate-save"),
                    Some(Icon::Check),
                    ButtonVariant::Primary,
                    Some(Message::SaveTranslateSettings),
                ),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        ]
        .spacing(10),
    );

    column![translate_card].spacing(10).into()
}

pub(crate) fn default_window_state_for(id: DesktopDialogId) -> DialogWindowState {
    match id {
        DesktopDialogId::Settings => DialogWindowState::new(660.0, 580.0)
            .min_size(500.0, 380.0)
            .max_size(1040.0, 900.0),
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
        DesktopDialogId::CanvasSize => DialogWindowState::new(460.0, 390.0).min_size(360.0, 310.0),
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
        DesktopDialogId::AdjustCurves => DialogWindowState::new(460.0, 520.0)
            .min_size(380.0, 440.0)
            .max_size(760.0, 860.0),
        DesktopDialogId::AdjustDenoise => DialogWindowState::new(440.0, 290.0)
            .min_size(340.0, 240.0)
            .max_size(720.0, 640.0),
        DesktopDialogId::SelectionModify => {
            DialogWindowState::new(400.0, 195.0).min_size(320.0, 170.0)
        }
        DesktopDialogId::ManagePlugins => DialogWindowState::new(640.0, 480.0)
            .min_size(500.0, 380.0)
            .max_size(1040.0, 820.0),
        DesktopDialogId::PluginParams => DialogWindowState::new(440.0, 420.0)
            .min_size(340.0, 280.0)
            .max_size(760.0, 800.0),
        DesktopDialogId::PluginModels => DialogWindowState::new(540.0, 440.0)
            .min_size(400.0, 320.0)
            .max_size(860.0, 800.0),
        DesktopDialogId::TranslateSettings => DialogWindowState::new(560.0, 520.0)
            .min_size(420.0, 360.0)
            .max_size(880.0, 840.0),
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

const fn batch_dialog_id(kind: BatchDialogKind) -> DesktopDialogId {
    match kind {
        BatchDialogKind::Resize => DesktopDialogId::BatchResize,
        BatchDialogKind::Crop => DesktopDialogId::BatchCrop,
        BatchDialogKind::Convert => DesktopDialogId::BatchConvert,
    }
}

fn dialog_id_for_in_app(dialog: &InAppDialog) -> DesktopDialogId {
    match dialog {
        InAppDialog::SelectionModify { .. } => DesktopDialogId::SelectionModify,
        InAppDialog::NewFolder { .. } => DesktopDialogId::NewFolder,
        InAppDialog::ConfirmResetLayout => DesktopDialogId::ConfirmResetLayout,
        InAppDialog::ConfirmUnsavedLeave { .. } => DesktopDialogId::ConfirmUnsavedLeave,
        InAppDialog::Resize { .. } => DesktopDialogId::Resize,
        InAppDialog::CanvasSize { .. } => DesktopDialogId::CanvasSize,
        InAppDialog::Export { .. } => DesktopDialogId::Export,
        InAppDialog::ColorPicker { .. } => DesktopDialogId::ColorPicker,
        InAppDialog::Batch(b) => batch_dialog_id(b.kind),
        InAppDialog::RenameItem { .. } => DesktopDialogId::RenameItem,
        InAppDialog::ConfirmDeletePermanently { .. } => DesktopDialogId::ConfirmDeletePermanently,
        InAppDialog::AdjustDenoise { .. } => DesktopDialogId::AdjustDenoise,
        InAppDialog::ManagePlugins { .. } => DesktopDialogId::ManagePlugins,
        InAppDialog::PluginParams { .. } => DesktopDialogId::PluginParams,
        InAppDialog::PluginModels { .. } => DesktopDialogId::PluginModels,
        InAppDialog::TranslateSettings => DesktopDialogId::TranslateSettings,
        InAppDialog::Alert { id, .. } => *id,
    }
}

pub(crate) fn apply_canvas_pan(pan_x: &mut f32, pan_y: &mut f32, dx: f32, dy: f32) {
    *pan_x += dx;
    *pan_y += dy;
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_canvas_zoom_at_cursor(
    zoom: &mut f32,
    pan_x: &mut f32,
    pan_y: &mut f32,
    factor: f32,
    cursor: Option<[f32; 2]>,
    bounds: [f32; 2],
    min_zoom: f32,
    max_zoom: f32,
) {
    let old_zoom = *zoom;
    let new_zoom = (old_zoom * factor).clamp(min_zoom, max_zoom);
    let ratio = new_zoom / old_zoom.max(0.001);
    if let Some([cx, cy]) = cursor {
        let half_w = bounds[0] * 0.5;
        let half_h = bounds[1] * 0.5;
        *pan_x = (cx - half_w) - ((cx - half_w) - *pan_x) * ratio;
        *pan_y = (cy - half_h) - ((cy - half_h) - *pan_y) * ratio;
    }
    *zoom = new_zoom;
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
            let active_pts = match ui.curves_channel {
                CurvesChannel::Rgb => ui.curves.rgb.clone(),
                CurvesChannel::Red => ui.curves.red.clone(),
                CurvesChannel::Green => ui.curves.green.clone(),
                CurvesChannel::Blue => ui.curves.blue.clone(),
            };
            let pts_count = active_pts.len();
            let channel_color = match ui.curves_channel {
                CurvesChannel::Rgb => {
                    if state.snapshot.meta.theme == Theme::Dark {
                        Color::from_rgb8(228, 228, 231)
                    } else {
                        Color::from_rgb8(39, 39, 42)
                    }
                }
                CurvesChannel::Red => Color::from_rgb8(239, 68, 68),
                CurvesChannel::Green => Color::from_rgb8(34, 197, 94),
                CurvesChannel::Blue => Color::from_rgb8(59, 130, 246),
            };
            let histogram = state.editor_histogram.as_ref().map(|(_, h)| Arc::clone(h));
            let graph = curve_graph(active_pts, channel_color, histogram, |ev| match ev {
                CurveGraphEvent::PointsChanged(pts) => Message::EditorCurvesChanged(pts),
                CurveGraphEvent::ResetChannel => Message::EditorResetActiveCurvesChannel,
            });
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
                container(graph)
                    .width(Fill)
                    .align_x(iced::alignment::Horizontal::Center),
                row![
                    text(tr_ui_text(
                        lang,
                        &UiText::with("label_curve_points", [("count", pts_count.to_string())]),
                    ))
                    .size(11)
                    .style(|th| text::Style {
                        color: Some(PaletteTokens::for_theme(th).text_muted),
                    }),
                    iced::widget::space::horizontal(),
                    button::labeled(
                        tr(lang, "action-reset-channel"),
                        Some(Icon::Reset),
                        ButtonVariant::Ghost,
                        Some(Message::EditorResetActiveCurvesChannel),
                    )
                    .padding([2, 8]),
                ]
                .align_y(Alignment::Center),
                text(tr(lang, "hint-curve-usage"))
                    .size(11)
                    .style(|th| text::Style {
                        color: Some(PaletteTokens::for_theme(th).text_muted),
                    }),
                row![
                    button::labeled(
                        tr(lang, "curve-preset-linear"),
                        None,
                        ButtonVariant::Secondary,
                        Some(Message::EditorCurvesChanged(vec![[0.0, 0.0], [1.0, 1.0]])),
                    ),
                    button::labeled(
                        tr(lang, "curve-preset-s-curve"),
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
                        tr(lang, "curve-preset-fade"),
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
                row![
                    button::labeled(
                        tr(lang, "curve-preset-high-contrast"),
                        None,
                        ButtonVariant::Secondary,
                        Some(Message::EditorCurvesChanged(vec![
                            [0.0, 0.0],
                            [0.20, 0.10],
                            [0.80, 0.90],
                            [1.0, 1.0],
                        ])),
                    ),
                    button::labeled(
                        tr(lang, "curve-preset-negative"),
                        None,
                        ButtonVariant::Secondary,
                        Some(Message::EditorCurvesChanged(vec![[0.0, 1.0], [1.0, 0.0]])),
                    ),
                ]
                .spacing(6),
            ]
            .spacing(6)
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

fn build_plugins_manager_body<'a>(
    state: &'a PhotonGui,
    search: &str,
    selected_id: Option<&str>,
    on_search: impl Fn(String) -> Message + 'a,
    on_select: impl Fn(String) -> Message + 'a,
    list_pane_h: f32,
) -> Element<'a, Message> {
    let lang = state.snapshot.meta.language;
    let query = search.trim().to_ascii_lowercase();
    let plugins: Vec<&PluginInfo> = state
        .snapshot
        .meta
        .plugins
        .iter()
        .filter(|p| {
            query.is_empty()
                || p.name.to_ascii_lowercase().contains(&query)
                || p.id.to_ascii_lowercase().contains(&query)
                || p.description.to_ascii_lowercase().contains(&query)
        })
        .collect();
    let active_plugin = selected_id
        .and_then(|sid| plugins.iter().copied().find(|p| p.id == sid))
        .or_else(|| plugins.first().copied());

    let top_bar = row![
        text_input(&tr(lang, "label-plugin-search"), search)
            .on_input(on_search)
            .size(TEXT_SIZE_CONTROL)
            .padding([6, 8])
            .style(input_style)
            .width(Fill),
        button::labeled(
            tr(lang, "action-install-plugin"),
            Some(Icon::Plus),
            ButtonVariant::Secondary,
            Some(Message::PickPluginInstallFolder),
        ),
        button::labeled(
            tr(lang, "action-open-plugins-folder"),
            Some(Icon::FolderOpen),
            ButtonVariant::Ghost,
            Some(Message::OpenPluginsFolder),
        ),
        button::compact_icon_tipped(
            Icon::Refresh,
            ButtonVariant::Ghost,
            Some(Message::Dispatch(Command::PluginRefresh)),
            tr(lang, "action-reload-plugins"),
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let mut list_col = column![].spacing(4).width(Fill);
    if plugins.is_empty() {
        list_col = list_col.push(
            text(tr(lang, "menu-plugins-no-enabled"))
                .size(12)
                .style(|th| text::Style {
                    color: Some(PaletteTokens::for_theme(th).text_muted),
                }),
        );
    } else {
        for p in &plugins {
            let is_sel = active_plugin.is_some_and(|a| a.id == p.id);
            let status_text = if p.error.is_some() {
                "!".to_owned()
            } else if p.enabled {
                tr(lang, "label-plugin-enabled")
            } else {
                tr(lang, "label-plugin-disabled")
            };
            list_col = list_col.push(
                button::labeled(
                    format!("{} ({status_text})", p.name),
                    Some(Icon::Plug),
                    ButtonVariant::Tab { active: is_sel },
                    Some(on_select(p.id.clone())),
                )
                .width(Fill),
            );
        }
    }

    let details: Element<'a, Message> = if let Some(p) = active_plugin {
        let caps = if p.capabilities.is_empty() {
            "—".to_owned()
        } else {
            p.capabilities
                .iter()
                .map(|c| match c {
                    PluginCapability::EditorFilter => "editor_filter",
                    PluginCapability::BrowserAction => "browser_action",
                    PluginCapability::ViewerAction => "viewer_action",
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let perms = if p.permissions.is_empty() {
            tr(lang, "label-plugin-no-permissions")
        } else {
            p.permissions.join(", ")
        };
        let can_run = p.enabled
            && p.available
            && p.error.is_none()
            && ((state.snapshot.meta.workspace == Workspace::Editor
                && state.snapshot.meta.editor.is_some()
                && p.capabilities.contains(&PluginCapability::EditorFilter))
                || (state.snapshot.meta.workspace == Workspace::Browser
                    && p.capabilities.contains(&PluginCapability::BrowserAction))
                || (state.snapshot.meta.workspace == Workspace::Viewer
                    && state.snapshot.meta.viewer_path.is_some()
                    && p.capabilities.contains(&PluginCapability::ViewerAction)));
        let has_plugin_settings = !p.models.is_empty() || p.id == "photon.translate";
        let avail_label = if p.available {
            tr(lang, "plugin-status-ready")
        } else {
            tr(lang, "plugin-status-unavailable")
        };

        let mut det_col = column![
            row![
                text(p.name.clone()).size(16),
                iced::widget::space::horizontal(),
                text(avail_label).size(11).style(move |th| text::Style {
                    color: Some(if p.available {
                        PaletteTokens::for_theme(th).success
                    } else {
                        PaletteTokens::for_theme(th).warning
                    }),
                }),
            ]
            .align_y(Alignment::Center),
            text(format!(
                "{}: {} · {}: {}",
                tr(lang, "label-plugin-version"),
                p.version,
                tr(lang, "label-plugin-author"),
                if p.author.is_empty() {
                    "—"
                } else {
                    &p.author
                }
            ))
            .size(11)
            .style(|th| text::Style {
                color: Some(PaletteTokens::for_theme(th).text_muted),
            }),
        ]
        .spacing(6);
        if !p.description.is_empty() {
            det_col = det_col.push(text(p.description.clone()).size(12));
        }
        det_col = det_col
            .push(text(format!("{}: {caps}", tr(lang, "label-plugin-capabilities"))).size(12))
            .push(text(format!("{}: {perms}", tr(lang, "label-plugin-permissions"))).size(12))
            .push(
                text(p.install_dir.clone())
                    .size(11)
                    .style(|th| text::Style {
                        color: Some(PaletteTokens::for_theme(th).text_muted),
                    }),
            );
        if let Some(err) = &p.error {
            det_col = det_col.push(text(err.clone()).size(12).style(|th| text::Style {
                color: Some(PaletteTokens::for_theme(th).danger),
            }));
        }
        let mut btn_row = row![
            button::labeled(
                if p.enabled {
                    tr(lang, "action-off")
                } else {
                    tr(lang, "action-on")
                },
                None,
                ButtonVariant::Secondary,
                Some(Message::Dispatch(Command::PluginSetEnabled {
                    id: p.id.clone(),
                    enabled: !p.enabled,
                })),
            ),
            button::labeled(
                tr(lang, "action-run-plugin"),
                Some(Icon::Check),
                ButtonVariant::Primary,
                can_run.then(|| Message::RunPlugin(p.id.clone())),
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if has_plugin_settings {
            btn_row = btn_row.push(button::labeled(
                tr(lang, "action-plugin-settings-models"),
                Some(Icon::Settings),
                ButtonVariant::Secondary,
                Some(Message::OpenPluginModels(p.id.clone())),
            ));
        }
        btn_row = btn_row
            .push(iced::widget::space::horizontal())
            .push(button::labeled(
                tr(lang, "action-uninstall-plugin"),
                Some(Icon::Trash),
                ButtonVariant::Danger,
                Some(Message::Dispatch(Command::PluginUninstall {
                    id: p.id.clone(),
                })),
            ));
        det_col = det_col.push(btn_row);
        scrollable(det_col).height(Fill).width(Fill).into()
    } else {
        container(
            text(tr(lang, "label-plugin-none-selected"))
                .size(12)
                .style(|th| text::Style {
                    color: Some(PaletteTokens::for_theme(th).text_muted),
                }),
        )
        .padding(12)
        .width(Fill)
        .height(Fill)
        .into()
    };

    column![
        top_bar,
        row![
            container(scrollable(list_col).height(Fill))
                .width(Length::Fixed(220.0))
                .height(Fill),
            container(details).width(Fill).height(Fill),
        ]
        .spacing(12)
        .height(Length::Fixed(list_pane_h)),
    ]
    .spacing(10)
    .width(Fill)
    .into()
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
        InAppDialog::SelectionModify { kind, value, error } => dialog_spec_to_managed(
            id,
            prompt_dialog_spec(
                PromptDialogSpec::new(
                    tr(lang, kind.title_key()),
                    tr(lang, kind.hint_key()),
                    value.clone(),
                    tr(lang, "dialog-select-modify-apply"),
                    tr(lang, "dialog-cancel"),
                    Message::SelectionModifyValueChanged,
                    Message::ConfirmSelectionModify,
                    Message::CloseDialog,
                    Message::Noop,
                )
                .label(tr(lang, kind.label_key()))
                .error(error.clone()),
            ),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
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
        InAppDialog::ConfirmUnsavedLeave {
            tab_id,
            close_editor,
        } => {
            let doc_name = tab_id
                .and_then(|tid| {
                    state
                        .snapshot
                        .meta
                        .editor_tabs
                        .iter()
                        .find(|t| t.id == tid)
                        .map(|t| t.name.clone())
                })
                .or_else(|| state.snapshot.meta.editor.as_ref().map(|d| d.name.clone()))
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
        InAppDialog::CanvasSize {
            dims,
            anchor,
            fill,
            error,
        } => {
            let active_anchor_idx = photon_protocol::CanvasAnchor::ALL
                .iter()
                .position(|a| a == anchor)
                .unwrap_or(4);
            let mut body_col = column![
                dims.view(lang, Message::CanvasSizeDimAction),
                row![
                    column![
                        text(tr(lang, "label-canvas-anchor")).size(12),
                        anchor_grid_3x3(active_anchor_idx, |idx| {
                            Message::CanvasSizeAnchorChanged(
                                photon_protocol::CanvasAnchor::ALL[idx],
                            )
                        }),
                    ]
                    .spacing(6),
                    iced::widget::space::horizontal(),
                    column![
                        text(tr(lang, "label-canvas-fill")).size(12),
                        checker_swatch(
                            fill.map(rgb_to_color),
                            44.0,
                            24.0,
                            Some(Message::OpenColorPicker(ColorPickerTarget::CanvasSizeFill)),
                        ),
                    ]
                    .spacing(6)
                    .align_x(Alignment::End),
                ]
                .align_y(Alignment::Start),
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
                    tr(lang, "action-apply"),
                    Some(Icon::Check),
                    ButtonVariant::Primary,
                    Some(Message::ConfirmCanvasSize),
                ),
            ]
            .align_y(Alignment::Center);

            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-canvas-size-title"),
                    width: 440.0,
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
        InAppDialog::Batch(batch_state) => {
            let defaults = default_window_state_for(id);
            let spec = match batch_state.kind {
                BatchDialogKind::Resize => {
                    build_batch_resize_dialog(&state.snapshot, batch_state, id, defaults, theme)
                }
                BatchDialogKind::Crop => {
                    build_batch_crop_dialog(&state.snapshot, batch_state, id, defaults, theme)
                }
                BatchDialogKind::Convert => {
                    build_batch_convert_dialog(&state.snapshot, batch_state, id, defaults, theme)
                }
            };
            map_batch_dialog_spec(spec)
        }
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
            map_editor_dialog_spec(build_export_dialog(
                lang,
                path,
                *quality,
                max_edge,
                *metadata,
                error.as_deref(),
            )),
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
            map_editor_dialog_spec(build_color_picker_dialog(
                lang,
                *target,
                *r,
                *g,
                *b,
                hex,
                *transparent,
            )),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::AdjustDenoise {
            strength,
            detail_preserve,
            target,
        } => {
            let body = column![
                row![
                    text(tr(lang, "label-apply-target")).size(11),
                    iced::widget::space::horizontal(),
                    button::labeled(
                        tr(lang, "target-active-layer"),
                        None,
                        ButtonVariant::Tab {
                            active: *target == ApplyTarget::ActiveLayer,
                        },
                        Some(Message::DenoiseDialogTargetChanged(
                            ApplyTarget::ActiveLayer
                        )),
                    ),
                    button::labeled(
                        tr(lang, "target-whole-image"),
                        None,
                        ButtonVariant::Tab {
                            active: *target == ApplyTarget::WholeImage,
                        },
                        Some(Message::DenoiseDialogTargetChanged(ApplyTarget::WholeImage)),
                    ),
                    button::labeled(
                        tr(lang, "target-new-layer"),
                        None,
                        ButtonVariant::Tab {
                            active: *target == ApplyTarget::NewLayer,
                        },
                        Some(Message::DenoiseDialogTargetChanged(ApplyTarget::NewLayer)),
                    ),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
                row![
                    text(tr(lang, "label-strength")).size(12),
                    iced::widget::space::horizontal(),
                    text(format!("{:.0}%", *strength * 100.0)).size(12),
                ],
                slider(0.05..=1.0, *strength, Message::DenoiseDialogStrengthChanged)
                    .step(0.05_f32)
                    .style(slider_style),
                row![
                    text(tr(lang, "label-preserve-detail")).size(12),
                    iced::widget::space::horizontal(),
                    text(format!("{:.0}%", *detail_preserve * 100.0)).size(12),
                ],
                slider(
                    0.0..=1.0,
                    *detail_preserve,
                    Message::DenoiseDialogDetailChanged
                )
                .step(0.05_f32)
                .style(slider_style),
            ]
            .spacing(8);

            let footer = row![
                button::labeled(
                    tr(lang, "dialog-cancel"),
                    None,
                    ButtonVariant::Secondary,
                    Some(Message::CloseDialog),
                ),
                iced::widget::space::horizontal(),
                button::labeled(
                    tr(lang, "action-apply"),
                    Some(Icon::Check),
                    ButtonVariant::Primary,
                    Some(Message::ConfirmDenoiseDialog),
                ),
            ]
            .align_y(Alignment::Center);

            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-adjust-denoise"),
                    width: 440.0,
                    body: body.into(),
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
        InAppDialog::ManagePlugins {
            search,
            selected_id,
        } => {
            let dlg_rect = state
                .dialog_manager
                .state(&DesktopDialogId::ManagePlugins)
                .cloned()
                .unwrap_or_else(|| default_window_state_for(id))
                .resolved_rect(state.window_size);
            let list_pane_h = (dlg_rect.height - 180.0).max(220.0);
            let body = build_plugins_manager_body(
                state,
                search,
                selected_id.as_deref(),
                Message::ManagePluginsSearchChanged,
                Message::ManagePluginsSelect,
                list_pane_h,
            );

            let footer = row![
                iced::widget::space::horizontal(),
                button::labeled(
                    tr(lang, "action-close"),
                    None,
                    ButtonVariant::Primary,
                    Some(Message::CloseDialog),
                ),
            ];

            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-manage-plugins-title"),
                    width: 640.0,
                    body,
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
        InAppDialog::PluginParams {
            plugin_name,
            specs,
            values,
            target,
            show_target,
            ..
        } => {
            let mut body_col = column![text(plugin_name.clone()).size(14)].spacing(8);
            if *show_target {
                body_col = body_col.push(
                    row![
                        text(tr(lang, "label-apply-target")).size(11),
                        iced::widget::space::horizontal(),
                        button::labeled(
                            tr(lang, "target-active-layer"),
                            None,
                            ButtonVariant::Tab {
                                active: *target == ApplyTarget::ActiveLayer,
                            },
                            Some(Message::PluginParamTargetChanged(ApplyTarget::ActiveLayer)),
                        ),
                        button::labeled(
                            tr(lang, "target-whole-image"),
                            None,
                            ButtonVariant::Tab {
                                active: *target == ApplyTarget::WholeImage,
                            },
                            Some(Message::PluginParamTargetChanged(ApplyTarget::WholeImage)),
                        ),
                        button::labeled(
                            tr(lang, "target-new-layer"),
                            None,
                            ButtonVariant::Tab {
                                active: *target == ApplyTarget::NewLayer,
                            },
                            Some(Message::PluginParamTargetChanged(ApplyTarget::NewLayer)),
                        ),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                );
            }

            for spec in specs {
                let key = spec.key.clone();
                let label = spec.label.clone();
                let current = values
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| default_value_for_plugin_param(spec));
                match spec.kind {
                    PluginParamKind::Float => {
                        let min = spec.min.unwrap_or(0.0);
                        let max = spec.max.unwrap_or(1.0).max(min + 0.001);
                        let step = spec.step.unwrap_or(0.05).max(0.001);
                        let val = match current {
                            PluginParamValue::Float(v) => v,
                            PluginParamValue::Int(v) => v as f64,
                            _ => min,
                        }
                        .clamp(min, max);
                        body_col = body_col.push(
                            column![
                                row![
                                    text(label).size(12),
                                    iced::widget::space::horizontal(),
                                    text(format!("{val:.2}")).size(12),
                                ],
                                slider(min..=max, val, move |v| {
                                    Message::PluginParamChanged(
                                        key.clone(),
                                        PluginParamValue::Float(v),
                                    )
                                })
                                .step(step)
                                .style(slider_style),
                            ]
                            .spacing(4),
                        );
                    }
                    PluginParamKind::Int => {
                        let min = spec.min.unwrap_or(0.0).round() as i64;
                        let max = (spec.max.unwrap_or(100.0).round() as i64).max(min + 1);
                        let step = spec.step.unwrap_or(1.0).max(1.0);
                        let val = match current {
                            PluginParamValue::Int(v) => v,
                            PluginParamValue::Float(v) => v.round() as i64,
                            _ => min,
                        }
                        .clamp(min, max);
                        body_col = body_col.push(
                            column![
                                row![
                                    text(label).size(12),
                                    iced::widget::space::horizontal(),
                                    text(val.to_string()).size(12),
                                ],
                                slider(min as f64..=max as f64, val as f64, move |v| {
                                    Message::PluginParamChanged(
                                        key.clone(),
                                        PluginParamValue::Int(v.round() as i64),
                                    )
                                })
                                .step(step)
                                .style(slider_style),
                            ]
                            .spacing(4),
                        );
                    }
                    PluginParamKind::Bool => {
                        let checked = matches!(current, PluginParamValue::Bool(true));
                        body_col = body_col.push(
                            checkbox(checked)
                                .label(label)
                                .text_size(TEXT_SIZE_CONTROL)
                                .on_toggle(move |b| {
                                    Message::PluginParamChanged(
                                        key.clone(),
                                        PluginParamValue::Bool(b),
                                    )
                                }),
                        );
                    }
                    PluginParamKind::Select => {
                        let selected = match current {
                            PluginParamValue::Select(s) => s,
                            _ => spec.options.first().cloned().unwrap_or_default(),
                        };
                        let mut opts_row = row![].spacing(4);
                        for opt in &spec.options {
                            let active = opt == &selected;
                            let k = key.clone();
                            let o = opt.clone();
                            opts_row = opts_row.push(button::labeled(
                                opt.clone(),
                                None,
                                ButtonVariant::Tab { active },
                                Some(Message::PluginParamChanged(k, PluginParamValue::Select(o))),
                            ));
                        }
                        body_col =
                            body_col.push(column![text(label).size(12), opts_row].spacing(4));
                    }
                    PluginParamKind::Color => {
                        let rgb = match current {
                            PluginParamValue::Color(c) => c,
                            _ => [0, 0, 0],
                        };
                        let k0 = key.clone();
                        let k1 = key.clone();
                        let k2 = key;
                        body_col = body_col.push(
                            column![
                                row![
                                    text(label).size(12),
                                    iced::widget::space::horizontal(),
                                    text(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]))
                                        .size(11),
                                ],
                                slider(0.0..=255.0, f32::from(rgb[0]), move |v| {
                                    Message::PluginParamChanged(
                                        k0.clone(),
                                        PluginParamValue::Color([v.round() as u8, rgb[1], rgb[2]]),
                                    )
                                })
                                .step(1.0_f32)
                                .style(slider_style),
                                slider(0.0..=255.0, f32::from(rgb[1]), move |v| {
                                    Message::PluginParamChanged(
                                        k1.clone(),
                                        PluginParamValue::Color([rgb[0], v.round() as u8, rgb[2]]),
                                    )
                                })
                                .step(1.0_f32)
                                .style(slider_style),
                                slider(0.0..=255.0, f32::from(rgb[2]), move |v| {
                                    Message::PluginParamChanged(
                                        k2.clone(),
                                        PluginParamValue::Color([rgb[0], rgb[1], v.round() as u8]),
                                    )
                                })
                                .step(1.0_f32)
                                .style(slider_style),
                            ]
                            .spacing(4),
                        );
                    }
                    PluginParamKind::String => {
                        let text_val = current.as_str().unwrap_or_default().to_owned();
                        body_col = body_col.push(
                            column![
                                text(label).size(12),
                                text_input("", &text_val)
                                    .on_input(move |s| {
                                        Message::PluginParamChanged(
                                            key.clone(),
                                            PluginParamValue::Select(s),
                                        )
                                    })
                                    .size(TEXT_SIZE_CONTROL)
                                    .padding([5, 8])
                                    .style(input_style),
                            ]
                            .spacing(4),
                        );
                    }
                }
                if let Some(desc) = &spec.description
                    && !desc.trim().is_empty()
                {
                    body_col = body_col.push(text(desc.clone()).size(11).style(|th| text::Style {
                        color: Some(PaletteTokens::for_theme(th).text_muted),
                    }));
                }
            }

            let footer = row![
                button::labeled(
                    tr(lang, "dialog-cancel"),
                    None,
                    ButtonVariant::Secondary,
                    Some(Message::CloseDialog),
                ),
                iced::widget::space::horizontal(),
                button::labeled(
                    tr(lang, "action-run-plugin"),
                    Some(Icon::Check),
                    ButtonVariant::Primary,
                    Some(Message::ConfirmPluginParams),
                ),
            ]
            .align_y(Alignment::Center);

            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-plugin-params-title"),
                    width: 440.0,
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
        InAppDialog::PluginModels { plugin_id } => {
            let title = state
                .snapshot
                .meta
                .plugins
                .iter()
                .find(|p| &p.id == plugin_id)
                .map(|p| format!("{} — {}", p.name, tr(lang, "action-plugin-settings-models")))
                .unwrap_or_else(|| tr(lang, "settings-plugins-models-title"));
            let body = scrollable(build_models_card_for_plugins(state, &[plugin_id.as_str()]))
                .height(Fill)
                .into();
            let footer = row![
                iced::widget::space::horizontal(),
                button::labeled(
                    tr(lang, "action-close"),
                    None,
                    ButtonVariant::Primary,
                    Some(Message::CloseDialog),
                ),
            ];
            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title,
                    width: 540.0,
                    body,
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
        InAppDialog::TranslateSettings => {
            let body = column![
                build_settings_translate_tab(state),
                section_card(
                    tr(lang, "settings-plugins-models-title"),
                    build_models_card_for_plugins(
                        state,
                        &["photon.ocr.tesseract", "photon.ocr.ai"]
                    ),
                ),
            ]
            .spacing(10);
            let footer = row![
                iced::widget::space::horizontal(),
                button::labeled(
                    tr(lang, "action-close"),
                    None,
                    ButtonVariant::Primary,
                    Some(Message::CloseDialog),
                ),
            ];
            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "action-translate-settings"),
                    width: 560.0,
                    body: scrollable(body).height(Fill).into(),
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

fn map_batch_dialog_spec<'a>(
    spec: ManagedDialogSpec<'a, DesktopDialogId, BatchDialogAction<DesktopDialogId>>,
) -> ManagedDialogSpec<'a, DesktopDialogId, Message> {
    let map_act = |act: BatchDialogAction<DesktopDialogId>| match act {
        BatchDialogAction::Msg(m) => Message::BatchDialog(m),
        BatchDialogAction::Close => Message::CloseDialog,
        BatchDialogAction::DialogEvent(ev) => Message::DialogEvent(ev),
        BatchDialogAction::Noop => Message::Noop,
    };
    let on_ev = spec.on_event;
    ManagedDialogSpec {
        id: spec.id,
        open: spec.open,
        title: spec.title,
        defaults: spec.defaults,
        resizable: spec.resizable,
        draggable: spec.draggable,
        overlay: spec.overlay,
        body: spec.body.map(map_act),
        footer: spec.footer.map(|f| f.map(map_act)),
        close_button: spec.close_button.map(|b| b.map(map_act)),
        style: spec.style,
        on_event: Box::new(move |ev| map_act(on_ev(ev))),
        on_close: map_act(spec.on_close),
        on_noop: map_act(spec.on_noop),
    }
}

fn map_browser_message(msg: BrowserMessage) -> Message {
    match msg {
        BrowserMessage::Noop => Message::Noop,
        BrowserMessage::Dispatch(cmd) => Message::Dispatch(cmd),
        BrowserMessage::ContextMenuDispatch(cmd) => Message::ContextMenuDispatch(cmd),
        BrowserMessage::ToggleMenu(kind) => {
            let m = match kind {
                BrowserMenuKind::File => ActiveMenu::BrowserFile,
                BrowserMenuKind::View => ActiveMenu::BrowserView,
                BrowserMenuKind::Tools => ActiveMenu::BrowserTools,
                BrowserMenuKind::Plugins => ActiveMenu::BrowserPlugins,
                BrowserMenuKind::Window => ActiveMenu::BrowserWindow,
                BrowserMenuKind::Help => ActiveMenu::BrowserHelp,
                BrowserMenuKind::Sort => ActiveMenu::BrowserSort,
                BrowserMenuKind::WorkspaceSwitcher => ActiveMenu::WorkspaceSwitcher,
            };
            Message::ToggleMenu(m)
        }
        BrowserMessage::ToggleWorkspacePanels => Message::ToggleMenu(ActiveMenu::WorkspacePanels),
        BrowserMessage::ToggleFullscreen => Message::ToggleFullscreen,
        BrowserMessage::OpenSettings => Message::OpenSettings,
        BrowserMessage::OpenShortcutsDialog => Message::OpenShortcutsDialog,
        BrowserMessage::OpenAboutDialog => Message::OpenAboutDialog,
        BrowserMessage::RequestResetLayout => Message::RequestResetLayout,
        BrowserMessage::OpenBatchDialog(k) => Message::OpenBatchDialog(k),
        BrowserMessage::TogglePathEdit(b) => Message::TogglePathEdit(b),
        BrowserMessage::FolderInputChanged(v) => Message::FolderInputChanged(v),
        BrowserMessage::SubmitFolderInput => Message::SubmitFolderInput,
        BrowserMessage::ToggleBrowserFilterBar => Message::ToggleBrowserFilterBar,
        BrowserMessage::BrowserFilterChanged(v) => Message::BrowserFilterChanged(v),
        BrowserMessage::ClearBrowserFilter => Message::ClearBrowserFilter,
        BrowserMessage::ToggleBrowserFilterCaseSensitive => {
            Message::ToggleBrowserFilterCaseSensitive
        }
        BrowserMessage::ToggleBrowserFilterRegex => Message::ToggleBrowserFilterRegex,
        BrowserMessage::SetBrowserSortField(f) => Message::SetBrowserSortField(f),
        BrowserMessage::SetBrowserSortOrder(o) => Message::SetBrowserSortOrder(o),
        BrowserMessage::SetBrowserThumbSize(s) => Message::SetBrowserThumbSize(s),
        BrowserMessage::SetBrowserViewMode(m) => Message::SetBrowserViewMode(m),
        BrowserMessage::BrowserEntryActivated {
            path,
            is_dir,
            is_image,
        } => Message::BrowserEntryActivated {
            path,
            is_dir,
            is_image,
        },
        BrowserMessage::ToggleFolderExpand(p) => Message::ToggleFolderExpand(p),
        BrowserMessage::OpenBrowserContextMenu { path, x, y } => {
            Message::OpenBrowserContextMenu { path, x, y }
        }
        BrowserMessage::OpenQuickAccessContextMenu { path, x, y } => {
            Message::OpenQuickAccessContextMenu { path, x, y }
        }
        BrowserMessage::BrowserCopyPath(p) => Message::BrowserCopyPath(p),
        BrowserMessage::BrowserStartRename(p) => Message::BrowserStartRename(p),
        BrowserMessage::BrowserCopyTo(ps) => Message::BrowserCopyTo(ps),
        BrowserMessage::BrowserMoveTo(ps) => Message::BrowserMoveTo(ps),
        BrowserMessage::BrowserConfirmDeletePermanently(ps) => {
            Message::BrowserConfirmDeletePermanently(ps)
        }
        BrowserMessage::BrowserCopyFiles(ps) => Message::BrowserCopyFiles(ps),
        BrowserMessage::BrowserPasteFiles => Message::BrowserPasteFiles,
        BrowserMessage::CopyImageToClipboard(p) => Message::CopyImageToClipboard(p),
        BrowserMessage::RevealInFolder(p) => Message::RevealInFolder(p),
        BrowserMessage::OpenManagePlugins => Message::OpenManagePlugins,
        BrowserMessage::ReloadPlugins => Message::Dispatch(Command::PluginRefresh),
        BrowserMessage::RunPlugin(id) => Message::RunPlugin(id),
        BrowserMessage::BeginDragBrowserPanel(p) => Message::BeginDragBrowserPanel(p),
        BrowserMessage::HoverBrowserPanelDrop(p) => Message::HoverBrowserPanelDrop(p),
        BrowserMessage::DropBrowserPanel(p) => Message::DropBrowserPanel(p),
        BrowserMessage::SwapBrowserLeftPanels => Message::SwapBrowserLeftPanels,
        BrowserMessage::ToggleBrowserLeftPanelCollapse(p) => {
            Message::ToggleBrowserLeftPanelCollapse(p)
        }
        BrowserMessage::CloseBrowserLeftPanel(p) => Message::CloseBrowserLeftPanel(p),
        BrowserMessage::ToggleBrowserPanelVisible(p) => Message::ToggleBrowserPanelVisible(p),
        BrowserMessage::BrowserSplitterDrag(w) => Message::BrowserSplitterDrag(w),
        BrowserMessage::BrowserSplitterRelease(w) => Message::BrowserSplitterRelease(w),
        BrowserMessage::BeginDragBrowserRightPanel(p) => Message::BeginDragBrowserRightPanel(p),
        BrowserMessage::HoverBrowserRightPanelDrop(p) => Message::HoverBrowserRightPanelDrop(p),
        BrowserMessage::DropBrowserRightPanel(p) => Message::DropBrowserRightPanel(p),
        BrowserMessage::SwapBrowserRightPanels => Message::SwapBrowserRightPanels,
        BrowserMessage::ToggleBrowserRightPanelCollapse(p) => {
            Message::ToggleBrowserRightPanelCollapse(p)
        }
        BrowserMessage::CloseBrowserRightPanel(p) => Message::CloseBrowserRightPanel(p),
        BrowserMessage::ToggleBrowserRightPanelVisible(p) => {
            Message::ToggleBrowserRightPanelVisible(p)
        }
        BrowserMessage::BrowserRightSplitterDrag(w) => Message::BrowserRightSplitterDrag(w),
        BrowserMessage::BrowserRightSplitterRelease(w) => Message::BrowserRightSplitterRelease(w),
        BrowserMessage::ToggleQuickAccessOptions => Message::ToggleQuickAccessOptions,
        BrowserMessage::ToggleQuickAccessEditing => Message::ToggleQuickAccessEditing,
        BrowserMessage::OpenInViewerWindow(p) => Message::OpenInViewerWindow(p),
    }
}

fn map_editor_dialog_spec<'a>(spec: DialogSpec<'a, EditorMessage>) -> DialogSpec<'a, Message> {
    DialogSpec {
        open: spec.open,
        title: spec.title,
        width: spec.width,
        body: spec.body.map(Message::Editor),
        footer: spec.footer.map(Message::Editor),
        on_close: map_editor_message(spec.on_close),
        on_noop: map_editor_message(spec.on_noop),
    }
}

fn map_editor_message(msg: EditorMessage) -> Message {
    match msg {
        EditorMessage::Noop => Message::Noop,
        EditorMessage::Dispatch(cmd) => Message::Dispatch(cmd),
        EditorMessage::ContextMenuDispatch(cmd) => Message::ContextMenuDispatch(cmd),
        EditorMessage::ToggleMenu(kind) => {
            let m = match kind {
                EditorMenuKind::File => ActiveMenu::EditorFile,
                EditorMenuKind::Edit => ActiveMenu::EditorEdit,
                EditorMenuKind::Image => ActiveMenu::EditorImage,
                EditorMenuKind::Filter => ActiveMenu::EditorFilter,
                EditorMenuKind::Plugins => ActiveMenu::EditorPlugins,
                EditorMenuKind::Select => ActiveMenu::EditorSelect,
                EditorMenuKind::View => ActiveMenu::EditorView,
                EditorMenuKind::Window => ActiveMenu::EditorWindow,
                EditorMenuKind::Help => ActiveMenu::EditorHelp,
                EditorMenuKind::AddLayer => ActiveMenu::EditorAddLayer,
                EditorMenuKind::WorkspaceSwitcher => ActiveMenu::WorkspaceSwitcher,
            };
            Message::ToggleMenu(m)
        }
        EditorMessage::HoverMenu(kind) => {
            let m = match kind {
                EditorMenuKind::File => ActiveMenu::EditorFile,
                EditorMenuKind::Edit => ActiveMenu::EditorEdit,
                EditorMenuKind::Image => ActiveMenu::EditorImage,
                EditorMenuKind::Filter => ActiveMenu::EditorFilter,
                EditorMenuKind::Plugins => ActiveMenu::EditorPlugins,
                EditorMenuKind::Select => ActiveMenu::EditorSelect,
                EditorMenuKind::View => ActiveMenu::EditorView,
                EditorMenuKind::Window => ActiveMenu::EditorWindow,
                EditorMenuKind::Help => ActiveMenu::EditorHelp,
                EditorMenuKind::AddLayer => ActiveMenu::EditorAddLayer,
                EditorMenuKind::WorkspaceSwitcher => ActiveMenu::WorkspaceSwitcher,
            };
            Message::HoverMenu(m)
        }
        EditorMessage::ToggleWorkspacePanels => Message::ToggleMenu(ActiveMenu::WorkspacePanels),
        EditorMessage::RequestEditorLeaveToBrowser => Message::RequestEditorLeaveToBrowser,
        EditorMessage::RequestEditorLeave { close_editor } => {
            Message::RequestEditorLeave { close_editor }
        }
        EditorMessage::Canvas(ev) => Message::Canvas(ev),
        EditorMessage::CloseDialog => Message::CloseDialog,
        EditorMessage::OpenExportDialog => Message::OpenExportDialog,
        EditorMessage::PickExportDialog => Message::PickExportDialog,
        EditorMessage::ConfirmExport => Message::ConfirmExport,
        EditorMessage::ExportPathChanged(v) => Message::ExportPathChanged(v),
        EditorMessage::ExportQualityChanged(v) => Message::ExportQualityChanged(v),
        EditorMessage::ExportMaxEdgeChanged(v) => Message::ExportMaxEdgeChanged(v),
        EditorMessage::ExportMetadataToggled(v) => Message::ExportMetadataToggled(v),
        EditorMessage::OpenResizeDialog => Message::OpenResizeDialog,
        EditorMessage::OpenCanvasSizeDialog => Message::OpenCanvasSizeDialog,
        EditorMessage::SwitchTab(id) => Message::EditorSwitchTab(id),
        EditorMessage::RequestCloseTab(id) => Message::EditorRequestCloseTab(id),
        EditorMessage::OpenNewTabDialog => Message::EditorOpenNewTabDialog,
        EditorMessage::OpenColorPicker(t) => Message::OpenColorPicker(t),
        EditorMessage::ColorPickerSetRgb(r, g, b) => Message::ColorPickerSetRgb(r, g, b),
        EditorMessage::ColorPickerHexChanged(v) => Message::ColorPickerHexChanged(v),
        EditorMessage::ColorPickerToggleTransparent(v) => Message::ColorPickerToggleTransparent(v),
        EditorMessage::ConfirmColorPicker => Message::ConfirmColorPicker,
        EditorMessage::ResetViewportBackground => Message::ResetViewportBackground,
        EditorMessage::CopyImageToClipboard(p) => Message::CopyImageToClipboard(p),
        EditorMessage::EditorSelectTool(t) => Message::EditorSelectTool(t),
        EditorMessage::EditorSetSelectOp(op) => Message::EditorSetSelectOp(op),
        EditorMessage::EditorSetSelectFeather(v) => Message::EditorSetSelectFeather(v),
        EditorMessage::EditorToggleSelectAntiAlias => Message::EditorToggleSelectAntiAlias,
        EditorMessage::EditorSetWandTolerance(v) => Message::EditorSetWandTolerance(v),
        EditorMessage::EditorToggleWandContiguous => Message::EditorToggleWandContiguous,
        EditorMessage::EditorToggleSelectSampleAll => Message::EditorToggleSelectSampleAll,
        EditorMessage::EditorSetQuickSelectRadius(v) => Message::EditorSetQuickSelectRadius(v),
        EditorMessage::EditorSetBrushSmoothingMode(m) => Message::EditorSetBrushSmoothingMode(m),
        EditorMessage::EditorSetStabilizerDistance(d) => Message::EditorSetStabilizerDistance(d),
        EditorMessage::EditorSetShapeFillMode(m) => Message::EditorSetShapeFillMode(m),
        EditorMessage::EditorSetShapeStrokeWidth(w) => Message::EditorSetShapeStrokeWidth(w),
        EditorMessage::EditorSetFloodFillTolerance(t) => Message::EditorSetFloodFillTolerance(t),
        EditorMessage::EditorToggleFloodFillContiguous => Message::EditorToggleFloodFillContiguous,
        EditorMessage::EditorSetFloodFillOpacity(v) => Message::EditorSetFloodFillOpacity(v),
        EditorMessage::EditorToggleFloodFillSampleAll => Message::EditorToggleFloodFillSampleAll,
        EditorMessage::EditorSetGradientKind(k) => Message::EditorSetGradientKind(k),
        EditorMessage::EditorSetGradientColorMode(c) => Message::EditorSetGradientColorMode(c),
        EditorMessage::EditorSetGradientOpacity(v) => Message::EditorSetGradientOpacity(v),
        EditorMessage::EditorToggleGradientReverse => Message::EditorToggleGradientReverse,
        EditorMessage::EditorToggleGradientDither => Message::EditorToggleGradientDither,
        EditorMessage::EditorSetGradientRepeat(r) => Message::EditorSetGradientRepeat(r),
        EditorMessage::EditorSetGradientBlendMode(b) => Message::EditorSetGradientBlendMode(b),
        EditorMessage::EditorToggleGradientInteractive => Message::EditorToggleGradientInteractive,
        EditorMessage::EditorApplyGradientPreset(id) => Message::EditorApplyGradientPreset(id),
        EditorMessage::EditorSelectGradientStop(i) => Message::EditorSelectGradientStop(i),
        EditorMessage::EditorAddGradientStop(p) => Message::EditorAddGradientStop(p),
        EditorMessage::EditorRemoveSelectedGradientStop => {
            Message::EditorRemoveSelectedGradientStop
        }
        EditorMessage::EditorSetGradientStopPos(i, p) => Message::EditorSetGradientStopPos(i, p),
        EditorMessage::EditorToggleGradientStopAlpha(i) => {
            Message::EditorToggleGradientStopAlpha(i)
        }
        EditorMessage::EditorSetGradientStopFg(i) => Message::EditorSetGradientStopFg(i),
        EditorMessage::EditorApplyGradientDraft => Message::EditorApplyGradientDraft,
        EditorMessage::EditorCancelGradientDraft => Message::EditorCancelGradientDraft,
        EditorMessage::EditorSetHealingMode(m) => Message::EditorSetHealingMode(m),
        EditorMessage::EditorSetHealingRadius(r) => Message::EditorSetHealingRadius(r),
        EditorMessage::EditorSetHealingHardness(h) => Message::EditorSetHealingHardness(h),
        EditorMessage::EditorSetHealingStrength(s) => Message::EditorSetHealingStrength(s),
        EditorMessage::EditorToggleHealingSampleAll => Message::EditorToggleHealingSampleAll,
        EditorMessage::EditorSetHealingSource(src) => Message::EditorSetHealingSource(src),
        EditorMessage::EditorSetDenoiseStrength(v) => Message::EditorSetDenoiseStrength(v),
        EditorMessage::EditorSetDenoiseDetail(v) => Message::EditorSetDenoiseDetail(v),
        EditorMessage::EditorSetTransformSpec(spec) => Message::EditorSetTransformSpec(spec),
        EditorMessage::EditorTransformCommit => Message::EditorTransformCommit,
        EditorMessage::EditorTransformCancel => Message::EditorTransformCancel,
        EditorMessage::OpenDenoiseDialog => Message::OpenDenoiseDialog,
        EditorMessage::OpenManagePlugins => Message::OpenManagePlugins,
        EditorMessage::ReloadPlugins => Message::Dispatch(Command::PluginRefresh),
        EditorMessage::RunPlugin(id) => Message::RunPlugin(id),
        EditorMessage::EditorCopyPixels { merged } => Message::EditorCopyPixels { merged },
        EditorMessage::EditorCutPixels => Message::EditorCutPixels,
        EditorMessage::EditorPastePixels { into_selection } => {
            Message::EditorPastePixels { into_selection }
        }
        EditorMessage::ZoomStep(f) => Message::ZoomStep(f),
        EditorMessage::ZoomFit => Message::Canvas(CanvasEvent::ResetFit),
        EditorMessage::ZoomActual => Message::EditorZoomActual,
        EditorMessage::CanvasPanTo { pan_x, pan_y } => Message::CanvasPanTo { pan_x, pan_y },
        EditorMessage::SetAntiAliasingMode(m) => Message::SetAntiAliasingMode(m),
        EditorMessage::SetViewportDenoiseMode(m) => Message::SetViewportDenoiseMode(m),
        EditorMessage::SelectEditorToolTab(t) => Message::SelectEditorToolTab(t),
        EditorMessage::EditorSwapColors => Message::EditorSwapColors,
        EditorMessage::EditorResetColors => Message::EditorResetColors,
        EditorMessage::EditorCycleCompareMode => Message::EditorCycleCompareMode,
        EditorMessage::EditorSetCompareMode(m) => Message::EditorSetCompareMode(m),
        EditorMessage::EditorOpenAdjustPopup(tab) => Message::EditorOpenAdjustPopup(tab),
        EditorMessage::EditorSetBrushRadius(v) => Message::EditorSetBrushRadius(v),
        EditorMessage::EditorSetBrushOpacity(v) => Message::EditorSetBrushOpacity(v),
        EditorMessage::EditorSetBrushHardness(v) => Message::EditorSetBrushHardness(v),
        EditorMessage::EditorSetBrushFlow(v) => Message::EditorSetBrushFlow(v),
        EditorMessage::EditorSetBrushSpacing(v) => Message::EditorSetBrushSpacing(v),
        EditorMessage::EditorSetBrushTip(t) => Message::EditorSetBrushTip(t),
        EditorMessage::EditorSetLiquifyMode(m) => Message::EditorSetLiquifyMode(m),
        EditorMessage::EditorSetLiquifyRadius(v) => Message::EditorSetLiquifyRadius(v),
        EditorMessage::EditorSetLiquifyStrength(v) => Message::EditorSetLiquifyStrength(v),
        EditorMessage::EditorSetRotateAngle(v) => Message::EditorSetRotateAngle(v),
        EditorMessage::EditorApplyRotateAngle => Message::EditorApplyRotateAngle,
        EditorMessage::ToggleCropMode => Message::ToggleCropMode,
        EditorMessage::SetCropPreset(p) => Message::SetCropPreset(p),
        EditorMessage::ApplyCrop => Message::ApplyCrop,
        EditorMessage::CancelCrop => Message::CancelCrop,
        EditorMessage::BeginDragEditorPanel(p) => Message::BeginDragEditorPanel(p),
        EditorMessage::HoverEditorPanelDrop(p) => Message::HoverEditorPanelDrop(p),
        EditorMessage::DropEditorPanel(p) => Message::DropEditorPanel(p),
        EditorMessage::SwapEditorPanelOrder => Message::SwapEditorPanelOrder,
        EditorMessage::ToggleEditorPanelCollapse(p) => Message::ToggleEditorPanelCollapse(p),
        EditorMessage::CloseEditorPanel(p) => Message::CloseEditorPanel(p),
        EditorMessage::CloseEditorToolsPanel => Message::CloseEditorToolsPanel,
        EditorMessage::DockSplitterDrag(k, v) => Message::DockSplitterDrag(k, v),
        EditorMessage::DockSplitterRelease(k, v) => Message::DockSplitterRelease(k, v),
        EditorMessage::OpenLayerContextMenu { layer_id } => {
            Message::OpenLayerContextMenu { layer_id }
        }
        EditorMessage::EditorStartLayerRename(id, name) => {
            Message::EditorStartLayerRename(id, name)
        }
        EditorMessage::EditorLayerRenameChanged(v) => Message::EditorLayerRenameChanged(v),
        EditorMessage::EditorCommitLayerRename => Message::EditorCommitLayerRename,
        EditorMessage::EditorCancelLayerRename => Message::EditorCancelLayerRename,
        EditorMessage::EditorSetApplyTarget(t) => Message::EditorSetApplyTarget(t),
        EditorMessage::CopyText(s) => Message::CopyText(s),
        EditorMessage::OpenPluginModels(id) => Message::OpenPluginModels(id),
        other @ (EditorMessage::AiSetRembgModel(_)
        | EditorMessage::AiSetRembgFeather(_)
        | EditorMessage::AiSetRembgRefine(_)
        | EditorMessage::AiSetRembgRefineRadius(_)
        | EditorMessage::AiSetRembgRefineSoftness(_)
        | EditorMessage::AiRunRemoveBackground
        | EditorMessage::AiSetWatermarkTab(_)
        | EditorMessage::AiSetWmDetector(_)
        | EditorMessage::AiSetWmInpainter(_)
        | EditorMessage::AiSetWmConfidence(_)
        | EditorMessage::AiSetWmMaskPadding(_)
        | EditorMessage::AiSetWmMaskBrushRadius(_)
        | EditorMessage::AiClearWmMaskStrokes
        | EditorMessage::AiSetWmRegion(_, _, _, _)
        | EditorMessage::AiSetWmGeminiSize(_)
        | EditorMessage::AiSetWmGeminiBrightness(_)
        | EditorMessage::AiSetWmGeminiOffsetX(_)
        | EditorMessage::AiSetWmGeminiOffsetY(_)
        | EditorMessage::AiToggleWmGeminiForce
        | EditorMessage::AiSetWmRegionFeather(_)
        | EditorMessage::AiRunWatermark
        | EditorMessage::AiSetCaptionModel(_)
        | EditorMessage::AiSetCaptionGeneralThreshold(_)
        | EditorMessage::AiSetCaptionCharThreshold(_)
        | EditorMessage::AiToggleCaptionUnderscores
        | EditorMessage::AiSetCaptionPrefix(_)
        | EditorMessage::AiSetCaptionSuffix(_)
        | EditorMessage::AiSetCaptionExclude(_)
        | EditorMessage::AiSetCaptionDraft(_)
        | EditorMessage::AiRunCaption
        | EditorMessage::AiSaveCaption(_)) => Message::EditorAi(other),
    }
}

fn map_viewer_message(msg: ViewerMessage) -> Message {
    match msg {
        ViewerMessage::Noop => Message::Noop,
        ViewerMessage::Dispatch(cmd) => Message::Dispatch(cmd),
        ViewerMessage::ContextMenuDispatch(cmd) => Message::ContextMenuDispatch(cmd),
        ViewerMessage::PickImagesDialog => Message::PickImagesDialog,
        ViewerMessage::DualStepSpread(d) => Message::DualStepSpread(d),
        ViewerMessage::ZoomStep(f) => Message::ZoomStep(f),
        ViewerMessage::Canvas(ev) => Message::Canvas(ev),
        ViewerMessage::ViewerCanvas(p, ev) => Message::ViewerCanvas(p, ev),
        ViewerMessage::ViewerZoomActual => Message::ViewerZoomActual,
        ViewerMessage::ViewerRotatePreview(d) => Message::ViewerRotatePreview(d),
        ViewerMessage::ToggleFullscreen => Message::ToggleFullscreen,
        ViewerMessage::SetViewerViewMode(m) => Message::SetViewerViewMode(m),
        ViewerMessage::SetAntiAliasingMode(m) => Message::SetAntiAliasingMode(m),
        ViewerMessage::SetViewportDenoiseMode(m) => Message::SetViewportDenoiseMode(m),
        ViewerMessage::ToggleReaderOptions => Message::ToggleMenu(ActiveMenu::ReaderOptions),
        ViewerMessage::ToggleWorkspaceSwitcher => {
            Message::ToggleMenu(ActiveMenu::WorkspaceSwitcher)
        }
        ViewerMessage::ToggleWorkspacePanels => Message::ToggleMenu(ActiveMenu::WorkspacePanels),
        ViewerMessage::OpenViewerContextMenu { path, pos } => {
            Message::OpenViewerContextMenu { path, pos }
        }
        ViewerMessage::CopyImageToClipboard(p) => Message::CopyImageToClipboard(p),
        ViewerMessage::CopyViewerPath => Message::CopyViewerPath,
        ViewerMessage::RevealInFolder(p) => Message::RevealInFolder(p),
        ViewerMessage::ToggleFilmstrip => Message::ToggleFilmstrip,
        ViewerMessage::CloseFilmstrip => Message::CloseFilmstrip,
        ViewerMessage::ToggleViewerInspector => Message::ToggleViewerInspector,
        ViewerMessage::ToggleViewerColorPopover => Message::ToggleViewerColorPopover,
        ViewerMessage::CloseViewerPanel(p) => Message::CloseViewerPanel(p),
        ViewerMessage::ToggleViewerPanelCollapse(p) => Message::ToggleViewerPanelCollapse(p),
        ViewerMessage::BeginDragViewerPanel(p) => Message::BeginDragViewerPanel(p),
        ViewerMessage::HoverViewerPanelDrop(p) => Message::HoverViewerPanelDrop(p),
        ViewerMessage::DropViewerPanel(p) => Message::DropViewerPanel(p),
        ViewerMessage::SwapViewerRightPanels => Message::SwapViewerRightPanels,
        ViewerMessage::ViewerSplitterDrag(w) => Message::ViewerSplitterDrag(w),
        ViewerMessage::ViewerSplitterRelease(w) => Message::ViewerSplitterRelease(w),
        ViewerMessage::SetViewerAdjExposure(v) => Message::SetViewerAdjExposure(v),
        ViewerMessage::SetViewerAdjGamma(v) => Message::SetViewerAdjGamma(v),
        ViewerMessage::SetViewerAdjBrightness(v) => Message::SetViewerAdjBrightness(v),
        ViewerMessage::SetViewerAdjContrast(v) => Message::SetViewerAdjContrast(v),
        ViewerMessage::SetViewerAdjSaturation(v) => Message::SetViewerAdjSaturation(v),
        ViewerMessage::SetViewerAdjHighlights(v) => Message::SetViewerAdjHighlights(v),
        ViewerMessage::SetViewerAdjShadows(v) => Message::SetViewerAdjShadows(v),
        ViewerMessage::ResetViewerAdj => Message::ResetViewerAdj,
        ViewerMessage::ViewerAdjOpenInEditor => Message::ViewerAdjOpenInEditor,
        ViewerMessage::WebtoonScrolled(vp) => Message::WebtoonScrolled(vp),
        ViewerMessage::OpenTranslateSettings => Message::OpenTranslateSettings,
        ViewerMessage::OpenInNewViewerWindow(p) => Message::OpenInViewerWindow(p),
        other @ (ViewerMessage::ToggleTranslateOverlay
        | ViewerMessage::CloseTranslateOverlay
        | ViewerMessage::SetTranslateMode(_)
        | ViewerMessage::SetTranslateOcrEngine(_)
        | ViewerMessage::SetTranslateSourceLang(_)
        | ViewerMessage::SetTranslateTargetLang(_)
        | ViewerMessage::SwapTranslateLangs
        | ViewerMessage::StartTranslateRegionPick
        | ViewerMessage::ScanTranslatePage
        | ViewerMessage::ToggleTranslateBubbles
        | ViewerMessage::SetTranslateFontScale(_)
        | ViewerMessage::SelectTranslateBlock(_)
        | ViewerMessage::UpdateTranslateBlockText(_, _)
        | ViewerMessage::MoveTranslateBlock(_, _, _)
        | ViewerMessage::RetryTranslateBlock(_)
        | ViewerMessage::RemoveTranslateBlock(_)
        | ViewerMessage::SaveTranslationSidecar
        | ViewerMessage::DeleteTranslationSidecar
        | ViewerMessage::CopyTranslatedText) => Message::ViewerTranslate(other),
    }
}

impl PhotonGui {
    fn editor_view_state(&self) -> EditorViewState<'_> {
        let active_menu = match self.active_menu {
            Some(ActiveMenu::EditorFile) => Some(EditorMenuKind::File),
            Some(ActiveMenu::EditorEdit) => Some(EditorMenuKind::Edit),
            Some(ActiveMenu::EditorImage) => Some(EditorMenuKind::Image),
            Some(ActiveMenu::EditorFilter) => Some(EditorMenuKind::Filter),
            Some(ActiveMenu::EditorPlugins) => Some(EditorMenuKind::Plugins),
            Some(ActiveMenu::EditorSelect) => Some(EditorMenuKind::Select),
            Some(ActiveMenu::EditorView) => Some(EditorMenuKind::View),
            Some(ActiveMenu::EditorWindow) => Some(EditorMenuKind::Window),
            Some(ActiveMenu::EditorHelp) => Some(EditorMenuKind::Help),
            Some(ActiveMenu::EditorAddLayer) => Some(EditorMenuKind::AddLayer),
            Some(ActiveMenu::WorkspaceSwitcher) => Some(EditorMenuKind::WorkspaceSwitcher),
            _ => None,
        };
        let curve_lut = if self.editor_ui.adjust_popup_tab == Some(AdjustmentPopupTab::Curves) {
            Some(compute_editor_curve_lut(&self.editor_ui.curves))
        } else {
            None
        };
        EditorViewState {
            snapshot: &self.snapshot,
            editor_frame: self.editor_frame.as_ref(),
            editor_original_frame: self.editor_original_frame.as_ref(),
            selection_mask: self.editor_selection_mask.as_ref(),
            marching_ants_phase: self.marching_ants_phase,
            editor_layer_thumbs: &self.editor_layer_thumbs,
            editor_mask_thumbs: &self.editor_mask_thumbs,
            editor_ui: &self.editor_ui,
            pending_adj: &self.pending_adj,
            curve_lut,
            aa_mode: self.snapshot.meta.layout.canvas_anti_aliasing,
            denoise_mode: self.snapshot.meta.layout.viewport_denoise,
            transform_preview: self.transform_preview.as_ref(),
            crop_uv: self.crop_uv,
            viewport_background: self.viewport_background,
            editor_tool_tab: self.editor_tool_tab,
            editor_history_open: self.editor_history_open,
            editor_layers_open: self.editor_layers_open,
            editor_history_collapsed: self.editor_history_collapsed,
            editor_layers_collapsed: self.editor_layers_collapsed,
            editor_panels: &self.editor_panels,
            dock_drag: &self.dock_drag,
            zoom: self.zoom,
            pan_x: self.pan_x,
            pan_y: self.pan_y,
            window_size: self.window_size,
            active_menu,
        }
    }

    fn browser_view_state(&self) -> BrowserViewState<'_> {
        let active_menu = match self.active_menu {
            Some(ActiveMenu::BrowserFile) => Some(BrowserMenuKind::File),
            Some(ActiveMenu::BrowserView) => Some(BrowserMenuKind::View),
            Some(ActiveMenu::BrowserTools) => Some(BrowserMenuKind::Tools),
            Some(ActiveMenu::BrowserPlugins) => Some(BrowserMenuKind::Plugins),
            Some(ActiveMenu::BrowserWindow) => Some(BrowserMenuKind::Window),
            Some(ActiveMenu::BrowserHelp) => Some(BrowserMenuKind::Help),
            Some(ActiveMenu::BrowserSort) => Some(BrowserMenuKind::Sort),
            Some(ActiveMenu::WorkspaceSwitcher) => Some(BrowserMenuKind::WorkspaceSwitcher),
            _ => None,
        };
        BrowserViewState {
            snapshot: &self.snapshot,
            thumbnails: &self.thumbnails,
            expanded_folders: &self.expanded_folders,
            folder_children: &self.folder_children,
            folder_input: &self.folder_input,
            path_editing: self.path_editing,
            browser_filter: &self.browser_filter,
            browser_filter_bar_open: self.browser_filter_bar_open,
            browser_filter_case_sensitive: self.browser_filter_case_sensitive,
            browser_filter_regex: self.browser_filter_regex,
            browser_quick_access_open: self.browser_quick_access_open,
            browser_quick_access_collapsed: self.browser_quick_access_collapsed,
            quick_access_options_open: self.quick_access_options_open,
            quick_access_editing: self.quick_access_editing,
            browser_explorer_open: self.browser_explorer_open,
            browser_explorer_collapsed: self.browser_explorer_collapsed,
            browser_preview_open: self.browser_preview_open,
            browser_preview_collapsed: self.browser_preview_collapsed,
            browser_info_open: self.browser_info_open,
            browser_info_collapsed: self.browser_info_collapsed,
            browser_left_panels: &self.browser_left_panels,
            browser_right_panels: &self.browser_right_panels,
            browser_left_width_drag: self.browser_left_width_drag,
            browser_right_width_drag: self.browser_right_width_drag,
            is_fullscreen: self.is_fullscreen,
            last_cursor: self.last_cursor,
            window_size: self.window_size,
            active_menu,
        }
    }

    fn viewer_view_state(&self, is_main_window: bool) -> ViewerViewState<'_> {
        ViewerViewState {
            snapshot: &self.snapshot,
            viewer_mode: self.viewer_mode,
            viewer_frame: self.viewer_frame.as_ref(),
            viewer_pair_frame: self.viewer_pair_frame.as_ref(),
            image_sizes: &self.image_sizes,
            thumbnails: &self.thumbnails,
            viewer_adj: self.viewer_adj,
            viewer_quarter_turns: self.viewer_quarter_turns,
            zoom: self.zoom,
            pan_x: self.pan_x,
            pan_y: self.pan_y,
            webtoon_zoom: self.webtoon_zoom,
            webtoon_visible_index: self.webtoon_visible_index,
            webtoon_pages: &self.webtoon_pages,
            viewer_manga_rtl: self.viewer_manga_rtl,
            viewer_manga_cover_separate: self.viewer_manga_cover_separate,
            viewer_manga_page_gap: self.viewer_manga_page_gap,
            viewer_webtoon_fit_mode: self.viewer_webtoon_fit_mode,
            viewer_adjustments_open: self.viewer_adjustments_open,
            viewer_info_open: self.viewer_info_open,
            viewer_filmstrip_open: self.viewer_filmstrip_open,
            viewer_right_panels: &self.viewer_right_panels,
            viewer_right_width_preview: self.viewer_right_width_preview,
            viewer_right_split_preview: None,
            is_fullscreen: self.is_fullscreen,
            window_size: self.window_size,
            is_main_window,
            reader_options_open: self.active_menu == Some(ActiveMenu::ReaderOptions),
            workspace_switcher_open: self.active_menu == Some(ActiveMenu::WorkspaceSwitcher),
            aa_mode: self.snapshot.meta.layout.canvas_anti_aliasing,
            denoise_mode: self.snapshot.meta.layout.viewport_denoise,
            viewport_background: self.viewer_viewport_background,
            translate: &self.viewer_translate,
        }
    }

    fn viewer_ui_presentation(&self) -> ViewerUiPresentation {
        ViewerUiPresentation {
            view_mode: self.viewer_mode,
            info_open: self.viewer_info_open,
            adjustments_open: self.viewer_adjustments_open,
            filmstrip_open: self.viewer_filmstrip_open,
            is_fullscreen: self.is_fullscreen,
            color_popover_open: self.viewer_color_popover_open,
            translate_open: self.viewer_translate.active,
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
        state.settings_open,
        state.settings_tab.as_str(),
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
        "toolbar.settings" | "settings.dialog" => update(state, Message::OpenSettings),
        "settings.tab.general"
        | "settings.tab.viewer"
        | "settings.tab.quick_access"
        | "settings.tab.plugins"
        | "settings.tab.translate" => {
            let tab = match target_id {
                "settings.tab.general" => SettingsTab::General,
                "settings.tab.viewer" => SettingsTab::Viewer,
                "settings.tab.quick_access" => SettingsTab::QuickAccess,
                "settings.tab.plugins" => SettingsTab::Plugins,
                _ => SettingsTab::Translate,
            };
            if !state.settings_open {
                let open_task = update(state, Message::OpenSettings);
                let tab_task = update(state, Message::SelectSettingsTab(tab));
                Task::batch([open_task, tab_task])
            } else {
                update(state, Message::SelectSettingsTab(tab))
            }
        }
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
        "browser.menu.plugins.manage" | "editor.menu.plugins.manage" => {
            update(state, Message::OpenManagePlugins)
        }
        "browser.menu.plugins.reload" | "editor.menu.plugins.reload" => {
            dispatch_to_core(state.core.clone(), Command::PluginRefresh)
        }
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
        "editor.tool.marquee_rect" => {
            update(state, Message::EditorSelectTool(EditorTool::MarqueeRect))
        }
        "editor.tool.marquee_ellipse" => {
            update(state, Message::EditorSelectTool(EditorTool::MarqueeEllipse))
        }
        "editor.tool.single_row" => update(state, Message::EditorSelectTool(EditorTool::SingleRow)),
        "editor.tool.single_col" => update(state, Message::EditorSelectTool(EditorTool::SingleCol)),
        "editor.tool.lasso" => update(state, Message::EditorSelectTool(EditorTool::Lasso)),
        "editor.tool.polygon_lasso" => {
            update(state, Message::EditorSelectTool(EditorTool::PolygonLasso))
        }
        "editor.tool.magnetic_lasso" => {
            update(state, Message::EditorSelectTool(EditorTool::MagneticLasso))
        }
        "editor.tool.bezier_select" => {
            update(state, Message::EditorSelectTool(EditorTool::BezierSelect))
        }
        "editor.tool.quick_select" => {
            update(state, Message::EditorSelectTool(EditorTool::QuickSelect))
        }
        "editor.tool.magic_wand" => update(state, Message::EditorSelectTool(EditorTool::MagicWand)),
        "editor.menu.select" => update(state, Message::ToggleMenu(ActiveMenu::EditorSelect)),
        "editor.tool.brush" => update(state, Message::EditorSelectTool(EditorTool::Brush)),
        "editor.tool.eraser" => update(state, Message::EditorSelectTool(EditorTool::Eraser)),
        "editor.tool.liquify" => update(state, Message::EditorSelectTool(EditorTool::Liquify)),
        "editor.tool.eyedropper" => {
            update(state, Message::EditorSelectTool(EditorTool::Eyedropper))
        }
        "editor.tool.crop" => update(state, Message::EditorSelectTool(EditorTool::Crop)),
        "editor.tool.rotate_angle" => {
            update(state, Message::EditorSelectTool(EditorTool::RotateAngle))
        }
        "editor.tool.draw_line" => update(state, Message::EditorSelectTool(EditorTool::DrawLine)),
        "editor.tool.draw_rect" => update(state, Message::EditorSelectTool(EditorTool::DrawRect)),
        "editor.tool.draw_ellipse" => {
            update(state, Message::EditorSelectTool(EditorTool::DrawEllipse))
        }
        "editor.tool.draw_polygon" => {
            update(state, Message::EditorSelectTool(EditorTool::DrawPolygon))
        }
        "editor.tool.draw_polyline" => {
            update(state, Message::EditorSelectTool(EditorTool::DrawPolyline))
        }
        "editor.tool.bezier_pen" => update(state, Message::EditorSelectTool(EditorTool::BezierPen)),
        "editor.tool.paint_bucket" => {
            update(state, Message::EditorSelectTool(EditorTool::PaintBucket))
        }
        "editor.tool.gradient" => update(state, Message::EditorSelectTool(EditorTool::Gradient)),
        "editor.tool.healing_brush" => {
            update(state, Message::EditorSelectTool(EditorTool::HealingBrush))
        }
        "editor.tool.transform" => update(state, Message::EditorSelectTool(EditorTool::Transform)),
        "editor.tool.remove_bg" => update(state, Message::EditorSelectTool(EditorTool::RemoveBg)),
        "editor.tool.watermark" => update(state, Message::EditorSelectTool(EditorTool::Watermark)),
        "editor.tool.caption" => update(state, Message::EditorSelectTool(EditorTool::Caption)),
        "settings.plugins.refresh" => {
            dispatch_to_core(state.core.clone(), Command::AiRefreshPlugins)
        }
        "viewer.translate.toggle" => update(
            state,
            Message::ViewerTranslate(ViewerMessage::ToggleTranslateOverlay),
        ),
        "viewer.translate.scan" => update(
            state,
            Message::ViewerTranslate(ViewerMessage::ScanTranslatePage),
        ),
        "viewer.translate.settings" => update(state, Message::OpenTranslateSettings),
        "editor.zoom_in" => update(state, Message::ZoomStep(1.25)),
        "editor.zoom_out" => update(state, Message::ZoomStep(0.8)),
        "editor.zoom_fit" => update(state, Message::Canvas(CanvasEvent::ResetFit)),
        "editor.zoom_actual" => update(state, Message::EditorZoomActual),
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
        "editor.menu.filter.denoise" => update(state, Message::OpenDenoiseDialog),
        "editor.menu.plugins" => update(state, Message::ToggleMenu(ActiveMenu::EditorPlugins)),
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
        "editor.canvas_size" => update(state, Message::OpenCanvasSizeDialog),
        "editor.tab.new" => update(state, Message::EditorOpenNewTabDialog),
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
            p.map(|path| open_in_editor_or_forward(state, path))
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
        "editor.context_menu.canvas_size" => update(state, Message::OpenCanvasSizeDialog),
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
        other => {
            if let Some(rest) = other.strip_prefix("editor.tab.") {
                if let Some(idx_str) = rest.strip_suffix(".close")
                    && let Ok(idx) = idx_str.parse::<usize>()
                    && let Some(tab) = state.snapshot.meta.editor_tabs.get(idx)
                {
                    return update(state, Message::EditorRequestCloseTab(tab.id));
                }
                if let Ok(idx) = rest.parse::<usize>()
                    && let Some(tab) = state.snapshot.meta.editor_tabs.get(idx)
                {
                    return update(state, Message::EditorSwitchTab(tab.id));
                }
            }
            Task::none()
        }
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
            (snap.meta.layout.window_width as f32).clamp(680.0, 1728.0),
            (snap.meta.layout.window_height as f32).clamp(500.0, 972.0),
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

    let w = (iw * scale + chrome_w).round().clamp(680.0, max_w);
    let h = (ih * scale + chrome_h).round().clamp(500.0, max_h);
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

fn resolve_select_op(state: &PhotonGui, shift: bool, alt: bool) -> SelectionBooleanOp {
    if shift && alt {
        SelectionBooleanOp::Intersect
    } else if shift {
        SelectionBooleanOp::Add
    } else if alt {
        SelectionBooleanOp::Subtract
    } else {
        state.editor_ui.select_op
    }
}

fn load_editor_selection_mask_task(core: CoreHandle) -> Task<Message> {
    Task::perform(
        async move {
            let (rev, w, h, alpha) = core.read_editor_selection_mask().await?;
            Some(CpuMaskFrame {
                key: format!("editor:sel:{rev}"),
                revision: rev,
                width: w,
                height: h,
                alpha: Arc::new(alpha),
            })
        },
        Message::EditorSelectionMaskLoaded,
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

fn load_editor_transform_frames_task(core: CoreHandle, layer_id: Option<u64>) -> Task<Message> {
    Task::perform(
        async move { core.read_editor_transform_frames_for(layer_id).await },
        Message::EditorTransformFramesLoaded,
    )
}

fn build_draw_gradient_command(
    ui: &photon_editor::EditorUiState,
    start: [f32; 2],
    end: [f32; 2],
) -> Command {
    Command::EditorDrawGradient {
        start,
        end,
        kind: ui.gradient_kind,
        color_mode: ui.gradient_color_mode,
        fg_color: Some(ui.fg_color),
        bg_color: ui.bg_color,
        stops: if ui.gradient_color_mode == GradientColorMode::Custom {
            ui.resolved_gradient_stops()
        } else {
            Vec::new()
        },
        opacity: Some(ui.gradient_opacity),
        repeat: ui.gradient_repeat,
        reverse: ui.gradient_reverse,
        dither: ui.gradient_dither,
        blend_mode: ui.gradient_blend_mode,
    }
}

fn sample_gradient_stops(stops: &[GradientStop], pos: f64) -> ([u8; 3], f64) {
    if stops.is_empty() {
        return ([0, 0, 0], 1.0);
    }
    if pos <= stops[0].pos {
        return (stops[0].color, stops[0].alpha);
    }
    for win in stops.windows(2) {
        let (a, b) = (&win[0], &win[1]);
        if pos <= b.pos {
            let span = (b.pos - a.pos).max(1e-9);
            let t = ((pos - a.pos) / span).clamp(0.0, 1.0);
            let lerp_u8 = |c0: u8, c1: u8| -> u8 {
                (c0 as f64 + (c1 as f64 - c0 as f64) * t)
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            return (
                [
                    lerp_u8(a.color[0], b.color[0]),
                    lerp_u8(a.color[1], b.color[1]),
                    lerp_u8(a.color[2], b.color[2]),
                ],
                a.alpha + (b.alpha - a.alpha) * t,
            );
        }
    }
    let last = &stops[stops.len() - 1];
    (last.color, last.alpha)
}

pub(crate) fn actual_zoom_for_bounds(
    img_w: u32,
    img_h: u32,
    vp_w: f32,
    vp_h: f32,
) -> (f32, f32, f32) {
    let vp_w = vp_w.max(1.0);
    let vp_h = vp_h.max(1.0);
    let iw = img_w.max(1) as f32;
    let ih = img_h.max(1) as f32;
    let fit_scale = (vp_w / iw).min(vp_h / ih).max(1e-6);
    let min_zoom = 0.02_f32;
    let max_zoom = (32.0 / fit_scale).max(32.0);
    let actual_zoom = (1.0 / fit_scale).clamp(min_zoom, max_zoom);
    (actual_zoom, min_zoom, max_zoom)
}

fn active_workspace_zoom_bounds(
    state: &PhotonGui,
    bounds_override: Option<[f32; 2]>,
) -> (f32, f32) {
    match state.snapshot.meta.workspace {
        Workspace::Editor => {
            if let Some(frame) = &state.editor_frame {
                let [vp_w, vp_h] = bounds_override.unwrap_or_else(|| {
                    let r = resolved_editor_canvas_bounds(state);
                    [r.width, r.height]
                });
                let (_, min_z, max_z) =
                    actual_zoom_for_bounds(frame.width, frame.height, vp_w, vp_h);
                (min_z, max_z)
            } else {
                (0.02, 32.0)
            }
        }
        Workspace::Viewer => {
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
                let [vp_w, vp_h] = bounds_override.unwrap_or([
                    (state.window_size.width - 40.0).max(120.0),
                    (state.window_size.height - 130.0).max(120.0),
                ]);
                let (_, min_z, max_z) =
                    actual_zoom_for_bounds(iw.max(1.0) as u32, ih.max(1.0) as u32, vp_w, vp_h);
                (min_z, max_z)
            } else {
                (0.02, 32.0)
            }
        }
        Workspace::Browser => (0.02, 32.0),
    }
}

fn compute_luminance_histogram(frame: &CpuImageFrame) -> Arc<[u32; 256]> {
    let mut bins = [0u32; 256];
    let total_pixels = frame.rgba.len() / 4;
    let step = (total_pixels / 250_000).max(1);
    let mut i = 0usize;
    while i < total_pixels {
        let off = i * 4;
        if off + 3 < frame.rgba.len() && frame.rgba[off + 3] > 0 {
            let r = u32::from(frame.rgba[off]);
            let g = u32::from(frame.rgba[off + 1]);
            let b = u32::from(frame.rgba[off + 2]);
            let luma = ((54 * r + 183 * g + 19 * b) >> 8).min(255) as usize;
            bins[luma] = bins[luma].saturating_add(1);
        }
        i += step;
    }
    Arc::new(bins)
}

pub(crate) fn compute_editor_curve_lut(curves: &photon_protocol::CurvesAdjustment) -> [u8; 1024] {
    let lut_rgb = photon_image::pixels::build_curve_lut(&curves.rgb);
    let lut_r = photon_image::pixels::build_curve_lut(&curves.red);
    let lut_g = photon_image::pixels::build_curve_lut(&curves.green);
    let lut_b = photon_image::pixels::build_curve_lut(&curves.blue);
    let mut out = [0u8; 1024];
    for i in 0..256 {
        let base = i * 4;
        out[base] = lut_rgb[usize::from(lut_r[i])];
        out[base + 1] = lut_rgb[usize::from(lut_g[i])];
        out[base + 2] = lut_rgb[usize::from(lut_b[i])];
        out[base + 3] = 255;
    }
    out
}

fn resolve_desktop_plugins_dir() -> PathBuf {
    if let Ok(env_dir) = std::env::var("PHOTON_PLUGINS_DIR")
        && !env_dir.trim().is_empty()
    {
        return PathBuf::from(env_dir.trim());
    }
    photon_storage::AppPaths::current().data_dir.join("plugins")
}

fn open_folder_in_file_manager(dir: &Path) {
    use std::process::{Command as ProcessCommand, Stdio};
    let _ = ProcessCommand::new("xdg-open")
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn default_value_for_plugin_param(spec: &PluginParamSpec) -> PluginParamValue {
    if let Some(def) = &spec.default {
        return def.clone();
    }
    match spec.kind {
        PluginParamKind::Float => PluginParamValue::Float(spec.min.unwrap_or(0.0)),
        PluginParamKind::Int => PluginParamValue::Int(spec.min.unwrap_or(0.0).round() as i64),
        PluginParamKind::Bool => PluginParamValue::Bool(false),
        PluginParamKind::Select => {
            PluginParamValue::Select(spec.options.first().cloned().unwrap_or_default())
        }
        PluginParamKind::String => {
            PluginParamValue::Select(spec.options.first().cloned().unwrap_or_default())
        }
        PluginParamKind::Color => PluginParamValue::Color([0, 0, 0]),
    }
}

fn build_models_card_for_plugins<'a>(
    state: &'a PhotonGui,
    plugin_ids: &[&str],
) -> Element<'a, Message> {
    let lang = state.snapshot.meta.language;
    let mut allowed_ids = BTreeSet::new();
    for p in &state.snapshot.meta.plugins {
        if plugin_ids.contains(&p.id.as_str()) {
            for m in &p.models {
                allowed_ids.insert(m.id.clone());
            }
        }
    }
    let models: Vec<_> = state
        .snapshot
        .meta
        .ai_models
        .iter()
        .filter(|m| allowed_ids.contains(&m.id))
        .collect();
    let task = state.snapshot.meta.ai_task.as_ref();
    let running = task.is_some_and(|t| t.running);

    let mut col = column![].spacing(6);
    if let Some(t) = task.filter(|t| t.running) {
        col = col.push(
            row![
                text(format!("{}% · {}", t.percent, t.stage))
                    .size(11)
                    .width(Fill),
                button::labeled(
                    tr(lang, "action-cancel"),
                    Some(Icon::Close),
                    ButtonVariant::Danger,
                    Some(Message::Dispatch(Command::AiCancel)),
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }

    if models.is_empty() {
        col = col.push(
            text(tr(lang, "menu-plugins-none"))
                .size(12)
                .style(|th| text::Style {
                    color: Some(PaletteTokens::for_theme(th).text_muted),
                }),
        );
    } else {
        for m in models {
            let size_label = if m.size_bytes > 0 {
                format!("{:.1} MB", m.size_bytes as f64 / (1024.0 * 1024.0))
            } else {
                String::new()
            };
            let badge = if m.installed {
                tr(lang, "model-status-installed")
            } else {
                tr(lang, "model-status-missing")
            };
            let installed = m.installed;
            let mut row_items = row![
                column![
                    text(m.name.clone()).size(12),
                    text(if size_label.is_empty() {
                        m.id.clone()
                    } else {
                        format!("{} · {size_label}", m.id)
                    })
                    .size(10)
                    .style(|th| text::Style {
                        color: Some(PaletteTokens::for_theme(th).text_muted),
                    }),
                ]
                .spacing(2)
                .width(Fill),
                text(badge).size(11).style(move |th| text::Style {
                    color: Some(if installed {
                        PaletteTokens::for_theme(th).success
                    } else {
                        PaletteTokens::for_theme(th).warning
                    }),
                }),
                button::labeled(
                    tr(lang, "action-local-file"),
                    Some(Icon::FolderOpen),
                    ButtonVariant::Ghost,
                    (!running).then(|| Message::PickCustomModelFile(m.id.clone())),
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center);

            if m.installed {
                row_items = row_items.push(button::labeled(
                    tr(lang, "action-delete"),
                    Some(Icon::Trash),
                    ButtonVariant::Danger,
                    (!running).then(|| {
                        Message::Dispatch(Command::AiDeleteModel {
                            model_id: m.id.clone(),
                        })
                    }),
                ));
            } else {
                row_items = row_items.push(button::labeled(
                    tr(lang, "action-download"),
                    None,
                    ButtonVariant::Primary,
                    (!running).then(|| {
                        Message::Dispatch(Command::AiDownloadModel {
                            model_id: m.id.clone(),
                        })
                    }),
                ));
            }
            col = col.push(row_items);
        }
    }

    col.into()
}

fn handle_viewer_translate_message(state: &mut PhotonGui, msg: ViewerMessage) -> Task<Message> {
    let lang = state.snapshot.meta.language;
    match msg {
        ViewerMessage::ToggleTranslateOverlay => {
            state.viewer_translate.active = !state.viewer_translate.active;
            if state.viewer_translate.active {
                if let Some(saved) = &state.snapshot.meta.viewer_saved_translation
                    && Some(saved.path.as_str()) == state.snapshot.meta.viewer_path.as_deref()
                {
                    state.viewer_translate.blocks = saved.blocks.clone();
                    state.viewer_translate.saved_sidecar = true;
                }
            } else {
                state.viewer_translate.picking_region = false;
                state.viewer_translate.editing_block_idx = None;
            }
            sync_ui_tree_task(state)
        }
        ViewerMessage::CloseTranslateOverlay => {
            state.viewer_translate.active = false;
            state.viewer_translate.picking_region = false;
            state.viewer_translate.editing_block_idx = None;
            sync_ui_tree_task(state)
        }
        ViewerMessage::SetTranslateMode(mode) => {
            state.viewer_translate.mode = mode;
            if mode != ViewerTranslateMode::Pan {
                state.viewer_translate.picking_region = false;
            }
            Task::none()
        }
        ViewerMessage::SetTranslateOcrEngine(engine) => {
            state.viewer_translate.ocr_engine = engine;
            Task::none()
        }
        ViewerMessage::SetTranslateSourceLang(s) => {
            state.viewer_translate.source_lang = s;
            Task::none()
        }
        ViewerMessage::SetTranslateTargetLang(s) => {
            state.viewer_translate.target_lang = s;
            Task::none()
        }
        ViewerMessage::SwapTranslateLangs => {
            std::mem::swap(
                &mut state.viewer_translate.source_lang,
                &mut state.viewer_translate.target_lang,
            );
            Task::none()
        }
        ViewerMessage::StartTranslateRegionPick => {
            state.context_menu = None;
            state.viewer_translate.active = true;
            state.viewer_translate.picking_region = !state.viewer_translate.picking_region;
            sync_ui_tree_task(state)
        }
        ViewerMessage::ScanTranslatePage => {
            let Some(path) = state.snapshot.meta.viewer_path.clone() else {
                return Task::none();
            };
            state.viewer_translate.picking_region = false;
            let target_lang = if state.viewer_translate.mode == ViewerTranslateMode::OcrOnly {
                String::new()
            } else {
                state.viewer_translate.target_lang.clone()
            };
            dispatch_to_core(
                state.core.clone(),
                Command::ViewerTranslate {
                    path,
                    source_lang: state.viewer_translate.source_lang.clone(),
                    target_lang,
                    region: None,
                    ocr_engine: state.viewer_translate.ocr_engine.as_str().to_owned(),
                },
            )
        }
        ViewerMessage::ToggleTranslateBubbles => {
            state.viewer_translate.show_bubbles = !state.viewer_translate.show_bubbles;
            Task::none()
        }
        ViewerMessage::SetTranslateFontScale(scale) => {
            state.viewer_translate.font_scale = scale.clamp(0.7, 2.0);
            Task::none()
        }
        ViewerMessage::SelectTranslateBlock(idx) => {
            state.viewer_translate.editing_block_idx = idx;
            Task::none()
        }
        ViewerMessage::UpdateTranslateBlockText(idx, text_val) => {
            if let Some(b) = state.viewer_translate.blocks.get_mut(idx) {
                if state.viewer_translate.mode == ViewerTranslateMode::OcrOnly {
                    b.original = text_val;
                } else {
                    b.translated = text_val;
                }
            }
            Task::none()
        }
        ViewerMessage::MoveTranslateBlock(idx, dx, dy) => {
            if let Some(b) = state.viewer_translate.blocks.get_mut(idx) {
                b.x = (b.x + dx).max(0);
                b.y = (b.y + dy).max(0);
            }
            Task::none()
        }
        ViewerMessage::RetryTranslateBlock(idx) => {
            let Some(path) = state.snapshot.meta.viewer_path.clone() else {
                return Task::none();
            };
            let Some(b) = state.viewer_translate.blocks.get(idx).cloned() else {
                return Task::none();
            };
            let target_lang = if state.viewer_translate.mode == ViewerTranslateMode::OcrOnly {
                String::new()
            } else {
                state.viewer_translate.target_lang.clone()
            };
            dispatch_to_core(
                state.core.clone(),
                Command::ViewerTranslate {
                    path,
                    source_lang: state.viewer_translate.source_lang.clone(),
                    target_lang,
                    region: Some([b.x.max(0), b.y.max(0), b.w.max(8), b.h.max(8)]),
                    ocr_engine: state.viewer_translate.ocr_engine.as_str().to_owned(),
                },
            )
        }
        ViewerMessage::RemoveTranslateBlock(idx) => {
            if idx < state.viewer_translate.blocks.len() {
                state.viewer_translate.blocks.remove(idx);
                if state.viewer_translate.editing_block_idx == Some(idx) {
                    state.viewer_translate.editing_block_idx = None;
                }
            }
            Task::none()
        }
        ViewerMessage::SaveTranslationSidecar => {
            let Some(path) = state.snapshot.meta.viewer_path.clone() else {
                return Task::none();
            };
            dispatch_to_core(
                state.core.clone(),
                Command::ViewerSaveTranslation {
                    path,
                    blocks: state.viewer_translate.blocks.clone(),
                    source_lang: state.viewer_translate.source_lang.clone(),
                    target_lang: state.viewer_translate.target_lang.clone(),
                },
            )
        }
        ViewerMessage::DeleteTranslationSidecar => {
            let Some(path) = state.snapshot.meta.viewer_path.clone() else {
                return Task::none();
            };
            dispatch_to_core(
                state.core.clone(),
                Command::ViewerDeleteTranslation { path },
            )
        }
        ViewerMessage::CopyTranslatedText => {
            let ocr_only = state.viewer_translate.mode == ViewerTranslateMode::OcrOnly;
            let combined: String = state
                .viewer_translate
                .blocks
                .iter()
                .map(|b| {
                    if ocr_only || b.translated.trim().is_empty() {
                        b.original.trim()
                    } else {
                        b.translated.trim()
                    }
                })
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if combined.is_empty() {
                return Task::none();
            }
            let toast_task = show_toast(
                state,
                tr(lang, "toast-clipboard-copied"),
                ToastKind::Success,
            );
            Task::batch([iced::clipboard::write(combined), toast_task])
        }
        _ => Task::none(),
    }
}

fn handle_editor_ai_message(state: &mut PhotonGui, msg: EditorMessage) -> Task<Message> {
    let ui = &mut state.editor_ui;
    match msg {
        EditorMessage::AiSetRembgModel(id) => {
            ui.ai_rembg_model = id;
            Task::none()
        }
        EditorMessage::AiSetRembgFeather(f) => {
            ui.ai_rembg_feather = f.min(20);
            Task::none()
        }
        EditorMessage::AiSetRembgRefine(m) => {
            ui.ai_rembg_refine = m;
            Task::none()
        }
        EditorMessage::AiSetRembgRefineRadius(r) => {
            ui.ai_rembg_refine_radius = r.clamp(2.0, 150.0);
            Task::none()
        }
        EditorMessage::AiSetRembgRefineSoftness(s) => {
            ui.ai_rembg_refine_softness = s.clamp(0.0, 1.0);
            Task::none()
        }
        EditorMessage::AiRunRemoveBackground => dispatch_to_core(
            state.core.clone(),
            Command::EditorAiRemoveBackground {
                model_id: ui.ai_rembg_model.clone(),
                feather: ui.ai_rembg_feather,
                target: ui.apply_target,
            },
        ),
        EditorMessage::AiSetWatermarkTab(t) => {
            ui.ai_wm_tab = t;
            Task::none()
        }
        EditorMessage::AiSetWmDetector(d) => {
            ui.ai_wm_detector = d;
            Task::none()
        }
        EditorMessage::AiSetWmInpainter(i) => {
            ui.ai_wm_inpainter = i;
            Task::none()
        }
        EditorMessage::AiSetWmConfidence(c) => {
            ui.ai_wm_confidence = c.clamp(0.05, 0.95);
            Task::none()
        }
        EditorMessage::AiSetWmMaskPadding(p) => {
            ui.ai_wm_mask_padding = p.min(64);
            Task::none()
        }
        EditorMessage::AiSetWmMaskBrushRadius(r) => {
            ui.ai_wm_brush_radius = r.clamp(4.0, 150.0);
            Task::none()
        }
        EditorMessage::AiClearWmMaskStrokes => {
            ui.ai_wm_mask_strokes.clear();
            Task::none()
        }
        EditorMessage::AiSetWmRegion(x, y, w, h) => {
            ui.ai_wm_region_x = x.clamp(0.0, 99.0);
            ui.ai_wm_region_y = y.clamp(0.0, 99.0);
            ui.ai_wm_region_w = w.clamp(1.0, 100.0);
            ui.ai_wm_region_h = h.clamp(1.0, 100.0);
            Task::none()
        }
        EditorMessage::AiSetWmGeminiSize(s) => {
            ui.ai_wm_gemini_size = s;
            Task::none()
        }
        EditorMessage::AiSetWmGeminiBrightness(b) => {
            ui.ai_wm_gemini_brightness = b.clamp(0.0, 1.0);
            Task::none()
        }
        EditorMessage::AiSetWmGeminiOffsetX(x) => {
            ui.ai_wm_gemini_offset_x = x.clamp(-20, 20);
            Task::none()
        }
        EditorMessage::AiSetWmGeminiOffsetY(y) => {
            ui.ai_wm_gemini_offset_y = y.clamp(-20, 20);
            Task::none()
        }
        EditorMessage::AiToggleWmGeminiForce => {
            ui.ai_wm_gemini_force = !ui.ai_wm_gemini_force;
            Task::none()
        }
        EditorMessage::AiSetWmRegionFeather(f) => {
            ui.ai_wm_region_feather = f.min(32);
            Task::none()
        }
        EditorMessage::AiRunWatermark => {
            let (doc_w, doc_h) = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .map(|ed| (ed.width.max(1) as f32, ed.height.max(1) as f32))
                .unwrap_or((1000.0, 1000.0));
            let rx = ((ui.ai_wm_region_x / 100.0) * doc_w).round() as i32;
            let ry = ((ui.ai_wm_region_y / 100.0) * doc_h).round() as i32;
            let rw = (((ui.ai_wm_region_w / 100.0) * doc_w).round() as i32).max(1);
            let rh = (((ui.ai_wm_region_h / 100.0) * doc_h).round() as i32).max(1);
            let target = ui.apply_target;
            match ui.ai_wm_tab {
                WatermarkTab::AiAuto => {
                    let strokes = ui.ai_wm_mask_strokes.clone();
                    let detector_model = ui.ai_wm_detector.clone();
                    let inpainter_model = ui.ai_wm_inpainter.clone();
                    let confidence = ui.ai_wm_confidence;
                    let mask_padding = ui.ai_wm_mask_padding;
                    dispatch_to_core(
                        state.core.clone(),
                        Command::EditorAiInpaint {
                            detector_model,
                            inpainter_model,
                            confidence,
                            mask_padding,
                            region_x: rx,
                            region_y: ry,
                            region_width: rw,
                            region_height: rh,
                            brush_strokes: strokes,
                            target,
                        },
                    )
                }
                WatermarkTab::Gemini => {
                    let size = match ui.ai_wm_gemini_size {
                        Some(48) => "48".to_owned(),
                        Some(96) => "96".to_owned(),
                        _ => "auto".to_owned(),
                    };
                    dispatch_to_core(
                        state.core.clone(),
                        Command::EditorDewatermarkGemini {
                            size,
                            logo_value: ui.ai_wm_gemini_brightness,
                            force: ui.ai_wm_gemini_force,
                            offset_x: ui.ai_wm_gemini_offset_x,
                            offset_y: ui.ai_wm_gemini_offset_y,
                            target,
                        },
                    )
                }
                WatermarkTab::Region => dispatch_to_core(
                    state.core.clone(),
                    Command::EditorInpaintRegion {
                        x: rx,
                        y: ry,
                        width: rw,
                        height: rh,
                        feather: ui.ai_wm_region_feather,
                        target,
                    },
                ),
            }
        }
        EditorMessage::AiSetCaptionModel(m) => {
            ui.ai_caption_model = m;
            Task::none()
        }
        EditorMessage::AiSetCaptionGeneralThreshold(t) => {
            ui.ai_caption_general_threshold = t.clamp(0.1, 0.9);
            Task::none()
        }
        EditorMessage::AiSetCaptionCharThreshold(t) => {
            ui.ai_caption_char_threshold = t.clamp(0.1, 0.95);
            Task::none()
        }
        EditorMessage::AiToggleCaptionUnderscores => {
            ui.ai_caption_replace_underscores = !ui.ai_caption_replace_underscores;
            Task::none()
        }
        EditorMessage::AiSetCaptionPrefix(p) => {
            ui.ai_caption_prefix = p;
            Task::none()
        }
        EditorMessage::AiSetCaptionSuffix(s) => {
            ui.ai_caption_suffix = s;
            Task::none()
        }
        EditorMessage::AiSetCaptionExclude(e) => {
            ui.ai_caption_exclude = e;
            Task::none()
        }
        EditorMessage::AiSetCaptionDraft(d) => {
            ui.ai_caption_draft = d;
            Task::none()
        }
        EditorMessage::AiRunCaption => {
            let model_id = ui.ai_caption_model.clone();
            let general_threshold = ui.ai_caption_general_threshold;
            let character_threshold = ui.ai_caption_char_threshold;
            let replace_underscores = ui.ai_caption_replace_underscores;
            let prefix = ui.ai_caption_prefix.clone();
            let suffix = ui.ai_caption_suffix.clone();
            let exclude_tags = ui.ai_caption_exclude.clone();
            dispatch_to_core(
                state.core.clone(),
                Command::EditorGenerateCaption {
                    model_id,
                    general_threshold,
                    character_threshold,
                    replace_underscores,
                    prefix,
                    suffix,
                    exclude_tags,
                },
            )
        }
        EditorMessage::AiSaveCaption(text) => {
            ui.ai_caption_draft = text.clone();
            dispatch_to_core(state.core.clone(), Command::EditorSaveCaption { text })
        }
        _ => Task::none(),
    }
}

fn write_png_bytes_to_clipboard(png_bytes: &[u8]) -> bool {
    use std::io::Write;
    use std::process::{Command as ProcessCommand, Stdio};

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
            let _ = stdin.write_all(png_bytes);
        }
        child.wait().is_ok_and(|s| s.success())
    };

    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && try_pipe("wl-copy", &["--type", "image/png"])
    {
        return true;
    }
    if try_pipe("wl-copy", &["--type", "image/png"])
        || try_pipe("xclip", &["-selection", "clipboard", "-t", "image/png"])
    {
        return true;
    }

    if let Ok(dyn_img) = image::load_from_memory(png_bytes) {
        let rgba = dyn_img.to_rgba8();
        let (w, h) = rgba.dimensions();
        if let Ok(mut cb) = arboard::Clipboard::new() {
            return cb
                .set_image(arboard::ImageData {
                    width: w as usize,
                    height: h as usize,
                    bytes: std::borrow::Cow::Owned(rgba.into_raw()),
                })
                .is_ok();
        }
    }
    false
}

fn read_clipboard_png_bytes() -> Option<Vec<u8>> {
    use image::ImageEncoder;
    use std::process::Command as ProcessCommand;

    let try_read = |program: &str, args: &[&str]| -> Option<Vec<u8>> {
        let out = ProcessCommand::new(program).args(args).output().ok()?;
        if out.status.success() && !out.stdout.is_empty() {
            Some(out.stdout)
        } else {
            None
        }
    };

    if let Ok(mut cb) = arboard::Clipboard::new()
        && let Ok(img) = cb.get_image()
    {
        let mut png_bytes = Vec::new();
        if image::codecs::png::PngEncoder::new(&mut png_bytes)
            .write_image(
                img.bytes.as_ref(),
                img.width as u32,
                img.height as u32,
                image::ExtendedColorType::Rgba8,
            )
            .is_ok()
            && !png_bytes.is_empty()
        {
            return Some(png_bytes);
        }
    }

    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && let Some(bytes) = try_read("wl-paste", &["--no-newline", "--type", "image/png"])
            .or_else(|| try_read("wl-paste", &["--type", "image/png"]))
    {
        return Some(bytes);
    }
    try_read(
        "xclip",
        &["-selection", "clipboard", "-t", "image/png", "-o"],
    )
}

fn write_file_uris_to_clipboard(paths: &[String]) -> bool {
    use std::io::Write;
    use std::process::{Command as ProcessCommand, Stdio};

    let uris: String = paths.iter().map(|p| format!("file://{p}\n")).collect();
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
            let _ = stdin.write_all(uris.as_bytes());
        }
        child.wait().is_ok_and(|s| s.success())
    };
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && try_pipe("wl-copy", &["--type", "text/uri-list"])
    {
        return true;
    }
    if try_pipe("wl-copy", &["--type", "text/uri-list"])
        || try_pipe("xclip", &["-selection", "clipboard", "-t", "text/uri-list"])
    {
        return true;
    }
    if let Ok(mut cb) = arboard::Clipboard::new() {
        return cb.set_text(uris).is_ok();
    }
    false
}

fn read_clipboard_uri_list() -> Vec<String> {
    use std::process::Command as ProcessCommand;

    let try_read = |program: &str, args: &[&str]| -> Option<String> {
        let out = ProcessCommand::new(program).args(args).output().ok()?;
        if out.status.success() {
            String::from_utf8(out.stdout).ok()
        } else {
            None
        }
    };

    let raw = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        try_read("wl-paste", &["--no-newline", "--type", "text/uri-list"]).or_else(|| {
            try_read(
                "xclip",
                &["-selection", "clipboard", "-t", "text/uri-list", "-o"],
            )
        })
    } else {
        try_read(
            "xclip",
            &["-selection", "clipboard", "-t", "text/uri-list", "-o"],
        )
        .or_else(|| try_read("wl-paste", &["--no-newline", "--type", "text/uri-list"]))
    };
    let Some(text) = raw else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let path = trimmed.strip_prefix("file://").unwrap_or(trimmed);
            let p = Path::new(path);
            if p.exists() {
                Some(path.to_owned())
            } else {
                None
            }
        })
        .collect()
}

fn resolve_browser_clipboard_paste(in_app_files: Vec<String>) -> Option<BrowserClipboardPaste> {
    let os_uris = read_clipboard_uri_list();
    if !os_uris.is_empty() {
        return Some(BrowserClipboardPaste::Files(os_uris));
    }
    if let Some(png_bytes) = read_clipboard_png_bytes() {
        return Some(BrowserClipboardPaste::ImagePng(png_bytes));
    }
    if !in_app_files.is_empty() {
        return Some(BrowserClipboardPaste::Files(in_app_files));
    }
    None
}

fn copy_image_png_to_clipboard(
    path: Option<&str>,
    cached_rgba: Option<(u32, u32, Arc<Vec<u8>>)>,
) -> bool {
    use image::ImageEncoder;

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

    write_png_bytes_to_clipboard(&png_bytes)
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

fn canonicalize_path_str(raw: &str) -> PathBuf {
    std::fs::canonicalize(raw).unwrap_or_else(|_| PathBuf::from(raw))
}

fn spawn_detached_exe(args: &[String]) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let _ = cmd.spawn();
}

fn open_or_focus_viewer_process(paths: Vec<String>) -> Task<Message> {
    if paths.is_empty() {
        return Task::none();
    }
    Task::perform(
        async move {
            let app_paths = photon_storage::AppPaths::current();
            for raw in paths {
                let canon = canonicalize_path_str(&raw);
                let _lock = photon_ipc::acquire_viewer_path_lock(
                    &app_paths,
                    &canon,
                    Duration::from_secs(2),
                );
                if let Some(matched) = photon_ipc::find_live_viewer_for_path(&app_paths, &canon) {
                    let env = Envelope::new(
                        format!("gui-{}", std::process::id()),
                        ClientKind::Gui,
                        Command::FocusWindow,
                    );
                    let _ = photon_ipc::send_once(
                        &matched.info.socket_path,
                        &photon_ipc::IpcRequest::Dispatch { envelope: env },
                    )
                    .await;
                    continue;
                }
                spawn_detached_exe(&["--viewer-window".to_owned(), raw]);
                // Hold the per-path lock briefly until the child registers so rapid double-clicks
                // deduplicate against the new process instead of racing a duplicate window.
                let deadline = Instant::now() + Duration::from_millis(500);
                while Instant::now() < deadline {
                    if photon_ipc::find_live_viewer_for_path(&app_paths, &canon).is_some() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
        },
        |_| Message::Noop,
    )
}

fn open_in_editor_or_forward(state: &PhotonGui, path: String) -> Task<Message> {
    if !is_standalone_viewer() {
        return dispatch_to_core(state.core.clone(), Command::EditorOpen { path });
    }
    Task::perform(
        async move {
            let app_paths = photon_storage::AppPaths::current();
            let env = Envelope::new(
                format!("gui-{}", std::process::id()),
                ClientKind::Gui,
                Command::EditorOpen { path: path.clone() },
            );
            if app_paths.socket_path.exists()
                && photon_ipc::send_once(
                    &app_paths.socket_path,
                    &photon_ipc::IpcRequest::Dispatch { envelope: env },
                )
                .await
                .is_ok()
            {
                let focus_env = Envelope::new(
                    format!("gui-{}", std::process::id()),
                    ClientKind::Gui,
                    Command::FocusWindow,
                );
                let _ = photon_ipc::send_once(
                    &app_paths.socket_path,
                    &photon_ipc::IpcRequest::Dispatch {
                        envelope: focus_env,
                    },
                )
                .await;
            } else {
                spawn_detached_exe(&["edit".to_owned(), path]);
            }
        },
        |_| Message::Noop,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_pan_allows_overscroll_and_cursor_zoom_without_locking() {
        let (mut pan_x, mut pan_y) = (0.0_f32, 0.0_f32);
        let mut zoom = 1.0_f32;

        // Even at zoom = 1.0 (fit), panning freely moves off-center without snapping back to 0.
        apply_canvas_pan(&mut pan_x, &mut pan_y, 120.0, -80.0);
        assert_eq!(pan_x, 120.0);
        assert_eq!(pan_y, -80.0);

        // Large-image dynamic zoom range allows 1:1 and beyond without clamping to 16.0.
        let (actual, min_z, max_z) = actual_zoom_for_bounds(16000, 12000, 800.0, 600.0);
        assert!(actual >= 20.0);
        apply_canvas_zoom_at_cursor(
            &mut zoom,
            &mut pan_x,
            &mut pan_y,
            actual,
            Some([400.0, 300.0]),
            [800.0, 600.0],
            min_z,
            max_z,
        );
        assert!((zoom - actual).abs() < 1e-4);
    }

    #[test]
    fn selection_modify_kind_validates_ranges() {
        let lang = Language::English;
        assert!(
            SelectionModifyKind::Feather
                .parse_and_validate("0.5", lang)
                .is_ok()
        );
        assert!(
            SelectionModifyKind::Feather
                .parse_and_validate("250.0", lang)
                .is_ok()
        );
        assert!(
            SelectionModifyKind::Feather
                .parse_and_validate("0.4", lang)
                .is_err()
        );
        assert!(
            SelectionModifyKind::Feather
                .parse_and_validate("250.1", lang)
                .is_err()
        );

        assert!(
            SelectionModifyKind::Expand
                .parse_and_validate("1", lang)
                .is_ok()
        );
        assert!(
            SelectionModifyKind::Expand
                .parse_and_validate("500", lang)
                .is_ok()
        );
        assert!(
            SelectionModifyKind::Expand
                .parse_and_validate("0", lang)
                .is_err()
        );
        assert!(
            SelectionModifyKind::Expand
                .parse_and_validate("501", lang)
                .is_err()
        );

        assert!(
            SelectionModifyKind::Grow
                .parse_and_validate("0", lang)
                .is_ok()
        );
        assert!(
            SelectionModifyKind::Grow
                .parse_and_validate("255", lang)
                .is_ok()
        );
        assert!(
            SelectionModifyKind::Grow
                .parse_and_validate("256", lang)
                .is_err()
        );
    }

    #[test]
    fn dynamic_large_image_actual_zoom_reaches_one_to_one_beyond_16x() {
        // 16000x12000 image in a 1000x800 viewport => fit_scale = 1000/16000 = 0.0625.
        // 1:1 pixel zoom = 16.0; for 32768x24576 in 800x600 => fit_scale = 800/32768 ≈ 0.0244,
        // so 1:1 pixel zoom ≈ 40.96 (well above the old hardcoded 16.0 ceiling).
        let (actual_32k, min_z, max_z) = actual_zoom_for_bounds(32768, 24576, 800.0, 600.0);
        assert_eq!(min_z, 0.02);
        assert!(
            actual_32k > 40.0,
            "expected 1:1 zoom > 40, got {actual_32k}"
        );
        assert!(max_z > actual_32k * 30.0);
        let fit_scale = (800.0_f32 / 32768.0).min(600.0_f32 / 24576.0);
        assert!((actual_32k * fit_scale - 1.0).abs() < 1e-4);
    }

    #[test]
    fn dialog_defaults_cover_curves_denoise_and_plugins() {
        let curves = default_window_state_for(DesktopDialogId::AdjustCurves);
        assert_eq!(curves.width, 460.0);
        assert_eq!(curves.height, 520.0);
        assert_eq!(curves.min_width, 380.0);
        assert_eq!(curves.min_height, 440.0);

        let plugins = default_window_state_for(DesktopDialogId::ManagePlugins);
        assert_eq!(plugins.width, 640.0);
        assert_eq!(plugins.height, 480.0);

        let denoise = default_window_state_for(DesktopDialogId::AdjustDenoise);
        assert_eq!(denoise.width, 440.0);
    }

    #[test]
    fn combined_editor_curve_lut_composes_channel_and_master_curves() {
        let curves = photon_protocol::CurvesAdjustment {
            rgb: vec![[0.0, 1.0], [1.0, 0.0]],
            red: vec![[0.0, 0.0], [1.0, 1.0]],
            green: vec![[0.0, 0.0], [1.0, 1.0]],
            blue: vec![[0.0, 0.0], [1.0, 1.0]],
        };
        let lut = compute_editor_curve_lut(&curves);
        assert_eq!(lut[0], 255);
        assert_eq!(lut[1], 255);
        assert_eq!(lut[2], 255);
        assert_eq!(lut[3], 255);
        assert_eq!(lut[255 * 4], 0);
        assert_eq!(lut[255 * 4 + 3], 255);
    }

    #[test]
    fn canvas_size_dialog_has_sensible_defaults_and_id_mapping() {
        let state = default_window_state_for(DesktopDialogId::CanvasSize);
        assert_eq!(state.width, 460.0);
        assert_eq!(state.height, 390.0);
        let dlg = InAppDialog::CanvasSize {
            dims: DimensionInputState::from_dims(100, 100),
            anchor: photon_protocol::CanvasAnchor::Center,
            fill: None,
            error: None,
        };
        assert_eq!(dialog_id_for_in_app(&dlg), DesktopDialogId::CanvasSize);
    }
}
