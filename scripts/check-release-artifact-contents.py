#!/usr/bin/env python3
"""Fail closed when a Bluey terminal release leaks recoverable development material.

This is distribution hygiene, not DRM. A shipped native binary can still be
inspected, so Bluey's proprietary decisions must remain server-authoritative.
"""

from __future__ import annotations

import argparse
import io
import os
from pathlib import Path, PurePosixPath
import re
import stat
import sys
import tarfile
import tempfile
import zipfile


MAX_ARCHIVE_FILES = 512
MAX_ARCHIVE_UNCOMPRESSED_BYTES = 2 * 1024 * 1024 * 1024
READ_CHUNK_BYTES = 1024 * 1024

FORBIDDEN_SUFFIXES = {
    ".asar",
    ".c",
    ".cc",
    ".cpp",
    ".cs",
    ".cxx",
    ".debug",
    ".dwarf",
    ".go",
    ".h",
    ".hpp",
    ".java",
    ".jsx",
    ".m",
    ".map",
    ".mm",
    ".pdb",
    ".py",
    ".rs",
    ".swift",
    ".ts",
    ".tsx",
}
FORBIDDEN_BASENAMES = {
    ".env",
    "bluey-dashboard",
    "cargo.lock",
    "cargo.toml",
    "cue-dashboard",
    "package-lock.json",
    "package.json",
    "pnpm-lock.yaml",
    "resources.pak",
    "tsconfig.json",
    "yarn.lock",
}
FORBIDDEN_COMPONENTS = {
    ".git",
    "__pycache__",
    "app.asar.unpacked",
    "bluey.app",
    "node_modules",
    "src",
    "tests",
}
STATIC_PATTERNS = {
    "dev flag BLUEY_OVERLAY_CAPTURE_VISIBLE": b"BLUEY_OVERLAY_CAPTURE_VISIBLE",
    "dev flag BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE": b"BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE",
    "dev flag BLUEY_LOCAL_VISIBLE_OVERLAY": b"BLUEY_LOCAL_VISIBLE_OVERLAY",
    "dev flag BLUEY_ALLOW_CAPTURE_VISIBLE_LOCAL": b"BLUEY_ALLOW_CAPTURE_VISIBLE_LOCAL",
    "dev flag BLUEY_DEV_OVERLAY": b"BLUEY_DEV_OVERLAY",
    "dev flag bluey-local-visible-overlay": b"bluey-local-visible-overlay",
    "dev flag bluey-overlay-capture-visible": b"bluey-overlay-capture-visible",
    "dev flag bluey-dev-overlay": b"bluey-dev-overlay",
    "placeholder local transcription": b"[stub transcription]",
}
SECRET_ENV_NAMES = {
    "OPENAI_API_KEYS",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEYS",
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEYS",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEYS",
    "GOOGLE_API_KEY",
    "DEEPSEEK_API_KEYS",
    "DEEPSEEK_API_KEY",
    "ZAI_API_KEYS",
    "ZAI_API_KEY",
    "ZHIPU_API_KEYS",
    "ZHIPU_API_KEY",
    "DEEPGRAM_API_KEYS",
    "DEEPGRAM_API_KEY",
    "BLUEY_WEB_SEARCH_API_KEY",
    "TAVILY_API_KEY",
    "BRAVE_SEARCH_API_KEY",
    "BLUEY_OBJECT_SECRET_ACCESS_KEY",
    "BLUEY_R2_SECRET_ACCESS_KEY",
    "AWS_SECRET_ACCESS_KEY",
    "BLUEY_SQUARE_ACCESS_TOKEN",
    "SQUARE_ACCESS_TOKEN",
    "STRIPE_SECRET_KEY",
}


class ArtifactViolation(Exception):
    pass


def configured_patterns() -> dict[str, bytes]:
    patterns = dict(STATIC_PATTERNS)
    seen: set[bytes] = set(patterns.values())
    for name in sorted(SECRET_ENV_NAMES):
        for raw in re.split(r"[\n,]", os.environ.get(name, "")):
            secret = raw.strip().encode()
            if len(secret) < 16 or secret in seen:
                continue
            seen.add(secret)
            patterns[f"configured secret from {name}"] = secret
    return patterns


def normalized_entry(name: str) -> PurePosixPath:
    normalized = name.replace("\\", "/")
    path = PurePosixPath(normalized)
    if not normalized or normalized.startswith("/") or path.is_absolute():
        raise ArtifactViolation(f"unsafe absolute archive path: {name!r}")
    if any(part in {"", ".", ".."} for part in path.parts):
        raise ArtifactViolation(f"unsafe archive path traversal: {name!r}")
    return path


def validate_entry_name(name: str) -> str:
    path = normalized_entry(name)
    lower_parts = tuple(part.casefold() for part in path.parts)
    basename = lower_parts[-1]
    suffix = PurePosixPath(basename).suffix
    if basename in FORBIDDEN_BASENAMES:
        raise ArtifactViolation(f"forbidden development artifact: {path}")
    if suffix in FORBIDDEN_SUFFIXES:
        raise ArtifactViolation(f"forbidden development suffix {suffix}: {path}")
    if any(part.endswith(".dsym") for part in lower_parts):
        raise ArtifactViolation(f"forbidden debug-symbol bundle: {path}")
    if any(part in FORBIDDEN_COMPONENTS for part in lower_parts):
        raise ArtifactViolation(f"forbidden development directory: {path}")
    return path.as_posix()


def scan_stream(stream: io.BufferedReader, patterns: dict[str, bytes], label: str) -> None:
    if not patterns:
        return
    overlap = max(len(pattern) for pattern in patterns.values()) - 1
    tail = b""
    while True:
        chunk = stream.read(READ_CHUNK_BYTES)
        if not chunk:
            return
        haystack = tail + chunk
        for description, pattern in patterns.items():
            if pattern in haystack:
                raise ArtifactViolation(f"{label}: {description}")
        tail = haystack[-overlap:] if overlap > 0 else b""


def register_entry(
    name: str,
    size: int,
    seen: set[str],
    totals: list[int],
) -> str:
    normalized = validate_entry_name(name)
    folded = normalized.casefold()
    if folded in seen:
        raise ArtifactViolation(f"duplicate/case-colliding archive path: {normalized}")
    seen.add(folded)
    totals[0] += 1
    totals[1] += max(size, 0)
    if totals[0] > MAX_ARCHIVE_FILES:
        raise ArtifactViolation("release archive contains too many files")
    if totals[1] > MAX_ARCHIVE_UNCOMPRESSED_BYTES:
        raise ArtifactViolation("release archive exceeds the uncompressed size budget")
    return normalized


def scan_zip(path: Path, patterns: dict[str, bytes]) -> int:
    seen: set[str] = set()
    totals = [0, 0]
    with zipfile.ZipFile(path) as archive:
        for info in archive.infolist():
            if info.is_dir():
                continue
            unix_mode = info.external_attr >> 16
            if stat.S_ISLNK(unix_mode):
                raise ArtifactViolation(f"symbolic link is forbidden: {info.filename}")
            name = register_entry(info.filename, info.file_size, seen, totals)
            with archive.open(info) as stream:
                scan_stream(stream, patterns, f"{path.name}:{name}")
    if totals[0] == 0:
        raise ArtifactViolation("release archive is empty")
    return totals[0]


def scan_tar(path: Path, patterns: dict[str, bytes]) -> int:
    seen: set[str] = set()
    totals = [0, 0]
    with tarfile.open(path, "r:gz") as archive:
        for member in archive.getmembers():
            if member.isdir():
                continue
            if not member.isfile():
                raise ArtifactViolation(f"links and special files are forbidden: {member.name}")
            name = register_entry(member.name, member.size, seen, totals)
            stream = archive.extractfile(member)
            if stream is None:
                raise ArtifactViolation(f"could not inspect archive member: {name}")
            with stream:
                scan_stream(stream, patterns, f"{path.name}:{name}")
    if totals[0] == 0:
        raise ArtifactViolation("release archive is empty")
    return totals[0]


def scan_artifact(path: Path, patterns: dict[str, bytes] | None = None) -> int:
    patterns = configured_patterns() if patterns is None else patterns
    if not path.is_file():
        raise ArtifactViolation(f"release artifact is missing: {path}")
    if path.suffix.casefold() == ".zip":
        return scan_zip(path, patterns)
    if path.name.casefold().endswith(".tar.gz"):
        return scan_tar(path, patterns)
    with path.open("rb") as stream:
        scan_stream(stream, patterns, path.name)
    return 1


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="bluey-artifact-scan-") as directory:
        root = Path(directory)
        clean = root / "clean.zip"
        with zipfile.ZipFile(clean, "w") as archive:
            archive.writestr("bluey/bluey.exe", b"native-binary")
            archive.writestr("bluey/NOTICE.txt", b"Bluey terminal release")
        assert scan_artifact(clean, patterns={"secret": b"secret-value-1234"}) == 2

        clean_tar = root / "clean.tar.gz"
        with tarfile.open(clean_tar, "w:gz") as archive:
            payload = b"native-binary"
            member = tarfile.TarInfo("bluey/bluey")
            member.size = len(payload)
            member.mode = 0o755
            archive.addfile(member, io.BytesIO(payload))
        assert scan_artifact(clean_tar, patterns={"secret": b"secret-value-1234"}) == 1

        def rejected(
            name: str,
            data: bytes = b"x",
            patterns: dict[str, bytes] | None = None,
        ) -> None:
            candidate = root / (re.sub(r"[^a-z]", "-", name.casefold()) + ".zip")
            with zipfile.ZipFile(candidate, "w") as archive:
                archive.writestr(name, data)
            try:
                scan_artifact(
                    candidate,
                    patterns=patterns or {"secret": b"secret-value-1234"},
                )
            except ArtifactViolation:
                return
            raise AssertionError(f"scanner accepted forbidden entry {name}")

        rejected("bluey/client.js.map")
        rejected("../escape.exe")
        rejected("bluey/src/main.rs")
        rejected("bluey/bluey.exe", b"prefix-secret-value-1234-suffix")
        rejected(
            "bluey/cue-whisper.exe",
            b"[stub transcription]",
            patterns={"placeholder": b"[stub transcription]"},
        )
        rejected("Bluey.app/Contents/MacOS/bluey")
        rejected("bin/bluey-dashboard")

        symlink_zip = root / "symlink.zip"
        with zipfile.ZipFile(symlink_zip, "w") as archive:
            link = zipfile.ZipInfo("bluey/current")
            link.create_system = 3
            link.external_attr = (stat.S_IFLNK | 0o777) << 16
            archive.writestr(link, "bluey.exe")
        try:
            scan_artifact(symlink_zip, patterns={})
        except ArtifactViolation:
            pass
        else:
            raise AssertionError("scanner accepted a symbolic link")
    print("Release artifact scanner self-test passed (2 clean + 8 rejection cases).")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("artifacts", nargs="*", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if not args.artifacts:
        parser.error("provide at least one release archive")
    patterns = configured_patterns()
    checked = 0
    failures: list[str] = []
    for artifact in args.artifacts:
        try:
            checked += scan_artifact(artifact, patterns)
        except (ArtifactViolation, OSError, tarfile.TarError, zipfile.BadZipFile) as error:
            failures.append(f"{artifact.name}: {error}")
    if failures:
        print("Release artifact content scan FAILED:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    secret_count = len(patterns) - len(STATIC_PATTERNS)
    print(
        "Release artifact content scan passed "
        f"({checked} files, {secret_count} configured secret value(s) covered)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
