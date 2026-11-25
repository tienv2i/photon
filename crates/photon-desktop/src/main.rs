//! `photon`: Unified Browser / Viewer / Editor (`iced 0.14` + `wgpu 27`) with `--headless` daemon mode.

mod app;
mod inspector;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use photon_core::{Core, CoreRuntime};
use photon_ipc::{IpcRequest, send_once, serve};
use photon_protocol::{ClientKind, Command, Envelope, Language};
use photon_storage::logging::{initialize, log_shutdown, log_startup};
use photon_storage::{AppIdentity, AppPaths, Config};

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

    /// Reset persisted panel layout to defaults before starting
    #[arg(long)]
    reset_layout: bool,

    /// Reset all settings to factory defaults before starting
    #[arg(long)]
    reset_config: bool,

    /// Override initial UI language (`en`, `vi`, or `system`)
    #[arg(long)]
    lang: Option<String>,

    /// Open in a standalone window instead of forwarding to an existing instance
    #[arg(long)]
    new_window: bool,

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

    if cli.reset_config {
        let _ = Config::reset_to_factory(&paths.config_file());
    } else if cli.reset_layout {
        let _ = Config::reset_layout_only(&paths.config_file());
    }

    let initial_cmd = resolve_initial_command(&cli);
    let opened_with_files = matches!(
        initial_cmd,
        Some(Command::OpenFiles { .. } | Command::EditorOpen { .. })
    );

    // Single-instance coordination: forward open commands to an active instance unless `--new-window` or `--headless`.
    if !cli.new_window
        && !cli.headless
        && let Some(cmd) = initial_cmd.clone()
        && try_forward_to_running(&paths.socket_path, cmd)
    {
        return;
    }

    log_startup(&identity, &paths, cli.headless);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut core = Core::new(cli.headless, cli.safe_mode);
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

    let socket_path = paths.socket_path.clone();
    let (handle, gpu_backend) = {
        let _guard = rt.enter();
        let handle = CoreRuntime::spawn(core);
        let ipc_handle = handle.clone();
        let ipc_socket = socket_path.clone();
        rt.spawn(async move {
            if let Err(err) = serve(&ipc_socket, ipc_handle).await {
                tracing::error!(target: "photon::ipc", %err, "IPC server stopped");
            }
        });
        let gpu_backend = if cli.headless {
            "headless"
        } else if cli.safe_mode {
            "tiny-skia"
        } else {
            "wgpu"
        };
        (handle, gpu_backend.to_owned())
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

    if cli.safe_mode {
        // Safety: invoked on main thread before `iced::daemon` initializes the renderer.
        unsafe { std::env::set_var("ICED_BACKEND", "tiny-skia") };
    }

    app::init_globals(handle, gpu_backend, opened_with_files);

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

fn try_forward_to_running(socket_path: &std::path::Path, command: Command) -> bool {
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
