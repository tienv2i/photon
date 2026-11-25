//! KogaView-inspired Editor workspace UI (`iced 0.14` + `photon-ui`):
//! Left tool rail + quick edits + FG/BG ColorPair, live GPU shader canvas
//! with Before/After Original compare & live strokes, floating Adjustments popup
//! (UI-locked, canvas-interactive), and 2-stack resizable right Inspector
//! (Top: Tool Options + History, Bottom: Layers + Layer Masks).

use iced::widget::overlay::menu as iced_menu;
use iced::widget::{
    Shader, Space, checkbox, column, container, pick_list, row, rule, scrollable, slider, text,
    text_input,
};
use iced::{Alignment, Border, Color, Element, Fill, Length, Shadow};
use photon_i18n::{tr, tr_ui_text};
use photon_panels::{
    DraggablePanelHeaderSpec, PanelFrameSpec, PanelTabItem, dock_panel_closable, kv_row,
    section_card, sidebar_dock, sidebar_panel, stacked_panel_heights,
};
use photon_protocol::{
    ApplyTarget, BlendMode, BrushSettings, BrushTip, CanvasAnchor, ColorAdjustments,
    ColorBalanceAdjustment, Command, CurvesAdjustment, EditorRightPanel, EditorState,
    EditorToolTab, FlipAxis, HsvAdjustment, Language, LayerEffect, LiquifyMode, Theme,
};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::dialog::DialogSpec;
use photon_ui::editor::{
    DimensionRatioSpec, LayerActionButton, LayerRowSpec, LengthUnit, anchor_grid_3x3,
    checker_swatch, color_pair_widget, dimension_ratio_grid, layer_action_bar, layer_row,
    px_to_unit, spacer_h, tool_rail_button_tipped, unit_to_px,
};
use photon_ui::icons::Icon;
use photon_ui::menu::{
    MenuEntry, MenuItem, MenubarTrigger, menu_panel_entries, menubar_row_hoverable,
};
use photon_ui::theme::{
    PaletteTokens, RADIUS_SM, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, canvas_backdrop_style,
    hairline_rule_style, input_style, slider_style,
};
use photon_ui::toolbar::{WorkspaceTab, toolbar, workspace_switcher_compact};
use photon_ui::{TooltipPosition, with_tooltip, with_tooltip_pos};

use crate::app::{
    ActiveMenu, Message, PhotonGui, WindowRole, toolbar_divider, toolbar_right_utilities,
};
use crate::panel_splitter::{DockSplitterKind, dock_splitter_handle, editor_vertical_splitter};
use crate::shader_canvas::{CanvasTool, ImageCanvasProgram, default_viewport_bg_rgb};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EditorTool {
    #[default]
    Pointer,
    Hand,
    Zoom,
    Brush,
    Eraser,
    Liquify,
    Eyedropper,
    Crop,
    CanvasSize,
    RotateAngle,
}

impl EditorTool {
    pub fn as_canvas_tool(self) -> CanvasTool {
        match self {
            Self::Pointer | Self::CanvasSize => CanvasTool::Pointer,
            Self::Hand => CanvasTool::Hand,
            Self::Zoom => CanvasTool::Zoom,
            Self::Brush => CanvasTool::Brush,
            Self::Eraser => CanvasTool::Eraser,
            Self::Liquify => CanvasTool::Liquify,
            Self::Eyedropper => CanvasTool::Eyedropper,
            Self::Crop => CanvasTool::Crop,
            Self::RotateAngle => CanvasTool::RotateAngle,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AdjustmentPopupTab {
    #[default]
    ColorAndLight,
    Hsv,
    ColorBalance,
    Curves,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CurvesChannel {
    #[default]
    Rgb,
    Red,
    Green,
    Blue,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorBalanceZone {
    Shadows,
    #[default]
    Midtones,
    Highlights,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CompareMode {
    #[default]
    Off,
    Split,
    HoldOriginal,
    SideBySide,
}

impl CompareMode {
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Split => 1,
            Self::SideBySide => 2,
            Self::HoldOriginal => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorPickerTarget {
    Foreground,
    Background,
    CanvasBackground,
    ViewportBackground,
    CropFill,
    CanvasSizeFill,
    RotateFill,
}

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
        }
    }

    pub fn view<'a, F>(&'a self, lang: Language, map_action: F) -> Element<'a, Message>
    where
        F: Fn(DimGridAction) -> Message + Clone + 'a,
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
    pub compare_mode: CompareMode,
    pub compare_split_x: f32,
    pub renaming_layer_id: Option<u64>,
    pub layer_rename_input: String,
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
            compare_mode: CompareMode::Off,
            compare_split_x: 0.5,
            renaming_layer_id: None,
            layer_rename_input: String::new(),
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

pub fn build_editor_toolbar(state: &PhotonGui, _role: WindowRole) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let meta = &state.snapshot.meta;
    let ed = meta.editor.as_ref();
    let cmp_mode = state.editor_ui.compare_mode;

    let menu_open = state.active_menu.is_some();
    let trigger = |label: String, m: ActiveMenu| {
        (
            MenubarTrigger {
                label,
                active: state.active_menu == Some(m),
                on_press: Message::ToggleMenu(m),
            },
            menu_open.then_some(Message::HoverMenu(m)),
        )
    };

    let quick_actions = row![
        button::toolbar_icon(
            Icon::Save,
            ed.is_some_and(|d| d.modified),
            ed.map(|_| Message::Dispatch(Command::EditorSave)),
            Some(tr(lang, "tooltip-editor-save")),
        ),
        button::toolbar_icon(
            Icon::Export,
            false,
            ed.map(|_| Message::OpenExportDialog),
            Some(tr(lang, "tooltip-editor-export")),
        ),
        toolbar_divider(),
        button::toolbar_icon(
            Icon::Undo,
            false,
            ed.filter(|d| d.can_undo)
                .map(|_| Message::Dispatch(Command::EditorUndo)),
            Some(tr(lang, "tooltip-editor-undo")),
        ),
        button::toolbar_icon(
            Icon::Redo,
            false,
            ed.filter(|d| d.can_redo)
                .map(|_| Message::Dispatch(Command::EditorRedo)),
            Some(tr(lang, "tooltip-editor-redo")),
        ),
        button::toolbar_icon(
            Icon::Reset,
            false,
            ed.filter(|d| d.modified)
                .map(|_| Message::Dispatch(Command::EditorReset)),
            Some(tr(lang, "tooltip-history-reset")),
        ),
        toolbar_divider(),
        button::toolbar_icon(
            Icon::Compare,
            ed.is_some() && cmp_mode != CompareMode::Off,
            ed.map(|_| Message::EditorCycleCompareMode),
            Some(match cmp_mode {
                CompareMode::Off => tr(lang, "tooltip-editor-compare"),
                CompareMode::Split => format!(
                    "{} ({})",
                    tr(lang, "action-compare-original"),
                    tr(lang, "label-compare-split")
                ),
                CompareMode::SideBySide => format!(
                    "{} ({})",
                    tr(lang, "action-compare-original"),
                    tr(lang, "label-compare-side-by-side")
                ),
                CompareMode::HoldOriginal => format!(
                    "{} ({})",
                    tr(lang, "action-compare-original"),
                    tr(lang, "label-compare-hold")
                ),
            }),
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let left = row![
        menubar_row_hoverable(vec![
            trigger(tr(lang, "menu-file"), ActiveMenu::EditorFile),
            trigger(tr(lang, "menu-edit"), ActiveMenu::EditorEdit),
            trigger(tr(lang, "menu-image"), ActiveMenu::EditorImage),
            trigger(tr(lang, "menu-filter"), ActiveMenu::EditorFilter),
            trigger(tr(lang, "menu-view"), ActiveMenu::EditorView),
            trigger(tr(lang, "menu-window"), ActiveMenu::EditorWindow),
            trigger(tr(lang, "menu-help"), ActiveMenu::EditorHelp),
        ]),
        toolbar_divider(),
        quick_actions,
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let switcher = workspace_switcher_compact(
        vec![
            WorkspaceTab::new(
                tr(lang, "workspace-browser"),
                Icon::Browser,
                false,
                Message::RequestEditorLeaveToBrowser,
            )
            .tooltip(tr(lang, "tooltip-workspace-browser")),
            WorkspaceTab::new(
                tr(lang, "workspace-viewer"),
                Icon::Viewer,
                false,
                Message::RequestEditorLeave {
                    close_editor: false,
                },
            )
            .tooltip(tr(lang, "tooltip-workspace-viewer")),
            WorkspaceTab::new(
                tr(lang, "workspace-editor"),
                Icon::Editor,
                true,
                Message::Noop,
            )
            .tooltip(tr(lang, "tooltip-workspace-editor")),
        ],
        state.window_size.width < 1120.0,
    );

    let right = row![switcher, toolbar_divider(), toolbar_right_utilities(state)]
        .spacing(8)
        .align_y(Alignment::Center);

    toolbar(left, right)
}

pub fn build_editor_view(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let dark = state.snapshot.meta.theme == Theme::Dark;
    let Some(ed) = &state.snapshot.meta.editor else {
        return container(
            column![
                photon_ui::icons::render(Icon::Editor, 36.0, None),
                text(tr(lang, "label-no-document")).size(14).style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.text_muted),
                    }
                }),
            ]
            .spacing(10)
            .align_x(Alignment::Center),
        )
        .center_x(Fill)
        .center_y(Fill)
        .style(canvas_backdrop_style)
        .into();
    };

    let adjust_open = state.editor_ui.adjust_popup_tab.is_some();
    let tool_rail = build_tool_rail(state, lang);
    let preview_fill = match state.editor_ui.rotate_fill {
        Some([r, g, b]) => [
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            1.0,
        ],
        None => [0.0, 0.0, 0.0, 0.0],
    };
    let preview_angle_rad = if !adjust_open && state.editor_ui.tool == EditorTool::RotateAngle {
        state.editor_ui.rotate_angle.to_radians() as f32
    } else {
        0.0
    };
    let brush_radius_px = if adjust_open {
        0.0
    } else {
        match state.editor_ui.tool {
            EditorTool::Brush | EditorTool::Eraser => state.editor_ui.brush_radius,
            EditorTool::Liquify => state.editor_ui.liquify_radius,
            _ => 0.0,
        }
    };
    // While the cutout Adjustment dialog is active, keep canvas pan/zoom
    // responsive without triggering brush/crop gestures underneath.
    let canvas_tool = if adjust_open {
        CanvasTool::Hand
    } else {
        state.editor_ui.tool.as_canvas_tool()
    };

    let canvas: Element<'_, Message> = Element::from(
        Shader::new(ImageCanvasProgram {
            frame: state.editor_frame.clone(),
            pair_frame: None,
            dual_gap_px: 0.0,
            compare_frame: state.editor_original_frame.clone(),
            zoom: state.zoom,
            pan_x: state.pan_x,
            pan_y: state.pan_y,
            adjustments: state.pending_adj,
            dark_theme: dark,
            crop_rect: state.crop_uv,
            crop_aspect: state.editor_ui.crop_aspect,
            quarter_turns: 0,
            tool: canvas_tool,
            preview_angle_rad,
            preview_fill,
            canvas_bg: ed.canvas_background,
            viewport_bg: state.viewport_background,
            compare_mode: state.editor_ui.compare_mode.as_u8(),
            compare_split_x: state.editor_ui.compare_split_x,
            brush_radius_px,
        })
        .width(Fill)
        .height(Fill),
    )
    .map(Message::Canvas);

    let canvas_area: Element<'_, Message> = container(canvas)
        .width(Fill)
        .height(Fill)
        .style(canvas_backdrop_style)
        .into();

    let show_tools = state.snapshot.meta.layout.panel("editor.tools");
    let show_inspector = (state.editor_history_open || state.editor_layers_open)
        && state.snapshot.meta.layout.panel("editor.inspector");

    let mut editor_row = row![].height(Fill).width(Fill);
    if show_tools {
        editor_row = editor_row
            .push(tool_rail)
            .push(rule::vertical(1).style(hairline_rule_style));
    }
    editor_row = editor_row.push(canvas_area);
    if show_inspector {
        let right_w = state
            .dock_drag
            .effective_editor_right(&state.snapshot.meta.layout);
        let inspector = build_editor_inspector(state, ed, lang, right_w);
        let splitter = dock_splitter_handle(DockSplitterKind::EditorRight, right_w);
        editor_row = editor_row.push(splitter).push(inspector);
    }
    editor_row.into()
}

fn build_tool_rail(state: &PhotonGui, lang: Language) -> Element<'_, Message> {
    let ui = &state.editor_ui;
    let active_tool = ui.tool;

    let tool_grid = column![
        row![
            tool_rail_button_tipped(
                Icon::Pointer,
                active_tool == EditorTool::Pointer,
                true,
                Some(Message::EditorSelectTool(EditorTool::Pointer)),
                tr(lang, "tooltip-tool-pointer"),
            ),
            tool_rail_button_tipped(
                Icon::Hand,
                active_tool == EditorTool::Hand,
                true,
                Some(Message::EditorSelectTool(EditorTool::Hand)),
                tr(lang, "tooltip-tool-hand"),
            ),
        ]
        .spacing(4),
        row![
            tool_rail_button_tipped(
                Icon::ZoomIn,
                active_tool == EditorTool::Zoom,
                true,
                Some(Message::EditorSelectTool(EditorTool::Zoom)),
                tr(lang, "tooltip-tool-zoom"),
            ),
            tool_rail_button_tipped(
                Icon::Eyedropper,
                active_tool == EditorTool::Eyedropper,
                true,
                Some(Message::EditorSelectTool(EditorTool::Eyedropper)),
                tr(lang, "tooltip-tool-eyedropper"),
            ),
        ]
        .spacing(4),
        row![
            tool_rail_button_tipped(
                Icon::Brush,
                active_tool == EditorTool::Brush,
                true,
                Some(Message::EditorSelectTool(EditorTool::Brush)),
                tr(lang, "tooltip-tool-brush"),
            ),
            tool_rail_button_tipped(
                Icon::Eraser,
                active_tool == EditorTool::Eraser,
                true,
                Some(Message::EditorSelectTool(EditorTool::Eraser)),
                tr(lang, "tooltip-tool-eraser"),
            ),
        ]
        .spacing(4),
        row![
            tool_rail_button_tipped(
                Icon::Liquify,
                active_tool == EditorTool::Liquify,
                true,
                Some(Message::EditorSelectTool(EditorTool::Liquify)),
                tr(lang, "tooltip-tool-liquify"),
            ),
            tool_rail_button_tipped(
                Icon::Crop,
                active_tool == EditorTool::Crop || state.crop_uv.is_some(),
                true,
                Some(Message::EditorSelectTool(EditorTool::Crop)),
                tr(lang, "tooltip-tool-crop"),
            ),
        ]
        .spacing(4),
        row![
            tool_rail_button_tipped(
                Icon::Resize,
                false,
                true,
                Some(Message::OpenResizeDialog),
                tr(lang, "tooltip-tool-resize"),
            ),
            tool_rail_button_tipped(
                Icon::CanvasSize,
                active_tool == EditorTool::CanvasSize,
                true,
                Some(Message::EditorSelectTool(EditorTool::CanvasSize)),
                tr(lang, "tooltip-tool-canvas-size"),
            ),
        ]
        .spacing(4),
        row![
            tool_rail_button_tipped(
                Icon::RotateAngle,
                active_tool == EditorTool::RotateAngle,
                true,
                Some(Message::EditorSelectTool(EditorTool::RotateAngle)),
                tr(lang, "tooltip-tool-rotate-angle"),
            ),
            Space::new().width(Length::Fixed(34.0)),
        ]
        .spacing(4),
    ]
    .spacing(4)
    .align_x(Alignment::Center);

    let quick_edits = column![
        row![
            tool_rail_button_tipped(
                Icon::RotateLeft,
                false,
                true,
                Some(Message::Dispatch(Command::EditorRotate {
                    degrees: -90,
                    target: ui.apply_target,
                })),
                tr(lang, "action-rotate-left"),
            ),
            tool_rail_button_tipped(
                Icon::RotateRight,
                false,
                true,
                Some(Message::Dispatch(Command::EditorRotate {
                    degrees: 90,
                    target: ui.apply_target,
                })),
                tr(lang, "action-rotate-right"),
            ),
        ]
        .spacing(4),
        row![
            tool_rail_button_tipped(
                Icon::FlipH,
                false,
                true,
                Some(Message::Dispatch(Command::EditorFlip {
                    axis: FlipAxis::Horizontal,
                    target: ui.apply_target,
                })),
                tr(lang, "action-flip-h"),
            ),
            tool_rail_button_tipped(
                Icon::FlipV,
                false,
                true,
                Some(Message::Dispatch(Command::EditorFlip {
                    axis: FlipAxis::Vertical,
                    target: ui.apply_target,
                })),
                tr(lang, "action-flip-v"),
            ),
        ]
        .spacing(4),
    ]
    .spacing(4)
    .align_x(Alignment::Center);

    let color_pair = color_pair_widget(
        rgb_to_color(ui.fg_color),
        ui.bg_color.map(rgb_to_color),
        Message::OpenColorPicker(ColorPickerTarget::Foreground),
        Message::OpenColorPicker(ColorPickerTarget::Background),
        Message::EditorSwapColors,
        Message::EditorResetColors,
    );

    let effective_viewport_rgb = state
        .viewport_background
        .unwrap_or_else(|| default_viewport_bg_rgb(state.snapshot.meta.theme == Theme::Dark));

    let viewport_bg_swatch = with_tooltip_pos(
        checker_swatch(
            Some(rgb_to_color(effective_viewport_rgb)),
            70.0,
            20.0,
            Some(Message::OpenColorPicker(
                ColorPickerTarget::ViewportBackground,
            )),
        ),
        tr(lang, "label-viewport-bg"),
        TooltipPosition::Right,
    );

    dock_panel_closable(
        tr(lang, "label-tools"),
        Some(Icon::Sliders),
        96.0,
        Some(tr(lang, "tooltip-panel-close")),
        Message::CloseEditorToolsPanel,
        column![
            tool_grid,
            rule::horizontal(1).style(hairline_rule_style),
            quick_edits,
            rule::horizontal(1).style(hairline_rule_style),
            color_pair,
            viewport_bg_swatch,
        ]
        .spacing(8)
        .align_x(Alignment::Center),
    )
}

fn build_editor_inspector<'a>(
    state: &'a PhotonGui,
    ed: &'a EditorState,
    lang: Language,
    width: f32,
) -> Element<'a, Message> {
    let split_pct = state
        .dock_drag
        .effective_editor_split_pct(&state.snapshot.meta.layout);

    let visible_panels: Vec<EditorRightPanel> = state
        .editor_panels
        .order()
        .iter()
        .copied()
        .filter(|panel| match panel {
            EditorRightPanel::ToolOptionsHistory => state.editor_history_open,
            EditorRightPanel::Layers => state.editor_layers_open,
        })
        .collect();

    let is_collapsed = |panel: EditorRightPanel| match panel {
        EditorRightPanel::ToolOptionsHistory => state.editor_history_collapsed,
        EditorRightPanel::Layers => state.editor_layers_collapsed,
    };

    let mut stacked = column![].spacing(6).height(Fill).width(Fill);
    if visible_panels.len() == 2 {
        let p0 = visible_panels[0];
        let p1 = visible_panels[1];
        let c0 = is_collapsed(p0);
        let c1 = is_collapsed(p1);
        let (h0, h1) = stacked_panel_heights(c0, c1, split_pct, 20.0, 80.0);

        let s0 = build_right_panel_section(state, ed, lang, width, p0, c0, true);
        let s1 = build_right_panel_section(state, ed, lang, width, p1, c1, true);

        stacked = stacked.push(container(s0).width(Fill).height(h0));
        if !c0 && !c1 {
            stacked = stacked.push(editor_vertical_splitter(
                split_pct,
                state.window_size.height,
            ));
        }
        stacked = stacked.push(container(s1).width(Fill).height(h1));
        if c0 && c1 {
            stacked = stacked.push(iced::widget::space::vertical());
        }
    } else if let Some(&p0) = visible_panels.first() {
        let c0 = is_collapsed(p0);
        let h0 = if c0 { Length::Shrink } else { Length::Fill };
        let s0 = build_right_panel_section(state, ed, lang, width, p0, c0, false);
        stacked = stacked.push(container(s0).width(Fill).height(h0));
        if c0 {
            stacked = stacked.push(iced::widget::space::vertical());
        }
    }

    sidebar_dock(width, stacked)
}

fn build_right_panel_section<'a>(
    state: &'a PhotonGui,
    ed: &'a EditorState,
    lang: Language,
    sidebar_width: f32,
    panel_kind: EditorRightPanel,
    collapsed: bool,
    can_reorder: bool,
) -> Element<'a, Message> {
    let dragging_any = state.editor_panels.is_dragging();
    let is_dragging_self = state.editor_panels.is_dragging_panel(panel_kind);
    let drop_ind = state.editor_panels.drop_indicator(panel_kind);

    let mut header = match panel_kind {
        EditorRightPanel::ToolOptionsHistory => DraggablePanelHeaderSpec::with_tabs(vec![
            PanelTabItem::new(
                "options",
                tr(lang, "tab-tool-options"),
                state.editor_tool_tab == EditorToolTab::Options,
                Message::SelectEditorToolTab(EditorToolTab::Options),
            )
            .icon(Icon::Sliders)
            .tooltip(tr(lang, "tab-tool-options")),
            PanelTabItem::new(
                "history",
                tr(lang, "tab-history"),
                state.editor_tool_tab == EditorToolTab::History,
                Message::SelectEditorToolTab(EditorToolTab::History),
            )
            .icon(Icon::History)
            .tooltip(tr(lang, "tab-history")),
        ])
        .compact_tabs(sidebar_width < 245.0),
        EditorRightPanel::Layers => DraggablePanelHeaderSpec::new(tr(lang, "label-layers"))
            .icon(Icon::Layers)
            .badge(ed.layers.len().to_string()),
    };

    header = header
        .collapsed(collapsed)
        .dragging(is_dragging_self)
        .collapse_tooltip(tr(lang, "tooltip-panel-collapse"))
        .close_tooltip(tr(lang, "tooltip-panel-close"))
        .on_toggle_collapse(Message::ToggleEditorPanelCollapse(panel_kind))
        .on_close(Some(Message::CloseEditorPanel(panel_kind)));

    if can_reorder {
        header = header
            .drag_tooltip(tr(lang, "tooltip-sidebar-panel-drag"))
            .swap_tooltip(tr(lang, "tooltip-panel-drag-reorder"))
            .on_drag_start(Message::BeginDragEditorPanel(panel_kind))
            .on_drag_hover(dragging_any.then_some(Message::HoverEditorPanelDrop(panel_kind)))
            .on_drag_end(dragging_any.then_some(Message::DropEditorPanel(panel_kind)))
            .on_swap(Some(Message::SwapEditorPanelOrder));
    }

    let body = (!collapsed).then(|| match panel_kind {
        EditorRightPanel::ToolOptionsHistory => build_tool_options_history_panel(state, ed, lang),
        EditorRightPanel::Layers => build_layers_panel(state, ed, lang),
    });

    sidebar_panel(
        PanelFrameSpec::new(header)
            .maybe_body(body)
            .fill_height(!collapsed)
            .drop_indicator(drop_ind)
            .on_drop_hover(dragging_any.then_some(Message::HoverEditorPanelDrop(panel_kind)))
            .on_drop_release(dragging_any.then_some(Message::DropEditorPanel(panel_kind))),
    )
}

fn build_tool_options_history_panel<'a>(
    state: &'a PhotonGui,
    ed: &'a EditorState,
    lang: Language,
) -> Element<'a, Message> {
    let tab_body: Element<'a, Message> = match state.editor_tool_tab {
        EditorToolTab::Options => {
            if let Some(tool_card) = build_tool_options_card(state, ed, lang) {
                tool_card
            } else {
                let tool_title = match state.editor_ui.tool {
                    EditorTool::Hand => tr(lang, "tool-hand"),
                    EditorTool::Zoom => tr(lang, "tool-zoom"),
                    _ => tr(lang, "tool-pointer"),
                };
                section_card(
                    tool_title,
                    text("No extra options for this tool.")
                        .size(TEXT_SIZE_CAPTION)
                        .style(|theme| {
                            let t = PaletteTokens::for_theme(theme);
                            text::Style {
                                color: Some(t.text_muted),
                            }
                        }),
                )
            }
        }
        EditorToolTab::History => build_history_panel(ed, lang),
    };

    scrollable(tab_body).height(Fill).width(Fill).into()
}

fn build_tool_options_card<'a>(
    state: &'a PhotonGui,
    ed: &'a EditorState,
    lang: Language,
) -> Option<Element<'a, Message>> {
    let ui = &state.editor_ui;

    if let Some(uv) = state.crop_uv {
        let (cx, cy, cw, ch) = crop_uv_to_signed(uv, ed.width, ed.height);
        return Some(section_card(
            tr(lang, "action-crop"),
            column![
                kv_row(
                    tr(lang, "label-dimensions"),
                    format!("{cw} × {ch} @ ({cx}, {cy})")
                ),
                row![
                    button::labeled(
                        "Free",
                        None,
                        ButtonVariant::Tab {
                            active: ui.crop_aspect.is_none(),
                        },
                        Some(Message::SetCropPreset(None)),
                    ),
                    button::labeled(
                        "1:1",
                        None,
                        ButtonVariant::Tab {
                            active: ui.crop_aspect == Some(1.0),
                        },
                        Some(Message::SetCropPreset(Some(1.0))),
                    ),
                    button::labeled(
                        "4:3",
                        None,
                        ButtonVariant::Tab {
                            active: ui.crop_aspect.is_some_and(|a| (a - 4.0 / 3.0).abs() < 0.01),
                        },
                        Some(Message::SetCropPreset(Some(4.0 / 3.0))),
                    ),
                    button::labeled(
                        "16:9",
                        None,
                        ButtonVariant::Tab {
                            active: ui
                                .crop_aspect
                                .is_some_and(|a| (a - 16.0 / 9.0).abs() < 0.01),
                        },
                        Some(Message::SetCropPreset(Some(16.0 / 9.0))),
                    ),
                ]
                .spacing(4),
                row![
                    text(tr(lang, "label-canvas-bg")).size(12).width(Fill),
                    checker_swatch(
                        ui.crop_fill.map(rgb_to_color),
                        24.0,
                        18.0,
                        Some(Message::OpenColorPicker(ColorPickerTarget::CropFill)),
                    ),
                ]
                .align_y(Alignment::Center),
                row![
                    button::labeled(
                        tr(lang, "dialog-cancel"),
                        None,
                        ButtonVariant::Ghost,
                        Some(Message::CancelCrop),
                    ),
                    with_tooltip_pos(
                        button::labeled(
                            tr(lang, "action-apply-crop"),
                            Some(Icon::Check),
                            ButtonVariant::Primary,
                            Some(Message::ApplyCrop),
                        )
                        .width(Fill),
                        tr(lang, "action-apply-crop"),
                        TooltipPosition::Top,
                    ),
                ]
                .spacing(6),
            ]
            .spacing(8),
        ));
    }

    match ui.tool {
        EditorTool::Brush | EditorTool::Eraser => {
            let title = if ui.tool == EditorTool::Eraser {
                tr(lang, "tool-eraser")
            } else {
                tr(lang, "tool-brush")
            };
            let tip_label = |tip: BrushTip| match tip {
                BrushTip::Round => tr(lang, "tip-round"),
                BrushTip::Flat => tr(lang, "tip-flat"),
                BrushTip::Chalk => tr(lang, "tip-chalk"),
                BrushTip::Spray => tr(lang, "tip-spray"),
                BrushTip::Pixel => tr(lang, "tip-pixel"),
            };
            let mut tip_buttons: Vec<Element<'_, Message>> = BrushTip::ALL
                .into_iter()
                .map(|tip| {
                    button::labeled(
                        tip_label(tip),
                        None,
                        ButtonVariant::Tab {
                            active: ui.brush_settings.tip == tip,
                        },
                        Some(Message::EditorSetBrushTip(tip)),
                    )
                    .width(Fill)
                    .into()
                })
                .collect();
            let second_tips = tip_buttons.split_off(3);
            let tips_grid =
                column![row(tip_buttons).spacing(4), row(second_tips).spacing(4),].spacing(4);
            Some(section_card(
                title,
                column![
                    kv_row(
                        tr(lang, "label-brush-size"),
                        format!("{:.0} px", ui.brush_radius)
                    ),
                    slider(1.0..=120.0, ui.brush_radius, Message::EditorSetBrushRadius)
                        .step(1.0_f32)
                        .style(slider_style),
                    kv_row(
                        tr(lang, "label-brush-opacity"),
                        format!("{:.0}%", ui.brush_opacity * 100.0)
                    ),
                    slider(0.05..=1.0, ui.brush_opacity, Message::EditorSetBrushOpacity)
                        .step(0.05_f32)
                        .style(slider_style),
                    kv_row(
                        tr(lang, "label-brush-hardness"),
                        format!("{:.0}%", ui.brush_settings.hardness * 100.0)
                    ),
                    slider(
                        0.0..=1.0,
                        ui.brush_settings.hardness,
                        Message::EditorSetBrushHardness
                    )
                    .step(0.05_f32)
                    .style(slider_style),
                    kv_row(
                        tr(lang, "label-brush-flow"),
                        format!("{:.0}%", ui.brush_settings.flow * 100.0)
                    ),
                    slider(
                        0.05..=1.0,
                        ui.brush_settings.flow,
                        Message::EditorSetBrushFlow
                    )
                    .step(0.05_f32)
                    .style(slider_style),
                    kv_row(
                        tr(lang, "label-brush-spacing"),
                        format!("{:.0}%", ui.brush_settings.spacing * 100.0)
                    ),
                    slider(
                        0.05..=1.0,
                        ui.brush_settings.spacing,
                        Message::EditorSetBrushSpacing
                    )
                    .step(0.05_f32)
                    .style(slider_style),
                    text(tr(lang, "label-brush-tip")).size(11),
                    tips_grid,
                ]
                .spacing(6),
            ))
        }
        EditorTool::Liquify => {
            let mode_label = |m: LiquifyMode| match m {
                LiquifyMode::Push => tr(lang, "liquify-push"),
                LiquifyMode::Bloat => tr(lang, "liquify-bloat"),
                LiquifyMode::Pucker => tr(lang, "liquify-pucker"),
                LiquifyMode::TwirlCw => tr(lang, "liquify-twirl-cw"),
                LiquifyMode::TwirlCcw => tr(lang, "liquify-twirl-ccw"),
                LiquifyMode::Reconstruct => tr(lang, "liquify-reconstruct"),
            };
            let mut mode_btns: Vec<Element<'_, Message>> = LiquifyMode::ALL
                .into_iter()
                .map(|m| {
                    button::labeled(
                        mode_label(m),
                        None,
                        ButtonVariant::Tab {
                            active: ui.liquify_mode == m,
                        },
                        Some(Message::EditorSetLiquifyMode(m)),
                    )
                    .width(Fill)
                    .into()
                })
                .collect();
            let row3 = mode_btns.split_off(4);
            let row2 = mode_btns.split_off(2);
            let mode_grid = column![
                row(mode_btns).spacing(4),
                row(row2).spacing(4),
                row(row3).spacing(4),
            ]
            .spacing(4);
            Some(section_card(
                tr(lang, "tool-liquify"),
                column![
                    mode_grid,
                    kv_row(
                        tr(lang, "label-brush-size"),
                        format!("{:.0} px", ui.liquify_radius)
                    ),
                    slider(
                        5.0..=150.0,
                        ui.liquify_radius,
                        Message::EditorSetLiquifyRadius
                    )
                    .step(1.0_f32)
                    .style(slider_style),
                    kv_row(
                        tr(lang, "label-strength"),
                        format!("{:.0}%", ui.liquify_strength * 100.0)
                    ),
                    slider(
                        0.05..=1.0,
                        ui.liquify_strength,
                        Message::EditorSetLiquifyStrength
                    )
                    .step(0.05_f32)
                    .style(slider_style),
                ]
                .spacing(6),
            ))
        }
        EditorTool::Eyedropper => {
            let swatch = ui.sampled_color.map(|c| Color::from_rgb8(c[0], c[1], c[2]));
            let hex = ui
                .sampled_color
                .map(|c| format!("#{:02X}{:02X}{:02X} (A:{})", c[0], c[1], c[2], c[3]))
                .unwrap_or_else(|| "-".to_owned());
            Some(section_card(
                tr(lang, "tool-eyedropper"),
                row![
                    checker_swatch(swatch, 26.0, 26.0, None),
                    column![
                        text(tr(lang, "label-sampled-color")).size(11),
                        text(hex).size(12),
                    ]
                    .spacing(2),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ))
        }
        EditorTool::RotateAngle => Some(section_card(
            tr(lang, "tool-rotate-angle"),
            column![
                kv_row(tr(lang, "label-angle"), format!("{:.1}°", ui.rotate_angle)),
                slider(-180.0..=180.0, ui.rotate_angle as f32, |v| {
                    Message::EditorSetRotateAngle(f64::from(v))
                })
                .step(0.5_f32)
                .style(slider_style),
                row![
                    button::labeled(
                        "-45°",
                        None,
                        ButtonVariant::Ghost,
                        Some(Message::EditorSetRotateAngle(-45.0)),
                    ),
                    button::labeled(
                        "-15°",
                        None,
                        ButtonVariant::Ghost,
                        Some(Message::EditorSetRotateAngle(-15.0)),
                    ),
                    button::labeled(
                        "0°",
                        None,
                        ButtonVariant::Ghost,
                        Some(Message::EditorSetRotateAngle(0.0)),
                    ),
                    button::labeled(
                        "+15°",
                        None,
                        ButtonVariant::Ghost,
                        Some(Message::EditorSetRotateAngle(15.0)),
                    ),
                    button::labeled(
                        "+45°",
                        None,
                        ButtonVariant::Ghost,
                        Some(Message::EditorSetRotateAngle(45.0)),
                    ),
                ]
                .spacing(3),
                row![
                    text(tr(lang, "label-canvas-bg")).size(12).width(Fill),
                    checker_swatch(
                        ui.rotate_fill.map(rgb_to_color),
                        24.0,
                        18.0,
                        Some(Message::OpenColorPicker(ColorPickerTarget::RotateFill)),
                    ),
                ]
                .align_y(Alignment::Center),
                with_tooltip_pos(
                    button::labeled(
                        tr(lang, "action-apply-rotate"),
                        Some(Icon::Check),
                        ButtonVariant::Primary,
                        (ui.rotate_angle.abs() > 0.01).then_some(Message::EditorApplyRotateAngle),
                    )
                    .width(Fill),
                    tr(lang, "action-apply-rotate"),
                    TooltipPosition::Top,
                ),
            ]
            .spacing(8),
        )),
        EditorTool::CanvasSize => {
            let pw = i64::from(ui.canvas_dims.width_px);
            let ph = i64::from(ui.canvas_dims.height_px);
            let dx = pw - i64::from(ed.width);
            let dy = ph - i64::from(ed.height);
            let delta_str = format!(
                "{}×{} ({dx:+} × {dy:+})",
                ui.canvas_dims.width_px, ui.canvas_dims.height_px
            );
            Some(section_card(
                tr(lang, "tool-canvas-size"),
                column![
                    ui.canvas_dims.view(lang, Message::EditorCanvasDimAction),
                    kv_row(tr(lang, "label-delta"), delta_str),
                    row![
                        column![
                            text(tr(lang, "label-anchor")).size(11),
                            anchor_grid_3x3(ui.canvas_anchor.index(), |idx| {
                                Message::EditorSetCanvasAnchor(CanvasAnchor::from_index(idx))
                            }),
                        ]
                        .spacing(4),
                        spacer_h(8.0),
                        column![
                            text(tr(lang, "label-canvas-bg")).size(11),
                            checker_swatch(
                                ui.canvas_fill.map(rgb_to_color),
                                32.0,
                                24.0,
                                Some(Message::OpenColorPicker(ColorPickerTarget::CanvasSizeFill)),
                            ),
                        ]
                        .spacing(4),
                    ]
                    .align_y(Alignment::Start),
                    with_tooltip_pos(
                        button::labeled(
                            tr(lang, "action-canvas-size"),
                            Some(Icon::Check),
                            ButtonVariant::Primary,
                            Some(Message::EditorApplyCanvasSize),
                        )
                        .width(Fill),
                        tr(lang, "action-canvas-size"),
                        TooltipPosition::Top,
                    ),
                ]
                .spacing(8),
            ))
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlendChoice {
    mode: BlendMode,
    label: String,
}

impl std::fmt::Display for BlendChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

fn build_layers_panel<'a>(
    state: &'a PhotonGui,
    ed: &'a EditorState,
    lang: Language,
) -> Element<'a, Message> {
    let ui = &state.editor_ui;
    let active_layer = ed
        .layers
        .iter()
        .find(|l| l.id == ed.active_layer_id)
        .or_else(|| ed.layers.first());

    let active_idx = ed
        .layers
        .iter()
        .position(|l| l.id == ed.active_layer_id)
        .unwrap_or(0);
    let active_unlocked = active_layer.is_some_and(|l| !l.locked);
    let below_unlocked = active_idx > 0
        && ed
            .layers
            .get(active_idx - 1)
            .is_some_and(|below| !below.locked);
    let can_move_up = active_idx + 1 < ed.layers.len();
    let can_move_down = active_idx > 0;
    let can_delete = ed.layers.len() > 1 && active_unlocked;
    let can_merge_down = active_idx > 0 && active_unlocked && below_unlocked;

    let blend_label = |b: BlendMode| -> String {
        match b {
            BlendMode::Normal => tr(lang, "blend-normal"),
            BlendMode::Multiply => tr(lang, "blend-multiply"),
            BlendMode::Screen => tr(lang, "blend-screen"),
            BlendMode::Overlay => tr(lang, "blend-overlay"),
            BlendMode::Darken => tr(lang, "blend-darken"),
            BlendMode::Lighten => tr(lang, "blend-lighten"),
            BlendMode::Difference => tr(lang, "blend-difference"),
        }
    };

    let mut props_col = column![].spacing(5);
    if let Some(layer) = active_layer {
        let layer_id = layer.id;
        let blend_options: Vec<BlendChoice> = BlendMode::ALL
            .into_iter()
            .map(|mode| BlendChoice {
                mode,
                label: blend_label(mode),
            })
            .collect();
        let selected_blend = blend_options
            .iter()
            .find(|c| c.mode == layer.blend_mode)
            .cloned();

        let blend_picker = with_tooltip_pos(
            pick_list(blend_options, selected_blend, move |choice: BlendChoice| {
                Message::Dispatch(Command::EditorLayerSetProps {
                    id: layer_id,
                    name: None,
                    visible: None,
                    locked: None,
                    opacity: None,
                    blend_mode: Some(choice.mode),
                    offset_x: None,
                    offset_y: None,
                })
            })
            .width(Length::Fixed(108.0))
            .text_size(TEXT_SIZE_CONTROL)
            .padding([4, 8])
            .style(|theme, status| {
                let t = PaletteTokens::for_theme(theme);
                let active = matches!(
                    status,
                    pick_list::Status::Hovered | pick_list::Status::Opened { .. }
                );
                pick_list::Style {
                    text_color: t.text_primary,
                    placeholder_color: t.text_muted,
                    handle_color: t.text_muted,
                    background: if active { t.bg_hover } else { t.bg_app }.into(),
                    border: Border {
                        color: if active { t.accent } else { t.border },
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
            }),
            tr(lang, "label-blend-mode"),
            TooltipPosition::Top,
        );

        let opacity_row = row![
            blend_picker,
            with_tooltip_pos(
                slider(0.0..=1.0, layer.opacity, move |v| {
                    Message::Dispatch(Command::EditorLayerSetProps {
                        id: layer_id,
                        name: None,
                        visible: None,
                        locked: None,
                        opacity: Some(v),
                        blend_mode: None,
                        offset_x: None,
                        offset_y: None,
                    })
                })
                .step(0.02_f32)
                .style(slider_style)
                .width(Fill),
                tr(lang, "label-opacity"),
                TooltipPosition::Top,
            ),
            text(format!("{:.0}%", layer.opacity * 100.0))
                .size(11)
                .width(Length::Fixed(34.0))
                .style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.text_muted),
                    }
                }),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        props_col = props_col.push(opacity_row);

        if let Some(effect) = &layer.effect {
            let effect_ctrl: Option<Element<'_, Message>> = match effect {
                LayerEffect::Blur { radius } => Some(
                    row![
                        text(tr(lang, "label-radius"))
                            .size(11)
                            .style(|theme| text::Style {
                                color: Some(PaletteTokens::for_theme(theme).text_muted),
                            }),
                        slider(1.0..=24.0, *radius, move |v| {
                            Message::Dispatch(Command::EditorLayerUpdateEffect {
                                id: layer_id,
                                effect: LayerEffect::Blur { radius: v },
                            })
                        })
                        .step(0.5_f32)
                        .style(slider_style)
                        .width(Fill),
                        text(format!("{radius:.0} px")).size(11),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .into(),
                ),
                LayerEffect::Sharpen { amount } => Some(
                    row![
                        text(tr(lang, "label-strength"))
                            .size(11)
                            .style(|theme| text::Style {
                                color: Some(PaletteTokens::for_theme(theme).text_muted),
                            }),
                        slider(0.0..=2.5, *amount, move |v| {
                            Message::Dispatch(Command::EditorLayerUpdateEffect {
                                id: layer_id,
                                effect: LayerEffect::Sharpen { amount: v },
                            })
                        })
                        .step(0.1_f32)
                        .style(slider_style)
                        .width(Fill),
                        text(format!("{amount:.1}")).size(11),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .into(),
                ),
                LayerEffect::ColorLight(_)
                | LayerEffect::Hsv(_)
                | LayerEffect::ColorBalance(_)
                | LayerEffect::Curves(_) => {
                    let tab = match effect {
                        LayerEffect::ColorLight(_) => AdjustmentPopupTab::ColorAndLight,
                        LayerEffect::Hsv(_) => AdjustmentPopupTab::Hsv,
                        LayerEffect::ColorBalance(_) => AdjustmentPopupTab::ColorBalance,
                        LayerEffect::Curves(_) => AdjustmentPopupTab::Curves,
                        _ => unreachable!(),
                    };
                    Some(
                        button::labeled(
                            tr(lang, "action-edit-effect"),
                            Some(Icon::Sliders),
                            ButtonVariant::Secondary,
                            (!layer.locked).then_some(Message::EditorOpenAdjustPopup(tab)),
                        )
                        .padding([3, 8])
                        .width(Fill)
                        .into(),
                    )
                }
                _ => None,
            };
            if let Some(el) = effect_ctrl {
                props_col = props_col.push(el);
            }
        }

        // Photoshop-style single Layer Mask quick controls for active layer
        let mask_controls: Element<'_, Message> = if layer.has_mask {
            row![
                with_tooltip_pos(
                    button::labeled(
                        tr(
                            lang,
                            if layer.editing_mask {
                                "action-mask-edit-pixels"
                            } else {
                                "action-mask-edit"
                            }
                        ),
                        Some(Icon::Mask),
                        ButtonVariant::Tab {
                            active: layer.editing_mask,
                        },
                        Some(Message::Dispatch(Command::EditorLayerMaskSetEditing {
                            id: layer_id,
                            editing: !layer.editing_mask,
                        })),
                    )
                    .padding([3, 6])
                    .width(Fill),
                    tr(lang, "tooltip-layer-mask"),
                    TooltipPosition::Top,
                ),
                button::compact_icon_tipped(
                    if layer.mask_enabled {
                        Icon::Eye
                    } else {
                        Icon::EyeOff
                    },
                    ButtonVariant::Tab {
                        active: layer.mask_enabled,
                    },
                    Some(Message::Dispatch(Command::EditorLayerMaskSetEnabled {
                        id: layer_id,
                        enabled: !layer.mask_enabled,
                    })),
                    tr(
                        lang,
                        if layer.mask_enabled {
                            "action-mask-disable"
                        } else {
                            "action-mask-enable"
                        }
                    ),
                ),
                button::compact_icon_tipped(
                    if layer.mask_linked {
                        Icon::Lock
                    } else {
                        Icon::Unlock
                    },
                    ButtonVariant::Tab {
                        active: layer.mask_linked,
                    },
                    Some(Message::Dispatch(Command::EditorLayerMaskSetLinked {
                        id: layer_id,
                        linked: !layer.mask_linked,
                    })),
                    tr(
                        lang,
                        if layer.mask_linked {
                            "tooltip-mask-linked"
                        } else {
                            "tooltip-mask-unlinked"
                        }
                    ),
                ),
                button::compact_icon_tipped(
                    Icon::Swap,
                    ButtonVariant::Ghost,
                    (!layer.locked).then_some(Message::Dispatch(Command::EditorLayerMaskInvert {
                        id: layer_id,
                    })),
                    tr(lang, "action-mask-invert"),
                ),
                button::compact_icon_tipped(
                    Icon::Check,
                    ButtonVariant::Ghost,
                    (!layer.locked).then_some(Message::Dispatch(Command::EditorLayerMaskApply {
                        id: layer_id,
                    })),
                    tr(lang, "action-mask-apply"),
                ),
                button::compact_icon_tipped(
                    Icon::Trash,
                    ButtonVariant::Ghost,
                    (!layer.locked).then_some(Message::Dispatch(Command::EditorLayerMaskDelete {
                        id: layer_id,
                    })),
                    tr(lang, "action-mask-delete"),
                ),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
        } else {
            with_tooltip_pos(
                button::labeled(
                    tr(lang, "action-mask-add"),
                    Some(Icon::Mask),
                    ButtonVariant::Secondary,
                    (!layer.locked).then_some(Message::Dispatch(Command::EditorLayerMaskAdd {
                        id: layer_id,
                        reveal_all: true,
                    })),
                )
                .padding([3, 8])
                .width(Fill),
                tr(lang, "action-mask-add"),
                TooltipPosition::Top,
            )
        };
        props_col = props_col.push(mask_controls);
    }

    let mut list_col = column![].spacing(4).width(Fill);
    for layer in ed.layers.iter().rev() {
        let is_active = layer.id == ed.active_layer_id;
        let is_renaming = ui.renaming_layer_id == Some(layer.id);
        let subtitle = if let Some(eff) = &layer.effect {
            let kind = tr(
                lang,
                if eff.is_filter() {
                    "layer-kind-filter"
                } else {
                    "layer-kind-adjustment"
                },
            );
            Some(format!("{kind} · {:.0}%", layer.opacity * 100.0))
        } else if layer.opacity < 0.999 || layer.blend_mode != BlendMode::Normal {
            Some(format!(
                "{} · {:.0}%",
                blend_label(layer.blend_mode),
                layer.opacity * 100.0
            ))
        } else {
            Some(format!("{}×{}", layer.width, layer.height))
        };

        let on_select = if is_active && layer.has_mask && layer.editing_mask {
            Message::Dispatch(Command::EditorLayerMaskSetEditing {
                id: layer.id,
                editing: false,
            })
        } else {
            Message::Dispatch(Command::EditorLayerSelect { id: layer.id })
        };

        let row_el = layer_row(LayerRowSpec {
            name: layer.name.clone(),
            subtitle,
            active: is_active,
            visible: layer.visible,
            locked: layer.locked,
            has_mask: layer.has_mask,
            mask_enabled: layer.mask_enabled,
            mask_linked: layer.mask_linked,
            editing_mask: layer.editing_mask,
            thumbnail: state.editor_layer_thumbs.get(&layer.id).cloned(),
            mask_thumbnail: state.editor_mask_thumbs.get(&layer.id).cloned(),
            renaming: is_renaming.then_some(ui.layer_rename_input.as_str()),
            on_select,
            on_toggle_visible: Message::Dispatch(Command::EditorLayerSetProps {
                id: layer.id,
                name: None,
                visible: Some(!layer.visible),
                locked: None,
                opacity: None,
                blend_mode: None,
                offset_x: None,
                offset_y: None,
            }),
            on_toggle_lock: Message::Dispatch(Command::EditorLayerSetProps {
                id: layer.id,
                name: None,
                visible: None,
                locked: Some(!layer.locked),
                opacity: None,
                blend_mode: None,
                offset_x: None,
                offset_y: None,
            }),
            on_toggle_mask_edit: layer.has_mask.then_some(Message::Dispatch(
                Command::EditorLayerMaskSetEditing {
                    id: layer.id,
                    editing: !(is_active && layer.editing_mask),
                },
            )),
            on_toggle_mask_link: layer.has_mask.then_some(Message::Dispatch(
                Command::EditorLayerMaskSetLinked {
                    id: layer.id,
                    linked: !layer.mask_linked,
                },
            )),
            on_toggle_mask: (!layer.locked).then(|| {
                if layer.has_mask {
                    Message::Dispatch(Command::EditorLayerMaskDelete { id: layer.id })
                } else {
                    Message::Dispatch(Command::EditorLayerMaskAdd {
                        id: layer.id,
                        reveal_all: true,
                    })
                }
            }),
            on_start_rename: Some(Message::EditorStartLayerRename(
                layer.id,
                layer.name.clone(),
            )),
            on_rename_input: Some(Box::new(Message::EditorLayerRenameChanged)),
            on_rename_submit: Some(Message::EditorCommitLayerRename),
            on_right_press: Some(Message::OpenLayerContextMenu { layer_id: layer.id }),
            visibility_tooltip: Some(tr(lang, "tooltip-layer-visibility")),
            lock_tooltip: Some(tr(lang, "tooltip-layer-lock")),
            mask_tooltip: Some(tr(lang, "tooltip-layer-mask")),
            mask_link_tooltip: Some(tr(
                lang,
                if layer.mask_linked {
                    "tooltip-mask-linked"
                } else {
                    "tooltip-mask-unlinked"
                },
            )),
            mask_toggle_tooltip: Some(tr(
                lang,
                if layer.has_mask {
                    "action-mask-delete"
                } else {
                    "action-mask-add"
                },
            )),
            rename_tooltip: Some(tr(lang, "action-rename")),
        });

        if is_renaming {
            list_col = list_col.push(
                row![
                    container(row_el).width(Fill),
                    button::compact_icon_tipped(
                        Icon::Close,
                        ButtonVariant::Ghost,
                        Some(Message::EditorCancelLayerRename),
                        tr(lang, "action-cancel"),
                    ),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            );
        } else {
            list_col = list_col.push(row_el);
        }
    }

    // Dedicated Canvas Background row below layers (KogaView LayersPanel.qml style)
    let is_bg_transparent = ed.canvas_background.is_none();
    let canvas_bg_row = container(
        row![
            button::compact_icon_tipped(
                if is_bg_transparent {
                    Icon::EyeOff
                } else {
                    Icon::Eye
                },
                ButtonVariant::Ghost,
                Some(Message::Dispatch(Command::EditorSetCanvasBackground {
                    color: if is_bg_transparent {
                        Some([255, 255, 255])
                    } else {
                        None
                    },
                    transparent: !is_bg_transparent,
                })),
                tr(lang, "label-canvas-bg"),
            ),
            with_tooltip(
                checker_swatch(
                    ed.canvas_background.map(rgb_to_color),
                    22.0,
                    18.0,
                    Some(Message::OpenColorPicker(
                        ColorPickerTarget::CanvasBackground
                    )),
                ),
                tr(lang, "label-canvas-bg"),
            ),
            text(tr(lang, "label-canvas-bg"))
                .size(TEXT_SIZE_CONTROL)
                .width(Fill),
            text(
                ed.canvas_background
                    .map(format_hex)
                    .unwrap_or_else(|| tr(lang, "label-transparent")),
            )
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_muted),
                }
            }),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([5, 6])
    .width(Fill)
    .style(|theme| {
        let t = PaletteTokens::for_theme(theme);
        container::Style {
            background: Some(t.bg_app.into()),
            border: iced::Border {
                color: t.border,
                width: 1.0,
                radius: photon_ui::theme::RADIUS_SM.into(),
            },
            ..Default::default()
        }
    });

    let action_bar = layer_action_bar(vec![
        LayerActionButton::new(
            Icon::Plus,
            Some(Message::ToggleMenu(ActiveMenu::EditorAddLayer)),
        )
        .active(state.active_menu == Some(ActiveMenu::EditorAddLayer))
        .tooltip(tr(lang, "tooltip-layer-add")),
        LayerActionButton::new(
            Icon::Duplicate,
            active_layer.map(|l| Message::Dispatch(Command::EditorLayerDuplicate { id: l.id })),
        )
        .tooltip(tr(lang, "tooltip-layer-duplicate")),
        LayerActionButton::new(
            Icon::ArrowUp,
            active_layer.filter(|_| can_move_up).map(|l| {
                Message::Dispatch(Command::EditorLayerReorder {
                    id: l.id,
                    new_index: active_idx + 1,
                })
            }),
        )
        .tooltip(tr(lang, "tooltip-layer-up")),
        LayerActionButton::new(
            Icon::ArrowDown,
            active_layer.filter(|_| can_move_down).map(|l| {
                Message::Dispatch(Command::EditorLayerReorder {
                    id: l.id,
                    new_index: active_idx - 1,
                })
            }),
        )
        .tooltip(tr(lang, "tooltip-layer-down")),
        LayerActionButton::new(
            Icon::MergeDown,
            active_layer
                .filter(|_| can_merge_down)
                .map(|l| Message::Dispatch(Command::EditorLayerMergeDown { id: l.id })),
        )
        .tooltip(tr(lang, "tooltip-layer-merge-down")),
        LayerActionButton::new(
            Icon::Flatten,
            (ed.layers.len() > 1).then_some(Message::Dispatch(Command::EditorLayerFlatten)),
        )
        .tooltip(tr(lang, "tooltip-layer-flatten")),
        LayerActionButton::new(
            Icon::Trash,
            active_layer
                .filter(|_| can_delete)
                .map(|l| Message::Dispatch(Command::EditorLayerDelete { id: l.id })),
        )
        .tooltip(tr(lang, "tooltip-layer-delete")),
    ]);

    column![
        props_col,
        scrollable(list_col).height(Fill).width(Fill),
        canvas_bg_row,
        action_bar,
    ]
    .spacing(8)
    .height(Fill)
    .width(Fill)
    .into()
}

fn build_history_panel<'a>(ed: &'a EditorState, lang: Language) -> Element<'a, Message> {
    let mut col = column![].spacing(4);
    let at_original = ed.history.is_empty();

    col = col.push(
        button::labeled(
            format!("0. {}", tr(lang, "label-history-original")),
            Some(Icon::History),
            ButtonVariant::Tab {
                active: at_original,
            },
            Some(Message::Dispatch(Command::EditorHistoryJump {
                target_index: 0,
            })),
        )
        .width(Fill),
    );

    for (idx, op) in ed.history.iter().enumerate() {
        let is_current = idx + 1 == ed.history.len();
        col = col.push(
            button::labeled(
                format!("{}. {}", idx + 1, tr_ui_text(lang, op)),
                None,
                if is_current {
                    ButtonVariant::Tab { active: true }
                } else {
                    ButtonVariant::Ghost
                },
                Some(Message::Dispatch(Command::EditorHistoryJump {
                    target_index: idx + 1,
                })),
            )
            .width(Fill),
        );
    }

    for (r_idx, op) in ed.redo_history.iter().enumerate() {
        let target = ed.history.len() + r_idx + 1;
        col = col.push(
            button::labeled(
                format!("↷ {}. {}", target, tr_ui_text(lang, op)),
                None,
                ButtonVariant::Ghost,
                Some(Message::Dispatch(Command::EditorHistoryJump {
                    target_index: target,
                })),
            )
            .width(Fill),
        );
    }

    if ed.history_trimmed {
        col = col.push(
            text(tr(lang, "label-history-trimmed"))
                .size(11)
                .style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.text_muted),
                    }
                }),
        );
    }

    let reset_btn = with_tooltip_pos(
        button::labeled(
            tr(lang, "action-reset"),
            Some(Icon::Reset),
            ButtonVariant::Secondary,
            ed.modified
                .then_some(Message::Dispatch(Command::EditorReset)),
        )
        .width(Fill),
        tr(lang, "tooltip-history-reset"),
        TooltipPosition::Top,
    );

    column![col, reset_btn].spacing(8).width(Fill).into()
}

#[allow(clippy::too_many_arguments)]
pub fn build_color_picker_dialog<'a>(
    lang: Language,
    target: ColorPickerTarget,
    r: u8,
    g: u8,
    b: u8,
    hex: &'a str,
    transparent: bool,
) -> DialogSpec<'a, Message> {
    let allow_transparent = !matches!(
        target,
        ColorPickerTarget::Foreground | ColorPickerTarget::ViewportBackground
    );
    let preview_color = if transparent && allow_transparent {
        None
    } else {
        Some(Color::from_rgb8(r, g, b))
    };

    const PRESETS: [[u8; 3]; 8] = [
        [0, 0, 0],
        [255, 255, 255],
        [239, 68, 68],
        [249, 115, 22],
        [234, 179, 8],
        [34, 197, 94],
        [59, 130, 246],
        [168, 85, 247],
    ];
    let mut preset_row = row![].spacing(6);
    for p in PRESETS {
        preset_row = preset_row.push(checker_swatch(
            Some(rgb_to_color(p)),
            22.0,
            22.0,
            Some(Message::ColorPickerSetRgb(p[0], p[1], p[2])),
        ));
    }

    let mut body = column![
        row![
            checker_swatch(preview_color, 44.0, 44.0, None),
            column![
                text("Hex").size(TEXT_SIZE_CAPTION),
                text_input("#000000", hex)
                    .on_input(Message::ColorPickerHexChanged)
                    .size(TEXT_SIZE_CONTROL)
                    .padding([6, 8])
                    .style(input_style),
            ]
            .spacing(4)
            .width(Fill),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        kv_row("R", r.to_string()),
        slider(0..=255, r, move |v| Message::ColorPickerSetRgb(v, g, b)).style(slider_style),
        kv_row("G", g.to_string()),
        slider(0..=255, g, move |v| Message::ColorPickerSetRgb(r, v, b)).style(slider_style),
        kv_row("B", b.to_string()),
        slider(0..=255, b, move |v| Message::ColorPickerSetRgb(r, g, v)).style(slider_style),
        preset_row,
    ]
    .spacing(8);

    if allow_transparent {
        body = body.push(
            checkbox(transparent)
                .label(tr(lang, "label-transparent"))
                .text_size(TEXT_SIZE_CONTROL)
                .on_toggle(Message::ColorPickerToggleTransparent),
        );
    }

    let title = match target {
        ColorPickerTarget::Foreground => tr(lang, "label-foreground"),
        ColorPickerTarget::Background => tr(lang, "label-background"),
        ColorPickerTarget::ViewportBackground => tr(lang, "label-viewport-bg"),
        ColorPickerTarget::CanvasBackground
        | ColorPickerTarget::CropFill
        | ColorPickerTarget::CanvasSizeFill
        | ColorPickerTarget::RotateFill => tr(lang, "label-canvas-bg"),
    };

    let mut footer = row![button::labeled(
        tr(lang, "dialog-cancel"),
        None,
        ButtonVariant::Ghost,
        Some(Message::CloseDialog),
    ),]
    .spacing(6)
    .align_y(Alignment::Center);
    if target == ColorPickerTarget::ViewportBackground {
        footer = footer.push(button::labeled(
            tr(lang, "action-reset"),
            Some(Icon::Reset),
            ButtonVariant::Ghost,
            Some(Message::ResetViewportBackground),
        ));
    }
    let footer = footer
        .push(iced::widget::space::horizontal())
        .push(button::labeled(
            "OK",
            Some(Icon::Check),
            ButtonVariant::Primary,
            Some(Message::ConfirmColorPicker),
        ));

    DialogSpec {
        open: true,
        title,
        width: 380.0,
        body: body.into(),
        footer: footer.into(),
        on_close: Message::CloseDialog,
        on_noop: Message::Noop,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_export_dialog<'a>(
    lang: Language,
    path: &'a str,
    quality: u8,
    max_edge: &'a str,
    metadata: bool,
    error: Option<&'a str>,
) -> DialogSpec<'a, Message> {
    let mut body = column![
        column![
            text(tr(lang, "label-export-file")).size(TEXT_SIZE_CONTROL),
            row![
                text_input("/path/to/export.jpg", path)
                    .on_input(Message::ExportPathChanged)
                    .size(TEXT_SIZE_CONTROL)
                    .padding([7, 10])
                    .style(input_style)
                    .width(Fill),
                button::labeled(
                    "…",
                    Some(Icon::FolderOpen),
                    ButtonVariant::Secondary,
                    Some(Message::PickExportDialog),
                ),
            ]
            .spacing(6),
        ]
        .spacing(4),
        kv_row(tr(lang, "label-quality"), format!("{quality}%")),
        slider(10..=100, quality, Message::ExportQualityChanged)
            .step(1_u8)
            .style(slider_style),
        column![
            text(tr(lang, "label-max-edge")).size(TEXT_SIZE_CONTROL),
            text_input(&tr(lang, "label-original-size"), max_edge)
                .on_input(Message::ExportMaxEdgeChanged)
                .size(TEXT_SIZE_CONTROL)
                .padding([7, 10])
                .style(input_style),
        ]
        .spacing(4),
        checkbox(metadata)
            .label(tr(lang, "label-keep-metadata"))
            .text_size(TEXT_SIZE_CONTROL)
            .on_toggle(Message::ExportMetadataToggled),
    ]
    .spacing(10);

    if let Some(err) = error {
        body = body.push(text(err.to_owned()).size(12).style(|theme| {
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
            tr(lang, "action-export"),
            Some(Icon::Export),
            ButtonVariant::Primary,
            Some(Message::ConfirmExport),
        ),
    ]
    .align_y(Alignment::Center);

    DialogSpec {
        open: true,
        title: tr(lang, "dialog-export-title"),
        width: 460.0,
        body: body.into(),
        footer: footer.into(),
        on_close: Message::CloseDialog,
        on_noop: Message::Noop,
    }
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

pub fn build_editor_canvas_context_menu(state: &PhotonGui) -> Option<Element<'_, Message>> {
    let ed = state.snapshot.meta.editor.as_ref()?;
    let lang = state.snapshot.meta.language;
    let entries = vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-undo"),
                Message::ContextMenuDispatch(Command::EditorUndo),
            )
            .icon(Icon::Undo)
            .shortcut("Ctrl+Z")
            .enabled(ed.can_undo),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-redo"),
                Message::ContextMenuDispatch(Command::EditorRedo),
            )
            .icon(Icon::Redo)
            .shortcut("Ctrl+Y")
            .enabled(ed.can_redo),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-copy-image"),
                Message::CopyImageToClipboard(None),
            )
            .icon(Icon::Copy)
            .shortcut("Ctrl+Shift+C")
            .enabled(state.editor_frame.is_some()),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-duplicate"),
                Message::ContextMenuDispatch(Command::EditorLayerDuplicate {
                    id: ed.active_layer_id,
                }),
            )
            .icon(Icon::Duplicate)
            .enabled(!ed.layers.is_empty()),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-adjustments"),
                Message::EditorOpenAdjustPopup(AdjustmentPopupTab::ColorAndLight),
            )
            .icon(Icon::Sliders),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-rotate-left"),
                Message::ContextMenuDispatch(Command::EditorRotate {
                    degrees: -90,
                    target: ApplyTarget::WholeImage,
                }),
            )
            .icon(Icon::RotateLeft),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-rotate-right"),
                Message::ContextMenuDispatch(Command::EditorRotate {
                    degrees: 90,
                    target: ApplyTarget::WholeImage,
                }),
            )
            .icon(Icon::RotateRight),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-flip-h"),
                Message::ContextMenuDispatch(Command::EditorFlip {
                    axis: FlipAxis::Horizontal,
                    target: ApplyTarget::WholeImage,
                }),
            )
            .icon(Icon::FlipH),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-flip-v"),
                Message::ContextMenuDispatch(Command::EditorFlip {
                    axis: FlipAxis::Vertical,
                    target: ApplyTarget::WholeImage,
                }),
            )
            .icon(Icon::FlipV),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-crop"), Message::ToggleCropMode)
                .icon(Icon::Crop)
                .checked(state.crop_uv.is_some()),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-resize"), Message::OpenResizeDialog)
                .icon(Icon::Resize)
                .shortcut("Ctrl+R"),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "tool-canvas-size"),
                Message::EditorSelectTool(EditorTool::CanvasSize),
            )
            .icon(Icon::CanvasSize),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-fit"),
                Message::Canvas(crate::shader_canvas::CanvasEvent::ResetFit),
            )
            .icon(Icon::Fit)
            .shortcut("F"),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-compare-original"),
                Message::EditorCycleCompareMode,
            )
            .icon(Icon::Compare)
            .checked(state.editor_ui.compare_mode != CompareMode::Off),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "label-compare-side-by-side"),
                Message::EditorSetCompareMode(CompareMode::SideBySide),
            )
            .icon(Icon::ViewDouble)
            .checked(state.editor_ui.compare_mode == CompareMode::SideBySide),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-reset"),
                Message::ContextMenuDispatch(Command::EditorReset),
            )
            .icon(Icon::Reset)
            .enabled(ed.modified),
        ),
    ];
    Some(menu_panel_entries(entries, 225.0))
}

pub fn build_editor_layer_context_menu(
    state: &PhotonGui,
    layer_id: u64,
) -> Option<Element<'_, Message>> {
    let ed = state.snapshot.meta.editor.as_ref()?;
    let stack_idx = ed.layers.iter().position(|l| l.id == layer_id)?;
    let layer = &ed.layers[stack_idx];
    let lang = state.snapshot.meta.language;

    let can_move_up = stack_idx + 1 < ed.layers.len();
    let can_move_down = stack_idx > 0;
    let can_merge_down = stack_idx > 0 && !layer.locked && !ed.layers[stack_idx - 1].locked;
    let can_flatten = ed.layers.len() > 1;
    let can_delete = ed.layers.len() > 1 && !layer.locked;

    let mut entries = vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-duplicate"),
                Message::ContextMenuDispatch(Command::EditorLayerDuplicate { id: layer_id }),
            )
            .icon(Icon::Duplicate),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-rename"),
                Message::EditorStartLayerRename(layer_id, layer.name.clone()),
            )
            .icon(Icon::Editor),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-move-up"),
                Message::ContextMenuDispatch(Command::EditorLayerReorder {
                    id: layer_id,
                    new_index: stack_idx + 1,
                }),
            )
            .icon(Icon::ArrowUp)
            .enabled(can_move_up),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-move-down"),
                Message::ContextMenuDispatch(Command::EditorLayerReorder {
                    id: layer_id,
                    new_index: stack_idx.saturating_sub(1),
                }),
            )
            .icon(Icon::ArrowDown)
            .enabled(can_move_down),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-merge-down"),
                Message::ContextMenuDispatch(Command::EditorLayerMergeDown { id: layer_id }),
            )
            .icon(Icon::MergeDown)
            .enabled(can_merge_down),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-flatten"),
                Message::ContextMenuDispatch(Command::EditorLayerFlatten),
            )
            .icon(Icon::Flatten)
            .enabled(can_flatten),
        ),
        MenuEntry::Separator,
    ];

    if layer.has_mask {
        entries.extend([
            MenuEntry::Item(
                MenuItem::new(
                    tr(
                        lang,
                        if layer.editing_mask {
                            "action-mask-edit-pixels"
                        } else {
                            "action-mask-edit"
                        },
                    ),
                    Message::ContextMenuDispatch(Command::EditorLayerMaskSetEditing {
                        id: layer_id,
                        editing: !layer.editing_mask,
                    }),
                )
                .icon(Icon::Mask)
                .checked(layer.editing_mask),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    tr(
                        lang,
                        if layer.mask_enabled {
                            "action-mask-disable"
                        } else {
                            "action-mask-enable"
                        },
                    ),
                    Message::ContextMenuDispatch(Command::EditorLayerMaskSetEnabled {
                        id: layer_id,
                        enabled: !layer.mask_enabled,
                    }),
                )
                .icon(if layer.mask_enabled {
                    Icon::EyeOff
                } else {
                    Icon::Eye
                }),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "action-mask-invert"),
                    Message::ContextMenuDispatch(Command::EditorLayerMaskInvert { id: layer_id }),
                )
                .icon(Icon::Swap)
                .enabled(!layer.locked),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "action-mask-apply"),
                    Message::ContextMenuDispatch(Command::EditorLayerMaskApply { id: layer_id }),
                )
                .icon(Icon::Check)
                .enabled(!layer.locked),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "action-mask-delete"),
                    Message::ContextMenuDispatch(Command::EditorLayerMaskDelete { id: layer_id }),
                )
                .icon(Icon::Trash)
                .enabled(!layer.locked),
            ),
            MenuEntry::Separator,
        ]);
    } else {
        entries.extend([
            MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "action-mask-add"),
                    Message::ContextMenuDispatch(Command::EditorLayerMaskAdd {
                        id: layer_id,
                        reveal_all: true,
                    }),
                )
                .icon(Icon::Mask)
                .enabled(!layer.locked),
            ),
            MenuEntry::Separator,
        ]);
    }

    entries.extend([
        MenuEntry::Item(
            MenuItem::new(
                tr(
                    lang,
                    if layer.visible {
                        "action-layer-hide"
                    } else {
                        "action-layer-show"
                    },
                ),
                Message::ContextMenuDispatch(Command::EditorLayerSetProps {
                    id: layer_id,
                    name: None,
                    visible: Some(!layer.visible),
                    locked: None,
                    opacity: None,
                    blend_mode: None,
                    offset_x: None,
                    offset_y: None,
                }),
            )
            .icon(if layer.visible {
                Icon::EyeOff
            } else {
                Icon::Eye
            }),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(
                    lang,
                    if layer.locked {
                        "action-layer-unlock"
                    } else {
                        "action-layer-lock"
                    },
                ),
                Message::ContextMenuDispatch(Command::EditorLayerSetProps {
                    id: layer_id,
                    name: None,
                    visible: None,
                    locked: Some(!layer.locked),
                    opacity: None,
                    blend_mode: None,
                    offset_x: None,
                    offset_y: None,
                }),
            )
            .icon(if layer.locked {
                Icon::Unlock
            } else {
                Icon::Lock
            }),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-delete"),
                Message::ContextMenuDispatch(Command::EditorLayerDelete { id: layer_id }),
            )
            .icon(Icon::Trash)
            .enabled(can_delete),
        ),
    ]);

    Some(menu_panel_entries(entries, 220.0))
}

pub(crate) fn build_editor_add_layer_menu(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let next_idx = state
        .snapshot
        .meta
        .editor
        .as_ref()
        .map_or(1, |e| e.layers.len() + 1);
    let entries = vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-add-paint"),
                Message::Dispatch(Command::EditorLayerAdd {
                    name: format!("Layer {next_idx}"),
                }),
            )
            .icon(Icon::Brush),
        ),
        MenuEntry::Separator,
        MenuEntry::Header(tr(lang, "action-layer-add-filter")),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "filter-blur"),
                Message::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "filter-blur"),
                    effect: LayerEffect::Blur { radius: 4.0 },
                }),
            )
            .icon(Icon::Liquify),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "filter-sharpen"),
                Message::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "filter-sharpen"),
                    effect: LayerEffect::Sharpen { amount: 0.8 },
                }),
            )
            .icon(Icon::Compare),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "filter-invert"),
                Message::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "filter-invert"),
                    effect: LayerEffect::Invert,
                }),
            )
            .icon(Icon::Swap),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "filter-desaturate"),
                Message::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "filter-desaturate"),
                    effect: LayerEffect::Desaturate,
                }),
            )
            .icon(Icon::Palette),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "filter-sepia"),
                Message::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "filter-sepia"),
                    effect: LayerEffect::Sepia,
                }),
            )
            .icon(Icon::Image),
        ),
        MenuEntry::Separator,
        MenuEntry::Header(tr(lang, "action-layer-add-adjustment")),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-filter-color"),
                Message::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "menu-filter-color"),
                    effect: LayerEffect::ColorLight(ColorAdjustments::default()),
                }),
            )
            .icon(Icon::Sliders),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-filter-hsv"),
                Message::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "menu-filter-hsv"),
                    effect: LayerEffect::Hsv(HsvAdjustment::default()),
                }),
            )
            .icon(Icon::Palette),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-filter-balance"),
                Message::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "menu-filter-balance"),
                    effect: LayerEffect::ColorBalance(ColorBalanceAdjustment::default()),
                }),
            )
            .icon(Icon::ColorBalance),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-filter-curves"),
                Message::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "menu-filter-curves"),
                    effect: LayerEffect::Curves(CurvesAdjustment::default()),
                }),
            )
            .icon(Icon::Curves),
        ),
    ];

    menu_panel_entries(entries, 224.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip_parses_with_or_without_hash() {
        assert_eq!(parse_hex_rgb("#3b82f6"), Some([0x3b, 0x82, 0xf6]));
        assert_eq!(parse_hex_rgb("ff00aa"), Some([0xff, 0x00, 0xaa]));
        assert_eq!(parse_hex_rgb("bad"), None);
        assert_eq!(format_hex([0x10, 0x20, 0x30]), "#102030");
    }

    #[test]
    fn crop_uv_to_signed_bounds_correctly() {
        let (x, y, w, h) = crop_uv_to_signed([0.1, 0.2, 0.9, 0.8], 100, 200);
        assert_eq!((x, y, w, h), (10, 40, 80, 120));
    }

    #[test]
    fn dimension_input_state_preserves_ratio_and_print_units() {
        let mut dims = DimensionInputState::from_dims(1920, 1080);
        assert_eq!(dims.ratio_str, "16:9");
        dims.apply(DimGridAction::WidthChanged("3840".to_owned()));
        assert_eq!(dims.width_px, 3840);
        assert_eq!(dims.height_px, 2160);

        dims.apply(DimGridAction::DpiChanged(300.0));
        dims.apply(DimGridAction::UnitChanged(LengthUnit::Inch));
        assert_eq!(dims.width_str, "12.80");
        dims.apply(DimGridAction::PresetRatio(1, 1));
        assert_eq!(dims.width_px, dims.height_px);
    }
}
