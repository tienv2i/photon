//! Shared CLI definition and initial-command resolution across `photon`,
//! `photon-manager`, `photon-viewer`, and `photon-editor`.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use photon_protocol::Command;

#[derive(Parser)]
#[command(
    name = "photon",
    version,
    about = "Photon — automation-first photo manager, viewer, and editor"
)]
pub struct Cli {
    /// Run Core + Local IPC without opening a GUI window
    #[arg(long)]
    pub headless: bool,

    /// Factory defaults in memory + force software renderer
    #[arg(long)]
    pub safe_mode: bool,

    /// Reset persisted panel layout & UI state to defaults before starting
    #[arg(long)]
    pub reset_layout: bool,

    /// Reset persisted settings to factory defaults before starting
    #[arg(long)]
    pub reset_settings: bool,

    /// Reset all settings and UI state to factory defaults before starting
    #[arg(long, alias = "reset-all")]
    pub reset_config: bool,

    /// Exit immediately after applying CLI reset flags without launching a GUI
    #[arg(long)]
    pub exit: bool,

    /// Override initial UI language (`en`, `vi`, or `system`)
    #[arg(long)]
    pub lang: Option<String>,

    /// Open in a standalone window instead of forwarding to an existing instance
    #[arg(long)]
    pub new_window: bool,

    /// Internal flag for spawning an independent Viewer process
    #[arg(long, hide = true)]
    pub viewer_window: bool,

    /// Start in Editor workspace (optionally opening positional FILE)
    #[arg(long)]
    pub edit: bool,

    #[command(subcommand)]
    pub sub: Option<Sub>,

    /// Positional image file(s) or folder to open
    pub paths: Vec<PathBuf>,
}

#[derive(Subcommand)]
pub enum Sub {
    /// Start in Browser workspace for DIR
    Browse { dir: Option<PathBuf> },
    /// Start in Viewer workspace for FILE...
    View { files: Vec<PathBuf> },
    /// Start in Editor workspace for optional FILE
    Edit { file: Option<PathBuf> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppRole {
    /// Unified `photon` CLI entrypoint supporting `browse`, `view`, `edit`, and `--headless`.
    Unified,
    /// Standalone `photon-manager` (Browser/Manager singleton).
    Manager,
    /// Standalone `photon-viewer` (Viewer multi-instance).
    Viewer,
    /// Standalone `photon-editor` (Editor singleton).
    Editor,
}

pub fn resolve_initial_command_for_role(cli: &Cli, role: AppRole) -> Option<Command> {
    if cli.sub.is_none() && !cli.paths.is_empty() {
        match role {
            AppRole::Editor => {
                return Some(Command::EditorOpen {
                    path: cli.paths[0].to_string_lossy().into_owned(),
                });
            }
            AppRole::Viewer => {
                return Some(Command::OpenFiles {
                    paths: cli
                        .paths
                        .iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect(),
                });
            }
            AppRole::Manager => {
                if cli.paths.len() == 1 && cli.paths[0].is_dir() {
                    return Some(Command::OpenFolder {
                        path: cli.paths[0].to_string_lossy().into_owned(),
                    });
                }
                if let Some(parent) = cli.paths[0].parent().filter(|p| !p.as_os_str().is_empty()) {
                    return Some(Command::OpenFolder {
                        path: parent.to_string_lossy().into_owned(),
                    });
                }
            }
            AppRole::Unified => {}
        }
    }
    resolve_initial_command(cli)
}

pub fn resolve_initial_command(cli: &Cli) -> Option<Command> {
    match &cli.sub {
        Some(Sub::Browse { dir }) => dir.as_ref().map(|d| Command::OpenFolder {
            path: d.to_string_lossy().into_owned(),
        }),
        Some(Sub::View { files }) if !files.is_empty() => Some(Command::OpenFiles {
            paths: files
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        }),
        Some(Sub::Edit { file }) => file.as_ref().map(|f| Command::EditorOpen {
            path: f.to_string_lossy().into_owned(),
        }),
        _ if !cli.paths.is_empty() => {
            if cli.edit {
                Some(Command::EditorOpen {
                    path: cli.paths[0].to_string_lossy().into_owned(),
                })
            } else if cli.paths.len() == 1 && cli.paths[0].is_dir() {
                Some(Command::OpenFolder {
                    path: cli.paths[0].to_string_lossy().into_owned(),
                })
            } else {
                Some(Command::OpenFiles {
                    paths: cli
                        .paths
                        .iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect(),
                })
            }
        }
        _ => None,
    }
}
