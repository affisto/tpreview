#!/usr/bin/env python3
"""Installer integration tests: real shell, tar, checksums and filesystem; fixture downloaders."""
import hashlib
import importlib.util
import io
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("release", ROOT / "scripts/release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)
VERSION = release.version()


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="tpreview-install-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / "tools"
        self.bin.mkdir()
        self.fixtures = self.root / "fixtures"
        self.fixtures.mkdir()
        self.destination = self.root / "install with spaces"
        self.scratch = self.root / "scratch"
        self.scratch.mkdir()
        self.log = self.root / "downloads.log"
        self.env = os.environ.copy()
        for key in ["TPREVIEW_VERSION", "TPREVIEW_INSTALL_DIR", "TPREVIEW_DOWNLOADER"]:
            self.env.pop(key, None)
        self.env.update({"PATH": str(self.bin) + os.pathsep + self.env["PATH"],
                         "TMPDIR": str(self.scratch), "FIXTURES": str(self.fixtures),
                         "DOWNLOAD_LOG": str(self.log), "MOCK_OS": "Darwin", "MOCK_ARCH": "arm64"})
        self.tool("uname", '#!/bin/sh\ncase "$1" in -s) echo "$MOCK_OS";; -m) echo "$MOCK_ARCH";; esac\n')
        self.tool("ldd", '#!/bin/sh\necho "${MOCK_LIBC:-glibc}"\n')
        download = f'''#!{sys.executable}
import os, pathlib, shutil, sys
args=sys.argv[1:]
url=args[-1]
with open(os.environ["DOWNLOAD_LOG"],"a") as log: log.write(pathlib.Path(sys.argv[0]).name+" "+url+"\\n")
if "--output" in args: dest=args[args.index("--output")+1]
else: dest=next(a.split("=",1)[1] for a in args if a.startswith("--output-document="))
source=pathlib.Path(os.environ["FIXTURES"])/url.rsplit("/",1)[1]
if not source.is_file(): sys.exit(22)
shutil.copyfile(source,dest)
'''
        self.tool("curl", download)
        self.tool("wget", download)

    def tool(self, name, text):
        file = self.bin / name
        file.write_text(text)
        file.chmod(0o755)

    def bundle(self, target="aarch64-apple-darwin", version=VERSION, missing=False, binary_version=None):
        name = f"tpreview-{target}"
        path = self.fixtures / f"{name}.tar.gz"
        executable = f'#!/bin/sh\nprintf "%s\\n" "tpreview {binary_version or version}"\n'.encode()
        with tarfile.open(path, "w:gz") as tar:
            for filename, data, mode in [("VERSION", (version+"\n").encode(), 0o644),
                                         ("missing" if missing else "tpreview", executable, 0o755)]:
                entry = tarfile.TarInfo(f"{name}/{filename}")
                entry.size = len(data)
                entry.mode = mode
                tar.addfile(entry, io.BytesIO(data))
        release.write_checksum(path)
        return path

    def install(self, *args, success=True):
        result = subprocess.run(["/bin/sh", str(ROOT / "install.sh"), "--bin-dir", str(self.destination), *args],
                                env=self.env, capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode == 0, success, result.stdout + result.stderr)
        self.assertEqual(list(self.scratch.iterdir()), [], "temporary downloads were not cleaned up")
        self.assertEqual(list(self.destination.glob(".tpreview-install.*")), [], "staged executable leaked")
        return result

    def preserve_previous(self):
        self.destination.mkdir()
        old = self.destination / "tpreview"
        old.write_text("previous version stays intact")
        return old

    def test_native_platform_selection_with_curl_and_wget(self):
        for system, machine, target in [("Darwin", "arm64", "aarch64-apple-darwin"),
                                        ("Darwin", "x86_64", "x86_64-apple-darwin"),
                                        ("Linux", "aarch64", "aarch64-unknown-linux-gnu"),
                                        ("Linux", "x86_64", "x86_64-unknown-linux-gnu")]:
            for downloader in ["curl", "wget"]:
                with self.subTest(target=target, downloader=downloader):
                    self.env.update(MOCK_OS=system, MOCK_ARCH=machine)
                    archive = self.bundle(target)
                    self.install("--downloader", downloader)
                    output = subprocess.check_output([str(self.destination / "tpreview"), "--version"], text=True)
                    self.assertEqual(output.strip(), f"tpreview {VERSION}")
                    self.assertIn(f"{downloader} https://github.com/affisto/tpreview/releases/download/v{VERSION}/{archive.name}", self.log.read_text())

    def test_explicit_version_and_environment_configuration(self):
        self.bundle(version="0.1.0")
        self.env.update(TPREVIEW_VERSION="v0.1.0", TPREVIEW_DOWNLOADER="wget")
        self.install()
        self.assertIn("/download/v0.1.0/", self.log.read_text())

    def test_checksum_mismatch_keeps_existing_binary(self):
        old = self.preserve_previous()
        archive = self.bundle()
        archive.write_bytes(archive.read_bytes() + b"tampered")
        result = self.install(success=False)
        self.assertIn("SHA-256 mismatch", result.stderr)
        self.assertEqual(old.read_text(), "previous version stays intact")

    def test_missing_download_keeps_existing_binary(self):
        old = self.preserve_previous()
        result = self.install(success=False)
        self.assertIn("download failed", result.stderr)
        self.assertEqual(old.read_text(), "previous version stays intact")

    def test_missing_executable_and_wrong_version_keep_existing_binary(self):
        old = self.preserve_previous()
        self.bundle(missing=True)
        self.install(success=False)
        self.assertEqual(old.read_text(), "previous version stays intact")
        self.bundle(binary_version="99.0.0")
        result = self.install(success=False)
        self.assertIn("Unexpected executable version", result.stderr)
        self.assertEqual(old.read_text(), "previous version stays intact")

    def test_rejects_malformed_manifest(self):
        archive = self.bundle()
        Path(str(archive)+".sha256").write_text(f'{"0"*64}  ../../somewhere\n')
        result = self.install(success=False)
        self.assertIn("Invalid checksum manifest", result.stderr)
        self.assertFalse(self.destination.exists())

    def test_rejects_unsupported_platform_arch_and_musl(self):
        for system, arch, libc in [("Windows", "x86_64", "glibc"), ("Linux", "riscv64", "glibc"), ("Linux", "x86_64", "musl")]:
            with self.subTest(system=system, arch=arch, libc=libc):
                self.env.update(MOCK_OS=system, MOCK_ARCH=arch, MOCK_LIBC=libc)
                self.install(success=False)
                self.assertFalse(self.log.exists())

    def test_rejects_bad_arguments_before_downloading(self):
        for args in [("--version", "v0.1.1\n../../evil"), ("--version", "../../evil"), ("--downloader", "bad"), ("--unknown",), ("--version",)]:
            with self.subTest(args=args):
                self.install(*args, success=False)
                self.assertFalse(self.log.exists())

    def test_release_preparation_requires_all_matching_checked_bundles(self):
        for target in release.TARGETS:
            self.bundle(target)
        release.prepare(self.fixtures)
        sums = (self.fixtures / "SHA256SUMS").read_text().splitlines()
        self.assertEqual(len(sums), 5)
        for line in sums:
            digest, name = line.split()
            self.assertEqual(digest, hashlib.sha256((self.fixtures / name).read_bytes()).hexdigest())
        self.bundle(release.TARGETS[0], version="9.0.0")
        with self.assertRaisesRegex(AssertionError, "wrong bundle version"):
            release.prepare(self.fixtures)
        self.bundle(release.TARGETS[0])
        archive = self.fixtures / f"tpreview-{release.TARGETS[0]}.tar.gz"
        archive.write_bytes(b"broken")
        with self.assertRaisesRegex(AssertionError, "checksum mismatch"):
            release.prepare(self.fixtures)


if __name__ == "__main__":
    unittest.main(verbosity=2)
