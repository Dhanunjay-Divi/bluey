#!/usr/bin/env python3
"""Verify that a Bluey Jobs worker release archive is portable and self-contained."""

from __future__ import annotations

import sys
import tarfile
from pathlib import PurePosixPath
from typing import NoReturn


def fail(message: str) -> NoReturn:
    print(f"verify-bluey-jobs-workers-archive: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    if len(sys.argv) != 3:
        fail("usage: verify-bluey-jobs-workers-archive.py ARCHIVE RELEASE_ID")

    archive_path, release_id = sys.argv[1:]
    expected_files = {
        f"{release_id}/manifest.json",
        f"{release_id}/jobs/workflows/dist/discovery-worker.js",
        f"{release_id}/jobs/workflows/dist/global-discovery-worker.js",
    }

    try:
        with tarfile.open(archive_path, "r:gz") as archive:
            members = archive.getmembers()
    except (OSError, tarfile.TarError) as error:
        fail(f"cannot read archive: {error}")

    if not members:
        fail("archive is empty")

    names = {member.name.rstrip("/") for member in members if member.name.rstrip("/")}
    roots: set[str] = set()
    for member in members:
        path = PurePosixPath(member.name)
        parts = path.parts
        if not parts:
            continue
        if path.is_absolute() or ".." in parts:
            fail(f"unsafe member path: {member.name}")
        if any(part.startswith("._") for part in parts):
            fail(f"AppleDouble metadata is not portable: {member.name}")
        roots.add(parts[0])

    if roots != {release_id}:
        fail(f"archive must have one release root named {release_id}: {sorted(roots)}")

    missing = sorted(expected_files - names)
    if missing:
        fail(f"archive is missing required runtime files: {', '.join(missing)}")


if __name__ == "__main__":
    main()
