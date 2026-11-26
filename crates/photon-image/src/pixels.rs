//! Parallel CPU raster arithmetic: color & light, curves, HSV, color balance,
//! geometry (resize modes, grow crop, canvas size, arbitrary-angle rotate), and brushes.

use image::imageops::{self, FilterType};
use image::{Rgba, RgbaImage};
use photon_protocol::{
    BrushSettings, BrushTip, CanvasAnchor, ColorAdjustments, ColorBalanceAdjustment,
    CurvesAdjustment, HsvAdjustment, LayerTransformSpec, ResizeFilter, ResizeMode,
};
use rayon::prelude::*;

use crate::selection::SelectionMask;

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// CPU parallel Color & Light adjustment matching the WGSL preview shader math:
/// (1) exposure, (2) gamma, (3) highlights & shadows, (4) contrast, (5) brightness,
/// (6) saturation, (7) clamp `[0, 1]`, preserving alpha.
pub fn adjust_color_and_light(src: &RgbaImage, adj: &ColorAdjustments) -> RgbaImage {
    let (w, h) = src.dimensions();
    let mut dst = vec![0u8; (w as usize) * (h as usize) * 4];
    let exposure_mul = 2.0_f32.powf(adj.exposure.clamp(-2.0, 2.0));
    let gamma = adj.gamma.clamp(0.2, 3.0);
    let inv_gamma = 1.0 / gamma;
    let apply_gamma = (gamma - 1.0).abs() > 1e-4;
    let shadows = adj.shadows.clamp(-1.0, 1.0);
    let highlights = adj.highlights.clamp(-1.0, 1.0);
    let c = adj.contrast.clamp(-1.0, 1.0);
    let contrast = if c >= 0.0 {
        1.0 / (1.0 - c * 0.999)
    } else {
        1.0 + c
    };
    let brightness = adj.brightness.clamp(-1.0, 1.0);
    let saturation = (1.0 + adj.saturation.clamp(-1.0, 1.0)).max(0.0);

    dst.par_chunks_exact_mut(4)
        .zip(src.as_raw().par_chunks_exact(4))
        .for_each(|(d, s)| {
            let mut r = (s[0] as f32 / 255.0) * exposure_mul;
            let mut g = (s[1] as f32 / 255.0) * exposure_mul;
            let mut b = (s[2] as f32 / 255.0) * exposure_mul;

            if apply_gamma {
                r = r.max(0.0).powf(inv_gamma);
                g = g.max(0.0).powf(inv_gamma);
                b = b.max(0.0).powf(inv_gamma);
            }

            let luma_pre = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let shadows_w = 1.0 - smoothstep(0.0, 0.55, luma_pre);
            let highlights_w = smoothstep(0.45, 1.0, luma_pre);
            let tone_delta = shadows * 0.5 * shadows_w + highlights * 0.5 * highlights_w;
            r += tone_delta;
            g += tone_delta;
            b += tone_delta;

            r = (r - 0.5) * contrast + 0.5 + brightness;
            g = (g - 0.5) * contrast + 0.5 + brightness;
            b = (b - 0.5) * contrast + 0.5 + brightness;

            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            r = luma + (r - luma) * saturation;
            g = luma + (g - luma) * saturation;
            b = luma + (b - luma) * saturation;

            d[0] = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
            d[1] = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
            d[2] = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
            d[3] = s[3];
        });

    RgbaImage::from_raw(w, h, dst).unwrap_or_else(|| src.clone())
}

/// Build a 256-entry lookup table from normalized control points `[[x, y], ...]`
/// in `0.0..=1.0` using Fritsch–Carlson monotone cubic Hermite interpolation.
pub fn build_curve_lut(points: &[[f32; 2]]) -> [u8; 256] {
    let mut identity = [0u8; 256];
    for (i, slot) in identity.iter_mut().enumerate() {
        *slot = i as u8;
    }
    if points.len() < 2 {
        return identity;
    }
    let mut pts: Vec<(f64, f64)> = points
        .iter()
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .map(|p| {
            (
                f64::from(p[0]).clamp(0.0, 1.0),
                f64::from(p[1]).clamp(0.0, 1.0),
            )
        })
        .collect();
    if pts.len() < 2 {
        return identity;
    }
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut dedup: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    for (x, y) in pts {
        if let Some(last) = dedup.last_mut()
            && (x - last.0).abs() < 1e-4
        {
            last.1 = y;
            continue;
        }
        dedup.push((x, y));
    }
    let n = dedup.len();
    if n < 2 {
        return identity;
    }

    let mut h = vec![0.0; n - 1];
    let mut d = vec![0.0; n - 1];
    for i in 0..(n - 1) {
        h[i] = dedup[i + 1].0 - dedup[i].0;
        d[i] = (dedup[i + 1].1 - dedup[i].1) / h[i].max(1e-6);
    }

    let mut m = vec![0.0; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for i in 1..(n - 1) {
        if d[i - 1] * d[i] <= 0.0 {
            m[i] = 0.0;
        } else {
            let w1 = 2.0 * h[i] + h[i - 1];
            let w2 = h[i] + 2.0 * h[i - 1];
            m[i] = (w1 + w2) / (w1 / d[i - 1] + w2 / d[i]);
        }
    }
    for i in 0..(n - 1) {
        if d[i].abs() < 1e-9 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
        } else {
            let alpha = m[i] / d[i];
            let beta = m[i + 1] / d[i];
            let s = alpha * alpha + beta * beta;
            if s > 9.0 {
                let tau = 3.0 / s.sqrt();
                m[i] = tau * alpha * d[i];
                m[i + 1] = tau * beta * d[i];
            }
        }
    }

    let mut lut = [0u8; 256];
    let mut seg = 0usize;
    for (i, slot) in lut.iter_mut().enumerate() {
        let x = i as f64 / 255.0;
        let y = if x <= dedup[0].0 {
            dedup[0].1
        } else if x >= dedup[n - 1].0 {
            dedup[n - 1].1
        } else {
            while seg + 1 < n - 1 && x > dedup[seg + 1].0 {
                seg += 1;
            }
            let hi = h[seg].max(1e-6);
            let t = ((x - dedup[seg].0) / hi).clamp(0.0, 1.0);
            let t2 = t * t;
            let t3 = t2 * t;
            let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
            let h10 = t3 - 2.0 * t2 + t;
            let h01 = -2.0 * t3 + 3.0 * t2;
            let h11 = t3 - t2;
            h00 * dedup[seg].1 + h10 * hi * m[seg] + h01 * dedup[seg + 1].1 + h11 * hi * m[seg + 1]
        };
        *slot = (y.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    lut
}

pub fn is_identity_lut(lut: &[u8; 256]) -> bool {
    lut.iter().enumerate().all(|(i, &v)| v == i as u8)
}

/// Apply per-channel + master tonal curves in parallel, preserving alpha.
pub fn adjust_curves(src: &RgbaImage, curves: &CurvesAdjustment) -> RgbaImage {
    let lut_rgb = build_curve_lut(&curves.rgb);
    let lut_r = build_curve_lut(&curves.red);
    let lut_g = build_curve_lut(&curves.green);
    let lut_b = build_curve_lut(&curves.blue);

    let (w, h) = src.dimensions();
    let mut dst = vec![0u8; (w as usize) * (h as usize) * 4];
    dst.par_chunks_exact_mut(4)
        .zip(src.as_raw().par_chunks_exact(4))
        .for_each(|(d, s)| {
            d[0] = lut_rgb[usize::from(lut_r[usize::from(s[0])])];
            d[1] = lut_rgb[usize::from(lut_g[usize::from(s[1])])];
            d[2] = lut_rgb[usize::from(lut_b[usize::from(s[2])])];
            d[3] = s[3];
        });
    RgbaImage::from_raw(w, h, dst).unwrap_or_else(|| src.clone())
}

/// Krita-style parallel HSV adjustment (`hue` in `-180..=180`, `saturation` & `value` in `-1..=1`).
pub fn adjust_hsv(src: &RgbaImage, adj: &HsvAdjustment) -> RgbaImage {
    let dh = f64::from(adj.hue).clamp(-180.0, 180.0);
    let ds = f64::from(adj.saturation).clamp(-1.0, 1.0);
    let dv = f64::from(adj.value).clamp(-1.0, 1.0);
    let (w, h) = src.dimensions();
    let mut dst = vec![0u8; (w as usize) * (h as usize) * 4];

    dst.par_chunks_exact_mut(4)
        .zip(src.as_raw().par_chunks_exact(4))
        .for_each(|(d, s)| {
            let r = f64::from(s[0]) / 255.0;
            let g = f64::from(s[1]) / 255.0;
            let b = f64::from(s[2]) / 255.0;

            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            let delta = max - min;

            let mut hue = if delta < 1e-6 {
                0.0
            } else if (max - r).abs() < 1e-6 {
                60.0 * (((g - b) / delta) % 6.0)
            } else if (max - g).abs() < 1e-6 {
                60.0 * (((b - r) / delta) + 2.0)
            } else {
                60.0 * (((r - g) / delta) + 4.0)
            };
            if hue < 0.0 {
                hue += 360.0;
            }
            let mut sat = if max < 1e-6 { 0.0 } else { delta / max };
            let mut val = max;

            hue = (hue + dh).rem_euclid(360.0);
            sat = if ds >= 0.0 {
                (sat + (1.0 - sat) * ds).clamp(0.0, 1.0)
            } else {
                (sat * (1.0 + ds)).clamp(0.0, 1.0)
            };
            val = if dv >= 0.0 {
                (val + (1.0 - val) * dv).clamp(0.0, 1.0)
            } else {
                (val * (1.0 + dv)).clamp(0.0, 1.0)
            };

            let c = val * sat;
            let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
            let m = val - c;
            let (nr, ng, nb) = if hue < 60.0 {
                (c, x, 0.0)
            } else if hue < 120.0 {
                (x, c, 0.0)
            } else if hue < 180.0 {
                (0.0, c, x)
            } else if hue < 240.0 {
                (0.0, x, c)
            } else if hue < 300.0 {
                (x, 0.0, c)
            } else {
                (c, 0.0, x)
            };
            d[0] = ((nr + m).clamp(0.0, 1.0) * 255.0).round() as u8;
            d[1] = ((ng + m).clamp(0.0, 1.0) * 255.0).round() as u8;
            d[2] = ((nb + m).clamp(0.0, 1.0) * 255.0).round() as u8;
            d[3] = s[3];
        });

    RgbaImage::from_raw(w, h, dst).unwrap_or_else(|| src.clone())
}

/// Krita/GIMP-style parallel Color Balance across Shadows, Midtones, and Highlights.
pub fn color_balance(src: &RgbaImage, adj: &ColorBalanceAdjustment) -> RgbaImage {
    let clamp3 = |v: [f32; 3]| {
        [
            f64::from(v[0]).clamp(-1.0, 1.0),
            f64::from(v[1]).clamp(-1.0, 1.0),
            f64::from(v[2]).clamp(-1.0, 1.0),
        ]
    };
    let sh = clamp3(adj.shadows);
    let md = clamp3(adj.midtones);
    let hi = clamp3(adj.highlights);
    let preserve = adj.preserve_luminosity;

    let mut w_sh = [0.0f64; 256];
    let mut w_md = [0.0f64; 256];
    let mut w_hi = [0.0f64; 256];
    for i in 0..256 {
        let t = i as f64 / 255.0;
        let a = 0.25;
        let b = 0.333;
        let scale = 0.7;
        w_sh[i] = ((t - b) / -a + 0.5).clamp(0.0, 1.0) * scale;
        w_md[i] = ((t - b) / a + 0.5).clamp(0.0, 1.0)
            * ((t + b - 1.0) / -a + 0.5).clamp(0.0, 1.0)
            * scale;
        w_hi[i] = ((t + b - 1.0) / a + 0.5).clamp(0.0, 1.0) * scale;
    }

    let (w, h) = src.dimensions();
    let mut dst = vec![0u8; (w as usize) * (h as usize) * 4];
    dst.par_chunks_exact_mut(4)
        .zip(src.as_raw().par_chunks_exact(4))
        .for_each(|(d, s)| {
            let r0 = f64::from(s[0]) / 255.0;
            let g0 = f64::from(s[1]) / 255.0;
            let b0 = f64::from(s[2]) / 255.0;

            let lum = (0.2126 * r0 + 0.7152 * g0 + 0.0722 * b0).clamp(0.0, 1.0);
            let li = (lum * 255.0).round() as usize;
            let ws = w_sh[li];
            let wm = w_md[li];
            let wh = w_hi[li];

            let mut r = (r0 + sh[0] * ws + md[0] * wm + hi[0] * wh).clamp(0.0, 1.0);
            let mut g = (g0 + sh[1] * ws + md[1] * wm + hi[1] * wh).clamp(0.0, 1.0);
            let mut b = (b0 + sh[2] * ws + md[2] * wm + hi[2] * wh).clamp(0.0, 1.0);

            if preserve {
                let l0 = 0.2126 * r0 + 0.7152 * g0 + 0.0722 * b0;
                let l1 = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                let diff = l0 - l1;
                r = (r + diff).clamp(0.0, 1.0);
                g = (g + diff).clamp(0.0, 1.0);
                b = (b + diff).clamp(0.0, 1.0);
            }

            d[0] = (r * 255.0).round() as u8;
            d[1] = (g * 255.0).round() as u8;
            d[2] = (b * 255.0).round() as u8;
            d[3] = s[3];
        });

    RgbaImage::from_raw(w, h, dst).unwrap_or_else(|| src.clone())
}

/// Compute `(dx, dy)` pixel shift for resizing a canvas with the given 9-point anchor.
pub fn anchor_offset(
    old_w: u32,
    old_h: u32,
    new_w: u32,
    new_h: u32,
    anchor: CanvasAnchor,
) -> (i32, i32) {
    let dw = new_w as i32 - old_w as i32;
    let dh = new_h as i32 - old_h as i32;
    let dx = match anchor {
        CanvasAnchor::TopLeft | CanvasAnchor::Left | CanvasAnchor::BottomLeft => 0,
        CanvasAnchor::TopRight | CanvasAnchor::Right | CanvasAnchor::BottomRight => dw,
        _ => dw / 2,
    };
    let dy = match anchor {
        CanvasAnchor::TopLeft | CanvasAnchor::Top | CanvasAnchor::TopRight => 0,
        CanvasAnchor::BottomLeft | CanvasAnchor::Bottom | CanvasAnchor::BottomRight => dh,
        _ => dh / 2,
    };
    (dx, dy)
}

/// Resize the canvas without scaling pixels, placing the source at `anchor`.
pub fn canvas_size(
    src: &RgbaImage,
    width: u32,
    height: u32,
    anchor: CanvasAnchor,
    fill: [u8; 3],
    fill_alpha: u8,
) -> RgbaImage {
    let (doc_w, doc_h) = src.dimensions();
    let background = if fill_alpha == 0 {
        Rgba([0, 0, 0, 0])
    } else {
        Rgba([fill[0], fill[1], fill[2], fill_alpha])
    };
    let mut output = RgbaImage::from_pixel(width, height, background);
    let (dx, dy) = anchor_offset(doc_w, doc_h, width, height, anchor);
    let out_w_i = i64::from(width);
    let out_h_i = i64::from(height);
    for sy in 0..doc_h {
        let ty = i64::from(sy) + i64::from(dy);
        if ty < 0 || ty >= out_h_i {
            continue;
        }
        for sx in 0..doc_w {
            let tx = i64::from(sx) + i64::from(dx);
            if tx < 0 || tx >= out_w_i {
                continue;
            }
            let s = *src.get_pixel(sx, sy);
            let d = *output.get_pixel(tx as u32, ty as u32);
            output.put_pixel(tx as u32, ty as u32, blend_over(d, s));
        }
    }
    output
}

/// Rotate `src` clockwise by `degrees`, expanding the canvas to the rotated bounding box.
pub fn rotate_angle(src: &RgbaImage, degrees: f64, fill: [u8; 3], fill_alpha: u8) -> RgbaImage {
    let (w, h) = src.dimensions();
    let wf = f64::from(w);
    let hf = f64::from(h);
    let rad = degrees.to_radians();
    let cos = rad.cos();
    let sin = rad.sin();
    let new_w = ((wf * cos.abs() + hf * sin.abs()).ceil() as u32).clamp(1, 16384);
    let new_h = ((wf * sin.abs() + hf * cos.abs()).ceil() as u32).clamp(1, 16384);
    let background = if fill_alpha == 0 {
        Rgba([0, 0, 0, 0])
    } else {
        Rgba([fill[0], fill[1], fill[2], fill_alpha])
    };
    let dst_cx = f64::from(new_w) / 2.0;
    let dst_cy = f64::from(new_h) / 2.0;
    let src_cx = wf / 2.0;
    let src_cy = hf / 2.0;

    let row_stride = (new_w as usize) * 4;
    let mut buf = vec![0u8; (new_h as usize) * row_stride];
    buf.par_chunks_exact_mut(row_stride)
        .enumerate()
        .for_each(|(y, row)| {
            let dy = (y as f64 + 0.5) - dst_cy;
            for x in 0..new_w {
                let dx = (f64::from(x) + 0.5) - dst_cx;
                let sx = dx * cos + dy * sin + src_cx;
                let sy = -dx * sin + dy * cos + src_cy;
                let px = if sx >= 0.0 && sx <= wf && sy >= 0.0 && sy <= hf {
                    let sampled = sample_bilinear(src, sx - 0.5, sy - 0.5);
                    blend_over(background, sampled).0
                } else {
                    background.0
                };
                let off = (x as usize) * 4;
                row[off..off + 4].copy_from_slice(&px);
            }
        });

    RgbaImage::from_raw(new_w, new_h, buf).unwrap_or_else(|| src.clone())
}

/// Crop or grow-crop `src` to `width x height` at `(x, y)`.
pub fn grow_crop(
    src: &RgbaImage,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    fill: [u8; 3],
    fill_alpha: u8,
) -> RgbaImage {
    let bg = if fill_alpha == 0 {
        Rgba([0, 0, 0, 0])
    } else {
        Rgba([fill[0], fill[1], fill[2], fill_alpha])
    };
    let mut canvas = RgbaImage::from_pixel(width, height, bg);
    let (doc_w, doc_h) = src.dimensions();
    let source_left = x.max(0).min(doc_w as i32);
    let source_top = y.max(0).min(doc_h as i32);
    let source_right = (i64::from(x) + i64::from(width))
        .min(i64::from(doc_w))
        .max(0) as i32;
    let source_bottom = (i64::from(y) + i64::from(height))
        .min(i64::from(doc_h))
        .max(0) as i32;
    if source_right > source_left && source_bottom > source_top {
        let cropped = imageops::crop_imm(
            src,
            source_left as u32,
            source_top as u32,
            (source_right - source_left) as u32,
            (source_bottom - source_top) as u32,
        )
        .to_image();
        imageops::replace(
            &mut canvas,
            &cropped,
            i64::from(source_left - x),
            i64::from(source_top - y),
        );
    }
    canvas
}

fn filter_type(filter: ResizeFilter) -> FilterType {
    match filter {
        ResizeFilter::Lanczos3 => FilterType::Lanczos3,
        ResizeFilter::Nearest => FilterType::Nearest,
        ResizeFilter::Triangle => FilterType::Triangle,
        ResizeFilter::CatmullRom => FilterType::CatmullRom,
    }
}

/// Resize `src` to `width x height` using `mode` (`Stretch`, `Cover`, `Pad`) and `filter`.
pub fn resize_with_mode(
    src: &RgbaImage,
    width: u32,
    height: u32,
    filter: ResizeFilter,
    mode: ResizeMode,
    fill: [u8; 3],
    fill_alpha: u8,
) -> RgbaImage {
    let f = filter_type(filter);
    match mode {
        ResizeMode::Stretch => imageops::resize(src, width, height, f),
        ResizeMode::Cover => {
            let (sw, sh) = src.dimensions();
            let (cx, cy, cw, ch) = cover_rect(sw, sh, width, height);
            let cropped = imageops::crop_imm(src, cx, cy, cw, ch).to_image();
            imageops::resize(&cropped, width, height, f)
        }
        ResizeMode::Pad => fit_with_padding(src, width, height, fill, fill_alpha, f),
    }
}

fn cover_rect(sw: u32, sh: u32, tw: u32, th: u32) -> (u32, u32, u32, u32) {
    let sw64 = u64::from(sw);
    let sh64 = u64::from(sh);
    let tw64 = u64::from(tw);
    let th64 = u64::from(th);
    let (cw, ch) = if sw64 * th64 > tw64 * sh64 {
        let w = ((sh64 * tw64 + th64 / 2) / th64).clamp(1, sw64) as u32;
        (w, sh)
    } else if sw64 * th64 < tw64 * sh64 {
        let h = ((sw64 * th64 + tw64 / 2) / tw64).clamp(1, sh64) as u32;
        (sw, h)
    } else {
        (sw, sh)
    };
    if sw.saturating_sub(cw) <= 1 && sh.saturating_sub(ch) <= 1 {
        return (0, 0, sw, sh);
    }
    ((sw - cw) / 2, (sh - ch) / 2, cw, ch)
}

fn fit_with_padding(
    src: &RgbaImage,
    tw: u32,
    th: u32,
    fill: [u8; 3],
    fill_alpha: u8,
    f: FilterType,
) -> RgbaImage {
    let (sw, sh) = src.dimensions();
    let sw64 = u64::from(sw);
    let sh64 = u64::from(sh);
    let tw64 = u64::from(tw);
    let th64 = u64::from(th);

    let (cw, ch) = if tw64 * sh64 <= th64 * sw64 {
        let h = (sh64 * tw64 + sw64 / 2) / sw64;
        (tw, h.clamp(1, th64) as u32)
    } else {
        let w = (sw64 * th64 + sh64 / 2) / sh64;
        (w.clamp(1, tw64) as u32, th)
    };
    let content = imageops::resize(src, cw, ch, f);
    let bg = if fill_alpha == 0 {
        Rgba([0, 0, 0, 0])
    } else {
        Rgba([fill[0], fill[1], fill[2], fill_alpha])
    };
    let mut canvas = RgbaImage::from_pixel(tw, th, bg);
    let x = (tw - cw) / 2;
    let y = (th - ch) / 2;
    imageops::overlay(&mut canvas, &content, i64::from(x), i64::from(y));
    canvas
}

/// Sample a single pixel from `rgba` at `(x, y)` if inside bounds.
pub fn sample_pixel(rgba: &RgbaImage, x: u32, y: u32) -> Option<[u8; 4]> {
    let (w, h) = rgba.dimensions();
    if x < w && y < h {
        Some(rgba.get_pixel(x, y).0)
    } else {
        None
    }
}

/// Paint or erase a freehand stroke supporting all 5 [`BrushTip`] profiles.
pub fn paint_stroke_with_brush(
    src: &RgbaImage,
    points: &[[f64; 2]],
    color: [u8; 3],
    radius: f32,
    opacity: f32,
    erase: bool,
    brush: BrushSettings,
) -> RgbaImage {
    paint_stroke_with_brush_clipped(src, points, color, radius, opacity, erase, brush, None)
}

/// Paint or erase a freehand stroke supporting all 5 [`BrushTip`] profiles,
/// optionally clipped to `selection` (`(mask, layer_offset_x, layer_offset_y)`).
#[allow(clippy::too_many_arguments)]
pub fn paint_stroke_with_brush_clipped(
    src: &RgbaImage,
    points: &[[f64; 2]],
    color: [u8; 3],
    radius: f32,
    opacity: f32,
    erase: bool,
    brush: BrushSettings,
    selection: Option<(&SelectionMask, i32, i32)>,
) -> RgbaImage {
    let active_sel = selection.filter(|(s, _, _)| s.has_selection());
    let smoothed = crate::drawing::smooth_stroke_points(
        points,
        brush.smoothing_mode,
        brush.stabilizer_distance,
    );
    let points = smoothed.as_slice();
    let radius = f64::from(radius);
    let hardness = f64::from(brush.hardness).clamp(0.0, 1.0);
    let flow = brush.flow.clamp(0.05, 1.0);
    let spacing = f64::from(brush.spacing).clamp(0.05, 2.0);
    let scatter = f64::from(brush.scatter).clamp(0.0, 2.5);

    if brush.tip == BrushTip::Round && hardness >= 0.84 && flow >= 0.99 && scatter <= 0.001 {
        return paint_smooth_round_stroke(
            src,
            points,
            color,
            radius,
            f64::from(opacity),
            erase,
            active_sel,
        );
    }

    let (width, height) = src.dimensions();
    let mut result = src.clone();
    if width == 0 || height == 0 || points.is_empty() {
        return result;
    }

    let scatter_pad = radius * scatter * 1.5;
    let feather = radius + scatter_pad + 2.0;
    let mut min_x = points[0][0];
    let mut min_y = points[0][1];
    let mut max_x = points[0][0];
    let mut max_y = points[0][1];
    for [x, y] in points {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }
    let left = (min_x - feather).floor().max(0.0) as u32;
    let top = (min_y - feather).floor().max(0.0) as u32;
    let right = ((max_x + feather).ceil().max(0.0) as u32).min(width);
    let bottom = ((max_y + feather).ceil().max(0.0) as u32).min(height);
    if left >= right || top >= bottom {
        return result;
    }

    let box_w = (right - left) as usize;
    let box_h = (bottom - top) as usize;
    let mut coverage = vec![0.0f32; box_w * box_h];

    let step = (radius * 2.0 * spacing).max(0.75);
    let mut dabs: Vec<[f64; 2]> = Vec::with_capacity(points.len() * 2);
    dabs.push(points[0]);
    let mut carry = 0.0f64;
    for pair in points.windows(2) {
        let a = pair[0];
        let b = pair[1];
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = dx.hypot(dy);
        if len < 1e-6 {
            continue;
        }
        let mut dist = step - carry;
        while dist <= len {
            let t = dist / len;
            dabs.push([a[0] + dx * t, a[1] + dy * t]);
            dist += step;
        }
        carry = len - (dist - step);
    }

    let angle_rad = f64::from(brush.angle).to_radians();
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();
    let r = radius.max(0.5);

    let stamp_dab =
        |coverage: &mut [f32], cx: f64, cy: f64, dab_r: f64, dab_flow: f32, seed: u32| {
            let pad = dab_r + 1.5;
            let x0 = (cx - pad).floor().max(left as f64) as usize;
            let y0 = (cy - pad).floor().max(top as f64) as usize;
            let x1 = ((cx + pad).ceil().max(0.0) as usize).min(right as usize);
            let y1 = ((cy + pad).ceil().max(0.0) as usize).min(bottom as usize);
            if x0 >= x1 || y0 >= y1 {
                return;
            }
            let inner = dab_r * hardness;
            let falloff = (dab_r - inner).max(0.5);
            for py in y0..y1 {
                let fy = py as f64 + 0.5 - cy;
                let row = (py - top as usize) * box_w;
                for px in x0..x1 {
                    let fx = px as f64 + 0.5 - cx;
                    let mask = match brush.tip {
                        BrushTip::Round => {
                            let d = fx.hypot(fy);
                            if d <= inner {
                                1.0f32
                            } else if d < dab_r + 0.5 {
                                let t =
                                    ((dab_r + 0.5 - d) / (falloff + 0.5)).clamp(0.0, 1.0) as f32;
                                t * t * (3.0 - 2.0 * t)
                            } else {
                                0.0
                            }
                        }
                        BrushTip::Flat => {
                            let rx = fx * cos_a + fy * sin_a;
                            let ry = (-fx * sin_a + fy * cos_a) * 3.2;
                            let d = rx.hypot(ry);
                            if d <= inner {
                                1.0f32
                            } else if d < dab_r + 0.5 {
                                let t =
                                    ((dab_r + 0.5 - d) / (falloff + 0.5)).clamp(0.0, 1.0) as f32;
                                t * t * (3.0 - 2.0 * t)
                            } else {
                                0.0
                            }
                        }
                        BrushTip::Chalk => {
                            let d = fx.hypot(fy);
                            if d >= dab_r + 0.5 {
                                0.0
                            } else {
                                let base = if d <= inner {
                                    1.0f32
                                } else {
                                    ((dab_r + 0.5 - d) / (falloff + 0.5)).clamp(0.0, 1.0) as f32
                                };
                                let g1 = hash_f32(px as i32, py as i32, 17);
                                let g2 = hash_f32((px as i32) >> 1, (py as i32) >> 1, 53);
                                let grain = (g1 * 0.55 + g2 * 0.45 - 0.18).clamp(0.0, 1.0);
                                (base * grain * 1.35).clamp(0.0, 1.0)
                            }
                        }
                        BrushTip::Pixel => {
                            if fx.abs() <= dab_r && fy.abs() <= dab_r {
                                1.0
                            } else {
                                0.0
                            }
                        }
                        BrushTip::Spray => {
                            let d = fx.hypot(fy);
                            if d <= dab_r {
                                let t = (1.0 - d / (dab_r + 0.5)).clamp(0.0, 1.0) as f32;
                                let n = hash_f32(px as i32, py as i32, seed);
                                if n > 0.35 { t } else { 0.0 }
                            } else {
                                0.0
                            }
                        }
                    };
                    if mask > 0.0 {
                        let idx = row + (px - left as usize);
                        let add = (mask * dab_flow).clamp(0.0, 1.0);
                        coverage[idx] = 1.0 - (1.0 - coverage[idx]) * (1.0 - add);
                    }
                }
            }
        };

    for (idx, &[dx, dy]) in dabs.iter().enumerate() {
        let seed = (idx as u32).wrapping_add(1);
        if brush.tip == BrushTip::Spray {
            let droplets = 9u32;
            let spread = r * (1.0 + scatter);
            for d in 0..droplets {
                let s = seed.wrapping_mul(31).wrapping_add(d * 7 + 3);
                let ang = hash_f32(idx as i32, d as i32, s) as f64 * std::f64::consts::TAU;
                let dist = (hash_f32(d as i32, idx as i32, s ^ 0x55aa) as f64).sqrt() * spread;
                let drop_r =
                    (r * 0.16 * (0.5 + hash_f32(idx as i32, d as i32, s ^ 0x1234) as f64)).max(0.8);
                stamp_dab(
                    &mut coverage,
                    dx + ang.cos() * dist,
                    dy + ang.sin() * dist,
                    drop_r,
                    flow,
                    s,
                );
            }
        } else {
            let (jx, jy) = if scatter > 0.0 {
                let ang = hash_f32(idx as i32, 1, seed) as f64 * std::f64::consts::TAU;
                let mag = hash_f32(idx as i32, 2, seed) as f64 * r * scatter;
                (ang.cos() * mag, ang.sin() * mag)
            } else {
                (0.0, 0.0)
            };
            stamp_dab(&mut coverage, dx + jx, dy + jy, r, flow, seed);
        }
    }

    let alpha_scale = opacity.clamp(0.0, 1.0);
    for y in top..bottom {
        for x in left..right {
            let amount = coverage[((y - top) as usize) * box_w + (x as usize - left as usize)];
            if amount <= 0.0 {
                continue;
            }
            let sel_factor = active_sel
                .map(|(s, ox, oy)| s.clip_factor(ox + x as i32, oy + y as i32))
                .unwrap_or(1.0);
            if sel_factor <= 0.0 {
                continue;
            }
            let alpha = (amount * alpha_scale * sel_factor).clamp(0.0, 1.0);
            apply_paint_coverage_pixel(result.get_pixel_mut(x, y), color, alpha, erase);
        }
    }
    result
}

#[inline]
fn apply_paint_coverage_pixel(pixel: &mut Rgba<u8>, color: [u8; 3], alpha: f32, erase: bool) {
    if alpha <= 0.0 {
        return;
    }
    if erase {
        let da = f32::from(pixel.0[3]) / 255.0;
        let out_alpha = da * (1.0 - alpha);
        pixel.0[3] = (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        if pixel.0[3] == 0 {
            pixel.0 = [0, 0, 0, 0];
        }
    } else {
        blend_source_over_pixel(pixel, color, alpha);
    }
}

fn paint_smooth_round_stroke(
    src: &RgbaImage,
    points: &[[f64; 2]],
    color: [u8; 3],
    radius: f64,
    opacity: f64,
    erase: bool,
    active_sel: Option<(&SelectionMask, i32, i32)>,
) -> RgbaImage {
    let (width, height) = src.dimensions();
    let mut result = src.clone();
    if width == 0 || height == 0 || points.is_empty() {
        return result;
    }

    let feather = radius + 1.0;
    let first = points[0];
    let mut min_x = first[0];
    let mut min_y = first[1];
    let mut max_x = first[0];
    let mut max_y = first[1];
    for [x, y] in points {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }
    let left = (min_x - feather).floor().max(0.0) as u32;
    let top = (min_y - feather).floor().max(0.0) as u32;
    let right = ((max_x + feather).ceil().max(0.0) as u32).min(width);
    let bottom = ((max_y + feather).ceil().max(0.0) as u32).min(height);
    if left >= right || top >= bottom {
        return result;
    }

    let box_w = (right - left) as usize;
    let box_h = (bottom - top) as usize;
    let mut coverage = vec![0.0f32; box_w * box_h];

    let segments: Vec<([f64; 2], [f64; 2])> = if points.len() == 1 {
        vec![(points[0], points[0])]
    } else {
        points.windows(2).map(|pair| (pair[0], pair[1])).collect()
    };

    for (a, b) in &segments {
        let seg_left = (a[0].min(b[0]) - feather).floor().max(left as f64) as usize;
        let seg_top = (a[1].min(b[1]) - feather).floor().max(top as f64) as usize;
        let seg_right = ((a[0].max(b[0]) + feather).ceil().max(0.0) as usize).min(right as usize);
        let seg_bottom = ((a[1].max(b[1]) + feather).ceil().max(0.0) as usize).min(bottom as usize);
        if seg_left >= seg_right || seg_top >= seg_bottom {
            continue;
        }
        let delta = [b[0] - a[0], b[1] - a[1]];
        let length_squared = delta[0] * delta[0] + delta[1] * delta[1];
        for y in seg_top..seg_bottom {
            let py = y as f64 + 0.5;
            for x in seg_left..seg_right {
                let px = x as f64 + 0.5;
                let t = if length_squared > 0.0 {
                    (((px - a[0]) * delta[0] + (py - a[1]) * delta[1]) / length_squared)
                        .clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let cx = a[0] + delta[0] * t;
                let cy = a[1] + delta[1] * t;
                let distance = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                let value = (radius + 0.5 - distance).clamp(0.0, 1.0) as f32;
                let index = (y - top as usize) * box_w + (x - left as usize);
                if value > coverage[index] {
                    coverage[index] = value;
                }
            }
        }
    }

    let alpha_scale = opacity.clamp(0.0, 1.0) as f32;
    for y in top..bottom {
        for x in left..right {
            let amount = coverage[((y - top) as usize) * box_w + (x as usize - left as usize)];
            if amount <= 0.0 {
                continue;
            }
            let sel_factor = active_sel
                .map(|(s, ox, oy)| s.clip_factor(ox + x as i32, oy + y as i32))
                .unwrap_or(1.0);
            if sel_factor <= 0.0 {
                continue;
            }
            let alpha = (amount * alpha_scale * sel_factor).clamp(0.0, 1.0);
            apply_paint_coverage_pixel(result.get_pixel_mut(x, y), color, alpha, erase);
        }
    }
    result
}

#[inline]
fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

#[inline]
fn hash_f32(a: i32, b: i32, seed: u32) -> f32 {
    let h = hash_u32(
        (a as u32)
            .wrapping_mul(374_761_393)
            .wrapping_add((b as u32).wrapping_mul(668_265_263))
            .wrapping_add(seed.wrapping_mul(1_442_695_041)),
    );
    (h as f32) / (u32::MAX as f32)
}

#[inline]
pub(crate) fn blend_source_over_pixel(pixel: &mut Rgba<u8>, color: [u8; 3], src_alpha: f32) {
    if src_alpha <= 0.0 {
        return;
    }
    let sa = src_alpha.clamp(0.0, 1.0);
    let dst = pixel.0;
    let da = f32::from(dst[3]) / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        pixel.0 = [0, 0, 0, 0];
        return;
    }
    let blend = |sc: u8, dc: u8| -> u8 {
        let num = f32::from(sc) * sa + f32::from(dc) * da * (1.0 - sa);
        (num / out_a).round().clamp(0.0, 255.0) as u8
    };
    pixel.0 = [
        blend(color[0], dst[0]),
        blend(color[1], dst[1]),
        blend(color[2], dst[2]),
        (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
    ];
}

#[inline]
fn blend_over(dst: Rgba<u8>, src: Rgba<u8>) -> Rgba<u8> {
    let sa = f32::from(src[3]) / 255.0;
    if sa >= 1.0 {
        return src;
    }
    if sa <= 0.0 {
        return dst;
    }
    let da = f32::from(dst[3]) / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return Rgba([0, 0, 0, 0]);
    }
    let mix = |s: u8, d: u8| -> u8 {
        let v = (f32::from(s) * sa + f32::from(d) * da * (1.0 - sa)) / out_a;
        v.round().clamp(0.0, 255.0) as u8
    };
    Rgba([
        mix(src[0], dst[0]),
        mix(src[1], dst[1]),
        mix(src[2], dst[2]),
        (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
    ])
}

pub(crate) fn sample_bilinear(image: &RgbaImage, x: f64, y: f64) -> Rgba<u8> {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return Rgba([0, 0, 0, 255]);
    }
    let x = x.clamp(0.0, f64::from(width - 1));
    let y = y.clamp(0.0, f64::from(height - 1));
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let fx = x - f64::from(x0);
    let fy = y - f64::from(y0);
    let tl = image.get_pixel(x0, y0);
    let tr = image.get_pixel(x1, y0);
    let bl = image.get_pixel(x0, y1);
    let br = image.get_pixel(x1, y1);
    let mut out = [0u8; 4];
    for c in 0..4 {
        let top = f64::from(tl[c]) * (1.0 - fx) + f64::from(tr[c]) * fx;
        let bot = f64::from(bl[c]) * (1.0 - fx) + f64::from(br[c]) * fx;
        out[c] = (top * (1.0 - fy) + bot * fy).round().clamp(0.0, 255.0) as u8;
    }
    Rgba(out)
}

pub fn box_blur_rgba(src: &RgbaImage, radius: f32) -> RgbaImage {
    let r = (radius.round() as i32).clamp(0, 24);
    if r == 0 {
        return src.clone();
    }
    let (w, h) = src.dimensions();
    if w == 0 || h == 0 {
        return src.clone();
    }
    let mut tmp = RgbaImage::new(w, h);
    let win = (r * 2 + 1) as f32;
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 4];
            for dx in -r..=r {
                let sx = (x as i32 + dx).clamp(0, w as i32 - 1) as u32;
                let p = src.get_pixel(sx, y).0;
                for c in 0..4 {
                    sum[c] += u32::from(p[c]);
                }
            }
            tmp.put_pixel(
                x,
                y,
                Rgba([
                    (sum[0] as f32 / win).round().clamp(0.0, 255.0) as u8,
                    (sum[1] as f32 / win).round().clamp(0.0, 255.0) as u8,
                    (sum[2] as f32 / win).round().clamp(0.0, 255.0) as u8,
                    (sum[3] as f32 / win).round().clamp(0.0, 255.0) as u8,
                ]),
            );
        }
    }
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 4];
            for dy in -r..=r {
                let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as u32;
                let p = tmp.get_pixel(x, sy).0;
                for c in 0..4 {
                    sum[c] += u32::from(p[c]);
                }
            }
            out.put_pixel(
                x,
                y,
                Rgba([
                    (sum[0] as f32 / win).round().clamp(0.0, 255.0) as u8,
                    (sum[1] as f32 / win).round().clamp(0.0, 255.0) as u8,
                    (sum[2] as f32 / win).round().clamp(0.0, 255.0) as u8,
                    (sum[3] as f32 / win).round().clamp(0.0, 255.0) as u8,
                ]),
            );
        }
    }
    out
}

pub fn sharpen_rgba(src: &RgbaImage, amount: f32) -> RgbaImage {
    let k = amount.clamp(0.0, 3.0);
    if k <= 1e-4 {
        return src.clone();
    }
    let blurred = box_blur_rgba(src, 1.0);
    let (w, h) = src.dimensions();
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let s = src.get_pixel(x, y).0;
            let b = blurred.get_pixel(x, y).0;
            let mut px = [0u8; 4];
            for c in 0..3 {
                let v = f32::from(s[c]) + (f32::from(s[c]) - f32::from(b[c])) * k;
                px[c] = v.round().clamp(0.0, 255.0) as u8;
            }
            px[3] = s[3];
            out.put_pixel(x, y, Rgba(px));
        }
    }
    out
}

pub fn invert_rgba(src: &RgbaImage) -> RgbaImage {
    let mut out = src.clone();
    for px in out.pixels_mut() {
        px.0[0] = 255 - px.0[0];
        px.0[1] = 255 - px.0[1];
        px.0[2] = 255 - px.0[2];
    }
    out
}

pub fn desaturate_rgba(src: &RgbaImage) -> RgbaImage {
    let mut out = src.clone();
    for px in out.pixels_mut() {
        let g = (0.2126 * f32::from(px.0[0])
            + 0.7152 * f32::from(px.0[1])
            + 0.0722 * f32::from(px.0[2]))
        .round()
        .clamp(0.0, 255.0) as u8;
        px.0[0] = g;
        px.0[1] = g;
        px.0[2] = g;
    }
    out
}

pub fn sepia_rgba(src: &RgbaImage) -> RgbaImage {
    let mut out = src.clone();
    for px in out.pixels_mut() {
        let r = f32::from(px.0[0]);
        let g = f32::from(px.0[1]);
        let b = f32::from(px.0[2]);
        px.0[0] = (0.393 * r + 0.769 * g + 0.189 * b)
            .round()
            .clamp(0.0, 255.0) as u8;
        px.0[1] = (0.349 * r + 0.686 * g + 0.168 * b)
            .round()
            .clamp(0.0, 255.0) as u8;
        px.0[2] = (0.272 * r + 0.534 * g + 0.131 * b)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    out
}

/// Selection-aware edge-preserving bilateral denoise.
/// When `selection` is active, neighbor taps are weighted by `sel.alpha_at(nx, ny)` so
/// exterior pixels across the selection border are NEVER sampled (zero border halo).
pub fn denoise_rgba(
    src: &RgbaImage,
    strength: f32,
    detail_preserve: f32,
    selection: Option<(&SelectionMask, i32, i32)>,
) -> RgbaImage {
    let (w, h) = src.dimensions();
    let strength = strength.clamp(0.0, 1.0);
    if w == 0 || h == 0 || strength <= 1e-4 {
        return src.clone();
    }
    let detail_preserve = detail_preserve.clamp(0.0, 1.0);
    let radius: i32 = if strength > 0.65 { 3 } else { 2 };
    let sigma_s = 1.0 + strength * 2.0;
    let inv_two_s2 = 1.0 / (2.0 * sigma_s * sigma_s);
    let sigma_r = (0.04 + (1.0 - detail_preserve) * 0.28) * (0.4 + 0.6 * strength);
    let inv_two_r2 = 1.0 / (2.0 * sigma_r * sigma_r);
    let active_sel = selection.filter(|(s, _, _)| s.has_selection());

    let row_stride = (w as usize) * 4;
    let mut dst = vec![0u8; (h as usize) * row_stride];
    let src_raw = src.as_raw();

    dst.par_chunks_exact_mut(row_stride)
        .enumerate()
        .for_each(|(y_usize, row)| {
            let y = y_usize as i32;
            for x in 0..w {
                let xi = x as i32;
                let base_idx = (y_usize * (w as usize) + x as usize) * 4;
                let center = [
                    src_raw[base_idx],
                    src_raw[base_idx + 1],
                    src_raw[base_idx + 2],
                    src_raw[base_idx + 3],
                ];
                let center_sa = if let Some((sel, ox, oy)) = active_sel {
                    let sa = sel.alpha_at(ox + xi, oy + y);
                    if sa == 0 {
                        let out_off = (x as usize) * 4;
                        row[out_off..out_off + 4].copy_from_slice(&center);
                        continue;
                    }
                    sa
                } else {
                    255
                };

                let cr = f32::from(center[0]) / 255.0;
                let cg = f32::from(center[1]) / 255.0;
                let cb = f32::from(center[2]) / 255.0;

                let mut acc_r = 0.0f32;
                let mut acc_g = 0.0f32;
                let mut acc_b = 0.0f32;
                let mut w_sum = 0.0f32;

                for dy in -radius..=radius {
                    let ny = y + dy;
                    if ny < 0 || ny >= h as i32 {
                        continue;
                    }
                    for dx in -radius..=radius {
                        let nx = xi + dx;
                        if nx < 0 || nx >= w as i32 {
                            continue;
                        }
                        let sel_w = if let Some((sel, ox, oy)) = active_sel {
                            let sa = sel.alpha_at(ox + nx, oy + ny);
                            if sa == 0 {
                                continue;
                            }
                            f32::from(sa) / 255.0
                        } else {
                            1.0
                        };
                        let n_idx = ((ny as usize) * (w as usize) + (nx as usize)) * 4;
                        let na = src_raw[n_idx + 3];
                        if center[3] > 0 && na == 0 {
                            continue;
                        }
                        let nr = f32::from(src_raw[n_idx]) / 255.0;
                        let ng = f32::from(src_raw[n_idx + 1]) / 255.0;
                        let nb = f32::from(src_raw[n_idx + 2]) / 255.0;

                        let d2_s = (dx * dx + dy * dy) as f32;
                        let dr = nr - cr;
                        let dg = ng - cg;
                        let db = nb - cb;
                        let d2_r = dr * dr + dg * dg + db * db;
                        let wt = (-d2_s * inv_two_s2 - d2_r * inv_two_r2).exp() * sel_w;
                        acc_r += nr * wt;
                        acc_g += ng * wt;
                        acc_b += nb * wt;
                        w_sum += wt;
                    }
                }

                let out_off = (x as usize) * 4;
                if w_sum > 1e-6 {
                    let fr = acc_r / w_sum;
                    let fg = acc_g / w_sum;
                    let fb = acc_b / w_sum;
                    let mix_t = (strength * (f32::from(center_sa) / 255.0)).clamp(0.0, 1.0);
                    let mr = cr * (1.0 - mix_t) + fr * mix_t;
                    let mg = cg * (1.0 - mix_t) + fg * mix_t;
                    let mb = cb * (1.0 - mix_t) + fb * mix_t;
                    row[out_off] = (mr.clamp(0.0, 1.0) * 255.0).round() as u8;
                    row[out_off + 1] = (mg.clamp(0.0, 1.0) * 255.0).round() as u8;
                    row[out_off + 2] = (mb.clamp(0.0, 1.0) * 255.0).round() as u8;
                    row[out_off + 3] = center[3];
                } else {
                    row[out_off..out_off + 4].copy_from_slice(&center);
                }
            }
        });

    RgbaImage::from_raw(w, h, dst).unwrap_or_else(|| src.clone())
}

#[inline]
fn catmull_rom_weight(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 1.0 {
        1.5 * ax * ax * ax - 2.5 * ax * ax + 1.0
    } else if ax < 2.0 {
        -0.5 * ax * ax * ax + 2.5 * ax * ax - 4.0 * ax + 2.0
    } else {
        0.0
    }
}

fn sample_bicubic_premultiplied(src: &RgbaImage, sx: f64, sy: f64) -> [u8; 4] {
    let (w, h) = src.dimensions();
    if w == 0 || h == 0 {
        return [0, 0, 0, 0];
    }
    if sx <= -1.0 || sy <= -1.0 || sx >= f64::from(w) || sy >= f64::from(h) {
        return [0, 0, 0, 0];
    }

    let ix = sx.floor() as i32;
    let iy = sy.floor() as i32;
    let fx = sx - f64::from(ix);
    let fy = sy - f64::from(iy);

    if fx.abs() < 1e-7 && fy.abs() < 1e-7 {
        if ix >= 0 && iy >= 0 && (ix as u32) < w && (iy as u32) < h {
            return src.get_pixel(ix as u32, iy as u32).0;
        }
        return [0, 0, 0, 0];
    }

    let mut wx = [0.0f64; 4];
    let mut wy = [0.0f64; 4];
    for i in 0..4 {
        wx[i] = catmull_rom_weight(f64::from(i as i32 - 1) - fx);
        wy[i] = catmull_rom_weight(f64::from(i as i32 - 1) - fy);
    }

    let mut pr = 0.0f64;
    let mut pg = 0.0f64;
    let mut pb = 0.0f64;
    let mut pa = 0.0f64;

    for (j, &w_y) in wy.iter().enumerate() {
        let py = iy + (j as i32 - 1);
        if py < 0 || py >= h as i32 {
            continue;
        }
        for (i, &w_x) in wx.iter().enumerate() {
            let px = ix + (i as i32 - 1);
            if px < 0 || px >= w as i32 {
                continue;
            }
            let wt = w_x * w_y;
            let p = src.get_pixel(px as u32, py as u32).0;
            let a = f64::from(p[3]) / 255.0;
            pr += f64::from(p[0]) * a * wt;
            pg += f64::from(p[1]) * a * wt;
            pb += f64::from(p[2]) * a * wt;
            pa += a * wt;
        }
    }

    if pa <= 1e-6 {
        return [0, 0, 0, 0];
    }
    let inv_pa = pa.recip();
    let out_a = pa.min(1.0);
    [
        (pr * inv_pa).round().clamp(0.0, 255.0) as u8,
        (pg * inv_pa).round().clamp(0.0, 255.0) as u8,
        (pb * inv_pa).round().clamp(0.0, 255.0) as u8,
        (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

#[inline]
fn snap_near_int(v: f64) -> f64 {
    let r = v.round();
    if (v - r).abs() < 1e-6 { r } else { v }
}

/// Apply an affine transform (`translate`, `scale`, `rotation`, `pivot`, `flip_h`, `flip_v`)
/// to `src`, returning `(transformed_rgba, delta_offset_x, delta_offset_y)`.
pub fn transform_rgba_affine(src: &RgbaImage, spec: &LayerTransformSpec) -> (RgbaImage, i32, i32) {
    let (w, h) = src.dimensions();
    if w == 0 || h == 0 || spec.is_identity() {
        return (src.clone(), 0, 0);
    }

    let wf = f64::from(w);
    let hf = f64::from(h);
    let px = f64::from(spec.pivot_x) * wf;
    let py = f64::from(spec.pivot_y) * hf;
    let sx_mag = if spec.scale_x.abs() < 1e-4 {
        1e-4_f64.copysign(f64::from(spec.scale_x))
    } else {
        f64::from(spec.scale_x)
    };
    let sy_mag = if spec.scale_y.abs() < 1e-4 {
        1e-4_f64.copysign(f64::from(spec.scale_y))
    } else {
        f64::from(spec.scale_y)
    };
    let eff_sx = if spec.flip_h { -sx_mag } else { sx_mag };
    let eff_sy = if spec.flip_v { -sy_mag } else { sy_mag };
    let rad = f64::from(spec.rotation_deg).to_radians();
    let cos_t = rad.cos();
    let sin_t = rad.sin();
    let tx_off = f64::from(spec.translate_x);
    let ty_off = f64::from(spec.translate_y);

    let forward = |x: f64, y: f64| -> (f64, f64) {
        let dx = (x - px) * eff_sx;
        let dy = (y - py) * eff_sy;
        (
            dx * cos_t - dy * sin_t + px + tx_off,
            dx * sin_t + dy * cos_t + py + ty_off,
        )
    };

    let corners = [
        forward(0.0, 0.0),
        forward(wf, 0.0),
        forward(0.0, hf),
        forward(wf, hf),
    ];
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (cx, cy) in corners {
        min_x = min_x.min(cx);
        min_y = min_y.min(cy);
        max_x = max_x.max(cx);
        max_y = max_y.max(cy);
    }

    let delta_ox = snap_near_int(min_x).floor() as i32;
    let delta_oy = snap_near_int(min_y).floor() as i32;
    let end_x = snap_near_int(max_x).ceil() as i32;
    let end_y = snap_near_int(max_y).ceil() as i32;
    let out_w = (end_x - delta_ox).clamp(1, crate::MAX_IMAGE_DIMENSION as i32) as u32;
    let out_h = (end_y - delta_oy).clamp(1, crate::MAX_IMAGE_DIMENSION as i32) as u32;

    let inv_sx = eff_sx.recip();
    let inv_sy = eff_sy.recip();
    let row_stride = (out_w as usize) * 4;
    let mut buf = vec![0u8; (out_h as usize) * row_stride];

    buf.par_chunks_exact_mut(row_stride)
        .enumerate()
        .for_each(|(oy, row)| {
            let ty = oy as f64 + 0.5 + f64::from(delta_oy);
            let ry = ty - py - ty_off;
            for ox in 0..out_w {
                let tx = f64::from(ox) + 0.5 + f64::from(delta_ox);
                let rx = tx - px - tx_off;
                let ux = rx * cos_t + ry * sin_t;
                let uy = -rx * sin_t + ry * cos_t;
                let src_x = ux * inv_sx + px;
                let src_y = uy * inv_sy + py;
                let px_rgba = sample_bicubic_premultiplied(src, src_x - 0.5, src_y - 0.5);
                let off = (ox as usize) * 4;
                row[off..off + 4].copy_from_slice(&px_rgba);
            }
        });

    let out_img = RgbaImage::from_raw(out_w, out_h, buf).unwrap_or_else(|| src.clone());
    (out_img, delta_ox, delta_oy)
}

/// Transform an 8-bit grayscale mask buffer with [`LayerTransformSpec`].
pub fn transform_mask_affine(
    mask: &[u8],
    w: u32,
    h: u32,
    spec: &LayerTransformSpec,
) -> (Vec<u8>, u32, u32, i32, i32) {
    let img = crate::drawing::mask_to_rgba(mask, w, h);
    let (transformed, dx, dy) = transform_rgba_affine(&img, spec);
    let (nw, nh) = transformed.dimensions();
    let mut out = vec![0u8; (nw as usize) * (nh as usize)];
    for y in 0..nh {
        let row = (y as usize) * (nw as usize);
        for x in 0..nw {
            let p = transformed.get_pixel(x, y).0;
            out[row + x as usize] = ((u16::from(p[0]) * u16::from(p[3]) + 127) / 255) as u8;
        }
    }
    (out, nw, nh, dx, dy)
}
