//! Photon XDG & sandbox-compatible runtime paths built atop `fw-storage`.

use std::ops::Deref;
use std::path::{Path, PathBuf};

pub use fw_storage::paths::home_dir;

use crate::identity::{AppIdentity, PHOTON_BRANDING, current_identity};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths(pub fw_storage::AppPaths);

impl Deref for AppPaths {
    type Target = fw_storage::AppPaths;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<fw_storage::AppPaths> for AppPaths {
    fn as_ref(&self) -> &fw_storage::AppPaths {
        &self.0
    }
}

impl AppPaths {
    pub fn for_identity(identity: &AppIdentity) -> Self {
        Self(fw_storage::AppPaths::for_identity(identity))
    }

    pub fn for_sandbox(root: &Path) -> Self {
        Self(fw_storage::AppPaths::for_sandbox_named(
            root,
            PHOTON_BRANDING.sandbox_dir_name,
        ))
    }

    pub fn current() -> Self {
        Self::for_identity(&current_identity())
    }

    pub fn ai_ipc_dir(&self) -> PathBuf {
        self.0.runtime_dir.join("ai-ipc")
    }

    pub fn viewers_dir(&self) -> PathBuf {
        self.0.runtime_dir.join("viewers")
    }

    pub fn editor_socket_path(&self) -> PathBuf {
        self.0.runtime_dir.join("editor.sock")
    }

    pub fn ensure_viewers_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.viewers_dir();
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewers_dir_resides_under_runtime_dir() {
        let paths = AppPaths::for_sandbox(Path::new("/tmp/photon-test-sandbox"));
        assert_eq!(paths.viewers_dir(), paths.runtime_dir.join("viewers"));
        assert_eq!(
            paths.editor_socket_path(),
            paths.runtime_dir.join("editor.sock")
        );
    }
}
