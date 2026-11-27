#!/usr/bin/env python3
"""System / user-local installer and uninstaller for Photon desktop integration.

Installs:
- `photon`, `photon-manager`, `photon-viewer`, `photon-editor`, and `photonctl` binaries into `<prefix>/bin`
- Browser desktop entry (`io.github.tienv2i.Photon.desktop` -> `photon-manager %U`)
- Standalone Viewer desktop entry (`io.github.tienv2i.Photon.Viewer.desktop` -> `photon-viewer %F`)
- Editor desktop entry (`io.github.tienv2i.Photon.Editor.desktop` -> `photon-editor %F`)
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
# Distinct taskbar/app-menu icons per process role (desktop entry `Icon=` names).
ICON_ROLES = {
    APP_ID: "browser",
    f"{APP_ID}.Viewer": "viewer",
    f"{APP_ID}.Editor": "editor",
}
ICON_SIZES = (16, 24, 32, 48, 64, 128, 256)

# Exact MIME types matching image codecs supported by Photon Editor & Viewer.
EDITOR_MIME_TYPES = ";".join(
    [
        "image/jpeg",
        "image/png",
        "image/apng",
        "image/webp",
        "image/gif",
        "image/tiff",
        "image/bmp",
        "image/avif",
        "image/heic",
        "image/heif",
        "image/jxl",
        "image/jp2",
        "image/x-jp2",
        "image/jpx",
        "image/jpm",
        "image/svg+xml",
        "image/svg+xml-compressed",
        "image/vnd.adobe.photoshop",
        "image/openraster",
        "application/x-krita",
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
        "image/x-adobe-dng",
        "image/x-canon-cr2",
        "image/x-canon-cr3",
        "image/x-nikon-nef",
        "image/x-sony-arw",
        "image/x-fuji-raf",
        "image/x-olympus-orf",
        "image/x-panasonic-rw2",
        "image/x-dcraw",
    ]
) + ";"

# Viewer additionally supports PDF and comic book / ZIP archives.
IMAGE_MIME_TYPES = (
    EDITOR_MIME_TYPES
    + ";".join(
        [
            "application/pdf",
            "application/zip",
            "application/vnd.comicbook+zip",
            "application/x-cbz",
        ]
    )
    + ";"
)


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


def invoking_user() -> tuple[str, pathlib.Path] | None:
    """Return `(name, home)` of the sudo caller, if the script runs as root."""
    name = os.environ.get("SUDO_USER")
    if os.geteuid() != 0 or not name:
        return None
    import pwd

    try:
        return name, pathlib.Path(pwd.getpwnam(name).pw_dir)
    except KeyError:
        return None


def warn_if_user_install_shadows(prefix: pathlib.Path) -> None:
    """A stale `~/.local` install shadows `/usr/local` for the same desktop IDs."""
    caller = invoking_user()
    home = caller[1] if caller else pathlib.Path.home()
    user_prefix = (home / ".local").resolve()
    if prefix.resolve() == user_prefix:
        return
    user_apps = user_prefix / "share" / "applications"
    shadowing = sorted(user_apps.glob(f"{APP_ID}*.desktop"))
    if not shadowing:
        return
    print(
        "WARNING: a user-local Photon install shadows this one:\n"
        + "".join(f"  {path}\n" for path in shadowing)
        + "  Remove it with: python3 scripts/install.py uninstall --user\n"
        + "  (or reinstall into ~/.local with `just install-user`).",
        file=sys.stderr,
    )


def set_default_viewer_mime_types() -> None:
    """Point the desktop's MIME defaults at the Viewer entry.

    When the installer is run through sudo, `xdg-mime` must run as the real
    user: otherwise it rewrites root's `mimeapps.list` and the desktop user
    keeps the old default handler.
    """
    xdg_mime = shutil.which("xdg-mime")
    if not xdg_mime:
        return
    caller = invoking_user()
    mimes = [m for m in IMAGE_MIME_TYPES.split(";") if m]
    for mime in mimes:
        cmd = [xdg_mime, "default", f"{APP_ID}.Viewer.desktop", mime]
        if caller:
            name, home = caller
            cmd = [
                "sudo",
                "-u",
                name,
                "env",
                f"HOME={home}",
                f"XDG_CONFIG_HOME={home / '.config'}",
                *cmd,
            ]
        subprocess.run(cmd, check=False)


def install_targets(prefix: pathlib.Path) -> dict[str, pathlib.Path]:
    share = prefix / "share"
    targets = {
        "photon": prefix / "bin" / "photon",
        "photon-manager": prefix / "bin" / "photon-manager",
        "photon-viewer": prefix / "bin" / "photon-viewer",
        "photon-editor": prefix / "bin" / "photon-editor",
        "photonctl": prefix / "bin" / "photonctl",
        "browser desktop": share / "applications" / f"{APP_ID}.desktop",
        "standalone viewer desktop": share / "applications" / f"{APP_ID}.Viewer.desktop",
        "editor desktop": share / "applications" / f"{APP_ID}.Editor.desktop",
        "metainfo": share / "metainfo" / f"{APP_ID}.metainfo.xml",
    }
    icons_dir = share / "icons" / "hicolor"
    for icon_name, role in ICON_ROLES.items():
        targets[f"icon {role} svg"] = (
            icons_dir / "scalable" / "apps" / f"{icon_name}.svg"
        )
        for edge in ICON_SIZES:
            targets[f"icon {role} {edge}"] = (
                icons_dir / f"{edge}x{edge}" / "apps" / f"{icon_name}.png"
            )
    return targets


def render_desktop(
    template_path: pathlib.Path,
    bindir: pathlib.Path,
    mimetypes: str = IMAGE_MIME_TYPES,
) -> str:
    text = template_path.read_text(encoding="utf-8")
    return (
        text.replace("@BINDIR@", str(bindir))
        .replace("@MIMETYPES@", mimetypes)
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
        cargo_cmd = [
            "cargo",
            "build",
            "-p",
            "photon-desktop",
            "-p",
            "photon-app-manager",
            "-p",
            "photon-app-viewer",
            "-p",
            "photon-app-editor",
            "-p",
            "photon-cli",
        ]
        if not dev:
            cargo_cmd.append("--release")
        print("+", " ".join(cargo_cmd))
        if run(cargo_cmd).returncode != 0:
            return 1

    profile_dir = ROOT / "target" / ("debug" if dev else "release")
    desktop_bins = ("photon", "photon-manager", "photon-viewer", "photon-editor")
    ctl_bin = profile_dir / "photonctl"
    if any(not (profile_dir / name).is_file() for name in desktop_bins) or not ctl_bin.is_file():
        print(f"error: built binaries missing in {profile_dir}", file=sys.stderr)
        return 1

    bindir = prefix / "bin"
    browser_desktop = stage / f"{APP_ID}.desktop"
    standalone_viewer_desktop = stage / f"{APP_ID}.Viewer.desktop"
    editor_desktop = stage / f"{APP_ID}.Editor.desktop"
    browser_desktop.write_text(
        render_desktop(PACKAGING / f"{APP_ID}.desktop.in", bindir, ""),
        encoding="utf-8",
    )
    standalone_viewer_desktop.write_text(
        render_desktop(PACKAGING / f"{APP_ID}.Viewer.desktop.in", bindir, IMAGE_MIME_TYPES),
        encoding="utf-8",
    )
    editor_desktop.write_text(
        render_desktop(PACKAGING / f"{APP_ID}.Editor.desktop.in", bindir, EDITOR_MIME_TYPES),
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

    sources: dict[str, tuple[pathlib.Path, str]] = {
        "photonctl": (ctl_bin, "755"),
        "browser desktop": (browser_desktop, "644"),
        "standalone viewer desktop": (standalone_viewer_desktop, "644"),
        "editor desktop": (editor_desktop, "644"),
        "metainfo": (metainfo, "644"),
    }
    for icon_name, role in ICON_ROLES.items():
        sources[f"icon {role} svg"] = (PACKAGING / f"{icon_name}.svg", "644")

    targets = install_targets(prefix)
    for bin_name in desktop_bins:
        built_bin = profile_dir / bin_name
        if dev:
            libexec_bin = prefix / "libexec" / bin_name
            wrapper = stage / f"{bin_name}-wrapper.sh"
            wrapper.write_text(
                "#!/bin/sh\n"
                'export PHOTON_PROFILE="${PHOTON_PROFILE:-prod}"\n'
                f'exec "{libexec_bin}" "$@"\n',
                encoding="utf-8",
            )
            targets[f"{bin_name} libexec"] = libexec_bin
            sources[f"{bin_name} libexec"] = (built_bin, "755")
            sources[bin_name] = (wrapper, "755")
        else:
            stale_libexec = prefix / "libexec" / bin_name
            if stale_libexec.exists() or stale_libexec.is_symlink():
                run(maybe_privileged(["rm", "-f", str(stale_libexec)], needs_root))
            sources[bin_name] = (built_bin, "755")

    for icon_name, role in ICON_ROLES.items():
        svg_src = PACKAGING / f"{icon_name}.svg"
        for edge in ICON_SIZES:
            staged_png = (
                stage / "icons" / role / f"{edge}x{edge}" / f"{icon_name}.png"
            )
            if rasterise_svg(svg_src, edge, staged_png):
                sources[f"icon {role} {edge}"] = (staged_png, "644")

    for key, (src, mode) in sources.items():
        dst = targets[key]
        res = run(maybe_privileged(["install", "-D", f"-m{mode}", str(src), str(dst)], needs_root))
        if res.returncode != 0:
            return res.returncode

    refresh_caches(prefix, needs_root)

    warn_if_user_install_shadows(prefix)

    if set_default_viewer:
        set_default_viewer_mime_types()

    print(f"Installed Photon ({APP_ID}) into {prefix}:")
    for key, (src, _) in sources.items():
        print(f"  {key:22} -> {targets[key]}")
    print("Registered MIME handlers:")
    print(f"  Browser: {targets['browser desktop']} (Exec: photon-manager %U)")
    print(
        f"  Viewer:  {targets['standalone viewer desktop']} "
        "(Exec: photon-viewer %F)"
    )
    print(f"  Editor:  {targets['editor desktop']} (Exec: photon-editor %F)")
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
    for bin_name in ("photon", "photon-manager", "photon-viewer", "photon-editor"):
        libexec_bin = prefix / "libexec" / bin_name
        if libexec_bin.exists():
            targets[f"{bin_name} libexec"] = libexec_bin

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
