# FIX-584: Restore the Hosted Managed Answer Contract

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The hosted v0.1.104 desktop could reach the managed completion service but a
normal answer request returned HTTP 400 before provider execution. The desktop
then displayed a generic failure instead of a useful recovery path.

## Root Cause

The desktop's managed `system` value contains Bluey's own fixed security
contract. The server's internal-disclosure guard scanned that entire trusted
contract as though it were caller-authored text. A disclosure-protection phrase
inside Bluey's own contract therefore matched the guard and blocked every
otherwise valid managed answer.

The contract was also duplicated between the daemon and server boundary, with
no versioned source of truth. Fixing only the current string would have broken
signed v0.1.97 through v0.1.101 clients that use earlier exact wire contracts.

## Fix Summary

- Move the current and historical signed-release managed contracts into the
  shared `cue-core` prompt-contract module.
- Accept only an exact known contract, optionally followed by the exact answer
  rule separator. Unknown, fuzzy, empty-tail, or forged variants fail closed.
- Exclude the recognized fixed contract from disclosure scanning, but continue
  scanning every caller-controlled field and every appended answer rule.
- Give internal Jobs callers an explicit trusted-system authority while still
  treating user text, context, images, identifiers, and other request fields as
  untrusted.
- Map only the allowlisted `internal_disclosure_blocked` reason to a bounded
  desktop error and a safe user-facing recovery message. Arbitrary server bodies
  remain hidden.
- Avoid sending the duplicate General-mode instruction block on managed routes;
  retain direct/BYOK instructions, explicit modes, and session-specific rules.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-core/src/prompt_contracts.rs` | Own exact current and historical managed wire contracts and separator. |
| `server/src/api/router.rs` | Separate trusted contract validation from scanning of untrusted fields. |
| `server/src/api/router/completion.rs` | Require explicit system-authority classification. |
| `server/src/api/router/streaming_completion.rs` | Apply external or trusted-server authority at each completion entry point. |
| `server/src/api/router/tests.rs` | Cover all supported contracts, benign rules, forged tails, and disclosure attempts. |
| `server/tests/integration_e2e.rs` | Exercise current contracts and a legacy signed-release contract over HTTP. |
| `crates/cue-cloud-client/src/error.rs` | Add a bounded typed disclosure-block error. |
| `crates/cue-cloud-client/src/client.rs` | Parse only the allowlisted reason for buffered and streaming requests. |
| `crates/cue-llm/src/bluey_managed.rs` | Preserve the bounded reason through the managed LLM adapter. |
| `crates/cue-daemon/src/app.rs` | Share the wire contract, compact managed rules, and surface safe recovery copy. |
| `crates/cue-daemon/src/rag_indexer.rs` | Handle the new exhaustive cloud error variant without leaking detail. |
| `CHANGELOG.md` | Record the contract, error, latency-path, and overlay changes. |

## Edge Cases Handled

- Exact v0.1.97-v0.1.98, v0.1.99-v0.1.101, and current v0.1.102+
  signed-release contracts remain accepted.
- A bare supported contract and supported contract plus benign session rules
  validate.
- Disclosure-bearing answer-rule tails, including confusable-character forms,
  are rejected.
- Unknown contracts, extra trusted-looking text, and empty answer-rule tails are
  rejected instead of being prefix-trusted.
- Internal Jobs prompts can use their server-owned system contract, but
  caller-controlled user and context fields cannot bypass the guard.
- Streaming and non-streaming clients receive the same bounded typed error.
- Managed General mode removes only the redundant rules. Code, System Design,
  Meeting, Writing, direct/BYOK, and explicit session rules remain intact.

## How to Test

```bash
cargo test -p cue-core prompt_contracts::tests
(cd server && cargo test every_supported_release_contract_is_accepted)
(cd server && cargo test every_supported_contract_accepts_benign_rules)
(cd server && cargo test --test integration_e2e \
  router_complete_accepts_exact_legacy_signed_release_contract)
cargo test -p cue-cloud-client
cargo test -p cue-llm
cargo test -p cue-daemon managed_general_mode_omits_duplicate_rules
```

The focused prompt-contract, server compatibility, client, LLM, and daemon
tests passed in this worktree. The complete root workspace and server test
suites, strict Clippy for all targets, both formatting checks, and
`git diff --check` also pass on the final source diff.

## Known Limitations

- This is source readiness only. It has not been merged, deployed, or included
  in a customer release.
- The prompt reduction removes duplicate local instructions, but live
  post-deployment time-to-first-token must be measured separately before making
  a latency claim.
- Physical Windows and packaged-release verification remain separate release
  gates.
- An unsupported or malformed managed contract currently fails closed using
  the disclosure-block reason. A future bounded `managed_contract_unsupported`
  reason could give an old or skewed client more precise update guidance
  without exposing server text.
