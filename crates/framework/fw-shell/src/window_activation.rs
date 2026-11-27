//! Platform-specific window activation helpers (KDE Wayland KWin scripting).

/// Build the KWin script body that unminimizes and raises the window owned by
/// `pid`, falling back to a `resourceClass`/`resourceName` substring match.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn kwin_activation_script(pid: u32, fallback_class: &str) -> String {
    let escaped = fallback_class.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"const pid = {pid};
const windows = workspace.windowList ? workspace.windowList() : workspace.stackingOrder;
let target = null;
for (const w of windows) {{
    if (w.pid === pid) {{ target = w; break; }}
}}
if (!target) {{
    for (const w of windows) {{
        const cls = (w.resourceClass || "") + " " + (w.resourceName || "");
        if (cls.indexOf("{escaped}") >= 0) {{ target = w; break; }}
    }}
}}
if (target) {{
    if (target.minimized) {{ target.minimized = false; }}
    workspace.activeWindow = target;
}}
"#
    )
}

/// Ask the current platform to unminimize and raise the window owned by `pid`.
pub fn request_platform_activation(pid: u32, fallback_class: &'static str) {
    #[cfg(target_os = "linux")]
    {
        if is_kde_wayland() {
            std::thread::spawn(move || activate_via_kwin(pid, fallback_class));
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (pid, fallback_class);
    }
}

#[cfg(target_os = "linux")]
fn is_kde_wayland() -> bool {
    let wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|v| v.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
        || std::env::var_os("WAYLAND_DISPLAY").is_some();
    let kde = std::env::var("XDG_CURRENT_DESKTOP")
        .map(|v| v.to_ascii_lowercase().contains("kde"))
        .unwrap_or(false);
    wayland && kde
}

#[cfg(target_os = "linux")]
fn activate_via_kwin(pid: u32, fallback_class: &str) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let name = format!("fw-activate-{pid}-{stamp}");
    let path = std::env::temp_dir().join(format!("{name}.js"));
    if std::fs::write(&path, kwin_activation_script(pid, fallback_class)).is_err() {
        return;
    }
    let path_arg = path.to_string_lossy().into_owned();
    let loaded = run_qdbus(&[
        "org.kde.KWin",
        "/Scripting",
        "org.kde.kwin.Scripting.loadScript",
        &path_arg,
        &name,
    ]);
    if loaded {
        let _ = run_qdbus(&["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.start"]);
        std::thread::sleep(std::time::Duration::from_millis(150));
        let _ = run_qdbus(&[
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting.unloadScript",
            &name,
        ]);
    }
    let _ = std::fs::remove_file(&path);
}

#[cfg(target_os = "linux")]
fn run_qdbus(args: &[&str]) -> bool {
    for bin in ["qdbus6", "qdbus"] {
        let status = std::process::Command::new(bin)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if matches!(status, Ok(status) if status.success()) {
            return true;
        }
    }
    false
}
