//! Domain-agnostic command envelope, response, error, and UI text protocol
//! shared across framework runtimes, IPC servers, CLI/MCP adapters, and apps.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Wire protocol version. Independent from application semantic version.
pub const PROTOCOL_VERSION: u32 = 1;

fn default_protocol() -> u32 {
    PROTOCOL_VERSION
}

/// Origin adapter submitting a command envelope.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Gui,
    #[default]
    Cli,
    Mcp,
    Automation,
    Replay,
}

/// Minimal metadata trait implemented by each application's `Command` enum so
/// framework journals and runtime tracing stay domain-independent.
pub trait ProtocolCommand: Clone + Serialize {
    /// Stable snake_case command name for logs, events, and journal records.
    fn command_name(&self) -> &str;

    /// Paths referenced by this command for sanitized journal indexing.
    fn journal_paths(&self) -> Vec<&str> {
        Vec::new()
    }
}

/// A command envelope. `id` is echoed in the response, recorded in the journal,
/// and propagated into `tracing` spans for end-to-end `trace --request-id`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Envelope<C> {
    pub id: String,
    #[serde(default = "default_protocol")]
    pub protocol: u32,
    #[serde(default)]
    pub source: ClientKind,
    /// Optional optimistic concurrency check. If provided and != current state revision,
    /// the gateway rejects the command with `revision_conflict`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<u64>,
    /// Optional idempotency key to prevent duplicate execution on retries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    pub command: C,
}

impl<C> Envelope<C> {
    pub fn new(id: impl Into<String>, source: ClientKind, command: C) -> Self {
        Self {
            id: id.into(),
            protocol: PROTOCOL_VERSION,
            source,
            expected_revision: None,
            idempotency_key: None,
            command,
        }
    }
}

/// Outcome of submitting an `Envelope` to the gateway.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommandResponse {
    pub id: String,
    pub ok: bool,
    pub status: CommandStatus,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProtocolError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Completed,
    Accepted,
    Failed,
}

/// Translatable message emitted by Core without pulling i18n dependencies into Core.
///
/// Core knows *what* happened (`code` + named `args`); the presentation edge
/// (GUI or localized CLI) renders it via Fluent `.ftl`, while logs and the
/// journal always record `UiText::english()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiText {
    pub code: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
}

impl UiText {
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            args: BTreeMap::new(),
        }
    }

    pub fn with(
        code: impl Into<String>,
        args: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        Self {
            code: code.into(),
            args: args
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }

    pub fn empty() -> Self {
        Self::new("")
    }

    pub fn is_empty(&self) -> bool {
        self.code.is_empty()
    }

    /// Mechanical English fallback for journals, tracing logs, and headless CLI.
    pub fn english(&self) -> String {
        if self.code.is_empty() {
            return String::new();
        }
        let mut out = self.code.replace(['_', '-'], " ");
        if !self.args.is_empty() {
            let formatted: Vec<String> =
                self.args.iter().map(|(k, v)| format!("{k}={v}")).collect();
            out.push_str(": ");
            out.push_str(&formatted.join(", "));
        }
        out
    }
}

/// Structured command/query rejection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiText>,
}

impl ProtocolError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            ui: None,
        }
    }

    pub fn line(code: impl Into<String>, ui: UiText) -> Self {
        let message = ui.english();
        Self {
            code: code.into(),
            message,
            ui: Some(ui),
        }
    }

    pub fn with_line(code: impl Into<String>, message: impl Into<String>, ui: UiText) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            ui: Some(ui),
        }
    }
}
