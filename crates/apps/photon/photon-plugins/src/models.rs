//! On-demand AI model & OCR language pack catalog, status resolution, and downloader.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use photon_protocol::{AiModelStatus, AiSettings, PluginModelSpec, TranslationResult};

use crate::{LoadedPlugin, PluginError};

/// Stable ID of the official Rust ONNX Remove Background plugin.
pub const REMBG_PLUGIN_ID: &str = "photon.rmbg.onnx";
/// Capability action for AI background removal.
pub const REMBG_ACTION: &str = "editor.remove_background";

/// Progress and completion events emitted by background AI downloads or plugin inference.
#[derive(Debug, Clone)]
pub enum AiWorkerEvent {
    Progress {
        percent: u8,
        stage: String,
    },
    DownloadDone {
        model_id: String,
        path: PathBuf,
    },
    InferenceDone {
        model_id: String,
        output_path: PathBuf,
        device_used: String,
        clipboard_text: Option<String>,
        translation: Option<TranslationResult>,
        caption_text: Option<String>,
        saved_path: Option<String>,
    },
    Failed {
        message: String,
    },
    Cancelled,
}

/// Built-in on-demand catalog for `rembg`-compatible ONNX models.
pub fn builtin_rembg_models() -> Vec<PluginModelSpec> {
    vec![
        PluginModelSpec {
            id: "u2netp".to_owned(),
            name: "U2-Net Lite".to_owned(),
            tier: "fast".to_owned(),
            file: "u2netp.onnx".to_owned(),
            size_bytes: 4_716_000,
            sha256: String::new(),
            url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx"
                .to_owned(),
        },
        PluginModelSpec {
            id: "silueta".to_owned(),
            name: "Silueta".to_owned(),
            tier: "fast".to_owned(),
            file: "silueta.onnx".to_owned(),
            size_bytes: 44_200_000,
            sha256: String::new(),
            url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/silueta.onnx"
                .to_owned(),
        },
        PluginModelSpec {
            id: "rmbg-1.4".to_owned(),
            name: "BRIA RMBG-1.4".to_owned(),
            tier: "balanced".to_owned(),
            file: "rmbg-1.4.onnx".to_owned(),
            size_bytes: 44_500_000,
            sha256: String::new(),
            url: "https://huggingface.co/briaai/RMBG-1.4/resolve/main/onnx/model_quantized.onnx"
                .to_owned(),
        },
        PluginModelSpec {
            id: "isnet-general-use".to_owned(),
            name: "IS-Net General".to_owned(),
            tier: "high".to_owned(),
            file: "isnet-general-use.onnx".to_owned(),
            size_bytes: 176_000_000,
            sha256: String::new(),
            url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/isnet-general-use.onnx"
                .to_owned(),
        },
        PluginModelSpec {
            id: "isnet-anime".to_owned(),
            name: "IS-Net Anime".to_owned(),
            tier: "anime".to_owned(),
            file: "isnet-anime.onnx".to_owned(),
            size_bytes: 176_000_000,
            sha256: String::new(),
            url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/isnet-anime.onnx"
                .to_owned(),
        },
        PluginModelSpec {
            id: "birefnet-general-lite".to_owned(),
            name: "BiRefNet Lite".to_owned(),
            tier: "high".to_owned(),
            file: "birefnet-general-lite.onnx".to_owned(),
            size_bytes: 214_000_000,
            sha256: String::new(),
            url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx"
                .to_owned(),
        },
        PluginModelSpec {
            id: "u2net_human_seg".to_owned(),
            name: "U2-Net Portrait".to_owned(),
            tier: "portrait".to_owned(),
            file: "u2net_human_seg.onnx".to_owned(),
            size_bytes: 176_000_000,
            sha256: String::new(),
            url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2net_human_seg.onnx"
                .to_owned(),
        },
        PluginModelSpec {
            id: "birefnet-portrait".to_owned(),
            name: "BiRefNet Portrait".to_owned(),
            tier: "portrait".to_owned(),
            file: "birefnet-portrait.onnx".to_owned(),
            size_bytes: 973_000_000,
            sha256: String::new(),
            url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/BiRefNet-portrait-epoch_150.onnx"
                .to_owned(),
        },
    ]
}

/// Collect all known models (built-in catalog + models declared in discovered plugin manifests).
pub fn catalog_models<'a>(
    plugins: impl IntoIterator<Item = &'a LoadedPlugin>,
) -> Vec<PluginModelSpec> {
    let mut specs = builtin_rembg_models();
    let mut seen: HashSet<String> = specs.iter().map(|m| m.id.clone()).collect();
    for plugin in plugins {
        for model in plugin.models() {
            if seen.insert(model.id.clone()) {
                specs.push(model.clone());
            }
        }
    }
    specs
}

/// Resolve installation status and effective on-disk path for every cataloged model.
pub fn model_statuses<'a>(
    models_dir: &Path,
    settings: &AiSettings,
    plugins: impl IntoIterator<Item = &'a LoadedPlugin>,
) -> Vec<AiModelStatus> {
    catalog_models(plugins)
        .into_iter()
        .map(|spec| {
            let custom_path = settings
                .custom_model_paths
                .get(&spec.id)
                .filter(|p| !p.trim().is_empty())
                .cloned();
            let effective = if let Some(custom) = &custom_path {
                let p = PathBuf::from(custom);
                p.is_file().then_some(p)
            } else {
                let p = models_dir.join(&spec.file);
                p.is_file().then_some(p)
            };
            AiModelStatus {
                id: spec.id,
                name: spec.name,
                tier: spec.tier,
                size_bytes: spec.size_bytes,
                installed: effective.is_some(),
                custom_path,
                effective_path: effective.map(|p| p.display().to_string()),
            }
        })
        .collect()
}

/// Remove a downloaded model file from `models_dir` if present.
pub fn delete_model_file(spec: &PluginModelSpec, models_dir: &Path) -> Result<bool, PluginError> {
    let path = models_dir.join(&spec.file);
    if !path.is_file() {
        return Ok(false);
    }
    fs::remove_file(&path).map_err(|source| PluginError::Io {
        context: format!("delete model file {}", path.display()),
        source,
    })?;
    Ok(true)
}

/// Download a model on demand into `models_dir/<spec.file>` with progress and
/// cancellation support. Uses atomic `.part` staging so a cancelled or failed
/// download never leaves a corrupt file behind.
pub fn download_model(
    spec: &PluginModelSpec,
    models_dir: &Path,
    cancel: Arc<AtomicBool>,
    mut on_event: impl FnMut(AiWorkerEvent),
) {
    if spec.url.trim().is_empty() {
        on_event(AiWorkerEvent::Failed {
            message: format!("model '{}' has no download URL", spec.id),
        });
        return;
    }

    if let Err(error) = fs::create_dir_all(models_dir) {
        on_event(AiWorkerEvent::Failed {
            message: format!("cannot create models directory: {error}"),
        });
        return;
    }

    let final_path = models_dir.join(&spec.file);
    let part_path = models_dir.join(format!("{}.part", spec.file));
    let _ = fs::remove_file(&part_path);

    on_event(AiWorkerEvent::Progress {
        percent: 0,
        stage: format!("Downloading {}…", spec.name),
    });

    let mut child = match Command::new("curl")
        .arg("-fSL")
        .arg("--connect-timeout")
        .arg("15")
        .arg("--output")
        .arg(&part_path)
        .arg(&spec.url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            on_event(AiWorkerEvent::Failed {
                message: format!("cannot start curl to download model: {error}"),
            });
            return;
        }
    };

    let mut last_percent = 0u8;
    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(&part_path);
            on_event(AiWorkerEvent::Cancelled);
            return;
        }

        if spec.size_bytes > 0
            && let Ok(meta) = fs::metadata(&part_path)
        {
            let pct = ((meta.len() as f64 / spec.size_bytes as f64) * 99.0).clamp(0.0, 99.0) as u8;
            if pct > last_percent {
                last_percent = pct;
                on_event(AiWorkerEvent::Progress {
                    percent: pct,
                    stage: format!("Downloading {} ({pct}%)…", spec.name),
                });
            }
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let mut err_text = String::new();
                    if let Some(mut stderr) = child.stderr.take() {
                        let _ = stderr.read_to_string(&mut err_text);
                    }
                    let _ = fs::remove_file(&part_path);
                    let detail = err_text.lines().last().unwrap_or("download failed").trim();
                    on_event(AiWorkerEvent::Failed {
                        message: format!("model download failed: {detail}"),
                    });
                    return;
                }
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(120)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_file(&part_path);
                on_event(AiWorkerEvent::Failed {
                    message: format!("download wait failed: {error}"),
                });
                return;
            }
        }
    }

    if !spec.sha256.trim().is_empty() {
        on_event(AiWorkerEvent::Progress {
            percent: 99,
            stage: "Verifying SHA-256…".to_owned(),
        });
        if let Err(error) = verify_sha256(&part_path, &spec.sha256) {
            let _ = fs::remove_file(&part_path);
            on_event(AiWorkerEvent::Failed { message: error });
            return;
        }
    }

    if let Err(error) = fs::rename(&part_path, &final_path) {
        let _ = fs::remove_file(&part_path);
        on_event(AiWorkerEvent::Failed {
            message: format!("cannot finalize model file: {error}"),
        });
        return;
    }

    on_event(AiWorkerEvent::DownloadDone {
        model_id: spec.id.clone(),
        path: final_path,
    });
}

fn verify_sha256(path: &Path, expected_hex: &str) -> Result<(), String> {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(|e| format!("cannot run sha256sum: {e}"))?;
    if !output.status.success() {
        return Err("sha256sum returned non-zero status".to_owned());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let actual = stdout.split_whitespace().next().unwrap_or("");
    if actual.eq_ignore_ascii_case(expected_hex.trim()) {
        Ok(())
    } else {
        Err(format!(
            "checksum mismatch (expected {}, got {actual})",
            expected_hex.trim()
        ))
    }
}
