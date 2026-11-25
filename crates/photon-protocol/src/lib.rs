//! Typed protocol shared across Photon's Core, GUI (`iced`), CLI (`photonctl`),
//! Local IPC, Automation runners, and MCP adapter.
//!
//! Commands are the only way to mutate Core state. Queries are read-only, and
//! Events are sequenced notifications emitted by Core.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod batch;
mod command;
mod editor_types;
mod layout;
mod state;
mod translate;

pub use batch::*;
pub use command::*;
pub use editor_types::*;
pub use layout::*;
pub use state::*;
pub use translate::*;

/// Wire protocol version. Independent from application semantic version.
pub const PROTOCOL_VERSION: u32 = 1;

/// Supported image extensions recognized by Browser, Viewer, and Editor.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "webp", "gif", "bmp", "tif", "tiff", "avif", "ico", "pnm",
    "pbm", "pgm", "ppm", "qoi", "tga", "hdr", "exr",
];

/// Three unified workspaces sharing one Core and state owner.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Workspace {
    #[default]
    Browser,
    Viewer,
    Editor,
}

impl Workspace {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::Viewer => "viewer",
            Self::Editor => "editor",
        }
    }
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
