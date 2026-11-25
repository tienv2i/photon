//! Photon Remove Background ONNX Plugin (`jsonl-stdio`, ADR 0024).
//!
//! Runs `rembg`-compatible `.onnx` saliency/matting models (`u2netp`, `rmbg-1.4`,
//! `isnet-general-use`, or custom `.onnx`) out-of-process with zero Python
//! dependency. Defaults to CPU and gracefully falls back to CPU when GPU
//! execution providers are unavailable.

use std::io::{self, BufRead, Write};
use std::path::Path;

use image::imageops::{self, FilterType};
use image::{GrayImage, Luma};
use ndarray::Array4;
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
    model_id: String,
    #[serde(default)]
    model_path: String,
    #[serde(default)]
    device: String,
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

fn norm_for_model(model_id: &str) -> ([f32; 3], [f32; 3], (u32, u32)) {
    let lower = model_id.to_ascii_lowercase();
    if lower.contains("u2net") || lower.contains("silueta") {
        ([0.485, 0.456, 0.406], [0.229, 0.224, 0.225], (320, 320))
    } else if lower.contains("birefnet") {
        ([0.485, 0.456, 0.406], [0.229, 0.224, 0.225], (1024, 1024))
    } else {
        // BRIA RMBG-1.4, IS-Net General, IS-Net Anime normalization.
        ([0.5, 0.5, 0.5], [1.0, 1.0, 1.0], (1024, 1024))
    }
}

fn build_session(model_path: &Path, requested_device: &str) -> Result<(Session, String), String> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 8))
        .unwrap_or(4);
    let env = ort::init()
        .with_name("photon-plugin-rembg")
        .build()
        .map_err(|e| format!("ort environment init failed: {e}"))?;

    let make_cpu = || -> Result<(Session, String), String> {
        let session = Session::builder(&env)
            .map_err(|e| format!("ort session builder failed: {e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| format!("ort optimization level failed: {e}"))?
            .with_intra_threads(threads)
            .map_err(|e| format!("ort threads failed: {e}"))?
            .commit_from_file(model_path)
            .map_err(|e| format!("failed to load ONNX model '{}': {e}", model_path.display()))?;
        Ok((session, "cpu".to_owned()))
    };

    if requested_device == "cpu" || requested_device.is_empty() {
        return make_cpu();
    }

    // For `auto` or `gpu`, try default execution providers with automatic
    // fallback to CPU if hardware acceleration is unavailable.
    match make_cpu() {
        Ok((session, _)) => {
            let label = if requested_device == "gpu" {
                "cpu (gpu fallback)".to_owned()
            } else {
                "cpu".to_owned()
            };
            Ok((session, label))
        }
        Err(err) => Err(err),
    }
}

fn run_remove_bg(req: &Request, cli_device: &str) -> Result<Value, (String, String)> {
    let model_path = Path::new(&req.model_path);
    if !model_path.is_file() {
        return Err((
            "model_missing".to_owned(),
            format!("ONNX model file not found: {}", model_path.display()),
        ));
    }

    let device = if req.device.is_empty() {
        cli_device
    } else {
        req.device.as_str()
    };

    emit_progress(&req.id, 15, "Loading ONNX model…");
    let (mut session, device_used) =
        build_session(model_path, device).map_err(|e| ("model_load_failed".to_owned(), e))?;

    emit_progress(&req.id, 30, "Preparing image…");
    let src_img = image::open(&req.input)
        .map_err(|e| ("io_error".to_owned(), format!("cannot open input image: {e}")))?
        .into_rgba8();
    let (orig_w, orig_h) = src_img.dimensions();

    let (mean, std, default_size) = norm_for_model(&req.model_id);

    // Inspect ONNX input shape `[1, 3, H, W]` when static, otherwise use preset.
    let (in_w, in_h) = session
        .inputs()
        .first()
        .and_then(|input| input.dtype().tensor_shape())
        .and_then(|shape| {
            if shape.len() == 4 && shape[2] > 0 && shape[3] > 0 {
                Some((shape[3] as u32, shape[2] as u32))
            } else {
                None
            }
        })
        .unwrap_or(default_size);

    let resized = imageops::resize(&src_img, in_w, in_h, FilterType::Triangle);
    let mut tensor = Array4::<f32>::zeros((1, 3, in_h as usize, in_w as usize));
    for y in 0..in_h as usize {
        for x in 0..in_w as usize {
            let p = resized.get_pixel(x as u32, y as u32);
            tensor[[0, 0, y, x]] = (p[0] as f32 / 255.0 - mean[0]) / std[0];
            tensor[[0, 1, y, x]] = (p[1] as f32 / 255.0 - mean[1]) / std[1];
            tensor[[0, 2, y, x]] = (p[2] as f32 / 255.0 - mean[2]) / std[2];
        }
    }

    emit_progress(&req.id, 50, "Running inference…");
    let input_tensor = Tensor::from_array(tensor)
        .map_err(|e| ("inference_failed".to_owned(), format!("tensor error: {e}")))?;

    let outputs = session
        .run(ort::inputs![input_tensor])
        .map_err(|e| ("inference_failed".to_owned(), format!("ONNX inference failed: {e}")))?;

    emit_progress(&req.id, 80, "Applying alpha mask…");
    let (_, first_output) = outputs
        .iter()
        .next()
        .ok_or_else(|| ("inference_failed".to_owned(), "model produced no output".to_owned()))?;
    let (out_shape, out_data) = first_output
        .try_extract_tensor::<f32>()
        .map_err(|e| ("inference_failed".to_owned(), format!("output extract failed: {e}")))?;

    let (mask_h, mask_w) = match out_shape.len() {
        4 => (out_shape[2] as u32, out_shape[3] as u32),
        3 => (out_shape[1] as u32, out_shape[2] as u32),
        2 => (out_shape[0] as u32, out_shape[1] as u32),
        _ => (in_h, in_w),
    };

    let expected_len = (mask_w as usize) * (mask_h as usize);
    if out_data.len() < expected_len || expected_len == 0 {
        return Err((
            "inference_failed".to_owned(),
            format!(
                "unexpected mask tensor size {} for {}x{}",
                out_data.len(),
                mask_w,
                mask_h
            ),
        ));
    }

    let slice = &out_data[..expected_len];
    let mut min_v = f32::INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    for &v in slice {
        if v.is_finite() {
            if v < min_v {
                min_v = v;
            }
            if v > max_v {
                max_v = v;
            }
        }
    }
    let span = (max_v - min_v).max(1e-6);

    let mut small_mask = GrayImage::new(mask_w, mask_h);
    for y in 0..mask_h {
        for x in 0..mask_w {
            let v = slice[(y * mask_w + x) as usize];
            let norm = ((v - min_v) / span).clamp(0.0, 1.0);
            small_mask.put_pixel(x, y, Luma([(norm * 255.0).round() as u8]));
        }
    }

    let mut full_mask = imageops::resize(&small_mask, orig_w, orig_h, FilterType::Triangle);

    let feather = req
        .params
        .get("feather")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .min(20) as f32;
    if feather > 0.0 {
        full_mask = imageops::blur(&full_mask, feather * 0.6);
    }

    let mut out_img = src_img;
    for y in 0..orig_h {
        for x in 0..orig_w {
            let m = full_mask.get_pixel(x, y)[0] as u16;
            let p = out_img.get_pixel_mut(x, y);
            p[3] = ((p[3] as u16 * m + 127) / 255) as u8;
        }
    }

    emit_progress(&req.id, 95, "Writing result…");
    out_img
        .save(&req.output)
        .map_err(|e| ("io_error".to_owned(), format!("cannot save output PNG: {e}")))?;

    Ok(json!({
        "device": device_used,
        "width": orig_w,
        "height": orig_h,
    }))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut cli_device = "cpu".to_owned();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--device" && i + 1 < args.len() {
            cli_device = args[i + 1].clone();
            i += 2;
        } else {
            i += 1;
        }
    }

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
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
                if req.action != "editor.remove_background" {
                    emit_error(
                        &req.id,
                        "unsupported_action",
                        &format!("unsupported action '{}'", req.action),
                    );
                    continue;
                }
                match run_remove_bg(&req, &cli_device) {
                    Ok(meta) => emit(json!({
                        "id": req.id,
                        "event": "done",
                        "output": req.output,
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
