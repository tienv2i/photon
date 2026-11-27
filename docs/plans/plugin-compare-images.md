# Kế hoạch: Plugin Compare 2 Ảnh & Cơ chế Host Tái Sử Dụng

> Trạng thái: **Kế hoạch (chưa triển khai)**

## 1. Mục tiêu
Xây dựng plugin **Compare 2 Images** hoạt động theo mô hình Viewer (có thể chạy từ Browser, Viewer hoặc Plugins menu), nhưng **không hardcode logic riêng của plugin vào app**. Thay vào đó, bổ sung các cơ chế tổng quát, tái sử dụng được trong `photon-protocol`, `photon-plugins`, `photon-canvas`, `photon-viewer` và `photon-desktop` để bất kỳ plugin so sánh/kiểm tra ảnh nào khác đều dùng chung được.

---

## 2. Yêu cầu tính năng cho Plugin Compare 2 Ảnh
1. **Chọn 2 ảnh để so sánh (`Image A` & `Image B`)**:
   - Chọn 2 file từ **Browser** (multi-select), hoặc lấy ảnh đang mở trong **Viewer/Editor** làm `Image A` rồi chọn `Image B` qua file picker, hoặc chọn cả 2 từ dialog.
2. **Swap 2 ảnh (`A ↔ B`)**:
   - Đảo vị trí `Image A` và `Image B` tức thì mà không làm mất mức zoom/pan hay vị trí thanh chia.
3. **2 chế độ Compare** (đồng bộ zoom/pan giữa 2 ảnh):
   - **Side-by-Side Split (chia đôi hiển thị cùng lúc 2 ảnh)**: có đường chia ở giữa, hiển thị đồng thời nửa/khung ảnh A và ảnh B cạnh nhau.
   - **Overlay Wipe Split (2 ảnh chồng lên nhau + thanh trượt ở giữa)**: ảnh A và B chồng khít lên nhau, kéo thanh trượt dọc ở giữa qua lại (`split_x: 0.0..1.0`) để lộ phần ảnh A bên trái và ảnh B bên phải.

---

## 3. Các cơ chế Host tổng quát cần bổ sung (tránh hardcode plugin)

### A. Mở rộng `PluginParamKind` (`photon-protocol` & `photon-desktop`)
Hiện `PluginParamKind` mới có `Float`, `Int`, `Bool`, `Select`, `Color`, `String`.
- Thêm **`PluginParamKind::ImageFile`** (chọn đường dẫn file ảnh):
  - Trong `InAppDialog::PluginParams` ([`dialogs.rs`](../../crates/photon-desktop/src/app/dialogs.rs)), render sẵn ô hiển thị tên file + nút **Browse…** (`rfd`) + nút **Use Current Image** (nếu đang mở ảnh trong Viewer/Editor).
  - Tái sử dụng cho mọi plugin cần ảnh thứ 2 (Compare, Blend/Overlay, Watermark, Texture, Color Match…).

### B. Chuẩn hóa Output dạng Host View (`PluginHostView` / `PluginOutputMode`)
Hiện plugin mới trả về 1 ảnh đầu ra (`output_path`) hoặc text (`TranslationResult`).
- Thêm **declarative host view response** trong `photon-protocol` / `photon-plugins`:
  ```json
  {
    "ok": true,
    "hostView": {
      "kind": "compare_pair",
      "leftPath": "/path/to/a.png",
      "rightPath": "/path/to/b.png",
      "leftLabel": "Original",
      "rightLabel": "Edited",
      "mode": "overlay_split",
      "splitX": 0.5,
      "allowSwap": true,
      "allowPick": true
    }
  }
  ```
- App host chỉ nhận biết **`kind: "compare_pair"`** (một capability chung của Viewer/Canvas, hoàn toàn không biết tên hay ID của plugin).

### C. Reusable `CompareCanvas` trong `photon-canvas` & `photon-viewer`
Tận dụng hạ tầng GPU đã có sẵn trong [`photon-canvas/src/gpu.rs`](../../crates/photon-canvas/src/gpu.rs#L44-L45) (`compare_frame`, `compare_mode` `1 = Split` / `2 = SideBySide`, `compare_split_x`):
1. **Nâng cấp `photon-canvas` cho Compare tổng quát**:
   - Đảm bảo `compare_mode = 1` (Overlay Wipe với thanh trượt kéo `compare_split_x`) và `compare_mode = 2` (Side-by-Side Split có thanh chia ở giữa) hoạt động độc lập với Editor (dùng được trong `Viewer` hoặc `CompareView` riêng).
   - Hỗ trợ so sánh 2 ảnh khác kích thước (căn fit theo khung chung hoặc giữ tỷ lệ 1:1 theo tùy chọn) và thanh trượt có tay nắm (handle) rõ ràng ở giữa.
2. **Tái sử dụng Toolbar / Controls của Viewer**:
   - Thêm một **Compare View Overlay / Sub-mode** tái sử dụng trong `photon-viewer` / `photon-desktop`:
     - Nút **Pick Image A / Pick Image B** (cập nhật `leftPath` / `rightPath` và nạp texture GPU).
     - Nút **Swap (`A ↔ B`)** (hoán đổi `frame` và `compare_frame`).
     - Nút chuyển đổi qua lại 2 chế độ: **Side-by-Side** vs **Overlay Slider**.
     - Giữ nguyên Zoom, Pan (`Space+Drag`, Middle Mouse, Wheel), Fit (`0`), `1:1` (`1`) của Viewer.

### D. Hook điểm mở rộng (`plugin_hooks`)
- Bổ sung hook `viewer.compare_tool` (hoặc dùng lại `browser.context_menu` + `viewer.context_menu` + `viewer.quick_action` đã có).
- Plugin `photon-plugin-compare` có thể:
  - Chế độ thuần hiển thị: nhận `image_a` + `image_b` (hoặc lấy ảnh hiện tại + ảnh vừa chọn) -> trả ngay `hostView: compare_pair`.
  - Chế độ nâng cao (tùy plugin): tính ảnh sai khác (Pixel Diff / Heatmap / SSIM) thành file tạm rồi trả `hostView: compare_pair` giữa ảnh gốc và heatmap!

---

## 4. Lộ trình triển khai đề xuất
1. **`photon-protocol` & `photon-plugins`**: thêm `PluginParamKind::ImageFile` và `PluginHostView::ComparePair` (NDJSON + `jsonl-stdio`).
2. **`photon-canvas`**: hoàn thiện `compare_mode` `1` (Overlay Wipe Slider) & `2` (Side-by-Side Split) cho 2 ảnh bất kỳ (kể cả khác resolution), vẽ thanh chia + tay nắm trực quan.
3. **`photon-desktop` & `photon-viewer`**: dựng UI chọn `Image A / B`, nút `Swap`, nút chuyển `Side-by-Side / Overlay Slider` cho `ComparePair` session và picker `ImageFile` trong `PluginParams`.
4. **`plugins/photon-plugin-compare`**: tạo plugin standalone khai báo `manifest.json` với `ViewerAction` + `BrowserAction` (hỗ trợ multi-select 2 ảnh ở Browser hoặc Pick Image B ở Viewer) trả về `hostView: compare_pair`.
