"""Freedesktop trash support (ZERO Qt imports).

Implements the subset of the
`Trash specification <https://specifications.freedesktop.org/trash-spec/>`_
that a viewer needs: move a file into ``$XDG_DATA_HOME/Trash/files`` and write
the matching ``info/<name>.trashinfo`` record so the desktop can restore it.
"""

from __future__ import annotations

import os
import shutil
from datetime import datetime
from pathlib import Path
from urllib.parse import quote

from photon.core.errors import FileNotFoundError_


def trash_dir() -> Path:
    """Return the user's trash directory.

    Honors ``XDG_DATA_HOME`` when set (also used by tests to stay isolated).
    """
    data_home = os.environ.get("XDG_DATA_HOME") or str(Path.home() / ".local" / "share")
    return Path(data_home) / "Trash"


def move_to_trash(path: Path) -> Path:
    """Move ``path`` to the trash and write its ``.trashinfo`` file.

    Args:
        path: File to trash

    Returns:
        New location inside ``Trash/files``

    Raises:
        FileNotFoundError_: If the file does not exist
        OSError: If the move fails
    """
    path = Path(path).resolve()
    if not path.exists():
        raise FileNotFoundError_(str(path))

    trash = trash_dir()
    files_dir = trash / "files"
    info_dir = trash / "info"
    for directory in (trash, files_dir, info_dir):
        directory.mkdir(parents=True, mode=0o700, exist_ok=True)

    # Never clobber an already-trashed file that has the same name.
    dest = files_dir / path.name
    counter = 1
    while dest.exists() or (info_dir / f"{dest.name}.trashinfo").exists():
        dest = files_dir / f"{path.stem}.{counter}{path.suffix}"
        counter += 1

    shutil.move(str(path), str(dest))

    info = info_dir / f"{dest.name}.trashinfo"
    info.write_text(
        "[Trash Info]\n"
        f"Path={quote(str(path))}\n"
        f"DeletionDate={datetime.now().strftime('%Y-%m-%dT%H:%M:%S')}\n",
        encoding="utf-8",
    )

    return dest
