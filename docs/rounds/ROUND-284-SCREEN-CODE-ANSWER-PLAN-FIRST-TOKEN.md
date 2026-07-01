# Round 284 - Screen Code Answer Plan First Token

## Trigger

The owner showed a live overlay case where a screen-context coding problem did not answer cleanly on the first attempt. Bluey showed the generic provider-error fallback, then later answered slowly and did not clearly treat the screen as a coding prompt.

## Root Cause

- The native overlay was sending the generic prompt `Answer using the attached screen capture, documents, and current session context.`
- The server `AnswerPlan` saw the word `documents` and could classify the request as `missing_context`, even when an image/screen capture was actually attached.
- The planner mostly looked at the visible question text and did not strongly inspect the appended screen/session context for code-shaped content.
- Production logs showed vision routing could start with the slower Gemini Pro preview path, hit 429/503 capacity errors, and only fall back after a long delay.
- The streaming first-token timeout guarded a connected stream, but there was no separate route-connect deadline for providers that were slow before the stream was usable.

## Fix

- Added planning-context extraction so the server can inspect appended `Session context`, `Screen context`, `Document context`, and attached-context blocks without leaking plaintext into logs.
- Added coding-signal detection from screen/session context. A generic screen prompt plus code-shaped screen context now plans as `coding` with `code_artifact`.
- Kept image-bearing requests on the `vision` lane while allowing the answer plan to classify them as coding, so the model still receives visual evidence.
- Made the generic screen-capture prompt stop requesting documents unless real document context is present.
- Fixed missing-context logic so attached images or planning context count as evidence instead of causing a false missing-context answer.
- Added privacy-safe diagnostics for answer planning: context character count, context hash, and a boolean `context_coding_signal`.
- Added stream route-connect deadlines:
  - `BLUEY_STREAM_ROUTE_CONNECT_TIMEOUT_MS`, default `12000`
  - `BLUEY_STREAM_DEEP_ROUTE_CONNECT_TIMEOUT_MS`, default `45000`
- Adjusted provider-mix vision routing so Gemini Flash and OpenAI accurate routes are preferred before the slower Gemini Pro preview fallback.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo test --manifest-path server/Cargo.toml answer_plan --quiet`
- `cargo test --manifest-path server/Cargo.toml provider_mix_keeps_vision_on_image_capable_routes --quiet`
- `cargo test --manifest-path server/Cargo.toml stream_route_connect_deadline --quiet`
- `cargo check --manifest-path server/Cargo.toml --quiet`

## Current State

- Code fix is implemented in the server planner/router and provider dispatcher.
- Production droplet deploy is complete.
- Previous production binary backup:
  `/var/backups/bluey-api/bin/bluey-server.previous-20260701T210318Z`
- Installed production binary SHA256:
  `87020cba27eabb004640920b1e2725322d7a2c377944fa807e2f11571a3c8192`
- `bluey-api.service` restarted active.
- `https://bluey.sh/health` returned `status=ok`.
- This is server-only; no Mac/Windows desktop parity change is required for this round.

## Remaining QA

- After deployment, live screen-coding smoke should show:
  - `answer_intent=coding`
  - `answer_output=code_artifact`
  - `context_coding_signal=true`
  - no false `missing_context` plan when images are attached
- If a vision provider stalls or returns 429/503 before a usable stream, Bluey should fall through to the next route faster.
