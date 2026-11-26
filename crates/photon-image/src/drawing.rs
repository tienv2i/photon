//! Vector/Bezier path tessellation, shape stroke & anti-aliased scanline fill,
//! brush smoothing modes, flood fill, and gradient rendering.

use std::collections::VecDeque;
use std::f64::consts::TAU;

use image::{Rgba, RgbaImage};
use photon_protocol::{
    BrushSettings, BrushSmoothingMode, BrushTip, DrawShapeKind, GradientColorMode, GradientKind,
    ShapeFillMode, VectorPathNode,
};

use crate::pixels::{self, blend_source_over_pixel};
use crate::selection::SelectionMask;

/// Apply [`BrushSmoothingMode`] to a sequence of raw pointer coordinates.
pub fn smooth_stroke_points(
    points: &[[f64; 2]],
    mode: BrushSmoothingMode,
    stabilizer_distance: f32,
) -> Vec<[f64; 2]> {
    if points.len() <= 1 || mode == BrushSmoothingMode::None {
        return points.to_vec();
    }
    match mode {
        BrushSmoothingMode::None => points.to_vec(),
        BrushSmoothingMode::Basic => {
            if points.len() <= 2 {
                return points.to_vec();
            }
            let mut out = Vec::with_capacity(points.len());
            out.push(points[0]);
            for i in 1..points.len() - 1 {
                let a = points[i - 1];
                let b = points[i];
                let c = points[i + 1];
                out.push([
                    (a[0] + 2.0 * b[0] + c[0]) * 0.25,
                    (a[1] + 2.0 * b[1] + c[1]) * 0.25,
                ]);
            }
            out.push(*points.last().expect("non-empty"));
            out
        }
        BrushSmoothingMode::Weighted => {
            if points.len() <= 2 {
                return points.to_vec();
            }
            let mut out = Vec::with_capacity(points.len());
            let last = points.len() - 1;
            out.push(points[0]);
            for i in 1..last {
                let p_m2 = points[i.saturating_sub(2)];
                let p_m1 = points[i - 1];
                let p_0 = points[i];
                let p_p1 = points[i + 1];
                let p_p2 = points[(i + 2).min(last)];
                out.push([
                    (p_m2[0] + 4.0 * p_m1[0] + 6.0 * p_0[0] + 4.0 * p_p1[0] + p_p2[0]) / 16.0,
                    (p_m2[1] + 4.0 * p_m1[1] + 6.0 * p_0[1] + 4.0 * p_p1[1] + p_p2[1]) / 16.0,
                ]);
            }
            out.push(points[last]);
            out
        }
        BrushSmoothingMode::Stabilizer => {
            let d_max = f64::from(stabilizer_distance).max(1.0);
            let mut cursor = points[0];
            let mut out = Vec::with_capacity(points.len());
            out.push(cursor);
            for &p in &points[1..] {
                let dx = p[0] - cursor[0];
                let dy = p[1] - cursor[1];
                let dist = dx.hypot(dy);
                if dist > d_max {
                    let scale = d_max / dist;
                    cursor = [p[0] - dx * scale, p[1] - dy * scale];
                    out.push(cursor);
                }
            }
            out
        }
    }
}

/// Adaptive De Casteljau cubic Bezier tessellation.
/// Returns the tessellated points excluding `p0` and including `p3`.
pub fn tessellate_cubic_bezier(
    p0: [f64; 2],
    p1: [f64; 2],
    p2: [f64; 2],
    p3: [f64; 2],
    tolerance: f64,
) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    de_casteljau_recursive(p0, p1, p2, p3, tolerance.max(0.05), 0, &mut out);
    out
}

fn de_casteljau_recursive(
    p0: [f64; 2],
    p1: [f64; 2],
    p2: [f64; 2],
    p3: [f64; 2],
    tolerance: f64,
    depth: u8,
    out: &mut Vec<[f64; 2]>,
) {
    if depth >= 10 || is_cubic_flat(p0, p1, p2, p3, tolerance) {
        out.push(p3);
        return;
    }
    let mid = |a: [f64; 2], b: [f64; 2]| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
    let q0 = mid(p0, p1);
    let q1 = mid(p1, p2);
    let q2 = mid(p2, p3);
    let r0 = mid(q0, q1);
    let r1 = mid(q1, q2);
    let s = mid(r0, r1);
    de_casteljau_recursive(p0, q0, r0, s, tolerance, depth + 1, out);
    de_casteljau_recursive(s, r1, q2, p3, tolerance, depth + 1, out);
}

fn is_cubic_flat(p0: [f64; 2], p1: [f64; 2], p2: [f64; 2], p3: [f64; 2], tolerance: f64) -> bool {
    let dx = p3[0] - p0[0];
    let dy = p3[1] - p0[1];
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-9 {
        let d1 = (p1[0] - p0[0]).hypot(p1[1] - p0[1]);
        let d2 = (p2[0] - p0[0]).hypot(p2[1] - p0[1]);
        return d1.max(d2) <= tolerance;
    }
    let inv_len = len_sq.sqrt().recip();
    let d1 = ((p1[0] - p0[0]) * dy - (p1[1] - p0[1]) * dx).abs() * inv_len;
    let d2 = ((p2[0] - p0[0]) * dy - (p2[1] - p0[1]) * dx).abs() * inv_len;
    d1.max(d2) <= tolerance
}

/// Tessellate a sequence of [`VectorPathNode`]s into a polyline.
pub fn tessellate_bezier_path(
    nodes: &[VectorPathNode],
    closed: bool,
    tolerance: f64,
) -> Vec<[f64; 2]> {
    if nodes.is_empty() {
        return Vec::new();
    }
    let mut pts = Vec::with_capacity(nodes.len() * 4);
    pts.push([f64::from(nodes[0].anchor[0]), f64::from(nodes[0].anchor[1])]);
    let seg_count = if closed && nodes.len() >= 2 {
        nodes.len()
    } else {
        nodes.len() - 1
    };
    for i in 0..seg_count {
        let a = nodes[i];
        let b = nodes[(i + 1) % nodes.len()];
        let p0 = [f64::from(a.anchor[0]), f64::from(a.anchor[1])];
        let p3 = [f64::from(b.anchor[0]), f64::from(b.anchor[1])];
        if a.handle_out.is_none() && b.handle_in.is_none() {
            pts.push(p3);
        } else {
            let p1 = a
                .handle_out
                .map_or(p0, |h| [f64::from(h[0]), f64::from(h[1])]);
            let p2 = b
                .handle_in
                .map_or(p3, |h| [f64::from(h[0]), f64::from(h[1])]);
            pts.extend(tessellate_cubic_bezier(p0, p1, p2, p3, tolerance));
        }
    }
    pts
}

/// Convert a [`DrawShapeKind`] and its control points/nodes into a polyline and closure flag.
pub fn shape_to_polyline(
    kind: DrawShapeKind,
    points: &[[f32; 2]],
    nodes: &[VectorPathNode],
    closed: bool,
) -> (Vec<[f64; 2]>, bool) {
    match kind {
        DrawShapeKind::Line => {
            if points.len() < 2 {
                return (Vec::new(), false);
            }
            (
                vec![
                    [f64::from(points[0][0]), f64::from(points[0][1])],
                    [f64::from(points[1][0]), f64::from(points[1][1])],
                ],
                false,
            )
        }
        DrawShapeKind::Rect => {
            if points.len() < 2 {
                return (Vec::new(), false);
            }
            let x0 = f64::from(points[0][0].min(points[1][0]));
            let y0 = f64::from(points[0][1].min(points[1][1]));
            let x1 = f64::from(points[0][0].max(points[1][0]));
            let y1 = f64::from(points[0][1].max(points[1][1]));
            (vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]], true)
        }
        DrawShapeKind::Ellipse => {
            if points.len() < 2 {
                return (Vec::new(), false);
            }
            let x0 = f64::from(points[0][0].min(points[1][0]));
            let y0 = f64::from(points[0][1].min(points[1][1]));
            let x1 = f64::from(points[0][0].max(points[1][0]));
            let y1 = f64::from(points[0][1].max(points[1][1]));
            let cx = (x0 + x1) * 0.5;
            let cy = (y0 + y1) * 0.5;
            let rx = (x1 - x0) * 0.5;
            let ry = (y1 - y0) * 0.5;
            let steps = ((rx + ry).max(8.0) * 1.5).clamp(24.0, 256.0) as usize;
            let mut poly = Vec::with_capacity(steps);
            for i in 0..steps {
                let theta = (i as f64 / steps as f64) * TAU;
                poly.push([cx + rx * theta.cos(), cy + ry * theta.sin()]);
            }
            (poly, true)
        }
        DrawShapeKind::Polygon => (
            points
                .iter()
                .map(|p| [f64::from(p[0]), f64::from(p[1])])
                .collect(),
            true,
        ),
        DrawShapeKind::Polyline => (
            points
                .iter()
                .map(|p| [f64::from(p[0]), f64::from(p[1])])
                .collect(),
            closed,
        ),
        DrawShapeKind::BezierPath => {
            if !nodes.is_empty() {
                (tessellate_bezier_path(nodes, closed, 0.25), closed)
            } else {
                (
                    points
                        .iter()
                        .map(|p| [f64::from(p[0]), f64::from(p[1])])
                        .collect(),
                    closed,
                )
            }
        }
    }
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

/// Stamp a single brush dab into a coverage buffer respecting [`BrushSettings`].
#[allow(clippy::too_many_arguments)]
pub fn paint_brush_dab(
    coverage: &mut [f32],
    box_left: u32,
    box_top: u32,
    box_w: usize,
    box_h: usize,
    cx: f64,
    cy: f64,
    dab_r: f64,
    brush: BrushSettings,
    seed: u32,
) {
    let right = box_left as usize + box_w;
    let bottom = box_top as usize + box_h;
    let pad = dab_r + 1.5;
    let x0 = (cx - pad).floor().max(box_left as f64) as usize;
    let y0 = (cy - pad).floor().max(box_top as f64) as usize;
    let x1 = ((cx + pad).ceil().max(0.0) as usize).min(right);
    let y1 = ((cy + pad).ceil().max(0.0) as usize).min(bottom);
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    let hardness = f64::from(brush.hardness).clamp(0.0, 1.0);
    let flow = brush.flow.clamp(0.05, 1.0);
    let angle_rad = f64::from(brush.angle).to_radians();
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();
    let inner = dab_r * hardness;
    let falloff = (dab_r - inner).max(0.5);

    for py in y0..y1 {
        let fy = py as f64 + 0.5 - cy;
        let row = (py - box_top as usize) * box_w;
        for px in x0..x1 {
            let fx = px as f64 + 0.5 - cx;
            let mask = match brush.tip {
                BrushTip::Round => {
                    let d = fx.hypot(fy);
                    if d <= inner {
                        1.0f32
                    } else if d < dab_r + 0.5 {
                        let t = ((dab_r + 0.5 - d) / (falloff + 0.5)).clamp(0.0, 1.0) as f32;
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
                        let t = ((dab_r + 0.5 - d) / (falloff + 0.5)).clamp(0.0, 1.0) as f32;
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
                let idx = row + (px - box_left as usize);
                let add = (mask * flow).clamp(0.0, 1.0);
                coverage[idx] = 1.0 - (1.0 - coverage[idx]) * (1.0 - add);
            }
        }
    }
}

/// Anti-aliased 4x4 supersampled scanline polygon fill (even-odd rule).
pub fn fill_polygon_aa(dst: &mut RgbaImage, poly: &[[f64; 2]], color: [u8; 3], opacity: f32) {
    let (w, h) = dst.dimensions();
    if w == 0 || h == 0 || poly.len() < 3 || opacity <= 0.0 {
        return;
    }

    let mut min_x = poly[0][0];
    let mut min_y = poly[0][1];
    let mut max_x = poly[0][0];
    let mut max_y = poly[0][1];
    for &[x, y] in &poly[1..] {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let left = min_x.floor().max(0.0) as u32;
    let top = min_y.floor().max(0.0) as u32;
    let right = (max_x.ceil().max(0.0) as u32).min(w);
    let bottom = (max_y.ceil().max(0.0) as u32).min(h);
    if left >= right || top >= bottom {
        return;
    }

    let box_w = (right - left) as usize;
    let alpha_scale = opacity.clamp(0.0, 1.0);
    const SUB: usize = 4;
    const SUB_OFFSETS: [f64; SUB] = [0.125, 0.375, 0.625, 0.875];

    let mut row_hits = vec![0u8; box_w];
    let mut xs = Vec::with_capacity(16);

    for py in top..bottom {
        row_hits.fill(0);
        for &sy_off in &SUB_OFFSETS {
            let sy = py as f64 + sy_off;
            xs.clear();
            let n = poly.len();
            for i in 0..n {
                let a = poly[i];
                let b = poly[(i + 1) % n];
                if (a[1] <= sy && b[1] > sy) || (b[1] <= sy && a[1] > sy) {
                    let t = (sy - a[1]) / (b[1] - a[1]);
                    xs.push(a[0] + t * (b[0] - a[0]));
                }
            }
            xs.sort_by(|a, b| a.total_cmp(b));
            for pair in xs.as_chunks::<2>().0 {
                let x_start = pair[0];
                let x_end = pair[1];
                let px_start = x_start.floor().max(left as f64) as u32;
                let px_end = (x_end.ceil().max(0.0) as u32).min(right);
                for px in px_start..px_end {
                    let mut count = 0u8;
                    for &sx_off in &SUB_OFFSETS {
                        let sx = px as f64 + sx_off;
                        if sx >= x_start && sx < x_end {
                            count += 1;
                        }
                    }
                    row_hits[(px - left) as usize] += count;
                }
            }
        }
        for px in left..right {
            let hits = row_hits[(px - left) as usize];
            if hits > 0 {
                let cov = f32::from(hits) / 16.0;
                let pixel = dst.get_pixel_mut(px, py);
                blend_source_over_pixel(pixel, color, (cov * alpha_scale).clamp(0.0, 1.0));
            }
        }
    }
}

/// Stroke a polyline (optionally closed) using [`pixels::paint_stroke_with_brush`].
#[allow(clippy::too_many_arguments)]
pub fn stroke_path(
    src: &RgbaImage,
    points: &[[f64; 2]],
    closed: bool,
    color: [u8; 3],
    radius: f32,
    opacity: f32,
    brush: BrushSettings,
) -> RgbaImage {
    if points.is_empty() {
        return src.clone();
    }
    let mut stroke_pts = points.to_vec();
    if closed && stroke_pts.len() >= 2 {
        stroke_pts.push(stroke_pts[0]);
    }
    pixels::paint_stroke_with_brush(src, &stroke_pts, color, radius, opacity, false, brush)
}

/// Draw a vector/geometric shape (stroke, fill, or both) onto `src`.
#[allow(clippy::too_many_arguments)]
pub fn draw_shape(
    src: &RgbaImage,
    kind: DrawShapeKind,
    points: &[[f32; 2]],
    nodes: &[VectorPathNode],
    fill_mode: ShapeFillMode,
    closed: bool,
    color: [u8; 3],
    radius: f32,
    opacity: f32,
    brush: BrushSettings,
) -> RgbaImage {
    let (poly, is_closed) = shape_to_polyline(kind, points, nodes, closed);
    if poly.is_empty() {
        return src.clone();
    }
    let mut out = src.clone();
    if matches!(
        fill_mode,
        ShapeFillMode::Fill | ShapeFillMode::StrokeAndFill
    ) && poly.len() >= 3
    {
        fill_polygon_aa(&mut out, &poly, color, opacity);
    }
    if matches!(
        fill_mode,
        ShapeFillMode::Stroke | ShapeFillMode::StrokeAndFill
    ) {
        out = stroke_path(&out, &poly, is_closed, color, radius, opacity, brush);
    }
    out
}

#[inline]
fn color_within_tolerance(a: [u8; 4], b: [u8; 4], tolerance: u8) -> bool {
    let tol = i32::from(tolerance);
    let dr = (i32::from(a[0]) - i32::from(b[0])).abs();
    let dg = (i32::from(a[1]) - i32::from(b[1])).abs();
    let db = (i32::from(a[2]) - i32::from(b[2])).abs();
    let da = (i32::from(a[3]) - i32::from(b[3])).abs();
    dr.max(dg).max(db).max(da) <= tol
}

/// Flood fill from `(x, y)` with `tolerance` (0..=255) and `contiguous` mode.
/// When `selection` is provided as `Some((mask, offset_x, offset_y))`, contiguous
/// traversal will not cross pixels outside the selection (`alpha == 0`).
#[allow(clippy::too_many_arguments)]
pub fn flood_fill(
    src: &RgbaImage,
    x: u32,
    y: u32,
    color: [u8; 3],
    opacity: f32,
    tolerance: u8,
    contiguous: bool,
    selection: Option<(&SelectionMask, i32, i32)>,
) -> RgbaImage {
    let (w, h) = src.dimensions();
    if w == 0 || h == 0 || x >= w || y >= h || opacity <= 0.0 {
        return src.clone();
    }
    let active_sel = selection.filter(|(sel, _, _)| sel.has_selection());
    if let Some((sel, ox, oy)) = active_sel
        && sel.alpha_at(ox + x as i32, oy + y as i32) == 0
    {
        return src.clone();
    }

    let seed = src.get_pixel(x, y).0;
    let mut out = src.clone();
    let alpha = opacity.clamp(0.0, 1.0);

    if !contiguous {
        for py in 0..h {
            for px in 0..w {
                if let Some((sel, ox, oy)) = active_sel
                    && sel.alpha_at(ox + px as i32, oy + py as i32) == 0
                {
                    continue;
                }
                if color_within_tolerance(src.get_pixel(px, py).0, seed, tolerance) {
                    blend_source_over_pixel(out.get_pixel_mut(px, py), color, alpha);
                }
            }
        }
        return out;
    }

    let total = (w as usize).saturating_mul(h as usize);
    let mut visited = vec![false; total];
    let mut queue = VecDeque::with_capacity(256);
    let start_idx = (y as usize) * (w as usize) + (x as usize);
    visited[start_idx] = true;
    queue.push_back((x, y));

    while let Some((cx, cy)) = queue.pop_front() {
        blend_source_over_pixel(out.get_pixel_mut(cx, cy), color, alpha);
        let neighbors = [
            cx.checked_sub(1).map(|nx| (nx, cy)),
            (cx + 1 < w).then_some((cx + 1, cy)),
            cy.checked_sub(1).map(|ny| (cx, ny)),
            (cy + 1 < h).then_some((cx, cy + 1)),
        ];
        for (nx, ny) in neighbors.into_iter().flatten() {
            let nidx = (ny as usize) * (w as usize) + (nx as usize);
            if visited[nidx] {
                continue;
            }
            if let Some((sel, ox, oy)) = active_sel
                && sel.alpha_at(ox + nx as i32, oy + ny as i32) == 0
            {
                continue;
            }
            if color_within_tolerance(src.get_pixel(nx, ny).0, seed, tolerance) {
                visited[nidx] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    out
}

const BAYER_4X4: [[f32; 4]; 4] = [
    [0.0 / 16.0 - 0.5, 0.0, 2.0 / 16.0 - 0.5, 10.0 / 16.0 - 0.5],
    [
        12.0 / 16.0 - 0.5,
        4.0 / 16.0 - 0.5,
        14.0 / 16.0 - 0.5,
        6.0 / 16.0 - 0.5,
    ],
    [
        3.0 / 16.0 - 0.5,
        11.0 / 16.0 - 0.5,
        1.0 / 16.0 - 0.5,
        9.0 / 16.0 - 0.5,
    ],
    [
        15.0 / 16.0 - 0.5,
        7.0 / 16.0 - 0.5,
        13.0 / 16.0 - 0.5,
        5.0 / 16.0 - 0.5,
    ],
];

/// Color/alpha stop in `[0.0, 1.0]` along a gradient ramp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStopSpec {
    pub pos: f64,
    pub color: [u8; 3],
    pub alpha: f64,
}

/// Krita-style gradient geometry shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GradientShapeSpec {
    #[default]
    Linear,
    Bilinear,
    Radial,
    Square,
    Conical,
    Spiral,
}

impl From<GradientKind> for GradientShapeSpec {
    fn from(kind: GradientKind) -> Self {
        match kind {
            GradientKind::Linear => Self::Linear,
            GradientKind::Bilinear | GradientKind::Reflected => Self::Bilinear,
            GradientKind::Radial => Self::Radial,
            GradientKind::Square => Self::Square,
            GradientKind::Conical => Self::Conical,
            GradientKind::Spiral => Self::Spiral,
        }
    }
}

/// Repeat policy outside `[0.0, 1.0]` for gradients.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GradientRepeatSpec {
    #[default]
    None,
    Forwards,
    Alternating,
}

/// Blend mode applied when compositing a gradient over `src`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GradientBlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
}

impl GradientBlendMode {
    pub fn from_key(key: &str) -> Self {
        match key {
            "multiply" => Self::Multiply,
            "screen" => Self::Screen,
            "overlay" => Self::Overlay,
            "darken" => Self::Darken,
            "lighten" => Self::Lighten,
            "difference" => Self::Difference,
            _ => Self::Normal,
        }
    }
}

/// Krita-style multi-stop gradient fill (`Linear`, `Bilinear`, `Radial`, `Square`,
/// `Conical`, `Spiral`) with repeat policy (`None`, `Forwards`, `Alternating`),
/// 4×4 Bayer ordered dithering, 7 blend modes, and optional `SelectionMask` clipping.
#[allow(clippy::too_many_arguments)]
pub fn render_gradient(
    src: &RgbaImage,
    start: [f64; 2],
    end: [f64; 2],
    start_color: [u8; 3],
    start_alpha: f64,
    end_color: [u8; 3],
    end_alpha: f64,
    stops: &[GradientStopSpec],
    opacity: f64,
    shape: GradientShapeSpec,
    repeat: GradientRepeatSpec,
    reverse: bool,
    dither: bool,
    blend_mode: GradientBlendMode,
    selection: Option<(&SelectionMask, i32, i32)>,
) -> RgbaImage {
    let mut out = src.clone();
    let (w, h) = out.dimensions();
    let op = opacity.clamp(0.0, 1.0) as f32;
    if w == 0 || h == 0 || op <= 0.0 {
        return out;
    }
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let len_sq = dx * dx + dy * dy;
    let len = len_sq.sqrt();
    if len < 1e-6 {
        return out;
    }
    let base_angle = dy.atan2(dx);
    let ux = dx / len;
    let uy = dy / len;
    let active_sel = selection.filter(|(sel, _, _)| sel.has_selection());

    let mut ramp: Vec<(f32, [u8; 3], f32)> = if stops.len() >= 2 {
        stops
            .iter()
            .map(|s| {
                (
                    s.pos.clamp(0.0, 1.0) as f32,
                    s.color,
                    s.alpha.clamp(0.0, 1.0) as f32,
                )
            })
            .collect()
    } else {
        vec![
            (0.0, start_color, start_alpha.clamp(0.0, 1.0) as f32),
            (1.0, end_color, end_alpha.clamp(0.0, 1.0) as f32),
        ]
    };
    ramp.sort_by(|a, b| a.0.total_cmp(&b.0));

    let sample_ramp = |t_in: f32| -> ([f32; 3], f32) {
        let t = if reverse { 1.0 - t_in } else { t_in }.clamp(0.0, 1.0);
        if t <= ramp[0].0 {
            let c = ramp[0].1;
            return (
                [f32::from(c[0]), f32::from(c[1]), f32::from(c[2])],
                ramp[0].2,
            );
        }
        let last = ramp.len() - 1;
        if t >= ramp[last].0 {
            let c = ramp[last].1;
            return (
                [f32::from(c[0]), f32::from(c[1]), f32::from(c[2])],
                ramp[last].2,
            );
        }
        for i in 0..last {
            let (p0, c0, a0) = ramp[i];
            let (p1, c1, a1) = ramp[i + 1];
            if t >= p0 && t <= p1 {
                let span = (p1 - p0).max(1e-6);
                let f = ((t - p0) / span).clamp(0.0, 1.0);
                let inv = 1.0 - f;
                let a = a0 * inv + a1 * f;
                let denom = a.max(1e-5);
                let r = (f32::from(c0[0]) * a0 * inv + f32::from(c1[0]) * a1 * f) / denom;
                let g = (f32::from(c0[1]) * a0 * inv + f32::from(c1[1]) * a1 * f) / denom;
                let b = (f32::from(c0[2]) * a0 * inv + f32::from(c1[2]) * a1 * f) / denom;
                return ([r, g, b], a);
            }
        }
        let c = ramp[last].1;
        (
            [f32::from(c[0]), f32::from(c[1]), f32::from(c[2])],
            ramp[last].2,
        )
    };

    for py in 0..h {
        let vy = f64::from(py) + 0.5 - start[1];
        let bayer_row = &BAYER_4X4[(py & 3) as usize];
        for px in 0..w {
            let sel_w = active_sel
                .map(|(sel, ox, oy)| sel.clip_factor(ox + px as i32, oy + py as i32))
                .unwrap_or(1.0);
            if sel_w <= 0.0 {
                continue;
            }
            let vx = f64::from(px) + 0.5 - start[0];
            let raw_t = match shape {
                GradientShapeSpec::Linear => (vx * dx + vy * dy) / len_sq,
                GradientShapeSpec::Bilinear => ((vx * dx + vy * dy) / len_sq).abs(),
                GradientShapeSpec::Radial => vx.hypot(vy) / len,
                GradientShapeSpec::Square => {
                    let rx = (vx * ux + vy * uy).abs();
                    let ry = (-vx * uy + vy * ux).abs();
                    rx.max(ry) / len
                }
                GradientShapeSpec::Conical => {
                    let a = (vy.atan2(vx) - base_angle).rem_euclid(TAU);
                    a / TAU
                }
                GradientShapeSpec::Spiral => {
                    let dist = vx.hypot(vy) / len;
                    let a = (vy.atan2(vx) - base_angle).rem_euclid(TAU) / TAU;
                    (dist + a).rem_euclid(1.0)
                }
            };
            let t = match repeat {
                GradientRepeatSpec::None => raw_t.clamp(0.0, 1.0),
                GradientRepeatSpec::Forwards => raw_t.rem_euclid(1.0),
                GradientRepeatSpec::Alternating => {
                    let m = raw_t.rem_euclid(2.0);
                    if m <= 1.0 { m } else { 2.0 - m }
                }
            } as f32;

            let ([rf, gf, bf], ramp_a) = sample_ramp(t);
            let sa = (ramp_a * op * sel_w).clamp(0.0, 1.0);
            if sa <= 1e-5 {
                continue;
            }
            let d = if dither {
                bayer_row[(px & 3) as usize]
            } else {
                0.0
            };
            let sr = (rf + d).round().clamp(0.0, 255.0) as u8;
            let sg = (gf + d).round().clamp(0.0, 255.0) as u8;
            let sb = (bf + d).round().clamp(0.0, 255.0) as u8;

            if blend_mode == GradientBlendMode::Normal {
                blend_source_over_pixel(out.get_pixel_mut(px, py), [sr, sg, sb], sa);
            } else {
                let pixel = out.get_pixel_mut(px, py);
                let dst = pixel.0;
                let da = f32::from(dst[3]) / 255.0;
                let out_a = sa + da * (1.0 - sa);
                if out_a <= 1e-5 {
                    pixel.0 = [0, 0, 0, 0];
                    continue;
                }
                let blend_ch = |sc_u: u8, dc_u: u8| -> u8 {
                    let sc = f32::from(sc_u) / 255.0;
                    let dc = f32::from(dc_u) / 255.0;
                    let b = match blend_mode {
                        GradientBlendMode::Normal => sc,
                        GradientBlendMode::Multiply => dc * sc,
                        GradientBlendMode::Screen => 1.0 - (1.0 - dc) * (1.0 - sc),
                        GradientBlendMode::Overlay => {
                            if dc < 0.5 {
                                2.0 * dc * sc
                            } else {
                                1.0 - 2.0 * (1.0 - dc) * (1.0 - sc)
                            }
                        }
                        GradientBlendMode::Darken => dc.min(sc),
                        GradientBlendMode::Lighten => dc.max(sc),
                        GradientBlendMode::Difference => (dc - sc).abs(),
                    };
                    let eff = (1.0 - da) * sc + da * b;
                    ((sa * eff + (1.0 - sa) * da * dc) / out_a * 255.0)
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                pixel.0 = [
                    blend_ch(sr, dst[0]),
                    blend_ch(sg, dst[1]),
                    blend_ch(sb, dst[2]),
                    (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
                ];
            }
        }
    }
    out
}

/// Render a luminance-converted multi-stop gradient onto a single-channel layer mask.
#[allow(clippy::too_many_arguments)]
pub fn render_gradient_on_mask(
    mask: &[u8],
    width: u32,
    height: u32,
    start: [f64; 2],
    end: [f64; 2],
    start_color: [u8; 3],
    start_alpha: f64,
    end_color: [u8; 3],
    end_alpha: f64,
    stops: &[GradientStopSpec],
    opacity: f64,
    shape: GradientShapeSpec,
    repeat: GradientRepeatSpec,
    reverse: bool,
    dither: bool,
    selection: Option<(&SelectionMask, i32, i32)>,
) -> Vec<u8> {
    let to_gray = |c: [u8; 3]| -> [u8; 3] {
        let g = rgb_luminance(c);
        [g, g, g]
    };
    let gray_stops: Vec<GradientStopSpec> = stops
        .iter()
        .map(|s| GradientStopSpec {
            pos: s.pos,
            color: to_gray(s.color),
            alpha: s.alpha,
        })
        .collect();
    let mask_img = mask_to_rgba(mask, width, height);
    let rendered = render_gradient(
        &mask_img,
        start,
        end,
        to_gray(start_color),
        start_alpha,
        to_gray(end_color),
        end_alpha,
        &gray_stops,
        opacity,
        shape,
        repeat,
        reverse,
        dither,
        GradientBlendMode::Normal,
        selection,
    );
    rgba_to_mask(&rendered)
}

/// Render a gradient (`Linear`, `Radial`, `Conical`, `Reflected`) onto `src`.
/// Supports `reverse`, ordered 4×4 Bayer `dither`, and optional `SelectionMask` clipping.
#[allow(clippy::too_many_arguments)]
pub fn draw_gradient(
    src: &RgbaImage,
    start: [f32; 2],
    end: [f32; 2],
    kind: GradientKind,
    color_mode: GradientColorMode,
    fg_color: [u8; 3],
    bg_color: [u8; 3],
    opacity: f32,
    reverse: bool,
    dither: bool,
    selection: Option<(&SelectionMask, i32, i32)>,
) -> RgbaImage {
    let (end_color, end_alpha) = match color_mode {
        GradientColorMode::FgToBg | GradientColorMode::Custom => (bg_color, 1.0),
        GradientColorMode::FgToTransparent => (fg_color, 0.0),
    };
    render_gradient(
        src,
        [f64::from(start[0]), f64::from(start[1])],
        [f64::from(end[0]), f64::from(end[1])],
        fg_color,
        1.0,
        end_color,
        end_alpha,
        &[],
        f64::from(opacity),
        GradientShapeSpec::from(kind),
        GradientRepeatSpec::None,
        reverse,
        dither,
        GradientBlendMode::Normal,
        selection,
    )
}

/// Convert RGB to grayscale luminance for mask operations.
pub(crate) fn rgb_luminance(rgb: [u8; 3]) -> u8 {
    (0.299 * f64::from(rgb[0]) + 0.587 * f64::from(rgb[1]) + 0.114 * f64::from(rgb[2]))
        .round()
        .clamp(0.0, 255.0) as u8
}

/// Helper to create an `RgbaImage` from a single-channel mask slice.
pub(crate) fn mask_to_rgba(mask: &[u8], w: u32, h: u32) -> RgbaImage {
    let mut img = RgbaImage::new(w, h);
    for y in 0..h {
        let row = (y as usize) * (w as usize);
        for x in 0..w {
            let v = mask.get(row + x as usize).copied().unwrap_or(255);
            img.put_pixel(x, y, Rgba([v, v, v, 255]));
        }
    }
    img
}

/// Extract channel 0 from an `RgbaImage` back into a single-channel mask buffer.
pub(crate) fn rgba_to_mask(img: &RgbaImage) -> Vec<u8> {
    let (w, h) = img.dimensions();
    let mut out = vec![0u8; (w as usize) * (h as usize)];
    for y in 0..h {
        let row = (y as usize) * (w as usize);
        for x in 0..w {
            out[row + x as usize] = img.get_pixel(x, y).0[0];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use photon_protocol::SelectionBooleanOp;

    #[test]
    fn bezier_tessellation_produces_smooth_curve_and_endpoints() {
        let nodes = vec![
            VectorPathNode {
                anchor: [0.0, 0.0],
                handle_in: None,
                handle_out: Some([0.0, 40.0]),
            },
            VectorPathNode {
                anchor: [40.0, 40.0],
                handle_in: Some([40.0, 0.0]),
                handle_out: None,
            },
        ];
        let pts = tessellate_bezier_path(&nodes, false, 0.25);
        assert!(
            pts.len() > 4,
            "expected subdivided points, got {}",
            pts.len()
        );
        assert_eq!(pts.first().copied(), Some([0.0, 0.0]));
        assert_eq!(pts.last().copied(), Some([40.0, 40.0]));
    }

    #[test]
    fn brush_smoothing_modes_reduce_jitter_and_stabilize() {
        let jittery = vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [20.0, 12.0],
            [30.0, 0.0],
            [40.0, 0.0],
        ];
        let basic = smooth_stroke_points(&jittery, BrushSmoothingMode::Basic, 24.0);
        assert_eq!(basic.len(), jittery.len());
        assert!(basic[2][1].abs() < jittery[2][1].abs());

        let weighted = smooth_stroke_points(&jittery, BrushSmoothingMode::Weighted, 24.0);
        assert_eq!(weighted.len(), jittery.len());
        assert!(weighted[2][1].abs() < jittery[2][1].abs());

        let long_stroke = vec![[0.0, 0.0], [5.0, 0.0], [10.0, 0.0], [50.0, 0.0]];
        let stab = smooth_stroke_points(&long_stroke, BrushSmoothingMode::Stabilizer, 20.0);
        assert_eq!(stab.len(), 2);
        assert_eq!(stab[0], [0.0, 0.0]);
        assert!((stab[1][0] - 30.0).abs() < 1e-6);
    }

    #[test]
    fn shape_stroke_and_fill_clipped_by_selection_mask() {
        let mut doc_img = RgbaImage::from_pixel(32, 32, Rgba([0, 0, 0, 255]));
        let mut sel = SelectionMask::empty(32, 32);
        let rect_alpha: Vec<u8> = (0..32 * 32)
            .map(|i| {
                let x = i % 32;
                let y = i / 32;
                if (8..24).contains(&x) && (8..24).contains(&y) {
                    255
                } else {
                    0
                }
            })
            .collect();
        sel.combine(&rect_alpha, SelectionBooleanOp::Replace);

        let drawn = draw_shape(
            &doc_img,
            DrawShapeKind::Rect,
            &[[2.0, 2.0], [30.0, 30.0]],
            &[],
            ShapeFillMode::StrokeAndFill,
            true,
            [200, 50, 50],
            2.0,
            1.0,
            BrushSettings::default(),
        );
        // Simulate SelectionMask clipping as Document does:
        for y in 0..32 {
            for x in 0..32 {
                if sel.alpha_at(x as i32, y as i32) == 255 {
                    doc_img.put_pixel(x, y, *drawn.get_pixel(x, y));
                }
            }
        }
        // Inside selection -> painted red
        assert_eq!(doc_img.get_pixel(16, 16).0, [200, 50, 50, 255]);
        // Outside selection -> untouched black
        assert_eq!(doc_img.get_pixel(4, 4).0, [0, 0, 0, 255]);
    }

    #[test]
    fn flood_fill_contiguous_and_non_contiguous_with_tolerance() {
        let mut img = RgbaImage::from_pixel(10, 10, Rgba([10, 10, 10, 255]));
        // Draw a vertical divider at x=5 with white
        for y in 0..10 {
            img.put_pixel(5, y, Rgba([255, 255, 255, 255]));
        }
        // Slightly off-color pixel at (2, 2)
        img.put_pixel(2, 2, Rgba([14, 10, 10, 255]));

        let filled_contig = flood_fill(&img, 1, 1, [0, 200, 0], 1.0, 5, true, None);
        assert_eq!(filled_contig.get_pixel(2, 2).0, [0, 200, 0, 255]);
        // Right side of divider must remain untouched
        assert_eq!(filled_contig.get_pixel(8, 5).0, [10, 10, 10, 255]);

        let filled_global = flood_fill(&img, 1, 1, [0, 200, 0], 1.0, 5, false, None);
        // Both sides of divider filled
        assert_eq!(filled_global.get_pixel(8, 5).0, [0, 200, 0, 255]);
        // White divider untouched
        assert_eq!(filled_global.get_pixel(5, 5).0, [255, 255, 255, 255]);
    }

    #[test]
    fn gradient_linear_radial_conical_reflected_and_transparent() {
        let base = RgbaImage::from_pixel(20, 10, Rgba([0, 0, 0, 255]));
        let lin = draw_gradient(
            &base,
            [0.0, 5.0],
            [20.0, 5.0],
            GradientKind::Linear,
            GradientColorMode::FgToBg,
            [255, 0, 0],
            [0, 0, 255],
            1.0,
            false,
            false,
            None,
        );
        assert!(lin.get_pixel(0, 5).0[0] > 230);
        assert!(lin.get_pixel(19, 5).0[2] > 230);

        let lin_rev = draw_gradient(
            &base,
            [0.0, 5.0],
            [20.0, 5.0],
            GradientKind::Linear,
            GradientColorMode::FgToBg,
            [255, 0, 0],
            [0, 0, 255],
            1.0,
            true,
            true,
            None,
        );
        assert!(lin_rev.get_pixel(0, 5).0[2] > 230);
        assert!(lin_rev.get_pixel(19, 5).0[0] > 230);

        let rad = draw_gradient(
            &base,
            [10.0, 5.0],
            [15.0, 5.0],
            GradientKind::Radial,
            GradientColorMode::FgToTransparent,
            [0, 255, 0],
            [0, 0, 0],
            1.0,
            false,
            false,
            None,
        );
        // Center is green, far corner stays black
        assert!(rad.get_pixel(10, 5).0[1] > 200);
        assert_eq!(rad.get_pixel(0, 0).0, [0, 0, 0, 255]);

        let con = draw_gradient(
            &base,
            [10.0, 5.0],
            [15.0, 5.0],
            GradientKind::Conical,
            GradientColorMode::FgToBg,
            [255, 255, 255],
            [0, 0, 0],
            1.0,
            false,
            false,
            None,
        );
        assert_ne!(con.get_pixel(12, 4).0, con.get_pixel(12, 6).0);

        let refl = draw_gradient(
            &base,
            [10.0, 5.0],
            [15.0, 5.0],
            GradientKind::Reflected,
            GradientColorMode::FgToBg,
            [255, 0, 0],
            [0, 0, 255],
            1.0,
            false,
            false,
            None,
        );
        // Symmetric around start x=10
        assert!(refl.get_pixel(10, 5).0[0] > 200);
        assert!(refl.get_pixel(2, 5).0[2] > 200);
        assert!(refl.get_pixel(18, 5).0[2] > 200);
    }

    #[test]
    fn gradient_selection_mask_clips_and_bayer_dither_varies_substeps() {
        let base = RgbaImage::from_pixel(32, 16, Rgba([10, 10, 10, 255]));
        let mut sel = SelectionMask::empty(32, 16);
        let mask: Vec<u8> = (0..32 * 16)
            .map(|i| {
                let x = i % 32;
                if (8..24).contains(&x) { 255 } else { 0 }
            })
            .collect();
        sel.combine(&mask, SelectionBooleanOp::Replace);

        let out = draw_gradient(
            &base,
            [0.0, 8.0],
            [32.0, 8.0],
            GradientKind::Linear,
            GradientColorMode::FgToBg,
            [200, 200, 200],
            [210, 210, 210],
            1.0,
            false,
            true,
            Some((&sel, 0, 0)),
        );
        // Outside selection stays untouched
        assert_eq!(out.get_pixel(2, 8).0, [10, 10, 10, 255]);
        assert_eq!(out.get_pixel(28, 8).0, [10, 10, 10, 255]);
        // Inside selection painted
        assert!(out.get_pixel(16, 8).0[0] >= 200);
    }

    #[test]
    fn multistop_shapes_repeat_blend_modes_and_mask_render() {
        let base = RgbaImage::from_pixel(32, 32, Rgba([100, 100, 100, 255]));
        let stops = [
            GradientStopSpec {
                pos: 1.0,
                color: [0, 0, 255],
                alpha: 1.0,
            },
            GradientStopSpec {
                pos: 0.0,
                color: [255, 0, 0],
                alpha: 1.0,
            },
            GradientStopSpec {
                pos: 0.5,
                color: [0, 255, 0],
                alpha: 1.0,
            },
        ];

        // 1. Multi-stop linear (unsorted input stops sorted internally)
        let lin = render_gradient(
            &base,
            [0.0, 16.0],
            [32.0, 16.0],
            [0, 0, 0],
            1.0,
            [0, 0, 0],
            1.0,
            &stops,
            1.0,
            GradientShapeSpec::Linear,
            GradientRepeatSpec::None,
            false,
            false,
            GradientBlendMode::Normal,
            None,
        );
        assert!(lin.get_pixel(0, 16).0[0] > 230);
        assert!(lin.get_pixel(16, 16).0[1] > 230);
        assert!(lin.get_pixel(31, 16).0[2] > 230);

        // 2. Square & Spiral & Bilinear shapes
        let sq = render_gradient(
            &base,
            [16.0, 16.0],
            [24.0, 16.0],
            [255, 255, 255],
            1.0,
            [0, 0, 0],
            1.0,
            &[],
            1.0,
            GradientShapeSpec::Square,
            GradientRepeatSpec::None,
            false,
            false,
            GradientBlendMode::Normal,
            None,
        );
        // Corners at equal Chebyshev distance (8 px from center) should match axis at 8 px
        assert_eq!(sq.get_pixel(24, 24).0, sq.get_pixel(24, 16).0);

        let sp = render_gradient(
            &base,
            [16.0, 16.0],
            [24.0, 16.0],
            [255, 0, 0],
            1.0,
            [0, 0, 255],
            1.0,
            &[],
            1.0,
            GradientShapeSpec::Spiral,
            GradientRepeatSpec::Forwards,
            false,
            false,
            GradientBlendMode::Normal,
            None,
        );
        assert_ne!(sp.get_pixel(18, 12).0, sp.get_pixel(22, 20).0);

        let bi = render_gradient(
            &base,
            [16.0, 16.0],
            [24.0, 16.0],
            [255, 0, 0],
            1.0,
            [0, 0, 255],
            1.0,
            &[],
            1.0,
            GradientShapeSpec::Bilinear,
            GradientRepeatSpec::Alternating,
            false,
            false,
            GradientBlendMode::Normal,
            None,
        );
        // Bilinear is symmetric around start x=16 (pixel centers 11.5 and 20.5 are equidistant)
        assert_eq!(bi.get_pixel(11, 16).0, bi.get_pixel(20, 16).0);
        assert!(bi.get_pixel(31, 16).0[0] > bi.get_pixel(24, 16).0[0]);

        // 3. Blend modes (Multiply darkens 100 with 128 -> ~50; Difference |100-200|=100)
        let mul = render_gradient(
            &base,
            [0.0, 0.0],
            [32.0, 0.0],
            [128, 128, 128],
            1.0,
            [128, 128, 128],
            1.0,
            &[],
            1.0,
            GradientShapeSpec::Linear,
            GradientRepeatSpec::None,
            false,
            false,
            GradientBlendMode::Multiply,
            None,
        );
        assert!((48..=53).contains(&mul.get_pixel(16, 16).0[0]));

        let diff = render_gradient(
            &base,
            [0.0, 0.0],
            [32.0, 0.0],
            [200, 200, 200],
            1.0,
            [200, 200, 200],
            1.0,
            &[],
            1.0,
            GradientShapeSpec::Linear,
            GradientRepeatSpec::None,
            false,
            false,
            GradientBlendMode::Difference,
            None,
        );
        assert!((98..=102).contains(&diff.get_pixel(16, 16).0[0]));

        // 4. Mask luminance rendering + selection clipping
        let mut sel = SelectionMask::empty(16, 16);
        let sel_mask: Vec<u8> = (0..16 * 16)
            .map(|i| if (i % 16) >= 8 { 255 } else { 0 })
            .collect();
        sel.combine(&sel_mask, SelectionBooleanOp::Replace);
        let init_mask = vec![255u8; 16 * 16];
        let next_mask = render_gradient_on_mask(
            &init_mask,
            16,
            16,
            [0.0, 8.0],
            [16.0, 8.0],
            [0, 0, 0],
            1.0,
            [255, 255, 255],
            1.0,
            &[],
            1.0,
            GradientShapeSpec::Linear,
            GradientRepeatSpec::None,
            false,
            false,
            Some((&sel, 0, 0)),
        );
        // Left half untouched (255), right half transitions toward 255 from ~128
        assert_eq!(next_mask[8 * 16 + 2], 255);
        assert!(next_mask[8 * 16 + 9] < 200);
        assert!(next_mask[8 * 16 + 15] > 230);
    }
}
