# Codex → Kiro: Phase 3 Round 11 Recheck Handoff

## 1. R11 Verdict

🔴 **REQUEST CHANGES**

Review written to: `docs/work/REVIEW-PHASE-3-ROUND-11.md`

Blockers:
- The production overlay path is now mostly hardened, but Windows `emit_ask_event()` still omits the token. That breaks the Send/Enter ask path on Windows once token validation is active.
- The new validator over-gates `AttachRequested` and `InstructionsRequested`. Those are the idle-state button-click events that should open the attach/style workflows, but they are currently rejected unless the state is already `AttachOpen` / `InstructionsOpen`.
- `overlay_ui_state` is not yet a real shared state machine. The initial spawn passes a separate idle mutex to the reader, and the daemon field is not updated anywhere after initialization.

Resolved from the prior R11 review:
- Specialized LLM callbacks now emit delta chunks instead of cumulative text.
- `app.rs::spawn_overlay()` now calls the gated overlay path resolver, binary verifier, token env wiring, and line validator.
- Tokenless/mismatched events are rejected when the daemon has a session token.
- Field length and state validation are covered by new production-path validator tests.
- Most Windows overlay simple events now carry the token.

Nit:
- `Responses.tsx` still discards non-empty `partial_text` when `finished: true`. Current OpenAI/Anthropic terminal chunks are empty, so this is not a blocker, but it is worth hardening.

## 2. R10 Verdict Status

🔴 **Still blocked by the R11 recheck**

Good R10 fixes landed:
- Streaming auth header names are obfuscated.
- SwiftWhisper is pinned to `.exact("1.2.0")`.
- PCM16 decode uses `loadUnaligned`.

Still not closed:
- The streaming delta bug is fixed, but the stacked R10/R11 branch still has R11 overlay blockers. Keep R10 status tied to the stacked recheck until this fix wave is clean.

## 3. Older Pending Verdicts

- R7-fix-3 release workflow checkout/script blocker: 🟢 **ACCEPT** based on the checkout before manifest generation in `.github/workflows/release.yml`.
- R8 fix-wave: 🟡 **ACCEPT WITH NITS** from the prior review remains valid.
- R9 full feature review: not completed in this pass; I prioritized R11 because it is the current requested review and contains security-critical claims.

## 4. What I Implemented

No product code changes.

Documentation changes only:
- Appended the recheck section in `docs/work/REVIEW-PHASE-3-ROUND-11.md`.
- Overwrote this handoff with the R11 recheck verdict and current pending-status summary.

## 5. What I Skipped and Why

- Product naming/white-label/runtime wording: intentionally not revisited per user direction.
- R9 full review: skipped because R11 has merge-blocking production/security issues that should be fixed before further stacked review.
- Additional feature implementation: skipped because this was a review request, and the branch is not merge-ready.

## 6. Pipeline Status

Checks run locally on `feat/phase-3-round-11`:

```bash
cargo fmt --all --check                              # ✅
cargo test -p cue-daemon --test cue_streaming_integration
                                                       # ✅ 5 passed
cargo test -p cue-daemon --test overlay_production_path
                                                       # ✅ 14 passed
git diff --check 030a63c..HEAD                        # ✅
```

## 7. Next Action for Kiro

Do not merge R11 yet.

Recommended fix order:
1. Add `emit_token_field()` to Windows `emit_ask_event()` before closing the JSON object, and add a test/fixture for ask events specifically.
2. Change state gating so `AttachRequested` and `InstructionsRequested` are allowed from `Idle`; keep `AttachFilesRequested` and `InstructionsUpdated` gated to their modal states.
3. Either remove the unused daemon-level `overlay_ui_state` for now or wire a single shared state handle that the production reader actually observes.
4. Add production-path tests for `attach_requested` and `instructions_requested` accepted from idle, plus `ask_requested` accepted with a Windows-style token envelope.
5. Re-hand as R11 recheck #2.
