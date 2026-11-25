# Photon Project State

Verified snapshot of the `/home/huynhat/Projects/photon` workspace.

## Workspace Inventory

- **Workspace members**: 11 crates (`edition = "2024"`, `rust-version = "1.88"`):
  - Core & automation: `photon-protocol`, `photon-storage`, `photon-i18n`, `photon-image`, `photon-core`, `photon-ipc`, `photon-cli`
  - UI & desktop: `photon-dialogs`, `photon-ui`, `photon-panels`, `photon-desktop`
- **Binaries**: `photon` (`crates/photon-desktop`) and `photonctl` (`crates/photon-cli`)

## Verified Checks (Current Tree)

All checks below were executed against the current workspace and passed:

| Check | Command | Result |
|---|---|---|
| Formatting | `just fmt-check` (`cargo fmt --all -- --check`) | Passed |
| Workspace tests | `just test` (`cargo test --workspace`) | **76 passed, 0 failed** |
| Lints (strict) | `cargo clippy --workspace --all-targets -- -D warnings` | Passed (0 warnings) |
| Release build | `just build-release` (`cargo build --workspace --release`) | Passed |

### Test Breakdown (`just test`)

- `photon-cli` (unit): 1 passed (`clap_cli_has_no_arg_collisions`)
- `photon-core` (unit): 15 passed (batch, protocol schemas, disabled translate, file ops, sidecar/TSV, startup/layout/guards, layer masks/liquify/editor/export)
- `photon-core` (`tests/automation_e2e.rs`): 2 passed (i18n coverage of all Core/Image `UiText` codes, IPC dispatch/conflict/idempotency/sandbox replay)
- `photon-desktop` (unit): 7 passed (crop UV, thumb stepper, hex parsing, formatting, dimension inputs, batch commands)
- `photon-dialogs` (unit): 11 passed (`DialogManager`, 8-way resize, viewport clamping, `CutoutStrips` partitioning, native filter helpers)
- `photon-i18n` (unit): 3 passed (`en-US`/`vi-VN` key & variable parity, Core/Editor `UiText` rendering)
- `photon-image` (unit): 7 passed (layers, masks, liquify, non-destructive undo/redo/reset, adjustments, canvas/crop/resize, bounded history & `.bak` save)
- `photon-ipc` (unit): 0 tests
- `photon-panels` (unit): 11 passed (`SidebarOrderState` DnD/normalization/swaps, `PanelSplitter` portions/collapse heights, panel/header builders)
- `photon-protocol` (unit): 4 passed (`LayoutState` defaults, legacy deserialization, order sanitization, visibility overrides)
- `photon-storage` (unit): 5 passed (profile identity isolation, legacy config deserialization, journal roundtrip, corruption backup/repair, bounds sanitization)
- `photon-ui` (unit): 10 passed (unit conversions, splitters, dialog theme bridge, menu clamping/height estimation, splines, tooltips, workspace switcher)

## Verification Scope Notes

- Automated `just test` validates domain logic, protocol schemas, IPC, replay, image processing, i18n parity, and `photon-ui`/`photon-dialogs`/`photon-panels` widget builders/math without opening an OS window.
- Live GPU/window rendering (`wgpu` / `iced::daemon` / OS `rfd` pickers) is verified interactively using Tier 3 workflow (`photon` + `photonctl debug ui-tree` + `photonctl debug screenshot`).

## Architecture & Refactoring Notes

- **Completed crate extractions**: Reusable dialog geometry/overlays live in `photon-dialogs`, and unified sidebar dock/header/DnD/splitter primitives live in `photon-panels`, shared across Browser, Viewer, and Editor.
- **Disabled feature policy**: AI translation (`ViewerTranslate` et al.) and Plugins are disabled at runtime while keeping protocol/config serde compatibility for existing configs and journal replay.
- **Future cleanup candidate**: `crates/photon-desktop/src/app.rs` (~7,091 lines) centralizes `PhotonGui` state, `Message` handling, menus, settings, and dialog wiring; splitting `Message` handlers by workspace/concern is the primary maintainability opportunity if desktop UI grows further.
