Branch:     feat/phase-3-round-12
Tip before: 81eecdc
Tip after:  <this commit>

# Observability Phase 2 — daemon + dashboard log rotation

## What changed

| Area | Files | Summary |
|---|---|---|
| Shared local logging | `crates/cue-core/src/logging.rs`, `crates/cue-core/src/lib.rs` | Added a reusable JSONL tracing initializer with daily `tracing-appender` files, standard `component` / `version` / `platform` fields, `BLUEY_LOG_DIR` / `CUE_LOG_DIR` override support, and 7-file retention. |
| Dependencies | `Cargo.toml`, `Cargo.lock`, `crates/cue-core/Cargo.toml` | Added `tracing-appender` and let `cue-core` own the shared subscriber/appender helper. |
| Daemon wiring | `crates/cue-daemon/src/app.rs` | Replaced stderr-only tracing init with `cue_core::init_local_json_logging("cue-daemon", ...)`. |
| Dashboard wiring | `crates/cue-dashboard/src/lib.rs` | Initializes the same local JSON logger before Tauri setup. |
| Tests | `crates/cue-core/src/logging.rs` | Added tests for support-tool-compatible prefixes, retention pruning, and an end-to-end JSON log write smoke. |

## Why

Phase 4 already added `bluey doctor` and `bluey logs export --redact`, but before Phase 2 there were no persistent daemon/dashboard log files for those tools to bundle. This round makes local support diagnostics durable without touching CLI command shape or Kiro-owned doctor/export code.

## Verification

Commands run before handoff:

```bash
cargo fmt --all
cargo test -p cue-core logging::tests -- --nocapture
cargo check -p cue-core
cargo check -p cue-daemon
cargo check -p cue-dashboard
RUST_LOG=info BLUEY_LOG_DIR="$(mktemp -d)" timeout 2 ./target/debug/bluey-daemon --no-overlay
```

Observed:

- `cue-core::logging` tests pass.
- `cue-core`, `cue-daemon`, and `cue-dashboard` compile.
- Daemon smoke created `daemon-log.2026-05-22.log` under `BLUEY_LOG_DIR`.
- The daemon smoke log contains JSON lines with `component`, `version`, `platform`, `level`, `target`, and `message`.

Full round-tip gate:

```bash
cargo fmt --all --check                              ✅
cargo clippy --all-targets -- -D warnings            ✅
cargo test --all-targets                             ✅
cd server && cargo clippy --all-targets -- -D warnings ✅
cd server && cargo test                              ✅ 90 passed
cd crates/cue-dashboard/ui && npm test -- --run      ✅ 15 passed
cd crates/cue-dashboard/ui && npm run build          ✅
git diff --check                                     ✅
```

## Areas most likely wrong

1. `tracing-appender` names daily files as `{prefix}.{YYYY-MM-DD}.log`, so the actual daemon path is `daemon-log.2026-05-22.log`, not exactly `daemon-2026-05-22.log`. This still matches Phase 4's `daemon-*` filter because it starts with `daemon-`.
2. `RUST_LOG` is honored. If the environment is `RUST_LOG=warn`, startup `info` events will not be written. That is intentional operator override behavior, but worth calling out during smoke tests.
3. The shared logger swallows subscriber-install failures and falls back to stderr if file setup fails, so Bluey startup is not blocked by a bad log directory. This is product-friendly, but it means a broken log directory is visible as an `eprintln!`, not a returned app error.

## Honest limitations

- No Tauri invoke trace propagation yet; that is Observability Phase 5.
- No frontend JS error capture or overlay lifecycle emits yet; that is Observability Phase 3.
- CLI `doctor` / `logs export` code was intentionally not edited because Kiro is touching `crates/cue-cli` in parallel.
- Swift overlay `os.log` / lifecycle work is not part of this phase.
