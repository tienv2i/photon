//! Secure local IPC server & client (Unix domain socket on Unix, named pipe stub on Windows).
//!
//! Speaks NDJSON lines for `IpcRequest` (`Dispatch`, `Query`, `Subscribe`) and
//! `IpcResponse` (`Command`, `Query`, `Event`, `Error`).

use std::path::Path;

use photon_core::CoreHandle;
use photon_protocol::{CommandResponse, Envelope, Event, ProtocolError, Query};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum IpcRequest {
    Dispatch { envelope: Envelope },
    Query { query: Query },
    Subscribe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IpcResponse {
    Command { response: CommandResponse },
    Query { data: serde_json::Value },
    Event { event: Event },
    Error { error: ProtocolError },
}

#[cfg(unix)]
pub mod transport {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::{UnixListener, UnixStream};

    pub async fn serve(socket_path: &Path, core: CoreHandle) -> std::io::Result<()> {
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if socket_path.exists() {
            // Only remove stale socket if nothing is listening on it.
            if UnixStream::connect(socket_path).await.is_ok() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    format!(
                        "another instance is already listening on {}",
                        socket_path.display()
                    ),
                ));
            }
            let _ = std::fs::remove_file(socket_path);
        }

        let listener = UnixListener::bind(socket_path)?;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
        tracing::info!(target: "photon::ipc", socket = %socket_path.display(), "local IPC listening");

        loop {
            let (stream, _) = listener.accept().await?;
            let handle = core.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_client(stream, handle).await {
                    tracing::debug!(target: "photon::ipc", %err, "IPC client disconnected");
                }
            });
        }
    }

    async fn handle_client(stream: UnixStream, core: CoreHandle) -> std::io::Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            let req: IpcRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    let msg = IpcResponse::Error {
                        error: ProtocolError::new("invalid_ipc_request", e.to_string()),
                    };
                    write_line(&mut writer, &msg).await?;
                    continue;
                }
            };

            match req {
                IpcRequest::Dispatch { envelope } => {
                    let resp = match core.dispatch(envelope).await {
                        Ok(r) => IpcResponse::Command { response: r },
                        Err(e) => IpcResponse::Error {
                            error: ProtocolError::new("runtime_error", e),
                        },
                    };
                    write_line(&mut writer, &resp).await?;
                }
                IpcRequest::Query { query } => {
                    let resp = match core.query(query).await {
                        Ok(Ok(data)) => IpcResponse::Query { data },
                        Ok(Err(err)) => IpcResponse::Error { error: err },
                        Err(e) => IpcResponse::Error {
                            error: ProtocolError::new("runtime_error", e),
                        },
                    };
                    write_line(&mut writer, &resp).await?;
                }
                IpcRequest::Subscribe => {
                    let mut rx = core.subscribe_events();
                    while let Ok(event) = rx.recv().await {
                        write_line(&mut writer, &IpcResponse::Event { event }).await?;
                    }
                    break;
                }
            }
        }
        Ok(())
    }

    pub async fn send_once(
        socket_path: &Path,
        request: &IpcRequest,
    ) -> Result<IpcResponse, String> {
        let stream = UnixStream::connect(socket_path)
            .await
            .map_err(|e| format!("cannot connect to {}: {e}", socket_path.display()))?;
        let (reader, mut writer) = stream.into_split();
        write_line(&mut writer, request)
            .await
            .map_err(|e| format!("IPC write failed: {e}"))?;
        let mut lines = BufReader::new(reader).lines();
        let line = lines
            .next_line()
            .await
            .map_err(|e| format!("IPC read failed: {e}"))?
            .ok_or_else(|| "IPC closed without response".to_owned())?;
        serde_json::from_str(&line).map_err(|e| format!("invalid IPC response: {e}"))
    }

    pub async fn subscribe_stream(
        socket_path: &Path,
        mut on_event: impl FnMut(Event) -> bool,
    ) -> Result<(), String> {
        let stream = UnixStream::connect(socket_path)
            .await
            .map_err(|e| format!("cannot connect to {}: {e}", socket_path.display()))?;
        let (reader, mut writer) = stream.into_split();
        write_line(&mut writer, &IpcRequest::Subscribe)
            .await
            .map_err(|e| format!("IPC write failed: {e}"))?;
        let mut lines = BufReader::new(reader).lines();
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| format!("IPC read failed: {e}"))?
        {
            if let Ok(IpcResponse::Event { event }) = serde_json::from_str(&line)
                && !on_event(event)
            {
                break;
            }
        }
        Ok(())
    }

    async fn write_line<W: AsyncWriteExt + Unpin, T: Serialize>(
        writer: &mut W,
        value: &T,
    ) -> std::io::Result<()> {
        let json = serde_json::to_string(value)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await
    }
}

#[cfg(not(unix))]
pub mod transport {
    use super::*;
    pub async fn serve(socket_path: &Path, _core: CoreHandle) -> std::io::Result<()> {
        let _ = socket_path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "named pipe transport not yet implemented on this OS",
        ))
    }
    pub async fn send_once(
        _socket_path: &Path,
        _request: &IpcRequest,
    ) -> Result<IpcResponse, String> {
        Err("named pipe transport not yet implemented on this OS".to_owned())
    }
    pub async fn subscribe_stream(
        _socket_path: &Path,
        _on_event: impl FnMut(Event) -> bool,
    ) -> Result<(), String> {
        Err("named pipe transport not yet implemented on this OS".to_owned())
    }
}

pub use transport::{send_once, serve, subscribe_stream};
