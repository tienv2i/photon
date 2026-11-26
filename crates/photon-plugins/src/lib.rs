//! Process-isolated NDJSON plugin bridge for Photon (headless-capable).
//!
//! Discovers, validates, installs, and executes external plugins over
//! single-line NDJSON `stdin`/`stdout` with scoped temporary image buffers.
//! Contains zero GUI or GPU dependencies.

mod bridge;
mod manifest;
mod models;
mod registry;

use std::path::PathBuf;
use std::time::Duration;

use photon_protocol::PluginCapability;
use thiserror::Error;

pub use bridge::{
    BrowserPluginResult, EditorPluginResult, PluginOutputMode, commit_browser_image_output,
    resolve_models_dir_for, run_browser_plugin, run_browser_plugin_with_timeout, run_editor_plugin,
    run_editor_plugin_with_timeout, run_jsonl_inference_streaming,
};
pub use manifest::{
    LoadedPlugin, load_plugin_from_dir, resolve_safe_entry, validate_manifest, validate_plugin_id,
};
pub use models::{
    AiWorkerEvent, REMBG_ACTION, REMBG_PLUGIN_ID, builtin_rembg_models, catalog_models,
    delete_model_file, download_model, model_statuses,
};
pub use registry::{
    PluginDiscoveryError, PluginRegistry, install_from_dir, resolve_plugins_dir, uninstall,
};

/// NDJSON wire protocol version spoken with external plugin processes.
pub const PLUGIN_BRIDGE_PROTOCOL: u32 = 1;

/// Environment variable overriding the default plugins directory.
pub const PLUGINS_DIR_ENV: &str = "PHOTON_PLUGINS_DIR";

/// Default timeout enforced on native NDJSON plugin subprocess execution.
pub const DEFAULT_PLUGIN_TIMEOUT: Duration = Duration::from_secs(30);

/// Default timeout enforced on `jsonl-stdio` AI/OCR plugin execution when no `timeoutMs` is set.
pub const DEFAULT_AI_PLUGIN_TIMEOUT: Duration = Duration::from_secs(600);

/// Maximum allowed width or height (in pixels) for plugin output images.
pub const MAX_PLUGIN_IMAGE_DIMENSION: u32 = 32_768;

/// Errors produced during plugin discovery, validation, installation, or execution.
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("invalid plugin manifest at {path}: {message}")]
    InvalidManifest { path: PathBuf, message: String },

    #[error("invalid plugin id '{id}': {reason}")]
    InvalidPluginId { id: String, reason: String },

    #[error("unsafe plugin entry path '{entry}': {reason}")]
    PathTraversal { entry: String, reason: String },

    #[error("plugin entry not found at {path}")]
    EntryNotFound { path: PathBuf },

    #[error("plugin '{id}' not found")]
    NotFound { id: String },

    #[error("plugin '{id}' does not declare capability {required:?}")]
    UnsupportedCapability {
        id: String,
        required: PluginCapability,
    },

    #[error("invalid plugin arguments: {message}")]
    InvalidParams { message: String },

    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{context}: {source}")]
    Image {
        context: String,
        #[source]
        source: image::ImageError,
    },

    #[error("failed to spawn plugin command '{command}': {source}")]
    SpawnFailed {
        command: String,
        #[source]
        source: std::io::Error,
    },

    #[error("plugin '{id}' timed out after {timeout_ms} ms")]
    Timeout { id: String, timeout_ms: u64 },

    #[error("plugin '{id}' exited with status {status:?}: {stderr}")]
    ProcessFailed {
        id: String,
        status: Option<i32>,
        stderr: String,
    },

    #[error("plugin '{id}' returned invalid response: {message}")]
    InvalidResponse { id: String, message: String },

    #[error("plugin '{id}' failed: {message}")]
    PluginReportedError { id: String, message: String },

    #[error("plugin output dimensions {width}x{height} exceed allowed maximum {max}x{max}")]
    InvalidOutputDimensions { width: u32, height: u32, max: u32 },
}
