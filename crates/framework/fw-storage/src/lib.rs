//! Domain-agnostic persistence, runtime identity, XDG paths, command journal,
//! and structured JSONL logging for applications built on the framework.

pub mod identity;
pub mod journal;
pub mod logging;
pub mod paths;
pub mod store;

pub use identity::{AppBranding, AppIdentity, Profile};
pub use journal::{Journal, JournalRecord, now_ms, sanitize_path};
pub use paths::AppPaths;
