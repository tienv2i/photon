//! Photon MCP JSON-RPC 2.0 server over stdio, powered by `fw-cli`.

use std::path::PathBuf;

use fw_cli::{McpServerConfig, McpToolDef, mcp_ipc_result, mcp_text_result, serve_mcp_stdio};
use photon_ipc::{IpcRequest, IpcResponse, send_once};
use photon_protocol::{ClientKind, Command, Envelope, Event, Query, protocol_schemas};
use serde_json::{Value, json};

pub async fn serve_stdio(socket_path: PathBuf) -> Result<(), String> {
    let config = McpServerConfig {
        server_name: "photon-mcp".to_owned(),
        server_version: env!("CARGO_PKG_VERSION").to_owned(),
    };
    serve_mcp_stdio(config, tool_definitions(), move |name, args| {
        let socket = socket_path.clone();
        async move { handle_tool_call(&socket, &name, args).await }
    })
    .await
}

async fn handle_tool_call(socket_path: &std::path::Path, name: &str, args: Value) -> Value {
    match name {
        "photon_dispatch" => {
            let cmd_val = args.get("command").cloned().unwrap_or(args.clone());
            match serde_json::from_value::<Command>(cmd_val) {
                Ok(cmd) => {
                    let mut env =
                        Envelope::new(format!("mcp-{}", std::process::id()), ClientKind::Mcp, cmd);
                    env.expected_revision = args.get("expected_revision").and_then(Value::as_u64);
                    let res: Result<IpcResponse, String> =
                        send_once(socket_path, &IpcRequest::Dispatch { envelope: env }).await;
                    mcp_ipc_result(res)
                }
                Err(e) => mcp_text_result(format!("invalid command payload: {e}"), true),
            }
        }
        "photon_query_state" => {
            let res: Result<IpcResponse, String> = send_once(
                socket_path,
                &IpcRequest::Query {
                    query: Query::State,
                },
            )
            .await;
            mcp_ipc_result(res)
        }
        "photon_query_ui_tree" => {
            let res: Result<IpcResponse, String> = send_once(
                socket_path,
                &IpcRequest::Query {
                    query: Query::UiTree,
                },
            )
            .await;
            mcp_ipc_result(res)
        }
        "photon_query_status" => {
            let res: Result<IpcResponse, String> = send_once(
                socket_path,
                &IpcRequest::Query {
                    query: Query::Status,
                },
            )
            .await;
            mcp_ipc_result(res)
        }
        "photon_schema" => mcp_text_result(protocol_schemas().to_string(), false),
        _ => {
            let _: Option<Event> = None;
            mcp_text_result(format!("unknown tool: {name}"), true)
        }
    }
}

fn tool_definitions() -> Vec<McpToolDef> {
    vec![
        McpToolDef {
            name: "photon_dispatch".to_owned(),
            description: "Execute any typed Photon Command through the Core gateway.".to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "object", "description": "Tagged Command object, e.g. {\"type\":\"set_workspace\",\"workspace\":\"viewer\"}" },
                    "expected_revision": { "type": "integer" }
                },
                "required": ["command"]
            }),
        },
        McpToolDef {
            name: "photon_query_state".to_owned(),
            description: "Read the authoritative AppState snapshot including revision, entries, viewer, and editor.".to_owned(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        McpToolDef {
            name: "photon_query_ui_tree".to_owned(),
            description: "Inspect the live iced semantic widget tree and presentation state.".to_owned(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        McpToolDef {
            name: "photon_query_status".to_owned(),
            description: "Get boot_id, profile, revision, active tasks, and IPC/log paths.".to_owned(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        McpToolDef {
            name: "photon_schema".to_owned(),
            description: "Return the complete JSON Schema for Photon Command, Query, Event, and Envelope.".to_owned(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
    ]
}
