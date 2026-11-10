import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

PLATFORMS = {
    "_universal.app.tar.gz": [
        "darwin-aarch64",
        "darwin-x86_64",
        "darwin-aarch64-app",
        "darwin-x86_64-app",
    ],
    "_amd64.AppImage": ["linux-x86_64", "linux-x86_64-appimage"],
    "_aarch64.AppImage": ["linux-aarch64", "linux-aarch64-appimage"],
    "_amd64.deb": ["linux-x86_64-deb"],
    "_arm64.deb": ["linux-aarch64-deb"],
    ".x86_64.rpm": ["linux-x86_64-rpm"],
    ".aarch64.rpm": ["linux-aarch64-rpm"],
    "_x64-setup.exe": ["windows-x86_64", "windows-x86_64-nsis"],
    "_x86-setup.exe": ["windows-i686", "windows-i686-nsis"],
    "_arm64-setup.exe": ["windows-aarch64", "windows-aarch64-nsis"],
}


def build(tag, repo, signatures, notes):
    platforms = {}
    sources = {}
    for path in sorted(Path(signatures).glob("*.sig")):
        asset = path.name.removesuffix(".sig")
        keys = next(
            (keys for suffix, keys in PLATFORMS.items() if asset.endswith(suffix)),
            None,
        )
        if keys is None:
            raise ValueError(f"{path.name} matches no known updater asset")
        entry = {
            "signature": path.read_text().strip(),
            "url": f"https://github.com/{repo}/releases/download/{tag}/{asset}",
        }
        for key in keys:
            if key in platforms:
                raise ValueError(f"{sources[key]} and {asset} both provide {key}")
            platforms[key] = entry
            sources[key] = asset
    missing = sorted(
        key for keys in PLATFORMS.values() for key in keys if key not in platforms
    )
    if missing:
        raise ValueError(f"no signature for {', '.join(missing)}")
    return {
        "version": tag.removeprefix("v"),
        "notes": notes,
        "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": platforms,
    }


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag", required=True)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--signatures", required=True)
    parser.add_argument("--notes-file", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args(argv)
    try:
        manifest = build(
            args.tag, args.repo, args.signatures, Path(args.notes_file).read_text()
        )
    except ValueError as error:
        print(f"updater manifest: {error}", file=sys.stderr)
        return 1
    Path(args.output).write_text(json.dumps(manifest, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
