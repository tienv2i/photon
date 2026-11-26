//! `photon`: Unified Browser / Viewer / Editor (`iced 0.14` + `wgpu 27`) with `--headless` daemon mode.

mod app;
mod inspector;
mod window_activation;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use clap::{Parser, Subcommand};
use photon_core::{Core, CoreRuntime};
use photon_ipc::{
    IpcRequest, IpcResponse, ViewerInstanceInfo, acquire_viewer_path_lock,
    find_live_viewer_for_path, register_viewer_instance, send_once, serve, viewer_socket_path,
};
use photon_protocol::{ClientKind, Command, Envelope, Language, Query};
use photon_storage::logging::{initialize, log_shutdown, log_startup};
use photon_storage::{AppIdentity, AppPaths, Config, UiState};

#[derive(Parser)]
#[command(
    name = "photon",
    version,
    about = "Photon — automation-first photo manager, viewer, and editor"
)]
struct Cli {
    /// Run Core + Local IPC without opening a GUI window
    #[arg(long)]
    headless: bool,

    /// Factory defaults in memory + force software renderer
    #[arg(long)]
    safe_mode: bool,

    /// Reset persisted panel layout & UI state to defaults before starting
    #[arg(long)]
    reset_layout: bool,

    /// Reset persisted settings to factory defaults before starting
    #[arg(long)]
    reset_settings: bool,

    /// Reset all settings and UI state to factory defaults before starting
    #[arg(long, alias = "reset-all")]
    reset_config: bool,

    /// Exit immediately after applying CLI reset flags without launching a GUI
    #[arg(long)]
    exit: bool,

    /// Override initial UI language (`en`, `vi`, or `system`)
    #[arg(long)]
    lang: Option<String>,

    /// Open in a standalone window instead of forwarding to an existing instance
    #[arg(long)]
    new_window: bool,

    /// Internal flag for spawning an independent Viewer process
    #[arg(long, hide = true)]
    viewer_window: bool,

    /// Start in Editor workspace (optionally opening positional FILE)
    #[arg(long)]
    edit: bool,

    #[command(subcommand)]
    sub: Option<Sub>,

    /// Positional image file(s) or folder to open
    paths: Vec<PathBuf>,
}

#[derive(Subcommand)]
enum Sub {
    /// Start in Browser workspace for DIR
    Browse { dir: Option<PathBuf> },
    /// Start in Viewer workspace for FILE...
    View { files: Vec<PathBuf> },
    /// Start in Editor workspace for optional FILE
    Edit { file: Option<PathBuf> },
}

fn main() {
    let cli = Cli::parse();
    let identity = AppIdentity::current();
    let paths = AppPaths::for_identity(&identity);

    if let Err(err) = initialize(&paths, true) {
        eprintln!("photon: structured logging unavailable ({err}); continuing");
    }

    let want_reset_settings = cli.reset_config || cli.reset_settings;
    let want_reset_layout = cli.reset_config || cli.reset_layout;
    if want_reset_settings || want_reset_layout {
        let forwarded = !cli.headless
            && try_forward_to_running(
                &paths.socket_path,
                Command::ResetConfig {
                    reset_layout: want_reset_layout,
                    reset_settings: want_reset_settings,
                },
            );
        if !forwarded {
            if want_reset_settings {
                let _ = Config::reset_settings_only(&paths.config_file());
            }
            if want_reset_layout {
                let _ = UiState::reset_layout_only(&paths.ui_state_file(), &paths.config_file());
            }
        }
        if cli.exit {
            return;
        }
    }

    let mut initial_cmd = resolve_initial_command(&cli);
    let is_sub_edit = matches!(cli.sub, Some(Sub::Edit { .. }));
    let route = compute_launch_route(
        cli.headless,
        cli.new_window,
        cli.viewer_window,
        cli.edit,
        is_sub_edit,
        initial_cmd.as_ref(),
    );
    let standalone_viewer = route == LaunchRoute::StandaloneViewer;
    let standalone_editor = route == LaunchRoute::StandaloneEditor;
    let opened_with_files = matches!(
        initial_cmd,
        Some(Command::OpenFiles { .. } | Command::OpenArchive { .. } | Command::EditorOpen { .. })
    );

    if standalone_viewer {
        let raw_files: Option<Vec<String>> = match &initial_cmd {
            Some(Command::OpenFiles { paths }) => Some(paths.clone()),
            Some(Command::OpenArchive { path }) => Some(vec![path.clone()]),
            _ => None,
        };
        if let Some(raw_files) = raw_files
            && !cli.viewer_window
            && !cli.new_window
        {
            let mut to_open = Vec::new();
            for raw in raw_files {
                let canon = canonicalize_path_str(&raw);
                let _lock = acquire_viewer_path_lock(&paths, &canon, Duration::from_secs(2));
                if let Some(matched) = find_live_viewer_for_path(&paths, &canon) {
                    let _ = try_forward_to_running(&matched.info.socket_path, Command::FocusWindow);
                } else if main_instance_viewing_path(&paths.socket_path, &canon) {
                    let _ = try_forward_to_running(&paths.socket_path, Command::FocusWindow);
                } else {
                    to_open.push(raw);
                }
            }
            if to_open.is_empty() {
                return;
            }
            if to_open.len() > 1 {
                for extra in &to_open[1..] {
                    spawn_viewer_window_child(extra, cli.safe_mode, cli.lang.as_deref());
                }
            }
            if matches!(initial_cmd, Some(Command::OpenFiles { .. }))
                && let Some(first) = to_open.into_iter().next()
            {
                initial_cmd = Some(Command::OpenFiles { paths: vec![first] });
            }
        }
    } else if standalone_editor {
        let editor_sock = paths.editor_socket_path();
        let forward_cmd = initial_cmd.clone().unwrap_or(Command::FocusWindow);
        if try_forward_to_running(&editor_sock, forward_cmd) {
            if initial_cmd.is_some() {
                let _ = try_forward_to_running(&editor_sock, Command::FocusWindow);
            }
            return;
        }
        let _ = std::fs::remove_file(&editor_sock);
        if initial_cmd.is_none() {
            initial_cmd = Some(Command::SetWorkspace {
                workspace: photon_protocol::Workspace::Editor,
            });
        }
    } else if route == LaunchRoute::ForwardOrStartBrowser {
        let forward_cmd = initial_cmd.clone().unwrap_or(Command::FocusWindow);
        if try_forward_to_running(&paths.socket_path, forward_cmd) {
            if initial_cmd.is_some() {
                let _ = try_forward_to_running(&paths.socket_path, Command::FocusWindow);
            }
            return;
        }
    }

    log_startup(&identity, &paths, cli.headless);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut core = if standalone_viewer || standalone_editor {
        Core::new_viewer_ephemeral(cli.headless, cli.safe_mode)
    } else {
        Core::new(cli.headless, cli.safe_mode)
    };
    if let Some(lang_str) = cli.lang.as_deref() {
        let lang = match lang_str.to_ascii_lowercase().as_str() {
            "vi" => Some(Language::Vietnamese),
            "en" => Some(Language::English),
            "system" => Some(Language::System),
            _ => None,
        };
        if let Some(language) = lang {
            let _ = core.dispatch(Envelope::new(
                "boot-lang",
                ClientKind::Cli,
                Command::SetLanguage { language },
            ));
        }
    }
    if let Some(cmd) = initial_cmd {
        let _ = core.dispatch(Envelope::new("boot-open", ClientKind::Cli, cmd));
    }

    let pid = std::process::id();
    let socket_path = if standalone_viewer {
        viewer_socket_path(&paths, pid)
    } else if standalone_editor {
        paths.editor_socket_path()
    } else {
        paths.socket_path.clone()
    };
    let initial_viewer_canon = core
        .snapshot()
        .meta
        .viewer_path
        .as_deref()
        .map(canonicalize_path_str);
    let hw_accel = core.config.hardware_acceleration;
    let use_software_renderer = cli.safe_mode || !hw_accel;
    let (handle, gpu_backend, _viewer_reg_guard) = {
        let _guard = rt.enter();
        let handle = CoreRuntime::spawn(core);
        let ipc_handle = handle.clone();
        let ipc_socket = socket_path.clone();
        rt.spawn(async move {
            if let Err(err) = serve(&ipc_socket, ipc_handle).await {
                tracing::error!(target: "photon::ipc", %err, "IPC server stopped");
            }
        });
        let viewer_guard = if standalone_viewer {
            let active_canonical_path = initial_viewer_canon.unwrap_or_default();
            let open_canonical_paths = if active_canonical_path.as_os_str().is_empty() {
                Vec::new()
            } else {
                vec![active_canonical_path.clone()]
            };
            let info = ViewerInstanceInfo {
                pid,
                socket_path: socket_path.clone(),
                active_canonical_path,
                open_canonical_paths,
            };
            match register_viewer_instance(&paths, &info) {
                Ok(guard) => Some(guard),
                Err(err) => {
                    tracing::warn!(target: "photon::ipc", %err, "failed to register viewer instance");
                    None
                }
            }
        } else {
            None
        };
        let gpu_backend = if cli.headless {
            "headless"
        } else if use_software_renderer
            || std::env::var("ICED_BACKEND").ok().as_deref() == Some("tiny-skia")
        {
            "tiny-skia"
        } else {
            "wgpu"
        };
        (handle, gpu_backend.to_owned(), viewer_guard)
    };

    if cli.headless {
        tracing::info!(target: "photon::startup", "running in headless automation mode (Ctrl+C to stop)");
        rt.block_on(async {
            let _ = tokio::signal::ctrl_c().await;
        });
        log_shutdown();
        return;
    }

    // Keep Tokio runtime alive on a background thread for `CoreRuntime` & `photon-ipc`
    // while `iced::daemon` drives the GUI event loop on the main thread.
    let _rt_keeper = std::thread::spawn(move || {
        rt.block_on(std::future::pending::<()>());
    });

    if use_software_renderer {
        // Safety: invoked on main thread before `iced::daemon` initializes the renderer.
        unsafe { std::env::set_var("ICED_BACKEND", "tiny-skia") };
    } else if std::env::var_os("ICED_BACKEND").is_none() {
        // Safety: invoked on main thread before `iced::daemon` initializes the renderer.
        unsafe { std::env::set_var("ICED_BACKEND", "wgpu") };
    }

    app::init_globals(
        handle,
        gpu_backend,
        opened_with_files,
        standalone_viewer,
        standalone_editor,
    );

    if let Err(err) = iced::daemon(app::boot, app::update, app::view)
        .title(app::title)
        .theme(app::theme)
        .subscription(app::subscription)
        .run()
    {
        tracing::error!(target: "photon::desktop", %err, "iced runtime exited with error");
    }

    log_shutdown();
}

fn resolve_initial_command(cli: &Cli) -> Option<Command> {
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

fn canonicalize_path_str(raw: &str) -> PathBuf {
    std::fs::canonicalize(raw).unwrap_or_else(|_| PathBuf::from(raw))
}

fn spawn_viewer_window_child(path: &str, safe_mode: bool, lang: Option<&str>) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--viewer-window");
    if safe_mode {
        cmd.arg("--safe-mode");
    }
    if let Some(l) = lang {
        cmd.args(["--lang", l]);
    }
    cmd.arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let _ = cmd.spawn();
}

fn main_instance_viewing_path(socket_path: &Path, canon: &Path) -> bool {
    if !socket_path.exists() {
        return false;
    }
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return false;
    };
    rt.block_on(async move {
        let Ok(IpcResponse::Query { data }) = send_once(
            socket_path,
            &IpcRequest::Query {
                query: Query::State,
            },
        )
        .await
        else {
            return false;
        };
        let ws = data
            .get("meta")
            .and_then(|m| m.get("workspace"))
            .and_then(|v| v.as_str());
        let vp = data
            .get("meta")
            .and_then(|m| m.get("viewerPath"))
            .and_then(|v| v.as_str());
        ws == Some("viewer") && vp.is_some_and(|p| canonicalize_path_str(p) == canon)
    })
}

fn try_forward_to_running(socket_path: &Path, command: Command) -> bool {
    if !socket_path.exists() {
        return false;
    }
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return false;
    };
    let env = Envelope::new(
        format!("fwd-{}", std::process::id()),
        ClientKind::Cli,
        command,
    );
    rt.block_on(async move {
        send_once(socket_path, &IpcRequest::Dispatch { envelope: env })
            .await
            .is_ok()
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaunchRoute {
    StandaloneViewer,
    StandaloneEditor,
    ForwardOrStartBrowser,
    DirectStart,
}

fn is_standalone_viewer_command(cmd: Option<&Command>) -> bool {
    matches!(
        cmd,
        Some(Command::OpenFiles { .. } | Command::OpenArchive { .. })
    )
}

fn is_standalone_editor_command(cmd: Option<&Command>) -> bool {
    matches!(
        cmd,
        Some(Command::EditorOpen { .. } | Command::OpenInEditorOrUnsupported { .. })
    )
}

fn compute_launch_route(
    headless: bool,
    new_window: bool,
    viewer_window: bool,
    editor_flag: bool,
    is_sub_edit: bool,
    initial_cmd: Option<&Command>,
) -> LaunchRoute {
    if !headless && (viewer_window || is_standalone_viewer_command(initial_cmd)) {
        LaunchRoute::StandaloneViewer
    } else if !headless
        && !new_window
        && (editor_flag || is_sub_edit || is_standalone_editor_command(initial_cmd))
    {
        LaunchRoute::StandaloneEditor
    } else if !new_window && !headless {
        LaunchRoute::ForwardOrStartBrowser
    } else {
        LaunchRoute::DirectStart
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_viewer_command_matches_files_and_archives_only() {
        assert!(is_standalone_viewer_command(Some(&Command::OpenFiles {
            paths: vec!["/tmp/a.png".to_owned()],
        })));
        assert!(is_standalone_viewer_command(Some(&Command::OpenArchive {
            path: "/tmp/book.cbz".to_owned(),
        })));
        assert!(!is_standalone_viewer_command(Some(&Command::OpenFolder {
            path: "/tmp".to_owned(),
        })));
        assert!(!is_standalone_viewer_command(Some(&Command::EditorOpen {
            path: "/tmp/a.png".to_owned(),
        })));
        assert!(!is_standalone_viewer_command(None));
    }

    #[test]
    fn launch_routing_enforces_browser_editor_singletons_and_multi_viewer() {
        // Empty launch (no args) and folder launch route through single-instance Browser.
        assert_eq!(
            compute_launch_route(false, false, false, false, false, None),
            LaunchRoute::ForwardOrStartBrowser
        );
        assert_eq!(
            compute_launch_route(
                false,
                false,
                false,
                false,
                false,
                Some(&Command::OpenFolder {
                    path: "/tmp".to_owned(),
                }),
            ),
            LaunchRoute::ForwardOrStartBrowser
        );
        // `photon edit` (with or without file) and `--edit` route through single-instance Editor.
        assert_eq!(
            compute_launch_route(false, false, false, false, true, None),
            LaunchRoute::StandaloneEditor
        );
        assert_eq!(
            compute_launch_route(false, false, false, true, false, None),
            LaunchRoute::StandaloneEditor
        );
        assert_eq!(
            compute_launch_route(
                false,
                false,
                false,
                false,
                true,
                Some(&Command::EditorOpen {
                    path: "/tmp/a.png".to_owned(),
                }),
            ),
            LaunchRoute::StandaloneEditor
        );
        // Files and archives open in independent multi-instance Viewer windows.
        assert_eq!(
            compute_launch_route(
                false,
                false,
                false,
                false,
                false,
                Some(&Command::OpenFiles {
                    paths: vec!["/tmp/a.png".to_owned()],
                }),
            ),
            LaunchRoute::StandaloneViewer
        );
        assert_eq!(
            compute_launch_route(
                false,
                false,
                false,
                false,
                false,
                Some(&Command::OpenArchive {
                    path: "/tmp/manga.pdf".to_owned(),
                }),
            ),
            LaunchRoute::StandaloneViewer
        );
        // Explicit --viewer-window forces StandaloneViewer; --new-window / --headless bypass forwarding.
        assert_eq!(
            compute_launch_route(false, false, true, false, false, None),
            LaunchRoute::StandaloneViewer
        );
        assert_eq!(
            compute_launch_route(false, true, false, false, false, None),
            LaunchRoute::DirectStart
        );
        assert_eq!(
            compute_launch_route(true, false, false, false, false, None),
            LaunchRoute::DirectStart
        );
    }
}
