//! Lightweight `ManagerCore` implementing `fw_runtime::AppCore` directly inside `photon-app-manager`,
//! with zero dependency on `photon-core`, `photon-plugins`, or Editor state.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use comp_file_browser::scan_dir_fast;
use fw_protocol::{CommandResponse, CommandStatus, Envelope, ProtocolError, UiText};
use fw_runtime::{AppCore, RuntimeContext};
use photon_protocol::{
    BrowserSortField, Command, Crumb, Entry, Event, EventKind, IMAGE_EXTENSIONS, Language, Query,
    SortOrder, Theme, UiStateMeta, UiStateSnapshot, UiTreeSnapshot, Workspace, protocol_schemas,
};
use photon_storage::{AppPaths, Config};
use serde_json::json;

#[derive(Debug, Clone)]
pub enum ManagerCoreCustom {
    SetUiTree(UiTreeSnapshot),
}

pub struct ManagerCore {
    pub config: Config,
    snapshot: UiStateSnapshot,
    ui_tree: Arc<Mutex<Option<UiTreeSnapshot>>>,
    next_seq: u64,
}

impl ManagerCore {
    pub fn new(safe_mode: bool, load_last_folder: bool) -> Self {
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
            workspace: Workspace::Browser,
            theme,
            language: config.language,
            current_folder: None,
            root_folder: None,
            breadcrumbs: Vec::new(),
            selection: Vec::new(),
            selection_info: None,
            layout: config.layout.clone(),
            remember_last_folder: config.remember_last_folder,
            save_ui_state_on_exit: config.save_ui_state_on_exit,
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
            favorites: config.favorites.clone(),
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
        let mut core = Self {
            config,
            snapshot: UiStateSnapshot {
                meta,
                entries: Vec::new(),
                viewer_images: Vec::new(),
            },
            ui_tree: Arc::new(Mutex::new(None)),
            next_seq: 1,
        };
        if load_last_folder
            && let Some(dir) = core.config.last_folder.clone()
            && Path::new(&dir).is_dir()
        {
            core.open_folder(Path::new(&dir));
        }
        core
    }

    pub fn open_folder(&mut self, dir: &Path) {
        if !dir.is_dir() {
            return;
        }
        let show_hidden = self.snapshot.meta.layout.browser_show_hidden;
        let scanned = scan_dir_fast(dir, show_hidden, true, |_| true).unwrap_or_default();
        let mut entries: Vec<Entry> = scanned
            .into_iter()
            .map(|e| {
                let is_image = !e.is_dir
                    && e.path
                        .extension()
                        .and_then(|x| x.to_str())
                        .is_some_and(|ext| {
                            IMAGE_EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(ext))
                        });
                let (size, modified_ms) = e
                    .path
                    .metadata()
                    .map(|m| {
                        let mtime = m
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        (m.len(), mtime)
                    })
                    .unwrap_or((0, 0));
                let kind = if e.is_dir {
                    "folder".to_owned()
                } else {
                    e.path
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|s| s.to_ascii_uppercase())
                        .unwrap_or_else(|| "FILE".to_owned())
                };
                Entry {
                    name: e.name,
                    path: e.path.to_string_lossy().into_owned(),
                    is_dir: e.is_dir,
                    is_image,
                    size,
                    modified_ms,
                    kind,
                }
            })
            .collect();

        let sort_field = self.snapshot.meta.layout.browser_sort_field;
        let sort_order = self.snapshot.meta.layout.browser_sort_order;
        entries.sort_by(|a, b| {
            let ord = b.is_dir.cmp(&a.is_dir).then_with(|| match sort_field {
                BrowserSortField::Name => a
                    .name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase()),
                BrowserSortField::Size => a.size.cmp(&b.size),
                BrowserSortField::Modified => a.modified_ms.cmp(&b.modified_ms),
                BrowserSortField::Kind => {
                    let ea = Path::new(&a.name)
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    let eb = Path::new(&b.name)
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    ea.cmp(eb).then_with(|| a.name.cmp(&b.name))
                }
            });
            if sort_order == SortOrder::Desc && a.is_dir == b.is_dir {
                ord.reverse()
            } else {
                ord
            }
        });

        let viewer_images: Vec<String> = entries
            .iter()
            .filter(|e| e.is_image)
            .map(|e| e.path.clone())
            .collect();

        let mut breadcrumbs = Vec::new();
        let mut acc = PathBuf::new();
        for comp in dir.components() {
            acc.push(comp);
            let label = comp.as_os_str().to_string_lossy().into_owned();
            breadcrumbs.push(Crumb {
                name: if label.is_empty() {
                    "/".to_owned()
                } else {
                    label
                },
                path: acc.to_string_lossy().into_owned(),
            });
        }

        self.snapshot.meta.current_folder = Some(dir.to_string_lossy().into_owned());
        self.snapshot.meta.breadcrumbs = breadcrumbs;
        self.snapshot.meta.selection.clear();
        self.snapshot.entries = entries;
        self.snapshot.viewer_images = viewer_images;
        self.snapshot.meta.listing_epoch += 1;
    }
}

impl AppCore for ManagerCore {
    type State = UiStateSnapshot;
    type Command = Command;
    type Query = Query;
    type Event = Event;
    type Custom = ManagerCoreCustom;

    fn snapshot(&self) -> Self::State {
        self.snapshot.clone()
    }

    fn revision(&self) -> u64 {
        self.snapshot.meta.revision
    }

    fn dispatch(&mut self, envelope: Envelope<Self::Command>) -> CommandResponse {
        match envelope.command {
            Command::OpenFolder { path } => self.open_folder(Path::new(&path)),
            Command::Refresh => {
                if let Some(cur) = self.snapshot.meta.current_folder.clone() {
                    self.open_folder(Path::new(&cur));
                }
            }
            Command::Select { paths } => {
                self.snapshot.meta.selection = paths;
            }
            Command::DeletePermanently { paths } => {
                for p in &paths {
                    let path = Path::new(p);
                    if path.is_dir() {
                        let _ = std::fs::remove_dir_all(path);
                    } else {
                        let _ = std::fs::remove_file(path);
                    }
                }
                if let Some(cur) = self.snapshot.meta.current_folder.clone() {
                    self.open_folder(Path::new(&cur));
                }
            }
            Command::RenamePath { path, new_name } => {
                let src = Path::new(&path);
                if let Some(parent) = src.parent() {
                    let _ = std::fs::rename(src, parent.join(new_name));
                }
                if let Some(cur) = self.snapshot.meta.current_folder.clone() {
                    self.open_folder(Path::new(&cur));
                }
            }
            Command::AddFavorite { path } => {
                if !self.snapshot.meta.favorites.contains(&path) {
                    self.snapshot.meta.favorites.push(path);
                }
            }
            Command::RemoveFavorite { path } => {
                self.snapshot.meta.favorites.retain(|p| p != &path);
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
                "app": "photon-manager",
                "revision": self.snapshot.meta.revision,
                "current_folder": self.snapshot.meta.current_folder,
                "entry_count": self.snapshot.entries.len(),
            })),
            Query::ListFolders { path } => {
                let dir = Path::new(&path);
                let mut folders = Vec::new();
                if let Ok(rd) = std::fs::read_dir(dir) {
                    for entry in rd.flatten() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if !self.snapshot.meta.layout.browser_show_hidden && name.starts_with('.') {
                            continue;
                        }
                        if entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                            folders.push(json!({
                                "name": name,
                                "path": entry.path().to_string_lossy(),
                            }));
                        }
                    }
                }
                Ok(json!({ "folders": folders }))
            }
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
                    workspace: Workspace::Browser,
                    listing_epoch: self.snapshot.meta.listing_epoch,
                },
            });
        }
    }

    fn handle_custom(&mut self, msg: Self::Custom, _ctx: &RuntimeContext<Self>) {
        match msg {
            ManagerCoreCustom::SetUiTree(tree) => {
                if let Ok(mut g) = self.ui_tree.lock() {
                    *g = Some(tree);
                }
            }
        }
    }
}
