# TESTING — Unit, UI và scenario

Ba tầng, không tầng nào thay thế tầng khác.

| Tầng | Chạy gì | Bắt được gì |
|---|---|---|
| Unit (`pytest`, tmp_path) | core/imaging/automation, không Qt | logic model, encode/decode, trash, undo |
| UI (`pytest-qt`, `qtbot`) | widget thật, offscreen | render sai (flip/rotate), shortcut, async load |
| Scenario (YAML) | app thật qua socket | regression end-to-end, IPC, state qua nhiều bước |

## Chạy tất cả

```bash
uv run ruff check src tests && uv run ruff format --check src tests \
  && uv run mypy src && uv run lint-imports && uv run pytest -q
```

## Scenario end-to-end

```bash
QT_QPA_PLATFORM=offscreen setsid nohup uv run photon \
  --socket /tmp/sc.sock > /tmp/sc.log 2>&1 < /dev/null &
PHOTON_SOCKET=/tmp/sc.sock uv run photon-ctl --script scenarios/02_edit_workflow.yaml --out /tmp/sc-out
```

- Scenario hiện có: `01_basic_viewer` (viewer + toolbar + sidebar + fullscreen),
  `02_edit_workflow` (rotate/flip/undo/redo/reset/resize/save-copy),
  `03_compare_frames` (2 khung độc lập + active frame; **tự skip** khi
  `core/features.py` đặt `COMPARE_ENABLED = False`).
- Step phải lỗi thì đánh `expect_fail: true`.
- Assert hỗ trợ: `equals`, `not_equals`, `greater_than`, `less_than`,
  `contains`, `not_contains` (viết trực tiếp hoặc qua `op`).

## Vì sao cần test UI

Bug flip là ví dụ chuẩn: `view.flip` trả `flip_h: true`, model đúng, unit test
xanh — nhưng canvas **không hề vẽ flip**. Chỉ test đọc pixel mới bắt được:

```python
canvas.grab().toImage().pixelColor(x, y)
```

Nguyên tắc: với mọi thay đổi "hiển thị", assert **pixel** hoặc **pixel-derived**
state, không assert state nội bộ.

## Test UI chạy offscreen

`pyproject.toml` đặt `qt_api = "pyside6"`. Trong CI:

```bash
QT_QPA_PLATFORM=offscreen uv run pytest -q
```

Lưu ý: khi đã có `QApplication`, code gọi thẳng `QFileDialog.getOpenFileName`
sẽ mở modal thật và **treo test**. Test portal vì vậy phải stub
`_open_via_qt` / `_save_via_qt`.

## Async decode

`file.open` decode nền. Trong test UI:

```python
window.open_path(path)
assert window.wait_for_image_load(5000) is True
```

Nếu quên bước này, `canvas.fit_mode`/`zoom` có thể chưa được áp dụng.
