//! Photon persistence, runtime identity, XDG paths, command journal, and structured logging.

pub mod config;
pub mod identity;
pub mod journal;
pub mod logging;
pub mod paths;

pub use config::{Config, UiState};
pub use fw_storage::store;
pub use identity::{
    AppBranding, AppIdentity, PHOTON_BRANDING, Profile, current_identity, identity_for_profile,
};
pub use journal::{Journal, JournalRecord};
pub use paths::AppPaths;
