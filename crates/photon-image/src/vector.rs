//! Non-destructive Vector Layer rendering, Bezier path node operations,
//! `Transform Each` (with 9-point anchor, repeat N steps, and apply-to-duplicate),
//! `Vector Blend` (shape/path interpolation), and SVG export.

use std::fmt::Write as _;

use image::{Rgba, RgbaImage};
use photon_protocol::{
    CanvasAnchor, TransformEachSpec, VectorBlendSpec, VectorGeometry, VectorHandleKind,
    VectorLayerData, VectorNodeMode, VectorPathNode, VectorShape, VectorStroke,
};

use crate::ImageError;
use crate::drawing::{fill_polygon_aa, stroke_polyline_aa, tessellate_bezier_path};

/// Cubic Bezier circle approximation constant (`4 * (sqrt(2) - 1) / 3`).
const KAPPA: f32 = 0.552_284_8;

/// Convert any [`VectorGeometry`] (`Path`, `Rect`, `Ellipse`, `Polygon`) into an equivalent
/// cubic Bezier node sequence `(nodes, closed)`.
pub fn geometry_to_bezier_path(geometry: &VectorGeometry) -> (Vec<VectorPathNode>, bool) {
    match geometry {
        VectorGeometry::Path { nodes, closed } => (nodes.clone(), *closed),
        VectorGeometry::Polygon { points, closed } => (
            points.iter().map(|&p| VectorPathNode::corner(p)).collect(),
            *closed,
        ),
        VectorGeometry::Ellipse { cx, cy, rx, ry } => {
            let rx = rx.abs();
            let ry = ry.abs();
            if rx <= 1e-5 && ry <= 1e-5 {
                return (Vec::new(), true);
            }
            let kx = rx * KAPPA;
            let ky = ry * KAPPA;
            let nodes = vec![
                VectorPathNode::new(
                    [*cx + rx, *cy],
                    Some([*cx + rx, *cy - ky]),
                    Some([*cx + rx, *cy + ky]),
                ),
                VectorPathNode::new(
                    [*cx, *cy + ry],
                    Some([*cx + kx, *cy + ry]),
                    Some([*cx - kx, *cy + ry]),
                ),
                VectorPathNode::new(
                    [*cx - rx, *cy],
                    Some([*cx - rx, *cy + ky]),
                    Some([*cx - rx, *cy - ky]),
                ),
                VectorPathNode::new(
                    [*cx, *cy - ry],
                    Some([*cx - kx, *cy - ry]),
                    Some([*cx + kx, *cy - ry]),
                ),
            ];
            (nodes, true)
        }
        VectorGeometry::Rect {
            x,
            y,
            w,
            h,
            corner_radius,
        } => {
            let x0 = x.min(x + w);
            let y0 = y.min(y + h);
            let rw = w.abs();
            let rh = h.abs();
            if rw <= 1e-5 && rh <= 1e-5 {
                return (Vec::new(), true);
            }
            let x1 = x0 + rw;
            let y1 = y0 + rh;
            let r = corner_radius.max(0.0).min(rw * 0.5).min(rh * 0.5);
            if r <= 1e-4 {
                let nodes = vec![
                    VectorPathNode::corner([x0, y0]),
                    VectorPathNode::corner([x1, y0]),
                    VectorPathNode::corner([x1, y1]),
                    VectorPathNode::corner([x0, y1]),
                ];
                return (nodes, true);
            }
            let k = r * KAPPA;
            let nodes = vec![
                VectorPathNode::new([x0 + r, y0], Some([x0 + r - k, y0]), None),
                VectorPathNode::new([x1 - r, y0], None, Some([x1 - r + k, y0])),
                VectorPathNode::new([x1, y0 + r], Some([x1, y0 + r - k]), None),
                VectorPathNode::new([x1, y1 - r], None, Some([x1, y1 - r + k])),
                VectorPathNode::new([x1 - r, y1], Some([x1 - r + k, y1]), None),
                VectorPathNode::new([x0 + r, y1], None, Some([x0 + r - k, y1])),
                VectorPathNode::new([x0, y1 - r], Some([x0, y1 - r + k]), None),
                VectorPathNode::new([x0, y0 + r], None, Some([x0, y0 + r - k])),
            ];
            (nodes, true)
        }
    }
}

/// Convert any [`VectorGeometry`] into a [`VectorGeometry::Path`].
pub fn convert_geometry_to_path(geometry: &VectorGeometry) -> VectorGeometry {
    let (nodes, closed) = geometry_to_bezier_path(geometry);
    VectorGeometry::Path { nodes, closed }
}

/// Convert a [`VectorShape`]'s geometry in-place to [`VectorGeometry::Path`].
/// Returns `true` if the shape was converted from a parametric primitive (`Rect`/`Ellipse`/`Polygon`).
pub fn convert_shape_to_path(shape: &mut VectorShape) -> bool {
    if matches!(shape.geometry, VectorGeometry::Path { .. }) {
        return false;
    }
    shape.geometry = convert_geometry_to_path(&shape.geometry);
    true
}

/// Tessellate a [`VectorGeometry`] into a polyline `(points, closed)` with the requested tolerance.
pub fn geometry_to_polyline(geometry: &VectorGeometry, tolerance: f64) -> (Vec<[f64; 2]>, bool) {
    let (nodes, closed) = geometry_to_bezier_path(geometry);
    if nodes.is_empty() {
        return (Vec::new(), closed);
    }
    (tessellate_bezier_path(&nodes, closed, tolerance), closed)
}

/// Compute the axis-aligned bounding box `[min_x, min_y, max_x, max_y]` of a [`VectorShape`].
pub fn shape_bounds(shape: &VectorShape) -> Option<[f32; 4]> {
    match &shape.geometry {
        VectorGeometry::Rect { x, y, w, h, .. } => {
            let min_x = x.min(x + w);
            let min_y = y.min(y + h);
            let max_x = x.max(x + w);
            let max_y = y.max(y + h);
            Some([min_x, min_y, max_x, max_y])
        }
        VectorGeometry::Ellipse { cx, cy, rx, ry } => {
            let rx = rx.abs();
            let ry = ry.abs();
            Some([cx - rx, cy - ry, cx + rx, cy + ry])
        }
        VectorGeometry::Polygon { points, .. } => {
            let first = *points.first()?;
            let mut min_x = first[0];
            let mut min_y = first[1];
            let mut max_x = first[0];
            let mut max_y = first[1];
            for &[x, y] in &points[1..] {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
            Some([min_x, min_y, max_x, max_y])
        }
        VectorGeometry::Path { nodes, closed } => {
            if nodes.is_empty() {
                return None;
            }
            let poly = tessellate_bezier_path(nodes, *closed, 0.25);
            let first = *poly.first()?;
            let mut min_x = first[0] as f32;
            let mut min_y = first[1] as f32;
            let mut max_x = min_x;
            let mut max_y = min_y;
            for &[x, y] in &poly[1..] {
                let xf = x as f32;
                let yf = y as f32;
                min_x = min_x.min(xf);
                min_y = min_y.min(yf);
                max_x = max_x.max(xf);
                max_y = max_y.max(yf);
            }
            Some([min_x, min_y, max_x, max_y])
        }
    }
}

/// Resolve one of the 9 [`CanvasAnchor`] points on a `[min_x, min_y, max_x, max_y]` bounding box.
pub fn anchor_point_for_bounds(bounds: [f32; 4], anchor: CanvasAnchor) -> [f32; 2] {
    let [min_x, min_y, max_x, max_y] = bounds;
    let mid_x = (min_x + max_x) * 0.5;
    let mid_y = (min_y + max_y) * 0.5;
    let x = match anchor {
        CanvasAnchor::TopLeft | CanvasAnchor::Left | CanvasAnchor::BottomLeft => min_x,
        CanvasAnchor::Top | CanvasAnchor::Center | CanvasAnchor::Bottom => mid_x,
        CanvasAnchor::TopRight | CanvasAnchor::Right | CanvasAnchor::BottomRight => max_x,
    };
    let y = match anchor {
        CanvasAnchor::TopLeft | CanvasAnchor::Top | CanvasAnchor::TopRight => min_y,
        CanvasAnchor::Left | CanvasAnchor::Center | CanvasAnchor::Right => mid_y,
        CanvasAnchor::BottomLeft | CanvasAnchor::Bottom | CanvasAnchor::BottomRight => max_y,
    };
    [x, y]
}

/// Render a single [`VectorShape`] (fill + stroke) onto an existing [`RgbaImage`].
pub fn render_vector_shape_onto(dst: &mut RgbaImage, shape: &VectorShape) {
    if !shape.visible || shape.opacity <= 1e-4 {
        return;
    }
    let opacity = shape.opacity.clamp(0.0, 1.0);
    let (poly, closed) = geometry_to_polyline(&shape.geometry, 0.25);
    if poly.is_empty() {
        return;
    }
    if let Some(fill_color) = shape.fill
        && poly.len() >= 3
    {
        fill_polygon_aa(dst, &poly, fill_color, opacity);
    }
    if shape.stroke.is_visible() {
        stroke_polyline_aa(dst, &poly, closed, shape.stroke, opacity);
    }
}

/// Render all visible shapes in a [`VectorLayerData`] onto a transparent `canvas_width x canvas_height` [`RgbaImage`].
pub fn render_vector_layer(
    data: &VectorLayerData,
    canvas_width: u32,
    canvas_height: u32,
) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(
        canvas_width.max(1),
        canvas_height.max(1),
        Rgba([0, 0, 0, 0]),
    );
    for shape in &data.shapes {
        render_vector_shape_onto(&mut img, shape);
    }
    img
}

// ============================================================================
// Node Operations (Add on Segment, Delete, Split, Join, Close/Open, Reverse,
// Node Mode Corner/Smooth/Symmetric, Simplify, Smooth)
// ============================================================================

#[inline]
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
fn lerp_pt(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [lerp_f32(a[0], b[0], t), lerp_f32(a[1], b[1], t)]
}

#[inline]
fn eval_cubic(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    let uu = u * u;
    let uuu = uu * u;
    let tt = t * t;
    let ttt = tt * t;
    [
        uuu * p0[0] + 3.0 * uu * t * p1[0] + 3.0 * u * tt * p2[0] + ttt * p3[0],
        uuu * p0[1] + 3.0 * uu * t * p1[1] + 3.0 * u * tt * p2[1] + ttt * p3[1],
    ]
}

/// Subdivide segment `segment_index` (between `nodes[segment_index]` and the next node)
/// at parameter `t` in `(0.0, 1.0)` using exact De Casteljau subdivision.
/// Returns the index of the newly inserted node.
pub fn add_node_on_segment(
    nodes: &mut Vec<VectorPathNode>,
    closed: bool,
    segment_index: usize,
    t: f32,
) -> Option<usize> {
    if nodes.len() < 2 {
        return None;
    }
    let seg_count = if closed { nodes.len() } else { nodes.len() - 1 };
    if segment_index >= seg_count {
        return None;
    }
    let t = t.clamp(0.05, 0.95);
    let next_idx = (segment_index + 1) % nodes.len();
    let a = nodes[segment_index];
    let b = nodes[next_idx];
    let p0 = a.anchor;
    let p3 = b.anchor;
    let is_line = a.handle_out.is_none() && b.handle_in.is_none();

    let new_node = if is_line {
        VectorPathNode::corner(lerp_pt(p0, p3, t))
    } else {
        let p1 = a.handle_out.unwrap_or(p0);
        let p2 = b.handle_in.unwrap_or(p3);
        let q0 = lerp_pt(p0, p1, t);
        let q1 = lerp_pt(p1, p2, t);
        let q2 = lerp_pt(p2, p3, t);
        let r0 = lerp_pt(q0, q1, t);
        let r1 = lerp_pt(q1, q2, t);
        let s = lerp_pt(r0, r1, t);
        nodes[segment_index].handle_out = Some(q0);
        nodes[next_idx].handle_in = Some(q2);
        VectorPathNode::new(s, Some(r0), Some(r1))
    };

    let insert_idx = segment_index + 1;
    nodes.insert(insert_idx, new_node);
    Some(insert_idx)
}

/// Find the closest segment on the Bezier path to `point` within `max_distance`
/// and insert a subdivided node there. Returns the inserted node index.
pub fn add_node_at_closest_point(
    nodes: &mut Vec<VectorPathNode>,
    closed: bool,
    point: [f32; 2],
    max_distance: f32,
) -> Option<usize> {
    if nodes.len() < 2 {
        return None;
    }
    let seg_count = if closed { nodes.len() } else { nodes.len() - 1 };
    let mut best_seg = None;
    let mut best_t = 0.5f32;
    let mut best_dist = max_distance.max(0.5);

    const SAMPLES: usize = 32;
    for seg_idx in 0..seg_count {
        let a = nodes[seg_idx];
        let b = nodes[(seg_idx + 1) % nodes.len()];
        let p0 = a.anchor;
        let p1 = a.handle_out.unwrap_or(p0);
        let p2 = b.handle_in.unwrap_or(b.anchor);
        let p3 = b.anchor;
        for s in 1..SAMPLES {
            let t = s as f32 / SAMPLES as f32;
            let pt = eval_cubic(p0, p1, p2, p3, t);
            let d = (pt[0] - point[0]).hypot(pt[1] - point[1]);
            if d <= best_dist {
                best_dist = d;
                best_seg = Some(seg_idx);
                best_t = t;
            }
        }
    }

    let seg_idx = best_seg?;
    add_node_on_segment(nodes, closed, seg_idx, best_t)
}

/// Delete a node at `index` from `nodes`. Returns the removed node if `index < nodes.len()`.
pub fn delete_node(nodes: &mut Vec<VectorPathNode>, index: usize) -> Option<VectorPathNode> {
    if index >= nodes.len() {
        return None;
    }
    Some(nodes.remove(index))
}

/// Delete multiple node indices from `nodes` (order-independent). Returns how many nodes were removed.
pub fn delete_nodes(nodes: &mut Vec<VectorPathNode>, indices: &[usize]) -> usize {
    let mut sorted: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|&i| i < nodes.len())
        .collect();
    sorted.sort_unstable();
    sorted.dedup();
    let mut count = 0;
    for &idx in sorted.iter().rev() {
        nodes.remove(idx);
        count += 1;
    }
    count
}

/// Split a path `(nodes, closed)` at `index`:
/// - If `closed` is `true`, opens the loop at `index` into a single open path starting and ending at `nodes[index].anchor`.
/// - If `closed` is `false` and `0 < index < nodes.len() - 1`, splits into two open subpaths `nodes[..=index]` and `nodes[index..]`.
pub fn split_path_at_node(
    nodes: &[VectorPathNode],
    closed: bool,
    index: usize,
) -> Vec<(Vec<VectorPathNode>, bool)> {
    if nodes.len() < 2 || index >= nodes.len() {
        return vec![(nodes.to_vec(), closed)];
    }
    if closed {
        let n = nodes.len();
        let mut opened = Vec::with_capacity(n + 1);
        for k in 0..=n {
            let mut node = nodes[(index + k) % n];
            if k == 0 {
                node.handle_in = None;
            } else if k == n {
                node.handle_out = None;
            }
            opened.push(node);
        }
        return vec![(opened, false)];
    }

    if index == 0 || index == nodes.len() - 1 {
        return vec![(nodes.to_vec(), false)];
    }

    let mut first = nodes[..=index].to_vec();
    if let Some(last) = first.last_mut() {
        last.handle_out = None;
    }
    let mut second = nodes[index..].to_vec();
    if let Some(head) = second.first_mut() {
        head.handle_in = None;
    }
    vec![(first, false), (second, false)]
}

/// Split a [`VectorShape`] at node `index`, returning one or two resulting [`VectorShape`]s.
pub fn split_shape_at_node(
    shape: &VectorShape,
    index: usize,
    next_id: &mut u64,
) -> Vec<VectorShape> {
    let (nodes, closed) = geometry_to_bezier_path(&shape.geometry);
    let parts = split_path_at_node(&nodes, closed, index);
    parts
        .into_iter()
        .enumerate()
        .map(|(i, (part_nodes, part_closed))| {
            let id = if i == 0 {
                shape.id
            } else {
                let id = *next_id;
                *next_id += 1;
                id
            };
            let name = if i == 0 {
                shape.name.clone()
            } else {
                format!("{} (split)", shape.name)
            };
            VectorShape {
                id,
                name,
                visible: shape.visible,
                opacity: shape.opacity,
                geometry: VectorGeometry::Path {
                    nodes: part_nodes,
                    closed: part_closed,
                },
                fill: shape.fill,
                stroke: shape.stroke,
            }
        })
        .collect()
}

/// Join two open node sequences `path_a` and `path_b` by connecting their closest endpoints.
/// If the closest endpoints are within `tolerance`, they are welded into a single anchor node
/// preserving the incoming and outgoing Bezier handles.
pub fn join_paths(
    path_a: &[VectorPathNode],
    path_b: &[VectorPathNode],
    tolerance: f32,
) -> Vec<VectorPathNode> {
    if path_a.is_empty() {
        return path_b.to_vec();
    }
    if path_b.is_empty() {
        return path_a.to_vec();
    }

    let a_start = path_a[0].anchor;
    let a_end = path_a[path_a.len() - 1].anchor;
    let b_start = path_b[0].anchor;
    let b_end = path_b[path_b.len() - 1].anchor;

    let dist = |p: [f32; 2], q: [f32; 2]| (p[0] - q[0]).hypot(p[1] - q[1]);
    let candidates = [
        (dist(a_end, b_start), false, false),
        (dist(a_end, b_end), false, true),
        (dist(a_start, b_start), true, false),
        (dist(a_start, b_end), true, true),
    ];
    let &(min_dist, rev_a, rev_b) = candidates
        .iter()
        .min_by(|x, y| x.0.total_cmp(&y.0))
        .expect("4 candidates");

    let mut a = path_a.to_vec();
    let mut b = path_b.to_vec();
    if rev_a {
        reverse_path_nodes(&mut a);
    }
    if rev_b {
        reverse_path_nodes(&mut b);
    }

    if min_dist <= tolerance.max(1e-3)
        && let (Some(last_a), Some(first_b)) = (a.last_mut(), b.first())
    {
        last_a.anchor = [
            (last_a.anchor[0] + first_b.anchor[0]) * 0.5,
            (last_a.anchor[1] + first_b.anchor[1]) * 0.5,
        ];
        last_a.handle_out = first_b.handle_out;
        a.extend_from_slice(&b[1..]);
        return a;
    }

    a.extend_from_slice(&b);
    a
}

/// Join two [`VectorShape`]s into a single open [`VectorShape`] retaining `shape_a`'s styling and ID.
pub fn join_shapes(shape_a: &VectorShape, shape_b: &VectorShape, tolerance: f32) -> VectorShape {
    let (nodes_a, _) = geometry_to_bezier_path(&shape_a.geometry);
    let (nodes_b, _) = geometry_to_bezier_path(&shape_b.geometry);
    let joined = join_paths(&nodes_a, &nodes_b, tolerance);
    VectorShape {
        id: shape_a.id,
        name: shape_a.name.clone(),
        visible: shape_a.visible,
        opacity: shape_a.opacity,
        geometry: VectorGeometry::Path {
            nodes: joined,
            closed: false,
        },
        fill: shape_a.fill,
        stroke: shape_a.stroke,
    }
}

/// Set whether a [`VectorGeometry`] path/polygon is closed (`true`) or open (`false`).
/// Converts `Rect` or `Ellipse` to `VectorGeometry::Path` first if `closed == false`.
pub fn set_path_closed(geometry: &mut VectorGeometry, closed: bool) -> bool {
    match geometry {
        VectorGeometry::Path {
            closed: cur_closed, ..
        }
        | VectorGeometry::Polygon {
            closed: cur_closed, ..
        } => {
            if *cur_closed == closed {
                return false;
            }
            *cur_closed = closed;
            true
        }
        VectorGeometry::Rect { .. } | VectorGeometry::Ellipse { .. } => {
            if closed {
                return false;
            }
            let (nodes, _) = geometry_to_bezier_path(geometry);
            *geometry = VectorGeometry::Path {
                nodes,
                closed: false,
            };
            true
        }
    }
}

/// Toggle the closed/open state of a [`VectorGeometry`]. Returns the new `closed` value.
pub fn toggle_path_closed(geometry: &mut VectorGeometry) -> bool {
    let next = !geometry.is_closed();
    set_path_closed(geometry, next);
    next
}

/// Reverse the winding direction of a [`VectorPathNode`] slice in-place,
/// swapping `handle_in` and `handle_out` on each node so curve geometry is preserved.
pub fn reverse_path_nodes(nodes: &mut [VectorPathNode]) {
    nodes.reverse();
    for node in nodes.iter_mut() {
        std::mem::swap(&mut node.handle_in, &mut node.handle_out);
    }
}

/// Reverse the path direction of a [`VectorShape`], converting parametric primitives to a path first.
pub fn reverse_shape_path(shape: &mut VectorShape) -> bool {
    match &mut shape.geometry {
        VectorGeometry::Path { nodes, .. } => {
            if nodes.len() < 2 {
                return false;
            }
            reverse_path_nodes(nodes);
            true
        }
        VectorGeometry::Polygon { points, .. } => {
            if points.len() < 2 {
                return false;
            }
            points.reverse();
            true
        }
        VectorGeometry::Rect { .. } | VectorGeometry::Ellipse { .. } => {
            let (mut nodes, closed) = geometry_to_bezier_path(&shape.geometry);
            reverse_path_nodes(&mut nodes);
            shape.geometry = VectorGeometry::Path { nodes, closed };
            true
        }
    }
}

/// Compute a default outgoing tangent vector `[tx, ty]` for node `index` based on neighboring anchors.
fn default_node_tangent(nodes: &[VectorPathNode], closed: bool, index: usize) -> [f32; 2] {
    let n = nodes.len();
    let cur = nodes[index].anchor;
    if n < 2 {
        return [20.0, 0.0];
    }
    let prev = if index > 0 {
        Some(nodes[index - 1].anchor)
    } else if closed {
        Some(nodes[n - 1].anchor)
    } else {
        None
    };
    let next = if index + 1 < n {
        Some(nodes[index + 1].anchor)
    } else if closed {
        Some(nodes[0].anchor)
    } else {
        None
    };

    match (prev, next) {
        (Some(p), Some(q)) => {
            let dx = q[0] - p[0];
            let dy = q[1] - p[1];
            let len = dx.hypot(dy);
            if len > 1e-4 {
                let d_prev = (cur[0] - p[0]).hypot(cur[1] - p[1]);
                let d_next = (q[0] - cur[0]).hypot(q[1] - cur[1]);
                let handle_len = (d_prev.min(d_next) * 0.33).max(4.0);
                [dx / len * handle_len, dy / len * handle_len]
            } else {
                [20.0, 0.0]
            }
        }
        (None, Some(q)) => {
            let dx = q[0] - cur[0];
            let dy = q[1] - cur[1];
            [dx * 0.33, dy * 0.33]
        }
        (Some(p), None) => {
            let dx = cur[0] - p[0];
            let dy = cur[1] - p[1];
            [dx * 0.33, dy * 0.33]
        }
        (None, None) => [20.0, 0.0],
    }
}

/// Convert node `index` to [`VectorNodeMode`] (`Corner`, `Smooth`, or `Symmetric`):
/// - `Corner`: clears `handle_in` and `handle_out` to `None` (sharp corner).
/// - `Smooth`: aligns `handle_in` and `handle_out` collinearly (180 degrees apart) while preserving their individual lengths.
/// - `Symmetric`: aligns `handle_in` and `handle_out` collinearly with equal average length.
pub fn set_node_mode(
    nodes: &mut [VectorPathNode],
    closed: bool,
    index: usize,
    mode: VectorNodeMode,
) -> bool {
    if index >= nodes.len() {
        return false;
    }
    let anchor = nodes[index].anchor;
    match mode {
        VectorNodeMode::Corner => {
            nodes[index].handle_in = None;
            nodes[index].handle_out = None;
            true
        }
        VectorNodeMode::Smooth | VectorNodeMode::Symmetric => {
            let fallback = default_node_tangent(nodes, closed, index);
            let fallback_len = fallback[0].hypot(fallback[1]).max(4.0);

            let vin = nodes[index]
                .handle_in
                .map(|h| [h[0] - anchor[0], h[1] - anchor[1]]);
            let vout = nodes[index]
                .handle_out
                .map(|h| [h[0] - anchor[0], h[1] - anchor[1]]);

            // Determine unit outgoing tangent direction `(ux, uy)` and lengths `(len_in, len_out)`.
            let (ux, uy, mut len_in, mut len_out) = match (vin, vout) {
                (Some(vi), Some(vo)) => {
                    let li = vi[0].hypot(vi[1]);
                    let lo = vo[0].hypot(vo[1]);
                    let tx = vo[0] - vi[0];
                    let ty = vo[1] - vi[1];
                    let tlen = tx.hypot(ty);
                    if tlen > 1e-4 {
                        (
                            tx / tlen,
                            ty / tlen,
                            li.max(fallback_len),
                            lo.max(fallback_len),
                        )
                    } else {
                        (
                            fallback[0] / fallback_len,
                            fallback[1] / fallback_len,
                            fallback_len,
                            fallback_len,
                        )
                    }
                }
                (None, Some(vo)) => {
                    let lo = vo[0].hypot(vo[1]).max(4.0);
                    (vo[0] / lo, vo[1] / lo, lo, lo)
                }
                (Some(vi), None) => {
                    let li = vi[0].hypot(vi[1]).max(4.0);
                    (-vi[0] / li, -vi[1] / li, li, li)
                }
                (None, None) => (
                    fallback[0] / fallback_len,
                    fallback[1] / fallback_len,
                    fallback_len,
                    fallback_len,
                ),
            };

            if mode == VectorNodeMode::Symmetric {
                let avg = (len_in + len_out) * 0.5;
                len_in = avg;
                len_out = avg;
            }

            nodes[index].handle_in = Some([anchor[0] - ux * len_in, anchor[1] - uy * len_in]);
            nodes[index].handle_out = Some([anchor[0] + ux * len_out, anchor[1] + uy * len_out]);
            true
        }
    }
}

/// Move node `index`'s anchor to `new_anchor`, translating its `handle_in` and `handle_out` by the same delta.
pub fn move_node_anchor(nodes: &mut [VectorPathNode], index: usize, new_anchor: [f32; 2]) -> bool {
    let Some(node) = nodes.get_mut(index) else {
        return false;
    };
    let dx = new_anchor[0] - node.anchor[0];
    let dy = new_anchor[1] - node.anchor[1];
    node.anchor = new_anchor;
    if let Some(h_in) = &mut node.handle_in {
        h_in[0] += dx;
        h_in[1] += dy;
    }
    if let Some(h_out) = &mut node.handle_out {
        h_out[0] += dx;
        h_out[1] += dy;
    }
    true
}

/// Move `handle` (`HandleIn` or `HandleOut`) of node `index` to `new_pos`, enforcing `mode`
/// (`Corner`: independent; `Smooth`: opposite handle stays collinear with its own length;
/// `Symmetric`: opposite handle mirrors with equal length).
pub fn move_node_handle(
    nodes: &mut [VectorPathNode],
    index: usize,
    handle: VectorHandleKind,
    new_pos: [f32; 2],
    mode: VectorNodeMode,
) -> bool {
    let Some(node) = nodes.get_mut(index) else {
        return false;
    };
    let anchor = node.anchor;
    let vx = new_pos[0] - anchor[0];
    let vy = new_pos[1] - anchor[1];
    let len = vx.hypot(vy);

    match handle {
        VectorHandleKind::HandleIn => {
            node.handle_in = Some(new_pos);
            if len > 1e-4 {
                match mode {
                    VectorNodeMode::Corner => {}
                    VectorNodeMode::Symmetric => {
                        node.handle_out = Some([anchor[0] - vx, anchor[1] - vy]);
                    }
                    VectorNodeMode::Smooth => {
                        let out_len = node
                            .handle_out
                            .map(|h| (h[0] - anchor[0]).hypot(h[1] - anchor[1]))
                            .unwrap_or(len)
                            .max(1.0);
                        node.handle_out = Some([
                            anchor[0] - vx / len * out_len,
                            anchor[1] - vy / len * out_len,
                        ]);
                    }
                }
            }
        }
        VectorHandleKind::HandleOut => {
            node.handle_out = Some(new_pos);
            if len > 1e-4 {
                match mode {
                    VectorNodeMode::Corner => {}
                    VectorNodeMode::Symmetric => {
                        node.handle_in = Some([anchor[0] - vx, anchor[1] - vy]);
                    }
                    VectorNodeMode::Smooth => {
                        let in_len = node
                            .handle_in
                            .map(|h| (h[0] - anchor[0]).hypot(h[1] - anchor[1]))
                            .unwrap_or(len)
                            .max(1.0);
                        node.handle_in =
                            Some([anchor[0] - vx / len * in_len, anchor[1] - vy / len * in_len]);
                    }
                }
            }
        }
    }
    true
}

fn point_to_segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len_sq = dx * dx + dy * dy;
    if len_sq <= 1e-8 {
        return (p[0] - a[0]).hypot(p[1] - a[1]);
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len_sq).clamp(0.0, 1.0);
    let proj = [a[0] + t * dx, a[1] + t * dy];
    (p[0] - proj[0]).hypot(p[1] - proj[1])
}

fn rdp_keep_flags(
    nodes: &[VectorPathNode],
    first: usize,
    last: usize,
    tol: f32,
    keep: &mut [bool],
) {
    if last <= first + 1 {
        return;
    }
    let a = nodes[first].anchor;
    let b = nodes[last].anchor;
    let mut max_dist = -1.0f32;
    let mut split_idx = first;
    for i in (first + 1)..last {
        let d = point_to_segment_distance(nodes[i].anchor, a, b);
        if d > max_dist {
            max_dist = d;
            split_idx = i;
        }
    }
    if max_dist > tol {
        keep[split_idx] = true;
        rdp_keep_flags(nodes, first, split_idx, tol, keep);
        rdp_keep_flags(nodes, split_idx, last, tol, keep);
    }
}

/// Simplify a [`VectorPathNode`] sequence using Ramer-Douglas-Peucker with `tolerance` (in pixels).
/// Preserves at least 2 nodes for open paths and 3 nodes for closed paths.
pub fn simplify_path_nodes(
    nodes: &[VectorPathNode],
    closed: bool,
    tolerance: f32,
) -> Vec<VectorPathNode> {
    let min_nodes = if closed { 3 } else { 2 };
    if nodes.len() <= min_nodes {
        return nodes.to_vec();
    }
    let tol = tolerance.max(0.1);
    let mut keep = vec![false; nodes.len()];
    keep[0] = true;
    keep[nodes.len() - 1] = true;
    rdp_keep_flags(nodes, 0, nodes.len() - 1, tol, &mut keep);

    let mut out: Vec<VectorPathNode> = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, &n)| keep[i].then_some(n))
        .collect();

    if closed && out.len() < 3 {
        let mid = nodes.len() / 2;
        out = vec![nodes[0], nodes[mid], nodes[nodes.len() - 1]];
    }
    out
}

/// Simplify a [`VectorShape`]'s path nodes within `tolerance` pixels.
pub fn simplify_shape(shape: &mut VectorShape, tolerance: f32) -> bool {
    let (nodes, closed) = geometry_to_bezier_path(&shape.geometry);
    if nodes.len() <= 2 {
        return false;
    }
    let simplified = simplify_path_nodes(&nodes, closed, tolerance);
    let changed =
        simplified.len() != nodes.len() || !matches!(shape.geometry, VectorGeometry::Path { .. });
    shape.geometry = VectorGeometry::Path {
        nodes: simplified,
        closed,
    };
    changed
}

/// Compute smooth Catmull-Rom cubic Bezier control handles (`handle_in`, `handle_out`)
/// for all nodes in `nodes` using `tension` (`0.0..=1.0`, typical `0.33`).
pub fn smooth_path_nodes(nodes: &mut [VectorPathNode], closed: bool, tension: f32) {
    let n = nodes.len();
    if n < 2 {
        return;
    }
    let factor = tension.clamp(0.05, 1.0) * 0.5;
    let anchors: Vec<[f32; 2]> = nodes.iter().map(|node| node.anchor).collect();

    for i in 0..n {
        if !closed && (i == 0 || i == n - 1) {
            if i == 0 {
                let dx = anchors[1][0] - anchors[0][0];
                let dy = anchors[1][1] - anchors[0][1];
                nodes[0].handle_in = None;
                nodes[0].handle_out =
                    Some([anchors[0][0] + dx * factor, anchors[0][1] + dy * factor]);
            } else {
                let dx = anchors[n - 1][0] - anchors[n - 2][0];
                let dy = anchors[n - 1][1] - anchors[n - 2][1];
                nodes[n - 1].handle_in = Some([
                    anchors[n - 1][0] - dx * factor,
                    anchors[n - 1][1] - dy * factor,
                ]);
                nodes[n - 1].handle_out = None;
            }
            continue;
        }

        let prev = anchors[(i + n - 1) % n];
        let cur = anchors[i];
        let next = anchors[(i + 1) % n];
        let tx = next[0] - prev[0];
        let ty = next[1] - prev[1];
        let tlen = tx.hypot(ty);
        if tlen <= 1e-4 {
            nodes[i].handle_in = None;
            nodes[i].handle_out = None;
            continue;
        }
        let ux = tx / tlen;
        let uy = ty / tlen;
        let d_in = (cur[0] - prev[0]).hypot(cur[1] - prev[1]) * factor;
        let d_out = (next[0] - cur[0]).hypot(next[1] - cur[1]) * factor;
        nodes[i].handle_in = Some([cur[0] - ux * d_in, cur[1] - uy * d_in]);
        nodes[i].handle_out = Some([cur[0] + ux * d_out, cur[1] + uy * d_out]);
    }
}

/// Smooth a [`VectorShape`]'s path nodes with Catmull-Rom handles (`tension` in `0.05..=1.0`).
pub fn smooth_shape(shape: &mut VectorShape, tension: f32) -> bool {
    let (mut nodes, closed) = geometry_to_bezier_path(&shape.geometry);
    if nodes.len() < 2 {
        return false;
    }
    smooth_path_nodes(&mut nodes, closed, tension);
    shape.geometry = VectorGeometry::Path { nodes, closed };
    true
}

// ============================================================================
// Transform Each (Scale, Rotate, Move, Reflect around each object's 9-point
// anchor, with Repeat N Steps and Apply to Duplicate)
// ============================================================================

#[inline]
fn transform_point_around_pivot(
    pt: [f32; 2],
    pivot: [f32; 2],
    sx: f32,
    sy: f32,
    cos_a: f32,
    sin_a: f32,
    tx: f32,
    ty: f32,
) -> [f32; 2] {
    let dx = (pt[0] - pivot[0]) * sx;
    let dy = (pt[1] - pivot[1]) * sy;
    let rx = dx * cos_a - dy * sin_a;
    let ry = dx * sin_a + dy * cos_a;
    [pivot[0] + rx + tx, pivot[1] + ry + ty]
}

/// Apply one step of [`TransformEachSpec`] to a single [`VectorShape`] around its own
/// [`CanvasAnchor`] point.
pub fn transform_shape_once(shape: &VectorShape, spec: &TransformEachSpec) -> VectorShape {
    let Some(bounds) = shape_bounds(shape) else {
        return shape.clone();
    };
    let pivot = anchor_point_for_bounds(bounds, spec.anchor);
    let sx = if spec.reflect_x {
        -spec.scale_x
    } else {
        spec.scale_x
    };
    let sy = if spec.reflect_y {
        -spec.scale_y
    } else {
        spec.scale_y
    };
    let rad = spec.rotation_deg.to_radians();
    let cos_a = rad.cos();
    let sin_a = rad.sin();
    let tx = spec.translate_x;
    let ty = spec.translate_y;
    let xform = |p: [f32; 2]| transform_point_around_pivot(p, pivot, sx, sy, cos_a, sin_a, tx, ty);

    let has_rotation = (spec.rotation_deg % 360.0).abs() > 1e-4;
    let new_geometry = match &shape.geometry {
        VectorGeometry::Rect {
            x,
            y,
            w,
            h,
            corner_radius,
        } if !has_rotation => {
            let p0 = xform([*x, *y]);
            let p1 = xform([*x + *w, *y + *h]);
            let min_x = p0[0].min(p1[0]);
            let min_y = p0[1].min(p1[1]);
            let new_w = (p1[0] - p0[0]).abs();
            let new_h = (p1[1] - p0[1]).abs();
            let scale_avg = ((sx.abs() + sy.abs()) * 0.5).max(0.0);
            VectorGeometry::Rect {
                x: min_x,
                y: min_y,
                w: new_w,
                h: new_h,
                corner_radius: (*corner_radius * scale_avg)
                    .min(new_w * 0.5)
                    .min(new_h * 0.5),
            }
        }
        VectorGeometry::Ellipse { cx, cy, rx, ry } if !has_rotation => {
            let new_c = xform([*cx, *cy]);
            VectorGeometry::Ellipse {
                cx: new_c[0],
                cy: new_c[1],
                rx: (*rx * sx).abs(),
                ry: (*ry * sy).abs(),
            }
        }
        VectorGeometry::Polygon { points, closed } => VectorGeometry::Polygon {
            points: points.iter().map(|&p| xform(p)).collect(),
            closed: *closed,
        },
        _ => {
            let (nodes, closed) = geometry_to_bezier_path(&shape.geometry);
            let transformed_nodes = nodes
                .into_iter()
                .map(|n| VectorPathNode {
                    anchor: xform(n.anchor),
                    handle_in: n.handle_in.map(xform),
                    handle_out: n.handle_out.map(xform),
                })
                .collect();
            VectorGeometry::Path {
                nodes: transformed_nodes,
                closed,
            }
        }
    };

    let mut out = shape.clone();
    out.geometry = new_geometry;
    out
}

/// Apply [`TransformEachSpec`] across `shapes` (filtering by `target_ids` when `Some`, or
/// transforming all shapes when `None`).
///
/// - When `spec.apply_to_duplicate == false`, each targeted shape is transformed `spec.repeat_steps.max(1)` times in-place.
/// - When `spec.apply_to_duplicate == true`, original shapes are preserved and `spec.repeat_steps.max(1)` cumulative
///   transformed duplicates are appended for each targeted shape.
pub fn transform_each_shapes(
    shapes: &[VectorShape],
    target_ids: Option<&[u64]>,
    spec: &TransformEachSpec,
    next_id: &mut u64,
) -> Vec<VectorShape> {
    let steps = spec.repeat_steps.max(1);
    let is_targeted = |id: u64| match target_ids {
        Some(ids) if !ids.is_empty() => ids.contains(&id),
        _ => true,
    };

    let mut out = Vec::with_capacity(
        shapes.len()
            * (if spec.apply_to_duplicate {
                steps as usize + 1
            } else {
                1
            }),
    );
    let mut duplicates = Vec::new();

    for shape in shapes {
        if !is_targeted(shape.id) {
            out.push(shape.clone());
            continue;
        }
        if spec.apply_to_duplicate {
            out.push(shape.clone());
            let mut cur = shape.clone();
            for step in 1..=steps {
                cur = transform_shape_once(&cur, spec);
                let mut copy = cur.clone();
                copy.id = *next_id;
                *next_id += 1;
                copy.name = if steps == 1 {
                    format!("{} copy", shape.name)
                } else {
                    format!("{} copy {step}", shape.name)
                };
                duplicates.push(copy);
            }
        } else {
            let mut cur = shape.clone();
            for _ in 0..steps {
                cur = transform_shape_once(&cur, spec);
            }
            out.push(cur);
        }
    }

    out.extend(duplicates);
    out
}

/// Apply [`TransformEachSpec`] directly to a [`VectorLayerData`], updating `selected_shape_ids`
/// to the transformed (or newly duplicated) shape IDs. Returns the affected/created IDs.
pub fn apply_transform_each_to_layer(
    layer: &mut VectorLayerData,
    target_ids: Option<&[u64]>,
    spec: &TransformEachSpec,
) -> Vec<u64> {
    let before_count = layer.shapes.len();
    let mut next_id = layer
        .next_shape_id
        .max(layer.shapes.iter().map(|s| s.id).max().unwrap_or(0) + 1);
    let resolved_targets: Option<Vec<u64>> = match target_ids {
        Some(ids) if !ids.is_empty() => Some(ids.to_vec()),
        _ if !layer.selected_shape_ids.is_empty() => Some(layer.selected_shape_ids.clone()),
        _ => None,
    };
    let next_shapes = transform_each_shapes(
        &layer.shapes,
        resolved_targets.as_deref(),
        spec,
        &mut next_id,
    );
    layer.next_shape_id = next_id;
    let affected_ids: Vec<u64> = if spec.apply_to_duplicate && next_shapes.len() > before_count {
        next_shapes[before_count..].iter().map(|s| s.id).collect()
    } else {
        match &resolved_targets {
            Some(ids) => ids.clone(),
            None => next_shapes.iter().map(|s| s.id).collect(),
        }
    };
    layer.shapes = next_shapes;
    if !affected_ids.is_empty() {
        layer.selected_shape_ids = affected_ids.clone();
    }
    affected_ids
}

// ============================================================================
// Vector Blend (Shape / Path Interpolation over N intermediate steps)
// ============================================================================

#[inline]
fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [
        lerp_f32(f32::from(a[0]), f32::from(b[0]), t)
            .round()
            .clamp(0.0, 255.0) as u8,
        lerp_f32(f32::from(a[1]), f32::from(b[1]), t)
            .round()
            .clamp(0.0, 255.0) as u8,
        lerp_f32(f32::from(a[2]), f32::from(b[2]), t)
            .round()
            .clamp(0.0, 255.0) as u8,
    ]
}

/// Subdivide a [`VectorPathNode`] path until it contains `target_count` nodes while preserving its exact curve geometry.
pub fn resample_bezier_path(
    nodes: &[VectorPathNode],
    closed: bool,
    target_count: usize,
) -> Vec<VectorPathNode> {
    if nodes.is_empty() || nodes.len() >= target_count {
        return nodes.to_vec();
    }
    let mut out = nodes.to_vec();
    if out.len() == 1 {
        out.resize(target_count, out[0]);
        return out;
    }
    while out.len() < target_count {
        // Subdivide the longest segment at t = 0.5.
        let seg_count = if closed { out.len() } else { out.len() - 1 };
        let mut longest_seg = 0;
        let mut longest_len = -1.0f32;
        for i in 0..seg_count {
            let a = out[i].anchor;
            let b = out[(i + 1) % out.len()].anchor;
            let d = (b[0] - a[0]).hypot(b[1] - a[1]);
            if d > longest_len {
                longest_len = d;
                longest_seg = i;
            }
        }
        if add_node_on_segment(&mut out, closed, longest_seg, 0.5).is_none() {
            break;
        }
    }
    out
}

/// Generate `steps` intermediate [`VectorShape`]s smoothly interpolating between `start` and `end`:
/// geometry (parametric Rect/Ellipse or resampled Bezier Path nodes & handles), position, scale,
/// fill color, stroke color, stroke width, and opacity.
pub fn blend_vector_shapes(
    start: &VectorShape,
    end: &VectorShape,
    steps: u32,
    next_id: &mut u64,
) -> Vec<VectorShape> {
    if steps == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(steps as usize);

    // Precompute matching node counts if we need path interpolation.
    let path_pair = match (&start.geometry, &end.geometry) {
        (VectorGeometry::Rect { .. }, VectorGeometry::Rect { .. })
        | (VectorGeometry::Ellipse { .. }, VectorGeometry::Ellipse { .. }) => None,
        _ => {
            let (na, ca) = geometry_to_bezier_path(&start.geometry);
            let (nb, cb) = geometry_to_bezier_path(&end.geometry);
            let target_len = na.len().max(nb.len()).max(1);
            Some((
                resample_bezier_path(&na, ca, target_len),
                resample_bezier_path(&nb, cb, target_len),
                ca && cb,
            ))
        }
    };

    for step in 1..=steps {
        let t = (step as f32) / ((steps + 1) as f32);
        let id = *next_id;
        *next_id += 1;

        let geometry = match (&start.geometry, &end.geometry, &path_pair) {
            (
                VectorGeometry::Rect {
                    x: x0,
                    y: y0,
                    w: w0,
                    h: h0,
                    corner_radius: r0,
                },
                VectorGeometry::Rect {
                    x: x1,
                    y: y1,
                    w: w1,
                    h: h1,
                    corner_radius: r1,
                },
                _,
            ) => VectorGeometry::Rect {
                x: lerp_f32(*x0, *x1, t),
                y: lerp_f32(*y0, *y1, t),
                w: lerp_f32(*w0, *w1, t),
                h: lerp_f32(*h0, *h1, t),
                corner_radius: lerp_f32(*r0, *r1, t),
            },
            (
                VectorGeometry::Ellipse {
                    cx: cx0,
                    cy: cy0,
                    rx: rx0,
                    ry: ry0,
                },
                VectorGeometry::Ellipse {
                    cx: cx1,
                    cy: cy1,
                    rx: rx1,
                    ry: ry1,
                },
                _,
            ) => VectorGeometry::Ellipse {
                cx: lerp_f32(*cx0, *cx1, t),
                cy: lerp_f32(*cy0, *cy1, t),
                rx: lerp_f32(*rx0, *rx1, t),
                ry: lerp_f32(*ry0, *ry1, t),
            },
            (_, _, Some((nodes_a, nodes_b, closed))) => {
                let nodes = nodes_a
                    .iter()
                    .zip(nodes_b.iter())
                    .map(|(a, b)| {
                        let anchor = lerp_pt(a.anchor, b.anchor, t);
                        let handle_in = if a.handle_in.is_some() || b.handle_in.is_some() {
                            Some(lerp_pt(
                                a.handle_in.unwrap_or(a.anchor),
                                b.handle_in.unwrap_or(b.anchor),
                                t,
                            ))
                        } else {
                            None
                        };
                        let handle_out = if a.handle_out.is_some() || b.handle_out.is_some() {
                            Some(lerp_pt(
                                a.handle_out.unwrap_or(a.anchor),
                                b.handle_out.unwrap_or(b.anchor),
                                t,
                            ))
                        } else {
                            None
                        };
                        VectorPathNode {
                            anchor,
                            handle_in,
                            handle_out,
                        }
                    })
                    .collect();
                VectorGeometry::Path {
                    nodes,
                    closed: *closed,
                }
            }
            _ => start.geometry.clone(),
        };

        let fill = match (start.fill, end.fill) {
            (Some(c0), Some(c1)) => Some(lerp_rgb(c0, c1, t)),
            (Some(c0), None) => (t < 0.5).then_some(c0),
            (None, Some(c1)) => (t >= 0.5).then_some(c1),
            (None, None) => None,
        };

        let stroke = VectorStroke {
            color: lerp_rgb(start.stroke.color, end.stroke.color, t),
            width: lerp_f32(start.stroke.width, end.stroke.width, t).max(0.0),
            cap: if t < 0.5 {
                start.stroke.cap
            } else {
                end.stroke.cap
            },
            join: if t < 0.5 {
                start.stroke.join
            } else {
                end.stroke.join
            },
        };

        out.push(VectorShape {
            id,
            name: format!("Blend {step}"),
            visible: start.visible || end.visible,
            opacity: lerp_f32(start.opacity, end.opacity, t).clamp(0.0, 1.0),
            geometry,
            fill,
            stroke,
        });
    }

    out
}

/// Interpolate `spec.steps` intermediate shapes between `spec.start_shape_id` and `spec.end_shape_id`
/// in `layer`, inserting them right before the end shape in z-order. Returns the generated shape IDs.
pub fn apply_blend_to_layer(
    layer: &mut VectorLayerData,
    spec: &VectorBlendSpec,
) -> Result<Vec<u64>, ImageError> {
    if spec.start_shape_id == spec.end_shape_id {
        return Err(ImageError::InvalidEdit(
            "blend requires two distinct vector shapes".to_owned(),
        ));
    }
    let Some(start_idx) = layer.index_of(spec.start_shape_id) else {
        return Err(ImageError::coded("vector_shape_not_found"));
    };
    let Some(end_idx) = layer.index_of(spec.end_shape_id) else {
        return Err(ImageError::coded("vector_shape_not_found"));
    };
    let start_shape = layer.shapes[start_idx].clone();
    let end_shape = layer.shapes[end_idx].clone();

    let mut next_id = layer
        .next_shape_id
        .max(layer.shapes.iter().map(|s| s.id).max().unwrap_or(0) + 1);
    let blended = blend_vector_shapes(
        &start_shape,
        &end_shape,
        spec.steps.clamp(1, 256),
        &mut next_id,
    );
    layer.next_shape_id = next_id;
    let ids: Vec<u64> = blended.iter().map(|s| s.id).collect();

    let insert_at = start_idx.max(end_idx);
    for (offset, shape) in blended.into_iter().enumerate() {
        layer.shapes.insert(insert_at + offset, shape);
    }
    Ok(ids)
}

// ============================================================================
// Export Vector Layer to SVG
// ============================================================================

/// Serialize a [`VectorLayerData`] into a self-contained SVG 1.1 document string.
pub fn vector_layer_to_svg(
    data: &VectorLayerData,
    canvas_width: u32,
    canvas_height: u32,
) -> String {
    let w = canvas_width.max(1);
    let h = canvas_height.max(1);
    let mut svg = String::with_capacity(512 + data.shapes.len() * 192);
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
    );

    for shape in &data.shapes {
        if !shape.visible || shape.opacity <= 1e-4 {
            continue;
        }
        let fill_attr = match shape.fill {
            Some([r, g, b]) => format!("rgb({r},{g},{b})"),
            None => "none".to_owned(),
        };
        let (stroke_attr, stroke_width) = if shape.stroke.is_visible() {
            let [r, g, b] = shape.stroke.color;
            (format!("rgb({r},{g},{b})"), shape.stroke.width)
        } else {
            ("none".to_owned(), 0.0)
        };
        let cap = shape.stroke.cap.as_str();
        let join = shape.stroke.join.as_str();
        let op = shape.opacity.clamp(0.0, 1.0);

        match &shape.geometry {
            VectorGeometry::Rect {
                x,
                y,
                w,
                h,
                corner_radius,
            } => {
                let rx = corner_radius.max(0.0);
                let _ = writeln!(
                    svg,
                    r#"  <rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" rx="{rx:.2}" fill="{fill_attr}" stroke="{stroke_attr}" stroke-width="{stroke_width:.2}" stroke-linecap="{cap}" stroke-linejoin="{join}" opacity="{op:.3}"/>"#
                );
            }
            VectorGeometry::Ellipse { cx, cy, rx, ry } => {
                let _ = writeln!(
                    svg,
                    r#"  <ellipse cx="{cx:.2}" cy="{cy:.2}" rx="{rx:.2}" ry="{ry:.2}" fill="{fill_attr}" stroke="{stroke_attr}" stroke-width="{stroke_width:.2}" stroke-linecap="{cap}" stroke-linejoin="{join}" opacity="{op:.3}"/>"#
                );
            }
            _ => {
                let (nodes, closed) = geometry_to_bezier_path(&shape.geometry);
                if nodes.is_empty() {
                    continue;
                }
                let mut d = String::new();
                let _ = write!(d, "M {:.2} {:.2}", nodes[0].anchor[0], nodes[0].anchor[1]);
                let seg_count = if closed && nodes.len() >= 2 {
                    nodes.len()
                } else {
                    nodes.len() - 1
                };
                for i in 0..seg_count {
                    let a = nodes[i];
                    let b = nodes[(i + 1) % nodes.len()];
                    if a.handle_out.is_none() && b.handle_in.is_none() {
                        let _ = write!(d, " L {:.2} {:.2}", b.anchor[0], b.anchor[1]);
                    } else {
                        let h1 = a.handle_out.unwrap_or(a.anchor);
                        let h2 = b.handle_in.unwrap_or(b.anchor);
                        let _ = write!(
                            d,
                            " C {:.2} {:.2}, {:.2} {:.2}, {:.2} {:.2}",
                            h1[0], h1[1], h2[0], h2[1], b.anchor[0], b.anchor[1]
                        );
                    }
                }
                if closed {
                    d.push_str(" Z");
                }
                let _ = writeln!(
                    svg,
                    r#"  <path d="{d}" fill="{fill_attr}" stroke="{stroke_attr}" stroke-width="{stroke_width:.2}" stroke-linecap="{cap}" stroke-linejoin="{join}" opacity="{op:.3}"/>"#
                );
            }
        }
    }

    svg.push_str("</svg>\n");
    svg
}

#[cfg(test)]
mod tests {
    use photon_protocol::{VectorStrokeCap, VectorStrokeJoin};

    use super::*;

    #[test]
    fn render_vector_layer_draws_filled_and_stroked_shapes() {
        let rect = VectorShape::new_rect(
            1,
            "Rect",
            10.0,
            10.0,
            40.0,
            40.0,
            6.0,
            Some([220, 40, 40]),
            VectorStroke::with_cap_join(
                [20, 200, 60],
                4.0,
                VectorStrokeCap::Square,
                VectorStrokeJoin::Miter,
            ),
        );
        let ellipse = VectorShape::new_ellipse(
            2,
            "Ellipse",
            70.0,
            50.0,
            16.0,
            12.0,
            Some([40, 100, 240]),
            VectorStroke::new([255, 255, 255], 2.0),
        );
        let data = VectorLayerData::from_shapes(vec![rect, ellipse]);
        let img = render_vector_layer(&data, 100, 80);

        // Interior of rect should be filled red.
        let center_rect = img.get_pixel(30, 30).0;
        assert_eq!(center_rect[0], 220);
        assert_eq!(center_rect[3], 255);

        // Border of rect should show green stroke.
        let border_rect = img.get_pixel(30, 10).0;
        assert!(
            border_rect[1] > 150,
            "expected green stroke at border: {border_rect:?}"
        );

        // Interior of ellipse should be filled blue.
        let center_ellipse = img.get_pixel(70, 50).0;
        assert_eq!(center_ellipse[2], 240);
        assert_eq!(center_ellipse[3], 255);

        // Far corner should remain transparent.
        assert_eq!(img.get_pixel(98, 2).0[3], 0);
    }

    #[test]
    fn node_ops_subdivide_delete_split_join_reverse_and_modes() {
        let mut nodes = vec![
            VectorPathNode::corner([0.0, 0.0]),
            VectorPathNode::corner([100.0, 0.0]),
            VectorPathNode::corner([100.0, 100.0]),
        ];

        // Subdivide segment 0 at t = 0.5 -> inserts [50.0, 0.0] at index 1.
        let idx = add_node_on_segment(&mut nodes, false, 0, 0.5).expect("subdivided");
        assert_eq!(idx, 1);
        assert_eq!(nodes.len(), 4);
        assert!((nodes[1].anchor[0] - 50.0).abs() < 1e-4);

        // Switch node 1 to Symmetric and move HandleOut -> HandleIn mirrors automatically.
        assert!(set_node_mode(
            &mut nodes,
            false,
            1,
            VectorNodeMode::Symmetric
        ));
        assert_eq!(nodes[1].inferred_mode(), VectorNodeMode::Symmetric);
        assert!(move_node_handle(
            &mut nodes,
            1,
            VectorHandleKind::HandleOut,
            [70.0, 10.0],
            VectorNodeMode::Symmetric,
        ));
        let h_in = nodes[1].handle_in.expect("mirrored handle_in");
        assert!((h_in[0] - 30.0).abs() < 1e-3 && (h_in[1] - -10.0).abs() < 1e-3);

        // Switch node 1 to Smooth and move HandleIn with different length -> HandleOut keeps its length (22.36) but aligns collinearly.
        assert!(move_node_handle(
            &mut nodes,
            1,
            VectorHandleKind::HandleIn,
            [10.0, 0.0],
            VectorNodeMode::Smooth,
        ));
        assert_eq!(nodes[1].inferred_mode(), VectorNodeMode::Smooth);

        // Switch node 1 back to Corner -> clears handles.
        assert!(set_node_mode(&mut nodes, false, 1, VectorNodeMode::Corner));
        assert!(nodes[1].handle_in.is_none() && nodes[1].handle_out.is_none());

        // Split open path at node 1 -> two subpaths of lengths 2 and 3.
        let parts = split_path_at_node(&nodes, false, 1);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].0.len(), 2);
        assert_eq!(parts[1].0.len(), 3);

        // Join the two subpaths back together within tolerance 1.0 -> 4 nodes again.
        let rejoined = join_paths(&parts[0].0, &parts[1].0, 1.0);
        assert_eq!(rejoined.len(), 4);

        // Reverse path nodes.
        let mut rev = rejoined.clone();
        reverse_path_nodes(&mut rev);
        assert_eq!(rev[0].anchor, [100.0, 100.0]);
        assert_eq!(rev[3].anchor, [0.0, 0.0]);

        // Simplify collinear node [50, 0] between [0, 0] and [100, 0].
        let simplified = simplify_path_nodes(&rejoined, false, 1.0);
        assert_eq!(simplified.len(), 3);

        // Smooth path nodes generates handles.
        let mut smoothed = simplified.clone();
        smooth_path_nodes(&mut smoothed, true, 0.4);
        assert!(smoothed[0].handle_in.is_some() && smoothed[0].handle_out.is_some());

        // Delete node.
        let removed = delete_node(&mut smoothed, 1);
        assert!(removed.is_some());
        assert_eq!(smoothed.len(), 2);
    }

    #[test]
    fn transform_each_scales_around_individual_anchors_and_duplicates_repeat_steps() {
        let s1 = VectorShape::new_rect(
            1,
            "Box 1",
            10.0,
            10.0,
            20.0,
            20.0,
            0.0,
            Some([255, 0, 0]),
            VectorStroke::none(),
        );
        let s2 = VectorShape::new_rect(
            2,
            "Box 2",
            100.0,
            10.0,
            20.0,
            20.0,
            0.0,
            Some([0, 0, 255]),
            VectorStroke::none(),
        );
        let mut layer = VectorLayerData::from_shapes(vec![s1, s2]);
        layer.selected_shape_ids = vec![1, 2];

        // Scale 200% around each shape's own Center anchor in-place.
        let spec_scale = TransformEachSpec {
            scale_x: 2.0,
            scale_y: 2.0,
            anchor: CanvasAnchor::Center,
            ..TransformEachSpec::default()
        };
        apply_transform_each_to_layer(&mut layer, None, &spec_scale);
        // Box 1 center was (20, 20), so 200% scale yields x=0, y=0, w=40, h=40.
        // Box 2 center was (110, 20), so 200% scale yields x=90, y=0, w=40, h=40.
        assert_eq!(shape_bounds(&layer.shapes[0]), Some([0.0, 0.0, 40.0, 40.0]));
        assert_eq!(
            shape_bounds(&layer.shapes[1]),
            Some([90.0, 0.0, 130.0, 40.0])
        );

        // Now apply Transform Each with repeat_steps = 3 and apply_to_duplicate = true on Box 1.
        let spec_dup = TransformEachSpec {
            translate_x: 50.0,
            repeat_steps: 3,
            apply_to_duplicate: true,
            ..TransformEachSpec::default()
        };
        let created = apply_transform_each_to_layer(&mut layer, Some(&[1]), &spec_dup);
        assert_eq!(created.len(), 3);
        assert_eq!(layer.shapes.len(), 5);
        assert_eq!(
            shape_bounds(&layer.shapes[2]),
            Some([50.0, 0.0, 90.0, 40.0])
        );
        assert_eq!(
            shape_bounds(&layer.shapes[3]),
            Some([100.0, 0.0, 140.0, 40.0])
        );
        assert_eq!(
            shape_bounds(&layer.shapes[4]),
            Some([150.0, 0.0, 190.0, 40.0])
        );
    }

    #[test]
    fn vector_blend_interpolates_geometry_colors_stroke_and_opacity() {
        let start = VectorShape {
            id: 1,
            name: "Start".to_owned(),
            visible: true,
            opacity: 0.2,
            geometry: VectorGeometry::Rect {
                x: 0.0,
                y: 0.0,
                w: 20.0,
                h: 20.0,
                corner_radius: 0.0,
            },
            fill: Some([0, 0, 0]),
            stroke: VectorStroke::new([0, 100, 200], 2.0),
        };
        let end = VectorShape {
            id: 2,
            name: "End".to_owned(),
            visible: true,
            opacity: 1.0,
            geometry: VectorGeometry::Ellipse {
                cx: 100.0,
                cy: 100.0,
                rx: 30.0,
                ry: 30.0,
            },
            fill: Some([200, 100, 0]),
            stroke: VectorStroke::new([200, 100, 0], 10.0),
        };
        let mut layer = VectorLayerData::from_shapes(vec![start, end]);
        let ids = apply_blend_to_layer(
            &mut layer,
            &VectorBlendSpec {
                start_shape_id: 1,
                end_shape_id: 2,
                steps: 3,
            },
        )
        .expect("blend succeeded");
        assert_eq!(ids.len(), 3);
        assert_eq!(layer.shapes.len(), 5);

        // Middle blended shape is at index 2 (t = 0.5).
        let mid = &layer.shapes[2];
        assert!((mid.opacity - 0.6).abs() < 1e-4);
        assert_eq!(mid.fill, Some([100, 50, 0]));
        assert_eq!(mid.stroke.color, [100, 100, 100]);
        assert!((mid.stroke.width - 6.0).abs() < 1e-4);

        let svg = vector_layer_to_svg(&layer, 200, 200);
        assert!(svg.contains("<svg") && svg.contains("<path"));
    }

    #[test]
    fn closed_path_split_toggle_closest_node_rotate_reflect_and_caps_joins() {
        let mut shape = VectorShape::new_polygon(
            1,
            "Triangle",
            vec![[20.0, 20.0], [80.0, 20.0], [50.0, 70.0]],
            true,
            Some([120, 80, 220]),
            VectorStroke::with_cap_join(
                [255, 220, 40],
                6.0,
                VectorStrokeCap::Butt,
                VectorStrokeJoin::Bevel,
            ),
        );
        assert!(convert_shape_to_path(&mut shape));
        assert!(shape.geometry.is_closed());

        // Split closed path at node 1 -> opens into a 4-node open path starting and ending at [80, 20].
        let mut next_id = 2;
        let split_shapes = split_shape_at_node(&shape, 1, &mut next_id);
        assert_eq!(split_shapes.len(), 1);
        assert!(!split_shapes[0].geometry.is_closed());

        // Toggle open path back to closed and add a node near [50, 20].
        let mut reopened = split_shapes[0].clone();
        assert!(toggle_path_closed(&mut reopened.geometry));
        if let VectorGeometry::Path { nodes, closed } = &mut reopened.geometry {
            let inserted = add_node_at_closest_point(nodes, *closed, [50.0, 20.5], 5.0);
            assert!(inserted.is_some());
            let idx = inserted.unwrap();
            assert!(move_node_anchor(nodes, idx, [50.0, 12.0]));
            assert_eq!(delete_nodes(nodes, &[idx]), 1);
        }

        // Rotate 90 degrees and reflect X around TopLeft anchor.
        let rotated = transform_shape_once(
            &shape,
            &TransformEachSpec {
                rotation_deg: 90.0,
                reflect_x: true,
                anchor: CanvasAnchor::TopLeft,
                ..TransformEachSpec::default()
            },
        );
        assert!(rotated.geometry.is_path());
        let mut canvas = RgbaImage::from_pixel(120, 120, Rgba([0, 0, 0, 0]));
        render_vector_shape_onto(&mut canvas, &shape);
        assert!(canvas.pixels().any(|p| p.0[3] > 0));
    }
}
