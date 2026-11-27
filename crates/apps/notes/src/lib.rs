//! Minimal Notes app built purely on `fw-*` crates (`fw-protocol`, `fw-storage`,
//! `fw-runtime`, `fw-ipc`, `fw-cli`, `fw-ui`) with zero `photon-*` dependencies.

use std::path::PathBuf;

use fw_protocol::{CommandResponse, CommandStatus, Envelope, ProtocolCommand, ProtocolError};
use fw_runtime::{AppCore, RuntimeContext};
use fw_storage::{AppBranding, AppIdentity, AppPaths};
use serde::{Deserialize, Serialize};

pub const NOTES_BRANDING: AppBranding = AppBranding {
    prod_app_id: "io.github.tienv2i.Notes",
    dev_app_id: "io.github.tienv2i.Notes.Dev",
    sandbox_app_id: "io.github.tienv2i.Notes.Sandbox",
    prod_dir_name: "notes",
    dev_dir_name: "notes-dev",
    sandbox_dir_name: "notes-sandbox",
    prod_display_name: "Notes",
    dev_display_name: "Notes (Dev)",
    sandbox_display_name: "Notes (Sandbox)",
    env_prefix: "NOTES",
};

pub fn notes_identity() -> AppIdentity {
    AppIdentity::current_for(NOTES_BRANDING, 1)
}

pub fn notes_paths() -> AppPaths {
    AppPaths::for_identity(&notes_identity())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteItem {
    pub id: u64,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotesState {
    pub revision: u64,
    pub notes: Vec<NoteItem>,
    pub selected_id: Option<u64>,
    pub search: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NoteCommand {
    CreateNote {
        title: String,
        body: String,
    },
    SelectNote {
        id: u64,
    },
    UpdateNote {
        id: u64,
        title: String,
        body: String,
    },
    DeleteNote {
        id: u64,
    },
    SetSearch {
        query: String,
    },
}

impl ProtocolCommand for NoteCommand {
    fn command_name(&self) -> &str {
        match self {
            Self::CreateNote { .. } => "create_note",
            Self::SelectNote { .. } => "select_note",
            Self::UpdateNote { .. } => "update_note",
            Self::DeleteNote { .. } => "delete_note",
            Self::SetSearch { .. } => "set_search",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NoteQuery {
    State,
    Status,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum NoteEvent {
    StateChanged { revision: u64 },
}

pub struct NotesCore {
    state: NotesState,
    next_id: u64,
    store_file: Option<PathBuf>,
}

impl NotesCore {
    pub fn new(store_file: Option<PathBuf>) -> Self {
        let mut state = store_file
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<NotesState>(&s).ok())
            .unwrap_or_default();
        if state.notes.is_empty() {
            state.notes.push(NoteItem {
                id: 1,
                title: "Welcome to Notes".to_owned(),
                body: "Built on the fw-* framework (fw-protocol, fw-storage, fw-runtime, fw-ipc, fw-cli, fw-ui).".to_owned(),
            });
            state.selected_id = Some(1);
        }
        let next_id = state.notes.iter().map(|n| n.id).max().unwrap_or(0) + 1;
        Self {
            state,
            next_id,
            store_file,
        }
    }

    fn persist(&self) {
        if let Some(path) = &self.store_file {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(&self.state) {
                let _ = std::fs::write(path, json);
            }
        }
    }
}

impl AppCore for NotesCore {
    type State = NotesState;
    type Command = NoteCommand;
    type Query = NoteQuery;
    type Event = NoteEvent;
    type Custom = ();

    fn snapshot(&self) -> Self::State {
        self.state.clone()
    }

    fn revision(&self) -> u64 {
        self.state.revision
    }

    fn dispatch(&mut self, envelope: Envelope<Self::Command>) -> CommandResponse {
        if let Some(expected) = envelope.expected_revision
            && expected != self.state.revision
        {
            return err_resp(
                envelope.id,
                self.state.revision,
                "REVISION_CONFLICT",
                format!(
                    "expected revision {expected}, current is {}",
                    self.state.revision
                ),
            );
        }

        match envelope.command {
            NoteCommand::CreateNote { title, body } => {
                let id = self.next_id;
                self.next_id += 1;
                self.state.notes.push(NoteItem { id, title, body });
                self.state.selected_id = Some(id);
            }
            NoteCommand::SelectNote { id } => {
                if !self.state.notes.iter().any(|n| n.id == id) {
                    return err_resp(
                        envelope.id,
                        self.state.revision,
                        "NOT_FOUND",
                        format!("note {id} not found"),
                    );
                }
                self.state.selected_id = Some(id);
            }
            NoteCommand::UpdateNote { id, title, body } => {
                let Some(note) = self.state.notes.iter_mut().find(|n| n.id == id) else {
                    return err_resp(
                        envelope.id,
                        self.state.revision,
                        "NOT_FOUND",
                        format!("note {id} not found"),
                    );
                };
                note.title = title;
                note.body = body;
            }
            NoteCommand::DeleteNote { id } => {
                let before = self.state.notes.len();
                self.state.notes.retain(|n| n.id != id);
                if self.state.notes.len() == before {
                    return err_resp(
                        envelope.id,
                        self.state.revision,
                        "NOT_FOUND",
                        format!("note {id} not found"),
                    );
                }
                if self.state.selected_id == Some(id) {
                    self.state.selected_id = self.state.notes.first().map(|n| n.id);
                }
            }
            NoteCommand::SetSearch { query } => {
                self.state.search = query;
            }
        }

        self.state.revision += 1;
        self.persist();
        CommandResponse {
            id: envelope.id,
            ok: true,
            status: CommandStatus::Completed,
            revision: self.state.revision,
            task_id: None,
            message: Some("ok".to_owned()),
            ui: None,
            data: serde_json::to_value(&self.state).ok(),
            error: None,
        }
    }

    fn query(&self, query: Self::Query) -> Result<serde_json::Value, ProtocolError> {
        match query {
            NoteQuery::State => Ok(serde_json::to_value(&self.state).unwrap_or_default()),
            NoteQuery::Status => Ok(serde_json::json!({
                "app": "notes",
                "revision": self.state.revision,
                "note_count": self.state.notes.len(),
                "selected_id": self.state.selected_id,
            })),
        }
    }

    fn on_dispatch_done(
        &mut self,
        _envelope: &Envelope<Self::Command>,
        response: &CommandResponse,
        ctx: &RuntimeContext<Self>,
    ) {
        if response.ok {
            ctx.emit_event(NoteEvent::StateChanged {
                revision: self.state.revision,
            });
        }
    }
}

fn err_resp(id: String, revision: u64, code: &str, message: String) -> CommandResponse {
    CommandResponse {
        id,
        ok: false,
        status: CommandStatus::Failed,
        revision,
        task_id: None,
        message: Some(message.clone()),
        ui: None,
        data: None,
        error: Some(ProtocolError {
            code: code.to_owned(),
            message,
            ui: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fw_protocol::ClientKind;

    #[test]
    fn notes_core_crud_and_revision_conflict() {
        let mut core = NotesCore::new(None);
        assert_eq!(core.revision(), 0);
        let r1 = core.dispatch(Envelope::new(
            "t1",
            ClientKind::Cli,
            NoteCommand::CreateNote {
                title: "Test".to_owned(),
                body: "Body".to_owned(),
            },
        ));
        assert!(r1.ok);
        assert_eq!(core.revision(), 1);

        let mut bad = Envelope::new("t2", ClientKind::Cli, NoteCommand::DeleteNote { id: 1 });
        bad.expected_revision = Some(0);
        let r2 = core.dispatch(bad);
        assert!(!r2.ok);
        assert_eq!(r2.error.as_ref().unwrap().code, "REVISION_CONFLICT");
    }
}
