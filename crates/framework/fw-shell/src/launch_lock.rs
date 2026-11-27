//! Per-socket exclusive launch claim (`flock(LOCK_EX | LOCK_NB)`).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Exclusive file lock held across single-instance startup until the IPC socket is bound.
pub struct LaunchLock {
    #[cfg(unix)]
    _file: std::fs::File,
}

/// Companion lockfile path adjacent to `socket_path`.
pub fn launch_lock_path(socket_path: &Path) -> PathBuf {
    let name = socket_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("control");
    socket_path.with_file_name(format!("{name}.launch.lock"))
}

/// Acquire an exclusive launch lock for `socket_path` within `timeout`.
///
/// The kernel releases the lock automatically if the process exits or crashes.
pub fn acquire_launch_lock(socket_path: &Path, timeout: Duration) -> std::io::Result<LaunchLock> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::io::AsRawFd;

        let lock_path = launch_lock_path(socket_path);
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        let _ = std::fs::set_permissions(&lock_path, std::fs::Permissions::from_mode(0o600));

        let deadline = Instant::now() + timeout;
        loop {
            let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if rc == 0 {
                return Ok(LaunchLock { _file: file });
            }
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::EWOULDBLOCK)
                && err.raw_os_error() != Some(libc::EAGAIN)
            {
                return Err(err);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("timed out waiting for launch lock {}", lock_path.display()),
                ));
            }
            std::thread::sleep(remaining.min(Duration::from_millis(50)));
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (socket_path, timeout);
        Ok(LaunchLock {})
    }
}
