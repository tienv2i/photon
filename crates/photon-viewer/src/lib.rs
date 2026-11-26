//! Viewer workspace UI (`iced 0.14` + `photon-ui` + `photon-panels`):
//! Single, Double-page spread (LTR/RTL + cover split), and continuous Manhwa/Webtoon strip,
//! unified resizable/reorderable right sidebar (`ViewAdjustment` & `Info`), and bottom Filmstrip bar.

use std::collections::HashMap;
use std::path::Path;

use iced::widget::overlay::menu as iced_menu;
use iced::widget::{
    Shader, button as iced_button, column, container, image as iced_image, mouse_area, pick_list,
    progress_bar, responsive, row, rule, scrollable, stack, text, text_input,
};
use iced::{Alignment, Border, Color, ContentFit, Element, Fill, Length, Padding, Shadow};
use photon_i18n::tr;
use photon_panels::{
    DraggablePanelHeaderSpec, PanelFrameSpec, PanelSplitterEdge, SidebarOrderState, kv_row,
    panel_splitter, sidebar_dock, sidebar_panel, stacked_panel_heights, vertical_panel_splitter,
};
use photon_protocol::{
    CanvasAntiAliasingMode, ColorAdjustments, Command, Language, Theme, TranslateProvider,
    TranslationBlock, UiStateSnapshot, ViewerRightPanel, ViewerViewMode, ViewportDenoiseMode,
    WebtoonFitMode, Workspace,
};
use photon_ui::button::{self, ButtonVariant, TOOLBAR_CONTROL_HEIGHT};
use photon_ui::grid::{FilmstripItem, filmstrip_bar};
use photon_ui::icons::Icon;
use photon_ui::menu::{MenuEntry, MenuItem, menu_panel_entries};
use photon_ui::theme::{
    PaletteTokens, RADIUS_SM, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, canvas_backdrop_style,
    card_style, hairline_rule_style, input_style, popover_style, surface_panel_style,
};
use photon_ui::toolbar::{
    WorkspaceSwitcherMode, WorkspaceTab, toolbar, toolbar_button_group,
    workspace_switcher_responsive,
};
use photon_ui::tooltip::{TooltipPosition, with_tooltip, with_tooltip_pos};

use photon_canvas::{CanvasEvent, CanvasTool, CpuImageFrame, ImageCanvasProgram};

pub type DualSpreadSlots<'a> = (Option<(usize, &'a str)>, Option<(usize, &'a str)>);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ViewerTranslateMode {
    #[default]
    Translate,
    OcrOnly,
    Pan,
    Review,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ViewerOcrEngine {
    #[default]
    Tesseract,
    MangaOcr,
    RapidOcr,
}

impl ViewerOcrEngine {
    pub const ALL: [Self; 3] = [Self::Tesseract, Self::MangaOcr, Self::RapidOcr];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tesseract => "tesseract",
            Self::MangaOcr => "manga_ocr",
            Self::RapidOcr => "rapidocr",
        }
    }

    pub const fn display_label(self) -> &'static str {
        match self {
            Self::Tesseract => "Tesseract",
            Self::MangaOcr => "Manga-OCR",
            Self::RapidOcr => "RapidOCR",
        }
    }
}

impl std::fmt::Display for ViewerOcrEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_label())
    }
}

#[derive(Debug, Clone)]
pub struct ViewerTranslateState {
    pub active: bool,
    pub mode: ViewerTranslateMode,
    pub ocr_engine: ViewerOcrEngine,
    pub source_lang: String,
    pub target_lang: String,
    pub picking_region: bool,
    pub show_bubbles: bool,
    pub frosted_bubbles: bool,
    pub space_pan_held: bool,
    pub font_scale: f32,
    pub editing_block_idx: Option<usize>,
    pub show_original_indices: std::collections::BTreeSet<usize>,
    pub blocks: Vec<TranslationBlock>,
    pub bubble_cache: HashMap<String, Vec<TranslationBlock>>,
    pub pending_replace_idx: Option<usize>,
    pub last_was_region: bool,
    pub saved_sidecar: bool,
    pub last_path: Option<String>,
    pub top_bar_offset: [f32; 2],
    pub result_card_offset: [f32; 2],
    pub result_card_visible: bool,
    pub result_card_show_original: bool,
    pub active_drag_delta: Option<(usize, [f32; 2])>,
    pub active_page_path: Option<String>,
    pub active_drag_page: Option<String>,
    pub show_original_page_blocks: std::collections::BTreeSet<(String, usize)>,
    pub webtoon_marquee: Option<(String, [f32; 4])>,
}

impl Default for ViewerTranslateState {
    fn default() -> Self {
        Self {
            active: false,
            mode: ViewerTranslateMode::Translate,
            ocr_engine: ViewerOcrEngine::Tesseract,
            source_lang: "eng".to_owned(),
            target_lang: "vi".to_owned(),
            picking_region: false,
            show_bubbles: true,
            frosted_bubbles: true,
            space_pan_held: false,
            font_scale: 1.0,
            editing_block_idx: None,
            show_original_indices: std::collections::BTreeSet::new(),
            blocks: Vec::new(),
            bubble_cache: HashMap::new(),
            pending_replace_idx: None,
            last_was_region: false,
            saved_sidecar: false,
            last_path: None,
            top_bar_offset: [0.0, 0.0],
            result_card_offset: [0.0, 0.0],
            result_card_visible: false,
            result_card_show_original: false,
            active_drag_delta: None,
            active_page_path: None,
            active_drag_page: None,
            show_original_page_blocks: std::collections::BTreeSet::new(),
            webtoon_marquee: None,
        }
    }
}

/// Borrowed state slice required to render and compute layout for the Viewer workspace.
#[derive(Clone, Copy)]
pub struct ViewerViewState<'a> {
    pub snapshot: &'a UiStateSnapshot,
    pub viewer_mode: ViewerViewMode,
    pub viewer_frame: Option<&'a CpuImageFrame>,
    pub viewer_pair_frame: Option<&'a CpuImageFrame>,
    pub image_sizes: &'a HashMap<String, (u32, u32)>,
    pub thumbnails: &'a HashMap<String, iced_image::Handle>,
    pub viewer_adj: ColorAdjustments,
    pub viewer_quarter_turns: u8,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub webtoon_zoom: f32,
    pub webtoon_visible_index: Option<usize>,
    pub webtoon_pages: &'a HashMap<String, iced_image::Handle>,
    pub viewer_manga_rtl: bool,
    pub viewer_manga_cover_separate: bool,
    pub viewer_manga_page_gap: u16,
    pub viewer_webtoon_fit_mode: WebtoonFitMode,
    pub viewer_adjustments_open: bool,
    pub viewer_info_open: bool,
    pub viewer_filmstrip_open: bool,
    pub viewer_right_panels: &'a SidebarOrderState<ViewerRightPanel>,
    pub viewer_right_width_preview: Option<f32>,
    pub viewer_right_split_preview: Option<f32>,
    pub is_fullscreen: bool,
    pub window_size: iced::Size,
    pub is_main_window: bool,
    pub reader_options_open: bool,
    pub workspace_switcher_open: bool,
    pub utilities_menu_open: bool,
    pub aa_mode: CanvasAntiAliasingMode,
    pub denoise_mode: ViewportDenoiseMode,
    pub viewport_background: Option<[u8; 3]>,
    pub translate: &'a ViewerTranslateState,
}

/// Messages emitted by the Viewer workspace UI.
#[derive(Debug, Clone)]
pub enum ViewerMessage {
    Noop,
    Dispatch(Command),
    ContextMenuDispatch(Command),
    PickImagesDialog,
    DualStepSpread(isize),
    ZoomStep(f32),
    Canvas(CanvasEvent),
    ViewerCanvas(Option<String>, CanvasEvent),
    ViewerZoomActual,
    ViewerRotatePreview(i8),
    ToggleFullscreen,
    SetViewerViewMode(ViewerViewMode),
    SetAntiAliasingMode(CanvasAntiAliasingMode),
    SetViewportDenoiseMode(ViewportDenoiseMode),
    ToggleReaderOptions,
    ToggleWorkspaceSwitcher,
    ToggleViewerUtilities,
    ToggleWorkspacePanels,
    OpenViewerContextMenu {
        path: Option<String>,
        pos: Option<[f32; 2]>,
    },
    CopyImageToClipboard(Option<String>),
    CopyViewerPath,
    RevealInFolder(String),
    ToggleFilmstrip,
    CloseFilmstrip,
    /// Horizontal scroll offset of the filmstrip in logical pixels.
    FilmstripScrolled {
        offset: f32,
    },
    /// Mouse wheel scroll over the filmstrip bar.
    FilmstripWheelScroll(iced::mouse::ScrollDelta),
    ToggleContextPluginsSubmenu,
    OpenContextPluginsSubmenu,
    ToggleContextPluginGroup(String),
    OpenContextPluginGroup(String),
    ToggleViewerInspector,
    ToggleViewerColorPopover,
    CloseViewerPanel(ViewerRightPanel),
    ToggleViewerPanelCollapse(ViewerRightPanel),
    BeginDragViewerPanel(ViewerRightPanel),
    HoverViewerPanelDrop(ViewerRightPanel),
    DropViewerPanel(ViewerRightPanel),
    SwapViewerRightPanels,
    ViewerSplitterDrag(f32),
    ViewerSplitterRelease(f32),
    ViewerRightSplitDrag(f32),
    ViewerRightSplitRelease(f32),
    SetViewerAdjExposure(f32),
    SetViewerAdjGamma(f32),
    SetViewerAdjBrightness(f32),
    SetViewerAdjContrast(f32),
    SetViewerAdjSaturation(f32),
    SetViewerAdjHighlights(f32),
    SetViewerAdjShadows(f32),
    ResetViewerAdj,
    ViewerAdjOpenInEditor,
    OpenBrowserWindow,
    WebtoonScrolled(scrollable::Viewport),
    OpenInNewViewerWindow(String),
    ToggleTranslateOverlay,
    CloseTranslateOverlay,
    SetTranslateMode(ViewerTranslateMode),
    SetTranslateProvider(TranslateProvider),
    SetTranslateOcrEngine(ViewerOcrEngine),
    SetTranslateSourceLang(String),
    SetTranslateTargetLang(String),
    SwapTranslateLangs,
    StartTranslateRegionPick,
    ScanTranslatePage,
    ToggleTranslateBubbles,
    ToggleTranslateBubbleStyle,
    ToggleTranslateBlockOriginal(usize),
    SetTranslateFontScale(f32),
    SelectTranslateBlock(Option<usize>),
    UpdateTranslateBlockOriginal(usize, String),
    UpdateTranslateBlockText(usize, String),
    MoveTranslateBlock(usize, i32, i32),
    RetryTranslateBlock(usize),
    RemoveTranslateBlock(usize),
    ClearTranslateBlocks,
    SaveTranslationSidecar,
    DeleteTranslationSidecar,
    CopyTranslatedText,
    OpenTranslateSettings,
    OpenPluginSettings,
    BeginDragTranslateBubble(usize),
    BeginDragTranslateTopBar,
    BeginDragTranslateResultCard,
    ResetTranslateTopBarPos,
    ResetTranslateResultCardPos,
    DismissTranslateResultCard,
    NextTranslateBubble,
    ToggleTranslateResultCardOriginal(bool),
    BeginDragTranslatePageBubble(String, usize),
    SelectTranslatePageBlock(String, usize),
    ToggleTranslatePageBlockOriginal(String, usize),
    RetryTranslatePageBlock(String, usize),
    RemoveTranslatePageBlock(String, usize),
    BeginWebtoonTranslateRegion {
        path: String,
        disp_w: f32,
        disp_h: f32,
    },
    WebtoonHoverPage {
        path: String,
        local_pos: [f32; 2],
        disp_w: f32,
        disp_h: f32,
    },
}

const VIEWER_RIGHT_MIN_W: f32 = 200.0;
const VIEWER_RIGHT_MAX_W: f32 = 600.0;
const VIEWER_RIGHT_DEFAULT_W: f32 = 290.0;
const VIEWER_SPLIT_DEFAULT_PCT: f32 = 58.0;
const VIEWER_SPLIT_MIN_PCT: f32 = 20.0;
const VIEWER_SPLIT_MAX_PCT: f32 = 80.0;

fn viewer_right_width(state: ViewerViewState<'_>) -> f32 {
    state
        .viewer_right_width_preview
        .unwrap_or(state.snapshot.meta.layout.right_width as f32)
        .clamp(VIEWER_RIGHT_MIN_W, VIEWER_RIGHT_MAX_W)
}

fn viewer_panel_is_open(state: ViewerViewState<'_>, panel: ViewerRightPanel) -> bool {
    match panel {
        ViewerRightPanel::ViewAdjustment => {
            state.viewer_adjustments_open && state.viewer_mode != ViewerViewMode::Webtoon
        }
        ViewerRightPanel::Info => state.viewer_info_open,
        ViewerRightPanel::Translate => state.translate.active,
    }
}

fn format_viewer_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Image position counter shown in the Viewer status bar (`1 / 5` or `2·3 / 5`).
pub fn viewer_counter_summary(state: ViewerViewState<'_>) -> Option<String> {
    let meta = &state.snapshot.meta;
    let total = meta.viewer_count;
    if total == 0 {
        return None;
    }
    match state.viewer_mode {
        ViewerViewMode::Webtoon => {
            let active_idx = state
                .webtoon_visible_index
                .or(meta.viewer_index)
                .unwrap_or(0)
                .min(total - 1);
            Some(format!("{} / {total}", active_idx + 1))
        }
        ViewerViewMode::Double => {
            let (left_slot, right_slot) = dual_spread_slots(state);
            match (left_slot, right_slot) {
                (Some((li, _)), Some((ri, _))) => Some(format!("{}·{} / {total}", li + 1, ri + 1)),
                (Some((li, _)), None) => Some(format!("{} / {total}", li + 1)),
                _ => meta.viewer_index.map(|i| format!("{} / {total}", i + 1)),
            }
        }
        ViewerViewMode::Single => {
            let idx = meta.viewer_index.unwrap_or(0).min(total - 1);
            Some(format!("{} / {total}", idx + 1))
        }
    }
}

/// Summary text shown on the left side of the bottom status bar while in Viewer workspace.
pub fn viewer_status_summary(state: ViewerViewState<'_>, lang: Language) -> String {
    let meta = &state.snapshot.meta;
    let viewer_mode = state.viewer_mode;
    let idx = meta.viewer_index.unwrap_or(0);
    let total = meta.viewer_count;
    let name_at = |i: usize| -> Option<String> {
        if let Some(arch) = &meta.viewer_archive
            && let Some(label) = arch.page_labels.get(i)
        {
            return Some(label.clone());
        }
        state
            .snapshot
            .viewer_images
            .get(i)
            .and_then(|p| Path::new(p).file_name())
            .and_then(|n| n.to_str())
            .map(ToOwned::to_owned)
    };
    match viewer_mode {
        ViewerViewMode::Webtoon => {
            let active_idx = state
                .webtoon_visible_index
                .or(meta.viewer_index)
                .unwrap_or(0)
                .min(total.saturating_sub(1));
            let mode_name = tr(lang, "action-viewer-manhwa");
            if let Some(name) = name_at(active_idx) {
                if let Some((w, h)) = state
                    .snapshot
                    .viewer_images
                    .get(active_idx)
                    .and_then(|p| state.image_sizes.get(p))
                {
                    format!("{mode_name} · {name} · {w}×{h}")
                } else {
                    format!("{mode_name} · {name}")
                }
            } else {
                mode_name
            }
        }
        ViewerViewMode::Double => {
            let (left_slot, right_slot) = dual_spread_slots(state);
            let dims_str = |path: &str, frame: Option<&CpuImageFrame>| -> Option<String> {
                if let Some(f) = frame {
                    Some(format!("{}×{}", f.width, f.height))
                } else {
                    state.image_sizes.get(path).map(|(w, h)| format!("{w}×{h}"))
                }
            };
            match (left_slot, right_slot) {
                (Some((_, lp)), Some((_, rp))) => {
                    let lname = left_slot
                        .and_then(|(li, _)| name_at(li))
                        .unwrap_or_default();
                    let rname = right_slot
                        .and_then(|(ri, _)| name_at(ri))
                        .unwrap_or_default();
                    let ldims = dims_str(lp, state.viewer_frame)
                        .map(|d| format!(" ({d})"))
                        .unwrap_or_default();
                    let rdims = dims_str(rp, state.viewer_pair_frame)
                        .map(|d| format!(" ({d})"))
                        .unwrap_or_default();
                    format!("{lname}{ldims}  |  {rname}{rdims}")
                }
                (Some((li, lp)), None) => {
                    let lname = name_at(li).unwrap_or_default();
                    if let Some(d) = dims_str(lp, state.viewer_frame) {
                        format!("{lname} · {d}")
                    } else {
                        lname
                    }
                }
                _ => tr(lang, "ready"),
            }
        }
        ViewerViewMode::Single => match (name_at(idx), &state.viewer_frame) {
            (Some(name), Some(frame)) => format!("{name} · {}×{}", frame.width, frame.height),
            (Some(name), None) => name,
            _ => tr(lang, "ready"),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerToolbarSwitcherKind {
    MainThreeTab(WorkspaceSwitcherMode),
    StandaloneTwoButton { show_labels: bool },
}

pub fn viewer_toolbar_switcher_kind(
    _is_main_window: bool,
    window_width: f32,
) -> ViewerToolbarSwitcherKind {
    ViewerToolbarSwitcherKind::StandaloneTwoButton {
        show_labels: window_width >= 1120.0,
    }
}

/// Width below which the four Viewer utility toggles would collapse.
/// Kept for API compatibility; utility toggles stay expanded on the toolbar.
pub const VIEWER_UTILITY_COLLAPSE_WIDTH: f32 = 720.0;

pub fn viewer_utility_collapsed(_window_width: f32) -> bool {
    false
}

/// Right-edge offset (from the window's right edge) of the collapsed utility
/// button, used to anchor its dropdown menu underneath it.
pub fn viewer_utilities_menu_right_offset(window_width: f32) -> f32 {
    let utilities_w = 12.0 + 32.0; // right padding + single Panels button
    let switcher_w = match viewer_toolbar_switcher_kind(false, window_width) {
        ViewerToolbarSwitcherKind::MainThreeTab(WorkspaceSwitcherMode::Full) => 252.0,
        ViewerToolbarSwitcherKind::MainThreeTab(WorkspaceSwitcherMode::CompactIcons) => 112.0,
        ViewerToolbarSwitcherKind::MainThreeTab(WorkspaceSwitcherMode::Dropdown) => 56.0,
        ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: true } => 164.0,
        ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: false } => 72.0,
    };
    let view_mode_w = if window_width >= 1120.0 {
        138.0 // Single / Double / Manhwa + Reader Options
    } else {
        36.0 // Reader Options only
    };
    // utilities + spacing + divider + spacing + switcher + spacing + view mode + spacing
    utilities_w + 17.0 + switcher_w + 8.0 + view_mode_w + 8.0
}

pub fn viewer_reader_options_right_offset(state: ViewerViewState<'_>) -> f32 {
    let utilities_w = 12.0 + 32.0; // right padding + single Panels&Settings button
    let switcher_w =
        match viewer_toolbar_switcher_kind(state.is_main_window, state.window_size.width) {
            ViewerToolbarSwitcherKind::MainThreeTab(WorkspaceSwitcherMode::Full) => 252.0,
            ViewerToolbarSwitcherKind::MainThreeTab(WorkspaceSwitcherMode::CompactIcons) => 112.0,
            ViewerToolbarSwitcherKind::MainThreeTab(WorkspaceSwitcherMode::Dropdown) => 56.0,
            ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: true } => 164.0,
            ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: false } => 72.0,
        };
    // utilities_w + spacing(8) + divider(9) + switcher_w + spacing(8)
    utilities_w + 17.0 + switcher_w + 8.0
}

/// Top toolbar for the Viewer workspace:
/// Left: Navigation + Zoom/Fit/1:1 + Rotate + Fullscreen.
/// Right: View Mode Switcher (`Single`/`Double`/`Manhwa`) + Reader Options + Workspace Switcher + Panels & Settings.
pub fn build_viewer_toolbar<'a>(
    state: ViewerViewState<'a>,
    right_utilities: Element<'a, ViewerMessage>,
) -> Element<'a, ViewerMessage> {
    let lang = state.snapshot.meta.language;
    let meta = &state.snapshot.meta;
    let has_nav = meta.viewer_count > 1;
    let has_image = meta.viewer_path.is_some();
    let viewer_mode = state.viewer_mode;
    let is_webtoon = viewer_mode == ViewerViewMode::Webtoon;
    let is_double = viewer_mode == ViewerViewMode::Double;

    // Navigation (Prev / Next)
    let prev_msg = has_nav.then_some({
        if is_double {
            ViewerMessage::DualStepSpread(-1)
        } else {
            ViewerMessage::Dispatch(Command::ViewerPrevious)
        }
    });
    let next_msg = has_nav.then_some({
        if is_double {
            ViewerMessage::DualStepSpread(1)
        } else {
            ViewerMessage::Dispatch(Command::ViewerNext)
        }
    });

    let nav_group = toolbar_button_group(vec![
        button::toolbar_icon(
            Icon::Prev,
            false,
            prev_msg,
            Some(tr(lang, "tooltip-viewer-prev")),
        ),
        button::toolbar_icon(
            Icon::Next,
            false,
            next_msg,
            Some(tr(lang, "tooltip-viewer-next")),
        ),
    ]);

    let mut actual_btn = iced_button(
        container(text(tr(lang, "action-actual-ratio")).size(TEXT_SIZE_CONTROL))
            .height(Length::Fill)
            .align_y(Alignment::Center),
    )
    .height(Length::Fixed(TOOLBAR_CONTROL_HEIGHT))
    .padding([0, 6])
    .style(button::style_for(ButtonVariant::Ghost));
    if let Some(msg) = (has_image && !is_webtoon).then_some(ViewerMessage::ViewerZoomActual) {
        actual_btn = actual_btn.on_press(msg);
    }
    let actual_ratio_btn = with_tooltip_pos(
        actual_btn,
        tr(lang, "tooltip-viewer-actual-size"),
        TooltipPosition::Bottom,
    );

    let zoom_group = toolbar_button_group(vec![
        button::toolbar_icon(
            Icon::ZoomOut,
            false,
            has_image.then_some(ViewerMessage::ZoomStep(0.8)),
            Some(tr(lang, "tooltip-viewer-zoom-out")),
        ),
        button::toolbar_icon(
            Icon::ZoomIn,
            false,
            has_image.then_some(ViewerMessage::ZoomStep(1.25)),
            Some(tr(lang, "tooltip-viewer-zoom-in")),
        ),
        button::toolbar_icon(
            Icon::Fit,
            false,
            has_image.then_some(ViewerMessage::Canvas(CanvasEvent::ResetFit)),
            Some(tr(lang, "tooltip-viewer-fit")),
        ),
        actual_ratio_btn,
    ]);

    let mut left = row![
        button::toolbar_icon(
            Icon::FolderOpen,
            false,
            Some(ViewerMessage::PickImagesDialog),
            Some(tr(lang, "action-open-file-dialog")),
        ),
        nav_group,
        zoom_group,
    ]
    .spacing(3)
    .align_y(Alignment::Center);

    if !is_webtoon {
        let rotate_group = toolbar_button_group(vec![
            button::toolbar_icon(
                Icon::RotateLeft,
                false,
                has_image.then_some(ViewerMessage::ViewerRotatePreview(-1)),
                Some(tr(lang, "tooltip-viewer-rotate-left")),
            ),
            button::toolbar_icon(
                Icon::RotateRight,
                false,
                has_image.then_some(ViewerMessage::ViewerRotatePreview(1)),
                Some(tr(lang, "tooltip-viewer-rotate-right")),
            ),
        ]);
        left = left.push(rotate_group);
    }

    // Right side: View Mode Switcher (Single | Double | Webtoon | Reader Options) + Workspace Switcher + Panels & Settings
    let reader_options_btn = button::toolbar_icon(
        Icon::BookOpen,
        state.reader_options_open,
        Some(ViewerMessage::ToggleReaderOptions),
        Some(tr(lang, "action-reader-options")),
    );
    let view_mode_group = if state.window_size.width >= 1120.0 {
        toolbar_button_group(vec![
            button::toolbar_icon(
                Icon::ViewSingle,
                viewer_mode == ViewerViewMode::Single,
                Some(ViewerMessage::SetViewerViewMode(ViewerViewMode::Single)),
                Some(format!("{} (S)", tr(lang, "action-viewer-single"))),
            ),
            button::toolbar_icon(
                Icon::ViewDouble,
                viewer_mode == ViewerViewMode::Double,
                Some(ViewerMessage::SetViewerViewMode(ViewerViewMode::Double)),
                Some(format!("{} (D)", tr(lang, "action-viewer-double"))),
            ),
            button::toolbar_icon(
                Icon::ViewManhwa,
                viewer_mode == ViewerViewMode::Webtoon,
                Some(ViewerMessage::SetViewerViewMode(ViewerViewMode::Webtoon)),
                Some(format!("{} (M)", tr(lang, "action-viewer-manhwa"))),
            ),
            reader_options_btn,
        ])
    } else {
        toolbar_button_group(vec![reader_options_btn])
    };

    let switcher = match viewer_toolbar_switcher_kind(state.is_main_window, state.window_size.width)
    {
        ViewerToolbarSwitcherKind::MainThreeTab(mode) => workspace_switcher_responsive(
            vec![
                WorkspaceTab::new(
                    tr(lang, "workspace-browser"),
                    Icon::Browser,
                    false,
                    ViewerMessage::Dispatch(Command::SetWorkspace {
                        workspace: Workspace::Browser,
                    }),
                )
                .tooltip(tr(lang, "tooltip-workspace-browser")),
                WorkspaceTab::new(
                    tr(lang, "workspace-viewer"),
                    Icon::Viewer,
                    true,
                    ViewerMessage::Dispatch(Command::SetWorkspace {
                        workspace: Workspace::Viewer,
                    }),
                )
                .tooltip(tr(lang, "tooltip-workspace-viewer")),
                WorkspaceTab::new(
                    tr(lang, "workspace-editor"),
                    Icon::Editor,
                    false,
                    ViewerMessage::Dispatch(Command::SetWorkspace {
                        workspace: Workspace::Editor,
                    }),
                )
                .tooltip(tr(lang, "tooltip-workspace-editor")),
            ],
            mode,
            state.workspace_switcher_open,
            Some(ViewerMessage::ToggleWorkspaceSwitcher),
        ),
        ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels } => {
            let browser_msg = Some(ViewerMessage::OpenBrowserWindow);
            let editor_msg = has_image.then_some(ViewerMessage::ViewerAdjOpenInEditor);
            let browser_tip = Some(tr(lang, "tooltip-workspace-browser"));
            let editor_tip = Some(tr(lang, "tooltip-workspace-editor"));
            if show_labels {
                toolbar_button_group(vec![
                    button::toolbar_labeled(
                        tr(lang, "workspace-browser"),
                        Some(Icon::Browser),
                        ButtonVariant::Ghost,
                        browser_msg,
                        browser_tip,
                    ),
                    button::toolbar_labeled(
                        tr(lang, "workspace-editor"),
                        Some(Icon::Editor),
                        ButtonVariant::Ghost,
                        editor_msg,
                        editor_tip,
                    ),
                ])
            } else {
                toolbar_button_group(vec![
                    button::toolbar_icon(Icon::Browser, false, browser_msg, browser_tip),
                    button::toolbar_icon(Icon::Editor, false, editor_msg, editor_tip),
                ])
            }
        }
    };
    // Utility toggles: Filmstrip → Fullscreen → View Adjustments → Info (always expanded).
    let utility_group = toolbar_button_group(vec![
        button::toolbar_icon(
            Icon::Filmstrip,
            state.viewer_filmstrip_open,
            Some(ViewerMessage::ToggleFilmstrip),
            Some(tr(lang, "tooltip-viewer-filmstrip")),
        ),
        button::toolbar_icon(
            if state.is_fullscreen {
                Icon::FullscreenExit
            } else {
                Icon::Fullscreen
            },
            state.is_fullscreen,
            Some(ViewerMessage::ToggleFullscreen),
            Some(tr(lang, "tooltip-viewer-fullscreen")),
        ),
        button::toolbar_icon(
            Icon::Palette,
            state.viewer_adjustments_open,
            Some(ViewerMessage::ToggleViewerColorPopover),
            Some(tr(lang, "panel-view-adjustments")),
        ),
        button::toolbar_icon(
            Icon::Info,
            state.viewer_info_open,
            Some(ViewerMessage::ToggleViewerInspector),
            Some(tr(lang, "action-info")),
        ),
    ]);

    let right: Element<'a, ViewerMessage> = row![
        utility_group,
        view_mode_group,
        switcher,
        container(rule::vertical(1).style(hairline_rule_style)).height(Length::Fixed(18.0)),
        right_utilities
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into();

    toolbar(left, right)
}

/// Main Viewer workspace view: Single/Double/Manhwa viewport,
/// unified resizable/reorderable right sidebar (`ViewAdjustment` & `Info`), and bottom Filmstrip bar.
pub fn build_viewer_view<'a>(state: ViewerViewState<'a>) -> Element<'a, ViewerMessage> {
    let lang = state.snapshot.meta.language;
    let dark = state.snapshot.meta.theme == Theme::Dark;
    let viewer_mode = state.viewer_mode;

    let canvas_tool = if matches!(viewer_mode, ViewerViewMode::Single | ViewerViewMode::Double)
        && state.translate.active
        && (state.translate.picking_region
            || matches!(
                state.translate.mode,
                ViewerTranslateMode::Translate | ViewerTranslateMode::OcrOnly
            )) {
        CanvasTool::MarqueeRect
    } else if state.translate.active && state.translate.mode == ViewerTranslateMode::Pan {
        CanvasTool::Hand
    } else {
        CanvasTool::Pointer
    };

    let viewport: Element<'_, ViewerMessage> = match viewer_mode {
        ViewerViewMode::Single => {
            let canvas: Element<'_, ViewerMessage> = Element::from(
                Shader::new(ImageCanvasProgram {
                    frame: state.viewer_frame.cloned(),
                    pair_frame: None,
                    dual_gap_px: 0.0,
                    compare_frame: None,
                    zoom: state.zoom,
                    pan_x: state.pan_x,
                    pan_y: state.pan_y,
                    adjustments: state.viewer_adj,
                    dark_theme: dark,
                    crop_rect: None,
                    crop_aspect: None,
                    quarter_turns: state.viewer_quarter_turns,
                    tool: canvas_tool,
                    preview_angle_rad: 0.0,
                    preview_fill: [0.0; 4],
                    canvas_bg: None,
                    viewport_bg: state.viewport_background,
                    compare_mode: 0,
                    compare_split_x: 0.5,
                    brush_radius_px: 0.0,
                    selection_mask: None,
                    has_selection: false,
                    quick_mask_active: false,
                    marching_ants_phase: 0.0,
                    restrict_pan_to_hand: false,
                    stabilizer_radius: 0.0,
                    aa_mode: state.aa_mode,
                    denoise_mode: state.denoise_mode,
                    ..Default::default()
                })
                .width(Fill)
                .height(Fill),
            )
            .map(|ev| ViewerMessage::ViewerCanvas(None, ev));

            container(canvas)
                .width(Fill)
                .height(Fill)
                .style(canvas_backdrop_style)
                .into()
        }
        ViewerViewMode::Double => build_viewer_double_panes(state, lang, dark, canvas_tool),
        ViewerViewMode::Webtoon => build_viewer_webtoon_strip(state),
    };

    let viewport: Element<'_, ViewerMessage> = if state.translate.active {
        build_viewer_translate_overlay(viewport, state, lang)
    } else {
        viewport
    };

    let edge_nav_enabled = matches!(viewer_mode, ViewerViewMode::Single | ViewerViewMode::Double)
        && state.snapshot.viewer_images.len() > 1
        && !state.translate.space_pan_held
        && !(state.translate.active
            && (state.translate.picking_region
                || matches!(
                    state.translate.mode,
                    ViewerTranslateMode::Pan
                        | ViewerTranslateMode::Translate
                        | ViewerTranslateMode::OcrOnly
                )));

    let viewport: Element<'_, ViewerMessage> = if edge_nav_enabled {
        let (left_msg, right_msg) = match viewer_mode {
            ViewerViewMode::Double => {
                if state.viewer_manga_rtl {
                    (
                        ViewerMessage::DualStepSpread(1),
                        ViewerMessage::DualStepSpread(-1),
                    )
                } else {
                    (
                        ViewerMessage::DualStepSpread(-1),
                        ViewerMessage::DualStepSpread(1),
                    )
                }
            }
            _ => (
                ViewerMessage::Dispatch(Command::ViewerPrevious),
                ViewerMessage::Dispatch(Command::ViewerNext),
            ),
        };
        let edge_btn = |icon: Icon, msg: ViewerMessage| -> Element<'_, ViewerMessage> {
            iced_button(
                container(photon_ui::icons::render(icon, 26.0, None))
                    .width(Fill)
                    .height(Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .width(Length::Fixed(56.0))
            .height(Fill)
            .padding(0)
            .on_press(msg)
            .style(|theme, status| {
                let pal = PaletteTokens::for_theme(theme);
                match status {
                    iced_button::Status::Hovered | iced_button::Status::Pressed => {
                        iced_button::Style {
                            background: Some(
                                pal.bg_elevated
                                    .scale_alpha(
                                        if matches!(status, iced_button::Status::Pressed) {
                                            0.7
                                        } else {
                                            0.45
                                        },
                                    )
                                    .into(),
                            ),
                            text_color: pal.text_primary,
                            border: Border::default(),
                            ..Default::default()
                        }
                    }
                    _ => iced_button::Style {
                        background: None,
                        text_color: iced::Color::TRANSPARENT,
                        border: Border::default(),
                        ..Default::default()
                    },
                }
            })
            .into()
        };
        let edge_overlay = row![
            edge_btn(Icon::Prev, left_msg),
            iced::widget::space::horizontal().width(Fill),
            edge_btn(Icon::Next, right_msg),
        ]
        .width(Fill)
        .height(Fill);
        stack![viewport, edge_overlay]
            .width(Fill)
            .height(Fill)
            .into()
    } else {
        viewport
    };

    let open_panels: Vec<ViewerRightPanel> = state
        .viewer_right_panels
        .order()
        .iter()
        .copied()
        .filter(|&p| viewer_panel_is_open(state, p))
        .collect();

    let main_area: Element<'_, ViewerMessage> = if !open_panels.is_empty() {
        let right_w = viewer_right_width(state);
        let splitter = panel_splitter(
            right_w,
            PanelSplitterEdge::Right.direction(),
            VIEWER_RIGHT_MIN_W,
            VIEWER_RIGHT_MAX_W,
            VIEWER_RIGHT_DEFAULT_W,
            ViewerMessage::ViewerSplitterDrag,
            ViewerMessage::ViewerSplitterRelease,
        );

        let right_dock = build_viewer_right_sidebar(state, lang, right_w, &open_panels);

        row![
            viewport,
            splitter,
            rule::vertical(1).style(hairline_rule_style),
            right_dock,
        ]
        .height(Fill)
        .width(Fill)
        .into()
    } else {
        viewport
    };

    if viewer_mode != ViewerViewMode::Webtoon
        && state.snapshot.viewer_images.len() > 1
        && state.viewer_filmstrip_open
    {
        let active_path = state.snapshot.meta.viewer_path.as_deref();
        let items: Vec<FilmstripItem<ViewerMessage>> = state
            .snapshot
            .viewer_images
            .iter()
            .map(|path| FilmstripItem {
                thumbnail: state.thumbnails.get(path).cloned(),
                active: active_path == Some(path.as_str()),
                on_press: ViewerMessage::Dispatch(Command::ViewerOpen { path: path.clone() }),
            })
            .collect();
        let close_btn = with_tooltip_pos(
            button::compact_icon(
                Icon::Close,
                ButtonVariant::Ghost,
                Some(ViewerMessage::CloseFilmstrip),
            ),
            tr(lang, "tooltip-panel-close"),
            TooltipPosition::Left,
        );
        let archive_label = state.snapshot.meta.viewer_archive.as_ref().map(|arch| {
            let cur = state
                .snapshot
                .meta
                .viewer_index
                .unwrap_or(0)
                .min(arch.page_count.saturating_sub(1));
            let page_lbl = arch
                .page_labels
                .get(cur)
                .cloned()
                .unwrap_or_else(|| format!("Page {}", cur + 1));
            let kind = if arch.kind == "pdf" { "PDF" } else { "CBZ" };
            format!("{kind} · {} / {} · {page_lbl}", cur + 1, arch.page_count)
        });
        let mut filmstrip_col = column![].width(Fill);
        if let Some(lbl) = archive_label {
            filmstrip_col = filmstrip_col.push(
                container(
                    text(lbl)
                        .size(TEXT_SIZE_CAPTION)
                        .style(|theme| text::Style {
                            color: Some(PaletteTokens::for_theme(theme).text_muted),
                        }),
                )
                .padding([4, 10]),
            );
        }
        let on_scroll: Option<Box<dyn Fn(scrollable::Viewport) -> ViewerMessage>> =
            Some(Box::new(move |viewport| ViewerMessage::FilmstripScrolled {
                offset: viewport.absolute_offset().x,
            }));
        let on_wheel: Option<Box<dyn Fn(iced::mouse::ScrollDelta) -> ViewerMessage>> =
            Some(Box::new(ViewerMessage::FilmstripWheelScroll));
        filmstrip_col = filmstrip_col.push(
            row![
                container(filmstrip_bar(
                    items,
                    filmstrip_scroll_id(),
                    on_scroll,
                    on_wheel
                ))
                .width(Fill),
                container(close_btn).padding([4, 6]),
            ]
            .align_y(Alignment::Center)
            .width(Fill),
        );
        let filmstrip_row = container(filmstrip_col)
            .width(Fill)
            .style(surface_panel_style);
        column![
            main_area,
            rule::horizontal(1).style(hairline_rule_style),
            filmstrip_row,
        ]
        .height(Fill)
        .width(Fill)
        .into()
    } else {
        main_area
    }
}

fn build_viewer_right_sidebar<'a>(
    state: ViewerViewState<'a>,
    lang: Language,
    right_w: f32,
    open_panels: &[ViewerRightPanel],
) -> Element<'a, ViewerMessage> {
    let dragging_any = state.viewer_right_panels.is_dragging();
    let is_two = open_panels.len() == 2;
    let is_multi = open_panels.len() >= 2;
    let split_pct = state
        .viewer_right_split_preview
        .unwrap_or(state.snapshot.meta.layout.viewer_right_split_pct as f32);
    let (top_h, bottom_h) = stacked_panel_heights(
        false,
        false,
        split_pct,
        VIEWER_SPLIT_MIN_PCT,
        VIEWER_SPLIT_MAX_PCT,
    );

    let mut stacked = column![].spacing(6).height(Fill).width(Fill);
    for (idx, &panel_kind) in open_panels.iter().enumerate() {
        let (title, icon, body) = match panel_kind {
            ViewerRightPanel::ViewAdjustment => (
                tr(lang, "panel-view-adjustments"),
                Icon::Palette,
                build_viewer_adjustments_body(state, lang, right_w),
            ),
            ViewerRightPanel::Info => (
                tr(lang, "panel-info"),
                Icon::Info,
                build_viewer_info_body(state, lang),
            ),
            ViewerRightPanel::Translate => (
                tr(lang, "panel-translate"),
                Icon::Translate,
                build_viewer_translate_panel_body(state, lang),
            ),
        };

        let header = DraggablePanelHeaderSpec::new(title)
            .icon(icon)
            .collapsed(false)
            .dragging(state.viewer_right_panels.is_dragging_panel(panel_kind))
            .drag_tooltip(tr(lang, "tooltip-sidebar-panel-drag"))
            .collapse_tooltip(tr(lang, "tooltip-panel-collapse"))
            .close_tooltip(tr(lang, "tooltip-panel-close"))
            .on_drag_start(ViewerMessage::BeginDragViewerPanel(panel_kind))
            .on_drag_hover(dragging_any.then_some(ViewerMessage::HoverViewerPanelDrop(panel_kind)))
            .on_drag_end(dragging_any.then_some(ViewerMessage::DropViewerPanel(panel_kind)))
            .on_toggle_collapse(ViewerMessage::ToggleViewerPanelCollapse(panel_kind))
            .on_close(Some(ViewerMessage::CloseViewerPanel(panel_kind)));

        let panel_el = sidebar_panel(
            PanelFrameSpec::new(header)
                .body(body)
                .fill_height(true)
                .body_padding(Padding::from([2, 2]))
                .drop_indicator(state.viewer_right_panels.drop_indicator(panel_kind))
                .on_drop_hover(
                    dragging_any.then_some(ViewerMessage::HoverViewerPanelDrop(panel_kind)),
                )
                .on_drop_release(
                    dragging_any.then_some(ViewerMessage::DropViewerPanel(panel_kind)),
                ),
        );

        let slot_height = if is_two {
            if idx == 0 { top_h } else { bottom_h }
        } else if is_multi {
            Length::FillPortion(1)
        } else {
            Length::Fill
        };

        stacked = stacked.push(
            container(panel_el)
                .width(Fill)
                .height(slot_height)
                .clip(true),
        );

        if idx == 0 && is_two {
            let height_hint = (state.window_size.height - 120.0).max(260.0);
            stacked = stacked.push(vertical_panel_splitter(
                split_pct,
                height_hint,
                VIEWER_SPLIT_MIN_PCT,
                VIEWER_SPLIT_MAX_PCT,
                VIEWER_SPLIT_DEFAULT_PCT,
                ViewerMessage::ViewerRightSplitDrag,
                ViewerMessage::ViewerRightSplitRelease,
            ));
        }
    }

    sidebar_dock(right_w, stacked)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TranslateProviderOption(TranslateProvider);

impl TranslateProviderOption {
    const ALL: [Self; 3] = [
        Self(TranslateProvider::None),
        Self(TranslateProvider::GoogleWeb),
        Self(TranslateProvider::LibreTranslate),
    ];
}

impl std::fmt::Display for TranslateProviderOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            TranslateProvider::None => f.write_str("Off (OCR only)"),
            TranslateProvider::GoogleWeb => f.write_str("Google Web"),
            TranslateProvider::LibreTranslate => f.write_str("LibreTranslate"),
        }
    }
}

const COMMON_OCR_LANGS: [&str; 8] = [
    "eng", "jpn", "jpn_vert", "vie", "chi_sim", "chi_tra", "kor", "fra",
];
const COMMON_TARGET_LANGS: [&str; 8] = ["vi", "en", "ja", "ko", "zh-CN", "zh-TW", "fr", "de"];

fn styled_pick_list<'a, T>(
    options: &'a [T],
    selected: Option<T>,
    on_selected: impl Fn(T) -> ViewerMessage + 'a,
) -> pick_list::PickList<'a, T, &'a [T], T, ViewerMessage>
where
    T: ToString + PartialEq + Clone + 'a,
{
    pick_list(options, selected, on_selected)
        .text_size(TEXT_SIZE_CONTROL)
        .padding([5, 8])
        .style(|theme, status| {
            let t = PaletteTokens::for_theme(theme);
            let active = matches!(
                status,
                pick_list::Status::Hovered | pick_list::Status::Opened { .. }
            );
            pick_list::Style {
                text_color: t.text_primary,
                placeholder_color: t.text_muted,
                handle_color: t.text_secondary,
                background: if active { t.bg_hover } else { t.bg_surface }.into(),
                border: Border {
                    color: if active { t.accent } else { t.border_strong },
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
            }
        })
        .menu_style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            iced_menu::Style {
                background: t.bg_elevated.into(),
                border: Border {
                    color: t.border_strong,
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                text_color: t.text_primary,
                selected_text_color: Color::WHITE,
                selected_background: t.accent.into(),
                shadow: Shadow::default(),
            }
        })
}

fn lang_combo_field<'a>(
    placeholder: &str,
    value: &str,
    presets: &'static [&'static str],
    enabled: bool,
    on_input: impl Fn(String) -> ViewerMessage + Clone + 'a,
) -> Element<'a, ViewerMessage> {
    if !enabled {
        return text_input(placeholder, value)
            .size(TEXT_SIZE_CAPTION)
            .padding([5, 8])
            .width(Length::FillPortion(1))
            .style(|theme, _status| {
                let pal = PaletteTokens::for_theme(theme);
                iced::widget::text_input::Style {
                    background: pal.bg_app.into(),
                    border: Border {
                        color: pal.border,
                        width: 1.0,
                        radius: RADIUS_SM.into(),
                    },
                    icon: pal.text_muted,
                    placeholder: pal.text_muted,
                    value: pal.text_muted,
                    selection: pal.bg_selected,
                }
            })
            .into();
    }

    let on_pick = on_input.clone();
    let input = text_input(placeholder, value)
        .on_input(on_input)
        .size(TEXT_SIZE_CAPTION)
        .padding([5, 8])
        .style(input_style)
        .width(Fill);

    let quick_pick = styled_pick_list(presets, None::<&'static str>, move |chosen| {
        on_pick(chosen.to_owned())
    })
    .placeholder("")
    .width(Length::Fixed(24.0))
    .padding([5, 4]);

    row![input, quick_pick]
        .spacing(2)
        .align_y(Alignment::Center)
        .width(Length::FillPortion(1))
        .into()
}

fn build_viewer_translate_panel_body<'a>(
    state: ViewerViewState<'a>,
    lang: Language,
) -> Element<'a, ViewerMessage> {
    let tr_state = state.translate;
    let has_path = state.snapshot.meta.viewer_path.is_some();
    let has_blocks = !tr_state.blocks.is_empty();
    let running_stage = state
        .snapshot
        .meta
        .ai_task
        .as_ref()
        .filter(|t| t.running)
        .map(|t| (t.percent, t.stage.clone()));

    let is_ocr_only = tr_state.mode == ViewerTranslateMode::OcrOnly;
    let is_pan_mode = tr_state.mode == ViewerTranslateMode::Pan;

    // 1. Mode row: Translate / OCR
    let mode_row = container(
        row![
            container(
                button::labeled(
                    tr(lang, "translate-mode-translate"),
                    None,
                    ButtonVariant::Tab {
                        active: !is_ocr_only,
                    },
                    Some(ViewerMessage::SetTranslateMode(
                        ViewerTranslateMode::Translate,
                    )),
                )
                .width(Fill),
            )
            .width(Length::FillPortion(1)),
            container(
                button::labeled(
                    tr(lang, "translate-mode-ocr"),
                    None,
                    ButtonVariant::Tab {
                        active: is_ocr_only,
                    },
                    Some(ViewerMessage::SetTranslateMode(
                        ViewerTranslateMode::OcrOnly,
                    )),
                )
                .width(Fill),
            )
            .width(Length::FillPortion(1)),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .width(Fill),
    )
    .padding(3)
    .width(Fill)
    .style(|theme| {
        let pal = PaletteTokens::for_theme(theme);
        container::Style {
            background: Some(pal.bg_app.into()),
            border: Border {
                color: pal.border,
                width: 1.0,
                radius: RADIUS_SM.into(),
            },
            ..Default::default()
        }
    });

    // 2. OCR Model dropdown + Translate Provider dropdown (dimmed when OCR Only)
    let current_provider = TranslateProviderOption(state.snapshot.meta.translate_settings.provider);
    let provider_picker: Element<'a, ViewerMessage> = if is_ocr_only {
        container(
            row![
                text(current_provider.to_string())
                    .size(TEXT_SIZE_CONTROL)
                    .style(|theme| text::Style {
                        color: Some(PaletteTokens::for_theme(theme).text_muted),
                    })
                    .width(Fill),
                text("▾")
                    .size(TEXT_SIZE_CAPTION)
                    .style(|theme| text::Style {
                        color: Some(PaletteTokens::for_theme(theme).text_muted),
                    }),
            ]
            .align_y(Alignment::Center),
        )
        .padding([5, 8])
        .width(Length::FillPortion(1))
        .style(|theme| {
            let pal = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(pal.bg_app.into()),
                border: Border {
                    color: pal.border,
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                ..Default::default()
            }
        })
        .into()
    } else {
        styled_pick_list(
            &TranslateProviderOption::ALL[..],
            Some(current_provider),
            |opt| ViewerMessage::SetTranslateProvider(opt.0),
        )
        .width(Length::FillPortion(1))
        .into()
    };

    let engine_picker = styled_pick_list(
        &ViewerOcrEngine::ALL[..],
        Some(tr_state.ocr_engine),
        ViewerMessage::SetTranslateOcrEngine,
    )
    .width(Length::FillPortion(1));

    let model_provider_row = row![engine_picker, provider_picker]
        .spacing(6)
        .align_y(Alignment::Center)
        .width(Fill);

    // 3. Languages row (combo-style dropdown + free input, smartly dimmed)
    let src_enabled = match (is_ocr_only, tr_state.ocr_engine) {
        (_, ViewerOcrEngine::MangaOcr) => false,
        (true, ViewerOcrEngine::RapidOcr) => false,
        _ => true,
    };
    let dst_enabled = !is_ocr_only;
    let src_display = if tr_state.ocr_engine == ViewerOcrEngine::MangaOcr {
        "jpn"
    } else {
        tr_state.source_lang.as_str()
    };

    let lang_row = row![
        lang_combo_field(
            "eng",
            src_display,
            &COMMON_OCR_LANGS,
            src_enabled,
            ViewerMessage::SetTranslateSourceLang,
        ),
        button::compact_icon(
            Icon::Swap,
            ButtonVariant::Ghost,
            (src_enabled && dst_enabled).then_some(ViewerMessage::SwapTranslateLangs),
        ),
        lang_combo_field(
            "vi",
            &tr_state.target_lang,
            &COMMON_TARGET_LANGS,
            dst_enabled,
            ViewerMessage::SetTranslateTargetLang,
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .width(Fill);

    // 4. Icon tool strip: Scan Area | Full Page | Hand | Bubbles | Clear | Save | Reload | Settings
    let tools_row = container(
        row![
            button::compact_icon_tipped(
                Icon::Crop,
                ButtonVariant::Tab {
                    active: !is_pan_mode,
                },
                Some(ViewerMessage::StartTranslateRegionPick),
                tr(lang, "translate-drag-region"),
            ),
            button::compact_icon_tipped(
                Icon::Fit,
                ButtonVariant::Ghost,
                has_path.then_some(ViewerMessage::ScanTranslatePage),
                tr(lang, "translate-select-full"),
            ),
            button::compact_icon_tipped(
                Icon::Hand,
                ButtonVariant::Tab {
                    active: is_pan_mode,
                },
                Some(ViewerMessage::SetTranslateMode(ViewerTranslateMode::Pan)),
                tr(lang, "translate-mode-pan"),
            ),
            button::compact_icon_tipped(
                if tr_state.show_bubbles {
                    Icon::Eye
                } else {
                    Icon::EyeOff
                },
                ButtonVariant::Tab {
                    active: tr_state.show_bubbles,
                },
                Some(ViewerMessage::ToggleTranslateBubbles),
                tr(lang, "translate-show-bubbles"),
            ),
            iced::widget::space::horizontal(),
            button::compact_icon_tipped(
                Icon::Trash,
                ButtonVariant::Ghost,
                has_blocks.then_some(ViewerMessage::ClearTranslateBlocks),
                tr(lang, "action-clear-translation"),
            ),
            button::compact_icon_tipped(
                Icon::Save,
                if tr_state.saved_sidecar {
                    ButtonVariant::Tab { active: true }
                } else {
                    ButtonVariant::Ghost
                },
                (has_path && has_blocks).then_some(ViewerMessage::SaveTranslationSidecar),
                tr(lang, "translate-save-sidecar"),
            ),
            button::compact_icon_tipped(
                Icon::Refresh,
                ButtonVariant::Ghost,
                has_path.then_some(ViewerMessage::ScanTranslatePage),
                tr(lang, "translate-scan-page"),
            ),
            button::compact_icon_tipped(
                Icon::Settings,
                ButtonVariant::Ghost,
                Some(ViewerMessage::OpenTranslateSettings),
                tr(lang, "action-translate-settings"),
            ),
        ]
        .spacing(2)
        .align_y(Alignment::Center)
        .width(Fill),
    )
    .padding([4, 6])
    .width(Fill)
    .style(card_style);

    // 5. Process / Progress bar
    let (pct_val, status_label) = if let Some((pct, stage)) = running_stage {
        (pct as f32, format!("{pct}% · {stage}"))
    } else if has_blocks {
        (
            100.0,
            format!(
                "{} ({})",
                tr(lang, "translate-result-title"),
                tr_state.blocks.len()
            ),
        )
    } else {
        (0.0, tr(lang, "translate-drag-region"))
    };
    let process_bar = column![
        progress_bar(0.0..=100.0, pct_val)
            .length(Fill)
            .girth(Length::Fixed(5.0))
            .style(|theme| {
                let pal = PaletteTokens::for_theme(theme);
                iced::widget::progress_bar::Style {
                    background: pal.bg_app.into(),
                    bar: pal.accent.into(),
                    border: Border {
                        color: pal.border,
                        width: 0.5,
                        radius: RADIUS_SM.into(),
                    },
                }
            }),
        text(status_label)
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_muted),
            }),
    ]
    .spacing(4)
    .width(Fill);

    let controls_tile = container(
        column![
            mode_row,
            model_provider_row,
            lang_row,
            tools_row,
            process_bar
        ]
        .spacing(7)
        .width(Fill),
    )
    .padding(10)
    .width(Fill)
    .style(card_style);

    // 6. Result tile (2 separate frames: Original + Translated)
    let sel_idx = tr_state
        .editing_block_idx
        .filter(|&i| i < tr_state.blocks.len())
        .or_else(|| has_blocks.then_some(0));

    let result_title = match sel_idx {
        Some(i) => format!(
            "{} · #{} / {}",
            tr(lang, "translate-result-title"),
            i + 1,
            tr_state.blocks.len()
        ),
        None => tr(lang, "translate-result-title"),
    };

    let mut result_header = row![
        text(result_title).size(12).style(|theme| text::Style {
            color: Some(PaletteTokens::for_theme(theme).text_muted),
        }),
        iced::widget::space::horizontal(),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .width(Fill);

    if let Some(idx) = sel_idx {
        result_header = result_header
            .push(button::compact_icon_tipped(
                Icon::Refresh,
                ButtonVariant::Ghost,
                Some(ViewerMessage::RetryTranslateBlock(idx)),
                tr(lang, "translate-retry-bubble"),
            ))
            .push(button::compact_icon_tipped(
                Icon::Trash,
                ButtonVariant::Ghost,
                Some(ViewerMessage::RemoveTranslateBlock(idx)),
                tr(lang, "translate-remove-bubble"),
            ))
            .push(button::compact_icon_tipped(
                Icon::Copy,
                ButtonVariant::Ghost,
                Some(ViewerMessage::CopyTranslatedText),
                tr(lang, "action-copy"),
            ))
            .push(button::labeled(
                tr(lang, "tooltip-viewer-next"),
                None,
                ButtonVariant::Secondary,
                Some(ViewerMessage::NextTranslateBubble),
            ));
    }

    let result_body: Element<'a, ViewerMessage> = if let Some(idx) = sel_idx {
        let b = &tr_state.blocks[idx];
        let orig_box = column![
            text(tr(lang, "translate-bubble-orig"))
                .size(10)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).text_muted),
                }),
            text_input("…", &b.original)
                .on_input(move |s| ViewerMessage::UpdateTranslateBlockOriginal(idx, s))
                .size(TEXT_SIZE_CAPTION)
                .padding([5, 8])
                .style(input_style)
                .width(Fill),
        ]
        .spacing(3)
        .width(Fill);

        let trans_box = column![
            text(tr(lang, "translate-bubble-trans"))
                .size(10)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).text_muted),
                }),
            if is_ocr_only {
                text_input("—", &b.translated)
                    .size(TEXT_SIZE_CAPTION)
                    .padding([5, 8])
                    .width(Fill)
                    .style(|theme, _status| {
                        let pal = PaletteTokens::for_theme(theme);
                        iced::widget::text_input::Style {
                            background: pal.bg_app.into(),
                            border: Border {
                                color: pal.border,
                                width: 1.0,
                                radius: RADIUS_SM.into(),
                            },
                            icon: pal.text_muted,
                            placeholder: pal.text_muted,
                            value: pal.text_muted,
                            selection: pal.bg_selected,
                        }
                    })
            } else {
                text_input(&tr(lang, "translate-text-placeholder"), &b.translated)
                    .on_input(move |s| ViewerMessage::UpdateTranslateBlockText(idx, s))
                    .size(TEXT_SIZE_CAPTION)
                    .padding([5, 8])
                    .style(input_style)
                    .width(Fill)
            },
        ]
        .spacing(3)
        .width(Fill);

        column![orig_box, trans_box].spacing(6).width(Fill).into()
    } else {
        text(tr(lang, "translate-empty-hint"))
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_muted),
            })
            .into()
    };

    let result_tile = container(
        column![
            result_header,
            rule::horizontal(1).style(hairline_rule_style),
            result_body,
        ]
        .spacing(6)
        .width(Fill),
    )
    .padding(10)
    .width(Fill)
    .style(card_style);

    // 7. History tile (separated scrollable list of blocks)
    let history_title = if has_blocks {
        format!(
            "{} ({})",
            tr(lang, "translate-history-title"),
            tr_state.blocks.len()
        )
    } else {
        tr(lang, "translate-history-title")
    };

    let history_body: Element<'a, ViewerMessage> = if tr_state.blocks.is_empty() {
        text("—")
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_muted),
            })
            .into()
    } else {
        let mut list_col = column![].spacing(4).width(Fill);
        for (idx, b) in tr_state.blocks.iter().enumerate() {
            let is_sel = sel_idx == Some(idx);
            let display_text = if is_ocr_only || b.translated.trim().is_empty() {
                b.original.trim()
            } else {
                b.translated.trim()
            };
            let snippet = if display_text.is_empty() {
                "…".to_owned()
            } else {
                display_text.to_owned()
            };
            let item_btn = iced_button(
                row![
                    text(format!("#{}", idx + 1))
                        .size(TEXT_SIZE_CAPTION)
                        .style(|theme| text::Style {
                            color: Some(PaletteTokens::for_theme(theme).accent),
                        }),
                    text(snippet).size(TEXT_SIZE_CAPTION),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
                .width(Fill),
            )
            .width(Fill)
            .padding([6, 8])
            .style(move |theme, status| {
                let pal = PaletteTokens::for_theme(theme);
                let bg = if is_sel {
                    pal.bg_selected
                } else if matches!(status, iced_button::Status::Hovered) {
                    pal.bg_hover
                } else {
                    pal.bg_app
                };
                iced_button::Style {
                    background: Some(bg.into()),
                    text_color: pal.text_primary,
                    border: Border {
                        color: if is_sel { pal.accent } else { pal.border },
                        width: 1.0,
                        radius: RADIUS_SM.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(ViewerMessage::SelectTranslateBlock(Some(idx)));
            list_col = list_col.push(item_btn);
        }
        scrollable(container(list_col).padding(Padding {
            top: 0.0,
            right: 10.0,
            bottom: 0.0,
            left: 0.0,
        }))
        .height(Fill)
        .into()
    };

    let history_tile = container(
        column![
            text(history_title).size(12).style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_muted),
            }),
            rule::horizontal(1).style(hairline_rule_style),
            container(history_body).width(Fill).height(Fill),
        ]
        .spacing(6)
        .width(Fill)
        .height(Fill),
    )
    .padding(10)
    .width(Fill)
    .height(Fill)
    .style(card_style);

    container(
        column![controls_tile, result_tile, history_tile]
            .spacing(8)
            .width(Fill)
            .height(Fill),
    )
    .padding([6, 6])
    .width(Fill)
    .height(Fill)
    .into()
}

fn build_viewer_info_body<'a>(
    state: ViewerViewState<'a>,
    lang: Language,
) -> Element<'a, ViewerMessage> {
    let meta = &state.snapshot.meta;
    let viewer_file = meta.viewer_path.as_deref().map(Path::new);
    let format_str = viewer_file
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_uppercase())
        .unwrap_or_else(|| "-".to_owned());
    let folder_name = viewer_file
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("-")
        .to_owned();
    let dims = state
        .viewer_frame
        .as_ref()
        .map(|f| format!("{} × {}", f.width, f.height))
        .unwrap_or_else(|| "-".to_owned());
    let file_size = meta
        .viewer_path
        .as_deref()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| format_viewer_bytes(m.len()))
        .unwrap_or_else(|| "-".to_owned());

    scrollable(
        container(
            column![
                kv_row(tr(lang, "label-format"), format_str),
                kv_row(tr(lang, "label-dimensions"), dims),
                kv_row(tr(lang, "label-file-size"), file_size),
                kv_row(tr(lang, "label-path"), folder_name),
                with_tooltip_pos(
                    button::labeled(
                        tr(lang, "action-copy-path"),
                        Some(Icon::Copy),
                        ButtonVariant::Secondary,
                        meta.viewer_path
                            .as_ref()
                            .map(|_| ViewerMessage::CopyViewerPath),
                    )
                    .width(Fill),
                    tr(lang, "action-copy-path"),
                    TooltipPosition::Top,
                ),
            ]
            .spacing(8)
            .width(Fill),
        )
        .padding(Padding {
            top: 0.0,
            right: 10.0,
            bottom: 0.0,
            left: 0.0,
        }),
    )
    .height(Fill)
    .into()
}

fn adj_slider_row<'a>(
    label: String,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    on_change: impl Fn(f32) -> ViewerMessage + 'a,
) -> Element<'a, ViewerMessage> {
    photon_ui::panel::slider_control_row(
        label,
        value,
        range,
        step,
        format!("{value:.2}"),
        on_change,
    )
}

fn build_viewer_adjustments_body<'a>(
    state: ViewerViewState<'a>,
    lang: Language,
    right_w: f32,
) -> Element<'a, ViewerMessage> {
    let adj = &state.viewer_adj;
    let has_image = state.snapshot.meta.viewer_path.is_some();

    let sliders = column![
        adj_slider_row(
            tr(lang, "label-exposure"),
            adj.exposure,
            -2.0..=2.0,
            0.05,
            ViewerMessage::SetViewerAdjExposure,
        ),
        adj_slider_row(
            tr(lang, "label-gamma"),
            adj.gamma,
            0.2..=3.0,
            0.05,
            ViewerMessage::SetViewerAdjGamma,
        ),
        adj_slider_row(
            tr(lang, "label-highlights"),
            adj.highlights,
            -1.0..=1.0,
            0.05,
            ViewerMessage::SetViewerAdjHighlights,
        ),
        adj_slider_row(
            tr(lang, "label-shadows"),
            adj.shadows,
            -1.0..=1.0,
            0.05,
            ViewerMessage::SetViewerAdjShadows,
        ),
        adj_slider_row(
            tr(lang, "label-brightness"),
            adj.brightness,
            -0.5..=0.5,
            0.02,
            ViewerMessage::SetViewerAdjBrightness,
        ),
        adj_slider_row(
            tr(lang, "label-contrast"),
            adj.contrast,
            -0.5..=0.5,
            0.02,
            ViewerMessage::SetViewerAdjContrast,
        ),
        adj_slider_row(
            tr(lang, "label-saturation"),
            adj.saturation,
            -1.0..=1.0,
            0.05,
            ViewerMessage::SetViewerAdjSaturation,
        ),
    ]
    .spacing(6)
    .width(Fill);

    let actions: Element<'_, ViewerMessage> = if right_w >= 340.0 {
        row![
            button::labeled_tipped(
                tr(lang, "action-reset-adjustments"),
                Some(Icon::Reset),
                ButtonVariant::Ghost,
                Some(ViewerMessage::ResetViewerAdj),
                tr(lang, "action-reset-adjustments"),
            ),
            iced::widget::space::horizontal(),
            button::labeled_tipped(
                tr(lang, "action-open-with-adjustments"),
                Some(Icon::Editor),
                ButtonVariant::Primary,
                has_image.then_some(ViewerMessage::ViewerAdjOpenInEditor),
                tr(lang, "tooltip-viewer-edit"),
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    } else {
        column![
            with_tooltip(
                button::labeled(
                    tr(lang, "action-reset-adjustments"),
                    Some(Icon::Reset),
                    ButtonVariant::Ghost,
                    Some(ViewerMessage::ResetViewerAdj),
                )
                .width(Fill),
                tr(lang, "action-reset-adjustments"),
            ),
            with_tooltip(
                button::labeled(
                    tr(lang, "action-open-with-adjustments"),
                    Some(Icon::Editor),
                    ButtonVariant::Primary,
                    has_image.then_some(ViewerMessage::ViewerAdjOpenInEditor),
                )
                .width(Fill),
                tr(lang, "tooltip-viewer-edit"),
            ),
        ]
        .spacing(6)
        .width(Fill)
        .into()
    };

    column![
        scrollable(container(sliders).padding(Padding {
            top: 0.0,
            right: 10.0,
            bottom: 0.0,
            left: 0.0,
        }),)
        .height(Fill),
        rule::horizontal(1).style(hairline_rule_style),
        actions,
    ]
    .spacing(8)
    .height(Fill)
    .width(Fill)
    .into()
}

fn build_viewer_double_panes<'a>(
    state: ViewerViewState<'a>,
    _lang: Language,
    dark: bool,
    canvas_tool: CanvasTool,
) -> Element<'a, ViewerMessage> {
    let gap = f32::from(state.viewer_manga_page_gap);
    let (left_slot, right_slot) = dual_spread_slots(state);
    let left_path = left_slot.map(|(_, p)| p.to_owned());
    let right_path = right_slot.map(|(_, p)| p.to_owned());

    let canvas: Element<'_, ViewerMessage> = Element::from(
        Shader::new(ImageCanvasProgram {
            frame: state.viewer_frame.cloned(),
            pair_frame: state.viewer_pair_frame.cloned(),
            dual_gap_px: gap,
            compare_frame: None,
            zoom: state.zoom,
            pan_x: state.pan_x,
            pan_y: state.pan_y,
            adjustments: state.viewer_adj,
            dark_theme: dark,
            crop_rect: None,
            crop_aspect: None,
            quarter_turns: state.viewer_quarter_turns,
            tool: canvas_tool,
            preview_angle_rad: 0.0,
            preview_fill: [0.0; 4],
            canvas_bg: None,
            viewport_bg: state.viewport_background,
            compare_mode: 0,
            compare_split_x: 0.5,
            brush_radius_px: 0.0,
            selection_mask: None,
            has_selection: false,
            quick_mask_active: false,
            marching_ants_phase: 0.0,
            restrict_pan_to_hand: false,
            stabilizer_radius: 0.0,
            aa_mode: state.aa_mode,
            denoise_mode: state.denoise_mode,
            ..Default::default()
        })
        .width(Fill)
        .height(Fill),
    )
    .map(move |ev| match ev {
        CanvasEvent::ContextMenuRequested { is_right_pair, .. } => {
            let target = if is_right_pair {
                right_path.clone().or_else(|| left_path.clone())
            } else {
                left_path.clone()
            };
            ViewerMessage::ViewerCanvas(target, ev)
        }
        other => ViewerMessage::ViewerCanvas(None, other),
    });

    container(canvas)
        .width(Fill)
        .height(Fill)
        .style(canvas_backdrop_style)
        .into()
}

pub fn webtoon_scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("viewer_webtoon_strip")
}

pub fn filmstrip_scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("viewer_filmstrip_strip")
}

pub fn compute_webtoon_layout(state: ViewerViewState<'_>) -> (f32, Vec<(String, f32, f32)>) {
    let right_open = ViewerRightPanel::ALL
        .iter()
        .any(|&p| viewer_panel_is_open(state, p));
    let right_chrome = if right_open {
        viewer_right_width(state) + 20.0
    } else {
        24.0
    };
    let avail_w = (state.window_size.width - right_chrome).max(320.0);
    let avail_h = (state.window_size.height - 130.0).max(240.0);

    let mut widths: Vec<u32> = state
        .snapshot
        .viewer_images
        .iter()
        .take(20)
        .filter_map(|p| state.image_sizes.get(p).map(|(w, _)| *w))
        .filter(|w| *w > 0)
        .collect();
    widths.sort_unstable();
    let median_w = widths
        .get(widths.len() / 2)
        .copied()
        .map(|w| w as f32)
        .unwrap_or(0.0);

    let base_col_w = match state.viewer_webtoon_fit_mode {
        WebtoonFitMode::Width => avail_w,
        WebtoonFitMode::Original => {
            if median_w > 0.0 {
                median_w
            } else {
                avail_w
            }
        }
        WebtoonFitMode::Auto | WebtoonFitMode::Height => {
            if median_w > 0.0 {
                median_w.max(360.0).min(avail_w)
            } else {
                avail_w
            }
        }
    };
    let col_w = (base_col_w * state.webtoon_zoom).max(240.0);

    let pages = state
        .snapshot
        .viewer_images
        .iter()
        .map(|p| {
            let aspect = state
                .image_sizes
                .get(p)
                .filter(|(w, h)| *w > 0 && *h > 0)
                .map(|(w, h)| (*h as f32 / *w as f32).clamp(0.2, 12.0))
                .unwrap_or(1.4);
            let (slot_w, slot_h) = if state.viewer_webtoon_fit_mode == WebtoonFitMode::Height {
                let h = (avail_h * state.webtoon_zoom).max(200.0);
                let w = (h / aspect).max(160.0);
                (w, h)
            } else {
                (col_w, (col_w * aspect).max(64.0))
            };
            (p.clone(), slot_w, slot_h)
        })
        .collect();

    (col_w, pages)
}

pub fn webtoon_offset_for_index(state: ViewerViewState<'_>, target: usize) -> Option<f32> {
    let (_, pages) = compute_webtoon_layout(state);
    if target >= pages.len() {
        return None;
    }
    let gap = f32::from(state.viewer_manga_page_gap);
    let y: f32 = pages.iter().take(target).map(|(_, _, h)| *h + gap).sum();
    Some(y)
}

pub fn webtoon_index_at_scroll(state: ViewerViewState<'_>, center_y: f32) -> Option<usize> {
    let (_, pages) = compute_webtoon_layout(state);
    let gap = f32::from(state.viewer_manga_page_gap);
    let mut y = 0.0f32;
    for (idx, (_, _, h)) in pages.iter().enumerate() {
        let bottom = y + *h + gap;
        if center_y < bottom || idx + 1 == pages.len() {
            return Some(idx);
        }
        y = bottom;
    }
    None
}

fn build_viewer_webtoon_strip<'a>(state: ViewerViewState<'a>) -> Element<'a, ViewerMessage> {
    let gap = f32::from(state.viewer_manga_page_gap);
    let (_, pages) = compute_webtoon_layout(state);
    let lang = state.snapshot.meta.language;
    let tr_state = state.translate;
    let tr_active = tr_state.active;
    let show_bubbles = tr_active && tr_state.show_bubbles;
    let is_review = tr_state.mode == ViewerTranslateMode::Review;
    let marquee_enabled = tr_active
        && !tr_state.space_pan_held
        && !is_review
        && (tr_state.picking_region
            || matches!(
                tr_state.mode,
                ViewerTranslateMode::Translate | ViewerTranslateMode::OcrOnly
            ));

    let mut col = column![]
        .spacing(gap)
        .align_x(Alignment::Center)
        .width(Fill);

    for (path, w, h) in pages {
        let handle = state
            .webtoon_pages
            .get(&path)
            .cloned()
            .or_else(|| state.thumbnails.get(&path).cloned());
        let page_el: Element<'_, ViewerMessage> = if let Some(hnd) = handle {
            container(
                iced_image(hnd)
                    .width(Length::Fixed(w))
                    .height(Length::Fixed(h))
                    .content_fit(ContentFit::Contain),
            )
            .width(Length::Fixed(w))
            .height(Length::Fixed(h))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
        } else {
            container(photon_ui::icons::render(Icon::Image, 28.0, None))
                .width(Length::Fixed(w))
                .height(Length::Fixed(h))
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .style(surface_panel_style)
                .into()
        };

        let page_blocks = tr_state.bubble_cache.get(&path).or_else(|| {
            (tr_state.active_page_path.as_deref() == Some(path.as_str())
                || (tr_state.active_page_path.is_none()
                    && state.snapshot.meta.viewer_path.as_deref() == Some(path.as_str())))
            .then_some(&tr_state.blocks)
        });

        let mut layers: Vec<Element<'_, ViewerMessage>> = vec![page_el];

        if show_bubbles
            && let Some(blocks) = page_blocks
            && !blocks.is_empty()
            && let Some(&(img_w, img_h)) = state.image_sizes.get(&path)
            && img_w > 0
            && img_h > 0
        {
            let doc_w = img_w as f32;
            let doc_h = img_h as f32;
            let mut bubble_items: Vec<Element<'_, ViewerMessage>> =
                Vec::with_capacity(blocks.len() + 1);
            bubble_items.push(
                iced::widget::space::horizontal()
                    .width(Fill)
                    .height(Fill)
                    .into(),
            );
            for (idx, b) in blocks.iter().enumerate() {
                let (dx_px, dy_px) = match tr_state.active_drag_delta {
                    Some((drag_idx, d))
                        if drag_idx == idx
                            && tr_state.active_drag_page.as_deref() == Some(path.as_str()) =>
                    {
                        (d[0], d[1])
                    }
                    _ => (0.0, 0.0),
                };
                let sx = ((b.x as f32 / doc_w) * w + dx_px).max(0.0);
                let sy = ((b.y as f32 / doc_h) * h + dy_px).max(0.0);
                let min_w = if is_review { 48.0 } else { 96.0 };
                let min_h = if is_review { 28.0 } else { 44.0 };
                let sw = ((b.w.max(1) as f32 / doc_w) * w).max(min_w);
                let sh = ((b.h.max(1) as f32 / doc_h) * h).max(min_h);
                if let Some(el) = render_translate_bubble_item(
                    tr_state,
                    lang,
                    Some(&path),
                    idx,
                    b,
                    sx,
                    sy,
                    sw,
                    sh,
                ) {
                    bubble_items.push(el);
                }
            }
            layers.push(
                stack(bubble_items)
                    .width(Length::Fixed(w))
                    .height(Length::Fixed(h))
                    .into(),
            );
        }

        if let Some((m_path, [mx, my, mw, mh])) = &tr_state.webtoon_marquee
            && m_path == &path
            && *mw >= 2.0
            && *mh >= 2.0
        {
            let marquee_box = container(iced::widget::space::horizontal().width(Fill).height(Fill))
                .width(Length::Fixed(*mw))
                .height(Length::Fixed(*mh))
                .style(|theme| {
                    let pal = PaletteTokens::for_theme(theme);
                    container::Style {
                        background: Some(pal.accent.scale_alpha(0.16).into()),
                        border: Border {
                            color: pal.accent,
                            width: 1.5,
                            radius: 2.0.into(),
                        },
                        ..Default::default()
                    }
                });
            layers.push(
                container(marquee_box)
                    .padding(Padding {
                        top: (*my).max(0.0),
                        right: 0.0,
                        bottom: 0.0,
                        left: (*mx).max(0.0),
                    })
                    .width(Length::Fixed(w))
                    .height(Length::Fixed(h))
                    .into(),
            );
        }

        let stacked_page: Element<'_, ViewerMessage> = if layers.len() == 1 {
            layers.pop().unwrap()
        } else {
            stack(layers)
                .width(Length::Fixed(w))
                .height(Length::Fixed(h))
                .into()
        };

        let mut area =
            mouse_area(stacked_page).on_right_press(ViewerMessage::OpenViewerContextMenu {
                path: Some(path.clone()),
                pos: None,
            });
        if marquee_enabled {
            let p_move = path.clone();
            let p_press = path;
            area = area
                .on_move(move |pt| ViewerMessage::WebtoonHoverPage {
                    path: p_move.clone(),
                    local_pos: [pt.x, pt.y],
                    disp_w: w,
                    disp_h: h,
                })
                .on_press(ViewerMessage::BeginWebtoonTranslateRegion {
                    path: p_press,
                    disp_w: w,
                    disp_h: h,
                });
        }
        col = col.push(area);
    }

    mouse_area(
        container(
            scrollable(container(col).padding([8, 0]).width(Fill))
                .id(webtoon_scroll_id())
                .on_scroll(ViewerMessage::WebtoonScrolled)
                .width(Fill)
                .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .style(canvas_backdrop_style),
    )
    .on_right_press(ViewerMessage::OpenViewerContextMenu {
        path: None,
        pos: None,
    })
    .into()
}

pub fn compute_dual_spreads(
    images: &[String],
    sizes: &HashMap<String, (u32, u32)>,
    separate_cover: bool,
) -> Vec<Vec<usize>> {
    let count = images.len();
    if count == 0 {
        return Vec::new();
    }
    let is_landscape = |idx: usize| -> bool {
        images
            .get(idx)
            .and_then(|p| sizes.get(p))
            .is_some_and(|(w, h)| *w > 0 && *h > 0 && *w > *h)
    };
    let mut spreads = Vec::new();
    let mut i = 0;
    if separate_cover && count > 1 {
        spreads.push(vec![0]);
        i = 1;
    }
    while i < count {
        if is_landscape(i) || i + 1 >= count || is_landscape(i + 1) {
            spreads.push(vec![i]);
            i += 1;
        } else {
            spreads.push(vec![i, i + 1]);
            i += 2;
        }
    }
    spreads
}

pub fn dual_spread_slots_for<'a>(
    images: &'a [String],
    sizes: &HashMap<String, (u32, u32)>,
    current_index: Option<usize>,
    separate_cover: bool,
    manga_rtl: bool,
) -> DualSpreadSlots<'a> {
    let spreads = compute_dual_spreads(images, sizes, separate_cover);
    if spreads.is_empty() {
        return (None, None);
    }
    let idx = current_index.unwrap_or(0);
    let spread = spreads
        .iter()
        .find(|sp| sp.contains(&idx))
        .unwrap_or(&spreads[0]);
    if spread.len() == 1 {
        let i = spread[0];
        return (images.get(i).map(|p| (i, p.as_str())), None);
    }
    let (li, ri) = if manga_rtl {
        (spread[1], spread[0])
    } else {
        (spread[0], spread[1])
    };
    (
        images.get(li).map(|p| (li, p.as_str())),
        images.get(ri).map(|p| (ri, p.as_str())),
    )
}

pub fn dual_spread_slots<'a>(state: ViewerViewState<'a>) -> DualSpreadSlots<'a> {
    dual_spread_slots_for(
        &state.snapshot.viewer_images,
        state.image_sizes,
        state.snapshot.meta.viewer_index,
        state.viewer_manga_cover_separate,
        state.viewer_manga_rtl,
    )
}

pub fn step_dual_spread(state: ViewerViewState<'_>, delta: isize) -> Option<String> {
    let images = &state.snapshot.viewer_images;
    let spreads =
        compute_dual_spreads(images, state.image_sizes, state.viewer_manga_cover_separate);
    if spreads.is_empty() {
        return None;
    }
    let cur_img = state.snapshot.meta.viewer_index.unwrap_or(0);
    let cur_sp = spreads
        .iter()
        .position(|sp| sp.contains(&cur_img))
        .unwrap_or(0) as isize;
    let next_sp = (cur_sp + delta).rem_euclid(spreads.len() as isize) as usize;
    let target_idx = spreads[next_sp][0];
    images.get(target_idx).cloned()
}

/// Reader/View quality menu entries for Canvas Anti-Aliasing and Viewport Denoise.
pub fn build_viewer_quality_menu_entries<'a>(
    state: ViewerViewState<'a>,
) -> Vec<MenuEntry<'a, ViewerMessage>> {
    let lang = state.snapshot.meta.language;
    vec![
        MenuEntry::Header(tr(lang, "settings-canvas-aa")),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "aa-mode-crisp"),
                ViewerMessage::SetAntiAliasingMode(CanvasAntiAliasingMode::Crisp),
            )
            .checked(state.aa_mode == CanvasAntiAliasingMode::Crisp),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "aa-mode-smooth"),
                ViewerMessage::SetAntiAliasingMode(CanvasAntiAliasingMode::Smooth),
            )
            .checked(state.aa_mode == CanvasAntiAliasingMode::Smooth),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "aa-mode-high-quality"),
                ViewerMessage::SetAntiAliasingMode(CanvasAntiAliasingMode::HighQuality),
            )
            .checked(state.aa_mode == CanvasAntiAliasingMode::HighQuality),
        ),
        MenuEntry::Separator,
        MenuEntry::Header(tr(lang, "settings-viewport-denoise")),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "denoise-mode-off"),
                ViewerMessage::SetViewportDenoiseMode(ViewportDenoiseMode::Off),
            )
            .checked(state.denoise_mode == ViewportDenoiseMode::Off),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "denoise-mode-light"),
                ViewerMessage::SetViewportDenoiseMode(ViewportDenoiseMode::Light),
            )
            .checked(state.denoise_mode == ViewportDenoiseMode::Light),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "denoise-mode-medium"),
                ViewerMessage::SetViewportDenoiseMode(ViewportDenoiseMode::Medium),
            )
            .checked(state.denoise_mode == ViewportDenoiseMode::Medium),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "denoise-mode-strong"),
                ViewerMessage::SetViewportDenoiseMode(ViewportDenoiseMode::Strong),
            )
            .checked(state.denoise_mode == ViewportDenoiseMode::Strong),
        ),
    ]
}

/// Right-click context menu for Viewer canvas & pages.
pub fn build_viewer_context_menu<'a>(
    state: ViewerViewState<'a>,
    target_path: Option<&str>,
) -> (f32, Element<'a, ViewerMessage>) {
    let lang = state.snapshot.meta.language;
    let resolved_path = target_path
        .map(str::to_owned)
        .or_else(|| state.snapshot.meta.viewer_path.clone());
    let has_path = resolved_path.is_some();
    let vmode = state.viewer_mode;
    let is_webtoon = vmode == ViewerViewMode::Webtoon;

    let edit_msg = resolved_path
        .as_ref()
        .map(|p| ViewerMessage::ContextMenuDispatch(Command::EditorOpen { path: p.clone() }))
        .unwrap_or(ViewerMessage::Noop);
    let new_win_msg = resolved_path
        .clone()
        .map(ViewerMessage::OpenInNewViewerWindow)
        .unwrap_or(ViewerMessage::Noop);
    let copy_img_msg = resolved_path
        .as_ref()
        .map(|p| ViewerMessage::CopyImageToClipboard(Some(p.clone())))
        .unwrap_or(ViewerMessage::Noop);
    let copy_path_msg = resolved_path
        .as_ref()
        .map(|_| ViewerMessage::CopyViewerPath)
        .unwrap_or(ViewerMessage::Noop);
    let reveal_msg = resolved_path
        .clone()
        .map(ViewerMessage::RevealInFolder)
        .unwrap_or(ViewerMessage::Noop);

    let entries = vec![
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-edit"), edit_msg)
                .icon(Icon::Editor)
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-open-new-viewer-window"), new_win_msg)
                .icon(Icon::Viewer)
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-copy-image"), copy_img_msg)
                .icon(Icon::Copy)
                .shortcut("Ctrl+Shift+C")
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-copy-path"), copy_path_msg)
                .icon(Icon::Copy)
                .shortcut("Ctrl+Alt+C")
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-reveal-folder"), reveal_msg)
                .icon(Icon::FolderOpen)
                .enabled(has_path),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-fit"),
                ViewerMessage::Canvas(CanvasEvent::ResetFit),
            )
            .icon(Icon::Fit)
            .shortcut("F")
            .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-actual-size"),
                ViewerMessage::ViewerZoomActual,
            )
            .icon(Icon::ZoomIn)
            .shortcut("1")
            .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-zoom-in"), ViewerMessage::ZoomStep(1.25))
                .icon(Icon::ZoomIn)
                .shortcut("+")
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-zoom-out"), ViewerMessage::ZoomStep(0.8))
                .icon(Icon::ZoomOut)
                .shortcut("-")
                .enabled(has_path),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-rotate-left"),
                ViewerMessage::ViewerRotatePreview(-1),
            )
            .icon(Icon::RotateLeft)
            .shortcut("Shift+R")
            .enabled(has_path && !is_webtoon),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-rotate-right"),
                ViewerMessage::ViewerRotatePreview(1),
            )
            .icon(Icon::RotateRight)
            .shortcut("R")
            .enabled(has_path && !is_webtoon),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-viewer-single"),
                ViewerMessage::SetViewerViewMode(ViewerViewMode::Single),
            )
            .icon(Icon::ViewSingle)
            .shortcut("S")
            .checked(vmode == ViewerViewMode::Single),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-viewer-double"),
                ViewerMessage::SetViewerViewMode(ViewerViewMode::Double),
            )
            .icon(Icon::ViewDouble)
            .shortcut("D")
            .checked(vmode == ViewerViewMode::Double),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-viewer-manhwa"),
                ViewerMessage::SetViewerViewMode(ViewerViewMode::Webtoon),
            )
            .icon(Icon::ViewManhwa)
            .shortcut("M")
            .checked(vmode == ViewerViewMode::Webtoon),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-viewer-color"),
                ViewerMessage::ToggleViewerColorPopover,
            )
            .icon(Icon::Palette)
            .checked(state.viewer_adjustments_open)
            .enabled(has_path && !is_webtoon),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-filmstrip"), ViewerMessage::ToggleFilmstrip)
                .icon(Icon::Filmstrip)
                .checked(state.viewer_filmstrip_open),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-fullscreen"),
                ViewerMessage::ToggleFullscreen,
            )
            .icon(if state.is_fullscreen {
                Icon::FullscreenExit
            } else {
                Icon::Fullscreen
            })
            .shortcut("F11")
            .checked(state.is_fullscreen),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-info"),
                ViewerMessage::ToggleViewerInspector,
            )
            .icon(Icon::Info)
            .shortcut("I")
            .checked(state.viewer_info_open),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-plugins"),
                ViewerMessage::ToggleContextPluginsSubmenu,
            )
            .icon(Icon::Plug)
            .submenu(true)
            .on_hover(ViewerMessage::OpenContextPluginsSubmenu),
        ),
    ];

    let est_h = photon_ui::menu::estimate_menu_height_with_spacing(&entries);
    (est_h, menu_panel_entries(entries, 225.0))
}

fn plugin_display_name(meta: &photon_protocol::UiStateMeta, plugin_id: &str) -> String {
    meta.plugins
        .iter()
        .find(|p| p.id == plugin_id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| plugin_id.to_owned())
}

/// Secondary submenu for Viewer context menu containing Read & Translate and installed Plugins
/// grouped by plugin (hover/click opens a tertiary submenu with that plugin's actions).
pub fn build_viewer_plugins_submenu<'a>(
    state: ViewerViewState<'a>,
    target_path: Option<&str>,
) -> Option<(f32, Element<'a, ViewerMessage>)> {
    let lang = state.snapshot.meta.language;
    let resolved_path = target_path
        .map(str::to_owned)
        .or_else(|| state.snapshot.meta.viewer_path.clone());
    let has_path = resolved_path.is_some();
    let is_webtoon = state.viewer_mode == ViewerViewMode::Webtoon;

    let mut entries = vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-viewer-read-translate"),
                ViewerMessage::ToggleTranslateOverlay,
            )
            .icon(Icon::Translate)
            .shortcut("L")
            .checked(state.translate.active)
            .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-viewer-scan-region"),
                ViewerMessage::StartTranslateRegionPick,
            )
            .icon(Icon::Crop)
            .enabled(has_path && !is_webtoon),
        ),
    ];

    if resolved_path.is_some() {
        let qas = viewer_quick_actions(&state.snapshot.meta);
        if !qas.is_empty() {
            entries.push(MenuEntry::Separator);
            let mut plugin_ids: Vec<String> = Vec::new();
            for qa in qas {
                if !plugin_ids.contains(&qa.plugin_id) {
                    plugin_ids.push(qa.plugin_id.clone());
                }
            }
            for plugin_id in plugin_ids {
                let label = plugin_display_name(&state.snapshot.meta, &plugin_id);
                entries.push(MenuEntry::Item(
                    MenuItem::new(
                        label,
                        ViewerMessage::ToggleContextPluginGroup(plugin_id.clone()),
                    )
                    .icon(Icon::Plug)
                    .submenu(true)
                    .on_hover(ViewerMessage::OpenContextPluginGroup(plugin_id)),
                ));
            }
        }
    }

    entries.push(MenuEntry::Separator);
    entries.push(MenuEntry::Item(
        MenuItem::new(
            tr(lang, "action-plugin-settings"),
            ViewerMessage::OpenPluginSettings,
        )
        .icon(Icon::Plug),
    ));

    let est_h = photon_ui::menu::estimate_menu_height_with_spacing(&entries);
    Some((est_h, menu_panel_entries(entries, 220.0)))
}

/// Tertiary submenu listing Quick Actions for a single plugin in the Viewer context menu.
pub fn build_viewer_plugin_group_submenu<'a>(
    state: ViewerViewState<'a>,
    target_path: Option<&str>,
    plugin_id: &str,
) -> Option<(f32, Element<'a, ViewerMessage>)> {
    let target_path = target_path
        .map(str::to_owned)
        .or_else(|| state.snapshot.meta.viewer_path.clone())?;
    let entries: Vec<MenuEntry<'a, ViewerMessage>> = viewer_quick_actions(&state.snapshot.meta)
        .into_iter()
        .filter(|qa| qa.plugin_id == plugin_id)
        .map(|qa| {
            MenuEntry::Item(
                MenuItem::new(
                    qa.label.clone(),
                    ViewerMessage::ContextMenuDispatch(Command::QuickActionRun {
                        plugin_id: qa.plugin_id.clone(),
                        action_id: qa.id.clone(),
                        paths: vec![target_path.clone()],
                    }),
                )
                .icon(Icon::Plug),
            )
        })
        .collect();
    if entries.is_empty() {
        return None;
    }
    let est_h = photon_ui::menu::estimate_menu_height_with_spacing(&entries);
    Some((est_h, menu_panel_entries(entries, 220.0)))
}

fn viewer_fit_geometry(
    doc_w: f32,
    doc_h: f32,
    quarter_turns: u8,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    vp_w: f32,
    vp_h: f32,
) -> (f32, f32, f32, f32, f32, f32) {
    let vp_w = vp_w.max(1.0);
    let vp_h = vp_h.max(1.0);
    let (fw, fh) = if quarter_turns % 2 == 1 {
        (doc_h.max(1.0), doc_w.max(1.0))
    } else {
        (doc_w.max(1.0), doc_h.max(1.0))
    };
    let vp_aspect = vp_w / vp_h;
    let img_aspect = fw / fh;
    let (fit_w, fit_h) = if img_aspect > vp_aspect {
        (vp_w, vp_w / img_aspect)
    } else {
        (vp_h * img_aspect, vp_h)
    };
    let drawn_w = fit_w * zoom.max(0.02);
    let drawn_h = fit_h * zoom.max(0.02);
    let cx = vp_w * 0.5 + pan_x;
    let cy = vp_h * 0.5 + pan_y;
    let left = cx - drawn_w * 0.5;
    let top = cy - drawn_h * 0.5;
    (left, top, drawn_w, drawn_h, fw, fh)
}

/// Transform unrotated image-space `(x, y, w, h)` into display-space coordinates for `quarter_turns`
/// (0, 1=90° CW, 2=180°, 3=270° CW) within an unrotated `(doc_w, doc_h)` image.
pub fn transform_block_for_quarter_turns(
    b: &TranslationBlock,
    doc_w: f32,
    doc_h: f32,
    quarter_turns: u8,
) -> (f32, f32, f32, f32) {
    let bx = b.x as f32;
    let by = b.y as f32;
    let bw = b.w.max(1) as f32;
    let bh = b.h.max(1) as f32;
    match quarter_turns % 4 {
        1 => ((doc_h - (by + bh)).max(0.0), bx.max(0.0), bh, bw),
        2 => (
            (doc_w - (bx + bw)).max(0.0),
            (doc_h - (by + bh)).max(0.0),
            bw,
            bh,
        ),
        3 => (by.max(0.0), (doc_w - (bx + bw)).max(0.0), bh, bw),
        _ => (bx.max(0.0), by.max(0.0), bw, bh),
    }
}

/// Screen-space geometry for a dual-page spread matching `photon-canvas` GPU rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoubleSpreadScreenGeometry {
    pub spread_left: f32,
    pub spread_top: f32,
    pub drawn_w: f32,
    pub drawn_h: f32,
    pub fw: f32,
    pub fh: f32,
    pub spread_w: f32,
    pub spread_h: f32,
    pub left_disp_w: f32,
    pub gap_doc: f32,
    pub right_disp_w: f32,
}

/// Compute screen-space spread geometry for Dual (`Double`) mode matching `photon-canvas/src/gpu.rs`.
#[allow(clippy::too_many_arguments)]
pub fn viewer_double_fit_geometries(
    left_size: (u32, u32),
    right_size: Option<(u32, u32)>,
    dual_gap_px: f32,
    quarter_turns: u8,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    vp_w: f32,
    vp_h: f32,
) -> DoubleSpreadScreenGeometry {
    let (spread_w, spread_h, left_disp_w, gap_doc, right_disp_w) = match right_size {
        Some(r) => {
            let (sw, sh, lw, gap) = photon_canvas::dual_spread_geometry(left_size, r, dual_gap_px);
            let rw = (sw - lw - gap).max(1.0);
            (sw, sh, lw, gap, rw)
        }
        None => {
            let lw = left_size.0.max(1) as f32;
            let lh = left_size.1.max(1) as f32;
            (lw, lh, lw, 0.0, 0.0)
        }
    };
    let (spread_left, spread_top, drawn_w, drawn_h, fw, fh) = viewer_fit_geometry(
        spread_w,
        spread_h,
        quarter_turns,
        zoom,
        pan_x,
        pan_y,
        vp_w,
        vp_h,
    );
    DoubleSpreadScreenGeometry {
        spread_left,
        spread_top,
        drawn_w,
        drawn_h,
        fw,
        fh,
        spread_w,
        spread_h,
        left_disp_w,
        gap_doc,
        right_disp_w,
    }
}

/// Map a translation block from unrotated page `(img_w, img_h)` coordinates into screen `(sx, sy, sw, sh)`
/// for either the left (`is_right == false`) or right (`is_right == true`) spread page.
pub fn transform_double_page_block_to_screen(
    b: &TranslationBlock,
    img_w: f32,
    img_h: f32,
    is_right: bool,
    geom: DoubleSpreadScreenGeometry,
    quarter_turns: u8,
) -> (f32, f32, f32, f32) {
    let (x_off, page_disp_w) = if is_right {
        (geom.left_disp_w + geom.gap_doc, geom.right_disp_w.max(1.0))
    } else {
        (0.0, geom.left_disp_w.max(1.0))
    };
    let sx_scale = page_disp_w / img_w.max(1.0);
    let sy_scale = geom.spread_h / img_h.max(1.0);
    let spread_block = TranslationBlock {
        x: (x_off + b.x as f32 * sx_scale).round() as i32,
        y: (b.y as f32 * sy_scale).round() as i32,
        w: (b.w.max(1) as f32 * sx_scale).round().max(1.0) as i32,
        h: (b.h.max(1) as f32 * sy_scale).round().max(1.0) as i32,
        original: String::new(),
        translated: String::new(),
    };
    let (rx, ry, rw, rh) = transform_block_for_quarter_turns(
        &spread_block,
        geom.spread_w,
        geom.spread_h,
        quarter_turns,
    );
    let sx = geom.spread_left + (rx / geom.fw) * geom.drawn_w;
    let sy = geom.spread_top + (ry / geom.fh) * geom.drawn_h;
    let sw = (rw / geom.fw) * geom.drawn_w;
    let sh = (rh / geom.fh) * geom.drawn_h;
    (sx, sy, sw, sh)
}

/// Compute an auto-fit pixel font size so translated text fits cleanly inside `(sw, sh)`.
pub fn compute_bubble_font_size(
    sw: f32,
    sh: f32,
    header_h: f32,
    char_count: usize,
    font_scale: f32,
) -> f32 {
    let avail_w = (sw - 14.0).max(24.0);
    let avail_h = (sh - header_h - 10.0).max(16.0);
    let chars = char_count.max(1) as f32;
    let area_px = ((avail_w * avail_h) / (chars * 0.62)).sqrt();
    let base_px = area_px.min((sh * 0.24).max(11.0)).clamp(9.0, 24.0);
    (base_px * font_scale).clamp(9.0, 28.0)
}

#[allow(clippy::too_many_arguments)]
fn render_translate_bubble_item<'a>(
    tr_state: &ViewerTranslateState,
    lang: Language,
    page_path: Option<&str>,
    idx: usize,
    b: &TranslationBlock,
    sx: f32,
    sy: f32,
    sw: f32,
    sh: f32,
) -> Option<Element<'a, ViewerMessage>> {
    let ocr_only = tr_state.mode == ViewerTranslateMode::OcrOnly;
    let is_review = tr_state.mode == ViewerTranslateMode::Review;
    let show_orig = ocr_only
        || match page_path {
            Some(p) => tr_state
                .show_original_page_blocks
                .contains(&(p.to_owned(), idx)),
            None => tr_state.show_original_indices.contains(&idx),
        };
    let has_both = !ocr_only && !b.original.trim().is_empty() && !b.translated.trim().is_empty();
    let display_text = if show_orig || b.translated.trim().is_empty() {
        b.original.clone()
    } else {
        b.translated.clone()
    };
    if display_text.trim().is_empty() {
        return None;
    }

    let header_h = if is_review { 0.0 } else { 22.0 };
    let font_size = compute_bubble_font_size(
        sw,
        sh,
        header_h,
        display_text.chars().count(),
        tr_state.font_scale,
    );
    let page_matches = match page_path {
        Some(p) => tr_state.active_page_path.as_deref() == Some(p),
        None => true,
    };
    let is_sel = page_matches && tr_state.editing_block_idx == Some(idx);
    let drag_page_matches = match page_path {
        Some(p) => tr_state.active_drag_page.as_deref() == Some(p),
        None => tr_state.active_drag_page.is_none(),
    };
    let is_dragging =
        drag_page_matches && matches!(tr_state.active_drag_delta, Some((d_idx, _)) if d_idx == idx);

    let toggle_msg = match page_path {
        Some(p) => ViewerMessage::ToggleTranslatePageBlockOriginal(p.to_owned(), idx),
        None => ViewerMessage::ToggleTranslateBlockOriginal(idx),
    };
    let select_msg = match page_path {
        Some(p) => ViewerMessage::SelectTranslatePageBlock(p.to_owned(), idx),
        None => ViewerMessage::SelectTranslateBlock(Some(idx)),
    };
    let drag_msg = match page_path {
        Some(p) => ViewerMessage::BeginDragTranslatePageBubble(p.to_owned(), idx),
        None => ViewerMessage::BeginDragTranslateBubble(idx),
    };
    let retry_msg = match page_path {
        Some(p) => ViewerMessage::RetryTranslatePageBlock(p.to_owned(), idx),
        None => ViewerMessage::RetryTranslateBlock(idx),
    };
    let remove_msg = match page_path {
        Some(p) => ViewerMessage::RemoveTranslatePageBlock(p.to_owned(), idx),
        None => ViewerMessage::RemoveTranslateBlock(idx),
    };

    let body_press = if is_review && has_both {
        toggle_msg.clone()
    } else {
        select_msg
    };

    let pass_through_pan = tr_state.space_pan_held || tr_state.mode == ViewerTranslateMode::Pan;

    let text_content = container(
        text(display_text)
            .size(font_size)
            .align_x(iced::alignment::Horizontal::Center)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_primary),
            }),
    )
    .width(Fill)
    .height(Fill)
    .padding([4, 7])
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center);

    let text_body: Element<'_, ViewerMessage> = if pass_through_pan {
        text_content.into()
    } else {
        iced_button(text_content)
            .width(Fill)
            .height(Fill)
            .padding(0)
            .style(|theme, _| {
                let pal = PaletteTokens::for_theme(theme);
                iced_button::Style {
                    background: None,
                    text_color: pal.text_primary,
                    ..Default::default()
                }
            })
            .on_press(body_press)
            .into()
    };

    let idx_badge = text(format!("#{}", idx + 1))
        .size(10)
        .style(|theme| text::Style {
            color: Some(PaletteTokens::for_theme(theme).accent),
        });

    let bubble_inner: Element<'_, ViewerMessage> = if is_review {
        text_body
    } else if pass_through_pan {
        let header_row = row![
            container(idx_badge)
                .padding([0, 6])
                .height(Length::Fixed(22.0))
                .align_y(iced::alignment::Vertical::Center),
            iced::widget::space::horizontal(),
        ]
        .spacing(3)
        .align_y(Alignment::Center)
        .height(Length::Fixed(22.0))
        .padding([0, 4]);

        let header_bar = container(header_row).width(Fill).style(|theme| {
            let pal = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(pal.bg_elevated.scale_alpha(0.95).into()),
                text_color: Some(pal.text_primary),
                border: Border {
                    color: pal.border,
                    width: 0.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        });

        column![
            header_bar,
            rule::horizontal(1).style(hairline_rule_style),
            text_body
        ]
        .width(Fill)
        .height(Fill)
        .into()
    } else {
        let drag_grip = mouse_area(
            container(
                row![
                    text("⋮⋮").size(10).style(|theme| text::Style {
                        color: Some(PaletteTokens::for_theme(theme).text_muted),
                    }),
                    idx_badge,
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .padding([0, 6])
            .height(Length::Fixed(22.0))
            .align_y(iced::alignment::Vertical::Center),
        )
        .on_press(drag_msg);

        let mut header_row = row![drag_grip, iced::widget::space::horizontal()]
            .spacing(3)
            .align_y(Alignment::Center)
            .height(Length::Fixed(22.0))
            .padding([0, 4]);

        if has_both {
            let toggle_lbl = tr(
                lang,
                if show_orig {
                    "translate-bubble-trans"
                } else {
                    "translate-bubble-orig"
                },
            );
            header_row = header_row.push(
                iced_button(text(toggle_lbl).size(10))
                    .padding([1, 5])
                    .style(button::style_for(if show_orig {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    }))
                    .on_press(toggle_msg),
            );
        }

        header_row = header_row
            .push(
                iced_button(text("↻").size(11))
                    .padding([1, 5])
                    .style(button::style_for(ButtonVariant::Ghost))
                    .on_press(retry_msg),
            )
            .push(
                iced_button(text("×").size(11))
                    .padding([1, 5])
                    .style(button::style_for(ButtonVariant::Ghost))
                    .on_press(remove_msg),
            );

        let header_bar = container(header_row).width(Fill).style(|theme| {
            let pal = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(pal.bg_elevated.scale_alpha(0.95).into()),
                text_color: Some(pal.text_primary),
                border: Border {
                    color: pal.border,
                    width: 0.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        });

        column![
            header_bar,
            rule::horizontal(1).style(hairline_rule_style),
            text_body
        ]
        .width(Fill)
        .height(Fill)
        .into()
    };

    let frosted = tr_state.frosted_bubbles;
    let bubble_box = container(bubble_inner)
        .width(Length::Fixed(sw))
        .height(Length::Fixed(sh))
        .style(move |theme| {
            let pal = PaletteTokens::for_theme(theme);
            let bg = if frosted {
                pal.bg_elevated.scale_alpha(0.92)
            } else {
                pal.bg_elevated.scale_alpha(0.98)
            };
            container::Style {
                background: Some(bg.into()),
                text_color: Some(pal.text_primary),
                border: Border {
                    color: if is_sel || is_dragging {
                        pal.accent
                    } else if is_review {
                        pal.border
                    } else {
                        pal.border_strong
                    },
                    width: if is_sel || is_dragging { 2.0 } else { 1.0 },
                    radius: 8.0.into(),
                },
                ..Default::default()
            }
        });

    Some(
        container(bubble_box)
            .padding(Padding {
                top: sy.max(0.0),
                right: 0.0,
                bottom: 0.0,
                left: sx.max(0.0),
            })
            .width(Fill)
            .height(Fill)
            .into(),
    )
}

fn build_viewer_translate_overlay<'a>(
    viewport: Element<'a, ViewerMessage>,
    state: ViewerViewState<'a>,
    lang: Language,
) -> Element<'a, ViewerMessage> {
    let tr_state = state.translate;
    let is_single = state.viewer_mode == ViewerViewMode::Single;
    let is_double = state.viewer_mode == ViewerViewMode::Double;
    let is_review = tr_state.mode == ViewerTranslateMode::Review;

    let mut stack_layers: Vec<Element<'a, ViewerMessage>> = vec![viewport];

    // 1. Full-box covering translated/OCR bubbles (Single & Double modes; Webtoon renders per-page inside scroll)
    if tr_state.show_bubbles
        && is_single
        && !tr_state.blocks.is_empty()
        && let Some(frame) = state.viewer_frame
    {
        let doc_w = frame.width as f32;
        let doc_h = frame.height as f32;
        let qturns = state.viewer_quarter_turns;
        let zoom = state.zoom;
        let pan_x = state.pan_x;
        let pan_y = state.pan_y;
        let tr_owned = tr_state.clone();

        let bubble_layer: Element<'a, ViewerMessage> = responsive(move |bounds| {
            let (left, top, drawn_w, drawn_h, fw, fh) = viewer_fit_geometry(
                doc_w,
                doc_h,
                qturns,
                zoom,
                pan_x,
                pan_y,
                bounds.width,
                bounds.height,
            );
            let mut items: Vec<Element<'_, ViewerMessage>> =
                Vec::with_capacity(tr_owned.blocks.len() + 1);
            items.push(
                iced::widget::space::horizontal()
                    .width(Fill)
                    .height(Fill)
                    .into(),
            );
            for (idx, b) in tr_owned.blocks.iter().enumerate() {
                let (rx, ry, rw, rh) = transform_block_for_quarter_turns(b, doc_w, doc_h, qturns);
                let (dx_px, dy_px) = match tr_owned.active_drag_delta {
                    Some((drag_idx, d)) if drag_idx == idx => (d[0], d[1]),
                    _ => (0.0, 0.0),
                };
                let sx = (left + (rx / fw) * drawn_w + dx_px).max(0.0);
                let sy = (top + (ry / fh) * drawn_h + dy_px).max(0.0);
                let min_w = if is_review { 48.0 } else { 96.0 };
                let min_h = if is_review { 28.0 } else { 44.0 };
                let sw = ((rw / fw) * drawn_w).max(min_w);
                let sh = ((rh / fh) * drawn_h).max(min_h);
                if let Some(el) =
                    render_translate_bubble_item(&tr_owned, lang, None, idx, b, sx, sy, sw, sh)
                {
                    items.push(el);
                }
            }
            stack(items).width(Fill).height(Fill).into()
        })
        .into();
        stack_layers.push(bubble_layer);
    } else if tr_state.show_bubbles
        && is_double
        && let Some(left_frame) = state.viewer_frame
    {
        let (left_slot, right_slot) = dual_spread_slots(state);
        let left_path = left_slot.map(|(_, p)| p.to_owned());
        let right_path = right_slot.map(|(_, p)| p.to_owned());
        let left_blocks: Vec<TranslationBlock> = left_path
            .as_deref()
            .and_then(|p| tr_state.bubble_cache.get(p).cloned())
            .or_else(|| {
                (left_path.as_deref() == state.snapshot.meta.viewer_path.as_deref())
                    .then(|| tr_state.blocks.clone())
            })
            .unwrap_or_default();
        let right_blocks: Vec<TranslationBlock> = right_path
            .as_deref()
            .and_then(|p| tr_state.bubble_cache.get(p).cloned())
            .unwrap_or_default();

        if !left_blocks.is_empty() || !right_blocks.is_empty() {
            let left_size = (left_frame.width, left_frame.height);
            let right_size = state.viewer_pair_frame.map(|f| (f.width, f.height));
            let gap = f32::from(state.viewer_manga_page_gap);
            let qturns = state.viewer_quarter_turns;
            let zoom = state.zoom;
            let pan_x = state.pan_x;
            let pan_y = state.pan_y;
            let tr_owned = tr_state.clone();

            let bubble_layer: Element<'a, ViewerMessage> = responsive(move |bounds| {
                let geom = viewer_double_fit_geometries(
                    left_size,
                    right_size,
                    gap,
                    qturns,
                    zoom,
                    pan_x,
                    pan_y,
                    bounds.width,
                    bounds.height,
                );
                let mut items: Vec<Element<'_, ViewerMessage>> =
                    Vec::with_capacity(left_blocks.len() + right_blocks.len() + 1);
                items.push(
                    iced::widget::space::horizontal()
                        .width(Fill)
                        .height(Fill)
                        .into(),
                );
                let min_w = if is_review { 48.0 } else { 96.0 };
                let min_h = if is_review { 28.0 } else { 44.0 };
                if let Some(ref lp) = left_path {
                    for (idx, b) in left_blocks.iter().enumerate() {
                        let (mut sx, mut sy, sw_raw, sh_raw) =
                            transform_double_page_block_to_screen(
                                b,
                                left_size.0 as f32,
                                left_size.1 as f32,
                                false,
                                geom,
                                qturns,
                            );
                        if let Some((drag_idx, d)) = tr_owned.active_drag_delta
                            && drag_idx == idx
                            && tr_owned.active_drag_page.as_deref() == Some(lp.as_str())
                        {
                            sx += d[0];
                            sy += d[1];
                        }
                        if let Some(el) = render_translate_bubble_item(
                            &tr_owned,
                            lang,
                            Some(lp.as_str()),
                            idx,
                            b,
                            sx,
                            sy,
                            sw_raw.max(min_w),
                            sh_raw.max(min_h),
                        ) {
                            items.push(el);
                        }
                    }
                }
                if let (Some(rp), Some(r_sz)) = (&right_path, right_size) {
                    for (idx, b) in right_blocks.iter().enumerate() {
                        let (mut sx, mut sy, sw_raw, sh_raw) =
                            transform_double_page_block_to_screen(
                                b,
                                r_sz.0 as f32,
                                r_sz.1 as f32,
                                true,
                                geom,
                                qturns,
                            );
                        if let Some((drag_idx, d)) = tr_owned.active_drag_delta
                            && drag_idx == idx
                            && tr_owned.active_drag_page.as_deref() == Some(rp.as_str())
                        {
                            sx += d[0];
                            sy += d[1];
                        }
                        if let Some(el) = render_translate_bubble_item(
                            &tr_owned,
                            lang,
                            Some(rp.as_str()),
                            idx,
                            b,
                            sx,
                            sy,
                            sw_raw.max(min_w),
                            sh_raw.max(min_h),
                        ) {
                            items.push(el);
                        }
                    }
                }
                stack(items).width(Fill).height(Fill).into()
            })
            .into();
            stack_layers.push(bubble_layer);
        }
    }

    // 2. Compact Review Pill when in Review mode (hidden while Space-panning)
    if !tr_state.space_pan_held && is_review {
        let review_pill = container(
            row![
                button::labeled(
                    "A−",
                    None,
                    ButtonVariant::Ghost,
                    Some(ViewerMessage::SetTranslateFontScale(
                        (tr_state.font_scale - 0.1).clamp(0.7, 2.0),
                    )),
                ),
                button::labeled(
                    "A+",
                    None,
                    ButtonVariant::Ghost,
                    Some(ViewerMessage::SetTranslateFontScale(
                        (tr_state.font_scale + 0.1).clamp(0.7, 2.0),
                    )),
                ),
                button::labeled(
                    tr(lang, "action-edit"),
                    None,
                    ButtonVariant::Secondary,
                    Some(ViewerMessage::SetTranslateMode(
                        ViewerTranslateMode::Translate,
                    )),
                ),
                button::compact_icon(
                    Icon::Close,
                    ButtonVariant::Ghost,
                    Some(ViewerMessage::CloseTranslateOverlay),
                ),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .padding([6, 10])
        .style(popover_style);

        stack_layers.push(
            container(review_pill)
                .width(Fill)
                .height(Fill)
                .align_x(iced::alignment::Horizontal::Right)
                .align_y(iced::alignment::Vertical::Top)
                .padding(12.0)
                .into(),
        );
    }

    stack(stack_layers).width(Fill).height(Fill).into()
}

/// Filter `meta.quick_actions` to available Quick Actions targeting `"viewer"`.
pub fn viewer_quick_actions<'a>(
    meta: &'a photon_protocol::UiStateMeta,
) -> Vec<&'a photon_protocol::AiQuickAction> {
    meta.quick_actions
        .iter()
        .filter(|qa| {
            qa.available
                && (qa.workspaces.is_empty() || qa.workspaces.iter().any(|w| w == "viewer"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_dual_spreads_respects_separate_cover_and_landscape() {
        let imgs = vec![
            "/tmp/01.png".to_owned(),
            "/tmp/02.png".to_owned(),
            "/tmp/03.png".to_owned(),
            "/tmp/04.png".to_owned(),
            "/tmp/05.png".to_owned(),
        ];
        let mut sizes = HashMap::new();
        sizes.insert("/tmp/01.png".to_owned(), (800, 1200));
        sizes.insert("/tmp/02.png".to_owned(), (800, 1200));
        sizes.insert("/tmp/03.png".to_owned(), (800, 1200));
        sizes.insert("/tmp/04.png".to_owned(), (1600, 900));
        sizes.insert("/tmp/05.png".to_owned(), (800, 1200));

        let spreads = compute_dual_spreads(&imgs, &sizes, true);
        assert_eq!(spreads, vec![vec![0], vec![1, 2], vec![3], vec![4]]);

        let (left, right) = dual_spread_slots_for(&imgs, &sizes, Some(1), true, true);
        assert_eq!(left, Some((2, "/tmp/03.png")));
        assert_eq!(right, Some((1, "/tmp/02.png")));
    }

    #[test]
    fn transform_block_rotates_coordinates_across_all_quarter_turns() {
        let block = TranslationBlock {
            x: 100,
            y: 50,
            w: 200,
            h: 80,
            original: "hi".to_owned(),
            translated: "chào".to_owned(),
        };
        assert_eq!(
            transform_block_for_quarter_turns(&block, 1000.0, 800.0, 0),
            (100.0, 50.0, 200.0, 80.0)
        );
        assert_eq!(
            transform_block_for_quarter_turns(&block, 1000.0, 800.0, 1),
            (670.0, 100.0, 80.0, 200.0)
        );
        assert_eq!(
            transform_block_for_quarter_turns(&block, 1000.0, 800.0, 2),
            (700.0, 670.0, 200.0, 80.0)
        );
        assert_eq!(
            transform_block_for_quarter_turns(&block, 1000.0, 800.0, 3),
            (50.0, 700.0, 80.0, 200.0)
        );
    }

    #[test]
    fn compute_bubble_font_size_scales_and_clamps() {
        let small = compute_bubble_font_size(60.0, 36.0, 0.0, 40, 1.0);
        let large = compute_bubble_font_size(240.0, 160.0, 22.0, 10, 1.0);
        let scaled = compute_bubble_font_size(240.0, 160.0, 22.0, 10, 1.5);
        assert!((9.0..=28.0).contains(&small));
        assert!(large > small);
        assert!(scaled >= large);
        assert!(scaled <= 28.0);
    }

    #[test]
    fn double_spread_geometry_maps_left_and_right_pages_across_rotations() {
        let geom = viewer_double_fit_geometries(
            (800, 1000),
            Some((1200, 1500)),
            10.0,
            0,
            1.0,
            0.0,
            0.0,
            1610.0,
            750.0,
        );
        // Normalized target_h = 1500; left_disp_w = 800 * 1.5 = 1200, right_disp_w = 1200.
        assert_eq!(geom.spread_h, 1500.0);
        assert_eq!(geom.left_disp_w, 1200.0);
        assert!((geom.right_disp_w - 1200.0).abs() < 1e-3);
        let left_block = TranslationBlock {
            x: 100,
            y: 200,
            w: 200,
            h: 100,
            original: String::new(),
            translated: String::new(),
        };
        let right_block = TranslationBlock {
            x: 150,
            y: 450,
            w: 300,
            h: 150,
            original: String::new(),
            translated: String::new(),
        };
        let (lx, ly, lw, lh) =
            transform_double_page_block_to_screen(&left_block, 800.0, 1000.0, false, geom, 0);
        let (rx, ry, rw, rh) =
            transform_double_page_block_to_screen(&right_block, 1200.0, 1500.0, true, geom, 0);
        assert!(rx > lx + lw);
        assert!(lw > 0.0 && lh > 0.0 && rw > 0.0 && rh > 0.0);
        assert!(ry > ly);
    }

    #[test]
    fn standalone_vs_main_toolbar_switcher_uses_two_button_pill_for_standalone() {
        assert_eq!(
            viewer_toolbar_switcher_kind(true, 1280.0),
            ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: true }
        );
        assert_eq!(
            viewer_toolbar_switcher_kind(true, 900.0),
            ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: false }
        );
        assert_eq!(
            viewer_toolbar_switcher_kind(false, 1120.0),
            ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: true }
        );
        assert_eq!(
            viewer_toolbar_switcher_kind(false, 1119.0),
            ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: false }
        );
    }

    #[test]
    fn utility_toggles_stay_expanded_at_all_widths() {
        assert!(!viewer_utility_collapsed(VIEWER_UTILITY_COLLAPSE_WIDTH));
        assert!(!viewer_utility_collapsed(1280.0));
        assert!(!viewer_utility_collapsed(
            VIEWER_UTILITY_COLLAPSE_WIDTH - 1.0
        ));
        assert!(!viewer_utility_collapsed(680.0));
    }

    #[test]
    fn collapsed_utilities_menu_anchors_left_of_view_mode_group() {
        // At narrow widths: utilities(44) + divider gap(17) + switcher(72)
        // + spacing(8) + reader options(36) + spacing(8).
        assert_eq!(viewer_utilities_menu_right_offset(680.0), 185.0);
        assert!(viewer_utilities_menu_right_offset(680.0) > 0.0);
    }
}
