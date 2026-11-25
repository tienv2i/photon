//! GPU-accelerated WGSL image viewport (`iced::widget::shader`) powered by `wgpu 27`.
//!
//! Supports zoom-at-cursor, pan, checkerboard transparency backdrop, canvas background,
//! real-time 60fps Color & Light adjustments (exposure, highlights, shadows, brightness,
//! contrast, saturation), live arbitrary-angle rotation preview, Before/After Original
//! split/hold comparison, and interactive Paint/Eraser/Eyedropper/Crop/Zoom tools.

use std::collections::HashMap;
use std::sync::Arc;

use iced::keyboard::Modifiers;
use iced::mouse;
use iced::wgpu;
use iced::widget::shader::{self, Action, Pipeline, Primitive, Viewport};
use iced::{Event, Rectangle};
use image::{DynamicImage, RgbaImage};
use photon_protocol::ColorAdjustments;

#[derive(Debug, Clone)]
pub struct CpuImageFrame {
    pub key: String,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
}

/// Interactive tool active on `ImageCanvasProgram`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CanvasTool {
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
}

#[derive(Debug, Clone, Default)]
pub struct CanvasInteractionState {
    pub dragging: bool,
    pub cropping: bool,
    pub painting: bool,
    pub sampling: bool,
    pub dragging_split: bool,
    pub modifiers: Modifiers,
    pub last_cursor: Option<iced::Point>,
    pub crop_anchor_uv: Option<[f32; 2]>,
}

/// Default studio canvas/viewport backdrop RGB matching `PaletteTokens::bg_canvas`.
pub const fn default_viewport_bg_rgb(dark_theme: bool) -> [u8; 3] {
    if dark_theme {
        [16, 18, 20]
    } else {
        [233, 236, 240]
    }
}

#[derive(Debug, Clone)]
pub enum CanvasEvent {
    ZoomBy {
        factor: f32,
        cursor: Option<[f32; 2]>,
        bounds: [f32; 2],
    },
    PanBy {
        dx: f32,
        dy: f32,
    },
    CropUpdated([f32; 4]),
    StrokeBegin([f64; 2]),
    StrokeStep(Vec<[f64; 2]>),
    StrokeEnd(bool),
    SampleColor([u32; 2]),
    CompareSplitChanged(f32),
    ContextMenuRequested {
        pos: [f32; 2],
        is_right_pair: bool,
    },
    ResetFit,
}

pub struct ImageCanvasProgram {
    pub frame: Option<CpuImageFrame>,
    pub pair_frame: Option<CpuImageFrame>,
    pub dual_gap_px: f32,
    pub compare_frame: Option<CpuImageFrame>,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub adjustments: ColorAdjustments,
    pub dark_theme: bool,
    pub crop_rect: Option<[f32; 4]>,
    pub crop_aspect: Option<f32>,
    pub quarter_turns: u8,
    pub tool: CanvasTool,
    pub preview_angle_rad: f32,
    pub preview_fill: [f32; 4],
    pub canvas_bg: Option<[u8; 3]>,
    pub viewport_bg: Option<[u8; 3]>,
    /// 0 = Off, 1 = Split (draggable divider), 2 = Side-by-Side (Original left | Edited right), 3 = HoldOriginal
    pub compare_mode: u8,
    pub compare_split_x: f32,
    /// Active brush/eraser/liquify radius in document pixels (`0.0` disables cursor ring).
    pub brush_radius_px: f32,
}

/// Compute unified spread dimensions `(spread_w, spread_h, left_disp_w, gap_img)`
/// matching KogaView's `ViewerDualSpread` height-normalization.
pub fn dual_spread_geometry(
    left_size: (u32, u32),
    right_size: (u32, u32),
    gap_px: f32,
) -> (f32, f32, f32, f32) {
    let lw = left_size.0.max(1) as f32;
    let lh = left_size.1.max(1) as f32;
    let rw = right_size.0.max(1) as f32;
    let rh = right_size.1.max(1) as f32;
    let target_h = lh.max(rh);
    let left_w = (lw * (target_h / lh)).round();
    let right_w = (rw * (target_h / rh)).round();
    let gap = if gap_px > 0.0 {
        gap_px * (target_h / 900.0).max(1.0)
    } else {
        0.0
    };
    (left_w + gap + right_w, target_h, left_w, gap)
}

impl ImageCanvasProgram {
    fn effective_doc_size(&self) -> Option<(f32, f32)> {
        let frame = self.frame.as_ref()?;
        if let Some(pair) = &self.pair_frame {
            let (sw, sh, _, _) = dual_spread_geometry(
                (frame.width, frame.height),
                (pair.width, pair.height),
                self.dual_gap_px,
            );
            Some((sw, sh))
        } else {
            Some((frame.width.max(1) as f32, frame.height.max(1) as f32))
        }
    }

    fn fit_geometry(&self, bounds: Rectangle) -> Option<(f32, f32, f32, f32, f32, f32)> {
        let (doc_w, doc_h) = self.effective_doc_size()?;
        let vp_w = bounds.width.max(1.0);
        let vp_h = bounds.height.max(1.0);
        let (fw, fh) = if self.quarter_turns % 2 == 1 {
            (doc_h, doc_w)
        } else {
            (doc_w, doc_h)
        };
        let ca = self.preview_angle_rad.cos().abs();
        let sa = self.preview_angle_rad.sin().abs();
        let eff_w = (fw * ca + fh * sa).max(1.0);
        let eff_h = (fw * sa + fh * ca).max(1.0);
        let vp_aspect = vp_w / vp_h;
        let img_aspect = eff_w / eff_h;
        let (fit_w, fit_h) = if img_aspect > vp_aspect {
            (vp_w, vp_w / img_aspect)
        } else {
            (vp_h * img_aspect, vp_h)
        };
        let drawn_w = fit_w * self.zoom.max(0.05);
        let drawn_h = fit_h * self.zoom.max(0.05);
        let cx = vp_w * 0.5 + self.pan_x;
        let cy = vp_h * 0.5 + self.pan_y;
        let left = cx - drawn_w * 0.5;
        let top = cy - drawn_h * 0.5;
        Some((left, top, drawn_w, drawn_h, fw, fh))
    }

    pub fn screen_to_image_uv(&self, pos: iced::Point, bounds: Rectangle) -> Option<[f32; 2]> {
        let (left, top, drawn_w, drawn_h, _, _) = self.fit_geometry(bounds)?;
        let rel_x = pos.x - bounds.x;
        let rel_y = pos.y - bounds.y;
        Some([
            ((rel_x - left) / drawn_w).clamp(0.0, 1.0),
            ((rel_y - top) / drawn_h).clamp(0.0, 1.0),
        ])
    }

    pub fn screen_to_image_px(&self, pos: iced::Point, bounds: Rectangle) -> Option<[f64; 2]> {
        let (left, top, drawn_w, drawn_h, fw, fh) = self.fit_geometry(bounds)?;
        let rel_x = pos.x - bounds.x;
        let rel_y = pos.y - bounds.y;
        let u = ((rel_x - left) / drawn_w).clamp(0.0, 1.0);
        let v = ((rel_y - top) / drawn_h).clamp(0.0, 1.0);
        Some([f64::from(u * fw), f64::from(v * fh)])
    }

    fn constrained_crop(&self, anchor: [f32; 2], cur: [f32; 2]) -> Option<[f32; 4]> {
        let frame = self.frame.as_ref()?;
        let Some(target_ratio) = self.crop_aspect else {
            let x0 = anchor[0].min(cur[0]);
            let y0 = anchor[1].min(cur[1]);
            let x1 = anchor[0].max(cur[0]);
            let y1 = anchor[1].max(cur[1]);
            if (x1 - x0) > 0.01 && (y1 - y0) > 0.01 {
                return Some([x0, y0, x1, y1]);
            }
            return None;
        };

        let img_aspect = (frame.width.max(1) as f32) / (frame.height.max(1) as f32);
        let uv_ratio = (target_ratio / img_aspect).max(0.01);
        let dx = cur[0] - anchor[0];
        let dy = cur[1] - anchor[1];
        let sx = if dx >= 0.0 { 1.0_f32 } else { -1.0_f32 };
        let sy = if dy >= 0.0 { 1.0_f32 } else { -1.0_f32 };

        let mut w = dx.abs();
        let mut h = w / uv_ratio;
        if h > dy.abs() {
            h = dy.abs();
            w = h * uv_ratio;
        }
        let max_w = if sx > 0.0 { 1.0 - anchor[0] } else { anchor[0] };
        let max_h = if sy > 0.0 { 1.0 - anchor[1] } else { anchor[1] };
        if w > max_w {
            w = max_w;
            h = w / uv_ratio;
        }
        if h > max_h {
            h = max_h;
            w = h * uv_ratio;
        }
        if w <= 0.01 || h <= 0.01 {
            return None;
        }
        let end_x = (anchor[0] + sx * w).clamp(0.0, 1.0);
        let end_y = (anchor[1] + sy * h).clamp(0.0, 1.0);
        Some([
            anchor[0].min(end_x),
            anchor[1].min(end_y),
            anchor[0].max(end_x),
            anchor[1].max(end_y),
        ])
    }

    fn is_over_split_handle(&self, pos: iced::Point, bounds: Rectangle) -> bool {
        if self.compare_mode != 1 {
            return false;
        }
        let split_x = bounds.x + bounds.width * self.compare_split_x.clamp(0.0, 1.0);
        (pos.x - split_x).abs() <= 10.0 && pos.y >= bounds.y && pos.y <= bounds.y + bounds.height
    }
}

impl shader::Program<CanvasEvent> for ImageCanvasProgram {
    type State = CanvasInteractionState;
    type Primitive = ImageQuadPrimitive;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<CanvasEvent>> {
        match event {
            Event::Keyboard(iced::keyboard::Event::ModifiersChanged(mods)) => {
                state.modifiers = *mods;
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let y = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => *y / 60.0,
                };
                if y.abs() > 0.001 {
                    let factor = if y > 0.0 { 1.12 } else { 1.0 / 1.12 };
                    let rel_cursor = cursor.position_in(bounds).map(|p| [p.x, p.y]);
                    return Some(
                        Action::publish(CanvasEvent::ZoomBy {
                            factor,
                            cursor: rel_cursor,
                            bounds: [bounds.width.max(1.0), bounds.height.max(1.0)],
                        })
                        .and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle))
                if cursor.is_over(bounds) =>
            {
                state.dragging = true;
                state.last_cursor = cursor.position();
                return Some(Action::capture());
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                if state.dragging {
                    state.dragging = false;
                    state.last_cursor = None;
                    return Some(Action::capture());
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
                if cursor.is_over(bounds) =>
            {
                if self.tool == CanvasTool::Zoom {
                    let rel_cursor = cursor.position_in(bounds).map(|p| [p.x, p.y]);
                    return Some(
                        Action::publish(CanvasEvent::ZoomBy {
                            factor: 0.8,
                            cursor: rel_cursor,
                            bounds: [bounds.width.max(1.0), bounds.height.max(1.0)],
                        })
                        .and_capture(),
                    );
                }
                if let Some(pos) = cursor.position() {
                    let mut is_right_pair = false;
                    if let (Some(frame), Some(pair)) = (&self.frame, &self.pair_frame)
                        && let Some(uv) = self.screen_to_image_uv(pos, bounds)
                    {
                        let (sw, _, left_w, gap) = dual_spread_geometry(
                            (frame.width, frame.height),
                            (pair.width, pair.height),
                            self.dual_gap_px,
                        );
                        let split_uv = (left_w + gap * 0.5) / sw.max(1.0);
                        is_right_pair = uv[0] > split_uv;
                    }
                    return Some(
                        Action::publish(CanvasEvent::ContextMenuRequested {
                            pos: [pos.x, pos.y],
                            is_right_pair,
                        })
                        .and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                if let Some(pos) = cursor.position()
                    && self.is_over_split_handle(pos, bounds)
                {
                    state.dragging_split = true;
                    return Some(Action::capture());
                }

                match self.tool {
                    CanvasTool::Zoom => {
                        let factor = if state.modifiers.shift() { 0.8 } else { 1.25 };
                        let rel_cursor = cursor.position_in(bounds).map(|p| [p.x, p.y]);
                        return Some(
                            Action::publish(CanvasEvent::ZoomBy {
                                factor,
                                cursor: rel_cursor,
                                bounds: [bounds.width.max(1.0), bounds.height.max(1.0)],
                            })
                            .and_capture(),
                        );
                    }
                    CanvasTool::Brush | CanvasTool::Eraser | CanvasTool::Liquify => {
                        if let Some(pos) = cursor.position()
                            && let Some(pt) = self.screen_to_image_px(pos, bounds)
                        {
                            state.painting = true;
                            return Some(
                                Action::publish(CanvasEvent::StrokeBegin(pt)).and_capture(),
                            );
                        }
                    }
                    CanvasTool::Eyedropper => {
                        if let Some(pos) = cursor.position()
                            && let Some([px, py]) = self.screen_to_image_px(pos, bounds)
                            && let Some(frame) = &self.frame
                        {
                            state.sampling = true;
                            let ix = (px.floor() as u32).min(frame.width.saturating_sub(1));
                            let iy = (py.floor() as u32).min(frame.height.saturating_sub(1));
                            return Some(
                                Action::publish(CanvasEvent::SampleColor([ix, iy])).and_capture(),
                            );
                        }
                    }
                    CanvasTool::Crop | CanvasTool::Pointer if self.crop_rect.is_some() => {
                        if let Some(pos) = cursor.position()
                            && let Some(uv) = self.screen_to_image_uv(pos, bounds)
                        {
                            state.cropping = true;
                            state.crop_anchor_uv = Some(uv);
                            return Some(Action::capture());
                        }
                    }
                    _ => {}
                }

                state.dragging = true;
                state.last_cursor = cursor.position();
                return Some(Action::capture());
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging_split {
                    state.dragging_split = false;
                    return Some(Action::capture());
                }
                if state.painting {
                    state.painting = false;
                    return Some(Action::publish(CanvasEvent::StrokeEnd(true)).and_capture());
                }
                if state.sampling {
                    state.sampling = false;
                    return Some(Action::capture());
                }
                if state.cropping {
                    state.cropping = false;
                    state.crop_anchor_uv = None;
                    return Some(Action::capture());
                }
                if state.dragging {
                    state.dragging = false;
                    state.last_cursor = None;
                    return Some(Action::capture());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.dragging_split => {
                let frac = ((position.x - bounds.x) / bounds.width.max(1.0)).clamp(0.02, 0.98);
                return Some(Action::publish(CanvasEvent::CompareSplitChanged(frac)).and_capture());
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.painting => {
                if let Some(pt) = self.screen_to_image_px(*position, bounds) {
                    return Some(Action::publish(CanvasEvent::StrokeStep(vec![pt])).and_capture());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.sampling => {
                if let Some([px, py]) = self.screen_to_image_px(*position, bounds)
                    && let Some(frame) = &self.frame
                {
                    let ix = (px.floor() as u32).min(frame.width.saturating_sub(1));
                    let iy = (py.floor() as u32).min(frame.height.saturating_sub(1));
                    return Some(Action::publish(CanvasEvent::SampleColor([ix, iy])).and_capture());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.cropping => {
                if let (Some(anchor), Some(cur)) = (
                    state.crop_anchor_uv,
                    self.screen_to_image_uv(*position, bounds),
                ) && let Some(rect) = self.constrained_crop(anchor, cur)
                {
                    return Some(Action::publish(CanvasEvent::CropUpdated(rect)).and_capture());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.dragging => {
                if let Some(prev) = state.last_cursor {
                    let dx = position.x - prev.x;
                    let dy = position.y - prev.y;
                    state.last_cursor = Some(*position);
                    return Some(Action::publish(CanvasEvent::PanBy { dx, dy }).and_capture());
                }
                state.last_cursor = Some(*position);
            }
            Event::Mouse(mouse::Event::CursorMoved { .. })
                if cursor.is_over(bounds)
                    && matches!(
                        self.tool,
                        CanvasTool::Brush | CanvasTool::Eraser | CanvasTool::Liquify
                    ) =>
            {
                return Some(Action::request_redraw());
            }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        _state: &Self::State,
        cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        let show_brush_ring = self.brush_radius_px > 0.0
            && matches!(
                self.tool,
                CanvasTool::Brush | CanvasTool::Eraser | CanvasTool::Liquify
            )
            && cursor.is_over(bounds);
        let cursor_doc_pos = if show_brush_ring
            && let Some(pos) = cursor.position()
            && let Some([dx, dy]) = self.screen_to_image_px(pos, bounds)
        {
            [dx as f32, dy as f32]
        } else {
            [-10000.0, -10000.0]
        };
        let cursor_radius_px = if show_brush_ring {
            self.brush_radius_px
        } else {
            0.0
        };
        ImageQuadPrimitive {
            frame: self.frame.clone(),
            pair_frame: self.pair_frame.clone(),
            dual_gap_px: self.dual_gap_px,
            compare_frame: self.compare_frame.clone(),
            zoom: self.zoom,
            pan_x: self.pan_x,
            pan_y: self.pan_y,
            adjustments: self.adjustments,
            dark_theme: self.dark_theme,
            crop_rect: self.crop_rect,
            quarter_turns: self.quarter_turns % 4,
            preview_angle_rad: self.preview_angle_rad,
            preview_fill: self.preview_fill,
            canvas_bg: self.canvas_bg,
            viewport_bg: self.viewport_bg,
            compare_mode: self.compare_mode,
            compare_split_x: self.compare_split_x,
            cursor_doc_pos,
            cursor_radius_px,
            bounds_size: [bounds.width.max(1.0), bounds.height.max(1.0)],
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging_split
            || (cursor
                .position()
                .is_some_and(|p| self.is_over_split_handle(p, bounds)))
        {
            mouse::Interaction::ResizingHorizontally
        } else if state.dragging {
            mouse::Interaction::Grabbing
        } else if !cursor.is_over(bounds) {
            mouse::Interaction::default()
        } else if self.crop_rect.is_some()
            || matches!(
                self.tool,
                CanvasTool::Brush
                    | CanvasTool::Eraser
                    | CanvasTool::Liquify
                    | CanvasTool::Eyedropper
                    | CanvasTool::Crop
            )
        {
            mouse::Interaction::Crosshair
        } else if self.tool == CanvasTool::Zoom {
            mouse::Interaction::ZoomIn
        } else {
            mouse::Interaction::Grab
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageQuadPrimitive {
    pub frame: Option<CpuImageFrame>,
    pub pair_frame: Option<CpuImageFrame>,
    pub dual_gap_px: f32,
    pub compare_frame: Option<CpuImageFrame>,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub adjustments: ColorAdjustments,
    pub dark_theme: bool,
    pub crop_rect: Option<[f32; 4]>,
    pub quarter_turns: u8,
    pub preview_angle_rad: f32,
    pub preview_fill: [f32; 4],
    pub canvas_bg: Option<[u8; 3]>,
    pub viewport_bg: Option<[u8; 3]>,
    pub compare_mode: u8,
    pub compare_split_x: f32,
    pub cursor_doc_pos: [f32; 2],
    pub cursor_radius_px: f32,
    pub bounds_size: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    viewport_size: [f32; 2],
    image_size: [f32; 2],
    pan: [f32; 2],
    zoom: f32,
    brightness: f32,
    contrast: f32,
    saturation: f32,
    exposure: f32,
    highlights: f32,
    shadows: f32,
    has_image: f32,
    dark_theme: f32,
    crop_active: f32,
    crop_rect: [f32; 4],
    quarter_turns: f32,
    preview_angle_rad: f32,
    has_canvas_bg: f32,
    has_compare: f32,
    canvas_bg: [f32; 4],
    preview_fill: [f32; 4],
    compare_mode: f32,
    compare_split_x: f32,
    gamma: f32,
    cursor_radius_px: f32,
    cursor_doc_pos: [f32; 2],
    has_viewport_bg: f32,
    _pad: f32,
    viewport_bg: [f32; 4],
    pair_size: [f32; 2],
    has_pair: f32,
    dual_gap_px: f32,
}

impl Uniforms {
    fn as_bytes(&self) -> &[u8] {
        // Safety: Uniforms is #[repr(C)] containing only plain f32 values (192 bytes).
        unsafe {
            std::slice::from_raw_parts(
                (self as *const Self).cast::<u8>(),
                std::mem::size_of::<Self>(),
            )
        }
    }
}

struct CachedSimpleTexture {
    revision: u64,
    width: u32,
    height: u32,
    view: wgpu::TextureView,
}

struct CachedGpuTexture {
    revision: u64,
    width: u32,
    height: u32,
    image_view: wgpu::TextureView,
    bound_compare: Option<(String, u64)>,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

pub struct ImageShaderPipeline {
    pipeline: wgpu::RenderPipeline,
    placeholder_uniform_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    placeholder_view: wgpu::TextureView,
    placeholder_bind_group: wgpu::BindGroup,
    textures: HashMap<String, CachedGpuTexture>,
    compare_textures: HashMap<String, CachedSimpleTexture>,
    recent_keys: Vec<String>,
}

const WGSL_SHADER: &str = r#"
struct Uniforms {
    viewport_size: vec2<f32>,
    image_size: vec2<f32>,
    pan: vec2<f32>,
    zoom: f32,
    brightness: f32,
    contrast: f32,
    saturation: f32,
    exposure: f32,
    highlights: f32,
    shadows: f32,
    has_image: f32,
    dark_theme: f32,
    crop_active: f32,
    crop_rect: vec4<f32>,
    quarter_turns: f32,
    preview_angle_rad: f32,
    has_canvas_bg: f32,
    has_compare: f32,
    canvas_bg: vec4<f32>,
    preview_fill: vec4<f32>,
    compare_mode: f32,
    compare_split_x: f32,
    gamma: f32,
    cursor_radius_px: f32,
    cursor_doc_pos: vec2<f32>,
    has_viewport_bg: f32,
    _pad: f32,
    viewport_bg: vec4<f32>,
    pair_size: vec2<f32>,
    has_pair: f32,
    dual_gap_px: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var img_tex: texture_2d<f32>;
@group(0) @binding(2) var img_sampler: sampler;
@group(0) @binding(3) var compare_tex: texture_2d<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );
    var out: VsOut;
    out.pos = vec4<f32>(positions[idx], 0.0, 1.0);
    out.uv = uvs[idx];
    return out;
}

fn apply_color_and_light(in_rgb: vec3<f32>) -> vec3<f32> {
    var rgb = in_rgb * exp2(clamp(u.exposure, -2.0, 2.0));
    let gamma = clamp(u.gamma, 0.2, 3.0);
    if (abs(gamma - 1.0) > 0.0001) {
        rgb = pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(1.0 / gamma));
    }
    let luma_pre = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let shadows_w = 1.0 - smoothstep(0.0, 0.55, luma_pre);
    let highlights_w = smoothstep(0.45, 1.0, luma_pre);
    let tone_delta = u.shadows * 0.5 * shadows_w + u.highlights * 0.5 * highlights_w;
    rgb = rgb + vec3<f32>(tone_delta);

    let c = clamp(u.contrast, -1.0, 1.0);
    let contrast = select(1.0 + c, 1.0 / (1.0 - c * 0.999), c >= 0.0);
    rgb = (rgb - vec3<f32>(0.5)) * contrast + vec3<f32>(0.5) + vec3<f32>(u.brightness);

    let luma = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let sat = max(1.0 + u.saturation, 0.0);
    return clamp(mix(vec3<f32>(luma), rgb, sat), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Compute the UV of `pixel` within a fitted image, given a sub-viewport
// defined by its left edge `vp_x0` and width `vp_w`, full height `vp_h`.
// Uses the same zoom/pan uniforms for synchronized navigation.
// Returns a vec3: (uv.x, uv.y, in_bounds) where in_bounds is 1.0 or 0.0.
fn half_vp_uv(pixel: vec2<f32>, img_sz: vec2<f32>, vp_x0: f32, vp_w: f32, vp_h: f32) -> vec3<f32> {
    let half_vp = vec2<f32>(vp_w, vp_h);
    let img_aspect = img_sz.x / max(img_sz.y, 1.0);
    let hv_aspect = vp_w / max(vp_h, 1.0);
    var fit: vec2<f32>;
    if (img_aspect > hv_aspect) {
        fit = vec2<f32>(vp_w, vp_w / img_aspect);
    } else {
        fit = vec2<f32>(vp_h * img_aspect, vp_h);
    }
    let drawn = fit * u.zoom;
    // Pan is relative to the half-viewport centre
    let half_center = vec2<f32>(vp_x0 + vp_w * 0.5, vp_h * 0.5) + u.pan;
    let tl = half_center - drawn * 0.5;
    let uv = (pixel - tl) / drawn;
    let in_bounds = select(0.0, 1.0,
        uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0);
    return vec3<f32>(clamp(uv.x, 0.0, 1.0), clamp(uv.y, 0.0, 1.0), in_bounds);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let pixel = in.uv * u.viewport_size;
    let default_bg = select(
        vec3<f32>(0.9137255, 0.9254902, 0.9411765),
        vec3<f32>(0.0627451, 0.07058824, 0.078431375),
        u.dark_theme > 0.5
    );
    let bg = select(default_bg, u.viewport_bg.rgb, u.has_viewport_bg > 0.5);

    if (u.has_image < 0.5) {
        return vec4<f32>(bg, 1.0);
    }

    let turns = i32(round(u.quarter_turns)) & 3;
    let odd_rot = (turns & 1) == 1;

    // ── Mode 2: Side-by-Side ──────────────────────────────────────────────────
    // Left half  = compare_frame (Original), fitted to left half-viewport.
    // Right half = primary edited frame,     fitted to right half-viewport.
    // Both halves share the same zoom & pan → synchronized navigation.
    if (u.compare_mode > 1.5 && u.compare_mode < 2.5 && u.has_compare > 0.5) {
        let vp_w = u.viewport_size.x;
        let vp_h = u.viewport_size.y;
        let half_w = vp_w * 0.5;
        let center_px = half_w;
        let sbs_size = select(u.image_size, vec2<f32>(u.image_size.y, u.image_size.x), odd_rot);

        // Checkerboard for this pixel
        let tile2 = vec2<i32>(floor(pixel / 10.0));
        let odd2 = ((tile2.x + tile2.y) & 1) == 1;
        let ca2 = select(vec3<f32>(0.85, 0.85, 0.85), vec3<f32>(0.18, 0.19, 0.21), u.dark_theme > 0.5);
        let cb2 = select(vec3<f32>(0.75, 0.75, 0.75), vec3<f32>(0.24, 0.25, 0.28), u.dark_theme > 0.5);
        var bd2 = select(ca2, cb2, odd2);
        if (u.has_canvas_bg > 0.5) {
            bd2 = mix(bd2, u.canvas_bg.rgb, u.canvas_bg.a);
        }

        // Crisp 1-px white center divider
        if (abs(pixel.x - center_px) < 1.0) {
            return vec4<f32>(1.0, 1.0, 1.0, 1.0);
        }

        var out2: vec3<f32> = bg;
        let is_left = pixel.x < center_px;
        let r = half_vp_uv(pixel, sbs_size, select(half_w, 0.0, is_left), half_w, vp_h);
        if (r.z > 0.5) {
            var sbs_uv = r.xy;
            if (turns == 1) {
                sbs_uv = vec2<f32>(sbs_uv.y, 1.0 - sbs_uv.x);
            } else if (turns == 2) {
                sbs_uv = vec2<f32>(1.0 - sbs_uv.x, 1.0 - sbs_uv.y);
            } else if (turns == 3) {
                sbs_uv = vec2<f32>(1.0 - sbs_uv.y, sbs_uv.x);
            }
            if (is_left) {
                // Left half: Original (compare_tex)
                let s = textureSample(compare_tex, img_sampler, sbs_uv);
                out2 = mix(bd2, s.rgb, s.a);
            } else {
                // Right half: Edited primary (img_tex)
                let s = textureSample(img_tex, img_sampler, sbs_uv);
                let adj = apply_color_and_light(s.rgb);
                out2 = mix(bd2, adj, s.a);
            }
        }
        return vec4<f32>(out2, 1.0);
    }
    // ─────────────────────────────────────────────────────────────────────────
    var doc_size = u.image_size;
    var dual_left_w = u.image_size.x;
    var dual_right_w = 0.0;
    var dual_gap = 0.0;
    if (u.has_pair > 0.5) {
        let target_h = max(u.image_size.y, u.pair_size.y);
        dual_left_w = round(u.image_size.x * (target_h / max(u.image_size.y, 1.0)));
        dual_right_w = round(u.pair_size.x * (target_h / max(u.pair_size.y, 1.0)));
        if (u.dual_gap_px > 0.0) {
            dual_gap = u.dual_gap_px * max(target_h / 900.0, 1.0);
        }
        doc_size = vec2<f32>(dual_left_w + dual_gap + dual_right_w, target_h);
    }
    let base_size = select(doc_size, vec2<f32>(doc_size.y, doc_size.x), odd_rot);

    let ca = cos(u.preview_angle_rad);
    let sa = sin(u.preview_angle_rad);
    let abs_ca = abs(ca);
    let abs_sa = abs(sa);
    let eff_size = vec2<f32>(
        base_size.x * abs_ca + base_size.y * abs_sa,
        base_size.x * abs_sa + base_size.y * abs_ca
    );

    let vp_aspect = u.viewport_size.x / max(u.viewport_size.y, 1.0);
    let img_aspect = eff_size.x / max(eff_size.y, 1.0);
    var fit_size = u.viewport_size;
    if (img_aspect > vp_aspect) {
        fit_size.y = u.viewport_size.x / img_aspect;
    } else {
        fit_size.x = u.viewport_size.y * img_aspect;
    }
    let drawn_size = fit_size * u.zoom;
    let center = u.viewport_size * 0.5 + u.pan;
    let top_left = center - drawn_size * 0.5;
    let img_uv = (pixel - top_left) / drawn_size;

    if (img_uv.x < 0.0 || img_uv.x > 1.0 || img_uv.y < 0.0 || img_uv.y > 1.0) {
        return vec4<f32>(bg, 1.0);
    }

    // Checkerboard behind alpha regions
    let tile = vec2<i32>(floor(pixel / 10.0));
    let odd = ((tile.x + tile.y) & 1) == 1;
    let check_a = select(vec3<f32>(0.85, 0.85, 0.85), vec3<f32>(0.18, 0.19, 0.21), u.dark_theme > 0.5);
    let check_b = select(vec3<f32>(0.75, 0.75, 0.75), vec3<f32>(0.24, 0.25, 0.28), u.dark_theme > 0.5);
    var backdrop = select(check_a, check_b, odd);
    if (u.has_canvas_bg > 0.5) {
        backdrop = mix(backdrop, u.canvas_bg.rgb, u.canvas_bg.a);
    }

    var rot_uv = img_uv;
    var in_rot_bounds = true;
    if (abs(u.preview_angle_rad) > 0.0001) {
        let p_rot = (img_uv - vec2<f32>(0.5)) * eff_size;
        let p_src = vec2<f32>(
            p_rot.x * ca + p_rot.y * sa,
            -p_rot.x * sa + p_rot.y * ca
        );
        rot_uv = p_src / base_size + vec2<f32>(0.5);
        in_rot_bounds = rot_uv.x >= 0.0 && rot_uv.x <= 1.0 && rot_uv.y >= 0.0 && rot_uv.y <= 1.0;
    }

    var tex_uv = clamp(rot_uv, vec2<f32>(0.0), vec2<f32>(1.0));
    if (turns == 1) {
        tex_uv = vec2<f32>(tex_uv.y, 1.0 - tex_uv.x);
    } else if (turns == 2) {
        tex_uv = vec2<f32>(1.0 - tex_uv.x, 1.0 - tex_uv.y);
    } else if (turns == 3) {
        tex_uv = vec2<f32>(1.0 - tex_uv.y, tex_uv.x);
    }

    let spread_x = tex_uv.x * doc_size.x;
    let is_dual = u.has_pair > 0.5;
    let in_dual_gap = is_dual && spread_x >= dual_left_w && spread_x < (dual_left_w + dual_gap);
    let on_right_page = is_dual && spread_x >= (dual_left_w + dual_gap);

    let primary_uv = select(
        tex_uv,
        vec2<f32>(clamp(spread_x / max(dual_left_w, 1.0), 0.0, 1.0), tex_uv.y),
        is_dual
    );
    let secondary_uv = select(
        tex_uv,
        vec2<f32>(clamp((spread_x - dual_left_w - dual_gap) / max(dual_right_w, 1.0), 0.0, 1.0), tex_uv.y),
        is_dual
    );

    let prim_texel = textureSample(img_tex, img_sampler, primary_uv);
    let sec_texel = textureSample(compare_tex, img_sampler, secondary_uv);
    let texel = select(prim_texel, sec_texel, on_right_page);
    let comp_texel = sec_texel;

    if (in_rot_bounds && in_dual_gap) {
        return vec4<f32>(bg, 1.0);
    }

    var out_rgb = backdrop;
    if (!in_rot_bounds) {
        out_rgb = mix(backdrop, u.preview_fill.rgb, u.preview_fill.a);
    } else {
        // Mode 1 (Split): left of split_x shows compare; Mode 3 (HoldOriginal): whole image shows compare
        let show_compare = u.has_compare > 0.5 && (
            u.compare_mode > 2.5 ||
            (u.compare_mode > 0.5 && u.compare_mode < 1.5 && in.uv.x < u.compare_split_x)
        );
        if (show_compare) {
            out_rgb = mix(backdrop, comp_texel.rgb, comp_texel.a);
        } else {
            let adj_rgb = apply_color_and_light(texel.rgb);
            out_rgb = mix(backdrop, adj_rgb, texel.a);
        }
    }

    // Mode 1 (Split): draggable vertical divider line
    if (u.compare_mode > 0.5 && u.compare_mode < 1.5) {
        let split_px = u.compare_split_x * u.viewport_size.x;
        if (abs(pixel.x - split_px) < 1.25) {
            out_rgb = vec3<f32>(1.0, 1.0, 1.0);
        }
    }

    if (u.crop_active > 0.5) {
        let c0 = u.crop_rect.xy;
        let c1 = u.crop_rect.zw;
        let inside = img_uv.x >= c0.x && img_uv.x <= c1.x && img_uv.y >= c0.y && img_uv.y <= c1.y;
        if (!inside) {
            out_rgb = out_rgb * 0.35;
        } else {
            let px_uv = 1.5 / max(drawn_size, vec2<f32>(1.0));
            let on_edge = (img_uv.x - c0.x) < px_uv.x || (c1.x - img_uv.x) < px_uv.x || (img_uv.y - c0.y) < px_uv.y || (c1.y - img_uv.y) < px_uv.y;
            if (on_edge) {
                out_rgb = vec3<f32>(1.0, 1.0, 1.0);
            } else {
                let c_size = max(c1 - c0, vec2<f32>(0.001));
                let rel = (img_uv - c0) / c_size;
                let g_uv = 1.0 / max(drawn_size * c_size, vec2<f32>(1.0));
                let on_third = abs(rel.x - 0.33333) < g_uv.x || abs(rel.x - 0.66667) < g_uv.x || abs(rel.y - 0.33333) < g_uv.y || abs(rel.y - 0.66667) < g_uv.y;
                if (on_third) {
                    out_rgb = mix(out_rgb, vec3<f32>(1.0), 0.28);
                }
            }
        }
    }

    if (u.cursor_radius_px > 0.5 && u.cursor_doc_pos.x > -9000.0) {
        let doc_px = rot_uv * base_size;
        let dist = length(doc_px - u.cursor_doc_pos);
        let px_per_screen = max(base_size.x / max(drawn_size.x, 1.0), 0.25);
        let diff = abs(dist - u.cursor_radius_px);
        if (diff <= px_per_screen * 0.9) {
            out_rgb = vec3<f32>(1.0, 1.0, 1.0);
        } else if (diff <= px_per_screen * 1.75) {
            out_rgb = vec3<f32>(0.05, 0.05, 0.08);
        }
    }
    return vec4<f32>(out_rgb, 1.0);
}
"#;

impl Pipeline for ImageShaderPipeline {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("photon_image_shader"),
            source: wgpu::ShaderSource::Wgsl(WGSL_SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("photon_image_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("photon_image_pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("photon_image_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        });

        let placeholder_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("photon_image_placeholder_uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("photon_image_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let placeholder_view = upload_rgba_texture(device, queue, 1, 1, &[0, 0, 0, 0]);
        let placeholder_bind_group = build_bind_group(
            device,
            &bind_group_layout,
            &placeholder_uniform_buffer,
            &sampler,
            &placeholder_view,
            &placeholder_view,
        );

        Self {
            pipeline,
            placeholder_uniform_buffer,
            bind_group_layout,
            sampler,
            placeholder_view,
            placeholder_bind_group,
            textures: HashMap::new(),
            compare_textures: HashMap::new(),
            recent_keys: Vec::new(),
        }
    }

    fn trim(&mut self) {
        if self.textures.len() > 16 {
            let keep: Vec<String> = self.recent_keys.iter().rev().take(4).cloned().collect();
            self.textures.retain(|k, _| keep.contains(k));
        }
        if self.compare_textures.len() > 8 {
            self.compare_textures.clear();
        }
    }
}

impl Primitive for ImageQuadPrimitive {
    type Pipeline = ImageShaderPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        let mut has_image = 0.0f32;
        let mut img_size = [1.0f32, 1.0f32];
        let mut has_compare = 0.0f32;
        let mut has_pair = 0.0f32;
        let mut pair_size = [1.0f32, 1.0f32];

        let secondary_ref = if self.compare_mode > 0 {
            self.compare_frame.as_ref()
        } else {
            self.pair_frame.as_ref()
        };

        let secondary_key_rev = secondary_ref.map(|c| (c.key.clone(), c.revision));
        if let Some(sec) = secondary_ref {
            let needs_sec = pipeline
                .compare_textures
                .get(&sec.key)
                .is_none_or(|t| t.revision != sec.revision);
            if needs_sec {
                let max_dim = device.limits().max_texture_dimension_2d.min(4096);
                let (w, h, bytes) = fit_within_gpu_limit(sec.width, sec.height, &sec.rgba, max_dim);
                let view = upload_rgba_texture(device, queue, w, h, &bytes);
                pipeline.compare_textures.insert(
                    sec.key.clone(),
                    CachedSimpleTexture {
                        revision: sec.revision,
                        width: w,
                        height: h,
                        view,
                    },
                );
            }
            if self.compare_mode > 0 {
                has_compare = 1.0;
            } else if let Some(cached) = pipeline.compare_textures.get(&sec.key) {
                has_pair = 1.0;
                pair_size = [cached.width as f32, cached.height as f32];
            }
        }

        let secondary_view = secondary_ref
            .and_then(|c| pipeline.compare_textures.get(&c.key))
            .map(|t| &t.view)
            .unwrap_or(&pipeline.placeholder_view);

        if let Some(frame) = &self.frame {
            let needs_upload = pipeline
                .textures
                .get(&frame.key)
                .is_none_or(|tex| tex.revision != frame.revision);

            if needs_upload {
                let max_dim = device.limits().max_texture_dimension_2d.min(4096);
                let (w, h, bytes) =
                    fit_within_gpu_limit(frame.width, frame.height, &frame.rgba, max_dim);
                let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("photon_image_uniforms"),
                    size: std::mem::size_of::<Uniforms>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let image_view = upload_rgba_texture(device, queue, w, h, &bytes);
                let bind_group = build_bind_group(
                    device,
                    &pipeline.bind_group_layout,
                    &uniform_buffer,
                    &pipeline.sampler,
                    &image_view,
                    secondary_view,
                );
                pipeline.textures.insert(
                    frame.key.clone(),
                    CachedGpuTexture {
                        revision: frame.revision,
                        width: w,
                        height: h,
                        image_view,
                        bound_compare: secondary_key_rev.clone(),
                        uniform_buffer,
                        bind_group,
                    },
                );
            } else if let Some(tex) = pipeline.textures.get_mut(&frame.key)
                && tex.bound_compare != secondary_key_rev
            {
                tex.bind_group = build_bind_group(
                    device,
                    &pipeline.bind_group_layout,
                    &tex.uniform_buffer,
                    &pipeline.sampler,
                    &tex.image_view,
                    secondary_view,
                );
                tex.bound_compare = secondary_key_rev;
            }

            if let Some(tex) = pipeline.textures.get(&frame.key) {
                has_image = 1.0;
                img_size = [tex.width as f32, tex.height as f32];
                pipeline.recent_keys.retain(|k| k != &frame.key);
                pipeline.recent_keys.push(frame.key.clone());
                if pipeline.recent_keys.len() > 32 {
                    pipeline.recent_keys.remove(0);
                }
            }
        }

        let (has_canvas_bg, canvas_bg) = match self.canvas_bg {
            Some([r, g, b]) => (
                1.0,
                [
                    f32::from(r) / 255.0,
                    f32::from(g) / 255.0,
                    f32::from(b) / 255.0,
                    1.0,
                ],
            ),
            None => (0.0, [0.0; 4]),
        };

        let (has_viewport_bg, viewport_bg) = match self.viewport_bg {
            Some([r, g, b]) => (
                1.0,
                [
                    f32::from(r) / 255.0,
                    f32::from(g) / 255.0,
                    f32::from(b) / 255.0,
                    1.0,
                ],
            ),
            None => (0.0, [0.0; 4]),
        };

        let uniforms = Uniforms {
            viewport_size: self.bounds_size,
            image_size: img_size,
            pan: [self.pan_x, self.pan_y],
            zoom: self.zoom.max(0.05),
            brightness: self.adjustments.brightness,
            contrast: self.adjustments.contrast,
            saturation: self.adjustments.saturation,
            exposure: self.adjustments.exposure,
            highlights: self.adjustments.highlights,
            shadows: self.adjustments.shadows,
            has_image,
            dark_theme: if self.dark_theme { 1.0 } else { 0.0 },
            crop_active: if self.crop_rect.is_some() { 1.0 } else { 0.0 },
            crop_rect: self.crop_rect.unwrap_or([0.0, 0.0, 1.0, 1.0]),
            quarter_turns: f32::from(self.quarter_turns % 4),
            preview_angle_rad: self.preview_angle_rad,
            has_canvas_bg,
            has_compare,
            canvas_bg,
            preview_fill: self.preview_fill,
            compare_mode: f32::from(self.compare_mode),
            compare_split_x: self.compare_split_x.clamp(0.0, 1.0),
            gamma: self.adjustments.gamma.clamp(0.2, 3.0),
            cursor_radius_px: self.cursor_radius_px.max(0.0),
            cursor_doc_pos: self.cursor_doc_pos,
            has_viewport_bg,
            _pad: 0.0,
            viewport_bg,
            pair_size,
            has_pair,
            dual_gap_px: self.dual_gap_px.max(0.0),
        };

        let target_buf = self
            .frame
            .as_ref()
            .and_then(|f| pipeline.textures.get(&f.key))
            .map(|t| &t.uniform_buffer)
            .unwrap_or(&pipeline.placeholder_uniform_buffer);
        queue.write_buffer(target_buf, 0, uniforms.as_bytes());
    }

    fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        let bind_group = self
            .frame
            .as_ref()
            .and_then(|f| pipeline.textures.get(&f.key))
            .map(|t| &t.bind_group)
            .unwrap_or(&pipeline.placeholder_bind_group);

        render_pass.set_pipeline(&pipeline.pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.draw(0..6, 0..1);
        true
    }
}

fn fit_within_gpu_limit(w: u32, h: u32, rgba: &[u8], max_dim: u32) -> (u32, u32, Vec<u8>) {
    if w <= max_dim && h <= max_dim {
        return (w, h, rgba.to_vec());
    }
    if let Some(img) = RgbaImage::from_raw(w, h, rgba.to_vec()) {
        let scaled = DynamicImage::ImageRgba8(img)
            .thumbnail(max_dim, max_dim)
            .to_rgba8();
        let (nw, nh) = scaled.dimensions();
        return (nw, nh, scaled.into_raw());
    }
    (w, h, rgba.to_vec())
}

fn upload_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> wgpu::TextureView {
    let size = wgpu::Extent3d {
        width: width.max(1),
        height: height.max(1),
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("photon_image_texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    if rgba.len() == (size.width as usize) * (size.height as usize) * 4 {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * size.width),
                rows_per_image: Some(size.height),
            },
            size,
        );
    }
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn build_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    image_view: &wgpu::TextureView,
    compare_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("photon_image_bind_group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(image_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(compare_view),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dual_spread_geometry_normalizes_height_and_scales_gap() {
        let (sw, sh, left_w, gap) = dual_spread_geometry((800, 1200), (1200, 1800), 8.0);
        assert_eq!(sh, 1800.0);
        assert_eq!(left_w, 1200.0);
        assert_eq!(gap, 16.0);
        assert_eq!(sw, 1200.0 + 16.0 + 1200.0);

        let (sw0, sh0, lw0, g0) = dual_spread_geometry((800, 900), (800, 900), 0.0);
        assert_eq!((sw0, sh0, lw0, g0), (1600.0, 900.0, 800.0, 0.0));
    }

    #[test]
    fn default_viewport_bg_rgb_matches_palette_canvas() {
        assert_eq!(default_viewport_bg_rgb(true), [16, 18, 20]);
        assert_eq!(default_viewport_bg_rgb(false), [233, 236, 240]);
    }
}
