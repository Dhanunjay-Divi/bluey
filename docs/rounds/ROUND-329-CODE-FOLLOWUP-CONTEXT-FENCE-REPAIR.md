# Round 329 - Code Follow-Up Context Fence Repair

Date: 2026-07-04
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## Trigger

The owner showed session `6CC7D7A4` where Bluey handled a coding challenge poorly:

- the first Alice/Bob coding answer was short and the canvas code was malformed
- the follow-up `Can you give me Python code?` said the rest of the problem statement was missing even though the prior turn had it
- the next follow-up errored with `Ref: 61B3DA69`

The expected behavior is that short follow-ups such as `give me Python code` retain the prior coding problem, produce complete code, and preserve line notes/complexity cleanly in the canvas.

## Root Cause

- The visible session was created on daemon `0.1.64`, before the Round 328 coding-answer-shape release.
- A desktop follow-up heuristic still treated short language-only code requests as standalone when the question terms were only generic words such as `python`.
- Because the prior problem statement was dropped, the model believed it did not have enough context and asked for the prompt again.
- Some streamed model output used malformed Markdown fences like ````pythonfrom typing import List` and inline closing fences like `return ...````. The daemon and server artifact parsers did not recover that shape, so code artifacts could lose the first line, keep closing fences inside code, or drop line notes.
- The desktop code artifact formatter preserved complexity but not line notes/notes when it built the artifact locally.

## Fix

- Desktop daemon:
  - keeps recent Bluey Q&A for short code regeneration follow-ups when all topic terms are generic code/language terms
  - still skips recent Q&A for specific new code topics such as `Write Fibonacci code in Python`
  - repairs malformed fenced code openers with inline code after the language tag
  - strips inline closing fences out of the code body
  - preserves `LINE NOTES`, `COMPLEXITY`, and `NOTES` as separate code canvas sections
  - treats `LINE NOTES` as a metadata boundary when extracting runnable code
- Managed server:
  - applies the same malformed fence repair to server-side response artifacts
  - keeps line notes outside copied code
- Bumped desktop workspace version from `0.1.66` to `0.1.67`.

Mac/Windows parity: the daemon changes are shared desktop code, so both macOS and Windows get the same follow-up/context/canvas behavior once packaged for each platform. The server parser fix applies to all clients.

## Verification

Passed locally:

```bash
cargo fmt --check
cargo test -p cue-daemon answer_overlay_artifact_repairs_malformed_python_fence -- --nocapture
cargo test -p cue-daemon meeting_context_keeps_recent_qa_for_short_code_regeneration_follow_up -- --nocapture
cargo test -p cue-daemon meeting_context_skips_recent_qa_for_specific_new_code_topic -- --nocapture
cargo test -p cue-daemon answer_overlay_artifact_detects_fenced_code -- --nocapture
cargo test -p bluey-server response_artifact_repairs_malformed_python_fence -- --nocapture
cargo test -p bluey-server response_artifact_detects_code -- --nocapture
cargo test -p bluey-server answer_plan_code_request_uses_deep_code_artifact -- --nocapture
cargo check -p cue-daemon
cargo check -p bluey-server
```

## Deployment

Completed.

- Commit: `b026fa7d15d15a7ee4d9059024e7f3366cdc3d55`
- API host: `root@165.227.77.152`
- API build directory: `/opt/bluey-build-codex-round329-code-fence`
- API binary backup before replacement:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260704T054425Z`
- API binary SHA256:
  `571e4fce6cc5ef06806b7a4c633cab2bb7c149a51e9c5eff7da94b35467810c5`
- API service after restart:
  `ActiveState=active`, `SubState=running`, `MainPID=1104546`, `NRestarts=0`
- Public health:
  `https://bluey.sh/health` returned commit `b026fa7d15d15a7ee4d9059024e7f3366cdc3d55`.
- Remote build note:
  the droplet's default `/usr/bin/cargo` was too old for the current dependency graph, so the release build used `/root/.cargo/bin/cargo` / `rustc 1.96.1`.

Desktop release:

- Version: `0.1.67`
- Public manifest: `https://bluey.sh/latest.json`
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.67/bluey-0.1.67-darwin-arm64.tar.gz`
- Darwin arm64 SHA256:
  `bea923de2af8c317513ddb04400be81fbada0bca02d1ecb83fd888fdee89351c`
- Artifact size: `9185460` bytes
- Publish verification passed:
  - release artifact dev-flag/secret scan
  - `latest.json` signature verification
  - `install.sh` MIME check: `application/x-shellscript`
  - `install.ps1` MIME check: `application/x-powershell`
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` report `0.1.67`
- Local public install completed from `https://bluey.sh/install.sh`.
- Local binaries:
  - `/Users/uno/.bluey/bin/bluey --version` -> `bluey 0.1.67`
  - `/Users/uno/.bluey/bin/bluey-daemon --version` -> `bluey-daemon 0.1.67`
  - `/usr/local/bin/bluey` and `/Users/uno/.local/bin/bluey` both point at `/Users/uno/.bluey/bin/bluey`
- Local daemon restart completed:
  - pid `30961`
  - fresh session `817ccef3-05a1-4f63-a6e2-58ab6914e39e`
  - `overlay_visible=true`
  - `overlay_capture_excluded=true` in normal production capture-excluded mode

## Remaining QA / Gates

- Retest the Alice/Bob prompt and follow-up in a fresh session, not the old `0.1.64` session.
