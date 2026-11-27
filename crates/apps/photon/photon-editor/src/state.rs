use crate::*;
use fw_ui::editor::{DimensionRatioSpec, LengthUnit, dimension_ratio_grid, px_to_unit, unit_to_px};
use iced::{Color, Element};
use photon_canvas::CanvasTool;
use photon_i18n::tr;
use photon_protocol::{
    AiMaskStroke, ApplyTarget, BlendMode, BrushSettings, CanvasAnchor, ColorBalanceAdjustment,
    CurvesAdjustment, GradientColorMode, GradientKind, GradientRepeat, GradientStop, HealingMode,
    HsvAdjustment, Language, LayerTransformSpec, LiquifyMode, ShapeFillMode,
};

#[derive(Debug, Clone)]
pub enum DimGridAction {
    WidthChanged(String),
    HeightChanged(String),
    RatioChanged(String),
    UnitChanged(LengthUnit),
    DpiChanged(f32),
    ToggleLockWidth,
    ToggleLockHeight,
    ToggleLockRatio,
    PresetRatio(u32, u32),
    PresetDimensions(u32, u32),
}

#[derive(Debug, Clone)]
pub struct DimensionInputState {
    pub ref_w: u32,
    pub ref_h: u32,
    pub width_px: u32,
    pub height_px: u32,
    pub width_str: String,
    pub height_str: String,
    pub ratio_str: String,
    pub unit: LengthUnit,
    pub dpi: f32,
    pub lock_width: bool,
    pub lock_height: bool,
    pub lock_ratio: bool,
}

impl Default for DimensionInputState {
    fn default() -> Self {
        Self::from_dims(1920, 1080)
    }
}

impl DimensionInputState {
    pub fn from_dims(w: u32, h: u32) -> Self {
        let rw = w.max(1);
        let rh = h.max(1);
        let ratio = format_ratio_string(rw, rh);
        Self {
            ref_w: rw,
            ref_h: rh,
            width_px: rw,
            height_px: rh,
            width_str: rw.to_string(),
            height_str: rh.to_string(),
            ratio_str: ratio,
            unit: LengthUnit::Px,
            dpi: 300.0,
            lock_width: false,
            lock_height: false,
            lock_ratio: true,
        }
    }

    pub fn sync_from_px(&mut self, w: u32, h: u32) {
        self.ref_w = w.max(1);
        self.ref_h = h.max(1);
        self.width_px = self.ref_w;
        self.height_px = self.ref_h;
        self.refresh_display_strings();
        self.ratio_str = format_ratio_string(self.width_px, self.height_px);
    }

    fn format_val(v: f32, unit: LengthUnit) -> String {
        match unit {
            LengthUnit::Px => format!("{}", v.round().max(1.0) as u32),
            LengthUnit::Percent => format!("{v:.1}"),
            LengthUnit::Cm | LengthUnit::Inch => format!("{v:.2}"),
        }
    }

    fn refresh_display_strings(&mut self) {
        let wv = px_to_unit(self.width_px, self.unit, self.dpi, self.ref_w);
        let hv = px_to_unit(self.height_px, self.unit, self.dpi, self.ref_h);
        self.width_str = Self::format_val(wv, self.unit);
        self.height_str = Self::format_val(hv, self.unit);
    }

    pub fn parsed_aspect(&self) -> f32 {
        parse_ratio_str(&self.ratio_str)
            .unwrap_or_else(|| (self.width_px as f32 / self.height_px.max(1) as f32).max(0.01))
    }

    pub fn apply(&mut self, action: DimGridAction) {
        match action {
            DimGridAction::WidthChanged(val) => {
                self.width_str = val.clone();
                if let Ok(parsed) = val.trim().parse::<f32>()
                    && parsed > 0.0
                {
                    let px = unit_to_px(parsed, self.unit, self.dpi, self.ref_w).clamp(1, 32_768);
                    self.width_px = px;
                    if self.lock_ratio && !self.lock_height {
                        let aspect = self.parsed_aspect();
                        self.height_px = ((px as f32 / aspect).round() as u32).clamp(1, 32_768);
                        let hv = px_to_unit(self.height_px, self.unit, self.dpi, self.ref_h);
                        self.height_str = Self::format_val(hv, self.unit);
                    } else if !self.lock_ratio {
                        self.ratio_str = format_ratio_string(self.width_px, self.height_px);
                    }
                }
            }
            DimGridAction::HeightChanged(val) => {
                self.height_str = val.clone();
                if let Ok(parsed) = val.trim().parse::<f32>()
                    && parsed > 0.0
                {
                    let px = unit_to_px(parsed, self.unit, self.dpi, self.ref_h).clamp(1, 32_768);
                    self.height_px = px;
                    if self.lock_ratio && !self.lock_width {
                        let aspect = self.parsed_aspect();
                        self.width_px = ((px as f32 * aspect).round() as u32).clamp(1, 32_768);
                        let wv = px_to_unit(self.width_px, self.unit, self.dpi, self.ref_w);
                        self.width_str = Self::format_val(wv, self.unit);
                    } else if !self.lock_ratio {
                        self.ratio_str = format_ratio_string(self.width_px, self.height_px);
                    }
                }
            }
            DimGridAction::RatioChanged(val) => {
                self.ratio_str = val.clone();
                if let Some(aspect) = parse_ratio_str(&val) {
                    if !self.lock_height {
                        self.height_px =
                            ((self.width_px as f32 / aspect).round() as u32).clamp(1, 32_768);
                    } else if !self.lock_width {
                        self.width_px =
                            ((self.height_px as f32 * aspect).round() as u32).clamp(1, 32_768);
                    }
                    self.refresh_display_strings();
                }
            }
            DimGridAction::UnitChanged(u) => {
                self.unit = u;
                self.refresh_display_strings();
            }
            DimGridAction::DpiChanged(d) => {
                self.dpi = d.clamp(36.0, 2400.0);
                if matches!(self.unit, LengthUnit::Cm | LengthUnit::Inch) {
                    if let Ok(wv) = self.width_str.trim().parse::<f32>()
                        && wv > 0.0
                    {
                        self.width_px =
                            unit_to_px(wv, self.unit, self.dpi, self.ref_w).clamp(1, 32_768);
                    }
                    if let Ok(hv) = self.height_str.trim().parse::<f32>()
                        && hv > 0.0
                    {
                        self.height_px =
                            unit_to_px(hv, self.unit, self.dpi, self.ref_h).clamp(1, 32_768);
                    }
                }
            }
            DimGridAction::ToggleLockWidth => {
                self.lock_width = !self.lock_width;
            }
            DimGridAction::ToggleLockHeight => {
                self.lock_height = !self.lock_height;
            }
            DimGridAction::ToggleLockRatio => {
                self.lock_ratio = !self.lock_ratio;
                if self.lock_ratio {
                    self.ratio_str = format_ratio_string(self.width_px, self.height_px);
                }
            }
            DimGridAction::PresetRatio(rw, rh) => {
                self.lock_ratio = true;
                self.ratio_str = format!("{rw}:{rh}");
                let aspect = (rw as f32 / rh.max(1) as f32).max(0.01);
                if !self.lock_height {
                    self.height_px =
                        ((self.width_px as f32 / aspect).round() as u32).clamp(1, 32_768);
                } else if !self.lock_width {
                    self.width_px =
                        ((self.height_px as f32 * aspect).round() as u32).clamp(1, 32_768);
                }
                self.refresh_display_strings();
            }
            DimGridAction::PresetDimensions(w, h) => {
                self.width_px = w.clamp(1, 32_768);
                self.height_px = h.clamp(1, 32_768);
                self.lock_width = false;
                self.lock_height = false;
                self.ratio_str = format_ratio_string(self.width_px, self.height_px);
                self.refresh_display_strings();
            }
        }
    }

    pub fn view<'a, M: Clone + 'a, F>(&'a self, lang: Language, map_action: F) -> Element<'a, M>
    where
        F: Fn(DimGridAction) -> M + Clone + 'a,
    {
        let f_w = map_action.clone();
        let f_h = map_action.clone();
        let f_r = map_action.clone();
        let f_u = map_action.clone();
        let f_d = map_action.clone();
        dimension_ratio_grid(DimensionRatioSpec {
            width_label: tr(lang, "label-width"),
            height_label: tr(lang, "label-height"),
            ratio_label: tr(lang, "label-presets"),
            unit_label: tr(lang, "label-unit"),
            dpi_label: tr(lang, "label-dpi"),
            width_str: &self.width_str,
            height_str: &self.height_str,
            ratio_str: &self.ratio_str,
            unit: self.unit,
            dpi: self.dpi,
            lock_width: self.lock_width,
            lock_height: self.lock_height,
            lock_ratio: self.lock_ratio,
            on_width_change: Box::new(move |s| f_w(DimGridAction::WidthChanged(s))),
            on_height_change: Box::new(move |s| f_h(DimGridAction::HeightChanged(s))),
            on_ratio_change: Box::new(move |s| f_r(DimGridAction::RatioChanged(s))),
            on_unit_change: Box::new(move |u| f_u(DimGridAction::UnitChanged(u))),
            on_dpi_change: Box::new(move |d| f_d(DimGridAction::DpiChanged(d))),
            on_toggle_lock_width: map_action(DimGridAction::ToggleLockWidth),
            on_toggle_lock_height: map_action(DimGridAction::ToggleLockHeight),
            on_toggle_lock_ratio: map_action(DimGridAction::ToggleLockRatio),
            ratio_presets: vec![
                (
                    "1:1".to_owned(),
                    map_action(DimGridAction::PresetRatio(1, 1)),
                ),
                (
                    "4:3".to_owned(),
                    map_action(DimGridAction::PresetRatio(4, 3)),
                ),
                (
                    "16:9".to_owned(),
                    map_action(DimGridAction::PresetRatio(16, 9)),
                ),
                (
                    "3:2".to_owned(),
                    map_action(DimGridAction::PresetRatio(3, 2)),
                ),
            ],
        })
    }
}

fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a.max(1)
}

pub fn format_ratio_string(w: u32, h: u32) -> String {
    let g = gcd_u32(w.max(1), h.max(1));
    let rw = w.max(1) / g;
    let rh = h.max(1) / g;
    if rw <= 32 && rh <= 32 {
        format!("{rw}:{rh}")
    } else {
        format!("{:.2}", w as f32 / h.max(1) as f32)
    }
}

pub fn parse_ratio_str(raw: &str) -> Option<f32> {
    let clean = raw.trim();
    if let Some((a, b)) = clean.split_once(':').or_else(|| clean.split_once('/')) {
        let lhs = a.trim().parse::<f32>().ok()?;
        let rhs = b.trim().parse::<f32>().ok()?;
        if lhs > 0.0 && rhs > 0.0 {
            return Some((lhs / rhs).clamp(0.05, 20.0));
        }
        return None;
    }
    let v = clean.parse::<f32>().ok()?;
    (v > 0.0).then(|| v.clamp(0.05, 20.0))
}

#[derive(Debug, Clone)]
pub struct EditorUiState {
    pub tool: EditorTool,
    pub adjust_popup_tab: Option<AdjustmentPopupTab>,
    pub fg_color: [u8; 3],
    pub bg_color: Option<[u8; 3]>,
    pub sampled_color: Option<[u8; 4]>,
    pub brush_radius: f32,
    pub brush_opacity: f32,
    pub brush_settings: BrushSettings,
    pub liquify_mode: LiquifyMode,
    pub liquify_radius: f32,
    pub liquify_strength: f32,
    pub stroke_active: bool,
    pub apply_target: ApplyTarget,
    pub apply_to_selection: bool,
    pub adjust_live_preview: bool,
    pub selected_layer_ids: Vec<u64>,
    pub layer_anchor_id: Option<u64>,
    pub dragging_layer_id: Option<u64>,
    pub align_target: photon_protocol::AlignTarget,
    pub hsv: HsvAdjustment,
    pub balance: ColorBalanceAdjustment,
    pub balance_zone: ColorBalanceZone,
    pub curves: CurvesAdjustment,
    pub curves_channel: CurvesChannel,
    pub rotate_angle: f64,
    pub rotate_fill: Option<[u8; 3]>,
    pub canvas_dims: DimensionInputState,
    pub canvas_anchor: CanvasAnchor,
    pub canvas_fill: Option<[u8; 3]>,
    pub crop_aspect: Option<f32>,
    pub crop_fill: Option<[u8; 3]>,
    pub show_rulers: bool,
    pub show_grid: bool,
    pub grid_spacing_px: f32,
    pub grid_subdivisions: u32,
    pub grid_opacity: f32,
    pub show_guides: bool,
    pub lock_guides: bool,
    pub guides: Vec<photon_canvas::EditorGuide>,
    pub next_guide_id: u64,
    pub pending_guide_preview: Option<(photon_canvas::GuideAxis, f32)>,
    pub canvas_viewport_size: Option<[f32; 2]>,
    pub snap: photon_canvas::SnapConfig,
    pub compare_mode: CompareMode,
    pub compare_split_x: f32,
    pub renaming_layer_id: Option<u64>,
    pub layer_rename_input: String,
    pub marquee_subtool: EditorTool,
    pub lasso_subtool: EditorTool,
    pub smart_subtool: EditorTool,
    pub select_op: photon_protocol::SelectionBooleanOp,
    pub select_feather: f32,
    pub select_anti_alias: bool,
    pub wand_tolerance: u8,
    pub wand_contiguous: bool,
    pub select_sample_all: bool,
    pub quick_select_radius: f32,
    pub shape_subtool: EditorTool,
    pub fill_subtool: EditorTool,
    pub move_subtool: EditorTool,
    pub crop_subtool: EditorTool,
    pub retouch_subtool: EditorTool,
    pub brush_subtool: EditorTool,
    pub pen_subtool: EditorTool,
    pub nav_subtool: EditorTool,
    pub ai_subtool: EditorTool,
    pub shape_fill_mode: ShapeFillMode,
    pub shape_stroke_width: f32,
    pub flood_fill_tolerance: u8,
    pub flood_fill_contiguous: bool,
    pub flood_fill_opacity: f32,
    pub flood_fill_sample_all: bool,
    pub gradient_kind: GradientKind,
    pub gradient_color_mode: GradientColorMode,
    pub gradient_preset_id: String,
    pub gradient_stops: Vec<GradientStop>,
    pub gradient_selected_stop: usize,
    pub gradient_repeat: GradientRepeat,
    pub gradient_blend_mode: BlendMode,
    pub gradient_interactive: bool,
    pub gradient_opacity: f32,
    pub gradient_reverse: bool,
    pub gradient_dither: bool,
    pub gradient_draft: Option<([f32; 2], [f32; 2])>,
    pub healing_mode: HealingMode,
    pub healing_radius: f32,
    pub healing_hardness: f32,
    pub healing_strength: f32,
    pub healing_sample_all: bool,
    pub healing_source_px: Option<[f32; 2]>,
    pub denoise_strength: f32,
    pub denoise_detail: f32,
    pub transform_spec: LayerTransformSpec,
    pub smart_paste: Option<SmartPasteState>,
    pub ai_rembg_model: String,
    pub ai_rembg_feather: u8,
    pub ai_rembg_refine: AlphaRefineMode,
    pub ai_rembg_refine_radius: f32,
    pub ai_rembg_refine_softness: f32,
    pub ai_wm_tab: WatermarkTab,
    pub ai_wm_detector: String,
    pub ai_wm_inpainter: String,
    pub ai_wm_confidence: f32,
    pub ai_wm_mask_padding: u32,
    pub ai_wm_brush_radius: f32,
    pub ai_wm_mask_strokes: Vec<AiMaskStroke>,
    pub ai_wm_region_x: f32,
    pub ai_wm_region_y: f32,
    pub ai_wm_region_w: f32,
    pub ai_wm_region_h: f32,
    pub ai_wm_gemini_size: Option<u32>,
    pub ai_wm_gemini_brightness: f32,
    pub ai_wm_gemini_offset_x: i32,
    pub ai_wm_gemini_offset_y: i32,
    pub ai_wm_gemini_force: bool,
    pub ai_wm_region_feather: u32,
    pub ai_caption_model: String,
    pub ai_caption_general_threshold: f32,
    pub ai_caption_char_threshold: f32,
    pub ai_caption_replace_underscores: bool,
    pub ai_caption_prefix: String,
    pub ai_caption_suffix: String,
    pub ai_caption_exclude: String,
    pub ai_caption_draft: String,
    pub vector_mode: bool,
    pub shape_corner_radius: f32,
    pub selected_vector_node: Option<usize>,
    pub vector_simplify_tolerance: f32,
    pub vector_smooth_tension: f32,
    pub transform_each_spec: photon_protocol::TransformEachSpec,
    pub vector_blend_spec: photon_protocol::VectorBlendSpec,
    pub text_draft: photon_protocol::TextLayerData,
}

impl EditorUiState {
    pub fn effective_canvas_tool(&self) -> CanvasTool {
        match self.tool {
            EditorTool::RemoveBg if self.ai_rembg_refine != AlphaRefineMode::Off => {
                CanvasTool::Brush
            }
            EditorTool::Watermark => match self.ai_wm_tab {
                WatermarkTab::AiAuto if self.ai_wm_detector == "brush" => CanvasTool::Brush,
                WatermarkTab::Region => CanvasTool::MarqueeRect,
                WatermarkTab::AiAuto if self.ai_wm_detector == "none" => CanvasTool::MarqueeRect,
                _ => CanvasTool::Pointer,
            },
            _ => self.tool.as_canvas_tool(),
        }
    }

    pub fn effective_brush_radius(&self) -> f32 {
        match self.tool {
            EditorTool::Brush | EditorTool::Eraser => self.brush_radius,
            EditorTool::HealingBrush => self.healing_radius,
            EditorTool::Liquify => self.liquify_radius,
            EditorTool::QuickSelect => self.quick_select_radius,
            EditorTool::RemoveBg if self.ai_rembg_refine != AlphaRefineMode::Off => {
                self.ai_rembg_refine_radius
            }
            EditorTool::Watermark
                if self.ai_wm_tab == WatermarkTab::AiAuto && self.ai_wm_detector == "brush" =>
            {
                self.ai_wm_brush_radius
            }
            _ => 0.0,
        }
    }

    pub fn raw_gradient_stops(&self) -> Vec<GradientStop> {
        if self.gradient_color_mode == GradientColorMode::Custom && self.gradient_stops.len() >= 2 {
            return self.gradient_stops.clone();
        }
        let bg = self.bg_color.unwrap_or([255, 255, 255]);
        match self.gradient_color_mode {
            GradientColorMode::FgToTransparent => vec![
                GradientStop {
                    pos: 0.0,
                    color: self.fg_color,
                    alpha: 1.0,
                },
                GradientStop {
                    pos: 1.0,
                    color: self.fg_color,
                    alpha: 0.0,
                },
            ],
            GradientColorMode::FgToBg | GradientColorMode::Custom => vec![
                GradientStop {
                    pos: 0.0,
                    color: self.fg_color,
                    alpha: 1.0,
                },
                GradientStop {
                    pos: 1.0,
                    color: bg,
                    alpha: 1.0,
                },
            ],
        }
    }

    pub fn resolved_gradient_stops(&self) -> Vec<GradientStop> {
        let mut list = self.raw_gradient_stops();
        list.sort_by(|a, b| a.pos.total_cmp(&b.pos));
        list
    }

    pub fn ensure_custom_gradient_stops(&mut self) {
        if self.gradient_color_mode != GradientColorMode::Custom || self.gradient_stops.len() < 2 {
            self.gradient_stops = self.resolved_gradient_stops();
            self.gradient_color_mode = GradientColorMode::Custom;
            self.gradient_preset_id = "custom".to_owned();
        }
    }

    pub fn apply_gradient_preset(&mut self, preset_id: &str) {
        self.gradient_preset_id = preset_id.to_owned();
        self.gradient_selected_stop = 0;
        let stop = |pos: f64, hex: &str, alpha: f64| GradientStop {
            pos,
            color: parse_hex_rgb(hex).unwrap_or([0, 0, 0]),
            alpha,
        };
        match preset_id {
            "fg_to_bg" => {
                self.gradient_color_mode = GradientColorMode::FgToBg;
                self.gradient_stops.clear();
            }
            "fg_to_transparent" => {
                self.gradient_color_mode = GradientColorMode::FgToTransparent;
                self.gradient_stops.clear();
            }
            "rainbow" => {
                self.gradient_color_mode = GradientColorMode::Custom;
                self.gradient_stops = vec![
                    stop(0.0, "#ff2a2a", 1.0),
                    stop(0.2, "#ffaa00", 1.0),
                    stop(0.4, "#ffee00", 1.0),
                    stop(0.6, "#22cc44", 1.0),
                    stop(0.8, "#2288ff", 1.0),
                    stop(1.0, "#aa33ff", 1.0),
                ];
            }
            "sunset" => {
                self.gradient_color_mode = GradientColorMode::Custom;
                self.gradient_stops = vec![
                    stop(0.0, "#2b1055", 1.0),
                    stop(0.45, "#b5179e", 1.0),
                    stop(0.78, "#f77f00", 1.0),
                    stop(1.0, "#fcbf49", 1.0),
                ];
            }
            "ocean" => {
                self.gradient_color_mode = GradientColorMode::Custom;
                self.gradient_stops = vec![
                    stop(0.0, "#03045e", 1.0),
                    stop(0.5, "#0077b6", 1.0),
                    stop(1.0, "#90e0ef", 1.0),
                ];
            }
            "fire" => {
                self.gradient_color_mode = GradientColorMode::Custom;
                self.gradient_stops = vec![
                    stop(0.0, "#140404", 1.0),
                    stop(0.35, "#c1121f", 1.0),
                    stop(0.7, "#ff9f1c", 1.0),
                    stop(1.0, "#fff8d6", 1.0),
                ];
            }
            "chrome" => {
                self.gradient_color_mode = GradientColorMode::Custom;
                self.gradient_stops = vec![
                    stop(0.0, "#243040", 1.0),
                    stop(0.45, "#e6eef8", 1.0),
                    stop(0.5, "#3a2e25", 1.0),
                    stop(0.75, "#d4c3a3", 1.0),
                    stop(1.0, "#ffffff", 1.0),
                ];
            }
            "vignette" => {
                self.gradient_color_mode = GradientColorMode::Custom;
                self.gradient_stops = vec![
                    stop(0.0, "#000000", 0.0),
                    stop(0.65, "#000000", 0.15),
                    stop(1.0, "#000000", 0.85),
                ];
            }
            _ => {}
        }
    }
}

impl Default for EditorUiState {
    fn default() -> Self {
        Self {
            tool: EditorTool::Pointer,
            adjust_popup_tab: None,
            fg_color: [24, 24, 27],
            bg_color: Some([255, 255, 255]),
            sampled_color: None,
            brush_radius: 14.0,
            brush_opacity: 1.0,
            brush_settings: BrushSettings::default(),
            liquify_mode: LiquifyMode::Push,
            liquify_radius: 40.0,
            liquify_strength: 0.5,
            stroke_active: false,
            apply_target: ApplyTarget::WholeImage,
            apply_to_selection: true,
            adjust_live_preview: true,
            selected_layer_ids: Vec::new(),
            layer_anchor_id: None,
            dragging_layer_id: None,
            align_target: photon_protocol::AlignTarget::Selection,
            hsv: HsvAdjustment::default(),
            balance: ColorBalanceAdjustment::default(),
            balance_zone: ColorBalanceZone::Midtones,
            curves: default_curves(),
            curves_channel: CurvesChannel::Rgb,
            rotate_angle: 0.0,
            rotate_fill: None,
            canvas_dims: DimensionInputState::default(),
            canvas_anchor: CanvasAnchor::Center,
            canvas_fill: None,
            crop_aspect: None,
            crop_fill: None,
            show_rulers: false,
            show_grid: false,
            grid_spacing_px: 64.0,
            grid_subdivisions: 4,
            grid_opacity: 1.0,
            show_guides: true,
            lock_guides: false,
            guides: Vec::new(),
            next_guide_id: 1,
            pending_guide_preview: None,
            canvas_viewport_size: None,
            snap: photon_canvas::SnapConfig::default(),
            compare_mode: CompareMode::Off,
            compare_split_x: 0.5,
            renaming_layer_id: None,
            layer_rename_input: String::new(),
            marquee_subtool: EditorTool::MarqueeRect,
            lasso_subtool: EditorTool::Lasso,
            smart_subtool: EditorTool::QuickSelect,
            select_op: photon_protocol::SelectionBooleanOp::Replace,
            select_feather: 0.0,
            select_anti_alias: true,
            wand_tolerance: 24,
            wand_contiguous: true,
            select_sample_all: true,
            quick_select_radius: 18.0,
            shape_subtool: EditorTool::DrawRect,
            fill_subtool: EditorTool::PaintBucket,
            move_subtool: EditorTool::Pointer,
            crop_subtool: EditorTool::Crop,
            retouch_subtool: EditorTool::HealingBrush,
            brush_subtool: EditorTool::Brush,
            pen_subtool: EditorTool::BezierPen,
            nav_subtool: EditorTool::Hand,
            ai_subtool: EditorTool::RemoveBg,
            shape_fill_mode: ShapeFillMode::Stroke,
            shape_stroke_width: 3.0,
            flood_fill_tolerance: 24,
            flood_fill_contiguous: true,
            flood_fill_opacity: 1.0,
            flood_fill_sample_all: false,
            gradient_kind: GradientKind::Linear,
            gradient_color_mode: GradientColorMode::FgToBg,
            gradient_preset_id: "fg_to_bg".to_owned(),
            gradient_stops: Vec::new(),
            gradient_selected_stop: 0,
            gradient_repeat: GradientRepeat::None,
            gradient_blend_mode: BlendMode::Normal,
            gradient_interactive: true,
            gradient_opacity: 1.0,
            gradient_reverse: false,
            gradient_dither: true,
            gradient_draft: None,
            healing_mode: HealingMode::Spot,
            healing_radius: 18.0,
            healing_hardness: 0.75,
            healing_strength: 1.0,
            healing_sample_all: false,
            healing_source_px: None,
            denoise_strength: 0.35,
            denoise_detail: 0.6,
            transform_spec: LayerTransformSpec::default(),
            smart_paste: None,
            ai_rembg_model: "birefnet-general".to_owned(),
            ai_rembg_feather: 2,
            ai_rembg_refine: AlphaRefineMode::Off,
            ai_rembg_refine_radius: 24.0,
            ai_rembg_refine_softness: 0.5,
            ai_wm_tab: WatermarkTab::AiAuto,
            ai_wm_detector: "yolo26".to_owned(),
            ai_wm_inpainter: "migan-pipeline-v2".to_owned(),
            ai_wm_confidence: 0.25,
            ai_wm_mask_padding: 8,
            ai_wm_brush_radius: 20.0,
            ai_wm_mask_strokes: Vec::new(),
            ai_wm_region_x: 78.0,
            ai_wm_region_y: 84.0,
            ai_wm_region_w: 20.0,
            ai_wm_region_h: 14.0,
            ai_wm_gemini_size: None,
            ai_wm_gemini_brightness: 0.5,
            ai_wm_gemini_offset_x: 0,
            ai_wm_gemini_offset_y: 0,
            ai_wm_gemini_force: false,
            ai_wm_region_feather: 8,
            ai_caption_model: "wd-vit-tagger-v3".to_owned(),
            ai_caption_general_threshold: 0.35,
            ai_caption_char_threshold: 0.85,
            ai_caption_replace_underscores: true,
            ai_caption_prefix: String::new(),
            ai_caption_suffix: String::new(),
            ai_caption_exclude: String::new(),
            ai_caption_draft: String::new(),
            vector_mode: false,
            shape_corner_radius: 0.0,
            selected_vector_node: None,
            vector_simplify_tolerance: 2.0,
            vector_smooth_tension: 0.33,
            transform_each_spec: photon_protocol::TransformEachSpec::default(),
            vector_blend_spec: photon_protocol::VectorBlendSpec::default(),
            text_draft: photon_protocol::TextLayerData::default(),
        }
    }
}

pub fn default_curves() -> CurvesAdjustment {
    let identity = vec![[0.0, 0.0], [1.0, 1.0]];
    CurvesAdjustment {
        rgb: identity.clone(),
        red: identity.clone(),
        green: identity.clone(),
        blue: identity,
    }
}

impl EditorUiState {
    pub fn sync_document_dims(&mut self, width: u32, height: u32) {
        self.canvas_dims.sync_from_px(width, height);
    }

    pub fn reset_adjustments(&mut self) {
        self.hsv = HsvAdjustment::default();
        self.balance = ColorBalanceAdjustment::default();
        self.curves = default_curves();
    }
}

pub fn rgb_to_color(rgb: [u8; 3]) -> Color {
    Color::from_rgb8(rgb[0], rgb[1], rgb[2])
}

pub fn format_hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

pub fn parse_hex_rgb(input: &str) -> Option<[u8; 3]> {
    let clean = input.trim().trim_start_matches('#');
    if clean.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&clean[0..2], 16).ok()?;
    let g = u8::from_str_radix(&clean[2..4], 16).ok()?;
    let b = u8::from_str_radix(&clean[4..6], 16).ok()?;
    Some([r, g, b])
}

pub fn crop_uv_to_signed(uv: [f32; 4], img_w: u32, img_h: u32) -> (i32, i32, u32, u32) {
    let iw = img_w.max(1);
    let ih = img_h.max(1);
    let x = ((uv[0].clamp(0.0, 1.0) * iw as f32).round() as u32).min(iw - 1);
    let y = ((uv[1].clamp(0.0, 1.0) * ih as f32).round() as u32).min(ih - 1);
    let x2 = ((uv[2].clamp(0.0, 1.0) * iw as f32).round() as u32).clamp(x + 1, iw);
    let y2 = ((uv[3].clamp(0.0, 1.0) * ih as f32).round() as u32).clamp(y + 1, ih);
    (x as i32, y as i32, x2 - x, y2 - y)
}
