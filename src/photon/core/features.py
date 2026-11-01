"""Temporary feature switches.

Đổi cờ ở đây để ẩn/hiện một tính năng mà không phải xoá code. Đọc giá trị qua
hàm ``*_enabled()`` (không import hằng số trực tiếp) để test có thể monkeypatch.
"""

from __future__ import annotations

#: Khung so sánh song song + cơ chế active frame (viền focus).
#: Đặt ``True`` để bật lại; UI (nút Compare, khung phải), automation
#: (``view.compare``, ``compare.*``, ``frame.*``) và scenario 03 sẽ theo cờ này.
COMPARE_ENABLED: bool = False


def compare_enabled() -> bool:
    """Whether the compare pane / active-frame (focus) feature is available."""
    return COMPARE_ENABLED
