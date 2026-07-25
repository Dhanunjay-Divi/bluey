# REVIEW: [Batch ID] — [Title]

> Codex preflight: load the `bluey-ops` skill, then verify its memory against
> the current repository state and task-specific docs.

**Commit range:** `abc1234..def5678`
**Reviewer:** [name/agent]
**Date:** YYYY-MM-DD

## Per-Task Review

### [Task ID] — [Title]

| Field | Value |
|-------|-------|
| Files | |
| Verdict | 🟢 accept / 🟡 minor nit / 🔴 blocker |

**Findings:**
-

---

## Cross-Task Findings

<!-- Issues that span multiple tasks: inconsistencies, missing integration, etc. -->
-

## Build & Test Verification

```bash
cargo fmt --check   # ✅ / ❌
cargo clippy        # ✅ / ❌
cargo build         # ✅ / ❌
cargo test          # ✅ / ❌ (X passed, Y failed)
```

## Overall Verdict

<!-- Pick one: -->
🟢 **ACCEPT** — Ready to merge.
🟡 **ACCEPT WITH NITS** — Merge after addressing minor items.
🔴 **REQUEST CHANGES** — Blockers must be resolved.

## Follow-ups for Next Batch

-
