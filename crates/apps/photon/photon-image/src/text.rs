//! Non-destructive Text Layer rasterization, word wrapping, bounding-box measurement,
//! and system font discovery powered by `usvg` (`fontdb` + `rustybuzz`) and `resvg` (`tiny-skia`),
//! with a built-in pure-Rust vector glyph fallback for fontless sandboxes.

use std::fmt::Write as _;
use std::sync::{Arc, OnceLock};

use image::{Rgba, RgbaImage};
use photon_protocol::{TextAlign, TextLayerData, VectorStroke, VectorStrokeCap, VectorStrokeJoin};

use crate::drawing::{fill_polygon_aa, stroke_polyline_aa};

static SYSTEM_FONTDB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();

/// Return the shared process-wide system font database (initialized once via `fontdb`).
pub fn system_fontdb() -> Arc<usvg::fontdb::Database> {
    Arc::clone(SYSTEM_FONTDB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        // Map generic CSS/SVG families to common cross-platform faces when present.
        for candidate in [
            "DejaVu Sans",
            "Noto Sans",
            "Fira Sans",
            "Liberation Sans",
            "Arial",
            "Ubuntu",
        ] {
            if db
                .faces()
                .any(|f| f.families.iter().any(|(name, _)| name == candidate))
            {
                db.set_sans_serif_family(candidate);
                break;
            }
        }
        for candidate in [
            "DejaVu Serif",
            "Noto Serif",
            "Liberation Serif",
            "Times New Roman",
        ] {
            if db
                .faces()
                .any(|f| f.families.iter().any(|(name, _)| name == candidate))
            {
                db.set_serif_family(candidate);
                break;
            }
        }
        for candidate in [
            "DejaVu Sans Mono",
            "Noto Sans Mono",
            "Fira Mono",
            "Liberation Mono",
            "Courier New",
        ] {
            if db
                .faces()
                .any(|f| f.families.iter().any(|(name, _)| name == candidate))
            {
                db.set_monospace_family(candidate);
                break;
            }
        }
        Arc::new(db)
    }))
}

/// Return a sorted, deduplicated list of available font families on the host system,
/// always including `"Sans Serif"`, `"Serif"`, and `"Monospace"`.
pub fn available_font_families() -> Vec<String> {
    let db = system_fontdb();
    let mut families: Vec<String> = vec![
        "Sans Serif".to_owned(),
        "Serif".to_owned(),
        "Monospace".to_owned(),
    ];
    for face in db.faces() {
        for (name, _) in &face.families {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                families.push(trimmed.to_owned());
            }
        }
    }
    families.sort_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()));
    families.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    families
}

#[inline]
fn approx_char_advance(ch: char, font_size: f32, bold: bool) -> f32 {
    let weight_scale = if bold { 1.06 } else { 1.0 };
    let ratio = if ch == ' ' {
        0.30
    } else if matches!(
        ch,
        'i' | 'l' | 'I' | '!' | '.' | ',' | ':' | ';' | '\'' | '|'
    ) {
        0.32
    } else if matches!(ch, 'm' | 'w' | 'M' | 'W' | '@' | '%') {
        0.78
    } else if !ch.is_ascii() {
        0.85
    } else {
        0.56
    };
    font_size.max(1.0) * ratio * weight_scale
}

#[inline]
fn approx_line_width(line: &str, font_size: f32, bold: bool) -> f32 {
    line.chars()
        .map(|c| approx_char_advance(c, font_size, bold))
        .sum()
}

/// Split `text` into explicit newline lines and word-wrap any line exceeding `box_width`
/// (when `box_width` is `Some(w)` with `w > 0.0`).
pub fn wrap_text_lines(text: &str, font_size: f32, box_width: Option<f32>) -> Vec<String> {
    let size = font_size.max(1.0);
    let max_w = box_width.filter(|&w| w > size * 0.6);
    let mut lines = Vec::new();

    for raw_line in text.split('\n') {
        let raw_line = raw_line.trim_end_matches('\r');
        let Some(limit) = max_w else {
            lines.push(raw_line.to_owned());
            continue;
        };
        if raw_line.is_empty() || approx_line_width(raw_line, size, false) <= limit {
            lines.push(raw_line.to_owned());
            continue;
        }

        let mut current = String::new();
        let mut current_w = 0.0f32;
        let space_w = approx_char_advance(' ', size, false);

        for word in raw_line.split(' ') {
            let word_w = approx_line_width(word, size, false);
            if current.is_empty() {
                if word_w <= limit {
                    current.push_str(word);
                    current_w = word_w;
                } else {
                    // Break an extra-long single token across lines by character width.
                    for ch in word.chars() {
                        let cw = approx_char_advance(ch, size, false);
                        if !current.is_empty() && current_w + cw > limit {
                            lines.push(std::mem::take(&mut current));
                            current_w = 0.0;
                        }
                        current.push(ch);
                        current_w += cw;
                    }
                }
            } else if current_w + space_w + word_w <= limit {
                current.push(' ');
                current.push_str(word);
                current_w += space_w + word_w;
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
                current_w = word_w;
            }
        }
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Measure the approximate bounding box `[x, y, width, height]` of a [`TextLayerData`] block
/// in canvas coordinates.
pub fn measure_text_bounds(data: &TextLayerData) -> [f32; 4] {
    let font_size = data.font_size.clamp(1.0, 2048.0);
    let line_height = data.line_height.clamp(0.5, 5.0);
    let lines = wrap_text_lines(&data.text, font_size, data.box_width);
    let max_line_w = lines
        .iter()
        .map(|l| approx_line_width(l, font_size, data.bold))
        .fold(0.0f32, f32::max);
    let width = match data.box_width.filter(|&w| w > 0.0) {
        Some(bw) => bw.max(max_line_w),
        None => max_line_w.max(font_size * 0.5),
    };
    let line_step = font_size * line_height;
    let height = if lines.len() <= 1 {
        font_size * 1.2
    } else {
        font_size * 1.2 + (lines.len() - 1) as f32 * line_step
    };
    let min_x = match (data.align, data.box_width.filter(|&w| w > 0.0)) {
        (_, Some(_)) | (TextAlign::Left, None) => data.x,
        (TextAlign::Center, None) => data.x - width * 0.5,
        (TextAlign::Right, None) => data.x - width,
    };
    [min_x, data.y, width, height]
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

fn build_text_svg(data: &TextLayerData, canvas_width: u32, canvas_height: u32) -> String {
    let w = canvas_width.max(1);
    let h = canvas_height.max(1);
    let font_size = data.font_size.clamp(1.0, 2048.0);
    let line_height = data.line_height.clamp(0.5, 5.0);
    let line_step = font_size * line_height;
    let lines = wrap_text_lines(&data.text, font_size, data.box_width);

    let (text_anchor, anchor_x) = match (data.align, data.box_width.filter(|&bw| bw > 0.0)) {
        (TextAlign::Left, _) => ("start", data.x),
        (TextAlign::Center, Some(bw)) => ("middle", data.x + bw * 0.5),
        (TextAlign::Center, None) => ("middle", data.x),
        (TextAlign::Right, Some(bw)) => ("end", data.x + bw),
        (TextAlign::Right, None) => ("end", data.x),
    };

    let raw_family = data.font_family.trim();
    let css_family = if raw_family.is_empty() || raw_family.eq_ignore_ascii_case("sans serif") {
        "sans-serif".to_owned()
    } else if raw_family.eq_ignore_ascii_case("serif") {
        "serif".to_owned()
    } else if raw_family.eq_ignore_ascii_case("monospace") {
        "monospace".to_owned()
    } else {
        format!("{}, sans-serif", xml_escape(raw_family))
    };

    let weight = if data.bold { "bold" } else { "normal" };
    let style = if data.italic { "italic" } else { "normal" };
    let [r, g, b] = data.color;

    let mut svg = String::with_capacity(512 + data.text.len() * 2);
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
    );

    // Baseline of the first line sits at `data.y + font_size * 0.88`.
    let base_y = data.y + font_size * 0.88;
    for (idx, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let y = base_y + (idx as f32) * line_step;
        let escaped = xml_escape(line);
        let _ = writeln!(
            svg,
            r#"  <text x="{anchor_x:.2}" y="{y:.2}" font-family="{css_family}" font-size="{font_size:.2}" font-weight="{weight}" font-style="{style}" text-anchor="{text_anchor}" fill="rgb({r},{g},{b})" xml:space="preserve">{escaped}</text>"#
        );
    }

    svg.push_str("</svg>\n");
    svg
}

/// Pure-Rust procedural vector glyph fallback used only if no system fonts are available
/// or if `usvg` could not shape a string. Draws clean stroked characters honoring size,
/// position, alignment, color, line height, bold, and italic slant.
fn render_fallback_vector_text(dst: &mut RgbaImage, data: &TextLayerData) {
    let font_size = data.font_size.clamp(4.0, 2048.0);
    let line_height = data.line_height.clamp(0.5, 5.0);
    let line_step = font_size * line_height;
    let lines = wrap_text_lines(&data.text, font_size, data.box_width);
    let char_h = font_size * 0.72;
    let char_w = font_size * 0.44;
    let advance = font_size * 0.58;
    let stroke_w = (font_size * if data.bold { 0.14 } else { 0.09 }).max(1.2);
    let slant = if data.italic { font_size * 0.16 } else { 0.0 };
    let stroke = VectorStroke::with_cap_join(
        data.color,
        stroke_w,
        VectorStrokeCap::Round,
        VectorStrokeJoin::Round,
    );

    for (line_idx, line) in lines.iter().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            continue;
        }
        let line_w = (chars.len() as f32) * advance;
        let start_x = match (data.align, data.box_width.filter(|&bw| bw > 0.0)) {
            (TextAlign::Left, _) => data.x,
            (TextAlign::Center, Some(bw)) => data.x + (bw - line_w) * 0.5,
            (TextAlign::Center, None) => data.x - line_w * 0.5,
            (TextAlign::Right, Some(bw)) => data.x + bw - line_w,
            (TextAlign::Right, None) => data.x - line_w,
        };
        let top_y = data.y + font_size * 0.16 + (line_idx as f32) * line_step;

        for (ci, &ch) in chars.iter().enumerate() {
            if ch.is_whitespace() {
                continue;
            }
            let cx = start_x + (ci as f32) * advance;
            let pt = |nx: f32, ny: f32| -> [f64; 2] {
                let x = cx + nx * char_w + (1.0 - ny) * slant;
                let y = top_y + ny * char_h;
                [f64::from(x), f64::from(y)]
            };

            // Simple 7-segment / stroke glyph approximations for ASCII & general characters.
            let upper = ch.to_ascii_uppercase();
            let segments: &[&[[f32; 2]]] = match upper {
                'A' => &[
                    &[[0.0, 1.0], [0.5, 0.0], [1.0, 1.0]],
                    &[[0.2, 0.6], [0.8, 0.6]],
                ],
                'B' | '8' => &[
                    &[[0.0, 0.0], [0.9, 0.0], [0.9, 1.0], [0.0, 1.0], [0.0, 0.0]],
                    &[[0.0, 0.5], [0.9, 0.5]],
                ],
                'C' | '(' | '[' | '{' => &[&[[1.0, 0.0], [0.0, 0.0], [0.0, 1.0], [1.0, 1.0]]],
                'D' | 'O' | '0' | 'Q' => {
                    &[&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]]]
                }
                'E' => &[
                    &[[1.0, 0.0], [0.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
                    &[[0.0, 0.5], [0.8, 0.5]],
                ],
                'F' => &[
                    &[[1.0, 0.0], [0.0, 0.0], [0.0, 1.0]],
                    &[[0.0, 0.5], [0.8, 0.5]],
                ],
                'H' => &[
                    &[[0.0, 0.0], [0.0, 1.0]],
                    &[[1.0, 0.0], [1.0, 1.0]],
                    &[[0.0, 0.5], [1.0, 0.5]],
                ],
                'I' | '1' | '|' => &[&[[0.5, 0.0], [0.5, 1.0]]],
                'L' => &[&[[0.0, 0.0], [0.0, 1.0], [1.0, 1.0]]],
                'M' => &[&[[0.0, 1.0], [0.0, 0.0], [0.5, 0.55], [1.0, 0.0], [1.0, 1.0]]],
                'N' => &[&[[0.0, 1.0], [0.0, 0.0], [1.0, 1.0], [1.0, 0.0]]],
                'P' => &[&[[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 0.5], [0.0, 0.5]]],
                'R' => &[
                    &[[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 0.5], [0.0, 0.5]],
                    &[[0.4, 0.5], [1.0, 1.0]],
                ],
                'S' | '5' => &[&[
                    [1.0, 0.0],
                    [0.0, 0.0],
                    [0.0, 0.5],
                    [1.0, 0.5],
                    [1.0, 1.0],
                    [0.0, 1.0],
                ]],
                'T' => &[&[[0.0, 0.0], [1.0, 0.0]], &[[0.5, 0.0], [0.5, 1.0]]],
                'U' | 'V' => &[&[[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]],
                'W' => &[&[[0.0, 0.0], [0.2, 1.0], [0.5, 0.45], [0.8, 1.0], [1.0, 0.0]]],
                'X' => &[&[[0.0, 0.0], [1.0, 1.0]], &[[1.0, 0.0], [0.0, 1.0]]],
                'Y' => &[
                    &[[0.0, 0.0], [0.5, 0.5], [1.0, 0.0]],
                    &[[0.5, 0.5], [0.5, 1.0]],
                ],
                'Z' | '2' => &[&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]]],
                '-' | '_' => &[&[[0.1, 0.55], [0.9, 0.55]]],
                '.' | ',' => {
                    let dot = [pt(0.35, 0.85), pt(0.65, 0.85), pt(0.65, 1.0), pt(0.35, 1.0)];
                    fill_polygon_aa(dst, &dot, data.color, 1.0);
                    continue;
                }
                _ => &[
                    &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]],
                    &[[0.0, 0.5], [1.0, 0.5]],
                ],
            };

            for seg in segments {
                let poly: Vec<[f64; 2]> = seg.iter().map(|&[nx, ny]| pt(nx, ny)).collect();
                stroke_polyline_aa(dst, &poly, false, stroke, 1.0);
            }
        }
    }
}

/// Rasterize a [`TextLayerData`] onto a transparent `canvas_width x canvas_height` [`RgbaImage`].
pub fn rasterize_text_layer(
    data: &TextLayerData,
    canvas_width: u32,
    canvas_height: u32,
) -> RgbaImage {
    let w = canvas_width.max(1);
    let h = canvas_height.max(1);
    let has_visible_chars = data.text.chars().any(|c| !c.is_whitespace());
    if !has_visible_chars {
        return RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 0]));
    }

    let svg = build_text_svg(data, w, h);
    let opt = usvg::Options {
        fontdb: system_fontdb(),
        font_family: if data.font_family.trim().is_empty() {
            "sans-serif".to_owned()
        } else {
            data.font_family.trim().to_owned()
        },
        font_size: data.font_size.clamp(1.0, 2048.0),
        ..usvg::Options::default()
    };

    if let Ok(tree) = usvg::Tree::from_str(&svg, &opt)
        && let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(w, h)
    {
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        let mut raw = pixmap.take();
        let mut any_visible = false;
        for px in raw.as_chunks_mut::<4>().0 {
            let a = px[3];
            if a > 0 {
                any_visible = true;
                if a < 255 {
                    let af = f32::from(a) / 255.0;
                    px[0] = (f32::from(px[0]) / af).round().clamp(0.0, 255.0) as u8;
                    px[1] = (f32::from(px[1]) / af).round().clamp(0.0, 255.0) as u8;
                    px[2] = (f32::from(px[2]) / af).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        if any_visible && let Some(img) = RgbaImage::from_raw(w, h, raw) {
            return img;
        }
    }

    let mut fallback = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 0]));
    render_fallback_vector_text(&mut fallback, data);
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rasterize_text_layer_produces_colored_glyph_pixels() {
        let data = TextLayerData {
            text: "PHOTON".to_owned(),
            font_family: "Sans Serif".to_owned(),
            font_size: 36.0,
            color: [240, 60, 40],
            align: TextAlign::Left,
            line_height: 1.25,
            bold: true,
            italic: false,
            box_width: None,
            x: 12.0,
            y: 12.0,
        };
        let img = rasterize_text_layer(&data, 220, 90);
        let non_empty: Vec<_> = img.pixels().filter(|p| p.0[3] > 0).collect();
        assert!(
            !non_empty.is_empty(),
            "expected rasterized text layer to contain non-transparent glyph pixels"
        );
        // Opaque glyph interior pixels should match the requested red color channel dominance.
        assert!(
            non_empty.iter().any(|p| p.0[0] > 200 && p.0[1] < 100),
            "expected red text pixels matching color [240, 60, 40]"
        );
    }

    #[test]
    fn wrap_text_lines_and_measure_bounds_respect_box_width_and_alignment() {
        let wrapped = wrap_text_lines(
            "Photon vector and text layers stay non-destructive",
            20.0,
            Some(120.0),
        );
        assert!(
            wrapped.len() >= 3,
            "expected text to wrap across multiple lines: {wrapped:?}"
        );

        let data = TextLayerData {
            text: "Line 1\nLine 2".to_owned(),
            font_size: 24.0,
            line_height: 1.5,
            align: TextAlign::Center,
            box_width: Some(200.0),
            x: 30.0,
            y: 40.0,
            ..TextLayerData::default()
        };
        let bounds = measure_text_bounds(&data);
        assert_eq!(bounds[0], 30.0);
        assert_eq!(bounds[1], 40.0);
        assert!(bounds[2] >= 200.0);
        assert!(bounds[3] > 48.0);
    }

    #[test]
    fn available_font_families_includes_standard_generics() {
        let families = available_font_families();
        assert!(
            families
                .iter()
                .any(|f| f.eq_ignore_ascii_case("Sans Serif"))
        );
        assert!(families.iter().any(|f| f.eq_ignore_ascii_case("Serif")));
        assert!(families.iter().any(|f| f.eq_ignore_ascii_case("Monospace")));
    }

    #[test]
    fn rasterize_text_layer_handles_alignment_italic_and_whitespace() {
        let empty = rasterize_text_layer(&TextLayerData::new("   \n  ", 10.0, 10.0), 64, 64);
        assert!(empty.pixels().all(|p| p.0[3] == 0));

        let centered = TextLayerData {
            text: "Centered\nItalic".to_owned(),
            font_family: "Monospace".to_owned(),
            font_size: 24.0,
            color: [60, 220, 120],
            align: TextAlign::Center,
            line_height: 1.3,
            bold: false,
            italic: true,
            box_width: Some(160.0),
            x: 10.0,
            y: 8.0,
        };
        let img = rasterize_text_layer(&centered, 200, 100);
        assert!(img.pixels().any(|p| p.0[1] > 180 && p.0[3] > 0));
    }
}
