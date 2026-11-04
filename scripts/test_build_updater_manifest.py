import base64
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from urllib.parse import quote

from build_updater_manifest import build_manifest, expected_assets, parse_version, read_public_key


ROOT = Path(__file__).resolve().parents[1]
TAURI_CLI = ROOT / "node_modules" / "@tauri-apps" / "cli" / "tauri.js"


class VersionTests(unittest.TestCase):
    def test_all_prerelease_suffixes(self):
        for tag in ("v1.2.3-alpha", "v1.2.3-beta.2", "v1.2.3-rc.1", "v1.2.3-preview", "v1.2.3-0"):
            with self.subTest(tag=tag):
                self.assertEqual(parse_version(tag), ((1, 2, 3), True))

    def test_stable_numeric_versions(self):
        self.assertEqual(parse_version("v1.10.0"), ((1, 10, 0), False))
        self.assertGreater(parse_version("v1.10.0")[0], parse_version("v1.9.99")[0])
        self.assertGreater(parse_version("v2.0.0")[0], parse_version("v1.99.99")[0])
        self.assertEqual(parse_version("v1.2.3+build.1"), ((1, 2, 3), False))

    def test_invalid_versions(self):
        for tag in ("v1.2", "v01.2.3", "v1.2.3-01", "v1.2.3-", "v1.2.3-rc..1", "v1.2.3\n", "v1.2.3/../bad"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                parse_version(tag)

    def test_missing_production_key_fails_with_recovery_instructions(self):
        with tempfile.TemporaryDirectory() as temporary:
            config = Path(temporary) / "tauri.conf.json"
            config.write_text('{}', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "default-branch updater-key recovery"):
                read_public_key(config)


@unittest.skipUnless(shutil.which("minisign") and shutil.which("node") and TAURI_CLI.is_file(),
                     "Real Minisign and the installed Tauri CLI are required for signing fixtures")
class ManifestFixtureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix="pipedash-manifest-fixtures-")
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.fixture = Path(cls.temporary.name)
        cls.private_key = cls.fixture / "updater.key"
        cls.public_key = cls.fixture / "updater.pub"
        cls.run_command(["minisign", "-G", "-s", str(cls.private_key), "-p", str(cls.public_key)],
                        input=b"fixture-only\nfixture-only\n")
        cls.template = cls.fixture / "assets"
        cls.template.mkdir()
        cls.version = "1.2.3-rc.1"
        cls.tag = f"v{cls.version}"
        for name in expected_assets(cls.version):
            artifact = cls.template / name
            artifact.write_bytes(f"real signed fixture: {name}\n".encode())
            cls.sign(artifact, cls.version)

    @classmethod
    def run_command(cls, arguments, **kwargs):
        subprocess.run(arguments, capture_output=True, check=True, timeout=120, **kwargs)

    @classmethod
    def sign(cls, artifact, version):
        environment = os.environ.copy()
        environment["TAURI_SIGNING_PRIVATE_KEY"] = base64.b64encode(cls.private_key.read_bytes()).decode("ascii")
        environment["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = "fixture-only"
        command = ["node", str(TAURI_CLI), "signer", "sign"]
        if version is not None:
            command.extend(["--app-version", version])
        command.append(str(artifact))
        cls.run_command(command, env=environment, cwd=ROOT)

    def setUp(self):
        self.temporary_case = tempfile.TemporaryDirectory(prefix="pipedash-manifest-case-")
        self.addCleanup(self.temporary_case.cleanup)
        self.directory = Path(self.temporary_case.name)
        self.assets = self.directory / "assets"
        shutil.copytree(self.template, self.assets)
        self.notes = self.directory / "notes.txt"
        self.notes.write_text("## Release notes\n\nActual release notes, not a placeholder.\n", encoding="utf-8")
        self.config = self.directory / "tauri.conf.json"
        self.configuration = {
            "plugins": {"updater": {
                "pubkey": base64.b64encode(self.public_key.read_bytes()).decode("ascii"),
                "requireSignedVersion": True,
                "allowDowngrades": False,
                "windows": {"installMode": "passive"},
                "endpoints": ["https://github.com/hcavarsan/pipedash/releases/latest/download/latest.json"],
            }}
        }
        self.save_config()

    def save_config(self):
        self.config.write_text(json.dumps(self.configuration), encoding="utf-8")

    def build(self, tag=None):
        return build_manifest(tag or self.tag, self.assets, self.notes, self.config)

    def test_complete_manifest_has_seven_targets_and_six_verified_payloads(self):
        manifest = self.build()
        self.assertEqual(manifest["version"], self.version)
        self.assertEqual(manifest["notes"], self.notes.read_text(encoding="utf-8"))
        self.assertNotIn("pub_date", manifest)
        self.assertEqual(len(manifest["platforms"]), 7)
        self.assertEqual(len({entry["url"] for entry in manifest["platforms"].values()}), 6)
        for name, targets in expected_assets(self.version).items():
            for target in targets:
                entry = manifest["platforms"][target]
                self.assertEqual(entry["signature"], (self.assets / f"{name}.sig").read_text().strip())
                self.assertEqual(entry["url"], f"https://github.com/hcavarsan/pipedash/releases/download/{quote(self.tag, safe='')}/{quote(name, safe='')}")
        self.assertEqual(manifest["platforms"]["darwin-x86_64-app"], manifest["platforms"]["darwin-aarch64-app"])

    def test_cli_writes_verified_manifest(self):
        output = self.directory / "latest.json"
        self.run_command([
            shutil.which("python3") or "python", str(ROOT / "scripts" / "build_updater_manifest.py"),
            "--tag", self.tag, "--assets-dir", str(self.assets), "--notes-file", str(self.notes),
            "--config", str(self.config), "--output", str(output),
        ])
        self.assertEqual(json.loads(output.read_text()), self.build())

    def test_missing_payload_or_signature(self):
        for path in sorted(self.assets.iterdir()):
            contents = path.read_bytes()
            path.unlink()
            try:
                with self.subTest(asset=path.name), self.assertRaisesRegex(ValueError, "Missing updater assets"):
                    self.build()
            finally:
                path.write_bytes(contents)

    def test_duplicate_payload_or_signature(self):
        nested = self.assets / "duplicate"
        nested.mkdir()
        name = next(iter(expected_assets(self.version)))
        for duplicate_name in (name, f"{name}.sig"):
            duplicate = nested / duplicate_name
            shutil.copyfile(self.assets / duplicate_name, duplicate)
            try:
                with self.subTest(asset=duplicate_name), self.assertRaisesRegex(ValueError, "Duplicate updater artifact"):
                    self.build()
            finally:
                duplicate.unlink()

    def test_wrong_version_artifact(self):
        name = next(iter(expected_assets(self.version)))
        (self.assets / name.replace(self.version, "1.2.2")).write_bytes(b"old release")
        with self.assertRaisesRegex(ValueError, "Unexpected updater artifact or wrong release version"):
            self.build()

    def test_announced_version_must_match_artifact_names(self):
        with self.assertRaisesRegex(ValueError, "Unexpected updater artifact or wrong release version"):
            self.build("v9.9.9")

    def test_valid_signature_with_wrong_bound_version_is_rejected(self):
        artifact = self.assets / next(iter(expected_assets(self.version)))
        self.sign(artifact, "1.2.2")
        with self.assertRaisesRegex(ValueError, "Trusted signed version"):
            self.build()

    def test_valid_signature_without_bound_version_is_rejected(self):
        artifact = self.assets / next(iter(expected_assets(self.version)))
        self.sign(artifact, None)
        with self.assertRaisesRegex(ValueError, "Trusted signed version"):
            self.build()

    def test_payload_tampering_is_rejected(self):
        artifact = self.assets / next(iter(expected_assets(self.version)))
        artifact.write_bytes(b"tampered")
        with self.assertRaisesRegex(ValueError, "Signature verification failed"):
            self.build()

    def test_wrong_signing_identity_is_rejected(self):
        other_private = self.directory / "other.key"
        other_public = self.directory / "other.pub"
        self.run_command(["minisign", "-G", "-W", "-s", str(other_private), "-p", str(other_public)])
        self.configuration["plugins"]["updater"]["pubkey"] = base64.b64encode(other_public.read_bytes()).decode("ascii")
        self.save_config()
        with self.assertRaisesRegex(ValueError, "Signature verification failed"):
            self.build()

    def test_tampered_trusted_comment_is_cryptographically_rejected(self):
        artifact = self.assets / next(iter(expected_assets(self.version)))
        self.sign(artifact, "1.2.2")
        signature = artifact.with_name(artifact.name + ".sig")
        decoded = base64.b64decode(signature.read_bytes()).replace(b"version:1.2.2", f"version:{self.version}".encode())
        signature.write_bytes(base64.b64encode(decoded))
        with self.assertRaisesRegex(ValueError, "Signature verification failed"):
            self.build()

    def test_malformed_signature_is_rejected(self):
        signature = self.assets / (next(iter(expected_assets(self.version))) + ".sig")
        for envelope in ("not base64", base64.b64encode(b"not a Minisign signature").decode()):
            signature.write_text(envelope)
            with self.subTest(envelope=envelope), self.assertRaisesRegex(ValueError, "Malformed"):
                self.build()

    def test_package_manager_downloads_are_not_native_targets(self):
        for name in ("pipedash.deb", "pipedash.rpm", "pipedash.dmg"):
            (self.assets / name).write_bytes(b"package-manager-owned")
        self.assertEqual(len(self.build()["platforms"]), 7)

    def test_release_requires_signed_versions_and_safe_configuration(self):
        updater = self.configuration["plugins"]["updater"]
        for field, unsafe in (("requireSignedVersion", False), ("allowDowngrades", True),
                              ("windows", {"installMode": "quiet"}), ("endpoints", ["http://example.com"])):
            original = updater[field]
            updater[field] = unsafe
            self.save_config()
            try:
                with self.subTest(field=field), self.assertRaisesRegex(ValueError, "Updater config must"):
                    self.build()
            finally:
                updater[field] = original
                self.save_config()

    def test_inner_public_key_is_not_a_complete_tauri_envelope(self):
        self.configuration["plugins"]["updater"]["pubkey"] = self.public_key.read_text().splitlines()[1]
        self.save_config()
        with self.assertRaises(ValueError):
            self.build()


if __name__ == "__main__":
    unittest.main()
