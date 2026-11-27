//! Debounced atomic auto-recovery for unsaved Editor tabs.
//!
//! Each modified tab writes at most one `.ora` draft (`tab_<id>.ora`) inside
//! `<data_dir>/recovery/` using temp-file + atomic rename, and removes it
//! immediately on explicit save, normal tab close, or clean exit.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use photon_image::EditorDocument;
use photon_image::layer::LayerStack;
use photon_storage::AppPaths;
use serde::{Deserialize, Serialize};

const MAX_RECOVERY_AGE_MS: u64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryTabMeta {
    pub tab_id: u64,
    pub name: String,
    pub source_path: String,
    pub save_path: String,
    pub ora_file: String,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RecoveryManifest {
    #[serde(default)]
    tabs: Vec<RecoveryTabMeta>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn recovery_dir(paths: &AppPaths) -> PathBuf {
    paths.data_dir.join("recovery")
}

fn manifest_path(dir: &Path) -> PathBuf {
    dir.join("manifest.json")
}

fn load_manifest(dir: &Path) -> RecoveryManifest {
    let path = manifest_path(dir);
    let Ok(raw) = fs::read_to_string(&path) else {
        return RecoveryManifest::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_manifest(dir: &Path, manifest: &RecoveryManifest) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("create recovery dir: {e}"))?;
    let json =
        serde_json::to_string_pretty(manifest).map_err(|e| format!("serialize manifest: {e}"))?;
    let tmp = dir.join("manifest.json.tmp");
    let dst = manifest_path(dir);
    let write_res = (|| -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(json.as_bytes())?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, &dst)
    })();
    if let Err(e) = write_res {
        let _ = fs::remove_file(&tmp);
        return Err(format!("save manifest: {e}"));
    }
    Ok(())
}

/// Count valid recoverable tabs and prune stale entries older than 7 days.
pub fn count_recoverable_tabs(paths: &AppPaths) -> usize {
    let dir = recovery_dir(paths);
    if !dir.is_dir() {
        return 0;
    }
    let mut manifest = load_manifest(&dir);
    let cutoff = now_ms().saturating_sub(MAX_RECOVERY_AGE_MS);
    let before_len = manifest.tabs.len();
    manifest.tabs.retain(|entry| {
        let ora = dir.join(&entry.ora_file);
        let valid = entry.updated_at_ms >= cutoff && ora.is_file();
        if !valid {
            let _ = fs::remove_file(&ora);
        }
        valid
    });
    if manifest.tabs.len() != before_len {
        let _ = save_manifest(&dir, &manifest);
    }
    manifest.tabs.len()
}

/// Atomically persist one tab's layer stack and update `manifest.json`.
pub fn write_tab_recovery(
    dir: &Path,
    tab_id: u64,
    name: &str,
    source_path: &str,
    save_path: &str,
    stack: &LayerStack,
) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("create recovery dir: {e}"))?;
    let ora_file = format!("tab_{tab_id}.ora");
    let tmp_ora = dir.join(format!("tab_{tab_id}.ora.tmp"));
    let dst_ora = dir.join(&ora_file);
    photon_image::ora::encode_ora_stack(stack, &tmp_ora)
        .map_err(|e| format!("encode recovery ora: {e}"))?;
    if let Err(e) = fs::rename(&tmp_ora, &dst_ora) {
        let _ = fs::remove_file(&tmp_ora);
        return Err(format!("rename recovery ora: {e}"));
    }

    let mut manifest = load_manifest(dir);
    let entry = RecoveryTabMeta {
        tab_id,
        name: name.to_owned(),
        source_path: source_path.to_owned(),
        save_path: save_path.to_owned(),
        ora_file,
        updated_at_ms: now_ms(),
    };
    if let Some(existing) = manifest.tabs.iter_mut().find(|t| t.tab_id == tab_id) {
        *existing = entry;
    } else {
        manifest.tabs.push(entry);
    }
    save_manifest(dir, &manifest)
}

/// Remove recovery draft for a single tab (after explicit save or normal close).
pub fn remove_tab_recovery(paths: &AppPaths, tab_id: u64) {
    let dir = recovery_dir(paths);
    if !dir.is_dir() {
        return;
    }
    let _ = fs::remove_file(dir.join(format!("tab_{tab_id}.ora")));
    let mut manifest = load_manifest(&dir);
    let prev = manifest.tabs.len();
    manifest.tabs.retain(|t| t.tab_id != tab_id);
    if manifest.tabs.is_empty() {
        cleanup_all_recovery(paths);
    } else if manifest.tabs.len() != prev {
        let _ = save_manifest(&dir, &manifest);
    }
}

/// Remove all recovery drafts (on clean exit or user discard).
pub fn cleanup_all_recovery(paths: &AppPaths) {
    let dir = recovery_dir(paths);
    if dir.exists() {
        let _ = fs::remove_dir_all(&dir);
    }
}

/// Load all recoverable tab drafts from disk and clear the recovery folder.
pub fn take_recoverable_tabs(paths: &AppPaths) -> Vec<(RecoveryTabMeta, EditorDocument)> {
    let dir = recovery_dir(paths);
    if !dir.is_dir() {
        return Vec::new();
    }
    let manifest = load_manifest(&dir);
    let mut out = Vec::new();
    for meta in manifest.tabs {
        let ora_path = dir.join(&meta.ora_file);
        if let Ok(doc) = EditorDocument::open(ora_path) {
            out.push((meta, doc));
        }
    }
    cleanup_all_recovery(paths);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use photon_image::layer::LayerStack;
    use std::sync::Arc;

    #[test]
    fn write_count_and_restore_recovery_tabs_roundtrip() {
        let dir = std::env::temp_dir().join(format!("photon-rec-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let paths = AppPaths::for_sandbox(&dir);
        let rec_dir = recovery_dir(&paths);

        let img = RgbaImage::from_pixel(4, 4, Rgba([20, 40, 60, 255]));
        let stack = LayerStack::from_single(Arc::new(img));
        write_tab_recovery(
            &rec_dir,
            42,
            "Test.png",
            "/tmp/Test.png",
            "/tmp/Test.png",
            &stack,
        )
        .expect("write recovery");

        assert_eq!(count_recoverable_tabs(&paths), 1);

        let restored = take_recoverable_tabs(&paths);
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].0.tab_id, 42);
        assert_eq!(restored[0].0.name, "Test.png");
        assert_eq!(restored[0].1.dimensions(), (4, 4));
        assert_eq!(count_recoverable_tabs(&paths), 0);

        let _ = fs::remove_dir_all(&dir);
    }
}
