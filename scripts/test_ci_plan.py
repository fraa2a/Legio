import os
import subprocess
import tempfile
import unittest
from contextlib import chdir
from pathlib import Path
from unittest.mock import patch

from ci_plan import compare_versions, is_documentation, parse_version, plan


class VersionTests(unittest.TestCase):
    def test_semver_order(self):
        versions = ["1.0.0-alpha.1", "1.0.0-alpha.2", "1.0.0-beta.1", "1.0.0-rc.1", "1.0.0", "1.0.1"]
        for earlier, later in zip(versions, versions[1:]):
            self.assertLess(compare_versions(earlier, later), 0)
            self.assertGreater(compare_versions(later, earlier), 0)
        self.assertEqual(compare_versions("1.0.0+abc", "1.0.0+def"), 0)

    def test_invalid_versions(self):
        for value in ["01.0.0", "1.2", "1.0.0-alpha.01", "1.0.0-", "1.0.0\n"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                parse_version(value)

    def test_only_documentation_paths_are_lightweight(self):
        for path in ["README.md", "_meta/docs/LINUX.md", "_meta/screenshots/home.png", "packaging/aur-git/README.md"]:
            self.assertTrue(is_documentation(path))
        for path in ["LICENSE", "THIRD_PARTY_NOTICES.md", ".github/workflows/ci.yml", "src/main.ts", "packaging/aur-git/PKGBUILD"]:
            self.assertFalse(is_documentation(path))


class PlanTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.context = chdir(self.directory.name)
        self.context.__enter__()
        self.addCleanup(self.context.__exit__, None, None, None)
        self.git("init", "-q")
        Path("src-tauri").mkdir()
        self.version("1.0.0")
        self.commit()
        self.base = self.git("rev-parse", "HEAD").strip()

    def git(self, *args):
        return subprocess.check_output(["git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid", *args], stderr=subprocess.DEVNULL).decode()

    def version(self, value):
        Path("src-tauri/Cargo.toml").write_text(f'[package]\nversion = "{value}"\n', encoding="utf-8")

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-qm", "test")

    def test_docs_skip_native_builds(self):
        Path("README.md").write_text("docs", encoding="utf-8")
        self.commit()
        result = plan(self.base, "push")
        self.assertFalse(result["code"])
        self.assertFalse(result["release"])

    def test_code_and_manual_events_run_checks_without_release(self):
        Path("source.rs").write_text("code", encoding="utf-8")
        self.commit()
        self.assertTrue(plan(self.base, "pull_request")["code"])
        self.assertTrue(plan(self.base, "workflow_dispatch")["code"])
        self.assertFalse(plan(self.base, "workflow_dispatch")["release"])

    def test_release_only_for_version_change_on_main_push(self):
        self.version("1.1.0-beta.1")
        self.commit()
        with patch.dict(os.environ, GITHUB_REF="refs/heads/main"):
            result = plan(self.base, "push")
            self.assertTrue(result["release"])
            self.assertTrue(result["code"])
            self.assertTrue(result["prerelease"])
            self.assertEqual(result["tag"], "v1.1.0-beta.1")
            self.assertFalse(plan(self.base, "pull_request")["release"])
        with patch.dict(os.environ, GITHUB_REF="refs/heads/feature"):
            self.assertFalse(plan(self.base, "push")["release"])

    def test_rejects_decrease_and_existing_tag(self):
        self.version("0.9.0")
        with self.assertRaisesRegex(ValueError, "must increase"):
            plan(self.base, "push")
        self.version("1.1.0")
        self.git("tag", "v1.1.0")
        with self.assertRaisesRegex(ValueError, "Tag already exists"):
            plan(self.base, "push")

    def test_unknown_base_is_conservative_without_releasing(self):
        result = plan("0" * 40, "push")
        self.assertTrue(result["code"])
        self.assertFalse(result["release"])


if __name__ == "__main__":
    unittest.main()
