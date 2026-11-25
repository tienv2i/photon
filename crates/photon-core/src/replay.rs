//! Deterministic journal replay in an isolated sandbox Core.

use std::path::Path;

use photon_protocol::{ClientKind, Envelope};
use photon_storage::Journal;
use serde::{Deserialize, Serialize};

use crate::Core;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayStepResult {
    pub original_id: String,
    pub command: String,
    pub original_ok: bool,
    pub replay_ok: bool,
    pub original_revision: u64,
    pub replay_revision: u64,
    pub skipped: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayReport {
    pub journal_path: String,
    pub sandbox_dir: String,
    pub total_records: usize,
    pub replayed: usize,
    pub skipped: usize,
    pub divergences: usize,
    pub final_revision: u64,
    pub steps: Vec<ReplayStepResult>,
}

pub fn replay_journal_in_sandbox(
    journal_path: &Path,
    sandbox_dir: &Path,
    stop_on_error: bool,
) -> Result<ReplayReport, String> {
    let records = Journal::read_records(journal_path, None)
        .map_err(|e| format!("failed to read journal {}: {e}", journal_path.display()))?;
    let mut core = Core::for_sandbox(sandbox_dir);

    let mut steps = Vec::with_capacity(records.len());
    let mut replayed = 0usize;
    let mut skipped = 0usize;
    let mut divergences = 0usize;

    for record in records {
        if !record.command.is_replayable() {
            skipped += 1;
            steps.push(ReplayStepResult {
                original_id: record.id,
                command: record.command_name,
                original_ok: record.ok,
                replay_ok: record.ok,
                original_revision: record.revision,
                replay_revision: core.revision(),
                skipped: true,
                error: None,
            });
            continue;
        }

        replayed += 1;
        let mut env = Envelope::new(
            format!("replay-{}", record.id),
            ClientKind::Replay,
            record.command,
        );
        env.expected_revision = record.expected_revision;
        let resp = core.dispatch(env);
        let diverged = resp.ok != record.ok;
        if diverged {
            divergences += 1;
        }
        let err_msg = resp.error.as_ref().map(|e| e.message.clone());
        steps.push(ReplayStepResult {
            original_id: record.id,
            command: record.command_name,
            original_ok: record.ok,
            replay_ok: resp.ok,
            original_revision: record.revision,
            replay_revision: resp.revision,
            skipped: false,
            error: err_msg,
        });
        if stop_on_error && !resp.ok {
            break;
        }
    }

    Ok(ReplayReport {
        journal_path: journal_path.to_string_lossy().into_owned(),
        sandbox_dir: sandbox_dir.to_string_lossy().into_owned(),
        total_records: steps.len(),
        replayed,
        skipped,
        divergences,
        final_revision: core.revision(),
        steps,
    })
}
