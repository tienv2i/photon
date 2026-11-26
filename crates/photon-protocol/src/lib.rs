//! Typed protocol shared across Photon's Core, GUI (`iced`), CLI (`photonctl`),
//! Local IPC, Automation runners, and MCP adapter.
//!
//! Commands are the only way to mutate Core state. Queries are read-only, and
//! Events are sequenced notifications emitted by Core.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod ai;
mod batch;
mod command;
mod editor_types;
mod layout;
mod state;
mod translate;

pub use ai::*;
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
    "jpg", "jpeg", "jpe", "jfif", "png", "webp", "gif", "bmp", "tif", "tiff", "avif", "ico",
    "icns", "pnm", "pbm", "pgm", "ppm", "qoi", "tga", "hdr", "exr", "ora", "kra", "svg", "svgz",
    "apng", "pam", "dds", "ff", "cur", "jxl", "psd", "heic", "heif", "hif", "jp2", "j2k", "jpx",
    "jpf", "dng", "cr2", "cr3", "nef", "arw", "raf", "orf", "rw2", "ari", "crm", "crw", "dcr",
    "dcs", "erf", "iiq", "kdc", "mef", "mos", "mrw", "nrw", "ori", "pef", "raw", "rwl", "srw",
    "3fr", "fff", "x3f", "qtk",
];

/// Supported archive/document container extensions opened as paged Viewer sessions.
pub const ARCHIVE_EXTENSIONS: &[&str] = &["cbz", "zip", "pdf"];

/// Returns `true` if `ext` (case-insensitive, without leading dot) is a supported archive/document container.
pub fn is_archive_extension(ext: &str) -> bool {
    let lower = ext.trim_start_matches('.').to_ascii_lowercase();
    ARCHIVE_EXTENSIONS.contains(&lower.as_str())
}

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

#[cfg(test)]
mod tests {
    use super::IMAGE_EXTENSIONS;

    #[test]
    fn image_extensions_include_jp2_and_heif_variants() {
        for ext in [
            "avif", "heic", "heif", "hif", "jp2", "j2k", "jpx", "jpf", "icns",
        ] {
            assert!(
                IMAGE_EXTENSIONS.contains(&ext),
                "missing extension {ext} in IMAGE_EXTENSIONS"
            );
        }
    }

    #[test]
    fn image_extensions_include_kra_and_extra_raw_formats() {
        for ext in [
            "kra", "ari", "crm", "crw", "dcr", "dcs", "erf", "iiq", "kdc", "mef", "mos", "mrw",
            "nrw", "ori", "pef", "raw", "rwl", "srw", "3fr", "fff", "x3f", "qtk",
        ] {
            assert!(
                IMAGE_EXTENSIONS.contains(&ext),
                "missing extension {ext} in IMAGE_EXTENSIONS"
            );
        }
    }

    #[test]
    fn archive_extensions_check_works_case_insensitively() {
        assert!(super::is_archive_extension("cbz"));
        assert!(super::is_archive_extension("CBZ"));
        assert!(super::is_archive_extension(".zip"));
        assert!(super::is_archive_extension("pdf"));
        assert!(!super::is_archive_extension("png"));
    }
}
