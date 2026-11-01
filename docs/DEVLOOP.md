# DEVLOOP — Chạy & debug app đang sống

Quy tắc: **log trước, đoán sau**. Mọi kết luận "đã sửa" phải kèm bằng chứng từ
`photon-ctl` trên app thật.

## 1. Khởi động một instance riêng

Đừng dùng socket mặc định khi đang thử nghiệm — instance cũ hoặc socket chết sẽ
làm bạn debug sai đối tượng.

```bash
QT_QPA_PLATFORM=offscreen setsid nohup uv run photon \
  --socket /tmp/photon-dev.sock > /tmp/photon-dev.log 2>&1 < /dev/null &
export PHOTON_SOCKET=/tmp/photon-dev.sock
uv run photon-ctl app.info
```

- `--headless`: không có window/canvas. Các lệnh `shot`, `ui.click`, `window.state`
  sẽ trả `no window` — chỉ dùng để test core/socket.
- `QT_QPA_PLATFORM=offscreen`: chạy được GUI thật (canvas, QPixmap, `shot`) mà
  không cần X/Wayland. Đây là cách chạy scenario trong CI.
- `setsid ... < /dev/null` để app không chết theo shell/terminal đóng.

## 2. Dừng instance

```bash
pgrep -f "[p]hoton --socket /tmp/photon-dev.sock" | xargs -r kill
```

`pkill -f photon` sẽ khớp cả command line của shell đang chạy lệnh (và cả
`photon-ctl`), nên nó tự giết chính bạn. Luôn dùng pattern đặc thù.

## 3. Vòng lặp debug

```bash
uv run photon-ctl app.doctor                  # socket/log/cache + lỗi gần đây
uv run photon-ctl log.tail '{"n":40}'
uv run photon-ctl log.errors '{"n":10}'       # mã PHOTON-<AREA>-NNN
uv run photon-ctl ui.events '{"n":30}'
uv run photon-ctl window.state
uv run photon-ctl shot '{"name":"dbg","widget":"canvas"}'
```

`shot` trả `blank: false` khi canvas thật sự có pixel khác nhau — đây là cách
duy nhất chứng minh "render đúng" thay vì chỉ tin state.

## 4. Kiểm chứng render bằng pixel

State có thể đúng trong khi hình vẽ sai (đây chính là bug flip trước đây).
So sánh pixel trước/sau:

```bash
uv run photon-ctl shot '{"name":"before","widget":"canvas"}'
uv run photon-ctl view.flip '{"axis":"h"}'
uv run photon-ctl shot '{"name":"after","widget":"canvas"}'

uv run python -c "
from PIL import Image
a=Image.open('$HOME/.cache/photon/shots/before.png').convert('RGB')
b=Image.open('$HOME/.cache/photon/shots/after.png').convert('RGB')
w,h=a.size
print(a.getpixel((int(w*.35),h//2)), b.getpixel((int(w*.35),h//2)))
"
```

## 5. Decode chạy nền

`ui/loader.py` decode ảnh trên `QThreadPool`; GUI chỉ nhận `QImage` rồi tạo
`QPixmap`. Nếu ảnh không hiện:

1. `log.errors` — tìm `file.open_failed`.
2. Kiểm `window.state.canvas.fit` / `zoom`.
3. Nếu đang test bằng script, gọi `window.wait_for_image_load()` trước khi assert.
   `file.open` (automation) đã tự gọi hàm này.

Chỉ request mới nhất được giữ: mở liên tiếp nhiều ảnh sẽ không vẽ đè ảnh cũ.
