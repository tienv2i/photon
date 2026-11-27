//! Lightweight WGSL shader and `wgpu` rendering pipeline for `ViewerCanvasProgram`.
//!
//! Designed strictly for fast image viewing (single page & dual-page spread) without
//! Editor layer, brush, selection, gradient, or vector overlay pipelines.

use std::collections::HashMap;

use iced::Rectangle;
use iced::wgpu;
use iced::widget::shader::{Pipeline, Primitive, Viewport};
use photon_protocol::{CanvasAntiAliasingMode, ColorAdjustments, ViewportDenoiseMode};

use crate::{ViewerImageFrame, dual_spread_geometry};

pub const MAX_GPU_TEXTURE_DIM: u32 = 16384;

#[derive(Debug, Clone)]
pub struct ViewerQuadPrimitive {
    pub frame: Option<ViewerImageFrame>,
    pub pair_frame: Option<ViewerImageFrame>,
    pub dual_gap_px: f32,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub adjustments: ColorAdjustments,
    pub dark_theme: bool,
    pub quarter_turns: u8,
    pub viewport_bg: Option<[u8; 3]>,
    pub aa_mode: CanvasAntiAliasingMode,
    pub denoise_mode: ViewportDenoiseMode,
    pub marquee_rect: Option<[f32; 4]>,
    pub bounds_size: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    viewport_size: [f32; 2],
    image_size: [f32; 2],
    pair_size: [f32; 2],
    pan: [f32; 2],
    zoom: f32,
    quarter_turns: f32,
    has_image: f32,
    has_pair: f32,
    dual_gap_px: f32,
    dark_theme: f32,
    has_viewport_bg: f32,
    aa_mode: f32,
    viewport_bg: [f32; 4],
    brightness: f32,
    contrast: f32,
    saturation: f32,
    exposure: f32,
    highlights: f32,
    shadows: f32,
    gamma: f32,
    denoise_mode: f32,
    marquee_rect: [f32; 4],
}

const WGSL_SHADER: &str = r#"
struct Uniforms {
    viewport_size: vec2<f32>,
    image_size: vec2<f32>,
    pair_size: vec2<f32>,
    pan: vec2<f32>,
    zoom: f32,
    quarter_turns: f32,
    has_image: f32,
    has_pair: f32,
    dual_gap_px: f32,
    dark_theme: f32,
    has_viewport_bg: f32,
    aa_mode: f32,
    viewport_bg: vec4<f32>,
    brightness: f32,
    contrast: f32,
    saturation: f32,
    exposure: f32,
    highlights: f32,
    shadows: f32,
    gamma: f32,
    denoise_mode: f32,
    marquee_rect: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var img_tex: texture_2d<f32>;
@group(0) @binding(2) var img_sampler: sampler;
@group(0) @binding(3) var pair_tex: texture_2d<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local_px: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let p = corners[vid];
    let ndc = vec2<f32>(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0);
    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.local_px = p * u.viewport_size;
    return out;
}

fn backdrop_color() -> vec4<f32> {
    if (u.has_viewport_bg > 0.5) {
        return u.viewport_bg;
    }
    if (u.dark_theme > 0.5) {
        return vec4<f32>(0.09, 0.095, 0.11, 1.0);
    }
    return vec4<f32>(0.93, 0.935, 0.945, 1.0);
}

fn checker_color(doc_px: vec2<f32>) -> vec3<f32> {
    let tile = floor(doc_px / 12.0);
    let parity = (i32(tile.x) + i32(tile.y)) & 1;
    if (u.dark_theme > 0.5) {
        return select(vec3<f32>(0.18, 0.19, 0.21), vec3<f32>(0.24, 0.25, 0.28), parity == 0);
    }
    return select(vec3<f32>(0.86, 0.87, 0.89), vec3<f32>(0.95, 0.955, 0.965), parity == 0);
}

fn rotate_uv(uv: vec2<f32>, turns: i32) -> vec2<f32> {
    if (turns == 1) { return vec2<f32>(uv.y, 1.0 - uv.x); }
    if (turns == 2) { return vec2<f32>(1.0 - uv.x, 1.0 - uv.y); }
    if (turns == 3) { return vec2<f32>(1.0 - uv.y, uv.x); }
    return uv;
}

fn sample_tex(tex: texture_2d<f32>, uv: vec2<f32>, tex_size: vec2<f32>) -> vec4<f32> {
    if (u.denoise_mode > 0.5) {
        let px = 1.0 / max(tex_size, vec2<f32>(1.0, 1.0));
        let c = textureSample(tex, img_sampler, uv);
        let n1 = textureSample(tex, img_sampler, uv + vec2<f32>(px.x, 0.0));
        let n2 = textureSample(tex, img_sampler, uv - vec2<f32>(px.x, 0.0));
        let n3 = textureSample(tex, img_sampler, uv + vec2<f32>(0.0, px.y));
        let n4 = textureSample(tex, img_sampler, uv - vec2<f32>(0.0, px.y));
        let avg = (c + n1 + n2 + n3 + n4) * 0.2;
        let strength = select(0.35, 0.65, u.denoise_mode > 1.5);
        return mix(c, avg, strength);
    }
    if (u.aa_mode > 0.5) {
        let dims = max(tex_size, vec2<f32>(1.0, 1.0));
        let tc = uv * dims - vec2<f32>(0.5);
        let f = fract(tc);
        let smooth_f = f * f * (3.0 - 2.0 * f);
        let smooth_uv = (floor(tc) + smooth_f + vec2<f32>(0.5)) / dims;
        return textureSample(tex, img_sampler, smooth_uv);
    }
    return textureSample(tex, img_sampler, uv);
}

fn apply_adjustments(rgb_in: vec3<f32>) -> vec3<f32> {
    var rgb = rgb_in;
    if (abs(u.exposure) > 0.001) {
        rgb = rgb * pow(2.0, u.exposure);
    }
    if (abs(u.brightness) > 0.001) {
        rgb = rgb + vec3<f32>(u.brightness);
    }
    if (abs(u.contrast - 1.0) > 0.001) {
        rgb = (rgb - vec3<f32>(0.5)) * u.contrast + vec3<f32>(0.5);
    }
    if (abs(u.gamma - 1.0) > 0.001) {
        rgb = pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(1.0 / max(u.gamma, 0.1)));
    }
    if (abs(u.shadows) > 0.001 || abs(u.highlights) > 0.001) {
        let luma = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        let shadow_w = clamp(1.0 - luma * 2.0, 0.0, 1.0);
        let high_w = clamp((luma - 0.5) * 2.0, 0.0, 1.0);
        rgb = rgb + vec3<f32>(u.shadows * 0.25 * shadow_w + u.highlights * 0.25 * high_w);
    }
    if (abs(u.saturation - 1.0) > 0.001) {
        let luma = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        rgb = mix(vec3<f32>(luma), rgb, u.saturation);
    }
    return clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let bg = backdrop_color();
    if (u.has_image < 0.5) {
        return bg;
    }

    let turns = i32(u.quarter_turns + 0.5) & 3;
    var doc_size = u.image_size;
    var left_w = u.image_size.x;
    var gap = 0.0;
    if (u.has_pair > 0.5) {
        let target_h = max(u.image_size.y, u.pair_size.y);
        left_w = round(u.image_size.x * (target_h / max(u.image_size.y, 1.0)));
        let right_w = round(u.pair_size.x * (target_h / max(u.pair_size.y, 1.0)));
        if (u.dual_gap_px > 0.5) {
            gap = u.dual_gap_px * max(target_h / 900.0, 1.0);
        }
        doc_size = vec2<f32>(left_w + gap + right_w, target_h);
    }

    let rot_size = select(doc_size, vec2<f32>(doc_size.y, doc_size.x), (turns & 1) == 1);
    let fit = min(u.viewport_size.x / max(rot_size.x, 1.0), u.viewport_size.y / max(rot_size.y, 1.0));
    let scale = max(fit * u.zoom, 0.0001);
    let disp_size = rot_size * scale;
    let origin = (u.viewport_size - disp_size) * 0.5 + u.pan;
    let uv_rot = (in.local_px - origin) / disp_size;

    var color = bg;
    if (uv_rot.x >= 0.0 && uv_rot.x <= 1.0 && uv_rot.y >= 0.0 && uv_rot.y <= 1.0) {
        let doc_uv = rotate_uv(uv_rot, turns);
        let doc_px = doc_uv * doc_size;
        var sampled = vec4<f32>(0.0);
        var in_page = true;

        if (u.has_pair > 0.5) {
            if (doc_px.x <= left_w) {
                let puv = vec2<f32>(doc_px.x / max(left_w, 1.0), doc_px.y / max(doc_size.y, 1.0));
                sampled = sample_tex(img_tex, puv, u.image_size);
            } else if (doc_px.x < left_w + gap) {
                in_page = false;
            } else {
                let right_w = max(doc_size.x - left_w - gap, 1.0);
                let puv = vec2<f32>((doc_px.x - left_w - gap) / right_w, doc_px.y / max(doc_size.y, 1.0));
                sampled = sample_tex(pair_tex, puv, u.pair_size);
            }
        } else {
            sampled = sample_tex(img_tex, doc_uv, u.image_size);
        }

        if (in_page) {
            let adj = apply_adjustments(sampled.rgb);
            let chk = checker_color(doc_px);
            color = vec4<f32>(mix(chk, adj, sampled.a), 1.0);
        }
    }

    if (u.marquee_rect.z > 0.5 && u.marquee_rect.w > 0.5) {
        let r0 = u.marquee_rect.xy;
        let r1 = r0 + u.marquee_rect.zw;
        let p = in.local_px;
        if (p.x >= r0.x && p.x <= r1.x && p.y >= r0.y && p.y <= r1.y) {
            let border = p.x < r0.x + 1.5 || p.x > r1.x - 1.5 || p.y < r0.y + 1.5 || p.y > r1.y - 1.5;
            if (border) {
                color = vec4<f32>(0.28, 0.58, 0.98, 1.0);
            } else {
                color = mix(color, vec4<f32>(0.28, 0.58, 0.98, 1.0), 0.16);
            }
        }
    }

    return color;
}
"#;

#[derive(Debug)]
struct CachedTexture {
    revision: u64,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: (u32, u32),
}

pub struct ViewerShaderPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
    dummy_view: wgpu::TextureView,
    textures: HashMap<String, CachedTexture>,
    bind_group: Option<wgpu::BindGroup>,
}

fn upload_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    rgba: &[u8],
    label: &str,
) -> (wgpu::Texture, wgpu::TextureView) {
    let w = width.clamp(1, MAX_GPU_TEXTURE_DIM);
    let h = height.clamp(1, MAX_GPU_TEXTURE_DIM);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    if rgba.len() >= (w as usize) * (h as usize) * 4 {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba[..(w as usize) * (h as usize) * 4],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

impl Pipeline for ViewerShaderPipeline {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("photon_viewer_shader"),
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
            label: Some("photon_viewer_bgl"),
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
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("photon_viewer_pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("photon_viewer_pipeline"),
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

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("photon_viewer_uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("photon_viewer_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let (_, dummy_view) =
            upload_rgba_texture(device, queue, 1, 1, &[0, 0, 0, 0], "photon_viewer_dummy");

        Self {
            pipeline,
            bind_group_layout,
            uniform_buffer,
            sampler,
            dummy_view,
            textures: HashMap::new(),
            bind_group: None,
        }
    }
}

impl ViewerShaderPipeline {
    fn ensure_frame_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &ViewerImageFrame,
    ) {
        let needs_upload = self
            .textures
            .get(&frame.key)
            .is_none_or(|c| c.revision != frame.revision || c.size != (frame.width, frame.height));
        if needs_upload {
            if self.textures.len() >= 4 {
                self.textures.clear();
            }
            let (tex, view) = upload_rgba_texture(
                device,
                queue,
                frame.width,
                frame.height,
                &frame.rgba,
                "photon_viewer_tex",
            );
            self.textures.insert(
                frame.key.clone(),
                CachedTexture {
                    revision: frame.revision,
                    _texture: tex,
                    view,
                    size: (frame.width, frame.height),
                },
            );
        }
    }
}

impl Primitive for ViewerQuadPrimitive {
    type Pipeline = ViewerShaderPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        if let Some(frame) = &self.frame {
            pipeline.ensure_frame_texture(device, queue, frame);
        }
        if let Some(pair) = &self.pair_frame {
            pipeline.ensure_frame_texture(device, queue, pair);
        }

        let (has_image, image_size) = if let Some(frame) = &self.frame {
            (1.0, [frame.width.max(1) as f32, frame.height.max(1) as f32])
        } else {
            (0.0, [1.0, 1.0])
        };

        let (has_pair, pair_size) = if let Some(pair) = &self.pair_frame {
            (1.0, [pair.width.max(1) as f32, pair.height.max(1) as f32])
        } else {
            (0.0, [1.0, 1.0])
        };

        let (has_viewport_bg, viewport_bg) = if let Some([r, g, b]) = self.viewport_bg {
            (
                1.0,
                [
                    f32::from(r) / 255.0,
                    f32::from(g) / 255.0,
                    f32::from(b) / 255.0,
                    1.0,
                ],
            )
        } else {
            (0.0, [0.0; 4])
        };

        let aa_mode = match self.aa_mode {
            CanvasAntiAliasingMode::Crisp => 0.0,
            CanvasAntiAliasingMode::Smooth | CanvasAntiAliasingMode::HighQuality => 1.0,
        };
        let denoise_mode = match self.denoise_mode {
            ViewportDenoiseMode::Off => 0.0,
            ViewportDenoiseMode::Light | ViewportDenoiseMode::Medium => 1.0,
            ViewportDenoiseMode::Strong => 2.0,
        };

        let _ = dual_spread_geometry;
        let uniforms = Uniforms {
            viewport_size: [self.bounds_size[0].max(1.0), self.bounds_size[1].max(1.0)],
            image_size,
            pair_size,
            pan: [self.pan_x, self.pan_y],
            zoom: self.zoom.max(0.02),
            quarter_turns: f32::from(self.quarter_turns % 4),
            has_image,
            has_pair,
            dual_gap_px: self.dual_gap_px.max(0.0),
            dark_theme: if self.dark_theme { 1.0 } else { 0.0 },
            has_viewport_bg,
            aa_mode,
            viewport_bg,
            brightness: self.adjustments.brightness,
            contrast: self.adjustments.contrast,
            saturation: self.adjustments.saturation,
            exposure: self.adjustments.exposure,
            highlights: self.adjustments.highlights,
            shadows: self.adjustments.shadows,
            gamma: self.adjustments.gamma.clamp(0.2, 3.0),
            denoise_mode,
            marquee_rect: self.marquee_rect.unwrap_or([0.0; 4]),
        };

        let bytes = unsafe {
            std::slice::from_raw_parts(
                (&uniforms as *const Uniforms).cast::<u8>(),
                std::mem::size_of::<Uniforms>(),
            )
        };
        queue.write_buffer(&pipeline.uniform_buffer, 0, bytes);

        let primary_view = self
            .frame
            .as_ref()
            .and_then(|f| pipeline.textures.get(&f.key))
            .map(|c| &c.view)
            .unwrap_or(&pipeline.dummy_view);
        let pair_view = self
            .pair_frame
            .as_ref()
            .and_then(|f| pipeline.textures.get(&f.key))
            .map(|c| &c.view)
            .unwrap_or(&pipeline.dummy_view);
        pipeline.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("photon_viewer_bg"),
            layout: &pipeline.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: pipeline.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(primary_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(pair_view),
                },
            ],
        }));
    }

    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let Some(bind_group) = pipeline.bind_group.as_ref() else {
            return;
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("photon_viewer_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width.max(1),
            clip_bounds.height.max(1),
        );
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}
