# Photon — Automation Command Reference

Toàn bộ lệnh của `photon-ctl`, params và return.

## Format

```bash
photon-ctl <command> '<json-args>'
```

Mọi lệnh trả JSON: `{"ok": true, "data": {...}}` hoặc `{"ok": false, "error": "PHOTON-XXX-NNN", "message": "..."}`.

---

## App

### `app.info`
Return: `{"version", "pid", "platform", "headless", "socket", "uptime_s"}`

### `app.doctor`
Kiểm tra sức khoẻ: socket, log file, cache dir, lỗi gần đây.
Return: `{"socket_ok", "log_ok", "cache_ok", "recent_errors": [...]}`

### `app.selftest`
Chạy self-test nội bộ (decode ảnh mẫu, encode, transform).
Return: `{"passed": [...], "failed": [...]}`

### `app.quit`
Đóng app sạch sẽ (flush log, đóng socket). Return: `{"quitting": true}`

---

## File

### `file.open {"path": "/abs/path.jpg"}`
Mở ảnh vào canvas. Đường dẫn tuyệt đối hoặc tương đối CWD của app.
Ảnh tự động **fit khung** (fit-to-window) và rotation reset về 0 ngay sau khi mở.
Return: `{"path", "width", "height", "format", "size_bytes"}`
Errors: `PHOTON-FILE-101` (not found), `PHOTON-DEC-101` (decode failed)

### `file.next` / `file.prev`
Chuyển ảnh kế/trước trong cùng folder.
Return: `{"index", "total", "path"}`

### `file.info`
Return: `{"path", "width", "height", "format", "mode", "size_bytes", "exif": {...}}`

### `file.save_copy {"path": "...", "format": "png|jpg|webp", "quality": 90}`
Lưu bản sao **đã áp toàn bộ edit stack** (rotate → flip → resize, đúng thứ tự
người dùng thao tác). **Không ghi đè ảnh gốc**; mặc định ghi ra
`<stem> (edited).<ext>`.
Return: `{"saved_to", "format", "quality", "size_bytes", "width", "height"}`

### `file.delete`
Chuyển ảnh hiện tại vào thùng rác freedesktop: `$XDG_DATA_HOME/Trash/files/`
(hoặc `~/.local/share/Trash`), kèm file `info/<name>.trashinfo` chứa đường dẫn
gốc. Nếu đang mở folder, app tự nhảy sang ảnh kế tiếp và bỏ file khỏi danh sách.
Return: `{"trashed_to": "/…/Trash/files/a.png", "path": "/…/a.png"}`

---

## Frames (khung ảnh)

> ⚠️ **Tính năng đang tạm ẩn** (`src/photon/core/features.py` →
> `COMPARE_ENABLED = False`): nút Compare, khung phải và focus/active frame
> không hiện. Các lệnh `view.compare`, `compare.*`, `frame.*` vẫn tồn tại
> nhưng trả lỗi `PHOTON-CTL-204` ("Feature temporarily disabled: compare").
> Đổi cờ thành `True` là bật lại toàn bộ (UI + automation + scenario 03).

Cửa sổ có **2 khung độc lập**: `main` (khung chính) và `compare` (khung tham
chiếu bên phải). Mỗi khung giữ zoom/rotation/flip/pan riêng.

**Mọi lệnh `view.*` chỉ tác động lên khung đang *active*** (mặc định là `main`).
Click chuột hoặc lăn chuột trên một khung sẽ đặt khung đó làm active — khung
active có viền xanh 2 px. Có thể chỉ định khung bằng tham số `frame`:
`{"frame": "main"|"compare"}`; bỏ trống = khung active.

Trên khung `main`, rotation/flip là **sửa ảnh** (vào document, undo được). Trên
khung `compare` chúng chỉ đổi cách hiển thị ảnh tham chiếu, không sửa document.

### `frame.activate {"frame": "main"|"compare"}`
Đặt khung active. Bật khung compare nếu nó đang ẩn. Ẩn khung compare sẽ tự
đưa active về `main`. Return: `{"active": "compare"}`

### `frame.state`
Trạng thái cả hai khung + khung đang active.
Return: `{"active": "main", "frames": {"main": {"active": true, "path": "/a.png",
"width": 400, "height": 200, "zoom": 1.0, "rotation": 0, "flip_h": false,
"flip_v": false, "offset_x": 0, "offset_y": 0, "fit": true}, "compare": {...}}}`

---

## View

Mọi lệnh dưới đây nhận thêm `frame` (mặc định: khung active) và trả về `frame`
đã áp dụng.

### `view.zoom {"factor": 1.5}` hoặc `{"level": "in"|"out"}`
Return: `{"zoom": 1.5, "frame": "main"}`

### `view.pan {"dx": 120, "dy": -40}`
Pan theo delta pixel, **kẹp trong biên ảnh** (cùng code path với kéo chuột).
Khi zoom nhỏ hơn khung nhìn thì ảnh đứng yên ở giữa.
Return: `{"offset_x": 120, "offset_y": -40, "max_x": 300, "max_y": 100,
"at_limit": false, "frame": "main"}`

### `view.fit` / `view.actual`
Fit-to-window hoặc 100%.
Return: `{"zoom": 0.75, "frame": "main"}`

### `view.rotate {"degrees": 90}` (90 | -90 | 180)
Return: `{"rotation": 90, "frame": "main"}`

### `view.flip {"axis": "h"|"v"|"hv"}`
Return: `{"flip_h": true, "flip_v": false, "frame": "main"}`

### `view.reset`
Reset zoom + rotation + flip + resize về gốc (chỉ khung được chọn).
Return: `{"zoom": 1.0, "rotation": 0, "frame": "main"}`

### `view.fullscreen {"enabled": true|false}`
Bật/tắt toàn màn hình (bỏ trống `enabled` = toggle). Trạng thái thật đọc lại
bằng `window.state`. Return: `{"fullscreen": true}`

### `view.compare {"enabled": true|false}`
Bật/tắt **khung so sánh song song bên phải** canvas. Bỏ trống `enabled` = toggle.
Lần đầu bật, khung được nạp ảnh đang mở (bản trên đĩa) làm ảnh tham chiếu; ẩn
khung **không** xoá ảnh tham chiếu. Hai khung **không** chia sẻ view state:
zoom/rotation/flip của khung này không ảnh hưởng khung kia.
Return: `{"active": true}`

### `compare.state`
Trạng thái khung so sánh.
Return: `{"visible": true, "active": false, "path": "/tmp/ref.png", "width": 400,
"height": 200, "zoom": 1.0, "rotation": 0, "flip_h": false, "flip_v": false,
"offset_x": 0, "offset_y": 0, "fit": true}`

### `compare.set_image {"path": "/abs/ref.png"}`
Đặt ảnh tham chiếu vào khung (decode ở worker thread, cùng code path với
drag & drop). Tự bật khung nếu đang ẩn và **đặt khung compare làm active**.
Return: như `compare.state`.

### `compare.clear`
Bỏ ảnh tham chiếu khỏi khung. Return: `{"path": null}`

---

## Edit

### `edit.set {"rotate": 90, "flip": "h", "resize": {"max": 1920}}`
Áp transform vào stack (undo-able). `resize.max` giới hạn cạnh dài khi lưu,
không phá pixel gốc trên canvas.
Return: `{"stack_len": 2, "rotation": 90, "flip_h": true, "flip_v": false, "resize_max": 1920}`

### `edit.undo` / `edit.redo` / `edit.reset`
Undo/redo khôi phục **đúng state** (rotation, flip, resize), canvas được vẽ lại
ngay. Return: `{"undone": true, "stack_len": 1, "rotation": 0, "flip_h": false,
"flip_v": false, "resize_max": null}`

---

## Sidebar

### `sidebar.show {"side": "left"|"right"|"both", "visible": true|false}`
Show/hide one sidebar or **both at once** (`side: "both"` hoặc `"edit_tools"`).
Return một bên: `{"side": "right", "visible": true}`; cả hai:
`{"side": "both", "visible": true, "left": true, "right": true}`.
`side` lạ trả `PHOTON-CTL-202` (invalid arguments).

### `sidebar.state {"side": "left"|"right"|"both"}`
Một bên: `{"side", "visible", "width", "content": "empty"}`.
Cả hai: `{"side": "both", "visible": left and right, "left": bool,
"right": bool, "width": {"left": 240, "right": 280}, "content": "empty"}`.

### `sidebar.set_content {"side": "right", "widget": "edit_tools"}`
(Cho tương lai) Gắn widget vào side bar.

---

## Toolbar

### `toolbar.click {"button": "open"|"edit_tools"}`
Giả lập click nút toolbar — **cùng code path** với click chuột thật.
Return: `{"clicked": "open"}`

### `toolbar.state`
Return: `{"buttons": ["open", "edit_tools"], "bottom": [...], "bottom_visible": [...], "bottom_checked": [...], "centered": true, "margins": {"left": 498, "right": 498}}`

`bottom` là thứ tự nút viewer toolbar dưới (trái → phải):
`bottom_zoom_in`, `bottom_zoom_out`, `bottom_fit`, `bottom_prev`,
`bottom_fullscreen`, `bottom_next`, `bottom_compare`, `bottom_rotate_left`,
`bottom_rotate_right`. Không có nút delete. `bottom_visible` là các nút đang
hiện (`bottom_compare` biến mất khi cờ compare tắt). Nút `bottom_fit` chính là
fit-to-window (checked khi **khung active** đang ở chế độ fit). `centered` đo
sai lệch 2 lề (≤ 4 px) và chỉ tính nút đang hiện.

Mọi nút zoom/fit/rotate chỉ áp dụng cho **khung đang active**; click vào một
khung (kể cả qua `ui.click {"widget": "compare_canvas"}`) để đổi khung active.
Nút `bottom_compare` bật/tắt khung so sánh.

---

## Dialog

### `dialog.open {"type": "open"|"save", "filters": ["*.jpg", "*.png"]}`
Mở XDG portal file dialog. Trả handle ngay, không block.
Return: `{"handle": "photon-dialog-1"}`

### `dialog.result {"handle": "photon-dialog-1"}`
Return: `{"status": "pending"|"done"|"cancelled", "paths": [...]}`

---

## UI (debug)

### `shot {"name": "viewer"}` hoặc `{"widget": "canvas"}`
Chụp màn hình app (hoặc widget cụ thể) → `~/.cache/photon/shots/<name>.png`.
Return: `{"path", "width", "height", "blank": false, "dpr": 1.0}`

### `wait {"condition": "viewer.loading == false", "timeout": 10}`
Chờ điều kiện (giây). Condition là biểu thức so sánh getter.
Getters: `view.zoom`, `view.rotation`, `view.flip_h`, `view.flip_v`,
`view.resize_max`, `viewer.loading`, `file.path`,
`file.width`, `file.height`, `file.format`, `window.width`, `window.height`,
`window.fullscreen`, `window.compare`, `window.fit`, `window.active_frame`,
`frame.active` ("main"|"compare"), `frame.main`, `frame.compare`,
`canvas.active`, `canvas.zoom`,
`canvas.rotation`, `canvas.flip_h`, `canvas.flip_v`, `canvas.width`,
`canvas.height`, `canvas.fit`, `compare.active`, `compare.enabled`,
`compare.visible`, `compare.zoom`, `compare.rotation`, `compare.path`,
`app.headless`, `app.version`, `app.pid`.
Return: `{"met": true, "waited": 0.3, "value": 0.75}`

Ghi chú: `file.open` decode ảnh trên worker thread (không chặn GUI), nhưng lệnh
tự chờ ảnh load xong trước khi trả về — nên assert ngay sau `file.open` vẫn
deterministic.

### `ui.events {"n": 30}`
Return: `{"events": [{"event": "toolbar.clicked", "button": "open", ...}]}`

### `ui.click {"widget": "toolbar_open", "x": 10, "y": 10}`
Click vào widget theo objectName (hoặc toạ độ).

### `ui.key {"key": "Ctrl+O"}`
Gửi phím tắt vào app.

### `window.state`
Return: `{"visible", "fullscreen", "compare", "active_frame", "title", "canvas": {"width", "height", "active", "zoom", "rotation", "flip_h", "flip_v", "offset_x", "offset_y", "fit"}, "compare_pane": {"visible", "active", "path", "width", "height", "zoom", "rotation", "flip_h", "flip_v", "offset_x", "offset_y", "fit"}, "geometry": {...}}`

### `window.resize {"width": 1280, "height": 800}`
Return: `{"width", "height"}`

---

## Log

### `log.tail {"n": 40}`
Return: `{"lines": ["event=... ", ...]}`

### `log.errors {"n": 10}` hoặc `{"n": 30, "exclude": ["cmd.error"]}`
Return: `{"errors": [{"code": "PHOTON-DEC-101", "message": "..."}]}`
`exclude` bỏ qua các event chỉ định (ví dụ lỗi cố ý của bước `expect_fail`).

---

## Scenario YAML

Setup chạy local trước các steps, hỗ trợ: `mkdir`, `copy`, `touch`, `rm`, `run`
(shell command). Assert hỗ trợ `equals`, `not_equals`, `greater_than`,
`less_than`, `contains`, `not_contains` — viết trực tiếp
(`greater_than: 0`) hoặc qua `op`.

Scenario có thể khai báo `requires_feature: compare` (đầu file): khi tính năng
đang bị ẩn bởi `core/features.py`, scenario được **skip** và `result.json` ghi
`skipped: 1` + `skip_reason` thay vì fail từng bước.

```yaml
name: 01_basic_viewer
setup:
  - mkdir: /tmp/photon-fixtures
  - copy: {src: samples/a.jpg, dst: /tmp/photon-fixtures/a.jpg}
  - run: >-
      .venv/bin/python -c "from PIL import Image;
      Image.new('RGB', (320, 240)).save('/tmp/photon-fixtures/test.jpg')"
steps:
  - cmd: file.open
    args: {path: /tmp/photon-fixtures/a.jpg}
  - cmd: wait
    args: {condition: "viewer.loading == false", timeout: 10}
  - cmd: shot
    args: {name: viewer}
    assert:
      - property: blank
        equals: false
  - cmd: view.rotate
    args: {degrees: 90}
    assert:
      - property: rotation
        equals: 90
  - cmd: sidebar.show
    args: {side: right, visible: true}
    assert:
      - property: visible
        equals: true
```

Chạy:
```bash
photon-ctl --script scenarios/01_basic_viewer.yaml --out /tmp/sc-viewer
```

Kết quả: `/tmp/sc-viewer/result.json` — từng step + assert pass/fail.
Bước biết-trước-lỗi: `expect_fail: true`.
