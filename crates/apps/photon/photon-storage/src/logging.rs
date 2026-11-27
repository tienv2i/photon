//! Photon structured daily-rolled JSONL logging built atop `fw-storage`.

use std::path::{Path, PathBuf};

use crate::{identity::AppIdentity, paths::AppPaths};

const LOG_PREFIX: &str = "photon.jsonl";
const DEFAULT_FILTER: &str = "info,photon_core=debug,photon_ipc=debug,photon_desktop=debug";

pub use fw_storage::logging::{current_boot_id, log_shutdown};

pub fn initialize(paths: &AppPaths, with_stderr: bool) -> Result<(), String> {
    fw_storage::logging::initialize_with(paths.as_ref(), LOG_PREFIX, DEFAULT_FILTER, with_stderr)
}

pub fn log_startup(identity: &AppIdentity, paths: &AppPaths, headless: bool) {
    fw_storage::logging::log_startup(identity, paths.as_ref(), headless);
}

pub fn latest_log_file(log_dir: &Path) -> Option<PathBuf> {
    fw_storage::logging::latest_log_file_with(log_dir, LOG_PREFIX)
}

pub fn read_log_lines(
    log_dir: &Path,
    last: Option<usize>,
    request_id: Option<&str>,
) -> std::io::Result<Vec<serde_json::Value>> {
    fw_storage::logging::read_log_lines_with(log_dir, LOG_PREFIX, last, request_id)
}
