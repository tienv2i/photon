# Photon — Automation-First Image Viewer/Editor

## Overview

Photon là image viewer/editor automation-first, xây dựng bằng PySide6 (Python 3.12) cho Linux với XDG portal dialogs. UI lấy cảm hứng từ Windows Photo Viewer.

**Mục tiêu chính:**
- Mọi tính năng đều control được qua CLI (`photon-ctl`)
- Mọi thao tác đều log có cấu trúc (JSON lines + ring buffer)
- Tự chụp screenshot để regression test
- Chạy headless được (không cần display) cho CI
- Debug app đang chạy mà không cần chuột

## Architecture

```
photon/
├── src/photon/
│   ├── core/           # Pure Python (ZERO Qt imports)
│   │   ├── log.py      # Structured logging, ring buffer, crash hook
│   │   ├── errors.py   # Error codes PHOTON-XXX-NNN
│   │   ├── events.py   # EventBus: in-memory + file journal
│   │   ├── model.py    # PhotoDoc: image path, transform stack, undo/redo
│   │   └── settings.py # JSON config ~/.config/photon
│   ├── imaging/        # Pillow decode/encode/ops (ZERO Qt imports)
│   ├── ui/             # Qt widgets (mỗi widget có objectName)
│   │   ├── window.py   # MainWindow: toolbar, sidebars, canvas
│   │   ├── canvas.py   # Zoom/pan/fit image display
│   │   ├── toolbar.py  # Top toolbar (Open, Show Edit Tools)
│   │   └── sidebar.py  # Left/right panels (empty, ready for edit tools)
│   ├── portal/         # XDG Desktop Portal dialogs (D-Bus)
│   └── automation/     # Headless IPC & scenario testing
│       ├── server.py   # Unix socket JSON-line server
│       ├── api.py      # AutomationAPI: single entry point
│       ├── commands.py # Command registry
│       ├── cli.py      # photon-ctl CLI
│       └── scenario.py # YAML scenario runner
├── scenarios/          # Regression test scenarios
├── tests/              # Unit tests
└── docs/               # Documentation
```

**Import Rules (enforced by import-linter):**
- `core/` + `imaging/` **không** import Qt/UI/automation
- Feature không import widget của feature khác; dùng `EventBus`
- UI thread không block >50ms; heavy work qua thread pool

## UI Layout

```
┌──────────────────────────────────────────────────────────┐
│  [Open]  [Show Edit Tools]              [min][max][x]    │  ← Toolbar
├──────────┬────────────────────────────────┬──────────────┤
│          │                                │              │
│  EDIT    │        CANVAS (image)          │    TOOL      │  ← Sidebars
│  TOOLS   │                                │   OPTIONS    │    (F9: cả 2)
│  (empty, │                                │              │
│  ready)  │                                │              │
├──────────┴────────────────────────────────┴──────────────┤
│  Status: 100% · 1920×1080 · img0.jpg                     │  ← Status bar
└──────────────────────────────────────────────────────────┘
```

**Toolbar (top):**
- **Open** (`Ctrl+O`): Mở file dialog (XDG portal)
- **Show Edit Tools** (`F9`): Ẩn/hiện **cả hai** panel trái + phải

**Sidebars:**
- Left (`F8`): **Edit tools** — danh sách công cụ sửa
- Right: **Tool options** — tùy chọn của công cụ đang chọn
- "Show Edit Tools" / `F9` bật/tắt cả 2 panel cùng lúc; `F8` chỉ bật/tắt panel
  trái. State lưu vào config.

## Automation Commands

| Domain | Commands |
|--------|----------|
| **App** | `app.info`, `app.doctor`, `app.selftest`, `app.quit` |
| **File** | `file.open`, `file.next`, `file.prev`, `file.info`, `file.save_copy`, `file.delete` |
| **View** | `view.zoom`, `view.fit`, `view.actual`, `view.rotate`, `view.flip`, `view.reset` |
| **Edit** | `edit.set`, `edit.undo`, `edit.redo`, `edit.reset` |
| **Sidebar** | `sidebar.show`, `sidebar.state`, `sidebar.set_content` |
| **Toolbar** | `toolbar.click`, `toolbar.state` |
| **Dialog** | `dialog.open`, `dialog.result` |
| **UI** | `shot`, `wait`, `ui.events`, `ui.click`, `ui.key`, `window.state`, `window.resize` |
| **Log** | `log.tail`, `log.errors` |

**Protocol:** Unix socket JSON-line
- Request: `{"cmd": "file.open", "args": {"path": "/tmp/a.jpg"}}`
- Response: `{"ok": true, "data": {...}}` hoặc `{"ok": false, "error": "PHOTON-XXX-NNN", "message": "..."}`

## Quick Start

```bash
# Install
uv sync

# Run GUI
uv run photon [optional_image_path]

# Run headless (no display)
uv run photon --headless &

# Control running instance
uv run photon-ctl app.info
uv run photon-ctl file.open '{"path":"/tmp/test.jpg"}'
uv run photon-ctl view.rotate '{"degrees":90}'
uv run photon-ctl shot '{"name":"viewer"}'
uv run photon-ctl app.quit

# Run scenario
uv run photon-ctl --script scenarios/01_basic_viewer.yaml --out /tmp/results
```

## Logging & Signals

**Mọi action** emit event:
```json
{
  "event": "view.rotate",
  "timestamp": "2026-10-31T15:30:00.123Z",
  "duration_ms": 45,
  "status": "ok",
  "meta": {"degrees": 90}
}
```

**Log files:**
- `~/.cache/photon/photon.log`: JSON lines
- `~/.cache/photon/shots/*.png`: Screenshots
- `logs/crash-<ts>.log`: Tracebacks

**Ring buffer:** 2000 events queryable via `photon-ctl ui.events n=50`

**Error codes:** `PHOTON-FILE-101` (not found), `PHOTON-DEC-101` (decode failed), `PHOTON-CTL-201` (unknown command), etc.

## Ctrl+C Shutdown

- SIGINT handler → `QCoreApplication.quit()` via zero-delay timer
- `app.aboutToQuit` → flush ring buffer, close socket, log `event=app.quit reason=signal`

## XDG Portal Dialogs

- Gọi `org.freedesktop.portal.Desktop` / `FileDialog` qua D-Bus
- `OpenFile(parent_window, options)` → handle token
- Chờ `Response` signal với selected paths
- Fallback: `QFileDialog` nếu portal không có

## Development

```bash
# Tests
uv run pytest -v

# Lint & type check
uv run ruff check src tests
uv run mypy src

# Format
uv run ruff format src tests

# Check import rules
uv run lint-imports
```

## Documentation

- `README.md`: Overview, quick start, architecture
- `AGENTS.md`: Agent guidelines, commands, rules
- `docs/AUTOMATION.md`: Full command reference + scenario format
- `docs/ARCHITECTURE.md`: Data flow, import rules, threading
- `docs/DEVLOOP.md`: Chạy app offscreen, debug app đang sống, kiểm chứng pixel
- `docs/TESTING.md`: Unit, UI (pytest-qt) và scenario
- `docs/KNOWN-ISSUES.md`: Bug đã sửa + vấn đề còn mở

## Roadmap

- [x] M1: Skeleton (core log/events, socket server, app.info/quit, Ctrl+C, headless)
- [x] M2: Viewer UI (window theo WPV, canvas zoom/pan, toolbar, statusbar, shot)
- [x] M3: Automation đầy đủ (wait/getter, scenario runner, portal dialogs)
- [x] M4: Edit + scenario regression (rotate/flip/resize/save-copy, undo/redo thật,
      delete → trash, decode nền, scenario 01–02)
- [ ] M5: Edit tools thật — **đang làm**
  - [x] Layout 2 panel: trái **Edit tools**, phải **Tool options**; nút Show Edit
        Tools / `F9` mở/ẩn cả hai như một khối (`F8` chỉ panel trái)
  - [ ] exposure/contrast/saturation/crop + preview non-destructive
  - [ ] `sidebar.set_content` gắn widget thật
- [ ] M6: Thư viện & metadata (left sidebar folder browser, EXIF panel, Recent Files menu)
- [ ] M7: Đóng gói (single-instance, drag & drop folder, `.desktop`, AppImage)

## License

MIT
