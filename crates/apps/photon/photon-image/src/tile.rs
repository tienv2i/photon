//! Dirty-rectangle tracking and 256×256 sparse Copy-on-Write tile surface.

use std::collections::HashMap;
use std::sync::Arc;

use image::RgbaImage;
use rayon::prelude::*;

/// Default edge length (in pixels) for square image tiles (`256×256×4 = 256 KiB` RGBA8).
pub const SURFACE_TILE_SIZE: u32 = 256;

/// Axis-aligned pixel rectangle used for dirty-region compositing and partial GPU uploads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl DirtyRect {
    /// Construct a `DirtyRect` from origin and dimensions.
    #[must_use]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Construct a `DirtyRect` covering the full `(width × height)` canvas.
    #[must_use]
    pub const fn full(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// Returns `true` when the rectangle covers zero pixels.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Exclusive right edge (`x + width`, saturating).
    #[must_use]
    pub const fn right(self) -> u32 {
        self.x.saturating_add(self.width)
    }

    /// Exclusive bottom edge (`y + height`, saturating).
    #[must_use]
    pub const fn bottom(self) -> u32 {
        self.y.saturating_add(self.height)
    }

    /// Return the bounding union of two rectangles, ignoring empty sides.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = self.right().max(other.right());
        let y1 = self.bottom().max(other.bottom());
        Self::new(x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
    }

    /// Clamp this rectangle to `(0, 0, canvas_w, canvas_h)`, returning `None` if empty.
    #[must_use]
    pub fn intersect_canvas(self, canvas_w: u32, canvas_h: u32) -> Option<Self> {
        let x0 = self.x.min(canvas_w);
        let y0 = self.y.min(canvas_h);
        let x1 = self.right().min(canvas_w);
        let y1 = self.bottom().min(canvas_h);
        if x1 <= x0 || y1 <= y0 {
            None
        } else {
            Some(Self::new(x0, y0, x1 - x0, y1 - y0))
        }
    }

    /// Compute a clamped bounding `DirtyRect` around stroke `points` expanded by `radius + pad`.
    #[must_use]
    pub fn from_points(
        points: &[[f64; 2]],
        radius: f64,
        pad: f64,
        canvas_w: u32,
        canvas_h: u32,
    ) -> Option<Self> {
        if points.is_empty() || canvas_w == 0 || canvas_h == 0 {
            return None;
        }
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for &[px, py] in points {
            if !px.is_finite() || !py.is_finite() {
                continue;
            }
            min_x = min_x.min(px);
            min_y = min_y.min(py);
            max_x = max_x.max(px);
            max_y = max_y.max(py);
        }
        if !min_x.is_finite() {
            return None;
        }
        let margin = radius.max(0.0) + pad.max(0.0);
        let x0 = (min_x - margin).floor().max(0.0) as u32;
        let y0 = (min_y - margin).floor().max(0.0) as u32;
        let x1 = ((max_x + margin).ceil().max(0.0) as u32).min(canvas_w);
        let y1 = ((max_y + margin).ceil().max(0.0) as u32).min(canvas_h);
        if x1 <= x0 || y1 <= y0 {
            None
        } else {
            Some(Self::new(x0, y0, x1 - x0, y1 - y0))
        }
    }
}

/// Coordinates of a single tile inside a [`TiledSurface`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileId {
    pub tx: u32,
    pub ty: u32,
}

impl TileId {
    /// Construct a `TileId`.
    #[must_use]
    pub const fn new(tx: u32, ty: u32) -> Self {
        Self { tx, ty }
    }
}

/// Sparse Copy-on-Write 256×256 RGBA8 tile surface.
///
/// Empty (all-zero transparent) tiles are omitted from `tiles` so large canvases
/// with sparse content consume minimal RAM, and individual tiles can be shared
/// across undo/redo deltas via `Arc<Vec<u8>>`.
#[derive(Debug, Clone)]
pub struct TiledSurface {
    pub width: u32,
    pub height: u32,
    pub tile_size: u32,
    pub tiles: HashMap<TileId, Arc<Vec<u8>>>,
}

impl TiledSurface {
    /// Create an empty (fully transparent) sparse `TiledSurface`.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            tile_size: SURFACE_TILE_SIZE,
            tiles: HashMap::new(),
        }
    }

    /// Number of tiles horizontally and vertically.
    #[must_use]
    pub fn grid_dimensions(&self) -> (u32, u32) {
        let cols = self.width.div_ceil(self.tile_size);
        let rows = self.height.div_ceil(self.tile_size);
        (cols, rows)
    }

    /// Pixel dimensions `(w, h)` of a specific tile (edge tiles may be smaller).
    #[must_use]
    pub fn tile_pixel_size(&self, id: TileId) -> (u32, u32) {
        let x0 = id.tx.saturating_mul(self.tile_size);
        let y0 = id.ty.saturating_mul(self.tile_size);
        let tw = self.width.saturating_sub(x0).min(self.tile_size);
        let th = self.height.saturating_sub(y0).min(self.tile_size);
        (tw, th)
    }

    /// Return all `TileId`s intersecting `rect`.
    #[must_use]
    pub fn tiles_in_rect(&self, rect: DirtyRect) -> Vec<TileId> {
        let Some(clamped) = rect.intersect_canvas(self.width, self.height) else {
            return Vec::new();
        };
        let tx0 = clamped.x / self.tile_size;
        let ty0 = clamped.y / self.tile_size;
        let tx1 = (clamped.right() - 1) / self.tile_size;
        let ty1 = (clamped.bottom() - 1) / self.tile_size;
        let mut out = Vec::with_capacity(((tx1 - tx0 + 1) * (ty1 - ty0 + 1)) as usize);
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                out.push(TileId::new(tx, ty));
            }
        }
        out
    }

    /// Build a sparse `TiledSurface` from a flat `RgbaImage`, omitting all-zero tiles.
    #[must_use]
    pub fn from_rgba_image(img: &RgbaImage) -> Self {
        let (width, height) = img.dimensions();
        let mut surface = Self::new(width, height);
        let (cols, rows) = surface.grid_dimensions();
        let raw = img.as_raw();
        let stride = (width as usize) * 4;
        for ty in 0..rows {
            for tx in 0..cols {
                let id = TileId::new(tx, ty);
                let (tw, th) = surface.tile_pixel_size(id);
                let x0 = (tx * surface.tile_size) as usize;
                let y0 = (ty * surface.tile_size) as usize;
                let row_bytes = (tw as usize) * 4;
                let mut buf = vec![0u8; (tw as usize) * (th as usize) * 4];
                let mut non_zero = false;
                for row in 0..th as usize {
                    let src_off = (y0 + row) * stride + x0 * 4;
                    let dst_off = row * row_bytes;
                    let slice = &raw[src_off..src_off + row_bytes];
                    if !non_zero && slice.iter().any(|&b| b != 0) {
                        non_zero = true;
                    }
                    buf[dst_off..dst_off + row_bytes].copy_from_slice(slice);
                }
                if non_zero {
                    surface.tiles.insert(id, Arc::new(buf));
                }
            }
        }
        surface
    }

    /// Materialize the full `RgbaImage` from this `TiledSurface`.
    #[must_use]
    pub fn to_rgba_image(&self) -> RgbaImage {
        let mut out = RgbaImage::new(self.width, self.height);
        let stride = (self.width as usize) * 4;
        let raw = out.as_mut();
        for (&id, tile) in &self.tiles {
            let (tw, th) = self.tile_pixel_size(id);
            let x0 = (id.tx * self.tile_size) as usize;
            let y0 = (id.ty * self.tile_size) as usize;
            let row_bytes = (tw as usize) * 4;
            for row in 0..th as usize {
                let src_off = row * row_bytes;
                let dst_off = (y0 + row) * stride + x0 * 4;
                raw[dst_off..dst_off + row_bytes]
                    .copy_from_slice(&tile[src_off..src_off + row_bytes]);
            }
        }
        out
    }

    /// Update only the tiles intersecting `rect` from `img`, returning `(TileId, previous_tile)`
    /// deltas for undo history.
    pub fn sync_region_from(
        &mut self,
        img: &RgbaImage,
        rect: DirtyRect,
    ) -> Vec<(TileId, Option<Arc<Vec<u8>>>)> {
        let Some(clamped) = rect.intersect_canvas(self.width, self.height) else {
            return Vec::new();
        };
        let ids = self.tiles_in_rect(clamped);
        let raw = img.as_raw();
        let stride = (self.width as usize) * 4;
        let mut deltas = Vec::with_capacity(ids.len());
        for id in ids {
            let (tw, th) = self.tile_pixel_size(id);
            let x0 = (id.tx * self.tile_size) as usize;
            let y0 = (id.ty * self.tile_size) as usize;
            let row_bytes = (tw as usize) * 4;
            let mut buf = vec![0u8; (tw as usize) * (th as usize) * 4];
            let mut non_zero = false;
            for row in 0..th as usize {
                let src_off = (y0 + row) * stride + x0 * 4;
                let dst_off = row * row_bytes;
                let slice = &raw[src_off..src_off + row_bytes];
                if !non_zero && slice.iter().any(|&b| b != 0) {
                    non_zero = true;
                }
                buf[dst_off..dst_off + row_bytes].copy_from_slice(slice);
            }
            let prev = self.tiles.get(&id).cloned();
            if prev.as_deref() == Some(&buf) && non_zero {
                continue;
            }
            if !non_zero && prev.is_none() {
                continue;
            }
            deltas.push((id, prev));
            if non_zero {
                self.tiles.insert(id, Arc::new(buf));
            } else {
                self.tiles.remove(&id);
            }
        }
        deltas
    }

    /// Total heap bytes used by currently allocated tiles.
    #[must_use]
    pub fn memory_bytes(&self) -> u64 {
        self.tiles.values().map(|t| t.len() as u64).sum()
    }
}

/// Extract only the `256×256` tiles of `before` that differ from `after` (for tile-level undo delta).
///
/// Both images must have identical dimensions.
#[must_use]
pub fn diff_rgba_tiles(before: &RgbaImage, after: &RgbaImage) -> Vec<(TileId, Arc<Vec<u8>>)> {
    let (w, h) = before.dimensions();
    if (w, h) != after.dimensions() || w == 0 || h == 0 {
        return Vec::new();
    }
    let cols = w.div_ceil(SURFACE_TILE_SIZE);
    let rows = h.div_ceil(SURFACE_TILE_SIZE);
    let b_raw = before.as_raw();
    let a_raw = after.as_raw();
    let stride = (w as usize) * 4;

    let coords: Vec<(u32, u32)> = (0..rows)
        .flat_map(|ty| (0..cols).map(move |tx| (tx, ty)))
        .collect();

    coords
        .into_par_iter()
        .filter_map(|(tx, ty)| {
            let x0 = (tx * SURFACE_TILE_SIZE) as usize;
            let y0 = (ty * SURFACE_TILE_SIZE) as usize;
            let tw = w
                .saturating_sub(tx * SURFACE_TILE_SIZE)
                .min(SURFACE_TILE_SIZE) as usize;
            let th = h
                .saturating_sub(ty * SURFACE_TILE_SIZE)
                .min(SURFACE_TILE_SIZE) as usize;
            let row_bytes = tw * 4;

            let mut differs = false;
            for row in 0..th {
                let off = (y0 + row) * stride + x0 * 4;
                if b_raw[off..off + row_bytes] != a_raw[off..off + row_bytes] {
                    differs = true;
                    break;
                }
            }
            if !differs {
                return None;
            }
            let mut buf = vec![0u8; tw * th * 4];
            for row in 0..th {
                let src_off = (y0 + row) * stride + x0 * 4;
                let dst_off = row * row_bytes;
                buf[dst_off..dst_off + row_bytes]
                    .copy_from_slice(&b_raw[src_off..src_off + row_bytes]);
            }
            Some((TileId::new(tx, ty), Arc::new(buf)))
        })
        .collect()
}

/// Patch `tiles` (each storing `256×256` or edge-tile RGBA8 bytes) back into `dst` in-place.
pub fn patch_rgba_tiles(dst: &mut RgbaImage, tiles: &[(TileId, Arc<Vec<u8>>)]) {
    let (w, h) = dst.dimensions();
    if w == 0 || h == 0 {
        return;
    }
    let stride = (w as usize) * 4;
    let raw = dst.as_mut();
    for (id, buf) in tiles {
        let x0 = (id.tx * SURFACE_TILE_SIZE) as usize;
        let y0 = (id.ty * SURFACE_TILE_SIZE) as usize;
        let tw = w
            .saturating_sub(id.tx * SURFACE_TILE_SIZE)
            .min(SURFACE_TILE_SIZE) as usize;
        let th = h
            .saturating_sub(id.ty * SURFACE_TILE_SIZE)
            .min(SURFACE_TILE_SIZE) as usize;
        let row_bytes = tw * 4;
        if buf.len() != tw * th * 4 {
            continue;
        }
        for row in 0..th {
            let src_off = row * row_bytes;
            let dst_off = (y0 + row) * stride + x0 * 4;
            raw[dst_off..dst_off + row_bytes].copy_from_slice(&buf[src_off..src_off + row_bytes]);
        }
    }
}

/// Execute `op` in parallel across all `256×256` tiles of `img` using `rayon`.
pub fn process_tiles_parallel<F>(img: &mut RgbaImage, op: F)
where
    F: Fn(TileId, u32, u32, &mut [u8]) + Sync,
{
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return;
    }
    let stride = (w as usize) * 4;
    let tile_rows = h.div_ceil(SURFACE_TILE_SIZE);
    let cols = w.div_ceil(SURFACE_TILE_SIZE);
    let strip_bytes = (SURFACE_TILE_SIZE as usize) * stride;
    let raw = img.as_mut();

    raw.par_chunks_mut(strip_bytes)
        .enumerate()
        .for_each(|(ty_idx, strip)| {
            let ty = ty_idx as u32;
            if ty >= tile_rows {
                return;
            }
            let th = h
                .saturating_sub(ty * SURFACE_TILE_SIZE)
                .min(SURFACE_TILE_SIZE);
            for tx in 0..cols {
                let tw = w
                    .saturating_sub(tx * SURFACE_TILE_SIZE)
                    .min(SURFACE_TILE_SIZE);
                let x0 = (tx * SURFACE_TILE_SIZE) as usize;
                let row_bytes = (tw as usize) * 4;
                let mut tile_buf = vec![0u8; (tw as usize) * (th as usize) * 4];
                for row in 0..th as usize {
                    let src_off = row * stride + x0 * 4;
                    let dst_off = row * row_bytes;
                    tile_buf[dst_off..dst_off + row_bytes]
                        .copy_from_slice(&strip[src_off..src_off + row_bytes]);
                }
                op(TileId::new(tx, ty), tw, th, &mut tile_buf);
                for row in 0..th as usize {
                    let src_off = row * row_bytes;
                    let dst_off = row * stride + x0 * 4;
                    strip[dst_off..dst_off + row_bytes]
                        .copy_from_slice(&tile_buf[src_off..src_off + row_bytes]);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn dirty_rect_from_points_and_union_clamp_to_canvas() {
        let r = DirtyRect::from_points(&[[10.0, 20.0], [50.0, 40.0]], 5.0, 2.0, 100, 100)
            .expect("rect");
        assert_eq!(r, DirtyRect::new(3, 13, 54, 34));
        let u = r.union(DirtyRect::new(0, 0, 10, 10));
        assert_eq!(u, DirtyRect::new(0, 0, 57, 47));
    }

    #[test]
    fn tiled_surface_sparse_roundtrip_and_region_deltas() {
        let mut img = RgbaImage::new(600, 300);
        img.put_pixel(10, 10, Rgba([255, 0, 0, 255]));
        let mut surface = TiledSurface::from_rgba_image(&img);
        // Only tile (0, 0) should be allocated out of 3x2 = 6 tiles.
        assert_eq!(surface.tiles.len(), 1);
        assert_eq!(surface.to_rgba_image(), img);

        // Paint in tile (2, 1) and sync only that region.
        img.put_pixel(550, 280, Rgba([0, 255, 0, 255]));
        let deltas = surface.sync_region_from(&img, DirtyRect::new(540, 270, 20, 20));
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].0, TileId::new(2, 1));
        assert!(deltas[0].1.is_none());
        assert_eq!(surface.tiles.len(), 2);
        assert_eq!(surface.to_rgba_image(), img);
    }

    #[test]
    fn diff_patch_and_parallel_tiles_roundtrip() {
        let mut before = RgbaImage::from_pixel(600, 300, Rgba([10, 20, 30, 255]));
        let mut after = before.clone();
        after.put_pixel(550, 280, Rgba([200, 100, 50, 255]));

        let delta = diff_rgba_tiles(&before, &after);
        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0].0, TileId::new(2, 1));

        let mut restored = after.clone();
        patch_rgba_tiles(&mut restored, &delta);
        assert_eq!(restored, before);

        process_tiles_parallel(&mut before, |_id, _tw, _th, buf| {
            for px in buf.chunks_exact_mut(4) {
                px[0] = px[0].saturating_add(5);
            }
        });
        assert_eq!(before.get_pixel(0, 0).0, [15, 20, 30, 255]);
        assert_eq!(before.get_pixel(599, 299).0, [15, 20, 30, 255]);
    }
}
