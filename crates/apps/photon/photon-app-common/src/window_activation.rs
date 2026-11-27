//! Photon window activation helpers built atop `fw-shell`.

#[cfg(test)]
pub fn kwin_activation_script(pid: u32) -> String {
    fw_shell::kwin_activation_script(pid, "Photon")
}

pub fn request_platform_activation(pid: u32) {
    fw_shell::request_platform_activation(pid, "Photon");
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
