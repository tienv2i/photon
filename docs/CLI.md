# Photon CLI & Agent Control Specification

## Desktop / Headless Executable (`photon`)
```sh
photon                         # Start GUI in Browser workspace
photon browse /path/to/dir     # Open folder in Browser
photon view /path/to/a.jpg ... # Open image(s) in Viewer
photon edit /path/to/a.jpg     # Open image directly in Editor
photon /path/to/a.jpg          # Alias for `view`
photon --headless              # Run Core + Local IPC without GUI window
photon --safe-mode             # Factory defaults in memory + software renderer
photon --reset-layout          # Reset layout preferences (backups config.json)
photon --reset-config          # Reset all settings (backups config.json)
photon --lang <en|vi|system>   # Override initial UI language
photon --new-window [path...]  # Open standalone window instead of forwarding to running instance
```

## Control & Agent Debug CLI (`photonctl`)
Global flags: `--socket <PATH>` (override IPC socket path), `--json` (compact JSON output for scripts/agents).

```sh
photonctl status
photonctl open /path/to/photo.png
photonctl open-folder ~/Pictures
photonctl edit /path/to/photo.png
photonctl workspace viewer
photonctl dispatch '{"type":"editor_rotate","degrees":90}' --expected-revision 4
photonctl query state
photonctl schema
photonctl events --limit 20

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
