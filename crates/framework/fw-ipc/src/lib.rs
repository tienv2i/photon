//! Domain-agnostic local IPC server & client (Unix domain socket on Unix,
//! named pipe stub on Windows).
//!
//! Speaks NDJSON lines for `IpcRequest<C, Q>` (`Dispatch`, `Query`, `Subscribe`)
//! and `IpcResponse<E>` (`Command`, `Query`, `Event`, `Error`).

use std::path::Path;

use fw_protocol::{CommandResponse, Envelope, ProtocolError};
use fw_runtime::{AppCore, CoreHandle};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum IpcRequest<C, Q> {
    Dispatch { envelope: Envelope<C> },
    Query { query: Q },
    Subscribe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IpcResponse<E> {
    Command { response: CommandResponse },
    Query { data: serde_json::Value },
    Event { event: E },
    Error { error: ProtocolError },
}

#[cfg(unix)]
pub mod transport {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::{UnixListener, UnixStream};

    /// Bind the Unix listener without serving it yet.
    ///
    /// Callers that must win a single-instance race use this to claim the socket
    /// *before* doing expensive initialisation. A pre-existing path is only
    /// reclaimed when no process answers on it.
    pub async fn bind_socket(socket_path: &Path) -> std::io::Result<UnixListener> {
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if socket_path.exists() {
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
        Ok(listener)
    }

    /// Serve an already-bound listener. The socket path is already claimed.
    pub async fn serve_bound<A>(listener: UnixListener, core: CoreHandle<A>) -> std::io::Result<()>
    where
        A: AppCore,
        A::Command: DeserializeOwned,
        A::Query: DeserializeOwned,
        A::Event: Serialize,
    {
        if let Ok(addr) = listener.local_addr()
            && let Some(path) = addr.as_pathname()
        {
            tracing::info!(target: "fw::ipc", socket = %path.display(), "local IPC listening");
        }

        loop {
            let (stream, _) = listener.accept().await?;
            let handle = core.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_client(stream, handle).await {
                    tracing::debug!(target: "fw::ipc", %err, "IPC client disconnected");
                }
            });
        }
    }

    /// Bind and serve in one call.
    pub async fn serve<A>(socket_path: &Path, core: CoreHandle<A>) -> std::io::Result<()>
    where
        A: AppCore,
        A::Command: DeserializeOwned,
        A::Query: DeserializeOwned,
        A::Event: Serialize,
    {
        let listener = bind_socket(socket_path).await?;
        serve_bound(listener, core).await
    }

    async fn handle_client<A>(stream: UnixStream, core: CoreHandle<A>) -> std::io::Result<()>
    where
        A: AppCore,
        A::Command: DeserializeOwned,
        A::Query: DeserializeOwned,
        A::Event: Serialize,
    {
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            let req: IpcRequest<A::Command, A::Query> = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    let msg: IpcResponse<A::Event> = IpcResponse::Error {
                        error: ProtocolError::new("invalid_ipc_request", e.to_string()),
                    };
                    write_line(&mut writer, &msg).await?;
                    continue;
                }
            };

            match req {
                IpcRequest::Dispatch { envelope } => {
                    let resp: IpcResponse<A::Event> = match core.dispatch(envelope).await {
                        Ok(r) => IpcResponse::Command { response: r },
                        Err(e) => IpcResponse::Error {
                            error: ProtocolError::new("runtime_error", e),
                        },
                    };
                    write_line(&mut writer, &resp).await?;
                }
                IpcRequest::Query { query } => {
                    let resp: IpcResponse<A::Event> = match core.query(query).await {
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

    pub async fn send_once<C: Serialize, Q: Serialize, E: DeserializeOwned>(
        socket_path: &Path,
        request: &IpcRequest<C, Q>,
    ) -> Result<IpcResponse<E>, String> {
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

    /// Bounded variant of [`send_once`] for launch-time forwarding.
    pub async fn send_once_timeout<C: Serialize, Q: Serialize, E: DeserializeOwned>(
        socket_path: &Path,
        request: &IpcRequest<C, Q>,
        connect_timeout: std::time::Duration,
        response_timeout: std::time::Duration,
    ) -> Result<IpcResponse<E>, String> {
        let stream = tokio::time::timeout(connect_timeout, UnixStream::connect(socket_path))
            .await
            .map_err(|_| {
                format!(
                    "timed out connecting to {} after {connect_timeout:?}",
                    socket_path.display()
                )
            })?
            .map_err(|e| format!("cannot connect to {}: {e}", socket_path.display()))?;
        let (reader, mut writer) = stream.into_split();

        let exchange = async {
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
        };

        tokio::time::timeout(response_timeout, exchange)
            .await
            .map_err(|_| {
                format!(
                    "timed out waiting for {} after {response_timeout:?}",
                    socket_path.display()
                )
            })?
    }

    pub async fn subscribe_stream<E: DeserializeOwned>(
        socket_path: &Path,
        mut on_event: impl FnMut(E) -> bool,
    ) -> Result<(), String> {
        let stream = UnixStream::connect(socket_path)
            .await
            .map_err(|e| format!("cannot connect to {}: {e}", socket_path.display()))?;
        let (reader, mut writer) = stream.into_split();
        write_line(&mut writer, &IpcRequest::<(), ()>::Subscribe)
            .await
            .map_err(|e| format!("IPC write failed: {e}"))?;
        let mut lines = BufReader::new(reader).lines();
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| format!("IPC read failed: {e}"))?
        {
            if let Ok(IpcResponse::Event { event }) = serde_json::from_str::<IpcResponse<E>>(&line)
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

    #[derive(Debug)]
    pub struct UnixListener;

    pub async fn bind_socket(_socket_path: &Path) -> std::io::Result<UnixListener> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "named pipe transport not yet implemented on this OS",
        ))
    }
    pub async fn serve_bound<A>(
        _listener: UnixListener,
        _core: CoreHandle<A>,
    ) -> std::io::Result<()>
    where
        A: AppCore,
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "named pipe transport not yet implemented on this OS",
        ))
    }
    pub async fn serve<A>(socket_path: &Path, _core: CoreHandle<A>) -> std::io::Result<()>
    where
        A: AppCore,
    {
        let _ = socket_path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "named pipe transport not yet implemented on this OS",
        ))
    }
    pub async fn send_once<C, Q, E>(
        _socket_path: &Path,
        _request: &IpcRequest<C, Q>,
    ) -> Result<IpcResponse<E>, String> {
        Err("named pipe transport not yet implemented on this OS".to_owned())
    }
    pub async fn send_once_timeout<C, Q, E>(
        _socket_path: &Path,
        _request: &IpcRequest<C, Q>,
        _connect_timeout: std::time::Duration,
        _response_timeout: std::time::Duration,
    ) -> Result<IpcResponse<E>, String> {
        Err("named pipe transport not yet implemented on this OS".to_owned())
    }
    pub async fn subscribe_stream<E>(
        _socket_path: &Path,
        _on_event: impl FnMut(E) -> bool,
    ) -> Result<(), String> {
        Err("named pipe transport not yet implemented on this OS".to_owned())
    }
}

pub use transport::{
    bind_socket, send_once, send_once_timeout, serve, serve_bound, subscribe_stream,
};
