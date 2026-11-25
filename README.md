# Photon (`io.github.tienv2i.Photon`)

Photon is an automation-first, headless-capable image browser, viewer, and non-destructive layered editor built in Rust with `iced 0.14`, `wgpu 27`, and a single authoritative Tokio `CoreRuntime`.

## Workspaces

All three workspaces run in one process sharing `CoreRuntime` and GPU/CPU image caches:

- **Browser**: Resizable `Quick Access` + `Explorer` sidebar (`photon-panels`), manual `Detail` inspector, `Grid` / `List` / `Details` views, `XS`–`XL` thumbnails, sorting/filtering, file operations, and separate **Batch Resize**, **Batch Crop**, and **Batch Convert** dialogs (`px` / `cm` / `in` / `%` + DPI).
- **Viewer**: Hardware-accelerated `Single`, `Double` spread (LTR/RTL, separate cover, page gap), and continuous `Webtoon` strip modes, unified right dock (`View Adjustments` + `Info`), and optional bottom `Filmstrip`.
- **Editor**: Non-destructive layered document (`photon-image`), layer masks, `Brush` / `Eraser` / `Liquify` / `Eyedropper` / `Crop` / `Resize` / `Canvas Size` / `Rotate Angle`, real-time GPU adjustments via 4 live-preview `Cutout` dialogs (`Color & Light`, `HSV`, `Color Balance`, `Curves`), Before/After `Compare` modes, and atomic backup saves (`file~`).

## Workspace Crates (11)

| Crate | Responsibility | GUI/GPU Free? |
|---|---|---|
| `photon-protocol` | `Envelope`, `Command`, `Query`, `Event`, `CommandResponse`, `UiText`, `LayoutState`, JSON Schema (`schemars`) | Yes |
| `photon-storage` | `AppIdentity` (`Dev`/`Prod`/`Sandbox`), `AppPaths` (XDG), atomic `Config`, `Journal` (`commands.jsonl`), daily JSONL logs | Yes |
| `photon-i18n` | Embedded Fluent `.ftl` (`en-US`, `vi-VN`), `UiText` translator, key & placeholder parity tests | Yes |
| `photon-image` | Image probing, RGBA decode, thumbnails, non-destructive `EditorDocument`, layers, masks, liquify, atomic `file~` backup save | Yes |
| `photon-core` | Authoritative `Core` state machine, `CoreRuntime` (Tokio `mpsc` + `watch` + `broadcast`), batch runner, sandbox `replay` | Yes |
| `photon-ipc` | Secure local IPC (`$XDG_RUNTIME_DIR/photon[-dev]/control.sock`, `0600` on Unix) with NDJSON request/response & event stream | Yes |
| `photon-dialogs` | Native OS pickers (`rfd`) + movable, 8-way resizable `DialogManager` frames and `DimmedFrosted` / `Cutout` / `Floating` overlays | `iced` UI |
| `photon-ui` | Pure-`iced` design system (`PaletteTokens`, typography, SVG `icons`, `button`, `menu`, `toolbar`, `grid`, `table`, `editor`, `splitter`) | `iced` UI |
| `photon-panels` | Unified dockable, collapsible, closable, drag-reorderable sidebar panels (`SidebarOrderState`, `DraggablePanelHeaderSpec`, `PanelSplitter`) | `iced` UI |
| `photon-cli` | `photonctl` binary: IPC command dispatch, queries, `debug` suite (`logs`, `journal`, `trace`, `ui-tree`, `screenshot`, `replay`), `mcp` stdio server | Yes |
| `photon-desktop` | `photon` binary: `iced 0.14` `daemon` multi-window GUI + `wgpu 27` WGSL `Shader` viewport + `--headless` mode | GUI binary |

## Quick Start (`just`)

```sh
# Build & test
just build
just build-release
just fmt-check
just clippy
just test
just test-core

# Run desktop app or headless Core + IPC
just run
just run-headless

# Drive or inspect a running instance via photonctl
just ctl status
just ctl query state
just ctl schema

# Install / uninstall desktop integration (binaries, .desktop, MIME types, icons)
just install-user     # installs to ~/.local (no sudo)
just install          # installs to /usr/local (or PREFIX=...)
just install-dev      # fast dev iteration using debug build
just uninstall-user
just uninstall
```

## CLI & Automation (`photon` & `photonctl`)

```sh
photon browse ~/Pictures
photon view image1.png image2.png
photon edit photo.png
photon --headless
photon --safe-mode

photonctl open-folder ~/Pictures
photonctl dispatch '{"type":"editor_rotate","degrees":90,"target":"whole_image"}'
photonctl debug ui-tree
photonctl debug screenshot --output /tmp/shot.png
photonctl debug replay --journal ~/.local/state/photon-dev/journal/commands.jsonl
photonctl mcp
```

## Documentation Index

- [AGENTS.md](file:///home/huynhat/Projects/photon/AGENTS.md) — Non-negotiable architectural invariants and debugging order
- [docs/SPEC.md](file:///home/huynhat/Projects/photon/docs/SPEC.md) — Functional and UI specification for dialogs, panels, and all 3 workspaces
- [docs/ARCHITECTURE.md](file:///home/huynhat/Projects/photon/docs/ARCHITECTURE.md) — Crate boundaries, command gateway, multi-window, and GPU pipeline
- [docs/CLI.md](file:///home/huynhat/Projects/photon/docs/CLI.md) — `photon` and `photonctl` CLI reference
- [docs/WORKFLOW.md](file:///home/huynhat/Projects/photon/docs/WORKFLOW.md) — Verification tiers and bug triage workflow
- [docs/STATE.md](file:///home/huynhat/Projects/photon/docs/STATE.md) — Current verified build/test status and roadmap notes
- [docs/MEMORY.md](file:///home/huynhat/Projects/photon/docs/MEMORY.md) — Engineering lessons learned and `iced 0.14` gotchas
