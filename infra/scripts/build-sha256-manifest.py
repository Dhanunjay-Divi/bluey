#!/usr/bin/env python3
"""Build sha256-manifest.json from SHA256SUMS.txt.

Run from a directory that contains `SHA256SUMS.txt` (output of
`shasum -a 256 ...`). Writes `sha256-manifest.json` next to it as a
flat `{filename: hex_digest}` JSON object that
`infra/scripts/bump-formulae.sh` consumes.
"""

import json
import sys
from pathlib import Path


def main() -> int:
    src = Path("SHA256SUMS.txt")
    if not src.exists():
        print("SHA256SUMS.txt not found in CWD", file=sys.stderr)
        return 1
    sums: dict[str, str] = {}
    with src.open() as f:
        for line in f:
            parts = line.strip().split(None, 1)
            if len(parts) == 2:
                sums[parts[1]] = parts[0]
    Path("sha256-manifest.json").write_text(
        json.dumps(sums, indent=2, sort_keys=True) + "\n"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
