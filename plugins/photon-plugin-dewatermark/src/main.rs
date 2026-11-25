//! Photon Watermark Remover Plugin (`jsonl-stdio`, ADR 0024).
//!
//! 1. **Gemini / Imagen Logo Removal (`editor.dewatermark_gemini`)**:
//!    Exact Reverse Alpha Blending ported from `gemini-watermark-remover`:
//!    `original = (watermarked - alpha * logo_value) / (1 - alpha)`
//!    using the embedded 48x48 (`bg_48.png`, 32px margin) and 96x96 (`bg_96.png`,
//!    64px margin when `width > 1024 && height > 1024`) reference alpha maps,
//!    with multi-method watermark presence scoring (`detect_watermark`) plus
//!    advanced overrides (`size`, `logoValue`, `force`, `offsetX`, `offsetY`).
//!
//! 2. **General Watermark Region Removal (`editor.inpaint_region`)**:
//!    Multi-scale harmonic diffusion inpainting with smooth boundary blending
//!    for removing arbitrary rectangular logos, timestamps, or corner marks.

use std::io::{self, BufRead, Write};
use std::path::Path;

use image::{Rgba, RgbaImage};
use serde::Deserialize;
use serde_json::{json, Value};

const BG_48_PNG: &[u8] = include_bytes!("../assets/bg_48.png");
const BG_96_PNG: &[u8] = include_bytes!("../assets/bg_96.png");

const ALPHA_THRESHOLD: f32 = 0.002;
const MAX_ALPHA: f32 = 0.99;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    #[serde(default)]
    id: String,
    op: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    input: String,
    #[serde(default)]
    output: String,
    #[serde(default)]
    params: Value,
}

fn emit(value: Value) {
    let mut out = io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

fn emit_progress(id: &str, percent: u8, stage: &str) {
    emit(json!({
        "id": id,
        "event": "progress",
        "percent": percent,
        "stage": stage,
    }));
}

fn emit_error(id: &str, code: &str, message: &str) {
    emit(json!({
        "id": id,
        "event": "error",
        "code": code,
        "message": message,
    }));
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WmSize {
    Small, // 48x48, margin 32
    Large, // 96x96, margin 64
}

impl WmSize {
    fn dims(self) -> (u32, u32, u32) {
        match self {
            Self::Small => (48, 48, 32),
            Self::Large => (96, 96, 64),
        }
    }

    fn for_image(w: u32, h: u32) -> Self {
        if w > 1024 && h > 1024 {
            Self::Large
        } else {
            Self::Small
        }
    }
}

fn load_alpha_map(size: WmSize) -> Result<Vec<f32>, String> {
    let bytes = match size {
        WmSize::Small => BG_48_PNG,
        WmSize::Large => BG_96_PNG,
    };
    let img = image::load_from_memory(bytes)
        .map_err(|e| format!("failed to decode embedded alpha map: {e}"))?
        .into_rgb8();
    let mut map = Vec::with_capacity((img.width() * img.height()) as usize);
    for p in img.pixels() {
        let max_ch = p[0].max(p[1]).max(p[2]);
        map.push(max_ch as f32 / 255.0);
    }
    Ok(map)
}

fn pearson_corr(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n < 2 {
        return 0.0;
    }
    let nf = n as f32;
    let mean_a = a.iter().take(n).sum::<f32>() / nf;
    let mean_b = b.iter().take(n).sum::<f32>() / nf;
    let mut num = 0.0f32;
    let mut da2 = 0.0f32;
    let mut db2 = 0.0f32;
    for i in 0..n {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        num += da * db;
        da2 += da * da;
        db2 += db * db;
    }
    let denom = (da2 * db2).sqrt();
    if denom < 1e-6 {
        0.0
    } else {
        num / denom
    }
}

fn std_dev(vals: &[f32]) -> f32 {
    if vals.is_empty() {
        return 0.0;
    }
    let n = vals.len() as f32;
    let mean = vals.iter().sum::<f32>() / n;
    let var = vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n;
    var.sqrt()
}

fn unblend_pixel(px: f32, alpha: f32, logo_value: f32) -> f32 {
    if alpha < ALPHA_THRESHOLD {
        return px;
    }
    let a = alpha.min(MAX_ALPHA);
    ((px - a * logo_value) / (1.0 - a)).clamp(0.0, 255.0)
}

/// Multi-method Gemini watermark detector ported from `gemini-watermark-remover`.
fn detect_gemini_watermark(
    img: &RgbaImage,
    x0: u32,
    y0: u32,
    w: u32,
    h: u32,
    alpha_map: &[f32],
    logo_value: f32,
) -> (bool, f32) {
    let n = (w * h) as usize;
    let mut gray = Vec::with_capacity(n);
    let mut diff_gray = Vec::with_capacity(n);
    let mut high_gray = Vec::new();
    let mut low_gray = Vec::new();
    let mut high_diff = Vec::new();

    for dy in 0..h {
        for dx in 0..w {
            let idx = (dy * w + dx) as usize;
            let a = alpha_map[idx];
            let p = img.get_pixel(x0 + dx, y0 + dy);
            let r = p[0] as f32;
            let g = p[1] as f32;
            let b = p[2] as f32;
            let lum = 0.299 * r + 0.587 * g + 0.114 * b;
            gray.push(lum);

            let rr = unblend_pixel(r, a, logo_value);
            let rg = unblend_pixel(g, a, logo_value);
            let rb = unblend_pixel(b, a, logo_value);
            let d = ((r - rr).abs() + (g - rg).abs() + (b - rb).abs()) / 3.0;
            diff_gray.push(d);

            if a > 0.3 {
                high_gray.push(lum);
                high_diff.push(d);
            } else if a < 0.1 {
                low_gray.push(lum);
            }
        }
    }

    let mut score = 0.0f32;
    let is_uniform = std_dev(&gray) < 15.0;
    let template_corr = pearson_corr(&gray, alpha_map);

    if !is_uniform && template_corr > 0.1 && std_dev(&diff_gray) > 1.0 {
        let diff_corr = pearson_corr(&diff_gray, alpha_map);
        if diff_corr > 0.7 {
            score += 35.0;
        } else if diff_corr > 0.5 {
            score += 25.0;
        } else if diff_corr > 0.3 {
            score += 15.0;
        }
    }

    let max_diff = diff_gray.iter().copied().fold(0.0f32, f32::max);
    let mean_diff_high = if high_diff.is_empty() {
        0.0
    } else {
        high_diff.iter().sum::<f32>() / high_diff.len() as f32
    };
    if !is_uniform && template_corr > 0.1 {
        if (5.0..150.0).contains(&mean_diff_high) {
            score += 15.0;
        }
        if (10.0..200.0).contains(&max_diff) {
            score += 10.0;
        }
    }

    if template_corr > 0.5 {
        score += 20.0;
    } else if template_corr > 0.3 {
        score += 12.0;
    } else if template_corr > 0.15 {
        score += 5.0;
    }

    if !high_gray.is_empty() && !low_gray.is_empty() {
        let mean_high = high_gray.iter().sum::<f32>() / high_gray.len() as f32;
        let mean_low = low_gray.iter().sum::<f32>() / low_gray.len() as f32;
        let bdiff = mean_high - mean_low;
        if bdiff > 30.0 {
            score += 15.0;
        } else if bdiff > 15.0 {
            score += 10.0;
        } else if bdiff > 5.0 {
            score += 5.0;
        }
        let sh = std_dev(&high_gray);
        if sh < 30.0 {
            score += 10.0;
        } else if sh < 50.0 {
            score += 5.0;
        }
    }

    (score >= 45.0, score)
}

fn run_gemini_dewatermark(req: &Request) -> Result<Value, (String, String)> {
    emit_progress(&req.id, 15, "Loading image…");
    let mut img = image::open(&req.input)
        .map_err(|e| ("io_error".to_owned(), format!("cannot open image: {e}")))?
        .into_rgba8();

    let img_w = img.width();
    let img_h = img.height();

    let size_param = req
        .params
        .get("size")
        .and_then(Value::as_str)
        .unwrap_or("auto");
    let size = match size_param {
        "small" | "48" => WmSize::Small,
        "large" | "96" => WmSize::Large,
        _ => WmSize::for_image(img_w, img_h),
    };
    let logo_value = req
        .params
        .get("logoValue")
        .and_then(Value::as_f64)
        .unwrap_or(255.0)
        .clamp(150.0, 255.0) as f32;
    let force = req
        .params
        .get("force")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let offset_x = req
        .params
        .get("offsetX")
        .and_then(Value::as_i64)
        .unwrap_or(0) as i32;
    let offset_y = req
        .params
        .get("offsetY")
        .and_then(Value::as_i64)
        .unwrap_or(0) as i32;

    let (wm_w, wm_h, margin) = size.dims();
    if img_w < wm_w || img_h < wm_h {
        return Err((
            "image_too_small".to_owned(),
            format!("image ({img_w}x{img_h}) is smaller than Gemini watermark ({wm_w}x{wm_h})"),
        ));
    }

    let base_x = (img_w as i32 - wm_w as i32 - margin as i32 + offset_x)
        .clamp(0, (img_w - wm_w) as i32) as u32;
    let base_y = (img_h as i32 - wm_h as i32 - margin as i32 + offset_y)
        .clamp(0, (img_h - wm_h) as i32) as u32;

    let alpha_map = load_alpha_map(size).map_err(|e| ("asset_error".to_owned(), e))?;

    emit_progress(&req.id, 40, "Detecting Gemini watermark…");
    let (detected, score) =
        detect_gemini_watermark(&img, base_x, base_y, wm_w, wm_h, &alpha_map, logo_value);

    if !detected && !force {
        return Err((
            "no_watermark".to_owned(),
            format!(
                "No Gemini watermark detected (score {:.0}/45). Use Force mode if the logo is faint or on a bright background.",
                score
            ),
        ));
    }

    emit_progress(&req.id, 70, "Applying Reverse Alpha Blending…");
    for dy in 0..wm_h {
        for dx in 0..wm_w {
            let a = alpha_map[(dy * wm_w + dx) as usize];
            if a < ALPHA_THRESHOLD {
                continue;
            }
            let p = *img.get_pixel(base_x + dx, base_y + dy);
            let r = unblend_pixel(p[0] as f32, a, logo_value).round() as u8;
            let g = unblend_pixel(p[1] as f32, a, logo_value).round() as u8;
            let b = unblend_pixel(p[2] as f32, a, logo_value).round() as u8;
            img.put_pixel(base_x + dx, base_y + dy, Rgba([r, g, b, p[3]]));
        }
    }

    emit_progress(&req.id, 92, "Saving cleaned image…");
    save_output(&img, Path::new(&req.output))?;

    Ok(json!({
        "device": "cpu",
        "watermarkSize": format!("{wm_w}x{wm_h}"),
        "detected": detected,
        "score": score,
        "forced": force,
    }))
}

/// General rectangular watermark removal via multi-scale harmonic diffusion
/// (Gauss-Seidel SOR solver initialized from directional boundary interpolation).
fn run_inpaint_region(req: &Request) -> Result<Value, (String, String)> {
    emit_progress(&req.id, 15, "Loading image…");
    let mut img = image::open(&req.input)
        .map_err(|e| ("io_error".to_owned(), format!("cannot open image: {e}")))?
        .into_rgba8();

    let img_w = img.width();
    let img_h = img.height();

    let rx = req
        .params
        .get("x")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .clamp(0, img_w.saturating_sub(1) as i64) as u32;
    let ry = req
        .params
        .get("y")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .clamp(0, img_h.saturating_sub(1) as i64) as u32;
    let rw = (req
        .params
        .get("width")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(1) as u32)
        .min(img_w.saturating_sub(rx));
    let rh = (req
        .params
        .get("height")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(1) as u32)
        .min(img_h.saturating_sub(ry));
    let feather = req
        .params
        .get("feather")
        .and_then(Value::as_u64)
        .unwrap_or(4)
        .clamp(0, 24) as u32;

    if rw == 0 || rh == 0 {
        return Err((
            "invalid_region".to_owned(),
            "Watermark region is empty".to_owned(),
        ));
    }

    emit_progress(&req.id, 40, "Inpainting watermark region…");

    // Extract padded patch (+2px border for Dirichlet boundary conditions)
    let pad = 2u32;
    let bx0 = rx.saturating_sub(pad);
    let by0 = ry.saturating_sub(pad);
    let bx1 = (rx + rw + pad).min(img_w);
    let by1 = (ry + rh + pad).min(img_h);
    let pw = (bx1 - bx0) as usize;
    let ph = (by1 - by0) as usize;

    let mx0 = (rx - bx0) as usize;
    let my0 = (ry - by0) as usize;
    let mx1 = mx0 + rw as usize;
    let my1 = my0 + rh as usize;

    let mut grid = vec![[0.0f32; 4]; pw * ph];
    let mut is_mask = vec![false; pw * ph];
    for y in 0..ph {
        for x in 0..pw {
            let p = img.get_pixel(bx0 + x as u32, by0 + y as u32);
            grid[y * pw + x] = [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32];
            if x >= mx0 && x < mx1 && y >= my0 && y < my1 {
                is_mask[y * pw + x] = true;
            }
        }
    }

    // Initialize interior with inverse-distance weighted interpolation from 4 edges
    let top_y = my0.saturating_sub(1);
    let bot_y = my1.min(ph - 1);
    let left_x = mx0.saturating_sub(1);
    let right_x = mx1.min(pw - 1);
    for y in my0..my1 {
        for x in mx0..mx1 {
            let wt = 1.0 / ((y - top_y).max(1) as f32);
            let wb = 1.0 / ((bot_y.saturating_sub(y)).max(1) as f32);
            let wl = 1.0 / ((x - left_x).max(1) as f32);
            let wr = 1.0 / ((right_x.saturating_sub(x)).max(1) as f32);
            let wsum = wt + wb + wl + wr;
            let pt = grid[top_y * pw + x];
            let pb = grid[bot_y * pw + x];
            let pl = grid[y * pw + left_x];
            let pr = grid[y * pw + right_x];
            let mut val = [0.0f32; 4];
            for c in 0..4 {
                val[c] = (pt[c] * wt + pb[c] * wb + pl[c] * wl + pr[c] * wr) / wsum;
            }
            grid[y * pw + x] = val;
        }
    }

    // Successive Over-Relaxation (SOR) harmonic solver for smooth seamless fill
    let iters = ((rw.max(rh) * 2).clamp(60, 260)) as usize;
    let omega = 1.72f32;
    for _ in 0..iters {
        for y in my0..my1 {
            for x in mx0..mx1 {
                let idx = y * pw + x;
                let n_up = grid[y.saturating_sub(1) * pw + x];
                let n_dn = grid[(y + 1).min(ph - 1) * pw + x];
                let n_lt = grid[y * pw + x.saturating_sub(1)];
                let n_rt = grid[y * pw + (x + 1).min(pw - 1)];
                let cur = grid[idx];
                let mut next = [0.0f32; 4];
                for c in 0..4 {
                    let avg = 0.25 * (n_up[c] + n_dn[c] + n_lt[c] + n_rt[c]);
                    next[c] = (cur[c] + omega * (avg - cur[c])).clamp(0.0, 255.0);
                }
                grid[idx] = next;
            }
        }
    }

    // Write back with optional edge feathering
    for y in my0..my1 {
        for x in mx0..mx1 {
            let solved = grid[y * pw + x];
            let orig = *img.get_pixel(bx0 + x as u32, by0 + y as u32);
            let blend = if feather == 0 {
                1.0f32
            } else {
                let dist_edge = (x - mx0)
                    .min(mx1 - 1 - x)
                    .min(y - my0)
                    .min(my1 - 1 - y) as f32;
                ((dist_edge + 1.0) / (feather as f32 + 1.0)).clamp(0.0, 1.0)
            };
            let r = (orig[0] as f32 * (1.0 - blend) + solved[0] * blend).round() as u8;
            let g = (orig[1] as f32 * (1.0 - blend) + solved[1] * blend).round() as u8;
            let b = (orig[2] as f32 * (1.0 - blend) + solved[2] * blend).round() as u8;
            let a = (orig[3] as f32 * (1.0 - blend) + solved[3] * blend).round() as u8;
            img.put_pixel(bx0 + x as u32, by0 + y as u32, Rgba([r, g, b, a]));
        }
    }

    emit_progress(&req.id, 92, "Saving inpainted image…");
    save_output(&img, Path::new(&req.output))?;

    Ok(json!({
        "device": "cpu",
        "region": [rx, ry, rw, rh],
        "feather": feather,
    }))
}

fn save_output(img: &RgbaImage, out_path: &Path) -> Result<(), (String, String)> {
    img.save(out_path)
        .map_err(|e| ("io_error".to_owned(), format!("cannot save output image: {e}")))
}

fn main() {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(err) => {
                emit_error("", "invalid_request", &format!("invalid JSON request: {err}"));
                continue;
            }
        };
        match req.op.as_str() {
            "shutdown" => break,
            "cancel" => {}
            "run" => {
                let res = match req.action.as_str() {
                    "editor.dewatermark_gemini" => run_gemini_dewatermark(&req),
                    "editor.inpaint_region" => run_inpaint_region(&req),
                    other => Err((
                        "unsupported_action".to_owned(),
                        format!("unsupported action '{other}'"),
                    )),
                };
                match res {
                    Ok(meta) => emit(json!({
                        "id": req.id,
                        "event": "done",
                        "meta": meta,
                    })),
                    Err((code, msg)) => emit_error(&req.id, &code, &msg),
                }
            }
            other => emit_error(
                &req.id,
                "unsupported_op",
                &format!("unsupported op '{other}'"),
            ),
        }
    }
}
