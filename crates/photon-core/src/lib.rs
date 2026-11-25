//! Authoritative Core state machine, Tokio actor command gateway,
//! event bus, and deterministic sandbox replay engine.

pub mod batch;
pub mod core;
pub mod file_ops;
pub mod replay;
pub mod runtime;
pub mod translate;

pub use core::{Core, EditorPixelClipboard};
pub use replay::{ReplayReport, ReplayStepResult, replay_journal_in_sandbox};
pub use runtime::{CoreHandle, CoreRuntime, EditorTransformFrames};
