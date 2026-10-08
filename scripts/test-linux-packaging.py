#!/usr/bin/env python3
"""Native packaging/installer regressions; uses a tiny C fixture, not the GUI app.

Run: python3 scripts/test-linux-packaging.py
Needs Linux, Bash, cc, binutils, coreutils and git. No network or package install.
Optional gio and desktop-file-validate additionally exercise real desktop parsing.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import struct
import subprocess
import tarfile
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parent.parent
HOST = platform.machine()
TARGETS = {"x86_64": ("x86_64-unknown-linux-gnu", "x64", 62), "aarch64": ("aarch64-unknown-linux-gnu", "arm64", 183)}


class PackagingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if platform.system() != "Linux" or HOST not in TARGETS:
            raise unittest.SkipTest("Native x86_64 or aarch64 Linux required")
        for command in ("cc", "readelf", "sha256sum", "bash"):
            if not shutil.which(command):
                raise unittest.SkipTest(f"Missing {command}")
        cls.target, cls.arch, cls.machine = TARGETS[HOST]
        cls.workspace = tempfile.TemporaryDirectory(prefix="nebulabook-packaging-tests-")
        cls.base = Path(cls.workspace.name)
        source = cls.base / "fixture.c"
        source.write_text('#include <stdio.h>\n#include <stdlib.h>\nint main(int argc, char **argv) { (void)argc; const char *path = getenv("NEBULABOOK_PACKAGE_MARKER"); if (path) { FILE *f = fopen(path, "w"); if (!f) return 3; char *resolved = realpath(argv[0], NULL); fputs(resolved ? resolved : argv[0], f); free(resolved); fclose(f); } return 0; }\n')
        cls.binary = cls.base / "fixture"
        subprocess.run(["cc", "-o", str(cls.binary), str(source)], check=True)
        # Only synthetic tests use this compiler metadata fixture; no Rust build is claimed.
        rustc = cls.base / "fixture-rustc"
        rustc.write_text(f"#!/bin/sh\nprintf '%s\\n' 'rustc packaging-test-fixture' 'host: {cls.target}'\n")
        rustc.chmod(0o755)
        cls.env = {**os.environ, "RUSTC": str(rustc), "GITHUB_SHA": "1" * 40, "SOURCE_DATE_EPOCH": "1700000000"}
        cls.smoke = cls.base / "synthetic-smoke-fixtures"
        cls.smoke.mkdir()
        checks = ["renderer-initialized", "cjk-glyphs-present", "process-alive", "new-edit-save", "normal-close", "reopen-preserves-data", "save-conflict-blocks-close", "external-data-not-overwritten"]
        for backend in ("x11", "wayland"):
            (cls.smoke / f"{backend}-result.json").write_text(json.dumps({"backend": backend, "binary": str(cls.binary), "binary_sha256": hashlib.sha256(cls.binary.read_bytes()).hexdigest(), "checks": checks, "rendering": "synthetic packaging-test evidence, not a GUI run"}))
        cls.output = cls.base / "release output"
        cls.invoke_package("--output-dir", str(cls.output))
        cls.info = json.loads((cls.output / f"build-info-linux-{cls.arch}.json").read_text())
        cls.archive = cls.output / cls.info["file"]
        cls.unpacked = cls.base / "unpacked"
        cls.unpacked.mkdir()
        # Generated archive has been independently checked before extraction.
        with tarfile.open(cls.archive) as archive:
            for entry in archive.getmembers():
                assert not entry.issym() and not entry.islnk()
                assert not entry.name.startswith("/") and ".." not in Path(entry.name).parts
            if hasattr(tarfile, "data_filter"):
                archive.extractall(cls.unpacked, filter="data")
            else:  # Ubuntu 22.04 Python 3.10 predates extraction filters.
                archive.extractall(cls.unpacked)
        cls.package = cls.unpacked / cls.archive.name.removesuffix(".tar.gz")

    @classmethod
    def tearDownClass(cls):
        cls.workspace.cleanup()

    @classmethod
    def invoke_package(cls, *arguments, success=True, target=None, binary=None, smoke=True, smoke_output=None):
        command = ["bash", str(ROOT / "scripts/package-linux.sh"), "--target", target or cls.target, "--binary", str(binary or cls.binary), *arguments]
        if smoke:
            command.extend(["--smoke-output", str(smoke_output or cls.smoke)])
        result = subprocess.run(command, env=cls.env, capture_output=True, text=True)
        if success and result.returncode:
            raise AssertionError(result.stdout + result.stderr)
        if not success and not result.returncode:
            raise AssertionError("Unexpected package success: " + result.stdout)
        return result

    def make_home(self, special=False):
        temporary = tempfile.TemporaryDirectory(dir=self.base)
        self.addCleanup(temporary.cleanup)
        home = Path(temporary.name) / ('home spaces 中文 =value "quote" $dollar `tick` \\slash %percent' if special else "home")
        home.mkdir()
        return home

    def install(self, home, *arguments, success=True, package=None, **extra_env):
        if os.geteuid() == 0:
            self.skipTest("Installer deliberately rejects root")
        env = {**os.environ, "HOME": str(home), "XDG_DATA_HOME": str(home / ".local/share"), **extra_env}
        result = subprocess.run(["bash", str((package or self.package) / "install-linux.sh"), *arguments], env=env, capture_output=True, text=True)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def test_archive_checksums_metadata_and_contents(self):
        info = self.info
        self.assertEqual(info["file"], f"nebulabook-v4.1.0-linux-{self.arch}.tar.gz")
        self.assertEqual(info["binary"]["file"], "nebulabook")
        self.assertEqual(info["commit"], "1" * 40)
        self.assertEqual(info["elf_machine"], self.machine)
        self.assertEqual(info["host_arch"], HOST)
        self.assertEqual(info["target"], self.target)
        self.assertTrue(info["release_eligible"])
        self.assertTrue(info["native_execution_verified"])
        self.assertEqual(set(info["smoke_evidence"]), {"x11", "wayland"})
        self.assertEqual(info["bytes"], self.archive.stat().st_size)
        self.assertEqual(info["sha256"], hashlib.sha256(self.archive.read_bytes()).hexdigest())
        self.assertEqual(info["binary"]["sha256"], hashlib.sha256(self.binary.read_bytes()).hexdigest())
        self.assertEqual(set(p.name for p in self.package.iterdir()), {"nebulabook", "README.md", "LICENSE", "nebulabook.desktop", "install-linux.sh", "SHA256SUMS"})
        subprocess.run(["sha256sum", "-c", f"SHA256SUMS-linux-{self.arch}.txt"], cwd=self.output, check=True, capture_output=True)
        subprocess.run(["sha256sum", "-c", "SHA256SUMS"], cwd=self.package, check=True, capture_output=True)
        self.assertEqual((self.package / "nebulabook").stat().st_mode & 0o777, 0o755)
        self.assertEqual((self.package / "install-linux.sh").stat().st_mode & 0o777, 0o755)
        self.assertIn("\nName=Nebulabook\n", (self.package / "nebulabook.desktop").read_text())
        self.assertIn("\nExec=nebulabook\n", (self.package / "nebulabook.desktop").read_text())

    def test_deterministic_archive(self):
        output = self.base / "second output"
        self.invoke_package("--output-dir", str(output))
        self.assertEqual(self.archive.read_bytes(), (output / self.archive.name).read_bytes())

    def test_refuse_cross_arch_host(self):
        other = next(value[0] for host, value in TARGETS.items() if host != HOST)
        result = self.invoke_package(target=other, success=False)
        self.assertIn("Native host required", result.stderr)

    def test_refuse_wrong_elf_machine(self):
        binary = self.base / "wrong-machine"
        content = bytearray(self.binary.read_bytes())
        struct.pack_into("<H", content, 18, 183 if self.machine == 62 else 62)
        binary.write_bytes(content)
        binary.chmod(0o755)
        result = self.invoke_package(binary=binary, success=False)
        self.assertIn("Wrong ELF type/machine", result.stderr)

    def test_refuse_non_elf(self):
        binary = self.base / "not-elf"
        binary.write_text("#!/bin/sh\nexit 0\n")
        binary.chmod(0o755)
        self.assertIn("Expected a 64-bit", self.invoke_package(binary=binary, success=False).stderr)

    def test_glibc_floor_enforced(self):
        result = self.invoke_package("--glibc-baseline", "2.1", success=False)
        self.assertIn("newer than baseline", result.stderr)

    def test_newer_baseline_cannot_be_release_labelled(self):
        result = self.invoke_package("--glibc-baseline", "2.99", success=False)
        self.assertIn("requires --local-only", result.stderr)
        output = self.base / "local-only"
        self.invoke_package("--glibc-baseline", "2.99", "--local-only", "--output-dir", str(output), smoke=False)
        info = json.loads((output / f"build-info-linux-{self.arch}-local-only.json").read_text())
        self.assertFalse(info["release_eligible"])
        self.assertFalse(info["native_execution_verified"])
        self.assertTrue(info["file"].endswith("-local-only.tar.gz"))
        self.assertTrue(all("-local-only" in path.name for path in output.iterdir()))

    def test_release_requires_smoke_evidence(self):
        result = self.invoke_package(success=False, smoke=False)
        self.assertIn("requires --smoke-output", result.stderr)

    def test_smoke_requires_both_backends(self):
        with tempfile.TemporaryDirectory(dir=self.base) as temporary:
            smoke = Path(temporary)
            shutil.copyfile(self.smoke / "x11-result.json", smoke / "x11-result.json")
            result = self.invoke_package(success=False, smoke_output=smoke)
            self.assertIn("wayland-result.json", result.stderr)

    def test_smoke_evidence_must_match_binary_and_checks(self):
        for field, value, error in (("binary_sha256", "0" * 64, "SHA256 mismatch"), ("binary", str(self.base / "other-binary"), "different binary path"), ("checks", ["renderer-initialized"], "missing required checks"), ("backend", "wayland", "Invalid x11")):
            with self.subTest(field=field):
                with tempfile.TemporaryDirectory(dir=self.base) as temporary:
                    smoke = Path(temporary)
                    for path in self.smoke.iterdir():
                        shutil.copyfile(path, smoke / path.name)
                    evidence_path = smoke / "x11-result.json"
                    evidence = json.loads(evidence_path.read_text())
                    evidence[field] = value
                    evidence_path.write_text(json.dumps(evidence))
                    result = self.invoke_package(success=False, smoke_output=smoke)
                    self.assertIn(error, result.stderr)

    def test_install_spaces_special_characters_and_upgrade(self):
        home = self.make_home(special=True)
        # The new program name must not rename, migrate or remove native data.
        note = home / ".local/share/nebulanotepad/notebook.json"
        note.parent.mkdir(parents=True)
        note.write_text("existing notes must not change")
        note.parent.chmod(0o700)
        note.chmod(0o600)
        note_stat = note.stat()
        data_dir_stat = note.parent.stat()
        legacy_files = {
            home / ".local/lib/nebula-notes/nebula-notes": "existing old executable",
            home / ".local/share/applications/nebula-notes.desktop": "existing old desktop entry",
        }
        for path, content in legacy_files.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
        old_launcher = home / ".local/bin/nebula-notes"
        old_launcher.parent.mkdir(parents=True)
        old_launcher.symlink_to(home / ".local/lib/nebula-notes/nebula-notes")
        self.install(home)
        self.install(home)
        self.assertEqual(note.read_text(), "existing notes must not change")
        self.assertEqual(list(note.parent.iterdir()), [note])
        self.assertEqual((note.stat().st_mode, note.stat().st_mtime_ns), (note_stat.st_mode, note_stat.st_mtime_ns))
        self.assertEqual((note.parent.stat().st_mode, note.parent.stat().st_mtime_ns), (data_dir_stat.st_mode, data_dir_stat.st_mtime_ns))
        self.assertFalse((home / ".local/share/nebulabook").exists())
        for path, content in legacy_files.items():
            self.assertEqual(path.read_text(), content)
        self.assertTrue(old_launcher.is_symlink())
        self.assertEqual(old_launcher.readlink(), home / ".local/lib/nebula-notes/nebula-notes")
        binary = home / ".local/lib/nebulabook/nebulabook"
        launcher = home / ".local/bin/nebulabook"
        desktop = home / ".local/share/applications/nebulabook.desktop"
        self.assertEqual(binary.read_bytes(), self.binary.read_bytes())
        self.assertEqual(launcher.resolve(), binary)
        self.assertIn('Exec=/usr/bin/env -C "', desktop.read_text())
        if shutil.which("desktop-file-validate"):
            subprocess.run(["desktop-file-validate", str(desktop)], check=True, capture_output=True)
        if shutil.which("gio"):
            marker = home / "launched"
            result = subprocess.run(["gio", "launch", str(desktop)], env={**os.environ, "NEBULABOOK_PACKAGE_MARKER": str(marker)}, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            for _ in range(100):
                if marker.exists():
                    break
                time.sleep(0.02)
            self.assertEqual(marker.read_text(), str(binary))

    def test_install_custom_prefix(self):
        home = self.make_home()
        prefix = home / "custom install"
        self.install(home, "--prefix", str(prefix))
        self.assertTrue((prefix / "lib/nebulabook/nebulabook").is_file())
        self.assertTrue((home / ".local/share/applications/nebulabook.desktop").is_file())

    def test_install_rejects_external_and_symlink_destinations(self):
        home = self.make_home()
        outside = self.base / "outside-user-home"
        outside.mkdir(exist_ok=True)
        result = self.install(home, "--prefix", str(outside), success=False)
        self.assertIn("inside HOME", result.stderr)
        (home / "redirect").symlink_to(outside, target_is_directory=True)
        self.install(home, "--prefix", str(home / "redirect"), success=False)
        self.install(home, success=False, XDG_DATA_HOME=str(outside))
        self.assertEqual(list(outside.iterdir()), [])

    def test_install_rejects_control_character_paths(self):
        home = self.make_home()
        result = self.install(home, "--prefix", str(home / "bad\npath"), success=False)
        self.assertIn("Control characters", result.stderr)

    def test_install_rejects_other_architecture(self):
        home = self.make_home()
        package = self.base / "other-architecture-package"
        shutil.copytree(self.package, package)
        binary = package / "nebulabook"
        content = bytearray(binary.read_bytes())
        struct.pack_into("<H", content, 18, 183 if self.machine == 62 else 62)
        binary.write_bytes(content)
        sums = package / "SHA256SUMS"
        sums.write_text("".join(f"{hashlib.sha256((package / name).read_bytes()).hexdigest()}  {name}\n" for name in [line.split("  ", 1)[1] for line in sums.read_text().splitlines()]))
        result = self.install(home, package=package, success=False)
        self.assertIn("architecture does not match", result.stderr)
        self.assertEqual(list(home.iterdir()), [])

    def test_install_rejects_tampered_package(self):
        home = self.make_home()
        package = self.base / "tampered-package"
        shutil.copytree(self.package, package)
        with (package / "nebulabook").open("ab") as binary:
            binary.write(b"tampered")
        result = self.install(home, package=package, success=False)
        self.assertIn("checksum verification failed", result.stderr)
        self.assertEqual(list(home.iterdir()), [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
