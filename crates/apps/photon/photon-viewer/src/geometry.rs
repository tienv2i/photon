use photon_protocol::TranslationBlock;

pub(crate) fn viewer_fit_geometry(
    doc_w: f32,
    doc_h: f32,
    quarter_turns: u8,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    vp_w: f32,
    vp_h: f32,
) -> (f32, f32, f32, f32, f32, f32) {
    let vp_w = vp_w.max(1.0);
    let vp_h = vp_h.max(1.0);
    let (fw, fh) = if quarter_turns % 2 == 1 {
        (doc_h.max(1.0), doc_w.max(1.0))
    } else {
        (doc_w.max(1.0), doc_h.max(1.0))
    };
    let vp_aspect = vp_w / vp_h;
    let img_aspect = fw / fh;
    let (fit_w, fit_h) = if img_aspect > vp_aspect {
        (vp_w, vp_w / img_aspect)
    } else {
        (vp_h * img_aspect, vp_h)
    };
    let drawn_w = fit_w * zoom.max(0.02);
    let drawn_h = fit_h * zoom.max(0.02);
    let cx = vp_w * 0.5 + pan_x;
    let cy = vp_h * 0.5 + pan_y;
    let left = cx - drawn_w * 0.5;
    let top = cy - drawn_h * 0.5;
    (left, top, drawn_w, drawn_h, fw, fh)
}

/// Transform unrotated image-space `(x, y, w, h)` into display-space coordinates for `quarter_turns`
/// (0, 1=90° CW, 2=180°, 3=270° CW) within an unrotated `(doc_w, doc_h)` image.
pub fn transform_block_for_quarter_turns(
    b: &TranslationBlock,
    doc_w: f32,
    doc_h: f32,
    quarter_turns: u8,
) -> (f32, f32, f32, f32) {
    let bx = b.x as f32;
    let by = b.y as f32;
    let bw = b.w.max(1) as f32;
    let bh = b.h.max(1) as f32;
    match quarter_turns % 4 {
        1 => ((doc_h - (by + bh)).max(0.0), bx.max(0.0), bh, bw),
        2 => (
            (doc_w - (bx + bw)).max(0.0),
            (doc_h - (by + bh)).max(0.0),
            bw,
            bh,
        ),
        3 => (by.max(0.0), (doc_w - (bx + bw)).max(0.0), bh, bw),
        _ => (bx.max(0.0), by.max(0.0), bw, bh),
    }
}

/// Screen-space geometry for a dual-page spread matching `photon-canvas` GPU rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoubleSpreadScreenGeometry {
    pub spread_left: f32,
    pub spread_top: f32,
    pub drawn_w: f32,
    pub drawn_h: f32,
    pub fw: f32,
    pub fh: f32,
    pub spread_w: f32,
    pub spread_h: f32,
    pub left_disp_w: f32,
    pub gap_doc: f32,
    pub right_disp_w: f32,
}

/// Compute screen-space spread geometry for Dual (`Double`) mode matching `photon-canvas/src/gpu.rs`.
#[allow(clippy::too_many_arguments)]
pub fn viewer_double_fit_geometries(
    left_size: (u32, u32),
    right_size: Option<(u32, u32)>,
    dual_gap_px: f32,
    quarter_turns: u8,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    vp_w: f32,
    vp_h: f32,
) -> DoubleSpreadScreenGeometry {
    let (spread_w, spread_h, left_disp_w, gap_doc, right_disp_w) = match right_size {
        Some(r) => {
            let (sw, sh, lw, gap) =
                photon_viewer_canvas::dual_spread_geometry(left_size, r, dual_gap_px);
            let rw = (sw - lw - gap).max(1.0);
            (sw, sh, lw, gap, rw)
        }
        None => {
            let lw = left_size.0.max(1) as f32;
            let lh = left_size.1.max(1) as f32;
            (lw, lh, lw, 0.0, 0.0)
        }
    };
    let (spread_left, spread_top, drawn_w, drawn_h, fw, fh) = viewer_fit_geometry(
        spread_w,
        spread_h,
        quarter_turns,
        zoom,
        pan_x,
        pan_y,
        vp_w,
        vp_h,
    );
    DoubleSpreadScreenGeometry {
        spread_left,
        spread_top,
        drawn_w,
        drawn_h,
        fw,
        fh,
        spread_w,
        spread_h,
        left_disp_w,
        gap_doc,
        right_disp_w,
    }
}

/// Map a translation block from unrotated page `(img_w, img_h)` coordinates into screen `(sx, sy, sw, sh)`
/// for either the left (`is_right == false`) or right (`is_right == true`) spread page.
pub fn transform_double_page_block_to_screen(
    b: &TranslationBlock,
    img_w: f32,
    img_h: f32,
    is_right: bool,
    geom: DoubleSpreadScreenGeometry,
    quarter_turns: u8,
) -> (f32, f32, f32, f32) {
    let (x_off, page_disp_w) = if is_right {
        (geom.left_disp_w + geom.gap_doc, geom.right_disp_w.max(1.0))
    } else {
        (0.0, geom.left_disp_w.max(1.0))
    };
    let sx_scale = page_disp_w / img_w.max(1.0);
    let sy_scale = geom.spread_h / img_h.max(1.0);
    let spread_block = TranslationBlock {
        x: (x_off + b.x as f32 * sx_scale).round() as i32,
        y: (b.y as f32 * sy_scale).round() as i32,
        w: (b.w.max(1) as f32 * sx_scale).round().max(1.0) as i32,
        h: (b.h.max(1) as f32 * sy_scale).round().max(1.0) as i32,
        original: String::new(),
        translated: String::new(),
    };
    let (rx, ry, rw, rh) = transform_block_for_quarter_turns(
        &spread_block,
        geom.spread_w,
        geom.spread_h,
        quarter_turns,
    );
    let sx = geom.spread_left + (rx / geom.fw) * geom.drawn_w;
    let sy = geom.spread_top + (ry / geom.fh) * geom.drawn_h;
    let sw = (rw / geom.fw) * geom.drawn_w;
    let sh = (rh / geom.fh) * geom.drawn_h;
    (sx, sy, sw, sh)
}

/// Compute an auto-fit pixel font size so translated text fits cleanly inside `(sw, sh)`.
pub fn compute_bubble_font_size(
    sw: f32,
    sh: f32,
    header_h: f32,
    char_count: usize,
    font_scale: f32,
) -> f32 {
    let avail_w = (sw - 14.0).max(24.0);
    let avail_h = (sh - header_h - 10.0).max(16.0);
    let chars = char_count.max(1) as f32;
    let area_px = ((avail_w * avail_h) / (chars * 0.62)).sqrt();
    let base_px = area_px.min((sh * 0.24).max(11.0)).clamp(9.0, 24.0);
    (base_px * font_scale).clamp(9.0, 28.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_block_rotates_coordinates_across_all_quarter_turns() {
        let block = TranslationBlock {
            x: 100,
            y: 50,
            w: 200,
            h: 80,
            original: "hi".to_owned(),
            translated: "chào".to_owned(),
        };
        assert_eq!(
            transform_block_for_quarter_turns(&block, 1000.0, 800.0, 0),
            (100.0, 50.0, 200.0, 80.0)
        );
        assert_eq!(
            transform_block_for_quarter_turns(&block, 1000.0, 800.0, 1),
            (670.0, 100.0, 80.0, 200.0)
        );
        assert_eq!(
            transform_block_for_quarter_turns(&block, 1000.0, 800.0, 2),
            (700.0, 670.0, 200.0, 80.0)
        );
        assert_eq!(
            transform_block_for_quarter_turns(&block, 1000.0, 800.0, 3),
            (50.0, 700.0, 80.0, 200.0)
        );
    }

    #[test]
    fn compute_bubble_font_size_scales_and_clamps() {
        let small = compute_bubble_font_size(60.0, 36.0, 0.0, 40, 1.0);
        let large = compute_bubble_font_size(240.0, 160.0, 22.0, 10, 1.0);
        let scaled = compute_bubble_font_size(240.0, 160.0, 22.0, 10, 1.5);
        assert!((9.0..=28.0).contains(&small));
        assert!(large > small);
        assert!(scaled >= large);
        assert!(scaled <= 28.0);
    }

    #[test]
    fn double_spread_geometry_maps_left_and_right_pages_across_rotations() {
        let geom = viewer_double_fit_geometries(
            (800, 1000),
            Some((1200, 1500)),
            10.0,
            0,
            1.0,
            0.0,
            0.0,
            1610.0,
            750.0,
        );
        // Normalized target_h = 1500; left_disp_w = 800 * 1.5 = 1200, right_disp_w = 1200.
        assert_eq!(geom.spread_h, 1500.0);
        assert_eq!(geom.left_disp_w, 1200.0);
        assert!((geom.right_disp_w - 1200.0).abs() < 1e-3);
        let left_block = TranslationBlock {
            x: 100,
            y: 200,
            w: 200,
            h: 100,
            original: String::new(),
            translated: String::new(),
        };
        let right_block = TranslationBlock {
            x: 150,
            y: 450,
            w: 300,
            h: 150,
            original: String::new(),
            translated: String::new(),
        };
        let (lx, ly, lw, lh) =
            transform_double_page_block_to_screen(&left_block, 800.0, 1000.0, false, geom, 0);
        let (rx, ry, rw, rh) =
            transform_double_page_block_to_screen(&right_block, 1200.0, 1500.0, true, geom, 0);
        assert!(rx > lx + lw);
        assert!(lw > 0.0 && lh > 0.0 && rw > 0.0 && rh > 0.0);
        assert!(ry > ly);
    }
}
