# Round 635 — Jobs current Rust qualification

Date: 2026-10-27. Preflight: `bluey-ops`, current local handoff and shared
machine policy. Base: `863042a0`. Status: bounded local qualification accepted;
privacy and separate AI review passed. PostgreSQL30/30 and all-target Rust
1,845/1,845 passed; the aggregate exited0 and its owned root was removed.

## Acceptance

1. Reuse the canonical launcher, `mac-build` lock and existing private PG17/
   pgvector wrapper. No owner/default URL, home, provider credential or flag.
2. Rerun the exact30-case configured selection in IMPL629, each in a fresh DB;
   do not run global authority fixtures in one shared database. Discover each
   test explicitly, reject empty/overlapping selections, require clean PG stop.
3. Reuse only that owned compiler root/source caches after PG shutdown. For full
   Rust, use private credential HOME/CARGO_HOME, SQLite primary, no PostgreSQL
   URL/require flag and an environment allowlist. Configured-only tests in this
   phase are not counted again as real PG evidence.
4. Run server fmt, all-target warnings-denied Clippy and all-target tests with
   both integration/simulator support features. Preserve the existing optimized
   curve25519/ed25519/sha2 test dependency profile from the qualified baseline;
   do not loosen a test, timeout, invariant or production setting to pass.
5. Freeze every product/test/manifest source from command start to terminal.
   Fix only a reproduced defect after terminal, with FIX/implementation/review
   and a fresh relevant rerun. No edits based on stale buffered output.
6. Require current-source identity, actual exit/counts, exact PG/root cleanup,
   privacy and independent bounded review. No deployment/release inference.

No production account, credential, application/email/calendar, push/PR/merge,
registry/signing, paid resource, desktop/overlay/Pinky change or VM startup.
Overall Jobs production/parity goal remains active and unfinished.

## Separate infrastructure evidence limit

IMPL629 records a host DNS listener despite Colima `port-forwarder=none`.
Phase634 proved its native image/container checks, but did not capture VM host
listeners while alive. Its container network-none/read-only inspection remains
valid; VM host-listener isolation is not inferred. No VM is currently running.
Before reusing that VM harness, qualify an explicit supported host-listener
policy. A read-only official-source audit found VZ management itself uses one
loopback TCP SSH forwarder even with vsock; literal zero host listeners is not
an honest acceptance criterion. A successor must reject public/DNS/application
listeners, explicitly inventory any permitted loopback management listener, and
prove owned hostagent/driver/usernet PIDs and listeners absent before unlinking
private state. Configuration flags alone cannot establish that proof. No source
edit or VM retry is allowed during this Rust source freeze.

## Separate application invariant finding

Read-only tracing found the active-company unique constraint binds account plus
normalized employer display name, not the signed canonical employer identity.
Existing coverage blocks punctuation/legal-suffix aliases across Tracks/emails,
but different normalized display names with the same verified employer can
produce distinct reservation keys. This is static evidence, not a reproduced
application or a claim that clients can forge signed postings. A bounded
successor needs canonical identity binding, legacy handling, both database
constraints and exact packet/pre-submit rechecks; do not silently change this
frozen qualification or claim that its passing tests close that separate gap.
