#!/usr/bin/env python3
"""Minimal, non-executing Electron ASAR reader used for this audit.

The script parses the Chromium-pickle header, lists members, and optionally
copies regular packed members to a caller-supplied temporary directory. It
does not import or execute archive content. Unpacked members and links are
reported but deliberately not copied.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import sys


def load_header(archive: Path) -> tuple[dict, int]:
    with archive.open("rb") as source:
        size_pickle = source.read(8)
        if len(size_pickle) != 8:
            raise ValueError("truncated ASAR size pickle")
        payload_size, header_size = struct.unpack("<II", size_pickle)
        if payload_size != 4 or header_size < 8:
            raise ValueError("unexpected ASAR size pickle")
        header_pickle = source.read(header_size)
    if len(header_pickle) != header_size:
        raise ValueError("truncated ASAR header pickle")
    string_payload_size, string_size = struct.unpack("<II", header_pickle[:8])
    if string_payload_size + 4 > len(header_pickle):
        raise ValueError("invalid ASAR header payload length")
    raw_json = header_pickle[8 : 8 + string_size]
    return json.loads(raw_json.decode("utf-8")), 8 + header_size


def walk(node: dict, prefix: str = ""):
    for name in sorted(node.get("files", {})):
        child = node["files"][name]
        path = f"{prefix}/{name}" if prefix else name
        if "files" in child:
            yield path, "directory", child
            yield from walk(child, path)
        elif "link" in child:
            yield path, "link", child
        else:
            yield path, "file", child


def safe_destination(root: Path, member: str) -> Path:
    candidate = (root / member).resolve()
    if os.path.commonpath((str(root.resolve()), str(candidate))) != str(root.resolve()):
        raise ValueError(f"unsafe ASAR member path: {member!r}")
    return candidate


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path)
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--extract-packed", type=Path)
    args = parser.parse_args()

    header, data_start = load_header(args.archive)
    members = list(walk(header))
    totals = {"directory": 0, "file": 0, "link": 0, "unpacked": 0}
    verified = 0
    mismatches = 0

    with args.archive.open("rb") as source:
        for path, kind, metadata in members:
            totals[kind] += 1
            if kind == "file" and metadata.get("unpacked"):
                totals["unpacked"] += 1
            if args.list:
                detail = ""
                if kind == "file":
                    detail = (
                        f" size={metadata.get('size', 0)}"
                        f" offset={metadata.get('offset', '-') }"
                        f" unpacked={bool(metadata.get('unpacked'))}"
                    )
                elif kind == "link":
                    detail = f" target={metadata.get('link')}"
                print(f"{kind}\t{path}{detail}")
            if (
                kind != "file"
                or metadata.get("unpacked")
                or args.extract_packed is None
            ):
                continue
            size = int(metadata.get("size", 0))
            offset = int(metadata.get("offset", 0))
            source.seek(data_start + offset)
            content = source.read(size)
            if len(content) != size:
                raise ValueError(f"truncated ASAR member: {path}")
            integrity = metadata.get("integrity", {})
            expected = integrity.get("hash")
            if expected and integrity.get("algorithm", "").upper() == "SHA256":
                actual = hashlib.sha256(content).hexdigest()
                if actual == expected.lower():
                    verified += 1
                else:
                    mismatches += 1
                    print(
                        f"INTEGRITY_MISMATCH\t{path}\texpected={expected}\tactual={actual}",
                        file=sys.stderr,
                    )
            destination = safe_destination(args.extract_packed, path)
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(content)

    print(
        "SUMMARY"
        f" data_start={data_start}"
        f" directories={totals['directory']}"
        f" files={totals['file']}"
        f" unpacked_files={totals['unpacked']}"
        f" links={totals['link']}"
        f" integrity_verified={verified}"
        f" integrity_mismatches={mismatches}"
    )
    return 1 if mismatches else 0


if __name__ == "__main__":
    raise SystemExit(main())
