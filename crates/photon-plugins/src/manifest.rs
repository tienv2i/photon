//! Manifest parsing, ID validation, and path-traversal-safe entry resolution.
//!
//! Supports both native Photon `plugin.json` (`PhotonNdjsonV1`) and KogaView
//! `manifest.json` (`JsonlStdioV1`) plugin manifests.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use photon_protocol::{
    PluginCapability, PluginInfo, PluginManifest, PluginModelSpec, PluginParamKind,
    PluginParamSpec, PluginParamValue, PluginQuickAction, plugin_hooks,
};
use serde::Deserialize;

use crate::{DEFAULT_AI_PLUGIN_TIMEOUT, DEFAULT_PLUGIN_TIMEOUT, PluginError};

/// Wire protocol spoken with the plugin subprocess.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PluginBridgeProtocol {
    PhotonNdjsonV1,
    JsonlStdioV1(JsonlStdioMeta),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct JsonlStdioMeta {
    pub default_editor_action: Option<String>,
    pub default_browser_action: Option<String>,
    pub capability_outputs: BTreeMap<String, String>,
    pub viewer_actions: Vec<String>,
    pub models: Vec<PluginModelSpec>,
    pub quick_actions: Vec<PluginQuickAction>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawJsonlManifest {
    api_version: u32,
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    author: String,
    entry: RawJsonlEntry,
    #[serde(default)]
    permissions: RawJsonlPermissions,
    #[serde(default)]
    models: Vec<PluginModelSpec>,
    #[serde(default)]
    capabilities: Vec<RawJsonlCapability>,
    #[serde(default)]
    quick_actions: Vec<PluginQuickAction>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default = "default_manifest_order")]
    order: u16,
    #[serde(default)]
    category: String,
    #[serde(default)]
    hooks: Vec<String>,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

fn default_manifest_order() -> u16 {
    100
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawJsonlEntry {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default = "default_jsonl_protocol")]
    protocol: String,
}

fn default_jsonl_protocol() -> String {
    "jsonl-stdio".to_owned()
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawJsonlPermissions {
    #[serde(default)]
    network: bool,
    #[serde(default)]
    gpu: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawJsonlCapability {
    action: String,
    #[serde(default)]
    workspace: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    output: String,
    #[serde(default)]
    params: Vec<RawJsonlParamSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawJsonlParamSpec {
    key: String,
    #[serde(default)]
    label: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
    #[serde(default)]
    step: Option<f64>,
    #[serde(default)]
    default: Option<serde_json::Value>,
    #[serde(default)]
    options: Vec<String>,
    #[serde(default)]
    description: Option<String>,
}

/// Validated plugin manifest paired with its on-disk directory and resolved entry path.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub plugin_dir: PathBuf,
    pub entry_path: PathBuf,
    pub(crate) protocol: PluginBridgeProtocol,
}

impl LoadedPlugin {
    /// Convert this loaded plugin into a `PluginInfo` snapshot for `UiStateMeta`.
    pub fn to_info(&self, disabled_plugins: &[String]) -> PluginInfo {
        let enabled = !disabled_plugins.iter().any(|id| id == &self.manifest.id);
        let mut info = PluginInfo::from_manifest(
            &self.manifest,
            enabled,
            self.plugin_dir.display().to_string(),
            None,
        );
        info.available = self.entry_path.is_file();
        info.viewer_actions = self.viewer_actions().to_vec();
        info.quick_actions = self.quick_actions().to_vec();
        info.models = self.models().to_vec();
        info
    }

    /// Configured execution timeout, falling back to the protocol default.
    pub fn timeout(&self) -> Duration {
        self.manifest
            .timeout_ms
            .filter(|&ms| ms > 0)
            .map(Duration::from_millis)
            .unwrap_or_else(|| {
                if self.is_jsonl_stdio() {
                    DEFAULT_AI_PLUGIN_TIMEOUT
                } else {
                    DEFAULT_PLUGIN_TIMEOUT
                }
            })
    }

    /// Whether this plugin uses the KogaView `jsonl-stdio` protocol.
    pub fn is_jsonl_stdio(&self) -> bool {
        matches!(self.protocol, PluginBridgeProtocol::JsonlStdioV1(_))
    }

    /// Declared downloadable/selectable AI models or language packs.
    pub fn models(&self) -> &[PluginModelSpec] {
        match &self.protocol {
            PluginBridgeProtocol::PhotonNdjsonV1 => &[],
            PluginBridgeProtocol::JsonlStdioV1(meta) => &meta.models,
        }
    }

    /// Declared Browser/Viewer Quick Actions.
    pub fn quick_actions(&self) -> &[PluginQuickAction] {
        match &self.protocol {
            PluginBridgeProtocol::PhotonNdjsonV1 => &[],
            PluginBridgeProtocol::JsonlStdioV1(meta) => &meta.quick_actions,
        }
    }

    /// Declared Viewer capability action IDs (e.g. `viewer.translate`, `viewer.ocr_copy`, `viewer.ai_ocr`).
    pub fn viewer_actions(&self) -> &[String] {
        match &self.protocol {
            PluginBridgeProtocol::PhotonNdjsonV1 => &[],
            PluginBridgeProtocol::JsonlStdioV1(meta) => &meta.viewer_actions,
        }
    }

    /// Whether this plugin declares capability or quick-action support for `action`.
    pub fn supports_action(&self, action: &str) -> bool {
        match &self.protocol {
            PluginBridgeProtocol::PhotonNdjsonV1 => false,
            PluginBridgeProtocol::JsonlStdioV1(meta) => {
                meta.capability_outputs.contains_key(action)
                    || meta.viewer_actions.iter().any(|a| a == action)
                    || meta.quick_actions.iter().any(|qa| qa.action == action)
            }
        }
    }
}

/// Validate that a plugin ID is safe as a single directory name.
pub fn validate_plugin_id(id: &str) -> Result<(), PluginError> {
    if id.is_empty() {
        return Err(PluginError::InvalidPluginId {
            id: id.to_owned(),
            reason: "plugin id must not be empty".to_owned(),
        });
    }
    if id.len() > 128 {
        return Err(PluginError::InvalidPluginId {
            id: id.to_owned(),
            reason: "plugin id exceeds 128 characters".to_owned(),
        });
    }
    if id == "." || id == ".." || id.contains("..") {
        return Err(PluginError::InvalidPluginId {
            id: id.to_owned(),
            reason: "plugin id must not contain '..'".to_owned(),
        });
    }
    let bytes = id.as_bytes();
    if !bytes[0].is_ascii_alphanumeric() || !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
        return Err(PluginError::InvalidPluginId {
            id: id.to_owned(),
            reason: "plugin id must start and end with an ASCII alphanumeric character".to_owned(),
        });
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err(PluginError::InvalidPluginId {
            id: id.to_owned(),
            reason: "plugin id may only contain ASCII alphanumeric characters, '.', '-', or '_'"
                .to_owned(),
        });
    }
    Ok(())
}

/// Resolve `entry` strictly inside `plugin_dir`, rejecting absolute paths, `..` traversal,
/// and symlink escapes outside `plugin_dir`.
pub fn resolve_safe_entry(plugin_dir: &Path, entry: &str) -> Result<PathBuf, PluginError> {
    if entry.trim().is_empty() || entry.contains('\0') {
        return Err(PluginError::PathTraversal {
            entry: entry.to_owned(),
            reason: "entry path must be non-empty".to_owned(),
        });
    }
    if entry.starts_with('/') || entry.starts_with('\\') {
        return Err(PluginError::PathTraversal {
            entry: entry.to_owned(),
            reason: "absolute entry paths are forbidden".to_owned(),
        });
    }
    if entry.split(['/', '\\']).any(|seg| seg == "..") {
        return Err(PluginError::PathTraversal {
            entry: entry.to_owned(),
            reason: "parent directory traversal ('..') is forbidden".to_owned(),
        });
    }

    let rel = Path::new(entry);
    if rel.is_absolute() {
        return Err(PluginError::PathTraversal {
            entry: entry.to_owned(),
            reason: "absolute entry paths are forbidden".to_owned(),
        });
    }

    let mut has_normal = false;
    for comp in rel.components() {
        match comp {
            Component::Normal(_) => has_normal = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PluginError::PathTraversal {
                    entry: entry.to_owned(),
                    reason: "entry path escapes plugin directory".to_owned(),
                });
            }
        }
    }

    if !has_normal {
        return Err(PluginError::PathTraversal {
            entry: entry.to_owned(),
            reason: "entry path must point to a file inside the plugin directory".to_owned(),
        });
    }

    let joined = plugin_dir.join(rel);
    if plugin_dir.exists() {
        let canon_dir = fs::canonicalize(plugin_dir).map_err(|source| PluginError::Io {
            context: format!("canonicalize plugin directory {}", plugin_dir.display()),
            source,
        })?;
        // Check the deepest existing ancestor (or the entry itself) to catch symlink escapes.
        let mut probe = joined.as_path();
        while !probe.exists() && probe.symlink_metadata().is_err() {
            let Some(parent) = probe.parent() else {
                break;
            };
            probe = parent;
        }
        if probe.exists() {
            let canon_probe = fs::canonicalize(probe).map_err(|source| PluginError::Io {
                context: format!("canonicalize plugin entry {}", probe.display()),
                source,
            })?;
            if !canon_probe.starts_with(&canon_dir) {
                return Err(PluginError::PathTraversal {
                    entry: entry.to_owned(),
                    reason: "resolved path escapes plugin directory via symlink".to_owned(),
                });
            }
        }
    }

    Ok(joined)
}

/// Validate a parsed `PluginManifest` against `plugin_dir` and return its safe entry path.
pub fn validate_manifest(
    manifest: &PluginManifest,
    plugin_dir: &Path,
) -> Result<PathBuf, PluginError> {
    let manifest_path = plugin_dir.join("plugin.json");
    validate_plugin_id(&manifest.id)?;
    if manifest.name.trim().is_empty() {
        return Err(PluginError::InvalidManifest {
            path: manifest_path,
            message: "manifest 'name' must not be empty".to_owned(),
        });
    }
    if manifest.version.trim().is_empty() {
        return Err(PluginError::InvalidManifest {
            path: manifest_path,
            message: "manifest 'version' must not be empty".to_owned(),
        });
    }
    resolve_safe_entry(plugin_dir, &manifest.entry)
}

/// Read, parse, and validate `<plugin_dir>/plugin.json` (or `<plugin_dir>/manifest.json`)
/// and ensure its entry file exists.
pub fn load_plugin_from_dir(plugin_dir: &Path) -> Result<LoadedPlugin, PluginError> {
    let photon_manifest = plugin_dir.join("plugin.json");
    let jsonl_manifest = plugin_dir.join("manifest.json");

    let (manifest, entry_path, protocol) = if photon_manifest.exists() {
        let raw = fs::read(&photon_manifest).map_err(|source| PluginError::Io {
            context: format!("read {}", photon_manifest.display()),
            source,
        })?;
        let manifest: PluginManifest =
            serde_json::from_slice(&raw).map_err(|err| PluginError::InvalidManifest {
                path: photon_manifest.clone(),
                message: err.to_string(),
            })?;
        let entry_path = validate_manifest(&manifest, plugin_dir)?;
        (manifest, entry_path, PluginBridgeProtocol::PhotonNdjsonV1)
    } else if jsonl_manifest.exists() {
        load_jsonl_manifest(plugin_dir, &jsonl_manifest)?
    } else {
        return Err(PluginError::Io {
            context: format!("read {}", photon_manifest.display()),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "neither plugin.json nor manifest.json found",
            ),
        });
    };

    ensure_entry_is_file(&entry_path)?;

    Ok(LoadedPlugin {
        manifest,
        plugin_dir: plugin_dir.to_path_buf(),
        entry_path,
        protocol,
    })
}

fn ensure_entry_is_file(entry_path: &Path) -> Result<(), PluginError> {
    let meta = fs::symlink_metadata(entry_path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            PluginError::EntryNotFound {
                path: entry_path.to_path_buf(),
            }
        } else {
            PluginError::Io {
                context: format!("inspect plugin entry {}", entry_path.display()),
                source: err,
            }
        }
    })?;
    if meta.file_type().is_symlink() {
        // Already canonicalized inside `resolve_safe_entry`; follow metadata to confirm regular file.
        let target_meta = fs::metadata(entry_path).map_err(|source| PluginError::Io {
            context: format!("inspect plugin entry target {}", entry_path.display()),
            source,
        })?;
        if !target_meta.is_file() {
            return Err(PluginError::EntryNotFound {
                path: entry_path.to_path_buf(),
            });
        }
    } else if !meta.is_file() {
        return Err(PluginError::EntryNotFound {
            path: entry_path.to_path_buf(),
        });
    }
    Ok(())
}

fn load_jsonl_manifest(
    plugin_dir: &Path,
    manifest_path: &Path,
) -> Result<(PluginManifest, PathBuf, PluginBridgeProtocol), PluginError> {
    let raw_bytes = fs::read(manifest_path).map_err(|source| PluginError::Io {
        context: format!("read {}", manifest_path.display()),
        source,
    })?;
    let raw: RawJsonlManifest =
        serde_json::from_slice(&raw_bytes).map_err(|err| PluginError::InvalidManifest {
            path: manifest_path.to_path_buf(),
            message: err.to_string(),
        })?;

    if raw.api_version != 1 {
        return Err(PluginError::InvalidManifest {
            path: manifest_path.to_path_buf(),
            message: format!("unsupported apiVersion {}", raw.api_version),
        });
    }
    if raw.entry.protocol != "jsonl-stdio" {
        return Err(PluginError::InvalidManifest {
            path: manifest_path.to_path_buf(),
            message: format!("unsupported entry protocol '{}'", raw.entry.protocol),
        });
    }

    validate_plugin_id(&raw.id)?;
    if raw.name.trim().is_empty() {
        return Err(PluginError::InvalidManifest {
            path: manifest_path.to_path_buf(),
            message: "manifest 'name' must not be empty".to_owned(),
        });
    }
    if raw.version.trim().is_empty() {
        return Err(PluginError::InvalidManifest {
            path: manifest_path.to_path_buf(),
            message: "manifest 'version' must not be empty".to_owned(),
        });
    }

    let entry_path = resolve_safe_entry(plugin_dir, &raw.entry.command)?;

    let mut caps = Vec::new();
    let mut has_editor = false;
    let mut has_browser = false;
    let mut has_viewer = false;
    let mut default_editor_action: Option<String> = None;
    let mut default_browser_action: Option<String> = None;
    let mut capability_outputs = BTreeMap::new();
    let mut viewer_actions = Vec::new();
    let mut param_specs = Vec::new();
    let mut seen_param_keys = BTreeSet::new();
    let mut cap_labels = Vec::new();

    for cap in &raw.capabilities {
        capability_outputs.insert(cap.action.clone(), cap.output.clone());
        if !cap.label.trim().is_empty() {
            cap_labels.push(cap.label.trim().to_owned());
        }
        if cap.workspace == "viewer"
            || cap.action.starts_with("viewer.")
            || cap.action.starts_with("caption.")
        {
            has_viewer = true;
            if !viewer_actions.contains(&cap.action) {
                viewer_actions.push(cap.action.clone());
            }
        }
        let is_image_output = matches!(cap.output.as_str(), "image" | "rgba_image");
        if cap.workspace == "editor" || is_image_output {
            has_editor = true;
            if default_editor_action.is_none() {
                default_editor_action = Some(cap.action.clone());
            }
        }
        if cap.workspace == "browser" {
            has_browser = true;
            if default_browser_action.is_none() {
                default_browser_action = Some(cap.action.clone());
            }
        }
        for p in &cap.params {
            if seen_param_keys.insert(p.key.clone())
                && let Some(spec) = convert_jsonl_param(p)
            {
                param_specs.push(spec);
            }
        }
    }

    for qa in &raw.quick_actions {
        if qa.workspaces.iter().any(|w| w == "browser") {
            has_browser = true;
            if default_browser_action.is_none() {
                default_browser_action = Some(qa.action.clone());
            }
        }
        if qa.workspaces.iter().any(|w| w == "viewer") {
            has_viewer = true;
        }
    }

    if has_editor {
        caps.push(PluginCapability::EditorFilter);
    }
    if has_browser {
        caps.push(PluginCapability::BrowserAction);
    }
    if has_viewer {
        caps.push(PluginCapability::ViewerAction);
    }

    if raw.capabilities.len() > 1 && seen_param_keys.insert("action".to_owned()) {
        let options: Vec<String> = raw.capabilities.iter().map(|c| c.action.clone()).collect();
        let default = options.first().cloned().map(PluginParamValue::Select);
        param_specs.push(PluginParamSpec {
            key: "action".to_owned(),
            label: "Action".to_owned(),
            kind: PluginParamKind::Select,
            default,
            min: None,
            max: None,
            step: None,
            options,
            description: None,
        });
    }

    let runnable_models: Vec<&PluginModelSpec> = raw
        .models
        .iter()
        .filter(|m| {
            !m.file.ends_with(".csv")
                && !m.file.ends_with(".txt")
                && !m.file.ends_with(".traineddata")
        })
        .collect();
    if runnable_models.len() > 1 && seen_param_keys.insert("modelId".to_owned()) {
        let options: Vec<String> = runnable_models.iter().map(|m| m.id.clone()).collect();
        let qa_default = raw
            .quick_actions
            .iter()
            .find_map(|qa| qa.params.get("modelId").and_then(|v| v.as_str()))
            .map(str::to_owned);
        let default = qa_default
            .filter(|id| options.contains(id))
            .or_else(|| options.first().cloned())
            .map(PluginParamValue::Select);
        param_specs.push(PluginParamSpec {
            key: "modelId".to_owned(),
            label: "Model".to_owned(),
            kind: PluginParamKind::Select,
            default,
            min: None,
            max: None,
            step: None,
            options,
            description: None,
        });
    }

    let mut permissions = Vec::new();
    if raw.permissions.network {
        permissions.push("network".to_owned());
    }
    if raw.permissions.gpu {
        permissions.push("gpu".to_owned());
    }

    let description = if raw.description.trim().is_empty() {
        cap_labels.join(" · ")
    } else {
        raw.description
    };

    let mut hooks = raw.hooks;
    if hooks.is_empty() {
        let mut derived = BTreeSet::new();
        if caps.contains(&PluginCapability::BrowserAction) {
            derived.insert(plugin_hooks::BROWSER_CONTEXT_MENU.to_owned());
        }
        if caps.contains(&PluginCapability::ViewerAction) {
            derived.insert(plugin_hooks::VIEWER_CONTEXT_MENU.to_owned());
        }
        if caps.contains(&PluginCapability::EditorFilter) {
            derived.insert(plugin_hooks::EDITOR_PLUGINS_MENU.to_owned());
        }
        hooks = derived.into_iter().collect();
    }

    let manifest = PluginManifest {
        id: raw.id,
        name: raw.name,
        version: raw.version,
        description,
        author: raw.author,
        entry: raw.entry.command,
        args: raw.entry.args,
        capabilities: caps,
        params: param_specs,
        permissions,
        timeout_ms: raw.timeout_ms,
        order: raw.order,
        category: raw.category,
        hooks,
        labels: raw.labels,
    };

    let meta = JsonlStdioMeta {
        default_editor_action,
        default_browser_action,
        capability_outputs,
        viewer_actions,
        models: raw.models,
        quick_actions: raw.quick_actions,
    };

    Ok((
        manifest,
        entry_path,
        PluginBridgeProtocol::JsonlStdioV1(meta),
    ))
}

fn convert_jsonl_param(raw: &RawJsonlParamSpec) -> Option<PluginParamSpec> {
    let kind = match raw.kind.as_str() {
        "float" => PluginParamKind::Float,
        "int" => PluginParamKind::Int,
        "bool" => PluginParamKind::Bool,
        "select" => PluginParamKind::Select,
        "color" => PluginParamKind::Color,
        "string" | "text" => PluginParamKind::String,
        _ => return None,
    };

    let default = raw
        .default
        .as_ref()
        .and_then(|v| convert_param_default(kind, v));

    Some(PluginParamSpec {
        key: raw.key.clone(),
        label: if raw.label.trim().is_empty() {
            raw.key.clone()
        } else {
            raw.label.clone()
        },
        kind,
        default,
        min: raw.min,
        max: raw.max,
        step: raw.step,
        options: raw.options.clone(),
        description: raw.description.clone(),
    })
}

fn convert_param_default(
    kind: PluginParamKind,
    val: &serde_json::Value,
) -> Option<PluginParamValue> {
    match kind {
        PluginParamKind::Bool => val.as_bool().map(PluginParamValue::Bool),
        PluginParamKind::Int => val.as_i64().map(PluginParamValue::Int),
        PluginParamKind::Float => val.as_f64().map(PluginParamValue::Float),
        PluginParamKind::Select | PluginParamKind::String => {
            val.as_str().map(|s| PluginParamValue::Select(s.to_owned()))
        }
        PluginParamKind::Color => {
            let arr = val.as_array()?;
            if arr.len() != 3 {
                return None;
            }
            let r = u8::try_from(arr[0].as_u64()?).ok()?;
            let g = u8::try_from(arr[1].as_u64()?).ok()?;
            let b = u8::try_from(arr[2].as_u64()?).ok()?;
            Some(PluginParamValue::Color([r, g, b]))
        }
    }
}
