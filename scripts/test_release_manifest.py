import tempfile
import unittest
from pathlib import Path

from release_manifest import release_manifest


class ReleaseManifestTests(unittest.TestCase):
    def test_requires_each_signed_installer(self):
        with tempfile.TemporaryDirectory() as directory:
            assets = Path(directory)
            for suffix in (".deb", ".rpm", ".AppImage", ".exe"):
                (assets / f"Legio{suffix}").write_bytes(b"bundle")
                (assets / f"Legio{suffix}.sig").write_text("signature", encoding="utf-8")

            manifest = release_manifest(assets, "1.2.3", "fraa2a/Legio")
            self.assertEqual(len(manifest["platforms"]), 4)

            (assets / "Legio.rpm.sig").unlink()
            with self.assertRaisesRegex(ValueError, "Missing signature"):
                release_manifest(assets, "1.2.3", "fraa2a/Legio")


if __name__ == "__main__":
    unittest.main()
