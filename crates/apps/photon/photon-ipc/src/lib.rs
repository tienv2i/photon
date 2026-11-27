//! Photon local IPC server & client built atop `fw-ipc`, plus multi-viewer instance registry.

use std::path::Path;

use photon_core::CoreHandle;
use photon_protocol::{Command, Event, Query};

pub type IpcRequest = fw_ipc::IpcRequest<Command, Query>;
pub type IpcResponse = fw_ipc::IpcResponse<Event>;

pub mod transport {
    use super::*;

    #[cfg(unix)]
    pub type UnixListener = tokio::net::UnixListener;
    #[cfg(not(unix))]
    pub type UnixListener = fw_ipc::transport::UnixListener;

    pub async fn bind_socket(socket_path: &Path) -> std::io::Result<UnixListener> {
        fw_ipc::transport::bind_socket(socket_path).await
    }

    pub async fn serve_bound(listener: UnixListener, core: CoreHandle) -> std::io::Result<()> {
        fw_ipc::transport::serve_bound(listener, core.inner().clone()).await
    }

    pub async fn serve(socket_path: &Path, core: CoreHandle) -> std::io::Result<()> {
        fw_ipc::transport::serve(socket_path, core.inner().clone()).await
    }

    pub async fn send_once(
        socket_path: &Path,
        request: &IpcRequest,
    ) -> Result<IpcResponse, String> {
        fw_ipc::transport::send_once(socket_path, request).await
    }

    pub async fn send_once_timeout(
        socket_path: &Path,
        request: &IpcRequest,
        connect_timeout: std::time::Duration,
        response_timeout: std::time::Duration,
    ) -> Result<IpcResponse, String> {
        fw_ipc::transport::send_once_timeout(
            socket_path,
            request,
            connect_timeout,
            response_timeout,
        )
        .await
    }

    pub async fn subscribe_stream(
        socket_path: &Path,
        on_event: impl FnMut(Event) -> bool,
    ) -> Result<(), String> {
        fw_ipc::transport::subscribe_stream(socket_path, on_event).await
    }
}

pub use transport::{
    bind_socket, send_once, send_once_timeout, serve, serve_bound, subscribe_stream,
};

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

    /// Find any live standalone viewer instance, cleaning up stale registration files.
    pub fn find_any_live_viewer(paths: &AppPaths) -> Option<ViewerInstanceInfo> {
        let dir = paths.viewers_dir();
        let rd = fs::read_dir(&dir).ok()?;
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
            return Some(info);
        }
        None
    }
}

pub use viewer_registry::{
    CanonicalPathLock, ViewerInstanceInfo, ViewerMatch, ViewerRegistrationGuard,
    acquire_viewer_path_lock, find_any_live_viewer, find_live_viewer_for_path,
    register_viewer_instance, update_viewer_instance, viewer_registration_path, viewer_socket_path,
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

    #[cfg(unix)]
    #[tokio::test]
    async fn bind_socket_twice_reports_addr_in_use() {
        let root = temp_sandbox("bind-twice");
        let sock = root.join("control.sock");

        let _first = bind_socket(&sock).await.unwrap();
        let err = bind_socket(&sock).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bind_socket_reclaims_stale_socket_file() {
        use std::os::unix::net::UnixListener as StdUnixListener;

        let root = temp_sandbox("bind-stale");
        let sock = root.join("control.sock");

        // Drop the listener without removing the path: a classic post-crash stale socket.
        let stale = StdUnixListener::bind(&sock).unwrap();
        drop(stale);
        assert!(sock.exists());

        let _listener = bind_socket(&sock).await.unwrap();
        assert!(sock.exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn send_once_timeout_returns_promptly_without_responder() {
        let root = temp_sandbox("send-timeout");
        let sock = root.join("control.sock");
        let listener = bind_socket(&sock).await.unwrap();

        // Accept the connection but never answer.
        let server = tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                tokio::time::sleep(Duration::from_secs(30)).await;
                drop(stream);
            }
        });

        let started = std::time::Instant::now();
        let err = send_once_timeout(
            &sock,
            &IpcRequest::Query {
                query: Query::State,
            },
            Duration::from_millis(200),
            Duration::from_millis(100),
        )
        .await
        .unwrap_err();
        assert!(err.contains("timed out"), "unexpected error: {err}");
        assert!(started.elapsed() < Duration::from_secs(2));

        server.abort();
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn serve_bound_answers_queries() {
        let root = temp_sandbox("serve-bound");
        let sock = root.join("control.sock");
        let listener = bind_socket(&sock).await.unwrap();

        let handle = photon_core::CoreRuntime::spawn(photon_core::Core::for_sandbox(&root));
        let server = tokio::spawn(serve_bound(listener, handle));

        let response = send_once(
            &sock,
            &IpcRequest::Query {
                query: Query::State,
            },
        )
        .await
        .expect("serve_bound should answer");
        assert!(matches!(response, IpcResponse::Query { .. }));

        server.abort();
        let _ = fs::remove_dir_all(&root);
    }
}
