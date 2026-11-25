# AI Agent Instructions — Photon (`io.github.tienv2i.Photon`)

Read `docs/ARCHITECTURE.md`, `docs/CLI.md`, and `docs/WORKFLOW.md` before implementing features.

## Core architectural invariants (non-negotiable)
1. **Automations-First & Headless-Capable Core**:
   - `photon-core` is the single authoritative owner of domain state (`revision`), config, and operations. It must NEVER depend on `iced` or `wgpu`.
   - `photon --headless` must run all domain, IPC, journal, and image operations with zero GUI/GPU dependencies.
2. **Single Command Gateway**:
   - GUI (`iced`), CLI (`photonctl`), Local IPC (`photon-ipc`), MCP (`photonctl mcp`), and Replay (`debug replay`) all submit typed `Envelope { id, protocol, source, expected_revision, command }` to `CoreRuntime`.
   - Queries (`Query`) are read-only; Events (`Event`) are sequenced notifications (`seq_id`).
3. **UI is the presentation edge (`photon-desktop`)**:
   - `iced` owns only ephemeral presentation state (viewport zoom/pan, hover, scroll, focus, animation, GPU texture cache).
   - Domain mutations always emit a typed `Command` to `CoreHandle`; never mutate files or domain state inside `iced::view()` or `update()` directly.
   - All major interactive widgets must carry semantic IDs and report through `UiTreeSnapshot` so `photonctl debug ui-tree` and `debug screenshot` can inspect real layout and pixels.
4. **Pure-Rust i18n (`photon-i18n`)**:
   - Core emits `UiText { code, args }` only — never translated UI sentences.
   - Journals and logs record `UiText::english()`; `photon-i18n` translates at the UI edge via embedded Fluent `.ftl` (`en-US`, `vi-VN`).
   - Every `en-US` key and placeholder must match `vi-VN` (enforced by `cargo test -p photon-i18n`).
5. **Non-destructive editing & Profile isolation**:
   - Dev (`photon-dev`), Prod (`photon`), and Sandbox (`PHOTON_SANDBOX_DIR`) never share config, logs, journals, or IPC sockets.
   - Overwriting an image in Editor always stages to a temp file and keeps numbered backups (`file~`).

## Debugging order (log-first & trace-first)
1. **Domain / CLI / IPC issues**:
   - `photonctl status`
   - `photonctl debug trace --request-id <id>` (correlates `commands.jsonl` + `photon.jsonl.<date>`)
   - `photonctl debug replay --journal <path> --sandbox` to reproduce deterministically
2. **GUI / `iced` / `wgpu` presentation issues**:
   - `photonctl debug ui-tree` (check semantic widgets, visibility, bounds, zoom/pan, GPU adapter)
   - `photonctl debug screenshot --output /tmp/shot.png` (direct `iced::window::screenshot` framebuffer grab)
3. **Verification**:
   - `just fmt-check && just test` before finishing any change.
   - Never claim real-window or GPU behavior works unless actually launched and inspected.
