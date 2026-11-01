# KNOWN-ISSUES — Đã sửa & còn mở

## Đã sửa (M4)

### 1. `view.flip` không vẽ gì lên canvas
- Triệu chứng: `view.flip` trả `flip_h: true`, nhưng ảnh trên màn hình không đổi.
- Nguyên nhân: `PhotoDoc` có `flip_h/flip_v`, `Canvas` chỉ có `_rotation`.
- Sửa: `Canvas.set_flip()` + `painter.scale(-1, 1)` sau `painter.rotate()`.
- Chống tái phát: `tests/test_canvas.py::test_flip_h_mirrors_rendering` (đọc pixel).

### 2. Undo/redo là no-op
- Triệu chứng: `edit.undo` trả `undone: true`, ảnh vẫn nghiêng.
- Nguyên nhân: chỉ pop/push stack, có `# TODO: Actually revert transform`.
- Sửa: `Transform` lưu `before`/`after` snapshot của state; undo/redo `_restore()`.
- Chống tái phát: `test_undo_restores_state`, `test_undo_shortcut_reverts_canvas`.

### 3. `file.save_copy` bỏ mọi edit
- Triệu chứng: save copy ra file y hệt ảnh gốc.
- Nguyên nhân: decode lại ảnh gốc rồi encode, không áp stack.
- Sửa: duyệt `doc.transforms` theo đúng thứ tự, áp rotate → flip → resize.
- Chống tái phát: `test_save_copy_applies_edits`, `_flips_pixels`, `_resizes`.

### 4. `file.delete` không xóa file
- Triệu chứng: `trashed_to: null`, file vẫn còn.
- Nguyên nhân: TODO, chỉ emit event.
- Sửa: `core/trash.py` theo freedesktop spec (`Trash/files` + `.trashinfo`).
- Kèm theo: UI `Delete` trước đây chỉ xóa view — nay gọi `file.delete` (một code path).
- Chống tái phát: `test_file_delete_moves_to_trash`, `test_delete_advances_to_next_image`.

### 5. Decode ảnh chặn UI thread
- Triệu chứng: ảnh lớn làm cửa sổ đứng hình khi mở.
- Nguyên nhân: `decode_image` + `QPixmap(path)` chạy đồng bộ trên GUI thread.
- Sửa: `ui/loader.py` decode trên `QThreadPool` → `QImage`; `probe_image()` đọc
  header cho metadata. `file.open` chờ trong nested event loop (UI vẫn vẽ).
- Chống tái phát: `test_image_loads_in_background`.

### 6. `edit.set` thiếu `resize`
- Triệu chứng: docs quảng cáo `resize` nhưng truyền vào bị bỏ qua.
- Sửa: `PhotoDoc.resize()` + bake khi save; undo/redo có resize.
- Chống tái phát: `test_edit_set_resize`, `test_save_copy_resizes`.

### 7. Test portal treo khi có QApplication
- Triệu chứng: thêm test UI xong, cả suite treo ở `test_portal_save_file_creates_handle`.
- Nguyên nhân: test gọi nhánh fallback mở `QFileDialog` modal thật.
- Sửa: stub `_open_via_qt` / `_save_via_qt` trong test.

### 8. `ui.key "Delete"` không khớp
- Triệu chứng: `no action for key: Delete`; `QKeySequence` chuẩn hoá thành `Del`.
- Sửa: so khớp bằng `QKeySequence.matches()` thay vì so chuỗi.

### 9. Ảnh điện thoại bị sai hướng
- Triệu chứng: ảnh chụp dọc hiện/lưu nằm ngang; width/height ngược.
- Nguyên nhân: canvas dùng `exif_transpose`, `save_copy` decode lại không transpose.
- Sửa: `decode_image()` mặc định `apply_exif_orientation=True`;
  `probe_image()` đổi chỗ width/height cho orientation 5-8. Một pipeline duy nhất.
- Chống tái phát: `test_exif_orientation_applied_everywhere`.

### 10. Pan kéo ảnh ra khỏi màn hình
- Triệu chứng: kéo chuột là ảnh trôi mất, chỉ còn nền xám; zoom nhỏ vẫn pan được.
- Nguyên nhân: `Canvas._offset` cộng delta vô điều kiện, không có biên.
- Sửa: `offset_bounds()` = `max(0, (kích thước hiệu dụng - khung nhìn) // 2)`,
  `_clamp_offset()` gọi sau pan/zoom/rotate/resize/set_image; ảnh fit thì khoá giữa.
- Chống tái phát: `test_pan_is_clamped_to_image_bounds`,
  `test_fitted_image_cannot_be_panned`, `test_drag_pan_stops_at_bounds`,
  `test_zoom_out_pulls_offset_back_in_bounds`, `test_rotation_recomputes_bounds`.

### 11. `view.compare` không so sánh song song
- Triệu chứng: bật compare chỉ đổi rotation tạm thời, không có khung thứ hai.
- Nguyên nhân: compare là cờ view, không có widget/pane riêng.
- Sửa: `ui/compare_pane.py` (canvas thứ hai bên phải, nhận drag & drop, caption
  tên file); `view.compare` ẩn/hiện pane, giữ ảnh tham chiếu khi ẩn.
- Chống tái phát: `tests/test_compare_pane.py`, `scenarios/03_compare_frames.yaml`.

### 12. View state dùng chung giữa 2 khung
- Triệu chứng: zoom/rotate khung compare kéo theo khung chính (và ngược lại);
  toolbar không rõ đang điều khiển khung nào.
- Nguyên nhân: compare pane link thẳng vào canvas chính (một nguồn view state).
- Sửa: mỗi `Canvas` tự giữ zoom/rotation/flip/pan; thêm khái niệm **active
  frame** (`main` | `compare`). Click/lăn chuột trên khung nào thì khung đó
  active (viền xanh 2 px); toolbar + mọi lệnh `view.*` chỉ áp lên khung active
  (có thể chỉ định bằng `frame`). Ẩn khung compare tự đưa active về `main`.
- Chống tái phát: `test_click_activates_frame`,
  `test_toolbar_commands_only_touch_active_frame`, `test_fit_only_touches_active_frame`,
  `test_hiding_compare_focuses_main`, `scenarios/03_compare_frames.yaml`.

---

## Đã sửa (M5 — bắt đầu edit tools)

### 1. Show Edit Tools chỉ mở sidebar phải
- Triệu chứng: bấm "Show Edit Tools" / `F9` chỉ hiện panel phải; panel trái
  không bao giờ mở cùng.
- Nguyên nhân: `_on_edit_tools_clicked()` gọi `toggle_sidebar("right")`.
- Sửa: `MainWindow.set_edit_tools_visible()` / `toggle_edit_tools()` bật/tắt cả
  hai panel như một khối (nếu một bên đang ẩn thì mở cả hai). `sidebar.show`
  nhận thêm `side: "both"|"edit_tools"`, `sidebar.state` trả `left`/`right`.
- Chống tái phát: `tests/test_sidebars.py`, `scenarios/01_basic_viewer.yaml`.

### 2. `F9` bị ambiguous (hai QAction cùng shortcut)
- Triệu chứng: bấm `F9` không có gì xảy ra; log Qt báo ambiguous shortcut.
- Nguyên nhân: toolbar gắn `F9` cho Show Edit Tools, window lại gắn `F9` cho
  toggle sidebar phải.
- Sửa: bỏ action `F9` trùng ở window; `F9` thuộc về toolbar (mở cả hai panel).
- Chống tái phát: `test_f9_shortcut_toggles_both`.

### 3. `ui.key` không bấm được shortcut của toolbar
- Triệu chứng: `ui.key {"key":"F9"}` (và `Ctrl+O`) trả
  `no action for key`, dù người dùng bấm được.
- Nguyên nhân: `ui_key()` chỉ quét `window.actions()`, không thấy QAction của
  toolbar con.
- Sửa: quét thêm `findChildren(QAction)`; chặn `key` rỗng để không trigger nhầm
  action không có shortcut.
- Chống tái phát: `test_f9_shortcut_toggles_both`,
  `test_empty_ui_key_does_not_trigger_an_action`.

### 4. Tiêu đề panel chưa đúng vai trò
- Sửa: trái = **Edit tools** (`sidebar.left`), phải = **Tool options**
  (`sidebar.right`), cập nhật cả `en.json`/`vi.json`.
- Chống tái phát: `tests/test_i18n.py` assert cả hai tiêu đề.

---

## Còn mở

### Trash không có Undo
File vào trash đúng spec nhưng chưa có lệnh khôi phục (`file.restore`) hay UI
"Undo delete".

### Trash cross-device chậm
`shutil.move` xử lý được khác filesystem (copy + unlink) nhưng file lớn sẽ chậm
và không có phản hồi tiến trình.

### Nội dung sidebar vẫn là placeholder
Layout đã có: trái **Edit tools**, phải **Tool options**, mở/ẩn cùng lúc bằng
`F9`. Nhưng cả hai vẫn chỉ hiện tiêu đề + "(Coming soon)";
`sidebar.set_content` mới emit event, chưa gắn widget thật (M5).

### Compare + active frame đang tạm ẩn
`core/features.py` đặt `COMPARE_ENABLED = False`: nút Compare, khung phải và
viền focus/active frame không hiện; `view.compare`, `compare.*`, `frame.*`
trả `PHOTON-CTL-204`; `scenarios/03_compare_frames.yaml` tự skip.
Đổi cờ thành `True` để bật lại (UI + automation + scenario). Khi bật lại, nhớ
đổi assertion `bottom_visible not_contains bottom_compare` trong
`scenarios/01_basic_viewer.yaml`.

### Không có single-instance
Mở file lần hai sẽ khởi động process mới, không chuyển sang cửa sổ đang chạy.

### Ảnh cực lớn vẫn giữ full-res trong RAM
Loader decode full resolution rồi scale khi vẽ. Ảnh 100 MP sẽ tốn ~1.6 GB RGBA.
Cần downsample theo kích thước canvas cho ảnh siêu lớn.
