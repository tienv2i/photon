//! Liquify warp strokes (`Push`, `Bloat`, `Pucker`, `TwirlCw`, `TwirlCcw`, `Reconstruct`).

use image::{Rgba, RgbaImage};
use photon_protocol::LiquifyMode;

use crate::pixels::sample_bilinear;

/// Apply a Liquify stroke onto `src` using smooth `(1 - d^2/r^2)^2` falloff
/// and bilinear sampling across local dab bounding boxes.
pub fn liquify_stroke(
    src: &RgbaImage,
    baseline: Option<&RgbaImage>,
    points: &[[f64; 2]],
    mode: LiquifyMode,
    radius: f32,
    strength: f32,
) -> RgbaImage {
    let mut result = src.clone();
    if points.is_empty() {
        return result;
    }
    let w = result.width() as i32;
    let h = result.height() as i32;
    if w <= 0 || h <= 0 {
        return result;
    }
    let radius = radius.clamp(4.0, 400.0);
    let strength = strength.clamp(0.05, 1.0);
    let r2 = radius * radius;
    let step_dist = (radius * 0.22).max(2.0);

    let apply_dab = |img: &mut RgbaImage, cx: f32, cy: f32, dx: f32, dy: f32| {
        let x_min = ((cx - radius).floor() as i32).max(0);
        let y_min = ((cy - radius).floor() as i32).max(0);
        let x_max = ((cx + radius).ceil() as i32).min(w - 1);
        let y_max = ((cy + radius).ceil() as i32).min(h - 1);
        if x_min > x_max || y_min > y_max {
            return;
        }
        let bw = (x_max - x_min + 1) as usize;
        let bh = (y_max - y_min + 1) as usize;
        let mut patch = vec![[0u8; 4]; bw * bh];

        for py in y_min..=y_max {
            for px in x_min..=x_max {
                let rx = px as f32 - cx;
                let ry = py as f32 - cy;
                let d2 = rx * rx + ry * ry;
                let idx = ((py - y_min) as usize) * bw + ((px - x_min) as usize);
                if d2 >= r2 {
                    patch[idx] = img.get_pixel(px as u32, py as u32).0;
                    continue;
                }
                let t = 1.0 - d2 / r2;
                let falloff = t * t * strength;
                if mode == LiquifyMode::Reconstruct {
                    let cur = img.get_pixel(px as u32, py as u32).0;
                    if let Some(base) = baseline
                        && (px as u32) < base.width()
                        && (py as u32) < base.height()
                    {
                        let b = base.get_pixel(px as u32, py as u32).0;
                        let inv = 1.0 - falloff;
                        let mix = |a: u8, z: u8| -> u8 {
                            (f32::from(a) * inv + f32::from(z) * falloff)
                                .round()
                                .clamp(0.0, 255.0) as u8
                        };
                        patch[idx] = [
                            mix(cur[0], b[0]),
                            mix(cur[1], b[1]),
                            mix(cur[2], b[2]),
                            mix(cur[3], b[3]),
                        ];
                    } else {
                        patch[idx] = cur;
                    }
                    continue;
                }
                let (sx, sy) = match mode {
                    LiquifyMode::Push => (px as f32 - dx * falloff, py as f32 - dy * falloff),
                    LiquifyMode::Bloat => {
                        let scale = 1.0 - 0.35 * falloff;
                        (cx + rx * scale, cy + ry * scale)
                    }
                    LiquifyMode::Pucker => {
                        let scale = 1.0 + 0.35 * falloff;
                        (cx + rx * scale, cy + ry * scale)
                    }
                    LiquifyMode::TwirlCw => {
                        let angle = -0.45 * falloff;
                        let (sa, ca) = angle.sin_cos();
                        (cx + rx * ca - ry * sa, cy + rx * sa + ry * ca)
                    }
                    LiquifyMode::TwirlCcw => {
                        let angle = 0.45 * falloff;
                        let (sa, ca) = angle.sin_cos();
                        (cx + rx * ca - ry * sa, cy + rx * sa + ry * ca)
                    }
                    LiquifyMode::Reconstruct => unreachable!(),
                };
                patch[idx] = sample_bilinear(img, f64::from(sx), f64::from(sy)).0;
            }
        }
        for py in y_min..=y_max {
            for px in x_min..=x_max {
                let idx = ((py - y_min) as usize) * bw + ((px - x_min) as usize);
                img.put_pixel(px as u32, py as u32, Rgba(patch[idx]));
            }
        }
    };

    if points.len() == 1 {
        let p = points[0];
        if mode != LiquifyMode::Push {
            apply_dab(&mut result, p[0] as f32, p[1] as f32, 0.0, 0.0);
        }
        return result;
    }

    for seg in points.windows(2) {
        let p0 = seg[0];
        let p1 = seg[1];
        let x0 = p0[0] as f32;
        let y0 = p0[1] as f32;
        let x1 = p1[0] as f32;
        let y1 = p1[1] as f32;
        let vx = x1 - x0;
        let vy = y1 - y0;
        let dist = (vx * vx + vy * vy).sqrt();
        if dist < 0.25 {
            if mode != LiquifyMode::Push {
                apply_dab(&mut result, x1, y1, 0.0, 0.0);
            }
            continue;
        }
        let steps = ((dist / step_dist).ceil() as usize).max(1);
        let ddx = vx / steps as f32;
        let ddy = vy / steps as f32;
        for s in 1..=steps {
            let frac = s as f32 / steps as f32;
            apply_dab(&mut result, x0 + vx * frac, y0 + vy * frac, ddx, ddy);
        }
    }

    result
}
