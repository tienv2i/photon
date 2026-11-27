//! Lightweight `EditorCore` implementing `fw_runtime::AppCore` directly inside `photon-app-editor`,
//! with zero dependency on `photon-core` or `photon-browser`/`photon-viewer`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use fw_protocol::{CommandResponse, CommandStatus, Envelope, ProtocolError, UiText};
use fw_runtime::{AppCore, RuntimeContext};
use photon_image::EditorDocument;
use photon_protocol::{
    Command, EditorSelectionSummary, EditorState, Event, EventKind, Language, Query, Theme,
    UiStateMeta, UiStateSnapshot, UiTreeSnapshot, Workspace, protocol_schemas,
};
use photon_storage::{AppPaths, Config};
use serde_json::json;
use tokio::sync::oneshot;

#[derive(Debug)]
pub enum EditorCoreCustom {
    SetUiTree(UiTreeSnapshot),
    ReadPixels(oneshot::Sender<Option<(u64, u32, u32, Vec<u8>)>>),
}

pub struct EditorCore {
    pub config: Config,
    doc: Option<EditorDocument>,
    snapshot: UiStateSnapshot,
    ui_tree: Arc<Mutex<Option<UiTreeSnapshot>>>,
    next_seq: u64,
    pixel_revision: u64,
}

impl EditorCore {
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
            workspace: Workspace::Editor,
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
            doc: None,
            snapshot: UiStateSnapshot {
                meta,
                entries: Vec::new(),
                viewer_images: Vec::new(),
            },
            ui_tree: Arc::new(Mutex::new(None)),
            next_seq: 1,
            pixel_revision: 1,
        }
    }

    fn sync_editor_meta(&mut self) {
        self.pixel_revision += 1;
        let Some(doc) = &self.doc else {
            self.snapshot.meta.editor = None;
            return;
        };
        let (w, h) = doc.dimensions();
        self.snapshot.meta.editor = Some(EditorState {
            id: 1,
            path: doc.source_path.to_string_lossy().into_owned(),
            name: doc
                .source_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Untitled")
                .to_owned(),
            width: w,
            height: h,
            can_undo: doc.can_undo(),
            can_redo: doc.can_redo(),
            modified: doc.modified(),
            saved: !doc.modified(),
            save_path: doc.save_path.to_string_lossy().into_owned(),
            pixel_revision: self.pixel_revision,
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
                saved_selections: doc
                    .selection()
                    .saved_selections
                    .iter()
                    .map(|s| photon_protocol::SavedSelectionSummary {
                        id: s.id,
                        name: s.name.clone(),
                    })
                    .collect(),
            },
            active_vector_data: doc.stack().active_layer().vector_data.clone(),
            active_text_data: doc.stack().active_layer().text_data.clone(),
        });
    }
}

impl AppCore for EditorCore {
    type State = UiStateSnapshot;
    type Command = Command;
    type Query = Query;
    type Event = Event;
    type Custom = EditorCoreCustom;

    fn snapshot(&self) -> Self::State {
        self.snapshot.clone()
    }

    fn revision(&self) -> u64 {
        self.snapshot.meta.revision
    }

    fn dispatch(&mut self, envelope: Envelope<Self::Command>) -> CommandResponse {
        match envelope.command {
            Command::EditorOpen { path } => {
                if let Ok(doc) = EditorDocument::open(Path::new(&path).to_path_buf()) {
                    self.doc = Some(doc);
                    self.sync_editor_meta();
                }
            }
            Command::EditorUndo => {
                if let Some(doc) = &mut self.doc
                    && doc.undo().is_ok()
                {
                    self.sync_editor_meta();
                }
            }
            Command::EditorRedo => {
                if let Some(doc) = &mut self.doc
                    && doc.redo().is_ok()
                {
                    self.sync_editor_meta();
                }
            }
            Command::EditorSaveAs { path } => {
                if let Some(doc) = &mut self.doc
                    && doc.save_to(Some(Path::new(&path))).is_ok()
                {
                    self.sync_editor_meta();
                }
            }
            Command::EditorClose => {
                self.doc = None;
                self.sync_editor_meta();
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
                "app": "photon-editor",
                "revision": self.snapshot.meta.revision,
                "has_document": self.doc.is_some(),
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
                    workspace: Workspace::Editor,
                    listing_epoch: self.snapshot.meta.listing_epoch,
                },
            });
        }
    }

    fn handle_custom(&mut self, msg: Self::Custom, _ctx: &RuntimeContext<Self>) {
        match msg {
            EditorCoreCustom::SetUiTree(tree) => {
                if let Ok(mut g) = self.ui_tree.lock() {
                    *g = Some(tree);
                }
            }
            EditorCoreCustom::ReadPixels(reply) => {
                let res = self.doc.as_ref().map(|doc| {
                    let img = photon_image::composite_stack(doc.stack());
                    let (w, h) = img.dimensions();
                    (self.pixel_revision, w, h, img.as_raw().clone())
                });
                let _ = reply.send(res);
            }
        }
    }
}
