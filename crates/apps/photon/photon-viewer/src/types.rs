use std::collections::HashMap;

use fw_panels::SidebarOrderState;
use iced::widget::{image as iced_image, scrollable};
use photon_canvas::{CanvasEvent, CpuImageFrame};
use photon_protocol::{
    CanvasAntiAliasingMode, ColorAdjustments, Command, TranslateProvider, TranslationBlock,
    UiStateSnapshot, ViewerRightPanel, ViewerViewMode, ViewportDenoiseMode, WebtoonFitMode,
};

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
    EditPath(String),
    ShowInBrowser(String),
    OpenWith(String),
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
    ToggleContextZoomSubmenu,
    OpenContextZoomSubmenu,
    ToggleContextPluginGroup(String),
    OpenContextPluginGroup(String),
    ConfirmDeletePermanently(Vec<String>),
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
    RunPlugin(String),
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
