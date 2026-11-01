# Photon — Automation-First Image Viewer/Editor

> **PySide6, Python 3.12, Linux XDG portal dialogs, Windows Photo Viewer-inspired UI**

## Overview

Photon is an automation-first image viewer/editor built for vibe-coding, rapid debugging, and automated testing. Every feature is accessible via CLI commands, every action is logged with structured signals, and the app can screenshot itself for regression testing.

**Key Design Principles:**
- **Single Code Path**: Toolbar buttons, keyboard shortcuts, and automation commands all call the same `AutomationAPI` methods.
- **Observable & Named**: Commands use `<domain>.<verb>` format, logs use `event=<domain>.<action>`, errors use `PHOTON-<AREA>-NNN`.
- **Headless & GUI Dual-Mode**: Run full GUI or pure headless server (`--headless`) for CI/testing without display compositor.
- **Structured Logging**: Every action logged with timing, status, metadata in JSON lines + 2000-event ring buffer.
- **XDG Portal Dialogs**: Native Linux file dialogs via `org.freedesktop.portal.Desktop` D-Bus.

---

## Quick Start

```bash
# Install dependencies
uv sync

# Run GUI (automation enabled by default)
uv run photon [optional_image_path]

# Run headless (no display required)
uv run photon --headless &

# Control running instance
uv run photon-ctl app.info
uv run photon-ctl file.open '{"path":"/path/to/image.jpg"}'
uv run photon-ctl view.zoom '{"factor":1.5}'
uv run photon-ctl shot '{"name":"viewer"}'
uv run photon-ctl app.quit

# Run automated scenarios
uv run photon-ctl --script scenarios/01_basic_viewer.yaml
```

---

## Architecture

```
src/photon/
├── core/                   # Pure Python (ZERO Qt imports!)
│   ├── log.py              # Structured logging, ring buffer, crash hook
│   ├── errors.py           # Error codes & PhotonError classes
│   ├── events.py           # EventBus: in-memory + file journal
│   ├── model.py            # PhotoDoc: image path, transform stack, undo/redo
│   ├── features.py         # Feature switches (COMPARE_ENABLED = False)
│   └── settings.py         # JSON config in ~/.config/photon
├── imaging/                # Pillow-based decode/encode/ops (ZERO Qt imports!)
│   ├── decode.py           # Load images, EXIF, thumbnails, header probe
│   ├── encode.py           # Save with quality/format options
│   └── ops.py              # Rotate, flip, resize, crop
├── ui/                     # Qt widgets (every widget has objectName)
│   ├── window.py           # Main window: toolbar, sidebars, canvas
│   ├── canvas.py           # Zoom/pan/fit image display (pan clamped to bounds)
│   ├── compare_pane.py     # Side-by-side reference frame (hidden: see core/features.py)
│   ├── loader.py           # Background decode (QThreadPool → QImage)
│   ├── toolbar.py          # Top toolbar (Open, Show Edit Tools)
│   ├── bottom_toolbar.py   # Floating viewer toolbar (zoom/rotate/compare…)
│   ├── sidebar.py          # Left "Edit tools" / right "Tool options" panels
│   └── statusbar.py        # Zoom %, dimensions, filename
├── portal/                 # XDG Desktop Portal dialogs
│   └── filedialog.py       # org.freedesktop.portal.Desktop OpenFile/SaveFile
└── automation/             # Headless IPC & scenario testing
    ├── server.py           # Unix domain socket JSON-line server
    ├── client.py           # photon-ctl client library
    ├── cli.py              # photon-ctl command line tool
    ├── commands.py         # Command registry
    ├── api.py              # AutomationAPI: single entry point for all actions
    └── scenario.py         # YAML scenario runner + assertions + screenshots
```

**Import Rules (enforced by import-linter):**
- `core/` and `imaging/` **cannot** import Qt, UI, features, workers, or automation
- Features cannot import widgets from other features; use `photon.events.EventBus`
- UI thread must never block >50ms; heavy work goes to thread pool

---

## UI Layout (Windows Photo Viewer-inspired)

```
┌──────────────────────────────────────────────────────────┐
│  [Open]  [Show Edit Tools]              [min][max][x]    │  ← Toolbar
├──────────┬────────────────────────────────┬──────────────┤
│  EDIT    │        CANVAS (image)          │    TOOL      │  ← Sidebars
│  TOOLS   │                                │   OPTIONS    │    (F9: cả 2)
│  (empty, │                                │              │
│  ready)  │                                │              │
├──────────┴────────────────────────────────┴──────────────┤
│        (zoom) (fit) ⏮ [▣] ⏭ (↻) (✕)                     │  ← Bottom toolbar
└──────────────────────────────────────────────────────────┘
```

**Toolbar (top):**
- **Open** (`Ctrl+O`): Open file dialog
- **Show Edit Tools** (`F9`): Show/hide **both** sidebars (left + right)

**Bottom Toolbar:**
- Zoom in/out, Fit to window, Previous/Next image, Rotate CW/CCW, Delete

**Sidebars:**
- Left: **Edit tools** — danh sách công cụ sửa
- Right: **Tool options** — tùy chọn của công cụ đang chọn
- "Show Edit Tools" / `F9` toggles both panels as one unit; `F8` toggles only the
  left panel. Visibility is saved to settings.

---

## Automation Commands

| Domain | Commands |
|--------|----------|
| **App** | `app.info`, `app.doctor`, `app.selftest`, `app.quit` |
| **File** | `file.open`, `file.next`, `file.prev`, `file.info`, `file.save_copy`, `file.delete` |
| **View** | `view.zoom`, `view.fit`, `view.actual`, `view.rotate`, `view.flip`, `view.reset` |
| **Edit** | `edit.set {rotate,flip,resize}`, `edit.undo`, `edit.redo`, `edit.reset` |
| **Sidebar** | `sidebar.show`, `sidebar.state`, `sidebar.set_content` |
| **Toolbar** | `toolbar.click`, `toolbar.state` |
| **Dialog** | `dialog.open {type:"open"/"save"}, dialog.result` |
| **UI** | `shot`, `wait {condition,timeout}`, `ui.events`, `ui.click`, `ui.key`, `window.state`, `window.resize` |
| **Log** | `log.tail`, `log.errors` |

**Examples:**
```bash
# Open image and wait for load (file.open already waits, wait is a safety net)
photon-ctl file.open '{"path":"/tmp/test.jpg"}'
photon-ctl wait '{"condition":"viewer.loading == false","timeout":10}'

# Rotate and screenshot
photon-ctl view.rotate '{"degrees":90}'
photon-ctl shot '{"name":"rotated"}'

# Toggle both sidebars (Edit tools + Tool options)
photon-ctl sidebar.show '{"side":"both","visible":true}'

# Run scenario
photon-ctl --script scenarios/02_edit_workflow.yaml --out /tmp/results
```

---

## Logging & Signals

**Every action** (click, keypress, socket command) emits:
```json
{
  "event": "view.rotate",
  "timestamp": "2026-10-31T15:30:00.123Z",
  "duration_ms": 45,
  "status": "ok",
  "meta": {"degrees": 90, "path": "/tmp/test.jpg"}
}
```

**Log files:**
- `~/.cache/photon/photon.log`: JSON lines, all events
- `~/.cache/photon/shots/*.png`: Screenshots from `shot` command
- `logs/crash-<timestamp>.log`: Tracebacks from crash handler

**Ring buffer:** Last 2000 events queryable via `photon-ctl ui.events n=50`

**Error codes:** `PHOTON-IMG-101` (decode failed), `PHOTON-CTL-202` (unknown command), etc.

---

## External Control

**Unix socket:** `~/.cache/photon/photon.sock` (0700 dir, 0600 socket)

**Signal file mode** (for environments without socket access):
```bash
photon --signal-file /tmp/photon-control.jsonl
echo '{"cmd":"file.open","args":{"path":"/tmp/a.jpg"}}' >> /tmp/photon-control.jsonl
```

**Ctrl+C shutdown:**
- SIGINT handler calls `QCoreApplication.quit()` via zero-delay timer
- `app.aboutToQuit` flushes ring buffer, closes socket, logs `event=app.quit reason=signal`

---

## XDG Portal Dialogs (Linux)

Uses `org.freedesktop.portal.Desktop` / `FileDialog` via D-Bus:
- `OpenFile(parent_window, options)` → returns handle token
- Waits for `Response` signal with selected file paths
- Fallback to `QFileDialog` if portal unavailable (old X11 systems)
- Automation: `dialog.open` returns handle, `dialog.result` returns selected file

---

## Development

```bash
# Run tests
uv run pytest -v

# Lint & type check
uv run ruff check src tests
uv run mypy src

# Format code
uv run ruff format src tests

# Check import rules
uv run lint-imports
```

---

## Documentation

- `AGENTS.md`: Agent guidelines, invariants, commands
- `docs/AUTOMATION.md`: Full command reference + scenario format
- `docs/ARCHITECTURE.md`: Data flow, import rules, threading
- `docs/DEVLOOP.md`: Chạy app offscreen, debug app đang sống, kiểm chứng pixel
- `docs/TESTING.md`: Unit, UI (pytest-qt) và scenario
- `docs/KNOWN-ISSUES.md`: Bug đã sửa + vấn đề còn mở

---

## License

MIT
