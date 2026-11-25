# Photon Architecture

## Invariant Flow
`iced/wgpu GUI | photonctl CLI | Local IPC | MCP | Replay -> Envelope -> CoreRuntime (Tokio Actor) -> Core Handlers -> AppState + Storage -> watch/broadcast -> Adapters`

## Workspace Crates (11)
| Crate | Responsibility | GUI/GPU Free? |
|---|---|---|
| `photon-protocol` | `Envelope`, `Command`, `Query`, `Event`, `CommandResponse`, `UiText`, `LayoutState`, JSON Schema (`schemars`) | Yes |
| `photon-storage` | `AppIdentity` (`Dev`/`Prod`/`Sandbox`), `AppPaths` (XDG), atomic `Config`, `Journal` (`commands.jsonl`), `tracing` daily JSONL logs | Yes |
| `photon-i18n` | Embedded Fluent `.ftl` (`en-US`, `vi-VN`) + `UiText` translator + key/placeholder parity test | Yes |
| `photon-image` | Image probing, RGBA decode, thumbnail generation, non-destructive `EditorDocument`, layers/masks/liquify, atomic `file~` backup save | Yes |
| `photon-core` | Authoritative `Core` state machine, `CoreRuntime` (Tokio `mpsc` + `watch` + `broadcast`), batch runner, sandbox `replay` engine | Yes |
| `photon-ipc` | Secure local IPC (`$XDG_RUNTIME_DIR/photon[-dev]/control.sock` `0600` on Unix) with NDJSON request/response & event stream | Yes |
| `photon-dialogs` | Native OS file/folder/save dialogs (`rfd`) + movable/8-way resizable `DialogManager` frames and `DimmedFrosted` / `Cutout` / `Floating` overlays | UI Kit only (no Core dep) |
| `photon-ui` | Reusable pure-`iced` Design System & UI Kit (`theme`, SVG `icons`, `button`, `menu`, `dialog` theme bridge, `toolbar`, `grid`, `table`, `editor`, `splitter`, `tooltip`) | UI Kit only (no Core dep) |
| `photon-panels` | Unified dockable, collapsible, closable, drag-reorderable sidebar panel system (`SidebarOrderState`, `DraggablePanelHeaderSpec`, `PanelFrameSpec`, `PanelSplitter`) | UI Kit only (no Core dep) |
| `photon-cli` | `photonctl` binary: control commands, `debug` suite, and `mcp` JSON-RPC stdio server | Yes |
| `photon-desktop` | `photon` binary: `iced 0.14` (`daemon` + multi-window) + `wgpu 27` WGSL shader image viewport + `--headless` mode + semantic UI inspector | GUI binary |

## UI, Panel & Dialog Architecture
- **Layering**: `photon-desktop` composes `photon-ui` (tokens, controls, menus, toolbars), `photon-dialogs` (OS pickers, `DialogManager<DesktopDialogId>`, `OverlayMode::{DimmedFrosted, Cutout, Floating}`), and `photon-panels` (`SidebarOrderState`, `DraggablePanelHeaderSpec`, `sidebar_panel`, `sidebar_dock`) while keeping all domain mutations routed through `CoreHandle::dispatch`.
- **Unified Sidebars**: Browser (`[Quick Access, Explorer]` + optional `Detail`), Viewer (`[View Adjustments, Info]`), and Editor (`[Tool Options | History, Layers]`) share identical drag-reorderable panel chrome, collapse state, and `LayoutState` persistence (`pending_layout_commands` guards against stale `watch` overwrites).
- **Live-Preview Cutouts**: Editor adjustment dialogs (`AdjustColor`, `AdjustHsv`, `AdjustColorBalance`, `AdjustCurves`) use `OverlayMode::Cutout` over `resolved_editor_canvas_bounds` so perimeter UI stays locked while the `wgpu` canvas remains undimmed and pan/zoom-interactive.
- **Disabled AI/Translate & Plugins**: Translation commands (`ViewerTranslate`, `ViewerSaveTranslation`, `ViewerDeleteTranslation`, `SetTranslateSettings`) remain serde-compatible in `photon-protocol` and `Config` for journal replay, but `Core::dispatch` rejects them with `feature_disabled` (`ai_feature_disabled`) and Browser's `Plugins` menu is a disabled placeholder.

## Modes & Multi-Window Strategy
Instead of 3 disconnected binaries (which break GPU texture sharing and cross-workspace transitions), `photon` runs **one CoreRuntime** in-process and exposes:
- **3 Workspaces** (`Browser`, `Viewer`, `Editor`) switchable instantly in-window with zero re-decode.
- **Multi-window (`iced::daemon`)**: extra windows can be opened (`--new-window` or UI action) sharing the same `CoreRuntime` and `wgpu` caches.
- **Headless mode (`photon --headless`)**: runs `CoreRuntime` + `photon-ipc` without opening any window or GPU surface for automation, CI, and CLI control.
- **Single-instance forwarding**: launching `photon <path>` when an instance is already listening on `control.sock` forwards `OpenFiles` / `OpenFolder` / `EditorOpen` over IPC and exits `0`.

## GPU & Large-Image Pipeline (`iced 0.14` + `wgpu 27`)
- `iced::widget::shader::Pipeline` renders the Viewer/Editor image canvas via WGSL shaders:
  - Hardware zoom-at-cursor, pan, checkerboard transparency backdrop, and real-time Brightness/Contrast/Saturation adjustments at 60fps without disk PNG roundtrips.
  - Images larger than GPU limits (`> 4096px`) are streamed/tiled or downsampled into safe GPU texture bounds while `photon-image` preserves full-resolution CPU truth for non-destructive export.
- `--safe-mode` forces `iced` software fallback (`tiny-skia`) and in-memory factory defaults.
