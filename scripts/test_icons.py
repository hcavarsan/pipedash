import json
import os
import shutil
import struct
import subprocess
import tempfile
import unittest
from pathlib import Path


class IconGenerationTest(unittest.TestCase):
    def test_task_generates_desktop_bundle_icons(self):
        root = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory(prefix="pipedash-icons-") as directory:
            checkout = Path(directory) / "checkout"
            shutil.copytree(
                root,
                checkout,
                ignore=shutil.ignore_patterns(
                    ".git", "node_modules", "target", "__pycache__"
                ),
            )
            (checkout / "node_modules").symlink_to(
                root / "node_modules", target_is_directory=True
            )
            desktop = checkout / "crates" / "pipedash-desktop"
            shutil.rmtree(desktop / "icons")
            result = subprocess.run(
                ["mise", "run", "icons"],
                cwd=checkout,
                env={**os.environ, "MISE_TRUSTED_CONFIG_PATHS": str(checkout)},
                capture_output=True,
                text=True,
                check=False,
                timeout=180,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            config = json.loads((desktop / "tauri.conf.json").read_text())
            png_sizes = {"32x32.png": 32, "128x128.png": 128, "128x128@2x.png": 256}
            for name in config["bundle"]["icon"]:
                with self.subTest(icon=name):
                    data = (desktop / name).read_bytes()
                    if name.endswith(".png"):
                        self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
                        size = png_sizes[Path(name).name]
                        self.assertEqual(
                            struct.unpack(">II", data[16:24]), (size, size)
                        )
                    elif name.endswith(".icns"):
                        self.assertEqual(data[:4], b"icns")
                        self.assertEqual(struct.unpack(">I", data[4:8])[0], len(data))
                    elif name.endswith(".ico"):
                        self.assertEqual(data[:4], b"\x00\x00\x01\x00")
                        count = struct.unpack("<H", data[4:6])[0]
                        sizes = set()
                        for index in range(count):
                            entry = 6 + index * 16
                            width, height = data[entry : entry + 2]
                            sizes.add((width or 256, height or 256))
                            length, offset = struct.unpack(
                                "<II", data[entry + 8 : entry + 16]
                            )
                            self.assertGreater(length, 0)
                            self.assertLessEqual(offset + length, len(data))
                        self.assertIn((32, 32), sizes)
                        self.assertIn((256, 256), sizes)
                    else:
                        self.fail(f"Unsupported bundle icon format: {name}")


if __name__ == "__main__":
    unittest.main()
