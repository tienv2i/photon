//! Screen-space 1px vector path overlay, anchor squares, Bezier tangent handles,
//! and hit-testing for `CanvasTool::PathSelect`.

use iced::Rectangle;
use photon_protocol::{
    VectorGeometry, VectorHandleKind, VectorLayerData, VectorNodeMode, VectorPathNode, VectorShape,
};

use crate::{CanvasInteractionState, ImageCanvasProgram, OverlayVertex};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VectorDragKind {
    Anchor {
        shape_id: u64,
        node_index: usize,
    },
    Handle {
        shape_id: u64,
        node_index: usize,
        handle: VectorHandleKind,
        mode: VectorNodeMode,
    },
    TranslateShape {
        shape_id: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VectorDragState {
    pub kind: VectorDragKind,
    pub start_px: [f32; 2],
    pub current_px: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VectorHit {
    Handle {
        shape_id: u64,
        node_index: usize,
        handle: VectorHandleKind,
        mode: VectorNodeMode,
    },
    Anchor {
        shape_id: u64,
        node_index: usize,
    },
    Segment {
        shape_id: u64,
    },
    ShapeBody {
        shape_id: u64,
    },
}

const KAPPA: f32 = 0.552_284_8;

/// Convert any [`VectorGeometry`] into cubic Bezier nodes `(nodes, closed)` for overlay rendering and hit-testing.
pub fn geometry_to_bezier_nodes(geometry: &VectorGeometry) -> (Vec<VectorPathNode>, bool) {
    match geometry {
        VectorGeometry::Path { nodes, closed } => (nodes.clone(), *closed),
        VectorGeometry::Polygon { points, closed } => (
            points
                .iter()
                .map(|&pt| VectorPathNode::corner(pt))
                .collect(),
            *closed,
        ),
        VectorGeometry::Rect {
            x,
            y,
            w,
            h,
            corner_radius,
        } => {
            let w = w.max(0.0);
            let h = h.max(0.0);
            let r = corner_radius.clamp(0.0, (w * 0.5).min(h * 0.5));
            if r <= 1e-3 {
                let nodes = vec![
                    VectorPathNode::corner([*x, *y]),
                    VectorPathNode::corner([*x + w, *y]),
                    VectorPathNode::corner([*x + w, *y + h]),
                    VectorPathNode::corner([*x, *y + h]),
                ];
                return (nodes, true);
            }
            let k = r * KAPPA;
            let x0 = *x;
            let y0 = *y;
            let x1 = *x + w;
            let y1 = *y + h;
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
        VectorGeometry::Ellipse { cx, cy, rx, ry } => {
            let rx = rx.abs();
            let ry = ry.abs();
            let kx = rx * KAPPA;
            let ky = ry * KAPPA;
            let nodes = vec![
                VectorPathNode::new(
                    [*cx, *cy - ry],
                    Some([*cx - kx, *cy - ry]),
                    Some([*cx + kx, *cy - ry]),
                ),
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
            ];
            (nodes, true)
        }
    }
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

fn translate_shape_preview(shape: &mut VectorShape, dx: f32, dy: f32) {
    match &mut shape.geometry {
        VectorGeometry::Rect { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
        VectorGeometry::Ellipse { cx, cy, .. } => {
            *cx += dx;
            *cy += dy;
        }
        VectorGeometry::Polygon { points, .. } => {
            for pt in points {
                pt[0] += dx;
                pt[1] += dy;
            }
        }
        VectorGeometry::Path { nodes, .. } => {
            for node in nodes {
                node.anchor[0] += dx;
                node.anchor[1] += dy;
                if let Some(h) = &mut node.handle_in {
                    h[0] += dx;
                    h[1] += dy;
                }
                if let Some(h) = &mut node.handle_out {
                    h[0] += dx;
                    h[1] += dy;
                }
            }
        }
    }
}

fn move_handle_preview(
    node: &mut VectorPathNode,
    handle: VectorHandleKind,
    new_pos: [f32; 2],
    mode: VectorNodeMode,
) {
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
}

/// Apply an in-progress mouse drag to a cloned [`VectorLayerData`] for 60fps overlay preview.
pub fn apply_live_vector_drag(
    vdata: &VectorLayerData,
    drag: Option<VectorDragState>,
) -> VectorLayerData {
    let Some(drag) = drag else {
        return vdata.clone();
    };
    let mut out = vdata.clone();
    match drag.kind {
        VectorDragKind::Anchor {
            shape_id,
            node_index,
        } => {
            if let Some(shape) = out.shape_mut(shape_id) {
                let (mut nodes, closed) = geometry_to_bezier_nodes(&shape.geometry);
                if let Some(node) = nodes.get_mut(node_index) {
                    let dx = drag.current_px[0] - node.anchor[0];
                    let dy = drag.current_px[1] - node.anchor[1];
                    node.anchor = drag.current_px;
                    if let Some(h) = &mut node.handle_in {
                        h[0] += dx;
                        h[1] += dy;
                    }
                    if let Some(h) = &mut node.handle_out {
                        h[0] += dx;
                        h[1] += dy;
                    }
                    shape.geometry = VectorGeometry::Path { nodes, closed };
                }
            }
        }
        VectorDragKind::Handle {
            shape_id,
            node_index,
            handle,
            mode,
        } => {
            if let Some(shape) = out.shape_mut(shape_id) {
                let (mut nodes, closed) = geometry_to_bezier_nodes(&shape.geometry);
                if let Some(node) = nodes.get_mut(node_index) {
                    move_handle_preview(node, handle, drag.current_px, mode);
                    shape.geometry = VectorGeometry::Path { nodes, closed };
                }
            }
        }
        VectorDragKind::TranslateShape { shape_id } => {
            let dx = drag.current_px[0] - drag.start_px[0];
            let dy = drag.current_px[1] - drag.start_px[1];
            if let Some(shape) = out.shape_mut(shape_id) {
                translate_shape_preview(shape, dx, dy);
            }
        }
    }
    out
}

/// Hit-test `vdata` in document pixel coordinates (`doc_pt`) with tolerance `hit_radius_doc`.
pub fn hit_test_vector_layer(
    vdata: &VectorLayerData,
    doc_pt: [f32; 2],
    hit_radius_doc: f32,
) -> Option<VectorHit> {
    let primary_selected_id = vdata
        .selected_shape_ids
        .last()
        .copied()
        .or_else(|| vdata.shapes.last().map(|s| s.id));

    // 1. Check Bezier handles on the currently selected shape first.
    if let Some(sid) = primary_selected_id
        && let Some(shape) = vdata.shape(sid)
        && shape.visible
    {
        let (nodes, _) = geometry_to_bezier_nodes(&shape.geometry);
        for (idx, node) in nodes.iter().enumerate() {
            let mode = node.inferred_mode();
            if let Some(hin) = node.handle_in
                && (hin[0] - doc_pt[0]).hypot(hin[1] - doc_pt[1]) <= hit_radius_doc
            {
                return Some(VectorHit::Handle {
                    shape_id: sid,
                    node_index: idx,
                    handle: VectorHandleKind::HandleIn,
                    mode,
                });
            }
            if let Some(hout) = node.handle_out
                && (hout[0] - doc_pt[0]).hypot(hout[1] - doc_pt[1]) <= hit_radius_doc
            {
                return Some(VectorHit::Handle {
                    shape_id: sid,
                    node_index: idx,
                    handle: VectorHandleKind::HandleOut,
                    mode,
                });
            }
        }
    }

    // 2. Check anchor nodes on the selected shape, then on all visible shapes top-to-bottom.
    if let Some(sid) = primary_selected_id
        && let Some(shape) = vdata.shape(sid)
        && shape.visible
    {
        let (nodes, _) = geometry_to_bezier_nodes(&shape.geometry);
        for (idx, node) in nodes.iter().enumerate() {
            if (node.anchor[0] - doc_pt[0]).hypot(node.anchor[1] - doc_pt[1]) <= hit_radius_doc {
                return Some(VectorHit::Anchor {
                    shape_id: sid,
                    node_index: idx,
                });
            }
        }
    }
    for shape in vdata.shapes.iter().rev() {
        if !shape.visible || Some(shape.id) == primary_selected_id {
            continue;
        }
        let (nodes, _) = geometry_to_bezier_nodes(&shape.geometry);
        for (idx, node) in nodes.iter().enumerate() {
            if (node.anchor[0] - doc_pt[0]).hypot(node.anchor[1] - doc_pt[1]) <= hit_radius_doc {
                return Some(VectorHit::Anchor {
                    shape_id: shape.id,
                    node_index: idx,
                });
            }
        }
    }

    // 3. Check curve segments on visible shapes top-to-bottom.
    let seg_thresh = (hit_radius_doc * 1.15).max(2.0);
    for shape in vdata.shapes.iter().rev() {
        if !shape.visible {
            continue;
        }
        let (nodes, closed) = geometry_to_bezier_nodes(&shape.geometry);
        if nodes.len() < 2 {
            continue;
        }
        let seg_count = if closed { nodes.len() } else { nodes.len() - 1 };
        const SAMPLES: usize = 20;
        for seg_idx in 0..seg_count {
            let a = nodes[seg_idx];
            let b = nodes[(seg_idx + 1) % nodes.len()];
            let p0 = a.anchor;
            let p1 = a.handle_out.unwrap_or(p0);
            let p2 = b.handle_in.unwrap_or(b.anchor);
            let p3 = b.anchor;
            let mut prev = p0;
            for s in 1..=SAMPLES {
                let t = s as f32 / SAMPLES as f32;
                let cur = eval_cubic(p0, p1, p2, p3, t);
                if point_to_segment_dist(doc_pt, prev, cur) <= seg_thresh {
                    return Some(VectorHit::Segment { shape_id: shape.id });
                }
                prev = cur;
            }
        }
    }

    // 4. Check bounding boxes of filled shapes top-to-bottom.
    for shape in vdata.shapes.iter().rev() {
        if !shape.visible {
            continue;
        }
        let (nodes, _) = geometry_to_bezier_nodes(&shape.geometry);
        if nodes.is_empty() {
            continue;
        }
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for n in &nodes {
            min_x = min_x.min(n.anchor[0]);
            min_y = min_y.min(n.anchor[1]);
            max_x = max_x.max(n.anchor[0]);
            max_y = max_y.max(n.anchor[1]);
        }
        if doc_pt[0] >= min_x - hit_radius_doc
            && doc_pt[0] <= max_x + hit_radius_doc
            && doc_pt[1] >= min_y - hit_radius_doc
            && doc_pt[1] <= max_y + hit_radius_doc
        {
            return Some(VectorHit::ShapeBody { shape_id: shape.id });
        }
    }

    None
}

fn point_to_segment_dist(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len_sq = dx * dx + dy * dy;
    if len_sq <= 1e-6 {
        return (p[0] - a[0]).hypot(p[1] - a[1]);
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len_sq).clamp(0.0, 1.0);
    (p[0] - (a[0] + t * dx)).hypot(p[1] - (a[1] + t * dy))
}

impl ImageCanvasProgram {
    /// Append crisp screen-space 1px vector path outlines, anchor node squares,
    /// and Bezier `handle_in` / `handle_out` tangent lines for the active vector layer.
    pub(crate) fn append_active_vector_layer_overlay(
        &self,
        state: &CanvasInteractionState,
        lines: &mut Vec<OverlayVertex>,
        tris: &mut Vec<OverlayVertex>,
        bounds: Rectangle,
    ) {
        let Some(ref raw_vdata) = self.active_vector_layer else {
            return;
        };
        if raw_vdata.shapes.is_empty() {
            return;
        }
        let vdata = apply_live_vector_drag(raw_vdata, state.vector_drag);
        let fallback_selected = vdata.shapes.last().map(|s| s.id);

        let dark_border = [0.05, 0.06, 0.09, 0.94];
        let selected_outline = [0.22, 0.86, 1.0, 0.96];
        let unselected_outline = [0.45, 0.72, 0.95, 0.65];
        let handle_color = [1.0, 0.78, 0.25, 0.94];
        let active_node_fill = [0.20, 0.88, 1.0, 1.0];
        let normal_node_fill = [1.0, 1.0, 1.0, 0.98];

        for shape in &vdata.shapes {
            if !shape.visible {
                continue;
            }
            let is_selected = if vdata.selected_shape_ids.is_empty() {
                Some(shape.id) == fallback_selected
            } else {
                vdata.selected_shape_ids.contains(&shape.id)
            };
            let (nodes, closed) = geometry_to_bezier_nodes(&shape.geometry);
            if nodes.is_empty() {
                continue;
            }
            let outline_col = if is_selected {
                selected_outline
            } else {
                unselected_outline
            };

            if nodes.len() >= 2 {
                let seg_count = if closed { nodes.len() } else { nodes.len() - 1 };
                for seg_idx in 0..seg_count {
                    let a = nodes[seg_idx];
                    let b = nodes[(seg_idx + 1) % nodes.len()];
                    let p0 = a.anchor;
                    let p3 = b.anchor;
                    if a.handle_out.is_none() && b.handle_in.is_none() {
                        if let (Some(na), Some(nb)) = (
                            self.doc_px_to_ndc(p0, bounds),
                            self.doc_px_to_ndc(p3, bounds),
                        ) {
                            self.push_overlay_line_ndc(
                                lines,
                                tris,
                                na,
                                nb,
                                outline_col,
                                1.1,
                                bounds,
                            );
                        }
                    } else {
                        let p1 = a.handle_out.unwrap_or(p0);
                        let p2 = b.handle_in.unwrap_or(p3);
                        const STEPS: usize = 16;
                        for s in 0..STEPS {
                            let t0 = s as f32 / STEPS as f32;
                            let t1 = (s + 1) as f32 / STEPS as f32;
                            let q0 = eval_cubic(p0, p1, p2, p3, t0);
                            let q1 = eval_cubic(p0, p1, p2, p3, t1);
                            if let (Some(na), Some(nb)) = (
                                self.doc_px_to_ndc(q0, bounds),
                                self.doc_px_to_ndc(q1, bounds),
                            ) {
                                self.push_overlay_line_ndc(
                                    lines,
                                    tris,
                                    na,
                                    nb,
                                    outline_col,
                                    1.1,
                                    bounds,
                                );
                            }
                        }
                    }
                }
            }

            if is_selected {
                for (idx, node) in nodes.iter().enumerate() {
                    for h in [node.handle_in, node.handle_out].into_iter().flatten() {
                        if let (Some(na), Some(nb)) = (
                            self.doc_px_to_ndc(node.anchor, bounds),
                            self.doc_px_to_ndc(h, bounds),
                        ) {
                            self.push_overlay_line_ndc(
                                lines,
                                tris,
                                na,
                                nb,
                                handle_color,
                                1.0,
                                bounds,
                            );
                        }
                        self.push_overlay_dot(tris, h, 3.6, dark_border, bounds);
                        self.push_overlay_dot(tris, h, 2.6, handle_color, bounds);
                    }
                    let is_active_node = self.selected_vector_node == Some(idx);
                    self.push_overlay_square(tris, node.anchor, 4.4, dark_border, bounds);
                    self.push_overlay_square(
                        tris,
                        node.anchor,
                        3.1,
                        if is_active_node {
                            active_node_fill
                        } else {
                            normal_node_fill
                        },
                        bounds,
                    );
                }
            }
        }
    }
}
