#!/usr/bin/env python3
"""System / user-local installer and uninstaller for Photon desktop integration.

Installs:
- `photon` and `photonctl` binaries into `<prefix>/bin`
- Browser desktop entry (`io.github.tienv2i.Photon.desktop` -> `photon %U`)
- Standalone Viewer desktop entry (`io.github.tienv2i.Photon.Viewer.desktop` -> `photon --viewer-window %F`)
- Editor desktop entry (`io.github.tienv2i.Photon.Editor.desktop` -> `photon edit %f`)
- Image MIME type associations (Viewer + Editor; `--set-default` makes the Viewer the default handler)
- AppStream metadata & hicolor SVG + PNG icons
"""

from __future__ import annotations

import argparse
import os
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
PACKAGING = ROOT / "packaging" / "linux"
APP_ID = "io.github.tienv2i.Photon"
ICON_SIZES = (16, 24, 32, 48, 64, 128, 256)

# Exact MIME types matching image codecs decoded by `photon-image`.
IMAGE_MIME_TYPES = ";".join(
    [
        "image/jpeg",
        "image/png",
        "image/webp",
        "image/gif",
        "image/tiff",
        "image/bmp",
        "image/avif",
        "image/heic",
        "image/heif",
        "image/jp2",
        "image/jpx",
        "image/x-exr",
        "image/vnd.radiance",
        "image/x-hdr",
        "image/vnd.microsoft.icon",
        "image/x-icns",
        "image/x-portable-anymap",
        "image/x-portable-bitmap",
        "image/x-portable-graymap",
        "image/x-portable-pixmap",
        "image/qoi",
        "image/x-tga",
        "image/x-farbfeld",
        "image/vnd.ms-dds",
        "application/vnd.comicbook+zip",
        "application/x-cbz",
    ]
) + ";"


def run(cmd: list[str], *, check: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=ROOT, text=True, check=check)


def is_writable_prefix(prefix: pathlib.Path) -> bool:
    cur = prefix
    while not cur.exists() and cur != cur.parent:
        cur = cur.parent
    return os.access(cur, os.W_OK)


def maybe_privileged(cmd: list[str], needs_root: bool) -> list[str]:
    if not needs_root or os.geteuid() == 0:
        return cmd
    res = subprocess.run(["sudo", "-n", "true"], capture_output=True, check=False)
    if res.returncode == 0:
        return ["sudo", "-n", "--", *cmd]
    return ["sudo", "--", *cmd]


def install_targets(prefix: pathlib.Path) -> dict[str, pathlib.Path]:
    share = prefix / "share"
    targets = {
        "photon": prefix / "bin" / "photon",
        "photonctl": prefix / "bin" / "photonctl",
        "browser desktop": share / "applications" / f"{APP_ID}.desktop",
        "standalone viewer desktop": share / "applications" / f"{APP_ID}.Viewer.desktop",
        "editor desktop": share / "applications" / f"{APP_ID}.Editor.desktop",
        "metainfo": share / "metainfo" / f"{APP_ID}.metainfo.xml",
        "icon svg": share / "icons" / "hicolor" / "scalable" / "apps" / f"{APP_ID}.svg",
    }
    for edge in ICON_SIZES:
        targets[f"icon {edge}"] = (
            share / "icons" / "hicolor" / f"{edge}x{edge}" / "apps" / f"{APP_ID}.png"
        )
    return targets


def render_desktop(template_path: pathlib.Path, bindir: pathlib.Path) -> str:
    text = template_path.read_text(encoding="utf-8")
    return (
        text.replace("@BINDIR@", str(bindir))
        .replace("@MIMETYPES@", IMAGE_MIME_TYPES)
    )


def rasterise_svg(svg_path: pathlib.Path, size: int, dest: pathlib.Path) -> bool:
    rsvg = shutil.which("rsvg-convert")
    if not rsvg:
        return False
    dest.parent.mkdir(parents=True, exist_ok=True)
    res = subprocess.run(
        [rsvg, "-w", str(size), "-h", str(size), str(svg_path), "-o", str(dest)],
        check=False,
    )
    return res.returncode == 0 and dest.is_file()


def refresh_caches(prefix: pathlib.Path, needs_root: bool) -> None:
    apps_dir = prefix / "share" / "applications"
    if shutil.which("update-desktop-database") and apps_dir.is_dir():
        run(maybe_privileged(["update-desktop-database", str(apps_dir)], needs_root))

    theme_dir = prefix / "share" / "icons" / "hicolor"
    if theme_dir.is_dir():
        sys_index = pathlib.Path("/usr/share/icons/hicolor/index.theme")
        local_index = theme_dir / "index.theme"
        if sys_index.is_file() and not local_index.is_file():
            stage_index = ROOT / "target" / "install-stage" / "index.theme"
            stage_index.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(sys_index, stage_index)
            run(maybe_privileged(
                ["install", "-D", "-m644", str(stage_index), str(local_index)],
                needs_root,
            ))
        if shutil.which("gtk-update-icon-cache") and local_index.is_file():
            run(maybe_privileged(["gtk-update-icon-cache", "-f", "-t", str(theme_dir)], needs_root))


def cmd_install(prefix: pathlib.Path, *, dev: bool, skip_build: bool, set_default_viewer: bool) -> int:
    needs_root = not is_writable_prefix(prefix)
    stage = ROOT / "target" / "install-stage"
    stage.mkdir(parents=True, exist_ok=True)

    if not skip_build:
        cargo_cmd = ["cargo", "build", "-p", "photon-desktop", "-p", "photon-cli"]
        if not dev:
            cargo_cmd.append("--release")
        print("+", " ".join(cargo_cmd))
        if run(cargo_cmd).returncode != 0:
            return 1

    profile_dir = ROOT / "target" / ("debug" if dev else "release")
    photon_bin = profile_dir / "photon"
    ctl_bin = profile_dir / "photonctl"
    if not photon_bin.is_file() or not ctl_bin.is_file():
        print(f"error: built binaries missing in {profile_dir}", file=sys.stderr)
        return 1

    bindir = prefix / "bin"
    browser_desktop = stage / f"{APP_ID}.desktop"
    standalone_viewer_desktop = stage / f"{APP_ID}.Viewer.desktop"
    editor_desktop = stage / f"{APP_ID}.Editor.desktop"
    browser_desktop.write_text(
        render_desktop(PACKAGING / f"{APP_ID}.desktop.in", bindir),
        encoding="utf-8",
    )
    standalone_viewer_desktop.write_text(
        render_desktop(PACKAGING / f"{APP_ID}.Viewer.desktop.in", bindir),
        encoding="utf-8",
    )
    editor_desktop.write_text(
        render_desktop(PACKAGING / f"{APP_ID}.Editor.desktop.in", bindir),
        encoding="utf-8",
    )

    validator = shutil.which("desktop-file-validate")
    if validator:
        for df in (browser_desktop, standalone_viewer_desktop, editor_desktop):
            res = subprocess.run([validator, str(df)], capture_output=True, text=True, check=False)
            if res.returncode != 0:
                print(res.stdout or res.stderr, file=sys.stderr)
                return res.returncode

    metainfo = PACKAGING / f"{APP_ID}.metainfo.xml"
    appstream = shutil.which("appstreamcli")
    if appstream:
        res = subprocess.run(
            [appstream, "validate", "--no-net", str(metainfo)],
            capture_output=True,
            text=True,
            check=False,
        )
        if res.returncode != 0:
            print(res.stdout or res.stderr, file=sys.stderr)
            return res.returncode

    svg_src = PACKAGING / f"{APP_ID}.svg"
    sources: dict[str, tuple[pathlib.Path, str]] = {
        "photonctl": (ctl_bin, "755"),
        "browser desktop": (browser_desktop, "644"),
        "standalone viewer desktop": (standalone_viewer_desktop, "644"),
        "editor desktop": (editor_desktop, "644"),
        "metainfo": (metainfo, "644"),
        "icon svg": (svg_src, "644"),
    }

    targets = install_targets(prefix)
    if dev:
        libexec_bin = prefix / "libexec" / "photon"
        wrapper = stage / "photon-wrapper.sh"
        wrapper.write_text(
            "#!/bin/sh\n"
            'export PHOTON_PROFILE="${PHOTON_PROFILE:-prod}"\n'
            f'exec "{libexec_bin}" "$@"\n',
            encoding="utf-8",
        )
        targets["photon libexec"] = libexec_bin
        sources["photon libexec"] = (photon_bin, "755")
        sources["photon"] = (wrapper, "755")
    else:
        sources["photon"] = (photon_bin, "755")

    for edge in ICON_SIZES:
        staged_png = stage / "icons" / f"{edge}x{edge}" / f"{APP_ID}.png"
        if rasterise_svg(svg_src, edge, staged_png):
            sources[f"icon {edge}"] = (staged_png, "644")

    for key, (src, mode) in sources.items():
        dst = targets[key]
        res = run(maybe_privileged(["install", "-D", f"-m{mode}", str(src), str(dst)], needs_root))
        if res.returncode != 0:
            return res.returncode

    refresh_caches(prefix, needs_root)

    if set_default_viewer and shutil.which("xdg-mime"):
        for mime in [m for m in IMAGE_MIME_TYPES.split(";") if m]:
            subprocess.run(
                ["xdg-mime", "default", f"{APP_ID}.Viewer.desktop", mime],
                check=False,
            )

    print(f"Installed Photon ({APP_ID}) into {prefix}:")
    for key, (src, _) in sources.items():
        print(f"  {key:16} -> {targets[key]}")
    print("Registered MIME handlers:")
    print(f"  Browser: {targets['browser desktop']} (Exec: photon %U)")
    print(
        f"  Viewer:  {targets['standalone viewer desktop']} "
        "(Exec: photon --viewer-window %F)"
    )
    print(f"  Editor:  {targets['editor desktop']} (Exec: photon edit %f)")
    if not set_default_viewer:
        print(
            "Default image handler unchanged. Set Photon Viewer as default e.g. with:\n"
            f"  xdg-mime default {APP_ID}.Viewer.desktop "
            "image/jpeg image/png image/webp"
        )
    return 0


def cmd_uninstall(prefix: pathlib.Path) -> int:
    needs_root = not is_writable_prefix(prefix)
    targets = install_targets(prefix)
    libexec_bin = prefix / "libexec" / "photon"
    if libexec_bin.exists():
        targets["photon libexec"] = libexec_bin

    removed = 0
    for key, dst in targets.items():
        if dst.exists() or dst.is_symlink():
            res = run(maybe_privileged(["rm", "-f", str(dst)], needs_root))
            if res.returncode != 0:
                return res.returncode
            print(f"  removed {key:16} {dst}")
            removed += 1

    if removed == 0:
        print(f"uninstall: nothing of {APP_ID} found under {prefix}")
        return 0

    refresh_caches(prefix, needs_root)
    print(f"Uninstalled {APP_ID} from {prefix} (user config/state preserved).")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Install or uninstall Photon desktop integration.")
    sub = parser.add_subparsers(dest="action", required=True)

    for action_name in ("install", "uninstall"):
        sp = sub.add_parser(action_name)
        sp.add_argument(
            "--prefix",
            type=pathlib.Path,
            default=None,
            help="Install prefix (default: /usr/local, or ~/.local with --user)",
        )
        sp.add_argument(
            "--user",
            action="store_true",
            help="Install into ~/.local (no root required)",
        )
        if action_name == "install":
            sp.add_argument("--dev", action="store_true", help="Reuse debug binary with PHOTON_PROFILE=prod wrapper")
            sp.add_argument("--skip-build", action="store_true", help="Skip cargo build step")
            sp.add_argument(
                "--set-default",
                action="store_true",
                help="Set Photon Viewer as the default xdg-mime handler for supported image types",
            )

    args = parser.parse_args()
    if args.prefix is not None:
        prefix = args.prefix.expanduser().resolve()
    elif args.user:
        prefix = (pathlib.Path.home() / ".local").resolve()
    else:
        prefix = pathlib.Path(os.environ.get("PREFIX", "/usr/local")).resolve()

    if args.action == "install":
        return cmd_install(
            prefix,
            dev=args.dev,
            skip_build=args.skip_build,
            set_default_viewer=args.set_default,
        )
    return cmd_uninstall(prefix)


if __name__ == "__main__":
    raise SystemExit(main())
