import json
import subprocess
import sys
import tempfile
import unittest
from datetime import datetime
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "updater_manifest.py"
ASSETS = {
    "pipedash_universal.app.tar.gz": [
        "darwin-aarch64",
        "darwin-x86_64",
        "darwin-aarch64-app",
        "darwin-x86_64-app",
    ],
    "pipedash_0.14.0_amd64.AppImage": ["linux-x86_64", "linux-x86_64-appimage"],
    "pipedash_0.14.0_aarch64.AppImage": ["linux-aarch64", "linux-aarch64-appimage"],
    "pipedash_0.14.0_amd64.deb": ["linux-x86_64-deb"],
    "pipedash_0.14.0_arm64.deb": ["linux-aarch64-deb"],
    "pipedash-0.14.0-1.x86_64.rpm": ["linux-x86_64-rpm"],
    "pipedash-0.14.0-1.aarch64.rpm": ["linux-aarch64-rpm"],
    "pipedash_0.14.0_x64-setup.exe": ["windows-x86_64", "windows-x86_64-nsis"],
    "pipedash_0.14.0_x86-setup.exe": ["windows-i686", "windows-i686-nsis"],
    "pipedash_0.14.0_arm64-setup.exe": ["windows-aarch64", "windows-aarch64-nsis"],
}


class UpdaterManifestTest(unittest.TestCase):
    def run_script(self, assets):
        with tempfile.TemporaryDirectory(prefix="pipedash-updater-") as directory:
            root = Path(directory)
            signatures = root / "signatures"
            signatures.mkdir()
            for asset in assets:
                (signatures / f"{asset}.sig").write_text(f"signature of {asset}\n")
            (root / "notes.md").write_text("### Features\n- In-app updates")
            output = root / "latest.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--tag",
                    "v0.14.0",
                    "--repo",
                    "hcavarsan/pipedash",
                    "--signatures",
                    str(signatures),
                    "--notes-file",
                    str(root / "notes.md"),
                    "--output",
                    str(output),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            manifest = json.loads(output.read_text()) if output.exists() else None
            return result, manifest

    def test_full_release_maps_every_platform_to_its_signed_asset(self):
        result, manifest = self.run_script(ASSETS)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(manifest["version"], "0.14.0")
        self.assertEqual(manifest["notes"], "### Features\n- In-app updates")
        datetime.strptime(manifest["pub_date"], "%Y-%m-%dT%H:%M:%SZ")
        expected = {
            key: {
                "signature": f"signature of {asset}",
                "url": f"https://github.com/hcavarsan/pipedash/releases/download/v0.14.0/{asset}",
            }
            for asset, keys in ASSETS.items()
            for key in keys
        }
        self.assertEqual(len(expected), 18)
        self.assertEqual(manifest["platforms"], expected)

    def test_missing_signature_names_the_platforms_and_writes_nothing(self):
        assets = [asset for asset in ASSETS if not asset.endswith("_arm64-setup.exe")]
        result, manifest = self.run_script(assets)
        self.assertEqual(result.returncode, 1)
        self.assertIn("windows-aarch64, windows-aarch64-nsis", result.stderr)
        self.assertIsNone(manifest)

    def test_unknown_signature_is_rejected(self):
        result, manifest = self.run_script([*ASSETS, "pipedash_0.14.0_x64_en-US.msi"])
        self.assertEqual(result.returncode, 1)
        self.assertIn("pipedash_0.14.0_x64_en-US.msi.sig", result.stderr)
        self.assertIsNone(manifest)

    def test_two_assets_for_one_platform_are_rejected(self):
        result, manifest = self.run_script([*ASSETS, "pipedash_0.14.1_amd64.deb"])
        self.assertEqual(result.returncode, 1)
        self.assertIn("both provide linux-x86_64-deb", result.stderr)
        self.assertIsNone(manifest)


if __name__ == "__main__":
    unittest.main()
