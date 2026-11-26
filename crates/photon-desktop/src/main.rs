//! `photon`: Unified Browser / Viewer / Editor (`iced 0.14` + `wgpu 27`) with `--headless` daemon mode.

mod app;
mod inspector;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use clap::{Parser, Subcommand};
use photon_core::{Core, CoreRuntime};
use photon_ipc::{
    IpcRequest, ViewerInstanceInfo, acquire_viewer_path_lock, find_live_viewer_for_path,
    register_viewer_instance, send_once, serve, viewer_socket_path,
};
use photon_protocol::{ClientKind, Command, Envelope, Language};
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
    /// Start in Editor workspace for FILE
    Edit { file: PathBuf },
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
    let standalone_viewer = !cli.headless
        && (cli.viewer_window || matches!(initial_cmd, Some(Command::OpenFiles { .. })));
    let opened_with_files = matches!(
        initial_cmd,
        Some(Command::OpenFiles { .. } | Command::EditorOpen { .. })
    );

    if standalone_viewer {
        if let Some(Command::OpenFiles {
            paths: ref raw_files,
        }) = initial_cmd.clone()
            && !cli.viewer_window
            && !cli.new_window
        {
            let mut to_open = Vec::new();
            for raw in raw_files {
                let canon = canonicalize_path_str(raw);
                let _lock = acquire_viewer_path_lock(&paths, &canon, Duration::from_secs(2));
                if let Some(matched) = find_live_viewer_for_path(&paths, &canon) {
                    let _ = try_forward_to_running(&matched.info.socket_path, Command::FocusWindow);
                } else {
                    to_open.push(raw.clone());
                }
            }
            if to_open.is_empty() && !raw_files.is_empty() {
                return;
            }
            if to_open.len() > 1 {
                for extra in &to_open[1..] {
                    spawn_viewer_window_child(extra, cli.safe_mode, cli.lang.as_deref());
                }
            }
            if let Some(first) = to_open.into_iter().next() {
                initial_cmd = Some(Command::OpenFiles { paths: vec![first] });
            }
        }
    } else if !cli.new_window
        && !cli.headless
        && let Some(cmd) = initial_cmd.clone()
        && try_forward_to_running(&paths.socket_path, cmd)
    {
        let _ = try_forward_to_running(&paths.socket_path, Command::FocusWindow);
        return;
    }

    log_startup(&identity, &paths, cli.headless);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut core = if standalone_viewer {
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

    app::init_globals(handle, gpu_backend, opened_with_files, standalone_viewer);

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
        Some(Sub::Edit { file }) => Some(Command::EditorOpen {
            path: file.to_string_lossy().into_owned(),
        }),
        _ if !cli.paths.is_empty() => {
            if cli.paths.len() == 1 && cli.paths[0].is_dir() {
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
