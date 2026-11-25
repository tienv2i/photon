//! Photon Non-AI Image Utilities Plugin (`jsonl-stdio`, ADR 0024).
//!
//! Lightweight, zero-model utilities:
//! - `utils.strip_exif`: re-encodes clean pixel data without EXIF/GPS/XMP/IPTC metadata.
//! - `utils.optimize_size`: re-encodes with high-compression PNG or smart-quality JPEG/WebP.

use std::fs::File;
use std::io::{self, BufRead, BufWriter, Write};
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ColorType, DynamicImage, ImageEncoder};
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

fn write_clean(img: &DynamicImage, src_ext: &str, output: &Path, optimize: bool, jpeg_q: u8) -> Result<(), String> {
    let file = File::create(output).map_err(|e| format!("cannot create output: {e}"))?;
    let writer = BufWriter::new(file);

    match src_ext {
        "jpg" | "jpeg" => {
            let rgb = img.to_rgb8();
            let quality = if optimize { jpeg_q.clamp(50, 95) } else { 92 };
            let mut enc = JpegEncoder::new_with_quality(writer, quality);
            enc.encode_image(&rgb)
                .map_err(|e| format!("JPEG encode failed: {e}"))
        }
        "png" => {
            let has_alpha = img.color().has_alpha();
            let comp = if optimize {
                CompressionType::Best
            } else {
                CompressionType::Default
            };
            let filter = if optimize {
                FilterType::Adaptive
            } else {
                FilterType::Sub
            };
            let enc = PngEncoder::new_with_quality(writer, comp, filter);
            if has_alpha {
                let rgba = img.to_rgba8();
                enc.write_image(rgba.as_raw(), rgba.width(), rgba.height(), ColorType::Rgba8.into())
                    .map_err(|e| format!("PNG encode failed: {e}"))
            } else {
                let rgb = img.to_rgb8();
                enc.write_image(rgb.as_raw(), rgb.width(), rgb.height(), ColorType::Rgb8.into())
                    .map_err(|e| format!("PNG encode failed: {e}"))
            }
        }
        _ => {
            // Preserve PNG RGBA cleanly for all other formats.
            let rgba = img.to_rgba8();
            let enc = PngEncoder::new_with_quality(
                writer,
                if optimize { CompressionType::Best } else { CompressionType::Default },
                FilterType::Adaptive,
            );
            enc.write_image(rgba.as_raw(), rgba.width(), rgba.height(), ColorType::Rgba8.into())
                .map_err(|e| format!("PNG encode failed: {e}"))
        }
    }
}

fn run_util(req: &Request) -> Result<Value, (String, String)> {
    let input_path = Path::new(&req.input);
    let output_path = Path::new(&req.output);
    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();

    emit_progress(&req.id, 25, "Reading image…");
    let img = image::open(input_path)
        .map_err(|e| ("io_error".to_owned(), format!("cannot open image: {e}")))?;

    let optimize = req.action == "utils.optimize_size";
    let jpeg_q = req
        .params
        .get("jpegQuality")
        .and_then(Value::as_u64)
        .unwrap_or(86) as u8;

    emit_progress(
        &req.id,
        65,
        if optimize {
            "Optimizing compression…"
        } else {
            "Stripping EXIF / GPS metadata…"
        },
    );
    write_clean(&img, &ext, output_path, optimize, jpeg_q)
        .map_err(|e| ("encode_failed".to_owned(), e))?;

    Ok(json!({
        "device": "cpu",
        "width": img.width(),
        "height": img.height(),
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
            Ok(req) => req,
            Err(err) => {
                emit_error("", "invalid_request", &format!("invalid JSON request: {err}"));
                continue;
            }
        };
        match req.op.as_str() {
            "shutdown" => break,
            "cancel" => {}
            "run" => match req.action.as_str() {
                "utils.strip_exif" | "utils.optimize_size" => match run_util(&req) {
                    Ok(meta) => emit(json!({
                        "id": req.id,
                        "event": "done",
                        "output": req.output,
                        "meta": meta,
                    })),
                    Err((code, msg)) => emit_error(&req.id, &code, &msg),
                },
                other => emit_error(
                    &req.id,
                    "unsupported_action",
                    &format!("unsupported action '{other}'"),
                ),
            },
            other => emit_error(
                &req.id,
                "unsupported_op",
                &format!("unsupported op '{other}'"),
            ),
        }
    }
}
