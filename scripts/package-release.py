#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only

"""Package the versioned binary, license, notices and README for a release target."""

import argparse
import hashlib
from pathlib import Path
import shutil
import tarfile
import tempfile
import tomllib
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--tag", required=True)
    args = parser.parse_args()

    root = Path(__file__).resolve().parents[1]
    manifest = tomllib.loads((root / "crates" / "muse" / "Cargo.toml").read_text(encoding="utf-8"))
    expected_tag = f"v{manifest['package']['version']}"
    if args.tag != expected_tag:
        parser.error(f"tag {args.tag!r} does not match Cargo version {expected_tag!r}")
    if not args.target or any(c not in "abcdefghijklmnopqrstuvwxyz0123456789-_" for c in args.target):
        parser.error("target must be a Rust target triple")

    windows = "windows" in args.target.split("-")
    binary_name = "muse.exe" if windows else "muse"
    binary = root / "target" / args.target / "release" / binary_name
    if not binary.is_file():
        parser.error(f"release binary not found: {binary}")

    dist = root / "dist"
    dist.mkdir(exist_ok=True)
    bundle_name = f"muse-{args.tag}-{args.target}"
    extension = ".zip" if windows else ".tar.gz"
    archive = dist / f"{bundle_name}{extension}"

    with tempfile.TemporaryDirectory(prefix="muse-release-") as temporary:
        bundle = Path(temporary) / bundle_name
        bundle.mkdir()
        for source in [binary, root / "LICENSE", root / "NOTICE", root / "README.md"]:
            shutil.copy2(source, bundle / source.name)
        shutil.copytree(root / "licenses", bundle / "licenses")

        if windows:
            with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as output:
                for path in sorted(bundle.rglob("*")):
                    if path.is_file():
                        output.write(path, f"{bundle_name}/{path.relative_to(bundle).as_posix()}")
        else:
            with tarfile.open(archive, "w:gz") as output:
                output.add(bundle, arcname=bundle_name)

    with archive.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    checksum = archive.with_name(archive.name + ".sha256")
    checksum.write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    print(archive)
    print(checksum)


if __name__ == "__main__":
    main()
