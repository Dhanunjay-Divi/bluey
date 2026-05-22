#!/usr/bin/env python3
"""Phase 6: standard field migration sweep.

Mechanically transforms tracing call sites identified by
analyze-tracing-calls.py:

  account_id = %<EXPR>            → account_id_hash = %cue_core::account_id_hash_prefix(<EXPR_AS_REF>)
  email = %<EXPR>                 → DROP (account_id_hash is enough; email re-derivable)
  session = ...                   → session_id = ... (rename)

Run:
  python3 scripts/migrate-tracing-fields.py --dry-run   # preview diffs
  python3 scripts/migrate-tracing-fields.py --apply     # write changes
"""

from __future__ import annotations

import argparse
import difflib
import re
import sys
from pathlib import Path

# ── Files to touch (pre-determined from the analyzer) ───────────────────

TARGETS = [
    "server/src/api/account.rs",
    "server/src/api/auth_routes.rs",
    "server/src/api/router.rs",
    "server/src/api/stt.rs",
    "server/src/api/usage.rs",
    "crates/cue-dashboard/src/lib.rs",
    # Added after Phase 6 close: a single `session = %session_id` site
    # the original analyzer flagged but my targeted sweep missed. The
    # site lives at cue-daemon/src/app.rs:6996 and migrates the bare
    # `session` alias to `session_id`.
    "crates/cue-daemon/src/app.rs",
]


# ── Migrations ──────────────────────────────────────────────────────────


# Match `account_id = %<expr>` where <expr> is a path like
# `account.id`, `session.account_id`, `&account.id`, etc.
# The trailing comma OR closing-paren OR newline determines the boundary.
ACCOUNT_ID_FIELD = re.compile(
    r"account_id\s*=\s*%(\&?)([a-zA-Z_][a-zA-Z0-9_.]*)"
)


def migrate_account_id(text: str) -> tuple[str, int]:
    """Replace `account_id = %X` → `account_id_hash = %cue_core::account_id_hash_prefix(&X)`.

    Handles the leading `&` (already-borrowed) by re-using it in the wrapped
    call. The result always passes a `&str` into the helper.
    """
    count = 0

    def repl(m: re.Match) -> str:
        nonlocal count
        count += 1
        already_borrowed = m.group(1)
        expr = m.group(2)
        # The hash function takes &str. If the expression is already a path
        # like `account.id` (likely String), we &-borrow it. If it was
        # already `&account.id`, keep the borrow.
        if already_borrowed == "&":
            arg = f"&{expr}"
        else:
            arg = f"&{expr}"
        return f"account_id_hash = %cue_core::account_id_hash_prefix({arg})"

    new = ACCOUNT_ID_FIELD.sub(repl, text)
    return new, count


# Match `email = %<expr>` field including the trailing comma+whitespace
# OR the preceding comma+whitespace. We handle both shapes:
#
#   - Inline: "..., email = %x.email, ..."     → drop ", email = %x.email"
#   - First field: "(email = %x.email, ..."    → drop "email = %x.email, "
#   - Last field: "..., email = %x.email)"     → drop ", email = %x.email"
#   - Solo field: "(email = %x.email)"         → drop "email = %x.email"
#   - Multi-line: "(\n    email = %x.email,\n  ...)" → drop the line entirely

# Strategy: do two passes:
# 1. Drop "email = %expr," followed by whitespace/newline (any non-last position).
# 2. Drop ", email = %expr" before a close-paren or newline (last position).
EMAIL_FIRST_OR_MIDDLE = re.compile(
    r"email\s*=\s*%[a-zA-Z_][a-zA-Z0-9_.]*\s*,\s*\n?\s*"
)
EMAIL_LAST = re.compile(
    r",\s*email\s*=\s*%[a-zA-Z_][a-zA-Z0-9_.]*"
)


def migrate_email(text: str) -> tuple[str, int]:
    new = text
    count_first = len(EMAIL_FIRST_OR_MIDDLE.findall(new))
    new = EMAIL_FIRST_OR_MIDDLE.sub("", new)
    count_last = len(EMAIL_LAST.findall(new))
    new = EMAIL_LAST.sub("", new)
    return new, count_first + count_last


# Rename `session = ` (alias) → `session_id = `.
# Care: don't rewrite legit identifiers like `session_id`, `session_token`,
# `session_count`, etc. Only the bare alias.
SESSION_ALIAS = re.compile(r"\bsession\s*=\s*(?!session)")


def migrate_session_alias(text: str) -> tuple[str, int]:
    """Rename `session = X` → `session_id = X` only when the field name
    is bare 'session'."""
    # Only match inside a tracing!() call. Use a coarse line-based
    # heuristic: replace if the line contains `tracing::` OR a known
    # tracing macro short form that we use, AND the line has the
    # bare 'session = '.
    out_lines = []
    count = 0
    in_tracing_call = False
    paren_depth = 0
    for line in text.splitlines(keepends=True):
        if re.search(r"\btracing::(?:trace|debug|info|warn|error)!\s*\(", line):
            in_tracing_call = True
            paren_depth = line.count("(") - line.count(")")
        elif in_tracing_call:
            paren_depth += line.count("(") - line.count(")")
            if paren_depth <= 0:
                in_tracing_call = False
        if in_tracing_call:
            new_line = re.sub(
                r"(\W|^)session\s*=\s*(?!session)",
                lambda m: f"{m.group(1)}session_id = ",
                line,
            )
            if new_line != line:
                count += 1
            out_lines.append(new_line)
        else:
            out_lines.append(line)
    return "".join(out_lines), count


# ── Driver ──────────────────────────────────────────────────────────────


def process_file(path: str, dry_run: bool) -> dict:
    p = Path(path)
    if not p.exists():
        return {"path": path, "skipped": True, "reason": "missing"}

    original = p.read_text()
    text = original

    text, n_account = migrate_account_id(text)
    text, n_email = migrate_email(text)
    text, n_session = migrate_session_alias(text)

    changed = text != original
    if changed and not dry_run:
        p.write_text(text)

    diff = ""
    if changed and dry_run:
        diff = "".join(
            difflib.unified_diff(
                original.splitlines(keepends=True),
                text.splitlines(keepends=True),
                fromfile=str(p),
                tofile=str(p),
                n=2,
            )
        )

    return {
        "path": path,
        "changed": changed,
        "account_id_subs": n_account,
        "email_drops": n_email,
        "session_renames": n_session,
        "diff": diff,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--apply", action="store_true", help="Write changes to disk.")
    ap.add_argument("--dry-run", action="store_true", help="Show diffs only (default).")
    args = ap.parse_args()

    if not args.apply and not args.dry_run:
        args.dry_run = True

    results = []
    for target in TARGETS:
        results.append(process_file(target, dry_run=args.dry_run))

    print(f"=== Phase 6 sweep ({'DRY-RUN' if args.dry_run else 'APPLIED'}) ===")
    print()
    total = {"account_id_subs": 0, "email_drops": 0, "session_renames": 0}
    for r in results:
        if r.get("skipped"):
            print(f"  ⊘ {r['path']:<48} (skipped: {r['reason']})")
            continue
        marker = "✓" if r["changed"] else "·"
        print(
            f"  {marker} {r['path']:<48} "
            f"account_id_subs={r['account_id_subs']:>2} "
            f"email_drops={r['email_drops']:>2} "
            f"session_renames={r['session_renames']:>2}"
        )
        for k in ("account_id_subs", "email_drops", "session_renames"):
            total[k] += r[k]
    print()
    print(f"  TOTAL: account_id_subs={total['account_id_subs']} "
          f"email_drops={total['email_drops']} "
          f"session_renames={total['session_renames']}")

    if args.dry_run:
        print()
        print("=== unified diff ===")
        for r in results:
            if r.get("diff"):
                print(r["diff"])

    return 0


if __name__ == "__main__":
    sys.exit(main())
