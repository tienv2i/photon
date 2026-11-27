//! 8-bit CoW pixel selection mask, rasterizers (Marquee, Ellipse, Single Row/Col,
//! Polygon/Lasso, Magic Wand, Color Range, Quick Selection), boolean ops, and
//! morphological/color modifications for `photon-image`.

use std::collections::VecDeque;
use std::sync::Arc;

use image::RgbaImage;
use photon_protocol::{SelectionBooleanOp, SelectionModifyOp, SelectionShape};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionSnapshot {
    pub width: u32,
    pub height: u32,
    pub alpha: Arc<Vec<u8>>,
    pub bounds: Option<[u32; 4]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatingSelection {
    pub layer_id: u64,
    pub pixels: Arc<RgbaImage>,
    pub offset_x: i32,
    pub offset_y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedSelectionChannel {
    pub id: u64,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub alpha: Arc<Vec<u8>>,
}

/// Non-destructive 8-bit grayscale pixel selection mask matching canvas dimensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionMask {
    pub width: u32,
    pub height: u32,
    pub alpha: Arc<Vec<u8>>,
    pub bounds: Option<[u32; 4]>,
    pub reselect: Option<SelectionSnapshot>,
    pub quick_mask_active: bool,
    pub saved_selections: Vec<SavedSelectionChannel>,
    pub next_saved_id: u64,
    pub floating: Option<FloatingSelection>,
}

impl SelectionMask {
    pub fn empty(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            alpha: Arc::new(vec![0; (width as usize).saturating_mul(height as usize)]),
            bounds: None,
            reselect: None,
            quick_mask_active: false,
            saved_selections: Vec::new(),
            next_saved_id: 1,
            floating: None,
        }
    }

    pub fn all(width: u32, height: u32) -> Self {
        let bounds = (width > 0 && height > 0).then_some([0, 0, width, height]);
        Self {
            width,
            height,
            alpha: Arc::new(vec![255; (width as usize).saturating_mul(height as usize)]),
            bounds,
            reselect: None,
            quick_mask_active: false,
            saved_selections: Vec::new(),
            next_saved_id: 1,
            floating: None,
        }
    }

    #[inline]
    pub fn has_selection(&self) -> bool {
        self.bounds.is_some()
    }

    #[inline]
    pub fn can_reselect(&self) -> bool {
        self.reselect.is_some()
    }

    /// Returns `0..=255` at canvas `(x, y)` (0 if out of bounds).
    #[inline]
    pub fn alpha_at(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 {
            return 0;
        }
        let (ux, uy) = (x as u32, y as u32);
        if ux >= self.width || uy >= self.height {
            return 0;
        }
        self.alpha[(uy as usize) * (self.width as usize) + (ux as usize)]
    }

    /// Clipping factor `[0.0, 1.0]` for editing tools; returns `1.0` when no selection is active
    /// (unless Quick Mask mode is active, in which case strokes target the mask directly).
    #[inline]
    pub fn clip_factor(&self, x: i32, y: i32) -> f32 {
        if !self.has_selection() {
            return 1.0;
        }
        self.alpha_at(x, y) as f32 / 255.0
    }

    pub fn recompute_bounds(&mut self) {
        self.bounds = compute_bounds(&self.alpha, self.width, self.height);
    }

    pub fn combine(&mut self, incoming: &[u8], op: SelectionBooleanOp) {
        let len = (self.width as usize).saturating_mul(self.height as usize);
        if incoming.len() != len {
            return;
        }
        let buf = Arc::make_mut(&mut self.alpha);
        if buf.len() != len {
            buf.resize(len, 0);
        }
        for (dst, &src) in buf.iter_mut().zip(incoming.iter()) {
            *dst = combine_alpha(*dst, src, op);
        }
        self.recompute_bounds();
    }

    pub fn save_selection(
        &mut self,
        name: String,
        target_id: Option<u64>,
        op: SelectionBooleanOp,
    ) -> u64 {
        let trimmed = name.trim();
        if let Some(tid) = target_id
            && let Some(existing) = self
                .saved_selections
                .iter_mut()
                .find(|s| s.id == tid && s.width == self.width && s.height == self.height)
        {
            if op == SelectionBooleanOp::Replace {
                existing.alpha = Arc::clone(&self.alpha);
            } else {
                let buf = Arc::make_mut(&mut existing.alpha);
                for (dst, &src) in buf.iter_mut().zip(self.alpha.iter()) {
                    *dst = combine_alpha(*dst, src, op);
                }
            }
            if !trimmed.is_empty() {
                existing.name = trimmed.to_string();
            }
            return existing.id;
        }
        let id = self.next_saved_id.max(1);
        self.next_saved_id = id + 1;
        let resolved_name = if trimmed.is_empty() {
            format!("Selection {id}")
        } else {
            trimmed.to_string()
        };
        self.saved_selections.push(SavedSelectionChannel {
            id,
            name: resolved_name,
            width: self.width,
            height: self.height,
            alpha: Arc::clone(&self.alpha),
        });
        id
    }

    pub fn load_selection(&mut self, id: u64, op: SelectionBooleanOp, invert: bool) -> bool {
        let Some(channel) = self
            .saved_selections
            .iter()
            .find(|s| s.id == id && s.width == self.width && s.height == self.height)
            .cloned()
        else {
            return false;
        };
        if invert {
            let inverted: Vec<u8> = channel.alpha.iter().map(|&a| 255 - a).collect();
            self.combine(&inverted, op);
        } else {
            self.combine(&channel.alpha, op);
        }
        true
    }

    pub fn delete_saved_selection(&mut self, id: u64) -> bool {
        let before = self.saved_selections.len();
        self.saved_selections.retain(|s| s.id != id);
        self.saved_selections.len() != before
    }
}

#[inline]
fn combine_alpha(old: u8, new: u8, op: SelectionBooleanOp) -> u8 {
    let o = old as u16;
    let n = new as u16;
    match op {
        SelectionBooleanOp::Replace => new,
        SelectionBooleanOp::Add => old.max(new),
        SelectionBooleanOp::Subtract => ((o * (255 - n) + 127) / 255) as u8,
        SelectionBooleanOp::Intersect => ((o * n + 127) / 255) as u8,
        SelectionBooleanOp::SymmetricDiff => old.abs_diff(new),
    }
}

impl SelectionMask {
    pub fn select_all(&mut self) {
        let len = (self.width as usize).saturating_mul(self.height as usize);
        let buf = Arc::make_mut(&mut self.alpha);
        buf.resize(len, 255);
        buf.fill(255);
        self.bounds =
            (self.width > 0 && self.height > 0).then_some([0, 0, self.width, self.height]);
    }

    pub fn deselect(&mut self) {
        if self.has_selection() {
            self.reselect = Some(SelectionSnapshot {
                width: self.width,
                height: self.height,
                alpha: Arc::clone(&self.alpha),
                bounds: self.bounds,
            });
        }
        let len = (self.width as usize).saturating_mul(self.height as usize);
        let buf = Arc::make_mut(&mut self.alpha);
        buf.resize(len, 0);
        buf.fill(0);
        self.bounds = None;
        self.quick_mask_active = false;
    }

    pub fn reselect(&mut self) -> bool {
        let Some(snap) = self.reselect.take() else {
            return false;
        };
        if snap.width == self.width && snap.height == self.height {
            self.alpha = snap.alpha;
            self.bounds = snap.bounds;
            return true;
        }
        false
    }

    pub fn invert(&mut self) {
        let len = (self.width as usize).saturating_mul(self.height as usize);
        let buf = Arc::make_mut(&mut self.alpha);
        buf.resize(len, 0);
        for px in buf.iter_mut() {
            *px = 255 - *px;
        }
        self.recompute_bounds();
    }

    pub fn modify(&mut self, op: SelectionModifyOp, source: &RgbaImage) {
        let (w, h) = (self.width, self.height);
        if w == 0 || h == 0 {
            return;
        }
        let next = match op {
            SelectionModifyOp::Feather(radius) => gaussian_feather(&self.alpha, w, h, radius),
            SelectionModifyOp::Expand(px) => morph_dilate(&self.alpha, w, h, px),
            SelectionModifyOp::Contract(px) => morph_erode(&self.alpha, w, h, px),
            SelectionModifyOp::Border(width) => {
                let half = (width / 2).max(1);
                let outer = morph_dilate(&self.alpha, w, h, half);
                let inner = morph_erode(&self.alpha, w, h, half);
                outer
                    .iter()
                    .zip(inner.iter())
                    .map(|(&o, &i)| o.saturating_sub(i))
                    .collect()
            }
            SelectionModifyOp::Smooth(radius) => {
                let blurred = gaussian_feather(&self.alpha, w, h, radius as f32);
                blurred
                    .into_iter()
                    .map(|v| if v >= 128 { 255 } else { 0 })
                    .collect()
            }
            SelectionModifyOp::Grow(tol) => grow_selection(&self.alpha, source, w, h, tol, true),
            SelectionModifyOp::Similar(tol) => {
                grow_selection(&self.alpha, source, w, h, tol, false)
            }
        };
        self.alpha = Arc::new(next);
        self.recompute_bounds();
    }

    pub fn crop(&self, cx: i32, cy: i32, new_w: u32, new_h: u32) -> Self {
        let mut out = vec![0u8; (new_w as usize).saturating_mul(new_h as usize)];
        for ny in 0..new_h {
            let sy = cy + ny as i32;
            if sy < 0 || sy >= self.height as i32 {
                continue;
            }
            for nx in 0..new_w {
                let sx = cx + nx as i32;
                if sx >= 0 && sx < self.width as i32 {
                    out[(ny as usize) * (new_w as usize) + (nx as usize)] =
                        self.alpha[(sy as usize) * (self.width as usize) + (sx as usize)];
                }
            }
        }
        let bounds = compute_bounds(&out, new_w, new_h);
        Self {
            width: new_w,
            height: new_h,
            alpha: Arc::new(out),
            bounds,
            reselect: None,
            quick_mask_active: self.quick_mask_active,
            saved_selections: Vec::new(),
            next_saved_id: 1,
            floating: None,
        }
    }

    pub fn resize(&self, new_w: u32, new_h: u32) -> Self {
        if new_w == 0 || new_h == 0 || self.width == 0 || self.height == 0 {
            return Self::empty(new_w, new_h);
        }
        let mut out = vec![0u8; (new_w as usize).saturating_mul(new_h as usize)];
        for ny in 0..new_h {
            let sy = ((ny as f32 + 0.5) * self.height as f32 / new_h as f32 - 0.5)
                .clamp(0.0, (self.height - 1) as f32);
            let y0 = sy.floor() as u32;
            let y1 = (y0 + 1).min(self.height - 1);
            let fy = sy - y0 as f32;
            for nx in 0..new_w {
                let sx = ((nx as f32 + 0.5) * self.width as f32 / new_w as f32 - 0.5)
                    .clamp(0.0, (self.width - 1) as f32);
                let x0 = sx.floor() as u32;
                let x1 = (x0 + 1).min(self.width - 1);
                let fx = sx - x0 as f32;
                let a00 = self.alpha[(y0 * self.width + x0) as usize] as f32;
                let a10 = self.alpha[(y0 * self.width + x1) as usize] as f32;
                let a01 = self.alpha[(y1 * self.width + x0) as usize] as f32;
                let a11 = self.alpha[(y1 * self.width + x1) as usize] as f32;
                let v = a00 * (1.0 - fx) * (1.0 - fy)
                    + a10 * fx * (1.0 - fy)
                    + a01 * (1.0 - fx) * fy
                    + a11 * fx * fy;
                out[(ny * new_w + nx) as usize] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
        let bounds = compute_bounds(&out, new_w, new_h);
        Self {
            width: new_w,
            height: new_h,
            alpha: Arc::new(out),
            bounds,
            reselect: None,
            quick_mask_active: self.quick_mask_active,
            saved_selections: Vec::new(),
            next_saved_id: 1,
            floating: None,
        }
    }

    pub fn rotate_90(&self, turns: i32) -> Self {
        let t = turns.rem_euclid(4);
        if t == 0 {
            return self.clone();
        }
        let (w, h) = (self.width, self.height);
        let (nw, nh) = if t % 2 == 1 { (h, w) } else { (w, h) };
        let mut out = vec![0u8; (nw as usize).saturating_mul(nh as usize)];
        for y in 0..h {
            for x in 0..w {
                let val = self.alpha[(y * w + x) as usize];
                let (dx, dy) = match t {
                    1 => (h - 1 - y, x),
                    2 => (w - 1 - x, h - 1 - y),
                    3 => (y, w - 1 - x),
                    _ => (x, y),
                };
                out[(dy * nw + dx) as usize] = val;
            }
        }
        let bounds = compute_bounds(&out, nw, nh);
        Self {
            width: nw,
            height: nh,
            alpha: Arc::new(out),
            bounds,
            reselect: None,
            quick_mask_active: self.quick_mask_active,
            saved_selections: Vec::new(),
            next_saved_id: 1,
            floating: None,
        }
    }

    pub fn flip(&self, horizontal: bool) -> Self {
        let (w, h) = (self.width, self.height);
        let mut out = vec![0u8; (w as usize).saturating_mul(h as usize)];
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = if horizontal {
                    (w - 1 - x, y)
                } else {
                    (x, h - 1 - y)
                };
                out[(dy * w + dx) as usize] = self.alpha[(y * w + x) as usize];
            }
        }
        let bounds = compute_bounds(&out, w, h);
        Self {
            width: w,
            height: h,
            alpha: Arc::new(out),
            bounds,
            reselect: None,
            quick_mask_active: self.quick_mask_active,
            saved_selections: Vec::new(),
            next_saved_id: 1,
            floating: None,
        }
    }
}

pub fn compute_bounds(alpha: &[u8], width: u32, height: u32) -> Option<[u32; 4]> {
    if width == 0 || height == 0 || alpha.len() < (width as usize) * (height as usize) {
        return None;
    }
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut any = false;
    for y in 0..height {
        let row = &alpha[(y as usize) * (width as usize)..((y + 1) as usize) * (width as usize)];
        for (x, &v) in row.iter().enumerate() {
            if v > 0 {
                let ux = x as u32;
                min_x = min_x.min(ux);
                min_y = min_y.min(y);
                max_x = max_x.max(ux);
                max_y = max_y.max(y);
                any = true;
            }
        }
    }
    any.then(|| [min_x, min_y, max_x - min_x + 1, max_y - min_y + 1])
}

/// Rasterize a [`SelectionShape`] onto a canvas-sized `0..=255` alpha buffer.
pub fn rasterize_shape(
    width: u32,
    height: u32,
    shape: &SelectionShape,
    composite: &RgbaImage,
    active_layer: Option<(&RgbaImage, i32, i32)>,
) -> Vec<u8> {
    let mut out = vec![0u8; (width as usize).saturating_mul(height as usize)];
    if width == 0 || height == 0 {
        return out;
    }
    match shape {
        SelectionShape::Rect {
            x,
            y,
            width: rw,
            height: rh,
            feather,
            anti_alias,
        } => {
            let x0 = x.min(x + rw);
            let y0 = y.min(y + rh);
            let x1 = x.max(x + rw);
            let y1 = y.max(y + rh);
            let sx = x0.floor().max(0.0) as u32;
            let sy = y0.floor().max(0.0) as u32;
            let ex = x1.ceil().min(width as f32).max(0.0) as u32;
            let ey = y1.ceil().min(height as f32).max(0.0) as u32;
            for py in sy..ey {
                let cy = py as f32 + 0.5;
                let vy = if *anti_alias {
                    ((cy - y0).clamp(0.0, 1.0)).min((y1 - cy).clamp(0.0, 1.0))
                } else if cy >= y0 && cy <= y1 {
                    1.0
                } else {
                    0.0
                };
                for px in sx..ex {
                    let cx = px as f32 + 0.5;
                    let vx = if *anti_alias {
                        ((cx - x0).clamp(0.0, 1.0)).min((x1 - cx).clamp(0.0, 1.0))
                    } else if cx >= x0 && cx <= x1 {
                        1.0
                    } else {
                        0.0
                    };
                    out[(py * width + px) as usize] =
                        (vx * vy * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
            if *feather > 0.2 {
                out = gaussian_feather(&out, width, height, *feather);
            }
        }
        SelectionShape::Ellipse {
            cx,
            cy,
            rx,
            ry,
            feather,
            anti_alias,
        } => {
            let rx = rx.abs().max(0.5);
            let ry = ry.abs().max(0.5);
            let sx = (*cx - rx - 1.0).floor().max(0.0) as u32;
            let sy = (*cy - ry - 1.0).floor().max(0.0) as u32;
            let ex = (*cx + rx + 1.0).ceil().min(width as f32).max(0.0) as u32;
            let ey = (*cy + ry + 1.0).ceil().min(height as f32).max(0.0) as u32;
            let inv_rx = 1.0 / rx;
            let inv_ry = 1.0 / ry;
            for py in sy..ey {
                let dy = (py as f32 + 0.5 - *cy) * inv_ry;
                for px in sx..ex {
                    let dx = (px as f32 + 0.5 - *cx) * inv_rx;
                    let dist = (dx * dx + dy * dy).sqrt();
                    let cov = if *anti_alias {
                        let px_step = (inv_rx + inv_ry) * 0.5;
                        ((1.0 - dist) / px_step + 0.5).clamp(0.0, 1.0)
                    } else if dist <= 1.0 {
                        1.0
                    } else {
                        0.0
                    };
                    out[(py * width + px) as usize] = (cov * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
            if *feather > 0.2 {
                out = gaussian_feather(&out, width, height, *feather);
            }
        }
        SelectionShape::SingleRow { y } => {
            if *y < height {
                let start = (*y as usize) * (width as usize);
                out[start..start + width as usize].fill(255);
            }
        }
        SelectionShape::SingleCol { x } => {
            if *x < width {
                for y in 0..height {
                    out[(y * width + *x) as usize] = 255;
                }
            }
        }
        SelectionShape::Polygon {
            points,
            feather,
            anti_alias,
        } => {
            if points.len() >= 3 {
                rasterize_polygon(points, width, height, *anti_alias, &mut out);
                if *feather > 0.2 {
                    out = gaussian_feather(&out, width, height, *feather);
                }
            }
        }
        SelectionShape::MagicWand {
            x,
            y,
            tolerance,
            contiguous,
            sample_all_layers,
            anti_alias,
        } => {
            let sample = pick_sample_image(composite, active_layer, *sample_all_layers);
            let (ox, oy) = if *sample_all_layers {
                (0, 0)
            } else {
                active_layer.map(|(_, lx, ly)| (lx, ly)).unwrap_or((0, 0))
            };
            magic_wand_fill(
                sample,
                ox,
                oy,
                width,
                height,
                *x,
                *y,
                *tolerance,
                *contiguous,
                &mut out,
            );
            if *anti_alias {
                let soft = gaussian_feather(&out, width, height, 0.6);
                for (d, s) in out.iter_mut().zip(soft.iter()) {
                    if *d == 0 || *d == 255 {
                        *d = ((*d as u16 + *s as u16) / 2) as u8;
                    }
                }
            }
        }
        SelectionShape::ColorRange {
            rgb,
            fuzziness,
            sample_all_layers,
        } => {
            let sample = pick_sample_image(composite, active_layer, *sample_all_layers);
            let (ox, oy) = if *sample_all_layers {
                (0, 0)
            } else {
                active_layer.map(|(_, lx, ly)| (lx, ly)).unwrap_or((0, 0))
            };
            let fuzz = (*fuzziness as f32).max(1.0);
            for py in 0..height {
                for px in 0..width {
                    let sx = px as i32 - ox;
                    let sy = py as i32 - oy;
                    if sx < 0
                        || sy < 0
                        || sx >= sample.width() as i32
                        || sy >= sample.height() as i32
                    {
                        continue;
                    }
                    let p = sample.get_pixel(sx as u32, sy as u32).0;
                    if p[3] == 0 {
                        continue;
                    }
                    let dist = color_dist_rgb([p[0], p[1], p[2]], *rgb);
                    if dist <= fuzz {
                        let cov = (1.0 - dist / (fuzz * 1.15)).clamp(0.0, 1.0);
                        out[(py * width + px) as usize] = (cov * 255.0).round() as u8;
                    }
                }
            }
        }
        SelectionShape::QuickSelectStroke {
            points,
            radius,
            sample_all_layers,
        } => {
            let sample = pick_sample_image(composite, active_layer, *sample_all_layers);
            let (ox, oy) = if *sample_all_layers {
                (0, 0)
            } else {
                active_layer.map(|(_, lx, ly)| (lx, ly)).unwrap_or((0, 0))
            };
            let tol = 32u8;
            let max_r = (*radius * 2.5).max(8.0);
            for pt in points {
                let cx = pt[0].round() as i32;
                let cy = pt[1].round() as i32;
                if cx < 0 || cy < 0 || cx >= width as i32 || cy >= height as i32 {
                    continue;
                }
                let sx = cx - ox;
                let sy = cy - oy;
                if sx < 0 || sy < 0 || sx >= sample.width() as i32 || sy >= sample.height() as i32 {
                    continue;
                }
                let seed = sample.get_pixel(sx as u32, sy as u32).0;
                let mut q = VecDeque::new();
                q.push_back((cx as u32, cy as u32));
                while let Some((qx, qy)) = q.pop_front() {
                    let idx = (qy * width + qx) as usize;
                    if out[idx] == 255 {
                        continue;
                    }
                    let dx = qx as f32 - pt[0] as f32;
                    let dy = qy as f32 - pt[1] as f32;
                    if (dx * dx + dy * dy).sqrt() > max_r {
                        continue;
                    }
                    let lsx = qx as i32 - ox;
                    let lsy = qy as i32 - oy;
                    if lsx < 0
                        || lsy < 0
                        || lsx >= sample.width() as i32
                        || lsy >= sample.height() as i32
                    {
                        continue;
                    }
                    let px = sample.get_pixel(lsx as u32, lsy as u32).0;
                    if color_dist_rgba(px, seed) <= tol as f32 {
                        out[idx] = 255;
                        if qx > 0 {
                            q.push_back((qx - 1, qy));
                        }
                        if qx + 1 < width {
                            q.push_back((qx + 1, qy));
                        }
                        if qy > 0 {
                            q.push_back((qx, qy - 1));
                        }
                        if qy + 1 < height {
                            q.push_back((qx, qy + 1));
                        }
                    }
                }
            }
        }
    }
    out
}

fn pick_sample_image<'a>(
    composite: &'a RgbaImage,
    active_layer: Option<(&'a RgbaImage, i32, i32)>,
    sample_all_layers: bool,
) -> &'a RgbaImage {
    if sample_all_layers {
        composite
    } else {
        active_layer.map(|(img, _, _)| img).unwrap_or(composite)
    }
}

#[inline]
fn color_dist_rgb(a: [u8; 3], b: [u8; 3]) -> f32 {
    let dr = a[0] as f32 - b[0] as f32;
    let dg = a[1] as f32 - b[1] as f32;
    let db = a[2] as f32 - b[2] as f32;
    ((dr * dr + dg * dg + db * db) / 3.0).sqrt()
}

#[inline]
fn color_dist_rgba(a: [u8; 4], b: [u8; 4]) -> f32 {
    let dr = a[0] as f32 - b[0] as f32;
    let dg = a[1] as f32 - b[1] as f32;
    let db = a[2] as f32 - b[2] as f32;
    let da = a[3] as f32 - b[3] as f32;
    ((dr * dr + dg * dg + db * db + da * da) / 4.0).sqrt()
}

#[allow(clippy::too_many_arguments)]
fn magic_wand_fill(
    sample: &RgbaImage,
    ox: i32,
    oy: i32,
    width: u32,
    height: u32,
    seed_x: u32,
    seed_y: u32,
    tolerance: u8,
    contiguous: bool,
    out: &mut [u8],
) {
    if seed_x >= width || seed_y >= height {
        return;
    }
    let sx = seed_x as i32 - ox;
    let sy = seed_y as i32 - oy;
    if sx < 0 || sy < 0 || sx >= sample.width() as i32 || sy >= sample.height() as i32 {
        return;
    }
    let seed_px = sample.get_pixel(sx as u32, sy as u32).0;
    let tol = tolerance as f32;

    if !contiguous {
        for py in 0..height {
            for px in 0..width {
                let lsx = px as i32 - ox;
                let lsy = py as i32 - oy;
                if lsx < 0
                    || lsy < 0
                    || lsx >= sample.width() as i32
                    || lsy >= sample.height() as i32
                {
                    continue;
                }
                let p = sample.get_pixel(lsx as u32, lsy as u32).0;
                if color_dist_rgba(p, seed_px) <= tol {
                    out[(py * width + px) as usize] = 255;
                }
            }
        }
        return;
    }

    let mut q = VecDeque::new();
    q.push_back((seed_x, seed_y));
    while let Some((qx, qy)) = q.pop_front() {
        let idx = (qy * width + qx) as usize;
        if out[idx] == 255 {
            continue;
        }
        let lsx = qx as i32 - ox;
        let lsy = qy as i32 - oy;
        if lsx < 0 || lsy < 0 || lsx >= sample.width() as i32 || lsy >= sample.height() as i32 {
            continue;
        }
        let p = sample.get_pixel(lsx as u32, lsy as u32).0;
        if color_dist_rgba(p, seed_px) <= tol {
            out[idx] = 255;
            if qx > 0 {
                q.push_back((qx - 1, qy));
            }
            if qx + 1 < width {
                q.push_back((qx + 1, qy));
            }
            if qy > 0 {
                q.push_back((qx, qy - 1));
            }
            if qy + 1 < height {
                q.push_back((qx, qy + 1));
            }
        }
    }
}

fn rasterize_polygon(
    points: &[[f32; 2]],
    width: u32,
    height: u32,
    anti_alias: bool,
    out: &mut [u8],
) {
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    for p in points {
        min_x = min_x.min(p[0]);
        max_x = max_x.max(p[0]);
        min_y = min_y.min(p[1]);
        max_y = max_y.max(p[1]);
    }
    let sy = min_y.floor().max(0.0) as u32;
    let ey = max_y.ceil().min(height as f32).max(0.0) as u32;
    let sx = min_x.floor().max(0.0) as u32;
    let ex = max_x.ceil().min(width as f32).max(0.0) as u32;

    let sub_offsets: &[(f32, f32)] = if anti_alias {
        &[(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]
    } else {
        &[(0.5, 0.5)]
    };

    let n = points.len();
    for py in sy..ey {
        for px in sx..ex {
            let mut hits = 0u32;
            for &(ox, oy) in sub_offsets {
                let x = px as f32 + ox;
                let y = py as f32 + oy;
                let mut inside = false;
                let mut j = n - 1;
                for i in 0..n {
                    let yi = points[i][1];
                    let yj = points[j][1];
                    if (yi > y) != (yj > y) {
                        let xi = points[i][0];
                        let xj = points[j][0];
                        let intersect_x = xi + (y - yi) * (xj - xi) / (yj - yi);
                        if x < intersect_x {
                            inside = !inside;
                        }
                    }
                    j = i;
                }
                if inside {
                    hits += 1;
                }
            }
            if hits > 0 {
                let cov = (hits * 255) / sub_offsets.len() as u32;
                out[(py * width + px) as usize] = cov as u8;
            }
        }
    }
}

fn gaussian_feather(alpha: &[u8], width: u32, height: u32, radius: f32) -> Vec<u8> {
    let r = radius.ceil().clamp(0.0, 128.0) as i32;
    if r == 0 || width == 0 || height == 0 {
        return alpha.to_vec();
    }
    let w = width as i32;
    let h = height as i32;
    let mut temp = vec![0.0f32; (width as usize) * (height as usize)];
    let mut out = vec![0u8; (width as usize) * (height as usize)];
    let sigma = (radius / 2.0).max(0.5);
    let mut kernel = Vec::with_capacity((2 * r + 1) as usize);
    let mut ksum = 0.0f32;
    for d in -r..=r {
        let g = (-(d as f32 * d as f32) / (2.0 * sigma * sigma)).exp();
        kernel.push(g);
        ksum += g;
    }
    for g in &mut kernel {
        *g /= ksum;
    }

    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0f32;
            for (ki, d) in (-r..=r).enumerate() {
                let sx = (x + d).clamp(0, w - 1);
                acc += alpha[(y * w + sx) as usize] as f32 * kernel[ki];
            }
            temp[(y * w + x) as usize] = acc;
        }
    }
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0f32;
            for (ki, d) in (-r..=r).enumerate() {
                let sy = (y + d).clamp(0, h - 1);
                acc += temp[(sy * w + x) as usize] * kernel[ki];
            }
            out[(y * w + x) as usize] = acc.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

fn morph_dilate(alpha: &[u8], width: u32, height: u32, px: u32) -> Vec<u8> {
    morph_op(alpha, width, height, px, true)
}

fn morph_erode(alpha: &[u8], width: u32, height: u32, px: u32) -> Vec<u8> {
    morph_op(alpha, width, height, px, false)
}

fn morph_op(alpha: &[u8], width: u32, height: u32, px: u32, dilate: bool) -> Vec<u8> {
    let r = px.min(64) as i32;
    if r == 0 || width == 0 || height == 0 {
        return alpha.to_vec();
    }
    let w = width as i32;
    let h = height as i32;
    let mut temp = vec![0u8; (width as usize) * (height as usize)];
    let mut out = vec![0u8; (width as usize) * (height as usize)];

    for y in 0..h {
        for x in 0..w {
            let mut best = if dilate { 0u8 } else { 255u8 };
            for d in -r..=r {
                let sx = x + d;
                let v = if sx < 0 || sx >= w {
                    0
                } else {
                    alpha[(y * w + sx) as usize]
                };
                best = if dilate { best.max(v) } else { best.min(v) };
            }
            temp[(y * w + x) as usize] = best;
        }
    }
    for y in 0..h {
        for x in 0..w {
            let mut best = if dilate { 0u8 } else { 255u8 };
            for d in -r..=r {
                let sy = y + d;
                let v = if sy < 0 || sy >= h {
                    0
                } else {
                    temp[(sy * w + x) as usize]
                };
                best = if dilate { best.max(v) } else { best.min(v) };
            }
            out[(y * w + x) as usize] = best;
        }
    }
    out
}

fn grow_selection(
    alpha: &[u8],
    source: &RgbaImage,
    width: u32,
    height: u32,
    tolerance: u8,
    contiguous: bool,
) -> Vec<u8> {
    let mut out = alpha.to_vec();
    let tol = tolerance as f32;
    let mut seeds = Vec::new();
    let mut seed_colors = Vec::new();
    for y in 0..height.min(source.height()) {
        for x in 0..width.min(source.width()) {
            if alpha[(y * width + x) as usize] >= 128 {
                seeds.push((x, y));
                let px = source.get_pixel(x, y).0;
                if seed_colors.len() < 32
                    && !seed_colors.iter().any(|c| color_dist_rgba(*c, px) < 6.0)
                {
                    seed_colors.push(px);
                }
            }
        }
    }
    if seed_colors.is_empty() {
        return out;
    }

    if !contiguous {
        for y in 0..height.min(source.height()) {
            for x in 0..width.min(source.width()) {
                let px = source.get_pixel(x, y).0;
                if seed_colors.iter().any(|c| color_dist_rgba(*c, px) <= tol) {
                    out[(y * width + x) as usize] = 255;
                }
            }
        }
        return out;
    }

    let mut q: VecDeque<(u32, u32)> = seeds.into_iter().collect();
    let mut visited = vec![false; (width as usize) * (height as usize)];
    for &(sx, sy) in &q {
        visited[(sy * width + sx) as usize] = true;
    }
    while let Some((qx, qy)) = q.pop_front() {
        for (nx, ny) in [
            (qx.wrapping_sub(1), qy),
            (qx + 1, qy),
            (qx, qy.wrapping_sub(1)),
            (qx, qy + 1),
        ] {
            if nx >= width.min(source.width()) || ny >= height.min(source.height()) {
                continue;
            }
            let idx = (ny * width + nx) as usize;
            if visited[idx] {
                continue;
            }
            visited[idx] = true;
            let px = source.get_pixel(nx, ny).0;
            if seed_colors.iter().any(|c| color_dist_rgba(*c, px) <= tol) {
                out[idx] = 255;
                q.push_back((nx, ny));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn rect_ellipse_and_boolean_ops_work() {
        let img = RgbaImage::from_pixel(32, 32, Rgba([100, 150, 200, 255]));
        let mut sel = SelectionMask::empty(32, 32);
        let rect = rasterize_shape(
            32,
            32,
            &SelectionShape::Rect {
                x: 4.0,
                y: 4.0,
                width: 12.0,
                height: 12.0,
                feather: 0.0,
                anti_alias: false,
            },
            &img,
            None,
        );
        sel.combine(&rect, SelectionBooleanOp::Replace);
        assert_eq!(sel.bounds, Some([4, 4, 12, 12]));
        assert_eq!(sel.alpha_at(6, 6), 255);
        assert_eq!(sel.alpha_at(2, 2), 0);

        let sub = rasterize_shape(
            32,
            32,
            &SelectionShape::Ellipse {
                cx: 10.0,
                cy: 10.0,
                rx: 3.0,
                ry: 3.0,
                feather: 0.0,
                anti_alias: false,
            },
            &img,
            None,
        );
        sel.combine(&sub, SelectionBooleanOp::Subtract);
        assert_eq!(sel.alpha_at(10, 10), 0);
        assert_eq!(sel.alpha_at(5, 5), 255);

        sel.deselect();
        assert!(!sel.has_selection());
        assert!(sel.reselect());
        assert!(sel.has_selection());
    }

    #[test]
    fn wand_color_range_and_modify_work() {
        let mut img = RgbaImage::from_pixel(20, 20, Rgba([10, 10, 10, 255]));
        for y in 5..12 {
            for x in 5..12 {
                img.put_pixel(x, y, Rgba([220, 40, 40, 255]));
            }
        }
        let wand = rasterize_shape(
            20,
            20,
            &SelectionShape::MagicWand {
                x: 8,
                y: 8,
                tolerance: 10,
                contiguous: true,
                sample_all_layers: true,
                anti_alias: false,
            },
            &img,
            None,
        );
        let mut sel = SelectionMask::empty(20, 20);
        sel.combine(&wand, SelectionBooleanOp::Replace);
        assert_eq!(sel.bounds, Some([5, 5, 7, 7]));

        sel.modify(SelectionModifyOp::Expand(2), &img);
        assert_eq!(sel.bounds, Some([3, 3, 11, 11]));

        sel.modify(SelectionModifyOp::Contract(2), &img);
        assert_eq!(sel.bounds, Some([5, 5, 7, 7]));
    }
}
