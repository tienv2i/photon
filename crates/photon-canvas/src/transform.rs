//! 2D affine Free Transform geometry, OBB handles, hit-testing, and interactive drag math.

use photon_protocol::LayerTransformSpec;

use crate::CpuImageFrame;

/// Live GPU preview state for `CanvasTool::Transform`.
#[derive(Debug, Clone)]
pub struct TransformPreview {
    pub base_frame: Option<CpuImageFrame>,
    pub layer_frame: CpuImageFrame,
    pub layer_offset: [i32; 2],
    pub layer_size: [u32; 2],
    pub spec: LayerTransformSpec,
}

/// Interactive handles on the Free Transform oriented bounding box (OBB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    Rotate,
    Pivot,
}

impl TransformHandle {
    pub const RESIZE_HANDLES: [Self; 8] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Right,
        Self::BottomRight,
        Self::Bottom,
        Self::BottomLeft,
        Self::Left,
    ];

    pub fn local_coords(self, lw: f32, lh: f32, spec: LayerTransformSpec) -> [f32; 2] {
        match self {
            Self::TopLeft => [0.0, 0.0],
            Self::Top => [lw * 0.5, 0.0],
            Self::TopRight => [lw, 0.0],
            Self::Right => [lw, lh * 0.5],
            Self::BottomRight => [lw, lh],
            Self::Bottom => [lw * 0.5, lh],
            Self::BottomLeft => [0.0, lh],
            Self::Left => [0.0, lh * 0.5],
            Self::Rotate => [lw * 0.5, 0.0],
            Self::Pivot => [spec.pivot_x * lw, spec.pivot_y * lh],
        }
    }

    /// `(hx, hy)` signs indicating which sides of the layer box this handle moves.
    pub fn axis_signs(self) -> (f32, f32) {
        match self {
            Self::TopLeft => (-1.0, -1.0),
            Self::Top => (0.0, -1.0),
            Self::TopRight => (1.0, -1.0),
            Self::Right => (1.0, 0.0),
            Self::BottomRight => (1.0, 1.0),
            Self::Bottom => (0.0, 1.0),
            Self::BottomLeft => (-1.0, 1.0),
            Self::Left => (-1.0, 0.0),
            Self::Rotate | Self::Pivot => (0.0, 0.0),
        }
    }
}

/// Active drag interaction kind for Free Transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformDragMode {
    Move,
    Scale(TransformHandle),
    Rotate,
}

/// Transform a point in layer-local pixel coordinates `[0..lw, 0..lh]` into document pixel space.
pub fn transform_local_to_doc(
    local: [f32; 2],
    layer_offset: [i32; 2],
    layer_size: [u32; 2],
    spec: LayerTransformSpec,
) -> [f32; 2] {
    let ox = layer_offset[0] as f32;
    let oy = layer_offset[1] as f32;
    let lw = layer_size[0].max(1) as f32;
    let lh = layer_size[1].max(1) as f32;
    let px = spec.pivot_x * lw;
    let py = spec.pivot_y * lh;
    let sx = spec.scale_x * if spec.flip_h { -1.0 } else { 1.0 };
    let sy = spec.scale_y * if spec.flip_v { -1.0 } else { 1.0 };
    let dx = (local[0] - px) * sx;
    let dy = (local[1] - py) * sy;
    let theta = spec.rotation_deg.to_radians();
    let c = theta.cos();
    let s = theta.sin();
    let rx = dx * c - dy * s;
    let ry = dx * s + dy * c;
    [
        ox + px + spec.translate_x + rx,
        oy + py + spec.translate_y + ry,
    ]
}

/// Transform a document pixel coordinate back into layer-local `[0..lw, 0..lh]` space.
pub fn transform_doc_to_local(
    doc: [f32; 2],
    layer_offset: [i32; 2],
    layer_size: [u32; 2],
    spec: LayerTransformSpec,
) -> [f32; 2] {
    let ox = layer_offset[0] as f32;
    let oy = layer_offset[1] as f32;
    let lw = layer_size[0].max(1) as f32;
    let lh = layer_size[1].max(1) as f32;
    let px = spec.pivot_x * lw;
    let py = spec.pivot_y * lh;
    let wx = doc[0] - (ox + px + spec.translate_x);
    let wy = doc[1] - (oy + py + spec.translate_y);
    let theta = spec.rotation_deg.to_radians();
    let c = theta.cos();
    let s = theta.sin();
    let ux = wx * c + wy * s;
    let uy = -wx * s + wy * c;
    let sx = spec.scale_x * if spec.flip_h { -1.0 } else { 1.0 };
    let sy = spec.scale_y * if spec.flip_v { -1.0 } else { 1.0 };
    let safe_sx = if sx.abs() < 1e-5 {
        if sx < 0.0 { -1e-5 } else { 1e-5 }
    } else {
        sx
    };
    let safe_sy = if sy.abs() < 1e-5 {
        if sy < 0.0 { -1e-5 } else { 1e-5 }
    } else {
        sy
    };
    [ux / safe_sx + px, uy / safe_sy + py]
}

/// Compute the 4 OBB corners `[TL, TR, BR, BL]` in document pixels.
pub fn transform_obb_corners(tp: &TransformPreview) -> [[f32; 2]; 4] {
    let lw = tp.layer_size[0].max(1) as f32;
    let lh = tp.layer_size[1].max(1) as f32;
    [
        transform_local_to_doc([0.0, 0.0], tp.layer_offset, tp.layer_size, tp.spec),
        transform_local_to_doc([lw, 0.0], tp.layer_offset, tp.layer_size, tp.spec),
        transform_local_to_doc([lw, lh], tp.layer_offset, tp.layer_size, tp.spec),
        transform_local_to_doc([0.0, lh], tp.layer_offset, tp.layer_size, tp.spec),
    ]
}

/// Compute handle positions in document pixels for the OBB (`rot_stem_doc` is stem length outside `Top`).
pub fn transform_handle_positions(
    tp: &TransformPreview,
    rot_stem_doc: f32,
) -> [(TransformHandle, [f32; 2]); 10] {
    let lw = tp.layer_size[0].max(1) as f32;
    let lh = tp.layer_size[1].max(1) as f32;
    let p = |h: TransformHandle| {
        transform_local_to_doc(
            h.local_coords(lw, lh, tp.spec),
            tp.layer_offset,
            tp.layer_size,
            tp.spec,
        )
    };
    let top_mid = p(TransformHandle::Top);
    let bot_mid = p(TransformHandle::Bottom);
    let dx = top_mid[0] - bot_mid[0];
    let dy = top_mid[1] - bot_mid[1];
    let len = (dx * dx + dy * dy).sqrt();
    let outward = if len > 1e-4 {
        [dx / len, dy / len]
    } else {
        let theta = tp.spec.rotation_deg.to_radians();
        [theta.sin(), -theta.cos()]
    };
    let rotate_pos = [
        top_mid[0] + outward[0] * rot_stem_doc,
        top_mid[1] + outward[1] * rot_stem_doc,
    ];
    [
        (TransformHandle::TopLeft, p(TransformHandle::TopLeft)),
        (TransformHandle::Top, top_mid),
        (TransformHandle::TopRight, p(TransformHandle::TopRight)),
        (TransformHandle::Right, p(TransformHandle::Right)),
        (
            TransformHandle::BottomRight,
            p(TransformHandle::BottomRight),
        ),
        (TransformHandle::Bottom, bot_mid),
        (TransformHandle::BottomLeft, p(TransformHandle::BottomLeft)),
        (TransformHandle::Left, p(TransformHandle::Left)),
        (TransformHandle::Rotate, rotate_pos),
        (TransformHandle::Pivot, p(TransformHandle::Pivot)),
    ]
}

/// Hit-test the Free Transform OBB handles (`Rotate`, 8 resize handles) and interior (`Move`).
pub fn hit_test_transform_handle(
    tp: &TransformPreview,
    doc_pt: [f32; 2],
    hit_radius_doc: f32,
    rot_stem_doc: f32,
) -> Option<TransformDragMode> {
    let handles = transform_handle_positions(tp, rot_stem_doc);
    let r2 = hit_radius_doc * hit_radius_doc;
    let dist2 = |a: [f32; 2], b: [f32; 2]| {
        let dx = a[0] - b[0];
        let dy = a[1] - b[1];
        dx * dx + dy * dy
    };

    // 1. External rotation handle
    if dist2(doc_pt, handles[8].1) <= r2 {
        return Some(TransformDragMode::Rotate);
    }

    // 2. 8 resize handles (corners & edge midpoints)
    for (handle, pos) in &handles[..8] {
        if dist2(doc_pt, *pos) <= r2 {
            return Some(TransformDragMode::Scale(*handle));
        }
    }

    // 3. Inside OBB -> Move
    let lw = tp.layer_size[0].max(1) as f32;
    let lh = tp.layer_size[1].max(1) as f32;
    let local = transform_doc_to_local(doc_pt, tp.layer_offset, tp.layer_size, tp.spec);
    if local[0] >= 0.0 && local[0] <= lw && local[1] >= 0.0 && local[1] <= lh {
        return Some(TransformDragMode::Move);
    }

    None
}

/// Compute an updated `LayerTransformSpec` for an active transform drag.
pub fn apply_transform_drag(
    tp: &TransformPreview,
    start_spec: LayerTransformSpec,
    mode: TransformDragMode,
    anchor_doc: [f32; 2],
    cur_doc: [f32; 2],
    shift: bool,
    alt: bool,
) -> LayerTransformSpec {
    let layer_offset = tp.layer_offset;
    let layer_size = tp.layer_size;
    let lw = layer_size[0].max(1) as f32;
    let lh = layer_size[1].max(1) as f32;
    let mut spec = start_spec;

    match mode {
        TransformDragMode::Move => {
            let mut dx = cur_doc[0] - anchor_doc[0];
            let mut dy = cur_doc[1] - anchor_doc[1];
            if shift {
                if dx.abs() >= dy.abs() {
                    dy = 0.0;
                } else {
                    dx = 0.0;
                }
            }
            spec.translate_x = start_spec.translate_x + dx;
            spec.translate_y = start_spec.translate_y + dy;
        }
        TransformDragMode::Rotate => {
            let piv_doc = transform_local_to_doc(
                [start_spec.pivot_x * lw, start_spec.pivot_y * lh],
                layer_offset,
                layer_size,
                start_spec,
            );
            let a0 = (anchor_doc[1] - piv_doc[1]).atan2(anchor_doc[0] - piv_doc[0]);
            let a1 = (cur_doc[1] - piv_doc[1]).atan2(cur_doc[0] - piv_doc[0]);
            let mut delta_deg = (a1 - a0).to_degrees();
            while delta_deg > 180.0 {
                delta_deg -= 360.0;
            }
            while delta_deg < -180.0 {
                delta_deg += 360.0;
            }
            let mut deg = start_spec.rotation_deg + delta_deg;
            if shift {
                deg = (deg / 15.0).round() * 15.0;
            }
            spec.rotation_deg = deg;
        }
        TransformDragMode::Scale(handle) => {
            let (hx, hy) = handle.axis_signs();
            if hx == 0.0 && hy == 0.0 {
                return spec;
            }
            let theta = start_spec.rotation_deg.to_radians();
            let c = theta.cos();
            let s = theta.sin();
            let flip_x = if start_spec.flip_h { -1.0 } else { 1.0 };
            let flip_y = if start_spec.flip_v { -1.0 } else { 1.0 };
            let u_dir = [c * flip_x, s * flip_x];
            let v_dir = [-s * flip_y, c * flip_y];

            let d_doc = [cur_doc[0] - anchor_doc[0], cur_doc[1] - anchor_doc[1]];
            let du = d_doc[0] * u_dir[0] + d_doc[1] * u_dir[1];
            let dv = d_doc[0] * v_dir[0] + d_doc[1] * v_dir[1];

            let mut new_sx = if hx != 0.0 {
                if alt {
                    let denom_x = if hx > 0.0 {
                        (1.0 - start_spec.pivot_x).max(0.1) * lw
                    } else {
                        start_spec.pivot_x.max(0.1) * lw
                    };
                    (start_spec.scale_x + (hx * du) / denom_x).max(0.01)
                } else {
                    ((start_spec.scale_x * lw + hx * du) / lw).max(0.01)
                }
            } else {
                start_spec.scale_x
            };

            let mut new_sy = if hy != 0.0 {
                if alt {
                    let denom_y = if hy > 0.0 {
                        (1.0 - start_spec.pivot_y).max(0.1) * lh
                    } else {
                        start_spec.pivot_y.max(0.1) * lh
                    };
                    (start_spec.scale_y + (hy * dv) / denom_y).max(0.01)
                } else {
                    ((start_spec.scale_y * lh + hy * dv) / lh).max(0.01)
                }
            } else {
                start_spec.scale_y
            };

            if shift {
                let rx = new_sx / start_spec.scale_x.max(1e-4);
                let ry = new_sy / start_spec.scale_y.max(1e-4);
                let ratio = if hx != 0.0 && hy != 0.0 {
                    if (rx - 1.0).abs() >= (ry - 1.0).abs() {
                        rx
                    } else {
                        ry
                    }
                } else if hx != 0.0 {
                    rx
                } else {
                    ry
                };
                new_sx = (start_spec.scale_x * ratio).max(0.01);
                new_sy = (start_spec.scale_y * ratio).max(0.01);
            }

            spec.scale_x = new_sx;
            spec.scale_y = new_sy;

            if !alt {
                let fx = if hx > 0.0 {
                    0.0
                } else if hx < 0.0 {
                    lw
                } else {
                    start_spec.pivot_x * lw
                };
                let fy = if hy > 0.0 {
                    0.0
                } else if hy < 0.0 {
                    lh
                } else {
                    start_spec.pivot_y * lh
                };
                let fixed_before =
                    transform_local_to_doc([fx, fy], layer_offset, layer_size, start_spec);
                let fixed_after = transform_local_to_doc([fx, fy], layer_offset, layer_size, spec);
                spec.translate_x += fixed_before[0] - fixed_after[0];
                spec.translate_y += fixed_before[1] - fixed_after[1];
            }
        }
    }

    spec
}
