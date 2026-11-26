//! Viewer OCR (`tesseract` TSV) + translation (`GoogleWeb` / `LibreTranslate` via `curl`)
//! and sidecar JSON persistence under `<dir>/.photon/translation/<filename>.json`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use photon_protocol::{
    ProtocolError, SavedTranslation, TranslateProvider, TranslateSettings, TranslationBlock,
    TranslationResult, UiText,
};

/// Parse Tesseract TSV output into line-grouped [`TranslationBlock`]s in document coordinates.
pub fn parse_tesseract_tsv(
    tsv: &str,
    offset_x: i32,
    offset_y: i32,
) -> (String, Vec<TranslationBlock>) {
    #[derive(Default)]
    struct LineAccum {
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
        words: Vec<String>,
    }

    let mut lines: BTreeMap<(i32, i32, i32, i32), LineAccum> = BTreeMap::new();
    for (idx, row) in tsv.lines().enumerate() {
        if idx == 0 && row.starts_with("level\t") {
            continue;
        }
        let cols: Vec<&str> = row.split('\t').collect();
        if cols.len() < 12 {
            continue;
        }
        let Ok(level) = cols[0].trim().parse::<i32>() else {
            continue;
        };
        if level != 5 {
            continue;
        }
        let text = cols[11].trim();
        if text.is_empty() {
            continue;
        }
        let page = cols[1].trim().parse::<i32>().unwrap_or(0);
        let block = cols[2].trim().parse::<i32>().unwrap_or(0);
        let par = cols[3].trim().parse::<i32>().unwrap_or(0);
        let line = cols[4].trim().parse::<i32>().unwrap_or(0);
        let left = cols[6].trim().parse::<i32>().unwrap_or(0) + offset_x;
        let top = cols[7].trim().parse::<i32>().unwrap_or(0) + offset_y;
        let width = cols[8].trim().parse::<i32>().unwrap_or(0).max(1);
        let height = cols[9].trim().parse::<i32>().unwrap_or(0).max(1);

        let key = (page, block, par, line);
        let entry = lines.entry(key).or_insert_with(|| LineAccum {
            min_x: left,
            min_y: top,
            max_x: left + width,
            max_y: top + height,
            words: Vec::new(),
        });
        entry.min_x = entry.min_x.min(left);
        entry.min_y = entry.min_y.min(top);
        entry.max_x = entry.max_x.max(left + width);
        entry.max_y = entry.max_y.max(top + height);
        entry.words.push(text.to_owned());
    }

    let mut blocks = Vec::with_capacity(lines.len());
    let mut joined = Vec::with_capacity(lines.len());
    for acc in lines.into_values() {
        let original = acc.words.join(" ");
        joined.push(original.clone());
        blocks.push(TranslationBlock {
            x: acc.min_x,
            y: acc.min_y,
            w: (acc.max_x - acc.min_x).max(1),
            h: (acc.max_y - acc.min_y).max(1),
            original,
            translated: String::new(),
        });
    }
    (joined.join("\n"), blocks)
}

/// Sidecar path `<dir>/.photon/translation/<filename>.json` for an image.
pub fn sidecar_path(image_path: &Path) -> Option<PathBuf> {
    let parent = image_path.parent()?;
    let file_name = image_path.file_name()?.to_str()?;
    Some(
        parent
            .join(".photon")
            .join("translation")
            .join(format!("{file_name}.json")),
    )
}

pub fn load_sidecar(image_path: &Path) -> Option<TranslationResult> {
    let path = sidecar_path(image_path)?;
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_sidecar(result: &TranslationResult) -> Result<(), String> {
    let Some(path) = sidecar_path(Path::new(&result.path)) else {
        return Err("invalid image path for sidecar".to_owned());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(result).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    if let Err(err) = fs::rename(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        return Err(err.to_string());
    }
    Ok(())
}

pub fn delete_sidecar(image_path: &Path) -> Result<bool, String> {
    let Some(path) = sidecar_path(image_path) else {
        return Ok(false);
    };
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_file(&path).map_err(|e| e.to_string())?;
    if let Some(trans_dir) = path.parent() {
        let _ = fs::remove_dir(trans_dir);
        if let Some(meta_dir) = trans_dir.parent() {
            let _ = fs::remove_dir(meta_dir);
        }
    }
    Ok(true)
}

/// Load persisted translation bubbles as `SavedTranslation` if non-empty.
pub fn load_saved_translation(image_path: &Path) -> Option<SavedTranslation> {
    let loaded = load_sidecar(image_path)?;
    if loaded.blocks.is_empty() {
        return None;
    }
    Some(SavedTranslation {
        path: image_path.to_string_lossy().into_owned(),
        blocks: loaded.blocks,
    })
}

/// Save (or remove if `blocks` is empty) translation bubbles to `<dir>/.photon/translation/<file>.json`.
pub fn save_translation_blocks(
    image_path: &Path,
    source_lang: &str,
    target_lang: &str,
    blocks: Vec<TranslationBlock>,
) -> Result<Option<SavedTranslation>, String> {
    if blocks.is_empty() {
        let _ = delete_sidecar(image_path)?;
        return Ok(None);
    }
    let path_str = image_path.to_string_lossy().into_owned();
    let result = TranslationResult {
        path: path_str.clone(),
        source_lang: source_lang.to_owned(),
        target_lang: target_lang.to_owned(),
        original_text: String::new(),
        translated_text: String::new(),
        blocks: blocks.clone(),
    };
    save_sidecar(&result)?;
    Ok(Some(SavedTranslation {
        path: path_str,
        blocks,
    }))
}

/// Run Tesseract OCR (optionally on cropped `region`) and translate if `target_lang` and
/// `settings.provider != TranslateProvider::None`.
pub fn run_ocr_and_translate(
    image_path: &Path,
    region: Option<[i32; 4]>,
    source_lang: &str,
    target_lang: &str,
    settings: &TranslateSettings,
) -> Result<TranslationResult, ProtocolError> {
    let src_lang = if source_lang.trim().is_empty() {
        settings.default_source_lang.as_str()
    } else {
        source_lang.trim()
    };
    let tgt_lang = target_lang.trim();

    let mut temp_crop: Option<PathBuf> = None;
    let (ocr_input, off_x, off_y) = if let Some([rx, ry, rw, rh]) = region {
        let img = photon_image::decode_rgba(image_path)
            .map_err(|e| ProtocolError::new("decode_error", e.to_string()))?;
        let (iw, ih) = img.dimensions();
        let x0 = rx.max(0) as u32;
        let y0 = ry.max(0) as u32;
        let w = (rw.max(1) as u32).min(iw.saturating_sub(x0).max(1));
        let h = (rh.max(1) as u32).min(ih.saturating_sub(y0).max(1));
        let cropped =
            photon_image::pixels::grow_crop(&img, x0 as i32, y0 as i32, w, h, [0, 0, 0], 255);
        let tmp = std::env::temp_dir().join(format!(
            "photon-ocr-{}-{}.png",
            std::process::id(),
            photon_storage::journal::now_ms()
        ));
        photon_image::codec::save_to(&cropped, &tmp)
            .map_err(|e| ProtocolError::new("io_error", e.to_string()))?;
        temp_crop = Some(tmp.clone());
        (tmp, x0 as i32, y0 as i32)
    } else {
        (image_path.to_path_buf(), 0, 0)
    };

    let output = Command::new("tesseract")
        .arg(&ocr_input)
        .arg("stdout")
        .arg("-l")
        .arg(src_lang)
        .arg("tsv")
        .output();

    if let Some(tmp) = temp_crop {
        let _ = fs::remove_file(tmp);
    }

    let output = match output {
        Ok(out) => out,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(ProtocolError::line(
                "ocr_unavailable",
                UiText::new("translate_ocr_unavailable"),
            ));
        }
        Err(err) => {
            return Err(ProtocolError::new("ocr_error", err.to_string()));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(ProtocolError::new(
            "ocr_error",
            if stderr.is_empty() {
                "tesseract failed".to_owned()
            } else {
                stderr
            },
        ));
    }

    let tsv = String::from_utf8_lossy(&output.stdout);
    let (original_text, mut blocks) = parse_tesseract_tsv(&tsv, off_x, off_y);

    let mut translated_text = String::new();
    if !tgt_lang.is_empty()
        && settings.provider != TranslateProvider::None
        && !original_text.trim().is_empty()
        && let Ok(translated) = translate_lines(&original_text, src_lang, tgt_lang, settings)
    {
        let t_lines: Vec<&str> = translated.lines().collect();
        for (i, blk) in blocks.iter_mut().enumerate() {
            if let Some(&line) = t_lines.get(i) {
                blk.translated = line.to_owned();
            }
        }
        translated_text = translated;
    }

    Ok(TranslationResult {
        path: image_path.to_string_lossy().into_owned(),
        source_lang: src_lang.to_owned(),
        target_lang: tgt_lang.to_owned(),
        original_text,
        translated_text,
        blocks,
    })
}

fn map_tesseract_to_iso(lang: &str) -> &str {
    match lang {
        "eng" => "en",
        "vie" => "vi",
        "jpn" => "ja",
        "kor" => "ko",
        "chi_sim" => "zh-CN",
        "chi_tra" => "zh-TW",
        "fra" => "fr",
        "deu" => "de",
        "spa" => "es",
        "rus" => "ru",
        "auto" => "auto",
        other => other,
    }
}

fn translate_lines(
    text: &str,
    src_lang: &str,
    tgt_lang: &str,
    settings: &TranslateSettings,
) -> Result<String, String> {
    let sl = map_tesseract_to_iso(src_lang);
    let tl = map_tesseract_to_iso(tgt_lang);
    match settings.provider {
        TranslateProvider::None => Ok(String::new()),
        TranslateProvider::LibreTranslate => {
            let endpoint = format!(
                "{}/translate",
                settings.libre_endpoint.trim_end_matches('/')
            );
            let mut body = serde_json::json!({
                "q": text,
                "source": sl,
                "target": tl,
                "format": "text",
            });
            if let Some(key) = &settings.api_key
                && !key.trim().is_empty()
            {
                body["api_key"] = serde_json::Value::String(key.trim().to_owned());
            }
            let out = Command::new("curl")
                .args([
                    "-sS",
                    "--max-time",
                    "10",
                    "-H",
                    "Content-Type: application/json",
                    "-d",
                    &body.to_string(),
                    &endpoint,
                ])
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).into_owned());
            }
            let val: serde_json::Value =
                serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
            val.get("translatedText")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .ok_or_else(|| "missing translatedText in LibreTranslate response".to_owned())
        }
        TranslateProvider::GoogleWeb => {
            let out = Command::new("curl")
                .args([
                    "-sS",
                    "--max-time",
                    "10",
                    "-G",
                    "--data-urlencode",
                    "client=gtx",
                    "--data-urlencode",
                    &format!("sl={sl}"),
                    "--data-urlencode",
                    &format!("tl={tl}"),
                    "--data-urlencode",
                    "dt=t",
                    "--data-urlencode",
                    &format!("q={text}"),
                    "https://translate.googleapis.com/translate_a/single",
                ])
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).into_owned());
            }
            let val: serde_json::Value =
                serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
            let Some(sentences) = val.get(0).and_then(|v| v.as_array()) else {
                return Err("unexpected GoogleWeb response".to_owned());
            };
            let mut joined = String::new();
            for s in sentences {
                if let Some(part) = s.get(0).and_then(|v| v.as_str()) {
                    joined.push_str(part);
                }
            }
            Ok(joined)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsv_parser_groups_words_into_document_coordinate_lines() {
        let sample = "\
level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext
4\t1\t1\t1\t1\t0\t10\t20\t100\t18\t-1\t
5\t1\t1\t1\t1\t1\t10\t20\t40\t18\t95\tHello
5\t1\t1\t1\t1\t2\t55\t22\t55\t16\t92\tworld
5\t1\t1\t1\t2\t1\t12\t45\t60\t20\t90\tPhoton";

        let (text, blocks) = parse_tesseract_tsv(sample, 100, 50);
        assert_eq!(text, "Hello world\nPhoton");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].original, "Hello world");
        assert_eq!(
            (blocks[0].x, blocks[0].y, blocks[0].w, blocks[0].h),
            (110, 70, 100, 18)
        );
        assert_eq!(blocks[1].original, "Photon");
        assert_eq!(
            (blocks[1].x, blocks[1].y, blocks[1].w, blocks[1].h),
            (112, 95, 60, 20)
        );
    }

    #[test]
    fn sidecar_roundtrips_and_deletes() {
        let dir = std::env::temp_dir().join(format!("photon-tr-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let img = dir.join("page01.png");

        let res = TranslationResult {
            path: img.to_string_lossy().into_owned(),
            source_lang: "eng".into(),
            target_lang: "vi".into(),
            original_text: "Hello".into(),
            translated_text: "Xin chao".into(),
            blocks: vec![TranslationBlock {
                x: 5,
                y: 10,
                w: 50,
                h: 20,
                original: "Hello".into(),
                translated: "Xin chao".into(),
            }],
        };
        save_sidecar(&res).unwrap();
        let loaded = load_sidecar(&img).unwrap();
        assert_eq!(loaded, res);
        assert!(delete_sidecar(&img).unwrap());
        assert!(load_sidecar(&img).is_none());

        let _ = fs::remove_dir_all(&dir);
    }
}
