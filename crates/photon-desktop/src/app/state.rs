use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowRole {
    Main,
    AuxiliaryViewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveMenu {
    BrowserFile,
    BrowserEdit,
    BrowserView,
    BrowserTools,
    BrowserPlugins,
    BrowserWindow,
    BrowserHelp,
    BrowserSort,
    ReaderOptions,
    ViewerUtilities,
    EditorFile,
    EditorEdit,
    EditorImage,
    EditorLayer,
    EditorVector,
    EditorFilter,
    EditorPlugins,
    EditorSelect,
    EditorView,
    EditorWindow,
    EditorHelp,
    EditorAddLayer,
    EditorToolFlyout(EditorToolGroup),
    WorkspaceSwitcher,
    WorkspacePanels,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DesktopDialogId {
    Settings,
    NewFolder,
    NewDocument,
    ConfirmResetLayout,
    ConfirmUnsavedLeave,
    Resize,
    CanvasSize,
    Export,
    ColorPicker,
    BatchResize,
    BatchCrop,
    BatchConvert,
    BatchTransform,
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
    TransformEach,
    VectorBlend,
    ExportVectorSvg,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ActiveContextMenu {
    BrowserEntry {
        x: f32,
        y: f32,
        path: String,
        plugins_submenu_open: bool,
        active_plugin_submenu: Option<String>,
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
        zoom_submenu_open: bool,
        plugins_submenu_open: bool,
        active_plugin_submenu: Option<String>,
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
    EditorTab {
        x: f32,
        y: f32,
        tab_id: u64,
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
    pub(super) const fn title_key(self) -> &'static str {
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

    pub(super) const fn label_key(self) -> &'static str {
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

    pub(super) const fn hint_key(self) -> &'static str {
        match self {
            Self::Feather => "hint-select-feather-range",
            Self::Expand | Self::Contract | Self::Border | Self::Smooth => "hint-select-px-range",
            Self::Grow | Self::Similar => "hint-select-tolerance-range",
        }
    }

    pub(super) const fn error_key(self) -> &'static str {
        match self {
            Self::Feather => "error-select-feather-range",
            Self::Expand | Self::Contract | Self::Border | Self::Smooth => "error-select-px-range",
            Self::Grow | Self::Similar => "error-select-tolerance-range",
        }
    }

    pub(super) const fn default_value(self) -> &'static str {
        match self {
            Self::Feather => "2.0",
            Self::Expand | Self::Contract | Self::Smooth => "2",
            Self::Border => "4",
            Self::Grow | Self::Similar => "24",
        }
    }

    pub(super) fn parse_and_validate(
        self,
        raw: &str,
        lang: Language,
    ) -> Result<SelectionModifyOp, String> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewDocBackground {
    Transparent,
    White,
    Black,
    Custom([u8; 3]),
}

impl NewDocBackground {
    pub const fn to_rgba(self) -> [u8; 4] {
        match self {
            Self::Transparent => [0, 0, 0, 0],
            Self::White => [255, 255, 255, 255],
            Self::Black => [0, 0, 0, 255],
            Self::Custom([r, g, b]) => [r, g, b, 255],
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) enum TranslateDragTarget {
    #[default]
    None,
    Bubble {
        page_path: Option<String>,
        idx: usize,
        start_cursor: Point,
        dx_px: f32,
        dy_px: f32,
    },
    TopBar {
        start_cursor: Point,
        base_offset: [f32; 2],
    },
    ResultCard {
        start_cursor: Point,
        base_offset: [f32; 2],
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WebtoonMarqueeDrag {
    pub(crate) path: String,
    pub(crate) start_local: [f32; 2],
    pub(crate) cur_local: [f32; 2],
    pub(crate) start_cursor: Point,
    pub(crate) disp_w: f32,
    pub(crate) disp_h: f32,
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
    NewDocument {
        dims: DimensionInputState,
        background: NewDocBackground,
        custom_rgb: [u8; 3],
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
        selection_only: bool,
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
    TransformEach {
        spec: TransformEachSpec,
    },
    VectorBlend {
        spec: VectorBlendSpec,
    },
    ExportVectorSvg {
        path: String,
        error: Option<String>,
    },
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
pub(crate) enum PluginSettingsTab {
    #[default]
    General,
    Models,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SettingsTab {
    #[default]
    General,
    Viewer,
    Browser,
    Editor,
    Plugins,
}

impl SettingsTab {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Viewer => "viewer",
            Self::Browser => "browser",
            Self::Editor => "editor",
            Self::Plugins => "plugins",
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
    pub(super) windows: BTreeMap<window::Id, WindowRole>,
    pub(super) main_window: Option<window::Id>,
    pub(super) last_active_window: Option<window::Id>,
    pub(super) active_dialog_window: Option<window::Id>,
    pub(crate) window_size: Size,
    pub(crate) os_scale_factor: f32,
    pub(super) resize_save_seq: u64,
    pub(super) suppress_resize_until: Option<Instant>,
    pub(crate) folder_input: String,
    pub(crate) browser_filter: String,
    pub(crate) browser_filter_bar_open: bool,
    pub(crate) browser_filter_case_sensitive: bool,
    pub(crate) browser_filter_regex: bool,
    pub(super) last_click: Option<(String, Instant)>,
    pub(crate) thumbnails: HashMap<String, iced_image::Handle>,
    pub(super) thumbnails_loading: BTreeSet<String>,
    pub(crate) browser_visible_range: Option<(usize, usize)>,
    pub(crate) editor_layer_thumbs: HashMap<u64, iced_image::Handle>,
    pub(crate) editor_mask_thumbs: HashMap<u64, iced_image::Handle>,
    pub(crate) zoom: f32,
    pub(crate) pan_x: f32,
    pub(crate) pan_y: f32,
    pub(crate) pending_adj: ColorAdjustments,
    pub(crate) viewer_adj: ColorAdjustments,
    pub(super) transfer_adj_to_editor: Option<ColorAdjustments>,
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
    pub(crate) pending_close_tabs: Vec<u64>,
    pub(crate) new_doc_clipboard_dims: Option<(u32, u32)>,
    pub(crate) new_doc_clipboard_png: Option<Vec<u8>>,
    pub(crate) new_doc_create_from_clipboard: bool,
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
    pub(crate) viewer_filmstrip_offset: f32,
    pub(crate) viewer_color_popover_open: bool,
    pub(crate) dock_drag: DockDragState,
    pub(crate) browser_left_width_drag: Option<f32>,
    pub(crate) browser_right_width_drag: Option<f32>,
    pub(crate) viewer_right_width_preview: Option<f32>,
    pub(crate) viewer_right_split_preview: Option<f32>,
    pub(crate) editor_tool_tab: EditorToolTab,
    pub(crate) editor_history_open: bool,
    pub(crate) editor_layers_open: bool,
    pub(crate) editor_history_collapsed: bool,
    pub(crate) editor_layers_collapsed: bool,
    pub(crate) editor_panels: SidebarOrderState<EditorRightPanel>,
    pub(crate) editor_panel_order: Vec<EditorRightPanel>,
    pub(crate) dragging_editor_panel: Option<EditorRightPanel>,
    pub(crate) editor_panel_drop_target: Option<EditorRightPanel>,
    pub(super) pending_leave_to_browser: bool,
    pub(super) pending_layout_commands: usize,
    pub(super) last_core_layout: photon_protocol::LayoutState,
    pub(super) pending_screenshots: BTreeMap<window::Id, (String, String)>,
    pub(super) settings_open: bool,
    pub(super) settings_tab: SettingsTab,
    pub(super) plugin_settings_tab: PluginSettingsTab,
    pub(super) settings_collapsed: BTreeSet<&'static str>,
    pub(super) settings_confirm_factory_reset: bool,
    pub(super) plugins_search: String,
    pub(super) plugins_selected_id: Option<String>,
    pub(super) translate_draft: TranslateSettings,
    pub(super) dialog: Option<InAppDialog>,
    pub(crate) dialog_manager: DialogManager<DesktopDialogId>,
    pub(super) toast: Option<ToastState>,
    pub(super) toast_seq: u64,
    pub(crate) path_editing: bool,
    pub(crate) active_menu: Option<ActiveMenu>,
    pub(crate) editor_add_layer_anchor: Option<Point>,
    pub(crate) context_menu: Option<ActiveContextMenu>,
    pub(crate) last_cursor: Option<Point>,
    pub(crate) viewer_quarter_turns: u8,
    pub(crate) webtoon_zoom: f32,
    pub(super) webtoon_scroll_y: f32,
    pub(crate) webtoon_visible_index: Option<usize>,
    pub(crate) image_sizes: HashMap<String, (u32, u32)>,
    pub(crate) webtoon_pages: HashMap<String, iced_image::Handle>,
    pub(super) webtoon_loading: BTreeSet<String>,
    pub(super) webtoon_order: VecDeque<String>,
    pub(crate) is_fullscreen: bool,
    pub(crate) crop_uv: Option<[f32; 4]>,
    pub(crate) viewport_background: Option<[u8; 3]>,
    pub(crate) viewer_viewport_background: Option<[u8; 3]>,
    pub(super) ctrl_held: bool,
    pub(super) shift_held: bool,
    pub(super) browser_selection_anchor: Option<String>,
    pub(super) editor_tool_hold: Option<(EditorToolGroup, u64)>,
    pub(super) editor_tool_hold_seq: u64,
    pub(crate) expanded_folders: BTreeSet<String>,
    pub(crate) folder_children: HashMap<String, Vec<(String, String)>>,
    pub(crate) viewer_translate: ViewerTranslateState,
    pub(crate) translate_drag: TranslateDragTarget,
    pub(crate) pending_translate_queue: VecDeque<Command>,
    pub(crate) active_translate_target: Option<String>,
    pub(crate) webtoon_hover: Option<(String, [f32; 2], f32, f32)>,
    pub(crate) webtoon_marquee_drag: Option<WebtoonMarqueeDrag>,
    pub(crate) editor_ai_stroke_pts: Vec<[f64; 2]>,
    pub(super) last_ai_clipboard: Option<String>,
    pub(super) last_ai_error: Option<String>,
    pub(super) editor_tab_view_states: HashMap<u64, EditorTabUiCache>,
    pub(crate) quick_access_editing: bool,
    pub(super) canvas_size_draft: Option<(
        DimensionInputState,
        photon_protocol::CanvasAnchor,
        Option<[u8; 3]>,
    )>,
    pub(super) reopen_canvas_size_after_picker: bool,
    pub(super) new_doc_draft: Option<(DimensionInputState, [u8; 3])>,
    pub(super) reopen_new_doc_after_picker: bool,
}

impl PhotonGui {
    pub(super) fn editor_view_state(&self) -> EditorViewState<'_> {
        let active_menu = match self.active_menu {
            Some(ActiveMenu::EditorFile) => Some(EditorMenuKind::File),
            Some(ActiveMenu::EditorEdit) => Some(EditorMenuKind::Edit),
            Some(ActiveMenu::EditorImage) => Some(EditorMenuKind::Image),
            Some(ActiveMenu::EditorLayer) => Some(EditorMenuKind::Layer),
            Some(ActiveMenu::EditorVector) => Some(EditorMenuKind::Vector),
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

    pub(super) fn browser_view_state(&self) -> BrowserViewState<'_> {
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

    pub(super) fn viewer_view_state(&self, is_main_window: bool) -> ViewerViewState<'_> {
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
            viewer_right_split_preview: self.viewer_right_split_preview,
            is_fullscreen: self.is_fullscreen,
            window_size: self.window_size,
            is_main_window,
            reader_options_open: self.active_menu == Some(ActiveMenu::ReaderOptions),
            workspace_switcher_open: self.active_menu == Some(ActiveMenu::WorkspaceSwitcher),
            utilities_menu_open: self.active_menu == Some(ActiveMenu::ViewerUtilities),
            aa_mode: self.snapshot.meta.layout.canvas_anti_aliasing,
            denoise_mode: self.snapshot.meta.layout.viewport_denoise,
            viewport_background: self.viewer_viewport_background,
            translate: &self.viewer_translate,
        }
    }

    pub(super) fn viewer_ui_presentation(&self) -> ViewerUiPresentation {
        ViewerUiPresentation {
            view_mode: self.viewer_mode,
            info_open: self.viewer_info_open,
            adjustments_open: self.viewer_adjustments_open,
            filmstrip_open: self.viewer_filmstrip_open,
            color_popover_open: self.viewer_color_popover_open,
            translate_open: self.viewer_translate.active,
        }
    }

    pub(super) fn reset_viewer_presentation_from_layout(
        &mut self,
        layout: &photon_protocol::LayoutState,
    ) {
        self.viewer_mode = layout.viewer_view_mode;
        self.viewer_manga_rtl = layout.manga_rtl;
        self.viewer_manga_cover_separate = layout.manga_cover_separate;
        self.viewer_manga_page_gap = layout.manga_page_gap;
        self.viewer_webtoon_fit_mode = layout.webtoon_fit_mode;
        self.viewer_info_open = layout.panel("viewer.info") || layout.panel("viewer.inspector");
        self.viewer_adjustments_open = layout.panel("viewer.adjustments");
        self.viewer_color_popover_open = self.viewer_adjustments_open;
        self.viewer_translate.active = layout.panel("viewer.translate");
        self.viewer_right_panels.set_order(
            layout.viewer_right_panel_order.clone(),
            &ViewerRightPanel::ALL,
        );
        self.viewer_filmstrip_open = layout.panel("viewer.filmstrip");
        self.viewer_quarter_turns = 0;
        self.webtoon_zoom = 1.0;
    }
}
