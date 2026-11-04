#!/usr/bin/env python3
"""Build the shared game identification cache from sources installed in Legio.

Legio stores the result of its own title identification in its SQLite database.
This tool exports that result, so the published files carry exactly what the
app computes and no matching logic is duplicated here. Files hold hashes only;
no source URL and no download title is written.

    python tools/source-cache/source_cache.py --out cache/v1 <url> [<url>...]
    python tools/source-cache/source_cache.py --out cache/v1 --from-file links.txt --upload

See README.md for the schema and for publishing.
"""

import argparse
import datetime
import hashlib
import json
import os
import re
import sqlite3
import sys
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

VERSION = 1
PREFIX = "v1/"
APP_IDENTIFIER = "dev.fraa2a.legio"
DATABASE_NAME = "legio.sqlite3"
HEX64 = re.compile(r"[0-9a-f]{64}")
MAX_APP_ID = 2**32 - 1
UPLOAD_VARIABLES = ("R2_ACCOUNT_ID", "R2_ACCESS_KEY_ID", "R2_SECRET_ACCESS_KEY", "R2_BUCKET")
CACHE_CONTROL = "public, max-age=3600"


class ToolError(Exception):
    pass


def sha256_hex(value):
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def default_database():
    if sys.platform == "win32":
        base = Path(os.environ.get("APPDATA", Path.home() / "AppData" / "Roaming"))
    elif sys.platform == "darwin":
        base = Path.home() / "Library" / "Application Support"
    else:
        base = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local" / "share"))
    return base / APP_IDENTIFIER / DATABASE_NAME


def normalize_url(url):
    """Mirror the validated form the app stores (lowercase host, no default port, `/` path)."""
    parts = urlsplit(url.strip())
    host = (parts.hostname or "").lower()
    port = parts.port
    netloc = host if port in (None, 443) else f"{host}:{port}"
    userinfo = parts.netloc.rpartition("@")[0]
    if userinfo:
        netloc = f"{userinfo}@{netloc}"
    return urlunsplit((parts.scheme.lower(), netloc, parts.path or "/", parts.query, ""))


def read_links(path):
    links = []
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            links.append(line)
    return links


def load_sources(database):
    path = Path(database)
    if not path.is_file():
        raise ToolError(f"Legio database not found: {path}")
    try:
        connection = sqlite3.connect(f"{path.resolve().as_uri()}?mode=ro", uri=True)
    except sqlite3.Error as error:
        raise ToolError(f"Could not open the Legio database: {error}") from error
    try:
        rows = connection.execute("SELECT url, url_sha256, manifest FROM download_sources").fetchall()
    except sqlite3.Error as error:
        raise ToolError(f"Could not read installed sources: {error}") from error
    finally:
        connection.close()
    return {url: (stored_hash, manifest) for url, stored_hash, manifest in rows}


def find_source(sources, link):
    for candidate in (link, normalize_url(link)):
        if candidate in sources:
            return candidate, sources[candidate]
    raise ToolError("Source is not installed in Legio. Add it in Settings > Sources first.")


def build_cache(url, stored_hash, manifest, generated_at):
    """Return the cache document for one installed source."""
    if not url.startswith("https://"):
        raise ToolError("Only HTTPS sources can be published.")
    if stored_hash != sha256_hex(url):
        raise ToolError("The stored link hash does not match the link.")
    try:
        catalog = json.loads(bytes(manifest))
    except (TypeError, ValueError) as error:
        raise ToolError("The stored source cannot be read.") from error
    if catalog.get("format") != "hydra":
        raise ToolError("Only Hydra sources use the identification cache.")
    content_sha256 = catalog.get("contentSha256")
    if not isinstance(content_sha256, str) or not HEX64.fullmatch(content_sha256):
        raise ToolError("Identification data is missing. Refresh the source in Legio, then retry.")
    matches = catalog.get("matches") or {}
    unmatched = set(catalog.get("unmatched") or [])
    titles = [entry["title"] for entry in (catalog.get("rawHydra") or {}).get("downloads", [])]
    pending = [title for title in titles if title not in matches and title not in unmatched]
    if pending:
        raise ToolError(
            f"{len(pending)} titles were not looked up (stopped or failed). "
            "Refresh the source in Legio until it identifies every title, then retry."
        )
    hashed_matches = {}
    for title, app_id in matches.items():
        if not isinstance(app_id, int) or isinstance(app_id, bool) or not 0 < app_id <= MAX_APP_ID:
            raise ToolError("The stored source contains an invalid app id.")
        hashed_matches[sha256_hex(title)] = app_id
    hashed_unmatched = {sha256_hex(title) for title in unmatched} - hashed_matches.keys()
    return {
        "version": VERSION,
        "contentSha256": content_sha256,
        "generatedAt": generated_at,
        "matches": dict(sorted(hashed_matches.items())),
        "unmatched": sorted(hashed_unmatched),
    }


def write_cache(out, name, document):
    out.mkdir(parents=True, exist_ok=True)
    path = out / name
    path.write_text(json.dumps(document, separators=(",", ":")) + "\n", encoding="utf-8")
    return path


def upload(paths):
    missing = [name for name in UPLOAD_VARIABLES if not os.environ.get(name)]
    if missing:
        raise ToolError(f"Set these environment variables to upload: {', '.join(missing)}")
    try:
        import boto3
    except ImportError as error:
        raise ToolError("--upload needs boto3. Install it with: pip install boto3") from error
    client = boto3.client(
        "s3",
        endpoint_url=f"https://{os.environ['R2_ACCOUNT_ID']}.r2.cloudflarestorage.com",
        aws_access_key_id=os.environ["R2_ACCESS_KEY_ID"],
        aws_secret_access_key=os.environ["R2_SECRET_ACCESS_KEY"],
        region_name="auto",
    )
    bucket = os.environ["R2_BUCKET"]
    for path in paths:
        client.put_object(
            Bucket=bucket,
            Key=PREFIX + path.name,
            Body=path.read_bytes(),
            ContentType="application/json",
            CacheControl=CACHE_CONTROL,
        )
        print(f"uploaded {PREFIX}{path.name}")


def parse_arguments(argv):
    parser = argparse.ArgumentParser(description="Export the Legio game identification cache.")
    parser.add_argument("links", nargs="*", metavar="url", help="HTTPS source links installed in Legio")
    parser.add_argument("--from-file", metavar="FILE", help="text file with one link per line")
    parser.add_argument("--out", required=True, metavar="DIR", help="directory for the cache files")
    parser.add_argument("--db", metavar="FILE", help="Legio database (default: the app data folder)")
    parser.add_argument("--upload", action="store_true", help="upload the files to Cloudflare R2")
    arguments = parser.parse_args(argv)
    if arguments.from_file:
        arguments.links += read_links(arguments.from_file)
    if not arguments.links:
        parser.error("give at least one source link or --from-file")
    return arguments


def main(argv=None):
    arguments = parse_arguments(argv)
    try:
        sources = load_sources(arguments.db or default_database())
    except ToolError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    generated_at = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    written, failed = [], 0
    for link in arguments.links:
        label = f"source {len(written) + failed + 1}"
        try:
            url, (stored_hash, manifest) = find_source(sources, link)
            document = build_cache(url, stored_hash, manifest, generated_at)
        except ToolError as error:
            failed += 1
            print(f"skipped {label}: {error}", file=sys.stderr)
            continue
        path = write_cache(Path(arguments.out), f"{stored_hash}.json", document)
        written.append(path)
        print(f"wrote {path} ({len(document['matches'])} matched, {len(document['unmatched'])} unmatched)")
    if arguments.upload and written:
        try:
            upload(written)
        except ToolError as error:
            print(f"error: {error}", file=sys.stderr)
            return 1
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
