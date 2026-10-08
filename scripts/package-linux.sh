#!/usr/bin/env bash
# Package an already built native Linux ELF; execution claims require matching smoke evidence.
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
exec python3 - "$repo_root" "$@" <<'PY'
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(sys.argv[1])
TARGETS = {
    "x86_64-unknown-linux-gnu": ("x64", "x86_64", 62),
    "aarch64-unknown-linux-gnu": ("arm64", "aarch64", 183),
}


def fail(message):
    raise ValueError(message)


def run(*command):
    return subprocess.check_output(command, cwd=ROOT, text=True, env={**os.environ, "LC_ALL": "C"}).strip()


def digest(path):
    with path.open("rb") as stream:
        value = hashlib.sha256()
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def version_tuple(value):
    if not re.fullmatch(r"[0-9]+\.[0-9]+(?:\.[0-9]+)?", value):
        fail("Invalid glibc version: " + value)
    parts = tuple(map(int, value.split(".")))
    return parts + (0,) * (3 - len(parts))


def main():
    parser = argparse.ArgumentParser(prog="scripts/package-linux.sh", description="Package a native Linux build; release ABI ceiling is glibc 2.35.")
    parser.add_argument("--target", required=True, choices=TARGETS)
    parser.add_argument("--binary", type=Path, help="Already built executable (default: $CARGO_TARGET_DIR/<target>/release/nebulabook)")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "release-assets")
    parser.add_argument("--glibc-baseline", default="2.35", help="Maximum allowed required GLIBC symbol version")
    parser.add_argument("--smoke-output", type=Path, help="Directory containing matching x11-result.json and wayland-result.json (required for release)")
    parser.add_argument("--local-only", action="store_true", help="Non-release package with explicit local-only filenames; permits a newer baseline")
    args = parser.parse_args(sys.argv[2:])
    if not args.local_only and args.smoke_output is None:
        fail("Release packaging requires --smoke-output with native X11 and Wayland evidence; use --local-only for unverified builds")
    baseline = version_tuple(args.glibc_baseline)
    if baseline > version_tuple("2.35") and not args.local_only:
        fail("A baseline above 2.35 requires --local-only; refusing a release-labelled artifact")
    arch, native_arch, machine = TARGETS[args.target]
    host_arch = platform.machine().lower()
    if platform.system() != "Linux" or host_arch != native_arch:
        fail(f"Native host required: target {args.target} needs Linux/{native_arch}, got {platform.system()}/{host_arch}")
    binary = (args.binary or Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / args.target / "release/nebulabook").resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        fail(f"Missing or non-executable binary: {binary}")
    binary_sha256 = digest(binary)
    with binary.open("rb") as stream:
        header = stream.read(64)
    if len(header) < 64 or header[:7] != b"\x7fELF\x02\x01\x01":
        fail("Expected a 64-bit little-endian ELF executable")
    elf_type, elf_machine = struct.unpack_from("<HH", header, 16)
    if elf_type not in (2, 3) or elf_machine != machine:
        fail(f"Wrong ELF type/machine: {elf_type}/{elf_machine}; expected executable/{machine}")
    dynamic = run("readelf", "--dynamic", "--wide", str(binary))
    needed = re.findall(r"\(NEEDED\).*?Shared library: \[([^\]]+)\]", dynamic)
    version_info = run("readelf", "--version-info", "--wide", str(binary))
    requirements = set(re.findall(r"Name:\s+(GLIBC_[^\s]+)", version_info))
    if "libc.so.6" not in needed or not requirements:
        fail("Expected a dynamically linked GNU libc executable with inspectable GLIBC requirements")
    if any(not re.fullmatch(r"GLIBC_[0-9]+\.[0-9]+(?:\.[0-9]+)?", name) for name in requirements):
        fail("Unrecognized GLIBC ABI requirement; cannot certify compatibility: " + ", ".join(sorted(requirements)))
    glibc_versions = sorted((name[6:] for name in requirements), key=version_tuple)
    glibc_minimum = glibc_versions[-1]
    if version_tuple(glibc_minimum) > baseline:
        fail(f"ELF requires GLIBC_{glibc_minimum}, newer than baseline {args.glibc_baseline}; rebuild on Ubuntu 22.04 for release")
    program_headers = run("readelf", "--program-headers", "--wide", str(binary))
    interpreter_match = re.search(r"Requesting program interpreter: ([^\]]+)\]", program_headers)
    if not interpreter_match:
        fail("ELF program interpreter is missing")
    interpreter = interpreter_match.group(1)
    expected_interpreter = {"x64": "/lib64/ld-linux-x86-64.so.2", "arm64": "/lib/ld-linux-aarch64.so.1"}[arch]
    if interpreter != expected_interpreter:
        fail(f"Unexpected ELF interpreter: {interpreter}")
    smoke_evidence = {}
    if args.smoke_output is not None:
        base_checks = {"renderer-initialized", "cjk-glyphs-present", "process-alive"}
        x11_checks = {"new-edit-save", "normal-close", "reopen-preserves-data", "save-conflict-blocks-close", "external-data-not-overwritten"}
        for backend in ("x11", "wayland"):
            evidence_path = args.smoke_output / f"{backend}-result.json"
            evidence = json.loads(evidence_path.read_text(encoding="utf-8"))
            if not isinstance(evidence, dict) or evidence.get("backend") != backend:
                fail(f"Invalid {backend} smoke evidence")
            evidence_binary = evidence.get("binary")
            if not isinstance(evidence_binary, str) or not Path(evidence_binary).is_absolute() or Path(evidence_binary).resolve() != binary:
                fail(f"{backend} smoke evidence refers to a different binary path")
            if evidence.get("binary_sha256") != binary_sha256:
                fail(f"{backend} smoke binary SHA256 mismatch; rerun smoke checks for this binary")
            checks = evidence.get("checks")
            required = base_checks | (x11_checks if backend == "x11" else set())
            if not isinstance(checks, list) or not all(isinstance(check, str) for check in checks) or not required.issubset(checks):
                fail(f"{backend} smoke evidence is missing required checks")
            smoke_evidence[backend] = {
                "file": evidence_path.name, "sha256": digest(evidence_path),
                "binary_sha256": binary_sha256, "checks": checks,
                "rendering": evidence.get("rendering"), "not_covered": evidence.get("not_covered", []),
            }
    cargo = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    package = re.search(r"(?ms)^\[package\]\s*\n(.*?)(?=^\[|\Z)", cargo)
    match = re.search(r'^version\s*=\s*"([^"\n]+)"', package.group(1), re.M) if package else None
    version = match.group(1) if match else ""
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?", version):
        fail("Invalid package version in Cargo.toml")
    commit = os.environ.get("GITHUB_SHA") or run("git", "rev-parse", "HEAD")
    if not re.fullmatch(r"[0-9a-fA-F]{40}(?:[0-9a-fA-F]{24})?", commit):
        fail("GITHUB_SHA/git HEAD must be a full commit hash")
    rust = run(os.environ.get("RUSTC", "rustc"), "--version", "--verbose")
    rust_host = re.search(r"^host: (.+)$", rust, re.M)
    if not rust_host or rust_host.group(1) != args.target:
        fail("Rust compiler host must match the native GNU Linux target")
    epoch = int(os.environ.get("SOURCE_DATE_EPOCH") or run("git", "show", "-s", "--format=%ct", "HEAD"))
    if not 0 <= epoch <= 0xFFFFFFFF:
        fail("SOURCE_DATE_EPOCH must fit an unsigned 32-bit timestamp")
    suffix = "-local-only" if args.local_only else ""
    package_name = f"nebulabook-v{version}-linux-{arch}{suffix}"
    archive_name = package_name + ".tar.gz"
    info_name = f"build-info-linux-{arch}{suffix}.json"
    sums_name = f"SHA256SUMS-linux-{arch}{suffix}.txt"
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".package-linux-", dir=output) as temporary:
        temporary = Path(temporary)
        stage = temporary / package_name
        stage.mkdir()
        contents = {
            "nebulabook": (binary, 0o755),
            "README.md": (ROOT / "README.md", 0o644),
            "LICENSE": (ROOT / "LICENSE", 0o644),
            "nebulabook.desktop": (ROOT / "packaging/nebulabook.desktop", 0o644),
            "install-linux.sh": (ROOT / "scripts/install-linux.sh", 0o755),
        }
        for name, (source, mode) in contents.items():
            shutil.copyfile(source, stage / name)
            (stage / name).chmod(mode)
        if digest(stage / "nebulabook") != binary_sha256:
            fail("Binary changed during packaging; rerun validation")
        # The installer verifies all payload files without needing Python.
        (stage / "SHA256SUMS").write_text("".join(f"{digest(stage / name)}  {name}\n" for name in sorted(contents)), encoding="ascii")
        (stage / "SHA256SUMS").chmod(0o644)
        archive = temporary / archive_name
        with archive.open("wb") as raw, gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=epoch) as compressed, tarfile.open(fileobj=compressed, mode="w", format=tarfile.GNU_FORMAT) as bundle:
            for entry in [stage, *sorted(stage.iterdir())]:
                info = bundle.gettarinfo(str(entry), arcname=str(Path(package_name) / entry.relative_to(stage)))
                info.uid = info.gid = 0
                info.uname = info.gname = "root"
                info.mtime = epoch
                if entry.is_file():
                    with entry.open("rb") as stream:
                        bundle.addfile(info, stream)
                else:
                    info.mode = 0o755
                    bundle.addfile(info)
        metadata = {
            "commit": commit.lower(), "version": version, "arch": arch,
            "target": args.target, "host_arch": host_arch, "host_os": "Linux",
            "elf_machine": elf_machine, "elf_class": 64,
            "rust": rust.splitlines()[0], "rust_verbose": rust,
            "file": archive_name, "bytes": archive.stat().st_size, "sha256": digest(archive),
            "binary": {"file": "nebulabook", "bytes": (stage / "nebulabook").stat().st_size, "sha256": digest(stage / "nebulabook")},
            "glibc_minimum": glibc_minimum, "glibc_baseline": args.glibc_baseline,
            "glibc_required_versions": glibc_versions, "dynamic_dependencies": needed,
            "elf_interpreter": interpreter, "release_eligible": not args.local_only,
            "native_execution_verified": bool(smoke_evidence), "smoke_evidence": smoke_evidence,
            "validation_note": "Packaging validates native architecture, ELF, direct GLIBC symbol requirements and checksums. Native execution status comes only from matching hashed X11 and Wayland smoke evidence. Graphics, portal and transitive runtime dependencies are not certified by this symbol scan.",
        }
        info_path = temporary / info_name
        info_path.write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        sums_path = temporary / sums_name
        sums_path.write_text(f"{digest(archive)}  {archive_name}\n{digest(info_path)}  {info_name}\n", encoding="ascii")
        # Reopen the archive and verify the executable bytes before publishing outputs.
        with tarfile.open(archive, "r:gz") as bundle:
            data = bundle.extractfile(f"{package_name}/nebulabook").read()
            if hashlib.sha256(data).hexdigest() != metadata["binary"]["sha256"]:
                fail("Archived binary checksum mismatch")
        for path in (archive, info_path, sums_path):
            os.replace(path, output / path.name)
    print(f"Packaged {output / archive_name}")
    print(f"ELF machine {elf_machine}; direct GLIBC requirement {glibc_minimum} <= {args.glibc_baseline}; native host {host_arch}")
    if args.local_only:
        print("LOCAL ONLY: this package is not eligible for the Ubuntu 22.04-baseline release")
    print("Verified matching native X11 and Wayland smoke evidence." if smoke_evidence else "No native execution evidence; executable was not run by packaging.")


try:
    main()
except (OSError, ValueError, subprocess.CalledProcessError, tarfile.TarError) as error:
    print(f"package-linux: {error}", file=sys.stderr)
    sys.exit(1)
PY
