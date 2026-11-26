use crate::*;
use iced::Element;
use photon_canvas::CanvasTool;
use photon_i18n::tr;
use photon_protocol::Language;
use photon_ui::icons::Icon;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AlphaRefineMode {
    #[default]
    Off,
    EraseBg,
    RestoreFg,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WatermarkTab {
    #[default]
    AiAuto,
    Gemini,
    Region,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorToolGroup {
    Move,
    Marquee,
    Lasso,
    SmartSelect,
    Crop,
    Eyedropper,
    Retouch,
    Brush,
    Fill,
    Pen,
    Text,
    Shape,
    Nav,
    Ai,
}

impl EditorToolGroup {
    pub const fn tools(self) -> &'static [EditorTool] {
        match self {
            Self::Move => &[EditorTool::Pointer, EditorTool::Transform],
            Self::Marquee => &[
                EditorTool::MarqueeRect,
                EditorTool::MarqueeEllipse,
                EditorTool::SingleRow,
                EditorTool::SingleCol,
            ],
            Self::Lasso => &[
                EditorTool::Lasso,
                EditorTool::PolygonLasso,
                EditorTool::MagneticLasso,
                EditorTool::BezierSelect,
            ],
            Self::SmartSelect => &[EditorTool::QuickSelect, EditorTool::MagicWand],
            Self::Crop => &[EditorTool::Crop, EditorTool::RotateAngle],
            Self::Eyedropper => &[EditorTool::Eyedropper],
            Self::Retouch => &[EditorTool::HealingBrush, EditorTool::Liquify],
            Self::Brush => &[EditorTool::Brush, EditorTool::Eraser],
            Self::Fill => &[EditorTool::PaintBucket, EditorTool::Gradient],
            Self::Pen => &[EditorTool::BezierPen, EditorTool::PathSelect],
            Self::Text => &[EditorTool::Text],
            Self::Shape => &[
                EditorTool::DrawRect,
                EditorTool::DrawEllipse,
                EditorTool::DrawLine,
                EditorTool::DrawPolygon,
                EditorTool::DrawPolyline,
            ],
            Self::Nav => &[EditorTool::Hand, EditorTool::Zoom],
            Self::Ai => &[
                EditorTool::RemoveBg,
                EditorTool::Watermark,
                EditorTool::Caption,
            ],
        }
    }

    pub const fn row_index(self) -> usize {
        match self {
            Self::Move | Self::Marquee => 0,
            Self::Lasso | Self::SmartSelect => 1,
            Self::Crop | Self::Eyedropper => 2,
            Self::Retouch | Self::Brush => 3,
            Self::Fill | Self::Pen => 4,
            Self::Text | Self::Shape => 5,
            Self::Nav | Self::Ai => 6,
        }
    }

    pub fn active_tool(self, ui: &EditorUiState) -> EditorTool {
        match self {
            Self::Move => ui.move_subtool,
            Self::Marquee => ui.marquee_subtool,
            Self::Lasso => ui.lasso_subtool,
            Self::SmartSelect => ui.smart_subtool,
            Self::Crop => ui.crop_subtool,
            Self::Eyedropper => EditorTool::Eyedropper,
            Self::Retouch => ui.retouch_subtool,
            Self::Brush => ui.brush_subtool,
            Self::Fill => ui.fill_subtool,
            Self::Pen => ui.pen_subtool,
            Self::Text => EditorTool::Text,
            Self::Shape => ui.shape_subtool,
            Self::Nav => ui.nav_subtool,
            Self::Ai => ui.ai_subtool,
        }
    }

    pub fn is_active(self, active: EditorTool) -> bool {
        self.tools().contains(&active)
    }
}

pub fn editor_tool_label(lang: Language, tool: EditorTool) -> String {
    match tool {
        EditorTool::Pointer => tr(lang, "tooltip-tool-pointer"),
        EditorTool::Transform => tr(lang, "tooltip-tool-transform"),
        EditorTool::Hand => tr(lang, "tooltip-tool-hand"),
        EditorTool::Zoom => tr(lang, "tooltip-tool-zoom"),
        EditorTool::Eyedropper => tr(lang, "tooltip-tool-eyedropper"),
        EditorTool::Brush => tr(lang, "tooltip-tool-brush"),
        EditorTool::Eraser => tr(lang, "tooltip-tool-eraser"),
        EditorTool::Liquify => tr(lang, "tooltip-tool-liquify"),
        EditorTool::Crop => tr(lang, "tooltip-tool-crop"),
        EditorTool::RotateAngle => tr(lang, "tooltip-tool-rotate-angle"),
        EditorTool::MarqueeRect => format!("{} (M)", tr(lang, "tool-marquee-rect")),
        EditorTool::MarqueeEllipse => format!("{} (M)", tr(lang, "tool-marquee-ellipse")),
        EditorTool::SingleRow => format!("{} (M)", tr(lang, "tool-single-row")),
        EditorTool::SingleCol => format!("{} (M)", tr(lang, "tool-single-col")),
        EditorTool::Lasso => format!("{} (L)", tr(lang, "tool-lasso")),
        EditorTool::PolygonLasso => format!("{} (L)", tr(lang, "tool-polygon-lasso")),
        EditorTool::MagneticLasso => format!("{} (L)", tr(lang, "tool-magnetic-lasso")),
        EditorTool::BezierSelect => format!("{} (L)", tr(lang, "tool-bezier-select")),
        EditorTool::QuickSelect => format!("{} (W)", tr(lang, "tool-quick-select")),
        EditorTool::MagicWand => format!("{} (W)", tr(lang, "tool-magic-wand")),
        EditorTool::DrawLine => tr(lang, "tooltip-tool-draw-line"),
        EditorTool::DrawRect => tr(lang, "tooltip-tool-draw-rect"),
        EditorTool::DrawEllipse => tr(lang, "tooltip-tool-draw-ellipse"),
        EditorTool::DrawPolygon => tr(lang, "tooltip-tool-draw-polygon"),
        EditorTool::DrawPolyline => tr(lang, "tooltip-tool-draw-polyline"),
        EditorTool::BezierPen => tr(lang, "tooltip-tool-bezier-pen"),
        EditorTool::PathSelect => tr(lang, "tooltip-tool-path-select"),
        EditorTool::Text => tr(lang, "tooltip-tool-text"),
        EditorTool::PaintBucket => tr(lang, "tooltip-tool-paint-bucket"),
        EditorTool::Gradient => tr(lang, "tooltip-tool-gradient"),
        EditorTool::HealingBrush => tr(lang, "tooltip-tool-healing-brush"),
        EditorTool::RemoveBg => tr(lang, "tool-ai-rembg"),
        EditorTool::Watermark => tr(lang, "tool-ai-watermark"),
        EditorTool::Caption => tr(lang, "tool-ai-caption"),
    }
}

pub fn build_tool_group_flyout_menu<'a>(
    ui: &EditorUiState,
    lang: Language,
    group: EditorToolGroup,
) -> (f32, Element<'a, EditorMessage>) {
    let entries: Vec<photon_ui::menu::MenuEntry<'a, EditorMessage>> = group
        .tools()
        .iter()
        .copied()
        .map(|tool| {
            photon_ui::menu::MenuEntry::Item(
                photon_ui::menu::MenuItem::new(
                    editor_tool_label(lang, tool),
                    EditorMessage::EditorSelectTool(tool),
                )
                .icon(tool.icon())
                .checked(ui.tool == tool),
            )
        })
        .collect();
    let est_h = photon_ui::menu::estimate_menu_height_with_spacing(&entries);
    (est_h, photon_ui::menu::menu_panel_entries(entries, 224.0))
}

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
    RotateAngle,
    MarqueeRect,
    MarqueeEllipse,
    SingleRow,
    SingleCol,
    Lasso,
    PolygonLasso,
    MagneticLasso,
    BezierSelect,
    QuickSelect,
    MagicWand,
    DrawLine,
    DrawRect,
    DrawEllipse,
    DrawPolygon,
    DrawPolyline,
    BezierPen,
    PathSelect,
    Text,
    PaintBucket,
    Gradient,
    HealingBrush,
    Transform,
    RemoveBg,
    Watermark,
    Caption,
}

impl EditorTool {
    pub fn is_selection(self) -> bool {
        self.as_canvas_tool().is_selection()
    }

    pub fn is_marquee_group(self) -> bool {
        matches!(
            self,
            Self::MarqueeRect | Self::MarqueeEllipse | Self::SingleRow | Self::SingleCol
        )
    }

    pub fn is_lasso_group(self) -> bool {
        matches!(
            self,
            Self::Lasso | Self::PolygonLasso | Self::MagneticLasso | Self::BezierSelect
        )
    }

    pub fn is_smart_group(self) -> bool {
        matches!(self, Self::QuickSelect | Self::MagicWand)
    }

    pub fn is_shape_group(self) -> bool {
        matches!(
            self,
            Self::DrawLine
                | Self::DrawRect
                | Self::DrawEllipse
                | Self::DrawPolygon
                | Self::DrawPolyline
        )
    }

    pub fn is_fill_group(self) -> bool {
        matches!(self, Self::PaintBucket | Self::Gradient)
    }

    pub fn icon(self) -> Icon {
        match self {
            Self::Pointer => Icon::Pointer,
            Self::Hand => Icon::Hand,
            Self::Zoom => Icon::ZoomIn,
            Self::Brush => Icon::Brush,
            Self::Eraser => Icon::Eraser,
            Self::Liquify => Icon::Liquify,
            Self::Eyedropper => Icon::Eyedropper,
            Self::Crop => Icon::Crop,
            Self::RotateAngle => Icon::RotateAngle,
            Self::MarqueeRect => Icon::MarqueeRect,
            Self::MarqueeEllipse => Icon::MarqueeEllipse,
            Self::SingleRow => Icon::SingleRow,
            Self::SingleCol => Icon::SingleCol,
            Self::Lasso => Icon::Lasso,
            Self::PolygonLasso => Icon::PolygonLasso,
            Self::MagneticLasso => Icon::MagneticLasso,
            Self::BezierSelect => Icon::BezierSelect,
            Self::QuickSelect => Icon::QuickSelect,
            Self::MagicWand => Icon::MagicWand,
            Self::DrawLine => Icon::DrawLine,
            Self::DrawRect => Icon::DrawRect,
            Self::DrawEllipse => Icon::DrawEllipse,
            Self::DrawPolygon => Icon::DrawPolygon,
            Self::DrawPolyline => Icon::DrawPolyline,
            Self::BezierPen => Icon::BezierPen,
            Self::PathSelect => Icon::PathSelect,
            Self::Text => Icon::Text,
            Self::PaintBucket => Icon::PaintBucket,
            Self::Gradient => Icon::Gradient,
            Self::HealingBrush => Icon::HealingBrush,
            Self::Transform => Icon::Transform,
            Self::RemoveBg => Icon::Mask,
            Self::Watermark => Icon::Denoise,
            Self::Caption => Icon::Plug,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::MarqueeRect => "tool-marquee-rect",
            Self::MarqueeEllipse => "tool-marquee-ellipse",
            Self::SingleRow => "tool-single-row",
            Self::SingleCol => "tool-single-col",
            Self::Lasso => "tool-lasso",
            Self::PolygonLasso => "tool-polygon-lasso",
            Self::MagneticLasso => "tool-magnetic-lasso",
            Self::BezierSelect => "tool-bezier-select",
            Self::QuickSelect => "tool-quick-select",
            Self::MagicWand => "tool-magic-wand",
            Self::DrawLine => "tool-draw-line",
            Self::DrawRect => "tool-draw-rect",
            Self::DrawEllipse => "tool-draw-ellipse",
            Self::DrawPolygon => "tool-draw-polygon",
            Self::DrawPolyline => "tool-draw-polyline",
            Self::BezierPen => "tool-bezier-pen",
            Self::PathSelect => "tool-path-select",
            Self::Text => "tool-text",
            Self::PaintBucket => "tool-paint-bucket",
            Self::Gradient => "tool-gradient",
            Self::HealingBrush => "tool-healing-brush",
            Self::Transform => "tool-transform",
            Self::RemoveBg => "tool-ai-rembg",
            Self::Watermark => "tool-ai-watermark",
            Self::Caption => "tool-ai-caption",
            _ => "tool-pointer",
        }
    }

    pub fn as_canvas_tool(self) -> CanvasTool {
        match self {
            Self::Pointer | Self::RemoveBg | Self::Watermark | Self::Caption => CanvasTool::Pointer,
            Self::Hand => CanvasTool::Hand,
            Self::Zoom => CanvasTool::Zoom,
            Self::Brush => CanvasTool::Brush,
            Self::Eraser => CanvasTool::Eraser,
            Self::Liquify => CanvasTool::Liquify,
            Self::HealingBrush => CanvasTool::HealingBrush,
            Self::Eyedropper => CanvasTool::Eyedropper,
            Self::Crop => CanvasTool::Crop,
            Self::RotateAngle => CanvasTool::RotateAngle,
            Self::MarqueeRect => CanvasTool::MarqueeRect,
            Self::MarqueeEllipse => CanvasTool::MarqueeEllipse,
            Self::SingleRow => CanvasTool::SingleRow,
            Self::SingleCol => CanvasTool::SingleCol,
            Self::Lasso => CanvasTool::Lasso,
            Self::PolygonLasso => CanvasTool::PolygonLasso,
            Self::MagneticLasso => CanvasTool::MagneticLasso,
            Self::BezierSelect => CanvasTool::BezierSelect,
            Self::QuickSelect => CanvasTool::QuickSelect,
            Self::MagicWand => CanvasTool::MagicWand,
            Self::DrawLine => CanvasTool::DrawLine,
            Self::DrawRect => CanvasTool::DrawRect,
            Self::DrawEllipse => CanvasTool::DrawEllipse,
            Self::DrawPolygon => CanvasTool::DrawPolygon,
            Self::DrawPolyline => CanvasTool::DrawPolyline,
            Self::BezierPen => CanvasTool::BezierPen,
            Self::PathSelect => CanvasTool::PathSelect,
            Self::Text => CanvasTool::Text,
            Self::PaintBucket => CanvasTool::PaintBucket,
            Self::Gradient => CanvasTool::Gradient,
            Self::Transform => CanvasTool::Transform,
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
    ViewerViewportBg,
    CropFill,
    CanvasSizeFill,
    RotateFill,
    GradientStop(usize),
    VectorShapeFill,
    VectorShapeStroke,
    TextLayerColor,
}
