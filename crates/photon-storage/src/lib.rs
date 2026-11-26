//! Persistence, runtime identity, XDG paths, command journal, and structured logging.

pub mod config;
pub mod identity;
pub mod journal;
pub mod logging;
pub mod paths;

pub use config::{Config, UiState};
pub use identity::{AppIdentity, Profile};
pub use journal::{Journal, JournalRecord};
pub use paths::AppPaths;
