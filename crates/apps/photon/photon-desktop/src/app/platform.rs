use super::*;

pub(super) fn xdg_open(target: &Path) {
    use std::process::{Command as ProcessCommand, Stdio};
    let _ = ProcessCommand::new("xdg-open")
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

pub(super) fn is_archive_path_str(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(photon_protocol::is_archive_extension)
}

pub(super) fn open_folder_in_file_manager(dir: &Path) {
    xdg_open(dir);
}

pub(super) fn write_png_bytes_to_clipboard(png_bytes: &[u8]) -> bool {
    use std::io::Write;
    use std::process::{Command as ProcessCommand, Stdio};

    let try_pipe = |program: &str, args: &[&str]| -> bool {
        let Ok(mut child) = ProcessCommand::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(png_bytes);
        }
        child.wait().is_ok_and(|s| s.success())
    };

    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && try_pipe("wl-copy", &["--type", "image/png"])
    {
        return true;
    }
    if try_pipe("wl-copy", &["--type", "image/png"])
        || try_pipe("xclip", &["-selection", "clipboard", "-t", "image/png"])
    {
        return true;
    }

    if let Ok(dyn_img) = image::load_from_memory(png_bytes) {
        let rgba = dyn_img.to_rgba8();
        let (w, h) = rgba.dimensions();
        if let Ok(mut cb) = arboard::Clipboard::new() {
            return cb
                .set_image(arboard::ImageData {
                    width: w as usize,
                    height: h as usize,
                    bytes: std::borrow::Cow::Owned(rgba.into_raw()),
                })
                .is_ok();
        }
    }
    false
}

pub(super) fn probe_png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

pub(super) fn read_clipboard_png_bytes() -> Option<Vec<u8>> {
    use image::ImageEncoder;
    use std::process::Command as ProcessCommand;

    let try_read = |program: &str, args: &[&str]| -> Option<Vec<u8>> {
        let out = ProcessCommand::new(program).args(args).output().ok()?;
        if out.status.success() && !out.stdout.is_empty() {
            Some(out.stdout)
        } else {
            None
        }
    };

    if let Ok(mut cb) = arboard::Clipboard::new()
        && let Ok(img) = cb.get_image()
    {
        let mut png_bytes = Vec::new();
        if image::codecs::png::PngEncoder::new(&mut png_bytes)
            .write_image(
                img.bytes.as_ref(),
                img.width as u32,
                img.height as u32,
                image::ExtendedColorType::Rgba8,
            )
            .is_ok()
            && !png_bytes.is_empty()
        {
            return Some(png_bytes);
        }
    }

    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && let Some(bytes) = try_read("wl-paste", &["--no-newline", "--type", "image/png"])
            .or_else(|| try_read("wl-paste", &["--type", "image/png"]))
    {
        return Some(bytes);
    }
    try_read(
        "xclip",
        &["-selection", "clipboard", "-t", "image/png", "-o"],
    )
}

pub(super) fn write_file_uris_to_clipboard(paths: &[String]) -> bool {
    use std::io::Write;
    use std::process::{Command as ProcessCommand, Stdio};

    let uris: String = paths.iter().map(|p| format!("file://{p}\n")).collect();
    let try_pipe = |program: &str, args: &[&str]| -> bool {
        let Ok(mut child) = ProcessCommand::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(uris.as_bytes());
        }
        child.wait().is_ok_and(|s| s.success())
    };
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && try_pipe("wl-copy", &["--type", "text/uri-list"])
    {
        return true;
    }
    if try_pipe("wl-copy", &["--type", "text/uri-list"])
        || try_pipe("xclip", &["-selection", "clipboard", "-t", "text/uri-list"])
    {
        return true;
    }
    if let Ok(mut cb) = arboard::Clipboard::new() {
        return cb.set_text(uris).is_ok();
    }
    false
}

pub(super) fn read_clipboard_uri_list() -> Vec<String> {
    use std::process::Command as ProcessCommand;

    let try_read = |program: &str, args: &[&str]| -> Option<String> {
        let out = ProcessCommand::new(program).args(args).output().ok()?;
        if out.status.success() {
            String::from_utf8(out.stdout).ok()
        } else {
            None
        }
    };

    let raw = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        try_read("wl-paste", &["--no-newline", "--type", "text/uri-list"]).or_else(|| {
            try_read(
                "xclip",
                &["-selection", "clipboard", "-t", "text/uri-list", "-o"],
            )
        })
    } else {
        try_read(
            "xclip",
            &["-selection", "clipboard", "-t", "text/uri-list", "-o"],
        )
        .or_else(|| try_read("wl-paste", &["--no-newline", "--type", "text/uri-list"]))
    };
    let Some(text) = raw else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            let path = trimmed.strip_prefix("file://").unwrap_or(trimmed);
            let p = Path::new(path);
            if p.exists() {
                Some(path.to_owned())
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn resolve_browser_clipboard_paste(
    in_app_files: Vec<String>,
) -> Option<BrowserClipboardPaste> {
    let os_uris = read_clipboard_uri_list();
    if !os_uris.is_empty() {
        return Some(BrowserClipboardPaste::Files(os_uris));
    }
    if let Some(png_bytes) = read_clipboard_png_bytes() {
        return Some(BrowserClipboardPaste::ImagePng(png_bytes));
    }
    if !in_app_files.is_empty() {
        return Some(BrowserClipboardPaste::Files(in_app_files));
    }
    None
}

pub(super) fn copy_image_png_to_clipboard(
    path: Option<&str>,
    cached_rgba: Option<(u32, u32, Arc<Vec<u8>>)>,
) -> bool {
    use image::ImageEncoder;

    let (w, h, raw): (u32, u32, Vec<u8>) = if let Some((w, h, arc)) = cached_rgba {
        (w, h, arc.as_ref().clone())
    } else if let Some(p) = path {
        let Ok(rgba) = photon_image::decode_rgba(Path::new(p)) else {
            return false;
        };
        let (w, h) = rgba.dimensions();
        (w, h, rgba.into_raw())
    } else {
        return false;
    };

    let mut png_bytes = Vec::new();
    if image::codecs::png::PngEncoder::new(&mut png_bytes)
        .write_image(&raw, w, h, image::ExtendedColorType::Rgba8)
        .is_err()
    {
        return false;
    }

    write_png_bytes_to_clipboard(&png_bytes)
}

pub(super) fn reveal_path_in_folder(path: &str) {
    let p = Path::new(path);
    let target_dir = p
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    xdg_open(target_dir);
}

pub(super) fn canonicalize_path_str(raw: &str) -> PathBuf {
    std::fs::canonicalize(raw).unwrap_or_else(|_| PathBuf::from(raw))
}

pub(super) fn spawn_sibling_or_self(
    sibling_bin: &str,
    sibling_args: &[&str],
    fallback_args: &[&str],
) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let sibling = exe.parent().map(|dir| dir.join(sibling_bin));
    let mut cmd = if let Some(bin) = sibling.filter(|p| p.is_file()) {
        let mut c = std::process::Command::new(bin);
        c.args(sibling_args);
        c
    } else {
        let mut c = std::process::Command::new(exe);
        c.args(fallback_args);
        c
    };
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let _ = cmd.spawn();
}

pub(super) fn browser_dir_for_state(state: &PhotonGui) -> Option<String> {
    state
        .snapshot
        .meta
        .viewer_archive
        .as_ref()
        .and_then(|arch| {
            Path::new(&arch.container_path)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
        })
        .or_else(|| {
            state.snapshot.meta.viewer_path.as_deref().and_then(|p| {
                Path::new(p)
                    .parent()
                    .map(|d| d.to_string_lossy().into_owned())
            })
        })
        .or_else(|| {
            state
                .snapshot
                .meta
                .editor
                .as_ref()
                .and_then(|d| {
                    if !d.save_path.is_empty() && Path::new(&d.save_path).is_file() {
                        Some(d.save_path.as_str())
                    } else if !d.path.is_empty() {
                        Some(d.path.as_str())
                    } else {
                        None
                    }
                })
                .and_then(|p| {
                    Path::new(p)
                        .parent()
                        .map(|d| d.to_string_lossy().into_owned())
                })
        })
        .or_else(|| {
            state
                .snapshot
                .meta
                .editor_unsupported
                .as_ref()
                .and_then(|u| {
                    Path::new(&u.path)
                        .parent()
                        .map(|d| d.to_string_lossy().into_owned())
                })
        })
        .or_else(|| state.snapshot.meta.current_folder.clone())
        .filter(|dir| !dir.is_empty())
}

pub(super) fn show_in_browser_or_forward(state: &PhotonGui, target_path: String) -> Task<Message> {
    let parent_dir = Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|d| !d.is_empty());
    if !is_standalone_viewer() && !is_standalone_editor() {
        let mut tasks = Vec::new();
        if let Some(dir) = parent_dir {
            tasks.push(dispatch_to_core(
                state.core.clone(),
                Command::OpenFolder { path: dir },
            ));
        }
        tasks.push(dispatch_to_core(
            state.core.clone(),
            Command::Select {
                paths: vec![target_path],
            },
        ));
        tasks.push(dispatch_to_core(
            state.core.clone(),
            Command::SetWorkspace {
                workspace: Workspace::Browser,
            },
        ));
        return Task::batch(tasks);
    }
    Task::perform(
        async move {
            let app_paths = photon_storage::AppPaths::current();
            if app_paths.socket_path.exists() {
                if let Some(dir) = parent_dir.as_ref() {
                    let open_env = Envelope::new(
                        format!("gui-{}", std::process::id()),
                        ClientKind::Gui,
                        Command::OpenFolder { path: dir.clone() },
                    );
                    let _ = photon_ipc::send_once(
                        &app_paths.socket_path,
                        &photon_ipc::IpcRequest::Dispatch { envelope: open_env },
                    )
                    .await;
                }
                let sel_env = Envelope::new(
                    format!("gui-{}", std::process::id()),
                    ClientKind::Gui,
                    Command::Select {
                        paths: vec![target_path],
                    },
                );
                let _ = photon_ipc::send_once(
                    &app_paths.socket_path,
                    &photon_ipc::IpcRequest::Dispatch { envelope: sel_env },
                )
                .await;
                let ws_env = Envelope::new(
                    format!("gui-{}", std::process::id()),
                    ClientKind::Gui,
                    Command::SetWorkspace {
                        workspace: Workspace::Browser,
                    },
                );
                if photon_ipc::send_once(
                    &app_paths.socket_path,
                    &photon_ipc::IpcRequest::Dispatch { envelope: ws_env },
                )
                .await
                .is_ok()
                {
                    let focus_env = Envelope::new(
                        format!("gui-{}", std::process::id()),
                        ClientKind::Gui,
                        Command::FocusWindow,
                    );
                    let _ = photon_ipc::send_once(
                        &app_paths.socket_path,
                        &photon_ipc::IpcRequest::Dispatch {
                            envelope: focus_env,
                        },
                    )
                    .await;
                    return;
                }
            }
            if let Some(dir) = parent_dir {
                spawn_sibling_or_self("photon-manager", &[&dir], &["browse", &dir]);
            } else {
                spawn_sibling_or_self("photon-manager", &[], &["browse"]);
            }
        },
        |()| Message::Noop,
    )
}

pub(super) fn forward_or_spawn_browser(browse_dir: Option<String>) -> Task<Message> {
    if !is_standalone_viewer() && !is_standalone_editor() {
        return Task::none();
    }
    Task::perform(
        async move {
            let app_paths = photon_storage::AppPaths::current();
            if app_paths.socket_path.exists() {
                if let Some(dir) = browse_dir.as_ref().filter(|dir| !dir.is_empty()) {
                    let open_env = Envelope::new(
                        format!("gui-{}", std::process::id()),
                        ClientKind::Gui,
                        Command::OpenFolder { path: dir.clone() },
                    );
                    let _ = photon_ipc::send_once(
                        &app_paths.socket_path,
                        &photon_ipc::IpcRequest::Dispatch { envelope: open_env },
                    )
                    .await;
                }
                let env = Envelope::new(
                    format!("gui-{}", std::process::id()),
                    ClientKind::Gui,
                    Command::SetWorkspace {
                        workspace: Workspace::Browser,
                    },
                );
                if photon_ipc::send_once(
                    &app_paths.socket_path,
                    &photon_ipc::IpcRequest::Dispatch { envelope: env },
                )
                .await
                .is_ok()
                {
                    let focus_env = Envelope::new(
                        format!("gui-{}", std::process::id()),
                        ClientKind::Gui,
                        Command::FocusWindow,
                    );
                    let _ = photon_ipc::send_once(
                        &app_paths.socket_path,
                        &photon_ipc::IpcRequest::Dispatch {
                            envelope: focus_env,
                        },
                    )
                    .await;
                    return;
                }
            }
            if let Some(dir) = browse_dir {
                spawn_sibling_or_self("photon-manager", &[&dir], &["browse", &dir]);
            } else {
                spawn_sibling_or_self("photon-manager", &[], &["browse"]);
            }
        },
        |()| Message::Noop,
    )
}

pub(super) fn open_editor_doc_in_viewer(state: &PhotonGui) -> Task<Message> {
    let target = state
        .snapshot
        .meta
        .editor
        .as_ref()
        .and_then(|d| {
            if !d.save_path.is_empty() && Path::new(&d.save_path).is_file() {
                Some(d.save_path.clone())
            } else if !d.path.is_empty() && Path::new(&d.path).is_file() {
                Some(d.path.clone())
            } else {
                None
            }
        })
        .or_else(|| {
            state
                .snapshot
                .meta
                .editor_unsupported
                .as_ref()
                .map(|u| u.path.clone())
        })
        .or_else(|| state.snapshot.meta.viewer_path.clone());
    if let Some(path) = target {
        open_or_focus_viewer_process(vec![path], state.snapshot.meta.viewer_single_instance)
    } else {
        Task::none()
    }
}

pub(super) fn forward_or_spawn_editor_only() -> Task<Message> {
    if is_standalone_editor() {
        return Task::none();
    }
    Task::perform(
        async move {
            let app_paths = photon_storage::AppPaths::current();
            let editor_sock = app_paths.editor_socket_path();
            if editor_sock.exists() {
                let focus_env = Envelope::new(
                    format!("gui-{}", std::process::id()),
                    ClientKind::Gui,
                    Command::FocusWindow,
                );
                if photon_ipc::send_once(
                    &editor_sock,
                    &photon_ipc::IpcRequest::Dispatch {
                        envelope: focus_env,
                    },
                )
                .await
                .is_ok()
                {
                    return;
                }
            }
            spawn_sibling_or_self("photon-editor", &[], &["edit"]);
        },
        |()| Message::Noop,
    )
}

pub(super) fn switch_workspace_from_standalone_viewer(
    state: &PhotonGui,
    target_ws: Workspace,
) -> Task<Message> {
    match target_ws {
        Workspace::Browser | Workspace::Viewer => {
            forward_or_spawn_browser(browser_dir_for_state(state))
        }
        Workspace::Editor => {
            if let Some(path) = state.snapshot.meta.viewer_path.clone() {
                open_in_editor_or_forward(state, path)
            } else {
                forward_or_spawn_editor_only()
            }
        }
    }
}

async fn main_instance_is_viewing_path(socket_path: &Path, canon: &Path) -> bool {
    if !socket_path.exists() {
        return false;
    }
    let Ok(photon_ipc::IpcResponse::Query { data }) = photon_ipc::send_once(
        socket_path,
        &photon_ipc::IpcRequest::Query {
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
}

pub(super) fn open_or_focus_viewer_process(
    paths: Vec<String>,
    single_instance: bool,
) -> Task<Message> {
    if paths.is_empty() {
        return Task::none();
    }
    Task::perform(
        async move {
            let app_paths = photon_storage::AppPaths::current();
            if single_instance {
                if let Some(live) = photon_ipc::find_any_live_viewer(&app_paths) {
                    let open_env = Envelope::new(
                        format!("gui-{}", std::process::id()),
                        ClientKind::Gui,
                        Command::OpenFiles { paths },
                    );
                    let _ = photon_ipc::send_once(
                        &live.socket_path,
                        &photon_ipc::IpcRequest::Dispatch { envelope: open_env },
                    )
                    .await;
                    let focus_env = Envelope::new(
                        format!("gui-{}", std::process::id()),
                        ClientKind::Gui,
                        Command::FocusWindow,
                    );
                    let _ = photon_ipc::send_once(
                        &live.socket_path,
                        &photon_ipc::IpcRequest::Dispatch {
                            envelope: focus_env,
                        },
                    )
                    .await;
                    return;
                }
                if let Some(first) = paths.into_iter().next() {
                    spawn_sibling_or_self("photon-viewer", &[&first], &["--viewer-window", &first]);
                }
                return;
            }
            for raw in paths {
                let canon = canonicalize_path_str(&raw);
                let _lock = photon_ipc::acquire_viewer_path_lock(
                    &app_paths,
                    &canon,
                    Duration::from_secs(2),
                );
                if let Some(matched) = photon_ipc::find_live_viewer_for_path(&app_paths, &canon) {
                    let env = Envelope::new(
                        format!("gui-{}", std::process::id()),
                        ClientKind::Gui,
                        Command::FocusWindow,
                    );
                    let _ = photon_ipc::send_once(
                        &matched.info.socket_path,
                        &photon_ipc::IpcRequest::Dispatch { envelope: env },
                    )
                    .await;
                    continue;
                }
                if main_instance_is_viewing_path(&app_paths.socket_path, &canon).await {
                    let env = Envelope::new(
                        format!("gui-{}", std::process::id()),
                        ClientKind::Gui,
                        Command::FocusWindow,
                    );
                    let _ = photon_ipc::send_once(
                        &app_paths.socket_path,
                        &photon_ipc::IpcRequest::Dispatch { envelope: env },
                    )
                    .await;
                    continue;
                }
                spawn_sibling_or_self("photon-viewer", &[&raw], &["--viewer-window", &raw]);
                // Hold the per-path lock briefly until the child registers so rapid double-clicks
                // deduplicate against the new process instead of racing a duplicate window.
                let deadline = Instant::now() + Duration::from_millis(500);
                while Instant::now() < deadline {
                    if photon_ipc::find_live_viewer_for_path(&app_paths, &canon).is_some() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
        },
        |_| Message::Noop,
    )
}

pub(super) fn open_in_editor_or_forward(state: &PhotonGui, path: String) -> Task<Message> {
    if is_standalone_editor() {
        return dispatch_to_core(
            state.core.clone(),
            Command::OpenInEditorOrUnsupported { path },
        );
    }
    Task::perform(
        async move {
            let app_paths = photon_storage::AppPaths::current();
            let editor_sock = app_paths.editor_socket_path();
            let env = Envelope::new(
                format!("gui-{}", std::process::id()),
                ClientKind::Gui,
                Command::OpenInEditorOrUnsupported { path: path.clone() },
            );
            if editor_sock.exists()
                && photon_ipc::send_once(
                    &editor_sock,
                    &photon_ipc::IpcRequest::Dispatch { envelope: env },
                )
                .await
                .is_ok()
            {
                let focus_env = Envelope::new(
                    format!("gui-{}", std::process::id()),
                    ClientKind::Gui,
                    Command::FocusWindow,
                );
                let _ = photon_ipc::send_once(
                    &editor_sock,
                    &photon_ipc::IpcRequest::Dispatch {
                        envelope: focus_env,
                    },
                )
                .await;
            } else {
                spawn_sibling_or_self("photon-editor", &[&path], &["edit", &path]);
            }
        },
        |_| Message::Noop,
    )
}
