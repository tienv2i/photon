//! Photon OCR Plugin (`jsonl-stdio`, ADR 0024).
//!
//! 100% offline text extraction using system `tesseract` + optional on-demand
//! `.traineddata` packs downloaded into `$XDG_DATA_HOME/photon[-dev]/models/`.
//! Supports:
//! - Full-image Quick Actions (`meta.clipboardText`)
//! - Interactive region crop (`params.region = {x, y, width, height}`) & full-page
//!   TSV blocks (`params.returnTranslation = true`) for the Viewer overlay
//! - Tier-1 manga/complex-background preprocessing (2x Lanczos3 upscale for small
//!   crops, percentile contrast stretch, dark-background polarity retry, and
//!   vertical `--psm 5` for `jpn_vert`).

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use image::{imageops, GrayImage, Luma, RgbaImage};
use serde::{Deserialize, Serialize};
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
    input: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranslationBlock {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    original: String,
    translated: String,
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

/// Tier-1 preprocessing for manga / complex backgrounds:
/// - Upscale small crops 2x with Lanczos3 so small Kanji/Kana/Hangul strokes stay crisp
/// - Convert to grayscale and stretch contrast between the 2nd and 98th percentiles
/// - Return `(normal, inverted, mean_luma)` so dark-background white text is retried automatically.
fn preprocess_region(crop: &RgbaImage) -> (GrayImage, GrayImage, f32) {
    let scaled = if crop.width() < 420 || crop.height() < 140 {
        imageops::resize(
            crop,
            (crop.width() * 2).min(2400),
            (crop.height() * 2).min(2400),
            imageops::FilterType::Lanczos3,
        )
    } else {
        crop.clone()
    };

    let gray = imageops::grayscale(&scaled);
    let mut hist = [0u32; 256];
    let mut sum: u64 = 0;
    for p in gray.pixels() {
        let v = p[0];
        hist[v as usize] += 1;
        sum += v as u64;
    }
    let total = (gray.width() as u64 * gray.height() as u64).max(1);
    let mean = (sum as f64 / total as f64) as f32;

    let lo_target = (total as f64 * 0.02) as u64;
    let hi_target = (total as f64 * 0.98) as u64;
    let mut acc = 0u64;
    let mut lo = 0u8;
    let mut hi = 255u8;
    for (i, &c) in hist.iter().enumerate() {
        acc += c as u64;
        if acc >= lo_target {
            lo = i as u8;
            break;
        }
    }
    acc = 0;
    for (i, &c) in hist.iter().enumerate() {
        acc += c as u64;
        if acc >= hi_target {
            hi = i as u8;
            break;
        }
    }
    let span = (hi.saturating_sub(lo)).max(1) as f32;

    let mut normal = GrayImage::new(gray.width(), gray.height());
    let mut inverted = GrayImage::new(gray.width(), gray.height());
    for (x, y, p) in gray.enumerate_pixels() {
        let v = (((p[0].saturating_sub(lo)) as f32 / span) * 255.0).clamp(0.0, 255.0) as u8;
        normal.put_pixel(x, y, Luma([v]));
        inverted.put_pixel(x, y, Luma([255u8.saturating_sub(v)]));
    }
    (normal, inverted, mean)
}

fn build_tesseract_cmd(
    img_path: &Path,
    lang: &str,
    tessdata_dir: Option<&Path>,
    tsv: bool,
    single_region: bool,
) -> Command {
    let mut cmd = Command::new("tesseract");
    cmd.arg(img_path).arg("stdout").arg("-l").arg(lang);
    if let Some(dir) = tessdata_dir {
        let candidate = dir.join(format!("{lang}.traineddata"));
        if candidate.is_file() {
            cmd.arg("--tessdata-dir").arg(dir);
        }
    }
    if single_region {
        let psm = if lang == "jpn_vert" { "5" } else { "6" };
        cmd.arg("--psm").arg(psm);
    }
    if tsv {
        cmd.arg("tsv");
    }
    cmd
}

fn ocr_image_file(
    img_path: &Path,
    lang: &str,
    tessdata_dir: Option<&Path>,
    single_region: bool,
) -> Result<String, (String, String)> {
    let output = build_tesseract_cmd(img_path, lang, tessdata_dir, false, single_region)
        .output()
        .map_err(|e| {
            (
                "tesseract_missing".to_owned(),
                format!("cannot run tesseract (install tesseract package): {e}"),
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr
            .lines()
            .last()
            .unwrap_or("tesseract OCR failed")
            .trim();
        return Err(("ocr_failed".to_owned(), detail.to_owned()));
    }
    let text = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(text)
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

fn resolve_tessdata_dir<'a>(req: &'a Request) -> Option<PathBuf> {
    if !req.model_path.is_empty() {
        if let Some(parent) = Path::new(&req.model_path).parent() {
            return Some(parent.to_path_buf());
        }
    }
    req.params
        .get("modelsDir")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| Some(default_models_dir()))
}

fn run_ocr(req: &Request) -> Result<Value, (String, String)> {
    let lang = req
        .params
        .get("lang")
        .and_then(Value::as_str)
        .unwrap_or("eng");
    let return_translation = req
        .params
        .get("returnTranslation")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let tessdata_dir = resolve_tessdata_dir(req);
    let input_path = Path::new(&req.input);

    // Case 1: Interactive region crop (`params.region = { x, y, width, height }`)
    if let Some(region) = req.params.get("region").and_then(Value::as_object) {
        let rx = region.get("x").and_then(Value::as_i64).unwrap_or(0) as i32;
        let ry = region.get("y").and_then(Value::as_i64).unwrap_or(0) as i32;
        let rw = region.get("width").and_then(Value::as_i64).unwrap_or(0) as i32;
        let rh = region.get("height").and_then(Value::as_i64).unwrap_or(0) as i32;

        emit_progress(&req.id, 20, "Preprocessing selected region…");
        let img = image::open(input_path)
            .map_err(|e| ("io_error".to_owned(), format!("cannot open image: {e}")))?
            .into_rgba8();
        let cx = rx.clamp(0, img.width().saturating_sub(1) as i32) as u32;
        let cy = ry.clamp(0, img.height().saturating_sub(1) as i32) as u32;
        let cw = (rw.max(1) as u32).min(img.width().saturating_sub(cx));
        let ch = (rh.max(1) as u32).min(img.height().saturating_sub(cy));
        if cw == 0 || ch == 0 {
            return Err((
                "invalid_region".to_owned(),
                "empty selection region".to_owned(),
            ));
        }

        let cropped = imageops::crop_imm(&img, cx, cy, cw, ch).to_image();
        let (normal, inverted, mean_luma) = preprocess_region(&cropped);

        let pid = std::process::id();
        let tmp_a = std::env::temp_dir().join(format!("photon-ocr-a-{pid}.png"));
        let tmp_b = std::env::temp_dir().join(format!("photon-ocr-b-{pid}.png"));
        let (first, second) = if mean_luma < 110.0 {
            (&inverted, &normal)
        } else {
            (&normal, &inverted)
        };
        first
            .save(&tmp_a)
            .map_err(|e| ("io_error".to_owned(), format!("cannot write crop PNG: {e}")))?;

        emit_progress(&req.id, 55, &format!("Running OCR ({lang})…"));
        let mut text = ocr_image_file(&tmp_a, lang, tessdata_dir.as_deref(), true)?;
        let _ = fs::remove_file(&tmp_a);

        // Retry with opposite polarity if dark/complex background yielded empty text.
        if text.trim().is_empty() {
            if second.save(&tmp_b).is_ok() {
                if let Ok(alt) = ocr_image_file(&tmp_b, lang, tessdata_dir.as_deref(), true) {
                    text = alt;
                }
                let _ = fs::remove_file(&tmp_b);
            }
        }

        if text.trim().is_empty() {
            return Err((
                "ocr_empty".to_owned(),
                "No text detected in selected region".to_owned(),
            ));
        }

        if return_translation {
            let block = TranslationBlock {
                x: cx as i32,
                y: cy as i32,
                w: cw as i32,
                h: ch as i32,
                original: text.clone(),
                translated: String::new(),
            };
            return Ok(json!({
                "device": "cpu",
                "lang": lang,
                "translation": {
                    "path": req.input,
                    "sourceLang": lang,
                    "targetLang": "",
                    "originalText": text,
                    "translatedText": "",
                    "blocks": [block],
                }
            }));
        }

        return Ok(json!({
            "device": "cpu",
            "lang": lang,
            "clipboardText": text,
        }));
    }

    // Case 2: Full-page overlay OCR (`returnTranslation == true`, no region)
    if return_translation {
        emit_progress(&req.id, 30, &format!("Scanning page ({lang})…"));
        let tsv_out = build_tesseract_cmd(input_path, lang, tessdata_dir.as_deref(), true, false)
            .output()
            .map_err(|e| {
                (
                    "tesseract_missing".to_owned(),
                    format!("cannot run tesseract: {e}"),
                )
            })?;
        if !tsv_out.status.success() {
            let err = String::from_utf8_lossy(&tsv_out.stderr);
            return Err((
                "ocr_failed".to_owned(),
                err.lines().last().unwrap_or("OCR failed").trim().to_owned(),
            ));
        }

        let tsv = String::from_utf8_lossy(&tsv_out.stdout);
        let mut groups: BTreeMap<(i32, i32), (i32, i32, i32, i32, Vec<String>)> = BTreeMap::new();
        for line in tsv.lines().skip(1) {
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() < 12 {
                continue;
            }
            let level: i32 = cols[0].parse().unwrap_or(0);
            if level != 5 {
                continue;
            }
            let conf: f32 = cols[10].parse().unwrap_or(-1.0);
            let word = cols[11].trim();
            if conf < 20.0 || word.is_empty() {
                continue;
            }
            let block_num: i32 = cols[2].parse().unwrap_or(0);
            let par_num: i32 = cols[3].parse().unwrap_or(0);
            let left: i32 = cols[6].parse().unwrap_or(0);
            let top: i32 = cols[7].parse().unwrap_or(0);
            let width: i32 = cols[8].parse().unwrap_or(0);
            let height: i32 = cols[9].parse().unwrap_or(0);
            let entry = groups
                .entry((block_num, par_num))
                .or_insert((left, top, left + width, top + height, Vec::new()));
            entry.0 = entry.0.min(left);
            entry.1 = entry.1.min(top);
            entry.2 = entry.2.max(left + width);
            entry.3 = entry.3.max(top + height);
            entry.4.push(word.to_owned());
        }

        let mut blocks = Vec::new();
        let mut all_orig = Vec::new();
        for (_, (x1, y1, x2, y2, words)) in groups {
            let orig = words.join(" ");
            if orig.trim().is_empty() {
                continue;
            }
            all_orig.push(orig.clone());
            blocks.push(TranslationBlock {
                x: x1,
                y: y1,
                w: (x2 - x1).max(1),
                h: (y2 - y1).max(1),
                original: orig,
                translated: String::new(),
            });
        }

        if blocks.is_empty() {
            return Err((
                "ocr_empty".to_owned(),
                "No text detected on this page".to_owned(),
            ));
        }

        return Ok(json!({
            "device": "cpu",
            "lang": lang,
            "translation": {
                "path": req.input,
                "sourceLang": lang,
                "targetLang": "",
                "originalText": all_orig.join("\n"),
                "translatedText": "",
                "blocks": blocks,
            }
        }));
    }

    // Case 3: Standard Quick Action full-image OCR -> clipboardText
    emit_progress(&req.id, 30, &format!("Running OCR ({lang})…"));
    let text = ocr_image_file(input_path, lang, tessdata_dir.as_deref(), false)?;
    if text.trim().is_empty() {
        return Err((
            "ocr_empty".to_owned(),
            "no text detected in image".to_owned(),
        ));
    }
    Ok(json!({
        "device": "cpu",
        "lang": lang,
        "clipboardText": text,
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
                emit_error(
                    "",
                    "invalid_request",
                    &format!("invalid JSON request: {err}"),
                );
                continue;
            }
        };
        match req.op.as_str() {
            "shutdown" => break,
            "cancel" => {}
            "run" => {
                if req.action != "viewer.ocr_copy" {
                    emit_error(
                        &req.id,
                        "unsupported_action",
                        &format!("unsupported action '{}'", req.action),
                    );
                    continue;
                }
                match run_ocr(&req) {
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
