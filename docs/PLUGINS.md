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

## Bundled Standalone Plugins (`plugins/`)

All 9 ported standalone crates under `plugins/` carry `license = "MIT"` and build with `--release`:

| Crate | Plugin ID | Version | Capabilities (`action` / `workspace`) | Permissions |
|---|---|---|---|---|
| `photon-plugin-ai-inpaint` | `photon.ai_inpaint.onnx` | `2.0.0` | `editor.ai_inpaint` (`editor`) | — |
| `photon-plugin-ai-ocr` | `photon.ocr.ai` | `1.3.0` | `viewer.ai_ocr` (`viewer`) | `network` |
| `photon-plugin-caption` | `photon.caption.onnx` | `2.0.0` | `caption.generate_save` (`viewer`), `editor.caption` (`editor`) | — |
| `photon-plugin-dewatermark` | `photon.dewatermark` | `2.0.0` | `editor.dewatermark_gemini`, `editor.inpaint_region` (`editor`) | — |
| `photon-plugin-ocr` | `photon.ocr.tesseract` | `1.3.0` | `viewer.ocr_copy` (`viewer`) | — |
| `photon-plugin-rembg` | `photon.rmbg.onnx` | `1.2.0` | `editor.remove_background` (`editor`) | `gpu` |
| `photon-plugin-translate` | `photon.translate` | `1.2.0` | `viewer.translate` (`viewer`) | `network` |
| `photon-plugin-upscale` | `photon.upscale.onnx` | `1.2.0` | `editor.upscale` (`editor`) | `gpu` |
| `photon-plugin-utils` | `photon.utils` | `1.2.0` | `utils.strip_exif`, `utils.optimize_size` (`browser`) | — |

## Building & Installing Plugins

```sh
# Build all 9 standalone plugin crates in release mode
just build-plugins

# Build and install into ~/.local/share/photon/plugins (production profile)
just install-plugins

# Build and install into ~/.local/share/photon-dev/plugins (dev profile)
just install-plugins dev
```

You can also install any plugin folder through the UI via **Plugins → Manage Plugins… → Install Plugin…** (`Command::PluginInstall`), or refresh discovery with **Reload Plugins** (`Command::PluginRefresh`).

## Third-Party Runtime & Model Notes

- Photon's core crates and the 9 plugin wrappers in `plugins/photon-plugin-*` are MIT-licensed (`LICENSE`).
- ONNX-based plugins (`ai-inpaint`, `ai-ocr`, `caption`, `rembg`, `upscale`) use `ort` (`2.0.0-rc.15`), and OCR/Translate plugins (`ocr`, `translate`) invoke the system `tesseract` CLI when installed.
- External `.onnx` weights, `.traineddata` language packs, and optional `.heic`/`.heif` external CLI decoders (`heif-convert`, `avifdec`, `ffmpeg`, `magick`) are not bundled in the repository and remain governed by their respective upstream licenses.
