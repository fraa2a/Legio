#!/usr/bin/env python3
"""Build the signed Tauri update manifest from release bundles."""

import argparse
import json
from pathlib import Path
from urllib.parse import quote


def release_manifest(assets: Path, version: str, repository: str) -> dict:
    platforms = {}
    bundles = {
        "linux-x86_64-deb": ".deb",
        "linux-x86_64-rpm": ".rpm",
        "linux-x86_64-appimage": ".AppImage",
        "windows-x86_64-nsis": ".exe",
    }
    for target, suffix in bundles.items():
        matches = sorted(path for path in assets.iterdir() if path.name.endswith(suffix))
        if len(matches) != 1:
            raise ValueError(f"Expected one {suffix} bundle, found {len(matches)}")
        bundle = matches[0]
        signature = bundle.with_name(bundle.name + ".sig")
        if not signature.is_file():
            raise ValueError(f"Missing signature: {signature.name}")
        platforms[target] = {
            "url": f"https://github.com/{repository}/releases/download/v{quote(version)}/{quote(bundle.name)}",
            "signature": signature.read_text(encoding="utf-8").strip(),
        }
    return {"version": version, "platforms": platforms}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("assets", type=Path)
    parser.add_argument("version")
    parser.add_argument("repository")
    args = parser.parse_args()
    (args.assets / "latest.json").write_text(
        json.dumps(release_manifest(args.assets, args.version, args.repository), indent=2) + "\n",
        encoding="utf-8",
    )
