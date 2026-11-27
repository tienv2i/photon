use std::path::PathBuf;

use clap::{Parser, Subcommand};
use fw_cli::{
    McpServerConfig, McpToolDef, dispatch_and_print, mcp_ipc_result, mcp_text_result, next_req_id,
    parse_envelope, query_and_print, serve_mcp_stdio,
};
use fw_ipc::{IpcRequest, IpcResponse, send_once};
use fw_protocol::{ClientKind, Envelope};
use notes_app::{NoteCommand, NoteEvent, NoteQuery, notes_paths};
use serde_json::json;

#[derive(Parser)]
#[command(
    name = "notesctl",
    version,
    about = "Control Notes via fw-ipc & fw-cli"
)]
struct Cli {
    #[arg(long, global = true)]
    socket: Option<PathBuf>,
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    sub: Sub,
}

#[derive(Subcommand)]
enum Sub {
    Status,
    State,
    Create { title: String, body: String },
    Dispatch { payload: String },
    Mcp,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let socket = cli.socket.unwrap_or_else(|| notes_paths().socket_path);
    let res = match cli.sub {
        Sub::Status => {
            query_and_print::<NoteCommand, NoteQuery, NoteEvent>(
                &socket,
                NoteQuery::Status,
                cli.json,
            )
            .await
        }
        Sub::State => {
            query_and_print::<NoteCommand, NoteQuery, NoteEvent>(
                &socket,
                NoteQuery::State,
                cli.json,
            )
            .await
        }
        Sub::Create { title, body } => {
            let env = Envelope::new(
                next_req_id("notesctl"),
                ClientKind::Cli,
                NoteCommand::CreateNote { title, body },
            );
            dispatch_and_print::<NoteCommand, NoteQuery, NoteEvent>(&socket, env, cli.json).await
        }
        Sub::Dispatch { payload } => match parse_envelope::<NoteCommand>(&payload, 1, None) {
            Ok(env) => {
                dispatch_and_print::<NoteCommand, NoteQuery, NoteEvent>(&socket, env, cli.json)
                    .await
            }
            Err(e) => Err(e),
        },
        Sub::Mcp => {
            let tools = vec![
                McpToolDef {
                    name: "notes_dispatch".to_owned(),
                    description: "Dispatch a NoteCommand to NotesCore".to_owned(),
                    input_schema: json!({
                        "type": "object",
                        "properties": { "command": { "type": "object" } },
                        "required": ["command"]
                    }),
                },
                McpToolDef {
                    name: "notes_query_state".to_owned(),
                    description: "Query NotesState".to_owned(),
                    input_schema: json!({ "type": "object", "properties": {} }),
                },
            ];
            let cfg = McpServerConfig {
                server_name: "notes-mcp".to_owned(),
                server_version: env!("CARGO_PKG_VERSION").to_owned(),
            };
            serve_mcp_stdio(cfg, tools, move |name, args| {
                let s = socket.clone();
                async move {
                    match name.as_str() {
                        "notes_dispatch" => {
                            let cmd_val = args.get("command").cloned().unwrap_or(args);
                            match serde_json::from_value::<NoteCommand>(cmd_val) {
                                Ok(cmd) => {
                                    let env =
                                        Envelope::new(next_req_id("mcp"), ClientKind::Mcp, cmd);
                                    let req = IpcRequest::<NoteCommand, NoteQuery>::Dispatch {
                                        envelope: env,
                                    };
                                    let r: Result<IpcResponse<NoteEvent>, String> =
                                        send_once(&s, &req).await;
                                    mcp_ipc_result(r)
                                }
                                Err(e) => mcp_text_result(e.to_string(), true),
                            }
                        }
                        "notes_query_state" => {
                            let req = IpcRequest::<NoteCommand, NoteQuery>::Query {
                                query: NoteQuery::State,
                            };
                            let r: Result<IpcResponse<NoteEvent>, String> =
                                send_once(&s, &req).await;
                            mcp_ipc_result(r)
                        }
                        _ => mcp_text_result(format!("unknown tool: {name}"), true),
                    }
                }
            })
            .await
        }
    };
    if let Err(err) = res {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
