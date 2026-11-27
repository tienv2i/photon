use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use fw_ipc::{IpcRequest, IpcResponse, send_once};
use fw_protocol::{ClientKind, Envelope};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

pub fn print_json(value: &Value, compact: bool) {
    let out = if compact {
        serde_json::to_string(value)
    } else {
        serde_json::to_string_pretty(value)
    };
    println!("{}", out.unwrap_or_else(|_| "{}".to_owned()));
}

pub fn print_ipc_response<E: Serialize>(
    resp: &IpcResponse<E>,
    compact: bool,
) -> Result<(), String> {
    match resp {
        IpcResponse::Command { response } => {
            print_json(&serde_json::to_value(response).unwrap_or_default(), compact);
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
            print_json(data, compact);
            Ok(())
        }
        IpcResponse::Event { event } => {
            print_json(&serde_json::to_value(event).unwrap_or_default(), compact);
            Ok(())
        }
        IpcResponse::Error { error } => {
            print_json(&serde_json::to_value(error).unwrap_or_default(), compact);
            Err(format!("{}: {}", error.code, error.message))
        }
    }
}

pub fn next_req_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{}-{nanos}", std::process::id())
}

pub fn parse_envelope<C: DeserializeOwned>(
    raw: &str,
    protocol_version: u32,
    expected_revision: Option<u64>,
) -> Result<Envelope<C>, String> {
    if let Ok(mut env) = serde_json::from_str::<Envelope<C>>(raw) {
        if expected_revision.is_some() {
            env.expected_revision = expected_revision;
        }
        return Ok(env);
    }
    let command: C =
        serde_json::from_str(raw).map_err(|e| format!("invalid Command or Envelope JSON: {e}"))?;
    let mut env = Envelope::new(next_req_id("cli"), ClientKind::Cli, command);
    env.protocol = protocol_version;
    env.expected_revision = expected_revision;
    Ok(env)
}

pub async fn dispatch_and_print<C, Q, E>(
    socket: &Path,
    envelope: Envelope<C>,
    compact: bool,
) -> Result<(), String>
where
    C: Serialize,
    Q: Serialize,
    E: DeserializeOwned + Serialize,
{
    let req: IpcRequest<C, Q> = IpcRequest::Dispatch { envelope };
    let resp: IpcResponse<E> = send_once(socket, &req).await?;
    print_ipc_response(&resp, compact)
}

pub async fn query_and_print<C, Q, E>(socket: &Path, query: Q, compact: bool) -> Result<(), String>
where
    C: Serialize,
    Q: Serialize,
    E: DeserializeOwned + Serialize,
{
    let req: IpcRequest<C, Q> = IpcRequest::Query { query };
    let resp: IpcResponse<E> = send_once(socket, &req).await?;
    print_ipc_response(&resp, compact)
}
