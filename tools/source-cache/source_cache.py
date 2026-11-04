#!/usr/bin/env python3
"""Maintain the shared game identification cache published by Legio.

Legio stores the result of its own title identification in its SQLite database.
This tool exports that result into one cache.json, so the published file carries
exactly what the app computes and no matching logic is duplicated here. The file
holds hashes only; no source URL and no download title is written.

    python tools/source-cache/source_cache.py <url> [<url>...]
    python tools/source-cache/source_cache.py --from-file links.txt --upload
    python tools/source-cache/source_cache.py --remove <url> --upload

See README.md for the schema, the credentials file and publishing.
"""

import argparse
import datetime
import hashlib
import json
import os
import re
import sqlite3
import sys
import urllib.error
import urllib.request
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

VERSION = 2
PUBLIC_URL = "https://legio-cache.fraa2a.dev/cache.json"
OBJECT_KEY = "cache.json"
MAX_BYTES = 32 * 1024 * 1024
APP_IDENTIFIER = "dev.fraa2a.legio"
DATABASE_NAME = "legio.sqlite3"
HEX64 = re.compile(r"[0-9a-f]{64}")
MAX_APP_ID = 2**32 - 1
CREDENTIALS_FILE = Path(__file__).resolve().with_name("r2.local.json")
# JSON field in the credentials file -> environment variable that overrides it.
CREDENTIAL_FIELDS = {
    "accountId": "R2_ACCOUNT_ID",
    "accessKeyId": "R2_ACCESS_KEY_ID",
    "secretAccessKey": "R2_SECRET_ACCESS_KEY",
    "bucket": "R2_BUCKET",
}
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


def build_entry(url, stored_hash, manifest, generated_at):
    """Return the cache entry for one installed source."""
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
        "contentSha256": content_sha256,
        "generatedAt": generated_at,
        "matches": dict(sorted(hashed_matches.items())),
        "unmatched": sorted(hashed_unmatched),
    }


def load_base(base):
    """Return the sources of the current cache: a local file, or the published one (empty on 404)."""
    if base:
        try:
            data = Path(base).read_bytes()
        except OSError as error:
            raise ToolError(f"Could not read the base file: {error}") from error
    else:
        request = urllib.request.Request(PUBLIC_URL, headers={"User-Agent": "legio-source-cache"})
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                data = response.read(MAX_BYTES + 1)
        except urllib.error.HTTPError as error:
            if error.code == 404:
                return {}
            raise ToolError(f"Could not download the published cache: HTTP {error.code}") from error
        except (urllib.error.URLError, TimeoutError) as error:
            raise ToolError(f"Could not download the published cache: {error}") from error
    if len(data) > MAX_BYTES:
        raise ToolError("The current cache exceeds the size limit.")
    try:
        document = json.loads(data)
    except ValueError as error:
        raise ToolError("The current cache is not valid JSON.") from error
    if not isinstance(document, dict) or document.get("version") != VERSION:
        raise ToolError(f"The current cache is not a schema v{VERSION} file.")
    sources = document.get("sources")
    if not isinstance(sources, dict):
        raise ToolError(f"The current cache is not a schema v{VERSION} file.")
    return sources


def remove_source(sources, link):
    """Drop the entry for `link`; return whether one existed."""
    found = False
    for key in {sha256_hex(link.strip()), sha256_hex(normalize_url(link))}:
        found = sources.pop(key, None) is not None or found
    return found


def write_cache(path, sources, generated_at):
    document = {"version": VERSION, "generatedAt": generated_at, "sources": dict(sorted(sources.items()))}
    data = (json.dumps(document, separators=(",", ":")) + "\n").encode("utf-8")
    if len(data) > MAX_BYTES:
        raise ToolError("The cache would exceed the 32 MiB limit.")
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return path


def load_credentials():
    """Read R2 credentials from r2.local.json; environment variables override the file."""
    stored = {}
    if CREDENTIALS_FILE.is_file():
        try:
            stored = json.loads(CREDENTIALS_FILE.read_text(encoding="utf-8"))
        except (OSError, ValueError) as error:
            raise ToolError(f"Could not read {CREDENTIALS_FILE.name}: {error}") from error
        if not isinstance(stored, dict):
            raise ToolError(f"{CREDENTIALS_FILE.name} must be a JSON object.")
    credentials = {}
    for field, variable in CREDENTIAL_FIELDS.items():
        value = os.environ.get(variable) or stored.get(field)
        if isinstance(value, str) and value.strip():
            credentials[field] = value.strip()
    missing = [f"{field} ({variable})" for field, variable in CREDENTIAL_FIELDS.items() if field not in credentials]
    if missing:
        raise ToolError(
            f"Missing R2 credentials: {', '.join(missing)}. "
            f"Fill in {CREDENTIALS_FILE.name} (template: r2.example.json) or set the environment variables."
        )
    return credentials


def upload(path, credentials):
    try:
        import boto3
    except ImportError as error:
        raise ToolError("--upload needs boto3. Install it with: pip install boto3") from error
    client = boto3.client(
        "s3",
        endpoint_url=f"https://{credentials['accountId']}.r2.cloudflarestorage.com",
        aws_access_key_id=credentials["accessKeyId"],
        aws_secret_access_key=credentials["secretAccessKey"],
        region_name="auto",
    )
    try:
        client.put_object(
            Bucket=credentials["bucket"],
            Key=OBJECT_KEY,
            Body=Path(path).read_bytes(),
            ContentType="application/json",
            CacheControl=CACHE_CONTROL,
        )
    except Exception as error:  # botocore errors vary; report only the type, never request details
        raise ToolError(f"Upload failed ({type(error).__name__}). Check the credentials and the bucket name.") from None
    print(f"uploaded {OBJECT_KEY}")


def parse_arguments(argv):
    parser = argparse.ArgumentParser(description="Update the Legio shared game identification cache.")
    parser.add_argument("links", nargs="*", metavar="url", help="HTTPS source links installed in Legio")
    parser.add_argument("--from-file", metavar="FILE", help="text file with one link per line")
    parser.add_argument("--remove", action="append", default=[], metavar="URL", help="drop a source from the cache")
    parser.add_argument("--base", metavar="FILE", help="start from this cache file instead of the published one")
    parser.add_argument("--out", default="cache.json", metavar="FILE", help="cache file to write (default: cache.json)")
    parser.add_argument("--db", metavar="FILE", help="Legio database (default: the app data folder)")
    parser.add_argument("--upload", action="store_true", help="upload the file to Cloudflare R2")
    arguments = parser.parse_args(argv)
    if arguments.from_file:
        arguments.links += read_links(arguments.from_file)
    if not arguments.links and not arguments.remove:
        parser.error("give at least one source link, --from-file or --remove")
    return arguments


def main(argv=None):
    arguments = parse_arguments(argv)
    try:
        credentials = load_credentials() if arguments.upload else None
        sources = load_base(arguments.base)
        installed = load_sources(arguments.db or default_database()) if arguments.links else {}
    except ToolError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    generated_at = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    changed, failed = 0, 0
    for link in arguments.remove:
        if remove_source(sources, link):
            changed += 1
            print("removed 1 source")
        else:
            print("remove: source is not in the cache", file=sys.stderr)
    for number, link in enumerate(arguments.links, start=1):
        try:
            url, (stored_hash, manifest) = find_source(installed, link)
            entry = build_entry(url, stored_hash, manifest, generated_at)
        except ToolError as error:
            failed += 1
            print(f"skipped source {number}: {error}", file=sys.stderr)
            continue
        sources[stored_hash] = entry
        changed += 1
        print(f"updated source {number} ({len(entry['matches'])} matched, {len(entry['unmatched'])} unmatched)")
    if not changed:
        print("error: nothing to publish", file=sys.stderr)
        return 1
    try:
        path = write_cache(arguments.out, sources, generated_at)
        print(f"wrote {path} ({len(sources)} sources)")
        if credentials:
            upload(path, credentials)
    except ToolError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
