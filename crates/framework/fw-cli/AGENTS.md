# `fw-cli` (`crates/framework/fw-cli`)

Domain-agnostic CLI output, IPC request/response helpers, and MCP JSON-RPC 2.0 stdio server framing.

## Invariants
- Pure Rust (`fw-ipc`, `fw-protocol`, `serde`, `serde_json`, `tokio`).
- Never depend on `iced`, `wgpu`, `clap` app-specific schemas, or `crates/apps/*`.
- Check isolation with `cargo tree -p fw-cli --depth 1`.
