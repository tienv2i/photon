//! `photon`: Unified CLI launcher dispatching to standalone `photon-manager`,
//! `photon-viewer`, and `photon-editor` binaries (including `--headless` via `photon-manager`).

use std::path::PathBuf;

use clap::Parser;
pub use photon_app_common::AppRole;
use photon_app_common::{
    Cli, LaunchRoute, Sub, compute_launch_route_for_role, resolve_initial_command_for_role,
};
use photon_storage::{AppPaths, Config, UiState};

pub fn run(role: AppRole) {
    let cli = Cli::parse();
    let identity = photon_storage::current_identity();
    let paths = AppPaths::for_identity(&identity);

    let want_reset_settings = cli.reset_config || cli.reset_settings;
    let want_reset_layout = cli.reset_config || cli.reset_layout;
    if want_reset_settings || want_reset_layout {
        if want_reset_settings {
            let _ = Config::reset_settings_only(&paths.config_file());
        }
        if want_reset_layout {
            let _ = UiState::reset_layout_only(&paths.ui_state_file(), &paths.config_file());
        }
        if cli.exit {
            return;
        }
    }

    let initial_cmd = resolve_initial_command_for_role(&cli, role);
    let is_sub_edit = matches!(cli.sub, Some(Sub::Edit { .. }));
    let route = compute_launch_route_for_role(
        role,
        cli.headless,
        cli.new_window,
        cli.viewer_window,
        cli.edit,
        is_sub_edit,
        initial_cmd.as_ref(),
    );
    let target_bin = if cli.headless {
        "photon-manager"
    } else {
        match route {
            LaunchRoute::StandaloneViewer => "photon-viewer",
            LaunchRoute::StandaloneEditor => "photon-editor",
            LaunchRoute::ForwardOrStartBrowser | LaunchRoute::DirectStart => "photon-manager",
        }
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(target_bin)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(target_bin));

    if cli.headless {
        let status = std::process::Command::new(&exe)
            .args(&args)
            .status()
            .unwrap_or_else(|err| {
                eprintln!("photon: failed to launch {}: {err}", exe.display());
                std::process::exit(1);
            });
        std::process::exit(status.code().unwrap_or(0));
    }

    if let Err(err) = std::process::Command::new(&exe).args(&args).spawn() {
        eprintln!("photon: failed to launch {}: {err}", exe.display());
        std::process::exit(1);
    }
}
