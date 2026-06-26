# Round 114 - Live Listen STT Postgres Spend Guard Fix - 2026-06-22

## User-Visible Problem

Clicking `Listen` started capture briefly and then auto-stopped. The local macOS
audio helper was available, permissions were already granted, and account
credits were present, so this looked like a product/UI failure even though the
actual failure was in the managed STT setup path.

## Root Cause

Production `/stt/session` requests were returning `500 Internal Server Error`.
Server logs showed:

```text
stt session endpoint failed error=error serializing parameter 0
```

The failure came from the Postgres implementation of
`usage::bluey_spend_cents_in_window`. The STT spend guard query multiplied an
untyped bind parameter by an interval:

```sql
now() - ($1 * interval '1 hour')
```

The Postgres client could not infer/serialize that parameter reliably in the
production runtime path, so STT session reservation failed before Deepgram relay
startup.

## Code Change

- Updated `server/src/db/usage.rs` to cast the spend-window bind parameter:

```sql
now() - ($1::bigint * interval '1 hour')
```

This keeps the existing query shape but gives Postgres a concrete parameter type.

## Deployment

Production was patched from the local workspace into `/opt/bluey-build`, then
rebuilt with the droplet's current Rust toolchain:

```bash
/root/.cargo/bin/cargo test --manifest-path server/Cargo.toml bluey_spend_cents_in_window_sums_recent_provider_cost
/root/.cargo/bin/cargo check --manifest-path server/Cargo.toml
/root/.cargo/bin/cargo build --release --manifest-path server/Cargo.toml --bin bluey-server
install -m 0755 server/target/release/bluey-server /usr/local/bin/bluey-server
systemctl restart bluey-api.service
```

The service restarted active on `bluey.sh` with Postgres runtime enabled.

## Verification

- Local focused server test passed.
- Local `cargo check --manifest-path server/Cargo.toml` passed.
- Remote focused server test passed.
- Remote `cargo check --manifest-path server/Cargo.toml` passed.
- Remote release build completed and `bluey-api.service` restarted active.
- `https://bluey.sh/health` returned healthy.
- Local smoke after deploy:
  - `bluey audio start` stayed in `Capturing` after the old failure window.
  - `bluey audio status` reported real native macOS capture with
    `bluey-managed:deepgram/nova-3 live`.
  - Transcript segments were emitted.
  - Production logs showed `/stt/session` returning `200` and `/stt/relay`
    upgrading with `101`.
- The test capture was stopped after verification to avoid leaving paid STT
  running.

## Follow-Ups

- Fix the Postgres shutdown/drop path that can log a runtime nesting panic while
  the old server process exits during `systemctl restart`.
- Improve the daemon/overlay UX when managed STT session setup fails so Listen
  reports a clear account/server setup error instead of appearing to blink off.
