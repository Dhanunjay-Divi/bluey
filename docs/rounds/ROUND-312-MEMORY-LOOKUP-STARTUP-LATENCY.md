# ROUND-312 Memory Lookup Startup Latency

Date: 2026-07-02

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Branch: `codex/bluey-overlay-spacing-20260626`

## Goal

Fix the confusing and slow-feeling answer startup where Bluey showed saved/conversation memory status before streaming and then sometimes failed with a generic message.

User-reported Bluey id: `25594F6D`

Resolved local session id:

```text
25594f6d-4cc7-4315-b99b-017b567851ae
```

## Diagnosis

- Local `bluey status` mapped `25594F6D` to the active meeting id.
- `active-meeting.json` showed `transcript_segments: 0` and `context_items: 0`.
- The active session contained two successful managed answers and no persisted failed turn.
- Production logs for session `25594f6d-4cc7-4315-b99b-017b567851ae` did not show a server-side failed completion. They showed three completed managed-chat requests.
- The saved-memory lookup in those prod logs returned `rag_match_count=0` quickly. The slow-feeling state was mostly because the old overlay kept showing the memory status while provider routing/first-token startup was still pending.
- One DeepSeek Pro route took `22612 ms` to first streamed token and `29188 ms` total, which made the UI look stuck on the wrong status.
- The native managed-provider path always pushed `Checking conversation context` for every non-screen streaming answer, even before the server selected a route.
- The server also performed saved-context/RAG lookup for every normal request before planning/provider dispatch. This was budgeted, but still added delay and made the UI blame memory when the real failure could be provider capacity, auth, stream, billing, or another downstream issue.
- Failed answer cards did not include a short request ref, so a screenshot/session id was not enough to locate the failed provider call later.

## Changes

- Native overlay no longer pushes `Checking conversation context` before every non-screen managed request.
- Native local RAG lookup is now explicit/follow-up only:
  - skipped for standalone direct asks like code, fresh live-caption wrappers, and selected-attachment cases;
  - kept for explicit memory/context requests and clear follow-ups such as previous code/answer/design.
- Server RAG lookup is now explicit/follow-up only before completion:
  - skipped for normal standalone/direct questions and generic live-caption wrappers with their own planning context;
  - kept for explicit saved-memory/session-context requests and short follow-ups.
- Server status wording now says `Using relevant conversation context...` only when relevant context is actually attached. It no longer emits a misleading long-running `Checking conversation context...` status.
- Failed overlay answers now append a short request ref like:

```text
Ref: 25594F6D
```

That makes future screenshots traceable without exposing raw prompt/content in logs.

## Verification

```bash
cargo fmt --all
cargo test --manifest-path server/Cargo.toml memory_lookup -- --nocapture
cargo test --manifest-path server/Cargo.toml answer_plan_context_wording -- --nocapture
cargo test -p cue-daemon answer_memory_lookup -- --nocapture
cargo test -p cue-daemon answer_error_ref -- --nocapture
cargo build -p cue-daemon --bin bluey-daemon
cargo build --manifest-path server/Cargo.toml
```

## Deployment

Local hot install:

- Built `target/release/bluey-daemon`.
- Backed up the installed daemon to:
  `/Users/uno/.bluey/bin/bluey-daemon.backup-20260702194158`
- Installed the rebuilt daemon to:
  `/Users/uno/.bluey/bin/bluey-daemon`
- Installed daemon SHA256:
  `1bd0731e59557b03a934da86b2dabd492e37ac0e544c3d07631888157fa0a14c`
- Restarted Bluey with `bluey off` then `bluey on`.

Production server deploy:

- Source archive from commit:
  `e6baa4f1cf759cdb4886a387293852be6b12b149`
- Build directory:
  `/opt/bluey-build-codex-round312-memory`
- Updated the droplet build toolchain to Rust/Cargo `1.96.1` because Cargo `1.75.0` could not parse a dependency with `edition2024` metadata.
- Built:
  `/opt/bluey-build-codex-round312-memory/server/target/release/bluey-server`
- Installed production server SHA256:
  `1ad3b4414d7afa54d8d2e41fd3149e801cc59fa0ad5be82caf68ba0561e15d4f`
- Previous production binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260702T235807Z`
- `bluey-api.service` restarted active with `NRestarts=0`.
- `https://bluey.sh/health` returned `status=ok`.

Desktop release:

- Bumped the public desktop release to `0.1.53`.
- Built:
  `dist/bluey-0.1.53-darwin-arm64.tar.gz`
- Artifact SHA256:
  `e553d932d62bb1f18b8697c3f695865844a95683a00bbf9a7539acb5037bafb5`
- Published:
  `https://bluey.sh/releases/v0.1.53/bluey-0.1.53-darwin-arm64.tar.gz`
- Live `latest.json` reports version `0.1.53`.
- Live verifier passed:
  - `latest.json` signature verified
  - installer MIME types are correct
  - `darwin-arm64` artifact SHA verified
  - unpacked binaries report `0.1.53`

## Follow-Up

- Add a persisted failed-answer diagnostic row with request id, session id, sanitized error category, provider/lane, and timing, but no prompt/transcript text.
- Add a `bluey inspect <ref>` owner/dev command that summarizes the last failed request by short ref.
