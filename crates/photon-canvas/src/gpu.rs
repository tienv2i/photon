//! WGSL shader sources, GPU texture management, and `wgpu` rendering pipeline for `ImageCanvasProgram`.

use std::collections::HashMap;
use std::sync::Arc;

use iced::Rectangle;
use iced::wgpu;
use iced::widget::shader::{Pipeline, Primitive, Viewport};
use image::{DynamicImage, RgbaImage};
use photon_protocol::{
    BlendMode, CanvasAntiAliasingMode, ColorAdjustments, GradientKind, GradientRepeat,
    ViewportDenoiseMode,
};

use crate::{CpuImageFrame, CpuMaskFrame, GradientPreview, OverlayVertex, TransformPreview};

/// Maximum GPU upload dimension clamp (bounded by `device.limits().max_texture_dimension_2d`).
pub const MAX_GPU_TEXTURE_DIM: u32 = 16384;

#[derive(Debug, Clone)]
pub struct ImageQuadPrimitive {
    pub frame: Option<CpuImageFrame>,
    pub pair_frame: Option<CpuImageFrame>,
    pub dual_gap_px: f32,
    pub compare_frame: Option<CpuImageFrame>,
    pub selection_mask: Option<CpuMaskFrame>,
    pub has_selection: bool,
    pub quick_mask_active: bool,
    pub marching_ants_phase: f32,
    pub sel_preview_kind: f32,
    pub sel_preview_rect: [f32; 4],
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub adjustments: ColorAdjustments,
    pub adj_selection_only: bool,
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
    pub overlay_lines: Vec<OverlayVertex>,
    pub overlay_tris: Vec<OverlayVertex>,
    pub curve_lut: Option<[u8; 1024]>,
    pub aa_mode: CanvasAntiAliasingMode,
    pub denoise_mode: ViewportDenoiseMode,
    pub transform_preview: Option<TransformPreview>,
    pub gradient_preview: Option<GradientPreview>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Uniforms {
    pub viewport_size: [f32; 2],
    pub image_size: [f32; 2],
    pub pan: [f32; 2],
    pub zoom: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub exposure: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub has_image: f32,
    pub dark_theme: f32,
    pub crop_active: f32,
    pub crop_rect: [f32; 4],
    pub quarter_turns: f32,
    pub preview_angle_rad: f32,
    pub has_canvas_bg: f32,
    pub has_compare: f32,
    pub canvas_bg: [f32; 4],
    pub preview_fill: [f32; 4],
    pub compare_mode: f32,
    pub compare_split_x: f32,
    pub gamma: f32,
    pub cursor_radius_px: f32,
    pub cursor_doc_pos: [f32; 2],
    pub has_viewport_bg: f32,
    pub _pad: f32,
    pub viewport_bg: [f32; 4],
    pub pair_size: [f32; 2],
    pub has_pair: f32,
    pub dual_gap_px: f32,
    pub has_selection: f32,
    pub quick_mask_active: f32,
    pub marching_ants_phase: f32,
    pub sel_preview_kind: f32,
    pub sel_preview_rect: [f32; 4],
    pub has_curve_lut: f32,
    pub aa_mode: f32,
    pub denoise_mode: f32,
    pub transform_active: f32,
    pub transform_layer_offset: [f32; 2],
    pub transform_layer_size: [f32; 2],
    pub transform_translate: [f32; 2],
    pub transform_scale: [f32; 2],
    pub transform_pivot: [f32; 2],
    pub transform_cos_sin: [f32; 2],
    pub transform_has_base: f32,
    pub adj_selection_only: f32,
    pub _pad3: [f32; 2],
    pub gradient_line: [f32; 4],
    pub gradient_params: [f32; 4],
    pub gradient_meta: [f32; 4],
    pub gradient_stop_colors: [[f32; 4]; 12],
    pub gradient_stop_pos: [[f32; 4]; 3],
}

impl Uniforms {
    fn as_bytes(&self) -> &[u8] {
        // Safety: Uniforms is #[repr(C)] containing only plain f32 values (304 bytes).
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
    rgba_ptr: usize,
    view: wgpu::TextureView,
}

type SimpleFrameKey = (String, u64, usize, u32, u32);

struct CachedGpuTexture {
    revision: u64,
    width: u32,
    height: u32,
    image_view: wgpu::TextureView,
    bound_base: Option<SimpleFrameKey>,
    bound_compare: Option<SimpleFrameKey>,
    bound_sel: Option<SimpleFrameKey>,
    bound_transform_layer: Option<SimpleFrameKey>,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

pub struct ImageShaderPipeline {
    pipeline: wgpu::RenderPipeline,
    overlay_line_pipeline: wgpu::RenderPipeline,
    overlay_tri_pipeline: wgpu::RenderPipeline,
    overlay_line_buffer: wgpu::Buffer,
    overlay_line_capacity: usize,
    overlay_line_count: u32,
    overlay_tri_buffer: wgpu::Buffer,
    overlay_tri_capacity: usize,
    overlay_tri_count: u32,
    placeholder_uniform_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    sel_sampler: wgpu::Sampler,
    curve_lut_texture: wgpu::Texture,
    curve_lut_view: wgpu::TextureView,
    last_curve_lut: Option<[u8; 1024]>,
    placeholder_view: wgpu::TextureView,
    placeholder_bind_group: wgpu::BindGroup,
    textures: HashMap<String, CachedGpuTexture>,
    compare_textures: HashMap<String, CachedSimpleTexture>,
    sel_mask_textures: HashMap<String, CachedSimpleTexture>,
    transform_base_textures: HashMap<String, CachedSimpleTexture>,
    transform_layer_textures: HashMap<String, CachedSimpleTexture>,
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
    has_selection: f32,
    quick_mask_active: f32,
    marching_ants_phase: f32,
    sel_preview_kind: f32,
    sel_preview_rect: vec4<f32>,
    has_curve_lut: f32,
    aa_mode: f32,
    denoise_mode: f32,
    transform_active: f32,
    transform_layer_offset: vec2<f32>,
    transform_layer_size: vec2<f32>,
    transform_translate: vec2<f32>,
    transform_scale: vec2<f32>,
    transform_pivot: vec2<f32>,
    transform_cos_sin: vec2<f32>,
    transform_has_base: f32,
    adj_selection_only: f32,
    _pad3: vec2<f32>,
    gradient_line: vec4<f32>,
    gradient_params: vec4<f32>,
    gradient_meta: vec4<f32>,
    gradient_stop_colors: array<vec4<f32>, 12>,
    gradient_stop_pos: array<vec4<f32>, 3>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var img_tex: texture_2d<f32>;
@group(0) @binding(2) var img_sampler: sampler;
@group(0) @binding(3) var compare_tex: texture_2d<f32>;
@group(0) @binding(4) var sel_mask_tex: texture_2d<f32>;
@group(0) @binding(5) var sel_sampler: sampler;
@group(0) @binding(6) var curve_lut_tex: texture_2d<f32>;
@group(0) @binding(7) var transform_layer_tex: texture_2d<f32>;

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

fn catmull_rom_weight(x: f32) -> f32 {
    let ax = abs(x);
    if (ax < 1.0) {
        return 1.5 * ax * ax * ax - 2.5 * ax * ax + 1.0;
    } else if (ax < 2.0) {
        return -0.5 * ax * ax * ax + 2.5 * ax * ax - 4.0 * ax + 2.0;
    }
    return 0.0;
}

fn sample_bicubic_catmull_rom(tex: texture_2d<f32>, uv: vec2<f32>) -> vec4<f32> {
    let tex_size = max(vec2<f32>(textureDimensions(tex)), vec2<f32>(1.0));
    let coord = uv * tex_size - vec2<f32>(0.5);
    let base = floor(coord);
    let f = coord - base;
    var sum = vec4<f32>(0.0);
    var w_sum = 0.0;
    for (var j: i32 = -1; j <= 2; j = j + 1) {
        let wy = catmull_rom_weight(f32(j) - f.y);
        for (var i: i32 = -1; i <= 2; i = i + 1) {
            let wx = catmull_rom_weight(f32(i) - f.x);
            let w = wx * wy;
            let sample_uv = clamp(
                (base + vec2<f32>(f32(i) + 0.5, f32(j) + 0.5)) / tex_size,
                vec2<f32>(0.0),
                vec2<f32>(1.0)
            );
            let s = textureSampleLevel(tex, sel_sampler, sample_uv, 0.0);
            sum = sum + s * w;
            w_sum = w_sum + w;
        }
    }
    return clamp(sum / max(w_sum, 0.0001), vec4<f32>(0.0), vec4<f32>(1.0));
}

fn sample_aa_tex(tex: texture_2d<f32>, uv: vec2<f32>) -> vec4<f32> {
    if (u.aa_mode < 0.5) {
        return textureSampleLevel(tex, sel_sampler, uv, 0.0);
    } else if (u.aa_mode < 1.5) {
        return textureSampleLevel(tex, img_sampler, uv, 0.0);
    } else {
        return sample_bicubic_catmull_rom(tex, uv);
    }
}

fn sample_denoised_image(uv: vec2<f32>) -> vec4<f32> {
    let center = sample_aa_tex(img_tex, uv);
    if (u.denoise_mode < 0.5) {
        return center;
    }
    var sigma_c = 0.08;
    var sigma_s = 1.4;
    var strength = 0.55;
    if (u.denoise_mode > 2.5) {
        sigma_c = 0.22;
        sigma_s = 2.2;
        strength = 0.95;
    } else if (u.denoise_mode > 1.5) {
        sigma_c = 0.14;
        sigma_s = 1.8;
        strength = 0.78;
    }
    let tex_size = max(vec2<f32>(textureDimensions(img_tex)), vec2<f32>(1.0));
    let step_uv = 1.0 / tex_size;
    let inv_2sc2 = 1.0 / (2.0 * sigma_c * sigma_c);
    let inv_2ss2 = 1.0 / (2.0 * sigma_s * sigma_s);
    var acc = vec3<f32>(0.0);
    var w_total = 0.0;
    for (var dy: i32 = -2; dy <= 2; dy = dy + 1) {
        for (var dx: i32 = -2; dx <= 2; dx = dx + 1) {
            let off = vec2<f32>(f32(dx), f32(dy));
            let suv = clamp(uv + off * step_uv, vec2<f32>(0.0), vec2<f32>(1.0));
            let n = textureSampleLevel(img_tex, img_sampler, suv, 0.0);
            let diff = n.rgb - center.rgb;
            let dist2 = dot(diff, diff);
            let spatial2 = dot(off, off);
            let w = exp(-spatial2 * inv_2ss2 - dist2 * inv_2sc2) * max(n.a, 0.01);
            acc = acc + n.rgb * w;
            w_total = w_total + w;
        }
    }
    let denoised_rgb = acc / max(w_total, 0.0001);
    return vec4<f32>(mix(center.rgb, denoised_rgb, strength), center.a);
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

fn apply_curve_lut(rgb: vec3<f32>) -> vec3<f32> {
    if (u.has_curve_lut < 0.5) {
        return rgb;
    }
    let c = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    let ur = (c.r * 255.0 + 0.5) / 256.0;
    let ug = (c.g * 255.0 + 0.5) / 256.0;
    let ub = (c.b * 255.0 + 0.5) / 256.0;
    let r = textureSampleLevel(curve_lut_tex, img_sampler, vec2<f32>(ur, 0.5), 0.0).r;
    let g = textureSampleLevel(curve_lut_tex, img_sampler, vec2<f32>(ug, 0.5), 0.0).g;
    let b = textureSampleLevel(curve_lut_tex, img_sampler, vec2<f32>(ub, 0.5), 0.0).b;
    return vec3<f32>(r, g, b);
}

fn doc_px_to_transform_local(doc_px: vec2<f32>) -> vec2<f32> {
    let piv_local = u.transform_pivot * u.transform_layer_size;
    let piv_world = u.transform_layer_offset + piv_local + u.transform_translate;
    let w = doc_px - piv_world;
    let c = u.transform_cos_sin.x;
    let s = u.transform_cos_sin.y;
    let u_rot = vec2<f32>(w.x * c + w.y * s, -w.x * s + w.y * c);
    let safe_sx = select(u.transform_scale.x, select(1e-5, -1e-5, u.transform_scale.x < 0.0), abs(u.transform_scale.x) < 1e-5);
    let safe_sy = select(u.transform_scale.y, select(1e-5, -1e-5, u.transform_scale.y < 0.0), abs(u.transform_scale.y) < 1e-5);
    return u_rot / vec2<f32>(safe_sx, safe_sy) + piv_local;
}

fn fetch_transform_texel(ix: i32, iy: i32, tex_size_i: vec2<i32>) -> vec4<f32> {
    if (ix < 0 || iy < 0 || ix >= tex_size_i.x || iy >= tex_size_i.y) {
        return vec4<f32>(0.0);
    }
    return textureLoad(transform_layer_tex, vec2<i32>(ix, iy), 0);
}

fn sample_transform_layer(local_px: vec2<f32>) -> vec4<f32> {
    let tex_size_u = textureDimensions(transform_layer_tex);
    let tex_size_i = vec2<i32>(i32(tex_size_u.x), i32(tex_size_u.y));
    let tex_size = max(vec2<f32>(f32(tex_size_i.x), f32(tex_size_i.y)), vec2<f32>(1.0));
    let tex_px = local_px * (tex_size / max(u.transform_layer_size, vec2<f32>(1.0)));

    if (u.aa_mode < 0.5) {
        let ix = i32(floor(tex_px.x));
        let iy = i32(floor(tex_px.y));
        return fetch_transform_texel(ix, iy, tex_size_i);
    }

    let coord = tex_px - vec2<f32>(0.5);
    if (coord.x <= -1.5 || coord.y <= -1.5 || coord.x >= tex_size.x + 0.5 || coord.y >= tex_size.y + 0.5) {
        return vec4<f32>(0.0);
    }
    let base = vec2<i32>(floor(coord));
    let f = coord - vec2<f32>(base);

    var pr = vec3<f32>(0.0);
    var pa = 0.0;
    if (u.aa_mode < 1.5) {
        let wx0 = 1.0 - f.x;
        let wx1 = f.x;
        let wy0 = 1.0 - f.y;
        let wy1 = f.y;
        let s00 = fetch_transform_texel(base.x, base.y, tex_size_i);
        let s10 = fetch_transform_texel(base.x + 1, base.y, tex_size_i);
        let s01 = fetch_transform_texel(base.x, base.y + 1, tex_size_i);
        let s11 = fetch_transform_texel(base.x + 1, base.y + 1, tex_size_i);
        let w00 = wx0 * wy0;
        let w10 = wx1 * wy0;
        let w01 = wx0 * wy1;
        let w11 = wx1 * wy1;
        pr = s00.rgb * (s00.a * w00) + s10.rgb * (s10.a * w10) + s01.rgb * (s01.a * w01) + s11.rgb * (s11.a * w11);
        pa = s00.a * w00 + s10.a * w10 + s01.a * w01 + s11.a * w11;
    } else {
        for (var j: i32 = -1; j <= 2; j = j + 1) {
            let wy = catmull_rom_weight(f32(j) - f.y);
            for (var i: i32 = -1; i <= 2; i = i + 1) {
                let wx = catmull_rom_weight(f32(i) - f.x);
                let w = wx * wy;
                let s = fetch_transform_texel(base.x + i, base.y + j, tex_size_i);
                pr = pr + s.rgb * (s.a * w);
                pa = pa + s.a * w;
            }
        }
    }
    if (pa <= 1e-6) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(
        clamp(pr / pa, vec3<f32>(0.0), vec3<f32>(1.0)),
        clamp(pa, 0.0, 1.0)
    );
}

fn sample_transform_overlay(doc_px: vec2<f32>, base_texel: vec4<f32>) -> vec4<f32> {
    let local_px = doc_px_to_transform_local(doc_px);
    let layer_s = sample_transform_layer(local_px);
    if (layer_s.a <= 1e-5) {
        return base_texel;
    }
    let out_a = layer_s.a + base_texel.a * (1.0 - layer_s.a);
    if (out_a <= 1e-5) {
        return vec4<f32>(0.0);
    }
    let out_rgb = (layer_s.rgb * layer_s.a + base_texel.rgb * base_texel.a * (1.0 - layer_s.a)) / out_a;
    return vec4<f32>(out_rgb, out_a);
}

fn sample_sel_mask_at(uv: vec2<f32>) -> f32 {
    if (u.transform_active > 0.5) {
        let local_px = doc_px_to_transform_local(uv * u.image_size);
        if (local_px.x < 0.0 || local_px.x >= u.transform_layer_size.x || local_px.y < 0.0 || local_px.y >= u.transform_layer_size.y) {
            return 0.0;
        }
        let src_doc_px = u.transform_layer_offset + local_px;
        if (src_doc_px.x < 0.0 || src_doc_px.x >= u.image_size.x || src_doc_px.y < 0.0 || src_doc_px.y >= u.image_size.y) {
            return 0.0;
        }
        let src_uv = src_doc_px / max(u.image_size, vec2<f32>(1.0));
        return textureSampleLevel(sel_mask_tex, sel_sampler, src_uv, 0.0).r;
    }
    return textureSampleLevel(sel_mask_tex, sel_sampler, uv, 0.0).r;
}

// Stage 1: Sample image content ONLY (AA + bilateral denoise + Free Transform + Color/Light + Curve LUT).
// Selection masks and vector overlays are intentionally isolated in Stage 2.
fn sample_processed_image(
    backdrop: vec3<f32>,
    in_rot_bounds: bool,
    is_dual: bool,
    on_right_page: bool,
    primary_uv: vec2<f32>,
    secondary_uv: vec2<f32>,
    in_uv_x: f32,
) -> vec3<f32> {
    if (!in_rot_bounds) {
        return mix(backdrop, u.preview_fill.rgb, u.preview_fill.a);
    }

    let show_compare = u.has_compare > 0.5 && (
        u.compare_mode > 2.5 ||
        (u.compare_mode > 0.5 && u.compare_mode < 1.5 && in_uv_x < u.compare_split_x)
    );
    if (show_compare) {
        let comp_texel = sample_aa_tex(compare_tex, secondary_uv);
        return mix(backdrop, comp_texel.rgb, comp_texel.a);
    }

    var texel = vec4<f32>(0.0);
    if (is_dual) {
        if (on_right_page) {
            texel = sample_aa_tex(compare_tex, secondary_uv);
        } else {
            texel = sample_denoised_image(primary_uv);
        }
    } else {
        if (u.transform_active > 0.5) {
            var base_texel = vec4<f32>(0.0);
            if (u.transform_has_base > 0.5) {
                base_texel = sample_denoised_image(primary_uv);
            }
            let doc_px = primary_uv * u.image_size;
            texel = sample_transform_overlay(doc_px, base_texel);
        } else {
            texel = sample_denoised_image(primary_uv);
        }
    }

    let adj_rgb = apply_curve_lut(apply_color_and_light(texel.rgb));
    var final_rgb = adj_rgb;
    if (u.adj_selection_only > 0.5 && u.has_selection > 0.5) {
        let sel_w = clamp(sample_sel_mask_at(primary_uv), 0.0, 1.0);
        final_rgb = mix(texel.rgb, adj_rgb, sel_w);
    }
    return mix(backdrop, final_rgb, texel.a);
}

// Stage 2: Apply crisp pixel-exact overlays (divider, crop, quick mask, marching ants via Nearest sel_sampler,
// selection marquee preview, and cursor ring) completely independent of Stage 1 denoise/AA.
fn grad_stop_pos(idx: i32) -> f32 {
    let v = u.gradient_stop_pos[idx / 4];
    let c = idx & 3;
    if (c == 0) { return v.x; }
    if (c == 1) { return v.y; }
    if (c == 2) { return v.z; }
    return v.w;
}

fn sample_grad_ramp(t_in: f32, count: i32, rev: f32) -> vec4<f32> {
    let t = clamp(select(t_in, 1.0 - t_in, rev > 0.5), 0.0, 1.0);
    if (count <= 1) {
        return u.gradient_stop_colors[0];
    }
    let p_first = grad_stop_pos(0);
    if (t <= p_first) {
        return u.gradient_stop_colors[0];
    }
    let last_idx = min(count, 12) - 1;
    let p_last = grad_stop_pos(last_idx);
    if (t >= p_last) {
        return u.gradient_stop_colors[last_idx];
    }
    for (var i: i32 = 0; i < 11; i = i + 1) {
        if (i >= last_idx) { break; }
        let p0 = grad_stop_pos(i);
        let p1 = grad_stop_pos(i + 1);
        if (t >= p0 && t <= p1) {
            let s0 = u.gradient_stop_colors[i];
            let s1 = u.gradient_stop_colors[i + 1];
            let f = clamp((t - p0) / max(p1 - p0, 1e-6), 0.0, 1.0);
            let inv = 1.0 - f;
            let a = s0.a * inv + s1.a * f;
            let rgb = (s0.rgb * s0.a * inv + s1.rgb * s1.a * f) / max(a, 1e-5);
            return vec4<f32>(rgb, a);
        }
    }
    return u.gradient_stop_colors[last_idx];
}

fn apply_grad_blend_ch(sc: f32, dc: f32, kind: i32) -> f32 {
    if (kind == 1) { return dc * sc; }
    if (kind == 2) { return 1.0 - (1.0 - dc) * (1.0 - sc); }
    if (kind == 3) {
        return select(1.0 - 2.0 * (1.0 - dc) * (1.0 - sc), 2.0 * dc * sc, dc < 0.5);
    }
    if (kind == 4) { return min(dc, sc); }
    if (kind == 5) { return max(dc, sc); }
    if (kind == 6) { return abs(dc - sc); }
    return sc;
}

fn apply_crisp_overlays(
    in_rgb: vec3<f32>,
    pixel: vec2<f32>,
    img_uv: vec2<f32>,
    rot_uv: vec2<f32>,
    base_size: vec2<f32>,
    drawn_size: vec2<f32>,
) -> vec3<f32> {
    var out_rgb = in_rgb;

    if (u.gradient_params.x > 0.5) {
        let g0 = u.gradient_line.xy;
        let g1 = u.gradient_line.zw;
        let dxy = g1 - g0;
        let len_sq = dot(dxy, dxy);
        if (len_sq >= 1e-4) {
            let len = sqrt(len_sq);
            let doc_px = rot_uv * base_size - g0;
            let unit = dxy / len;
            let base_angle = atan2(dxy.y, dxy.x);
            let tau = 6.2831853;
            let shape = i32(round(u.gradient_params.y));
            var raw_t = 0.0;
            if (shape == 0) {
                raw_t = dot(doc_px, dxy) / len_sq;
            } else if (shape == 1) {
                raw_t = abs(dot(doc_px, dxy) / len_sq);
            } else if (shape == 2) {
                raw_t = length(doc_px) / len;
            } else if (shape == 3) {
                let rx = abs(doc_px.x * unit.x + doc_px.y * unit.y);
                let ry = abs(-doc_px.x * unit.y + doc_px.y * unit.x);
                raw_t = max(rx, ry) / len;
            } else if (shape == 4) {
                let a = atan2(doc_px.y, doc_px.x) - base_angle;
                raw_t = fract(a / tau);
            } else {
                let dist = length(doc_px) / len;
                let a = fract((atan2(doc_px.y, doc_px.x) - base_angle) / tau);
                raw_t = fract(dist + a);
            }
            let rep = i32(round(u.gradient_params.z));
            var t = clamp(raw_t, 0.0, 1.0);
            if (rep == 1) {
                t = fract(raw_t);
            } else if (rep == 2) {
                let m = raw_t - 2.0 * floor(raw_t * 0.5);
                t = select(2.0 - m, m, m <= 1.0);
            }
            let ramp = sample_grad_ramp(t, i32(round(u.gradient_meta.x)), u.gradient_meta.y);
            var sel_w = 1.0;
            if (u.has_selection > 0.5) {
                sel_w = sample_sel_mask_at(img_uv);
            }
            let sa = clamp(ramp.a * u.gradient_params.w * sel_w, 0.0, 1.0);
            if (sa > 1e-4) {
                let bkind = i32(round(u.gradient_meta.z));
                let blended = vec3<f32>(
                    apply_grad_blend_ch(ramp.r, out_rgb.r, bkind),
                    apply_grad_blend_ch(ramp.g, out_rgb.g, bkind),
                    apply_grad_blend_ch(ramp.b, out_rgb.b, bkind),
                );
                out_rgb = mix(out_rgb, blended, sa);
            }
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
        let has_box = c1.x > c0.x && c1.y > c0.y;
        let inside = has_box
            && img_uv.x >= c0.x
            && img_uv.x <= c1.x
            && img_uv.y >= c0.y
            && img_uv.y <= c1.y;
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

    if (u.has_selection > 0.5 || u.quick_mask_active > 0.5) {
        let sel_m = sample_sel_mask_at(img_uv);
        if (u.quick_mask_active > 0.5) {
            let unmasked = 1.0 - sel_m;
            out_rgb = mix(out_rgb, vec3<f32>(0.92, 0.14, 0.16), unmasked * 0.45);
        }
        if (u.has_selection > 0.5) {
            let step_uv = 1.0 / max(drawn_size, vec2<f32>(1.0));
            let ml = sample_sel_mask_at(img_uv - vec2<f32>(step_uv.x, 0.0));
            let mr = sample_sel_mask_at(img_uv + vec2<f32>(step_uv.x, 0.0));
            let mu = sample_sel_mask_at(img_uv - vec2<f32>(0.0, step_uv.y));
            let md = sample_sel_mask_at(img_uv + vec2<f32>(0.0, step_uv.y));
            let min_n = min(min(ml, mr), min(mu, md));
            let max_n = max(max(ml, mr), max(mu, md));
            if (sel_m >= 0.5 && min_n < 0.5 || sel_m < 0.5 && max_n >= 0.5) {
                let dash = fract((pixel.x + pixel.y) * 0.125 - u.marching_ants_phase);
                out_rgb = select(vec3<f32>(0.08, 0.08, 0.10), vec3<f32>(1.0, 1.0, 1.0), dash < 0.5);
            }
        }
    }

    if (u.sel_preview_kind > 0.5) {
        let s0 = u.sel_preview_rect.xy;
        let s1 = u.sel_preview_rect.zw;
        let px_uv = 1.35 / max(drawn_size, vec2<f32>(1.0));
        var on_sel_edge = false;
        if (u.sel_preview_kind < 1.5) {
            let in_rect = img_uv.x >= s0.x && img_uv.x <= s1.x && img_uv.y >= s0.y && img_uv.y <= s1.y;
            let near_x = abs(img_uv.x - s0.x) < px_uv.x || abs(img_uv.x - s1.x) < px_uv.x;
            let near_y = abs(img_uv.y - s0.y) < px_uv.y || abs(img_uv.y - s1.y) < px_uv.y;
            on_sel_edge = in_rect && (near_x || near_y);
        } else {
            let c = (s0 + s1) * 0.5;
            let r = max((s1 - s0) * 0.5, vec2<f32>(0.0005));
            let d = length((img_uv - c) / r);
            let step_d = max(px_uv.x / r.x, px_uv.y / r.y);
            on_sel_edge = abs(d - 1.0) <= step_d;
        }
        if (on_sel_edge) {
            let dash = fract((pixel.x + pixel.y) * 0.125 - u.marching_ants_phase);
            out_rgb = select(vec3<f32>(0.08, 0.08, 0.10), vec3<f32>(1.0, 1.0, 1.0), dash < 0.5);
        }
    }

    if (u.cursor_radius_px > 0.5 && u.cursor_doc_pos.x > -9000.0) {
        let doc_px = rot_uv * base_size;
        let dist = length(doc_px - u.cursor_doc_pos);
        let px_per_screen = max(base_size.x / max(drawn_size.x, 1.0), 0.25);
        let diff = abs(dist - u.cursor_radius_px);
        if (u.aa_mode > 1.5) {
            let dark_a = 1.0 - smoothstep(px_per_screen * 1.25, px_per_screen * 2.15, diff);
            let white_a = 1.0 - smoothstep(px_per_screen * 0.55, px_per_screen * 1.15, diff);
            out_rgb = mix(out_rgb, vec3<f32>(0.05, 0.05, 0.08), dark_a);
            out_rgb = mix(out_rgb, vec3<f32>(1.0, 1.0, 1.0), white_a);
        } else {
            if (diff <= px_per_screen * 0.9) {
                out_rgb = vec3<f32>(1.0, 1.0, 1.0);
            } else if (diff <= px_per_screen * 1.75) {
                out_rgb = vec3<f32>(0.05, 0.05, 0.08);
            }
        }
    }

    return out_rgb;
}

fn half_vp_uv(pixel: vec2<f32>, img_sz: vec2<f32>, vp_x0: f32, vp_w: f32, vp_h: f32) -> vec3<f32> {
    let img_aspect = img_sz.x / max(img_sz.y, 1.0);
    let hv_aspect = vp_w / max(vp_h, 1.0);
    var fit: vec2<f32>;
    if (img_aspect > hv_aspect) {
        fit = vec2<f32>(vp_w, vp_w / img_aspect);
    } else {
        fit = vec2<f32>(vp_h * img_aspect, vp_h);
    }
    let drawn = fit * u.zoom;
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

    if (u.compare_mode > 1.5 && u.compare_mode < 2.5 && u.has_compare > 0.5) {
        let vp_w = u.viewport_size.x;
        let vp_h = u.viewport_size.y;
        let half_w = vp_w * 0.5;
        let center_px = half_w;
        let sbs_size = select(u.image_size, vec2<f32>(u.image_size.y, u.image_size.x), odd_rot);

        let tile2 = vec2<i32>(floor(pixel / 10.0));
        let odd2 = ((tile2.x + tile2.y) & 1) == 1;
        let ca2 = select(vec3<f32>(0.85, 0.85, 0.85), vec3<f32>(0.18, 0.19, 0.21), u.dark_theme > 0.5);
        let cb2 = select(vec3<f32>(0.75, 0.75, 0.75), vec3<f32>(0.24, 0.25, 0.28), u.dark_theme > 0.5);
        var bd2 = select(ca2, cb2, odd2);
        if (u.has_canvas_bg > 0.5) {
            bd2 = mix(bd2, u.canvas_bg.rgb, u.canvas_bg.a);
        }

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
                let s = sample_aa_tex(compare_tex, sbs_uv);
                out2 = mix(bd2, s.rgb, s.a);
            } else {
                let s = sample_denoised_image(sbs_uv);
                let adj = apply_curve_lut(apply_color_and_light(s.rgb));
                out2 = mix(bd2, adj, s.a);
            }
        }
        return vec4<f32>(out2, 1.0);
    }

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

    if (in_rot_bounds && in_dual_gap) {
        return vec4<f32>(bg, 1.0);
    }

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

    let stage1_rgb = sample_processed_image(
        backdrop,
        in_rot_bounds,
        is_dual,
        on_right_page,
        primary_uv,
        secondary_uv,
        in.uv.x
    );
    let out_rgb = apply_crisp_overlays(stage1_rgb, pixel, img_uv, rot_uv, base_size, drawn_size);
    return vec4<f32>(out_rgb, 1.0);
}
"#;

const OVERLAY_WGSL_SHADER: &str = r#"
struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) aa_uv: vec2<f32>,
    @location(3) aa_kind: f32,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) aa_uv: vec2<f32>,
    @location(2) aa_kind: f32,
};

@vertex
fn vs_overlay(in: VsIn) -> VsOut {
    var out: VsOut;
    out.pos = vec4<f32>(in.pos, 0.0, 1.0);
    out.color = in.color;
    out.aa_uv = in.aa_uv;
    out.aa_kind = in.aa_kind;
    return out;
}

@fragment
fn fs_overlay(in: VsOut) -> @location(0) vec4<f32> {
    var col = in.color;
    if (in.aa_kind > 1.5) {
        let d = length(in.aa_uv);
        let fw = max(fwidth(d), 0.08);
        col.a = col.a * (1.0 - smoothstep(1.0 - fw, 1.0 + fw, d));
    } else if (in.aa_kind > 0.5) {
        let d = abs(in.aa_uv.y);
        let fw = max(fwidth(d), 0.12);
        col.a = col.a * (1.0 - smoothstep(1.0 - fw, 1.0 + fw, d));
    }
    return col;
}
"#;

impl Pipeline for ImageShaderPipeline {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("photon_image_shader"),
            source: wgpu::ShaderSource::Wgsl(WGSL_SHADER.into()),
        });

        let tex_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };

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
                tex_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                tex_entry(3),
                tex_entry(4),
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                tex_entry(6),
                tex_entry(7),
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
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let sel_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("photon_sel_sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let mut identity_lut = [0u8; 1024];
        for i in 0..256 {
            let v = i as u8;
            let base = i * 4;
            identity_lut[base] = v;
            identity_lut[base + 1] = v;
            identity_lut[base + 2] = v;
            identity_lut[base + 3] = 255;
        }
        let (curve_lut_texture, curve_lut_view) =
            create_rgba_texture(device, queue, 256, 1, &identity_lut, "photon_curve_lut");

        let (_, placeholder_view) =
            create_rgba_texture(device, queue, 1, 1, &[0, 0, 0, 0], "photon_placeholder");
        let placeholder_bind_group = build_bind_group(
            device,
            &bind_group_layout,
            &placeholder_uniform_buffer,
            &sampler,
            &sel_sampler,
            &placeholder_view,
            &placeholder_view,
            &placeholder_view,
            &curve_lut_view,
            &placeholder_view,
        );

        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("photon_overlay_shader"),
            source: wgpu::ShaderSource::Wgsl(OVERLAY_WGSL_SHADER.into()),
        });
        let overlay_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("photon_overlay_pl"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let overlay_vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<OverlayVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        };
        let make_overlay_pipeline = |label: &str, topology: wgpu::PrimitiveTopology| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&overlay_pl),
                vertex: wgpu::VertexState {
                    module: &overlay_shader,
                    entry_point: Some("vs_overlay"),
                    buffers: std::slice::from_ref(&overlay_vbl),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &overlay_shader,
                    entry_point: Some("fs_overlay"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                multiview: None,
                cache: None,
            })
        };
        let overlay_line_pipeline = make_overlay_pipeline(
            "photon_overlay_line_pipeline",
            wgpu::PrimitiveTopology::LineList,
        );
        let overlay_tri_pipeline = make_overlay_pipeline(
            "photon_overlay_tri_pipeline",
            wgpu::PrimitiveTopology::TriangleList,
        );
        let initial_cap = 256usize;
        let overlay_line_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("photon_overlay_line_vb"),
            size: (initial_cap * std::mem::size_of::<OverlayVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let overlay_tri_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("photon_overlay_tri_vb"),
            size: (initial_cap * std::mem::size_of::<OverlayVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            overlay_line_pipeline,
            overlay_tri_pipeline,
            overlay_line_buffer,
            overlay_line_capacity: initial_cap,
            overlay_line_count: 0,
            overlay_tri_buffer,
            overlay_tri_capacity: initial_cap,
            overlay_tri_count: 0,
            placeholder_uniform_buffer,
            bind_group_layout,
            sampler,
            sel_sampler,
            curve_lut_texture,
            curve_lut_view,
            last_curve_lut: Some(identity_lut),
            placeholder_view,
            placeholder_bind_group,
            textures: HashMap::new(),
            compare_textures: HashMap::new(),
            sel_mask_textures: HashMap::new(),
            transform_base_textures: HashMap::new(),
            transform_layer_textures: HashMap::new(),
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
        if self.sel_mask_textures.len() > 8 {
            self.sel_mask_textures.clear();
        }
        if self.transform_base_textures.len() > 4 {
            self.transform_base_textures.clear();
        }
        if self.transform_layer_textures.len() > 4 {
            self.transform_layer_textures.clear();
        }
    }
}

fn simple_frame_key(frame: &CpuImageFrame) -> SimpleFrameKey {
    (
        frame.key.clone(),
        frame.revision,
        Arc::as_ptr(&frame.rgba) as usize,
        frame.width,
        frame.height,
    )
}

fn ensure_simple_rgba_texture(
    map: &mut HashMap<String, CachedSimpleTexture>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: &CpuImageFrame,
    max_dim: u32,
) {
    let rgba_ptr = Arc::as_ptr(&frame.rgba) as usize;
    let needs = map.get(&frame.key).is_none_or(|t| {
        t.revision != frame.revision
            || t.width != frame.width
            || t.height != frame.height
            || t.rgba_ptr != rgba_ptr
    });
    if needs {
        let (w, h, bytes) = fit_within_gpu_limit(frame.width, frame.height, &frame.rgba, max_dim);
        let (_, view) = create_rgba_texture(device, queue, w, h, &bytes, "photon_simple_texture");
        map.insert(
            frame.key.clone(),
            CachedSimpleTexture {
                revision: frame.revision,
                width: frame.width,
                height: frame.height,
                rgba_ptr,
                view,
            },
        );
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
        let max_dim = device
            .limits()
            .max_texture_dimension_2d
            .min(MAX_GPU_TEXTURE_DIM);
        let mut has_image = 0.0f32;
        let mut img_size = [1.0f32, 1.0f32];
        let mut has_compare = 0.0f32;
        let mut has_pair = 0.0f32;
        let mut pair_size = [1.0f32, 1.0f32];

        // Upload curve LUT into persistent 256x1 texture when changed.
        let has_curve_lut = if let Some(lut) = self.curve_lut {
            if pipeline.last_curve_lut != Some(lut) {
                let size = wgpu::Extent3d {
                    width: 256,
                    height: 1,
                    depth_or_array_layers: 1,
                };
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &pipeline.curve_lut_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &lut,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256 * 4),
                        rows_per_image: Some(1),
                    },
                    size,
                );
                pipeline.last_curve_lut = Some(lut);
            }
            1.0
        } else {
            0.0
        };

        let secondary_ref = if self.compare_mode > 0 {
            self.compare_frame.as_ref()
        } else {
            self.pair_frame.as_ref()
        };

        let secondary_key_rev = secondary_ref.map(simple_frame_key);
        if let Some(sec) = secondary_ref {
            ensure_simple_rgba_texture(&mut pipeline.compare_textures, device, queue, sec, max_dim);
            if self.compare_mode > 0 {
                has_compare = 1.0;
            } else if pipeline.compare_textures.contains_key(&sec.key) {
                has_pair = 1.0;
                // Preserve true document dimensions for pair frame so overlays/spread math stay aligned.
                pair_size = [sec.width.max(1) as f32, sec.height.max(1) as f32];
            }
        }

        let secondary_view = secondary_ref
            .and_then(|c| pipeline.compare_textures.get(&c.key))
            .map(|t| &t.view)
            .unwrap_or(&pipeline.placeholder_view);

        let sel_key_rev = self.selection_mask.as_ref().map(|m| {
            (
                m.key.clone(),
                m.revision,
                Arc::as_ptr(&m.alpha) as usize,
                m.width,
                m.height,
            )
        });
        if let Some(sel) = &self.selection_mask {
            let alpha_ptr = Arc::as_ptr(&sel.alpha) as usize;
            let needs_sel = pipeline.sel_mask_textures.get(&sel.key).is_none_or(|t| {
                t.revision != sel.revision
                    || t.width != sel.width
                    || t.height != sel.height
                    || t.rgba_ptr != alpha_ptr
            });
            if needs_sel {
                let mut rgba = Vec::with_capacity(sel.alpha.len() * 4);
                for &a in sel.alpha.iter() {
                    rgba.extend_from_slice(&[a, a, a, 255]);
                }
                let (w, h, bytes) = fit_within_gpu_limit(sel.width, sel.height, &rgba, max_dim);
                let (_, view) =
                    create_rgba_texture(device, queue, w, h, &bytes, "photon_sel_mask_texture");
                pipeline.sel_mask_textures.insert(
                    sel.key.clone(),
                    CachedSimpleTexture {
                        revision: sel.revision,
                        width: sel.width,
                        height: sel.height,
                        rgba_ptr: alpha_ptr,
                        view,
                    },
                );
            }
        }
        let sel_view = self
            .selection_mask
            .as_ref()
            .and_then(|m| pipeline.sel_mask_textures.get(&m.key))
            .map(|t| &t.view)
            .unwrap_or(&pipeline.placeholder_view);

        // Upload Free Transform base & layer textures if active.
        let mut transform_active = 0.0f32;
        let mut transform_has_base = 0.0f32;
        let mut transform_layer_offset = [0.0f32; 2];
        let mut transform_layer_size = [1.0f32; 2];
        let mut transform_translate = [0.0f32; 2];
        let mut transform_scale = [1.0f32; 2];
        let mut transform_pivot = [0.5f32; 2];
        let mut transform_cos_sin = [1.0f32, 0.0f32];
        let mut base_override_key_rev: Option<SimpleFrameKey> = None;
        let mut transform_layer_key_rev: Option<SimpleFrameKey> = None;

        if let Some(tp) = &self.transform_preview {
            transform_active = 1.0;
            ensure_simple_rgba_texture(
                &mut pipeline.transform_layer_textures,
                device,
                queue,
                &tp.layer_frame,
                max_dim,
            );
            transform_layer_key_rev = Some(simple_frame_key(&tp.layer_frame));
            if let Some(base) = &tp.base_frame {
                transform_has_base = 1.0;
                ensure_simple_rgba_texture(
                    &mut pipeline.transform_base_textures,
                    device,
                    queue,
                    base,
                    max_dim,
                );
                base_override_key_rev = Some(simple_frame_key(base));
            }
            transform_layer_offset = [tp.layer_offset[0] as f32, tp.layer_offset[1] as f32];
            transform_layer_size = [
                tp.layer_size[0].max(1) as f32,
                tp.layer_size[1].max(1) as f32,
            ];
            transform_translate = [tp.spec.translate_x, tp.spec.translate_y];
            let sx = tp.spec.scale_x * if tp.spec.flip_h { -1.0 } else { 1.0 };
            let sy = tp.spec.scale_y * if tp.spec.flip_v { -1.0 } else { 1.0 };
            transform_scale = [sx, sy];
            transform_pivot = [tp.spec.pivot_x, tp.spec.pivot_y];
            let theta = tp.spec.rotation_deg.to_radians();
            transform_cos_sin = [theta.cos(), theta.sin()];
        }

        let transform_layer_view = self
            .transform_preview
            .as_ref()
            .and_then(|tp| pipeline.transform_layer_textures.get(&tp.layer_frame.key))
            .map(|t| &t.view)
            .unwrap_or(&pipeline.placeholder_view);

        let base_override_view = self
            .transform_preview
            .as_ref()
            .and_then(|tp| tp.base_frame.as_ref())
            .and_then(|b| pipeline.transform_base_textures.get(&b.key))
            .map(|t| &t.view);

        if let Some(frame) = &self.frame {
            let (target_w, target_h) = if frame.width > max_dim || frame.height > max_dim {
                let scale = max_dim as f32 / frame.width.max(frame.height).max(1) as f32;
                (
                    ((frame.width as f32 * scale).round() as u32).max(1),
                    ((frame.height as f32 * scale).round() as u32).max(1),
                )
            } else {
                (frame.width.max(1), frame.height.max(1))
            };
            let needs_upload = pipeline.textures.get(&frame.key).is_none_or(|tex| {
                tex.revision != frame.revision || tex.width != target_w || tex.height != target_h
            });

            if needs_upload {
                let (w, h, bytes) =
                    fit_within_gpu_limit(frame.width, frame.height, &frame.rgba, max_dim);
                let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("photon_image_uniforms"),
                    size: std::mem::size_of::<Uniforms>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let (_, image_view) =
                    create_rgba_texture(device, queue, w, h, &bytes, "photon_image_texture");
                let active_base_view = base_override_view.unwrap_or(&image_view);
                let bind_group = build_bind_group(
                    device,
                    &pipeline.bind_group_layout,
                    &uniform_buffer,
                    &pipeline.sampler,
                    &pipeline.sel_sampler,
                    active_base_view,
                    secondary_view,
                    sel_view,
                    &pipeline.curve_lut_view,
                    transform_layer_view,
                );
                pipeline.textures.insert(
                    frame.key.clone(),
                    CachedGpuTexture {
                        revision: frame.revision,
                        width: w,
                        height: h,
                        image_view,
                        bound_base: base_override_key_rev.clone(),
                        bound_compare: secondary_key_rev.clone(),
                        bound_sel: sel_key_rev.clone(),
                        bound_transform_layer: transform_layer_key_rev.clone(),
                        uniform_buffer,
                        bind_group,
                    },
                );
            } else if let Some(tex) = pipeline.textures.get_mut(&frame.key)
                && (tex.bound_base != base_override_key_rev
                    || tex.bound_compare != secondary_key_rev
                    || tex.bound_sel != sel_key_rev
                    || tex.bound_transform_layer != transform_layer_key_rev)
            {
                let active_base_view = base_override_view.unwrap_or(&tex.image_view);
                tex.bind_group = build_bind_group(
                    device,
                    &pipeline.bind_group_layout,
                    &tex.uniform_buffer,
                    &pipeline.sampler,
                    &pipeline.sel_sampler,
                    active_base_view,
                    secondary_view,
                    sel_view,
                    &pipeline.curve_lut_view,
                    transform_layer_view,
                );
                tex.bound_base = base_override_key_rev;
                tex.bound_compare = secondary_key_rev;
                tex.bound_sel = sel_key_rev;
                tex.bound_transform_layer = transform_layer_key_rev;
            }

            if pipeline.textures.contains_key(&frame.key) {
                has_image = 1.0;
                // Always keep true document dimensions (`frame.width`, `frame.height`) in `Uniforms.image_size`
                // so cursor rings, selection masks, and overlays never desync when `> max_dim` is downsampled.
                img_size = [frame.width.max(1) as f32, frame.height.max(1) as f32];
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

        let aa_mode = match self.aa_mode {
            CanvasAntiAliasingMode::Crisp => 0.0,
            CanvasAntiAliasingMode::Smooth => 1.0,
            CanvasAntiAliasingMode::HighQuality => 2.0,
        };
        let denoise_mode = match self.denoise_mode {
            ViewportDenoiseMode::Off => 0.0,
            ViewportDenoiseMode::Light => 1.0,
            ViewportDenoiseMode::Medium => 2.0,
            ViewportDenoiseMode::Strong => 3.0,
        };

        let (
            gradient_line,
            gradient_params,
            gradient_meta,
            gradient_stop_colors,
            gradient_stop_pos,
        ) = if let Some(gp) = &self.gradient_preview
            && let (Some(sa), Some(sb)) = (gp.start, gp.end)
        {
            let dx = sb[0] - sa[0];
            let dy = sb[1] - sa[1];
            let active = if (dx * dx + dy * dy) >= 1.0 { 1.0 } else { 0.0 };
            let shape_idx = match gp.kind {
                GradientKind::Linear => 0.0,
                GradientKind::Bilinear | GradientKind::Reflected => 1.0,
                GradientKind::Radial => 2.0,
                GradientKind::Square => 3.0,
                GradientKind::Conical => 4.0,
                GradientKind::Spiral => 5.0,
            };
            let rep_idx = match gp.repeat {
                GradientRepeat::None => 0.0,
                GradientRepeat::Forwards => 1.0,
                GradientRepeat::Alternating => 2.0,
            };
            let blend_idx = match gp.blend_mode {
                BlendMode::Normal => 0.0,
                BlendMode::Multiply => 1.0,
                BlendMode::Screen => 2.0,
                BlendMode::Overlay => 3.0,
                BlendMode::Darken => 4.0,
                BlendMode::Lighten => 5.0,
                BlendMode::Difference => 6.0,
            };
            let mut sorted = gp.stops.clone();
            sorted.sort_by(|a, b| a.pos.total_cmp(&b.pos));
            if sorted.is_empty() {
                sorted.push(photon_protocol::GradientStop {
                    pos: 0.0,
                    color: [0, 0, 0],
                    alpha: 1.0,
                });
            }
            let count = sorted.len().min(12);
            let mut cols = [[0.0_f32; 4]; 12];
            let mut pos_vecs = [[0.0_f32; 4]; 3];
            for (idx, s) in sorted.iter().take(12).enumerate() {
                cols[idx] = [
                    f32::from(s.color[0]) / 255.0,
                    f32::from(s.color[1]) / 255.0,
                    f32::from(s.color[2]) / 255.0,
                    (s.alpha as f32).clamp(0.0, 1.0),
                ];
                pos_vecs[idx / 4][idx % 4] = (s.pos as f32).clamp(0.0, 1.0);
            }
            (
                [sa[0], sa[1], sb[0], sb[1]],
                [active, shape_idx, rep_idx, gp.opacity.clamp(0.0, 1.0)],
                [
                    count as f32,
                    if gp.reverse { 1.0 } else { 0.0 },
                    blend_idx,
                    0.0,
                ],
                cols,
                pos_vecs,
            )
        } else {
            ([0.0; 4], [0.0; 4], [0.0; 4], [[0.0; 4]; 12], [[0.0; 4]; 3])
        };

        let uniforms = Uniforms {
            viewport_size: self.bounds_size,
            image_size: img_size,
            pan: [self.pan_x, self.pan_y],
            zoom: self.zoom.max(0.02),
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
            has_selection: if self.has_selection { 1.0 } else { 0.0 },
            quick_mask_active: if self.quick_mask_active { 1.0 } else { 0.0 },
            marching_ants_phase: self.marching_ants_phase,
            sel_preview_kind: self.sel_preview_kind,
            sel_preview_rect: self.sel_preview_rect,
            has_curve_lut,
            aa_mode,
            denoise_mode,
            transform_active,
            transform_layer_offset,
            transform_layer_size,
            transform_translate,
            transform_scale,
            transform_pivot,
            transform_cos_sin,
            transform_has_base,
            adj_selection_only: if self.adj_selection_only { 1.0 } else { 0.0 },
            _pad3: [0.0; 2],
            gradient_line,
            gradient_params,
            gradient_meta,
            gradient_stop_colors,
            gradient_stop_pos,
        };

        let target_buf = self
            .frame
            .as_ref()
            .and_then(|f| pipeline.textures.get(&f.key))
            .map(|t| &t.uniform_buffer)
            .unwrap_or(&pipeline.placeholder_uniform_buffer);
        queue.write_buffer(target_buf, 0, uniforms.as_bytes());

        pipeline.overlay_line_count = upload_overlay_vertices(
            device,
            queue,
            &mut pipeline.overlay_line_buffer,
            &mut pipeline.overlay_line_capacity,
            &self.overlay_lines,
            "photon_overlay_line_vb",
        );
        pipeline.overlay_tri_count = upload_overlay_vertices(
            device,
            queue,
            &mut pipeline.overlay_tri_buffer,
            &mut pipeline.overlay_tri_capacity,
            &self.overlay_tris,
            "photon_overlay_tri_vb",
        );
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

        if pipeline.overlay_line_count > 0 {
            render_pass.set_pipeline(&pipeline.overlay_line_pipeline);
            render_pass.set_vertex_buffer(0, pipeline.overlay_line_buffer.slice(..));
            render_pass.draw(0..pipeline.overlay_line_count, 0..1);
        }
        if pipeline.overlay_tri_count > 0 {
            render_pass.set_pipeline(&pipeline.overlay_tri_pipeline);
            render_pass.set_vertex_buffer(0, pipeline.overlay_tri_buffer.slice(..));
            render_pass.draw(0..pipeline.overlay_tri_count, 0..1);
        }
        true
    }
}

fn upload_overlay_vertices(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &mut wgpu::Buffer,
    capacity: &mut usize,
    vertices: &[OverlayVertex],
    label: &str,
) -> u32 {
    if vertices.is_empty() {
        return 0;
    }
    if vertices.len() > *capacity {
        let new_cap = vertices.len().next_power_of_two().max(64);
        *buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (new_cap * std::mem::size_of::<OverlayVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        *capacity = new_cap;
    }
    // Safety: OverlayVertex is #[repr(C)] containing only plain f32 values.
    let bytes = unsafe {
        std::slice::from_raw_parts(
            vertices.as_ptr().cast::<u8>(),
            std::mem::size_of_val(vertices),
        )
    };
    queue.write_buffer(buffer, 0, bytes);
    vertices.len() as u32
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

fn create_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    rgba: &[u8],
    label: &str,
) -> (wgpu::Texture, wgpu::TextureView) {
    let size = wgpu::Extent3d {
        width: width.max(1),
        height: height.max(1),
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
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
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

#[allow(clippy::too_many_arguments)]
fn build_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    sel_sampler: &wgpu::Sampler,
    image_view: &wgpu::TextureView,
    compare_view: &wgpu::TextureView,
    sel_mask_view: &wgpu::TextureView,
    curve_lut_view: &wgpu::TextureView,
    transform_layer_view: &wgpu::TextureView,
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
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(sel_mask_view),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(sel_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(curve_lut_view),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(transform_layer_view),
            },
        ],
    })
}
