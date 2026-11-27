//! Shared Photon application launcher, CLI parser, IPC forwarding,
//! native image dialogs, and platform window activation.
//!
//! Intentionally independent of `photon-browser`, `photon-viewer`,
//! `photon-editor`, and `photon-canvas` so each app binary can stay lean.

pub mod cli;
pub mod image_dialogs;
pub mod ipc_launch;
pub mod window_activation;

pub use cli::{AppRole, Cli, Sub, resolve_initial_command, resolve_initial_command_for_role};
pub use ipc_launch::{
    LaunchRoute, canonicalize_path_str, compute_launch_route, compute_launch_route_for_role,
    forward_to_owner, main_instance_viewing_path, spawn_viewer_window_child,
    try_forward_to_running,
};
