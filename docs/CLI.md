# Photon CLI & Agent Control Specification

## Desktop / Headless Executable (`photon`)
```sh
photon                         # Start GUI in Browser workspace (maximized on first run)
photon browse /path/to/dir     # Open folder in Browser
photon view /path/to/a.jpg ... # Open image(s) or .cbz/.zip/.pdf archive in Viewer
photon edit [/path/to/a.jpg]   # Open image (or empty Editor) in Editor singleton
photon --edit [/path/to/a.jpg] # Start in Editor workspace (optionally opening FILE)
photon /path/to/a.jpg          # Alias for `view`
photon --headless              # Run Core + Local IPC without GUI window
photon --safe-mode             # Factory defaults in memory + software renderer (tiny-skia)
photon --reset-layout          # Reset UI state (ui_state.json; preserves config.json Settings)
photon --reset-settings        # Reset Settings (config.json; preserves UI layout)
photon --reset-config          # Reset both Settings & UI state (alias: --reset-all; backs up .bak)
photon --reset-layout --exit   # Apply reset and exit immediately without launching GUI
photon --lang <en|vi|system>   # Override initial UI language
photon --new-window [path...]  # Open standalone window instead of forwarding to running instance
photon --viewer-window [path]  # Internal flag for spawning an independent Viewer process
```

### Environment Variables
- `PHOTON_PROFILE=dev|prod` — Override profile in debug builds (`prod` vs `dev`).
- `PHOTON_SANDBOX_DIR=<path>` — Isolate config, state, cache, logs, journal, data/plugins, and IPC socket inside `<path>`.
- `PHOTON_PLUGINS_DIR=<path>` — Override the plugin discovery directory (`<data_dir>/plugins`).
- `PHOTON_MODELS_DIR=<path>` — Override the model lookup directory used by AI/OCR `jsonl-stdio` plugins.

## Control & Agent Debug CLI (`photonctl`)
Global flags: `--socket <PATH>` (override IPC socket path), `--json` (compact JSON output for scripts/agents).

```sh
photonctl status
photonctl open /path/to/photo.png
photonctl open-folder ~/Pictures
photonctl edit /path/to/photo.png
photonctl workspace viewer
photonctl dispatch '{"type":"editor_rotate","degrees":90,"target":"whole_image"}' --expected-revision 4
photonctl dispatch '{"type":"plugin_refresh"}'
photonctl dispatch '{"type":"plugin_run","id":"photon.utils","params":{"action":"utils.strip_exif"}}'
photonctl query state
photonctl query meta
photonctl query clipboard        # Inspect EditorPixelClipboard dimensions, offsets & PNG bytes
photonctl schema
photonctl events --limit 20
photonctl reset layout           # Reset ui_state.json (online via IPC if running, or offline)
photonctl reset settings         # Reset config.json Settings (online or offline)
photonctl reset all --offline    # Offline reset of both config.json & ui_state.json with .bak backups

# Agent Debugging & Reproducibility
photonctl debug logs --last 100
photonctl debug journal --last 50 --failed-only
photonctl debug trace --request-id cli-123
photonctl debug state
photonctl debug ui-tree
photonctl debug ui-action nav.workspace.editor click
photonctl debug screenshot --output /tmp/photon-shot.png
photonctl debug replay --journal ~/.local/state/photon-dev/journal/commands.jsonl
photonctl debug bundle --output /tmp/photon-report.json

# Model Context Protocol (MCP) stdio adapter
photonctl mcp
```
