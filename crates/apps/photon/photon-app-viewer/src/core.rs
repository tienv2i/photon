//! Lightweight `ViewerCore` implementing `fw_runtime::AppCore` directly inside `photon-app-viewer`,
//! with zero dependency on `photon-core`, `photon-plugins`, or Editor state.

use std::path::Path;
use std::sync::{Arc, Mutex};

use comp_file_browser::scan_dir_fast;
use fw_protocol::{CommandResponse, CommandStatus, Envelope, ProtocolError, UiText};
use fw_runtime::{AppCore, RuntimeContext};
use photon_protocol::{
    ArchiveViewerSessionMeta, Command, Event, EventKind, IMAGE_EXTENSIONS, Language, Query, Theme,
    UiStateMeta, UiStateSnapshot, UiTreeSnapshot, Workspace, protocol_schemas,
};
use photon_storage::{AppPaths, Config};
use serde_json::json;

#[derive(Debug, Clone)]
pub enum ViewerCoreCustom {
    SetUiTree(UiTreeSnapshot),
}

pub struct ViewerCore {
    pub config: Config,
    snapshot: UiStateSnapshot,
    ui_tree: Arc<Mutex<Option<UiTreeSnapshot>>>,
    next_seq: u64,
}

impl ViewerCore {
    pub fn new(safe_mode: bool) -> Self {
        let paths = AppPaths::current();
        let config = if safe_mode {
            Config::default()
        } else {
            Config::load_or_create(&paths.config_file()).0
        };
        let theme = match config.theme {
            Theme::Dark => Theme::Dark,
            Theme::Light => Theme::Light,
        };
        let mut meta = UiStateMeta {
            revision: 0,
            listing_epoch: 0,
            tree_revision: 0,
            workspace: Workspace::Viewer,
            theme,
            language: config.language,
            current_folder: None,
            root_folder: None,
            breadcrumbs: Vec::new(),
            selection: Vec::new(),
            selection_info: None,
            layout: config.layout.clone(),
            remember_last_folder: false,
            save_ui_state_on_exit: false,
            hardware_acceleration: config.hardware_acceleration,
            backup_method: config.backup_method,
            viewer_index: None,
            viewer_path: None,
            viewer_count: 0,
            editor: None,
            editor_tabs: Vec::new(),
            active_editor_tab_id: None,
            focus_request_seq: 0,
            pending_recovery_count: 0,
            status: UiText::empty(),
            safe_mode,
            active_tasks: Vec::new(),
            favorites: Vec::new(),
            batch: None,
            translation: None,
            viewer_saved_translation: None,
            translate_settings: Default::default(),
            plugins: Vec::new(),
            ai_settings: Default::default(),
            ai_models: Vec::new(),
            quick_actions: Vec::new(),
            disabled_quick_action_plugins: Vec::new(),
            viewer_single_instance: config.viewer_single_instance,
            ai_task: None,
            editor_caption: None,
            has_pixel_clipboard: false,
            viewer_archive: None,
            editor_unsupported: None,
            editor_source_is_virtual: false,
        };
        if let Language::System = meta.language {
            meta.language = Language::English;
        }
        Self {
            config,
            snapshot: UiStateSnapshot {
                meta,
                entries: Vec::new(),
                viewer_images: Vec::new(),
            },
            ui_tree: Arc::new(Mutex::new(None)),
            next_seq: 1,
        }
    }

    pub fn snapshot(&self) -> UiStateSnapshot {
        self.snapshot.clone()
    }

    fn scan_folder_for_viewer(&mut self, folder: &Path, target_file: Option<&str>) {
        let mut viewer_images: Vec<String> = scan_dir_fast(
            folder,
            self.config.layout.browser_show_hidden,
            false,
            is_supported_image,
        )
        .map(|scanned| {
            scanned
                .into_iter()
                .map(|e| e.path.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
        viewer_images.sort_by_cached_key(|a| a.to_ascii_lowercase());
        let images = viewer_images;

        let idx = target_file
            .and_then(|t| images.iter().position(|p| p == t))
            .or_else(|| if images.is_empty() { None } else { Some(0) });
        self.snapshot.meta.current_folder = Some(folder.to_string_lossy().into_owned());
        self.snapshot.meta.viewer_archive = None;
        self.snapshot.viewer_images = images;
        self.snapshot.meta.viewer_count = self.snapshot.viewer_images.len();
        self.snapshot.meta.viewer_index = idx;
        self.snapshot.meta.viewer_path =
            idx.and_then(|i| self.snapshot.viewer_images.get(i).cloned());
        if let Some(t) = target_file
            && self.snapshot.meta.viewer_path.is_none()
            && Path::new(t).is_file()
        {
            self.snapshot.viewer_images = vec![t.to_owned()];
            self.snapshot.meta.viewer_count = 1;
            self.snapshot.meta.viewer_index = Some(0);
            self.snapshot.meta.viewer_path = Some(t.to_owned());
        }
        self.snapshot.meta.listing_epoch += 1;
    }
}

fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| IMAGE_EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(ext)))
}

impl AppCore for ViewerCore {
    type State = UiStateSnapshot;
    type Command = Command;
    type Query = Query;
    type Event = Event;
    type Custom = ViewerCoreCustom;

    fn snapshot(&self) -> Self::State {
        self.snapshot.clone()
    }

    fn revision(&self) -> u64 {
        self.snapshot.meta.revision
    }

    fn dispatch(&mut self, envelope: Envelope<Self::Command>) -> CommandResponse {
        if let Some(expected) = envelope.expected_revision
            && expected != self.snapshot.meta.revision
        {
            return CommandResponse {
                id: envelope.id,
                ok: false,
                status: CommandStatus::Failed,
                revision: self.snapshot.meta.revision,
                task_id: None,
                message: Some("revision conflict".to_owned()),
                ui: None,
                data: None,
                error: Some(ProtocolError {
                    code: "REVISION_CONFLICT".to_owned(),
                    message: format!("expected {expected}, got {}", self.snapshot.meta.revision),
                    ui: None,
                }),
            };
        }

        match envelope.command {
            Command::OpenFiles { paths } => {
                if paths.len() == 1 {
                    let p = Path::new(&paths[0]);
                    if p.is_dir() {
                        self.scan_folder_for_viewer(p, None);
                    } else if let Some(parent) = p.parent() {
                        self.scan_folder_for_viewer(parent, Some(&paths[0]));
                    }
                } else if !paths.is_empty() {
                    self.snapshot.meta.viewer_archive = None;
                    self.snapshot.viewer_images = paths;
                    self.snapshot.meta.viewer_count = self.snapshot.viewer_images.len();
                    self.snapshot.meta.viewer_index = Some(0);
                    self.snapshot.meta.viewer_path = self.snapshot.viewer_images.first().cloned();
                    self.snapshot.meta.listing_epoch += 1;
                }
            }
            Command::OpenFolder { path } => {
                self.scan_folder_for_viewer(Path::new(&path), None);
            }
            Command::OpenArchive { path } => {
                if let Ok(mut sess) = photon_image::archive::ArchiveSession::open(Path::new(&path))
                {
                    let kind = format!("{:?}", sess.kind()).to_ascii_lowercase();
                    let page_count = sess.page_count();
                    let page_labels: Vec<String> =
                        sess.pages().iter().map(|p| p.label.clone()).collect();
                    let mut pages = Vec::with_capacity(page_count);
                    for i in 0..page_count {
                        if let Ok(p) = sess.materialize_page(i) {
                            pages.push(p.to_string_lossy().into_owned());
                        }
                    }
                    self.snapshot.meta.viewer_archive = Some(ArchiveViewerSessionMeta {
                        container_path: path,
                        kind,
                        page_count,
                        page_labels,
                    });
                    self.snapshot.viewer_images = pages;
                    self.snapshot.meta.viewer_count = self.snapshot.viewer_images.len();
                    self.snapshot.meta.viewer_index =
                        (!self.snapshot.viewer_images.is_empty()).then_some(0);
                    self.snapshot.meta.viewer_path = self.snapshot.viewer_images.first().cloned();
                    self.snapshot.meta.listing_epoch += 1;
                }
            }
            Command::ViewerOpen { path } => {
                if let Some(idx) = self.snapshot.viewer_images.iter().position(|p| p == &path) {
                    self.snapshot.meta.viewer_index = Some(idx);
                    self.snapshot.meta.viewer_path = Some(path);
                } else {
                    let p = Path::new(&path);
                    if let Some(parent) = p.parent() {
                        self.scan_folder_for_viewer(parent, Some(&path));
                    }
                }
            }
            Command::ViewerNext => {
                let len = self.snapshot.viewer_images.len();
                if len > 0 {
                    let next = self
                        .snapshot
                        .meta
                        .viewer_index
                        .map(|i| (i + 1) % len)
                        .unwrap_or(0);
                    self.snapshot.meta.viewer_index = Some(next);
                    self.snapshot.meta.viewer_path = self.snapshot.viewer_images.get(next).cloned();
                }
            }
            Command::ViewerPrevious => {
                let len = self.snapshot.viewer_images.len();
                if len > 0 {
                    let prev = self
                        .snapshot
                        .meta
                        .viewer_index
                        .map(|i| if i == 0 { len - 1 } else { i - 1 })
                        .unwrap_or(0);
                    self.snapshot.meta.viewer_index = Some(prev);
                    self.snapshot.meta.viewer_path = self.snapshot.viewer_images.get(prev).cloned();
                }
            }
            Command::DeletePermanently { paths } => {
                for p in &paths {
                    let _ = std::fs::remove_file(p);
                }
                self.snapshot.viewer_images.retain(|x| !paths.contains(x));
                self.snapshot.meta.viewer_count = self.snapshot.viewer_images.len();
                if self.snapshot.viewer_images.is_empty() {
                    self.snapshot.meta.viewer_index = None;
                    self.snapshot.meta.viewer_path = None;
                } else {
                    let i = self
                        .snapshot
                        .meta
                        .viewer_index
                        .unwrap_or(0)
                        .min(self.snapshot.viewer_images.len() - 1);
                    self.snapshot.meta.viewer_index = Some(i);
                    self.snapshot.meta.viewer_path = self.snapshot.viewer_images.get(i).cloned();
                }
                self.snapshot.meta.listing_epoch += 1;
            }
            Command::SetLanguage { language } => {
                self.snapshot.meta.language = match language {
                    Language::System => Language::English,
                    other => other,
                };
            }
            Command::FocusWindow => {
                self.snapshot.meta.focus_request_seq += 1;
            }
            _ => {}
        }

        self.snapshot.meta.revision += 1;
        CommandResponse {
            id: envelope.id,
            ok: true,
            status: CommandStatus::Completed,
            revision: self.snapshot.meta.revision,
            task_id: None,
            message: None,
            ui: None,
            data: None,
            error: None,
        }
    }

    fn query(&self, query: Self::Query) -> Result<serde_json::Value, ProtocolError> {
        match query {
            Query::State => Ok(serde_json::to_value(&self.snapshot).unwrap_or_default()),
            Query::Meta => Ok(serde_json::to_value(&self.snapshot.meta).unwrap_or_default()),
            Query::Status => Ok(json!({
                "app": "photon-viewer",
                "revision": self.snapshot.meta.revision,
                "viewer_path": self.snapshot.meta.viewer_path,
                "viewer_count": self.snapshot.meta.viewer_count,
            })),
            Query::UiTree => Ok(self
                .ui_tree
                .lock()
                .ok()
                .and_then(|g| g.clone())
                .map(|t| serde_json::to_value(t).unwrap_or_default())
                .unwrap_or(serde_json::Value::Null)),
            Query::Schema => Ok(protocol_schemas()),
            _ => Ok(serde_json::Value::Null),
        }
    }

    fn on_dispatch_done(
        &mut self,
        envelope: &Envelope<Self::Command>,
        response: &CommandResponse,
        ctx: &RuntimeContext<Self>,
    ) {
        if let Command::DebugUiAction { target_id, action } = &envelope.command {
            let seq = self.next_seq;
            self.next_seq += 1;
            ctx.emit_event(Event {
                seq_id: seq,
                timestamp_ms: 0,
                revision: self.snapshot.meta.revision,
                kind: EventKind::UiActionRequested {
                    request_id: envelope.id.clone(),
                    target_id: target_id.clone(),
                    action: action.clone(),
                },
            });
        }
        if response.ok {
            let seq = self.next_seq;
            self.next_seq += 1;
            ctx.emit_event(Event {
                seq_id: seq,
                timestamp_ms: 0,
                revision: self.snapshot.meta.revision,
                kind: EventKind::StateChanged {
                    workspace: Workspace::Viewer,
                    listing_epoch: self.snapshot.meta.listing_epoch,
                },
            });
        }
    }

    fn handle_custom(&mut self, msg: Self::Custom, _ctx: &RuntimeContext<Self>) {
        match msg {
            ViewerCoreCustom::SetUiTree(tree) => {
                if let Ok(mut g) = self.ui_tree.lock() {
                    *g = Some(tree);
                }
            }
        }
    }
}
