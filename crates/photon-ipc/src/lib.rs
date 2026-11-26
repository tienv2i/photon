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

pub mod viewer_registry {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    use photon_storage::AppPaths;
    use serde::{Deserialize, Serialize};

    const STALE_LOCK_AGE: Duration = Duration::from_secs(10);

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ViewerInstanceInfo {
        pub pid: u32,
        pub socket_path: PathBuf,
        pub active_canonical_path: PathBuf,
        #[serde(default)]
        pub open_canonical_paths: Vec<PathBuf>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ViewerMatch {
        pub info: ViewerInstanceInfo,
        pub is_active_path: bool,
    }

    pub struct ViewerRegistrationGuard {
        json_path: PathBuf,
        socket_path: PathBuf,
    }

    impl ViewerRegistrationGuard {
        pub fn json_path(&self) -> &Path {
            &self.json_path
        }

        pub fn socket_path(&self) -> &Path {
            &self.socket_path
        }
    }

    impl Drop for ViewerRegistrationGuard {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.json_path);
            let _ = fs::remove_file(&self.socket_path);
        }
    }

    pub struct CanonicalPathLock {
        lock_path: PathBuf,
    }

    impl Drop for CanonicalPathLock {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.lock_path);
        }
    }

    pub fn viewer_socket_path(paths: &AppPaths, pid: u32) -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            let _ = paths;
            PathBuf::from(format!(r"\\.\pipe\photon-viewer-{pid}"))
        }
        #[cfg(not(target_os = "windows"))]
        {
            paths.viewers_dir().join(format!("v-{pid}.sock"))
        }
    }

    pub fn viewer_registration_path(paths: &AppPaths, pid: u32) -> PathBuf {
        paths.viewers_dir().join(format!("v-{pid}.json"))
    }

    pub fn register_viewer_instance(
        paths: &AppPaths,
        info: &ViewerInstanceInfo,
    ) -> std::io::Result<ViewerRegistrationGuard> {
        update_viewer_instance(paths, info)?;
        Ok(ViewerRegistrationGuard {
            json_path: viewer_registration_path(paths, info.pid),
            socket_path: info.socket_path.clone(),
        })
    }

    pub fn update_viewer_instance(
        paths: &AppPaths,
        info: &ViewerInstanceInfo,
    ) -> std::io::Result<()> {
        paths.ensure_viewers_dir()?;
        let final_path = viewer_registration_path(paths, info.pid);
        let tmp_path = paths.viewers_dir().join(format!("v-{}.json.tmp", info.pid));
        let bytes =
            serde_json::to_vec_pretty(info).map_err(|e| std::io::Error::other(e.to_string()))?;
        {
            let mut f = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
            }
            f.write_all(&bytes)?;
            f.write_all(b"\n")?;
            f.sync_all()?;
        }
        fs::rename(&tmp_path, &final_path)?;
        Ok(())
    }

    fn fnv1a_64(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in bytes {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        hash
    }

    pub fn acquire_viewer_path_lock(
        paths: &AppPaths,
        canonical: &Path,
        timeout: Duration,
    ) -> std::io::Result<CanonicalPathLock> {
        let viewers_dir = paths.ensure_viewers_dir()?;
        let hash = fnv1a_64(canonical.to_string_lossy().as_bytes());
        let lock_path = viewers_dir.join(format!("lock-{hash:016x}.lock"));
        let deadline = SystemTime::now() + timeout;

        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(mut f) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o600));
                    }
                    let _ = writeln!(f, "{}", std::process::id());
                    return Ok(CanonicalPathLock { lock_path });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if let Ok(meta) = fs::metadata(&lock_path)
                        && let Ok(modified) = meta.modified()
                        && let Ok(age) = SystemTime::now().duration_since(modified)
                        && age > STALE_LOCK_AGE
                    {
                        let _ = fs::remove_file(&lock_path);
                        continue;
                    }
                    if SystemTime::now() >= deadline {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            format!("timed out waiting for lock {}", lock_path.display()),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(15));
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn is_socket_alive(socket_path: &Path) -> bool {
        #[cfg(unix)]
        {
            std::os::unix::net::UnixStream::connect(socket_path).is_ok()
        }
        #[cfg(not(unix))]
        {
            let _ = socket_path;
            false
        }
    }

    pub fn find_live_viewer_for_path(paths: &AppPaths, canonical: &Path) -> Option<ViewerMatch> {
        let dir = paths.viewers_dir();
        let rd = fs::read_dir(&dir).ok()?;
        let mut fallback: Option<ViewerMatch> = None;

        for entry in rd.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with("v-") || !name.ends_with(".json") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(info) = serde_json::from_str::<ViewerInstanceInfo>(&text) else {
                continue;
            };
            if !is_socket_alive(&info.socket_path) {
                let _ = fs::remove_file(&path);
                let _ = fs::remove_file(&info.socket_path);
                continue;
            }
            if info.active_canonical_path == canonical {
                return Some(ViewerMatch {
                    info,
                    is_active_path: true,
                });
            }
            if fallback.is_none() && info.open_canonical_paths.iter().any(|p| p == canonical) {
                fallback = Some(ViewerMatch {
                    info,
                    is_active_path: false,
                });
            }
        }

        fallback
    }
}

pub use viewer_registry::{
    CanonicalPathLock, ViewerInstanceInfo, ViewerMatch, ViewerRegistrationGuard,
    acquire_viewer_path_lock, find_live_viewer_for_path, register_viewer_instance,
    update_viewer_instance, viewer_registration_path, viewer_socket_path,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;

    use photon_storage::AppPaths;

    fn temp_sandbox(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("photon-ipc-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn registration_guard_writes_atomically_and_cleans_up_on_drop() {
        let root = temp_sandbox("reg-guard");
        let paths = AppPaths::for_sandbox(&root);
        let sock = viewer_socket_path(&paths, 4242);
        let info = ViewerInstanceInfo {
            pid: 4242,
            socket_path: sock.clone(),
            active_canonical_path: PathBuf::from("/tmp/a.png"),
            open_canonical_paths: vec![PathBuf::from("/tmp/a.png")],
        };

        let json_path;
        {
            let guard = register_viewer_instance(&paths, &info).unwrap();
            json_path = guard.json_path().to_path_buf();
            assert!(json_path.is_file());
            let parsed: ViewerInstanceInfo =
                serde_json::from_str(&fs::read_to_string(&json_path).unwrap()).unwrap();
            assert_eq!(parsed, info);
            fs::write(&sock, b"").unwrap();
            assert!(sock.exists());
        }

        assert!(!json_path.exists());
        assert!(!sock.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_live_viewer_cleans_up_stale_dead_entries() {
        let root = temp_sandbox("stale-cleanup");
        let paths = AppPaths::for_sandbox(&root);
        let dead_sock = viewer_socket_path(&paths, 999_001);
        let dead_info = ViewerInstanceInfo {
            pid: 999_001,
            socket_path: dead_sock.clone(),
            active_canonical_path: PathBuf::from("/tmp/dead.png"),
            open_canonical_paths: vec![PathBuf::from("/tmp/dead.png")],
        };
        update_viewer_instance(&paths, &dead_info).unwrap();
        fs::write(&dead_sock, b"stale").unwrap();

        let matched = find_live_viewer_for_path(&paths, Path::new("/tmp/dead.png"));
        assert!(matched.is_none());
        assert!(!viewer_registration_path(&paths, 999_001).exists());
        assert!(!dead_sock.exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn find_live_viewer_prefers_active_path_over_open_list() {
        use std::os::unix::net::UnixListener;

        let root = temp_sandbox("prefer-active");
        let paths = AppPaths::for_sandbox(&root);
        paths.ensure_viewers_dir().unwrap();

        let sock1 = viewer_socket_path(&paths, 1001);
        let _l1 = UnixListener::bind(&sock1).unwrap();
        let info1 = ViewerInstanceInfo {
            pid: 1001,
            socket_path: sock1.clone(),
            active_canonical_path: PathBuf::from("/tmp/other.png"),
            open_canonical_paths: vec![
                PathBuf::from("/tmp/other.png"),
                PathBuf::from("/tmp/target.png"),
            ],
        };
        let _g1 = register_viewer_instance(&paths, &info1).unwrap();

        let matched_open = find_live_viewer_for_path(&paths, Path::new("/tmp/target.png")).unwrap();
        assert_eq!(matched_open.info.pid, 1001);
        assert!(!matched_open.is_active_path);

        let sock2 = viewer_socket_path(&paths, 1002);
        let _l2 = UnixListener::bind(&sock2).unwrap();
        let info2 = ViewerInstanceInfo {
            pid: 1002,
            socket_path: sock2.clone(),
            active_canonical_path: PathBuf::from("/tmp/target.png"),
            open_canonical_paths: vec![PathBuf::from("/tmp/target.png")],
        };
        let _g2 = register_viewer_instance(&paths, &info2).unwrap();

        let matched_active =
            find_live_viewer_for_path(&paths, Path::new("/tmp/target.png")).unwrap();
        assert_eq!(matched_active.info.pid, 1002);
        assert!(matched_active.is_active_path);

        drop(_g2);
        drop(_g1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn canonical_path_lock_blocks_concurrent_acquire_until_released() {
        let root = temp_sandbox("path-lock");
        let paths = AppPaths::for_sandbox(&root);
        let target = Path::new("/tmp/lock-target.png");

        let lock = acquire_viewer_path_lock(&paths, target, Duration::from_millis(200)).unwrap();
        let err = acquire_viewer_path_lock(&paths, target, Duration::from_millis(45))
            .err()
            .expect("second lock should time out while first is held");
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);

        drop(lock);
        let reacquired = acquire_viewer_path_lock(&paths, target, Duration::from_millis(200));
        assert!(reacquired.is_ok());

        drop(reacquired);
        let _ = fs::remove_dir_all(&root);
    }
}
