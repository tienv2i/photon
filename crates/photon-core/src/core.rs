//! Synchronous Core state owner and command execution handlers.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, UNIX_EPOCH};

use photon_image::{EditOp, EditorDocument, ImageError, LayerOperation};
use photon_protocol::{
    ApplyTarget, BatchSource, BatchState, ColorAdjustments, ColorBalanceAdjustment, Command,
    CommandResponse, CommandStatus, Crumb, CurvesAdjustment, EditorSelectionSummary, EditorState,
    Entry, HsvAdjustment, IMAGE_EXTENSIONS, LayoutState, ProtocolError, Query, RuntimeStatus,
    SelectionInfo, TaskRunStatus, TaskSnapshot, UiStateMeta, UiStateSnapshot, UiText,
    UiTreeSnapshot, Workspace, protocol_schemas,
};
use photon_storage::journal::now_ms;
use photon_storage::logging::current_boot_id;
use photon_storage::{AppIdentity, AppPaths, Config, Journal};

use crate::batch::BatchPlan;
use crate::file_ops;

pub struct Core {
    pub identity: AppIdentity,
    pub paths: AppPaths,
    pub config: Config,
    pub journal: Journal,
    pub headless: bool,
    pub safe_mode: bool,
    started_at: Instant,
    revision: u64,
    listing_epoch: u64,
    tree_revision: u64,
    workspace: Workspace,
    current_folder: Option<PathBuf>,
    root_folder: Option<PathBuf>,
    entries: Vec<Entry>,
    selection: Vec<String>,
    selection_info: Option<SelectionInfo>,
    viewer_images: Vec<String>,
    viewer_index: Option<usize>,
    editor: Option<EditorDocument>,
    next_original_revision: u64,
    batch: Option<BatchState>,
    batch_cancel: Option<Arc<AtomicBool>>,
    pending_batch: Option<(BatchPlan, Arc<AtomicBool>)>,
    next_job_seq: u64,
    status: UiText,
    tasks: Vec<TaskSnapshot>,
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
        let _ = paths.ensure_runtime_dirs();
        let config = if safe_mode {
            let mut cfg = Config::factory_default();
            cfg.hardware_acceleration = false;
            cfg.remember_last_folder = false;
            cfg
        } else {
            let (cfg, warn) = Config::load_or_create(&paths.config_file());
            if let Some(warning) = warn {
                tracing::warn!(target: "photon::config", %warning, "config warning");
            }
            cfg
        };
        let journal = Journal::new(&paths.journal_dir, current_boot_id());
        let status = if safe_mode {
            UiText::new("safe_mode_active")
        } else {
            UiText::new("ready")
        };

        let mut core = Self {
            identity,
            paths,
            config,
            journal,
            headless,
            safe_mode,
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
            editor: None,
            next_original_revision: 1,
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
            if !safe_mode {
                core.persist_config();
            }
        }

        if !safe_mode
            && core.config.remember_last_folder
            && let Some(folder) = core.config.last_folder.clone()
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
            hardware_acceleration: self.config.hardware_acceleration,
            viewer_index: self.viewer_index,
            viewer_path,
            viewer_count: self.viewer_images.len(),
            editor: self.editor.as_ref().map(|doc| {
                let (w, h) = doc.dimensions();
                EditorState {
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
            translation: None,
            translate_settings: self.config.translate.clone(),
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
                let text = self.open_folder(&p)?;
                if self.root_folder.as_ref() != Some(&p) {
                    self.root_folder = Some(p.clone());
                    self.tree_revision += 1;
                }
                self.workspace = Workspace::Browser;
                self.sync_session_prefs();
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
                    self.sync_session_prefs();
                    Ok((None, None))
                }
                Workspace::Editor => {
                    if self.editor.is_none() {
                        let path = self
                            .viewer_index
                            .and_then(|i| self.viewer_images.get(i))
                            .cloned()
                            .ok_or_else(no_doc_err)?;
                        self.open_editor_path(Path::new(&path))?;
                    }
                    self.workspace = Workspace::Editor;
                    self.sync_session_prefs();
                    Ok((None, None))
                }
            },
            Command::WorkspaceBack => {
                self.workspace = match self.workspace {
                    Workspace::Editor => {
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
                self.config.layout = layout.clone().sanitized();
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
                self.persist_config();
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
                self.persist_config();
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
                self.persist_config();
                Ok((None, None))
            }
            Command::SetSettings {
                theme,
                language,
                remember_last_folder,
                hardware_acceleration,
            } => {
                self.config.theme = *theme;
                self.config.language = *language;
                self.config.remember_last_folder = *remember_last_folder;
                self.config.hardware_acceleration = *hardware_acceleration;
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
                if !p.is_file() {
                    return Err(ProtocolError::new(
                        "not_found",
                        format!("file not found: {path}"),
                    ));
                }
                if let Some(idx) = self.viewer_images.iter().position(|x| x == path) {
                    self.viewer_index = Some(idx);
                } else {
                    self.viewer_images = vec![path.clone()];
                    self.viewer_index = Some(0);
                    self.listing_epoch += 1;
                }
                self.workspace = Workspace::Viewer;
                self.sync_session_prefs();
                Ok((Some(self.current_viewer_status()), None))
            }
            Command::EditorOpen { path } => {
                let text = self.open_editor_path(Path::new(path))?;
                self.workspace = Workspace::Editor;
                Ok((Some(text), None))
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
            } => self.apply_editor_op(
                EditOp::DrawShape {
                    kind: *kind,
                    points: points.clone(),
                    nodes: nodes.clone(),
                    fill_mode: *fill_mode,
                    closed: *closed,
                    color: [0, 0, 0],
                    radius: 3.0,
                    opacity: 1.0,
                    brush: photon_protocol::BrushSettings::default(),
                },
                ApplyTarget::ActiveLayer,
            ),
            Command::EditorFloodFill {
                x,
                y,
                tolerance,
                contiguous,
            } => self.apply_editor_op(
                EditOp::FloodFill {
                    x: *x,
                    y: *y,
                    tolerance: *tolerance,
                    contiguous: *contiguous,
                    color: [0, 0, 0],
                    opacity: 1.0,
                },
                ApplyTarget::ActiveLayer,
            ),
            Command::EditorDrawGradient {
                start,
                end,
                kind,
                color_mode,
            } => self.apply_editor_op(
                EditOp::DrawGradient {
                    start: *start,
                    end: *end,
                    kind: *kind,
                    color_mode: *color_mode,
                    fg_color: [0, 0, 0],
                    bg_color: [255, 255, 255],
                    opacity: 1.0,
                },
                ApplyTarget::ActiveLayer,
            ),
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
            Command::ViewerTranslate { .. }
            | Command::ViewerSaveTranslation { .. }
            | Command::ViewerDeleteTranslation { .. }
            | Command::SetTranslateSettings { .. } => Err(ProtocolError::line(
                "feature_disabled",
                UiText::new("ai_feature_disabled"),
            )),
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
                Ok((
                    Some(UiText::with(
                        "editor_saved",
                        [("path", saved.to_string_lossy().into_owned())],
                    )),
                    Some(serde_json::json!({ "path": saved })),
                ))
            }
            Command::EditorClose => {
                self.editor = None;
                if self.workspace == Workspace::Editor {
                    self.workspace = if self.viewer_images.is_empty() {
                        Workspace::Browser
                    } else {
                        Workspace::Viewer
                    };
                    self.sync_session_prefs();
                }
                Ok((Some(UiText::new("editor_closed")), None))
            }
            Command::CancelTask { task_id } => {
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
                    self.config.layout = LayoutState::default();
                }
                if *reset_settings {
                    let layout = self.config.layout.clone();
                    self.config = Config::factory_default();
                    if !*reset_layout {
                        self.config.layout = layout;
                    }
                }
                self.persist_config();
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
        if self.safe_mode || !self.config.remember_last_folder {
            return;
        }
        self.config.last_folder = self
            .current_folder
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        self.persist_config();
    }

    fn persist_config(&self) {
        if !self.safe_mode {
            let _ = self.config.save(&self.paths.config_file());
        }
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

        let total = entries.len();
        let img_count = viewer_images.len();
        self.current_folder = Some(dir.to_path_buf());
        self.entries = entries;
        self.viewer_images = viewer_images;
        self.viewer_index = (!self.viewer_images.is_empty()).then_some(0);
        self.selection.clear();
        self.selection_info = None;
        self.listing_epoch += 1;

        Ok(UiText::with(
            "folder_contents",
            [
                ("items", total.to_string()),
                ("images", img_count.to_string()),
            ],
        ))
    }

    fn open_editor_path(&mut self, path: &Path) -> Result<UiText, ProtocolError> {
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
        self.editor = Some(doc);
        Ok(UiText::with(
            "editor_opened",
            [
                ("name", name),
                ("width", w.to_string()),
                ("height", h.to_string()),
            ],
        ))
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
        self.viewer_index = Some(next);
        self.workspace = Workspace::Viewer;
        self.sync_session_prefs();
        Ok((Some(self.current_viewer_status()), None))
    }

    fn refresh_after_file_op(&mut self) {
        self.tree_revision += 1;
        if let Some(dir) = self.current_folder.clone() {
            let _ = self.open_folder(&dir);
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
}

fn no_doc_err() -> ProtocolError {
    ProtocolError::line("no_document", UiText::new("needs_a_document"))
}

fn map_image_err(e: ImageError) -> ProtocolError {
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
    fn translate_commands_are_cleanly_disabled() {
        let dir = std::env::temp_dir().join(format!("photon-no-ai-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let img = dir.join("pic.png");
        RgbaImage::from_pixel(8, 8, Rgba([10, 20, 30, 255]))
            .save(&img)
            .unwrap();
        let img_str = img.to_string_lossy().into_owned();

        let mut core = Core::for_sandbox(&dir.join("sandbox"));
        let cmds = [
            Command::ViewerTranslate {
                path: img_str.clone(),
                source_lang: "eng".into(),
                target_lang: "vi".into(),
                region: None,
            },
            Command::ViewerSaveTranslation {
                path: img_str.clone(),
                blocks: vec![],
                source_lang: "eng".into(),
                target_lang: "vi".into(),
            },
            Command::ViewerDeleteTranslation { path: img_str },
            Command::SetTranslateSettings {
                settings: photon_protocol::TranslateSettings::default(),
            },
        ];

        for (idx, cmd) in cmds.into_iter().enumerate() {
            let resp = core.dispatch(Envelope::new(format!("tr-{idx}"), ClientKind::Cli, cmd));
            assert!(!resp.ok);
            let err = resp.error.expect("expected ProtocolError");
            assert_eq!(err.code, "feature_disabled");
            assert_eq!(err.ui, Some(UiText::new("ai_feature_disabled")));
        }
        assert!(core.meta().active_tasks.is_empty());
        assert!(core.meta().translation.is_none());

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
}
