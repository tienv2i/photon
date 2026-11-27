//! Domain-agnostic desktop shell utilities: single-instance launch locks
//! and platform window activation.

pub mod launch_lock;
pub mod window_activation;

pub use launch_lock::{LaunchLock, acquire_launch_lock, launch_lock_path};
pub use window_activation::{kwin_activation_script, request_platform_activation};
