//! Photon AI Watermark Remover ONNX Plugin (`jsonl-stdio`, ADR 0024).
//!
//! Combines a selectable YOLO watermark detector (`watermark-yolo26` or
//! `watermark-yolo11`, or `"none"` for manual region mode) with a selectable
//! ONNX inpainter (`migan-pipeline-v2` for fast CPU removal or `lama-fp32` for
//! high-quality large-mask Fourier inpainting).

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use image::imageops::{self, FilterType};
use image::{GrayImage, Luma, RgbImage, RgbaImage};
use ndarray::Array4;
use ort::environment::Environment;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use serde::Deserialize;
use serde_json::{json, Value};

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

#[derive(Debug, Clone, Copy)]
struct DetBox {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    score: f32,
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

static ORT_ENV: OnceLock<Result<Environment, String>> = OnceLock::new();

fn ort_env() -> Result<&'static Environment, String> {
    let res = ORT_ENV.get_or_init(|| {
        ort::init()
            .with_name("photon-plugin-ai-inpaint")
            .build()
            .map_err(|e| format!("ort environment init failed: {e}"))
    });
    res.as_ref().map_err(Clone::clone)
}

fn build_session(model_path: &Path) -> Result<Session, String> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 8))
        .unwrap_or(4);
    let env = ort_env()?;
    Session::builder(env)
        .map_err(|e| format!("ort session builder failed: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| format!("ort optimization level failed: {e}"))?
        .with_intra_threads(threads)
        .map_err(|e| format!("ort threads failed: {e}"))?
        .commit_from_file(model_path)
        .map_err(|e| format!("failed to load ONNX model '{}': {e}", model_path.display()))
}

fn model_spec(model_id: &str) -> Option<(&'static str, &'static str)> {
    match model_id {
        "watermark-yolo26" => Some((
            "watermark-yolo26.onnx",
            "https://huggingface.co/ayan4m1/Watermark-Detection-YOLO26-ONNX/resolve/main/onnx/model.onnx",
        )),
        "watermark-yolo11" => Some((
            "watermark-yolo11.onnx",
            "https://huggingface.co/ayan4m1/Watermark-Detection-YOLO11-ONNX/resolve/main/onnx/model.onnx",
        )),
        "migan-pipeline-v2" => Some((
            "migan-pipeline-v2.onnx",
            "https://huggingface.co/andraniksargsyan/migan/resolve/main/migan_pipeline_v2.onnx",
        )),
        "lama-fp32" => Some((
            "lama-fp32.onnx",
            "https://huggingface.co/Carve/LaMa-ONNX/resolve/main/lama_fp32.onnx",
        )),
        _ => None,
    }
}

fn ensure_model(req_id: &str, models_dir: &Path, model_id: &str, label: &str) -> Result<PathBuf, String> {
    let (file_name, url) = model_spec(model_id)
        .ok_or_else(|| format!("Unknown model id '{model_id}'"))?;
    let path = models_dir.join(file_name);
    if path.is_file() && fs::metadata(&path).map(|m| m.len() > 1024).unwrap_or(false) {
        return Ok(path);
    }
    fs::create_dir_all(models_dir)
        .map_err(|e| format!("cannot create models dir '{}': {e}", models_dir.display()))?;
    emit_progress(req_id, 12, &format!("Downloading {label}…"));
    let tmp_path = models_dir.join(format!("{file_name}.part"));
    let status = Command::new("curl")
        .args(["-fSL", "--retry", "2", "--connect-timeout", "20", "-o"])
        .arg(&tmp_path)
        .arg(url)
        .status()
        .map_err(|e| format!("failed to launch curl for {label}: {e}"))?;
    if !status.success() {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!(
            "Failed to download {label} ({file_name}). Download it in Preferences → Plugins & AI."
        ));
    }
    fs::rename(&tmp_path, &path)
        .map_err(|e| format!("failed to save downloaded model '{file_name}': {e}"))?;
    Ok(path)
}

fn iou(a: DetBox, b: DetBox) -> f32 {
    let ix1 = a.x1.max(b.x1);
    let iy1 = a.y1.max(b.y1);
    let ix2 = a.x2.min(b.x2);
    let iy2 = a.y2.min(b.y2);
    let iw = (ix2 - ix1).max(0.0);
    let ih = (iy2 - iy1).max(0.0);
    let inter = iw * ih;
    let area_a = (a.x2 - a.x1).max(0.0) * (a.y2 - a.y1).max(0.0);
    let area_b = (b.x2 - b.x1).max(0.0) * (b.y2 - b.y1).max(0.0);
    let union = area_a + area_b - inter;
    if union <= 1e-6 {
        0.0
    } else {
        inter / union
    }
}

fn nms(mut boxes: Vec<DetBox>, iou_thresh: f32) -> Vec<DetBox> {
    boxes.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut keep = Vec::new();
    for b in boxes {
        if keep.iter().all(|&k| iou(k, b) < iou_thresh) {
            keep.push(b);
        }
    }
    keep
}

/// Run YOLO26 or YOLO11 watermark detector on `rgba` and return detected boxes
/// in original image coordinates.
///
/// Matches `preprocessor_config.json` of `ayan4m1/Watermark-Detection-YOLO*-ONNX`:
/// resize longest edge to 640, bottom-right pad to 640x640, rescale by 1/255.
fn detect_watermarks(
    rgba: &RgbaImage,
    model_path: &Path,
    confidence: f32,
) -> Result<Vec<DetBox>, String> {
    let (orig_w, orig_h) = rgba.dimensions();
    let max_dim = orig_w.max(orig_h).max(1) as f32;
    let scale = 640.0 / max_dim;
    let new_w = ((orig_w as f32 * scale).round() as u32).clamp(1, 640);
    let new_h = ((orig_h as f32 * scale).round() as u32).clamp(1, 640);

    let rgb = image::DynamicImage::ImageRgba8(rgba.clone()).to_rgb8();
    let resized = imageops::resize(&rgb, new_w, new_h, FilterType::Triangle);

    let mut input = Array4::<f32>::zeros((1, 3, 640, 640));
    for y in 0..new_h as usize {
        for x in 0..new_w as usize {
            let p = resized.get_pixel(x as u32, y as u32);
            input[[0, 0, y, x]] = p[0] as f32 / 255.0;
            input[[0, 1, y, x]] = p[1] as f32 / 255.0;
            input[[0, 2, y, x]] = p[2] as f32 / 255.0;
        }
    }

    let mut session = build_session(model_path)?;
    let tensor = Tensor::from_array(input)
        .map_err(|e| format!("detector input tensor error: {e}"))?;
    let outputs = session
        .run(ort::inputs![tensor])
        .map_err(|e| format!("detector inference failed: {e}"))?;

    let (shape, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("detector output extract failed: {e}"))?;

    let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
    let mut raw_boxes = Vec::new();

    if dims.len() == 3 && dims[2] == 6 {
        // End-to-end NMS-free YOLO26: [1, N, 6] => [x1, y1, x2, y2, score, class_id]
        let n = dims[1];
        for i in 0..n {
            let base = i * 6;
            let score = data[base + 4];
            if score < confidence {
                continue;
            }
            let x1 = (data[base] / scale).clamp(0.0, orig_w as f32);
            let y1 = (data[base + 1] / scale).clamp(0.0, orig_h as f32);
            let x2 = (data[base + 2] / scale).clamp(0.0, orig_w as f32);
            let y2 = (data[base + 3] / scale).clamp(0.0, orig_h as f32);
            if x2 - x1 >= 2.0 && y2 - y1 >= 2.0 {
                raw_boxes.push(DetBox { x1, y1, x2, y2, score });
            }
        }
    } else if dims.len() == 3 && dims[1] >= 5 {
        // Anchor-based YOLO11: [1, 5+C, anchors] => rows [cx, cy, w, h, score...]
        let channels = dims[1];
        let anchors = dims[2];
        for a in 0..anchors {
            let mut score = 0.0f32;
            for c in 4..channels {
                score = score.max(data[c * anchors + a]);
            }
            if score < confidence {
                continue;
            }
            let cx = data[a];
            let cy = data[anchors + a];
            let bw = data[2 * anchors + a];
            let bh = data[3 * anchors + a];
            let x1 = ((cx - bw * 0.5) / scale).clamp(0.0, orig_w as f32);
            let y1 = ((cy - bh * 0.5) / scale).clamp(0.0, orig_h as f32);
            let x2 = ((cx + bw * 0.5) / scale).clamp(0.0, orig_w as f32);
            let y2 = ((cy + bh * 0.5) / scale).clamp(0.0, orig_h as f32);
            if x2 - x1 >= 2.0 && y2 - y1 >= 2.0 {
                raw_boxes.push(DetBox { x1, y1, x2, y2, score });
            }
        }
    } else {
        return Err(format!("Unsupported detector output shape: {dims:?}"));
    }

    Ok(nms(raw_boxes, 0.45))
}

/// Paint a filled disk onto `mask` at `(cx, cy)` with radius `r`.
fn paint_disk(mask: &mut GrayImage, cx: f32, cy: f32, r: f32) {
    let (w, h) = mask.dimensions();
    let r2 = r * r;
    let x0 = ((cx - r).floor() as i32).clamp(0, w as i32 - 1);
    let y0 = ((cy - r).floor() as i32).clamp(0, h as i32 - 1);
    let x1 = ((cx + r).ceil() as i32).clamp(0, w as i32 - 1);
    let y1 = ((cy + r).ceil() as i32).clamp(0, h as i32 - 1);
    for y in y0..=y1 {
        let dy = y as f32 - cy;
        for x in x0..=x1 {
            let dx = x as f32 - cx;
            if dx * dx + dy * dy <= r2 {
                mask.put_pixel(x as u32, y as u32, Luma([255u8]));
            }
        }
    }
}

/// Rasterize freehand brush strokes (`[{ radius, points: [[x,y], ...] }]`) onto a
/// binary mask and return per-stroke bounding boxes for LaMa clustering.
fn build_mask_from_brush(w: u32, h: u32, strokes_val: &Value) -> Result<(GrayImage, Vec<DetBox>), String> {
    let Some(arr) = strokes_val.as_array() else {
        return Err("No brush strokes drawn. Paint over the watermark on the image first.".to_owned());
    };
    let mut mask = GrayImage::from_pixel(w, h, Luma([0u8]));
    let mut boxes = Vec::new();
    for s in arr {
        let radius = s
            .get("radius")
            .and_then(|v| v.as_f64())
            .unwrap_or(16.0)
            .clamp(2.0, 200.0) as f32;
        let Some(pts) = s.get("points").and_then(|v| v.as_array()) else {
            continue;
        };
        if pts.is_empty() {
            continue;
        }
        let mut parsed: Vec<(f32, f32)> = Vec::with_capacity(pts.len());
        for p in pts {
            if let Some(pair) = p.as_array() {
                if pair.len() >= 2 {
                    if let (Some(x), Some(y)) = (pair[0].as_f64(), pair[1].as_f64()) {
                        parsed.push((x as f32, y as f32));
                    }
                }
            }
        }
        if parsed.is_empty() {
            continue;
        }
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (w as f32, h as f32, 0.0f32, 0.0f32);
        for &(x, y) in &parsed {
            min_x = min_x.min(x - radius);
            min_y = min_y.min(y - radius);
            max_x = max_x.max(x + radius);
            max_y = max_y.max(y + radius);
        }
        if parsed.len() == 1 {
            paint_disk(&mut mask, parsed[0].0, parsed[0].1, radius);
        } else {
            for win in parsed.windows(2) {
                let (x0, y0) = win[0];
                let (x1, y1) = win[1];
                let dist = ((x1 - x0).hypot(y1 - y0)).max(1.0);
                let step = (radius * 0.35).max(1.5);
                let steps = (dist / step).ceil() as usize;
                for i in 0..=steps {
                    let t = i as f32 / steps.max(1) as f32;
                    paint_disk(&mut mask, x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, radius);
                }
            }
        }
        boxes.push(DetBox {
            x1: min_x.clamp(0.0, w as f32),
            y1: min_y.clamp(0.0, h as f32),
            x2: max_x.clamp(0.0, w as f32),
            y2: max_y.clamp(0.0, h as f32),
            score: 1.0,
        });
    }
    if boxes.is_empty() {
        return Err("No brush strokes drawn. Paint over the watermark on the image first.".to_owned());
    }
    Ok((mask, boxes))
}

/// Build a binary mask (`255` = remove, `0` = keep) from detected boxes + padding.
fn build_mask_from_boxes(w: u32, h: u32, boxes: &[DetBox], pad: u32) -> GrayImage {
    let mut mask = GrayImage::from_pixel(w, h, Luma([0u8]));
    let pad_f = pad as f32;
    for b in boxes {
        let x1 = ((b.x1 - pad_f).floor() as i32).clamp(0, w as i32) as u32;
        let y1 = ((b.y1 - pad_f).floor() as i32).clamp(0, h as i32) as u32;
        let x2 = ((b.x2 + pad_f).ceil() as i32).clamp(0, w as i32) as u32;
        let y2 = ((b.y2 + pad_f).ceil() as i32).clamp(0, h as i32) as u32;
        for y in y1..y2 {
            for x in x1..x2 {
                mask.put_pixel(x, y, Luma([255u8]));
            }
        }
    }
    mask
}

/// Run MI-GAN v2 pipeline (`migan_pipeline_v2.onnx`):
/// - `image`: uint8 `[1, 3, H, W]` RGB `0..=255`
/// - `mask`:  uint8 `[1, 1, H, W]` (`255` = keep, `0` = hole/remove)
/// - `result`: uint8 `[1, 3, H, W]` RGB `0..=255`
fn inpaint_migan(
    rgba: &RgbaImage,
    remove_mask: &GrayImage,
    model_path: &Path,
) -> Result<RgbaImage, String> {
    let (w, h) = rgba.dimensions();
    let (wu, hu) = (w as usize, h as usize);

    let mut img_arr = Array4::<u8>::zeros((1, 3, hu, wu));
    let mut mask_arr = Array4::<u8>::zeros((1, 1, hu, wu));

    for y in 0..hu {
        for x in 0..wu {
            let p = rgba.get_pixel(x as u32, y as u32);
            img_arr[[0, 0, y, x]] = p[0];
            img_arr[[0, 1, y, x]] = p[1];
            img_arr[[0, 2, y, x]] = p[2];
            // MI-GAN polarity: 255 = known/keep, 0 = hole/remove
            let m = remove_mask.get_pixel(x as u32, y as u32)[0];
            mask_arr[[0, 0, y, x]] = if m > 127 { 0u8 } else { 255u8 };
        }
    }

    let mut session = build_session(model_path)?;
    let img_t = Tensor::from_array(img_arr)
        .map_err(|e| format!("MI-GAN image tensor error: {e}"))?;
    let mask_t = Tensor::from_array(mask_arr)
        .map_err(|e| format!("MI-GAN mask tensor error: {e}"))?;

    let outputs = session
        .run(ort::inputs!["image" => img_t, "mask" => mask_t])
        .map_err(|e| format!("MI-GAN inference failed: {e}"))?;

    let (shape, data) = outputs[0]
        .try_extract_tensor::<u8>()
        .map_err(|e| format!("MI-GAN output extract failed: {e}"))?;
    let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
    if dims.len() != 4 || dims[1] != 3 || dims[2] != hu || dims[3] != wu {
        return Err(format!("Unexpected MI-GAN output shape: {dims:?}"));
    }

    let plane = hu * wu;
    let soft_mask = imageops::blur(remove_mask, 1.5);
    let mut out = rgba.clone();
    for y in 0..hu {
        for x in 0..wu {
            let hard = remove_mask.get_pixel(x as u32, y as u32)[0];
            let soft = soft_mask.get_pixel(x as u32, y as u32)[0];
            let alpha = (hard.max(soft) as f32) / 255.0;
            if alpha <= 0.001 {
                continue;
            }
            let idx = y * wu + x;
            let orig = rgba.get_pixel(x as u32, y as u32);
            let r = (orig[0] as f32 * (1.0 - alpha) + data[idx] as f32 * alpha).round() as u8;
            let g = (orig[1] as f32 * (1.0 - alpha) + data[plane + idx] as f32 * alpha).round() as u8;
            let b = (orig[2] as f32 * (1.0 - alpha) + data[2 * plane + idx] as f32 * alpha).round() as u8;
            out.put_pixel(x as u32, y as u32, image::Rgba([r, g, b, orig[3]]));
        }
    }
    Ok(out)
}

/// Cluster nearby/overlapping boxes (within `gap` px) so LaMa runs a tight
/// high-res 512×512 context crop around each watermark cluster instead of
/// downscaling the whole image when watermarks sit in opposite corners.
fn cluster_boxes(boxes: &[DetBox], pad: u32, w: u32, h: u32, gap: f32) -> Vec<(u32, u32, u32, u32)> {
    let pad_f = pad as f32;
    let mut rects: Vec<(f32, f32, f32, f32)> = boxes
        .iter()
        .map(|b| {
            (
                (b.x1 - pad_f).clamp(0.0, w as f32),
                (b.y1 - pad_f).clamp(0.0, h as f32),
                (b.x2 + pad_f).clamp(0.0, w as f32),
                (b.y2 + pad_f).clamp(0.0, h as f32),
            )
        })
        .collect();
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                let a = rects[i];
                let b = rects[j];
                if a.0 - gap <= b.2 && b.0 - gap <= a.2 && a.1 - gap <= b.3 && b.1 - gap <= a.3 {
                    rects[i] = (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3));
                    rects.remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
    }
    rects
        .into_iter()
        .filter_map(|(x1, y1, x2, y2)| {
            let ix1 = (x1.floor() as u32).min(w);
            let iy1 = (y1.floor() as u32).min(h);
            let ix2 = (x2.ceil() as u32).min(w);
            let iy2 = (y2.ceil() as u32).min(h);
            if ix2 > ix1 && iy2 > iy1 {
                Some((ix1, iy1, ix2, iy2))
            } else {
                None
            }
        })
        .collect()
}

/// Run LaMa FFC (`lama_fp32.onnx`, fixed 512×512) per watermark cluster using
/// tight context-crop compositing so high-resolution images preserve 100%
/// original sharpness outside each watermark.
fn inpaint_lama(
    rgba: &RgbaImage,
    remove_mask: &GrayImage,
    boxes: &[DetBox],
    pad: u32,
    model_path: &Path,
) -> Result<RgbaImage, String> {
    let (w, h) = rgba.dimensions();
    let clusters = cluster_boxes(boxes, pad, w, h, 64.0);
    if clusters.is_empty() {
        return Ok(rgba.clone());
    }

    let mut session = build_session(model_path)?;
    let mut out = rgba.clone();

    for (min_x, min_y, max_x, max_y) in clusters {
        let mask_w = max_x - min_x;
        let mask_h = max_y - min_y;
        let margin = (mask_w.max(mask_h) / 2).max(128);
        let side = (mask_w + 2 * margin).max(mask_h + 2 * margin).max(512);
        let crop_w = side.min(w);
        let crop_h = side.min(h);
        let cx = (min_x + max_x) / 2;
        let cy = (min_y + max_y) / 2;
        let crop_x = cx.saturating_sub(crop_w / 2).min(w - crop_w);
        let crop_y = cy.saturating_sub(crop_h / 2).min(h - crop_h);

        let mut crop_rgb = RgbImage::new(crop_w, crop_h);
        let mut crop_mask = GrayImage::new(crop_w, crop_h);
        for y in 0..crop_h {
            for x in 0..crop_w {
                let p = out.get_pixel(crop_x + x, crop_y + y);
                crop_rgb.put_pixel(x, y, image::Rgb([p[0], p[1], p[2]]));
                crop_mask.put_pixel(x, y, *remove_mask.get_pixel(crop_x + x, crop_y + y));
            }
        }

        let img_512 = imageops::resize(&crop_rgb, 512, 512, FilterType::Lanczos3);
        let mask_512 = imageops::resize(&crop_mask, 512, 512, FilterType::Nearest);

        let mut img_arr = Array4::<f32>::zeros((1, 3, 512, 512));
        let mut mask_arr = Array4::<f32>::zeros((1, 1, 512, 512));
        for y in 0..512usize {
            for x in 0..512usize {
                let p = img_512.get_pixel(x as u32, y as u32);
                img_arr[[0, 0, y, x]] = p[0] as f32 / 255.0;
                img_arr[[0, 1, y, x]] = p[1] as f32 / 255.0;
                img_arr[[0, 2, y, x]] = p[2] as f32 / 255.0;
                mask_arr[[0, 0, y, x]] = if mask_512.get_pixel(x as u32, y as u32)[0] > 127 {
                    1.0
                } else {
                    0.0
                };
            }
        }

        let img_t = Tensor::from_array(img_arr)
            .map_err(|e| format!("LaMa image tensor error: {e}"))?;
        let mask_t = Tensor::from_array(mask_arr)
            .map_err(|e| format!("LaMa mask tensor error: {e}"))?;

        let outputs = session
            .run(ort::inputs!["image" => img_t, "mask" => mask_t])
            .map_err(|e| format!("LaMa inference failed: {e}"))?;

        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("LaMa output extract failed: {e}"))?;
        let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
        if dims.len() != 4 || dims[1] != 3 || dims[2] != 512 || dims[3] != 512 {
            return Err(format!("Unexpected LaMa output shape: {dims:?}"));
        }

        let max_val = data.iter().copied().fold(0.0f32, f32::max);
        let scale = if max_val <= 1.5 { 255.0 } else { 1.0 };
        let plane = 512 * 512;
        let mut filled_512 = RgbImage::new(512, 512);
        for y in 0..512usize {
            for x in 0..512usize {
                let idx = y * 512 + x;
                let r = (data[idx] * scale).round().clamp(0.0, 255.0) as u8;
                let g = (data[plane + idx] * scale).round().clamp(0.0, 255.0) as u8;
                let b = (data[2 * plane + idx] * scale).round().clamp(0.0, 255.0) as u8;
                filled_512.put_pixel(x as u32, y as u32, image::Rgb([r, g, b]));
            }
        }

        let filled_crop = imageops::resize(&filled_512, crop_w, crop_h, FilterType::Lanczos3);
        let soft_mask = imageops::blur(&crop_mask, 1.5);

        for y in 0..crop_h {
            for x in 0..crop_w {
                let hard = crop_mask.get_pixel(x, y)[0];
                let soft = soft_mask.get_pixel(x, y)[0];
                let alpha = (hard.max(soft) as f32) / 255.0;
                if alpha <= 0.001 {
                    continue;
                }
                let ox = crop_x + x;
                let oy = crop_y + y;
                let orig = out.get_pixel(ox, oy);
                let fill = filled_crop.get_pixel(x, y);
                let r = (orig[0] as f32 * (1.0 - alpha) + fill[0] as f32 * alpha).round() as u8;
                let g = (orig[1] as f32 * (1.0 - alpha) + fill[1] as f32 * alpha).round() as u8;
                let b = (orig[2] as f32 * (1.0 - alpha) + fill[2] as f32 * alpha).round() as u8;
                out.put_pixel(ox, oy, image::Rgba([r, g, b, orig[3]]));
            }
        }
    }
    Ok(out)
}

fn default_models_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("PHOTON_MODELS_DIR").filter(|s| !s.is_empty()) {
        return PathBuf::from(dir);
    }
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_else(|| ".".into());
            PathBuf::from(home).join(".local/share")
        });
    for sub in ["photon/models", "photon-dev/models", "kogaview/models"] {
        let candidate = data_home.join(sub);
        if candidate.is_dir() {
            return candidate;
        }
    }
    data_home.join("photon/models")
}

fn handle_infer(req: &Request) -> Result<Value, String> {
    if req.action != "editor.ai_inpaint" {
        return Err(format!("Unsupported action '{}'", req.action));
    }
    if req.input.is_empty() || req.output.is_empty() {
        return Err("Missing input or output image path".to_owned());
    }

    let models_dir = req
        .params
        .get("modelsDir")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_models_dir);

    let detector_model = req
        .params
        .get("detectorModel")
        .and_then(|v| v.as_str())
        .unwrap_or("watermark-yolo26");
    let inpainter_model = req
        .params
        .get("inpainterModel")
        .and_then(|v| v.as_str())
        .unwrap_or("migan-pipeline-v2");
    let confidence = req
        .params
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.35)
        .clamp(0.05, 0.99) as f32;
    let mask_padding = req
        .params
        .get("maskPadding")
        .and_then(|v| v.as_u64())
        .unwrap_or(8)
        .min(64) as u32;

    emit_progress(&req.id, 10, "Loading image…");
    let img = image::open(&req.input)
        .map_err(|e| format!("failed to open '{}': {e}", req.input))?
        .to_rgba8();
    let (w, h) = img.dimensions();

    let (mask, boxes, effective_pad) = if detector_model == "brush" {
        let strokes = req.params.get("brushStrokes").unwrap_or(&Value::Null);
        let (m, bs) = build_mask_from_brush(w, h, strokes)?;
        (m, bs, 0u32)
    } else if detector_model == "none" {
        let rx = req.params.get("regionX").and_then(|v| v.as_i64()).unwrap_or(0) as f32;
        let ry = req.params.get("regionY").and_then(|v| v.as_i64()).unwrap_or(0) as f32;
        let rw = req.params.get("regionWidth").and_then(|v| v.as_i64()).unwrap_or(120).max(4) as f32;
        let rh = req.params.get("regionHeight").and_then(|v| v.as_i64()).unwrap_or(60).max(4) as f32;
        let b = DetBox {
            x1: rx,
            y1: ry,
            x2: rx + rw,
            y2: ry + rh,
            score: 1.0,
        };
        let bs = vec![b];
        (build_mask_from_boxes(w, h, &bs, 0), bs, 0u32)
    } else {
        let det_path = ensure_model(&req.id, &models_dir, detector_model, detector_model)?;
        emit_progress(&req.id, 30, &format!("Detecting watermarks ({detector_model})…"));
        let raw_bs = detect_watermarks(&img, &det_path, confidence)?;
        if raw_bs.is_empty() {
            return Err(format!(
                "No watermark detected ({detector_model}, confidence >= {confidence:.2}). Try lowering confidence or switching to Manual region."
            ));
        }
        let max_area_ratio = req
            .params
            .get("maxBoxAreaRatio")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.35)
            .clamp(0.05, 1.0) as f32;
        let img_area = (w as f32 * h as f32).max(1.0);
        let bs: Vec<DetBox> = raw_bs
            .into_iter()
            .filter(|b| {
                let area = (b.x2 - b.x1).max(0.0) * (b.y2 - b.y1).max(0.0);
                area / img_area <= max_area_ratio
            })
            .collect();
        if bs.is_empty() {
            return Err(
                "Detector only found a full-image watermark (>35% area), which inpainting cannot safely erase. Switch to Manual region to target specific logos or text."
                    .to_owned(),
            );
        }
        (build_mask_from_boxes(w, h, &bs, mask_padding), bs, mask_padding)
    };
    let detected_count = boxes.len();

    let inp_path = ensure_model(&req.id, &models_dir, inpainter_model, inpainter_model)?;
    emit_progress(
        &req.id,
        60,
        &format!("Inpainting {detected_count} region(s) with {inpainter_model}…"),
    );

    let result = match inpainter_model {
        "lama-fp32" => inpaint_lama(&img, &mask, &boxes, effective_pad, &inp_path)?,
        _ => inpaint_migan(&img, &mask, &inp_path)?,
    };

    emit_progress(&req.id, 92, "Saving result…");
    result
        .save(&req.output)
        .map_err(|e| format!("failed to write '{}': {e}", req.output))?;

    Ok(json!({
        "id": req.id,
        "event": "done",
        "meta": {
            "output": req.output,
            "detectedCount": detected_count,
            "detectorModel": detector_model,
            "inpainterModel": inpainter_model,
        },
    }))
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
            Ok(r) => r,
            Err(e) => {
                emit_error("", "invalid_request", &format!("malformed json: {e}"));
                continue;
            }
        };
        match req.op.as_str() {
            "shutdown" => break,
            "cancel" => {}
            "ping" => emit(json!({ "id": req.id, "event": "pong", "ok": true })),
            "run" | "infer" => match handle_infer(&req) {
                Ok(res) => emit(res),
                Err(msg) => emit_error(&req.id, "ai_inpaint_failed", &msg),
            },
            other => emit_error(&req.id, "unsupported_op", &format!("unknown op '{other}'")),
        }
    }
}
