//! AI settings, on-demand model catalog, quick actions, and async AI task state types.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::TranslationResult;

/// Execution device preference for local AI plugins.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AiDevice {
    #[default]
    Cpu,
    Auto,
    Gpu,
}

impl AiDevice {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Auto => "auto",
            Self::Gpu => "gpu",
        }
    }
}

/// Persisted AI plugin preferences stored in `config.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct AiSettings {
    pub device: AiDevice,
    /// Seconds of idle time before shutting down a spawned worker (`10..=600`).
    pub auto_unload_secs: u32,
    /// Optional user-supplied local `.onnx` path overrides keyed by `model_id`.
    pub custom_model_paths: BTreeMap<String, String>,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            device: AiDevice::Cpu,
            auto_unload_secs: 60,
            custom_model_paths: BTreeMap::new(),
        }
    }
}

impl AiSettings {
    pub fn sanitized(mut self) -> Self {
        self.auto_unload_secs = self.auto_unload_secs.clamp(10, 600);
        self.custom_model_paths
            .retain(|k, v| !k.trim().is_empty() && !v.trim().is_empty());
        self
    }
}

/// One downloadable/selectable AI model or language pack declared by a plugin manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginModelSpec {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub tier: String,
    pub file: String,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub url: String,
}

/// One context-menu Quick Action contributed by a plugin's `manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginQuickAction {
    pub id: String,
    #[serde(default)]
    pub label: String,
    pub action: String,
    #[serde(default)]
    pub workspaces: Vec<String>,
    #[serde(default)]
    pub slot: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Live Quick Action published in `UiStateMeta` for Browser/Viewer context menus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiQuickAction {
    pub plugin_id: String,
    pub id: String,
    pub label: String,
    pub action: String,
    pub workspaces: Vec<String>,
    #[serde(default)]
    pub slot: String,
    pub params: serde_json::Value,
    pub available: bool,
}

/// Live installation status of one AI model or OCR language pack in `UiStateMeta`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiModelStatus {
    pub id: String,
    pub name: String,
    pub tier: String,
    pub size_bytes: u64,
    pub installed: bool,
    pub custom_path: Option<String>,
    pub effective_path: Option<String>,
}

/// One freehand brush stroke in document coordinates for alpha refinement or AI inpainting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiMaskStroke {
    pub radius: f32,
    pub points: Vec<[f32; 2]>,
}

/// Active or most-recent AI background job (model download, inference, OCR/translate, caption).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiTaskState {
    /// `"download"` | `"inference"` | `"translate"` | `"caption"` | `""`
    pub kind: String,
    pub target: String,
    pub running: bool,
    pub percent: u8,
    pub stage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clipboard_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<TranslationResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_path: Option<String>,
}
