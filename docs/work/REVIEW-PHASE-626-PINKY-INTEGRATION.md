# REVIEW: Phase 626 — preparation checkpoint

Load `bluey-ops` and `pinky-ops` before continuation/review.

Date: 2026-10-08. Reviewers: parent and independent `isolation_review` agent.

## Verdict

I0 offline preparation accepted; I1 hosted integration remains HOLD. This is
not a finished product integration or merge/deploy approval. The owner lifted
the original usage cutoff after the interrupted checkpoint.

## Verification observed

- Bluey `git diff --check`: passed.
- `bash scripts/check-bluey-ops-docs.sh`: passed.
- Dirty owner checkouts were preserved; edits are in separate worktrees.
- No deploy, service restart, DNS change, database migration, real payment,
  production push or Bluey Jobs change occurred.

## Pinky offline tool — final verification

Parent and independent reviewer reran the stable post-fix source:

```text
python3 -B scripts/ops/test_bluey_integration_profile.py
Ran 16 tests — OK
python3 -B scripts/ops/check-bluey-integration-profile.py
PASS
git diff --check
clean
```

Malformed under-cap JSON (5,000-digit integer and 1,100-level nesting) returns
exit 1, stdout `HOLD`, empty stderr, without caller-path traceback. Profile
reads are bounded to 32 KiB + 1; role URLs/service/storage reference bindings
are exact. Initial role/parser failures were fixed, not relabeled as passes.
No retained bytecode/build output was found.

Final SHA-256 pins:

| Pinky path | SHA-256 |
| --- | --- |
| `deploy/bluey-integration.profile.json` | `4f27ecc0c02a33eb2cd5cbeab4cae00c8f200b7263e0e873958f8792c483090b` |
| `scripts/ops/check-bluey-integration-profile.py` | `96dd3f192db19349bb85aab0a831641715ade98e284ce94bcff2f9af6c923932` |
| `scripts/ops/test_bluey_integration_profile.py` | `0ff8c681f7366a02aea38db1de235dbab5bc038f7927114a17b5f09789cf877f` |

## Not verified

The interrupted draft was unverified; the final offline slice passed the
checks above. No full
product build, live authentication, model latency, billing, native Mac/Windows
interaction or independent-resource verification is claimed. Existing Pinky
preprod was not changed or tested by this preparation task.
Symbolic secret references cannot prove independent secret material. A profile
PASS proves no live-resource or release-admission property. Pinky's current
runbook preflight script is absent from the stable preparation base; porting
the newer deployment guard closure is required, not silently waived.

Read-only host capacity evidence: Bluey's documented host returned 0 available
bytes / 100% use. No integration deployment is safe there without separate
capacity work. This task did not perform cleanup.

## Next bounded action

Reconcile the active
Pinky team's accepted runtime base and infrastructure identities. Follow I1–I7
in `docs/rounds/PHASE-626-PINKY-INTEGRATION-PREPROD.md` without bypassing gates.
