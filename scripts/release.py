#!/usr/bin/env python3
"""Build-time packaging and release validation (Python 3.11+; never needed at runtime)."""
import argparse
import hashlib
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TARGETS = (
    "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu",
    "aarch64-apple-darwin", "x86_64-apple-darwin",
)


def version():
    value = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    installer = (ROOT / "install.sh").read_text()
    assert f"TPREVIEW_DEFAULT_VERSION='v{value}'" in installer, "installer/Cargo version mismatch"
    if os.environ.get("GITHUB_REF_TYPE") == "tag":
        assert os.environ["GITHUB_REF_NAME"] == f"v{value}", "tag/Cargo version mismatch"
    return value


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_checksum(path):
    Path(str(path) + ".sha256").write_text(f"{checksum(path)}  {path.name}\n")


def package(target):
    release_version = version()
    arch = {"arm64": "aarch64", "aarch64": "aarch64", "AMD64": "x86_64", "x86_64": "x86_64"}[platform.machine()]
    system = {"Darwin": "apple-darwin", "Linux": "unknown-linux-gnu"}[platform.system()]
    assert target == f"{arch}-{system}", "bundle target must match the native build host"
    executable_version = subprocess.check_output([ROOT / "target/release/tpreview", "--version"], text=True).strip()
    assert executable_version == f"tpreview {release_version}", "stale executable; run cargo build --release"
    output = ROOT / "dist"
    output.mkdir(exist_ok=True)
    name = f"tpreview-{target}"
    with tempfile.TemporaryDirectory(prefix="tpreview-package-") as temp:
        directory = Path(temp) / name
        directory.mkdir()
        for source in ["target/release/tpreview", "LICENSE", "README.md", "THIRD_PARTY_NOTICES.md", "install.sh"]:
            shutil.copy2(ROOT / source, directory / Path(source).name)
        shutil.copy2(ROOT / "assets/OFL.txt", directory / "FONT-LICENSE.txt")
        shutil.copytree(ROOT / "docs", directory / "docs")
        (directory / "VERSION").write_text(release_version + "\n")
        archive = output / f"{name}.tar.gz"
        with tarfile.open(archive, "w:gz") as bundle:
            bundle.add(directory, arcname=name)
        write_checksum(archive)
        print(archive)


def prepare(directory):
    release_version = version()
    archives = []
    for target in TARGETS:
        name = f"tpreview-{target}"
        archive = directory / f"{name}.tar.gz"
        manifest = directory / f"{archive.name}.sha256"
        assert manifest.read_text() == f"{checksum(archive)}  {archive.name}\n", f"checksum mismatch: {name}"
        with tarfile.open(archive) as bundle:
            member = bundle.getmember(f"{name}/VERSION")
            assert member.isfile() and member.size < 128, f"invalid VERSION member: {name}"
            assert bundle.extractfile(member).read().decode().strip() == release_version, f"wrong bundle version: {name}"
            binary = bundle.getmember(f"{name}/tpreview")
            assert binary.isfile() and binary.size > 0 and binary.mode & 0o111, f"invalid executable: {name}"
        archives.append(archive)
    shutil.copy2(ROOT / "install.sh", directory / "install.sh")
    write_checksum(directory / "install.sh")
    (directory / "SHA256SUMS").write_text("".join(f"{checksum(p)}  {p.name}\n" for p in [*archives, directory / "install.sh"]))
    print(f"Verified {len(archives)} bundles for v{release_version}; installer and checksums ready")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("version")
    commands.add_parser("package").add_argument("target", choices=TARGETS)
    commands.add_parser("prepare").add_argument("directory", type=Path)
    args = parser.parse_args()
    if args.command == "package":
        package(args.target)
    elif args.command == "prepare":
        prepare(args.directory)
    else:
        print(version())
