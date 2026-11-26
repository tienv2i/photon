//! Synchronous Core state owner and command execution handlers.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, UNIX_EPOCH};

use image::{GrayImage, ImageEncoder, Rgba, RgbaImage};
use photon_image::archive::{ArchiveKind, ArchiveSession, is_archive_path, sanitize_zip_filename};
use photon_image::{EditOp, EditorDocument, ImageError, LayerOperation, SelectionMask};
use photon_plugins::{
    PluginError, PluginOutputMode, PluginRegistry, resolve_plugins_dir, run_browser_plugin,
    run_editor_plugin,
};
use photon_protocol::{
    AiTaskState, ApplyTarget, ArchiveViewerSessionMeta, BatchSource, BatchState, ColorAdjustments,
    ColorBalanceAdjustment, Command, CommandResponse, CommandStatus, Crumb, CurvesAdjustment,
    EditorSelectionSummary, EditorState, EditorTabSummary, Entry, HsvAdjustment, IMAGE_EXTENSIONS,
    PluginCapability, PluginParamValue, ProtocolError, Query, RuntimeStatus, SavedTranslation,
    SelectionInfo, TaskRunStatus, TaskSnapshot, TranslationResult, UiStateMeta, UiStateSnapshot,
    UiText, UiTreeSnapshot, UnsupportedEditorFile, Workspace, protocol_schemas,
};
use photon_storage::journal::now_ms;
use photon_storage::logging::current_boot_id;
use photon_storage::{AppIdentity, AppPaths, Config, Journal, UiState};

use crate::ai::{self, PendingAiJob};
use crate::batch::BatchPlan;
use crate::file_ops;
use crate::translate;

/// In-memory Editor pixel clipboard backing Cut/Copy/Paste and `Query::EditorClipboardPng`.
#[derive(Debug, Clone)]
pub struct EditorPixelClipboard {
    pub width: u32,
    pub height: u32,
    pub offset_x: i32,
    pub offset_y: i32,
    pub rgba: Arc<Vec<u8>>,
    pub png: Arc<Vec<u8>>,
}

pub(crate) struct EditorTabSlot {
    pub(crate) id: u64,
    pub(crate) doc: Option<EditorDocument>,
    pub(crate) caption: Option<String>,
    pub(crate) source_is_virtual: bool,
}

pub struct Core {
    pub identity: AppIdentity,
    pub paths: AppPaths,
    pub config: Config,
    pub ui_state: UiState,
    pub(crate) ui_state_dirty: bool,
    pub journal: Journal,
    pub headless: bool,
    pub safe_mode: bool,
    pub viewer_ephemeral: bool,
    pub(crate) started_at: Instant,
    pub(crate) revision: u64,
    listing_epoch: u64,
    tree_revision: u64,
    workspace: Workspace,
    current_folder: Option<PathBuf>,
    root_folder: Option<PathBuf>,
    entries: Vec<Entry>,
    selection: Vec<String>,
    selection_info: Option<SelectionInfo>,
    pub(crate) viewer_images: Vec<String>,
    pub(crate) viewer_index: Option<usize>,
    pub(crate) archive_session: Arc<Mutex<Option<ArchiveSession>>>,
    pub(crate) viewer_archive: Option<ArchiveViewerSessionMeta>,
    pub(crate) editor: Option<EditorDocument>,
    pub(crate) editor_unsupported: Option<UnsupportedEditorFile>,
    pub(crate) editor_source_is_virtual: bool,
    editor_tabs: Vec<EditorTabSlot>,
    active_editor_tab_id: Option<u64>,
    next_editor_tab_id: u64,
    focus_request_seq: u64,
    next_original_revision: u64,
    pixel_clipboard: Option<EditorPixelClipboard>,
    pub(crate) plugin_registry: PluginRegistry,
    pub(crate) ai_task: Option<AiTaskState>,
    pub(crate) ai_cancel: Option<Arc<AtomicBool>>,
    pub(crate) pending_ai: Option<PendingAiJob>,
    pub(crate) ai_generation: u64,
    pub(crate) ai_temp_input: Option<PathBuf>,
    pub(crate) ai_temp_output: Option<PathBuf>,
    pub(crate) ai_quick_action_target: Option<(String, String, PathBuf)>,
    pub(crate) ai_apply_target: ApplyTarget,
    pub(crate) ai_plugin_label: Option<String>,
    pub(crate) ai_active_task_id: Option<String>,
    pub(crate) latest_translation: Option<TranslationResult>,
    pub(crate) viewer_saved_translation: Option<SavedTranslation>,
    pub(crate) editor_caption: Option<String>,
    batch: Option<BatchState>,
    batch_cancel: Option<Arc<AtomicBool>>,
    pending_batch: Option<(BatchPlan, Arc<AtomicBool>)>,
    pub(crate) next_job_seq: u64,
    pub(crate) status: UiText,
    pub(crate) tasks: Vec<TaskSnapshot>,
    ui_tree: Option<UiTreeSnapshot>,
    idempotency_cache: HashMap<String, CommandResponse>,
    idempotency_order: VecDeque<String>,
}

impl Core {
    pub fn new(headless: bool, safe_mode: bool) -> Self {
        let identity = AppIdentity::current();
        let paths = AppPaths::for_identity(&identity);
        Self::with_paths(identity, paths, headless, safe_mode)
    }

    pub fn new_viewer_ephemeral(headless: bool, safe_mode: bool) -> Self {
        let identity = AppIdentity::current();
        let paths = AppPaths::for_identity(&identity);
        Self::with_paths_mode(identity, paths, headless, safe_mode, true)
    }

    pub fn for_sandbox(sandbox_root: &Path) -> Self {
        let identity =
            AppIdentity::for_profile(photon_storage::Profile::Sandbox(sandbox_root.to_path_buf()));
        let paths = AppPaths::for_sandbox(sandbox_root);
        Self::with_paths(identity, paths, true, false)
    }

    pub fn with_paths(
        identity: AppIdentity,
        paths: AppPaths,
        headless: bool,
        safe_mode: bool,
    ) -> Self {
        Self::with_paths_mode(identity, paths, headless, safe_mode, false)
    }

    fn with_paths_mode(
        identity: AppIdentity,
        paths: AppPaths,
        headless: bool,
        safe_mode: bool,
        viewer_ephemeral: bool,
    ) -> Self {
        let _ = paths.ensure_runtime_dirs();
        let (mut config, ui_state) = if safe_mode {
            let mut cfg = Config::factory_default();
            cfg.hardware_acceleration = false;
            cfg.remember_last_folder = false;
            (cfg, UiState::default())
        } else {
            let (cfg, warn) = Config::load_or_create(&paths.config_file());
            if let Some(warning) = warn {
                tracing::warn!(target: "photon::config", %warning, "config warning");
            }
            let ui = if viewer_ephemeral {
                UiState::default()
            } else {
                let (ui, ui_warn) = UiState::load_or_migrate(&paths.ui_state_file(), &cfg);
                if let Some(warning) = ui_warn {
                    tracing::warn!(target: "photon::config", %warning, "ui_state warning");
                }
                ui
            };
            (cfg, ui)
        };
        if !viewer_ephemeral && !safe_mode {
            let mut merged_layout = ui_state.layout.clone();
            merged_layout.apply_settings_from(&config.layout);
            config.layout = merged_layout;
        }
        let journal = Journal::new(&paths.journal_dir, current_boot_id());
        let plugins_dir = resolve_plugins_dir(paths.data_dir.join("plugins"));
        let plugin_registry = match PluginRegistry::discover(&plugins_dir) {
            Ok(reg) => {
                for err in reg.errors() {
                    tracing::warn!(
                        target: "photon::plugins",
                        dir = %err.dir.display(),
                        error = %err.message,
                        "skipped invalid plugin"
                    );
                }
                reg
            }
            Err(err) => {
                tracing::warn!(target: "photon::plugins", %err, "failed to scan plugins dir");
                PluginRegistry::default()
            }
        };
        let status = if safe_mode {
            UiText::new("safe_mode_active")
        } else {
            UiText::new("ready")
        };

        let mut core = Self {
            identity,
            paths,
            config,
            ui_state,
            ui_state_dirty: false,
            journal,
            headless,
            safe_mode,
            viewer_ephemeral,
            started_at: Instant::now(),
            revision: 1,
            listing_epoch: 1,
            tree_revision: 1,
            workspace: Workspace::Browser,
            current_folder: None,
            root_folder: None,
            entries: Vec::new(),
            selection: Vec::new(),
            selection_info: None,
            viewer_images: Vec::new(),
            viewer_index: None,
            archive_session: Arc::new(Mutex::new(None)),
            viewer_archive: None,
            editor: None,
            editor_unsupported: None,
            editor_source_is_virtual: false,
            editor_tabs: Vec::new(),
            active_editor_tab_id: None,
            next_editor_tab_id: 1,
            focus_request_seq: 0,
            next_original_revision: 1,
            pixel_clipboard: None,
            plugin_registry,
            ai_task: None,
            ai_cancel: None,
            pending_ai: None,
            ai_generation: 0,
            ai_temp_input: None,
            ai_temp_output: None,
            ai_quick_action_target: None,
            ai_apply_target: ApplyTarget::WholeImage,
            ai_plugin_label: None,
            ai_active_task_id: None,
            latest_translation: None,
            viewer_saved_translation: None,
            editor_caption: None,
            batch: None,
            batch_cancel: None,
            pending_batch: None,
            next_job_seq: 1,
            status,
            tasks: Vec::new(),
            ui_tree: None,
            idempotency_cache: HashMap::new(),
            idempotency_order: VecDeque::new(),
        };

        // Seed default Quick Access places on first launch so users can immediately
        // reorder / remove them, without them reappearing if the list is later cleared.
        if !core.config.favorites_initialized && core.config.favorites.is_empty() {
            let home = photon_storage::paths::home_dir();
            // Always add the home directory.
            let home_str = home.to_string_lossy().into_owned();
            core.config.favorites.push(home_str);
            // Add Pictures, Downloads, Desktop only if they exist.
            for name in &["Pictures", "Downloads", "Desktop"] {
                let dir = home.join(name);
                if dir.is_dir() {
                    core.config
                        .favorites
                        .push(dir.to_string_lossy().into_owned());
                }
            }
            core.config.favorites_initialized = true;
            if !safe_mode && !viewer_ephemeral {
                core.persist_config();
            }
        }

        if !safe_mode
            && !viewer_ephemeral
            && core.config.remember_last_folder
            && let Some(folder) = core.ui_state.last_folder.clone()
        {
            let path = PathBuf::from(folder);
            if path.is_dir() && core.open_folder(&path).is_ok() {
                core.root_folder = Some(path);
            }
        }
        core
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn listing_epoch(&self) -> u64 {
        self.listing_epoch
    }

    pub fn workspace(&self) -> Workspace {
        self.workspace
    }

    pub fn editor_document(&self) -> Option<&EditorDocument> {
        self.editor.as_ref()
    }

    pub fn take_pending_batch(&mut self) -> Option<(BatchPlan, Arc<AtomicBool>)> {
        self.pending_batch.take()
    }

    pub fn apply_batch_progress(
        &mut self,
        done: usize,
        failed: usize,
        current: String,
    ) -> Option<TaskSnapshot> {
        let state = self.batch.as_mut()?;
        state.done = done;
        state.failed = failed;
        state.current = current.clone();
        let total = state.total.max(1);
        let pct = (((done + failed) * 100) / total).min(99) as u8;
        let job_id = state.job_id.clone();
        if let Some(task) = self.tasks.iter_mut().find(|t| t.task_id == job_id) {
            task.percent = pct;
            task.stage = current;
            return Some(task.clone());
        }
        None
    }

    pub fn apply_batch_finished(
        &mut self,
        done: usize,
        failed: usize,
        cancelled: bool,
        message: UiText,
    ) -> Option<TaskSnapshot> {
        self.batch_cancel = None;
        let job_id = {
            let state = self.batch.as_mut()?;
            state.done = done;
            state.failed = failed;
            state.running = false;
            state.cancelled = cancelled;
            state.current.clear();
            state.job_id.clone()
        };
        self.status = message;
        self.revision += 1;
        if done > 0
            && let Some(dir) = self.current_folder.clone()
        {
            let _ = self.open_folder(&dir);
        }
        if let Some(task) = self.tasks.iter_mut().find(|t| t.task_id == job_id) {
            task.status = if cancelled {
                TaskRunStatus::Cancelled
            } else {
                TaskRunStatus::Completed
            };
            task.percent = 100;
            task.stage = self.status.english();
            return Some(task.clone());
        }
        None
    }

    pub fn set_ui_tree(&mut self, tree: UiTreeSnapshot) {
        self.ui_tree = Some(tree);
    }

    pub fn snapshot(&self) -> UiStateSnapshot {
        UiStateSnapshot {
            meta: self.meta(),
            entries: self.entries.clone(),
            viewer_images: self.viewer_images.clone(),
        }
    }

    pub fn meta(&self) -> UiStateMeta {
        let current_folder_str = self
            .current_folder
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        let root_folder_str = self
            .root_folder
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        let breadcrumbs = self
            .current_folder
            .as_deref()
            .map(build_breadcrumbs)
            .unwrap_or_default();
        let viewer_path = self
            .viewer_index
            .and_then(|idx| self.viewer_images.get(idx))
            .cloned();

        let editor_tabs: Vec<EditorTabSummary> = self
            .editor_tabs
            .iter()
            .filter_map(|slot| {
                let is_active = Some(slot.id) == self.active_editor_tab_id;
                let doc = if is_active {
                    self.editor.as_ref().or(slot.doc.as_ref())
                } else {
                    slot.doc.as_ref()
                }?;
                let (w, h) = doc.dimensions();
                Some(EditorTabSummary {
                    id: slot.id,
                    path: doc.source_path.to_string_lossy().into_owned(),
                    name: doc
                        .source_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .to_owned(),
                    width: w,
                    height: h,
                    modified: doc.modified(),
                    active: is_active,
                })
            })
            .collect();

        UiStateMeta {
            revision: self.revision,
            listing_epoch: self.listing_epoch,
            tree_revision: self.tree_revision,
            workspace: self.workspace,
            theme: self.config.theme,
            language: self.config.language,
            current_folder: current_folder_str,
            root_folder: root_folder_str,
            breadcrumbs,
            selection: self.selection.clone(),
            selection_info: self.selection_info.clone(),
            layout: self.config.layout.clone(),
            remember_last_folder: self.config.remember_last_folder,
            save_ui_state_on_exit: self.config.save_ui_state_on_exit,
            hardware_acceleration: self.config.hardware_acceleration,
            backup_method: self.config.backup_method,
            viewer_index: self.viewer_index,
            viewer_path,
            viewer_count: self.viewer_images.len(),
            editor: self.editor.as_ref().map(|doc| {
                let (w, h) = doc.dimensions();
                EditorState {
                    id: self.active_editor_tab_id.unwrap_or(0),
                    path: doc.source_path.to_string_lossy().into_owned(),
                    name: doc
                        .source_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .to_owned(),
                    width: w,
                    height: h,
                    can_undo: doc.can_undo(),
                    can_redo: doc.can_redo(),
                    modified: doc.modified(),
                    saved: !doc.modified(),
                    save_path: doc.save_path.to_string_lossy().into_owned(),
                    pixel_revision: doc.pixel_revision,
                    history: doc.history_labels(),
                    redo_history: doc.redo_labels(),
                    history_trimmed: doc.history_trimmed,
                    layers: doc.stack().summaries(),
                    active_layer_id: doc.stack().active_layer_id,
                    canvas_background: doc.stack().background,
                    original_revision: doc.original_revision,
                    adjustments: doc.latest_color_adjustments(),
                    selection: EditorSelectionSummary {
                        has_selection: doc.selection().has_selection(),
                        bounds: doc.selection().bounds,
                        quick_mask_active: doc.selection().quick_mask_active,
                        can_reselect: doc.selection().can_reselect(),
                        selection_revision: doc.selection_revision,
                    },
                }
            }),
            editor_tabs,
            active_editor_tab_id: self.active_editor_tab_id,
            focus_request_seq: self.focus_request_seq,
            status: self.status.clone(),
            safe_mode: self.safe_mode,
            active_tasks: self
                .tasks
                .iter()
                .filter(|t| t.status == TaskRunStatus::Running)
                .cloned()
                .collect(),
            favorites: self.config.favorites.clone(),
            batch: self.batch.clone(),
            translation: self.latest_translation.clone(),
            viewer_saved_translation: self.viewer_saved_translation.clone(),
            translate_settings: self.config.translate.clone(),
            plugins: self
                .plugin_registry
                .plugin_infos(&self.config.disabled_plugins),
            ai_settings: self.config.ai.clone(),
            ai_models: self
                .plugin_registry
                .model_statuses(&self.effective_models_dir(), &self.config.ai),
            quick_actions: self
                .plugin_registry
                .quick_actions(&self.config.disabled_plugins),
            ai_task: self.ai_task.clone(),
            editor_caption: self.editor_caption.clone(),
            has_pixel_clipboard: self.pixel_clipboard.is_some(),
            viewer_archive: self.viewer_archive.clone(),
            editor_unsupported: self.editor_unsupported.clone(),
            editor_source_is_virtual: self.editor_source_is_virtual,
        }
    }

    pub fn runtime_status(&self) -> RuntimeStatus {
        RuntimeStatus {
            app_id: self.identity.app_id.to_owned(),
            display_name: self.identity.display_name.to_owned(),
            profile: self.identity.profile.as_str().to_owned(),
            version: self.identity.version.to_owned(),
            protocol_version: self.identity.protocol_version,
            boot_id: current_boot_id().to_owned(),
            pid: std::process::id(),
            headless: self.headless,
            uptime_ms: self.started_at.elapsed().as_millis() as u64,
            revision: self.revision,
            workspace: self.workspace,
            active_tasks: self
                .tasks
                .iter()
                .filter(|t| t.status == TaskRunStatus::Running)
                .count(),
            journal_path: self.paths.journal_file().to_string_lossy().into_owned(),
            log_dir: self.paths.log_dir.to_string_lossy().into_owned(),
            socket_path: self.paths.socket_path.to_string_lossy().into_owned(),
        }
    }

    pub fn query(&self, query: &Query) -> Result<serde_json::Value, ProtocolError> {
        match query {
            Query::State => Ok(serde_json::to_value(self.snapshot()).unwrap_or_default()),
            Query::Meta => Ok(serde_json::to_value(self.meta()).unwrap_or_default()),
            Query::Status => Ok(serde_json::to_value(self.runtime_status()).unwrap_or_default()),
            Query::ListFolders { path } => {
                let dir = PathBuf::from(path);
                if !dir.is_dir() {
                    return Err(ProtocolError::new(
                        "not_found",
                        format!("{} is not a directory", dir.display()),
                    ));
                }
                let mut folders = Vec::new();
                let rd = fs::read_dir(&dir)
                    .map_err(|e| ProtocolError::new("io_error", e.to_string()))?;
                for entry in rd.flatten() {
                    let p = entry.path();
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') {
                        continue;
                    }
                    if p.is_dir() {
                        folders.push(serde_json::json!({
                            "name": name,
                            "path": p.to_string_lossy(),
                        }));
                    }
                }
                folders.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
                Ok(serde_json::json!({ "folders": folders }))
            }
            Query::Tasks => Ok(serde_json::json!({ "tasks": self.tasks })),
            Query::UiTree => Ok(serde_json::to_value(&self.ui_tree).unwrap_or_default()),
            Query::Schema => Ok(protocol_schemas()),
            Query::EditorClipboardPng => match &self.pixel_clipboard {
                Some(cb) => Ok(serde_json::json!({
                    "hasClipboard": true,
                    "width": cb.width,
                    "height": cb.height,
                    "offsetX": cb.offset_x,
                    "offsetY": cb.offset_y,
                    "pngBytes": cb.png.as_ref(),
                })),
                None => Ok(serde_json::json!({
                    "hasClipboard": false,
                    "pngBytes": serde_json::Value::Null,
                })),
            },
        }
    }

    pub fn dispatch(&mut self, envelope: photon_protocol::Envelope) -> CommandResponse {
        if let Some(key) = &envelope.idempotency_key
            && let Some(cached) = self.idempotency_cache.get(key)
        {
            return cached.clone();
        }

        if let Some(expected) = envelope.expected_revision
            && expected != self.revision
        {
            let err = ProtocolError::new(
                "revision_conflict",
                format!("expected revision {expected}, current is {}", self.revision),
            );
            let res: Result<Option<UiText>, ProtocolError> = Err(err.clone());
            self.journal.record(
                &envelope.id,
                envelope.source,
                envelope.expected_revision,
                &envelope.command,
                self.revision,
                &res,
            );
            return CommandResponse {
                id: envelope.id,
                ok: false,
                status: CommandStatus::Failed,
                revision: self.revision,
                task_id: None,
                message: None,
                ui: None,
                data: None,
                error: Some(err),
            };
        }

        let _span = tracing::info_span!(
            "photon::command",
            request_id = %envelope.id,
            command = envelope.command.name(),
            source = ?envelope.source
        )
        .entered();

        let outcome = self.execute(&envelope.command);
        let response = match outcome {
            Ok((line, data)) => {
                self.revision += 1;
                if let Some(ref ui) = line {
                    self.status = ui.clone();
                }
                let res_for_journal: Result<Option<UiText>, ProtocolError> = Ok(line.clone());
                self.journal.record(
                    &envelope.id,
                    envelope.source,
                    envelope.expected_revision,
                    &envelope.command,
                    self.revision,
                    &res_for_journal,
                );
                tracing::debug!(
                    target: "photon::command",
                    request_id = %envelope.id,
                    command = envelope.command.name(),
                    revision = self.revision,
                    "command completed"
                );
                let async_task_id = self.pending_batch.as_ref().map(|(p, _)| p.job_id.clone());
                let status = if async_task_id.is_some() {
                    CommandStatus::Accepted
                } else {
                    CommandStatus::Completed
                };
                CommandResponse {
                    id: envelope.id,
                    ok: true,
                    status,
                    revision: self.revision,
                    task_id: async_task_id,
                    message: line.as_ref().map(UiText::english),
                    ui: line,
                    data,
                    error: None,
                }
            }
            Err(err) => {
                let res_for_journal: Result<Option<UiText>, ProtocolError> = Err(err.clone());
                self.journal.record(
                    &envelope.id,
                    envelope.source,
                    envelope.expected_revision,
                    &envelope.command,
                    self.revision,
                    &res_for_journal,
                );
                tracing::warn!(
                    target: "photon::command",
                    request_id = %envelope.id,
                    command = envelope.command.name(),
                    code = %err.code,
                    message = %err.message,
                    "command rejected"
                );
                CommandResponse {
                    id: envelope.id,
                    ok: false,
                    status: CommandStatus::Failed,
                    revision: self.revision,
                    task_id: None,
                    message: None,
                    ui: err.ui.clone(),
                    data: None,
                    error: Some(err),
                }
            }
        };

        if let Some(key) = envelope.idempotency_key {
            if self.idempotency_order.len() >= 128
                && let Some(old) = self.idempotency_order.pop_front()
            {
                self.idempotency_cache.remove(&old);
            }
            self.idempotency_order.push_back(key.clone());
            self.idempotency_cache.insert(key, response.clone());
        }

        response
    }

    fn execute(
        &mut self,
        command: &Command,
    ) -> Result<(Option<UiText>, Option<serde_json::Value>), ProtocolError> {
        match command {
            Command::OpenFolder { path } => {
                let p = PathBuf::from(path);
                if p.is_file() && is_archive_path(&p) {
                    let text = self.open_archive_container(&p)?;
                    return Ok((Some(text), None));
                }
                let text = self.open_folder(&p)?;
                if self.root_folder.as_ref() != Some(&p) {
                    self.root_folder = Some(p.clone());
                    self.tree_revision += 1;
                }
                self.workspace = Workspace::Browser;
                self.editor_unsupported = None;
                self.sync_session_prefs();
                Ok((Some(text), None))
            }
            Command::OpenArchive { path } => {
                let text = self.open_archive_container(Path::new(path))?;
                Ok((Some(text), None))
            }
            Command::Navigate { path } => {
                let p = PathBuf::from(path);
                let text = self.open_folder(&p)?;
                if self
                    .root_folder
                    .as_ref()
                    .is_none_or(|root| !p.starts_with(root))
                {
                    self.root_folder = Some(p.clone());
                    self.tree_revision += 1;
                }
                self.workspace = Workspace::Browser;
                self.sync_session_prefs();
                Ok((Some(text), None))
            }
            Command::Refresh => {
                self.tree_revision += 1;
                if let Some(dir) = self.current_folder.clone() {
                    self.open_folder(&dir)?;
                }
                Ok((Some(UiText::new("refreshed")), None))
            }
            Command::OpenFiles { paths } => {
                if paths.is_empty() {
                    return Err(ProtocolError::new(
                        "invalid_argument",
                        "open_files requires at least one file path",
                    ));
                }
                if paths.len() == 1 {
                    let p = PathBuf::from(&paths[0]);
                    if p.is_file() && is_archive_path(&p) {
                        let text = self.open_archive_container(&p)?;
                        return Ok((Some(text), None));
                    }
                }
                let mut valid = Vec::new();
                for raw in paths {
                    let p = PathBuf::from(raw);
                    if !p.is_file() {
                        return Err(ProtocolError::new(
                            "not_found",
                            format!("file not found: {}", p.display()),
                        ));
                    }
                    valid.push(p.to_string_lossy().into_owned());
                }
                self.clear_archive_session();
                if let Some(first) = PathBuf::from(&valid[0]).parent()
                    && first.is_dir()
                    && valid.len() == 1
                {
                    let _ = self.open_folder(first);
                    if self.root_folder.is_none() {
                        self.root_folder = Some(first.to_path_buf());
                        self.tree_revision += 1;
                    }
                    if let Some(pos) = self.viewer_images.iter().position(|x| x == &valid[0]) {
                        self.viewer_index = Some(pos);
                    } else {
                        self.viewer_images = valid.clone();
                        self.viewer_index = Some(0);
                    }
                } else {
                    self.viewer_images = valid.clone();
                    self.viewer_index = Some(0);
                    self.listing_epoch += 1;
                }
                self.workspace = Workspace::Viewer;
                self.editor_unsupported = None;
                self.sync_viewer_saved_translation();
                self.sync_session_prefs();
                let text = self.current_viewer_status();
                Ok((Some(text), None))
            }
            Command::Select { paths } => {
                self.selection = paths.clone();
                self.selection_info = paths
                    .first()
                    .and_then(|p| build_selection_info(Path::new(p)));
                Ok((None, None))
            }
            Command::SetWorkspace { workspace } => match workspace {
                Workspace::Browser => {
                    self.workspace = Workspace::Browser;
                    self.editor_unsupported = None;
                    self.sync_session_prefs();
                    Ok((None, None))
                }
                Workspace::Viewer => {
                    if self.viewer_images.is_empty() {
                        return Err(ProtocolError::line(
                            "no_images",
                            UiText::new("viewer_no_images"),
                        ));
                    }
                    if self.viewer_index.is_none() {
                        self.viewer_index = Some(0);
                    }
                    self.workspace = Workspace::Viewer;
                    self.editor_unsupported = None;
                    self.sync_session_prefs();
                    Ok((None, None))
                }
                Workspace::Editor => {
                    if self.editor.is_none() {
                        if let Some(first_id) = self.editor_tabs.first().map(|s| s.id) {
                            let _ = self.activate_editor_tab(first_id);
                            self.editor_unsupported = None;
                        } else {
                            let path = self
                                .viewer_index
                                .and_then(|i| self.viewer_images.get(i))
                                .cloned()
                                .ok_or_else(no_doc_err)?;
                            let _ = Self::ensure_archive_page_for_path(
                                &self.archive_session,
                                Path::new(&path),
                            );
                            if let Err(err) = self.open_editor_path(Path::new(&path)) {
                                self.set_editor_unsupported(&path, err.message);
                            } else {
                                self.editor_unsupported = None;
                            }
                        }
                    } else {
                        self.editor_unsupported = None;
                    }
                    self.workspace = Workspace::Editor;
                    self.sync_session_prefs();
                    Ok((None, None))
                }
            },
            Command::WorkspaceBack => {
                self.workspace = match self.workspace {
                    Workspace::Editor => {
                        self.editor_unsupported = None;
                        if self.viewer_images.is_empty() {
                            Workspace::Browser
                        } else {
                            Workspace::Viewer
                        }
                    }
                    Workspace::Viewer | Workspace::Browser => Workspace::Browser,
                };
                self.sync_session_prefs();
                Ok((None, None))
            }
            Command::SetTheme { theme } => {
                self.config.theme = *theme;
                self.persist_config();
                Ok((
                    Some(UiText::with("theme_set", [("theme", theme.as_str())])),
                    None,
                ))
            }
            Command::SetBackupMethod { method } => {
                self.config.backup_method = *method;
                self.persist_config();
                Ok((None, None))
            }
            Command::ToggleTheme => {
                let next = self.config.theme.toggled();
                self.config.theme = next;
                self.persist_config();
                Ok((
                    Some(UiText::with("theme_set", [("theme", next.as_str())])),
                    None,
                ))
            }
            Command::SetLanguage { language } => {
                self.config.language = *language;
                self.persist_config();
                Ok((
                    Some(UiText::with(
                        "language_set",
                        [("language", language.as_str())],
                    )),
                    None,
                ))
            }
            Command::SetLayout { layout } => {
                let prev_sort = (
                    self.config.layout.browser_sort_field,
                    self.config.layout.browser_sort_order,
                );
                let next_layout = layout.clone().sanitized();
                let settings_changed = !self.config.layout.settings_fields_eq(&next_layout);
                self.config.layout = next_layout.clone();
                self.ui_state.layout = next_layout;
                self.ui_state_dirty = true;
                if prev_sort
                    != (
                        self.config.layout.browser_sort_field,
                        self.config.layout.browser_sort_order,
                    )
                    && !self.entries.is_empty()
                {
                    file_ops::sort_entries(
                        &mut self.entries,
                        self.config.layout.browser_sort_field,
                        self.config.layout.browser_sort_order,
                    );
                    self.listing_epoch += 1;
                }
                if settings_changed {
                    self.persist_config();
                }
                Ok((Some(UiText::new("layout_saved")), None))
            }
            Command::ToggleLeftPanel => {
                let key = match self.workspace {
                    Workspace::Browser => "browser.sidebar",
                    Workspace::Viewer => "viewer.sidebar",
                    Workspace::Editor => "editor.tools",
                };
                let cur = self.config.layout.panel(key);
                self.config.layout = self.config.layout.clone().with_panel(key, !cur);
                self.ui_state.layout = self.config.layout.clone();
                self.ui_state_dirty = true;
                Ok((None, None))
            }
            Command::ToggleRightPanel { workspace } => {
                let ws = workspace.unwrap_or(self.workspace);
                let key = match ws {
                    Workspace::Browser => "browser.inspector",
                    Workspace::Viewer => "viewer.inspector",
                    Workspace::Editor => "editor.inspector",
                };
                let cur = self.config.layout.panel(key);
                self.config.layout = self.config.layout.clone().with_panel(key, !cur);
                self.ui_state.layout = self.config.layout.clone();
                self.ui_state_dirty = true;
                Ok((None, None))
            }
            Command::SetSettings {
                theme,
                language,
                remember_last_folder,
                hardware_acceleration,
                save_ui_state_on_exit,
            } => {
                self.config.theme = *theme;
                self.config.language = *language;
                self.config.remember_last_folder = *remember_last_folder;
                self.config.hardware_acceleration = *hardware_acceleration;
                self.config.save_ui_state_on_exit = *save_ui_state_on_exit;
                self.persist_config();
                Ok((Some(UiText::new("settings_saved")), None))
            }
            Command::NewFolder { parent, name } => {
                let clean = name.trim();
                if clean.is_empty() || clean.contains('/') || clean.contains('\\') {
                    return Err(ProtocolError::new("invalid_name", "invalid folder name"));
                }
                let target = Path::new(parent).join(clean);
                fs::create_dir(&target)
                    .map_err(|e| ProtocolError::new("io_error", e.to_string()))?;
                self.tree_revision += 1;
                if self.current_folder.as_deref() == Some(Path::new(parent)) {
                    let _ = self.open_folder(Path::new(parent));
                }
                Ok((
                    Some(UiText::with("created_folder", [("name", clean)])),
                    None,
                ))
            }
            Command::ViewerNext => self.step_viewer(1),
            Command::ViewerPrevious => self.step_viewer(-1),
            Command::ViewerOpen { path } => {
                let p = Path::new(path);
                let materialized = Self::ensure_archive_page_for_path(&self.archive_session, p)
                    .map_err(|e| ProtocolError::new("decode_error", e.to_string()))?
                    .is_some();
                if !materialized && !p.is_file() {
                    return Err(ProtocolError::new(
                        "not_found",
                        format!("file not found: {path}"),
                    ));
                }
                if let Some(idx) = self.viewer_images.iter().position(|x| x == path) {
                    self.viewer_index = Some(idx);
                } else {
                    self.clear_archive_session();
                    self.viewer_images = vec![path.clone()];
                    self.viewer_index = Some(0);
                    self.listing_epoch += 1;
                }
                self.workspace = Workspace::Viewer;
                self.editor_unsupported = None;
                self.sync_viewer_saved_translation();
                self.sync_session_prefs();
                Ok((Some(self.current_viewer_status()), None))
            }
            Command::EditorOpen { path } => {
                let _ = Self::ensure_archive_page_for_path(&self.archive_session, Path::new(path));
                let text = self.open_editor_path(Path::new(path))?;
                self.editor_unsupported = None;
                self.workspace = Workspace::Editor;
                Ok((Some(text), None))
            }
            Command::EditorNew {
                width,
                height,
                background,
            } => {
                let text = self.new_editor_document(*width, *height, *background)?;
                self.workspace = Workspace::Editor;
                Ok((Some(text), None))
            }
            Command::OpenInEditorOrUnsupported { path } => {
                let _ = Self::ensure_archive_page_for_path(&self.archive_session, Path::new(path));
                match self.open_editor_path(Path::new(path)) {
                    Ok(text) => {
                        self.editor_unsupported = None;
                        self.workspace = Workspace::Editor;
                        Ok((Some(text), None))
                    }
                    Err(err) => {
                        self.set_editor_unsupported(path, err.message);
                        self.workspace = Workspace::Editor;
                        Ok((None, None))
                    }
                }
            }
            Command::EditorResize {
                width,
                height,
                filter,
                mode,
                fill,
                fill_alpha,
            } => self.apply_editor_op(
                EditOp::Resize {
                    width: *width,
                    height: *height,
                    filter: *filter,
                    mode: *mode,
                    fill: *fill,
                    fill_alpha: *fill_alpha,
                },
                ApplyTarget::WholeImage,
            ),
            Command::EditorCrop {
                x,
                y,
                width,
                height,
                fill,
                fill_alpha,
            } => self.apply_editor_op(
                EditOp::Crop {
                    x: *x,
                    y: *y,
                    width: *width,
                    height: *height,
                    fill: *fill,
                    fill_alpha: *fill_alpha,
                },
                ApplyTarget::WholeImage,
            ),
            Command::EditorRotate { degrees, target } => {
                self.apply_editor_op(EditOp::Rotate { degrees: *degrees }, *target)
            }
            Command::EditorFlip { axis, target } => {
                self.apply_editor_op(EditOp::Flip { axis: *axis }, *target)
            }
            Command::EditorAdjustColor {
                brightness,
                contrast,
                saturation,
                exposure,
                highlights,
                shadows,
                gamma,
                target,
            } => self.apply_editor_op(
                EditOp::AdjustColor(ColorAdjustments {
                    brightness: *brightness,
                    contrast: *contrast,
                    saturation: *saturation,
                    exposure: *exposure,
                    highlights: *highlights,
                    shadows: *shadows,
                    gamma: *gamma,
                }),
                *target,
            ),
            Command::EditorCanvasSize {
                width,
                height,
                anchor,
                fill,
                fill_alpha,
            } => self.apply_editor_op(
                EditOp::CanvasSize {
                    width: *width,
                    height: *height,
                    anchor: *anchor,
                    fill: *fill,
                    fill_alpha: *fill_alpha,
                },
                ApplyTarget::WholeImage,
            ),
            Command::EditorRotateAngle {
                degrees,
                fill,
                fill_alpha,
                target,
            } => self.apply_editor_op(
                EditOp::RotateAngle {
                    degrees: *degrees,
                    fill: *fill,
                    fill_alpha: *fill_alpha,
                },
                *target,
            ),
            Command::EditorAdjustHsv {
                hue,
                saturation,
                value,
                target,
            } => self.apply_editor_op(
                EditOp::AdjustHsv(HsvAdjustment {
                    hue: *hue,
                    saturation: *saturation,
                    value: *value,
                }),
                *target,
            ),
            Command::EditorColorBalance {
                shadows,
                midtones,
                highlights,
                preserve_luminosity,
                target,
            } => self.apply_editor_op(
                EditOp::ColorBalance(ColorBalanceAdjustment {
                    shadows: *shadows,
                    midtones: *midtones,
                    highlights: *highlights,
                    preserve_luminosity: *preserve_luminosity,
                }),
                *target,
            ),
            Command::EditorAdjustCurves {
                rgb,
                red,
                green,
                blue,
                target,
            } => self.apply_editor_op(
                EditOp::AdjustCurves(CurvesAdjustment {
                    rgb: rgb.clone(),
                    red: red.clone(),
                    green: green.clone(),
                    blue: blue.clone(),
                }),
                *target,
            ),
            Command::EditorPaintStroke {
                points,
                color,
                radius,
                opacity,
                erase,
                brush,
            } => self.apply_editor_op(
                EditOp::Paint {
                    points: points.clone(),
                    color: *color,
                    radius: *radius,
                    opacity: *opacity,
                    erase: *erase,
                    brush: *brush,
                },
                ApplyTarget::ActiveLayer,
            ),
            Command::EditorLiveStrokeBegin { stroke, x, y } => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.live_stroke_begin(*stroke, *x, *y)
                    .map_err(map_image_err)?;
                Ok((None, None))
            }
            Command::EditorLiveStrokeStep { points } => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.live_stroke_step(points).map_err(map_image_err)?;
                Ok((None, None))
            }
            Command::EditorLiveStrokeEnd { commit } => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                let changed = doc.live_stroke_end(*commit).map_err(map_image_err)?;
                if !changed {
                    return Err(ProtocolError::line("edit_noop", UiText::new("editor_noop")));
                }
                let ui = if *commit {
                    doc.history_labels().last().cloned()
                } else {
                    None
                };
                Ok((ui, None))
            }
            Command::EditorDrawShape {
                kind,
                points,
                nodes,
                fill_mode,
                closed,
                color,
                radius,
                opacity,
            } => self.apply_editor_op(
                EditOp::DrawShape {
                    kind: *kind,
                    points: points.clone(),
                    nodes: nodes.clone(),
                    fill_mode: *fill_mode,
                    closed: *closed,
                    color: color.unwrap_or([0, 0, 0]),
                    radius: radius.unwrap_or(3.0),
                    opacity: opacity.unwrap_or(1.0),
                    brush: photon_protocol::BrushSettings::default(),
                },
                ApplyTarget::ActiveLayer,
            ),
            Command::EditorFloodFill {
                x,
                y,
                tolerance,
                contiguous,
                color,
                opacity,
                sample_all_layers,
            } => self.exec_flood_fill(
                *x,
                *y,
                *tolerance,
                *contiguous,
                color.unwrap_or([0, 0, 0]),
                opacity.unwrap_or(1.0),
                *sample_all_layers,
            ),
            Command::EditorDrawGradient {
                start,
                end,
                kind,
                color_mode,
                fg_color,
                bg_color,
                stops,
                opacity,
                repeat,
                reverse,
                dither,
                blend_mode,
            } => self.apply_editor_op(
                EditOp::DrawGradient {
                    start: *start,
                    end: *end,
                    kind: *kind,
                    color_mode: *color_mode,
                    fg_color: fg_color.unwrap_or([0, 0, 0]),
                    bg_color: bg_color.unwrap_or([255, 255, 255]),
                    stops: stops.clone(),
                    opacity: opacity.unwrap_or(1.0),
                    repeat: *repeat,
                    reverse: *reverse,
                    dither: *dither,
                    blend_mode: *blend_mode,
                },
                ApplyTarget::ActiveLayer,
            ),
            Command::EditorHealingStroke {
                points,
                mode,
                radius,
                hardness,
                strength,
                source,
            } => self.apply_editor_op(
                EditOp::Healing {
                    points: points.clone(),
                    mode: *mode,
                    radius: *radius,
                    strength: *strength,
                    source: *source,
                    brush: photon_protocol::BrushSettings {
                        hardness: *hardness,
                        ..photon_protocol::BrushSettings::default()
                    },
                },
                ApplyTarget::ActiveLayer,
            ),
            Command::EditorDenoise {
                strength,
                detail_preserve,
                target,
            } => self.apply_editor_op(
                EditOp::Denoise {
                    strength: *strength,
                    detail_preserve: *detail_preserve,
                },
                *target,
            ),
            Command::EditorTransformLayer {
                layer_id,
                transform,
            } => self.apply_editor_op(
                EditOp::TransformLayer {
                    layer_id: *layer_id,
                    transform: *transform,
                },
                ApplyTarget::ActiveLayer,
            ),
            Command::EditorCopyPixels { merged } => self.copy_editor_pixels(*merged),
            Command::EditorCutPixels => self.cut_editor_pixels(),
            Command::EditorPastePixels {
                png_bytes,
                into_selection,
            } => self.paste_editor_pixels(png_bytes.as_deref(), *into_selection),
            Command::EditorLayerAdd { name } => {
                self.apply_layer_op(LayerOperation::AddEmpty { name: name.clone() })
            }
            Command::EditorLayerAddEffect { name, effect } => {
                self.apply_layer_op(LayerOperation::AddEffect {
                    name: name.clone(),
                    effect: effect.clone(),
                })
            }
            Command::EditorLayerUpdateEffect { id, effect } => {
                self.apply_layer_op(LayerOperation::UpdateEffect {
                    id: *id,
                    effect: effect.clone(),
                })
            }
            Command::EditorLayerDuplicate { id } => {
                self.apply_layer_op(LayerOperation::Duplicate { id: *id })
            }
            Command::EditorLayerDelete { id } => {
                self.apply_layer_op(LayerOperation::Delete { id: *id })
            }
            Command::EditorLayerSelect { id } => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.select_layer(*id).map_err(map_image_err)?;
                Ok((None, None))
            }
            Command::EditorLayerReorder { id, new_index } => {
                self.apply_layer_op(LayerOperation::Reorder {
                    id: *id,
                    new_index: *new_index,
                })
            }
            Command::EditorLayerSetProps {
                id,
                name,
                visible,
                locked,
                opacity,
                blend_mode,
                offset_x,
                offset_y,
            } => self.apply_layer_op(LayerOperation::SetProps {
                id: *id,
                name: name.clone(),
                visible: *visible,
                locked: *locked,
                opacity: *opacity,
                blend_mode: *blend_mode,
                offset_x: *offset_x,
                offset_y: *offset_y,
            }),
            Command::EditorLayerMergeDown { id } => {
                self.apply_layer_op(LayerOperation::MergeDown { id: *id })
            }
            Command::EditorLayerFlatten => self.apply_layer_op(LayerOperation::Flatten),
            Command::EditorSetCanvasBackground { color, transparent } => {
                let resolved = if *transparent { None } else { *color };
                self.apply_layer_op(LayerOperation::SetCanvasBackground { color: resolved })
            }
            Command::EditorHistoryJump { target_index } => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                match doc.jump_history(*target_index) {
                    Ok(true) => Ok((
                        Some(UiText::with(
                            "history_jumped",
                            [("index", target_index.to_string())],
                        )),
                        None,
                    )),
                    Ok(false) => Err(ProtocolError::line("edit_noop", UiText::new("editor_noop"))),
                    Err(e) => Err(map_image_err(e)),
                }
            }
            Command::EditorExport {
                path,
                quality,
                metadata,
                max_edge,
            } => {
                let doc = self.editor.as_ref().ok_or_else(no_doc_err)?;
                let exported = doc
                    .export_to(Path::new(path), *quality, *metadata, *max_edge)
                    .map_err(|e| ProtocolError::new("export_error", e.to_string()))?;
                Ok((
                    Some(UiText::with(
                        "editor_exported",
                        [("path", exported.to_string_lossy().into_owned())],
                    )),
                    Some(serde_json::json!({ "path": exported })),
                ))
            }
            Command::EditorLayerMaskAdd { id, reveal_all } => {
                self.apply_layer_op(LayerOperation::AddMask {
                    id: *id,
                    reveal_all: *reveal_all,
                })
            }
            Command::EditorLayerMaskDelete { id } => {
                self.apply_layer_op(LayerOperation::DeleteMask { id: *id })
            }
            Command::EditorLayerMaskApply { id } => {
                self.apply_layer_op(LayerOperation::ApplyMask { id: *id })
            }
            Command::EditorLayerMaskInvert { id } => {
                self.apply_layer_op(LayerOperation::InvertMask { id: *id })
            }
            Command::EditorLayerMaskSetEnabled { id, enabled } => {
                self.apply_layer_op(LayerOperation::SetMaskEnabled {
                    id: *id,
                    enabled: *enabled,
                })
            }
            Command::EditorLayerMaskSetEditing { id, editing } => {
                self.apply_layer_op(LayerOperation::SetEditingMask {
                    id: *id,
                    editing: *editing,
                })
            }
            Command::EditorLayerMaskSetLinked { id, linked } => {
                self.apply_layer_op(LayerOperation::SetMaskLinked {
                    id: *id,
                    linked: *linked,
                })
            }
            Command::EditorLiquifyStroke {
                points,
                mode,
                radius,
                strength,
            } => self.apply_editor_op(
                EditOp::Liquify {
                    points: points.clone(),
                    mode: *mode,
                    radius: *radius,
                    strength: *strength,
                },
                ApplyTarget::ActiveLayer,
            ),
            Command::EditorSelectAll => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.select_all();
                Ok((None, None))
            }
            Command::EditorDeselect => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.deselect();
                Ok((None, None))
            }
            Command::EditorReselect => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.reselect();
                Ok((None, None))
            }
            Command::EditorSelectInvert => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.invert_selection();
                Ok((None, None))
            }
            Command::EditorSelectShape { shape, op } => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.select_shape(shape, *op);
                Ok((None, None))
            }
            Command::EditorSelectModify { op } => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.modify_selection(*op);
                Ok((None, None))
            }
            Command::EditorSelectToggleQuickMask => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.toggle_quick_mask();
                Ok((None, None))
            }
            Command::EditorSelectFromLayerAlpha { op } => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.select_from_layer_alpha(*op);
                Ok((None, None))
            }
            Command::EditorSelectionToMask => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.selection_to_mask().map_err(map_image_err)?;
                Ok((None, None))
            }
            Command::EditorSelectionClearPixels => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.clear_selected_pixels().map_err(map_image_err)?;
                Ok((None, None))
            }
            Command::EditorSelectionDuplicateLayer => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.duplicate_selection_to_layer().map_err(map_image_err)?;
                Ok((None, None))
            }
            Command::EditorCropToSelection => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                doc.crop_to_selection().map_err(map_image_err)?;
                Ok((None, None))
            }
            Command::BatchRun {
                operation,
                source,
                output,
            } => {
                if self.batch.as_ref().is_some_and(|b| b.running) {
                    return Err(ProtocolError::line(
                        "batch_already_running",
                        UiText::new("batch_already_running"),
                    ));
                }
                let inputs: Vec<PathBuf> = match source {
                    BatchSource::Selection { paths } => paths
                        .iter()
                        .map(PathBuf::from)
                        .filter(|p| p.is_file() && is_supported_image(p))
                        .collect(),
                    BatchSource::Folder { path } => {
                        let dir = Path::new(path);
                        if !dir.is_dir() {
                            return Err(ProtocolError::new(
                                "not_found",
                                format!("{} is not a directory", dir.display()),
                            ));
                        }
                        let mut imgs: Vec<PathBuf> = fs::read_dir(dir)
                            .map_err(|e| ProtocolError::new("io_error", e.to_string()))?
                            .flatten()
                            .filter_map(|e| {
                                let name = e.file_name().to_string_lossy().into_owned();
                                let p = e.path();
                                (!name.starts_with('.') && p.is_file() && is_supported_image(&p))
                                    .then_some(p)
                            })
                            .collect();
                        imgs.sort();
                        imgs
                    }
                };
                let job_id = format!("batch-{}", self.next_job_seq);
                self.next_job_seq += 1;
                let plan =
                    BatchPlan::build(job_id.clone(), operation.clone(), inputs, output.clone())?;
                let state = plan.initial_state();
                let cancel = Arc::new(AtomicBool::new(false));
                self.batch = Some(state);
                self.batch_cancel = Some(Arc::clone(&cancel));
                self.pending_batch = Some((plan, cancel));
                self.tasks.push(TaskSnapshot {
                    task_id: job_id.clone(),
                    kind: "batch".to_owned(),
                    status: TaskRunStatus::Running,
                    percent: 0,
                    stage: operation.name().to_owned(),
                });
                Ok((None, Some(serde_json::json!({ "jobId": job_id }))))
            }
            Command::BatchCancel => {
                if let Some(cancel) = &self.batch_cancel
                    && let Some(batch) = self.batch.as_mut()
                    && batch.running
                {
                    cancel.store(true, Ordering::SeqCst);
                    batch.cancelled = true;
                    return Ok((None, None));
                }
                Err(ProtocolError::new("not_running", "no active batch job"))
            }
            Command::AddFavorite { path } => {
                let ui = file_ops::add_favorite(&mut self.config.favorites, Path::new(path))?;
                if ui.is_some() {
                    self.persist_config();
                }
                Ok((ui, None))
            }
            Command::RemoveFavorite { path } => {
                let ui = file_ops::remove_favorite(&mut self.config.favorites, path);
                if ui.is_some() {
                    self.persist_config();
                }
                Ok((ui, None))
            }
            Command::MoveFavorite { path, direction } => {
                if file_ops::move_favorite(&mut self.config.favorites, path, *direction) {
                    self.persist_config();
                }
                Ok((None, None))
            }
            Command::RenamePath { path, new_name } => {
                let renamed = file_ops::rename_path(Path::new(path), new_name)?;
                let name = renamed
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(new_name)
                    .to_owned();
                self.refresh_after_file_op();
                Ok((
                    Some(UiText::with("renamed_item", [("name", name)])),
                    Some(serde_json::json!({ "path": renamed })),
                ))
            }
            Command::CopyTo { paths, destination } => {
                let dst = Path::new(destination);
                let mut count = 0usize;
                for raw in paths {
                    file_ops::copy_path(Path::new(raw), dst)
                        .map_err(|e| ProtocolError::new("io_error", e))?;
                    count += 1;
                }
                self.refresh_after_file_op();
                Ok((
                    Some(UiText::with("copied_items", [("count", count.to_string())])),
                    None,
                ))
            }
            Command::MoveTo { paths, destination } => {
                let dst = Path::new(destination);
                let mut count = 0usize;
                for raw in paths {
                    file_ops::move_path(Path::new(raw), dst)
                        .map_err(|e| ProtocolError::new("io_error", e))?;
                    count += 1;
                }
                self.refresh_after_file_op();
                Ok((
                    Some(UiText::with("moved_items", [("count", count.to_string())])),
                    None,
                ))
            }
            Command::Trash { paths } => {
                let mut targets = Vec::with_capacity(paths.len());
                for raw in paths {
                    let p = PathBuf::from(raw);
                    file_ops::safe_for_deletion(&p)?;
                    targets.push(p);
                }
                file_ops::trash_paths(&targets).map_err(|e| ProtocolError::new("io_error", e))?;
                let count = targets.len();
                self.refresh_after_file_op();
                Ok((
                    Some(UiText::with(
                        "trashed_items",
                        [("count", count.to_string())],
                    )),
                    None,
                ))
            }
            Command::DeletePermanently { paths } => {
                let mut targets = Vec::with_capacity(paths.len());
                for raw in paths {
                    let p = PathBuf::from(raw);
                    file_ops::safe_for_deletion(&p)?;
                    targets.push(p);
                }
                for p in &targets {
                    file_ops::remove_path(p).map_err(|e| ProtocolError::new("io_error", e))?;
                }
                let count = targets.len();
                self.refresh_after_file_op();
                Ok((
                    Some(UiText::with(
                        "deleted_items",
                        [("count", count.to_string())],
                    )),
                    None,
                ))
            }
            Command::PasteImageBytes {
                folder,
                filename_hint,
                png_bytes,
            } => self.paste_image_bytes(Path::new(folder), filename_hint, png_bytes),
            Command::PluginRefresh | Command::AiRefreshPlugins => self.plugin_refresh(),
            Command::PluginSetEnabled { id, enabled } => self.plugin_set_enabled(id, *enabled),
            Command::PluginInstall { source_dir } => self.plugin_install(Path::new(source_dir)),
            Command::PluginUninstall { id } => self.plugin_uninstall(id),
            Command::PluginRun { id, params, target } => self.plugin_run(id, params, *target),
            Command::AiSetSettings {
                device,
                auto_unload_secs,
            } => self.ai_set_settings(*device, *auto_unload_secs),
            Command::AiDownloadModel { model_id } => self.ai_download_model(model_id),
            Command::AiDeleteModel { model_id } => self.ai_delete_model(model_id),
            Command::AiSetCustomModel { model_id, path } => {
                self.ai_set_custom_model(model_id, path)
            }
            Command::AiCancel => self.ai_cancel(),
            Command::QuickActionRun {
                plugin_id,
                action_id,
                paths,
            } => self.quick_action_run(plugin_id, action_id, paths.clone()),
            Command::EditorAiRemoveBackground {
                model_id,
                feather,
                target,
            } => self.editor_ai_remove_background(model_id, *feather, *target),
            Command::EditorDewatermarkGemini {
                size,
                logo_value,
                force,
                offset_x,
                offset_y,
                target,
            } => self.editor_dewatermark_gemini(
                size,
                *logo_value,
                *force,
                *offset_x,
                *offset_y,
                *target,
            ),
            Command::EditorInpaintRegion {
                x,
                y,
                width,
                height,
                feather,
                target,
            } => self.editor_inpaint_region(*x, *y, *width, *height, *feather, *target),
            Command::EditorAiInpaint {
                detector_model,
                inpainter_model,
                confidence,
                mask_padding,
                region_x,
                region_y,
                region_width,
                region_height,
                brush_strokes,
                target,
            } => self.editor_ai_inpaint(
                detector_model,
                inpainter_model,
                *confidence,
                *mask_padding,
                *region_x,
                *region_y,
                *region_width,
                *region_height,
                brush_strokes,
                *target,
            ),
            Command::EditorRefineAlpha {
                erase_strokes,
                restore_strokes,
                softness,
            } => self.editor_refine_alpha(erase_strokes, restore_strokes, *softness),
            Command::EditorGenerateCaption {
                model_id,
                general_threshold,
                character_threshold,
                replace_underscores,
                prefix,
                suffix,
                exclude_tags,
            } => self.editor_generate_caption(
                model_id,
                *general_threshold,
                *character_threshold,
                *replace_underscores,
                prefix,
                suffix,
                exclude_tags,
            ),
            Command::EditorSaveCaption { text } => self.editor_save_caption(text),
            Command::ViewerTranslate {
                path,
                source_lang,
                target_lang,
                region,
                ocr_engine,
            } => self.viewer_translate(path, source_lang, target_lang, *region, ocr_engine),
            Command::ViewerSaveTranslation {
                path,
                blocks,
                source_lang,
                target_lang,
            } => self.viewer_save_translation(path, source_lang, target_lang, blocks.clone()),
            Command::ViewerDeleteTranslation { path } => self.viewer_delete_translation(path),
            Command::SetTranslateSettings { settings } => {
                self.set_translate_settings(settings.clone())
            }
            Command::EditorUndo => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                match doc.undo() {
                    Ok(true) => Ok((Some(UiText::new("editor_undone")), None)),
                    Ok(false) => Err(ProtocolError::line(
                        "nothing_to_undo",
                        UiText::new("nothing_to_undo"),
                    )),
                    Err(e) => Err(ProtocolError::new("edit_error", e.to_string())),
                }
            }
            Command::EditorRedo => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                match doc.redo() {
                    Ok(true) => Ok((Some(UiText::new("editor_redone")), None)),
                    Ok(false) => Err(ProtocolError::line(
                        "nothing_to_redo",
                        UiText::new("nothing_to_redo"),
                    )),
                    Err(e) => Err(ProtocolError::new("edit_error", e.to_string())),
                }
            }
            Command::EditorReset => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                if doc.reset() {
                    Ok((Some(UiText::new("editor_reset")), None))
                } else {
                    Err(ProtocolError::line(
                        "nothing_to_reset",
                        UiText::new("nothing_to_reset"),
                    ))
                }
            }
            Command::EditorSave => {
                if self.editor_source_is_virtual {
                    return Err(ProtocolError::new(
                        "save_as_required",
                        "virtual or unsaved document requires Save As",
                    ));
                }
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                let saved = doc
                    .save_to(None)
                    .map_err(|e| ProtocolError::new("save_error", e.to_string()))?;
                Ok((
                    Some(UiText::with(
                        "editor_saved",
                        [("path", saved.to_string_lossy().into_owned())],
                    )),
                    Some(serde_json::json!({ "path": saved })),
                ))
            }
            Command::EditorSaveAs { path } => {
                let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
                let saved = doc
                    .save_to(Some(Path::new(path)))
                    .map_err(|e| ProtocolError::new("save_error", e.to_string()))?;
                self.editor_source_is_virtual = false;
                Ok((
                    Some(UiText::with(
                        "editor_saved",
                        [("path", saved.to_string_lossy().into_owned())],
                    )),
                    Some(serde_json::json!({ "path": saved })),
                ))
            }
            Command::EditorClose => {
                let text = self.close_editor_tab(None)?;
                Ok((Some(text), None))
            }
            Command::EditorSwitchTab { tab_id } => {
                if !self.activate_editor_tab(*tab_id) {
                    return Err(ProtocolError::new(
                        "not_found",
                        format!("unknown editor tab {tab_id}"),
                    ));
                }
                self.workspace = Workspace::Editor;
                Ok((None, None))
            }
            Command::EditorCloseTab { tab_id } => {
                let text = self.close_editor_tab(*tab_id)?;
                Ok((Some(text), None))
            }
            Command::EditorReorderTab { tab_id, new_index } => {
                self.reorder_editor_tab(*tab_id, *new_index)?;
                Ok((None, None))
            }
            Command::FocusWindow => {
                self.focus_request_seq += 1;
                Ok((None, None))
            }
            Command::SaveUiState { force } => {
                self.persist_ui_state(*force);
                Ok((None, None))
            }
            Command::CancelTask { task_id } => {
                if self.ai_active_task_id.as_deref() == Some(task_id.as_str()) {
                    return self.ai_cancel();
                }
                if let Some(task) = self.tasks.iter_mut().find(|t| t.task_id == *task_id) {
                    task.status = TaskRunStatus::Cancelled;
                    Ok((
                        Some(UiText::with(
                            "task_cancelled",
                            [("taskId", task_id.as_str())],
                        )),
                        None,
                    ))
                } else {
                    Err(ProtocolError::new(
                        "not_found",
                        format!("unknown task {task_id}"),
                    ))
                }
            }
            Command::ResetConfig {
                reset_layout,
                reset_settings,
            } => {
                if *reset_layout {
                    self.config.layout = self.config.layout.reset_ui_state_preserving_settings();
                    self.ui_state = UiState {
                        last_folder: None,
                        layout: self.config.layout.clone(),
                    };
                    self.ui_state_dirty = false;
                    if !self.viewer_ephemeral {
                        let ui_path = self.paths.ui_state_file();
                        let _ = Config::backup_existing(&ui_path);
                        let _ = self.ui_state.save(&ui_path);
                    }
                }
                if *reset_settings {
                    let preserved_ui = self.config.layout.clone();
                    self.config = Config::factory_default();
                    if !*reset_layout {
                        let mut merged = preserved_ui;
                        merged.apply_settings_from(&self.config.layout);
                        self.config.layout = merged;
                    }
                    if !self.viewer_ephemeral {
                        let cfg_path = self.paths.config_file();
                        let _ = Config::backup_existing(&cfg_path);
                        let _ = self.config.save(&cfg_path);
                    }
                }
                Ok((Some(UiText::new("config_reset_done")), None))
            }
            Command::DebugScreenshot { output_path } => {
                let out = output_path.clone().unwrap_or_else(|| {
                    self.paths
                        .screenshot_dir
                        .join(format!("shot-{}.png", now_ms()))
                        .to_string_lossy()
                        .into_owned()
                });
                Ok((None, Some(serde_json::json!({ "outputPath": out }))))
            }
            Command::DebugUiAction { target_id, action } => Ok((
                Some(UiText::with(
                    "ui_action_requested",
                    [("target", target_id.as_str())],
                )),
                Some(serde_json::json!({ "targetId": target_id, "action": action })),
            )),
        }
    }

    fn sync_session_prefs(&mut self) {
        if self.safe_mode || self.viewer_ephemeral || !self.config.remember_last_folder {
            return;
        }
        self.ui_state.last_folder = self
            .current_folder
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        self.ui_state_dirty = true;
    }

    pub(crate) fn persist_config(&self) {
        if !self.safe_mode && !self.viewer_ephemeral {
            let _ = self.config.save(&self.paths.config_file());
        }
    }

    pub fn persist_ui_state(&mut self, force: bool) {
        if self.safe_mode || self.viewer_ephemeral {
            return;
        }
        if !force && (!self.config.save_ui_state_on_exit || !self.ui_state_dirty) {
            return;
        }
        self.ui_state.layout = self.config.layout.clone();
        if self.config.remember_last_folder {
            self.ui_state.last_folder = self
                .current_folder
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned());
        }
        let _ = self.ui_state.save(&self.paths.ui_state_file());
        self.ui_state_dirty = false;
    }

    fn open_folder(&mut self, dir: &Path) -> Result<UiText, ProtocolError> {
        if !dir.is_dir() {
            return Err(ProtocolError::with_line(
                "not_found",
                format!("{} is not a directory", dir.display()),
                UiText::with("folder_error", [("error", dir.display().to_string())]),
            ));
        }
        let rd = fs::read_dir(dir).map_err(|e| ProtocolError::new("io_error", e.to_string()))?;
        let mut entries = Vec::new();
        let mut viewer_images = Vec::new();
        for item in rd.flatten() {
            let p = item.path();
            let name = item.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let meta = fs::metadata(&p).ok();
            let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
            let is_image = !is_dir && is_supported_image(&p);
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified_ms = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let path_str = p.to_string_lossy().into_owned();
            if is_image {
                viewer_images.push(path_str.clone());
            }
            let kind = file_ops::kind_label(&p, is_dir);
            entries.push(Entry {
                name,
                path: path_str,
                is_dir,
                is_image,
                size,
                modified_ms,
                kind,
            });
        }
        file_ops::sort_entries(
            &mut entries,
            self.config.layout.browser_sort_field,
            self.config.layout.browser_sort_order,
        );
        viewer_images.sort_by_key(|a| a.to_lowercase());

        self.clear_archive_session();
        let total = entries.len();
        let img_count = viewer_images.len();
        self.current_folder = Some(dir.to_path_buf());
        self.entries = entries;
        self.viewer_images = viewer_images;
        self.viewer_index = (!self.viewer_images.is_empty()).then_some(0);
        self.selection.clear();
        self.selection_info = None;
        self.listing_epoch += 1;
        self.sync_viewer_saved_translation();

        Ok(UiText::with(
            "folder_contents",
            [
                ("items", total.to_string()),
                ("images", img_count.to_string()),
            ],
        ))
    }

    fn park_active_editor_tab(&mut self) {
        if let Some(active_id) = self.active_editor_tab_id
            && let Some(slot) = self.editor_tabs.iter_mut().find(|s| s.id == active_id)
        {
            slot.doc = self.editor.take();
            slot.caption = self.editor_caption.take();
            slot.source_is_virtual = self.editor_source_is_virtual;
        }
    }

    fn activate_editor_tab(&mut self, tab_id: u64) -> bool {
        if self.active_editor_tab_id == Some(tab_id) {
            return self.editor.is_some();
        }
        if !self.editor_tabs.iter().any(|s| s.id == tab_id) {
            return false;
        }
        self.park_active_editor_tab();
        if let Some(slot) = self.editor_tabs.iter_mut().find(|s| s.id == tab_id) {
            self.editor = slot.doc.take();
            self.editor_caption = slot.caption.take();
            self.editor_source_is_virtual = slot.source_is_virtual;
            self.editor_unsupported = None;
            self.active_editor_tab_id = Some(tab_id);
            true
        } else {
            false
        }
    }

    fn close_editor_tab(&mut self, target_id: Option<u64>) -> Result<UiText, ProtocolError> {
        let id = target_id
            .or(self.active_editor_tab_id)
            .ok_or_else(no_doc_err)?;
        let Some(pos) = self.editor_tabs.iter().position(|s| s.id == id) else {
            return Err(ProtocolError::new(
                "not_found",
                format!("unknown editor tab {id}"),
            ));
        };
        let closing_active = Some(id) == self.active_editor_tab_id;
        self.editor_tabs.remove(pos);
        if closing_active {
            self.editor = None;
            self.editor_caption = None;
            self.editor_source_is_virtual = false;
            if self.editor_tabs.is_empty() {
                self.active_editor_tab_id = None;
                if self.workspace == Workspace::Editor {
                    self.workspace = if self.viewer_images.is_empty() {
                        Workspace::Browser
                    } else {
                        Workspace::Viewer
                    };
                    self.sync_session_prefs();
                }
            } else {
                let next_idx = pos.min(self.editor_tabs.len() - 1);
                let next_id = self.editor_tabs[next_idx].id;
                self.active_editor_tab_id = Some(next_id);
                self.editor = self.editor_tabs[next_idx].doc.take();
                self.editor_caption = self.editor_tabs[next_idx].caption.take();
                self.editor_source_is_virtual = self.editor_tabs[next_idx].source_is_virtual;
            }
        }
        Ok(UiText::new("editor_closed"))
    }

    fn reorder_editor_tab(&mut self, tab_id: u64, new_index: usize) -> Result<(), ProtocolError> {
        let Some(old_idx) = self.editor_tabs.iter().position(|s| s.id == tab_id) else {
            return Err(ProtocolError::new(
                "not_found",
                format!("unknown editor tab {tab_id}"),
            ));
        };
        let target_idx = new_index.min(self.editor_tabs.len().saturating_sub(1));
        if old_idx != target_idx {
            let item = self.editor_tabs.remove(old_idx);
            self.editor_tabs.insert(target_idx, item);
        }
        Ok(())
    }

    fn open_editor_path(&mut self, path: &Path) -> Result<UiText, ProtocolError> {
        let target_canon = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let existing_id = self.editor_tabs.iter().find_map(|slot| {
            let doc = if Some(slot.id) == self.active_editor_tab_id {
                self.editor.as_ref().or(slot.doc.as_ref())
            } else {
                slot.doc.as_ref()
            }?;
            let slot_canon =
                fs::canonicalize(&doc.source_path).unwrap_or_else(|_| doc.source_path.clone());
            (slot_canon == target_canon).then_some(slot.id)
        });
        if let Some(tab_id) = existing_id {
            self.activate_editor_tab(tab_id);
            let doc = self.editor.as_ref().ok_or_else(no_doc_err)?;
            let (w, h) = doc.dimensions();
            let name = doc
                .source_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_owned();
            return Ok(UiText::with(
                "editor_opened",
                [
                    ("name", name),
                    ("width", w.to_string()),
                    ("height", h.to_string()),
                ],
            ));
        }

        let mut doc = EditorDocument::open(path.to_path_buf())
            .map_err(|e| ProtocolError::new("decode_error", e.to_string()))?;
        doc.original_revision = self.next_original_revision;
        self.next_original_revision += 1;
        let (w, h) = doc.dimensions();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let is_virtual = self.is_archive_temp_path(path);
        self.park_active_editor_tab();
        let tab_id = self.next_editor_tab_id;
        self.next_editor_tab_id += 1;
        self.editor_tabs.push(EditorTabSlot {
            id: tab_id,
            doc: None,
            caption: None,
            source_is_virtual: is_virtual,
        });
        self.active_editor_tab_id = Some(tab_id);
        self.editor = Some(doc);
        self.editor_source_is_virtual = is_virtual;
        self.editor_unsupported = None;
        self.editor_caption = if is_virtual {
            None
        } else {
            ai::load_caption_for_image(path)
        };
        Ok(UiText::with(
            "editor_opened",
            [
                ("name", name),
                ("width", w.to_string()),
                ("height", h.to_string()),
            ],
        ))
    }

    pub fn archive_session(&self) -> Arc<Mutex<Option<ArchiveSession>>> {
        Arc::clone(&self.archive_session)
    }

    /// Materialize the archive/PDF page matching `path` if it belongs to the active archive session.
    pub fn ensure_archive_page_for_path(
        session_arc: &Arc<Mutex<Option<ArchiveSession>>>,
        path: &Path,
    ) -> Result<Option<PathBuf>, ImageError> {
        let mut guard = session_arc
            .lock()
            .map_err(|_| ImageError::InvalidEdit("archive session lock poisoned".to_owned()))?;
        let Some(session) = guard.as_mut() else {
            return Ok(None);
        };
        let Some(index) = find_archive_page_index(session, path) else {
            return Ok(None);
        };
        session.materialize_page(index).map(Some)
    }

    /// Render/decode a thumbnail for the archive/PDF page matching `path`.
    pub fn archive_thumbnail_for_path(
        session_arc: &Arc<Mutex<Option<ArchiveSession>>>,
        path: &Path,
        max_edge: u32,
    ) -> Result<Option<RgbaImage>, ImageError> {
        let mut guard = session_arc
            .lock()
            .map_err(|_| ImageError::InvalidEdit("archive session lock poisoned".to_owned()))?;
        let Some(session) = guard.as_mut() else {
            return Ok(None);
        };
        let Some(index) = find_archive_page_index(session, path) else {
            return Ok(None);
        };
        session.page_thumbnail(index, max_edge).map(Some)
    }

    fn is_archive_temp_path(&self, path: &Path) -> bool {
        let Ok(guard) = self.archive_session.lock() else {
            return false;
        };
        let Some(session) = guard.as_ref() else {
            return false;
        };
        path.starts_with(session.temp_dir())
    }

    fn clear_archive_session(&mut self) {
        if let Ok(mut guard) = self.archive_session.lock() {
            *guard = None;
        }
        self.viewer_archive = None;
    }

    fn open_archive_container(&mut self, path: &Path) -> Result<UiText, ProtocolError> {
        if !path.is_file() {
            return Err(ProtocolError::new(
                "not_found",
                format!("archive not found: {}", path.display()),
            ));
        }
        let mut session = ArchiveSession::open(path)
            .map_err(|e| ProtocolError::new("decode_error", e.to_string()))?;
        if session.page_count() == 0 {
            return Err(ProtocolError::new(
                "empty_archive",
                format!("no viewable pages in {}", path.display()),
            ));
        }
        session
            .materialize_page(0)
            .map_err(|e| ProtocolError::new("decode_error", e.to_string()))?;

        let viewer_images: Vec<String> = (0..session.page_count())
            .map(|idx| {
                planned_archive_page_path(&session, idx)
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        let page_labels: Vec<String> = session.pages().iter().map(|p| p.label.clone()).collect();
        let kind = match session.kind() {
            ArchiveKind::ZipCbz => "zip_cbz",
            ArchiveKind::Pdf => "pdf",
        }
        .to_owned();
        let meta = ArchiveViewerSessionMeta {
            container_path: path.to_string_lossy().into_owned(),
            kind,
            page_count: session.page_count(),
            page_labels,
        };

        if let Ok(mut guard) = self.archive_session.lock() {
            *guard = Some(session);
        }
        self.viewer_archive = Some(meta);
        self.viewer_images = viewer_images;
        self.viewer_index = (!self.viewer_images.is_empty()).then_some(0);
        self.listing_epoch += 1;
        self.workspace = Workspace::Viewer;
        self.editor_unsupported = None;
        self.sync_viewer_saved_translation();
        self.sync_session_prefs();
        Ok(self.current_viewer_status())
    }

    fn new_editor_document(
        &mut self,
        width: u32,
        height: u32,
        background: [u8; 4],
    ) -> Result<UiText, ProtocolError> {
        let mut doc =
            EditorDocument::new_blank(width, height, background).map_err(map_image_err)?;
        doc.original_revision = self.next_original_revision;
        self.next_original_revision += 1;
        let (w, h) = doc.dimensions();
        self.park_active_editor_tab();
        let tab_id = self.next_editor_tab_id;
        self.next_editor_tab_id += 1;
        self.editor_tabs.push(EditorTabSlot {
            id: tab_id,
            doc: None,
            caption: None,
            source_is_virtual: true,
        });
        self.active_editor_tab_id = Some(tab_id);
        self.editor = Some(doc);
        self.editor_source_is_virtual = true;
        self.editor_unsupported = None;
        self.editor_caption = None;
        Ok(UiText::with(
            "editor_opened",
            [
                ("name", "Untitled".to_owned()),
                ("width", w.to_string()),
                ("height", h.to_string()),
            ],
        ))
    }

    fn set_editor_unsupported(&mut self, path: &str, reason: String) {
        self.park_active_editor_tab();
        self.editor = None;
        self.active_editor_tab_id = None;
        self.editor_source_is_virtual = false;
        self.editor_caption = None;
        let extension = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        self.editor_unsupported = Some(UnsupportedEditorFile {
            path: path.to_owned(),
            extension,
            reason,
        });
    }

    fn apply_editor_op(
        &mut self,
        op: EditOp,
        target: ApplyTarget,
    ) -> Result<(Option<UiText>, Option<serde_json::Value>), ProtocolError> {
        let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
        let fallback_label = op.ui_label();
        match doc.apply_with_target(op, target) {
            Ok(true) => {
                let label = doc.history_labels().pop().unwrap_or(fallback_label);
                Ok((Some(label), None))
            }
            Ok(false) => Err(ProtocolError::line("edit_noop", UiText::new("editor_noop"))),
            Err(e) => Err(map_image_err(e)),
        }
    }

    fn apply_layer_op(
        &mut self,
        op: LayerOperation,
    ) -> Result<(Option<UiText>, Option<serde_json::Value>), ProtocolError> {
        let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
        let fallback_label = op.ui_label();
        match doc.apply_layer_op(op) {
            Ok(true) => {
                let label = doc.history_labels().pop().unwrap_or(fallback_label);
                Ok((Some(label), None))
            }
            Ok(false) => Err(ProtocolError::line("edit_noop", UiText::new("editor_noop"))),
            Err(e) => Err(map_image_err(e)),
        }
    }

    fn step_viewer(
        &mut self,
        delta: isize,
    ) -> Result<(Option<UiText>, Option<serde_json::Value>), ProtocolError> {
        if self.viewer_images.is_empty() {
            return Err(ProtocolError::line(
                "no_images",
                UiText::new("viewer_no_images"),
            ));
        }
        let len = self.viewer_images.len() as isize;
        let cur = self.viewer_index.unwrap_or(0) as isize;
        let next = (cur + delta).rem_euclid(len) as usize;
        if self.viewer_archive.is_some()
            && let Some(next_path) = self.viewer_images.get(next)
        {
            let _ = Self::ensure_archive_page_for_path(&self.archive_session, Path::new(next_path));
        }
        self.viewer_index = Some(next);
        self.workspace = Workspace::Viewer;
        self.sync_viewer_saved_translation();
        self.sync_session_prefs();
        Ok((Some(self.current_viewer_status()), None))
    }

    pub(crate) fn sync_viewer_saved_translation(&mut self) {
        self.viewer_saved_translation = self
            .viewer_index
            .and_then(|idx| self.viewer_images.get(idx))
            .and_then(|p| translate::load_saved_translation(Path::new(p)));
    }

    pub(crate) fn refresh_after_file_op(&mut self) {
        self.tree_revision += 1;
        let prev_viewer_path = self
            .viewer_index
            .and_then(|idx| self.viewer_images.get(idx).cloned());
        let prev_selection = self.selection.clone();
        if let Some(dir) = self.current_folder.clone() {
            let _ = self.open_folder(&dir);
        }
        if let Some(vp) = prev_viewer_path
            && let Some(idx) = self.viewer_images.iter().position(|p| p == &vp)
        {
            self.viewer_index = Some(idx);
            self.viewer_saved_translation = translate::load_saved_translation(Path::new(&vp));
        }
        if !prev_selection.is_empty() {
            let kept: Vec<String> = prev_selection
                .into_iter()
                .filter(|p| self.entries.iter().any(|e| e.path == *p))
                .collect();
            if !kept.is_empty() {
                self.selection_info = kept
                    .first()
                    .and_then(|p| build_selection_info(Path::new(p)));
                self.selection = kept;
            }
        }
    }

    fn current_viewer_status(&self) -> UiText {
        let total = self.viewer_images.len();
        let idx = self.viewer_index.unwrap_or(0);
        let name = self
            .viewer_images
            .get(idx)
            .and_then(|p| Path::new(p).file_name())
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        UiText::with(
            "viewer_opened",
            [
                ("name", name),
                ("index", (idx + 1).to_string()),
                ("total", total.to_string()),
            ],
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn exec_flood_fill(
        &mut self,
        x: u32,
        y: u32,
        tolerance: u8,
        contiguous: bool,
        color: [u8; 3],
        opacity: f32,
        sample_all_layers: bool,
    ) -> Result<(Option<UiText>, Option<serde_json::Value>), ProtocolError> {
        let should_sample_all = {
            let doc = self.editor.as_ref().ok_or_else(no_doc_err)?;
            let stack = doc.stack();
            let active = stack.active_layer();
            sample_all_layers
                && !stack.selection.quick_mask_active
                && !(active.editing_mask && active.mask.is_some())
                && (stack.layers.len() > 1
                    || stack.background.is_some()
                    || active.offset_x != 0
                    || active.offset_y != 0
                    || active.dimensions() != (stack.canvas_width, stack.canvas_height))
        };
        if !should_sample_all {
            return self.apply_editor_op(
                EditOp::FloodFill {
                    x,
                    y,
                    tolerance,
                    contiguous,
                    color,
                    opacity,
                },
                ApplyTarget::ActiveLayer,
            );
        }

        let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
        let (cw, ch) = doc.dimensions();
        let active = doc.stack().active_layer();
        if active.locked {
            return Err(ProtocolError::line(
                "layer_locked",
                UiText::new("layer_locked"),
            ));
        }
        if !active.visible {
            return Err(ProtocolError::line(
                "layer_hidden",
                UiText::new("layer_hidden"),
            ));
        }
        if x >= cw || y >= ch || !opacity.is_finite() || opacity <= 0.0 {
            return Err(ProtocolError::line("edit_noop", UiText::new("editor_noop")));
        }
        let sel_opt = doc
            .selection()
            .has_selection()
            .then(|| doc.selection().clone());
        if let Some(ref sel) = sel_opt
            && sel.alpha_at(x as i32, y as i32) == 0
        {
            return Err(ProtocolError::line("edit_noop", UiText::new("editor_noop")));
        }

        let filled = flood_fill_mask_from_composite(
            &doc.current,
            x,
            y,
            tolerance,
            contiguous,
            sel_opt.as_ref(),
        );
        let (lw, lh) = active.dimensions();
        let ox = active.offset_x;
        let oy = active.offset_y;
        let mut next = (*active.pixels).clone();
        let base_alpha = opacity.clamp(0.0, 1.0);
        let mut changed = false;

        for ly in 0..lh {
            let cy = oy + ly as i32;
            if cy < 0 || cy >= ch as i32 {
                continue;
            }
            for lx in 0..lw {
                let cx = ox + lx as i32;
                if cx < 0 || cx >= cw as i32 {
                    continue;
                }
                if !filled[(cy as usize) * (cw as usize) + (cx as usize)] {
                    continue;
                }
                let sel_factor = sel_opt
                    .as_ref()
                    .map_or(1.0, |s| f32::from(s.alpha_at(cx, cy)) / 255.0);
                let sa = base_alpha * sel_factor;
                if sa <= 0.0 {
                    continue;
                }
                let px = next.get_pixel_mut(lx, ly);
                blend_source_over_rgba(px, color, sa);
                changed = true;
            }
        }
        if !changed {
            return Err(ProtocolError::line("edit_noop", UiText::new("editor_noop")));
        }
        commit_active_layer_pixels(doc, next)?;
        Ok((Some(UiText::new("op_flood_fill")), None))
    }

    fn copy_editor_pixels(
        &mut self,
        merged: bool,
    ) -> Result<(Option<UiText>, Option<serde_json::Value>), ProtocolError> {
        let doc = self.editor.as_ref().ok_or_else(no_doc_err)?;
        let clip = extract_editor_clipboard(doc, merged)?;
        let (w, h) = (clip.width, clip.height);
        self.pixel_clipboard = Some(clip);
        Ok((
            Some(UiText::new("toast_copied")),
            Some(serde_json::json!({ "width": w, "height": h })),
        ))
    }

    fn cut_editor_pixels(
        &mut self,
    ) -> Result<(Option<UiText>, Option<serde_json::Value>), ProtocolError> {
        let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
        {
            let active = doc.stack().active_layer();
            if active.locked {
                return Err(ProtocolError::line(
                    "layer_locked",
                    UiText::new("layer_locked"),
                ));
            }
            if !active.visible {
                return Err(ProtocolError::line(
                    "layer_hidden",
                    UiText::new("layer_hidden"),
                ));
            }
        }
        let clip = extract_editor_clipboard(doc, false)?;
        let (w, h) = (clip.width, clip.height);
        if doc.selection().has_selection() {
            doc.clear_selected_pixels().map_err(map_image_err)?;
        } else {
            let (lw, lh) = doc.stack().active_layer().dimensions();
            let cleared = RgbaImage::from_pixel(lw, lh, Rgba([0, 0, 0, 0]));
            commit_active_layer_pixels(doc, cleared)?;
        }
        self.pixel_clipboard = Some(clip);
        Ok((
            Some(UiText::new("toast_cut")),
            Some(serde_json::json!({ "width": w, "height": h })),
        ))
    }

    fn paste_editor_pixels(
        &mut self,
        png_bytes: Option<&[u8]>,
        into_selection: bool,
    ) -> Result<(Option<UiText>, Option<serde_json::Value>), ProtocolError> {
        let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
        let (cw, ch) = doc.dimensions();
        let (mut img, ox, oy) = if let Some(bytes) = png_bytes {
            let matched_internal = self
                .pixel_clipboard
                .as_ref()
                .filter(|cb| cb.png.as_slice() == bytes);
            let dyn_img = image::load_from_memory(bytes)
                .map_err(|e| ProtocolError::new("decode_error", e.to_string()))?;
            let rgba = dyn_img.to_rgba8();
            photon_image::validate_dimensions(rgba.width(), rgba.height())
                .map_err(map_image_err)?;
            let (ox, oy) = if let Some(cb) = matched_internal {
                (cb.offset_x, cb.offset_y)
            } else {
                (
                    (cw as i32 - rgba.width() as i32) / 2,
                    (ch as i32 - rgba.height() as i32) / 2,
                )
            };
            (rgba, ox, oy)
        } else {
            let cb = self.pixel_clipboard.as_ref().ok_or_else(|| {
                ProtocolError::line("clipboard_empty", UiText::new("toast_clipboard_empty"))
            })?;
            let rgba =
                RgbaImage::from_raw(cb.width, cb.height, (*cb.rgba).clone()).ok_or_else(|| {
                    ProtocolError::new("invalid_clipboard", "corrupt pixel clipboard buffer")
                })?;
            (rgba, cb.offset_x, cb.offset_y)
        };

        if into_selection && doc.selection().has_selection() {
            let sel = doc.selection();
            let (iw, ih) = img.dimensions();
            for py in 0..ih {
                let cy = oy + py as i32;
                for px in 0..iw {
                    let cx = ox + px as i32;
                    let s = sel.alpha_at(cx, cy);
                    if s == 255 {
                        continue;
                    }
                    let p = img.get_pixel_mut(px, py);
                    if s == 0 {
                        p.0 = [0, 0, 0, 0];
                    } else {
                        p.0[3] = ((u16::from(p.0[3]) * u16::from(s) + 127) / 255) as u8;
                        if p.0[3] == 0 {
                            p.0 = [0, 0, 0, 0];
                        }
                    }
                }
            }
        }

        doc.apply_layer_op(LayerOperation::AddImage {
            name: "Pasted".to_owned(),
            offset_x: ox,
            offset_y: oy,
            pixels: Arc::new(img),
        })
        .map_err(map_image_err)?;

        Ok((Some(UiText::new("toast_pasted")), None))
    }

    fn paste_image_bytes(
        &mut self,
        folder: &Path,
        filename_hint: &str,
        png_bytes: &[u8],
    ) -> Result<(Option<UiText>, Option<serde_json::Value>), ProtocolError> {
        if !folder.is_dir() {
            return Err(ProtocolError::new(
                "not_found",
                format!("{} is not a directory", folder.display()),
            ));
        }
        if png_bytes.is_empty() {
            return Err(ProtocolError::new(
                "invalid_argument",
                "paste_image_bytes requires non-empty image bytes",
            ));
        }
        let is_png = matches!(image::guess_format(png_bytes), Ok(image::ImageFormat::Png));
        let out_bytes: Vec<u8> = if is_png {
            let reader = image::ImageReader::new(Cursor::new(png_bytes))
                .with_guessed_format()
                .map_err(|e| ProtocolError::new("decode_error", e.to_string()))?;
            let (w, h) = reader
                .into_dimensions()
                .map_err(|e| ProtocolError::new("decode_error", e.to_string()))?;
            photon_image::validate_dimensions(w, h).map_err(map_image_err)?;
            png_bytes.to_vec()
        } else {
            let dyn_img = image::load_from_memory(png_bytes)
                .map_err(|e| ProtocolError::new("decode_error", e.to_string()))?;
            let rgba = dyn_img.to_rgba8();
            photon_image::validate_dimensions(rgba.width(), rgba.height())
                .map_err(map_image_err)?;
            encode_rgba_to_png(&rgba)?
        };

        let target = file_ops::unique_png_path(folder, filename_hint);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|e| ProtocolError::new("io_error", e.to_string()))?;
        file.write_all(&out_bytes)
            .map_err(|e| ProtocolError::new("io_error", e.to_string()))?;
        file.sync_all()
            .map_err(|e| ProtocolError::new("io_error", e.to_string()))?;

        self.refresh_after_file_op();
        Ok((
            Some(UiText::new("toast_pasted")),
            Some(serde_json::json!({ "path": target })),
        ))
    }

    fn plugin_refresh(
        &mut self,
    ) -> Result<(Option<UiText>, Option<serde_json::Value>), ProtocolError> {
        let plugins_dir = resolve_plugins_dir(self.paths.data_dir.join("plugins"));
        if self.plugin_registry.plugins_dir() != plugins_dir {
            self.plugin_registry =
                PluginRegistry::discover(&plugins_dir).map_err(map_plugin_err)?;
        } else {
            self.plugin_registry.refresh().map_err(map_plugin_err)?;
        }
        let count = self.plugin_registry.plugins().count();
        Ok((
            Some(UiText::with(
                "plugins_refreshed",
                [("count", count.to_string())],
            )),
            Some(serde_json::json!({ "count": count })),
        ))
    }

    fn plugin_set_enabled(
        &mut self,
        id: &str,
        enabled: bool,
    ) -> Result<(Option<UiText>, Option<serde_json::Value>), ProtocolError> {
        let plugin = self.plugin_registry.get(id).ok_or_else(|| {
            ProtocolError::with_line(
                "plugin_not_found",
                format!("plugin '{id}' not found"),
                UiText::with("plugin_not_found", [("id", id)]),
            )
        })?;
        let name = plugin.manifest.name.clone();
        if enabled {
            self.config.disabled_plugins.retain(|x| x != id);
        } else if !self.config.disabled_plugins.iter().any(|x| x == id) {
            self.config.disabled_plugins.push(id.to_owned());
        }
        self.persist_config();
        let code = if enabled {
            "plugin_enabled"
        } else {
            "plugin_disabled"
        };
        Ok((Some(UiText::with(code, [("name", name)])), None))
    }

    fn plugin_install(
        &mut self,
        source_dir: &Path,
    ) -> Result<(Option<UiText>, Option<serde_json::Value>), ProtocolError> {
        let plugins_dir = resolve_plugins_dir(self.paths.data_dir.join("plugins"));
        if self.plugin_registry.plugins_dir() != plugins_dir {
            self.plugin_registry =
                PluginRegistry::discover(&plugins_dir).map_err(map_plugin_err)?;
        }
        let installed = self
            .plugin_registry
            .install(source_dir)
            .map_err(map_plugin_err)?;
        let id = installed.manifest.id.clone();
        let name = installed.manifest.name.clone();
        Ok((
            Some(UiText::with("plugin_installed", [("name", name)])),
            Some(serde_json::json!({ "id": id })),
        ))
    }

    fn plugin_uninstall(
        &mut self,
        id: &str,
    ) -> Result<(Option<UiText>, Option<serde_json::Value>), ProtocolError> {
        let plugins_dir = resolve_plugins_dir(self.paths.data_dir.join("plugins"));
        if self.plugin_registry.plugins_dir() != plugins_dir {
            self.plugin_registry =
                PluginRegistry::discover(&plugins_dir).map_err(map_plugin_err)?;
        }
        let name = self
            .plugin_registry
            .get(id)
            .map(|p| p.manifest.name.clone())
            .unwrap_or_else(|| id.to_owned());
        self.plugin_registry.uninstall(id).map_err(map_plugin_err)?;
        let prev_len = self.config.disabled_plugins.len();
        self.config.disabled_plugins.retain(|x| x != id);
        if self.config.disabled_plugins.len() != prev_len {
            self.persist_config();
        }
        Ok((
            Some(UiText::with("plugin_uninstalled", [("name", name)])),
            None,
        ))
    }

    fn plugin_run(
        &mut self,
        id: &str,
        params: &BTreeMap<String, PluginParamValue>,
        target: ApplyTarget,
    ) -> Result<(Option<UiText>, Option<serde_json::Value>), ProtocolError> {
        let plugin = self.plugin_registry.get(id).cloned().ok_or_else(|| {
            ProtocolError::with_line(
                "plugin_not_found",
                format!("plugin '{id}' not found"),
                UiText::with("plugin_not_found", [("id", id)]),
            )
        })?;
        let plugin_name = plugin.manifest.name.clone();
        if self.config.disabled_plugins.iter().any(|d| d == id) {
            return Err(ProtocolError::with_line(
                "plugin_disabled",
                format!("plugin '{id}' is disabled"),
                UiText::with("plugin_disabled", [("name", plugin_name)]),
            ));
        }

        let caps = &plugin.manifest.capabilities;
        let run_editor = if self.workspace == Workspace::Editor {
            caps.contains(&PluginCapability::EditorFilter)
        } else if caps.contains(&PluginCapability::BrowserAction) {
            false
        } else {
            caps.contains(&PluginCapability::EditorFilter)
        };

        if run_editor {
            let (input, mask_img, offset_x, offset_y, single_full_layer) = {
                let doc = self.editor.as_ref().ok_or_else(no_doc_err)?;
                let (cw, ch) = doc.dimensions();
                if target == ApplyTarget::ActiveLayer {
                    let active = doc.stack().active_layer();
                    if active.locked {
                        return Err(ProtocolError::line(
                            "layer_locked",
                            UiText::new("layer_locked"),
                        ));
                    }
                    if !active.visible {
                        return Err(ProtocolError::line(
                            "layer_hidden",
                            UiText::new("layer_hidden"),
                        ));
                    }
                    let (lw, lh) = active.dimensions();
                    let mask = doc.selection().has_selection().then(|| {
                        let mut g = GrayImage::new(lw, lh);
                        for py in 0..lh {
                            for px in 0..lw {
                                g.put_pixel(
                                    px,
                                    py,
                                    image::Luma([doc.selection().alpha_at(
                                        active.offset_x + px as i32,
                                        active.offset_y + py as i32,
                                    )]),
                                );
                            }
                        }
                        g
                    });
                    (
                        (*active.pixels).clone(),
                        mask,
                        active.offset_x,
                        active.offset_y,
                        true,
                    )
                } else {
                    let stack = doc.stack();
                    let single_full = stack.layers.len() == 1
                        && stack.background.is_none()
                        && stack.layers[0].offset_x == 0
                        && stack.layers[0].offset_y == 0
                        && stack.layers[0].dimensions() == (cw, ch);
                    if single_full && target == ApplyTarget::WholeImage {
                        let active = stack.active_layer();
                        if active.locked {
                            return Err(ProtocolError::line(
                                "layer_locked",
                                UiText::new("layer_locked"),
                            ));
                        }
                        if !active.visible {
                            return Err(ProtocolError::line(
                                "layer_hidden",
                                UiText::new("layer_hidden"),
                            ));
                        }
                    }
                    let mask = doc.selection().has_selection().then(|| {
                        GrayImage::from_raw(cw, ch, (*doc.selection().alpha).clone())
                            .unwrap_or_else(|| GrayImage::new(cw, ch))
                    });
                    ((*doc.current).clone(), mask, 0, 0, single_full)
                }
            };

            let result = run_editor_plugin(&plugin, &input, mask_img.as_ref(), params)
                .map_err(|err| plugin_exec_err(&plugin_name, err))?;

            let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
            let replace_active = target != ApplyTarget::NewLayer
                && result.mode == PluginOutputMode::ReplaceLayer
                && (target == ApplyTarget::ActiveLayer || single_full_layer);

            if replace_active {
                commit_active_layer_pixels(doc, result.image)?;
            } else {
                let layer_name = result
                    .layer_name
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| plugin_name.clone());
                doc.apply_layer_op(LayerOperation::AddImage {
                    name: layer_name,
                    offset_x,
                    offset_y,
                    pixels: Arc::new(result.image),
                })
                .map_err(map_image_err)?;
            }

            let data = result.message.map(|m| serde_json::json!({ "message": m }));
            return Ok((
                Some(UiText::with("plugin_executed", [("name", plugin_name)])),
                data,
            ));
        }

        if caps.contains(&PluginCapability::BrowserAction) {
            let folder_str = self
                .current_folder
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned());
            let result =
                run_browser_plugin(&plugin, &self.selection, folder_str.as_deref(), params)
                    .map_err(|err| plugin_exec_err(&plugin_name, err))?;
            self.refresh_after_file_op();
            let data = result.message.map(|m| serde_json::json!({ "message": m }));
            return Ok((
                Some(UiText::with("plugin_executed", [("name", plugin_name)])),
                data,
            ));
        }

        if caps.contains(&PluginCapability::ViewerAction) {
            let path = self
                .viewer_index
                .and_then(|idx| self.viewer_images.get(idx))
                .cloned()
                .ok_or_else(|| ProtocolError::new("not_found", "no image is open in Viewer"))?;
            let src_lang = self.config.translate.default_source_lang.clone();
            let tgt_lang = self.config.translate.default_target_lang.clone();
            if plugin.supports_action("viewer.translate") {
                return self.viewer_translate(&path, &src_lang, &tgt_lang, None, "tesseract");
            }
            if plugin.supports_action("viewer.ai_ocr") {
                let engine = match params.get("engine") {
                    Some(PluginParamValue::Select(s)) if !s.trim().is_empty() => s.clone(),
                    _ => "manga_ocr".to_owned(),
                };
                return self.viewer_translate(&path, &src_lang, &tgt_lang, None, &engine);
            }
            if plugin.supports_action("viewer.ocr_copy") {
                let lang = match params.get("lang") {
                    Some(PluginParamValue::Select(s)) if !s.trim().is_empty() => s.clone(),
                    _ => src_lang,
                };
                return self.viewer_translate(&path, &lang, "", None, "tesseract");
            }
        }

        Err(ProtocolError::with_line(
            "plugin_failed",
            format!("plugin '{id}' declares no runnable capability"),
            UiText::with(
                "plugin_failed",
                [
                    ("name", plugin_name),
                    ("error", "no supported capability".to_owned()),
                ],
            ),
        ))
    }
}

fn encode_rgba_to_png(img: &RgbaImage) -> Result<Vec<u8>, ProtocolError> {
    let mut buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    encoder
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| ProtocolError::new("encode_error", e.to_string()))?;
    Ok(buf)
}

fn extract_editor_clipboard(
    doc: &EditorDocument,
    merged: bool,
) -> Result<EditorPixelClipboard, ProtocolError> {
    let sel = doc.selection();
    let (img, offset_x, offset_y) = if sel.has_selection() {
        let [bx, by, bw, bh] = sel
            .bounds
            .ok_or_else(|| ProtocolError::new("invalid_selection", "missing selection bounds"))?;
        let mut out = RgbaImage::from_pixel(bw, bh, Rgba([0, 0, 0, 0]));
        if merged {
            let (cw, ch) = doc.current.dimensions();
            for sy in 0..bh {
                let cy = by + sy;
                if cy >= ch {
                    continue;
                }
                for sx in 0..bw {
                    let cx = bx + sx;
                    if cx >= cw {
                        continue;
                    }
                    let s = sel.alpha_at(cx as i32, cy as i32);
                    if s == 0 {
                        continue;
                    }
                    let mut p = doc.current.get_pixel(cx, cy).0;
                    p[3] = ((u16::from(p[3]) * u16::from(s) + 127) / 255) as u8;
                    if p[3] == 0 {
                        p = [0, 0, 0, 0];
                    }
                    out.put_pixel(sx, sy, Rgba(p));
                }
            }
        } else {
            let active = doc.stack().active_layer();
            let (lw, lh) = active.dimensions();
            for sy in 0..bh {
                let cy = by as i32 + sy as i32;
                let ly = cy - active.offset_y;
                if ly < 0 || ly >= lh as i32 {
                    continue;
                }
                for sx in 0..bw {
                    let cx = bx as i32 + sx as i32;
                    let lx = cx - active.offset_x;
                    if lx < 0 || lx >= lw as i32 {
                        continue;
                    }
                    let s = sel.alpha_at(cx, cy);
                    if s == 0 {
                        continue;
                    }
                    let mut p = active.pixels.get_pixel(lx as u32, ly as u32).0;
                    p[3] = ((u16::from(p[3]) * u16::from(s) + 127) / 255) as u8;
                    if p[3] == 0 {
                        p = [0, 0, 0, 0];
                    }
                    out.put_pixel(sx, sy, Rgba(p));
                }
            }
        }
        (out, bx as i32, by as i32)
    } else if merged {
        ((*doc.current).clone(), 0, 0)
    } else {
        let active = doc.stack().active_layer();
        ((*active.pixels).clone(), active.offset_x, active.offset_y)
    };

    let (width, height) = img.dimensions();
    let png = encode_rgba_to_png(&img)?;
    Ok(EditorPixelClipboard {
        width,
        height,
        offset_x,
        offset_y,
        rgba: Arc::new(img.into_raw()),
        png: Arc::new(png),
    })
}

pub(crate) fn commit_active_layer_pixels(
    doc: &mut EditorDocument,
    next: RgbaImage,
) -> Result<(), ProtocolError> {
    // Use Liquify live stroke so `editing_mask` is never touched while capturing `before_stack`.
    doc.live_stroke_begin(
        photon_protocol::LiveStrokeKind::Liquify {
            mode: photon_protocol::LiquifyMode::Reconstruct,
            radius: 4.0,
            strength: 0.05,
        },
        0.0,
        0.0,
    )
    .map_err(map_image_err)?;
    doc.stack.active_layer_mut().pixels = Arc::new(next);
    doc.live_stroke_end(true).map_err(map_image_err)?;
    Ok(())
}

fn flood_fill_mask_from_composite(
    src: &RgbaImage,
    x: u32,
    y: u32,
    tolerance: u8,
    contiguous: bool,
    selection: Option<&SelectionMask>,
) -> Vec<bool> {
    let (w, h) = src.dimensions();
    let total = (w as usize).saturating_mul(h as usize);
    let mut visited = vec![false; total];
    if w == 0 || h == 0 || x >= w || y >= h {
        return visited;
    }
    let tol = i32::from(tolerance);
    let seed = src.get_pixel(x, y).0;
    let matches = |px: [u8; 4]| -> bool {
        (i32::from(px[0]) - i32::from(seed[0]))
            .abs()
            .max((i32::from(px[1]) - i32::from(seed[1])).abs())
            .max((i32::from(px[2]) - i32::from(seed[2])).abs())
            .max((i32::from(px[3]) - i32::from(seed[3])).abs())
            <= tol
    };

    if !contiguous {
        for py in 0..h {
            for px in 0..w {
                if let Some(sel) = selection
                    && sel.alpha_at(px as i32, py as i32) == 0
                {
                    continue;
                }
                if matches(src.get_pixel(px, py).0) {
                    visited[(py as usize) * (w as usize) + (px as usize)] = true;
                }
            }
        }
        return visited;
    }

    let mut queue = VecDeque::with_capacity(256);
    visited[(y as usize) * (w as usize) + (x as usize)] = true;
    queue.push_back((x, y));
    while let Some((cx, cy)) = queue.pop_front() {
        let neighbors = [
            cx.checked_sub(1).map(|nx| (nx, cy)),
            (cx + 1 < w).then_some((cx + 1, cy)),
            cy.checked_sub(1).map(|ny| (cx, ny)),
            (cy + 1 < h).then_some((cx, cy + 1)),
        ];
        for (nx, ny) in neighbors.into_iter().flatten() {
            let nidx = (ny as usize) * (w as usize) + (nx as usize);
            if visited[nidx] {
                continue;
            }
            if let Some(sel) = selection
                && sel.alpha_at(nx as i32, ny as i32) == 0
            {
                continue;
            }
            if matches(src.get_pixel(nx, ny).0) {
                visited[nidx] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    visited
}

fn blend_source_over_rgba(dst: &mut Rgba<u8>, color: [u8; 3], sa: f32) {
    let sa = sa.clamp(0.0, 1.0);
    if sa <= 0.0 {
        return;
    }
    let da = f32::from(dst.0[3]) / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 1e-6 {
        dst.0 = [0, 0, 0, 0];
        return;
    }
    let inv_sa = 1.0 - sa;
    let mix = |sc: u8, dc: u8| -> u8 {
        ((f32::from(sc) * sa + f32::from(dc) * da * inv_sa) / out_a)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    dst.0 = [
        mix(color[0], dst.0[0]),
        mix(color[1], dst.0[1]),
        mix(color[2], dst.0[2]),
        (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
    ];
}

fn plugin_exec_err(plugin_name: &str, err: PluginError) -> ProtocolError {
    ProtocolError::with_line(
        "plugin_failed",
        err.to_string(),
        UiText::with(
            "plugin_failed",
            [("name", plugin_name), ("error", &err.to_string())],
        ),
    )
}

pub(crate) fn map_plugin_err(err: PluginError) -> ProtocolError {
    match err {
        PluginError::NotFound { id } => ProtocolError::with_line(
            "plugin_not_found",
            format!("plugin '{id}' not found"),
            UiText::with("plugin_not_found", [("id", id)]),
        ),
        PluginError::InvalidManifest { .. }
        | PluginError::InvalidPluginId { .. }
        | PluginError::PathTraversal { .. }
        | PluginError::EntryNotFound { .. } => {
            let msg = err.to_string();
            ProtocolError::with_line(
                "plugin_invalid_manifest",
                msg.clone(),
                UiText::with("plugin_invalid_manifest", [("error", msg)]),
            )
        }
        other => ProtocolError::new("plugin_error", other.to_string()),
    }
}

pub(crate) fn no_doc_err() -> ProtocolError {
    ProtocolError::line("no_document", UiText::new("needs_a_document"))
}

fn planned_archive_page_path(session: &ArchiveSession, index: usize) -> PathBuf {
    match session.kind() {
        ArchiveKind::ZipCbz => {
            let label = session
                .pages()
                .get(index)
                .map(|p| p.label.as_str())
                .unwrap_or("page");
            session.temp_dir().join(sanitize_zip_filename(index, label))
        }
        ArchiveKind::Pdf => session
            .temp_dir()
            .join(format!("page-{:04}.png", index + 1)),
    }
}

fn find_archive_page_index(session: &ArchiveSession, path: &Path) -> Option<usize> {
    if !path.starts_with(session.temp_dir()) {
        return None;
    }
    (0..session.page_count()).find(|&idx| planned_archive_page_path(session, idx) == path)
}

pub(crate) fn map_image_err(e: ImageError) -> ProtocolError {
    match e {
        ImageError::Coded { code, ui } => ProtocolError::line(code, ui),
        other => ProtocolError::new("invalid_edit", other.to_string()),
    }
}

fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

fn build_selection_info(path: &Path) -> Option<SelectionInfo> {
    let meta = fs::metadata(path).ok()?;
    let is_dir = meta.is_dir();
    let is_image = !is_dir && is_supported_image(path);
    let dims = if is_image {
        photon_image::probe_dimensions(path)
    } else {
        None
    };
    Some(SelectionInfo {
        path: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned(),
        kind: if is_dir {
            "folder".to_owned()
        } else {
            path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("file")
                .to_ascii_uppercase()
        },
        size: meta.len(),
        is_image,
        width: dims.map(|(w, _)| w),
        height: dims.map(|(_, h)| h),
    })
}

fn build_breadcrumbs(path: &Path) -> Vec<Crumb> {
    let mut out = Vec::new();
    let mut acc = PathBuf::new();
    for comp in path.components() {
        acc.push(comp);
        let name = match comp {
            Component::RootDir => "/".to_owned(),
            _ => comp.as_os_str().to_string_lossy().into_owned(),
        };
        out.push(Crumb {
            name,
            path: acc.to_string_lossy().into_owned(),
        });
    }
    out
}

impl Drop for Core {
    fn drop(&mut self) {
        self.clear_archive_session();
        self.persist_ui_state(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use photon_protocol::{ClientKind, Envelope};

    #[test]
    fn gateway_executes_open_folder_viewer_editor_and_records_journal() {
        let dir = std::env::temp_dir().join(format!("photon-core-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let photos = dir.join("photos");
        fs::create_dir_all(&photos).unwrap();
        let img_path = photos.join("sample.png");
        RgbaImage::from_pixel(32, 16, Rgba([200, 100, 50, 255]))
            .save(&img_path)
            .unwrap();

        let mut core = Core::for_sandbox(&dir.join("sandbox"));
        let r1 = core.dispatch(Envelope::new(
            "c1",
            ClientKind::Cli,
            Command::OpenFolder {
                path: photos.to_string_lossy().into_owned(),
            },
        ));
        assert!(r1.ok);
        assert_eq!(core.snapshot().entries.len(), 1);

        let r2 = core.dispatch(Envelope::new(
            "c2",
            ClientKind::Cli,
            Command::EditorOpen {
                path: img_path.to_string_lossy().into_owned(),
            },
        ));
        assert!(r2.ok);
        assert_eq!(core.workspace(), Workspace::Editor);

        let r3 = core.dispatch(Envelope::new(
            "c3",
            ClientKind::Cli,
            Command::EditorRotate {
                degrees: 90,
                target: ApplyTarget::WholeImage,
            },
        ));
        assert!(r3.ok);
        assert_eq!(core.editor_document().unwrap().dimensions(), (16, 32));

        let journal_records = Journal::read_records(&core.paths.journal_file(), None).unwrap();
        assert_eq!(journal_records.len(), 3);
        assert_eq!(journal_records[2].command_name, "editor_rotate");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn context_guards_and_root_folder_subtree_work() {
        let dir = std::env::temp_dir().join(format!("photon-guard-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let root = dir.join("root");
        let sub = root.join("sub");
        let empty = dir.join("empty");
        fs::create_dir_all(&sub).unwrap();
        fs::create_dir_all(&empty).unwrap();
        let img = sub.join("pic.png");
        RgbaImage::from_pixel(12, 8, Rgba([10, 20, 30, 255]))
            .save(&img)
            .unwrap();

        let mut core = Core::for_sandbox(&dir.join("sandbox"));

        // Empty workspace rejects Viewer and Editor
        let r_view = core.dispatch(Envelope::new(
            "g1",
            ClientKind::Cli,
            Command::SetWorkspace {
                workspace: Workspace::Viewer,
            },
        ));
        assert!(!r_view.ok);
        assert_eq!(r_view.error.unwrap().code, "no_images");

        let r_edit = core.dispatch(Envelope::new(
            "g2",
            ClientKind::Cli,
            Command::SetWorkspace {
                workspace: Workspace::Editor,
            },
        ));
        assert!(!r_edit.ok);
        assert_eq!(r_edit.error.unwrap().code, "no_document");

        // OpenFolder sets root_folder; Navigate inside subtree preserves root_folder
        core.dispatch(Envelope::new(
            "g3",
            ClientKind::Cli,
            Command::OpenFolder {
                path: root.to_string_lossy().into_owned(),
            },
        ));
        assert_eq!(
            core.meta().root_folder.as_deref(),
            Some(root.to_string_lossy().as_ref())
        );

        core.dispatch(Envelope::new(
            "g4",
            ClientKind::Cli,
            Command::Navigate {
                path: sub.to_string_lossy().into_owned(),
            },
        ));
        assert_eq!(
            core.meta().root_folder.as_deref(),
            Some(root.to_string_lossy().as_ref())
        );
        assert_eq!(
            core.meta().current_folder.as_deref(),
            Some(sub.to_string_lossy().as_ref())
        );

        // Open image -> Viewer -> Edit -> Back -> Back
        core.dispatch(Envelope::new(
            "g5",
            ClientKind::Cli,
            Command::ViewerOpen {
                path: img.to_string_lossy().into_owned(),
            },
        ));
        assert_eq!(core.workspace(), Workspace::Viewer);

        let r_to_ed = core.dispatch(Envelope::new(
            "g6",
            ClientKind::Cli,
            Command::SetWorkspace {
                workspace: Workspace::Editor,
            },
        ));
        assert!(r_to_ed.ok);
        assert_eq!(core.workspace(), Workspace::Editor);
        assert!(core.editor_document().is_some());

        core.dispatch(Envelope::new("g7", ClientKind::Cli, Command::WorkspaceBack));
        assert_eq!(core.workspace(), Workspace::Viewer);
        core.dispatch(Envelope::new("g8", ClientKind::Cli, Command::WorkspaceBack));
        assert_eq!(core.workspace(), Workspace::Browser);

        // Navigating outside root re-roots
        core.dispatch(Envelope::new(
            "g9",
            ClientKind::Cli,
            Command::Navigate {
                path: empty.to_string_lossy().into_owned(),
            },
        ));
        assert_eq!(
            core.meta().root_folder.as_deref(),
            Some(empty.to_string_lossy().as_ref())
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn startup_opens_in_browser_and_restores_folder_and_layout() {
        let dir = std::env::temp_dir().join(format!("photon-core-session-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let photos = dir.join("photos");
        let sandbox = dir.join("sandbox");
        fs::create_dir_all(&photos).unwrap();
        let a = photos.join("a.png");
        let b = photos.join("b.png");
        RgbaImage::from_pixel(10, 10, Rgba([20, 30, 40, 255]))
            .save(&a)
            .unwrap();
        RgbaImage::from_pixel(14, 10, Rgba([50, 60, 70, 255]))
            .save(&b)
            .unwrap();

        {
            let mut core = Core::for_sandbox(&sandbox);
            assert!(
                core.dispatch(Envelope::new(
                    "s1",
                    ClientKind::Cli,
                    Command::OpenFolder {
                        path: photos.to_string_lossy().into_owned(),
                    },
                ))
                .ok
            );
            assert!(
                core.dispatch(Envelope::new(
                    "s2",
                    ClientKind::Cli,
                    Command::ViewerOpen {
                        path: b.to_string_lossy().into_owned(),
                    },
                ))
                .ok
            );
            let mut layout = core.meta().layout;
            layout.window_width = 960;
            layout.window_height = 640;
            layout.browser_view_mode = photon_protocol::BrowserViewMode::List;
            assert!(
                core.dispatch(Envelope::new(
                    "s3",
                    ClientKind::Gui,
                    Command::SetLayout { layout },
                ))
                .ok
            );
        }

        let restored = Core::for_sandbox(&sandbox);
        assert_eq!(restored.workspace(), Workspace::Browser);
        assert_eq!(
            restored.meta().viewer_path.as_deref(),
            Some(a.to_string_lossy().as_ref())
        );
        assert_eq!(
            restored.meta().current_folder.as_deref(),
            Some(photos.to_string_lossy().as_ref())
        );
        assert_eq!(restored.meta().layout.window_width, 960);
        assert_eq!(restored.meta().layout.window_height, 640);
        assert_eq!(
            restored.meta().layout.browser_view_mode,
            photon_protocol::BrowserViewMode::List
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn editor_layers_geometry_adjustments_live_stroke_history_and_export() {
        let dir = std::env::temp_dir().join(format!("photon-core-editor-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("photo.png");
        RgbaImage::from_pixel(32, 24, Rgba([100, 140, 180, 255]))
            .save(&src)
            .unwrap();

        let mut core = Core::for_sandbox(&dir.join("sandbox"));
        assert!(
            core.dispatch(Envelope::new(
                "e1",
                ClientKind::Gui,
                Command::EditorOpen {
                    path: src.to_string_lossy().into_owned(),
                },
            ))
            .ok
        );

        // Layer stack operations + error mappings
        let base_id = core.meta().editor.as_ref().unwrap().active_layer_id;
        let del_last = core.dispatch(Envelope::new(
            "e-del-last",
            ClientKind::Gui,
            Command::EditorLayerDelete { id: base_id },
        ));
        assert!(!del_last.ok);
        assert_eq!(del_last.error.unwrap().code, "cannot_delete_last_layer");

        let merge_bot = core.dispatch(Envelope::new(
            "e-merge-bot",
            ClientKind::Gui,
            Command::EditorLayerMergeDown { id: base_id },
        ));
        assert!(!merge_bot.ok);
        assert_eq!(merge_bot.error.unwrap().code, "cannot_merge_bottom_layer");

        assert!(
            core.dispatch(Envelope::new(
                "e2",
                ClientKind::Gui,
                Command::EditorLayerAdd {
                    name: "Paint".into(),
                },
            ))
            .ok
        );
        let paint_id = core.meta().editor.as_ref().unwrap().active_layer_id;
        assert_eq!(core.meta().editor.as_ref().unwrap().layers.len(), 2);

        // Lock active layer -> live stroke must reject with `layer_locked`
        assert!(
            core.dispatch(Envelope::new(
                "e3",
                ClientKind::Gui,
                Command::EditorLayerSetProps {
                    id: paint_id,
                    name: None,
                    visible: None,
                    locked: Some(true),
                    opacity: None,
                    blend_mode: None,
                    offset_x: None,
                    offset_y: None,
                },
            ))
            .ok
        );
        let locked_stroke = core.dispatch(Envelope::new(
            "e-locked",
            ClientKind::Gui,
            Command::EditorLiveStrokeBegin {
                stroke: photon_protocol::LiveStrokeKind::Paint {
                    color: [255, 0, 0],
                    radius: 2.0,
                    opacity: 1.0,
                    erase: false,
                    brush: photon_protocol::BrushSettings::default(),
                },
                x: 4.0,
                y: 4.0,
            },
        ));
        assert!(!locked_stroke.ok);
        assert_eq!(locked_stroke.error.unwrap().code, "layer_locked");

        // Unlock, paint live stroke, commit
        assert!(
            core.dispatch(Envelope::new(
                "e4",
                ClientKind::Gui,
                Command::EditorLayerSetProps {
                    id: paint_id,
                    name: None,
                    visible: None,
                    locked: Some(false),
                    opacity: Some(0.8),
                    blend_mode: Some(photon_protocol::BlendMode::Multiply),
                    offset_x: None,
                    offset_y: None,
                },
            ))
            .ok
        );
        assert!(
            core.dispatch(Envelope::new(
                "e5",
                ClientKind::Gui,
                Command::EditorLiveStrokeBegin {
                    stroke: photon_protocol::LiveStrokeKind::Paint {
                        color: [220, 40, 40],
                        radius: 2.0,
                        opacity: 1.0,
                        erase: false,
                        brush: photon_protocol::BrushSettings::default(),
                    },
                    x: 4.0,
                    y: 4.0,
                },
            ))
            .ok
        );
        assert!(
            core.dispatch(Envelope::new(
                "e6",
                ClientKind::Gui,
                Command::EditorLiveStrokeStep {
                    points: vec![[10.0, 10.0]],
                },
            ))
            .ok
        );
        assert!(
            core.dispatch(Envelope::new(
                "e7",
                ClientKind::Gui,
                Command::EditorLiveStrokeEnd { commit: true },
            ))
            .ok
        );

        // Color & Light, HSV, Color Balance, Curves, Canvas Size, RotateAngle
        assert!(
            core.dispatch(Envelope::new(
                "e8",
                ClientKind::Gui,
                Command::EditorAdjustColor {
                    brightness: 0.05,
                    contrast: 0.1,
                    saturation: 0.1,
                    exposure: 0.25,
                    highlights: -0.1,
                    shadows: 0.15,
                    gamma: 1.2,
                    target: ApplyTarget::WholeImage,
                },
            ))
            .ok
        );
        assert!(
            core.dispatch(Envelope::new(
                "e9",
                ClientKind::Gui,
                Command::EditorAdjustHsv {
                    hue: 15.0,
                    saturation: 0.1,
                    value: 0.0,
                    target: ApplyTarget::WholeImage,
                },
            ))
            .ok
        );
        assert!(
            core.dispatch(Envelope::new(
                "e10",
                ClientKind::Gui,
                Command::EditorColorBalance {
                    shadows: [0.05, 0.0, -0.05],
                    midtones: [0.0, 0.05, 0.0],
                    highlights: [0.0, 0.0, 0.05],
                    preserve_luminosity: true,
                    target: ApplyTarget::WholeImage,
                },
            ))
            .ok
        );
        assert!(
            core.dispatch(Envelope::new(
                "e11",
                ClientKind::Gui,
                Command::EditorAdjustCurves {
                    rgb: vec![[0.0, 0.0], [0.5, 0.6], [1.0, 1.0]],
                    red: vec![],
                    green: vec![],
                    blue: vec![],
                    target: ApplyTarget::WholeImage,
                },
            ))
            .ok
        );
        assert!(
            core.dispatch(Envelope::new(
                "e12",
                ClientKind::Gui,
                Command::EditorCanvasSize {
                    width: 48,
                    height: 32,
                    anchor: photon_protocol::CanvasAnchor::Center,
                    fill: Some([255, 255, 255]),
                    fill_alpha: 255,
                },
            ))
            .ok
        );
        assert!(
            core.dispatch(Envelope::new(
                "e13",
                ClientKind::Gui,
                Command::EditorRotateAngle {
                    degrees: 15.0,
                    fill: None,
                    fill_alpha: 0,
                    target: ApplyTarget::WholeImage,
                },
            ))
            .ok
        );

        // History jump back and forward
        let total_steps = core.meta().editor.as_ref().unwrap().history.len();
        assert!(total_steps >= 8);
        assert!(
            core.dispatch(Envelope::new(
                "e14",
                ClientKind::Gui,
                Command::EditorHistoryJump { target_index: 2 },
            ))
            .ok
        );
        let meta_after_jump = core.meta().editor.unwrap();
        assert_eq!(meta_after_jump.history.len(), 2);
        assert_eq!(meta_after_jump.redo_history.len(), total_steps - 2);

        // Save creates .bak and Export creates a separate file
        assert!(
            core.dispatch(Envelope::new("e15", ClientKind::Gui, Command::EditorSave))
                .ok
        );
        assert!(dir.join("photo.png.bak").exists());

        let export_path = dir.join("photo-web.jpg");
        assert!(
            core.dispatch(Envelope::new(
                "e16",
                ClientKind::Gui,
                Command::EditorExport {
                    path: export_path.to_string_lossy().into_owned(),
                    quality: Some(85),
                    metadata: true,
                    max_edge: Some(24),
                },
            ))
            .ok
        );
        assert!(export_path.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn protocol_schemas_generates_successfully() {
        let schemas = protocol_schemas();
        assert_eq!(schemas["protocol_version"], 1);
        assert!(schemas["command"].is_object());
    }

    #[test]
    fn layer_masks_liquify_favorites_file_ops_and_layout_clamping() {
        let dir = std::env::temp_dir().join(format!("photon-wave1-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let photos = dir.join("photos");
        let dest = dir.join("dest");
        fs::create_dir_all(&photos).unwrap();
        fs::create_dir_all(&dest).unwrap();
        let img = photos.join("pic.png");
        RgbaImage::from_pixel(16, 16, Rgba([180, 90, 45, 255]))
            .save(&img)
            .unwrap();

        let mut core = Core::for_sandbox(&dir.join("sandbox"));

        // Layout clamping for editor_right_width & editor_right_split_pct
        let mut layout = core.meta().layout;
        layout.editor_right_width = 999;
        layout.editor_right_split_pct = 5;
        assert!(
            core.dispatch(Envelope::new(
                "l1",
                ClientKind::Gui,
                Command::SetLayout { layout },
            ))
            .ok
        );
        assert_eq!(core.meta().layout.editor_right_width, 560);
        assert_eq!(core.meta().layout.editor_right_split_pct, 20);

        // Favorites + Rename + CopyTo + OpenFolder sorting
        let initial_fav_count = core.meta().favorites.len();
        assert!(
            core.dispatch(Envelope::new(
                "f1",
                ClientKind::Gui,
                Command::AddFavorite {
                    path: photos.to_string_lossy().into_owned(),
                },
            ))
            .ok
        );
        assert_eq!(core.meta().favorites.len(), initial_fav_count + 1);
        assert!(
            core.meta()
                .favorites
                .iter()
                .any(|f| f == photos.to_string_lossy().as_ref())
        );
        assert!(
            core.dispatch(Envelope::new(
                "f2",
                ClientKind::Gui,
                Command::CopyTo {
                    paths: vec![img.to_string_lossy().into_owned()],
                    destination: dest.to_string_lossy().into_owned(),
                },
            ))
            .ok
        );
        assert!(dest.join("pic.png").exists());
        assert!(
            core.dispatch(Envelope::new(
                "f3",
                ClientKind::Gui,
                Command::RenamePath {
                    path: dest.join("pic.png").to_string_lossy().into_owned(),
                    new_name: "renamed.png".into(),
                },
            ))
            .ok
        );
        assert!(dest.join("renamed.png").exists());

        // Editor Masks + Liquify
        assert!(
            core.dispatch(Envelope::new(
                "m1",
                ClientKind::Gui,
                Command::EditorOpen {
                    path: img.to_string_lossy().into_owned(),
                },
            ))
            .ok
        );
        let lid = core.meta().editor.as_ref().unwrap().active_layer_id;
        assert!(
            core.dispatch(Envelope::new(
                "m2",
                ClientKind::Gui,
                Command::EditorLayerMaskAdd {
                    id: lid,
                    reveal_all: true,
                },
            ))
            .ok
        );
        assert!(core.meta().editor.as_ref().unwrap().layers[0].has_mask);
        assert!(
            core.dispatch(Envelope::new(
                "m3",
                ClientKind::Gui,
                Command::EditorLayerMaskInvert { id: lid },
            ))
            .ok
        );
        assert!(
            core.dispatch(Envelope::new(
                "m4",
                ClientKind::Gui,
                Command::EditorLayerMaskDelete { id: lid },
            ))
            .ok
        );
        assert!(!core.meta().editor.as_ref().unwrap().layers[0].has_mask);

        assert!(
            core.dispatch(Envelope::new(
                "q1",
                ClientKind::Gui,
                Command::EditorLiquifyStroke {
                    points: vec![[4.0, 4.0], [10.0, 10.0]],
                    mode: photon_protocol::LiquifyMode::Bloat,
                    radius: 6.0,
                    strength: 0.6,
                },
            ))
            .ok
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn translate_settings_and_sidecar_and_ai_plugins_work() {
        let dir = std::env::temp_dir().join(format!("photon-ai-core-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let img = dir.join("pic.png");
        RgbaImage::from_pixel(12, 12, Rgba([10, 20, 30, 255]))
            .save(&img)
            .unwrap();
        let img_str = img.to_string_lossy().into_owned();

        let sandbox = dir.join("sandbox");
        let plugins_dir = sandbox.join("data/plugins");
        let models_dir = sandbox.join("data/models");
        fs::create_dir_all(plugins_dir.join("photon.mock.ai/bin")).unwrap();
        fs::create_dir_all(&models_dir).unwrap();
        fs::write(models_dir.join("rmbg-1.4.onnx"), b"weights").unwrap();

        let worker = r#"
import json, shutil, sys
for line in sys.stdin:
    msg = json.loads(line)
    if msg.get("op") == "shutdown":
        break
    if msg.get("op") == "run":
        act = msg["action"]
        if act == "viewer.translate":
            print(json.dumps({
                "id": msg["id"],
                "event": "done",
                "meta": {
                    "device": "cpu",
                    "translation": {
                        "path": msg["input"],
                        "sourceLang": "jpn",
                        "targetLang": "en",
                        "originalText": "こんにちは",
                        "translatedText": "Hello",
                        "blocks": [{"x": 1, "y": 2, "w": 8, "h": 6, "original": "こんにちは", "translated": "Hello"}]
                    }
                }
            }), flush=True)
        elif act == "editor.caption":
            print(json.dumps({
                "id": msg["id"],
                "event": "done",
                "meta": { "captionText": "1girl, solo, smile" }
            }), flush=True)
        else:
            shutil.copyfile(msg["input"], msg["output"])
            print(json.dumps({"id": msg["id"], "event": "done", "meta": {"device": "cpu"}}), flush=True)
"#;
        fs::write(plugins_dir.join("photon.mock.ai/bin/worker.py"), worker).unwrap();
        fs::write(
            plugins_dir.join("photon.mock.ai/manifest.json"),
            r#"{
                "apiVersion": 1,
                "id": "photon.mock.ai",
                "name": "Mock AI Suite",
                "version": "1.0.0",
                "entry": { "command": "bin/worker.py", "args": [], "protocol": "jsonl-stdio" },
                "capabilities": [
                    { "action": "editor.remove_background", "workspace": "editor", "label": "Remove BG", "output": "rgba_image" },
                    { "action": "editor.caption", "workspace": "editor", "label": "Caption", "output": "text" },
                    { "action": "viewer.translate", "workspace": "viewer", "label": "Translate", "output": "translation" }
                ],
                "quickActions": [
                    { "id": "qa-rembg", "label": "Quick Remove BG", "action": "editor.remove_background", "workspaces": ["browser", "viewer"], "params": { "modelId": "rmbg-1.4" } }
                ]
            }"#,
        )
        .unwrap();

        let mut core = Core::for_sandbox(&sandbox);
        assert_eq!(core.meta().quick_actions.len(), 1);
        assert!(
            core.meta()
                .ai_models
                .iter()
                .find(|m| m.id == "rmbg-1.4")
                .is_some_and(|m| m.installed)
        );

        // 1. SetTranslateSettings persists
        let mut tset = photon_protocol::TranslateSettings::default();
        tset.provider = photon_protocol::TranslateProvider::GoogleWeb;
        let resp = core.dispatch(Envelope::new(
            "ts-1",
            ClientKind::Cli,
            Command::SetTranslateSettings {
                settings: tset.clone(),
            },
        ));
        assert!(resp.ok);
        assert_eq!(core.meta().translate_settings, tset);

        // 2. AiSetSettings persists
        let resp = core.dispatch(Envelope::new(
            "ai-set",
            ClientKind::Cli,
            Command::AiSetSettings {
                device: photon_protocol::AiDevice::Auto,
                auto_unload_secs: 120,
            },
        ));
        assert!(resp.ok);
        assert_eq!(
            core.meta().ai_settings.device,
            photon_protocol::AiDevice::Auto
        );
        assert_eq!(core.meta().ai_settings.auto_unload_secs, 120);

        // 3. ViewerTranslate routes to plugin & ViewerSaveTranslation / DeleteTranslation persist sidecar
        let resp = core.dispatch(Envelope::new(
            "tr-1",
            ClientKind::Cli,
            Command::ViewerTranslate {
                path: img_str.clone(),
                source_lang: "jpn".into(),
                target_lang: "en".into(),
                region: None,
                ocr_engine: "tesseract".into(),
            },
        ));
        assert!(resp.ok);
        assert!(core.run_pending_ai_blocking());
        assert_eq!(
            core.meta()
                .translation
                .as_ref()
                .map(|t| t.translated_text.as_str()),
            Some("Hello")
        );

        let block = photon_protocol::TranslationBlock {
            x: 1,
            y: 2,
            w: 8,
            h: 6,
            original: "こんにちは".into(),
            translated: "Hello".into(),
        };
        assert!(
            core.dispatch(Envelope::new(
                "tr-save",
                ClientKind::Cli,
                Command::ViewerSaveTranslation {
                    path: img_str.clone(),
                    blocks: vec![block],
                    source_lang: "jpn".into(),
                    target_lang: "en".into(),
                },
            ))
            .ok
        );
        assert!(
            core.dispatch(Envelope::new(
                "vo-1",
                ClientKind::Cli,
                Command::ViewerOpen {
                    path: img_str.clone(),
                },
            ))
            .ok
        );
        assert_eq!(
            core.meta()
                .viewer_saved_translation
                .as_ref()
                .map(|s| s.blocks.len()),
            Some(1)
        );
        assert!(
            core.dispatch(Envelope::new(
                "tr-del",
                ClientKind::Cli,
                Command::ViewerDeleteTranslation {
                    path: img_str.clone(),
                },
            ))
            .ok
        );
        assert!(core.meta().viewer_saved_translation.is_none());

        // 4. QuickActionRun backs up original and succeeds
        assert!(
            core.dispatch(Envelope::new(
                "qa-1",
                ClientKind::Cli,
                Command::QuickActionRun {
                    plugin_id: "photon.mock.ai".into(),
                    action_id: "qa-rembg".into(),
                    paths: vec![img_str.clone()],
                },
            ))
            .ok
        );
        assert!(core.run_pending_ai_blocking());
        assert!(dir.join("pic.png~").is_file());

        // 5. Editor: RemoveBG + RefineAlpha + GenerateCaption + SaveCaption
        assert!(
            core.dispatch(Envelope::new(
                "ed-1",
                ClientKind::Cli,
                Command::EditorOpen { path: img_str },
            ))
            .ok
        );
        assert!(
            core.dispatch(Envelope::new(
                "ed-rembg",
                ClientKind::Cli,
                Command::EditorAiRemoveBackground {
                    model_id: "rmbg-1.4".into(),
                    feather: 2,
                    target: ApplyTarget::ActiveLayer,
                },
            ))
            .ok
        );
        assert!(core.run_pending_ai_blocking());
        assert!(
            core.dispatch(Envelope::new(
                "ed-ref",
                ClientKind::Cli,
                Command::EditorRefineAlpha {
                    erase_strokes: vec![photon_protocol::AiMaskStroke {
                        radius: 2.0,
                        points: vec![[4.0, 4.0]],
                    }],
                    restore_strokes: vec![],
                    softness: 0.5,
                },
            ))
            .ok
        );
        assert!(
            core.dispatch(Envelope::new(
                "ed-cap",
                ClientKind::Cli,
                Command::EditorGenerateCaption {
                    model_id: "wd-vit-tagger-v3".into(),
                    general_threshold: 0.35,
                    character_threshold: 0.85,
                    replace_underscores: true,
                    prefix: String::new(),
                    suffix: String::new(),
                    exclude_tags: String::new(),
                },
            ))
            .ok
        );
        assert!(core.run_pending_ai_blocking());
        assert_eq!(
            core.meta().editor_caption.as_deref(),
            Some("1girl, solo, smile")
        );
        assert!(
            core.dispatch(Envelope::new(
                "ed-capsave",
                ClientKind::Cli,
                Command::EditorSaveCaption {
                    text: "1girl, solo, smile, outdoors".into(),
                },
            ))
            .ok
        );
        assert_eq!(
            fs::read_to_string(dir.join("pic.txt")).unwrap(),
            "1girl, solo, smile, outdoors"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn layout_defaults_and_manual_browser_inspector_persistence_work() {
        let dir = std::env::temp_dir().join(format!("photon-layout-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let photos = dir.join("photos");
        let sandbox = dir.join("sandbox");
        fs::create_dir_all(&photos).unwrap();
        let img = photos.join("a.png");
        RgbaImage::from_pixel(10, 10, Rgba([20, 30, 40, 255]))
            .save(&img)
            .unwrap();
        let img_str = img.to_string_lossy().into_owned();

        {
            let mut core = Core::for_sandbox(&sandbox);
            assert!(!core.meta().layout.panel("viewer.filmstrip"));
            assert!(!core.meta().layout.panel("browser.inspector"));

            // OpenFolder and Select never force-open browser.inspector.
            assert!(
                core.dispatch(Envelope::new(
                    "l1",
                    ClientKind::Gui,
                    Command::OpenFolder {
                        path: photos.to_string_lossy().into_owned(),
                    },
                ))
                .ok
            );
            assert!(!core.meta().layout.panel("browser.inspector"));
            assert!(
                core.dispatch(Envelope::new(
                    "l2",
                    ClientKind::Gui,
                    Command::Select {
                        paths: vec![img_str.clone()],
                    },
                ))
                .ok
            );
            assert!(!core.meta().layout.panel("browser.inspector"));

            // Manual toggle turns it on, and subsequent OpenFolder/Select never close it.
            assert!(
                core.dispatch(Envelope::new(
                    "l3",
                    ClientKind::Gui,
                    Command::ToggleRightPanel {
                        workspace: Some(Workspace::Browser),
                    },
                ))
                .ok
            );
            assert!(core.meta().layout.panel("browser.inspector"));
            assert!(
                core.dispatch(Envelope::new(
                    "l4",
                    ClientKind::Gui,
                    Command::OpenFolder {
                        path: photos.to_string_lossy().into_owned(),
                    },
                ))
                .ok
            );
            assert!(core.meta().layout.panel("browser.inspector"));

            // Persist Editor right-sidebar order and active tool tab via SetLayout.
            let mut layout = core.meta().layout;
            layout.editor_right_panel_order = vec![
                photon_protocol::EditorRightPanel::Layers,
                photon_protocol::EditorRightPanel::ToolOptionsHistory,
            ];
            layout.editor_tool_tab = photon_protocol::EditorToolTab::History;
            assert!(
                core.dispatch(Envelope::new(
                    "l5",
                    ClientKind::Gui,
                    Command::SetLayout { layout },
                ))
                .ok
            );
        }

        let restored = Core::for_sandbox(&sandbox);
        assert!(restored.meta().layout.panel("browser.inspector"));
        assert!(!restored.meta().layout.panel("viewer.filmstrip"));
        assert_eq!(
            restored.meta().layout.editor_right_panel_order,
            vec![
                photon_protocol::EditorRightPanel::Layers,
                photon_protocol::EditorRightPanel::ToolOptionsHistory,
            ]
        );
        assert_eq!(
            restored.meta().layout.editor_tool_tab,
            photon_protocol::EditorToolTab::History
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn first_launch_seeds_default_quick_access_places() {
        let dir = std::env::temp_dir().join(format!("photon-qa-seed-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let core = Core::for_sandbox(&dir.join("sandbox"));
        let favs = &core.config.favorites;

        // Home directory is always the first entry.
        let home = photon_storage::paths::home_dir();
        assert!(!favs.is_empty(), "favorites must be seeded on first launch");
        assert_eq!(
            favs[0],
            home.to_string_lossy(),
            "first favorite must be the home directory"
        );

        // favorites_initialized flag must be set so the defaults don't reappear
        // after the user clears them.
        assert!(
            core.config.favorites_initialized,
            "favorites_initialized must be set after seeding"
        );

        // All seeded paths must be real directories (home is always present;
        // Pictures/Downloads/Desktop are added only when they exist).
        for fav in favs {
            assert!(
                std::path::Path::new(fav).is_dir(),
                "seeded favorite {fav:?} is not a directory"
            );
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn second_launch_does_not_re_seed_after_user_clears_favorites() {
        let dir =
            std::env::temp_dir().join(format!("photon-qa-noseed-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let sandbox = dir.join("sandbox");

        // First launch seeds defaults.
        {
            let core = Core::for_sandbox(&sandbox);
            assert!(!core.config.favorites.is_empty());
            assert!(core.config.favorites_initialized);
        }

        // Simulate user clearing all favorites via the config file.
        {
            let config_path = AppPaths::for_sandbox(&sandbox).config_file();
            let (mut cfg, _) = photon_storage::Config::load_or_create(&config_path);
            cfg.favorites.clear();
            // favorites_initialized remains true – that's the key invariant.
            assert!(cfg.favorites_initialized);
            cfg.save(&config_path).unwrap();
        }

        // Second launch must NOT re-seed because favorites_initialized is still true.
        let core2 = Core::for_sandbox(&sandbox);
        assert!(
            core2.config.favorites.is_empty(),
            "favorites must stay empty when user cleared them and favorites_initialized is set"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_favorite_up_and_down_on_seeded_defaults() {
        use photon_protocol::MoveDirection;

        let dir = std::env::temp_dir().join(format!("photon-qa-move-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let mut core = Core::for_sandbox(&dir.join("sandbox"));
        // Need at least 2 seeded favorites to test movement.
        if core.config.favorites.len() < 2 {
            let _ = fs::remove_dir_all(&dir);
            return; // CI environment with no subdirectories – skip gracefully.
        }

        let initial: Vec<String> = core.config.favorites.clone();
        let first = initial[0].clone();
        let second = initial[1].clone();

        // Move the first item Down – it should swap with the second.
        let r = core.dispatch(photon_protocol::Envelope::new(
            "mv1",
            photon_protocol::ClientKind::Cli,
            Command::MoveFavorite {
                path: first.clone(),
                direction: MoveDirection::Down,
            },
        ));
        assert!(r.ok, "MoveFavorite Down should succeed");
        assert_eq!(
            core.config.favorites[0], second,
            "second item should now be first after moving first item Down"
        );
        assert_eq!(
            core.config.favorites[1], first,
            "first item should now be second after moving it Down"
        );

        // Move it back Up.
        let r2 = core.dispatch(photon_protocol::Envelope::new(
            "mv2",
            photon_protocol::ClientKind::Cli,
            Command::MoveFavorite {
                path: first.clone(),
                direction: MoveDirection::Up,
            },
        ));
        assert!(r2.ok, "MoveFavorite Up should succeed");
        assert_eq!(
            core.config.favorites[0], first,
            "first item should be restored to position 0 after moving Up"
        );

        // Moving the first item Up should be a no-op (already at top).
        let r3 = core.dispatch(photon_protocol::Envelope::new(
            "mv3",
            photon_protocol::ClientKind::Cli,
            Command::MoveFavorite {
                path: first.clone(),
                direction: MoveDirection::Up,
            },
        ));
        assert!(r3.ok, "MoveFavorite Up at top should return ok (no-op)");
        assert_eq!(
            core.config.favorites[0], first,
            "first item must remain at position 0 when already at top"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn multi_tab_editor_opens_dedups_switches_reorders_and_closes() {
        let dir = std::env::temp_dir().join(format!("photon-multitab-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let p1 = dir.join("one.png");
        let p2 = dir.join("two.png");
        let p3 = dir.join("three.png");
        image::RgbaImage::from_pixel(10, 10, image::Rgba([255, 0, 0, 255]))
            .save(&p1)
            .unwrap();
        image::RgbaImage::from_pixel(20, 12, image::Rgba([0, 255, 0, 255]))
            .save(&p2)
            .unwrap();
        image::RgbaImage::from_pixel(30, 16, image::Rgba([0, 0, 255, 255]))
            .save(&p3)
            .unwrap();

        let mut core = Core::for_sandbox(&dir);
        for p in [&p1, &p2, &p3] {
            let r = core.dispatch(photon_protocol::Envelope::new(
                "open",
                photon_protocol::ClientKind::Cli,
                Command::EditorOpen {
                    path: p.to_string_lossy().into_owned(),
                },
            ));
            assert!(r.ok);
        }

        let meta = core.meta();
        assert_eq!(meta.editor_tabs.len(), 3);
        let id1 = meta.editor_tabs[0].id;
        let id2 = meta.editor_tabs[1].id;
        let id3 = meta.editor_tabs[2].id;
        assert_eq!(meta.active_editor_tab_id, Some(id3));
        assert_eq!(meta.editor.as_ref().unwrap().id, id3);

        // Mutate tab 3, then re-open p1 (should deduplicate & switch to tab 1).
        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "rot",
                photon_protocol::ClientKind::Cli,
                Command::EditorRotate {
                    degrees: 90,
                    target: ApplyTarget::WholeImage,
                },
            ))
            .ok
        );
        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "reopen1",
                photon_protocol::ClientKind::Cli,
                Command::EditorOpen {
                    path: p1.to_string_lossy().into_owned(),
                },
            ))
            .ok
        );
        let meta = core.meta();
        assert_eq!(meta.editor_tabs.len(), 3);
        assert_eq!(meta.active_editor_tab_id, Some(id1));
        assert!(!meta.editor_tabs[0].modified);
        assert!(meta.editor_tabs[2].modified);

        // Reorder tab 3 to index 0, then close inactive tab 2.
        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "reord",
                photon_protocol::ClientKind::Cli,
                Command::EditorReorderTab {
                    tab_id: id3,
                    new_index: 0,
                },
            ))
            .ok
        );
        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "close2",
                photon_protocol::ClientKind::Cli,
                Command::EditorCloseTab { tab_id: Some(id2) },
            ))
            .ok
        );
        let meta = core.meta();
        assert_eq!(
            meta.editor_tabs.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![id3, id1]
        );
        assert_eq!(meta.active_editor_tab_id, Some(id1));

        // Close active tab 1 -> switches to remaining tab 3 and stays in Editor.
        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "close1",
                photon_protocol::ClientKind::Cli,
                Command::EditorClose,
            ))
            .ok
        );
        assert_eq!(core.workspace(), Workspace::Editor);
        assert_eq!(core.meta().active_editor_tab_id, Some(id3));
        assert!(core.meta().editor.as_ref().unwrap().modified);

        // Close last tab -> leaves Editor workspace.
        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "close3",
                photon_protocol::ClientKind::Cli,
                Command::EditorCloseTab { tab_id: None },
            ))
            .ok
        );
        assert!(core.meta().editor_tabs.is_empty());
        assert!(core.meta().editor.is_none());
        assert_eq!(core.workspace(), Workspace::Browser);

        // FocusWindow increments focus_request_seq.
        let prev_seq = core.meta().focus_request_seq;
        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "focus",
                photon_protocol::ClientKind::Cli,
                Command::FocusWindow,
            ))
            .ok
        );
        assert_eq!(core.meta().focus_request_seq, prev_seq + 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ui_state_separation_defers_disk_write_and_respects_save_on_exit_and_force() {
        let dir =
            std::env::temp_dir().join(format!("photon-ui-save-policy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut core = Core::for_sandbox(&dir);
        let ui_path = core.paths.ui_state_file();

        // SetLayout marks ui_state_dirty without flushing ui_state.json immediately.
        let mut next_layout = core.config.layout.clone();
        next_layout.window_width = 1680;
        next_layout.viewer_viewport_background = Some([40, 42, 48]);
        next_layout.app_launch_window_mode = photon_protocol::AppLaunchWindowMode::Maximized;
        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "layout1",
                photon_protocol::ClientKind::Cli,
                Command::SetLayout {
                    layout: next_layout.clone(),
                },
            ))
            .ok
        );
        assert!(core.ui_state_dirty);

        // Disable save_ui_state_on_exit -> non-forced persist_ui_state(false) skips writing 1680.
        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "settings1",
                photon_protocol::ClientKind::Cli,
                Command::SetSettings {
                    theme: core.config.theme,
                    language: core.config.language,
                    remember_last_folder: core.config.remember_last_folder,
                    hardware_acceleration: core.config.hardware_acceleration,
                    save_ui_state_on_exit: false,
                },
            ))
            .ok
        );
        core.persist_ui_state(false);
        assert!(core.ui_state_dirty);
        let (on_disk_before, _) = UiState::load_or_migrate(&ui_path, &core.config);
        assert_ne!(on_disk_before.layout.window_width, 1680);

        // Explicit force save writes ui_state.json even when save_ui_state_on_exit is false.
        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "save-force",
                photon_protocol::ClientKind::Cli,
                Command::SaveUiState { force: true },
            ))
            .ok
        );
        assert!(!core.ui_state_dirty);
        let (on_disk_after, _) = UiState::load_or_migrate(&ui_path, &core.config);
        assert_eq!(on_disk_after.layout.window_width, 1680);
        assert_eq!(
            on_disk_after.layout.viewer_viewport_background,
            Some([40, 42, 48])
        );

        // ResetConfig (layout only) resets window_width & backdrop, but preserves Settings defaults.
        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "reset-layout",
                photon_protocol::ClientKind::Cli,
                Command::ResetConfig {
                    reset_layout: true,
                    reset_settings: false,
                },
            ))
            .ok
        );
        assert_eq!(core.config.layout.window_width, 1240);
        assert_eq!(core.config.layout.viewer_viewport_background, None);
        assert_eq!(
            core.config.layout.app_launch_window_mode,
            photon_protocol::AppLaunchWindowMode::Maximized
        );
        assert!(!core.config.save_ui_state_on_exit);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn editor_new_creates_blank_doc_and_clears_unsupported_state() {
        let mut core = Core::new(true, true);
        let dir = std::env::temp_dir().join(format!("photon-editor-new-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let bad_file = dir.join("broken.bin");
        fs::write(&bad_file, b"not an image").unwrap();

        let resp = core.dispatch(photon_protocol::Envelope::new(
            "u1",
            photon_protocol::ClientKind::Cli,
            Command::OpenInEditorOrUnsupported {
                path: bad_file.to_string_lossy().into_owned(),
            },
        ));
        assert!(resp.ok);
        assert_eq!(core.workspace(), Workspace::Editor);
        assert!(core.meta().editor.is_none());
        let unsup = core
            .meta()
            .editor_unsupported
            .expect("unsupported metadata");
        assert_eq!(unsup.extension.as_deref(), Some("bin"));

        let new_resp = core.dispatch(photon_protocol::Envelope::new(
            "n1",
            photon_protocol::ClientKind::Cli,
            Command::EditorNew {
                width: 32,
                height: 24,
                background: [255, 255, 255, 255],
            },
        ));
        assert!(new_resp.ok);
        let meta = core.meta();
        assert_eq!(meta.workspace, Workspace::Editor);
        assert!(meta.editor_unsupported.is_none());
        assert!(meta.editor_source_is_virtual);
        let ed = meta.editor.expect("new blank doc");
        assert_eq!((ed.width, ed.height), (32, 24));

        // Saving a virtual/new blank doc without Save As requires Save As.
        let save_resp = core.dispatch(photon_protocol::Envelope::new(
            "s1",
            photon_protocol::ClientKind::Cli,
            Command::EditorSave,
        ));
        assert!(!save_resp.ok);
        assert_eq!(save_resp.error.unwrap().code, "save_as_required");

        let save_as_path = dir.join("created.png");
        let save_as_resp = core.dispatch(photon_protocol::Envelope::new(
            "s2",
            photon_protocol::ClientKind::Cli,
            Command::EditorSaveAs {
                path: save_as_path.to_string_lossy().into_owned(),
            },
        ));
        assert!(save_as_resp.ok);
        assert!(!core.meta().editor_source_is_virtual);
        assert!(save_as_path.is_file());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn switching_viewer_to_editor_on_corrupt_file_sets_editor_unsupported() {
        let mut core = Core::new(true, true);
        let dir = std::env::temp_dir().join(format!("photon-editor-unsup-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let corrupt = dir.join("corrupt.png");
        fs::write(&corrupt, b"corrupted png bytes").unwrap();

        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "o1",
                photon_protocol::ClientKind::Cli,
                Command::OpenFiles {
                    paths: vec![corrupt.to_string_lossy().into_owned()],
                },
            ))
            .ok
        );
        assert_eq!(core.workspace(), Workspace::Viewer);

        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "e1",
                photon_protocol::ClientKind::Cli,
                Command::SetWorkspace {
                    workspace: Workspace::Editor,
                },
            ))
            .ok
        );
        let meta = core.meta();
        assert_eq!(meta.workspace, Workspace::Editor);
        assert!(meta.editor.is_none());
        assert!(meta.editor_unsupported.is_some());
        assert_eq!(meta.viewer_count, 1);

        // Back to Viewer clears editor_unsupported and preserves viewer_images.
        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "b1",
                photon_protocol::ClientKind::Cli,
                Command::WorkspaceBack,
            ))
            .ok
        );
        assert_eq!(core.workspace(), Workspace::Viewer);
        assert!(core.meta().editor_unsupported.is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_cbz_populates_viewer_archive_materializes_page_zero_and_cleans_up() {
        let dir = std::env::temp_dir().join(format!("photon-cbz-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let cbz_path = dir.join("comic.cbz");

        let doc = EditorDocument::new_blank(4, 4, [10, 20, 30, 255]).unwrap();
        let zip_bytes = photon_image::ora::encode_ora_bytes(doc.stack()).unwrap();
        fs::write(&cbz_path, &zip_bytes).unwrap();

        let mut core = Core::new(true, true);
        let resp = core.dispatch(photon_protocol::Envelope::new(
            "a1",
            photon_protocol::ClientKind::Cli,
            Command::OpenFiles {
                paths: vec![cbz_path.to_string_lossy().into_owned()],
            },
        ));
        assert!(resp.ok);
        let snap = core.snapshot();
        assert_eq!(snap.meta.workspace, Workspace::Viewer);
        let archive_meta = snap.meta.viewer_archive.expect("viewer_archive");
        assert_eq!(archive_meta.kind, "zip_cbz");
        assert!(archive_meta.page_count >= 2);
        assert_eq!(snap.viewer_images.len(), archive_meta.page_count);

        let page0 = PathBuf::from(&snap.viewer_images[0]);
        let page1 = PathBuf::from(&snap.viewer_images[1]);
        assert!(page0.is_file());
        assert!(!page1.exists());
        let temp_dir = page0.parent().unwrap().to_path_buf();

        // Switching to a regular folder cleans up the archive temp dir.
        assert!(
            core.dispatch(photon_protocol::Envelope::new(
                "f1",
                photon_protocol::ClientKind::Cli,
                Command::OpenFolder {
                    path: dir.to_string_lossy().into_owned(),
                },
            ))
            .ok
        );
        assert!(core.meta().viewer_archive.is_none());
        assert!(!temp_dir.exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
