# `notes-app` (`crates/apps/notes`)

Minimal reference application built directly on `crates/framework/fw-*` (`fw-protocol`, `fw-storage`, `fw-runtime`, `fw-ipc`, `fw-cli`, `fw-ui`) to verify framework reusability.

## Invariants
- Must NEVER depend on any `photon-*` crate (`cargo tree -p notes-app --depth 1`).
- Keeps `NotesCore` (`fw_runtime::AppCore`) headless-capable (`notes --headless`) and controllable via `notesctl` (`status`, `state`, `create`, `dispatch`, `mcp`).
- Profile/sandbox isolation via `NOTES_BRANDING` (`NOTES_SANDBOX_DIR`).
