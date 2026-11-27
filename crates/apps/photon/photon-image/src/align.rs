//! Pure-geometry Alignment & Distribution (even spacing) for bounding boxes.

use photon_protocol::{AlignOp, DistributeOp};

/// An item with a unique ID and axis-aligned bounding box `[min_x, min_y, max_x, max_y]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlignItem {
    pub id: u64,
    pub bounds: [f32; 4],
}

impl AlignItem {
    #[inline]
    pub fn width(&self) -> f32 {
        (self.bounds[2] - self.bounds[0]).max(0.0)
    }

    #[inline]
    pub fn height(&self) -> f32 {
        (self.bounds[3] - self.bounds[1]).max(0.0)
    }

    #[inline]
    pub fn center_x(&self) -> f32 {
        (self.bounds[0] + self.bounds[2]) * 0.5
    }

    #[inline]
    pub fn center_y(&self) -> f32 {
        (self.bounds[1] + self.bounds[3]) * 0.5
    }
}

/// Compute the union `[min_x, min_y, max_x, max_y]` of `items`.
pub fn union_bounds(items: &[AlignItem]) -> Option<[f32; 4]> {
    let first = items.first()?;
    let mut b = first.bounds;
    for item in &items[1..] {
        b[0] = b[0].min(item.bounds[0]);
        b[1] = b[1].min(item.bounds[1]);
        b[2] = b[2].max(item.bounds[2]);
        b[3] = b[3].max(item.bounds[3]);
    }
    Some(b)
}

/// Compute `(id, [dx, dy])` translation deltas to align `items` to `reference_bounds`.
pub fn compute_align_deltas(
    items: &[AlignItem],
    op: AlignOp,
    reference_bounds: [f32; 4],
) -> Vec<(u64, [f32; 2])> {
    let [ref_min_x, ref_min_y, ref_max_x, ref_max_y] = reference_bounds;
    let ref_cx = (ref_min_x + ref_max_x) * 0.5;
    let ref_cy = (ref_min_y + ref_max_y) * 0.5;

    items
        .iter()
        .map(|item| {
            let delta = match op {
                AlignOp::Left => [ref_min_x - item.bounds[0], 0.0],
                AlignOp::CenterH => [ref_cx - item.center_x(), 0.0],
                AlignOp::Right => [ref_max_x - item.bounds[2], 0.0],
                AlignOp::Top => [0.0, ref_min_y - item.bounds[1]],
                AlignOp::CenterV => [0.0, ref_cy - item.center_y()],
                AlignOp::Bottom => [0.0, ref_max_y - item.bounds[3]],
            };
            (item.id, delta)
        })
        .collect()
}

/// Compute `(id, [dx, dy])` translation deltas to distribute or evenly space `items`.
///
/// - For `SpaceH` / `SpaceV`, requires `>= 3` items in auto mode (or `>= 2` when `fixed_gap_px` is set).
/// - For edge/center distribute modes, requires `>= 3` items.
pub fn compute_distribute_deltas(
    items: &[AlignItem],
    op: DistributeOp,
    fixed_gap_px: Option<f32>,
) -> Vec<(u64, [f32; 2])> {
    let min_count =
        if matches!(op, DistributeOp::SpaceH | DistributeOp::SpaceV) && fixed_gap_px.is_some() {
            2
        } else {
            3
        };
    if items.len() < min_count {
        return Vec::new();
    }

    let mut sorted = items.to_vec();
    match op {
        DistributeOp::Left | DistributeOp::SpaceH => {
            sorted.sort_by(|a, b| a.bounds[0].total_cmp(&b.bounds[0]));
        }
        DistributeOp::CenterH => {
            sorted.sort_by(|a, b| a.center_x().total_cmp(&b.center_x()));
        }
        DistributeOp::Right => {
            sorted.sort_by(|a, b| a.bounds[2].total_cmp(&b.bounds[2]));
        }
        DistributeOp::Top | DistributeOp::SpaceV => {
            sorted.sort_by(|a, b| a.bounds[1].total_cmp(&b.bounds[1]));
        }
        DistributeOp::CenterV => {
            sorted.sort_by(|a, b| a.center_y().total_cmp(&b.center_y()));
        }
        DistributeOp::Bottom => {
            sorted.sort_by(|a, b| a.bounds[3].total_cmp(&b.bounds[3]));
        }
    }

    let n = sorted.len();
    let mut deltas = Vec::with_capacity(n);

    match op {
        DistributeOp::SpaceH => {
            let gap = match fixed_gap_px {
                Some(g) => g,
                None => {
                    let min_x = sorted
                        .iter()
                        .map(|i| i.bounds[0])
                        .fold(f32::INFINITY, f32::min);
                    let max_x = sorted
                        .iter()
                        .map(|i| i.bounds[2])
                        .fold(f32::NEG_INFINITY, f32::max);
                    let sum_w: f32 = sorted.iter().map(AlignItem::width).sum();
                    (max_x - min_x - sum_w) / (n - 1) as f32
                }
            };
            let mut cursor_x = sorted[0].bounds[2] + gap;
            deltas.push((sorted[0].id, [0.0, 0.0]));
            for item in &sorted[1..] {
                let dx = cursor_x - item.bounds[0];
                deltas.push((item.id, [dx, 0.0]));
                cursor_x += item.width() + gap;
            }
        }
        DistributeOp::SpaceV => {
            let gap = match fixed_gap_px {
                Some(g) => g,
                None => {
                    let min_y = sorted
                        .iter()
                        .map(|i| i.bounds[1])
                        .fold(f32::INFINITY, f32::min);
                    let max_y = sorted
                        .iter()
                        .map(|i| i.bounds[3])
                        .fold(f32::NEG_INFINITY, f32::max);
                    let sum_h: f32 = sorted.iter().map(AlignItem::height).sum();
                    (max_y - min_y - sum_h) / (n - 1) as f32
                }
            };
            let mut cursor_y = sorted[0].bounds[3] + gap;
            deltas.push((sorted[0].id, [0.0, 0.0]));
            for item in &sorted[1..] {
                let dy = cursor_y - item.bounds[1];
                deltas.push((item.id, [0.0, dy]));
                cursor_y += item.height() + gap;
            }
        }
        DistributeOp::Left => {
            let start = sorted[0].bounds[0];
            let step = (sorted[n - 1].bounds[0] - start) / (n - 1) as f32;
            for (idx, item) in sorted.iter().enumerate() {
                deltas.push((item.id, [start + step * idx as f32 - item.bounds[0], 0.0]));
            }
        }
        DistributeOp::CenterH => {
            let start = sorted[0].center_x();
            let step = (sorted[n - 1].center_x() - start) / (n - 1) as f32;
            for (idx, item) in sorted.iter().enumerate() {
                deltas.push((item.id, [start + step * idx as f32 - item.center_x(), 0.0]));
            }
        }
        DistributeOp::Right => {
            let start = sorted[0].bounds[2];
            let step = (sorted[n - 1].bounds[2] - start) / (n - 1) as f32;
            for (idx, item) in sorted.iter().enumerate() {
                deltas.push((item.id, [start + step * idx as f32 - item.bounds[2], 0.0]));
            }
        }
        DistributeOp::Top => {
            let start = sorted[0].bounds[1];
            let step = (sorted[n - 1].bounds[1] - start) / (n - 1) as f32;
            for (idx, item) in sorted.iter().enumerate() {
                deltas.push((item.id, [0.0, start + step * idx as f32 - item.bounds[1]]));
            }
        }
        DistributeOp::CenterV => {
            let start = sorted[0].center_y();
            let step = (sorted[n - 1].center_y() - start) / (n - 1) as f32;
            for (idx, item) in sorted.iter().enumerate() {
                deltas.push((item.id, [0.0, start + step * idx as f32 - item.center_y()]));
            }
        }
        DistributeOp::Bottom => {
            let start = sorted[0].bounds[3];
            let step = (sorted[n - 1].bounds[3] - start) / (n - 1) as f32;
            for (idx, item) in sorted.iter().enumerate() {
                deltas.push((item.id, [0.0, start + step * idx as f32 - item.bounds[3]]));
            }
        }
    }

    deltas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_deltas_align_to_reference_bounds() {
        let items = vec![
            AlignItem {
                id: 1,
                bounds: [10.0, 20.0, 50.0, 40.0],
            },
            AlignItem {
                id: 2,
                bounds: [80.0, 60.0, 120.0, 100.0],
            },
        ];
        let ref_b = [0.0, 0.0, 200.0, 200.0];
        let left = compute_align_deltas(&items, AlignOp::Left, ref_b);
        assert_eq!(left, vec![(1, [-10.0, 0.0]), (2, [-80.0, 0.0])]);

        let cx = compute_align_deltas(&items, AlignOp::CenterH, ref_b);
        assert_eq!(cx, vec![(1, [70.0, 0.0]), (2, [0.0, 0.0])]);

        let bot = compute_align_deltas(&items, AlignOp::Bottom, ref_b);
        assert_eq!(bot, vec![(1, [0.0, 160.0]), (2, [0.0, 100.0])]);
    }

    #[test]
    fn distribute_space_h_and_v_equalizes_gaps_for_uneven_sizes() {
        // Items: [0..20] (w=20), [30..80] (w=50), [140..170] (w=30)
        // Total span = 170, sum_w = 100 -> gap = (170 - 100)/2 = 35
        // Expected positions: [0..20], [55..105], [140..170]
        let items = vec![
            AlignItem {
                id: 1,
                bounds: [0.0, 0.0, 20.0, 10.0],
            },
            AlignItem {
                id: 2,
                bounds: [30.0, 15.0, 80.0, 45.0],
            },
            AlignItem {
                id: 3,
                bounds: [140.0, 90.0, 170.0, 100.0],
            },
        ];
        let dh = compute_distribute_deltas(&items, DistributeOp::SpaceH, None);
        assert_eq!(dh, vec![(1, [0.0, 0.0]), (2, [25.0, 0.0]), (3, [0.0, 0.0])]);

        // Fixed gap 10px between 2 items
        let fixed = compute_distribute_deltas(&items[..2], DistributeOp::SpaceH, Some(10.0));
        assert_eq!(fixed, vec![(1, [0.0, 0.0]), (2, [0.0, 0.0])]);

        // SpaceV: span=100, sum_h = 10+30+10 = 50 -> gap = 25 -> [0..10], [35..65], [90..100]
        let dv = compute_distribute_deltas(&items, DistributeOp::SpaceV, None);
        assert_eq!(dv, vec![(1, [0.0, 0.0]), (2, [0.0, 20.0]), (3, [0.0, 0.0])]);
    }
}
