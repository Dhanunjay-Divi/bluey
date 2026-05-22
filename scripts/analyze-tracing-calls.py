#!/usr/bin/env python3
"""Analyze tracing macro call sites across the Bluey codebase.

Reads .rs files under crates/, server/, native/ and identifies every
`tracing::{info,warn,error,debug,trace}!(...)` invocation. Produces
a structured report showing:

  1. Summary counts by component, level, and field-set shape.
  2. Field-name inconsistencies (request_id vs req_id vs requestId).
  3. PII risks (raw email or raw account_id literals in messages).
  4. Per-file inventory of non-conformant call sites.

This is the Phase 6 pre-work for the Observability Round. Once Codex's
Phase 1 lands the standard fields struct in cue-core, this script
becomes the input for the mechanical migration sweep.

Usage:
  scripts/analyze-tracing-calls.py            # full report to stdout
  scripts/analyze-tracing-calls.py --json     # JSON output for tooling
  scripts/analyze-tracing-calls.py --pii-only # only PII-risk findings
  scripts/analyze-tracing-calls.py --fields   # field-name histogram
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

# ── Configuration ──────────────────────────────────────────────────────

# Levels we look for. The macros may be invoked as either `tracing::info!`
# or via `use tracing::info; info!(...)`. We look for both.
LEVELS = ("trace", "debug", "info", "warn", "error")

# Field names we recognize as standard per the Observability Round plan §2.
# Anything else gets flagged as "non-standard" so the sweep can normalize.
STANDARD_FIELDS = {
    "component",
    "version",
    "platform",
    "trace_id",
    "request_id",
    "session_id",
    "account_id_hash",
    "status",
    "latency_ms",
    "provider",
    "model",
    "cost_cents_to_customer",
    "cost_cents_to_bluey",
    "error",       # canonical for std::error::Error chain
    "input_tokens",
    "output_tokens",
}

# Fields we know are still in flight; warn but don't classify as risky.
TRANSITIONAL_FIELDS = {
    "account_id",   # should become account_id_hash post-Phase 1
    "email",        # should be redacted or hashed
    "user_id",      # should become user_id_hash
    "device_id",    # should become device_id_hash
}

# Aliases that should map to a canonical name.
ALIAS_TO_CANONICAL = {
    "req_id": "request_id",
    "requestId": "request_id",
    "reqId": "request_id",
    "tid": "trace_id",
    "traceId": "trace_id",
    "session": "session_id",
    "sessionId": "session_id",
    "acct_id": "account_id",
    "accountId": "account_id",
}

# Substrings in the message string that indicate PII risk.
PII_MESSAGE_PATTERNS = [
    re.compile(r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b"),
    re.compile(r"\bsk-[A-Za-z0-9_\-]{8,}\b"),       # OpenAI/Anthropic key shape
    re.compile(r"\bBearer\s+[A-Za-z0-9._\-]+\b"),    # bearer header
    re.compile(r"\b[a-f0-9]{40}\b"),                  # Deepgram-shape hex
    re.compile(r"\b(?:cus|pi|sk|whsec|cs)_[A-Za-z0-9]{6,}\b"),  # Stripe IDs
]

# Crate roots to crawl.
CRATE_ROOTS = ["crates", "server", "native"]

# Exclude test directories and build artifacts.
EXCLUDE_PATTERNS = [
    re.compile(r"/target/"),
    re.compile(r"/\.git/"),
    re.compile(r"/build/"),
    re.compile(r"/\.swiftpm/"),
]

# ── Models ──────────────────────────────────────────────────────────────


@dataclass
class TracingCall:
    file: str
    line: int
    level: str
    fields: list[str]                         # field names, in order
    field_values: dict[str, str]              # raw value text for each field
    message: str                              # the format string (best-effort)
    raw: str                                  # the matched substring
    component: str                            # crate detected from path

    def has_field(self, name: str) -> bool:
        return name in self.fields

    def is_conformant(self) -> bool:
        # A call is conformant when it carries enough fields for log
        # correlation. The minimum "join key" set is component (we can
        # infer this from the source path) plus at least one of trace_id
        # or session_id or request_id when the call is operation-shaped.
        # Errors and lifecycle messages can be more relaxed.
        if self.level in ("trace", "debug"):
            return True
        if self.level == "error":
            return self.has_field("error") or "error" in self.message.lower()
        # info/warn at operation boundary:
        return any(f in self.fields for f in ("trace_id", "request_id", "session_id"))


@dataclass
class Report:
    calls: list[TracingCall] = field(default_factory=list)
    field_counter: Counter = field(default_factory=Counter)
    alias_counter: Counter = field(default_factory=Counter)
    pii_findings: list[TracingCall] = field(default_factory=list)
    transitional_findings: list[TracingCall] = field(default_factory=list)


# ── Crawling ────────────────────────────────────────────────────────────


def walk_rust_files(roots: Iterable[str]) -> Iterable[Path]:
    for root in roots:
        if not os.path.isdir(root):
            continue
        for dirpath, dirnames, filenames in os.walk(root):
            full = dirpath
            if any(p.search(full) for p in EXCLUDE_PATTERNS):
                dirnames[:] = []
                continue
            for fn in filenames:
                if fn.endswith(".rs"):
                    yield Path(dirpath) / fn


def crate_name_from_path(path: Path) -> str:
    """Best-effort: crates/cue-foo/src/bar.rs -> cue-foo;
    server/src/api/bar.rs -> bluey-server."""
    parts = path.parts
    if "crates" in parts:
        idx = parts.index("crates")
        if idx + 1 < len(parts):
            return parts[idx + 1]
    if "server" in parts:
        return "bluey-server"
    if "native" in parts:
        idx = parts.index("native")
        if idx + 2 < len(parts):
            return parts[idx + 2]
    return "?"


# ── Parsing ─────────────────────────────────────────────────────────────


# Match `tracing::<level>!(...)` AND short-form `<level>!(...)` (after `use
# tracing::<level>;`). We approximate: match `<level>!(` where <level> is
# one of LEVELS, capture the args until the matching `)`.
LEVEL_PATTERN = re.compile(
    r"(?<![A-Za-z0-9_])(?:tracing::)?(trace|debug|info|warn|error)!\s*\("
)


def find_call_sites(text: str) -> list[tuple[int, str, str]]:
    """Return list of (line, level, args_text) for each tracing call.

    Crude paren-balance for the args, no full Rust tokenizer. Should be
    accurate enough for typical Bluey call sites; will drop a handful of
    pathological cases (string literals containing unmatched `(` etc.).
    """
    out = []
    for match in LEVEL_PATTERN.finditer(text):
        level = match.group(1)
        start = match.end()  # just past the opening paren
        depth = 1
        i = start
        in_string = False
        in_string_char = ""
        while i < len(text):
            ch = text[i]
            if in_string:
                if ch == "\\":
                    i += 2
                    continue
                if ch == in_string_char:
                    in_string = False
            else:
                if ch in ('"', "'"):
                    # Skip char literals for simplicity.
                    in_string = True
                    in_string_char = ch
                elif ch == "(":
                    depth += 1
                elif ch == ")":
                    depth -= 1
                    if depth == 0:
                        # `start` to `i` is the args body.
                        args = text[start:i]
                        line = text.count("\n", 0, match.start()) + 1
                        out.append((line, level, args))
                        break
            i += 1
    return out


# Field-shape patterns inside macro args. Tracing accepts these forms:
#   field_name = value
#   field_name = %expr      (Display)
#   field_name = ?expr      (Debug)
#   ?expr                   (shorthand: name = "expr", rendered with Debug)
#   %expr                   (shorthand: name = "expr", rendered with Display)
#   "literal message"       (the format string)
#   "format {arg}", arg
FIELD_PATTERN = re.compile(
    r"(?:^|,)\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([%?]?)([^,]+?)(?=,|\Z)",
    re.DOTALL,
)


def parse_args(args: str) -> tuple[list[str], dict[str, str], str]:
    """Return (field_names, field_values, message_string)."""
    fields = []
    values = {}
    # Find the message — first quoted string literal not inside an existing
    # `field = "..."` form. Approximation: find LAST quoted string in args
    # which usually is the message.
    message = ""
    for m in re.finditer(r'(?<!\\)"((?:[^"\\]|\\.)*)"', args):
        message = m.group(1)
    # Find named fields.
    for m in FIELD_PATTERN.finditer(args):
        name = m.group(1)
        sigil = m.group(2)
        rawval = m.group(3).strip()
        # Skip if this looks like the message format string itself
        # (rare false-positive: name == something + value is a long
        # quoted string). Fallback: keep it; downstream consumers will
        # see this and ignore.
        fields.append(name)
        values[name] = (sigil + rawval).strip()
    return fields, values, message


# ── Detection ───────────────────────────────────────────────────────────


def detect_pii_in_message(message: str) -> bool:
    return any(p.search(message) for p in PII_MESSAGE_PATTERNS)


def normalize_field(name: str) -> str:
    return ALIAS_TO_CANONICAL.get(name, name)


def analyze() -> Report:
    report = Report()
    for path in walk_rust_files(CRATE_ROOTS):
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        component = crate_name_from_path(path)
        for line, level, args in find_call_sites(text):
            fields_list, values, message = parse_args(args)
            call = TracingCall(
                file=str(path),
                line=line,
                level=level,
                fields=fields_list,
                field_values=values,
                message=message,
                raw=args[:160].replace("\n", " "),
                component=component,
            )
            report.calls.append(call)
            for f in fields_list:
                canon = normalize_field(f)
                report.field_counter[canon] += 1
                if f != canon:
                    report.alias_counter[(f, canon)] += 1

            if detect_pii_in_message(message):
                report.pii_findings.append(call)

            if any(f in TRANSITIONAL_FIELDS for f in fields_list):
                report.transitional_findings.append(call)

    return report


# ── Reporting ────────────────────────────────────────────────────────────


def print_summary(report: Report) -> None:
    by_component: dict[str, Counter] = defaultdict(Counter)
    by_level: Counter = Counter()
    conformant = 0
    non_conformant = 0
    for call in report.calls:
        by_component[call.component][call.level] += 1
        by_level[call.level] += 1
        if call.is_conformant():
            conformant += 1
        else:
            non_conformant += 1

    print("=" * 64)
    print("Bluey Tracing Call Inventory")
    print("=" * 64)
    print(f"Total call sites: {len(report.calls)}")
    print(f"Conformant:       {conformant}")
    print(f"Non-conformant:   {non_conformant}")
    print()

    print("── By level ──")
    for lvl, count in sorted(by_level.items(), key=lambda kv: -kv[1]):
        print(f"  {lvl:<8} {count}")
    print()

    print("── By component ──")
    for comp, counts in sorted(by_component.items(), key=lambda kv: -sum(kv[1].values())):
        total = sum(counts.values())
        breakdown = " ".join(f"{lvl}={n}" for lvl, n in counts.most_common())
        print(f"  {comp:<24} {total:>4} ({breakdown})")
    print()

    print("── Field-name inconsistency (alias → canonical) ──")
    if not report.alias_counter:
        print("  (none — field names already canonical)")
    else:
        for (alias, canon), n in sorted(report.alias_counter.items(), key=lambda kv: -kv[1]):
            print(f"  {alias:<16} → {canon:<16} ({n} sites)")
    print()

    print("── Top 20 fields by usage ──")
    for name, n in report.field_counter.most_common(20):
        marker = "  "
        if name in STANDARD_FIELDS:
            marker = "✓ "
        elif name in TRANSITIONAL_FIELDS:
            marker = "△ "
        print(f"  {marker}{name:<24} {n}")
    print()


def print_pii_findings(report: Report) -> None:
    if not report.pii_findings:
        print("PII findings: none detected in tracing message strings.")
        return
    print("=" * 64)
    print(f"PII RISK: {len(report.pii_findings)} call site(s) embed PII-shaped")
    print("data in the message string. These should be moved to redacted")
    print("fields (account_id_hash, <email>, etc.) before logging.")
    print("=" * 64)
    for call in report.pii_findings:
        print(f"  {call.file}:{call.line} [{call.level}] {call.message[:100]}")
    print()


def print_transitional_findings(report: Report) -> None:
    if not report.transitional_findings:
        return
    print("=" * 64)
    print(f"TRANSITIONAL: {len(report.transitional_findings)} call site(s) use")
    print("fields that should migrate to the standard set in Phase 6:")
    print("  account_id  → account_id_hash")
    print("  user_id     → user_id_hash")
    print("  device_id   → device_id_hash")
    print("  email       → drop or hash")
    print("=" * 64)
    by_field = defaultdict(list)
    for call in report.transitional_findings:
        for f in call.fields:
            if f in TRANSITIONAL_FIELDS:
                by_field[f].append(call)
    for fname, calls in sorted(by_field.items(), key=lambda kv: -len(kv[1])):
        print(f"\n  {fname} ({len(calls)} sites):")
        for c in calls[:5]:
            print(f"    {c.file}:{c.line} [{c.level}] {c.message[:80]}")
        if len(calls) > 5:
            print(f"    ... +{len(calls) - 5} more")
    print()


def emit_json(report: Report) -> str:
    return json.dumps(
        {
            "total_calls": len(report.calls),
            "conformant": sum(1 for c in report.calls if c.is_conformant()),
            "by_component": dict(
                Counter((c.component, c.level) for c in report.calls).most_common()
            ),
            "alias_counts": {f"{a}->{b}": n for (a, b), n in report.alias_counter.items()},
            "pii_findings": [
                {"file": c.file, "line": c.line, "level": c.level, "message": c.message}
                for c in report.pii_findings
            ],
            "transitional_findings": [
                {"file": c.file, "line": c.line, "fields": c.fields}
                for c in report.transitional_findings
            ],
        },
        indent=2,
        default=str,
    )


# ── Entry ───────────────────────────────────────────────────────────────


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--json", action="store_true", help="Emit JSON")
    ap.add_argument("--pii-only", action="store_true", help="Only print PII findings")
    ap.add_argument("--fields", action="store_true", help="Only print field histogram")
    args = ap.parse_args()

    report = analyze()

    if args.json:
        print(emit_json(report))
        return 0

    if args.pii_only:
        print_pii_findings(report)
        return 1 if report.pii_findings else 0

    if args.fields:
        print("── Top 50 fields by usage ──")
        for name, n in report.field_counter.most_common(50):
            marker = "  "
            if name in STANDARD_FIELDS:
                marker = "✓ "
            elif name in TRANSITIONAL_FIELDS:
                marker = "△ "
            print(f"  {marker}{name:<28} {n}")
        return 0

    print_summary(report)
    print_pii_findings(report)
    print_transitional_findings(report)
    return 0


if __name__ == "__main__":
    sys.exit(main())
