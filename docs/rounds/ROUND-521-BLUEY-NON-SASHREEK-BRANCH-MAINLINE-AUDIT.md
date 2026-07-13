# Round 521 - Bluey non-Sashreek branch mainline audit

Date: 2026-07-13

Status: complete; no launch-ready non-Sashreek patch is missing from `origin/main`

## Objective

Re-audit every surviving Bluey branch and worktree after the Round 519 release,
excluding Sashreek-owned branches, and confirm that the web UI, Jobs, answer-quality,
runtime, security, and release work represented by Codex branches is present on the
mainline without reintroducing obsolete implementations.

This round is a source reconciliation audit. It does not rebuild or redeploy the API,
Jobs service, web bundle, Caddy configuration, or signed native `0.1.100` artifacts.

## Mainline source of truth

- Checked-out release branch: `codex/bluey-interrupted-asks-round519-20260712`
- `HEAD`: `10c8214cf7ad20ea54711bb77f8f5cce55b06f45`
- `origin/main`: `10c8214cf7ad20ea54711bb77f8f5cce55b06f45`
- Worktree status before this document: clean
- Deployed server source remains `f18e0deae01581bceb2e0af35894d0a95306a28a`
- Signed native release remains `0.1.100`

## Explicit exclusions

The following Sashreek-owned refs were inspected only to identify and exclude them.
They were not merged, cherry-picked, reset, edited, or deployed:

- `origin/agent/agent-bridge`
- `origin/agent/agent-bridge-fixes`
- `origin/agent/meeting-frontend`
- `origin/agent/parakeet-stt`
- `origin/meeting-main`

## Audit method

The audit used both graph ancestry and stable patch equivalence. A branch that is not
a graph ancestor is not necessarily missing from main: rebases, squashes, documentation
renumbering, and release reconciliation can produce different commit IDs for the same
change. For each non-merged Codex branch, the decisive check was:

```bash
git log --right-only --cherry-pick origin/main...<branch>
```

Any remaining patch-unique commit was then compared to the current implementation by
behavior and tests. Obsolete historical branches were not force-merged merely to make
the ancestry graph green.

## Fully represented branches

### Web UI

`origin/codex/bluey-web-ui-parallel-20260704` at
`34767e87e51e6312c277b7cc51087e5306ed5392` has zero patch-unique commits and is a
strict ancestor of main. The landing, account, billing, balance, session history,
theme, authentication, download, and legal-page work from that branch is represented
on main.

### Jobs

`origin/codex/bluey-jobs-20260710` at
`a5467d9443ccbbe461e45f43db380448d0e503e6` is not shown as graph-merged because the
final release history was reconciled, but it has zero patch-unique commits against
main. The reviewed Jobs portal/API work and its production documentation are already
represented. Re-merging this branch would add no product code.

### Release reconciliation

`origin/codex/bluey-branch-reconciliation-20260712` at
`53031e35264d0e575280ad642b4f502f0068f3cd` also has zero patch-unique commits. Its
release and edge evidence is represented by Rounds 517-520 and the current mainline.

### Earlier Codex and Phase 3 work

All other non-Sashreek Codex, `feat/phase-3-*`, and `fix-*` branches are either graph
ancestors of main or patch-equivalent to current commits. This includes transcript,
local STT, routing, streaming, updater, disguise, overlay hardening, distribution,
trial, latency, and AI-site work.

## Semantically superseded legacy patches

Seven old May patches retain different patch IDs because their implementations were
rewritten or expanded. Their required behavior is present in current main:

| Legacy commit | Historical behavior | Current source of truth |
| --- | --- | --- |
| `b6ebaf2a` | Obfuscated provider endpoints and auth headers | `crates/cue-llm/src/{openai,anthropic,ollama}.rs` and `crates/cue-daemon/src/stt/{openai,deepgram}.rs` |
| `c1d15bbe` | Platform anti-debug checks | `crates/cue-stealth/src/{linux,windows,macos}.rs` and `lib.rs` |
| `04ab90d5`, `e3926b6c` | Local RAG crate and indexing | `crates/cue-rag`, `crates/cue-daemon/src/rag_indexer.rs`, and `crates/cue-daemon/src/db/rag.rs` |
| `446dbc7c` | User-rebindable keybindings | dashboard command registration, native shortcut handling, and web shortcut surfaces |
| `7e71d85e` | Safe Windows JSON type extraction | `native/windows/cue-overlay/json_type_extract.h` and overlay protocol tests |
| `986d473e` | Overlay event state machine and field limits | `crates/cue-core/src/overlay_ipc.rs` and `crates/cue-daemon/src/overlay.rs` |

These patches must not be cherry-picked over the newer implementations.

## Stream-attachments branch disposition

`origin/codex/bluey-stream-attachments-20260704` at
`fbe30c2669a31f77a811637eb3370edd6dc79897` has 11 historical patch-unique commits.
They are not 11 missing launch patches:

- Coding follow-up recovery and disclosure handling were replaced by the current
  router classifiers, continuity logic, and Round 519 disclosure guards.
- Try Us, 15-minute trial, 24-hour temporary access, and abuse controls are present in
  the current web and server authentication/trial paths.
- Device-code connection, account ownership, and shared-balance behavior are present
  in the current web, server, and daemon paths with newer fail-closed hardening.
- Balance identity and refresh behavior is present in the current account and daemon
  balance implementation.
- macOS click-through, scroll, screen-follow-up, and attachment-during-stream behavior
  was reimplemented or superseded by the current native and Round 519 runtime paths.
- The branch's `0.1.91` release bump is obsolete; production is signed `0.1.100`.

The preserved commit set is `bf0d6508`, `afdb1342`, `d1a5c573`, `b8a2d65b`,
`69a4c0d1`, `a6bbc460`, `d9110edc`, `c2619f81`, `ec783e18`, `668ce615`, and
`fbe30c26`.

A wholesale merge would regress later trial enforcement, device registration,
provider selection, and release metadata. The branch therefore remains intentionally
unmerged while its required behavior is represented on main.

## Preserved auxiliary worktrees

- `/Users/uno/Downloads/cue-answerplan-fix` has an old locally checked-out `main` and
  preserved dirty work. It was not reset or used to move the canonical main pointer.
- `/Users/uno/Downloads/cue-bluey-jobs` has preserved concurrent Jobs experiments on
  an older source base. Its reviewed committed production patches are represented on
  main; its dirty files are not safe to merge wholesale.
- `/Users/uno/Downloads/cue-runtime-stream-attachments` preserves the historical
  stream branch for reference. It was not modified.
- Temporary release and reconciliation worktrees were treated as evidence sources,
  not independent mainline candidates.

## Remaining non-merged remote refs

After excluding Sashreek refs, `git branch -r --no-merged origin/main` contains only:

- `origin/codex/bluey-jobs-20260710` - zero patch-unique commits
- `origin/codex/bluey-branch-reconciliation-20260712` - zero patch-unique commits
- `origin/codex/bluey-stream-attachments-20260704` - semantically reconciled and unsafe
  to merge wholesale

This is an expected topology state, not a product-code discrepancy.

## Outcome

The canonical remote mainline contains all reviewed, launch-ready non-Sashreek Bluey
work from the UI, Jobs, answer-quality, runtime, security, and release streams. No safe
commit remains to merge or cherry-pick. Forcing the three remaining Codex branch tips
into main would either add no semantic change or restore obsolete code.

Future experimental work must continue as a fresh, focused port onto current main with
its own tests and release gate. The protected Sashreek branches remain untouched.
