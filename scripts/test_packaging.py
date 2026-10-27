import hashlib
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class PackagingTests(unittest.TestCase):
    def test_git_package_verifies_wrapper_but_not_moving_git_source(self):
        recipe = ROOT / "packaging/aur-git"
        text = (recipe / "PKGBUILD").read_text(encoding="utf-8")
        match = re.search(r"sha256sums=\('SKIP' '([a-f0-9]{64})'\)", text)
        self.assertIsNotNone(match, "Keep the Git source unpinned and the local wrapper checksummed")
        expected = hashlib.sha256((recipe / "legio-launcher").read_bytes()).hexdigest()
        self.assertEqual(match[1], expected, "Update the wrapper checksum in PKGBUILD when changing the wrapper")

    def test_bundles_include_required_license_notices(self):
        manifest = ROOT / "src-tauri/tauri.conf.json"
        resources = json.loads(manifest.read_text(encoding="utf-8"))["bundle"]["resources"]
        required = {
            "../LICENSE": "LICENSE",
            "../THIRD_PARTY_NOTICES.md": "THIRD_PARTY_NOTICES.md",
            "../src/assets/fonts/OFL.txt": "licenses/Inter-OFL.txt",
            "../_meta/licenses/glow-icons.txt": "licenses/glow-icons.txt",
            "vendor/flexsearch.LICENSE": "licenses/flexsearch.LICENSE",
        }
        for source, destination in required.items():
            self.assertEqual(resources.get(source), destination)
            self.assertTrue((manifest.parent / source).is_file(), source)


if __name__ == "__main__":
    unittest.main()
