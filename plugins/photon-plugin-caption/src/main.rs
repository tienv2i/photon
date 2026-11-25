//! Photon AI Captioner / WD14 v3 Tagger ONNX Plugin (`jsonl-stdio`, ADR 0024).
//!
//! Runs SmilingWolf WD-ViT v3 and WD-SwinV2 v3 ONNX models out-of-process:
//! - `caption.generate_save` (Quick Action): predicts tags and atomically writes
//!   `<image_stem>.txt` beside the image, returning `meta.clipboardText` + `meta.savedPath`.
//! - `editor.caption` (Editor Advanced): predicts tags with custom thresholds,
//!   prefix/suffix, exclude list, and underscore replacement, returning `meta.captionText`.

use std::collections::HashSet;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use image::imageops::{self, FilterType};
use image::{Rgb, RgbImage};
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
    model_path: String,
    #[serde(default)]
    input: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug)]
struct TagEntry {
    name: String,
    category: u8, // 0 = general, 4 = character, 9 = rating
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
            .with_name("photon-plugin-caption")
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

fn load_tags_csv(path: &Path) -> Result<Vec<TagEntry>, String> {
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("cannot read tags CSV '{}': {e}", path.display()))?;
    let mut tags = Vec::new();
    for line in raw.lines().skip(1) {
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 3 {
            continue;
        }
        let name = cols[1].trim().to_owned();
        let category = cols[2].trim().parse::<u8>().unwrap_or(0);
        tags.push(TagEntry { name, category });
    }
    Ok(tags)
}

/// SmilingWolf WD14 v3 preprocessing:
/// 1. Alpha-composite onto white RGB background
/// 2. Pad to square with white borders (preserving aspect ratio)
/// 3. Resize to `target x target` (448x448)
fn preprocess_wd_square(img_path: &Path, target: u32) -> Result<RgbImage, String> {
    let rgba = image::open(img_path)
        .map_err(|e| format!("cannot open image: {e}"))?
        .into_rgba8();
    let (w, h) = rgba.dimensions();
    let side = w.max(h);
    let mut square = RgbImage::from_pixel(side, side, Rgb([255, 255, 255]));
    let ox = (side - w) / 2;
    let oy = (side - h) / 2;
    for y in 0..h {
        for x in 0..w {
            let p = rgba.get_pixel(x, y);
            let a = p[3] as f32 / 255.0;
            let r = (p[0] as f32 * a + 255.0 * (1.0 - a)).round() as u8;
            let g = (p[1] as f32 * a + 255.0 * (1.0 - a)).round() as u8;
            let b = (p[2] as f32 * a + 255.0 * (1.0 - a)).round() as u8;
            square.put_pixel(ox + x, oy + y, Rgb([r, g, b]));
        }
    }
    Ok(imageops::resize(&square, target, target, FilterType::CatmullRom))
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

fn resolve_model_and_csv(req: &Request) -> Result<(PathBuf, PathBuf), (String, String)> {
    let models_dir = req
        .params
        .get("modelsDir")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_models_dir);
    let model_id = req
        .params
        .get("modelId")
        .and_then(Value::as_str)
        .unwrap_or("wd-vit-tagger-v3");

    let model_path = if !req.model_path.is_empty() && Path::new(&req.model_path).is_file() {
        PathBuf::from(&req.model_path)
    } else {
        let filename = match model_id {
            "wd-swinv2-tagger-v3" => "wd-swinv2-tagger-v3.onnx",
            _ => "wd-vit-tagger-v3.onnx",
        };
        models_dir.join(filename)
    };

    let csv_dir = model_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| models_dir.as_path());
    let csv_path = csv_dir.join("wd-v3-selected-tags.csv");

    let mut missing = Vec::new();
    if !model_path.is_file() {
        missing.push(model_id);
    }
    if !csv_path.is_file() {
        missing.push("WD v3 Tag Vocabulary (wd-v3-selected-tags.csv)");
    }
    if !missing.is_empty() {
        return Err((
            "model_missing".to_owned(),
            format!(
                "Download required Caption model(s) in Settings → Plugins: {}",
                missing.join(", ")
            ),
        ));
    }
    Ok((model_path, csv_path))
}

fn caption_txt_path(image_path: &Path) -> PathBuf {
    image_path.with_extension("txt")
}

fn run_caption(req: &Request, save_to_disk: bool) -> Result<Value, (String, String)> {
    let (model_path, csv_path) = resolve_model_and_csv(req)?;

    let gen_thresh = req
        .params
        .get("generalThreshold")
        .and_then(Value::as_f64)
        .unwrap_or(0.35)
        .clamp(0.05, 0.99) as f32;
    let char_thresh = req
        .params
        .get("characterThreshold")
        .and_then(Value::as_f64)
        .unwrap_or(0.85)
        .clamp(0.05, 0.99) as f32;
    let replace_underscores = req
        .params
        .get("replaceUnderscores")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let prefix = req
        .params
        .get("prefix")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let suffix = req
        .params
        .get("suffix")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let exclude_raw = req
        .params
        .get("excludeTags")
        .and_then(Value::as_str)
        .unwrap_or("");
    let exclude: HashSet<String> = exclude_raw
        .split(',')
        .map(|s| s.trim().to_lowercase().replace(' ', "_"))
        .filter(|s| !s.is_empty())
        .collect();

    emit_progress(&req.id, 15, "Loading tag vocabulary…");
    let tags = load_tags_csv(&csv_path).map_err(|e| ("io_error".to_owned(), e))?;

    emit_progress(&req.id, 30, "Loading WD14 v3 ONNX model…");
    let mut session = build_session(&model_path).map_err(|e| ("ort_error".to_owned(), e))?;

    let (in_name, is_nchw, target_size) = {
        let inp = session
            .inputs()
            .first()
            .ok_or_else(|| ("ort_error".to_owned(), "model has no inputs".to_owned()))?;
        let name = inp.name().to_owned();
        let shape = inp.dtype().tensor_shape();
        let (nchw, sz) = if let Some(dims) = shape {
            if dims.len() == 4 && dims[1] == 3 {
                (true, dims[2].max(224) as u32)
            } else if dims.len() == 4 && dims[3] == 3 {
                (false, dims[1].max(224) as u32)
            } else {
                (false, 448u32)
            }
        } else {
            (false, 448u32)
        };
        (name, nchw, sz)
    };

    emit_progress(&req.id, 50, "Preprocessing image…");
    let resized = preprocess_wd_square(Path::new(&req.input), target_size)
        .map_err(|e| ("io_error".to_owned(), e))?;
    let s = target_size as usize;

    let input_tensor = if is_nchw {
        let mut arr = Array4::<f32>::zeros((1, 3, s, s));
        for y in 0..s {
            for x in 0..s {
                let p = resized.get_pixel(x as u32, y as u32);
                arr[[0, 0, y, x]] = p[2] as f32; // BGR
                arr[[0, 1, y, x]] = p[1] as f32;
                arr[[0, 2, y, x]] = p[0] as f32;
            }
        }
        Tensor::from_array(arr)
    } else {
        let mut arr = Array4::<f32>::zeros((1, s, s, 3));
        for y in 0..s {
            for x in 0..s {
                let p = resized.get_pixel(x as u32, y as u32);
                arr[[0, y, x, 0]] = p[2] as f32; // BGR float32 [0..255]
                arr[[0, y, x, 1]] = p[1] as f32;
                arr[[0, y, x, 2]] = p[0] as f32;
            }
        }
        Tensor::from_array(arr)
    }
    .map_err(|e| ("ort_error".to_owned(), format!("tensor creation failed: {e}")))?;

    emit_progress(&req.id, 72, "Running AI caption inference…");
    let outputs = session
        .run(ort::inputs![in_name.as_str() => input_tensor])
        .map_err(|e| ("ort_error".to_owned(), format!("inference failed: {e}")))?;

    let (_shape, probs) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| ("ort_error".to_owned(), format!("output extract failed: {e}")))?;

    let mut char_tags: Vec<(String, f32)> = Vec::new();
    let mut gen_tags: Vec<(String, f32)> = Vec::new();

    for (idx, tag) in tags.iter().enumerate() {
        let Some(&score) = probs.get(idx) else { break };
        if exclude.contains(&tag.name.to_lowercase()) {
            continue;
        }
        match tag.category {
            4 if score >= char_thresh => char_tags.push((tag.name.clone(), score)),
            0 if score >= gen_thresh => gen_tags.push((tag.name.clone(), score)),
            _ => {}
        }
    }

    char_tags.sort_by(|a, b| b.1.total_cmp(&a.1));
    gen_tags.sort_by(|a, b| b.1.total_cmp(&a.1));

    let format_tag = |raw: &str| -> String {
        if replace_underscores && raw.len() > 3 {
            raw.replace('_', " ")
        } else {
            raw.to_owned()
        }
    };

    let mut parts: Vec<String> = Vec::new();
    if !prefix.is_empty() {
        parts.push(prefix.to_owned());
    }
    for (t, _) in &char_tags {
        parts.push(format_tag(t));
    }
    for (t, _) in &gen_tags {
        parts.push(format_tag(t));
    }
    if !suffix.is_empty() {
        parts.push(suffix.to_owned());
    }

    let caption = parts.join(", ");
    if caption.is_empty() {
        return Err((
            "caption_empty".to_owned(),
            "No tags exceeded the configured threshold — try lowering General threshold".to_owned(),
        ));
    }

    if save_to_disk {
        let txt_path = caption_txt_path(Path::new(&req.input));
        let tmp_path = txt_path.with_extension("txt.tmp");
        fs::write(&tmp_path, caption.as_bytes()).map_err(|e| {
            (
                "io_error".to_owned(),
                format!("cannot write '{}': {e}", tmp_path.display()),
            )
        })?;
        fs::rename(&tmp_path, &txt_path).map_err(|e| {
            let _ = fs::remove_file(&tmp_path);
            (
                "io_error".to_owned(),
                format!("cannot save '{}': {e}", txt_path.display()),
            )
        })?;
        return Ok(json!({
            "device": "cpu",
            "captionText": caption,
            "clipboardText": caption,
            "savedPath": txt_path.display().to_string(),
            "tagCount": char_tags.len() + gen_tags.len(),
        }));
    }

    Ok(json!({
        "device": "cpu",
        "captionText": caption,
        "tagCount": char_tags.len() + gen_tags.len(),
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
                let res = match req.action.as_str() {
                    "caption.generate_save" => run_caption(&req, true),
                    "editor.caption" => run_caption(&req, false),
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
