#!/usr/bin/env bash
# Scan release-facing files for accidental secrets and production-unsafe flags.
#
# This is intentionally conservative: it fails on real-looking credential
# material and release artifacts that set dev-only flags, while warning about
# allowed documentation mentions. Run before publishing bluey.sh or release
# archives.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [ "$#" -gt 0 ]; then
    TARGETS=("$@")
else
    TARGETS=(web ops scripts docs/deploy docs/PRELAUNCH-CHECKLIST.md docs/SECURITY-HARDENING.md)
    [ -d dist ] && TARGETS+=(dist)
fi

python3 - "$ROOT" "${TARGETS[@]}" <<'PY'
import os
import re
import sys
from pathlib import Path

root = Path(sys.argv[1]).resolve()
raw_targets = [Path(p) for p in sys.argv[2:]]

SECRET_PATTERNS = [
    ("OpenAI API key", re.compile(r"\bsk-(?:svcacct|proj)?[A-Za-z0-9_-]{24,}\b")),
    ("Anthropic API key", re.compile(r"\bsk-ant-api\d{2,}-[A-Za-z0-9_-]{24,}\b")),
    ("Google/Gemini API key", re.compile(r"\bAIza[0-9A-Za-z_-]{24,}\b")),
    ("Square access/token-like value", re.compile(r"\bsq0(?:atp|atb|csp|idp|idb)-[0-9A-Za-z_-]{10,}\b")),
    ("Square OAuth bearer", re.compile(r"\bEAAA[0-9A-Za-z_-]{24,}\b")),
    ("Resend API key", re.compile(r"\bre_[0-9A-Za-z_]{20,}\b")),
    ("JWT secret assignment", re.compile(r"\bBLUEY_JWT_SECRET\s*=\s*['\"]?[A-Za-z0-9._~+/=-]{24,}")),
]

PROD_UNSAFE_ASSIGNMENTS = [
    re.compile(r"\bBLUEY_OVERLAY_CAPTURE_VISIBLE\s*=\s*1\b"),
    re.compile(r"\bBLUEY_HOST_OVERLAY_CAPTURE_VISIBLE\s*=\s*1\b"),
    re.compile(r"\bBLUEY_LOCAL_VISIBLE_OVERLAY\s*=\s*1\b"),
    re.compile(r"\bBLUEY_ALLOW_CAPTURE_VISIBLE_LOCAL\s*=\s*1\b"),
    re.compile(r"\bBLUEY_DEV_OVERLAY\s*=\s*1\b"),
    re.compile(r"\bBLUEY_DEV_BYOK\s*=\s*1\b"),
    re.compile(r"\bBLUEY_DEV_DIRECT_PROVIDERS\s*=\s*1\b"),
    re.compile(r"\bBLUEY_DEV_DIRECT_STT\s*=\s*1\b"),
    re.compile(r"\bBLUEY_DEV_DIRECT_VISION\s*=\s*1\b"),
    re.compile(r"\bBLUEY_ALLOW_PLAINTEXT_TOKENS\s*=\s*1\b"),
    re.compile(r"\bBLUEY_DEV_PLAINTEXT_TOKENS\s*=\s*1\b"),
    re.compile(r"\bBLUEY_UPDATE_ALLOW_UNSIGNED\s*=\s*1\b"),
    re.compile(r"\bBLUEY_RELEASE_ALLOW_UNSIGNED\s*=\s*1\b"),
]

EXCLUDED_DIRS = {
    ".git",
    "target",
    "node_modules",
    ".venv",
    "__pycache__",
    ".pytest_cache",
    ".cargo",
}
EXCLUDED_SUFFIXES = {
    ".db",
    ".sqlite",
    ".sqlite3",
    ".wal",
    ".shm",
    ".png",
    ".jpg",
    ".jpeg",
    ".gif",
    ".ico",
    ".icns",
    ".mov",
    ".mp4",
    ".zip",
    ".gz",
    ".tgz",
    ".tar",
}
DEV_SCRIPT_ALLOWLIST = {
    "scripts/bluey-visible-local.sh",
    "scripts/macos-overlay-visual-smoke.sh",
}
LINE_ALLOWLIST = re.compile(
    r"(replace-with|example|dummy|redacted|REDACTED|test_secret|test-secret|"
    r"secret-at-least|secret_access|secret-refresh|Set .* for local/dev-only|"
    r"BLUEY_RELEASE_ALLOW_UNSIGNED=1 for a local/dev-only publish|"
    r"publish is disabled unless BLUEY_RELEASE_ALLOW_UNSIGNED=1)"
)


def iter_files(targets):
    for target in targets:
        path = (root / target).resolve() if not target.is_absolute() else target.resolve()
        if not path.exists():
            continue
        if path.is_file():
            yield path
            continue
        for current, dirnames, filenames in os.walk(path):
            rel_parts = Path(current).resolve().relative_to(root).parts
            if any(part in EXCLUDED_DIRS for part in rel_parts):
                dirnames[:] = []
                continue
            dirnames[:] = [d for d in dirnames if d not in EXCLUDED_DIRS]
            for filename in filenames:
                file_path = Path(current) / filename
                if file_path.suffix.lower() in EXCLUDED_SUFFIXES:
                    continue
                yield file_path.resolve()


def safe_read(path):
    try:
        data = path.read_bytes()
    except OSError:
        return None
    if b"\x00" in data[:4096]:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        try:
            return data.decode("utf-8", errors="ignore")
        except Exception:
            return None


failures = []
warnings = []
seen = set()
for path in iter_files(raw_targets):
    if path in seen:
        continue
    seen.add(path)
    try:
        rel = path.relative_to(root).as_posix()
    except ValueError:
        rel = str(path)
    text = safe_read(path)
    if text is None:
        continue
    for lineno, line in enumerate(text.splitlines(), 1):
        if LINE_ALLOWLIST.search(line):
            continue
        for label, pattern in SECRET_PATTERNS:
            if pattern.search(line):
                failures.append(f"{rel}:{lineno}: possible {label}")
        for pattern in PROD_UNSAFE_ASSIGNMENTS:
            if pattern.search(line):
                if rel in DEV_SCRIPT_ALLOWLIST or rel.startswith("docs/"):
                    warnings.append(f"{rel}:{lineno}: dev-only flag mention")
                else:
                    failures.append(f"{rel}:{lineno}: production-unsafe dev flag assignment")

if warnings:
    print("Release hygiene warnings:")
    for warning in warnings:
        print(f"  - {warning}")

if failures:
    print("Release hygiene scan FAILED:")
    for failure in failures:
        print(f"  - {failure}")
    sys.exit(1)

print(f"Release hygiene scan passed ({len(seen)} files checked).")
PY
