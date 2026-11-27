# Kế hoạch: Refine Edge (Select & Mask) & Bộ Filter Photoshop cho Photon

> Trạng thái: **Kế hoạch (chưa triển khai)**

## 1. Refine Edge / Select & Mask (`Select → Refine Edge...`)

### Cơ chế hoạt động (tham chiếu Photoshop)
1. **Trimap & Edge Detection (`Radius`, `Smart Radius`)**:
   - Tạo dải chuyển tiếp (transition band) quanh biên vùng chọn hiện tại `SelectionMask`.
   - `Smart Radius` tự động mở rộng dải ở vùng chi tiết mảnh (tóc, lông, lá) và giữ hẹp ở cạnh sắc nét.
   - Cho phép dùng cọ **Refine Edge Brush** tô trực tiếp vào vùng biên cần tách chi tiết.
2. **Alpha Matting / Guided Filter**:
   - Dùng ảnh gốc `EditorDocument::current` làm ảnh dẫn (guide) để ước lượng lại giá trị alpha `0..255` trong dải chuyển tiếp dựa trên độ tương đồng màu với Foreground/Background lân cận.
3. **Global Refinements**:
   - `Smooth (0..100)`: làm trơn đường biên mask.
   - `Feather (px)`: làm mềm chuyển tiếp alpha.
   - `Contrast (0..100%)`: tăng độ gắt dải chuyển tiếp.
   - `Shift Edge (-100%..+100%)`: co/nở biên theo pháp tuyến để loại bỏ viền nền.
4. **Decontaminate Colors**:
   - Khử ám màu nền cũ ở các pixel bán trong suốt quanh biên bằng màu Foreground thuần lân cận.
5. **View Modes & Output To**:
   - Preview: `Marching Ants`, `Overlay (Red)`, `On Black`, `On White`, `Black & White (Mask)`.
   - Output: `Selection`, `Layer Mask`, `New Layer`, `New Layer with Layer Mask`.

---

## 2. Bộ Filter Photoshop (`Filter` Menu)

### Kiến trúc dùng chung
- **`photon-image`**: thuật toán RGBA thuần Rust (tối ưu separable kernel), chạy qua [`apply_with_selection`](../../crates/photon-image/src/document.rs#L371) để tự động hỗ trợ **Selection Only / Whole Layer** và **Undo/Redo**.
- **`photon-protocol` & `photon-core`**: `Command::EditorApplyFilter { filter: FilterSpec, target, selection_only }`.
- **`photon-desktop`**: submenu phân nhóm trong `Filter`, dialog preview trực tiếp (hỗ trợ `Enter` -> Apply và auto-focus ô nhập đầu tiên), chuỗi dịch `en-US` / `vi-VN`.

### Lộ trình triển khai

#### Giai đoạn 1 — Nhóm cốt lõi (Blur, Sharpen, Noise, High Pass)
1. **Filter → Blur**:
   - `Gaussian Blur`: `Radius (px)` (separable 1D Gaussian).
   - `Box Blur`: `Radius (px)` (prefix-sum `O(1)`).
   - `Motion Blur`: `Angle (°)`, `Distance (px)`.
   - `Radial Blur`: `Spin` / `Zoom`, `Amount`.
2. **Filter → Sharpen**:
   - `Unsharp Mask`: `Amount (%)`, `Radius (px)`, `Threshold`.
   - `Smart Sharpen`: tăng nét ưu tiên vùng cạnh, hạn chế khuếch đại nhiễu vùng phẳng.
3. **Filter → Noise**:
   - `Add Noise`: `Amount (%)`, `Uniform` / `Gaussian`, `Monochromatic`.
   - `Median` (Despeckle / Dust & Scratches): lọc trung vị theo `Radius (px)`.
4. **Filter → Other**:
   - `High Pass`: `Radius (px)` (`128 + (orig - blurred)`), dùng phối hợp Blend Mode `Overlay` / `Soft Light`.

#### Giai đoạn 2 — Nhóm Pixelate, Stylize, Distort & Other
5. **Filter → Pixelate**:
   - `Mosaic`: `Cell Size (px)`.
   - `Halftone` / `Crystallize`.
6. **Filter → Stylize**:
   - `Emboss`: `Angle (°)`, `Height (px)`, `Amount (%)`.
   - `Find Edges`: Sobel edge magnitude (tùy chọn đảo ngược nét đen nền trắng).
7. **Filter → Distort / Lens**:
   - `Vignette`: `Amount`, `Midpoint`, `Feather`.
   - `Lens Distortion`: `Amount` (barrel / pincushion).
8. **Filter → Other**:
   - `Offset`: dịch chuyển tuần hoàn `X / Y (px)` (wrap-around cho texture seamless).

#### Giai đoạn 3 — Refine Edge
- Tích hợp `Select → Refine Edge...` với Guided Filter matting, Refine Edge Brush, Decontaminate Colors, View Modes và Output modes.
