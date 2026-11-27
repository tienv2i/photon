use std::future::Future;

use fw_ipc::IpcResponse;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Clone, Serialize)]
pub struct McpToolDef {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub server_name: String,
    pub server_version: String,
}

pub fn mcp_text_result(text: String, is_error: bool) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    })
}

pub fn mcp_ipc_result<E: Serialize>(res: Result<IpcResponse<E>, String>) -> Value {
    match res {
        Ok(resp) => {
            let text = serde_json::to_string_pretty(&resp).unwrap_or_default();
            let is_error = matches!(resp, IpcResponse::Error { .. })
                || matches!(&resp, IpcResponse::Command { response } if !response.ok);
            mcp_text_result(text, is_error)
        }
        Err(err) => mcp_text_result(err, true),
    }
}

pub async fn serve_mcp_stdio<F, Fut>(
    config: McpServerConfig,
    tools: Vec<McpToolDef>,
    mut on_tool_call: F,
) -> Result<(), String>
where
    F: FnMut(String, Value) -> Fut,
    Fut: Future<Output = Value>,
{
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
                    "serverInfo": {
                        "name": config.server_name,
                        "version": config.server_version
                    }
                }
            }),
            "tools/list" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": tools }
            }),
            "tools/call" => {
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                let result = on_tool_call(name, args).await;
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": result,
                })
            }
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

async fn write_json(stdout: &mut tokio::io::Stdout, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    stdout.write_all(&bytes).await.map_err(|e| e.to_string())?;
    stdout.flush().await.map_err(|e| e.to_string())
}
