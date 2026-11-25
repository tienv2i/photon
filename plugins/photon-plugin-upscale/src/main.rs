//! Photon AI Upscale (Super-Resolution) ONNX Plugin (`jsonl-stdio`, ADR 0024).
//!
//! Runs Real-ESRGAN (General & Anime 4x) and Swin2SR (2x/4x) `.onnx` models
//! out-of-process with tiled inference so large images never exhaust RAM/VRAM.
//! Preserves alpha channels and auto-detects the model's exact scale factor.

use std::io::{self, BufRead, Write};
use std::path::Path;

use image::imageops::{self, FilterType};
use image::{GrayImage, Luma, Rgba, RgbaImage};
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

fn build_session(model_path: &Path, requested_device: &str) -> Result<(Session, String), String> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 8))
        .unwrap_or(4);
    let env = ort::init()
        .with_name("photon-plugin-upscale")
        .build()
        .map_err(|e| format!("ort environment init failed: {e}"))?;

    let session = Session::builder(&env)
        .map_err(|e| format!("ort session builder failed: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| format!("ort optimization level failed: {e}"))?
        .with_intra_threads(threads)
        .map_err(|e| format!("ort threads failed: {e}"))?
        .commit_from_file(model_path)
        .map_err(|e| format!("failed to load ONNX model '{}': {e}", model_path.display()))?;

    let label = if requested_device == "gpu" {
        "cpu (gpu fallback)".to_owned()
    } else {
        "cpu".to_owned()
    };
    Ok((session, label))
}

fn run_tile(
    session: &mut Session,
    src: &RgbaImage,
    x0: u32,
    y0: u32,
    w: u32,
    h: u32,
) -> Result<(u32, u32, Vec<u8>), String> {
    let (src_w, src_h) = src.dimensions();
    // Pad tile dimensions to a multiple of 8 for Swin2SR/ESRGAN window compatibility.
    let pad_w = ((w + 7) / 8) * 8;
    let pad_h = ((h + 7) / 8) * 8;

    let mut tensor = Array4::<f32>::zeros((1, 3, pad_h as usize, pad_w as usize));
    for ty in 0..pad_h {
        let sy = (y0 + ty).min(src_h - 1);
        for tx in 0..pad_w {
            let sx = (x0 + tx).min(src_w - 1);
            let p = src.get_pixel(sx, sy);
            tensor[[0, 0, ty as usize, tx as usize]] = p[0] as f32 / 255.0;
            tensor[[0, 1, ty as usize, tx as usize]] = p[1] as f32 / 255.0;
            tensor[[0, 2, ty as usize, tx as usize]] = p[2] as f32 / 255.0;
        }
    }

    let input_tensor = Tensor::from_array(tensor).map_err(|e| format!("tensor error: {e}"))?;
    let outputs = session
        .run(ort::inputs![input_tensor])
        .map_err(|e| format!("ONNX inference failed: {e}"))?;
    let (_, first_output) = outputs
        .iter()
        .next()
        .ok_or_else(|| "model produced no output".to_owned())?;
    let (out_shape, out_data) = first_output
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("output extract failed: {e}"))?;

    if out_shape.len() != 4 || out_shape[1] < 3 {
        return Err(format!("unexpected output shape: {out_shape:?}"));
    }
    let out_h = out_shape[2] as u32;
    let out_w = out_shape[3] as u32;
    let scale = (out_w / pad_w).max(1);
    let valid_w = (w * scale).min(out_w);
    let valid_h = (h * scale).min(out_h);
    let plane = (out_h as usize) * (out_w as usize);

    let mut rgb = vec![0u8; (valid_w as usize) * (valid_h as usize) * 3];
    for oy in 0..valid_h as usize {
        for ox in 0..valid_w as usize {
            let idx = oy * (out_w as usize) + ox;
            let r = (out_data[idx].clamp(0.0, 1.0) * 255.0).round() as u8;
            let g = (out_data[plane + idx].clamp(0.0, 1.0) * 255.0).round() as u8;
            let b = (out_data[2 * plane + idx].clamp(0.0, 1.0) * 255.0).round() as u8;
            let dst = (oy * (valid_w as usize) + ox) * 3;
            rgb[dst] = r;
            rgb[dst + 1] = g;
            rgb[dst + 2] = b;
        }
    }
    Ok((valid_w, valid_h, rgb))
}

fn run_upscale(req: &Request, cli_device: &str) -> Result<Value, (String, String)> {
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

    emit_progress(&req.id, 10, "Loading upscale ONNX model…");
    let (mut session, device_used) =
        build_session(model_path, device).map_err(|e| ("model_load_failed".to_owned(), e))?;

    let src = image::open(&req.input)
        .map_err(|e| ("io_error".to_owned(), format!("cannot open input image: {e}")))?
        .into_rgba8();
    let (src_w, src_h) = src.dimensions();

    // Respect fixed static input dimensions if the ONNX model requires them.
    let static_tile = session
        .inputs()
        .first()
        .and_then(|input| input.dtype().tensor_shape())
        .and_then(|shape| {
            if shape.len() == 4 && shape[2] > 0 && shape[3] > 0 {
                Some((shape[3] as u32, shape[2] as u32))
            } else {
                None
            }
        });

    let default_tile = req
        .params
        .get("tileSize")
        .and_then(Value::as_u64)
        .unwrap_or(192)
        .clamp(64, 512) as u32;
    let (tile_w, tile_h) = static_tile.unwrap_or((default_tile, default_tile));
    let overlap: u32 = 16.min(tile_w / 4).min(tile_h / 4);
    let step_x = (tile_w - overlap).max(1);
    let step_y = (tile_h - overlap).max(1);

    let cols = (src_w + step_x - 1) / step_x;
    let rows = (src_h + step_y - 1) / step_y;
    let total_tiles = (cols * rows).max(1);

    let mut out_img: Option<RgbaImage> = None;
    let mut detected_scale: u32 = 0;
    let mut done_tiles = 0u32;

    for gy in 0..rows {
        let y0 = gy * step_y;
        let h = tile_h.min(src_h.saturating_sub(y0)).max(1);
        for gx in 0..cols {
            let x0 = gx * step_x;
            let w = tile_w.min(src_w.saturating_sub(x0)).max(1);

            let (tw, th, rgb) = run_tile(&mut session, &src, x0, y0, w, h)
                .map_err(|e| ("inference_failed".to_owned(), e))?;
            let scale = (tw / w).max(1);
            if out_img.is_none() {
                detected_scale = scale;
                out_img = Some(RgbaImage::new(src_w * scale, src_h * scale));
            }
            let canvas = out_img.as_mut().unwrap();

            // Crop half the overlap on interior borders to eliminate tile seams.
            let crop_left = if gx > 0 { (overlap / 2) * scale } else { 0 };
            let crop_top = if gy > 0 { (overlap / 2) * scale } else { 0 };
            let out_x0 = x0 * scale;
            let out_y0 = y0 * scale;

            for ty in crop_top..th {
                let dy = out_y0 + ty;
                if dy >= canvas.height() {
                    break;
                }
                for tx in crop_left..tw {
                    let dx = out_x0 + tx;
                    if dx >= canvas.width() {
                        break;
                    }
                    let idx = ((ty * tw + tx) * 3) as usize;
                    canvas.put_pixel(dx, dy, Rgba([rgb[idx], rgb[idx + 1], rgb[idx + 2], 255]));
                }
            }

            done_tiles += 1;
            let pct = 15 + ((done_tiles as f32 / total_tiles as f32) * 78.0) as u8;
            emit_progress(
                &req.id,
                pct.min(93),
                &format!("Upscaling tile {done_tiles}/{total_tiles}…"),
            );
        }
    }

    let mut result = out_img.ok_or_else(|| {
        (
            "inference_failed".to_owned(),
            "no tiles were processed".to_owned(),
        )
    })?;

    // Preserve transparency cleanly when the source image has an alpha channel.
    let has_transparency = src.pixels().any(|p| p[3] < 255);
    if has_transparency {
        let mut alpha_src = GrayImage::new(src_w, src_h);
        for y in 0..src_h {
            for x in 0..src_w {
                alpha_src.put_pixel(x, y, Luma([src.get_pixel(x, y)[3]]));
            }
        }
        let alpha_up =
            imageops::resize(&alpha_src, result.width(), result.height(), FilterType::CatmullRom);
        for y in 0..result.height() {
            for x in 0..result.width() {
                result.get_pixel_mut(x, y)[3] = alpha_up.get_pixel(x, y)[0];
            }
        }
    }

    emit_progress(&req.id, 96, "Writing upscaled image…");
    result
        .save(&req.output)
        .map_err(|e| ("io_error".to_owned(), format!("cannot save output PNG: {e}")))?;

    Ok(json!({
        "device": device_used,
        "scale": detected_scale,
        "width": result.width(),
        "height": result.height(),
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
                if req.action != "editor.upscale" {
                    emit_error(
                        &req.id,
                        "unsupported_action",
                        &format!("unsupported action '{}'", req.action),
                    );
                    continue;
                }
                match run_upscale(&req, &cli_device) {
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
