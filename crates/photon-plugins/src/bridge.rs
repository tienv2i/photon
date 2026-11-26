//! Process-isolated NDJSON stdio execution bridge for Editor and Browser plugins.
//!
//! Supports both native Photon 1-line NDJSON (`PhotonNdjsonV1`) and KogaView
//! multi-line `jsonl-stdio` (`JsonlStdioV1`) plugin processes.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use image::{GrayImage, ImageFormat, ImageReader, RgbaImage};
use photon_protocol::{
    AiDevice, PluginCapability, PluginParamSpec, PluginParamValue, TranslationResult,
};
use serde::{Deserialize, Serialize};

use crate::manifest::{JsonlStdioMeta, LoadedPlugin, PluginBridgeProtocol};
use crate::{AiWorkerEvent, MAX_PLUGIN_IMAGE_DIMENSION, PLUGIN_BRIDGE_PROTOCOL, PluginError};

static TEMP_DIR_SEQ: AtomicU64 = AtomicU64::new(1);
const MAX_STDERR_BYTES: u64 = 16 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(5);
const MODELS_DIR_ENV: &str = "PHOTON_MODELS_DIR";

/// Output handling mode returned by a plugin in its JSON response.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginOutputMode {
    #[default]
    ReplaceLayer,
    NewLayer,
    BrowserRefresh,
}

/// Successful output from running an `editor_filter` plugin.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorPluginResult {
    pub image: RgbaImage,
    pub mode: PluginOutputMode,
    pub layer_name: Option<String>,
    pub message: Option<String>,
}

/// Successful output from running a `browser_action` plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserPluginResult {
    pub mode: PluginOutputMode,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
struct EditorPluginRequest<'a> {
    protocol: u32,
    action: &'static str,
    capability: PluginCapability,
    params: &'a BTreeMap<String, PluginParamValue>,
    input_path: String,
    selection_mask_path: Option<String>,
    width: u32,
    height: u32,
    output_path: String,
}

#[derive(Debug, Serialize)]
struct BrowserPluginRequest<'a> {
    protocol: u32,
    action: &'static str,
    capability: PluginCapability,
    params: &'a BTreeMap<String, PluginParamValue>,
    selected_paths: &'a [String],
    current_folder: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct RawPluginResponse {
    ok: bool,
    #[serde(default)]
    mode: Option<PluginOutputMode>,
    #[serde(default)]
    layer_name: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

/// Scoped temporary directory (`photon-plugin-<pid>-<seq>`) removed on drop.
struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new() -> Result<Self, PluginError> {
        let base = std::env::temp_dir();
        let pid = std::process::id();
        for _ in 0..32 {
            let seq = TEMP_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
            let candidate = base.join(format!("photon-plugin-{pid}-{seq}"));
            match fs::create_dir(&candidate) {
                Ok(()) => return Ok(Self { path: candidate }),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(source) => {
                    return Err(PluginError::Io {
                        context: format!("create plugin temp directory {}", candidate.display()),
                        source,
                    });
                }
            }
        }
        Err(PluginError::Io {
            context: "allocate unique plugin temp directory".to_owned(),
            source: std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "exhausted unique temp directory attempts",
            ),
        })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Execute an `editor_filter` plugin with the timeout configured in its manifest (default 30s).
pub fn run_editor_plugin(
    plugin: &LoadedPlugin,
    input: &RgbaImage,
    selection_mask: Option<&GrayImage>,
    params: &BTreeMap<String, PluginParamValue>,
) -> Result<EditorPluginResult, PluginError> {
    run_editor_plugin_with_timeout(plugin, input, selection_mask, params, None)
}

/// Execute an `editor_filter` plugin with an explicit optional timeout override.
pub fn run_editor_plugin_with_timeout(
    plugin: &LoadedPlugin,
    input: &RgbaImage,
    selection_mask: Option<&GrayImage>,
    params: &BTreeMap<String, PluginParamValue>,
    timeout_override: Option<Duration>,
) -> Result<EditorPluginResult, PluginError> {
    ensure_capability(plugin, PluginCapability::EditorFilter)?;

    let (width, height) = input.dimensions();
    if width == 0 || height == 0 {
        return Err(PluginError::InvalidParams {
            message: "input image must have non-zero dimensions".to_owned(),
        });
    }
    if let Some(mask) = selection_mask
        && mask.dimensions() != (width, height)
    {
        return Err(PluginError::InvalidParams {
            message: format!(
                "selection mask dimensions {}x{} do not match input {}x{}",
                mask.width(),
                mask.height(),
                width,
                height
            ),
        });
    }

    let temp_dir = TempDirGuard::new()?;
    let input_path = temp_dir.path().join("input.png");
    let output_path = temp_dir.path().join("output.png");

    input
        .save_with_format(&input_path, ImageFormat::Png)
        .map_err(|source| PluginError::Image {
            context: format!("encode plugin input PNG {}", input_path.display()),
            source,
        })?;

    let merged_params = merge_param_defaults(&plugin.manifest.params, params);
    let timeout = timeout_override.unwrap_or_else(|| plugin.timeout());

    match &plugin.protocol {
        PluginBridgeProtocol::PhotonNdjsonV1 => {
            let selection_mask_path = if let Some(mask) = selection_mask {
                let mask_path = temp_dir.path().join("selection_mask.png");
                mask.save_with_format(&mask_path, ImageFormat::Png)
                    .map_err(|source| PluginError::Image {
                        context: format!(
                            "encode plugin selection mask PNG {}",
                            mask_path.display()
                        ),
                        source,
                    })?;
                Some(mask_path.display().to_string())
            } else {
                None
            };

            let request = EditorPluginRequest {
                protocol: PLUGIN_BRIDGE_PROTOCOL,
                action: "run",
                capability: PluginCapability::EditorFilter,
                params: &merged_params,
                input_path: input_path.display().to_string(),
                selection_mask_path,
                width,
                height,
                output_path: output_path.display().to_string(),
            };
            let request_line =
                serde_json::to_string(&request).map_err(|err| PluginError::InvalidParams {
                    message: format!("serialize editor plugin request: {err}"),
                })?;

            let resp = run_ndjson_subprocess(plugin, &request_line, timeout)?;

            let mode = resp.mode.unwrap_or(PluginOutputMode::ReplaceLayer);
            if !matches!(
                mode,
                PluginOutputMode::ReplaceLayer | PluginOutputMode::NewLayer
            ) {
                return Err(PluginError::InvalidResponse {
                    id: plugin.manifest.id.clone(),
                    message: format!("editor_filter returned unsupported mode {mode:?}"),
                });
            }

            let decoded = decode_validated_output_png(&plugin.manifest.id, &output_path)?;
            Ok(EditorPluginResult {
                image: decoded,
                mode,
                layer_name: resp.layer_name,
                message: resp.message,
            })
        }
        PluginBridgeProtocol::JsonlStdioV1(meta) => {
            let action =
                resolve_jsonl_action(&merged_params, meta.default_editor_action.as_deref())
                    .ok_or_else(|| PluginError::UnsupportedCapability {
                        id: plugin.manifest.id.clone(),
                        required: PluginCapability::EditorFilter,
                    })?;
            let (done_meta, message) = run_jsonl_stdio_job(
                plugin,
                meta,
                &action,
                &input_path,
                &output_path,
                &merged_params,
                timeout,
            )?;

            let declared_output = meta
                .capability_outputs
                .get(&action)
                .map(String::as_str)
                .unwrap_or("image");
            let expects_image = matches!(declared_output, "image" | "rgba_image");

            let decoded = if expects_image || output_path.is_file() {
                decode_validated_output_png(&plugin.manifest.id, &output_path)?
            } else {
                // Text/caption/OCR-only actions produce metadata without an output PNG.
                let _ = done_meta;
                input.clone()
            };

            Ok(EditorPluginResult {
                image: decoded,
                mode: PluginOutputMode::ReplaceLayer,
                layer_name: None,
                message,
            })
        }
    }
}

/// Execute a `browser_action` plugin with the timeout configured in its manifest (default 30s).
pub fn run_browser_plugin(
    plugin: &LoadedPlugin,
    selected_paths: &[String],
    current_folder: Option<&str>,
    params: &BTreeMap<String, PluginParamValue>,
) -> Result<BrowserPluginResult, PluginError> {
    run_browser_plugin_with_timeout(plugin, selected_paths, current_folder, params, None)
}

/// Execute a `browser_action` plugin with an explicit optional timeout override.
pub fn run_browser_plugin_with_timeout(
    plugin: &LoadedPlugin,
    selected_paths: &[String],
    current_folder: Option<&str>,
    params: &BTreeMap<String, PluginParamValue>,
    timeout_override: Option<Duration>,
) -> Result<BrowserPluginResult, PluginError> {
    ensure_capability(plugin, PluginCapability::BrowserAction)?;

    let merged_params = merge_param_defaults(&plugin.manifest.params, params);
    let timeout = timeout_override.unwrap_or_else(|| plugin.timeout());

    match &plugin.protocol {
        PluginBridgeProtocol::PhotonNdjsonV1 => {
            let request = BrowserPluginRequest {
                protocol: PLUGIN_BRIDGE_PROTOCOL,
                action: "run",
                capability: PluginCapability::BrowserAction,
                params: &merged_params,
                selected_paths,
                current_folder,
            };
            let request_line =
                serde_json::to_string(&request).map_err(|err| PluginError::InvalidParams {
                    message: format!("serialize browser plugin request: {err}"),
                })?;

            let resp = run_ndjson_subprocess(plugin, &request_line, timeout)?;

            let mode = resp.mode.unwrap_or(PluginOutputMode::BrowserRefresh);
            if mode != PluginOutputMode::BrowserRefresh {
                return Err(PluginError::InvalidResponse {
                    id: plugin.manifest.id.clone(),
                    message: format!("browser_action returned unsupported mode {mode:?}"),
                });
            }

            Ok(BrowserPluginResult {
                mode,
                message: resp.message,
            })
        }
        PluginBridgeProtocol::JsonlStdioV1(meta) => {
            let Some(first_selected) = selected_paths.first() else {
                return Err(PluginError::InvalidParams {
                    message: "select an image file in Browser to run this plugin".to_owned(),
                });
            };
            let input_path = Path::new(first_selected);
            if !input_path.is_file() {
                return Err(PluginError::InvalidParams {
                    message: format!("selected file does not exist: {}", input_path.display()),
                });
            }

            let action =
                resolve_jsonl_action(&merged_params, meta.default_browser_action.as_deref())
                    .or_else(|| meta.default_editor_action.clone())
                    .ok_or_else(|| PluginError::UnsupportedCapability {
                        id: plugin.manifest.id.clone(),
                        required: PluginCapability::BrowserAction,
                    })?;

            let temp_dir = TempDirGuard::new()?;
            let ext = input_path
                .extension()
                .and_then(|e| e.to_str())
                .filter(|e| !e.is_empty())
                .unwrap_or("png");
            let temp_output = if action.starts_with("utils.") {
                temp_dir.path().join(format!("output.{ext}"))
            } else {
                temp_dir.path().join("output.png")
            };

            let (_done_meta, mut message) = run_jsonl_stdio_job(
                plugin,
                meta,
                &action,
                input_path,
                &temp_output,
                &merged_params,
                timeout,
            )?;

            let declared_output = meta
                .capability_outputs
                .get(&action)
                .map(String::as_str)
                .unwrap_or("");
            if matches!(declared_output, "image" | "rgba_image") || temp_output.is_file() {
                let saved = commit_browser_image_output(
                    &plugin.manifest.id,
                    &action,
                    input_path,
                    &temp_output,
                )?;
                if message.is_none() {
                    message = Some(format!("Saved {}", saved.display()));
                }
            }

            Ok(BrowserPluginResult {
                mode: PluginOutputMode::BrowserRefresh,
                message,
            })
        }
    }
}

fn ensure_capability(plugin: &LoadedPlugin, required: PluginCapability) -> Result<(), PluginError> {
    if plugin.manifest.capabilities.contains(&required) {
        Ok(())
    } else {
        Err(PluginError::UnsupportedCapability {
            id: plugin.manifest.id.clone(),
            required,
        })
    }
}

fn merge_param_defaults(
    specs: &[PluginParamSpec],
    provided: &BTreeMap<String, PluginParamValue>,
) -> BTreeMap<String, PluginParamValue> {
    let mut merged = provided.clone();
    for spec in specs {
        if !merged.contains_key(&spec.key)
            && let Some(default) = &spec.default
        {
            merged.insert(spec.key.clone(), default.clone());
        }
    }
    merged
}

fn resolve_jsonl_action(
    params: &BTreeMap<String, PluginParamValue>,
    default_action: Option<&str>,
) -> Option<String> {
    if let Some(PluginParamValue::Select(action)) = params.get("action")
        && !action.trim().is_empty()
    {
        return Some(action.clone());
    }
    default_action.map(str::to_owned)
}

/// Resolve the models directory for AI/OCR plugins.
pub fn resolve_models_dir_for(
    plugin: Option<&LoadedPlugin>,
    override_dir: Option<&Path>,
) -> PathBuf {
    if let Some(dir) = override_dir
        && !dir.as_os_str().is_empty()
    {
        return dir.to_path_buf();
    }
    if let Some(env_dir) = std::env::var_os(MODELS_DIR_ENV).filter(|s| !s.is_empty()) {
        return PathBuf::from(env_dir);
    }
    if let Some(data_dir) = plugin.and_then(|p| p.plugin_dir.parent().and_then(|d| d.parent())) {
        let sibling_models = data_dir.join("models");
        if sibling_models.is_dir() {
            return sibling_models;
        }
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
    if let Some(data_dir) = plugin.and_then(|p| p.plugin_dir.parent().and_then(|d| d.parent())) {
        return data_dir.join("models");
    }
    data_home.join("photon/models")
}

fn resolve_models_dir(
    plugin: &LoadedPlugin,
    params: &BTreeMap<String, PluginParamValue>,
) -> PathBuf {
    let param_dir = match params.get("modelsDir") {
        Some(PluginParamValue::Select(dir)) if !dir.trim().is_empty() => Some(Path::new(dir)),
        _ => None,
    };
    resolve_models_dir_for(Some(plugin), param_dir)
}

pub fn commit_browser_image_output(
    plugin_id: &str,
    action: &str,
    input_path: &Path,
    temp_output: &Path,
) -> Result<PathBuf, PluginError> {
    if !temp_output.is_file() {
        return Err(PluginError::InvalidResponse {
            id: plugin_id.to_owned(),
            message: format!(
                "plugin did not produce output image at {}",
                temp_output.display()
            ),
        });
    }

    // Validate output dimensions before touching the user's file on disk.
    let decoded = decode_validated_output_png(plugin_id, temp_output)?;

    let input_ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    // Background removal adds alpha; if source wasn't PNG/WebP, write a sibling `.png`.
    if action == "editor.remove_background" && !matches!(input_ext.as_str(), "png" | "webp") {
        let stem = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image");
        let dest = input_path.with_file_name(format!("{stem}_nobg.png"));
        let backup = create_numbered_backup_if_exists(&dest)?;
        let _ = backup;
        decoded
            .save_with_format(&dest, ImageFormat::Png)
            .map_err(|source| PluginError::Image {
                context: format!("save plugin output {}", dest.display()),
                source,
            })?;
        return Ok(dest);
    }

    let _ = create_numbered_backup_if_exists(input_path)?;
    if action.starts_with("utils.") || input_ext == "png" {
        // `utils` writes exact target extension bytes, and PNG outputs can copy directly.
        if fs::copy(temp_output, input_path).is_err() {
            let data = fs::read(temp_output).map_err(|source| PluginError::Io {
                context: format!("read plugin output {}", temp_output.display()),
                source,
            })?;
            fs::write(input_path, data).map_err(|source| PluginError::Io {
                context: format!("write plugin output {}", input_path.display()),
                source,
            })?;
        }
    } else {
        let fmt = ImageFormat::from_path(input_path).unwrap_or(ImageFormat::Png);
        decoded
            .save_with_format(input_path, fmt)
            .map_err(|source| PluginError::Image {
                context: format!("save plugin output {}", input_path.display()),
                source,
            })?;
    }

    Ok(input_path.to_path_buf())
}

fn create_numbered_backup_if_exists(path: &Path) -> Result<Option<PathBuf>, PluginError> {
    if !path.exists() {
        return Ok(None);
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
    let first = path.with_file_name(format!("{name}~"));
    let backup_path = if !first.exists() {
        first
    } else {
        let mut idx = 2u32;
        loop {
            let candidate = path.with_file_name(format!("{name}.{idx}~"));
            if !candidate.exists() {
                break candidate;
            }
            idx = idx.saturating_add(1);
        }
    };
    fs::copy(path, &backup_path).map_err(|source| PluginError::Io {
        context: format!("create backup {}", backup_path.display()),
        source,
    })?;
    Ok(Some(backup_path))
}

fn decode_validated_output_png(id: &str, output_path: &Path) -> Result<RgbaImage, PluginError> {
    if !output_path.is_file() {
        return Err(PluginError::InvalidResponse {
            id: id.to_owned(),
            message: format!(
                "plugin did not produce output image at {}",
                output_path.display()
            ),
        });
    }

    let reader = ImageReader::open(output_path)
        .map_err(|source| PluginError::Io {
            context: format!("open plugin output {}", output_path.display()),
            source,
        })?
        .with_guessed_format()
        .map_err(|source| PluginError::Io {
            context: format!("detect format of plugin output {}", output_path.display()),
            source,
        })?;

    let (width, height) = reader
        .into_dimensions()
        .map_err(|source| PluginError::Image {
            context: format!("read dimensions of plugin output {}", output_path.display()),
            source,
        })?;

    if width == 0
        || height == 0
        || width > MAX_PLUGIN_IMAGE_DIMENSION
        || height > MAX_PLUGIN_IMAGE_DIMENSION
    {
        return Err(PluginError::InvalidOutputDimensions {
            width,
            height,
            max: MAX_PLUGIN_IMAGE_DIMENSION,
        });
    }

    let img = ImageReader::open(output_path)
        .map_err(|source| PluginError::Io {
            context: format!("open plugin output {}", output_path.display()),
            source,
        })?
        .with_guessed_format()
        .map_err(|source| PluginError::Io {
            context: format!("detect format of plugin output {}", output_path.display()),
            source,
        })?
        .decode()
        .map_err(|source| PluginError::Image {
            context: format!("decode plugin output PNG {}", output_path.display()),
            source,
        })?;

    Ok(img.to_rgba8())
}

/// Spawn a `jsonl-stdio` plugin for one asynchronous inference/OCR/caption job,
/// streaming `AiWorkerEvent` progress, honoring cancellation, and sending `shutdown`
/// so RAM/VRAM is released cleanly.
#[allow(clippy::too_many_arguments)]
pub fn run_jsonl_inference_streaming(
    plugin: &LoadedPlugin,
    action: &str,
    model_id: &str,
    model_path: &Path,
    device: AiDevice,
    models_dir: &Path,
    input_path: &Path,
    output_path: &Path,
    mut params: serde_json::Value,
    cancel: Arc<AtomicBool>,
    mut on_event: impl FnMut(AiWorkerEvent),
) {
    if !plugin.entry_path.is_file() {
        on_event(AiWorkerEvent::Failed {
            message: format!(
                "plugin '{}' binary not found at {}",
                plugin.manifest.id,
                plugin.entry_path.display()
            ),
        });
        return;
    }

    if let Some(obj) = params.as_object_mut()
        && !models_dir.as_os_str().is_empty()
    {
        obj.entry("modelsDir".to_owned())
            .or_insert_with(|| serde_json::Value::String(models_dir.display().to_string()));
    }

    let expanded_args: Vec<String> = plugin
        .manifest
        .args
        .iter()
        .map(|arg| arg.replace("${device}", device.as_str()))
        .collect();

    let mut cmd = build_plugin_command(&plugin.entry_path, &expanded_args);
    cmd.current_dir(&plugin.plugin_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(unix)]
    if let Some(bin_dir) = plugin.entry_path.parent() {
        let existing = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
        let combined = if existing.is_empty() {
            bin_dir.display().to_string()
        } else {
            format!("{}:{existing}", bin_dir.display())
        };
        cmd.env("LD_LIBRARY_PATH", combined);
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(error) => {
            on_event(AiWorkerEvent::Failed {
                message: format!("failed to launch plugin '{}': {error}", plugin.manifest.id),
            });
            return;
        }
    };

    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        on_event(AiWorkerEvent::Failed {
            message: "plugin stdin unavailable".to_owned(),
        });
        return;
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        on_event(AiWorkerEvent::Failed {
            message: "plugin stdout unavailable".to_owned(),
        });
        return;
    };

    let req = serde_json::json!({
        "id": "req-1",
        "op": "run",
        "action": action,
        "modelId": model_id,
        "modelPath": model_path.display().to_string(),
        "device": device.as_str(),
        "input": input_path.display().to_string(),
        "output": output_path.display().to_string(),
        "params": params,
    });
    if let Err(error) = writeln!(stdin, "{req}") {
        let _ = child.kill();
        let _ = child.wait();
        on_event(AiWorkerEvent::Failed {
            message: format!("failed to send request to plugin: {error}"),
        });
        return;
    }
    let _ = stdin.flush();

    let child_pid = child.id();
    let cancel_watch = Arc::clone(&cancel);
    let done_flag = Arc::new(AtomicBool::new(false));
    let done_watch = Arc::clone(&done_flag);
    std::thread::spawn(move || {
        while !done_watch.load(Ordering::SeqCst) {
            if cancel_watch.load(Ordering::SeqCst) {
                #[cfg(unix)]
                {
                    let _ = Command::new("kill")
                        .arg("-TERM")
                        .arg(child_pid.to_string())
                        .status();
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(80));
        }
    });

    let stderr_pipe = child.stderr.take();
    let stderr_handle = std::thread::spawn(move || -> String {
        let Some(pipe) = stderr_pipe else {
            return String::new();
        };
        let mut buf = Vec::new();
        let _ = pipe.take(MAX_STDERR_BYTES).read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).trim().to_owned()
    });

    let reader = BufReader::new(stdout);
    let mut completed = false;
    let mut failed_msg: Option<String> = None;
    let mut device_used = device.as_str().to_owned();
    let mut clipboard_text: Option<String> = None;
    let mut translation: Option<TranslationResult> = None;
    let mut caption_text: Option<String> = None;
    let mut saved_path: Option<String> = None;

    for line in reader.lines() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let Ok(line) = line else {
            break;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        match msg
            .get("event")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
        {
            "progress" => {
                let percent = msg
                    .get("percent")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as u8;
                let stage = msg
                    .get("stage")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Processing…")
                    .to_owned();
                on_event(AiWorkerEvent::Progress {
                    percent: percent.min(99),
                    stage,
                });
            }
            "done" => {
                if let Some(meta) = msg.get("meta") {
                    if let Some(dev) = meta.get("device").and_then(serde_json::Value::as_str) {
                        device_used = dev.to_owned();
                    }
                    if let Some(text) = meta
                        .get("clipboardText")
                        .and_then(serde_json::Value::as_str)
                    {
                        clipboard_text = Some(text.to_owned());
                    }
                    if let Some(cap) = meta.get("captionText").and_then(serde_json::Value::as_str) {
                        caption_text = Some(cap.to_owned());
                    }
                    if let Some(saved) = meta.get("savedPath").and_then(serde_json::Value::as_str) {
                        saved_path = Some(saved.to_owned());
                    }
                    if let Some(tr_val) = meta.get("translation")
                        && let Ok(parsed) =
                            serde_json::from_value::<TranslationResult>(tr_val.clone())
                    {
                        translation = Some(parsed);
                    }
                }
                completed = true;
                break;
            }
            "error" => {
                let message = msg
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("plugin reported an error")
                    .to_owned();
                failed_msg = Some(message);
                break;
            }
            _ => {}
        }
    }

    let _ = writeln!(stdin, "{{\"op\":\"shutdown\"}}");
    drop(stdin);
    done_flag.store(true, Ordering::SeqCst);

    if cancel.load(Ordering::SeqCst) {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stderr_handle.join();
        on_event(AiWorkerEvent::Cancelled);
        return;
    }

    let _ = child.wait();
    let stderr_text = stderr_handle.join().unwrap_or_default();

    if let Some(message) = failed_msg {
        on_event(AiWorkerEvent::Failed { message });
    } else if completed
        && (output_path.is_file()
            || clipboard_text.is_some()
            || translation.is_some()
            || caption_text.is_some()
            || saved_path.is_some())
    {
        on_event(AiWorkerEvent::InferenceDone {
            model_id: model_id.to_owned(),
            output_path: output_path.to_path_buf(),
            device_used,
            clipboard_text,
            translation,
            caption_text,
            saved_path,
        });
    } else {
        let detail = stderr_text
            .lines()
            .last()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("plugin exited without producing an output")
            .trim()
            .to_owned();
        on_event(AiWorkerEvent::Failed { message: detail });
    }
}

fn run_jsonl_stdio_job(
    plugin: &LoadedPlugin,
    meta: &JsonlStdioMeta,
    action: &str,
    input_path: &Path,
    output_path: &Path,
    params: &BTreeMap<String, PluginParamValue>,
    timeout: Duration,
) -> Result<(Option<serde_json::Value>, Option<String>), PluginError> {
    let device = match params.get("device") {
        Some(PluginParamValue::Select(d)) if !d.trim().is_empty() => d.clone(),
        _ => "cpu".to_owned(),
    };

    let models_dir = resolve_models_dir(plugin, params);

    let mut json_params = serde_json::Map::new();
    // Apply matching quickAction defaults first if no explicit overrides exist.
    if let Some(qa) = meta.quick_actions.iter().find(|qa| qa.action == action)
        && let Some(obj) = qa.params.as_object()
    {
        for (k, v) in obj {
            json_params.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in params {
        if let Ok(val) = serde_json::to_value(v) {
            json_params.insert(k.clone(), val);
        }
    }
    json_params
        .entry("modelsDir".to_owned())
        .or_insert_with(|| serde_json::Value::String(models_dir.display().to_string()));

    let model_id = json_params
        .get("modelId")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .or_else(|| {
            meta.models
                .iter()
                .find(|m| !m.file.ends_with(".csv") && !m.file.ends_with(".txt"))
                .map(|m| m.id.clone())
        })
        .unwrap_or_default();

    let model_path = meta
        .models
        .iter()
        .find(|m| m.id == model_id)
        .map(|m| models_dir.join(&m.file).display().to_string())
        .unwrap_or_default();

    let req = serde_json::json!({
        "id": "req-1",
        "op": "run",
        "action": action,
        "modelId": model_id,
        "modelPath": model_path,
        "device": device,
        "input": input_path.display().to_string(),
        "output": output_path.display().to_string(),
        "params": serde_json::Value::Object(json_params),
    });

    let expanded_args: Vec<String> = plugin
        .manifest
        .args
        .iter()
        .map(|arg| arg.replace("${device}", &device))
        .collect();

    let mut cmd = build_plugin_command(&plugin.entry_path, &expanded_args);
    let cmd_desc = format!("{cmd:?}");
    cmd.current_dir(&plugin.plugin_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(unix)]
    if let Some(bin_dir) = plugin.entry_path.parent() {
        let existing = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
        let combined = if existing.is_empty() {
            bin_dir.display().to_string()
        } else {
            format!("{}:{existing}", bin_dir.display())
        };
        cmd.env("LD_LIBRARY_PATH", combined);
    }

    let mut child = cmd.spawn().map_err(|source| PluginError::SpawnFailed {
        command: cmd_desc,
        source,
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        let mut payload = req.to_string();
        payload.push_str("\n{\"op\":\"shutdown\"}\n");
        if let Err(err) = stdin.write_all(payload.as_bytes())
            && err.kind() != std::io::ErrorKind::BrokenPipe
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(PluginError::Io {
                context: format!("write stdin for plugin '{}'", plugin.manifest.id),
                source: err,
            });
        }
        let _ = stdin.flush();
    }

    enum JsonlOutcome {
        Done(Option<serde_json::Value>),
        Error(String),
        Eof,
    }

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_handle = std::thread::spawn(move || -> JsonlOutcome {
        let Some(pipe) = stdout_pipe else {
            return JsonlOutcome::Eof;
        };
        let mut reader = BufReader::new(pipe);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => return JsonlOutcome::Eof,
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) else {
                        continue;
                    };
                    match val.get("event").and_then(|e| e.as_str()).unwrap_or("") {
                        "progress" => continue,
                        "done" => return JsonlOutcome::Done(val.get("meta").cloned()),
                        "error" => {
                            let msg = val
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("plugin reported an error")
                                .to_owned();
                            return JsonlOutcome::Error(msg);
                        }
                        _ => continue,
                    }
                }
            }
        }
    });

    let stderr_handle = std::thread::spawn(move || -> String {
        let Some(pipe) = stderr_pipe else {
            return String::new();
        };
        let mut buf = Vec::new();
        let _ = pipe.take(MAX_STDERR_BYTES).read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).trim().to_owned()
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                let now = Instant::now();
                if now >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(PluginError::Timeout {
                        id: plugin.manifest.id.clone(),
                        timeout_ms: u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
                    });
                }
                std::thread::sleep(POLL_INTERVAL.min(deadline - now));
            }
            Err(source) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(PluginError::Io {
                    context: format!("wait for plugin '{}'", plugin.manifest.id),
                    source,
                });
            }
        }
    };

    let outcome = stdout_handle.join().unwrap_or(JsonlOutcome::Eof);
    let stderr_text = stderr_handle.join().unwrap_or_default();
    let id = &plugin.manifest.id;

    match outcome {
        JsonlOutcome::Error(message) => Err(PluginError::PluginReportedError {
            id: id.clone(),
            message,
        }),
        JsonlOutcome::Done(done_meta) => {
            if !status.success() {
                return Err(PluginError::ProcessFailed {
                    id: id.clone(),
                    status: status.code(),
                    stderr: stderr_text,
                });
            }
            let summary = summarize_jsonl_meta(done_meta.as_ref());
            Ok((done_meta, summary))
        }
        JsonlOutcome::Eof => {
            if !status.success() {
                Err(PluginError::ProcessFailed {
                    id: id.clone(),
                    status: status.code(),
                    stderr: stderr_text,
                })
            } else {
                Err(PluginError::InvalidResponse {
                    id: id.clone(),
                    message: "plugin exited without emitting a 'done' or 'error' event".to_owned(),
                })
            }
        }
    }
}

fn summarize_jsonl_meta(meta: Option<&serde_json::Value>) -> Option<String> {
    let obj = meta?.as_object()?;
    for key in ["captionText", "clipboardText", "savedPath"] {
        if let Some(s) = obj.get(key).and_then(|v| v.as_str())
            && !s.trim().is_empty()
        {
            return Some(s.trim().to_owned());
        }
    }
    if let Some(tr) = obj.get("translation") {
        if let Some(text) = tr.get("translatedText").and_then(|v| v.as_str())
            && !text.trim().is_empty()
        {
            return Some(text.trim().to_owned());
        }
        if let Some(text) = tr.get("originalText").and_then(|v| v.as_str())
            && !text.trim().is_empty()
        {
            return Some(text.trim().to_owned());
        }
    }
    None
}

fn run_ndjson_subprocess(
    plugin: &LoadedPlugin,
    request_line: &str,
    timeout: Duration,
) -> Result<RawPluginResponse, PluginError> {
    let mut cmd = build_plugin_command(&plugin.entry_path, &plugin.manifest.args);
    let cmd_desc = format!("{cmd:?}");
    cmd.current_dir(&plugin.plugin_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|source| PluginError::SpawnFailed {
        command: cmd_desc,
        source,
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        let mut payload = String::with_capacity(request_line.len() + 1);
        payload.push_str(request_line);
        payload.push('\n');
        if let Err(err) = stdin.write_all(payload.as_bytes())
            && err.kind() != std::io::ErrorKind::BrokenPipe
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(PluginError::Io {
                context: format!("write stdin for plugin '{}'", plugin.manifest.id),
                source: err,
            });
        }
        let _ = stdin.flush();
    }

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_handle = std::thread::spawn(move || -> Option<String> {
        let pipe = stdout_pipe?;
        let mut reader = BufReader::new(pipe);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => return None,
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_owned());
                    }
                }
                Err(_) => return None,
            }
        }
    });

    let stderr_handle = std::thread::spawn(move || -> String {
        let Some(pipe) = stderr_pipe else {
            return String::new();
        };
        let mut buf = Vec::new();
        let _ = pipe.take(MAX_STDERR_BYTES).read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).trim().to_owned()
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                let now = Instant::now();
                if now >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    // Detach pipe reader threads in case a grandchild inherited stdout/stderr.
                    return Err(PluginError::Timeout {
                        id: plugin.manifest.id.clone(),
                        timeout_ms: u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
                    });
                }
                std::thread::sleep(POLL_INTERVAL.min(deadline - now));
            }
            Err(source) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(PluginError::Io {
                    context: format!("wait for plugin '{}'", plugin.manifest.id),
                    source,
                });
            }
        }
    };

    let stdout_line = stdout_handle.join().ok().flatten();
    let stderr_text = stderr_handle.join().unwrap_or_default();

    let id = &plugin.manifest.id;
    let Some(line) = stdout_line else {
        if !status.success() {
            return Err(PluginError::ProcessFailed {
                id: id.clone(),
                status: status.code(),
                stderr: stderr_text,
            });
        }
        return Err(PluginError::InvalidResponse {
            id: id.clone(),
            message: "plugin produced no JSON response on stdout".to_owned(),
        });
    };

    match serde_json::from_str::<RawPluginResponse>(&line) {
        Ok(resp) => {
            if !resp.ok {
                return Err(PluginError::PluginReportedError {
                    id: id.clone(),
                    message: resp
                        .error
                        .unwrap_or_else(|| "plugin returned ok=false".to_owned()),
                });
            }
            if !status.success() {
                return Err(PluginError::ProcessFailed {
                    id: id.clone(),
                    status: status.code(),
                    stderr: stderr_text,
                });
            }
            Ok(resp)
        }
        Err(err) => {
            if !status.success() {
                Err(PluginError::ProcessFailed {
                    id: id.clone(),
                    status: status.code(),
                    stderr: stderr_text,
                })
            } else {
                Err(PluginError::InvalidResponse {
                    id: id.clone(),
                    message: format!("malformed JSON line: {err}"),
                })
            }
        }
    }
}

fn interpreter_for_extension(entry_path: &Path) -> Option<&'static str> {
    let ext = entry_path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "py" => Some(if cfg!(windows) { "python" } else { "python3" }),
        "sh" => Some("sh"),
        "js" | "mjs" | "cjs" => Some("node"),
        _ => None,
    }
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|meta| meta.is_file() && (meta.permissions().mode() & 0o111 != 0))
        .unwrap_or(false)
}

#[cfg(unix)]
fn starts_with_shebang(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic).is_ok() && magic == *b"#!"
}

fn build_plugin_command(entry_path: &Path, extra_args: &[String]) -> Command {
    let mut cmd = {
        #[cfg(unix)]
        {
            if is_executable_file(entry_path)
                && (interpreter_for_extension(entry_path).is_none()
                    || starts_with_shebang(entry_path))
            {
                Command::new(entry_path)
            } else if let Some(interp) = interpreter_for_extension(entry_path) {
                let mut c = Command::new(interp);
                c.arg(entry_path);
                c
            } else {
                Command::new(entry_path)
            }
        }
        #[cfg(not(unix))]
        {
            if let Some(interp) = interpreter_for_extension(entry_path) {
                let mut c = Command::new(interp);
                c.arg(entry_path);
                c
            } else {
                Command::new(entry_path)
            }
        }
    };

    cmd.args(extra_args);
    cmd
}
