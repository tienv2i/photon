//! Plugin discovery, installation, uninstallation, and `PluginInfo` projection.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use photon_protocol::{AiModelStatus, AiQuickAction, AiSettings, PluginInfo, PluginModelSpec};

use crate::manifest::{LoadedPlugin, load_plugin_from_dir, validate_plugin_id};
use crate::models;
use crate::{PLUGINS_DIR_ENV, PluginError};

static INSTALL_SEQ: AtomicU64 = AtomicU64::new(1);

/// Non-fatal error recorded when an invalid plugin folder is skipped during discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDiscoveryError {
    pub dir: PathBuf,
    pub message: String,
}

/// Resolve the active plugins directory, honoring `PHOTON_PLUGINS_DIR` when non-empty.
pub fn resolve_plugins_dir(default_plugins_dir: impl AsRef<Path>) -> PathBuf {
    match std::env::var_os(PLUGINS_DIR_ENV) {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => default_plugins_dir.as_ref().to_path_buf(),
    }
}

/// Discovered plugins under a plugins root directory.
#[derive(Debug, Clone, Default)]
pub struct PluginRegistry {
    plugins_dir: PathBuf,
    plugins: BTreeMap<String, LoadedPlugin>,
    errors: Vec<PluginDiscoveryError>,
}

impl PluginRegistry {
    /// Discover `<plugins_dir>/*/plugin.json` using an explicit plugins directory.
    ///
    /// Missing `plugins_dir` yields an empty registry. Invalid or unsafe plugin folders are
    /// rejected and recorded in [`Self::errors`] without aborting discovery of valid plugins.
    pub fn discover(plugins_dir: impl AsRef<Path>) -> Result<Self, PluginError> {
        let mut registry = Self {
            plugins_dir: plugins_dir.as_ref().to_path_buf(),
            plugins: BTreeMap::new(),
            errors: Vec::new(),
        };
        registry.refresh()?;
        Ok(registry)
    }

    /// Discover plugins from `PHOTON_PLUGINS_DIR` if set, otherwise `default_plugins_dir`.
    pub fn discover_or_env(default_plugins_dir: impl AsRef<Path>) -> Result<Self, PluginError> {
        Self::discover(resolve_plugins_dir(default_plugins_dir))
    }

    /// Root directory scanned by this registry.
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    /// Re-scan [`Self::plugins_dir`] on disk.
    pub fn refresh(&mut self) -> Result<(), PluginError> {
        self.plugins.clear();
        self.errors.clear();

        if !self.plugins_dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&self.plugins_dir).map_err(|source| PluginError::Io {
            context: format!("read plugins directory {}", self.plugins_dir.display()),
            source,
        })?;

        for entry_res in entries {
            let entry = entry_res.map_err(|source| PluginError::Io {
                context: format!("iterate plugins directory {}", self.plugins_dir.display()),
                source,
            })?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if !meta.is_dir() {
                continue;
            }
            if !path.join("plugin.json").exists() && !path.join("manifest.json").exists() {
                continue;
            }
            match load_plugin_from_dir(&path) {
                Ok(loaded) => {
                    if self.plugins.contains_key(&loaded.manifest.id) {
                        self.errors.push(PluginDiscoveryError {
                            dir: path,
                            message: format!("duplicate plugin id '{}'", loaded.manifest.id),
                        });
                    } else {
                        self.plugins.insert(loaded.manifest.id.clone(), loaded);
                    }
                }
                Err(err) => {
                    self.errors.push(PluginDiscoveryError {
                        dir: path,
                        message: err.to_string(),
                    });
                }
            }
        }

        Ok(())
    }

    /// Look up a valid discovered plugin by ID.
    pub fn get(&self, id: &str) -> Option<&LoadedPlugin> {
        self.plugins.get(id)
    }

    /// Iterate over all valid discovered plugins ordered by ID.
    pub fn plugins(&self) -> impl Iterator<Item = &LoadedPlugin> {
        self.plugins.values()
    }

    /// Non-fatal errors encountered while scanning invalid plugin directories.
    pub fn errors(&self) -> &[PluginDiscoveryError] {
        &self.errors
    }

    /// Build `PluginInfo` summaries with `enabled` computed from `disabled_plugins`.
    pub fn plugin_infos(&self, disabled_plugins: &[String]) -> Vec<PluginInfo> {
        self.plugins
            .values()
            .map(|plugin| plugin.to_info(disabled_plugins))
            .collect()
    }

    /// Collect all Quick Actions declared by enabled discovered plugins.
    pub fn quick_actions(&self, disabled_plugins: &[String]) -> Vec<AiQuickAction> {
        let mut out = Vec::new();
        for plugin in self.plugins.values() {
            if disabled_plugins.iter().any(|id| id == &plugin.manifest.id) {
                continue;
            }
            let available = plugin.entry_path.is_file();
            for qa in plugin.quick_actions() {
                out.push(AiQuickAction {
                    plugin_id: plugin.manifest.id.clone(),
                    id: qa.id.clone(),
                    label: qa.label.clone(),
                    action: qa.action.clone(),
                    workspaces: qa.workspaces.clone(),
                    params: qa.params.clone(),
                    available,
                });
            }
        }
        out
    }

    /// Find an enabled plugin supporting `action` (capability or quick action).
    pub fn find_by_action(
        &self,
        action: &str,
        disabled_plugins: &[String],
    ) -> Option<&LoadedPlugin> {
        self.plugins.values().find(|p| {
            !disabled_plugins.iter().any(|id| id == &p.manifest.id)
                && p.entry_path.is_file()
                && p.supports_action(action)
        })
    }

    /// Collect all cataloged models from built-in specs and discovered plugins.
    pub fn catalog_models(&self) -> Vec<PluginModelSpec> {
        models::catalog_models(self.plugins.values())
    }

    /// Resolve model statuses against `models_dir` and `AiSettings`.
    pub fn model_statuses(&self, models_dir: &Path, settings: &AiSettings) -> Vec<AiModelStatus> {
        models::model_statuses(models_dir, settings, self.plugins.values())
    }

    /// Validate and install a plugin from `source_dir` into this registry's directory and refresh.
    pub fn install(&mut self, source_dir: impl AsRef<Path>) -> Result<LoadedPlugin, PluginError> {
        let installed = install_from_dir(source_dir, &self.plugins_dir)?;
        self.refresh()?;
        Ok(installed)
    }

    /// Uninstall a plugin by `id` from this registry's directory and refresh.
    pub fn uninstall(&mut self, id: &str) -> Result<(), PluginError> {
        uninstall(id, &self.plugins_dir)?;
        self.refresh()?;
        Ok(())
    }
}

struct StagingCleanup {
    path: PathBuf,
    armed: bool,
}

impl StagingCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for StagingCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// Validate `source_dir/plugin.json` and safely copy the plugin into `<plugins_root>/<id>`.
pub fn install_from_dir(
    source_dir: impl AsRef<Path>,
    plugins_root: impl AsRef<Path>,
) -> Result<LoadedPlugin, PluginError> {
    let source_dir = source_dir.as_ref();
    let plugins_root = plugins_root.as_ref();

    if !source_dir.is_dir() {
        return Err(PluginError::Io {
            context: format!(
                "plugin source '{}' is not a directory",
                source_dir.display()
            ),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "source is not a directory"),
        });
    }

    let validated = load_plugin_from_dir(source_dir)?;
    let id = &validated.manifest.id;

    fs::create_dir_all(plugins_root).map_err(|source| PluginError::Io {
        context: format!("create plugins directory {}", plugins_root.display()),
        source,
    })?;

    let dest_dir = plugins_root.join(id);
    if dest_dir.exists() {
        let canon_src = fs::canonicalize(source_dir).map_err(|source| PluginError::Io {
            context: format!("canonicalize {}", source_dir.display()),
            source,
        })?;
        let canon_dest = fs::canonicalize(&dest_dir).map_err(|source| PluginError::Io {
            context: format!("canonicalize {}", dest_dir.display()),
            source,
        })?;
        if canon_src == canon_dest {
            return Ok(validated);
        }
    }

    let seq = INSTALL_SEQ.fetch_add(1, Ordering::Relaxed);
    let staging_dir = plugins_root.join(format!(".staging-{id}-{}-{seq}", std::process::id()));
    let mut staging_guard = StagingCleanup::new(staging_dir.clone());

    copy_dir_no_symlinks(source_dir, &staging_dir)?;

    let backup_dir = plugins_root.join(format!(".backup-{id}-{}-{seq}", std::process::id()));
    let had_existing = fs::symlink_metadata(&dest_dir).is_ok();
    if had_existing {
        fs::rename(&dest_dir, &backup_dir).map_err(|source| PluginError::Io {
            context: format!("move existing plugin {} aside", dest_dir.display()),
            source,
        })?;
    }

    if let Err(err) = fs::rename(&staging_dir, &dest_dir) {
        if had_existing {
            let _ = fs::rename(&backup_dir, &dest_dir);
        }
        return Err(PluginError::Io {
            context: format!("finalize plugin install at {}", dest_dir.display()),
            source: err,
        });
    }
    staging_guard.disarm();

    if had_existing {
        let _ = fs::remove_dir_all(&backup_dir);
    }

    load_plugin_from_dir(&dest_dir)
}

/// Remove `<plugins_root>/<id>` after validating `id` and checking against symlinks.
pub fn uninstall(id: &str, plugins_root: impl AsRef<Path>) -> Result<(), PluginError> {
    validate_plugin_id(id)?;
    let target = plugins_root.as_ref().join(id);
    let meta = fs::symlink_metadata(&target).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            PluginError::NotFound { id: id.to_owned() }
        } else {
            PluginError::Io {
                context: format!("inspect installed plugin {}", target.display()),
                source: err,
            }
        }
    })?;
    if meta.file_type().is_symlink() {
        return Err(PluginError::PathTraversal {
            entry: id.to_owned(),
            reason: "refusing to uninstall through symlink".to_owned(),
        });
    }
    if !meta.is_dir() {
        return Err(PluginError::NotFound { id: id.to_owned() });
    }
    fs::remove_dir_all(&target).map_err(|source| PluginError::Io {
        context: format!("remove plugin directory {}", target.display()),
        source,
    })
}

fn copy_dir_no_symlinks(src: &Path, dst: &Path) -> Result<(), PluginError> {
    fs::create_dir_all(dst).map_err(|source| PluginError::Io {
        context: format!("create directory {}", dst.display()),
        source,
    })?;

    let entries = fs::read_dir(src).map_err(|source| PluginError::Io {
        context: format!("read directory {}", src.display()),
        source,
    })?;

    for entry_res in entries {
        let entry = entry_res.map_err(|source| PluginError::Io {
            context: format!("iterate directory {}", src.display()),
            source,
        })?;
        let src_path = entry.path();
        let meta = fs::symlink_metadata(&src_path).map_err(|source| PluginError::Io {
            context: format!("inspect {}", src_path.display()),
            source,
        })?;
        if meta.file_type().is_symlink() {
            return Err(PluginError::PathTraversal {
                entry: src_path.display().to_string(),
                reason: "symlinks are not permitted inside plugin packages".to_owned(),
            });
        }
        let dst_path = dst.join(entry.file_name());
        if meta.is_dir() {
            copy_dir_no_symlinks(&src_path, &dst_path)?;
        } else if meta.is_file() {
            fs::copy(&src_path, &dst_path).map_err(|source| PluginError::Io {
                context: format!("copy {} to {}", src_path.display(), dst_path.display()),
                source,
            })?;
            let _ = fs::set_permissions(&dst_path, meta.permissions());
        }
    }

    Ok(())
}
