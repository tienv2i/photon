//! Pure-Rust 2D polygon boolean operations (Pathfinder) and line knife splitting
//! for [`VectorLayerData`] shapes.

use photon_protocol::{
    VectorBooleanOp, VectorGeometry, VectorLayerData, VectorPathNode, VectorShape,
};

use crate::vector::geometry_to_polyline;

#[inline]
fn point_in_polygon(pt: [f64; 2], poly: &[[f64; 2]]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let xi = poly[i][0];
        let yi = poly[i][1];
        let xj = poly[j][0];
        let yj = poly[j][1];
        let intersect =
            ((yi > pt[1]) != (yj > pt[1])) && (pt[0] < (xj - xi) * (pt[1] - yi) / (yj - yi) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[inline]
fn seg_intersect(
    a1: [f64; 2],
    a2: [f64; 2],
    b1: [f64; 2],
    b2: [f64; 2],
) -> Option<(f64, f64, [f64; 2])> {
    let dax = a2[0] - a1[0];
    let day = a2[1] - a1[1];
    let dbx = b2[0] - b1[0];
    let dby = b2[1] - b1[1];
    let denom = dax * dby - day * dbx;
    if denom.abs() < 1e-9 {
        return None;
    }
    let dx = b1[0] - a1[0];
    let dy = b1[1] - a1[1];
    let ta = (dx * dby - dy * dbx) / denom;
    let tb = (dx * day - dy * dax) / denom;
    if (0.0..=1.0).contains(&ta) && (0.0..=1.0).contains(&tb) {
        Some((ta, tb, [a1[0] + ta * dax, a1[1] + ta * day]))
    } else {
        None
    }
}

#[derive(Clone, Debug)]
struct GhVertex {
    pt: [f64; 2],
    is_intersect: bool,
    entry: bool,
    partner: Option<usize>,
    visited: bool,
}

fn ensure_ccw(poly: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut area = 0.0_f64;
    for i in 0..poly.len() {
        let j = (i + 1) % poly.len();
        area += poly[i][0] * poly[j][1] - poly[j][0] * poly[i][1];
    }
    let mut out = poly.to_vec();
    if area < 0.0 {
        out.reverse();
    }
    out
}

fn build_gh_ring(subject: &[[f64; 2]], clip: &[[f64; 2]]) -> (Vec<GhVertex>, Vec<GhVertex>, bool) {
    let na = subject.len();
    let nb = clip.len();
    let mut a_inserts: Vec<Vec<(f64, [f64; 2], usize)>> = vec![Vec::new(); na];
    let mut b_inserts: Vec<Vec<(f64, [f64; 2], usize)>> = vec![Vec::new(); nb];
    let mut pairs: Vec<(usize, usize)> = Vec::new();

    for (ia, a_ins) in a_inserts.iter_mut().enumerate() {
        let a1 = subject[ia];
        let a2 = subject[(ia + 1) % na];
        for (ib, b_ins) in b_inserts.iter_mut().enumerate() {
            let b1 = clip[ib];
            let b2 = clip[(ib + 1) % nb];
            if let Some((ta, tb, pt)) = seg_intersect(a1, a2, b1, b2) {
                // Perturb exact endpoint hits slightly so Greiner-Hormann stays non-degenerate.
                let ta = ta.clamp(1e-6, 1.0 - 1e-6);
                let tb = tb.clamp(1e-6, 1.0 - 1e-6);
                let pid = pairs.len();
                pairs.push((0, 0));
                a_ins.push((ta, pt, pid));
                b_ins.push((tb, pt, pid));
            }
        }
    }

    let has_intersections = !pairs.is_empty();
    let mut va = Vec::new();
    for (ia, &pt) in subject.iter().enumerate() {
        va.push(GhVertex {
            pt,
            is_intersect: false,
            entry: false,
            partner: None,
            visited: false,
        });
        a_inserts[ia].sort_by(|x, y| x.0.total_cmp(&y.0));
        for &(_alpha, ipt, pid) in &a_inserts[ia] {
            let idx = va.len();
            pairs[pid].0 = idx;
            va.push(GhVertex {
                pt: ipt,
                is_intersect: true,
                entry: false,
                partner: None,
                visited: false,
            });
        }
    }

    let mut vb = Vec::new();
    for (ib, &pt) in clip.iter().enumerate() {
        vb.push(GhVertex {
            pt,
            is_intersect: false,
            entry: false,
            partner: None,
            visited: false,
        });
        b_inserts[ib].sort_by(|x, y| x.0.total_cmp(&y.0));
        for &(_alpha, ipt, pid) in &b_inserts[ib] {
            let idx = vb.len();
            pairs[pid].1 = idx;
            vb.push(GhVertex {
                pt: ipt,
                is_intersect: true,
                entry: false,
                partner: None,
                visited: false,
            });
        }
    }

    for &(ia, ib) in &pairs {
        va[ia].partner = Some(ib);
        vb[ib].partner = Some(ia);
    }

    // Mark entry/exit status for subject against clip, and clip against subject.
    let mut status_a = point_in_polygon(va[0].pt, clip);
    for v in &mut va {
        if v.is_intersect {
            v.entry = !status_a;
            status_a = !status_a;
        }
    }
    let mut status_b = point_in_polygon(vb[0].pt, subject);
    for v in &mut vb {
        if v.is_intersect {
            v.entry = !status_b;
            status_b = !status_b;
        }
    }

    (va, vb, has_intersections)
}

fn trace_gh(
    mut va: Vec<GhVertex>,
    mut vb: Vec<GhVertex>,
    a_forward_on_entry: bool,
    b_forward_on_entry: bool,
) -> Vec<Vec<[f64; 2]>> {
    let mut results = Vec::new();
    let max_steps = (va.len() + vb.len()) * 2 + 16;

    for start in 0..va.len() {
        if !va[start].is_intersect || va[start].visited {
            continue;
        }
        let mut poly = Vec::new();
        let mut in_a = true;
        let mut cur = start;
        let mut forward = if va[cur].entry {
            a_forward_on_entry
        } else {
            !a_forward_on_entry
        };

        for _ in 0..max_steps {
            if in_a {
                if va[cur].visited && cur == start && poly.len() >= 3 {
                    break;
                }
                va[cur].visited = true;
                poly.push(va[cur].pt);
                if va[cur].is_intersect
                    && poly.len() > 1
                    && let Some(p) = va[cur].partner
                {
                    vb[p].visited = true;
                    in_a = false;
                    cur = p;
                    forward = if vb[cur].entry {
                        b_forward_on_entry
                    } else {
                        !b_forward_on_entry
                    };
                }
                let n = va.len();
                cur = if forward {
                    (cur + 1) % n
                } else {
                    (cur + n - 1) % n
                };
            } else {
                vb[cur].visited = true;
                poly.push(vb[cur].pt);
                if vb[cur].is_intersect
                    && let Some(p) = vb[cur].partner
                {
                    va[p].visited = true;
                    in_a = true;
                    cur = p;
                    forward = if va[cur].entry {
                        a_forward_on_entry
                    } else {
                        !a_forward_on_entry
                    };
                    if cur == start && poly.len() >= 3 {
                        break;
                    }
                }
                let n = vb.len();
                cur = if forward {
                    (cur + 1) % n
                } else {
                    (cur + n - 1) % n
                };
            }
        }
        if poly.len() >= 3 {
            results.push(poly);
        }
    }
    results
}

/// Perform a 2D boolean operation on two closed polygons `poly_a` and `poly_b`.
pub fn boolean_polygons(
    poly_a: &[[f64; 2]],
    poly_b: &[[f64; 2]],
    op: VectorBooleanOp,
) -> Vec<Vec<[f64; 2]>> {
    if poly_a.len() < 3 || poly_b.len() < 3 {
        return Vec::new();
    }
    let a = ensure_ccw(poly_a);
    // Tiny deterministic perturbation avoids degenerate collinear/shared-vertex edges in Greiner-Hormann.
    let b: Vec<[f64; 2]> = ensure_ccw(poly_b)
        .into_iter()
        .map(|[x, y]| [x + 1e-4, y + 1.3e-4])
        .collect();
    let (va, vb, has_x) = build_gh_ring(&a, &b);

    if !has_x {
        let a_in_b = point_in_polygon(a[0], &b);
        let b_in_a = point_in_polygon(b[0], &a);
        return match op {
            VectorBooleanOp::Union => {
                if a_in_b {
                    vec![b]
                } else if b_in_a {
                    vec![a]
                } else {
                    vec![a, b]
                }
            }
            VectorBooleanOp::Intersect => {
                if a_in_b {
                    vec![a]
                } else if b_in_a {
                    vec![b]
                } else {
                    Vec::new()
                }
            }
            VectorBooleanOp::Subtract => {
                if a_in_b {
                    Vec::new()
                } else {
                    vec![a]
                }
            }
            VectorBooleanOp::Exclude => {
                if a_in_b {
                    vec![b]
                } else if b_in_a {
                    vec![a]
                } else {
                    vec![a, b]
                }
            }
        };
    }

    match op {
        VectorBooleanOp::Union => trace_gh(va, vb, false, false),
        VectorBooleanOp::Intersect => trace_gh(va, vb, true, true),
        VectorBooleanOp::Subtract => trace_gh(va, vb, false, true),
        VectorBooleanOp::Exclude => {
            let mut out = trace_gh(va.clone(), vb.clone(), false, true);
            out.extend(trace_gh(vb, va, false, true));
            out
        }
    }
}

/// Split a polyline/polygon along a line segment `(line_a, line_b)` at edge intersections.
pub fn knife_split_polyline(
    poly: &[[f64; 2]],
    closed: bool,
    line_a: [f64; 2],
    line_b: [f64; 2],
) -> Vec<(Vec<[f64; 2]>, bool)> {
    if poly.len() < 2 {
        return Vec::new();
    }
    let edge_count = if closed { poly.len() } else { poly.len() - 1 };
    let mut hits: Vec<(usize, [f64; 2])> = Vec::new();

    for i in 0..edge_count {
        let p0 = poly[i];
        let p1 = poly[(i + 1) % poly.len()];
        if let Some((_, _, pt)) = seg_intersect(p0, p1, line_a, line_b) {
            hits.push((i, pt));
        }
    }

    if closed && hits.len() >= 2 {
        let (e0, h0) = hits[0];
        let (e1, h1) = hits[1];
        let mut part1 = vec![h0];
        for p in poly.iter().take(e1 + 1).skip(e0 + 1) {
            part1.push(*p);
        }
        part1.push(h1);

        let mut part2 = vec![h1];
        for p in poly.iter().skip(e1 + 1) {
            part2.push(*p);
        }
        for p in poly.iter().take(e0 + 1) {
            part2.push(*p);
        }
        part2.push(h0);
        return vec![(part1, true), (part2, true)];
    }

    if !closed && !hits.is_empty() {
        let mut out = Vec::new();
        let mut cur = vec![poly[0]];
        let mut hit_iter = hits.into_iter().peekable();
        for i in 0..edge_count {
            while let Some(&(ei, pt)) = hit_iter.peek() {
                if ei != i {
                    break;
                }
                hit_iter.next();
                cur.push(pt);
                if cur.len() >= 2 {
                    out.push((cur, false));
                }
                cur = vec![pt];
            }
            cur.push(poly[i + 1]);
        }
        if cur.len() >= 2 {
            out.push((cur, false));
        }
        return out;
    }

    vec![(poly.to_vec(), closed)]
}

fn poly_to_vector_geometry(poly: &[[f64; 2]], closed: bool) -> VectorGeometry {
    VectorGeometry::Path {
        nodes: poly
            .iter()
            .map(|&[x, y]| VectorPathNode::corner([x as f32, y as f32]))
            .collect(),
        closed,
    }
}

/// Apply [`VectorBooleanOp`] across the selected shapes in `vdata`, replacing them with the result(s).
pub fn apply_vector_boolean_to_layer(
    vdata: &mut VectorLayerData,
    shape_ids: &[u64],
    op: VectorBooleanOp,
) -> bool {
    let target_ids: Vec<u64> = if shape_ids.len() >= 2 {
        shape_ids.to_vec()
    } else if vdata.selected_shape_ids.len() >= 2 {
        vdata.selected_shape_ids.clone()
    } else if vdata.shapes.len() >= 2 {
        vdata.shapes.iter().map(|s| s.id).collect()
    } else {
        return false;
    };

    let selected_shapes: Vec<VectorShape> = vdata
        .shapes
        .iter()
        .filter(|s| target_ids.contains(&s.id))
        .cloned()
        .collect();
    if selected_shapes.len() < 2 {
        return false;
    }

    let base = selected_shapes[0].clone();
    let (mut acc_polys, _) = {
        let (p, _) = geometry_to_polyline(&selected_shapes[0].geometry, 0.5);
        (vec![p], true)
    };
    for s in &selected_shapes[1..] {
        let (other, _) = geometry_to_polyline(&s.geometry, 0.5);
        let mut next = Vec::new();
        for a in &acc_polys {
            next.extend(boolean_polygons(a, &other, op));
        }
        acc_polys = next;
    }

    vdata.shapes.retain(|s| !target_ids.contains(&s.id));
    let mut new_ids = Vec::new();
    for poly in acc_polys {
        let id = vdata.next_shape_id;
        vdata.next_shape_id += 1;
        new_ids.push(id);
        let mut shape = base.clone();
        shape.id = id;
        shape.geometry = poly_to_vector_geometry(&poly, true);
        vdata.shapes.push(shape);
    }
    vdata.selected_shape_ids = new_ids;
    true
}

/// Split intersecting shapes in `vdata` along the line segment `(line_start, line_end)`.
pub fn apply_knife_to_layer(
    vdata: &mut VectorLayerData,
    line_start: [f32; 2],
    line_end: [f32; 2],
) -> bool {
    let la = [f64::from(line_start[0]), f64::from(line_start[1])];
    let lb = [f64::from(line_end[0]), f64::from(line_end[1])];
    let mut next_shapes = Vec::with_capacity(vdata.shapes.len());
    let mut new_selected = Vec::new();
    let mut changed = false;

    for shape in &vdata.shapes {
        let (poly, closed) = geometry_to_polyline(&shape.geometry, 0.5);
        let pieces = knife_split_polyline(&poly, closed, la, lb);
        if pieces.len() <= 1 {
            next_shapes.push(shape.clone());
        } else {
            changed = true;
            for (piece, p_closed) in pieces {
                let id = vdata.next_shape_id;
                vdata.next_shape_id += 1;
                new_selected.push(id);
                let mut ns = shape.clone();
                ns.id = id;
                ns.geometry = poly_to_vector_geometry(&piece, p_closed);
                next_shapes.push(ns);
            }
        }
    }

    if changed {
        vdata.shapes = next_shapes;
        vdata.selected_shape_ids = new_selected;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boolean_union_intersect_subtract_exclude_work_for_overlapping_rects() {
        let a = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
        let b = vec![[50.0, 0.0], [150.0, 0.0], [150.0, 100.0], [50.0, 100.0]];

        let u = boolean_polygons(&a, &b, VectorBooleanOp::Union);
        assert_eq!(u.len(), 1);
        assert!(u[0].len() >= 4);

        let inter = boolean_polygons(&a, &b, VectorBooleanOp::Intersect);
        assert_eq!(inter.len(), 1);

        let sub = boolean_polygons(&a, &b, VectorBooleanOp::Subtract);
        assert_eq!(sub.len(), 1);

        let excl = boolean_polygons(&a, &b, VectorBooleanOp::Exclude);
        assert_eq!(excl.len(), 2);
    }

    #[test]
    fn knife_splits_closed_square_into_two_closed_halves() {
        let sq = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
        let pieces = knife_split_polyline(&sq, true, [-10.0, 50.0], [110.0, 50.0]);
        assert_eq!(pieces.len(), 2);
        assert!(pieces[0].1 && pieces[1].1);
        assert!(pieces[0].0.len() >= 4 && pieces[1].0.len() >= 4);
    }
}
