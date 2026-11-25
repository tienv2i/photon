//! Structured daily-rolled JSONL logging with stderr mirror and log inspection helpers.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{identity::AppIdentity, journal::now_ms, paths::AppPaths};

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();
static BOOT_ID: OnceLock<String> = OnceLock::new();

pub fn current_boot_id() -> &'static str {
    BOOT_ID.get_or_init(|| format!("boot-{}-{}", now_ms(), std::process::id()))
}

pub fn initialize(paths: &AppPaths, with_stderr: bool) -> Result<(), String> {
    if LOG_GUARD.get().is_some() {
        return Ok(());
    }
    paths
        .ensure_runtime_dirs()
        .map_err(|e| format!("cannot create runtime directories: {e}"))?;

    let appender = tracing_appender::rolling::daily(&paths.log_dir, "photon.jsonl");
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info,photon_core=debug,photon_ipc=debug,photon_desktop=debug")
    });

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().json().with_ansi(false).with_writer(writer));

    if with_stderr {
        registry
            .with(fmt::layer().compact().with_writer(std::io::stderr))
            .try_init()
            .map_err(|e| format!("cannot initialize tracing subscriber: {e}"))?;
    } else {
        registry
            .try_init()
            .map_err(|e| format!("cannot initialize tracing subscriber: {e}"))?;
    }

    let _ = LOG_GUARD.set(guard);
    Ok(())
}

pub fn log_startup(identity: &AppIdentity, paths: &AppPaths, headless: bool) {
    tracing::info!(
        target: "photon::startup",
        app_id = identity.app_id,
        display_name = identity.display_name,
        profile = identity.profile.as_str(),
        version = identity.version,
        protocol_version = identity.protocol_version,
        boot_id = current_boot_id(),
        headless,
        state_dir = %paths.state_dir.display(),
        log_dir = %paths.log_dir.display(),
        socket_path = %paths.socket_path.display(),
        "application starting"
    );
}

pub fn log_shutdown() {
    tracing::info!(
        target: "photon::startup",
        boot_id = current_boot_id(),
        "application stopping"
    );
}

pub fn latest_log_file(log_dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(log_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("photon.jsonl"))
        })
        .collect();
    files.sort();
    files.pop()
}

pub fn read_log_lines(
    log_dir: &Path,
    last: Option<usize>,
    request_id: Option<&str>,
) -> std::io::Result<Vec<serde_json::Value>> {
    let Some(file_path) = latest_log_file(log_dir) else {
        return Ok(Vec::new());
    };
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if let Some(req_id) = request_id {
            let text = line.as_str();
            if !text.contains(req_id) {
                continue;
            }
        }
        entries.push(val);
    }
    if let Some(limit) = last
        && entries.len() > limit
    {
        entries = entries.split_off(entries.len() - limit);
    }
    Ok(entries)
}
