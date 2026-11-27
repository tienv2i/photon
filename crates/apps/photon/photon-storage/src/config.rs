//! Atomic user configuration with corruption recovery and safe-mode defaults.

use std::path::{Path, PathBuf};

use photon_protocol::{
    AiSettings, AppLaunchWindowMode, BackupMethod, Language, LayoutState, Theme, TranslateSettings,
};
use serde::{Deserialize, Serialize};

use crate::store::{backup_existing_file, load_json_or_recover, write_json_atomically};

/// Current on-disk `Config` schema version.
///
/// Version `0` means the file predates this marker (legacy config). Bump this
/// whenever a migration must run exactly once per config file.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

fn default_true() -> bool {
    true
}

fn is_legacy_schema_version(version: &u32) -> bool {
    *version == 0
}

/// The pre-`Maximized` default layout was `remembered` with a maximized window.
/// This isn't a perfect oracle (a user could have deliberately picked
/// `remembered` while the window happened to be maximized), but it is the only
/// signal available in legacy files and matches the old factory default.
fn is_legacy_remembered_default(layout: &LayoutState) -> bool {
    layout.app_launch_window_mode == AppLaunchWindowMode::Remembered && layout.window_maximized
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// On-disk schema version. Legacy files omit it (deserialized as `0`).
    #[serde(default, skip_serializing_if = "is_legacy_schema_version")]
    pub schema_version: u32,
    pub theme: Theme,
    pub language: Language,
    pub last_folder: Option<String>,
    pub layout: LayoutState,
    pub remember_last_folder: bool,
    #[serde(default = "default_true")]
    pub save_ui_state_on_exit: bool,
    pub hardware_acceleration: bool,
    #[serde(default)]
    pub backup_method: BackupMethod,
    pub favorites: Vec<String>,
    /// Set to `true` after the first-launch default Quick Access items have been
    /// seeded into `favorites`.  Prevents them from reappearing after the user
    /// deliberately clears all favourites.
    #[serde(default)]
    pub favorites_initialized: bool,
    pub translate: TranslateSettings,
    #[serde(default)]
    pub disabled_plugins: Vec<String>,
    #[serde(default)]
    pub disabled_quick_action_plugins: Vec<String>,
    #[serde(default)]
    pub viewer_single_instance: bool,
    #[serde(default)]
    pub ai: AiSettings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            theme: Theme::Dark,
            language: Language::System,
            last_folder: None,
            layout: LayoutState::default(),
            remember_last_folder: true,
            save_ui_state_on_exit: true,
            hardware_acceleration: true,
            backup_method: BackupMethod::default(),
            favorites: Vec::new(),
            favorites_initialized: false,
            translate: TranslateSettings::default(),
            disabled_plugins: Vec::new(),
            disabled_quick_action_plugins: Vec::new(),
            viewer_single_instance: false,
            ai: AiSettings::default(),
        }
    }
}

impl Config {
    pub fn factory_default() -> Self {
        Self::default().sanitized()
    }

    pub fn sanitized(mut self) -> Self {
        self.layout = self.layout.sanitized();
        self.ai = self.ai.clone().sanitized();
        self
    }

    pub fn backup_existing(path: &Path) -> Option<PathBuf> {
        backup_existing_file(path)
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

    pub fn reset_settings_only(path: &Path) -> (Self, Option<String>) {
        let backup = Self::backup_existing(path);
        let (existing, _) = Self::load_or_create(path);
        let mut cfg = Self::factory_default();
        cfg.last_folder = existing.last_folder;
        let mut preserved_layout = existing.layout;
        preserved_layout.apply_settings_from(&cfg.layout);
        cfg.layout = preserved_layout;
        let err = cfg.save(path).err();
        let note = match (backup, err) {
            (Some(bak), None) => Some(format!("settings reset (backup at {})", bak.display())),
            (None, None) => Some("settings reset to defaults".to_owned()),
            (_, Some(e)) => Some(format!("settings reset in memory, save failed: {e}")),
        };
        (cfg, note)
    }

    /// Applies one-time migrations for configs written before `schema_version`
    /// existed. Returns `true` when the config was changed and should be
    /// persisted.
    ///
    /// Migration rule (legacy files only):
    /// - If `layout.app_launch_window_mode == Remembered` **and**
    ///   `layout.window_maximized`, the layout matches the old factory default
    ///   (before `Maximized` became the launch default), so switch it to
    ///   `Maximized`.
    /// - Any other legacy value (including an explicit `remembered` with a
    ///   non-maximized window) is left untouched.
    /// - The version is always stamped to `CONFIG_SCHEMA_VERSION`, so the
    ///   migration runs exactly once even when no layout change was needed.
    fn migrate_legacy_schema(&mut self) -> bool {
        if self.schema_version >= CONFIG_SCHEMA_VERSION {
            return false;
        }
        if is_legacy_remembered_default(&self.layout) {
            self.layout.app_launch_window_mode = AppLaunchWindowMode::Maximized;
        }
        self.schema_version = CONFIG_SCHEMA_VERSION;
        true
    }

    pub fn load_or_create(path: &Path) -> (Self, Option<String>) {
        load_json_or_recover(
            path,
            "config",
            |mut cfg: Self| {
                let migrated = cfg.migrate_legacy_schema();
                (cfg.sanitized(), migrated)
            },
            Self::factory_default,
        )
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_json_atomically(self, path)
    }
}

/// Persisted window, folder, panel, and view UI state stored separately from `config.json`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UiState {
    pub last_folder: Option<String>,
    pub layout: LayoutState,
}

impl UiState {
    pub fn sanitized(mut self) -> Self {
        self.layout = self.layout.sanitized();
        self
    }

    /// `ui_state.json` has no schema version of its own. A UI-state layout that
    /// still carries the old `remembered` + maximized default is stale unless
    /// the current config explicitly asks for `remembered` (a deliberate user
    /// choice). In that case migrate it to `Maximized` so it cannot overwrite
    /// the config migration on the next launch.
    fn migrate_legacy_launch_mode(&mut self, config: &Config) -> bool {
        let config_explicitly_remembers = config.schema_version >= CONFIG_SCHEMA_VERSION
            && config.layout.app_launch_window_mode == AppLaunchWindowMode::Remembered;
        if !config_explicitly_remembers && is_legacy_remembered_default(&self.layout) {
            self.layout.app_launch_window_mode = AppLaunchWindowMode::Maximized;
            true
        } else {
            false
        }
    }

    pub fn load_or_migrate(path: &Path, legacy_config: &Config) -> (Self, Option<String>) {
        load_json_or_recover(
            path,
            "ui_state",
            |state: Self| {
                let mut state = state.sanitized();
                let migrated = state.migrate_legacy_launch_mode(legacy_config);
                (state, migrated)
            },
            || {
                let mut migrated = Self {
                    last_folder: legacy_config.last_folder.clone(),
                    layout: legacy_config.layout.clone(),
                }
                .sanitized();
                migrated.migrate_legacy_launch_mode(legacy_config);
                migrated
            },
        )
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_json_atomically(self, path)
    }

    pub fn reset_to_default(path: &Path) -> (Self, Option<String>) {
        let backup = Config::backup_existing(path);
        let state = Self::default().sanitized();
        let err = state.save(path).err();
        let note = match (backup, err) {
            (Some(bak), None) => Some(format!(
                "ui_state reset to defaults (backup at {})",
                bak.display()
            )),
            (None, None) => Some("ui_state reset to defaults".to_owned()),
            (_, Some(e)) => Some(format!("ui_state reset in memory, save failed: {e}")),
        };
        (state, note)
    }

    pub fn reset_layout_only(ui_path: &Path, config_path: &Path) -> (Self, Option<String>) {
        let backup = Config::backup_existing(ui_path);
        let (cfg, _) = Config::load_or_create(config_path);
        let mut state = Self::default();
        state.layout.apply_settings_from(&cfg.layout);
        state = state.sanitized();
        let err = state.save(ui_path).err();
        let note = match (backup, err) {
            (Some(bak), None) => Some(format!(
                "ui_state layout reset (backup at {})",
                bak.display()
            )),
            (None, None) => Some("ui_state layout reset to defaults".to_owned()),
            (_, Some(e)) => Some(format!("ui_state layout reset in memory, save failed: {e}")),
        };
        (state, note)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
        assert!(loaded.disabled_plugins.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn disabled_plugins_persists_roundtrip() {
        let dir = std::env::temp_dir().join(format!("photon-cfg-plugins-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        let cfg = Config {
            disabled_plugins: vec!["com.example.one".to_owned()],
            ..Default::default()
        };
        cfg.save(&path).unwrap();

        let (loaded, warn) = Config::load_or_create(&path);
        assert!(warn.is_none());
        assert_eq!(loaded.disabled_plugins, vec!["com.example.one"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_config_migrates_remembered_launch_mode_to_maximized() {
        let dir = std::env::temp_dir().join(format!("photon-cfg-migrate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        // No `schema_version`: this is a legacy file with the old default state.
        fs::write(
            &path,
            r#"{
                "theme": "dark",
                "layout": {
                    "windowWidth": 1280,
                    "windowHeight": 800,
                    "appLaunchWindowMode": "remembered",
                    "windowMaximized": true
                }
            }"#,
        )
        .unwrap();

        let (loaded, warn) = Config::load_or_create(&path);
        assert!(warn.is_none());
        assert_eq!(loaded.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(
            loaded.layout.app_launch_window_mode,
            AppLaunchWindowMode::Maximized
        );

        // The migration is persisted, including the version marker.
        let on_disk: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(on_disk["schema_version"], CONFIG_SCHEMA_VERSION);
        assert_eq!(
            on_disk["layout"]["appLaunchWindowMode"],
            serde_json::json!("maximized")
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn current_version_remembered_launch_mode_is_preserved() {
        let dir = std::env::temp_dir().join(format!("photon-cfg-version-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(
            &path,
            format!(
                r#"{{
                    "schema_version": {CONFIG_SCHEMA_VERSION},
                    "layout": {{
                        "appLaunchWindowMode": "remembered",
                        "windowMaximized": true
                    }}
                }}"#
            ),
        )
        .unwrap();

        let (loaded, warn) = Config::load_or_create(&path);
        assert!(warn.is_none());
        assert_eq!(loaded.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(
            loaded.layout.app_launch_window_mode,
            AppLaunchWindowMode::Remembered
        );

        let on_disk: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            on_disk["layout"]["appLaunchWindowMode"],
            serde_json::json!("remembered")
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_launch_mode_defaults_to_maximized() {
        let dir =
            std::env::temp_dir().join(format!("photon-cfg-missing-mode-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(
            &path,
            r#"{
                "layout": {
                    "windowWidth": 1280,
                    "windowHeight": 800
                }
            }"#,
        )
        .unwrap();

        let (loaded, warn) = Config::load_or_create(&path);
        assert!(warn.is_none());
        assert_eq!(
            loaded.layout.app_launch_window_mode,
            AppLaunchWindowMode::Maximized
        );
        assert_eq!(loaded.schema_version, CONFIG_SCHEMA_VERSION);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ui_state_legacy_remembered_launch_mode_is_migrated() {
        let dir = std::env::temp_dir().join(format!("fw-ui-mode-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let ui_path = dir.join("ui_state.json");
        fs::write(
            &ui_path,
            r#"{
                "layout": {
                    "appLaunchWindowMode": "remembered",
                    "windowMaximized": true
                }
            }"#,
        )
        .unwrap();

        // Config has already been migrated to the new Maximized default.
        let config = Config::default();
        assert_eq!(
            config.layout.app_launch_window_mode,
            AppLaunchWindowMode::Maximized
        );

        let (state, warn) = UiState::load_or_migrate(&ui_path, &config);
        assert!(warn.is_none());
        assert_eq!(
            state.layout.app_launch_window_mode,
            AppLaunchWindowMode::Maximized
        );

        let on_disk: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&ui_path).unwrap()).unwrap();
        assert_eq!(
            on_disk["layout"]["appLaunchWindowMode"],
            serde_json::json!("maximized")
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ui_state_keeps_remembered_when_config_explicitly_chooses_it() {
        let dir = std::env::temp_dir().join(format!("fw-ui-mode-keep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let ui_path = dir.join("ui_state.json");
        fs::write(
            &ui_path,
            r#"{
                "layout": {
                    "appLaunchWindowMode": "remembered",
                    "windowMaximized": true
                }
            }"#,
        )
        .unwrap();

        let config = Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            layout: LayoutState {
                app_launch_window_mode: AppLaunchWindowMode::Remembered,
                window_maximized: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let (state, warn) = UiState::load_or_migrate(&ui_path, &config);
        assert!(warn.is_none());
        assert_eq!(
            state.layout.app_launch_window_mode,
            AppLaunchWindowMode::Remembered
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ui_state_migrates_from_legacy_config_and_saves_independently() {
        let dir = std::env::temp_dir().join(format!("fw-ui-state-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let ui_path = dir.join("ui_state.json");

        let legacy = Config {
            last_folder: Some("/tmp/legacy".to_owned()),
            layout: LayoutState {
                window_width: 1600,
                window_height: 900,
                ..Default::default()
            },
            ..Default::default()
        };

        let (migrated, warn) = UiState::load_or_migrate(&ui_path, &legacy);
        assert!(warn.is_none());
        assert!(ui_path.is_file());
        assert_eq!(migrated.last_folder.as_deref(), Some("/tmp/legacy"));
        assert_eq!(migrated.layout.window_width, 1600);

        let mut updated = migrated;
        updated.layout.window_width = 1440;
        updated.layout.viewer_viewport_background = Some([32, 34, 38]);
        updated.save(&ui_path).unwrap();

        let (reloaded, warn2) = UiState::load_or_migrate(&ui_path, &legacy);
        assert!(warn2.is_none());
        assert_eq!(reloaded.layout.window_width, 1440);
        assert_eq!(
            reloaded.layout.viewer_viewport_background,
            Some([32, 34, 38])
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
