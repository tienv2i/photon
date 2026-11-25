//! Photon Read & Translate Plugin (`jsonl-stdio`, ADR 0024).
//!
//! Supports both single-bubble region crops (`params.region = {x, y, width, height}`)
//! and full-page manga/document translation (grouping Tesseract TSV blocks into
//! speech-bubble bounding boxes), then translating via Google Translate.

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

fn google_sl_for(tess_lang: &str) -> &'static str {
    match tess_lang {
        "eng" => "en",
        "jpn" | "jpn_vert" => "ja",
        "chi_sim" => "zh-CN",
        "chi_tra" => "zh-TW",
        "kor" => "ko",
        "vie" => "vi",
        _ => "auto",
    }
}

fn google_translate(text: &str, sl: &str, tl: &str) -> Result<String, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let output = Command::new("curl")
        .arg("-fsSL")
        .arg("--connect-timeout")
        .arg("6")
        .arg("--max-time")
        .arg("15")
        .arg("--retry")
        .arg("2")
        .arg("--retry-delay")
        .arg("1")
        .arg("--get")
        .arg("--data-urlencode")
        .arg("client=gtx")
        .arg("--data-urlencode")
        .arg(format!("sl={sl}"))
        .arg("--data-urlencode")
        .arg(format!("tl={tl}"))
        .arg("--data-urlencode")
        .arg("dt=t")
        .arg("--data-urlencode")
        .arg(format!("q={trimmed}"))
        .arg("https://translate.googleapis.com/translate_a/single")
        .output()
        .map_err(|e| format!("cannot run curl for Google Translate: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Google Translate request failed: {}", err.trim()));
    }
    let body = String::from_utf8_lossy(&output.stdout);
    let parsed: Value =
        serde_json::from_str(&body).map_err(|e| format!("invalid Google Translate response: {e}"))?;

    let mut translated = String::new();
    if let Some(sentences) = parsed.get(0).and_then(Value::as_array) {
        for seg in sentences {
            if let Some(part) = seg.get(0).and_then(Value::as_str) {
                translated.push_str(part);
            }
        }
    }
    Ok(translated.trim().to_owned())
}

/// Tier-1 preprocessing for manga / complex backgrounds:
/// - Upscale small crops 2x with Lanczos3 so small Kanji/Kana/Hangul stay crisp
/// - Convert to grayscale and stretch contrast between the 2nd and 98th percentiles
/// - Return `(normal, inverted, mean_luma)` for automatic dark-background polarity retry.
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

fn run_tesseract_args(
    img_path: &Path,
    lang: &str,
    models_dir: &str,
    tsv: bool,
    single_region: bool,
) -> Command {
    let mut cmd = Command::new("tesseract");
    cmd.arg(img_path).arg("stdout").arg("-l").arg(lang);
    if !models_dir.is_empty() {
        let candidate = Path::new(models_dir).join(format!("{lang}.traineddata"));
        if candidate.is_file() {
            cmd.arg("--tessdata-dir").arg(models_dir);
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

fn ocr_crop_file(img_path: &Path, lang: &str, models_dir: &str) -> Result<String, (String, String)> {
    let ocr_out = run_tesseract_args(img_path, lang, models_dir, false, true)
        .output()
        .map_err(|e| {
            (
                "tesseract_missing".to_owned(),
                format!("cannot run tesseract: {e}"),
            )
        })?;
    if !ocr_out.status.success() {
        let err = String::from_utf8_lossy(&ocr_out.stderr);
        return Err((
            "ocr_failed".to_owned(),
            err.lines().last().unwrap_or("OCR failed").trim().to_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&ocr_out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" "))
}

fn default_models_dir() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("PHOTON_MODELS_DIR").filter(|s| !s.is_empty()) {
        return std::path::PathBuf::from(dir);
    }
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_else(|| ".".into());
            std::path::PathBuf::from(home).join(".local/share")
        });
    for sub in ["photon/models", "photon-dev/models", "kogaview/models"] {
        let candidate = data_home.join(sub);
        if candidate.is_dir() {
            return candidate;
        }
    }
    data_home.join("photon/models")
}

fn run_translate(req: &Request) -> Result<Value, (String, String)> {
    let src_lang = req
        .params
        .get("sourceLang")
        .and_then(Value::as_str)
        .unwrap_or("eng");
    let target_lang = req
        .params
        .get("targetLang")
        .and_then(Value::as_str)
        .unwrap_or("vi");
    let models_dir_buf = req
        .params
        .get("modelsDir")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(default_models_dir);
    let models_dir = models_dir_buf.to_string_lossy();
    let models_dir = models_dir.as_ref();
    let sl = google_sl_for(src_lang);

    let input_path = Path::new(&req.input);

    // Case 1: User dragged a specific speech-bubble region `[x, y, width, height]`.
    if let Some(region) = req.params.get("region").and_then(Value::as_object) {
        let rx = region.get("x").and_then(Value::as_i64).unwrap_or(0) as i32;
        let ry = region.get("y").and_then(Value::as_i64).unwrap_or(0) as i32;
        let rw = region.get("width").and_then(Value::as_i64).unwrap_or(0) as i32;
        let rh = region.get("height").and_then(Value::as_i64).unwrap_or(0) as i32;

        emit_progress(&req.id, 20, "Cropping selected region…");
        let img = image::open(input_path)
            .map_err(|e| ("io_error".to_owned(), format!("cannot open image: {e}")))?
            .into_rgba8();
        let cx = rx.clamp(0, img.width().saturating_sub(1) as i32) as u32;
        let cy = ry.clamp(0, img.height().saturating_sub(1) as i32) as u32;
        let cw = (rw.max(1) as u32).min(img.width().saturating_sub(cx));
        let ch = (rh.max(1) as u32).min(img.height().saturating_sub(cy));
        if cw == 0 || ch == 0 {
            return Err(("invalid_region".to_owned(), "empty selection region".to_owned()));
        }

        let cropped = imageops::crop_imm(&img, cx, cy, cw, ch).to_image();
        let (normal, inverted, mean_luma) = preprocess_region(&cropped);
        let pid = std::process::id();
        let tmp_a: PathBuf = std::env::temp_dir().join(format!("photon-translate-a-{pid}.png"));
        let tmp_b: PathBuf = std::env::temp_dir().join(format!("photon-translate-b-{pid}.png"));
        let (first, second) = if mean_luma < 110.0 {
            (&inverted, &normal)
        } else {
            (&normal, &inverted)
        };
        first
            .save(&tmp_a)
            .map_err(|e| ("io_error".to_owned(), format!("cannot write crop PNG: {e}")))?;

        emit_progress(&req.id, 45, &format!("Running OCR ({src_lang})…"));
        let mut original = ocr_crop_file(&tmp_a, src_lang, models_dir)?;
        let _ = fs::remove_file(&tmp_a);

        // Retry with opposite polarity if dark/complex manga background yielded empty text.
        if original.trim().is_empty() {
            if second.save(&tmp_b).is_ok() {
                if let Ok(alt) = ocr_crop_file(&tmp_b, src_lang, models_dir) {
                    original = alt;
                }
                let _ = fs::remove_file(&tmp_b);
            }
        }

        if original.is_empty() {
            return Err((
                "ocr_empty".to_owned(),
                "No text detected in selected region".to_owned(),
            ));
        }

        emit_progress(&req.id, 75, "Translating with Google Translate…");
        let translated = google_translate(&original, sl, target_lang)
            .map_err(|e| ("translate_failed".to_owned(), e))?;

        let block = TranslationBlock {
            x: cx as i32,
            y: cy as i32,
            w: cw as i32,
            h: ch as i32,
            original: original.clone(),
            translated: translated.clone(),
        };
        return Ok(json!({
            "device": "cpu",
            "translation": {
                "path": req.input,
                "sourceLang": src_lang,
                "targetLang": target_lang,
                "originalText": original,
                "translatedText": translated,
                "blocks": [block],
            }
        }));
    }

    // Case 2: Full-page translation — group Tesseract TSV word boxes by (block_num, par_num).
    emit_progress(&req.id, 30, &format!("Scanning full page ({src_lang})…"));
    let tsv_out = run_tesseract_args(input_path, src_lang, models_dir, true, false)
        .output()
        .map_err(|e| ("tesseract_missing".to_owned(), format!("cannot run tesseract: {e}")))?;
    if !tsv_out.status.success() {
        let err = String::from_utf8_lossy(&tsv_out.stderr);
        return Err((
            "ocr_failed".to_owned(),
            err.lines().last().unwrap_or("OCR failed").trim().to_owned(),
        ));
    }

    let tsv = String::from_utf8_lossy(&tsv_out.stdout);
    // Group words into paragraph blocks: key = (block_num, par_num) -> (min_x, min_y, max_x, max_y, words)
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
        if word.is_empty() || conf < 25.0 {
            continue;
        }
        let block_num: i32 = cols[2].parse().unwrap_or(0);
        let par_num: i32 = cols[3].parse().unwrap_or(0);
        let x: i32 = cols[6].parse().unwrap_or(0);
        let y: i32 = cols[7].parse().unwrap_or(0);
        let w: i32 = cols[8].parse().unwrap_or(0);
        let h: i32 = cols[9].parse().unwrap_or(0);

        let entry = groups
            .entry((block_num, par_num))
            .or_insert((x, y, x + w, y + h, Vec::new()));
        entry.0 = entry.0.min(x);
        entry.1 = entry.1.min(y);
        entry.2 = entry.2.max(x + w);
        entry.3 = entry.3.max(y + h);
        entry.4.push(word.to_owned());
    }

    if groups.is_empty() {
        return Err((
            "ocr_empty".to_owned(),
            "No text blocks detected on page".to_owned(),
        ));
    }

    let total = groups.len().max(1);
    let mut blocks = Vec::new();
    let mut all_orig = Vec::new();
    let mut all_trans = Vec::new();

    for (idx, (_, (x1, y1, x2, y2, words))) in groups.into_iter().enumerate() {
        let orig = words.join(" ");
        let pct = 45 + (((idx + 1) as f32 / total as f32) * 50.0) as u8;
        emit_progress(
            &req.id,
            pct.min(95),
            &format!("Translating block {}/{total}…", idx + 1),
        );
        let trans = google_translate(&orig, sl, target_lang)
            .map_err(|e| ("translate_failed".to_owned(), e))?;
        all_orig.push(orig.clone());
        all_trans.push(trans.clone());
        blocks.push(TranslationBlock {
            x: x1,
            y: y1,
            w: (x2 - x1).max(16),
            h: (y2 - y1).max(16),
            original: orig,
            translated: trans,
        });
    }

    Ok(json!({
        "device": "cpu",
        "translation": {
            "path": req.input,
            "sourceLang": src_lang,
            "targetLang": target_lang,
            "originalText": all_orig.join("\n"),
            "translatedText": all_trans.join("\n"),
            "blocks": blocks,
        }
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
            "run" => {
                if req.action != "viewer.translate" {
                    emit_error(
                        &req.id,
                        "unsupported_action",
                        &format!("unsupported action '{}'", req.action),
                    );
                    continue;
                }
                match run_translate(&req) {
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
