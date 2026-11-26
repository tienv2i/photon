# Photon (`io.github.tienv2i.Photon`)

Photon is an automation-first, headless-capable image browser, viewer, and non-destructive layered editor built in Rust with `iced 0.14`, `wgpu 27`, and a single authoritative Tokio `CoreRuntime`.

## Workspaces & 3-Process Architecture

Photon runs a unified `CoreRuntime`, `PluginRegistry`, and GPU/CPU image pipeline, coordinating three distinct window roles (with separate Linux `application_id` / `StartupWMClass` taskbar identities):

- **Browser** (`photon-browser`, singleton via `photon.sock`, `io.github.tienv2i.Photon`):
  - Resizable `Quick Access` (`Icon::Folder` rows with header pencil Edit toggle) + `Explorer` sidebar (`photon-panels`), manual `Detail` inspector, `Grid` / `List` / `Details` views with clean long-filename clipping (`clip(true)`), `XS`–`XL` thumbnails, sorting/filtering, **Edit** menu (`Cut`/`Copy`/`Paste`, `Select All`), Open / Edit / `xdg-open` context actions, bottom-anchored cascading `Plugins > <Plugin> > <Actions>` submenus with normalized plugin/action labels, and separate **Batch Resize**, **Batch Crop**, and **Batch Convert** dialogs (`Selection` / `Current Folder` / `Custom Folder` source, `px` / `cm` / `in` / `%` + DPI, and dropdown `pick_list` option selectors with 2-line slider+steppers).
  - **Window, Typography & Startup Modes**: Opens maximized on first launch and remembers window size/maximized state (`AppLaunchWindowMode::{Maximized, Remembered}`), separated in Settings (`config.json`) from runtime UI session state (`ui_state.json` with configurable **Save UI state on exit** and `Ctrl+Q`/`Ctrl+Close` force-save), and from **Opening Image Files** (`SystemOpenWindowMode`: `Fit Image`, `Fullscreen`, `Remembered`). Settings also provides live **UI Scale**, **Font Scale**, and **Display Font** controls with GNOME fractional scaling support.
  - **Clipboard & Plugins**: `Ctrl+C` / `Ctrl+V` image & file copy/paste (`Command::PasteImageBytes` with safe `stem (copy).ext` collision naming) and `Plugins` menu (`BrowserAction` plugins, `Manage Plugins…`, `Reload Plugins`).
- **Viewer** (`photon-viewer`, multi-instance via `v-<pid>.sock`, `io.github.tienv2i.Photon.Viewer`):
  - Independent multi-image Viewer processes (`photon view <file>`, Browser open, or "Open in New Window") with canonical-path window deduplication & focus/restore activation (`photon-ipc::viewer_registry`), `[ Browser | Editor ]` navigation, `Filmstrip` (`Icon::PanelsTopLeft`) → `Fullscreen` → `Info` → `Adjustment` toolbar flow, responsive `ReaderOptions` (`Icon::BookOpen` + `MenuEntry::IconRow`), `Single` / `Double` (with Manga RTL support) / `Webtoon` modes, auto-hiding hover-reveal edge Next/Back zones (auto-disabled during `Space` pan, marquee drag, and when the image fits or doesn't overflow), synchronized PDF/CBZ/Image toolbar (filename & page info in status bar), hybrid `.cbz`/`.zip`/`.pdf` `ArchiveSession` with windowed Filmstrip, bottom-anchored cascading `Plugins > <Plugin> > <Actions>` context submenus, and interactive Translate overlay (`GoogleWeb` default, `⋮⋮` drag handles, bottom-centered `result_card`, full-box masking, `SwapTranslateLangs`, and Dual/Manga/Webtoon `drag region` marquee + on-demand PDF page rasterization).
  - Configurable **Canvas Anti-Aliasing** (`Crisp`, `Smooth`, `HighQuality`), **Viewport Denoise** (`Off`, `Light`, `Medium`, `Strong`), and **Canvas Backdrop** presets/custom color in the auto-closing `Workspace & View` menu.
- **Editor** (`photon-editor`, singleton via `editor.sock`, `io.github.tienv2i.Photon.Editor`):
  - **Multi-Tab Workspace**: Document tab bar below the Editor toolbar (`+` / `Ctrl+N` new document with **New from Clipboard** support, tab context menu for Close / Close Others / Close Right / Duplicate, virtual `Save As` for unsaved buffers, Unsupported Format screen with `Back to Viewer` / `Back to Browser`, dirty indicators, per-tab view cache, dirty-close confirmation on `×`/`Ctrl+W`, `Ctrl+Tab`/`Alt+1..9` switching, and Photoshop-aligned shortcuts).
  - **Non-Destructive Vector & Text Layers**: Full `LayerKind::{Raster, Vector, Text}` support with `V` / `T` layer badges, **Path Select** (`A`) & **Pen** (`P`) / **Shape** (`U`) path node/handle editing, **Text Tool** (`T`) with live font/size/alignment/style/color options, a dedicated **Vector** menu (**Transform Each…** with `Repeat Step` & `Apply to Duplicate`, **Vector Blend…**, **Convert Shape to Path**, **Rasterize Layer**), and `.ora` / CPU rasterization integration.
  - Non-destructive layered document (`photon-image`), full-width `Blend Mode` selector above `Opacity` in the Layers panel, 2-line slider+stepper controls across panels and dialogs, cursor-anchored `Add Layer` menu with accurate viewport clamping, layer masks, Before/After `Compare` modes, atomic backup saves (`file~`), menu-driven **Image Size…** (`Ctrl+R`), **Canvas Size…** (`Ctrl+Shift+R`), and **Duplicate Image** popups with dropdown controls, and real-time GPU adjustments via `Cutout` dialogs (`Color & Light`, `HSV`, `Color Balance`, `Curves`), plus **Filter → Denoise…** (`LayerEffect::Denoise`):
    - **Interactive Curves + Live GPU LUT**: `CurveGraphProgram` (`360×360` tonal curve editor inside a `500×650` dialog with luminance histogram backdrop, monotone cubic Hermite spline, `Presets` dropdown for `Linear`/`S-Curve`/`Fade`/`High Contrast`/`Negative`, channel reset, and live 60fps `256×1` RGBA8 GPU 1D-LUT preview).
  - **Selection Suite**: Rectangular / Elliptical / Single Row / Single Column Marquee (`M`), Freehand Lasso, Krita-style Polygon Lasso, Sobel + Dijkstra Magnetic Lasso, Bézier Select, Magic Wand (`W`), Quick Select, Color Range, Quick Mask (`Q`), animated GPU marching ants on a dedicated `Nearest` `sel_sampler` (isolated from image AA/Denoise), `Select` menu with `SelectionModify` dialog, and `SelectionMask` clipping across paint/draw/adjustment/denoise/healing ops.
  - **Fill, Multi-Stop Gradient, Healing & Krita-style Drawing**: `Brush` / `Eraser` with `BrushSmoothingMode` (`None`, `Basic`, `Weighted`, `Stabilizer`) and selection-clipped live strokes, `Shape` flyout (`Line`, `Rect`, `Ellipse`, `Polygon`, `Polyline` — `U`), interactive `Bézier Pen` (`P`), **Paint Bucket** (`F`, color/opacity + `sample_all_layers`), **Gradient** (`G`, multi-stop presets & stop editor, 6 shapes `Linear`/`Bilinear`/`Radial`/`Square`/`Conical`/`Spiral`, 3 repeat modes, 7 blend modes, 4×4 Bayer `dither`, interactive canvas handles + live preview), and **Healing Brush** (`J`, `Spot` and `Alt+Click` `Sampled` frequency-separated texture transfer).
  - **Live 60fps Free Transform (`Ctrl+T`)**: Direct canvas OBB handles (Move, 8-way Scale with `Shift` aspect-lock & `Alt` center-scale, external Rotate, Pivot crosshair), numeric X/Y/W%/H%/Angle°/Flip controls, live GPU inverse affine preview (`TransformPreview`), and `Enter` commit / `Esc` cancel as a single undoable step.
  - **Free Canvas Navigation & Oversized (`>4096px`) GPU Support**: Free pan on `Pointer`/`Hand` and tool-safe Middle-mouse or `Space + Drag` pan across all tools (with `Unfocused` transient flag reset), `Shift + Wheel` / touchpad horizontal pan, draggable Canvas Scrollbars, Zoom `-` / `%` / `+` / `Fit` (`0`) / `1:1` (`1`) controls, dynamic zoom ceiling (`min_zoom = 0.02`, `max_zoom = (32.0 / fit_scale).max(32.0)`), and GPU texture cap raised to `device.limits().max_texture_dimension_2d.min(16384)` with exact `doc_size` coordinate alignment.
  - **Pixel Clipboard & Renderer Controls**: Selection/layer-aware `Cut` (`Ctrl+X`), `Copy` (`Ctrl+C`), `Copy Merged` (`Ctrl+Shift+C`), `Paste` (`Ctrl+V`), **Smart Paste** (fit/center/as-layer options), and `Paste Into Selection` (`arboard` + `wl-copy`/`wl-paste`/`xclip`), plus Hardware Acceleration toggle (`GPU (wgpu)` vs `Software (tiny-skia)`).

## Supported Image & Archive Formats

- **Multi-layer**: OpenRaster (`.ora` full read/write preserving layers, offsets, opacity, visibility, blend modes, thumbnail, and `.bak` backups), Photoshop (`.psd` multi-layer import), Krita (`.kra` merged preview/decode).
- **Vector, HDR, Next-Gen, Icon & Camera RAW**: `.svg` / `.svgz` (`usvg`/`resvg`), `.jxl` (`jxl-oxide`), `.jp2`/`.j2k`/`.jpx`/`.jpf` (`libopenjp2`), `.heic`/`.heif`/`.hif` (`libheif`), native `.avif` (`avif-native`), `.icns` (`image/x-icns`), Camera RAW (29 formats via `rawler`: `.dng`, `.cr2`, `.cr3`, `.nef`, `.arw`, `.raf`, `.orf`, `.rw2`, `.ari`, `.crm`, `.crw`, `.dcr`, `.dcs`, `.erf`, `.iiq`, `.kdc`, `.mef`, `.mos`, `.mrw`, `.nrw`, `.ori`, `.pef`, `.raw`, `.rwl`, `.srw`, `.3fr`, `.fff`, `.x3f`, `.qtk`), `.exr`, `.hdr`, `.ico`, `.cur`, `.pam`, `.dds`, `.ff` (Farbfeld), `.apng`, `.pnm`/`.ppm`/`.pgm`/`.pbm`, `.qoi`, `.tga`, `.webp`, `.tiff`, `.gif`, `.bmp`, `.png`, `.jpg`/`.jpeg`.
- **Paged Archives & Documents**: `.cbz`, `.zip`, `.pdf` (lazy `ArchiveSession` page indexing and materialization for Viewer & Translate).
- **Save / Export Formats**: `.png`, `.jpg`, `.webp`, `.tiff`, `.gif`, `.bmp`, `.tga`, `.qoi`, `.ora`, `.avif`, `.exr`, `.hdr`, `.ico`, `.pnm`, `.ff`.

## Plugin System (`photon-plugins`)

Photon runs external plugins out-of-process over `stdin`/`stdout` (supporting both native `plugin.json` `PhotonNdjsonV1` and KogaView `manifest.json` `JsonlStdioV1`), accessible via the **Plugins** menu in Browser & Editor, **Plugins → Manage Plugins…** (`640×480`), and `--headless` / `photonctl`.

Nine standalone MIT-licensed plugin crates are included under `plugins/`:
- `photon-plugin-ai-inpaint` (`photon.ai_inpaint.onnx`)
- `photon-plugin-ai-ocr` (`photon.ocr.ai`)
- `photon-plugin-caption` (`photon.caption.onnx`)
- `photon-plugin-dewatermark` (`photon.dewatermark`)
- `photon-plugin-ocr` (`photon.ocr.tesseract`)
- `photon-plugin-rembg` (`photon.rmbg.onnx`)
- `photon-plugin-translate` (`photon.translate`)
- `photon-plugin-upscale` (`photon.upscale.onnx`)
- `photon-plugin-utils` (`photon.utils`)

See [docs/PLUGINS.md](docs/PLUGINS.md) for wire protocols, model lookup paths, and plugin packaging details.

## Workspace Crates (16)

| Crate | Responsibility | GUI/GPU Free? |
|---|---|---|
| `photon-protocol` | `Envelope`, `Command`, `Query`, `Event`, `CommandResponse`, `UiText`, `LayoutState`, `PluginManifest`, `LayerTransformSpec`, JSON Schema (`schemars`) | Yes |
| `photon-storage` | `AppIdentity` (`Dev`/`Prod`/`Sandbox`), `AppPaths` (XDG), atomic `Config`, `Journal` (`commands.jsonl`), daily JSONL logs | Yes |
| `photon-i18n` | Embedded Fluent `.ftl` (`en-US`, `vi-VN`), `UiText` translator, key & placeholder parity tests | Yes |
| `photon-image` | Image probing, RGBA decode, thumbnails, `.ora`/`.psd`/RAW/SVG/JXL codecs, `EditorDocument`, layers, masks, `SelectionMask`, `healing`, `drawing`, `denoise_rgba`, affine transforms, atomic `file~` backup save | Yes |
| `photon-core` | Authoritative `Core` state machine, `CoreRuntime` (Tokio `mpsc` + `watch` + `broadcast`), pixel clipboard, plugin dispatch, batch runner, sandbox `replay` | Yes |
| `photon-plugins` | Process-isolated NDJSON & `jsonl-stdio` plugin discovery, path-traversal-safe validation, and execution bridge | Yes |
| `photon-ipc` | Secure local IPC (`$XDG_RUNTIME_DIR/photon[-dev]/control.sock`, `0600` on Unix) with NDJSON request/response & event stream | Yes |
| `photon-dialogs` | Native OS pickers (`rfd`) + movable, 8-way resizable `DialogManager` frames and `DimmedFrosted` / `Cutout` / `Floating` overlays | `iced` UI |
| `photon-ui` | Pure-`iced` design system (`PaletteTokens`, typography, SVG `icons`, `curve_editor`, `button`, `menu`, `toolbar`, `grid`, `table`, `editor`, `splitter`) | `iced` UI |
| `photon-panels` | Unified dockable, collapsible, closable, drag-reorderable sidebar panels (`SidebarOrderState`, `DraggablePanelHeaderSpec`, `PanelSplitter`) | `iced` UI |
| `photon-canvas` | `wgpu 27` `ImageCanvasProgram`, Stage 1/Stage 2 WGSL pipeline (1D Curve LUT, AA, Denoise, live Transform, `Nearest` `sel_sampler`), vector overlay, `Space+Drag` & `Alt+Click` | `iced` + `wgpu` |
| `photon-browser` | Browser workspace UI, file grid/list/details views, clipboard/plugins menus, and batch resize/crop/convert dialogs | `iced` UI |
| `photon-viewer` | Viewer workspace UI, spread/webtoon layout, filmstrip, quality options, and view adjustments dock | `iced` UI |
| `photon-editor` | Editor workspace UI, tool rail, tool options card, canvas scrollbars & zoom controls, layers/adjustments docks, and splitters | `iced` UI |
| `photon-cli` | `photonctl` binary: IPC command dispatch, queries, `debug` suite (`logs`, `journal`, `trace`, `ui-tree`, `screenshot`, `replay`), `mcp` stdio server | Yes |
| `photon-desktop` | `photon` binary: `iced 0.14` `daemon` multi-window shell, menus, dialogs (`ManagePlugins`, `PluginParams`, `AdjustDenoise`), semantic inspector, and `--headless` mode | GUI binary |

## Quick Start (`just`)

```sh
# Build & test
just build
just build-release
just fmt-check
just clippy
just test
just test-core

# Build & install the 9 standalone plugins
just build-plugins
just install-plugins       # installs into ~/.local/share/photon/plugins
just install-plugins dev   # installs into ~/.local/share/photon-dev/plugins

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

Use one prefix at a time: a `~/.local` install shadows `/usr/local` because both
ship the same desktop IDs. If you switch between them, uninstall the other one
first (`just uninstall-user` before `just install`, or vice versa). Add
`--set-default` to either install command (e.g. `just install --set-default`) to
make the standalone Viewer the default image handler for the current user.

## CLI & Automation (`photon` & `photonctl`)

```sh
photon browse ~/Pictures
photon view image1.png image2.png
photon edit [photo.png]
photon --edit [photo.png]
photon --headless
photon --safe-mode

photonctl open-folder ~/Pictures
photonctl dispatch '{"type":"editor_rotate","degrees":90,"target":"whole_image"}'
photonctl query clipboard
photonctl debug ui-tree
photonctl debug screenshot --output /tmp/shot.png
photonctl debug replay --journal ~/.local/state/photon-dev/journal/commands.jsonl
photonctl mcp
```

## Documentation Index

- [AGENTS.md](AGENTS.md) — Non-negotiable architectural invariants and debugging order
- [docs/SPEC.md](docs/SPEC.md) — Functional and UI specification for dialogs, panels, and all 3 workspaces
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — Crate boundaries, command gateway, multi-window, and GPU pipeline
- [docs/PLUGINS.md](docs/PLUGINS.md) — Plugin bridge protocols, manifest formats, model paths, and bundled plugins
- [docs/CLI.md](docs/CLI.md) — `photon` and `photonctl` CLI reference
- [docs/WORKFLOW.md](docs/WORKFLOW.md) — Verification tiers and bug triage workflow
- [docs/STATE.md](docs/STATE.md) — Current verified build/test status and implemented milestones
- [docs/MEMORY.md](docs/MEMORY.md) — Engineering lessons learned and `iced 0.14` gotchas

## License

Photon and its bundled standalone plugins are licensed under the [MIT License](LICENSE). See [docs/PLUGINS.md](docs/PLUGINS.md#third-party-runtime--model-notes) for third-party ONNX runtime, Tesseract, and external model/decoder notes.
