//! Platform-specific window activation helpers.
//!
//! `iced`/`winit` cannot raise or unminimize windows on Wayland: `gain_focus`
//! and `minimize` are effectively no-ops there. KDE Plasma (KWin) exposes a
//! scripting D-Bus API, so on KDE Wayland we drive it out-of-process from a
//! detached thread (never blocking the UI thread) to unminimize and focus the
//! Photon window.

/// Build the KWin script body that unminimizes and raises the Photon window
/// owned by `pid`, falling back to a resource-class match when the process id
/// cannot be found.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn kwin_activation_script(pid: u32) -> String {
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
        if (cls.indexOf("Photon") >= 0) {{ target = w; break; }}
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
///
/// No-op off Linux, and on Linux only when running under KDE Wayland (where
/// winit cannot do it itself). All failures are ignored.
pub fn request_platform_activation(pid: u32) {
    #[cfg(target_os = "linux")]
    {
        if is_kde_wayland() {
            std::thread::spawn(move || activate_via_kwin(pid));
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
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
fn activate_via_kwin(pid: u32) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let name = format!("photon-activate-{pid}-{stamp}");
    let path = std::env::temp_dir().join(format!("{name}.js"));
    if std::fs::write(&path, kwin_activation_script(pid)).is_err() {
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

/// Invoke `qdbus6`, falling back to the older `qdbus` binary.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_targets_pid_and_raises() {
        let script = kwin_activation_script(4242);
        assert!(script.contains("4242"));
        assert!(script.contains("w.pid"));
        assert!(script.contains("workspace.activeWindow"));
        assert!(script.contains("minimized"));
    }

    #[test]
    fn script_falls_back_to_resource_class() {
        let script = kwin_activation_script(7);
        assert!(script.contains("resourceClass"));
        assert!(script.contains("Photon"));
    }
}
