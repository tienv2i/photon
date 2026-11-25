//! XDG & sandbox-compatible runtime paths.

use std::path::{Path, PathBuf};

use crate::identity::{AppIdentity, Profile};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub state_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub journal_dir: PathBuf,
    pub screenshot_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub socket_path: PathBuf,
}

impl AppPaths {
    pub fn for_identity(identity: &AppIdentity) -> Self {
        if let Profile::Sandbox(root) = &identity.profile {
            return Self::for_sandbox(root);
        }
        let dir_name = identity.profile.dir_name();
        let state_dir = state_home().join(dir_name);
        let cache_dir = cache_home().join(dir_name);
        let config_dir = config_home().join(dir_name);
        let data_dir = data_home().join(dir_name);
        let runtime_dir = runtime_home().join(dir_name);
        Self {
            log_dir: state_dir.join("logs"),
            journal_dir: state_dir.join("journal"),
            screenshot_dir: state_dir.join("screenshots"),
            socket_path: ipc_socket_path(&runtime_dir, dir_name),
            state_dir,
            cache_dir,
            config_dir,
            data_dir,
            runtime_dir,
        }
    }

    pub fn for_sandbox(root: &Path) -> Self {
        let state_dir = root.join("state");
        let cache_dir = root.join("cache");
        let config_dir = root.join("config");
        let data_dir = root.join("data");
        let runtime_dir = root.join("run");
        Self {
            log_dir: state_dir.join("logs"),
            journal_dir: state_dir.join("journal"),
            screenshot_dir: state_dir.join("screenshots"),
            socket_path: ipc_socket_path(&runtime_dir, "photon-sandbox"),
            state_dir,
            cache_dir,
            config_dir,
            data_dir,
            runtime_dir,
        }
    }

    pub fn current() -> Self {
        Self::for_identity(&AppIdentity::current())
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.json")
    }

    pub fn journal_file(&self) -> PathBuf {
        self.journal_dir.join("commands.jsonl")
    }

    pub fn ensure_runtime_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.log_dir)?;
        std::fs::create_dir_all(&self.journal_dir)?;
        std::fs::create_dir_all(&self.screenshot_dir)?;
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(&self.cache_dir)?;
        std::fs::create_dir_all(&self.runtime_dir)?;
        Ok(())
    }
}

fn ipc_socket_path(runtime_dir: &Path, dir_name: &str) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let _ = runtime_dir;
        PathBuf::from(format!(r"\\.\pipe\{dir_name}-control"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = dir_name;
        runtime_dir.join("control.sock")
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub fn home_dir() -> PathBuf {
    env_path("HOME")
        .or_else(|| env_path("USERPROFILE"))
        .unwrap_or_else(std::env::temp_dir)
}

#[cfg(target_os = "windows")]
fn state_home() -> PathBuf {
    env_path("LOCALAPPDATA").unwrap_or_else(home_dir)
}

#[cfg(not(target_os = "windows"))]
fn state_home() -> PathBuf {
    env_path("XDG_STATE_HOME").unwrap_or_else(|| home_dir().join(".local/state"))
}

#[cfg(target_os = "windows")]
fn cache_home() -> PathBuf {
    env_path("LOCALAPPDATA").unwrap_or_else(home_dir)
}

#[cfg(not(target_os = "windows"))]
fn cache_home() -> PathBuf {
    env_path("XDG_CACHE_HOME").unwrap_or_else(|| home_dir().join(".cache"))
}

#[cfg(target_os = "windows")]
fn config_home() -> PathBuf {
    env_path("APPDATA").unwrap_or_else(home_dir)
}

#[cfg(not(target_os = "windows"))]
fn config_home() -> PathBuf {
    env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home_dir().join(".config"))
}

#[cfg(target_os = "windows")]
fn data_home() -> PathBuf {
    env_path("LOCALAPPDATA").unwrap_or_else(home_dir)
}

#[cfg(not(target_os = "windows"))]
fn data_home() -> PathBuf {
    env_path("XDG_DATA_HOME").unwrap_or_else(|| home_dir().join(".local/share"))
}

#[cfg(target_os = "windows")]
fn runtime_home() -> PathBuf {
    std::env::temp_dir()
}

#[cfg(not(target_os = "windows"))]
fn runtime_home() -> PathBuf {
    env_path("XDG_RUNTIME_DIR").unwrap_or_else(std::env::temp_dir)
}
