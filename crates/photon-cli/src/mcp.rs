//! Minimal MCP (Model Context Protocol) JSON-RPC 2.0 server over stdio.
//!
//! Exposes Photon commands, queries, UI tree inspection, screenshot capture,
//! and protocol schemas to external AI coding/automation agents.

use std::path::{Path, PathBuf};

use photon_ipc::{IpcRequest, IpcResponse, send_once};
use photon_protocol::{ClientKind, Command, Envelope, Query, protocol_schemas};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub async fn serve_stdio(socket_path: PathBuf) -> Result<(), String> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut lines = BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": { "code": -32700, "message": format!("parse error: {e}") }
                });
                write_json(&mut stdout, &err).await?;
                continue;
            }
        };

        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(Value::Null);

        // Notifications without `id` do not receive a JSON-RPC response.
        if id.is_null() && method.starts_with("notifications/") {
            continue;
        }

        let response = match method {
            "initialize" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "photon-mcp", "version": env!("CARGO_PKG_VERSION") }
                }
            }),
            "tools/list" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": tool_definitions() }
            }),
            "tools/call" => handle_tool_call(&socket_path, id, params).await,
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("method not found: {method}") }
            }),
        };

        write_json(&mut stdout, &response).await?;
    }
    Ok(())
}

async fn handle_tool_call(socket_path: &Path, id: Value, params: Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = match name {
        "photon_dispatch" => {
            let cmd_val = args.get("command").cloned().unwrap_or(args.clone());
            match serde_json::from_value::<Command>(cmd_val) {
                Ok(cmd) => {
                    let mut env =
                        Envelope::new(format!("mcp-{}", std::process::id()), ClientKind::Mcp, cmd);
                    env.expected_revision = args.get("expected_revision").and_then(Value::as_u64);
                    send_once(socket_path, &IpcRequest::Dispatch { envelope: env }).await
                }
                Err(e) => Err(format!("invalid command payload: {e}")),
            }
        }
        "photon_query_state" => {
            send_once(
                socket_path,
                &IpcRequest::Query {
                    query: Query::State,
                },
            )
            .await
        }
        "photon_query_ui_tree" => {
            send_once(
                socket_path,
                &IpcRequest::Query {
                    query: Query::UiTree,
                },
            )
            .await
        }
        "photon_query_status" => {
            send_once(
                socket_path,
                &IpcRequest::Query {
                    query: Query::Status,
                },
            )
            .await
        }
        "photon_schema" => {
            return json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": protocol_schemas().to_string() }]
                }
            });
        }
        _ => Err(format!("unknown tool: {name}")),
    };

    match result {
        Ok(ipc_resp) => {
            let text = serde_json::to_string_pretty(&ipc_resp).unwrap_or_default();
            let is_error = matches!(ipc_resp, IpcResponse::Error { .. })
                || matches!(&ipc_resp, IpcResponse::Command { response } if !response.ok);
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "isError": is_error,
                    "content": [{ "type": "text", "text": text }]
                }
            })
        }
        Err(err) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "isError": true,
                "content": [{ "type": "text", "text": err }]
            }
        }),
    }
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "photon_dispatch",
            "description": "Execute any typed Photon Command through the Core gateway.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": { "type": "object", "description": "Tagged Command object, e.g. {\"type\":\"set_workspace\",\"workspace\":\"viewer\"}" },
                    "expected_revision": { "type": "integer" }
                },
                "required": ["command"]
            }
        }),
        json!({
            "name": "photon_query_state",
            "description": "Read the authoritative AppState snapshot including revision, entries, viewer, and editor.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "photon_query_ui_tree",
            "description": "Inspect the live iced semantic widget tree and presentation state.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "photon_query_status",
            "description": "Get boot_id, profile, revision, active tasks, and IPC/log paths.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "photon_schema",
            "description": "Return the complete JSON Schema for Photon Command, Query, Event, and Envelope.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
    ]
}

async fn write_json(stdout: &mut tokio::io::Stdout, value: &Value) -> Result<(), String> {
    let line = serde_json::to_string(value).map_err(|e| e.to_string())?;
    stdout
        .write_all(line.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    stdout.write_all(b"\n").await.map_err(|e| e.to_string())?;
    stdout.flush().await.map_err(|e| e.to_string())
}
