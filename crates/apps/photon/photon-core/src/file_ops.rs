//! Safe filesystem operations, entry sorting, desktop trash (`gio trash`),
//! and Quick Access favorites management for `photon-core`.

use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use photon_protocol::{BrowserSortField, Entry, MoveDirection, ProtocolError, SortOrder, UiText};
use photon_storage::paths::home_dir;

/// Compute a short human-readable type label for a directory or file entry.
pub fn kind_label(path: &Path, is_dir: bool) -> String {
    if is_dir {
        return "folder".to_owned();
    }
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if !ext.is_empty() => ext.to_ascii_uppercase(),
        _ => "file".to_owned(),
    }
}

/// Normalize `.` and `..` path components without resolving symlinks.
pub fn normalize_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let mut out = PathBuf::new();
    for comp in absolute.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(Component::RootDir.as_os_str());
                }
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

/// Sort folder entries keeping directories first, then ordering by `field` and `order`.
pub fn sort_entries(entries: &mut [Entry], field: BrowserSortField, order: SortOrder) {
    entries.sort_by(|a, b| {
        let dir_cmp = b.is_dir.cmp(&a.is_dir);
        if dir_cmp != std::cmp::Ordering::Equal {
            return dir_cmp;
        }
        let primary = match field {
            BrowserSortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            BrowserSortField::Modified => a.modified_ms.cmp(&b.modified_ms),
            BrowserSortField::Size => a.size.cmp(&b.size),
            BrowserSortField::Kind => a.kind.to_lowercase().cmp(&b.kind.to_lowercase()),
        };
        let ordered = match order {
            SortOrder::Asc => primary,
            SortOrder::Desc => primary.reverse(),
        };
        ordered.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// Validate a file or folder name (rejects empty, `.`, `..`, or path separators).
pub fn valid_entry_name(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty()
        && trimmed != "."
        && trimmed != ".."
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
        && !trimmed.contains('\0')
}

/// Refuse relative paths, filesystem roots, and the user's home folder from deletion.
pub fn safe_for_deletion(path: &Path) -> Result<(), ProtocolError> {
    if !path.is_absolute() {
        return Err(ProtocolError::new(
            "unsafe_delete",
            "relative paths cannot be deleted",
        ));
    }
    let mut components = path.components();
    if components.next().is_none() || components.next().is_none() {
        return Err(ProtocolError::new(
            "unsafe_delete",
            "filesystem root cannot be deleted",
        ));
    }
    let normalized = normalize_path(path);
    if normalized == normalize_path(&home_dir()) {
        return Err(ProtocolError::new(
            "unsafe_delete",
            "home folder cannot be deleted",
        ));
    }
    Ok(())
}

/// Move `paths` to the freedesktop Trash via `gio trash`.
/// Never falls back silently to permanent deletion.
pub fn trash_paths(paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        let mut cmd = std::process::Command::new("gio");
        cmd.arg("trash").arg("--");
        for p in paths {
            cmd.arg(p);
        }
        match cmd.output() {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
                Err(if stderr.is_empty() {
                    "gio trash failed".to_owned()
                } else {
                    format!("gio trash failed: {stderr}")
                })
            }
            Err(err) => Err(format!(
                "cannot run gio trash ({err}); use permanent delete instead"
            )),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = paths;
        Err("move to trash is not implemented on this platform".to_owned())
    }
}

pub fn remove_path(path: &Path) -> Result<(), String> {
    let meta =
        fs::symlink_metadata(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if meta.is_dir() {
        fs::remove_dir_all(path).map_err(|e| format!("cannot delete {}: {e}", path.display()))
    } else {
        fs::remove_file(path).map_err(|e| format!("cannot delete {}: {e}", path.display()))
    }
}

pub fn rename_path(path: &Path, new_name: &str) -> Result<PathBuf, ProtocolError> {
    let clean = new_name.trim();
    if !valid_entry_name(clean) {
        return Err(ProtocolError::new(
            "invalid_name",
            "invalid file or folder name",
        ));
    }
    if !path.exists() && fs::symlink_metadata(path).is_err() {
        return Err(ProtocolError::new(
            "not_found",
            format!("not found: {}", path.display()),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| ProtocolError::new("invalid_argument", "path has no parent"))?;
    let target = parent.join(clean);
    if target == path {
        return Ok(target);
    }
    if target.exists() {
        return Err(ProtocolError::new(
            "already_exists",
            format!("{} already exists", target.display()),
        ));
    }
    fs::rename(path, &target).map_err(|e| ProtocolError::new("io_error", e.to_string()))?;
    Ok(target)
}

fn split_stem_ext(name: &OsStr) -> (String, String) {
    let p = Path::new(name);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "item".to_owned());
    let ext = p
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (stem, ext)
}

fn unique_target(destination: &Path, name: &OsStr) -> PathBuf {
    let mut target = destination.join(name);
    if !target.exists() {
        return target;
    }
    let (stem, ext) = split_stem_ext(name);
    let mut idx = 1usize;
    loop {
        target = destination.join(format!("{stem} ({idx}){ext}"));
        if !target.exists() {
            return target;
        }
        idx += 1;
    }
}

fn unique_copy_target(destination: &Path, name: &OsStr) -> PathBuf {
    let target = destination.join(name);
    if !target.exists() {
        return target;
    }
    let (stem, ext) = split_stem_ext(name);
    let first = destination.join(format!("{stem} (copy){ext}"));
    if !first.exists() {
        return first;
    }
    let mut idx = 2usize;
    loop {
        let candidate = destination.join(format!("{stem} (copy {idx}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
        idx += 1;
    }
}

/// Sanitize a filename hint and return a non-colliding `.png` path inside `folder`.
pub fn unique_png_path(folder: &Path, filename_hint: &str) -> PathBuf {
    let raw_name = Path::new(filename_hint.trim())
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let stem = if valid_entry_name(raw_name) {
        Path::new(raw_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::trim)
            .filter(|s| valid_entry_name(s))
            .unwrap_or("pasted")
    } else {
        "pasted"
    };
    let candidate = format!("{stem}.png");
    unique_copy_target(folder, OsStr::new(&candidate))
}

pub fn copy_path(source: &Path, destination: &Path) -> Result<PathBuf, String> {
    if !destination.is_dir() {
        return Err(format!("{} is not a directory", destination.display()));
    }
    if source.is_dir() && normalize_path(destination).starts_with(normalize_path(source)) {
        return Err(format!(
            "cannot copy folder {} inside itself",
            source.display()
        ));
    }
    let name = source
        .file_name()
        .ok_or_else(|| format!("{} has no file name", source.display()))?;
    let target = unique_copy_target(destination, name);
    copy_recursive(source, &target)?;
    Ok(target)
}

fn copy_recursive(source: &Path, target: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(source)
        .map_err(|e| format!("cannot read {}: {e}", source.display()))?;
    if meta.file_type().is_symlink() {
        return copy_symlink(source, target);
    }
    if meta.is_dir() {
        fs::create_dir(target).map_err(|e| format!("cannot create {}: {e}", target.display()))?;
        let rd =
            fs::read_dir(source).map_err(|e| format!("cannot read {}: {e}", source.display()))?;
        for item in rd {
            let item = item.map_err(|e| format!("cannot read entry: {e}"))?;
            copy_recursive(&item.path(), &target.join(item.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(source, target)
            .map(|_| ())
            .map_err(|e| format!("cannot copy {}: {e}", source.display()))
    }
}

#[cfg(unix)]
fn copy_symlink(source: &Path, target: &Path) -> Result<(), String> {
    let link =
        fs::read_link(source).map_err(|e| format!("cannot read link {}: {e}", source.display()))?;
    std::os::unix::fs::symlink(&link, target).map_err(|e| {
        format!(
            "cannot link {} -> {}: {e}",
            target.display(),
            link.display()
        )
    })
}

#[cfg(not(unix))]
fn copy_symlink(source: &Path, target: &Path) -> Result<(), String> {
    fs::copy(source, target)
        .map(|_| ())
        .map_err(|e| format!("cannot copy {}: {e}", source.display()))
}

pub fn move_path(source: &Path, destination: &Path) -> Result<PathBuf, String> {
    if !destination.is_dir() {
        return Err(format!("{} is not a directory", destination.display()));
    }
    let name = source
        .file_name()
        .ok_or_else(|| format!("{} has no file name", source.display()))?;
    let target = unique_target(destination, name);
    match fs::rename(source, &target) {
        Ok(()) => Ok(target),
        Err(_) => {
            copy_recursive(source, &target)?;
            remove_path(source)?;
            Ok(target)
        }
    }
}

pub fn add_favorite(
    favorites: &mut Vec<String>,
    path: &Path,
) -> Result<Option<UiText>, ProtocolError> {
    if !path.is_dir() {
        return Err(ProtocolError::new(
            "not_found",
            format!("{} is not a directory", path.display()),
        ));
    }
    let norm = normalize_path(path).to_string_lossy().into_owned();
    if favorites.iter().any(|f| f == &norm) {
        return Ok(None);
    }
    favorites.push(norm.clone());
    let name = Path::new(&norm)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&norm)
        .to_owned();
    Ok(Some(UiText::with("favorite_added", [("name", name)])))
}

pub fn remove_favorite(favorites: &mut Vec<String>, path: &str) -> Option<UiText> {
    let norm = normalize_path(Path::new(path))
        .to_string_lossy()
        .into_owned();
    let prev_len = favorites.len();
    favorites.retain(|f| f != path && f != &norm);
    if favorites.len() == prev_len {
        return None;
    }
    let name = Path::new(&norm)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&norm)
        .to_owned();
    Some(UiText::with("favorite_removed", [("name", name)]))
}

pub fn move_favorite(favorites: &mut [String], path: &str, direction: MoveDirection) -> bool {
    let norm = normalize_path(Path::new(path))
        .to_string_lossy()
        .into_owned();
    let Some(idx) = favorites.iter().position(|f| {
        f == path || f == &norm || normalize_path(Path::new(f)).to_string_lossy() == norm
    }) else {
        return false;
    };
    match direction {
        MoveDirection::Up if idx > 0 => {
            favorites.swap(idx, idx - 1);
            true
        }
        MoveDirection::Down if idx + 1 < favorites.len() => {
            favorites.swap(idx, idx + 1);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_for_deletion_rejects_relative_root_and_home() {
        assert!(safe_for_deletion(Path::new("relative/file.png")).is_err());
        assert!(safe_for_deletion(Path::new("/")).is_err());
        assert!(safe_for_deletion(&home_dir()).is_err());
        assert!(safe_for_deletion(Path::new("/tmp/some-test-file.png")).is_ok());
    }

    #[test]
    fn sort_entries_keeps_folders_first_and_sorts_by_field() {
        let mut entries = vec![
            Entry {
                name: "b.png".into(),
                path: "/b.png".into(),
                is_dir: false,
                is_image: true,
                size: 100,
                modified_ms: 20,
                kind: "PNG".into(),
            },
            Entry {
                name: "z_dir".into(),
                path: "/z_dir".into(),
                is_dir: true,
                is_image: false,
                size: 0,
                modified_ms: 10,
                kind: "folder".into(),
            },
            Entry {
                name: "a.jpg".into(),
                path: "/a.jpg".into(),
                is_dir: false,
                is_image: true,
                size: 300,
                modified_ms: 5,
                kind: "JPG".into(),
            },
        ];

        sort_entries(&mut entries, BrowserSortField::Size, SortOrder::Desc);
        assert_eq!(entries[0].name, "z_dir");
        assert_eq!(entries[1].name, "a.jpg");
        assert_eq!(entries[2].name, "b.png");
    }

    #[test]
    fn favorites_add_move_remove_work() {
        let dir = std::env::temp_dir().join(format!("photon-fav-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let a = dir.join("a");
        let b = dir.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        let mut favs = Vec::new();
        assert!(add_favorite(&mut favs, &a).unwrap().is_some());
        assert!(add_favorite(&mut favs, &a).unwrap().is_none());
        assert!(add_favorite(&mut favs, &b).unwrap().is_some());
        assert_eq!(favs.len(), 2);

        assert!(move_favorite(
            &mut favs,
            &b.to_string_lossy(),
            MoveDirection::Up
        ));
        assert!(favs[0].ends_with("/b"));
        assert!(remove_favorite(&mut favs, &a.to_string_lossy()).is_some());
        assert_eq!(favs.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }
}
