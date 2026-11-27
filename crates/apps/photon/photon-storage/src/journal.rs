//! Photon command journal built atop `fw-storage`.

pub use fw_storage::{Journal, now_ms, sanitize_path};

pub type JournalRecord = fw_storage::JournalRecord<photon_protocol::Command>;
