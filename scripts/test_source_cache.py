import contextlib
import hashlib
import importlib.util
import io
import json
import sqlite3
import tempfile
import unittest
import os
import unittest.mock
from pathlib import Path

TOOL = Path(__file__).resolve().parents[1] / "tools" / "source-cache" / "source_cache.py"
spec = importlib.util.spec_from_file_location("source_cache", TOOL)
source_cache = importlib.util.module_from_spec(spec)
spec.loader.exec_module(source_cache)

URL = "https://example.test/hydra.json"
CONTENT = "ab" * 32


def sha(value):
    return hashlib.sha256(value.encode()).hexdigest()


def catalog(**changes):
    value = {
        "format": "hydra",
        "contentSha256": CONTENT,
        "matches": {"Portal": 400},
        "unmatched": ["Mystery Game"],
        "rawHydra": {"name": "Test", "downloads": [{"title": "Portal"}, {"title": "Mystery Game"}]},
    }
    value.update(changes)
    return value


class SourceCacheTests(unittest.TestCase):
    def build(self, value, url=URL):
        return source_cache.build_entry(url, sha(url), json.dumps(value).encode(), "2026-11-04T00:00:00Z")

    def test_hashes_titles_and_stores_no_source_content(self):
        entry = self.build(catalog())
        self.assertEqual(entry["contentSha256"], CONTENT)
        self.assertEqual(entry["matches"], {sha("Portal"): 400})
        self.assertEqual(entry["unmatched"], [sha("Mystery Game")])
        text = json.dumps(entry)
        self.assertNotIn("Portal", text)
        self.assertNotIn("example.test", text)

    def test_rejects_incomplete_or_unsupported_sources(self):
        cases = [
            catalog(unmatched=[]),
            catalog(contentSha256=None),
            catalog(matches={"Portal": 0}),
            catalog(format="legio"),
        ]
        for value in cases:
            with self.assertRaises(source_cache.ToolError):
                self.build(value)
        with self.assertRaises(source_cache.ToolError):
            self.build(catalog(), url="http://example.test/hydra.json")
        with self.assertRaises(source_cache.ToolError):
            source_cache.build_entry(URL, "0" * 64, json.dumps(catalog()).encode(), "now")

    def make_database(self, root):
        database = root / "legio.sqlite3"
        connection = sqlite3.connect(database)
        connection.execute("CREATE TABLE download_sources (url TEXT, url_sha256 TEXT, manifest BLOB)")
        connection.execute("INSERT INTO download_sources VALUES (?, ?, ?)", (URL, sha(URL), json.dumps(catalog()).encode()))
        connection.commit()
        connection.close()
        return database

    def run_main(self, *arguments):
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            return source_cache.main(list(arguments))

    def test_upserts_into_one_file_and_keeps_other_sources(self):
        other = {"contentSha256": CONTENT, "generatedAt": "old", "matches": {}, "unmatched": []}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = self.make_database(root)
            base = root / "base.json"
            base.write_text(json.dumps({"version": 2, "generatedAt": "old", "sources": {"f" * 64: other}}), encoding="utf-8")
            out = root / "out" / "cache.json"
            code = self.run_main("--db", str(database), "--base", str(base), "--out", str(out), "https://EXAMPLE.test:443/hydra.json")
            self.assertEqual(code, 0)
            written = json.loads(out.read_text(encoding="utf-8"))
            self.assertEqual(written["version"], 2)
            self.assertEqual(set(written["sources"]), {"f" * 64, sha(URL)})
            self.assertEqual(written["sources"]["f" * 64], other)
            self.assertEqual(written["sources"][sha(URL)]["matches"], {sha("Portal"): 400})
            missing = self.run_main("--db", str(database), "--base", str(base), "--out", str(out), "https://other.test/a.json")
            self.assertEqual(missing, 1)

    def test_removes_a_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            base = root / "base.json"
            entry = {"contentSha256": CONTENT, "generatedAt": "old", "matches": {}, "unmatched": []}
            base.write_text(json.dumps({"version": 2, "generatedAt": "old", "sources": {sha(URL): entry}}), encoding="utf-8")
            out = root / "cache.json"
            self.assertEqual(self.run_main("--base", str(base), "--out", str(out), "--remove", "https://EXAMPLE.test:443/hydra.json"), 0)
            self.assertEqual(json.loads(out.read_text(encoding="utf-8"))["sources"], {})
            self.assertEqual(self.run_main("--base", str(out), "--out", str(out), "--remove", URL), 1)

    def test_rejects_a_base_with_another_version(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory) / "base.json"
            base.write_text(json.dumps({"version": 1, "sources": {}}), encoding="utf-8")
            with self.assertRaises(source_cache.ToolError):
                source_cache.load_base(str(base))

    def test_starts_empty_when_nothing_is_published(self):
        error = source_cache.urllib.error.HTTPError(source_cache.PUBLIC_URL, 404, "Not Found", None, None)
        with unittest.mock.patch.object(source_cache.urllib.request, "urlopen", side_effect=error):
            self.assertEqual(source_cache.load_base(None), {})
        failure = source_cache.urllib.error.HTTPError(source_cache.PUBLIC_URL, 500, "Error", None, None)
        with unittest.mock.patch.object(source_cache.urllib.request, "urlopen", side_effect=failure):
            with self.assertRaises(source_cache.ToolError):
                source_cache.load_base(None)

    def test_credentials_name_the_missing_fields(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "r2.local.json"
            file.write_text(json.dumps({"accountId": "", "accessKeyId": "key", "secretAccessKey": "", "bucket": ""}), encoding="utf-8")
            with unittest.mock.patch.object(source_cache, "CREDENTIALS_FILE", file):
                with unittest.mock.patch.dict(os.environ, {}, clear=True):
                    with self.assertRaises(source_cache.ToolError) as raised:
                        source_cache.load_credentials()
                message = str(raised.exception)
                for name in ("accountId", "R2_ACCOUNT_ID", "secretAccessKey", "bucket"):
                    self.assertIn(name, message)
                self.assertNotIn("accessKeyId", message)
                environment = {"R2_ACCOUNT_ID": "acc", "R2_SECRET_ACCESS_KEY": "secret", "R2_BUCKET": "legio"}
                with unittest.mock.patch.dict(os.environ, environment, clear=True):
                    self.assertEqual(
                        source_cache.load_credentials(),
                        {"accountId": "acc", "accessKeyId": "key", "secretAccessKey": "secret", "bucket": "legio"},
                    )

    def test_credentials_template_has_empty_values(self):
        template = TOOL.with_name("r2.example.json")
        self.assertEqual(
            json.loads(template.read_text(encoding="utf-8")),
            {"accountId": "", "accessKeyId": "", "secretAccessKey": "", "bucket": ""},
        )

    def test_upload_stops_before_work_without_credentials(self):
        with tempfile.TemporaryDirectory() as directory:
            missing = Path(directory) / "r2.local.json"
            with unittest.mock.patch.object(source_cache, "CREDENTIALS_FILE", missing):
                with unittest.mock.patch.dict(os.environ, {}, clear=True):
                    self.assertEqual(self.run_main("--upload", "--base", str(missing), URL), 1)


if __name__ == "__main__":
    unittest.main()
