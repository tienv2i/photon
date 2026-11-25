# Photon (`io.github.tienv2i.Photon`)

Photon is an automation-first, headless-capable image browser, viewer, and non-destructive layered editor built in Rust with `iced 0.14`, `wgpu 27`, and a single authoritative Tokio `CoreRuntime`.

## Workspaces

All three workspaces run in one process sharing `CoreRuntime` and GPU/CPU image caches:

- **Browser** (`photon-browser`): Resizable `Quick Access` + `Explorer` sidebar (`photon-panels`), manual `Detail` inspector, `Grid` / `List` / `Details` views, `XS`–`XL` thumbnails, sorting/filtering, file operations, and separate **Batch Resize**, **Batch Crop**, and **Batch Convert** dialogs (`px` / `cm` / `in` / `%` + DPI).
- **Viewer** (`photon-viewer`): Hardware-accelerated `Single`, `Double` spread (LTR/RTL, separate cover, page gap), and continuous `Webtoon` strip modes, unified right dock (`View Adjustments` + `Info`), and optional bottom `Filmstrip`.
- **Editor** (`photon-editor`): Non-destructive layered document (`photon-image`), layer masks, Before/After `Compare` modes, atomic backup saves (`file~`), and real-time GPU adjustments via 4 live-preview `Cutout` dialogs (`Color & Light`, `HSV`, `Color Balance`, `Curves`), plus:
  - **Selection Suite**: Rectangular / Elliptical / Single Row / Single Column Marquee (`M`), Freehand Lasso, Krita-style Polygon Lasso, Sobel + Dijkstra Magnetic Lasso, Bézier Select, Magic Wand (`W`), Quick Select, Color Range, Quick Mask (`Q`), animated GPU marching ants, `Select` menu with numeric `SelectionModify` dialog (`Feather...`, `Expand...`, `Contract...`, `Border...`, `Smooth...`, `Grow...`, `Similar...`), and automatic `SelectionMask` clipping across all paint/draw/adjustment ops.
  - **Krita-style Drawing & Curve Tools**: `Brush` / `Eraser` with `BrushSmoothingMode` (`None`, `Basic`, `Weighted`, `Stabilizer`), `Shape` flyout (`Line`, `Rect`, `Ellipse`, `Polygon`, `Polyline` — `U`), interactive `Bézier Pen` (`P`) with anchor/tangent handles on the GPU vector overlay (`Enter` stroke/fill, `Ctrl+Enter` Path → Selection), `Paint Bucket` (`F`), and `Gradient` (`G` — Linear, Radial, Conical, Reflected).
  - **Canvas Navigation**: Scroll-to-zoom at cursor, viewport-clamped panning restricted to `Hand` (`H`) or global spring-loaded `Space + Drag` (preserving in-progress Polygon/Bézier nodes), and global `Alt + Click` color sampling for paint/draw tools (`Alt` = Subtract for Selection tools).

## Workspace Crates (15)

| Crate | Responsibility | GUI/GPU Free? |
|---|---|---|
| `photon-protocol` | `Envelope`, `Command`, `Query`, `Event`, `CommandResponse`, `UiText`, `LayoutState`, `BrushSmoothingMode`, `VectorPathNode`, JSON Schema (`schemars`) | Yes |
| `photon-storage` | `AppIdentity` (`Dev`/`Prod`/`Sandbox`), `AppPaths` (XDG), atomic `Config`, `Journal` (`commands.jsonl`), daily JSONL logs | Yes |
| `photon-i18n` | Embedded Fluent `.ftl` (`en-US`, `vi-VN`), `UiText` translator, key & placeholder parity tests | Yes |
| `photon-image` | Image probing, RGBA decode, thumbnails, non-destructive `EditorDocument`, layers, masks, `SelectionMask`, `drawing` (Bézier, shapes, flood fill, gradients), liquify, atomic `file~` backup save | Yes |
| `photon-core` | Authoritative `Core` state machine, `CoreRuntime` (Tokio `mpsc` + `watch` + `broadcast`), batch runner, sandbox `replay` | Yes |
| `photon-ipc` | Secure local IPC (`$XDG_RUNTIME_DIR/photon[-dev]/control.sock`, `0600` on Unix) with NDJSON request/response & event stream | Yes |
| `photon-dialogs` | Native OS pickers (`rfd`) + movable, 8-way resizable `DialogManager` frames and `DimmedFrosted` / `Cutout` / `Floating` overlays | `iced` UI |
| `photon-ui` | Pure-`iced` design system (`PaletteTokens`, typography, SVG `icons`, `button`, `menu`, `toolbar`, `grid`, `table`, `editor`, `splitter`) | `iced` UI |
| `photon-panels` | Unified dockable, collapsible, closable, drag-reorderable sidebar panels (`SidebarOrderState`, `DraggablePanelHeaderSpec`, `PanelSplitter`) | `iced` UI |
| `photon-canvas` | `wgpu 27` `ImageCanvasProgram`, WGSL marching-ants/Quick-Mask shader, GPU vector overlay sub-pipeline, Sobel+Dijkstra `magnetic_snap`, `Space + Drag` & `Alt + Click` | `iced` + `wgpu` |
| `photon-browser` | Browser workspace UI, file grid/list/details views, and batch resize/crop/convert dialogs | `iced` UI |
| `photon-viewer` | Viewer workspace UI, spread/webtoon layout, filmstrip, and view adjustments dock | `iced` UI |
| `photon-editor` | Editor workspace UI, tool rail & flyouts, tool options card, layers/adjustments docks, and splitters | `iced` UI |
| `photon-cli` | `photonctl` binary: IPC command dispatch, queries, `debug` suite (`logs`, `journal`, `trace`, `ui-tree`, `screenshot`, `replay`), `mcp` stdio server | Yes |
| `photon-desktop` | `photon` binary: `iced 0.14` `daemon` multi-window shell, menus, dialogs, semantic inspector, and `--headless` mode | GUI binary |

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

- [AGENTS.md](AGENTS.md) — Non-negotiable architectural invariants and debugging order
- [docs/SPEC.md](docs/SPEC.md) — Functional and UI specification for dialogs, panels, and all 3 workspaces
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — Crate boundaries, command gateway, multi-window, and GPU pipeline
- [docs/CLI.md](docs/CLI.md) — `photon` and `photonctl` CLI reference
- [docs/WORKFLOW.md](docs/WORKFLOW.md) — Verification tiers and bug triage workflow
- [docs/STATE.md](docs/STATE.md) — Current verified build/test status and roadmap notes
- [docs/MEMORY.md](docs/MEMORY.md) — Engineering lessons learned and `iced 0.14` gotchas
