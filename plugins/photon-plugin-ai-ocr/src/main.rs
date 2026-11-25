//! Photon AI OCR ONNX Plugin (`jsonl-stdio`, ADR 0024).
//!
//! Runs two specialized ONNX OCR engines out-of-process with zero startup cost:
//! - `manga_ocr`: ViT encoder + BERT decoder (`l0wgear/manga-ocr-2025-onnx`)
//!   purpose-built for Japanese manga (vertical/horizontal, furigana, screentones,
//!   complex artwork backgrounds).
//! - `rapidocr`: PP-OCRv4 DBNet detector + SVTR/CTC recognizer (`SWHL/RapidOCR`)
//!   purpose-built for text overlaid on complex colored backgrounds, manhwa/manhua,
//!   and full-page multi-box detection.
//!
//! When `targetLang` is non-empty, translates recognized text via Google Translate
//! (`curl`); when `targetLang` is empty (`OCR only`), runs 100% offline.

use std::collections::VecDeque;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use image::imageops::{self, FilterType};
use image::RgbaImage;
use ndarray::{Array2, Array3, Array4};
use ort::environment::Environment;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
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

static ORT_ENV: OnceLock<Result<Environment, String>> = OnceLock::new();

fn ort_env() -> Result<&'static Environment, String> {
    let res = ORT_ENV.get_or_init(|| {
        ort::init()
            .with_name("photon-plugin-ai-ocr")
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

fn google_sl_for(source_lang: &str, engine: &str) -> &'static str {
    if engine == "manga_ocr" {
        return "ja";
    }
    match source_lang {
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
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|e| format!("invalid Google Translate response: {e}"))?;

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

// ============================================================================
// Engine 1: Manga-OCR (ViT Encoder + BERT Decoder ONNX, Japanese Manga)
// ============================================================================

fn run_manga_ocr_on_crop(
    req_id: &str,
    crop: &RgbaImage,
    models_dir: &Path,
) -> Result<String, (String, String)> {
    let enc_path = models_dir.join("manga-ocr-encoder.onnx");
    let dec_path = models_dir.join("manga-ocr-decoder.onnx");
    let vocab_path = models_dir.join("manga-ocr-vocab.txt");

    let mut missing = Vec::new();
    if !enc_path.is_file() {
        missing.push("Manga-OCR — Vision Encoder");
    }
    if !dec_path.is_file() {
        missing.push("Manga-OCR — Text Decoder");
    }
    if !vocab_path.is_file() {
        missing.push("Manga-OCR — Vocabulary");
    }
    if !missing.is_empty() {
        return Err((
            "model_missing".to_owned(),
            format!(
                "Download required Manga-OCR model(s) in Settings → Plugins → OCR languages: {}",
                missing.join(", ")
            ),
        ));
    }

    let vocab_raw = fs::read_to_string(&vocab_path)
        .map_err(|e| ("io_error".to_owned(), format!("cannot read vocab.txt: {e}")))?;
    let vocab: Vec<&str> = vocab_raw.lines().collect();

    emit_progress(req_id, 25, "Loading Manga-OCR encoder…");
    let mut encoder =
        build_session(&enc_path).map_err(|e| ("ort_error".to_owned(), e))?;

    // Manga-OCR preprocessing: grayscale -> RGB, resize 224x224, normalize (x/255 - 0.5) / 0.5
    let gray = imageops::grayscale(crop);
    let resized = imageops::resize(&gray, 224, 224, FilterType::Triangle);
    let mut pixel_values = Array4::<f32>::zeros((1, 3, 224, 224));
    for y in 0..224usize {
        for x in 0..224usize {
            let v = (resized.get_pixel(x as u32, y as u32)[0] as f32 / 255.0 - 0.5) / 0.5;
            pixel_values[[0, 0, y, x]] = v;
            pixel_values[[0, 1, y, x]] = v;
            pixel_values[[0, 2, y, x]] = v;
        }
    }

    let enc_input_name = encoder
        .inputs()
        .first()
        .map(|i| i.name().to_owned())
        .unwrap_or_else(|| "pixel_values".to_owned());
    let enc_tensor = Tensor::from_array(pixel_values)
        .map_err(|e| ("ort_error".to_owned(), format!("encoder input failed: {e}")))?;

    emit_progress(req_id, 45, "Encoding manga bubble…");
    let enc_outputs = encoder
        .run(ort::inputs![enc_input_name.as_str() => enc_tensor])
        .map_err(|e| ("ort_error".to_owned(), format!("encoder inference failed: {e}")))?;

    let (enc_shape, enc_data) = enc_outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| ("ort_error".to_owned(), format!("encoder output extract failed: {e}")))?;
    if enc_shape.len() != 3 {
        return Err((
            "ort_error".to_owned(),
            format!("unexpected encoder hidden state rank: {:?}", enc_shape),
        ));
    }
    let hidden_seq = enc_shape[1] as usize;
    let hidden_dim = enc_shape[2] as usize;
    let hidden_arr = Array3::from_shape_vec((1, hidden_seq, hidden_dim), enc_data.to_vec())
        .map_err(|e| ("ort_error".to_owned(), format!("hidden reshape failed: {e}")))?;

    emit_progress(req_id, 60, "Decoding Japanese manga text…");
    let mut decoder =
        build_session(&dec_path).map_err(|e| ("ort_error".to_owned(), e))?;

    let dec_input_names: Vec<String> = decoder
        .inputs()
        .iter()
        .map(|i| i.name().to_owned())
        .collect();
    let ids_name = dec_input_names
        .iter()
        .find(|n| n.contains("input_ids"))
        .cloned()
        .unwrap_or_else(|| "input_ids".to_owned());
    let hidden_name = dec_input_names
        .iter()
        .find(|n| n.contains("encoder_hidden_states"))
        .cloned()
        .unwrap_or_else(|| "encoder_hidden_states".to_owned());
    let attn_mask_name = dec_input_names
        .iter()
        .find(|n| n.contains("encoder_attention_mask"))
        .cloned();

    // Greedy autoregressive decoding (`decoder_start_token_id = 2`, `eos_token_id = 3`)
    let mut token_ids: Vec<i64> = vec![2];
    const EOS_ID: i64 = 3;
    const MAX_TOKENS: usize = 128;

    for _step in 0..MAX_TOKENS {
        let seq_len = token_ids.len();
        let ids_arr = Array2::from_shape_vec((1, seq_len), token_ids.clone())
            .map_err(|e| ("ort_error".to_owned(), format!("ids shape failed: {e}")))?;
        let ids_tensor = Tensor::from_array(ids_arr)
            .map_err(|e| ("ort_error".to_owned(), format!("ids tensor failed: {e}")))?;
        let hidden_tensor = Tensor::from_array(hidden_arr.clone())
            .map_err(|e| ("ort_error".to_owned(), format!("hidden tensor failed: {e}")))?;

        let dec_outputs = if let Some(mask_name) = &attn_mask_name {
            let mask_arr = Array2::<i64>::ones((1, hidden_seq));
            let mask_tensor = Tensor::from_array(mask_arr)
                .map_err(|e| ("ort_error".to_owned(), format!("mask tensor failed: {e}")))?;
            decoder.run(ort::inputs![
                ids_name.as_str() => ids_tensor,
                hidden_name.as_str() => hidden_tensor,
                mask_name.as_str() => mask_tensor,
            ])
        } else {
            decoder.run(ort::inputs![
                ids_name.as_str() => ids_tensor,
                hidden_name.as_str() => hidden_tensor,
            ])
        }
        .map_err(|e| ("ort_error".to_owned(), format!("decoder step failed: {e}")))?;

        let (logits_shape, logits) = dec_outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| ("ort_error".to_owned(), format!("logits extract failed: {e}")))?;
        if logits_shape.len() != 3 {
            return Err((
                "ort_error".to_owned(),
                format!("unexpected logits rank: {:?}", logits_shape),
            ));
        }
        let vocab_size = logits_shape[2] as usize;
        let last_offset = (seq_len - 1) * vocab_size;
        let slice = &logits[last_offset..last_offset + vocab_size];

        let mut best_id = 0usize;
        let mut best_val = f32::NEG_INFINITY;
        for (idx, &val) in slice.iter().enumerate() {
            if val > best_val {
                best_val = val;
                best_id = idx;
            }
        }
        let next_id = best_id as i64;
        if next_id == EOS_ID {
            break;
        }
        token_ids.push(next_id);
    }

    let mut decoded = String::new();
    for &id in &token_ids {
        if id <= 4 {
            continue;
        }
        if let Some(&tok) = vocab.get(id as usize) {
            let clean = tok.strip_prefix("##").unwrap_or(tok);
            decoded.push_str(clean);
        }
    }
    Ok(decoded.trim().to_owned())
}

// ============================================================================
// Engine 2: RapidOCR (PP-OCRv4 Detector + Recognizer ONNX, Complex Backgrounds)
// ============================================================================

fn rapidocr_detect_boxes(
    det_session: &mut Session,
    img: &RgbaImage,
) -> Result<Vec<(u32, u32, u32, u32)>, String> {
    let orig_w = img.width();
    let orig_h = img.height();
    let max_side = 960.0f32;
    let ratio = (max_side / orig_w.max(orig_h) as f32).min(1.0);
    let mut new_w = ((orig_w as f32 * ratio).round() as u32).max(32);
    let mut new_h = ((orig_h as f32 * ratio).round() as u32).max(32);
    new_w = ((new_w + 31) / 32) * 32;
    new_h = ((new_h + 31) / 32) * 32;

    let resized = imageops::resize(img, new_w, new_h, FilterType::Triangle);
    let mean = [0.485f32, 0.456, 0.406];
    let std = [0.229f32, 0.224, 0.225];
    let mut input = Array4::<f32>::zeros((1, 3, new_h as usize, new_w as usize));
    for y in 0..new_h as usize {
        for x in 0..new_w as usize {
            let px = resized.get_pixel(x as u32, y as u32);
            input[[0, 0, y, x]] = (px[0] as f32 / 255.0 - mean[0]) / std[0];
            input[[0, 1, y, x]] = (px[1] as f32 / 255.0 - mean[1]) / std[1];
            input[[0, 2, y, x]] = (px[2] as f32 / 255.0 - mean[2]) / std[2];
        }
    }

    let in_name = det_session
        .inputs()
        .first()
        .map(|i| i.name().to_owned())
        .unwrap_or_else(|| "x".to_owned());
    let tensor = Tensor::from_array(input).map_err(|e| format!("det input failed: {e}"))?;
    let outputs = det_session
        .run(ort::inputs![in_name.as_str() => tensor])
        .map_err(|e| format!("det inference failed: {e}"))?;
    let (shape, probs) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("det output extract failed: {e}"))?;
    if shape.len() != 4 {
        return Err(format!("unexpected det output shape: {:?}", shape));
    }
    let mh = shape[2] as usize;
    let mw = shape[3] as usize;

    // Connected-component labeling on prob >= 0.30
    let mut visited = vec![false; mh * mw];
    let mut boxes = Vec::new();
    let sx = orig_w as f32 / mw as f32;
    let sy = orig_h as f32 / mh as f32;

    for y in 0..mh {
        for x in 0..mw {
            let idx = y * mw + x;
            if visited[idx] || probs[idx] < 0.30 {
                continue;
            }
            let mut q = VecDeque::new();
            q.push_back((x, y));
            visited[idx] = true;
            let (mut min_x, mut min_y, mut max_x, mut max_y) = (x, y, x, y);
            let mut area = 0usize;
            while let Some((cx, cy)) = q.pop_front() {
                area += 1;
                min_x = min_x.min(cx);
                min_y = min_y.min(cy);
                max_x = max_x.max(cx);
                max_y = max_y.max(cy);
                for (nx, ny) in [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ] {
                    if nx < mw && ny < mh {
                        let nidx = ny * mw + nx;
                        if !visited[nidx] && probs[nidx] >= 0.30 {
                            visited[nidx] = true;
                            q.push_back((nx, ny));
                        }
                    }
                }
            }
            if area < 12 {
                continue;
            }
            // DBNet-style slight expansion (unclip) so edge strokes aren't clipped
            let bw = (max_x - min_x + 1) as f32;
            let bh = (max_y - min_y + 1) as f32;
            let pad_x = (bh * 0.25).max(3.0);
            let pad_y = (bh * 0.25).max(3.0);
            let ox1 = (((min_x as f32 - pad_x).max(0.0)) * sx) as u32;
            let oy1 = (((min_y as f32 - pad_y).max(0.0)) * sy) as u32;
            let ox2 = ((((max_x as f32 + 1.0 + pad_x).min(mw as f32)) * sx) as u32).min(orig_w);
            let oy2 = ((((max_y as f32 + 1.0 + pad_y).min(mh as f32)) * sy) as u32).min(orig_h);
            let w = ox2.saturating_sub(ox1);
            let h = oy2.saturating_sub(oy1);
            if w >= 6 && h >= 6 && bw >= 3.0 && bh >= 3.0 {
                boxes.push((ox1, oy1, w, h));
            }
        }
    }

    // Sort top-to-bottom (grouped into rough rows), then left-to-right
    boxes.sort_by_key(|&(x, y, _, _)| (y / 24, x));
    Ok(boxes)
}

fn rapidocr_recognize_crop(
    rec_session: &mut Session,
    crop: &RgbaImage,
    keys: &[String],
) -> Result<String, String> {
    let target_h = 48u32;
    let aspect = crop.width() as f32 / crop.height().max(1) as f32;
    let target_w = ((target_h as f32 * aspect).round() as u32).clamp(16, 640);
    let padded_w = ((target_w + 3) / 4) * 4;

    let resized = imageops::resize(crop, target_w, target_h, FilterType::Triangle);
    let mut input = Array4::<f32>::zeros((1, 3, target_h as usize, padded_w as usize));
    for y in 0..target_h as usize {
        for x in 0..target_w as usize {
            let px = resized.get_pixel(x as u32, y as u32);
            // PP-OCRv4 rec expects BGR normalized to [-1, 1]
            input[[0, 0, y, x]] = (px[2] as f32 / 255.0 - 0.5) / 0.5;
            input[[0, 1, y, x]] = (px[1] as f32 / 255.0 - 0.5) / 0.5;
            input[[0, 2, y, x]] = (px[0] as f32 / 255.0 - 0.5) / 0.5;
        }
    }

    let in_name = rec_session
        .inputs()
        .first()
        .map(|i| i.name().to_owned())
        .unwrap_or_else(|| "x".to_owned());
    let tensor = Tensor::from_array(input).map_err(|e| format!("rec input failed: {e}"))?;
    let outputs = rec_session
        .run(ort::inputs![in_name.as_str() => tensor])
        .map_err(|e| format!("rec inference failed: {e}"))?;
    let (shape, probs) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("rec output extract failed: {e}"))?;
    if shape.len() != 3 {
        return Err(format!("unexpected rec output shape: {:?}", shape));
    }
    let steps = shape[1] as usize;
    let classes = shape[2] as usize;

    // CTC greedy decode (0 = blank, 1..=keys.len() = keys[idx-1], last = space)
    let mut out = String::new();
    let mut prev_idx = 0usize;
    for t in 0..steps {
        let base = t * classes;
        let mut best_i = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        for c in 0..classes {
            let v = probs[base + c];
            if v > best_v {
                best_v = v;
                best_i = c;
            }
        }
        if best_i != 0 && best_i != prev_idx {
            if let Some(ch) = keys.get(best_i - 1) {
                out.push_str(ch);
            } else if best_i == keys.len() + 1 {
                out.push(' ');
            }
        }
        prev_idx = best_i;
    }
    Ok(out.trim().to_owned())
}

fn run_rapidocr(
    req_id: &str,
    img: &RgbaImage,
    offset_x: i32,
    offset_y: i32,
    is_region: bool,
    models_dir: &Path,
) -> Result<Vec<(i32, i32, i32, i32, String)>, (String, String)> {
    let det_path = models_dir.join("ch_PP-OCRv4_det_infer.onnx");
    let rec_path = models_dir.join("ch_PP-OCRv4_rec_infer.onnx");
    let keys_path = models_dir.join("ppocr_keys_v1.txt");

    let mut missing = Vec::new();
    if !det_path.is_file() {
        missing.push("RapidOCR — PP-OCRv4 Text Detector");
    }
    if !rec_path.is_file() {
        missing.push("RapidOCR — PP-OCRv4 Recognizer");
    }
    if !keys_path.is_file() {
        missing.push("RapidOCR — Character Dictionary");
    }
    if !missing.is_empty() {
        return Err((
            "model_missing".to_owned(),
            format!(
                "Download required RapidOCR model(s) in Settings → Plugins → OCR languages: {}",
                missing.join(", ")
            ),
        ));
    }

    let keys_raw = fs::read_to_string(&keys_path)
        .map_err(|e| ("io_error".to_owned(), format!("cannot read ppocr_keys_v1.txt: {e}")))?;
    let keys: Vec<String> = keys_raw.lines().map(|l| l.to_owned()).collect();

    emit_progress(req_id, 25, "Detecting text boxes (RapidOCR)…");
    let mut det_session =
        build_session(&det_path).map_err(|e| ("ort_error".to_owned(), e))?;
    let boxes = rapidocr_detect_boxes(&mut det_session, img)
        .map_err(|e| ("ort_error".to_owned(), e))?;
    drop(det_session);

    emit_progress(req_id, 55, "Recognizing text (RapidOCR)…");
    let mut rec_session =
        build_session(&rec_path).map_err(|e| ("ort_error".to_owned(), e))?;

    let mut results = Vec::new();
    if boxes.is_empty() && is_region {
        // Fallback: recognize the whole cropped region directly as a single line
        let text = rapidocr_recognize_crop(&mut rec_session, img, &keys)
            .map_err(|e| ("ort_error".to_owned(), e))?;
        if !text.is_empty() {
            results.push((
                offset_x,
                offset_y,
                img.width() as i32,
                img.height() as i32,
                text,
            ));
        }
    } else if is_region {
        // Inside a user-selected bubble, combine all detected lines into one bubble block
        let mut lines = Vec::new();
        for (bx, by, bw, bh) in boxes {
            let sub = imageops::crop_imm(img, bx, by, bw, bh).to_image();
            if let Ok(line) = rapidocr_recognize_crop(&mut rec_session, &sub, &keys) {
                if !line.is_empty() {
                    lines.push(line);
                }
            }
        }
        let joined = lines.join(" ");
        if !joined.is_empty() {
            results.push((
                offset_x,
                offset_y,
                img.width() as i32,
                img.height() as i32,
                joined,
            ));
        }
    } else {
        for (bx, by, bw, bh) in boxes {
            let sub = imageops::crop_imm(img, bx, by, bw, bh).to_image();
            if let Ok(line) = rapidocr_recognize_crop(&mut rec_session, &sub, &keys) {
                if !line.is_empty() {
                    results.push((
                        offset_x + bx as i32,
                        offset_y + by as i32,
                        bw as i32,
                        bh as i32,
                        line,
                    ));
                }
            }
        }
    }

    Ok(results)
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

fn handle_run(req: &Request) -> Result<Value, (String, String)> {
    let engine = req
        .params
        .get("engine")
        .and_then(Value::as_str)
        .unwrap_or("manga_ocr");
    let src_lang = req
        .params
        .get("sourceLang")
        .and_then(Value::as_str)
        .unwrap_or("jpn");
    let target_lang = req
        .params
        .get("targetLang")
        .and_then(Value::as_str)
        .unwrap_or("");
    let models_dir = req
        .params
        .get("modelsDir")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(default_models_dir);
    let models_dir = models_dir.as_path();

    emit_progress(&req.id, 10, "Loading image…");
    let full_img = image::open(&req.input)
        .map_err(|e| ("io_error".to_owned(), format!("cannot open image: {e}")))?
        .into_rgba8();

    let (work_img, ox, oy, is_region) =
        if let Some(region) = req.params.get("region").and_then(Value::as_object) {
            let rx = region.get("x").and_then(Value::as_i64).unwrap_or(0) as i32;
            let ry = region.get("y").and_then(Value::as_i64).unwrap_or(0) as i32;
            let rw = region.get("width").and_then(Value::as_i64).unwrap_or(0) as i32;
            let rh = region.get("height").and_then(Value::as_i64).unwrap_or(0) as i32;
            let cx = rx.clamp(0, full_img.width().saturating_sub(1) as i32) as u32;
            let cy = ry.clamp(0, full_img.height().saturating_sub(1) as i32) as u32;
            let cw = (rw.max(1) as u32).min(full_img.width().saturating_sub(cx));
            let ch = (rh.max(1) as u32).min(full_img.height().saturating_sub(cy));
            if cw == 0 || ch == 0 {
                return Err(("invalid_region".to_owned(), "empty selection region".to_owned()));
            }
            (
                imageops::crop_imm(&full_img, cx, cy, cw, ch).to_image(),
                cx as i32,
                cy as i32,
                true,
            )
        } else {
            (full_img, 0, 0, false)
        };

    let raw_blocks: Vec<(i32, i32, i32, i32, String)> = match engine {
        "rapidocr" => run_rapidocr(&req.id, &work_img, ox, oy, is_region, models_dir)?,
        _ => {
            let text = run_manga_ocr_on_crop(&req.id, &work_img, models_dir)?;
            if text.is_empty() {
                Vec::new()
            } else {
                vec![(ox, oy, work_img.width() as i32, work_img.height() as i32, text)]
            }
        }
    };

    if raw_blocks.is_empty() {
        return Err((
            "ocr_empty".to_owned(),
            "No text detected in selected region".to_owned(),
        ));
    }

    let do_translate = !target_lang.trim().is_empty();
    let sl = google_sl_for(src_lang, engine);
    if do_translate {
        emit_progress(&req.id, 80, "Translating with Google Translate…");
    }

    let mut blocks = Vec::new();
    let mut all_orig = Vec::new();
    let mut all_trans = Vec::new();
    for (x, y, w, h, orig) in raw_blocks {
        let trans = if do_translate {
            google_translate(&orig, sl, target_lang).unwrap_or_else(|_| orig.clone())
        } else {
            String::new()
        };
        all_orig.push(orig.clone());
        if !trans.is_empty() {
            all_trans.push(trans.clone());
        }
        blocks.push(TranslationBlock {
            x,
            y,
            w,
            h,
            original: orig,
            translated: trans,
        });
    }

    Ok(json!({
        "device": "cpu",
        "engine": engine,
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
                if req.action != "viewer.ai_ocr" {
                    emit_error(
                        &req.id,
                        "unsupported_action",
                        &format!("unsupported action '{}'", req.action),
                    );
                    continue;
                }
                match handle_run(&req) {
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
