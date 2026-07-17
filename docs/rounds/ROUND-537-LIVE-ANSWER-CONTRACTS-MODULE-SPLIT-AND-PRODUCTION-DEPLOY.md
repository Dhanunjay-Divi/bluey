# Round 537 - Live answer contracts, module split, and production deploy

Status: implementation and local verification complete; production promotion in progress

## Outcome

This round closes the live-answer defects found by the Round 539 production
canary, splits the two largest touched source files along existing behavioral
boundaries, and restores a previously interrupted attachment-staging behavior
without reviving obsolete branch code.

The immutable runtime source is:

`d4a3d84c3d6dfbaf3adb8d7878dffe68e8183108`

That commit is on `origin/main`. Documentation-only descendants do not change
the deployed server binary.

## Product changes

- Strengthened answer routing and prompt contracts for:
  - large foreign-key migrations (`Q10`);
  - pre-production RAG evaluation plans (`Q27`);
  - feature-store point-in-time and online/offline consistency (`Q38`);
  - payment operation idempotency and ledger movement (`Q39`);
  - ambiguous payment timeouts (`Q40`);
  - revocable URL-shortener redirects and cache boundaries (`Q41`); and
  - director-level priority conflict alignment (`Q47`).
- Added adversarial semantic checks so negation, substring, qualification, and
  unsafe omission cases cannot receive false passes.
- Restored next-answer staging for context items added while an attachment
  picker, screen-answer handoff, or answer stream is active.

## Module boundaries

The refactor preserved behavior while reducing concentration in the two
largest touched files:

| Area | Result |
| --- | --- |
| Rust router core | `server/src/api/router.rs`: 11,484 lines |
| Rust response artifacts | `server/src/api/router/response_artifacts.rs`: 1,250 lines |
| Rust router tests | `server/src/api/router/tests.rs`: 4,589 lines |
| Python evaluator runner | `scripts/bluey-interview-eval.py`: 4,044 lines |
| Python payment contracts | `scripts/bluey_eval/payment_contracts.py`: 2,483 lines |
| Python system contracts | `scripts/bluey_eval/system_contracts.py`: 1,552 lines |

The Rust response-artifact module exports only the 13 symbols used across its
module boundary. Its other helpers remain private. The Python runner resolves
the extracted package from its own script directory, including when invoked
from another working directory.

## Interrupted-work reconciliation

The interrupted Round 519 branch was compared to `origin/main` by patch ID and
range diff. All three commits have exact equivalents on main:

| Interrupted commit | Main equivalent | Result |
| --- | --- | --- |
| `2897e011` | `d63f9f54` | Jobs packet and dispatch safeguards already present |
| `4037dbef` | `3c244736` | Typed managed-answer context already present |
| `4e1cb65d` | `6294f72a` | Jobs execution safety already present |

The older stream-attachments branch was also audited. Its answer fallback,
scroll routing, trials, and release work are present or superseded. One narrow
regression remained: newly added context items were not always staged for the
next answer during an active picker, screen handoff, or answer stream. This
round ports only that current-architecture behavior and its focused policy
test. No obsolete release, version, or prompt code was cherry-picked.

## Verification before promotion

- `cargo test --manifest-path server/Cargo.toml`
  - 506 library tests passed;
  - 75 integration tests passed;
  - all six focused integration/schema test binaries passed;
  - no failures.
- `cargo test --manifest-path server/Cargo.toml api::router`
  - 194 passed, 0 failed.
- `cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings`
  - passed.
- Rust formatting check for the three router modules
  - passed.
- `swift build --package-path native/macos/cue-overlay`
  - passed.
- `BLUEY_CONTEXT_STAGING_TESTS`
  - passed.
- Python compilation for the runner and both extracted contract modules
  - passed.
- Full 50-case evaluator dry run
  - prepared all 50 cases from seven local evidence files.
- Targeted `Q10,Q27,Q38,Q39,Q40,Q41,Q47` dry run
  - prepared all seven cases and passed evaluator startup self-checks.
- Replay of the saved Round 539 canary
  - continued to reject the known unsafe `Q10`, `Q39`, `Q41`, and `Q47`
    answers, proving the extraction did not weaken the acceptance gate.
- `git diff --check`
  - passed.

Independent alternate-model reviewers found no lost router code or tests, no
Python import regression, and no unmerged Round 519 work.

## Production plan and rollback

Planned server release ID:

`round540-d4a3d84c`

Promotion must use a release directory built from the exact immutable source
commit above. Immediately before switching the API binary:

1. require API, Jobs API, and Caddy to be active with zero restarts;
2. require sufficient root-disk headroom;
3. capture and validate a fresh PostgreSQL custom-format backup;
4. preserve the currently running API binary and its checksum in a timestamped
   rollback snapshot;
5. install the already-built release binary atomically; and
6. restart only `bluey-api.service`.

The Jobs binary and service must remain byte-identical and uninterrupted.
Rollback is to atomically restore the saved API binary and restart only the API
service. Database restore is not expected because this round has no migration.

## Live acceptance gate

After promotion:

1. require `/health` to report the exact runtime commit;
2. require API, Jobs API, and Caddy active with `NRestarts=0`;
3. require no warning-or-higher API journal entries caused by promotion;
4. run the 13-case production canary with no evaluator retries;
5. manually inspect every answer and require all deterministic gates to pass;
6. only then run the full 50-case production evaluation with no retries; and
7. record cost, latency, answer findings, hashes, backup, rollback snapshot,
   and service evidence below.

## Production evidence

Pending promotion and live acceptance. This section must be updated with
observed values; no deployment claim is made yet.
