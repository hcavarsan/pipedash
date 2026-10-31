import argparse
import base64
import binascii
import json
from pathlib import Path
import re
import subprocess
import tempfile
from urllib.parse import quote


SEMVER = re.compile(
    r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-((?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
    r"(?:\.(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
)


def parse_version(tag):
    match = SEMVER.fullmatch(tag.removeprefix("v"))
    if not match:
        raise ValueError("Expected a valid SemVer tag such as v1.2.3 or v1.2.3-rc.1")
    return tuple(int(part) for part in match.group(1, 2, 3)), match.group(4) is not None


def decode_envelope(value, label):
    try:
        return base64.b64decode(value.strip(), validate=True).decode("utf-8")
    except (ValueError, binascii.Error, UnicodeDecodeError) as error:
        raise ValueError(f"Malformed {label} envelope") from error


def read_public_key(config):
    configuration = json.loads(Path(config).read_text(encoding="utf-8"))
    key = configuration.get("plugins", {}).get("updater", {}).get("pubkey")
    if not isinstance(key, str) or not key.strip():
        raise ValueError(
            "Missing production updater public key in plugins.updater.pubkey. "
            "Run the trusted default-branch updater-key recovery workflow, then commit "
            "its complete public envelope; do not generate a replacement key."
        )
    public_key = decode_envelope(key, "updater public key")
    lines = public_key.splitlines()
    if len(lines) != 2 or not lines[0].startswith("untrusted comment: "):
        raise ValueError("Updater public key must contain the complete Minisign public envelope")
    updater = configuration["plugins"]["updater"]
    if (updater.get("requireSignedVersion") is not True
            or updater.get("allowDowngrades") is not False
            or updater.get("windows", {}).get("installMode") != "passive"
            or updater.get("endpoints") != [
                "https://github.com/hcavarsan/pipedash/releases/latest/download/latest.json"
            ]):
        raise ValueError(
            "Updater config must require signed versions, forbid downgrades, "
            "use passive Windows installation and the stable HTTPS endpoint"
        )
    try:
        raw_key = base64.b64decode(lines[1], validate=True)
    except (ValueError, binascii.Error) as error:
        raise ValueError("Malformed Minisign public key") from error
    if len(raw_key) != 42 or raw_key[:2] != b"Ed":
        raise ValueError("Malformed Minisign public key")
    return public_key


def expected_assets(version):
    return {
        f"pipedash_{version}_amd64.AppImage": ("linux-x86_64-appimage",),
        f"pipedash_{version}_aarch64.AppImage": ("linux-aarch64-appimage",),
        f"pipedash_{version}_universal.app.tar.gz": (
            "darwin-x86_64-app", "darwin-aarch64-app"
        ),
        f"pipedash_{version}_x86-setup.exe": ("windows-i686-nsis",),
        f"pipedash_{version}_x64-setup.exe": ("windows-x86_64-nsis",),
        f"pipedash_{version}_arm64-setup.exe": ("windows-aarch64-nsis",),
    }


def verify_signature(artifact, envelope, public_key, version):
    signature = decode_envelope(envelope, f"signature for {artifact.name}")
    lines = signature.splitlines()
    if len(lines) != 4 or not lines[2].startswith("trusted comment: "):
        raise ValueError(f"Malformed Minisign signature for {artifact.name}")
    with tempfile.TemporaryDirectory(prefix="pipedash-signature-") as temporary:
        key_path = Path(temporary) / "updater.pub"
        signature_path = Path(temporary) / "artifact.minisig"
        key_path.write_text(public_key, encoding="utf-8")
        signature_path.write_text(signature, encoding="utf-8")
        result = subprocess.run(
            ["minisign", "-V", "-q", "-p", str(key_path), "-m", str(artifact),
             "-x", str(signature_path)],
            capture_output=True,
            timeout=120,
            check=False,
        )
        if result.returncode:
            raise ValueError(f"Signature verification failed for {artifact.name}")
    fields = lines[2].removeprefix("trusted comment: ").split("\t")
    versions = [field.removeprefix("version:") for field in fields if field.startswith("version:")]
    if versions != [version]:
        raise ValueError(f"Trusted signed version does not equal {version} for {artifact.name}")


def build_manifest(tag, assets_dir, notes_file, config):
    if not tag.startswith("v"):
        raise ValueError("Release tag must start with v")
    parse_version(tag)
    version = tag[1:]
    public_key = read_public_key(config)
    expected = expected_assets(version)
    required = set(expected) | {f"{name}.sig" for name in expected}
    files = {}
    for path in Path(assets_dir).rglob("*"):
        name = path.name
        artifact_name = name.removesuffix(".sig")
        is_updater_asset = artifact_name.endswith((".AppImage", ".app.tar.gz", "-setup.exe"))
        if name not in required and not is_updater_asset:
            continue
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"Updater asset is not a regular file: {name}")
        if name not in required:
            raise ValueError(f"Unexpected updater artifact or wrong release version: {name}")
        if name in files:
            raise ValueError(f"Duplicate updater artifact: {name}")
        files[name] = path
    missing = required - files.keys()
    if missing:
        raise ValueError(f"Missing updater assets: {', '.join(sorted(missing))}")
    platforms = {}
    for name, targets in expected.items():
        envelope = files[f"{name}.sig"].read_text(encoding="utf-8").strip()
        verify_signature(files[name], envelope, public_key, version)
        entry = {
            "signature": envelope,
            "url": f"https://github.com/hcavarsan/pipedash/releases/download/{quote(tag, safe='')}/{quote(name, safe='')}",
        }
        for target in targets:
            platforms[target] = entry.copy()
    return {
        "version": version,
        "notes": Path(notes_file).read_text(encoding="utf-8"),
        "platforms": platforms,
    }


def main():
    parser = argparse.ArgumentParser()
    for name in ("tag", "assets-dir", "notes-file", "config", "output"):
        parser.add_argument(f"--{name}", required=True)
    args = parser.parse_args()
    try:
        manifest = build_manifest(args.tag, args.assets_dir, args.notes_file, args.config)
        Path(args.output).write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Updater manifest error: {error}\n")


if __name__ == "__main__":
    main()
