//! Authoritative serializable snapshots published by Core and inspected by UI/CLI/Agents.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AiModelStatus, AiQuickAction, AiSettings, AiTaskState, BatchState, Language, LayerSummary,
    LayoutState, PluginInfo, SavedTranslation, TextLayerData, Theme, TranslateSettings,
    TranslationResult, UiText, VectorLayerData, Workspace, default_one_f32,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_image: bool,
    pub size: u64,
    pub modified_ms: u64,
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Crumb {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SelectionInfo {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub size: u64,
    pub is_image: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ColorAdjustments {
    #[serde(default)]
    pub brightness: f32,
    #[serde(default)]
    pub contrast: f32,
    #[serde(default)]
    pub saturation: f32,
    #[serde(default)]
    pub exposure: f32,
    #[serde(default)]
    pub highlights: f32,
    #[serde(default)]
    pub shadows: f32,
    #[serde(default = "default_one_f32")]
    pub gamma: f32,
}

impl Default for ColorAdjustments {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            exposure: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            gamma: 1.0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditorState {
    #[serde(default)]
    pub id: u64,
    pub path: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub can_undo: bool,
    pub can_redo: bool,
    pub modified: bool,
    pub saved: bool,
    pub save_path: String,
    pub pixel_revision: u64,
    pub history: Vec<UiText>,
    #[serde(default)]
    pub redo_history: Vec<UiText>,
    #[serde(default)]
    pub history_trimmed: bool,
    #[serde(default)]
    pub layers: Vec<LayerSummary>,
    #[serde(default)]
    pub active_layer_id: u64,
    #[serde(default)]
    pub canvas_background: Option<[u8; 3]>,
    #[serde(default)]
    pub original_revision: u64,
    pub adjustments: ColorAdjustments,
    #[serde(default)]
    pub selection: EditorSelectionSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_vector_data: Option<VectorLayerData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_text_data: Option<TextLayerData>,
}

/// Compact summary of one open Editor document tab published in `UiStateMeta`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditorTabSummary {
    pub id: u64,
    pub path: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub modified: bool,
    pub active: bool,
}

/// Summary of a named selection channel saved in the Editor document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SavedSelectionSummary {
    pub id: u64,
    pub name: String,
}

/// Summary of the Editor's active pixel selection mask.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditorSelectionSummary {
    pub has_selection: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<[u32; 4]>,
    pub quick_mask_active: bool,
    pub can_reselect: bool,
    pub selection_revision: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub saved_selections: Vec<SavedSelectionSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub task_id: String,
    pub kind: String,
    pub status: TaskRunStatus,
    pub percent: u8,
    pub stage: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskRunStatus {
    Running,
    Completed,
    Cancelled,
    Failed,
}

/// Active paged archive (`.cbz`/`.zip`) or `.pdf` session metadata for Viewer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveViewerSessionMeta {
    pub container_path: String,
    pub kind: String,
    pub page_count: usize,
    pub page_labels: Vec<String>,
}

/// Metadata published when Editor is switched to an unsupported or undecodable file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnsupportedEditorFile {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    pub reason: String,
}

/// Lightweight state snapshot excluding folder listing (`entries`, `viewerImages`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiStateMeta {
    pub revision: u64,
    pub listing_epoch: u64,
    pub tree_revision: u64,
    pub workspace: Workspace,
    pub theme: Theme,
    pub language: Language,
    pub current_folder: Option<String>,
    pub root_folder: Option<String>,
    pub breadcrumbs: Vec<Crumb>,
    pub selection: Vec<String>,
    pub selection_info: Option<SelectionInfo>,
    pub layout: LayoutState,
    pub remember_last_folder: bool,
    #[serde(default = "crate::default_true")]
    pub save_ui_state_on_exit: bool,
    pub hardware_acceleration: bool,
    #[serde(default)]
    pub backup_method: crate::BackupMethod,
    pub viewer_index: Option<usize>,
    pub viewer_path: Option<String>,
    pub viewer_count: usize,
    pub editor: Option<EditorState>,
    #[serde(default)]
    pub editor_tabs: Vec<EditorTabSummary>,
    #[serde(default)]
    pub active_editor_tab_id: Option<u64>,
    #[serde(default)]
    pub focus_request_seq: u64,
    #[serde(default)]
    pub pending_recovery_count: usize,
    pub status: UiText,
    pub safe_mode: bool,
    pub active_tasks: Vec<TaskSnapshot>,
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub batch: Option<BatchState>,
    #[serde(default)]
    pub translation: Option<TranslationResult>,
    #[serde(default)]
    pub viewer_saved_translation: Option<SavedTranslation>,
    #[serde(default)]
    pub translate_settings: TranslateSettings,
    #[serde(default)]
    pub plugins: Vec<PluginInfo>,
    #[serde(default)]
    pub ai_settings: AiSettings,
    #[serde(default)]
    pub ai_models: Vec<AiModelStatus>,
    #[serde(default)]
    pub quick_actions: Vec<AiQuickAction>,
    #[serde(default)]
    pub disabled_quick_action_plugins: Vec<String>,
    #[serde(default)]
    pub viewer_single_instance: bool,
    #[serde(default)]
    pub ai_task: Option<AiTaskState>,
    #[serde(default)]
    pub editor_caption: Option<String>,
    #[serde(default)]
    pub has_pixel_clipboard: bool,
    #[serde(default)]
    pub viewer_archive: Option<ArchiveViewerSessionMeta>,
    #[serde(default)]
    pub editor_unsupported: Option<UnsupportedEditorFile>,
    #[serde(default)]
    pub editor_source_is_virtual: bool,
}

/// Full state snapshot returned by `Query::State`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiStateSnapshot {
    #[serde(flatten)]
    pub meta: UiStateMeta,
    pub entries: Vec<Entry>,
    pub viewer_images: Vec<String>,
}

/// Runtime status payload returned by `Query::Status`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub app_id: String,
    pub display_name: String,
    pub profile: String,
    pub version: String,
    pub protocol_version: u32,
    pub boot_id: String,
    pub pid: u32,
    pub headless: bool,
    pub uptime_ms: u64,
    pub revision: u64,
    pub workspace: Workspace,
    pub active_tasks: usize,
    pub journal_path: String,
    pub log_dir: String,
    pub socket_path: String,
}

/// Semantic widget descriptor exposed by `iced` UI adapter for `debug ui-tree`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SemanticWidgetNode {
    pub id: String,
    pub role: String,
    pub label: String,
    pub workspace: Workspace,
    pub enabled: bool,
    pub visible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<WidgetRect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PresentationState {
    pub window_count: usize,
    pub window_width: f32,
    pub window_height: f32,
    pub scale_factor: f32,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub gpu_backend: String,
    pub gpu_adapter: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiTreeSnapshot {
    pub revision: u64,
    pub workspace: Workspace,
    pub presentation: PresentationState,
    pub widgets: Vec<SemanticWidgetNode>,
}
