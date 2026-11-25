//! Append-only, replayable command journal (`commands.jsonl`).

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use photon_protocol::{ClientKind, Command, ProtocolError, UiText};
use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalRecord {
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
    pub command: Command,
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

    pub fn record(
        &self,
        command_id: &str,
        source: ClientKind,
        expected_revision: Option<u64>,
        command: &Command,
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
        let record = JournalRecord {
            timestamp_ms: now_ms(),
            boot_id: self.boot_id.clone(),
            id: command_id.to_owned(),
            source,
            expected_revision,
            command_name: command.name().to_owned(),
            revision,
            ok,
            message,
            error,
            paths: sanitized_paths(command),
            command: command.clone(),
        };
        let Ok(line) = serde_json::to_string(&record) else {
            return;
        };
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{line}");
        }
    }

    pub fn read_records(path: &Path, limit: Option<usize>) -> std::io::Result<Vec<JournalRecord>> {
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
            if let Ok(record) = serde_json::from_str::<JournalRecord>(&line) {
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

fn sanitized_paths(command: &Command) -> Vec<String> {
    match command {
        Command::OpenFiles { paths }
        | Command::Select { paths }
        | Command::Trash { paths }
        | Command::DeletePermanently { paths } => paths.iter().map(|p| sanitize_path(p)).collect(),
        Command::CopyTo { paths, destination } | Command::MoveTo { paths, destination } => {
            let mut out: Vec<String> = paths.iter().map(|p| sanitize_path(p)).collect();
            out.push(sanitize_path(destination));
            out
        }
        Command::OpenFolder { path }
        | Command::Navigate { path }
        | Command::ViewerOpen { path }
        | Command::EditorOpen { path }
        | Command::EditorSaveAs { path }
        | Command::AddFavorite { path }
        | Command::RemoveFavorite { path }
        | Command::RenamePath { path, .. }
        | Command::ViewerTranslate { path, .. } => vec![sanitize_path(path)],
        Command::NewFolder { parent, .. } | Command::PasteImageBytes { folder: parent, .. } => {
            vec![sanitize_path(parent)]
        }
        Command::PluginInstall { source_dir } => vec![sanitize_path(source_dir)],
        Command::DebugScreenshot {
            output_path: Some(path),
        } => vec![sanitize_path(path)],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_roundtrips_replayable_records() {
        let dir = std::env::temp_dir().join(format!("photon-journal-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let journal = Journal::new(&dir, "boot-test");
        let cmd = Command::Select {
            paths: vec!["/tmp/photo.png".to_owned()],
        };
        let res: Result<Option<UiText>, ProtocolError> = Ok(Some(UiText::new("selected")));
        journal.record("req-1", ClientKind::Cli, None, &cmd, 2, &res);

        let records = Journal::read_records(&dir.join("commands.jsonl"), None).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "req-1");
        assert_eq!(records[0].command_name, "select");
        assert_eq!(records[0].revision, 2);
        assert!(records[0].ok);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
