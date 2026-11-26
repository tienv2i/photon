//! Viewer workspace UI (`iced 0.14` + `photon-ui` + `photon-panels`):
//! Single, Double-page spread (LTR/RTL + cover split), and continuous Manhwa/Webtoon strip,
//! unified resizable/reorderable right sidebar (`ViewAdjustment` & `Info`), and bottom Filmstrip bar.

use std::collections::HashMap;
use std::path::Path;

use iced::widget::{
    Shader, button as iced_button, column, container, image as iced_image, mouse_area, responsive,
    row, rule, scrollable, slider, stack, text, text_input,
};
use iced::{Alignment, Border, ContentFit, Element, Fill, Length, Padding};
use photon_i18n::tr;
use photon_panels::{
    DraggablePanelHeaderSpec, PanelFrameSpec, PanelSplitterEdge, SidebarOrderState, kv_row,
    panel_splitter, sidebar_dock, sidebar_panel, stacked_panel_heights, vertical_panel_splitter,
};
use photon_protocol::{
    CanvasAntiAliasingMode, ColorAdjustments, Command, Language, Theme, TranslationBlock,
    UiStateSnapshot, ViewerRightPanel, ViewerViewMode, ViewportDenoiseMode, WebtoonFitMode,
    Workspace,
};
use photon_ui::button::{self, ButtonVariant, TOOLBAR_CONTROL_HEIGHT};
use photon_ui::grid::{FilmstripItem, filmstrip_bar};
use photon_ui::icons::Icon;
use photon_ui::menu::{MenuEntry, MenuItem, menu_panel_entries};
use photon_ui::theme::{
    PaletteTokens, RADIUS_SM, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, canvas_backdrop_style,
    hairline_rule_style, input_style, popover_style, slider_style, surface_panel_style,
};
use photon_ui::toolbar::{
    WorkspaceSwitcherMode, WorkspaceTab, toolbar, toolbar_button_group, workspace_switcher_mode,
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
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tesseract => "tesseract",
            Self::MangaOcr => "manga_ocr",
            Self::RapidOcr => "rapidocr",
        }
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
    pub font_scale: f32,
    pub editing_block_idx: Option<usize>,
    pub blocks: Vec<TranslationBlock>,
    pub saved_sidecar: bool,
    pub last_path: Option<String>,
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
            font_scale: 1.0,
            editing_block_idx: None,
            blocks: Vec::new(),
            saved_sidecar: false,
            last_path: None,
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
    SetViewerAdjExposure(f32),
    SetViewerAdjGamma(f32),
    SetViewerAdjBrightness(f32),
    SetViewerAdjContrast(f32),
    SetViewerAdjSaturation(f32),
    SetViewerAdjHighlights(f32),
    SetViewerAdjShadows(f32),
    ResetViewerAdj,
    ViewerAdjOpenInEditor,
    WebtoonScrolled(scrollable::Viewport),
    OpenInNewViewerWindow(String),
    ToggleTranslateOverlay,
    CloseTranslateOverlay,
    SetTranslateMode(ViewerTranslateMode),
    SetTranslateOcrEngine(ViewerOcrEngine),
    SetTranslateSourceLang(String),
    SetTranslateTargetLang(String),
    SwapTranslateLangs,
    StartTranslateRegionPick,
    ScanTranslatePage,
    ToggleTranslateBubbles,
    SetTranslateFontScale(f32),
    SelectTranslateBlock(Option<usize>),
    UpdateTranslateBlockText(usize, String),
    MoveTranslateBlock(usize, i32, i32),
    RetryTranslateBlock(usize),
    RemoveTranslateBlock(usize),
    SaveTranslationSidecar,
    DeleteTranslationSidecar,
    CopyTranslatedText,
    OpenTranslateSettings,
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

pub fn viewer_reader_options_right_offset(state: ViewerViewState<'_>) -> f32 {
    let utilities_w = 12.0 + 32.0; // right padding + single Panels&Settings button
    if !state.is_main_window {
        return utilities_w + 4.0;
    }
    let switcher_w = match workspace_switcher_mode(state.window_size.width) {
        WorkspaceSwitcherMode::Full => 252.0,
        WorkspaceSwitcherMode::CompactIcons => 112.0,
        WorkspaceSwitcherMode::Dropdown => 56.0,
    };
    // utilities_w + gap(3) + switcher_w + gap(3)
    utilities_w + 3.0 + switcher_w + 3.0
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

    left = left.push(button::toolbar_icon(
        if state.is_fullscreen {
            Icon::FullscreenExit
        } else {
            Icon::Fullscreen
        },
        state.is_fullscreen,
        Some(ViewerMessage::ToggleFullscreen),
        Some(tr(lang, "tooltip-viewer-fullscreen")),
    ));

    left = left.push(button::toolbar_icon(
        Icon::Translate,
        state.translate.active,
        has_image.then_some(ViewerMessage::ToggleTranslateOverlay),
        Some(tr(lang, "menu-viewer-read-translate")),
    ));

    // Right side: View Mode Switcher (Single | Double | Webtoon | Reader Options) + Workspace Switcher + Panels & Settings
    let view_mode_group = toolbar_button_group(vec![
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
        button::toolbar_icon(
            Icon::Sliders,
            state.reader_options_open,
            Some(ViewerMessage::ToggleReaderOptions),
            Some(tr(lang, "action-reader-options")),
        ),
    ]);

    let right: Element<'a, ViewerMessage> = if state.is_main_window {
        let switcher = workspace_switcher_responsive(
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
            workspace_switcher_mode(state.window_size.width),
            state.workspace_switcher_open,
            Some(ViewerMessage::ToggleWorkspaceSwitcher),
        );
        row![view_mode_group, switcher, right_utilities]
            .spacing(3)
            .align_y(Alignment::Center)
            .into()
    } else {
        row![view_mode_group, right_utilities]
            .spacing(3)
            .align_y(Alignment::Center)
            .into()
    };

    toolbar(left, right)
}

/// Main Viewer workspace view: Single/Double/Manhwa viewport,
/// unified resizable/reorderable right sidebar (`ViewAdjustment` & `Info`), and bottom Filmstrip bar.
pub fn build_viewer_view<'a>(state: ViewerViewState<'a>) -> Element<'a, ViewerMessage> {
    let lang = state.snapshot.meta.language;
    let dark = state.snapshot.meta.theme == Theme::Dark;
    let viewer_mode = state.viewer_mode;

    let canvas_tool = if viewer_mode == ViewerViewMode::Single
        && state.translate.active
        && state.translate.picking_region
    {
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
        ViewerViewMode::Double => build_viewer_double_panes(state, lang, dark),
        ViewerViewMode::Webtoon => build_viewer_webtoon_strip(state),
    };

    let viewport: Element<'_, ViewerMessage> = if state.translate.active {
        build_viewer_translate_overlay(viewport, state, lang)
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
        let filmstrip_row = container(
            row![
                container(filmstrip_bar(items)).width(Fill),
                container(close_btn).padding([4, 6]),
            ]
            .align_y(Alignment::Center)
            .width(Fill),
        )
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
    let can_swap = open_panels.len() >= 2;
    let (top_h, bottom_h) = stacked_panel_heights(
        false,
        false,
        VIEWER_SPLIT_DEFAULT_PCT,
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

        let slot_height = if can_swap {
            if idx == 0 { top_h } else { bottom_h }
        } else {
            Length::Fill
        };

        stacked = stacked.push(container(panel_el).width(Fill).height(slot_height));

        if idx == 0 && can_swap {
            let height_hint = (state.window_size.height - 120.0).max(260.0);
            stacked = stacked.push(vertical_panel_splitter(
                VIEWER_SPLIT_DEFAULT_PCT,
                height_hint,
                VIEWER_SPLIT_MIN_PCT,
                VIEWER_SPLIT_MAX_PCT,
                VIEWER_SPLIT_DEFAULT_PCT,
                |_| ViewerMessage::Noop,
                |_| ViewerMessage::Noop,
            ));
        }
    }

    sidebar_dock(right_w, stacked)
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
    column![
        kv_row(label, format!("{value:.2}")),
        slider(range, value, on_change)
            .step(step)
            .style(slider_style),
    ]
    .spacing(3)
    .into()
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
        scrollable(sliders).height(Fill),
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
            tool: CanvasTool::Pointer,
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
        let wrapped_page =
            mouse_area(page_el).on_right_press(ViewerMessage::OpenViewerContextMenu {
                path: Some(path),
                pos: None,
            });
        col = col.push(wrapped_page);
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
) -> Element<'a, ViewerMessage> {
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

    let mut entries = entries;
    if let Some(target_path) = resolved_path {
        let qas = viewer_quick_actions(&state.snapshot.meta);
        if !qas.is_empty() {
            entries.push(MenuEntry::Separator);
            entries.push(MenuEntry::Header(tr(lang, "menu-quick-actions")));
            for qa in qas {
                entries.push(MenuEntry::Item(
                    MenuItem::new(
                        qa.label.clone(),
                        ViewerMessage::ContextMenuDispatch(Command::QuickActionRun {
                            plugin_id: qa.plugin_id.clone(),
                            action_id: qa.id.clone(),
                            paths: vec![target_path.to_owned()],
                        }),
                    )
                    .icon(Icon::Plug),
                ));
            }
        }
    }

    menu_panel_entries(entries, 225.0)
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

fn build_viewer_translate_overlay<'a>(
    viewport: Element<'a, ViewerMessage>,
    state: ViewerViewState<'a>,
    lang: Language,
) -> Element<'a, ViewerMessage> {
    let tr_state = state.translate;
    let has_path = state.snapshot.meta.viewer_path.is_some();
    let is_single = state.viewer_mode == ViewerViewMode::Single;
    let running_stage = state
        .snapshot
        .meta
        .ai_task
        .as_ref()
        .filter(|t| t.running)
        .map(|t| (t.percent, t.stage.clone()));

    let mut stack_layers: Vec<Element<'a, ViewerMessage>> = vec![viewport];

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
        let font_size = (12.0 * tr_state.font_scale).clamp(8.0, 26.0);
        let ocr_only = tr_state.mode == ViewerTranslateMode::OcrOnly;
        let blocks = tr_state.blocks.clone();
        let selected_idx = tr_state.editing_block_idx;

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
            let mut items: Vec<Element<'_, ViewerMessage>> = Vec::with_capacity(blocks.len() + 1);
            items.push(
                iced::widget::space::horizontal()
                    .width(Fill)
                    .height(Fill)
                    .into(),
            );
            for (idx, b) in blocks.iter().enumerate() {
                let display_text = if ocr_only || b.translated.trim().is_empty() {
                    b.original.clone()
                } else {
                    b.translated.clone()
                };
                if display_text.trim().is_empty() {
                    continue;
                }
                let sx =
                    (left + (b.x as f32 / fw) * drawn_w).clamp(4.0, (bounds.width - 90.0).max(4.0));
                let sy = (top + (b.y as f32 / fh) * drawn_h)
                    .clamp(52.0, (bounds.height - 40.0).max(52.0));
                let bw = ((b.w as f32 / fw) * drawn_w).clamp(88.0, 320.0);
                let is_sel = selected_idx == Some(idx);

                let bubble_btn = iced_button(
                    container(text(display_text).size(font_size))
                        .padding([4, 6])
                        .max_width(bw)
                        .style(move |theme| {
                            let pal = PaletteTokens::for_theme(theme);
                            container::Style {
                                background: Some(pal.bg_elevated.into()),
                                text_color: Some(pal.text_primary),
                                border: Border {
                                    color: if is_sel {
                                        pal.accent
                                    } else {
                                        pal.border_strong
                                    },
                                    width: if is_sel { 2.0 } else { 1.0 },
                                    radius: RADIUS_SM.into(),
                                },
                                ..Default::default()
                            }
                        }),
                )
                .padding(0)
                .style(|_, _| iced_button::Style::default())
                .on_press(ViewerMessage::SelectTranslateBlock(Some(idx)));

                items.push(
                    container(bubble_btn)
                        .padding(Padding {
                            top: sy,
                            right: 0.0,
                            bottom: 0.0,
                            left: sx,
                        })
                        .width(Fill)
                        .height(Fill)
                        .into(),
                );
            }
            stack(items).width(Fill).height(Fill).into()
        })
        .into();
        stack_layers.push(bubble_layer);
    }

    let mode_row = row![
        button::labeled(
            tr(lang, "translate-mode-translate"),
            None,
            ButtonVariant::Tab {
                active: tr_state.mode == ViewerTranslateMode::Translate,
            },
            Some(ViewerMessage::SetTranslateMode(
                ViewerTranslateMode::Translate
            )),
        ),
        button::labeled(
            tr(lang, "translate-mode-ocr"),
            None,
            ButtonVariant::Tab {
                active: tr_state.mode == ViewerTranslateMode::OcrOnly,
            },
            Some(ViewerMessage::SetTranslateMode(
                ViewerTranslateMode::OcrOnly
            )),
        ),
        button::labeled(
            tr(lang, "translate-mode-pan"),
            None,
            ButtonVariant::Tab {
                active: tr_state.mode == ViewerTranslateMode::Pan,
            },
            Some(ViewerMessage::SetTranslateMode(ViewerTranslateMode::Pan)),
        ),
        button::labeled(
            tr(lang, "translate-mode-review"),
            None,
            ButtonVariant::Tab {
                active: tr_state.mode == ViewerTranslateMode::Review,
            },
            Some(ViewerMessage::SetTranslateMode(ViewerTranslateMode::Review)),
        ),
    ]
    .spacing(3);

    let engine_row = row![
        text(format!("{}:", tr(lang, "translate-engine-label"))).size(TEXT_SIZE_CAPTION),
        button::labeled(
            tr(lang, "translate-engine-tesseract"),
            None,
            ButtonVariant::Tab {
                active: tr_state.ocr_engine == ViewerOcrEngine::Tesseract,
            },
            Some(ViewerMessage::SetTranslateOcrEngine(
                ViewerOcrEngine::Tesseract
            )),
        ),
        button::labeled(
            tr(lang, "translate-engine-manga-ocr"),
            None,
            ButtonVariant::Tab {
                active: tr_state.ocr_engine == ViewerOcrEngine::MangaOcr,
            },
            Some(ViewerMessage::SetTranslateOcrEngine(
                ViewerOcrEngine::MangaOcr
            )),
        ),
        button::labeled(
            tr(lang, "translate-engine-rapidocr"),
            None,
            ButtonVariant::Tab {
                active: tr_state.ocr_engine == ViewerOcrEngine::RapidOcr,
            },
            Some(ViewerMessage::SetTranslateOcrEngine(
                ViewerOcrEngine::RapidOcr
            )),
        ),
    ]
    .spacing(3)
    .align_y(Alignment::Center);

    let lang_row = row![
        text_input("eng", &tr_state.source_lang)
            .on_input(ViewerMessage::SetTranslateSourceLang)
            .size(TEXT_SIZE_CAPTION)
            .padding([4, 6])
            .style(input_style)
            .width(Length::Fixed(64.0)),
        button::compact_icon(
            Icon::Swap,
            ButtonVariant::Ghost,
            Some(ViewerMessage::SwapTranslateLangs),
        ),
        text_input("vi", &tr_state.target_lang)
            .on_input(ViewerMessage::SetTranslateTargetLang)
            .size(TEXT_SIZE_CAPTION)
            .padding([4, 6])
            .style(input_style)
            .width(Length::Fixed(52.0)),
    ]
    .spacing(3)
    .align_y(Alignment::Center);

    let action_row = row![
        button::labeled(
            tr(lang, "translate-scan-page"),
            Some(Icon::Translate),
            ButtonVariant::Primary,
            has_path.then_some(ViewerMessage::ScanTranslatePage),
        ),
        button::labeled(
            tr(lang, "translate-drag-region"),
            Some(Icon::Crop),
            ButtonVariant::Tab {
                active: tr_state.picking_region,
            },
            (has_path && is_single).then_some(ViewerMessage::StartTranslateRegionPick),
        ),
        button::labeled(
            tr(lang, "translate-show-bubbles"),
            None,
            ButtonVariant::Tab {
                active: tr_state.show_bubbles,
            },
            Some(ViewerMessage::ToggleTranslateBubbles),
        ),
        text(tr(lang, "translate-font-scale")).size(TEXT_SIZE_CAPTION),
        button::labeled(
            "-",
            None,
            ButtonVariant::Secondary,
            Some(ViewerMessage::SetTranslateFontScale(
                (tr_state.font_scale - 0.1).clamp(0.7, 2.0),
            )),
        ),
        text(format!("{:.1}x", tr_state.font_scale)).size(TEXT_SIZE_CAPTION),
        button::labeled(
            "+",
            None,
            ButtonVariant::Secondary,
            Some(ViewerMessage::SetTranslateFontScale(
                (tr_state.font_scale + 0.1).clamp(0.7, 2.0),
            )),
        ),
        button::labeled(
            tr(lang, "translate-save-sidecar"),
            Some(Icon::Save),
            ButtonVariant::Secondary,
            (has_path && !tr_state.blocks.is_empty())
                .then_some(ViewerMessage::SaveTranslationSidecar),
        ),
        button::labeled(
            tr(lang, "translate-delete-sidecar"),
            Some(Icon::Trash),
            ButtonVariant::Ghost,
            (has_path && tr_state.saved_sidecar).then_some(ViewerMessage::DeleteTranslationSidecar),
        ),
        button::labeled(
            tr(lang, "action-copy"),
            Some(Icon::Copy),
            ButtonVariant::Ghost,
            (!tr_state.blocks.is_empty()).then_some(ViewerMessage::CopyTranslatedText),
        ),
        button::compact_icon_tipped(
            Icon::Settings,
            ButtonVariant::Ghost,
            Some(ViewerMessage::OpenTranslateSettings),
            tr(lang, "action-translate-settings"),
        ),
        button::compact_icon(
            Icon::Close,
            ButtonVariant::Ghost,
            Some(ViewerMessage::CloseTranslateOverlay),
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let mut top_bar_col = column![
        row![
            mode_row,
            iced::widget::space::horizontal(),
            engine_row,
            lang_row
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        action_row,
    ]
    .spacing(4);

    if let Some((pct, stage)) = running_stage {
        top_bar_col = top_bar_col.push(
            text(format!("{pct}% · {stage}"))
                .size(TEXT_SIZE_CAPTION)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).accent),
                }),
        );
    } else if tr_state.saved_sidecar {
        top_bar_col = top_bar_col.push(
            text(tr(lang, "translate-saved-badge"))
                .size(TEXT_SIZE_CAPTION)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).text_muted),
                }),
        );
    }

    let top_bar_card = container(top_bar_col).padding([6, 10]).style(popover_style);

    let mut hud_col = column![top_bar_card].spacing(6).width(Fill);

    if tr_state.mode == ViewerTranslateMode::Review || tr_state.editing_block_idx.is_some() {
        let mut review_col = column![].spacing(6);
        if tr_state.blocks.is_empty() {
            review_col = review_col.push(
                text(tr(lang, "translate-empty-hint"))
                    .size(TEXT_SIZE_CAPTION)
                    .style(|theme| text::Style {
                        color: Some(PaletteTokens::for_theme(theme).text_muted),
                    }),
            );
        } else {
            let indices: Vec<usize> = if tr_state.mode == ViewerTranslateMode::Review {
                (0..tr_state.blocks.len()).collect()
            } else if let Some(idx) = tr_state
                .editing_block_idx
                .filter(|&i| i < tr_state.blocks.len())
            {
                vec![idx]
            } else {
                Vec::new()
            };
            for idx in indices {
                let b = &tr_state.blocks[idx];
                let val = if tr_state.mode == ViewerTranslateMode::OcrOnly
                    || b.translated.trim().is_empty()
                {
                    b.original.clone()
                } else {
                    b.translated.clone()
                };
                review_col = review_col.push(
                    column![
                        row![
                            text(format!("#{} ({}, {})", idx + 1, b.x, b.y))
                                .size(TEXT_SIZE_CAPTION),
                            iced::widget::space::horizontal(),
                            button::labeled(
                                "←",
                                None,
                                ButtonVariant::Ghost,
                                Some(ViewerMessage::MoveTranslateBlock(idx, -16, 0)),
                            ),
                            button::labeled(
                                "↑",
                                None,
                                ButtonVariant::Ghost,
                                Some(ViewerMessage::MoveTranslateBlock(idx, 0, -16)),
                            ),
                            button::labeled(
                                "↓",
                                None,
                                ButtonVariant::Ghost,
                                Some(ViewerMessage::MoveTranslateBlock(idx, 0, 16)),
                            ),
                            button::labeled(
                                "→",
                                None,
                                ButtonVariant::Ghost,
                                Some(ViewerMessage::MoveTranslateBlock(idx, 16, 0)),
                            ),
                            button::labeled(
                                tr(lang, "translate-retry-bubble"),
                                Some(Icon::Refresh),
                                ButtonVariant::Secondary,
                                Some(ViewerMessage::RetryTranslateBlock(idx)),
                            ),
                            button::labeled(
                                tr(lang, "translate-remove-bubble"),
                                Some(Icon::Trash),
                                ButtonVariant::Danger,
                                Some(ViewerMessage::RemoveTranslateBlock(idx)),
                            ),
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center),
                        text_input(&tr(lang, "translate-text-placeholder"), &val)
                            .on_input(move |s| ViewerMessage::UpdateTranslateBlockText(idx, s))
                            .size(TEXT_SIZE_CONTROL)
                            .padding([5, 8])
                            .style(input_style),
                    ]
                    .spacing(4),
                );
            }
        }
        let review_card = container(scrollable(review_col).height(Length::Shrink))
            .padding([8, 10])
            .max_width(480.0)
            .max_height(240.0)
            .style(popover_style);
        hud_col = hud_col.push(review_card);
    }

    stack_layers.push(
        container(hud_col)
            .padding(Padding::from(10.0))
            .width(Fill)
            .into(),
    );

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
}
