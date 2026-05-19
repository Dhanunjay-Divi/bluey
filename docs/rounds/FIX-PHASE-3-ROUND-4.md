# FIX-PHASE-3-ROUND-4: Overlay Supervisor Crash-Loop Cap + Clean Shutdown

## Issue

Codex review of Phase 3 Round 4 returned 🔴 REQUEST CHANGES. Among the 6
blockers identified across R4+R5, **Blocker 2** (overlay supervisor
crash-loop cap and shutdown lifecycle) is logically an R4 carryover: the
overlay supervisor was first introduced in commit `19ff43a` during R4, but
lacked proper cap-exhaustion behavior and clean shutdown of long-running
children.

Codex's finding: the supervisor reset `consecutive_failures` on successful
spawn (before the child had actually run), meaning a child that spawned
successfully but immediately exited non-zero would never exhaust the restart
cap. Additionally, `shutdown()` did not drop stdin, leaving long-running
children blocked on reads indefinitely.

## Root Cause

`crates/cue-daemon/src/overlay.rs` — the `run_supervisor` loop:

1. **Cap reset bug**: `consecutive_failures` was reset to 0 after a
   successful `Command::new(...).spawn()`. Since spawn succeeds even for a
   child that immediately crashes, the counter never accumulated past 1.
   The `MAX_RESTART_ATTEMPTS` guard was effectively dead code.

2. **Shutdown hang**: When `send_rx.recv()` returned `None` (channel closed
   by `shutdown()`), the supervisor awaited `child.wait()` without first
   dropping the child's stdin handle. Children that block on stdin (the
   expected behavior for overlay processes reading NDJSON) would never exit,
   causing the shutdown to hang until the 2s timeout.

## Fix Summary

Commit `02b2cb7` addresses both issues:

**Cap-exhaustion path:**
- Removed the erroneous `consecutive_failures = 0` reset on successful
  spawn. The counter now correctly accumulates across consecutive non-zero
  child exits until `MAX_RESTART_ATTEMPTS` (5) is exceeded.
- After cap exhaustion, pending `send_rx` messages are drained and
  `recv_tx` is dropped so downstream readers see channel close immediately.
- Supervisor state transitions to `Failed` and stays there.

**Shutdown path:**
- When `send_rx.recv()` returns `None` (channel closed by `shutdown()`),
  stdin is immediately dropped so the child sees EOF and exits cleanly.
- When `shutdown_requested` atomic is set, stdin is also dropped before
  awaiting `child.wait()` to ensure the child is not blocked on reads.
- Shutdown timeout increased from 2s to 5s for robustness with slow
  children.

**New test infrastructure:**
- `overlay_stub_crash` binary: exits immediately with code 1 (simulates
  repeated crashes for cap-exhaustion testing).
- `overlay_stub_long` binary: reads stdin in a loop, writes Pong+Echo per
  message, exits cleanly on EOF (simulates a well-behaved long-running
  overlay for shutdown testing).

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/overlay.rs` | Remove spurious `consecutive_failures` reset; drop stdin on shutdown/channel-close; increase timeout to 5s |
| `crates/cue-daemon/src/bin/overlay_stub_crash.rs` | New — exits immediately with code 1 |
| `crates/cue-daemon/src/bin/overlay_stub_long.rs` | New — stdin loop with Pong+Echo, clean exit on EOF |
| `crates/cue-daemon/tests/overlay_lifecycle.rs` | New — 2 integration tests for cap exhaustion + clean shutdown |
| `crates/cue-daemon/Cargo.toml` | Add `[[bin]]` entries for new stubs |

## Edge Cases Handled

- **Rapid consecutive crashes** (child exits non-zero immediately after
  spawn): counter accumulates correctly; after 5 crashes the supervisor
  enters `Failed` state and `send()` returns `Err`.
- **Long-running child during shutdown**: stdin drop causes EOF → child
  exits cleanly → `child.wait()` resolves within timeout.
- **Channel close race**: if `shutdown()` is called while a message write
  is in-flight, the write completes (or fails on broken pipe) before the
  supervisor observes the closed channel on the next iteration.
- **Spawn failure during restart**: treated as a crash (counter increments),
  supervisor retries until cap exhaustion.

## How to Test

```bash
cargo test -p cue-daemon --test overlay_lifecycle
```

Expected: 2 tests pass:
- `overlay_supervisor_caps_at_max_restart_attempts` — verifies state reaches
  `Failed` and stays there; `send()` fails after exhaustion.
- `overlay_long_running_child_shuts_down_cleanly` — sends 10 messages,
  receives all responses, then `shutdown()` completes within 3s.

Full pipeline verification:

```bash
cargo fmt --all --check                     # ✅
cargo clippy --all-targets -- -D warnings   # ✅
cargo test --all-targets                    # ✅ 164 pass
```

## Commits Applied

| Hash | Message |
|------|---------|
| `02b2cb7` | `fix(daemon): overlay supervisor crash-loop cap + clean shutdown + tests [P3.R5 fix]` |

## Known Limitations

- The overlay supervisor does not yet implement graceful drain of in-flight
  IPC messages on cap exhaustion — messages queued in `send_rx` at the
  moment of exhaustion are dropped (logged at warn level). This is
  acceptable for the alpha since the overlay will be restarted by the user.
- The 5s shutdown timeout is generous but not configurable. A future round
  may expose this as a setting if needed.

## References

- Original R4 overlay supervisor: commit `19ff43a`
- R4 handoff: `docs/work/PHASE-3-ROUND-4-HANDOFF-FOR-CODEX-REVIEW.md`
- R5 blockers 1, 3, 4, 5, 6: see `docs/work/FIX-PHASE-3-ROUND-5.md`
