"""Select checks and releases for one GitHub Actions event."""

import os
import re
import subprocess
import tomllib
from pathlib import Path


VERSION_PATTERN = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)
MANIFEST = "src-tauri/Cargo.toml"
DOCUMENTS = {
    "README.md", "CONTRIBUTING.md", "SECURITY.md", "AGENTS.md",
    "scripts/README.md", "tools/linux-rendering/README.md",
}


def parse_version(value):
    match = VERSION_PATTERN.fullmatch(value)
    if not match:
        raise ValueError(f"Invalid Semantic Version: {value}")
    major, minor, patch, prerelease, _build = match.groups()
    identifiers = prerelease.split(".") if prerelease else None
    if identifiers and any(item.isdigit() and len(item) > 1 and item[0] == "0" for item in identifiers):
        raise ValueError(f"Invalid numeric prerelease identifier: {value}")
    return (int(major), int(minor), int(patch)), identifiers


def compare_versions(left, right):
    left_core, left_pre = parse_version(left)
    right_core, right_pre = parse_version(right)
    if left_core != right_core:
        return 1 if left_core > right_core else -1
    if left_pre is None or right_pre is None:
        return (left_pre is None) - (right_pre is None)
    for a, b in zip(left_pre, right_pre):
        if a == b:
            continue
        if a.isdigit() and b.isdigit():
            return 1 if int(a) > int(b) else -1
        if a.isdigit() != b.isdigit():
            return -1 if a.isdigit() else 1
        return 1 if a > b else -1
    return (len(left_pre) > len(right_pre)) - (len(left_pre) < len(right_pre))


def is_documentation(path):
    return path in DOCUMENTS or path.endswith("/README.md") or path.startswith(("_meta/docs/", "_meta/screenshots/"))


def git(*args):
    return subprocess.check_output(["git", *args], stderr=subprocess.DEVNULL).decode("utf-8")


def plan(base, event):
    current = tomllib.loads(Path(MANIFEST).read_text(encoding="utf-8"))["package"]["version"]
    parse_version(current)
    try:
        previous = tomllib.loads(git("show", f"{base}:{MANIFEST}"))["package"]["version"]
        paths = git("diff", "--name-only", "--no-renames", base, "HEAD").splitlines()
    except subprocess.CalledProcessError:
        previous = None
        paths = git("ls-tree", "-r", "--name-only", "HEAD").splitlines()

    changed = previous is not None and current != previous
    if changed:
        if compare_versions(current, previous) <= 0:
            raise ValueError(f"Version must increase: {previous} -> {current}")
        if git("tag", "--list", f"v{current}").strip():
            raise ValueError(f"Tag already exists: v{current}")

    release = changed and event == "push" and os.environ.get("GITHUB_REF") == "refs/heads/main"
    return {
        "code": event == "workflow_dispatch" or any(not is_documentation(path) for path in paths),
        "release": release,
        "version": current,
        "tag": f"v{current}",
        "prerelease": parse_version(current)[1] is not None,
    }


if __name__ == "__main__":
    values = plan(os.environ.get("BASE_SHA", ""), os.environ["GITHUB_EVENT_NAME"])
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        for name, value in values.items():
            output.write(f"{name}={str(value).lower() if isinstance(value, bool) else value}\n")
