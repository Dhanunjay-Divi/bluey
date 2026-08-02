# REVIEW: Jobs Round 586 - Track Auto-submit Authority And Form Read-back

> Codex preflight: load the `bluey-ops` skill, then verify its memory against
> the current repository state and task-specific docs.

**Commit range:** `origin/main..feature branch`
**Reviewer:** Codex self-review
**Date:** 2026-08-02

## Per-Task Review

### Track-scoped Auto-submit authorization

| Field | Value |
|-------|-------|
| Files | Jobs migrations, server persistence/API/authority, portal Settings and Matches |
| Verdict | 🟢 accept |

**Findings:**

- Authorization is account- and Track-scoped, revisioned, and single-active.
- The server owns the authority fingerprint and never trusts the portal to
  decide whether it is current.
- Identity, source resume, policy, and confirmed-fact changes fail closed.

### Frozen execution admission

| Field | Value |
|-------|-------|
| Files | `server/src/api/jobs.rs`, `server/src/db/jobs/execution_authority.rs` |
| Verdict | 🟢 accept |

**Findings:**

- Review approval and Track Auto-submit are distinct admissions.
- Admission is covered by the packet checksum.
- Legacy Auto-submit cannot bypass the new authority.
- The irreversible Submit path revalidates current authority.

### Employer form read-back

| Field | Value |
|-------|-------|
| Files | Jobs automation read-back module, shared adapters, Greenhouse, Lever |
| Verdict | 🟢 accept |

**Findings:**

- Fresh browser state is read after filling and immediately before Submit.
- Text/select/boolean/file controls are verified.
- Silent ATS rejection becomes a blocking intervention.
- Error strings do not include candidate values or local document paths.

## Cross-Task Findings

- This batch creates the authority and form-integrity prerequisites for
  unattended submission. It deliberately does not make disabled or uncertified
  runners appear available.
- The useful `career-ops` principle was adapted cleanly; Bluey source remains
  original and its provenance boundary is documented.

## Build & Test Verification

```text
Jobs workspace: 489 tests passed
  automation 198
  browser 100
  runner 50
  workflows 54
  portal 87

Server library: passed
Server Jobs HTTP integration: 16 passed, 61 filtered
Rust fmt and strict Clippy: passed
Jobs typecheck and production build: passed
Privacy, schema parity, provenance/license, client boundary, CI guard: passed
Responsive visual QA: desktop and mobile passed
```

Dependency review patched the reachable production advisories. The remaining
React Router advisory applies to React Server Component action execution; this
portal is a client-only Vite SPA with no RSC or server-action endpoint. This is
an accepted, documented residual rather than a hidden green audit.

## Overall Verdict

🟢 **ACCEPT** - The authority and read-back layer is ready to merge. Production
runner distribution must remain disabled until its separate certification gate
passes.

## Follow-ups for Next Batch

- Provider certification, durable runner recovery, and physical packaged
  Browser verification.
