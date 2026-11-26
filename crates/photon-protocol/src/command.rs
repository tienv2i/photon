//! Command envelope, typed commands, queries, events, UI text, and errors.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AiDevice, AiMaskStroke, ApplyTarget, BatchOperation, BatchOutput, BatchSource, BlendMode,
    BrushSettings, CanvasAnchor, ClientKind, DrawShapeKind, GradientColorMode, GradientKind,
    GradientRepeat, GradientStop, HealingMode, Language, LayerEffect, LayerTransformSpec,
    LayoutState, LiquifyMode, LiveStrokeKind, MoveDirection, PROTOCOL_VERSION, PluginParamValue,
    ResizeMode, SelectionBooleanOp, SelectionModifyOp, SelectionShape, ShapeFillMode, Theme,
    TranslateSettings, TranslationBlock, VectorPathNode, Workspace, default_brush_hardness,
    default_half_f32, default_one_f32, default_opaque_alpha,
};

fn default_protocol() -> u32 {
    PROTOCOL_VERSION
}

fn default_true() -> bool {
    true
}

fn default_white_rgba() -> [u8; 4] {
    [255, 255, 255, 255]
}

fn default_ai_auto_unload_secs() -> u32 {
    60
}

fn default_rembg_model_id() -> String {
    "rmbg-1.4".to_owned()
}

fn default_gemini_size() -> String {
    "auto".to_owned()
}

fn default_gemini_logo_value() -> f32 {
    255.0
}

fn default_ai_detector_model() -> String {
    "watermark-yolo26".to_owned()
}

fn default_ai_inpainter_model() -> String {
    "migan-pipeline-v2".to_owned()
}

fn default_ai_inpaint_confidence() -> f32 {
    0.45
}

fn default_ai_inpaint_padding() -> u32 {
    8
}

fn default_region_dim() -> i32 {
    64
}

fn default_refine_softness() -> f32 {
    0.30
}

fn default_caption_model_id() -> String {
    "wd-vit-tagger-v3".to_owned()
}

fn default_caption_general_threshold() -> f32 {
    0.35
}

fn default_caption_character_threshold() -> f32 {
    0.85
}

fn default_ocr_engine() -> String {
    "tesseract".to_owned()
}

/// A command envelope. `id` is echoed in the response, recorded in the journal,
/// and propagated into `tracing` spans for end-to-end `trace --request-id`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Envelope {
    pub id: String,
    #[serde(default = "default_protocol")]
    pub protocol: u32,
    #[serde(default)]
    pub source: ClientKind,
    /// Optional optimistic concurrency check. If provided and != current state revision,
    /// the gateway rejects the command with `revision_conflict`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<u64>,
    /// Optional idempotency key to prevent duplicate execution on retries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    pub command: Command,
}

impl Envelope {
    pub fn new(id: impl Into<String>, source: ClientKind, command: Command) -> Self {
        Self {
            id: id.into(),
            protocol: PROTOCOL_VERSION,
            source,
            expected_revision: None,
            idempotency_key: None,
            command,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FlipAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResizeFilter {
    #[default]
    Lanczos3,
    Nearest,
    Triangle,
    CatmullRom,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    OpenFiles {
        paths: Vec<String>,
    },
    OpenArchive {
        path: String,
    },
    OpenFolder {
        path: String,
    },
    Navigate {
        path: String,
    },
    Refresh,
    Select {
        paths: Vec<String>,
    },
    SetWorkspace {
        workspace: Workspace,
    },
    WorkspaceBack,
    SetTheme {
        theme: Theme,
    },
    SetBackupMethod {
        method: crate::BackupMethod,
    },
    ToggleTheme,
    SetLanguage {
        language: Language,
    },
    SetUiScale {
        scale_pct: u16,
    },
    SetFontScale {
        scale_pct: u16,
    },
    SetDisplayFont {
        font: crate::DisplayFont,
    },
    SetLayout {
        layout: LayoutState,
    },
    ToggleLeftPanel,
    ToggleRightPanel {
        #[serde(default)]
        workspace: Option<Workspace>,
    },
    SetSettings {
        theme: Theme,
        language: Language,
        remember_last_folder: bool,
        hardware_acceleration: bool,
        #[serde(default = "default_true")]
        save_ui_state_on_exit: bool,
    },
    NewFolder {
        parent: String,
        name: String,
    },
    ViewerNext,
    ViewerPrevious,
    ViewerOpen {
        path: String,
    },
    EditorOpen {
        path: String,
    },
    EditorNew {
        width: u32,
        height: u32,
        #[serde(default = "default_white_rgba")]
        background: [u8; 4],
    },
    OpenInEditorOrUnsupported {
        path: String,
    },
    EditorResize {
        width: u32,
        height: u32,
        #[serde(default)]
        filter: ResizeFilter,
        #[serde(default)]
        mode: ResizeMode,
        #[serde(default)]
        fill: Option<[u8; 3]>,
        #[serde(default = "default_opaque_alpha")]
        fill_alpha: u8,
    },
    EditorCrop {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        #[serde(default)]
        fill: Option<[u8; 3]>,
        #[serde(default = "default_opaque_alpha")]
        fill_alpha: u8,
    },
    EditorRotate {
        /// Degrees: 90, 180, 270, or -90.
        degrees: i32,
        #[serde(default)]
        target: ApplyTarget,
    },
    EditorFlip {
        axis: FlipAxis,
        #[serde(default)]
        target: ApplyTarget,
    },
    EditorAdjustColor {
        /// -1.0 ..= 1.0
        #[serde(default)]
        brightness: f32,
        /// -1.0 ..= 1.0
        #[serde(default)]
        contrast: f32,
        /// -1.0 ..= 1.0
        #[serde(default)]
        saturation: f32,
        /// -2.0 ..= 2.0
        #[serde(default)]
        exposure: f32,
        /// -1.0 ..= 1.0
        #[serde(default)]
        highlights: f32,
        /// -1.0 ..= 1.0
        #[serde(default)]
        shadows: f32,
        /// 0.2 ..= 3.0
        #[serde(default = "default_one_f32")]
        gamma: f32,
        #[serde(default)]
        target: ApplyTarget,
    },
    EditorCanvasSize {
        width: u32,
        height: u32,
        #[serde(default)]
        anchor: CanvasAnchor,
        #[serde(default)]
        fill: Option<[u8; 3]>,
        #[serde(default = "default_opaque_alpha")]
        fill_alpha: u8,
    },
    EditorRotateAngle {
        degrees: f64,
        #[serde(default)]
        fill: Option<[u8; 3]>,
        #[serde(default = "default_opaque_alpha")]
        fill_alpha: u8,
        #[serde(default)]
        target: ApplyTarget,
    },
    EditorAdjustHsv {
        #[serde(default)]
        hue: f32,
        #[serde(default)]
        saturation: f32,
        #[serde(default)]
        value: f32,
        #[serde(default)]
        target: ApplyTarget,
    },
    EditorColorBalance {
        #[serde(default)]
        shadows: [f32; 3],
        #[serde(default)]
        midtones: [f32; 3],
        #[serde(default)]
        highlights: [f32; 3],
        #[serde(default = "default_true")]
        preserve_luminosity: bool,
        #[serde(default)]
        target: ApplyTarget,
    },
    EditorAdjustCurves {
        #[serde(default)]
        rgb: Vec<[f32; 2]>,
        #[serde(default)]
        red: Vec<[f32; 2]>,
        #[serde(default)]
        green: Vec<[f32; 2]>,
        #[serde(default)]
        blue: Vec<[f32; 2]>,
        #[serde(default)]
        target: ApplyTarget,
    },
    EditorPaintStroke {
        points: Vec<[f64; 2]>,
        color: [u8; 3],
        radius: f32,
        #[serde(default = "default_one_f32")]
        opacity: f32,
        #[serde(default)]
        erase: bool,
        #[serde(flatten, default)]
        brush: BrushSettings,
    },
    EditorLiveStrokeBegin {
        #[serde(flatten)]
        stroke: LiveStrokeKind,
        x: f64,
        y: f64,
    },
    EditorLiveStrokeStep {
        points: Vec<[f64; 2]>,
    },
    EditorLiveStrokeEnd {
        #[serde(default = "default_true")]
        commit: bool,
    },
    EditorDrawShape {
        kind: DrawShapeKind,
        #[serde(default)]
        points: Vec<[f32; 2]>,
        #[serde(default)]
        nodes: Vec<VectorPathNode>,
        #[serde(default)]
        fill_mode: ShapeFillMode,
        #[serde(default)]
        closed: bool,
        #[serde(default)]
        color: Option<[u8; 3]>,
        #[serde(default)]
        radius: Option<f32>,
        #[serde(default)]
        opacity: Option<f32>,
    },
    EditorFloodFill {
        x: u32,
        y: u32,
        tolerance: u8,
        #[serde(default = "default_true")]
        contiguous: bool,
        #[serde(default)]
        color: Option<[u8; 3]>,
        #[serde(default)]
        opacity: Option<f32>,
        #[serde(default)]
        sample_all_layers: bool,
    },
    EditorDrawGradient {
        start: [f32; 2],
        end: [f32; 2],
        #[serde(default)]
        kind: GradientKind,
        #[serde(default)]
        color_mode: GradientColorMode,
        #[serde(default)]
        fg_color: Option<[u8; 3]>,
        #[serde(default)]
        bg_color: Option<[u8; 3]>,
        #[serde(default)]
        stops: Vec<GradientStop>,
        #[serde(default)]
        opacity: Option<f32>,
        #[serde(default)]
        repeat: GradientRepeat,
        #[serde(default)]
        reverse: bool,
        #[serde(default)]
        dither: bool,
        #[serde(default)]
        blend_mode: BlendMode,
    },
    EditorHealingStroke {
        points: Vec<[f64; 2]>,
        #[serde(default)]
        mode: HealingMode,
        radius: f32,
        #[serde(default = "default_brush_hardness")]
        hardness: f32,
        #[serde(default = "default_one_f32")]
        strength: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<[f64; 2]>,
    },
    EditorDenoise {
        strength: f32,
        #[serde(default = "default_half_f32")]
        detail_preserve: f32,
        #[serde(default)]
        target: ApplyTarget,
    },
    EditorTransformLayer {
        #[serde(default)]
        layer_id: Option<u64>,
        transform: LayerTransformSpec,
    },
    EditorCopyPixels {
        #[serde(default)]
        merged: bool,
    },
    EditorCutPixels,
    EditorPastePixels {
        #[serde(default)]
        png_bytes: Option<Vec<u8>>,
        #[serde(default)]
        into_selection: bool,
    },
    EditorLayerAdd {
        #[serde(default)]
        name: String,
    },
    EditorLayerAddEffect {
        #[serde(default)]
        name: String,
        effect: LayerEffect,
    },
    EditorLayerUpdateEffect {
        id: u64,
        effect: LayerEffect,
    },
    EditorLayerDuplicate {
        id: u64,
    },
    EditorLayerDelete {
        id: u64,
    },
    EditorLayerSelect {
        id: u64,
    },
    EditorLayerReorder {
        id: u64,
        new_index: usize,
    },
    EditorLayerSetProps {
        id: u64,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        visible: Option<bool>,
        #[serde(default)]
        locked: Option<bool>,
        #[serde(default)]
        opacity: Option<f32>,
        #[serde(default)]
        blend_mode: Option<BlendMode>,
        #[serde(default)]
        offset_x: Option<i32>,
        #[serde(default)]
        offset_y: Option<i32>,
    },
    EditorLayerMergeDown {
        id: u64,
    },
    EditorLayerFlatten,
    EditorSetCanvasBackground {
        #[serde(default)]
        color: Option<[u8; 3]>,
        #[serde(default)]
        transparent: bool,
    },
    EditorHistoryJump {
        target_index: usize,
    },
    EditorExport {
        path: String,
        #[serde(default)]
        quality: Option<u8>,
        #[serde(default)]
        metadata: bool,
        #[serde(default)]
        max_edge: Option<u32>,
    },
    EditorLayerMaskAdd {
        id: u64,
        #[serde(default = "default_true")]
        reveal_all: bool,
    },
    EditorLayerMaskDelete {
        id: u64,
    },
    EditorLayerMaskApply {
        id: u64,
    },
    EditorLayerMaskInvert {
        id: u64,
    },
    EditorLayerMaskSetEnabled {
        id: u64,
        enabled: bool,
    },
    EditorLayerMaskSetEditing {
        id: u64,
        editing: bool,
    },
    EditorLayerMaskSetLinked {
        id: u64,
        linked: bool,
    },
    EditorLiquifyStroke {
        points: Vec<[f64; 2]>,
        #[serde(default)]
        mode: LiquifyMode,
        radius: f32,
        #[serde(default = "default_half_f32")]
        strength: f32,
    },
    EditorSelectAll,
    EditorDeselect,
    EditorReselect,
    EditorSelectInvert,
    EditorSelectShape {
        shape: SelectionShape,
        #[serde(default)]
        op: SelectionBooleanOp,
    },
    EditorSelectModify {
        op: SelectionModifyOp,
    },
    EditorSelectToggleQuickMask,
    EditorSelectFromLayerAlpha {
        #[serde(default)]
        op: SelectionBooleanOp,
    },
    EditorSelectionToMask,
    EditorSelectionClearPixels,
    EditorSelectionDuplicateLayer,
    EditorCropToSelection,
    BatchRun {
        operation: BatchOperation,
        source: BatchSource,
        #[serde(default)]
        output: BatchOutput,
    },
    BatchCancel,
    AddFavorite {
        path: String,
    },
    RemoveFavorite {
        path: String,
    },
    MoveFavorite {
        path: String,
        direction: MoveDirection,
    },
    RenamePath {
        path: String,
        new_name: String,
    },
    CopyTo {
        paths: Vec<String>,
        destination: String,
    },
    MoveTo {
        paths: Vec<String>,
        destination: String,
    },
    Trash {
        paths: Vec<String>,
    },
    DeletePermanently {
        paths: Vec<String>,
    },
    PasteImageBytes {
        folder: String,
        #[serde(default)]
        filename_hint: String,
        png_bytes: Vec<u8>,
    },
    PluginRefresh,
    PluginSetEnabled {
        id: String,
        enabled: bool,
    },
    PluginInstall {
        source_dir: String,
    },
    PluginUninstall {
        id: String,
    },
    PluginRun {
        id: String,
        #[serde(default)]
        params: BTreeMap<String, PluginParamValue>,
        #[serde(default)]
        target: ApplyTarget,
    },
    AiRefreshPlugins,
    AiSetSettings {
        device: AiDevice,
        #[serde(default = "default_ai_auto_unload_secs")]
        auto_unload_secs: u32,
    },
    AiDownloadModel {
        model_id: String,
    },
    AiDeleteModel {
        model_id: String,
    },
    AiSetCustomModel {
        model_id: String,
        path: String,
    },
    AiCancel,
    QuickActionRun {
        plugin_id: String,
        action_id: String,
        #[serde(default)]
        paths: Vec<String>,
    },
    EditorAiRemoveBackground {
        #[serde(default = "default_rembg_model_id")]
        model_id: String,
        #[serde(default)]
        feather: u8,
        #[serde(default)]
        target: ApplyTarget,
    },
    EditorDewatermarkGemini {
        #[serde(default = "default_gemini_size")]
        size: String,
        #[serde(default = "default_gemini_logo_value")]
        logo_value: f32,
        #[serde(default)]
        force: bool,
        #[serde(default)]
        offset_x: i32,
        #[serde(default)]
        offset_y: i32,
        #[serde(default)]
        target: ApplyTarget,
    },
    EditorInpaintRegion {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        #[serde(default)]
        feather: u32,
        #[serde(default)]
        target: ApplyTarget,
    },
    EditorAiInpaint {
        #[serde(default = "default_ai_detector_model")]
        detector_model: String,
        #[serde(default = "default_ai_inpainter_model")]
        inpainter_model: String,
        #[serde(default = "default_ai_inpaint_confidence")]
        confidence: f32,
        #[serde(default = "default_ai_inpaint_padding")]
        mask_padding: u32,
        #[serde(default)]
        region_x: i32,
        #[serde(default)]
        region_y: i32,
        #[serde(default = "default_region_dim")]
        region_width: i32,
        #[serde(default = "default_region_dim")]
        region_height: i32,
        #[serde(default)]
        brush_strokes: Vec<AiMaskStroke>,
        #[serde(default)]
        target: ApplyTarget,
    },
    EditorRefineAlpha {
        #[serde(default)]
        erase_strokes: Vec<AiMaskStroke>,
        #[serde(default)]
        restore_strokes: Vec<AiMaskStroke>,
        #[serde(default = "default_refine_softness")]
        softness: f32,
    },
    EditorGenerateCaption {
        #[serde(default = "default_caption_model_id")]
        model_id: String,
        #[serde(default = "default_caption_general_threshold")]
        general_threshold: f32,
        #[serde(default = "default_caption_character_threshold")]
        character_threshold: f32,
        #[serde(default = "default_true")]
        replace_underscores: bool,
        #[serde(default)]
        prefix: String,
        #[serde(default)]
        suffix: String,
        #[serde(default)]
        exclude_tags: String,
    },
    EditorSaveCaption {
        text: String,
    },
    ViewerTranslate {
        path: String,
        #[serde(default)]
        source_lang: String,
        #[serde(default)]
        target_lang: String,
        #[serde(default)]
        region: Option<[i32; 4]>,
        #[serde(default = "default_ocr_engine")]
        ocr_engine: String,
    },
    ViewerSaveTranslation {
        path: String,
        blocks: Vec<TranslationBlock>,
        #[serde(default)]
        source_lang: String,
        #[serde(default)]
        target_lang: String,
    },
    ViewerDeleteTranslation {
        path: String,
    },
    SetTranslateSettings {
        settings: TranslateSettings,
    },
    EditorUndo,
    EditorRedo,
    EditorReset,
    EditorSave,
    EditorSaveAs {
        path: String,
    },
    EditorClose,
    EditorSwitchTab {
        tab_id: u64,
    },
    EditorCloseTab {
        #[serde(default)]
        tab_id: Option<u64>,
    },
    EditorReorderTab {
        tab_id: u64,
        new_index: usize,
    },
    FocusWindow,
    SaveUiState {
        #[serde(default)]
        force: bool,
    },
    CancelTask {
        task_id: String,
    },
    ResetConfig {
        #[serde(default = "default_true")]
        reset_layout: bool,
        #[serde(default = "default_true")]
        reset_settings: bool,
    },
    /// Request the attached GUI adapter to capture a framebuffer screenshot.
    DebugScreenshot {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_path: Option<String>,
    },
    /// Trigger a registered UI presentation action by semantic widget ID.
    DebugUiAction {
        target_id: String,
        action: String,
    },
}

impl Command {
    /// Stable snake_case name used in journal, logs, and metrics.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::OpenFiles { .. } => "open_files",
            Self::OpenArchive { .. } => "open_archive",
            Self::OpenFolder { .. } => "open_folder",
            Self::Navigate { .. } => "navigate",
            Self::Refresh => "refresh",
            Self::Select { .. } => "select",
            Self::SetWorkspace { .. } => "set_workspace",
            Self::WorkspaceBack => "workspace_back",
            Self::SetTheme { .. } => "set_theme",
            Self::SetBackupMethod { .. } => "set_backup_method",
            Self::ToggleTheme => "toggle_theme",
            Self::SetLanguage { .. } => "set_language",
            Self::SetUiScale { .. } => "set_ui_scale",
            Self::SetFontScale { .. } => "set_font_scale",
            Self::SetDisplayFont { .. } => "set_display_font",
            Self::SetLayout { .. } => "set_layout",
            Self::ToggleLeftPanel => "toggle_left_panel",
            Self::ToggleRightPanel { .. } => "toggle_right_panel",
            Self::SetSettings { .. } => "set_settings",
            Self::NewFolder { .. } => "new_folder",
            Self::ViewerNext => "viewer_next",
            Self::ViewerPrevious => "viewer_previous",
            Self::ViewerOpen { .. } => "viewer_open",
            Self::EditorOpen { .. } => "editor_open",
            Self::EditorNew { .. } => "editor_new",
            Self::OpenInEditorOrUnsupported { .. } => "open_in_editor_or_unsupported",
            Self::EditorResize { .. } => "editor_resize",
            Self::EditorCrop { .. } => "editor_crop",
            Self::EditorRotate { .. } => "editor_rotate",
            Self::EditorFlip { .. } => "editor_flip",
            Self::EditorAdjustColor { .. } => "editor_adjust_color",
            Self::EditorCanvasSize { .. } => "editor_canvas_size",
            Self::EditorRotateAngle { .. } => "editor_rotate_angle",
            Self::EditorAdjustHsv { .. } => "editor_adjust_hsv",
            Self::EditorColorBalance { .. } => "editor_color_balance",
            Self::EditorAdjustCurves { .. } => "editor_adjust_curves",
            Self::EditorPaintStroke { .. } => "editor_paint_stroke",
            Self::EditorLiveStrokeBegin { .. } => "editor_live_stroke_begin",
            Self::EditorLiveStrokeStep { .. } => "editor_live_stroke_step",
            Self::EditorLiveStrokeEnd { .. } => "editor_live_stroke_end",
            Self::EditorDrawShape { .. } => "editor_draw_shape",
            Self::EditorFloodFill { .. } => "editor_flood_fill",
            Self::EditorDrawGradient { .. } => "editor_draw_gradient",
            Self::EditorHealingStroke { .. } => "editor_healing_stroke",
            Self::EditorDenoise { .. } => "editor_denoise",
            Self::EditorTransformLayer { .. } => "editor_transform_layer",
            Self::EditorCopyPixels { .. } => "editor_copy_pixels",
            Self::EditorCutPixels => "editor_cut_pixels",
            Self::EditorPastePixels { .. } => "editor_paste_pixels",
            Self::EditorLayerAdd { .. } => "editor_layer_add",
            Self::EditorLayerAddEffect { .. } => "editor_layer_add_effect",
            Self::EditorLayerUpdateEffect { .. } => "editor_layer_update_effect",
            Self::EditorLayerDuplicate { .. } => "editor_layer_duplicate",
            Self::EditorLayerDelete { .. } => "editor_layer_delete",
            Self::EditorLayerSelect { .. } => "editor_layer_select",
            Self::EditorLayerReorder { .. } => "editor_layer_reorder",
            Self::EditorLayerSetProps { .. } => "editor_layer_set_props",
            Self::EditorLayerMergeDown { .. } => "editor_layer_merge_down",
            Self::EditorLayerFlatten => "editor_layer_flatten",
            Self::EditorSetCanvasBackground { .. } => "editor_set_canvas_background",
            Self::EditorHistoryJump { .. } => "editor_history_jump",
            Self::EditorExport { .. } => "editor_export",
            Self::EditorLayerMaskAdd { .. } => "editor_layer_mask_add",
            Self::EditorLayerMaskDelete { .. } => "editor_layer_mask_delete",
            Self::EditorLayerMaskApply { .. } => "editor_layer_mask_apply",
            Self::EditorLayerMaskInvert { .. } => "editor_layer_mask_invert",
            Self::EditorLayerMaskSetEnabled { .. } => "editor_layer_mask_set_enabled",
            Self::EditorLayerMaskSetEditing { .. } => "editor_layer_mask_set_editing",
            Self::EditorLayerMaskSetLinked { .. } => "editor_layer_mask_set_linked",
            Self::EditorLiquifyStroke { .. } => "editor_liquify_stroke",
            Self::EditorSelectAll => "editor_select_all",
            Self::EditorDeselect => "editor_deselect",
            Self::EditorReselect => "editor_reselect",
            Self::EditorSelectInvert => "editor_select_invert",
            Self::EditorSelectShape { .. } => "editor_select_shape",
            Self::EditorSelectModify { .. } => "editor_select_modify",
            Self::EditorSelectToggleQuickMask => "editor_select_toggle_quick_mask",
            Self::EditorSelectFromLayerAlpha { .. } => "editor_select_from_layer_alpha",
            Self::EditorSelectionToMask => "editor_selection_to_mask",
            Self::EditorSelectionClearPixels => "editor_selection_clear_pixels",
            Self::EditorSelectionDuplicateLayer => "editor_selection_duplicate_layer",
            Self::EditorCropToSelection => "editor_crop_to_selection",
            Self::BatchRun { .. } => "batch_run",
            Self::BatchCancel => "batch_cancel",
            Self::AddFavorite { .. } => "add_favorite",
            Self::RemoveFavorite { .. } => "remove_favorite",
            Self::MoveFavorite { .. } => "move_favorite",
            Self::RenamePath { .. } => "rename_path",
            Self::CopyTo { .. } => "copy_to",
            Self::MoveTo { .. } => "move_to",
            Self::Trash { .. } => "trash",
            Self::DeletePermanently { .. } => "delete_permanently",
            Self::PasteImageBytes { .. } => "paste_image_bytes",
            Self::PluginRefresh => "plugin_refresh",
            Self::PluginSetEnabled { .. } => "plugin_set_enabled",
            Self::PluginInstall { .. } => "plugin_install",
            Self::PluginUninstall { .. } => "plugin_uninstall",
            Self::PluginRun { .. } => "plugin_run",
            Self::AiRefreshPlugins => "ai_refresh_plugins",
            Self::AiSetSettings { .. } => "ai_set_settings",
            Self::AiDownloadModel { .. } => "ai_download_model",
            Self::AiDeleteModel { .. } => "ai_delete_model",
            Self::AiSetCustomModel { .. } => "ai_set_custom_model",
            Self::AiCancel => "ai_cancel",
            Self::QuickActionRun { .. } => "quick_action_run",
            Self::EditorAiRemoveBackground { .. } => "editor_ai_remove_background",
            Self::EditorDewatermarkGemini { .. } => "editor_dewatermark_gemini",
            Self::EditorInpaintRegion { .. } => "editor_inpaint_region",
            Self::EditorAiInpaint { .. } => "editor_ai_inpaint",
            Self::EditorRefineAlpha { .. } => "editor_refine_alpha",
            Self::EditorGenerateCaption { .. } => "editor_generate_caption",
            Self::EditorSaveCaption { .. } => "editor_save_caption",
            Self::ViewerTranslate { .. } => "viewer_translate",
            Self::ViewerSaveTranslation { .. } => "viewer_save_translation",
            Self::ViewerDeleteTranslation { .. } => "viewer_delete_translation",
            Self::SetTranslateSettings { .. } => "set_translate_settings",
            Self::EditorUndo => "editor_undo",
            Self::EditorRedo => "editor_redo",
            Self::EditorReset => "editor_reset",
            Self::EditorSave => "editor_save",
            Self::EditorSaveAs { .. } => "editor_save_as",
            Self::EditorClose => "editor_close",
            Self::EditorSwitchTab { .. } => "editor_switch_tab",
            Self::EditorCloseTab { .. } => "editor_close_tab",
            Self::EditorReorderTab { .. } => "editor_reorder_tab",
            Self::FocusWindow => "focus_window",
            Self::SaveUiState { .. } => "save_ui_state",
            Self::CancelTask { .. } => "cancel_task",
            Self::ResetConfig { .. } => "reset_config",
            Self::DebugScreenshot { .. } => "debug_screenshot",
            Self::DebugUiAction { .. } => "debug_ui_action",
        }
    }

    /// Whether this command can be replayed deterministically in sandbox mode.
    pub const fn is_replayable(&self) -> bool {
        !matches!(
            self,
            Self::DebugScreenshot { .. }
                | Self::DebugUiAction { .. }
                | Self::FocusWindow
                | Self::SaveUiState { .. }
                | Self::CancelTask { .. }
                | Self::BatchCancel
                | Self::AiCancel
                | Self::AiDownloadModel { .. }
                | Self::AiDeleteModel { .. }
        )
    }
}

/// Read-only query surface. Queries never mutate domain state.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Query {
    State,
    Meta,
    Status,
    ListFolders { path: String },
    Tasks,
    UiTree,
    Schema,
    EditorClipboardPng,
}

/// Outcome of submitting an `Envelope` to the gateway.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommandResponse {
    pub id: String,
    pub ok: bool,
    pub status: CommandStatus,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProtocolError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Completed,
    Accepted,
    Failed,
}

/// Sequenced event emitted on Core's event bus for GUI, CLI `--follow`, and MCP.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    pub seq_id: u64,
    pub timestamp_ms: u64,
    pub revision: u64,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EventKind {
    StateChanged {
        workspace: Workspace,
        listing_epoch: u64,
    },
    CommandExecuted {
        request_id: String,
        command: String,
        ok: bool,
    },
    TaskStarted {
        task_id: String,
        kind: String,
    },
    TaskProgress {
        task_id: String,
        percent: u8,
        stage: String,
    },
    TaskFinished {
        task_id: String,
        ok: bool,
        message: String,
    },
    ScreenshotRequested {
        request_id: String,
        output_path: Option<String>,
    },
    ScreenshotCaptured {
        request_id: String,
        path: String,
        width: u32,
        height: u32,
    },
    UiActionRequested {
        request_id: String,
        target_id: String,
        action: String,
    },
}

/// Translatable message emitted by Core without pulling i18n dependencies into Core.
///
/// Core knows *what* happened (`code` + named `args`); the presentation edge
/// (GUI or localized CLI) renders it via Fluent `.ftl`, while logs and the
/// journal always record `UiText::english()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiText {
    pub code: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
}

impl UiText {
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            args: BTreeMap::new(),
        }
    }

    pub fn with(
        code: impl Into<String>,
        args: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self {
            code: code.into(),
            args: args
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }

    pub fn empty() -> Self {
        Self::new("")
    }

    pub fn is_empty(&self) -> bool {
        self.code.is_empty()
    }

    /// Mechanical English fallback for journals, tracing logs, and headless CLI.
    pub fn english(&self) -> String {
        if self.code.is_empty() {
            return String::new();
        }
        let mut out = self.code.replace(['_', '-'], " ");
        if !self.args.is_empty() {
            let formatted: Vec<String> =
                self.args.iter().map(|(k, v)| format!("{k}={v}")).collect();
            out.push_str(": ");
            out.push_str(&formatted.join(", "));
        }
        out
    }
}

/// Structured command/query rejection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiText>,
}

impl ProtocolError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            ui: None,
        }
    }

    pub fn line(code: impl Into<String>, ui: UiText) -> Self {
        let message = ui.english();
        Self {
            code: code.into(),
            message,
            ui: Some(ui),
        }
    }

    pub fn with_line(code: impl Into<String>, message: impl Into<String>, ui: UiText) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            ui: Some(ui),
        }
    }
}

/// Generate JSON Schema bundle for the entire Photon protocol (used by agents & MCP).
pub fn protocol_schemas() -> serde_json::Value {
    serde_json::json!({
        "protocol_version": PROTOCOL_VERSION,
        "envelope": schemars::schema_for!(Envelope),
        "command": schemars::schema_for!(Command),
        "query": schemars::schema_for!(Query),
        "response": schemars::schema_for!(CommandResponse),
        "event": schemars::schema_for!(Event),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_fill_gradient_shape_commands_deserialize_with_defaults() {
        let fill: Command =
            serde_json::from_str(r#"{"type":"editor_flood_fill","x":10,"y":20,"tolerance":12}"#)
                .unwrap();
        assert_eq!(fill.name(), "editor_flood_fill");

        let grad: Command = serde_json::from_str(
            r#"{"type":"editor_draw_gradient","start":[0.0,0.0],"end":[10.0,10.0]}"#,
        )
        .unwrap();
        assert_eq!(grad.name(), "editor_draw_gradient");

        let shape: Command =
            serde_json::from_str(r#"{"type":"editor_draw_shape","kind":"rect"}"#).unwrap();
        assert_eq!(shape.name(), "editor_draw_shape");
    }

    #[test]
    fn new_commands_and_schemas_build_cleanly() {
        let cmds = [
            Command::EditorDenoise {
                strength: 0.5,
                detail_preserve: 0.5,
                target: ApplyTarget::ActiveLayer,
            },
            Command::EditorTransformLayer {
                layer_id: None,
                transform: LayerTransformSpec::default(),
            },
            Command::EditorCopyPixels { merged: true },
            Command::EditorCutPixels,
            Command::EditorPastePixels {
                png_bytes: None,
                into_selection: false,
            },
            Command::PasteImageBytes {
                folder: "/tmp".into(),
                filename_hint: "paste.png".into(),
                png_bytes: vec![1, 2, 3],
            },
            Command::PluginRefresh,
            Command::PluginSetEnabled {
                id: "demo".into(),
                enabled: false,
            },
            Command::PluginInstall {
                source_dir: "/tmp/demo".into(),
            },
            Command::PluginUninstall { id: "demo".into() },
            Command::PluginRun {
                id: "demo".into(),
                params: BTreeMap::new(),
                target: ApplyTarget::ActiveLayer,
            },
        ];
        for cmd in &cmds {
            assert!(!cmd.name().is_empty());
        }
        let schemas = protocol_schemas();
        assert!(schemas.get("command").is_some());
    }

    #[test]
    fn gradient_command_multi_stop_roundtrip() {
        let cmd = Command::EditorDrawGradient {
            start: [10.0, 20.0],
            end: [110.0, 20.0],
            kind: crate::GradientKind::Spiral,
            color_mode: crate::GradientColorMode::Custom,
            fg_color: Some([255, 0, 0]),
            bg_color: Some([0, 0, 255]),
            stops: vec![
                crate::GradientStop {
                    pos: 0.0,
                    color: [255, 0, 0],
                    alpha: 1.0,
                },
                crate::GradientStop {
                    pos: 0.5,
                    color: [0, 255, 0],
                    alpha: 0.5,
                },
                crate::GradientStop {
                    pos: 1.0,
                    color: [0, 0, 255],
                    alpha: 1.0,
                },
            ],
            opacity: Some(0.85),
            repeat: crate::GradientRepeat::Alternating,
            reverse: true,
            dither: true,
            blend_mode: crate::BlendMode::Multiply,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let back: Command = serde_json::from_str(&json).unwrap();
        assert_eq!(
            serde_json::to_value(&back).unwrap(),
            serde_json::to_value(&cmd).unwrap()
        );
    }
}
