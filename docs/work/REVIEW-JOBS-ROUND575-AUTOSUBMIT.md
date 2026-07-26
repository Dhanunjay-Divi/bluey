# REVIEW: Jobs Round 575 - Track Auto-submit Authority And Form Read-back

> Codex preflight: load the `bluey-ops` skill, then verify its memory against
> the current repository state and task-specific docs.

**Commit range:** `origin/main..feature branch`
**Reviewer:** Codex self-review
**Date:** 2026-07-26

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

```bash
cargo fmt --all -- --check                 # passed
cargo clippy --all-targets -- -D warnings  # passed
cargo test --lib --quiet                   # passed
Jobs workspace tests                       # passed
Jobs workspace typecheck                   # passed
Portal production build                    # passed
privacy/schema/provenance/boundary gates   # passed
responsive visual QA                       # passed
```

## Overall Verdict

🟢 **ACCEPT** - Ready for branch review. Production runner distribution must
remain disabled until its separate certification gate passes.

## Follow-ups for Next Batch

- Provider certification, durable runner recovery, and physical packaged
  Browser verification.
