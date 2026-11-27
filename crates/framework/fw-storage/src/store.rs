//! Generic atomic JSON persistence with corruption backup and migration hooks.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Back up an existing JSON file to `<path>.bak` if present.
pub fn backup_existing_file(path: &Path) -> Option<PathBuf> {
    if !path.is_file() {
        return None;
    }
    let backup = path.with_extension("json.bak");
    fs::copy(path, &backup).ok().map(|_| backup)
}

/// Write `value` as pretty-printed JSON atomically via a temporary sibling file.
pub fn write_json_atomically<T: Serialize>(value: &T, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|e| std::io::Error::other(e.to_string()))?;
    let res = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    res
}

/// Load a JSON file, sanitize/migrate it, or recover from corruption/missing file.
///
/// `on_load` receives the deserialized value and returns `(final_value, should_persist)`.
/// `on_missing` produces the initial value when the file does not exist yet.
pub fn load_json_or_recover<T, FLoad, FMissing>(
    path: &Path,
    label: &str,
    on_load: FLoad,
    on_missing: FMissing,
) -> (T, Option<String>)
where
    T: Serialize + DeserializeOwned,
    FLoad: FnOnce(T) -> (T, bool),
    FMissing: Fn() -> T,
{
    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<T>(&text) {
            Ok(parsed) => {
                let (value, should_save) = on_load(parsed);
                let warn = if should_save {
                    write_json_atomically(&value, path)
                        .err()
                        .map(|e| format!("{label} migrated in memory, save failed: {e}"))
                } else {
                    None
                };
                (value, warn)
            }
            Err(error) => {
                let backup = path.with_extension("json.corrupt");
                let _ = fs::copy(path, &backup);
                let value = on_missing();
                let _ = write_json_atomically(&value, path);
                (
                    value,
                    Some(format!(
                        "{label} corrupt ({error}); defaults restored, backup kept at {}",
                        backup.display()
                    )),
                )
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let value = on_missing();
            let warn = write_json_atomically(&value, path)
                .err()
                .map(|e| format!("cannot create initial {label}: {e}"));
            (value, warn)
        }
        Err(err) => (
            on_missing(),
            Some(format!("cannot read {label} {}: {err}", path.display())),
        ),
    }
}
