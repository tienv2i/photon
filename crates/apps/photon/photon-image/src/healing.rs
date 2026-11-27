//! Spot and Sampled Healing Brush with boundary-ring gradient/texture synthesis
//! and frequency-separated texture transfer, respecting [`SelectionMask`] and [`BrushSettings`].

use image::{Rgba, RgbaImage};
use photon_protocol::{BrushSettings, HealingMode};

use crate::drawing::{paint_brush_dab, smooth_stroke_points};
use crate::pixels::{blend_source_over_pixel, sample_bilinear};
use crate::selection::SelectionMask;

struct DabPatch {
    cx: f64,
    cy: f64,
    mean_rgb: [f64; 3],
    grad_x: [f64; 3],
    grad_y: [f64; 3],
    mean_alpha: f64,
}

/// Apply a Healing Brush stroke (`Spot` or `Sampled`) onto `src`.
#[allow(clippy::too_many_arguments)]
pub fn healing_stroke(
    src: &RgbaImage,
    points: &[[f64; 2]],
    mode: HealingMode,
    radius: f32,
    strength: f32,
    source: Option<[f64; 2]>,
    brush: BrushSettings,
    selection: Option<(&SelectionMask, i32, i32)>,
) -> RgbaImage {
    healing_stroke_with_reference(
        src, src, points, mode, radius, strength, source, brush, selection,
    )
}

/// Apply a Healing Brush stroke onto `dst_base` while reading source/surroundings from `reference`.
#[allow(clippy::too_many_arguments)]
pub fn healing_stroke_with_reference(
    dst_base: &RgbaImage,
    reference: &RgbaImage,
    points: &[[f64; 2]],
    mode: HealingMode,
    radius: f32,
    strength: f32,
    source: Option<[f64; 2]>,
    brush: BrushSettings,
    selection: Option<(&SelectionMask, i32, i32)>,
) -> RgbaImage {
    let (w, h) = dst_base.dimensions();
    if w == 0 || h == 0 || points.is_empty() || reference.dimensions() != (w, h) {
        return dst_base.clone();
    }
    let strength = strength.clamp(0.0, 1.0);
    if strength <= 1e-4 {
        return dst_base.clone();
    }

    let smoothed = smooth_stroke_points(points, brush.smoothing_mode, brush.stabilizer_distance);
    if smoothed.is_empty() {
        return dst_base.clone();
    }

    let r = f64::from(radius).clamp(0.5, 4096.0);
    let spacing = f64::from(brush.spacing).clamp(0.05, 2.0);
    let dabs = interpolate_dabs(&smoothed, r, spacing);

    let pad = r + 2.0;
    let mut min_x = dabs[0][0];
    let mut min_y = dabs[0][1];
    let mut max_x = dabs[0][0];
    let mut max_y = dabs[0][1];
    for &[dx, dy] in &dabs {
        min_x = min_x.min(dx);
        min_y = min_y.min(dy);
        max_x = max_x.max(dx);
        max_y = max_y.max(dy);
    }
    let left = (min_x - pad).floor().max(0.0) as u32;
    let top = (min_y - pad).floor().max(0.0) as u32;
    let right = ((max_x + pad).ceil().max(0.0) as u32).min(w);
    let bottom = ((max_y + pad).ceil().max(0.0) as u32).min(h);
    if left >= right || top >= bottom {
        return dst_base.clone();
    }

    let box_w = (right - left) as usize;
    let box_h = (bottom - top) as usize;
    let mut coverage = vec![0.0f32; box_w * box_h];
    for (idx, &[dx, dy]) in dabs.iter().enumerate() {
        paint_brush_dab(
            &mut coverage,
            left,
            top,
            box_w,
            box_h,
            dx,
            dy,
            r,
            brush,
            (idx as u32).wrapping_add(1),
        );
    }

    let active_sel = selection.filter(|(sel, _, _)| sel.has_selection());
    let mut out = dst_base.clone();

    match mode {
        HealingMode::Sampled => {
            if let Some([src_x, src_y]) = source {
                let off_x = src_x - points[0][0];
                let off_y = src_y - points[0][1];
                apply_sampled_heal(
                    &mut out, reference, &coverage, left, top, right, bottom, r, strength, off_x,
                    off_y, active_sel,
                );
                return out;
            }
            apply_spot_heal(
                &mut out, reference, &dabs, &coverage, left, top, right, bottom, r, strength,
                active_sel,
            );
        }
        HealingMode::Spot => {
            apply_spot_heal(
                &mut out, reference, &dabs, &coverage, left, top, right, bottom, r, strength,
                active_sel,
            );
        }
    }

    out
}

fn interpolate_dabs(points: &[[f64; 2]], radius: f64, spacing: f64) -> Vec<[f64; 2]> {
    let step = (radius * 2.0 * spacing).max(0.75);
    let mut dabs = Vec::with_capacity(points.len() * 2);
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
    dabs
}

#[inline]
fn coverage_at(
    coverage: &[f32],
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    x: i32,
    y: i32,
) -> f32 {
    if x < left as i32 || y < top as i32 || x >= right as i32 || y >= bottom as i32 {
        return 0.0;
    }
    let bw = (right - left) as usize;
    let idx = ((y as u32 - top) as usize) * bw + ((x as u32 - left) as usize);
    coverage.get(idx).copied().unwrap_or(0.0)
}

#[allow(clippy::too_many_arguments)]
fn build_dab_patch(
    reference: &RgbaImage,
    coverage: &[f32],
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    cx: f64,
    cy: f64,
    r: f64,
    active_sel: Option<(&SelectionMask, i32, i32)>,
) -> DabPatch {
    let (w, h) = reference.dimensions();
    let ring_in = (r + 1.0).max(1.5);
    let ring_out = (r * 1.85 + 4.0).min(r + 28.0);
    let sx0 = (cx - ring_out).floor().max(0.0) as i32;
    let sy0 = (cy - ring_out).floor().max(0.0) as i32;
    let sx1 = ((cx + ring_out).ceil().max(0.0) as i32).min(w as i32 - 1);
    let sy1 = ((cy + ring_out).ceil().max(0.0) as i32).min(h as i32 - 1);
    let step = ((ring_out / 10.0).floor() as i32).max(1);

    let mut samples: Vec<(f64, f64, [f64; 4], f64)> = Vec::new();
    let mut fallback: Vec<(f64, f64, [f64; 4], f64)> = Vec::new();

    for py in (sy0..=sy1).step_by(step as usize) {
        let dy = py as f64 + 0.5 - cy;
        for px in (sx0..=sx1).step_by(step as usize) {
            let dx = px as f64 + 0.5 - cx;
            let dist = dx.hypot(dy);
            if dist < ring_in || dist > ring_out {
                continue;
            }
            let cov = coverage_at(coverage, left, top, right, bottom, px, py);
            if cov > 0.25 {
                continue;
            }
            let p = reference.get_pixel(px as u32, py as u32).0;
            let pa = f64::from(p[3]) / 255.0;
            let ring_w = (1.0 - (dist - ring_in) / (ring_out - ring_in + 1.0)).max(0.1);
            let sel_w = active_sel
                .map(|(sel, ox, oy)| f64::from(sel.clip_factor(ox + px, oy + py)))
                .unwrap_or(1.0);
            let rgba = [
                f64::from(p[0]),
                f64::from(p[1]),
                f64::from(p[2]),
                f64::from(p[3]),
            ];
            let w_clean = ring_w * (1.0 - f64::from(cov)) * (0.05 + pa);
            if sel_w > 0.01 {
                samples.push((dx, dy, rgba, w_clean * sel_w));
            } else {
                fallback.push((dx, dy, rgba, w_clean));
            }
        }
    }

    if samples.is_empty() {
        samples = fallback;
    }
    if samples.is_empty() {
        let px = cx.round().clamp(0.0, (w - 1) as f64) as u32;
        let py = cy.round().clamp(0.0, (h - 1) as f64) as u32;
        let p = reference.get_pixel(px, py).0;
        return DabPatch {
            cx,
            cy,
            mean_rgb: [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])],
            grad_x: [0.0; 3],
            grad_y: [0.0; 3],
            mean_alpha: f64::from(p[3]),
        };
    }

    let mut w_sum = 0.0f64;
    let mut mean_rgb = [0.0f64; 3];
    let mut mean_alpha = 0.0f64;
    let mut mean_dx = 0.0f64;
    let mut mean_dy = 0.0f64;
    for &(dx, dy, rgba, wt) in &samples {
        w_sum += wt;
        mean_dx += dx * wt;
        mean_dy += dy * wt;
        for c in 0..3 {
            mean_rgb[c] += rgba[c] * wt;
        }
        mean_alpha += rgba[3] * wt;
    }
    let inv_w = w_sum.max(1e-9).recip();
    mean_dx *= inv_w;
    mean_dy *= inv_w;
    for c in &mut mean_rgb {
        *c *= inv_w;
    }
    mean_alpha *= inv_w;

    // Fit weighted linear gradient for smooth directional interpolation across blemishes.
    let mut sxx = 0.0f64;
    let mut syy = 0.0f64;
    let mut sxy = 0.0f64;
    let mut sxv = [0.0f64; 3];
    let mut syv = [0.0f64; 3];
    for &(dx, dy, rgba, wt) in &samples {
        let rx = dx - mean_dx;
        let ry = dy - mean_dy;
        sxx += wt * rx * rx;
        syy += wt * ry * ry;
        sxy += wt * rx * ry;
        for c in 0..3 {
            let dv = rgba[c] - mean_rgb[c];
            sxv[c] += wt * rx * dv;
            syv[c] += wt * ry * dv;
        }
    }

    let det = sxx * syy - sxy * sxy;
    let mut grad_x = [0.0f64; 3];
    let mut grad_y = [0.0f64; 3];
    if det.abs() > 1e-3 {
        let inv_det = det.recip();
        for c in 0..3 {
            grad_x[c] = (syy * sxv[c] - sxy * syv[c]) * inv_det;
            grad_y[c] = (sxx * syv[c] - sxy * sxv[c]) * inv_det;
        }
    }

    DabPatch {
        cx,
        cy,
        mean_rgb,
        grad_x,
        grad_y,
        mean_alpha,
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_spot_heal(
    out: &mut RgbaImage,
    reference: &RgbaImage,
    dabs: &[[f64; 2]],
    coverage: &[f32],
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    r: f64,
    strength: f32,
    active_sel: Option<(&SelectionMask, i32, i32)>,
) {
    let patches: Vec<DabPatch> = dabs
        .iter()
        .map(|&[cx, cy]| {
            build_dab_patch(
                reference, coverage, left, top, right, bottom, cx, cy, r, active_sel,
            )
        })
        .collect();
    if patches.is_empty() {
        return;
    }

    let (w, h) = reference.dimensions();
    let box_w = (right - left) as usize;

    for py in top..bottom {
        let fy = py as f64 + 0.5;
        let row = (py - top) as usize * box_w;
        for px in left..right {
            let cov = coverage[row + (px - left) as usize];
            if cov <= 1e-4 {
                continue;
            }
            let sel_w = active_sel
                .map(|(sel, ox, oy)| sel.clip_factor(ox + px as i32, oy + py as i32))
                .unwrap_or(1.0);
            let alpha = (cov * strength * sel_w).clamp(0.0, 1.0);
            if alpha <= 1e-4 {
                continue;
            }

            let fx = px as f64 + 0.5;
            // Find nearest dab patch for local gradient + reflected boundary texture.
            let mut best = &patches[0];
            let mut best_d2 = (fx - best.cx).powi(2) + (fy - best.cy).powi(2);
            for p in &patches[1..] {
                let d2 = (fx - p.cx).powi(2) + (fy - p.cy).powi(2);
                if d2 < best_d2 {
                    best = p;
                    best_d2 = d2;
                }
            }

            let dx = fx - best.cx;
            let dy = fy - best.cy;
            let dist = dx.hypot(dy);
            let ring_dist = r + 2.0 + (r - dist).max(0.0) * 0.45;
            let (ux, uy) = if dist > 1e-4 {
                (dx / dist, dy / dist)
            } else {
                (1.0, 0.0)
            };
            let tex_x = (best.cx + ux * ring_dist).clamp(0.5, f64::from(w) - 0.5);
            let tex_y = (best.cy + uy * ring_dist).clamp(0.5, f64::from(h) - 0.5);

            // Only inject reflected texture when sample point lies outside stroke core.
            let tex_cov = coverage_at(
                coverage,
                left,
                top,
                right,
                bottom,
                tex_x.floor() as i32,
                tex_y.floor() as i32,
            );
            let tex_sample = sample_bilinear(reference, tex_x - 0.5, tex_y - 0.5).0;
            let t_dx = tex_x - best.cx;
            let t_dy = tex_y - best.cy;

            let mut healed_rgb = [0u8; 3];
            for c in 0..3 {
                let base_at_target = best.mean_rgb[c] + best.grad_x[c] * dx + best.grad_y[c] * dy;
                let base_at_tex = best.mean_rgb[c] + best.grad_x[c] * t_dx + best.grad_y[c] * t_dy;
                let high_freq = if tex_cov < 0.25 {
                    (f64::from(tex_sample[c]) - base_at_tex) * 0.65
                } else {
                    0.0
                };
                healed_rgb[c] = (base_at_target + high_freq).round().clamp(0.0, 255.0) as u8;
            }

            let dst_px = out.get_pixel_mut(px, py);
            if dst_px.0[3] == 0 && best.mean_alpha > 0.0 {
                let a = ((best.mean_alpha / 255.0) * f64::from(alpha)).clamp(0.0, 1.0) as f32;
                blend_source_over_pixel(dst_px, healed_rgb, a);
            } else {
                let a = alpha;
                let mix = |d: u8, s: u8| -> u8 {
                    (f32::from(d) * (1.0 - a) + f32::from(s) * a)
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                *dst_px = Rgba([
                    mix(dst_px.0[0], healed_rgb[0]),
                    mix(dst_px.0[1], healed_rgb[1]),
                    mix(dst_px.0[2], healed_rgb[2]),
                    dst_px.0[3],
                ]);
            }
        }
    }
}

/// Sample local low-frequency RGB average in `reference` centered at `(cx, cy)`.
fn sample_local_low_freq(reference: &RgbaImage, cx: f64, cy: f64, blur_r: i32) -> [f64; 3] {
    let (w, h) = reference.dimensions();
    let ix = cx.round() as i32;
    let iy = cy.round() as i32;
    let stride = (blur_r / 3).max(1);
    let mut sum = [0.0f64; 3];
    let mut w_sum = 0.0f64;
    for dy in (-blur_r..=blur_r).step_by(stride as usize) {
        let ny = (iy + dy).clamp(0, h as i32 - 1) as u32;
        for dx in (-blur_r..=blur_r).step_by(stride as usize) {
            let nx = (ix + dx).clamp(0, w as i32 - 1) as u32;
            let p = reference.get_pixel(nx, ny).0;
            let wt = 0.05 + f64::from(p[3]) / 255.0;
            sum[0] += f64::from(p[0]) * wt;
            sum[1] += f64::from(p[1]) * wt;
            sum[2] += f64::from(p[2]) * wt;
            w_sum += wt;
        }
    }
    let inv = w_sum.max(1e-9).recip();
    [sum[0] * inv, sum[1] * inv, sum[2] * inv]
}

/// Sample target low-frequency color while downweighting stroke interior pixels so
/// blemishes under the brush adapt to clean surroundings.
#[allow(clippy::too_many_arguments)]
fn sample_target_low_freq(
    reference: &RgbaImage,
    coverage: &[f32],
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    cx: i32,
    cy: i32,
    search_r: i32,
    active_sel: Option<(&SelectionMask, i32, i32)>,
) -> [f64; 3] {
    let (w, h) = reference.dimensions();
    let stride = (search_r / 4).max(1);
    let mut clean_sum = [0.0f64; 3];
    let mut clean_w = 0.0f64;
    let mut any_sum = [0.0f64; 3];
    let mut any_w = 0.0f64;

    for dy in (-search_r..=search_r).step_by(stride as usize) {
        let ny = cy + dy;
        if ny < 0 || ny >= h as i32 {
            continue;
        }
        for dx in (-search_r..=search_r).step_by(stride as usize) {
            let nx = cx + dx;
            if nx < 0 || nx >= w as i32 {
                continue;
            }
            let p = reference.get_pixel(nx as u32, ny as u32).0;
            let pa = 0.05 + f64::from(p[3]) / 255.0;
            let dist_w = 1.0 / (1.0 + (dx * dx + dy * dy) as f64 * 0.04);
            let wt_any = pa * dist_w;
            any_sum[0] += f64::from(p[0]) * wt_any;
            any_sum[1] += f64::from(p[1]) * wt_any;
            any_sum[2] += f64::from(p[2]) * wt_any;
            any_w += wt_any;

            let cov = coverage_at(coverage, left, top, right, bottom, nx, ny);
            let sel_w = active_sel
                .map(|(sel, ox, oy)| f64::from(sel.clip_factor(ox + nx, oy + ny)))
                .unwrap_or(1.0);
            if cov < 0.25 && sel_w > 0.01 {
                let wt = wt_any * (1.0 - f64::from(cov)) * sel_w;
                clean_sum[0] += f64::from(p[0]) * wt;
                clean_sum[1] += f64::from(p[1]) * wt;
                clean_sum[2] += f64::from(p[2]) * wt;
                clean_w += wt;
            }
        }
    }

    if clean_w > 1e-6 {
        let inv = clean_w.recip();
        [clean_sum[0] * inv, clean_sum[1] * inv, clean_sum[2] * inv]
    } else if any_w > 1e-6 {
        let inv = any_w.recip();
        [any_sum[0] * inv, any_sum[1] * inv, any_sum[2] * inv]
    } else {
        let p = reference
            .get_pixel(
                cx.clamp(0, w as i32 - 1) as u32,
                cy.clamp(0, h as i32 - 1) as u32,
            )
            .0;
        [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_sampled_heal(
    out: &mut RgbaImage,
    reference: &RgbaImage,
    coverage: &[f32],
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    r: f64,
    strength: f32,
    off_x: f64,
    off_y: f64,
    active_sel: Option<(&SelectionMask, i32, i32)>,
) {
    let (w, h) = reference.dimensions();
    let box_w = (right - left) as usize;
    let blur_r = (r * 0.45).round().clamp(2.0, 10.0) as i32;
    let target_search_r = (r * 1.6 + 4.0).round().clamp(4.0, 28.0) as i32;

    for py in top..bottom {
        let row = (py - top) as usize * box_w;
        for px in left..right {
            let cov = coverage[row + (px - left) as usize];
            if cov <= 1e-4 {
                continue;
            }
            let sel_w = active_sel
                .map(|(sel, ox, oy)| sel.clip_factor(ox + px as i32, oy + py as i32))
                .unwrap_or(1.0);
            let alpha = (cov * strength * sel_w).clamp(0.0, 1.0);
            if alpha <= 1e-4 {
                continue;
            }

            let sx = f64::from(px) + 0.5 + off_x;
            let sy = f64::from(py) + 0.5 + off_y;
            if sx < 0.0 || sy < 0.0 || sx >= f64::from(w) || sy >= f64::from(h) {
                continue;
            }

            let src_px = sample_bilinear(reference, sx - 0.5, sy - 0.5).0;
            let src_low = sample_local_low_freq(reference, sx - 0.5, sy - 0.5, blur_r);
            let tgt_low = sample_target_low_freq(
                reference,
                coverage,
                left,
                top,
                right,
                bottom,
                px as i32,
                py as i32,
                target_search_r,
                active_sel,
            );

            let mut healed_rgb = [0u8; 3];
            for c in 0..3 {
                let high = f64::from(src_px[c]) - src_low[c];
                healed_rgb[c] = (tgt_low[c] + high).round().clamp(0.0, 255.0) as u8;
            }

            let dst_px = out.get_pixel_mut(px, py);
            if dst_px.0[3] == 0 && src_px[3] > 0 {
                let a = (f32::from(src_px[3]) / 255.0) * alpha;
                blend_source_over_pixel(dst_px, healed_rgb, a);
            } else {
                let mix = |d: u8, s: u8| -> u8 {
                    (f32::from(d) * (1.0 - alpha) + f32::from(s) * alpha)
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                *dst_px = Rgba([
                    mix(dst_px.0[0], healed_rgb[0]),
                    mix(dst_px.0[1], healed_rgb[1]),
                    mix(dst_px.0[2], healed_rgb[2]),
                    dst_px.0[3],
                ]);
            }
        }
    }
}
