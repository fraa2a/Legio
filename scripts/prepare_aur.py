#!/usr/bin/env python3
"""Fill AUR package metadata with the published AppImage and checksums."""

import argparse
import hashlib
from pathlib import Path
from urllib.parse import quote


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("version")
    args = parser.parse_args()
    appimages = list(args.directory.glob("*.AppImage"))
    if len(appimages) != 1:
        raise SystemExit(f"Expected one AppImage, found {len(appimages)}")
    image = appimages[0]
    package = args.directory / "PKGBUILD"
    content = package.read_text(encoding="utf-8")
    values = {
        "pkgver=0.1.0": f"pkgver={args.version}",
        "APPIMAGE_URL": f"https://github.com/fraa2a/Legio/releases/download/v{quote(args.version)}/{quote(image.name)}",
        "APPIMAGE_SHA256": sha256(image),
        "ICON_SHA256": sha256(args.directory / "icon.png"),
        "WRAPPER_SHA256": sha256(args.directory / "legio-launcher"),
        "DESKTOP_SHA256": sha256(args.directory / "legio-launcher.desktop"),
        "LICENSE_SHA256": sha256(args.directory / "LICENSE"),
    }
    for old, new in values.items():
        if old not in content:
            raise SystemExit(f"Missing template value: {old}")
        content = content.replace(old, new)
    package.write_text(content, encoding="utf-8")
