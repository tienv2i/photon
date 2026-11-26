//! `photonctl`: lightweight CLI, agent control surface, debugger, and MCP stdio server.

mod mcp;

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand, ValueEnum};
use photon_core::replay_journal_in_sandbox;
use photon_ipc::{IpcRequest, IpcResponse, send_once, subscribe_stream};
use photon_protocol::{
    ClientKind, Command, Envelope, EventKind, PROTOCOL_VERSION, Query, Workspace, protocol_schemas,
};
use photon_storage::logging::read_log_lines;
use photon_storage::{AppPaths, Config, Journal, UiState};

#[derive(Parser)]
#[command(
    name = "photonctl",
    version,
    about = "Control & debug Photon instances and automation"
)]
struct Cli {
    /// Override IPC socket path (defaults to current profile's socket)
    #[arg(long, global = true)]
    socket: Option<PathBuf>,

    /// Output compact JSON for scripts and agents
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Sub,
}

#[derive(Clone, Copy, ValueEnum)]
enum WorkspaceArg {
    Browser,
    Viewer,
    Editor,
}

impl From<WorkspaceArg> for Workspace {
    fn from(value: WorkspaceArg) -> Self {
        match value {
            WorkspaceArg::Browser => Self::Browser,
            WorkspaceArg::Viewer => Self::Viewer,
            WorkspaceArg::Editor => Self::Editor,
        }
    }
}

#[derive(Subcommand)]
enum Sub {
    /// Inspect runtime status of the running Photon instance
    Status,
    /// Open image file(s) in Viewer
    Open { paths: Vec<String> },
    /// Open folder in Browser
    OpenFolder { path: String },
    /// Open image in Editor
    Edit { path: String },
    /// Switch active workspace
    Workspace { target: WorkspaceArg },
    /// Dispatch a typed Command JSON (or full Envelope JSON)
    Dispatch {
        /// Command or Envelope JSON string
        payload: String,
        #[arg(long)]
        expected_revision: Option<u64>,
    },
    /// Query read-only Core state
    Query {
        #[arg(value_enum)]
        kind: QueryArg,
    },
    /// Print JSON Schema of the entire Photon protocol
    Schema,
    /// Stream real-time events from Core
    Events {
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Debugging, journal inspection, UI inspection, and deterministic replay
    Debug {
        #[command(subcommand)]
        cmd: DebugSub,
    },
    /// Run MCP (Model Context Protocol) JSON-RPC server over stdio
    Mcp,
    /// Reset layout, settings, or both (online via IPC when running, or offline)
    Reset {
        #[arg(value_enum)]
        target: ResetTargetArg,
        /// Force offline file reset without contacting a running instance
        #[arg(long)]
        offline: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ResetTargetArg {
    Layout,
    Settings,
    All,
}

#[derive(Clone, Copy, ValueEnum)]
enum QueryArg {
    State,
    Meta,
    Status,
    Tasks,
    UiTree,
    Schema,
    Clipboard,
}

#[derive(Subcommand)]
enum DebugSub {
    /// Tail structured JSONL logs
    Logs {
        #[arg(long, default_value_t = 100)]
        last: usize,
        #[arg(long)]
        request_id: Option<String>,
    },
    /// Tail append-only command journal
    Journal {
        #[arg(long, default_value_t = 50)]
        last: usize,
        #[arg(long)]
        failed_only: bool,
    },
    /// Correlate journal record and tracing logs for a single request ID
    Trace {
        #[arg(long)]
        request_id: String,
    },
    /// Inspect full Core state snapshot
    State,
    /// Inspect semantic UI widget tree & presentation bounds from GUI
    UiTree,
    /// Trigger a semantic UI action on the running GUI
    UiAction { target_id: String, action: String },
    /// Capture framebuffer screenshot via the running GUI
    Screenshot {
        #[arg(long)]
        output: Option<String>,
    },
    /// Deterministically replay a command journal inside an isolated sandbox
    Replay {
        #[arg(long)]
        journal: Option<PathBuf>,
        #[arg(long)]
        sandbox_dir: Option<PathBuf>,
        #[arg(long)]
        stop_on_error: bool,
    },
    /// Export a sanitized diagnostic bundle JSON file
    Bundle {
        #[arg(long)]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let paths = AppPaths::current();
    let socket = cli
        .socket
        .clone()
        .unwrap_or_else(|| paths.socket_path.clone());

    let exit_code = match run(cli, &paths, &socket).await {
        Ok(()) => 0,
        Err(err) => {
            let payload = serde_json::json!({ "ok": false, "error": err });
            eprintln!("{}", serde_json::to_string_pretty(&payload).unwrap_or(err));
            1
        }
    };
    std::process::exit(exit_code);
}

async fn run(cli: Cli, paths: &AppPaths, socket: &Path) -> Result<(), String> {
    match cli.command {
        Sub::Status => print_query(socket, Query::Status, cli.json).await,
        Sub::Open { paths: files } => {
            dispatch_cmd(socket, Command::OpenFiles { paths: files }, None, cli.json).await
        }
        Sub::OpenFolder { path } => {
            dispatch_cmd(socket, Command::OpenFolder { path }, None, cli.json).await
        }
        Sub::Edit { path } => {
            dispatch_cmd(socket, Command::EditorOpen { path }, None, cli.json).await
        }
        Sub::Workspace { target } => {
            dispatch_cmd(
                socket,
                Command::SetWorkspace {
                    workspace: target.into(),
                },
                None,
                cli.json,
            )
            .await
        }
        Sub::Dispatch {
            payload,
            expected_revision,
        } => {
            let envelope = parse_envelope(&payload, expected_revision)?;
            let resp = send_once(socket, &IpcRequest::Dispatch { envelope }).await?;
            print_ipc_response(&resp, cli.json)
        }
        Sub::Query { kind } => {
            let q = match kind {
                QueryArg::State => Query::State,
                QueryArg::Meta => Query::Meta,
                QueryArg::Status => Query::Status,
                QueryArg::Tasks => Query::Tasks,
                QueryArg::UiTree => Query::UiTree,
                QueryArg::Schema => Query::Schema,
                QueryArg::Clipboard => Query::EditorClipboardPng,
            };
            print_query(socket, q, cli.json).await
        }
        Sub::Schema => {
            print_json(&protocol_schemas(), cli.json);
            Ok(())
        }
        Sub::Events { limit } => {
            let mut seen = 0usize;
            subscribe_stream(socket, |event| {
                println!(
                    "{}",
                    serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_owned())
                );
                seen += 1;
                limit.is_none_or(|max| seen < max)
            })
            .await
        }
        Sub::Debug { cmd } => run_debug(cmd, paths, socket, cli.json).await,
        Sub::Mcp => mcp::serve_stdio(socket.to_path_buf()).await,
        Sub::Reset { target, offline } => {
            let (reset_layout, reset_settings) = match target {
                ResetTargetArg::Layout => (true, false),
                ResetTargetArg::Settings => (false, true),
                ResetTargetArg::All => (true, true),
            };
            if !offline && socket.exists() {
                let env = Envelope::new(
                    format!("cli-{}", std::process::id()),
                    ClientKind::Cli,
                    Command::ResetConfig {
                        reset_layout,
                        reset_settings,
                    },
                );
                if let Ok(resp) = send_once(socket, &IpcRequest::Dispatch { envelope: env }).await {
                    return print_ipc_response(&resp, cli.json);
                }
            }
            let mut notes = Vec::new();
            if reset_settings {
                let (_, note) = Config::reset_settings_only(&paths.config_file());
                if let Some(n) = note {
                    notes.push(n);
                }
            }
            if reset_layout {
                let (_, note) =
                    UiState::reset_layout_only(&paths.ui_state_file(), &paths.config_file());
                if let Some(n) = note {
                    notes.push(n);
                }
            }
            print_json(
                &serde_json::json!({ "ok": true, "offline": true, "notes": notes }),
                cli.json,
            );
            Ok(())
        }
    }
}

async fn run_debug(
    cmd: DebugSub,
    paths: &AppPaths,
    socket: &Path,
    pretty_flag: bool,
) -> Result<(), String> {
    match cmd {
        DebugSub::Logs { last, request_id } => {
            let lines = read_log_lines(&paths.log_dir, Some(last), request_id.as_deref())
                .map_err(|e| format!("failed to read logs in {}: {e}", paths.log_dir.display()))?;
            print_json(&serde_json::Value::Array(lines), pretty_flag);
            Ok(())
        }
        DebugSub::Journal { last, failed_only } => {
            let mut records = Journal::read_records(&paths.journal_file(), None)
                .map_err(|e| format!("failed to read journal: {e}"))?;
            if failed_only {
                records.retain(|r| !r.ok);
            }
            if records.len() > last {
                records = records.split_off(records.len() - last);
            }
            print_json(
                &serde_json::to_value(records).unwrap_or_default(),
                pretty_flag,
            );
            Ok(())
        }
        DebugSub::Trace { request_id } => {
            let records = Journal::read_records(&paths.journal_file(), None)
                .map_err(|e| format!("failed to read journal: {e}"))?;
            let journal_hits: Vec<_> = records.into_iter().filter(|r| r.id == request_id).collect();
            let log_hits = read_log_lines(&paths.log_dir, None, Some(&request_id))
                .map_err(|e| format!("failed to read logs: {e}"))?;
            let trace = serde_json::json!({
                "requestId": request_id,
                "journal": journal_hits,
                "logs": log_hits,
            });
            print_json(&trace, pretty_flag);
            Ok(())
        }
        DebugSub::State => print_query(socket, Query::State, pretty_flag).await,
        DebugSub::UiTree => print_query(socket, Query::UiTree, pretty_flag).await,
        DebugSub::UiAction { target_id, action } => {
            dispatch_cmd(
                socket,
                Command::DebugUiAction { target_id, action },
                None,
                pretty_flag,
            )
            .await
        }
        DebugSub::Screenshot { output } => {
            let req_id = next_req_id("shot");
            let env = Envelope::new(
                req_id.clone(),
                ClientKind::Cli,
                Command::DebugScreenshot {
                    output_path: output,
                },
            );
            let resp = send_once(socket, &IpcRequest::Dispatch { envelope: env }).await?;
            // Wait briefly for ScreenshotCaptured event if GUI is attached.
            if let IpcResponse::Command { response } = &resp
                && response.ok
            {
                let mut captured: Option<serde_json::Value> = None;
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    subscribe_stream(socket, |event| {
                        if let EventKind::ScreenshotCaptured {
                            request_id,
                            path,
                            width,
                            height,
                        } = event.kind
                            && request_id == req_id
                        {
                            captured = Some(serde_json::json!({
                                "ok": true,
                                "requestId": request_id,
                                "revision": event.revision,
                                "path": path,
                                "width": width,
                                "height": height,
                            }));
                            return false;
                        }
                        true
                    }),
                )
                .await;
                if let Some(val) = captured {
                    print_json(&val, pretty_flag);
                    return Ok(());
                }
            }
            print_ipc_response(&resp, pretty_flag)
        }
        DebugSub::Replay {
            journal,
            sandbox_dir,
            stop_on_error,
        } => {
            let journal_path = journal.unwrap_or_else(|| paths.journal_file());
            let sandbox = sandbox_dir.unwrap_or_else(|| {
                std::env::temp_dir().join(format!(
                    "photon-replay-{}-{}",
                    std::process::id(),
                    now_nanos()
                ))
            });
            let report = replay_journal_in_sandbox(&journal_path, &sandbox, stop_on_error)?;
            print_json(
                &serde_json::to_value(&report).unwrap_or_default(),
                pretty_flag,
            );
            if report.divergences > 0 {
                return Err(format!(
                    "replay detected {} divergence(s)",
                    report.divergences
                ));
            }
            Ok(())
        }
        DebugSub::Bundle { output } => {
            let journal =
                Journal::read_records(&paths.journal_file(), Some(200)).unwrap_or_default();
            let logs = read_log_lines(&paths.log_dir, Some(300), None).unwrap_or_default();
            let state = send_once(
                socket,
                &IpcRequest::Query {
                    query: Query::State,
                },
            )
            .await
            .ok();
            let ui_tree = send_once(
                socket,
                &IpcRequest::Query {
                    query: Query::UiTree,
                },
            )
            .await
            .ok();
            let bundle = serde_json::json!({
                "generatedAtMs": now_nanos() / 1_000_000,
                "stateDir": paths.state_dir,
                "journal": journal,
                "logs": logs,
                "liveState": state,
                "liveUiTree": ui_tree,
            });
            if let Some(parent) = output.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let bytes = serde_json::to_vec_pretty(&bundle).map_err(|e| e.to_string())?;
            std::fs::write(&output, bytes).map_err(|e| e.to_string())?;
            print_json(
                &serde_json::json!({ "ok": true, "bundle": output }),
                pretty_flag,
            );
            Ok(())
        }
    }
}

async fn dispatch_cmd(
    socket: &Path,
    command: Command,
    expected_revision: Option<u64>,
    json_flag: bool,
) -> Result<(), String> {
    let mut env = Envelope::new(next_req_id("cli"), ClientKind::Cli, command);
    env.expected_revision = expected_revision;
    let resp = send_once(socket, &IpcRequest::Dispatch { envelope: env }).await?;
    print_ipc_response(&resp, json_flag)
}

async fn print_query(socket: &Path, query: Query, json_flag: bool) -> Result<(), String> {
    let resp = send_once(socket, &IpcRequest::Query { query }).await?;
    print_ipc_response(&resp, json_flag)
}

fn print_ipc_response(resp: &IpcResponse, json_flag: bool) -> Result<(), String> {
    match resp {
        IpcResponse::Command { response } => {
            print_json(
                &serde_json::to_value(response).unwrap_or_default(),
                json_flag,
            );
            if response.ok {
                Ok(())
            } else {
                Err(response
                    .error
                    .as_ref()
                    .map(|e| format!("{}: {}", e.code, e.message))
                    .unwrap_or_else(|| "command failed".to_owned()))
            }
        }
        IpcResponse::Query { data } => {
            print_json(data, json_flag);
            Ok(())
        }
        IpcResponse::Event { event } => {
            print_json(&serde_json::to_value(event).unwrap_or_default(), json_flag);
            Ok(())
        }
        IpcResponse::Error { error } => {
            print_json(&serde_json::to_value(error).unwrap_or_default(), json_flag);
            Err(format!("{}: {}", error.code, error.message))
        }
    }
}

fn print_json(value: &serde_json::Value, compact: bool) {
    let out = if compact {
        serde_json::to_string(value)
    } else {
        serde_json::to_string_pretty(value)
    };
    println!("{}", out.unwrap_or_else(|_| "{}".to_owned()));
}

fn parse_envelope(raw: &str, expected_revision: Option<u64>) -> Result<Envelope, String> {
    if let Ok(mut env) = serde_json::from_str::<Envelope>(raw) {
        if expected_revision.is_some() {
            env.expected_revision = expected_revision;
        }
        return Ok(env);
    }
    let command: Command =
        serde_json::from_str(raw).map_err(|e| format!("invalid Command or Envelope JSON: {e}"))?;
    let mut env = Envelope::new(next_req_id("cli"), ClientKind::Cli, command);
    env.protocol = PROTOCOL_VERSION;
    env.expected_revision = expected_revision;
    Ok(env)
}

fn next_req_id(prefix: &str) -> String {
    format!("{prefix}-{}-{}", std::process::id(), now_nanos())
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn clap_cli_has_no_arg_collisions() {
        Cli::command().debug_assert();
    }
}
