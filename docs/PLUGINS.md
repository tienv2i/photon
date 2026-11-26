# Photon Plugin System (`photon-plugins`)

Photon supports external, process-isolated plugins via `crates/photon-plugins` (zero GUI/GPU dependencies — works identically in the desktop GUI, `--headless` daemon, `photonctl`, MCP, and `debug replay`).

## Architecture & Security Model

- **Process Isolation**: Plugins run as separate child processes over `stdin`/`stdout` with scoped temporary directories (`photon-plugin-<pid>-<seq>`) cleaned up on drop.
- **Path & ID Validation**:
  - `validate_plugin_id` restricts IDs to `1..=128` ASCII alphanumeric characters plus `.`, `-`, `_` (starting and ending with an alphanumeric character, no `..`).
  - `resolve_safe_entry` rejects absolute paths, `..` traversal, NUL bytes, and symlink escapes outside the plugin directory.
  - `install_from_dir` validates the manifest before staging and rejects symlinks anywhere inside plugin packages.
- **Guardrails**:
  - Default execution timeout: `30s` (`DEFAULT_PLUGIN_TIMEOUT`), configurable per manifest via `timeout_ms` / `timeoutMs`.
  - Output image dimension limit: `32_768 × 32_768` (`MAX_PLUGIN_IMAGE_DIMENSION`).
  - Browser image-modifying plugins automatically create numbered sibling backups (`file~`, `file.2~`, ...) before overwriting, or write `<stem>_nobg.png` for `editor.remove_background` on non-PNG/WebP inputs.
  - Editor plugins (`PluginCapability::EditorFilter`) push undoable `EditOp::Plugin` history steps (`ReplaceLayer` or `NewLayer`).

## Discovery & Directory Layout

`PluginRegistry::discover_or_env` scans `<plugins_dir>/*/` for either `plugin.json` or `manifest.json`:

1. `PHOTON_PLUGINS_DIR` environment variable (if set and non-empty).
2. Otherwise `<AppPaths::data_dir>/plugins`:
   - **Production** (`photon`): `~/.local/share/photon/plugins`
   - **Development** (`photon-dev`): `~/.local/share/photon-dev/plugins`
   - **Sandbox** (`PHOTON_SANDBOX_DIR=<dir>`): `<dir>/data/plugins`

Enabled/disabled states are persisted in `Config.disabled_plugins` (`Command::PluginSetEnabled`).

### Model Lookup Order (`JsonlStdioV1`)

For AI/OCR plugins that load external model files, `photon-plugins` resolves `modelsDir` in this order:

1. Explicit `params.modelsDir` parameter (if provided)
2. `PHOTON_MODELS_DIR` environment variable
3. Sibling `<plugins_dir>/../models` directory (if it exists)
4. First existing directory in:
   - `$XDG_DATA_HOME/photon/models` (`~/.local/share/photon/models`)
   - `$XDG_DATA_HOME/photon-dev/models` (`~/.local/share/photon-dev/models`)
   - `$XDG_DATA_HOME/kogaview/models` (`~/.local/share/kogaview/models`)

## Supported Protocols

### 1. Native Photon NDJSON (`plugin.json` / `PhotonNdjsonV1`)

Manifest (`plugin.json`):
```json
{
  "id": "example.invert",
  "name": "Example Filter",
  "version": "1.0.0",
  "description": "Optional description",
  "author": "Author",
  "entry": "bin/example-plugin",
  "args": [],
  "capabilities": ["editor_filter", "browser_action"],
  "params": [
    {
      "key": "strength",
      "label": "Strength",
      "kind": "float",
      "default": 0.5,
      "min": 0.0,
      "max": 1.0,
      "step": 0.05
    }
  ],
  "permissions": ["gpu"],
  "timeout_ms": 30000
}
```

Supported `PluginParamKind` values: `float`, `int`, `bool`, `select`, `color` (`[u8; 3]`).

Wire exchange (`PLUGIN_BRIDGE_PROTOCOL = 1`, single-line JSON on `stdin` -> single-line JSON on `stdout`):
- **Editor request**: `{"protocol":1,"action":"run","capability":"editor_filter","params":{...},"input_path":".../input.png","selection_mask_path":".../selection_mask.png" | null,"width":W,"height":H,"output_path":".../output.png"}`
- **Browser request**: `{"protocol":1,"action":"run","capability":"browser_action","params":{...},"selected_paths":[...],"current_folder":"..."}`
- **Response**: `{"ok":true,"mode":"replace_layer"|"new_layer"|"browser_refresh","layer_name":"...","message":"..."}` or `{"ok":false,"error":"..."}`.

### 2. KogaView `jsonl-stdio` (`manifest.json` / `JsonlStdioV1`)

Supports `manifest.json` with `"apiVersion": 1` and `"entry": { "command": "...", "args": [...], "protocol": "jsonl-stdio" }`:
- Expands `${device}` in `entry.args` (defaults to `"cpu"` unless a `device` parameter is supplied) and prepends the entry binary's parent directory to `LD_LIBRARY_PATH` on Unix.
- Projects `capabilities`, `quickActions`, `models` (auto-exposing a `modelId` select parameter when multiple runnable models exist), `action` select parameter (when multiple capabilities exist), and `permissions` (`network`, `gpu`) into `PluginManifest` / `PluginInfo` so the GUI auto-generates `DesktopDialogId::PluginParams`.
- Sends `{"id":"req-1","op":"run","action":...,"modelId":...,"modelPath":...,"device":...,"input":...,"output":...,"params":{...}}` followed by `{"op":"shutdown"}`.
- Reads multi-line JSONL events (`progress`, `done`, `error`) from `stdout`. Text/caption/OCR/translation metadata (`meta.captionText`, `meta.clipboardText`, `meta.savedPath`, `meta.translation`) is surfaced in the response `message`.

## Declarative Hooks, Ordering & Localization API

To keep plugins decoupled from hardcoded IDs in `photon-browser`, `photon-viewer`, `photon-editor`, and `photon-desktop`, `manifest.json` / `PluginManifest` / `PluginInfo` support:

- `order` (`u16`, default `100`): Deterministic sort priority in menus and hook lists.
- `category` (`String`, optional): Grouping tag (e.g. `"retouch"`, `"upscale"`, `"ocr"`, `"translate"`, `"utility"`).
- `hooks` (`Vec<String>`): Extension hook points declared by the plugin. Standard constants in `photon_protocol::plugin_hooks`:
  - `browser.context_menu`
  - `viewer.context_menu`
  - `editor.plugins_menu`
  - `editor.tool_panel`
  - `viewer.ocr_engine`
  - `viewer.translate_provider`
  - `settings.panel`
  *(If `hooks` is omitted in legacy manifests, `browser.context_menu`, `viewer.context_menu`, and `editor.plugins_menu` are auto-derived from `capabilities`.)*
- `labels` (`BTreeMap<String, String>`): Locale-keyed display names (`"en-US"`, `"vi-VN"`), resolved via `PluginInfo::display_name(lang)`.
- **Registry Hook & Model Lookup (`photon-plugins`)**:
  - `PluginRegistry::find_by_hook(hook, disabled_plugins)` returns enabled plugins for a hook sorted by `(order, name, id)`.
  - `PluginRegistry::find_by_model(model_id, disabled_plugins)` resolves the owning plugin for a model ID dynamically.

## Reference Plugin Specifications (Legacy Bundled Plugins)

The 9 standalone crates previously under `plugins/photon-plugin-*` have been removed from the workspace; their mechanisms, `manifest.json` contracts, actions, parameters, and model catalogs are preserved here as the reference spec for external `jsonl-stdio` plugins (`apiVersion: 1`).

### Summary Matrix

| Plugin ID | Name | Version | Capabilities (`action` / `workspace` / `output`) | Permissions |
|---|---|---|---|---|
| `photon.rmbg.onnx` | Remove BG | `1.2.0` | `editor.remove_background` (`editor`, `rgba_image`) | `gpu` |
| `photon.upscale.onnx` | Upscale | `1.2.0` | `editor.upscale` (`editor`, `rgba_image`) | `gpu` |
| `photon.dewatermark` | Remove Gemini Watermark | `2.0.0` | `editor.dewatermark_gemini`, `editor.inpaint_region` (`editor`, `image`) | — |
| `photon.ai_inpaint.onnx` | AI Watermark Remover | `2.0.0` | `editor.ai_inpaint` (`editor`, `image`) | — |
| `photon.caption.onnx` | AI Caption | `2.0.0` | `caption.generate_save` (`viewer`, `text`), `editor.caption` (`editor`, `text`) | — |
| `photon.ocr.tesseract` | OCR Copy | `1.3.0` | `viewer.ocr_copy` (`viewer`, `text`) | — |
| `photon.ocr.ai` | AI OCR | `1.3.0` | `viewer.ai_ocr` (`viewer`, `translation`) | `network` |
| `photon.translate` | Read & Translate | `1.2.0` | `viewer.translate` (`viewer`, `translation`) | `network` |
| `photon.utils` | Image Utils | `1.2.0` | `utils.strip_exif`, `utils.optimize_size` (`browser`, `rgba_image`) | — |

### Detailed Plugin Mechanisms & Contracts

#### 1. `photon.rmbg.onnx` — Remove BG (`v1.2.0`, `gpu: true`)
- **Entry**: `bin/photon-plugin-rembg`, `args: ["--device", "${device}"]`, `protocol: "jsonl-stdio"`.
- **Mechanism**: Runs ONNX foreground segmentation (`ort`) to produce an alpha matte; applies optional `feather` (`int`, `0..=20`, default `0`) and outputs an `rgba_image`.
- **Models**:
  - `u2netp` (`u2netp.onnx`, tier `fast`, ~4.7 MB, `danielgatis/rembg` v0.0.0)
  - `silueta` (`silueta.onnx`, tier `fast`, ~44.2 MB, `danielgatis/rembg` v0.0.0)
  - `rmbg-1.4` (`rmbg-1.4.onnx`, tier `balanced`, ~44.5 MB, `briaai/RMBG-1.4` quantized ONNX)
  - `isnet-general-use` (`isnet-general-use.onnx`, tier `high`, ~176 MB)
  - `isnet-anime` (`isnet-anime.onnx`, tier `anime`, ~176 MB)
  - `birefnet-general-lite` (`birefnet-general-lite.onnx`, tier `high`, ~214 MB)
  - `u2net_human_seg` (`u2net_human_seg.onnx`, tier `portrait`, ~176 MB)
  - `birefnet-portrait` (`birefnet-portrait.onnx`, tier `portrait`, ~973 MB)
- **Quick Actions (`browser`, `viewer`)**: `rembg-general` (`modelId: "rmbg-1.4"`, `feather: 0`), `rembg-anime` (`modelId: "isnet-anime"`, `feather: 0`).

#### 2. `photon.upscale.onnx` — Upscale (`v1.2.0`, `gpu: true`)
- **Entry**: `bin/photon-plugin-upscale`, `args: ["--device", "${device}"]`, `protocol: "jsonl-stdio"`.
- **Mechanism**: Tiled ONNX super-resolution (`tileSize`: `int`, `64..=512`, default `192`) with overlap blending, outputting `rgba_image`.
- **Models**:
  - `swin2sr-x2` (`swin2sr-x2.onnx`, tier `upscale-2x`, ~21.5 MB, `Xenova/swin2SR-classical-sr-x2-64`)
  - `realesr-general-x4v3` (`realesr-general-x4v3.onnx`, tier `upscale-4x`, ~4.9 MB, `thisismert/real-esrgan-onnx`)
  - `realesrgan-x4plus-anime` (`realesrgan-x4plus-anime.onnx`, tier `anime`, ~17.9 MB)
  - `realesrgan-x4plus` (`realesrgan-x4plus.onnx`, tier `upscale-4x`, ~67.0 MB)
- **Quick Actions (`browser`, `viewer`)**: `upscale-2x` (`swin2sr-x2`), `upscale-4x-general` (`realesr-general-x4v3`), `upscale-4x-anime` (`realesrgan-x4plus-anime`), all with `tileSize: 192`.

#### 3. `photon.dewatermark` — Remove Gemini Watermark (`v2.0.0`)
- **Entry**: `bin/photon-plugin-dewatermark`, `protocol: "jsonl-stdio"`.
- **Capabilities (`editor`, output `image`)**:
  1. `editor.dewatermark_gemini` ("Remove Gemini"): Reverse alpha-blending using bundled Gemini logo alpha maps (`assets/`), with `size` (`"auto" | "48" | "96"`), `logoValue` (`255.0`), and `force` (`bool`).
  2. `editor.inpaint_region` ("Inpaint Region"): General watermark region removal using mask/region bounds.
- **Quick Actions (`browser`, `viewer`)**: `dewatermark-gemini-auto` ("Auto Detect", `force: false`), `dewatermark-gemini-force` ("Force Clean", `force: true`).

#### 4. `photon.ai_inpaint.onnx` — AI Watermark Remover (`v2.0.0`)
- **Entry**: `bin/photon-plugin-ai-inpaint`, `protocol: "jsonl-stdio"`.
- **Mechanism**: Two-stage ONNX pipeline (`editor.ai_inpaint`, output `image`):
  1. **Detector** (`tier: detector`): `watermark-yolo26` (~9.8 MB, `ayan4m1/Watermark-Detection-YOLO26-ONNX`) or `watermark-yolo11` (~11.1 MB) detects watermark bounding boxes with `confidence` + `maskPadding`.
  2. **Inpainter** (`tier: inpaint`): `migan-pipeline-v2` (~28.1 MB, `andraniksargsyan/migan`) or `lama-fp32` (~208 MB, `Carve/LaMa-ONNX`) fills masked regions.
- **Quick Actions (`browser`, `viewer`)**: `ai-dewatermark-yolo26-migan` ("Fast", conf `0.45`), `ai-dewatermark-yolo11-migan` ("Balanced", conf `0.35`), `ai-dewatermark-yolo26-lama` ("High Quality", conf `0.35`), all with `maskPadding: 8`.

#### 5. `photon.caption.onnx` — AI Caption (`v2.0.0`)
- **Entry**: `bin/photon-plugin-caption`, `protocol: "jsonl-stdio"`.
- **Mechanism**: WD v3 ONNX image tagger (`wd-vit-tagger-v3` ~378.5 MB or `wd-swinv2-tagger-v3` ~467.5 MB + `wd-v3-selected-tags.csv` vocabulary). Surfaces `meta.captionText` (`output: "text"`).
- **Capabilities**: `caption.generate_save` (`viewer`, writes sidecar/text), `editor.caption` (`editor`).
- **Quick Actions (`browser`, `viewer`)**: `caption-wd-vit-save` ("WD-ViT"), `caption-wd-swinv2-save` ("SwinV2") with `generalThreshold: 0.35`, `characterThreshold: 0.85`, `replaceUnderscores: true`.

#### 6. `photon.ocr.tesseract` — OCR Copy (`v1.3.0`)
- **Entry**: `bin/photon-plugin-ocr`, `protocol: "jsonl-stdio"`.
- **Mechanism**: Invokes system `tesseract` CLI with `--tessdata-dir` pointing to downloaded `.traineddata` packs (`ocr-vie`, `ocr-jpn`, `ocr-jpn_vert`, `ocr-chi_sim`, `ocr-kor` from `tesseract-ocr/tessdata_fast`). Capability `viewer.ocr_copy` (`viewer`, `output: "text"`), returns `meta.clipboardText`.
- **Quick Actions (`browser`, `viewer`)**: `ocr-copy-en` (`eng`), `ocr-copy-vi` (`vie`), `ocr-copy-jpn` (`jpn`), `ocr-copy-zh` (`chi_sim`), `ocr-copy-ko` (`kor`).

#### 7. `photon.ocr.ai` — AI OCR (`v1.3.0`, `network: true`)
- **Entry**: `bin/photon-plugin-ai-ocr`, `protocol: "jsonl-stdio"`.
- **Mechanism**: Capability `viewer.ai_ocr` (`viewer`, `output: "translation"`). Runs either **Manga-OCR** (`manga-ocr-encoder.onnx` ~22.4 MB + `manga-ocr-decoder.onnx` ~118.1 MB + `manga-ocr-vocab.txt` from `l0wgear/manga-ocr-2025-onnx`) or **RapidOCR** (`ch_PP-OCRv4_det_infer.onnx` ~4.7 MB + `ch_PP-OCRv4_rec_infer.onnx` ~10.9 MB + `ppocr_keys_v1.txt` from `PaddleOCR`), returning structured `meta.translation` blocks.

#### 8. `photon.translate` — Read & Translate (`v1.2.0`, `network: true`)
- **Entry**: `bin/photon-plugin-translate`, `protocol: "jsonl-stdio"`.
- **Mechanism**: Capability `viewer.translate` (`viewer`, `output: "translation"`). Performs OCR (`tesseract` or pre-extracted blocks) + machine translation via Google Web or LibreTranslate endpoints, returning `meta.translation` blocks (`origText`, `transText`, normalized bbox).

#### 9. `photon.utils` — Image Utils (`v1.2.0`)
- **Entry**: `bin/photon-plugin-utils`, `protocol: "jsonl-stdio"`.
- **Capabilities (`browser`, `output: "rgba_image"`)**:
  - `utils.strip_exif` ("Strip EXIF / GPS"): Re-encodes image without EXIF/GPS metadata.
  - `utils.optimize_size` ("Optimize Size"): Re-encodes with `jpegQuality` (default `86`).
- **Quick Actions (`browser`, `viewer`)**: `utils-strip-exif`, `utils-optimize-size`.

## Installing & Managing External Plugins

Install any external plugin folder through the UI via **Settings → Plugins → Install Plugin…** (`Command::PluginInstall`), refresh discovery with **Reload Plugins** (`Command::PluginRefresh` / `Command::AiRefreshPlugins`), or download/manage on-demand `.onnx` weights and OCR `.traineddata` packs (`Command::AiDownloadModel`, `Command::AiDeleteModel`, `Command::AiSetCustomModel`).

## Third-Party Runtime & Model Notes

- Photon's core crates are MIT-licensed (`LICENSE`).
- External `.onnx` weights, `.traineddata` language packs, and optional `.heic`/`.heif` external CLI decoders (`heif-convert`, `avifdec`, `ffmpeg`, `magick`) are not bundled in the repository and remain governed by their respective upstream licenses.
