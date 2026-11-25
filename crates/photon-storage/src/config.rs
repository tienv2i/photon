//! Atomic user configuration with corruption recovery and safe-mode defaults.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use photon_protocol::{Language, LayoutState, Theme, TranslateSettings};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: Theme,
    pub language: Language,
    pub last_folder: Option<String>,
    pub layout: LayoutState,
    pub remember_last_folder: bool,
    pub hardware_acceleration: bool,
    pub favorites: Vec<String>,
    /// Set to `true` after the first-launch default Quick Access items have been
    /// seeded into `favorites`.  Prevents them from reappearing after the user
    /// deliberately clears all favourites.
    #[serde(default)]
    pub favorites_initialized: bool,
    pub translate: TranslateSettings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            language: Language::System,
            last_folder: None,
            layout: LayoutState::default(),
            remember_last_folder: true,
            hardware_acceleration: true,
            favorites: Vec::new(),
            favorites_initialized: false,
            translate: TranslateSettings::default(),
        }
    }
}

impl Config {
    pub fn factory_default() -> Self {
        Self::default().sanitized()
    }

    pub fn sanitized(mut self) -> Self {
        self.layout = self.layout.sanitized();
        self
    }

    pub fn backup_existing(path: &Path) -> Option<PathBuf> {
        if !path.is_file() {
            return None;
        }
        let backup = path.with_extension("json.bak");
        fs::copy(path, &backup).ok().map(|_| backup)
    }

    pub fn reset_to_factory(path: &Path) -> (Self, Option<String>) {
        let backup = Self::backup_existing(path);
        let cfg = Self::factory_default();
        let err = cfg.save(path).err();
        let note = match (backup, err) {
            (Some(bak), None) => Some(format!(
                "configuration reset to factory defaults (backup at {})",
                bak.display()
            )),
            (None, None) => Some("configuration reset to factory defaults".to_owned()),
            (_, Some(e)) => Some(format!("configuration reset in memory, save failed: {e}")),
        };
        (cfg, note)
    }

    pub fn reset_layout_only(path: &Path) -> (Self, Option<String>) {
        let backup = Self::backup_existing(path);
        let (mut cfg, _) = Self::load_or_create(path);
        cfg.layout = LayoutState::default();
        let err = cfg.save(path).err();
        let note = match (backup, err) {
            (Some(bak), None) => Some(format!("layout reset (backup at {})", bak.display())),
            (None, None) => Some("layout reset to defaults".to_owned()),
            (_, Some(e)) => Some(format!("layout reset in memory, save failed: {e}")),
        };
        (cfg, note)
    }

    pub fn load_or_create(path: &Path) -> (Self, Option<String>) {
        match fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str::<Self>(&text) {
                Ok(cfg) => (cfg.sanitized(), None),
                Err(error) => {
                    let backup = path.with_extension("json.corrupt");
                    let _ = fs::copy(path, &backup);
                    let cfg = Self::factory_default();
                    let _ = cfg.save(path);
                    (
                        cfg,
                        Some(format!(
                            "config corrupt ({error}); defaults restored, backup kept at {}",
                            backup.display()
                        )),
                    )
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let cfg = Self::factory_default();
                let warn = cfg
                    .save(path)
                    .err()
                    .map(|e| format!("cannot create initial config: {e}"));
                (cfg, warn)
            }
            Err(err) => (
                Self::factory_default(),
                Some(format!("cannot read config {}: {err}", path.display())),
            ),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        let bytes =
            serde_json::to_vec_pretty(self).map_err(|e| std::io::Error::other(e.to_string()))?;
        {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(&bytes)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
        }
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_config_is_backed_up_and_repaired() {
        let dir = std::env::temp_dir().join(format!("photon-cfg-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, "{broken").unwrap();

        let (cfg, warn) = Config::load_or_create(&path);
        assert!(warn.is_some());
        assert_eq!(cfg, Config::factory_default());
        assert!(path.with_extension("json.corrupt").is_file());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn persists_session_fields_and_sanitizes_bounds() {
        let dir = std::env::temp_dir().join(format!("photon-cfg-session-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        let cfg = Config {
            last_folder: Some("/tmp/photos".to_owned()),
            layout: LayoutState {
                window_width: 99_999,
                window_height: 100,
                browser_view_mode: photon_protocol::BrowserViewMode::List,
                ..Default::default()
            },
            ..Default::default()
        };
        cfg.save(&path).unwrap();

        let (loaded, warn) = Config::load_or_create(&path);
        assert!(warn.is_none());
        assert_eq!(loaded.last_folder.as_deref(), Some("/tmp/photos"));
        assert_eq!(loaded.layout.window_width, 3840);
        assert_eq!(loaded.layout.window_height, 500);
        assert_eq!(
            loaded.layout.browser_view_mode,
            photon_protocol::BrowserViewMode::List
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_config_with_removed_viewer_fields_deserializes_cleanly() {
        let dir = std::env::temp_dir().join(format!("photon-cfg-legacy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(
            &path,
            r#"{
                "theme": "light",
                "last_folder": "/tmp/photos",
                "last_workspace": "viewer",
                "last_viewer_path": "/tmp/photos/a.png",
                "remember_last_folder": true
            }"#,
        )
        .unwrap();

        let (loaded, warn) = Config::load_or_create(&path);
        assert!(warn.is_none());
        assert_eq!(loaded.theme, Theme::Light);
        assert_eq!(loaded.last_folder.as_deref(), Some("/tmp/photos"));
        assert!(loaded.remember_last_folder);

        let _ = fs::remove_dir_all(&dir);
    }
}
