import contextlib
import hashlib
import importlib.util
import io
import json
import sqlite3
import tempfile
import unittest
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
        return source_cache.build_cache(url, sha(url), json.dumps(value).encode(), "2026-11-04T00:00:00Z")

    def test_hashes_titles_and_stores_no_source_content(self):
        document = self.build(catalog())
        self.assertEqual(document["version"], 1)
        self.assertEqual(document["contentSha256"], CONTENT)
        self.assertEqual(document["matches"], {sha("Portal"): 400})
        self.assertEqual(document["unmatched"], [sha("Mystery Game")])
        text = json.dumps(document)
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
            source_cache.build_cache(URL, "0" * 64, json.dumps(catalog()).encode(), "now")

    def test_exports_files_named_after_the_link_hash(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "legio.sqlite3"
            connection = sqlite3.connect(database)
            connection.execute("CREATE TABLE download_sources (url TEXT, url_sha256 TEXT, manifest BLOB)")
            connection.execute("INSERT INTO download_sources VALUES (?, ?, ?)", (URL, sha(URL), json.dumps(catalog()).encode()))
            connection.commit()
            connection.close()
            out = root / "out"
            with contextlib.redirect_stdout(io.StringIO()):
                code = source_cache.main(["--db", str(database), "--out", str(out), "https://EXAMPLE.test:443/hydra.json"])
            self.assertEqual(code, 0)
            written = json.loads((out / f"{sha(URL)}.json").read_text(encoding="utf-8"))
            self.assertEqual(written["matches"], {sha("Portal"): 400})
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                missing = source_cache.main(["--db", str(database), "--out", str(out), "https://other.test/a.json"])
            self.assertEqual(missing, 1)

    def test_upload_needs_credentials_from_the_environment(self):
        with unittest.mock.patch.dict("os.environ", {}, clear=True):
            with self.assertRaisesRegex(source_cache.ToolError, "R2_BUCKET"):
                source_cache.upload([])


if __name__ == "__main__":
    unittest.main()
