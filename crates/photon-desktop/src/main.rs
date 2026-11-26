//! `photon`: Unified Browser / Viewer / Editor (`iced 0.14` + `wgpu 27`) with `--headless` daemon mode.

mod app;
mod inspector;
mod window_activation;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use photon_core::{Core, CoreRuntime};
use photon_ipc::{
    IpcRequest, IpcResponse, ViewerInstanceInfo, acquire_viewer_path_lock, bind_socket,
    find_any_live_viewer, find_live_viewer_for_path, register_viewer_instance, send_once_timeout,
    serve, serve_bound, viewer_socket_path,
};
use photon_protocol::{ClientKind, Command, Envelope, Language, Query};
use photon_storage::logging::{initialize, log_shutdown, log_startup};
use photon_storage::{AppIdentity, AppPaths, Config, UiState};

/// How long a losing launch waits for the winner to publish its socket.
const LAUNCH_LOCK_WAIT: Duration = Duration::from_millis(1500);
/// Per-attempt connect bound for launch-time forwarding.
const FORWARD_CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
/// Total budget for launch-time forwarding retries.
const FORWARD_TOTAL_TIMEOUT: Duration = Duration::from_secs(2);
/// Maximum single response wait inside the forwarding budget.
const FORWARD_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(unix)]
type PreboundListener = tokio::net::UnixListener;
#[cfg(not(unix))]
type PreboundListener = ();

/// Exclusive per-socket launch claim. Released when the process exits or the
/// file handle is dropped; the lock file itself is left in place.
struct LaunchLock {
    #[cfg(unix)]
    _file: std::fs::File,
}

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
            let viewer_single = !cli.safe_mode
                && Config::load_or_create(&paths.config_dir.join("config.json"))
                    .0
                    .viewer_single_instance;
            if viewer_single && let Some(live) = find_any_live_viewer(&paths) {
                let _ = try_forward_to_running(
                    &live.socket_path,
                    Command::OpenFiles { paths: raw_files },
                );
                let _ = try_forward_to_running(&live.socket_path, Command::FocusWindow);
                return;
            }
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
        // Stale socket files are reclaimed atomically by `bind_socket` after the
        // launch lock is held; removing the path here could delete a live
        // instance's socket while it is busy.
        if initial_cmd.is_none() {
            initial_cmd = Some(Command::SetWorkspace {
                workspace: photon_protocol::Workspace::Editor,
            });
        }
    } else if route == LaunchRoute::ForwardOrStartBrowser {
        let forward_cmd = initial_cmd.clone().unwrap_or(Command::SetWorkspace {
            workspace: photon_protocol::Workspace::Browser,
        });
        if try_forward_to_running(&paths.socket_path, forward_cmd) {
            let _ = try_forward_to_running(&paths.socket_path, Command::FocusWindow);
            return;
        }
    }

    log_startup(&identity, &paths, cli.headless);

    let pid = std::process::id();
    let socket_path = if standalone_viewer {
        viewer_socket_path(&paths, pid)
    } else if standalone_editor {
        paths.editor_socket_path()
    } else {
        paths.socket_path.clone()
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    // Claim singleton ownership *before* heavy Core/GUI initialisation. The
    // winner binds the socket immediately; losers forward and exit so two
    // near-simultaneous launches can never open duplicate windows.
    let mut _launch_lock: Option<LaunchLock> = None;
    let mut prebound_listener: Option<PreboundListener> = None;
    // Only singleton roles claim the runtime socket. `--new-window` and
    // `--headless` use `DirectStart` and must remain able to start alongside
    // an existing Browser/Editor.
    if matches!(
        route,
        LaunchRoute::ForwardOrStartBrowser | LaunchRoute::StandaloneEditor
    ) {
        let lock = match acquire_launch_lock(&socket_path, LAUNCH_LOCK_WAIT) {
            Ok(lock) => lock,
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {
                tracing::warn!(
                    target: "photon::startup",
                    socket = %socket_path.display(),
                    %err,
                    "another launch holds the instance lock; forwarding"
                );
                if forward_to_owner(&socket_path, &initial_cmd) {
                    return;
                }
                eprintln!(
                    "photon: another instance is starting for {} but did not answer within the launch timeout; not opening a duplicate window",
                    socket_path.display()
                );
                std::process::exit(1);
            }
            Err(err) => {
                tracing::error!(
                    target: "photon::startup",
                    socket = %socket_path.display(),
                    %err,
                    "failed to acquire launch lock"
                );
                eprintln!(
                    "photon: cannot acquire launch lock for {}: {err}",
                    socket_path.display()
                );
                std::process::exit(1);
            }
        };
        _launch_lock = Some(lock);

        #[cfg(unix)]
        {
            match rt.block_on(bind_socket(&socket_path)) {
                Ok(listener) => prebound_listener = Some(listener),
                Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
                    tracing::warn!(
                        target: "photon::startup",
                        socket = %socket_path.display(),
                        %err,
                        "another instance already owns the socket; forwarding"
                    );
                    if forward_to_owner(&socket_path, &initial_cmd) {
                        return;
                    }
                    eprintln!(
                        "photon: {} is owned by another instance that did not answer within the launch timeout; not opening a duplicate window",
                        socket_path.display()
                    );
                    std::process::exit(1);
                }
                Err(err) => {
                    tracing::error!(
                        target: "photon::startup",
                        socket = %socket_path.display(),
                        %err,
                        "failed to bind IPC socket"
                    );
                    eprintln!("photon: failed to bind {}: {err}", socket_path.display());
                    std::process::exit(1);
                }
            }
        }
    }

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

    let initial_viewer_canon = core
        .snapshot()
        .meta
        .viewer_path
        .as_deref()
        .map(canonicalize_path_str);
    let hw_accel = core.config.hardware_acceleration;
    let initial_display_font = core.config.layout.display_font;
    let initial_font_scale = core.config.layout.font_scale();
    let use_software_renderer = cli.safe_mode || !hw_accel;
    let (handle, gpu_backend, _viewer_reg_guard) = {
        let _guard = rt.enter();
        let handle = CoreRuntime::spawn(core);
        let ipc_handle = handle.clone();
        let ipc_socket = socket_path.clone();
        rt.spawn(async move {
            #[cfg(unix)]
            let result = match prebound_listener {
                Some(listener) => serve_bound(listener, ipc_handle).await,
                None => serve(&ipc_socket, ipc_handle).await,
            };
            #[cfg(not(unix))]
            let result = {
                let _ = prebound_listener;
                serve(&ipc_socket, ipc_handle).await
            };
            if let Err(err) = result {
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
        .settings(iced::Settings {
            default_font: app::iced_font_for(initial_display_font),
            default_text_size: iced::Pixels(13.0 * initial_font_scale),
            ..Default::default()
        })
        .title(app::title)
        .theme(app::theme)
        .scale_factor(app::scale_factor)
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
        let Ok(IpcResponse::Query { data }) = send_once_timeout(
            socket_path,
            &IpcRequest::Query {
                query: Query::State,
            },
            FORWARD_CONNECT_TIMEOUT,
            FORWARD_RESPONSE_TIMEOUT,
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

/// Forward a command to a live instance with a short overall budget. Returns
/// `false` immediately when the socket does not exist so cold starts stay fast.
fn try_forward_to_running(socket_path: &Path, command: Command) -> bool {
    if !socket_path.exists() {
        return false;
    }
    forward_command_bounded(socket_path, command)
}

/// Lock/acquisition loser path: the owner may still be publishing its socket,
/// so retry within the launch budget even if the path is not visible yet.
fn forward_to_owner(socket_path: &Path, initial_cmd: &Option<Command>) -> bool {
    let forward_cmd = initial_cmd.clone().unwrap_or(Command::FocusWindow);
    if !forward_command_bounded(socket_path, forward_cmd) {
        return false;
    }
    if initial_cmd.is_some() {
        // `app.rs` raises the window on the wake sequence; keep the legacy
        // "command + FocusWindow" semantics from the previous fix.
        let _ = forward_command_bounded(socket_path, Command::FocusWindow);
    }
    true
}

fn forward_command_bounded(socket_path: &Path, command: Command) -> bool {
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
    rt.block_on(forward_envelope(socket_path, env))
}

async fn forward_envelope(socket_path: &Path, envelope: Envelope) -> bool {
    let deadline = Instant::now() + FORWARD_TOTAL_TIMEOUT;
    let mut last_error: Option<String> = None;
    const ATTEMPTS: usize = 3;

    for attempt in 0..ATTEMPTS {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let response_timeout = remaining.min(FORWARD_RESPONSE_TIMEOUT);
        match send_once_timeout(
            socket_path,
            &IpcRequest::Dispatch {
                envelope: envelope.clone(),
            },
            FORWARD_CONNECT_TIMEOUT,
            response_timeout,
        )
        .await
        {
            Ok(_) => return true,
            Err(err) => last_error = Some(err),
        }
        if attempt + 1 < ATTEMPTS {
            let pause = deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100));
            if pause.is_zero() {
                break;
            }
            tokio::time::sleep(pause).await;
        }
    }

    tracing::warn!(
        target: "photon::startup",
        socket = %socket_path.display(),
        error = last_error.as_deref().unwrap_or("unknown forwarding error"),
        "could not reach the running instance within the launch timeout"
    );
    false
}

fn launch_lock_path(socket_path: &Path) -> PathBuf {
    let name = socket_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("ipc");
    socket_path.with_file_name(format!("{name}.launch.lock"))
}

/// Per-socket exclusive launch claim (`flock(LOCK_EX | LOCK_NB)`).
///
/// A dedicated lock per Browser/Editor socket avoids serialising the two
/// roles against each other while still making simultaneous launches of the
/// same role deterministic. The kernel releases the lock automatically when
/// the process dies, so a crashed owner cannot leave a stuck claim.
fn acquire_launch_lock(socket_path: &Path, timeout: Duration) -> std::io::Result<LaunchLock> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::io::AsRawFd;

        let lock_path = launch_lock_path(socket_path);
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        let _ = std::fs::set_permissions(&lock_path, std::fs::Permissions::from_mode(0o600));

        let deadline = Instant::now() + timeout;
        loop {
            let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if rc == 0 {
                return Ok(LaunchLock { _file: file });
            }
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::EWOULDBLOCK)
                && err.raw_os_error() != Some(libc::EAGAIN)
            {
                return Err(err);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("timed out waiting for launch lock {}", lock_path.display()),
                ));
            }
            std::thread::sleep(remaining.min(Duration::from_millis(50)));
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (socket_path, timeout);
        Ok(LaunchLock {})
    }
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

    #[cfg(unix)]
    #[test]
    fn launch_lock_is_exclusive_and_released_on_drop() {
        let root = std::env::temp_dir().join(format!(
            "photon-launch-lock-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let socket = root.join("control.sock");

        let first = acquire_launch_lock(&socket, Duration::from_millis(50)).unwrap();
        let err = acquire_launch_lock(&socket, Duration::from_millis(60))
            .err()
            .expect("second claim must not acquire while the first is held");
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);

        drop(first);
        let reacquired = acquire_launch_lock(&socket, Duration::from_millis(200));
        assert!(reacquired.is_ok());

        drop(reacquired);
        let _ = std::fs::remove_dir_all(&root);
    }
}
