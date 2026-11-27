//! Single-instance launch routing and bounded IPC command forwarding.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use photon_ipc::{IpcRequest, IpcResponse, send_once_timeout};
use photon_protocol::{ClientKind, Command, Envelope, Query};

use crate::cli::AppRole;

/// Per-attempt connect bound for launch-time forwarding.
const FORWARD_CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
/// Total budget for launch-time forwarding retries.
const FORWARD_TOTAL_TIMEOUT: Duration = Duration::from_secs(2);
/// Maximum single response wait inside the forwarding budget.
const FORWARD_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);

pub fn canonicalize_path_str(raw: &str) -> PathBuf {
    std::fs::canonicalize(raw).unwrap_or_else(|_| PathBuf::from(raw))
}

pub fn spawn_viewer_window_child(path: &str, safe_mode: bool, lang: Option<&str>) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let viewer_bin = exe.parent().map(|dir| dir.join("photon-viewer"));
    let mut cmd = if let Some(bin) = viewer_bin.filter(|p| p.is_file()) {
        std::process::Command::new(bin)
    } else {
        let mut fallback = std::process::Command::new(exe);
        fallback.arg("--viewer-window");
        fallback
    };
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

pub fn main_instance_viewing_path(socket_path: &Path, canon: &Path) -> bool {
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
pub fn try_forward_to_running(socket_path: &Path, command: Command) -> bool {
    if !socket_path.exists() {
        return false;
    }
    forward_command_bounded(socket_path, command)
}

/// Lock/acquisition loser path: the owner may still be publishing its socket,
/// so retry within the launch budget even if the path is not visible yet.
pub fn forward_to_owner(socket_path: &Path, initial_cmd: &Option<Command>) -> bool {
    let forward_cmd = initial_cmd.clone().unwrap_or(Command::FocusWindow);
    if !forward_command_bounded(socket_path, forward_cmd) {
        return false;
    }
    if initial_cmd.is_some() {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchRoute {
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

pub fn compute_launch_route_for_role(
    role: AppRole,
    headless: bool,
    new_window: bool,
    viewer_window: bool,
    editor_flag: bool,
    is_sub_edit: bool,
    initial_cmd: Option<&Command>,
) -> LaunchRoute {
    match role {
        AppRole::Manager => {
            if !new_window && !headless {
                LaunchRoute::ForwardOrStartBrowser
            } else {
                LaunchRoute::DirectStart
            }
        }
        AppRole::Viewer => {
            if headless {
                LaunchRoute::DirectStart
            } else {
                LaunchRoute::StandaloneViewer
            }
        }
        AppRole::Editor => {
            if !new_window && !headless {
                LaunchRoute::StandaloneEditor
            } else {
                LaunchRoute::DirectStart
            }
        }
        AppRole::Unified => compute_launch_route(
            headless,
            new_window,
            viewer_window,
            editor_flag,
            is_sub_edit,
            initial_cmd,
        ),
    }
}

pub fn compute_launch_route(
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
    use fw_shell::acquire_launch_lock;

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

    #[test]
    fn app_roles_route_independently() {
        assert_eq!(
            compute_launch_route_for_role(
                AppRole::Manager,
                false,
                false,
                false,
                false,
                false,
                None
            ),
            LaunchRoute::ForwardOrStartBrowser
        );
        assert_eq!(
            compute_launch_route_for_role(AppRole::Viewer, false, false, false, false, false, None),
            LaunchRoute::StandaloneViewer
        );
        assert_eq!(
            compute_launch_route_for_role(AppRole::Editor, false, false, false, false, false, None),
            LaunchRoute::StandaloneEditor
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

        let _ = std::fs::remove_dir_all(&root);
    }
}
