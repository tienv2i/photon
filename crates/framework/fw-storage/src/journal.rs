//! Append-only, replayable command journal (`commands.jsonl`).

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fw_protocol::{ClientKind, ProtocolCommand, ProtocolError, UiText};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalRecord<C> {
    pub timestamp_ms: u64,
    pub boot_id: String,
    pub id: String,
    pub source: ClientKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<u64>,
    pub command_name: String,
    pub revision: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProtocolError>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    pub command: C,
}

pub struct Journal {
    path: Option<PathBuf>,
    boot_id: String,
}

impl Journal {
    pub fn new(dir: &Path, boot_id: impl Into<String>) -> Self {
        let path = dir.join("commands.jsonl");
        let enabled = dir.is_dir() || std::fs::create_dir_all(dir).is_ok();
        Self {
            path: enabled.then_some(path),
            boot_id: boot_id.into(),
        }
    }

    pub fn disabled() -> Self {
        Self {
            path: None,
            boot_id: "disabled".to_owned(),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn record<C: ProtocolCommand>(
        &self,
        command_id: &str,
        source: ClientKind,
        expected_revision: Option<u64>,
        command: &C,
        revision: u64,
        result: &Result<Option<UiText>, ProtocolError>,
    ) {
        let Some(path) = &self.path else {
            return;
        };
        let (ok, message, error) = match result {
            Ok(line) => (true, line.as_ref().map(UiText::english), None),
            Err(err) => (false, None, Some(err.clone())),
        };
        let paths = command
            .journal_paths()
            .into_iter()
            .map(sanitize_path)
            .collect();
        let record = JournalRecord {
            timestamp_ms: now_ms(),
            boot_id: self.boot_id.clone(),
            id: command_id.to_owned(),
            source,
            expected_revision,
            command_name: command.command_name().to_owned(),
            revision,
            ok,
            message,
            error,
            paths,
            command: command.clone(),
        };
        let Ok(line) = serde_json::to_string(&record) else {
            return;
        };
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{line}");
        }
    }

    pub fn read_records<C: DeserializeOwned>(
        path: &Path,
        limit: Option<usize>,
    ) -> std::io::Result<Vec<JournalRecord<C>>> {
        if !path.is_file() {
            return Ok(Vec::new());
        }
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut records = Vec::new();
        for line in reader.lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(record) = serde_json::from_str::<JournalRecord<C>>(&line) {
                records.push(record);
            }
        }
        if let Some(limit) = limit
            && records.len() > limit
        {
            let start = records.len() - limit;
            records = records.split_off(start);
        }
        Ok(records)
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

pub fn sanitize_path(raw: &str) -> String {
    let home = paths::home_dir();
    let home = home.to_string_lossy();
    if !home.is_empty() && raw.starts_with(home.as_ref()) {
        format!("~{}", &raw[home.len()..])
    } else {
        raw.to_owned()
    }
}
