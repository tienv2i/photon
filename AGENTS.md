# AGENTS.md — Photon

Hướng dẫn cho agent làm việc trong repo này. Mục tiêu: **debug và sửa app đang chạy
mà không cần chuột**, không cần đoán.

## Quy tắc Debug lỗi Runtime (BẮT BUỘC)

- **Log First**: Khi người dùng báo lỗi ứng dụng đang chạy hoặc lỗi UI, BẮT BUỘC phải
  kiểm tra file log runtime (`~/.cache/<app>/<app>.log` hoặc lệnh
  `<app>-ctl log.errors` / `<app>-ctl log.tail`) TRƯỚC KHI suy đoán hoặc sửa code.
- **Không ảo tưởng linter**: Pytest và Ruff/Linter pass KHÔNG đồng nghĩa với việc ứng
  dụng chạy đúng. Tuyệt đối không được kết luận "Đã sửa xong" chỉ vì `pytest` pass.
- **Tái hiện thực tế**: Phải dùng CLI/Automation kích hoạt hành động gây lỗi để kiểm tra
  response thực tế trước và sau khi sửa.

## Quy tắc Thư viện & Hệ thống

- Không được bịa (hallucinate) method hoặc thuộc tính thư viện (như Jeepney, D-Bus, Qt).
  Nếu không chắc chắn về signature, phải chạy script Python nhỏ để introspect object
  thực tế.
- Tránh vòng lặp vô tận: Nếu một lệnh shell/introspect không đem lại thông tin mới,
  KHÔNG được lặp lại quá 2 lần. Phải chuyển hướng sang phân tích log và mã nguồn.

## Commands

```bash
uv sync                              # cài deps vào .venv (uv quản lý)
uv run photon [file]                 # chạy app GUI (automation bật mặc định)
uv run photon --headless             # chạy không cần display (CI/test)
uv run photon-ctl <cmd> '<json>'     # điều khiển app đang chạy
uv run photon-ctl --script scenarios/01_basic_viewer.yaml   # chạy scenario
uv run photon-ctl --script scenarios/02_edit_workflow.yaml   # edit/undo/save-copy
uv run photon-ctl --script scenarios/03_compare_frames.yaml   # 2 khung + active frame
```

Chạy scenario end-to-end trên app thật (không cần display):

```bash
QT_QPA_PLATFORM=offscreen setsid nohup uv run photon \
  --socket /tmp/photon-dev.sock > /tmp/photon-dev.log 2>&1 < /dev/null &
PHOTON_SOCKET=/tmp/photon-dev.sock uv run photon-ctl --script scenarios/01_basic_viewer.yaml
```

Lưu ý: `pkill -f photon` giết luôn shell đang chạy lệnh nếu pattern khớp với
command line của chính nó — dùng pattern đủ đặc thù (`photon --socket /tmp/x.sock`).

`just` **không** có trên máy này. Chạy check tay:

```bash
uv run ruff check src tests && uv run ruff format --check src tests \
  && uv run mypy src && uv run lint-imports && uv run pytest -q
```

## Automation — bề mặt chính để debug

Socket: `~/.cache/photon/photon.sock` (dir 0700, socket 0600).
Instance khác: `--socket /tmp/a.sock` hoặc env `PHOTON_SOCKET` (cả server lẫn client đọc).

| Nhóm | Lệnh |
|---|---|
| App | `app.info`, `app.doctor`, `app.selftest`, `app.quit` |
| File | `file.open`, `file.next`, `file.prev`, `file.info`, `file.save_copy`, `file.delete` |
| View | `view.zoom`, `view.pan`, `view.fit`, `view.actual`, `view.rotate`, `view.flip`, `view.reset`, `view.fullscreen`, `view.compare` |
| Frame | `frame.activate`, `frame.state` |
| Edit | `edit.set`, `edit.undo`, `edit.redo`, `edit.reset` |
| Sidebar | `sidebar.show`, `sidebar.state`, `sidebar.set_content` |
| Toolbar | `toolbar.click`, `toolbar.state` |
| Dialog | `dialog.open`, `dialog.result` |
| UI | `shot`, `wait`, `ui.events`, `ui.click`, `ui.key`, `window.state`, `window.resize` |
| Log | `log.tail`, `log.errors` |

Bảng đầy đủ params/return: `docs/AUTOMATION.md`.

Ví dụ luồng điển hình:

```bash
C=.venv/bin/photon-ctl
$C file.open '{"path":"/tmp/fixtures/a.jpg"}'
$C wait '{"condition":"viewer.loading == false","timeout":10}'
$C view.rotate '{"degrees":90}'
$C shot '{"name":"rotated"}'
$C sidebar.show '{"side":"right","visible":true}'
$C log.tail '{"n":20}'
```

`wait` nhận số, chuỗi, `true/false`, hoặc so sánh getter (`view.zoom == 1.0`).

## Scenario (regression end-to-end)

```bash
.venv/bin/photon-ctl --script scenarios/01_basic_viewer.yaml --out /tmp/sc-viewer
```

Có `setup:` (mkdir/copy/rm/touch) chạy local trước steps; `result.json` ghi từng bước
và từng assert. Bước phải lỗi đánh dấu `expect_fail: true`.

## Debug nhanh

1. `app.info` — app có sống, platform gì, version nào.
2. `app.doctor` — cache/socket/log/lỗi gần đây.
3. `ui.events '{"n":30}'` — 30 thao tác UI cuối (kind, widget, pos).
4. `shot '{"name":"x"}'` — ảnh chụp; kiểm `blank`, `dpr`, `visible`.
5. `log.tail '{"n":40}'` — log app; mã lỗi dạng `PHOTON-DEC-001`.
6. Sửa code → restart app → chạy lại scenario.

## Rules

- `core/` + `imaging/` **không** import Qt/UI/automation (import-linter chặn).
- Feature không import widget của feature khác; dùng `photon.core.events.EventBus`.
- Tính năng đang ẩn dùng cờ trong `core/features.py` (ví dụ `COMPARE_ENABLED`):
  ẩn UI + lệnh automation trả `PHOTON-CTL-204` + scenario khai báo
  `requires_feature: <tên>` để tự skip. Không xoá code khi chỉ muốn ẩn tạm.
- Không chặn UI thread >50 ms: decode/ghi file phải qua thread pool.
- Mọi tính năng mới phải có lệnh automation gọi **cùng code path** với UI
  (thêm vào `automation/commands.py` + `docs/AUTOMATION.md`, thêm assert vào scenario).
- Không ghi đè ảnh gốc: save → `<stem> (edited).<ext>`.
- Widget chính phải có `objectName` (dùng cho `shot`/`ui.hit_test`/automation).
- Chạy đủ check ở trên trước khi commit.
