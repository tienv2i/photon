//! NTFS/FUSE-aware two-phase directory scanner.
//!
//! Phase 1 uses `read_dir` + `DirEntry::file_type()` (`d_type`) and filters by
//! extension/hidden-name before touching `stat()`, keeping folder scans fast on
//! slow FUSE/NTFS/network mounts. Phase 2 (`stat_entry_metadata`) fetches
//! `size` and `modified_ms` on demand.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Filesystem performance hint used to throttle prefetching and metadata scans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemProfile {
    FastLocal,
    SlowFuseOrNetwork,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Detect whether `dir` resides on a slow FUSE/NTFS/network mount on Linux.
#[cfg(target_os = "linux")]
pub fn detect_filesystem_profile(dir: &Path) -> FilesystemProfile {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let Ok(c_path) = CString::new(dir.as_os_str().as_bytes()) else {
        return FilesystemProfile::FastLocal;
    };
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statfs(c_path.as_ptr(), &mut stat) };
    if rc != 0 {
        return FilesystemProfile::FastLocal;
    }
    const FUSE_SUPER_MAGIC: i64 = 0x6573_5546;
    const NTFS_SB_MAGIC: i64 = 0x5346_544e;
    const NTFS3_MAGIC: i64 = 0x7366_746e;
    const SMB2_MAGIC: i64 = 0xfe53_4d42;
    const CIFS_MAGIC: i64 = 0xff53_4d42;
    const NFS_SUPER_MAGIC: i64 = 0x6969;

    match stat.f_type as i64 {
        FUSE_SUPER_MAGIC | NTFS_SB_MAGIC | NTFS3_MAGIC | SMB2_MAGIC | CIFS_MAGIC
        | NFS_SUPER_MAGIC => FilesystemProfile::SlowFuseOrNetwork,
        _ => FilesystemProfile::FastLocal,
    }
}

#[cfg(not(target_os = "linux"))]
pub fn detect_filesystem_profile(_dir: &Path) -> FilesystemProfile {
    FilesystemProfile::FastLocal
}

/// Phase-1 metadata-free directory scan.
///
/// `include_file` is evaluated against the path *before* falling back to
/// `DirEntry::metadata()` when `d_type` is unavailable (e.g. `DT_UNKNOWN` or
/// symlinks), avoiding unnecessary `stat()` calls on filtered-out files.
pub fn scan_dir_fast<F>(
    dir: &Path,
    show_hidden: bool,
    include_dirs: bool,
    mut include_file: F,
) -> std::io::Result<Vec<ScannedEntry>>
where
    F: FnMut(&Path) -> bool,
{
    let rd = fs::read_dir(dir)?;
    let mut out = Vec::new();
    for item in rd.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let path = item.path();
        match item.file_type() {
            Ok(ft) if ft.is_dir() => {
                if include_dirs {
                    out.push(ScannedEntry {
                        name,
                        path,
                        is_dir: true,
                    });
                }
            }
            Ok(ft) if ft.is_file() => {
                if include_file(&path) {
                    out.push(ScannedEntry {
                        name,
                        path,
                        is_dir: false,
                    });
                }
            }
            _ => {
                let candidate_file = include_file(&path);
                if !include_dirs && !candidate_file {
                    continue;
                }
                let Ok(meta) = item.metadata() else {
                    continue;
                };
                if meta.is_dir() && include_dirs {
                    out.push(ScannedEntry {
                        name,
                        path,
                        is_dir: true,
                    });
                } else if meta.is_file() && candidate_file {
                    out.push(ScannedEntry {
                        name,
                        path,
                        is_dir: false,
                    });
                }
            }
        }
    }
    Ok(out)
}

/// Phase-2 on-demand metadata probe returning `(size_bytes, modified_ms)`.
pub fn stat_entry_metadata(entry: &fs::DirEntry) -> (u64, u64) {
    let meta = entry.metadata().ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let modified_ms = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    (size, modified_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_dir_fast_filters_before_collecting_and_skips_hidden() {
        let dir = std::env::temp_dir().join(format!("comp-fb-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub.png")).unwrap();
        fs::write(dir.join("a.png"), b"1").unwrap();
        fs::write(dir.join("b.txt"), b"2").unwrap();
        fs::write(dir.join(".hidden.png"), b"3").unwrap();

        let files = scan_dir_fast(&dir, false, false, |p| {
            p.extension().and_then(|e| e.to_str()) == Some("png")
        })
        .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "a.png");
        assert!(!files[0].is_dir);

        let with_dirs = scan_dir_fast(&dir, false, true, |_| false).unwrap();
        assert_eq!(with_dirs.len(), 1);
        assert_eq!(with_dirs[0].name, "sub.png");
        assert!(with_dirs[0].is_dir);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_filesystem_profile_handles_temp_dir() {
        let _ = detect_filesystem_profile(&std::env::temp_dir());
    }
}
