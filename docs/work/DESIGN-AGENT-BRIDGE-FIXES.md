# Design contract — agent-bridge research fixes

> Phase-2 output of workflow wf_3f3c0007-739. Binding contract the two implementers coded against.
> Built on the live-verified facts in VERIFY-AGENT-BRIDGE-FIXES.md.

## Descoped (not shipped, with reason)

- Antigravity(2.0) Replay→NativeResume tier flip — REFUTED by live verification: agy's CLI store (~/.gemini/antigravity-cli, SQLite index) is disjoint from the desktop store (~/.gemini/antigravity, proto index) that Bluey's AntigravityIndex reader and data_dir_globs target; every id Bluey can list today hard-fails `agy --conversation <id>` with 'trajectory not found' (exit 1), and agy parses as PlainText so no session id is ever captured for chaining. Flipping the tier would convert working replay continuations into hard errors with zero chaining benefit. Follow-up if wanted: teach the sessions layer the antigravity-cli store, or capture the minted id from agy's cli-*.log, then revisit.
- Adding 'trajectory not found' to continuation/recoverable.rs — moot while Antigravity stays Replay (resume is never attempted), and adding an unreachable match phrase would be dead code under the no-dead-code rule.
- Claude Code effort flag — the research doc's `claude --effort low..max` claim is REFUTED on the installed claude 2.0.42 (full --help inspected: no effort/thinking/reasoning flag headless); wiring it would break every Claude drive. Claude stays prose-only; revisit version-gated if a future CLI ships the flag.
- Gemini and agy effort flags — none exist (live --help); prose-only.
- Cursor effort flag — effort embeds inside the --model bracket syntax (`--model 'id[effort=high,...]'`), so a speed control would couple to and overwrite the user's model choice; left prose-only in v1.
- Overlay/frontend UI for the model picker and for sending `model` on attach — out of scope this round by instruction (daemon/IPC wiring only). Follow-up: an overlay model chooser feeding the extended AgentAttachRequested{model} event (the dormant AskOpts.provider/model type plumbing in cue-meeting-overlay/ui exists but nothing renders a picker, and overlay-sent provider/model is currently discarded on the agent route at app.rs:6296).
- A native ACP model/effort parameter — the ACP schema has no such field; instead of extending the protocol adapter, the contract forces the CLI route whenever explicit overrides are present (bridge B2 gate). Follow-up: per-bridge ACP model params if vendors add them.
- Fallback_models entries for Cursor/Copilot/Antigravity — model id lists are release/plan-dependent and churn (Cursor ids are versioned; Copilot's list is plan-gated and not enumerable from --help; agy values are backend-fetched display labels), so hardcoding fallbacks is unsafe; the model_flag stays inert until the user picks a model.
- Tolerating Cursor's empty-result turns (successful resume turns can return result:"" when the answer surfaces only as redacted reasoning; app.rs:7481 then shows the offline card) — pre-existing behavior of the Cursor JSON path, not introduced by this change; noted as a known limitation for a separate fix.
- Ledger compaction/rotation and read-side caching — read-time keep-latest dedup only at MVP (~150 bytes per new session id; multi-year growth is single-digit MB); revisit only if file size ever becomes measurable.
- Ledgering headless/CLI ask paths — spawns with no OverlayAnswerStream never reach the persist block (no daemon handle), so they are not ledgered; the store re-scrape fallback covers them. The ledger is an optimization layer, not a replacement.
- Carrying a pending model through the BYOT billing-disclosure re-attach round-trip — would ripple a field through BillingDisclosureResponded for cloud rows that have no model_flag anyway; the re-run passes model None.
- Fixing the stale ACP id-space comment at cue-daemon/src/app.rs:7129-7133 — explicitly out of scope per the ledger recon (belongs to the true-resume follow-up).
- Editing docs/AGENT-MODEL-SPEED-CONTROL.md to correct its refuted Claude --effort row and Cursor cwd claims — the doc is the research input, not part of either implementer's file ownership this round; flag for the build agent/doc owner.

## Binding contract (exact API signatures / registry changes / formats)

BINDING CONTRACT — agent model/speed control + session ledger (branch agent/agent-bridge-fixes)
Both implementers code against these signatures blind. Anything not listed here keeps its current signature.

=== A. FILE OWNERSHIP (disjoint by crate) ===
- BRIDGE implementer: crates/cue-agent-bridge/** ONLY.
- DAEMON implementer: crates/cue-daemon/** and crates/cue-core/** ONLY.
- NEITHER touches: CHANGELOG.md (build agent writes it), docs/reviews/**, crates/cue-cli/** (verified: no caller of drive_with_overrides there), crates/cue-daemon/src/ledger.rs and crates/cue-core/src/ledger.rs (untracked in-flight DECISIONS-ledger files from a parallel effort — do not rename, edit, or `mod`-register them), the stale ACP-id-space comment at cue-daemon/src/app.rs:7129-7133 (out of scope, leave verbatim), dev-live-view/, docs/LEDGER-PLAN.md, docs/TRANSCRIPTION-PRODUCTION-PLAN.md and all other dirty/untracked files from the parallel STT effort.
- NO new dependencies in any Cargo.toml. NO git add/commit/push/branch by either implementer.
- Build/test commands (both): `cargo fmt --check`, `cargo clippy --target aarch64-apple-darwin -- -D warnings`, `cargo test --target aarch64-apple-darwin -p <crate>`; for cue-daemon prefix `PKG_CONFIG_PATH=/opt/homebrew/opt/openblas/lib/pkgconfig`.
- Sequencing: the daemon code calls new bridge API, so cue-daemon will NOT compile until the bridge half exists. Implement in parallel against this contract; the build agent compiles the union. Signatures below are therefore FROZEN.

=== B. BRIDGE PUBLIC API (new/changed) ===

B1. crates/cue-agent-bridge/src/drive/mod.rs — new struct (re-export from lib.rs as `pub use drive::DriveOverrides;`):
```rust
/// Per-run CLI overrides the daemon threads into a local-CLI drive. Exact argv
/// tokens, data-driven off the registry (`model_flag` / `effort_args`) — the
/// drive layer never names a model or a flag itself.
#[derive(Debug, Clone, Default)]
pub struct DriveOverrides {
    /// e.g. ["--model", "composer-2.5"] or ["-m", "gpt-5.1-codex"]. Empty = none.
    pub model_args: Vec<String>,
    /// e.g. ["--effort", "high"] or ["-c", "model_reasoning_effort=\"high\""]. Empty = none.
    pub effort_args: Vec<String>,
}
impl DriveOverrides {
    #[must_use]
    pub fn is_empty(&self) -> bool { self.model_args.is_empty() && self.effort_args.is_empty() }
}
```

B2. crates/cue-agent-bridge/src/lib.rs — CHANGED signature (sole external caller is cue-daemon/src/app.rs:7166):
```rust
pub async fn drive_with_overrides(
    agent: AgentKind,
    question: Question,
    overrides: DriveOverrides,
) -> anyhow::Result<AnswerStream>
```
Routing rule (BINDING): the ACP branch is taken only when `should_use_acp(&agent) && overrides.is_empty()`. Non-empty overrides FORCE the cloud-check-then-CLI path (ACP has no model/effort parameter; an explicit user/resolver pick must never be a silent no-op — this also makes the ModelBlocked retry deterministic instead of ACP-then-fallback). When overrides are empty, behavior is byte-identical to today. CLI branch builds `DriveOptions { model_override: overrides.model_args, effort_override: overrides.effort_args, ..Default::default() }`.

B3. crates/cue-agent-bridge/src/drive/cli.rs — DriveOptions gains one field:
```rust
pub struct DriveOptions {
    /* existing: timeout, max_output_bytes, mode, model_override, cwd */
    /// Per-run effort/reasoning-depth argv tokens (registry `effort_args` row),
    /// appended immediately AFTER `model_override`. Empty (default) = nothing.
    pub effort_override: Vec<String>,
}
```
Default impl adds `effort_override: Vec::new()`. In `drive_with_options`, append `opts.effort_override` tokens right after the existing `model_override` append loop (cli.rs:586-588). prove_drive.rs and all `..Default::default()` construction sites are unaffected.

B4. crates/cue-agent-bridge/src/registry.rs — AgentEntry gains TWO fields (every row in REGISTRY must set them):
```rust
/// Per-run reasoning-effort args by overlay speed tier. `None` = this CLI has
/// no live-verified per-run effort control (prose instructions only).
pub effort_args: Option<EffortArgs>,
/// When true, NativeResume by id is only safe for sessions Bluey ITSELF minted
/// via the CLI (recorded in the spawn-time session ledger). Ids listed from the
/// agent's GUI/IDE store are NOT proven resumable; apply_tier degrades those to
/// Replay. Live-verified for Cursor: CLI-minted id + original cwd resumes; a
/// wrong-cwd or unknown id SILENTLY mints an empty session (exit 0) — no
/// error-based degrade can catch it, so provenance must gate the resume.
pub resume_requires_ledger: bool,
```
```rust
/// Argv tokens per overlay speed tier ("balanced" always sends nothing = the CLI default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffortArgs {
    pub fast: &'static [&'static str],
    pub deep: &'static [&'static str],
}
```
New pure accessor (registry.rs):
```rust
/// Per-run effort argv for an agent + overlay speed tier ("fast"|"balanced"|"deep",
/// case-insensitive, trimmed). Balanced/unknown/no-row → empty. Data-driven.
#[must_use]
pub fn effort_args_for(agent: &AgentKind, speed: &str) -> Vec<String>
```

B5. Registry ROW VALUES (binding):
- Cursor (KindTag::Cursor): `model_flag: Some("--model")`, `continuation: ContinuationTier::NativeResume`, `resume_requires_ledger: true`, `effort_args: None`, `fallback_models: &[]` (unchanged).
- Copilot (KindTag::Copilot): `model_flag: Some("--model")`, `effort_args: Some(EffortArgs { fast: &["--effort", "low"], deep: &["--effort", "high"] })`, `resume_requires_ledger: false`, `fallback_models: &[]`, continuation unchanged (NativeResume).
- Antigravity (KindTag::Antigravity): `model_flag: Some("--model")`, `continuation: ContinuationTier::Replay` (UNCHANGED — flip descoped), `resume_requires_ledger: false`, `effort_args: None`.
- Codex (KindTag::Codex): `effort_args: Some(EffortArgs { fast: &["-c", "model_reasoning_effort=\"low\""], deep: &["-c", "model_reasoning_effort=\"high\""] })` — CONDITIONAL on the mandatory live smoke (bridge task B8); if the key is rejected, set `effort_args: None` with a comment. Note the value tokens carry literal TOML quotes, matching the existing answer_args style (`sandbox_mode=\"read-only\"`).
- ALL other rows (ClaudeCode, ClaudeCodeApp, ClaudeCodeAgent, AntigravityIde, Gemini, Aider, Windsurf, VsCode): `effort_args: None`, `resume_requires_ledger: false`; no other field changes.

B6. Session ledger — NEW module crates/cue-agent-bridge/src/sessions/ledger.rs (register `pub mod ledger;` in sessions/mod.rs). NO cue-core dependency (bridge stays cue-core-free); only std + serde/serde_json/anyhow (already deps).
```rust
/// File name of the spawn-time agent-session ledger, created under the daemon's
/// data dir. Deliberately named agent-session-* to disambiguate from the
/// unrelated meeting DECISIONS ledger.
pub const AGENT_SESSION_LEDGER_FILE: &str = "agent-session-ledger.jsonl";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionLedgerRecord {
    pub agent: AgentKind,          // serde snake_case, same string as settings.attached_agent
    pub session_id: String,
    pub cwd: Option<String>,       // the EFFECTIVE drive cwd
    pub spawned_at: u64,           // unix millis
}
impl SessionLedgerRecord {
    /// Stamps spawned_at from SystemTime (0 on a pre-epoch clock; never panics).
    #[must_use]
    pub fn new(agent: AgentKind, session_id: impl Into<String>, cwd: Option<String>) -> Self
}
/// O_APPEND create-if-missing; one serde_json line + '\n' in a single write_all. No fsync
/// (the ledger is a resume optimization; a lost tail line degrades to the store re-scrape).
pub fn append(path: &std::path::Path, record: &SessionLedgerRecord) -> anyhow::Result<()>
/// Missing file → empty. Malformed lines skipped (fail-soft, same invariant as every
/// SessionReader). Dedup on (agent, session_id) keeping the LAST occurrence.
#[must_use]
pub fn read_all(path: &std::path::Path) -> Vec<SessionLedgerRecord>
#[must_use]
pub fn lookup(path: &std::path::Path, agent: &AgentKind, session_id: &str) -> Option<SessionLedgerRecord>
```

B7. crates/cue-agent-bridge/src/continuation/tier.rs — CHANGED signatures (update the re-exports in continuation/mod.rs: add ResolvedSession):
```rust
/// Result of resolving a pinned session: its project cwd, transcript, and
/// whether the cwd came from the spawn-time ledger (provenance for
/// `resume_requires_ledger` agents).
#[derive(Debug, Clone, Default)]
pub struct ResolvedSession {
    pub project: Option<String>,
    pub transcript: Option<Transcript>,
    pub ledger_hit: bool,
}

pub async fn resolve_session(
    agent: &AgentKind,
    session_id: &str,
    tier: ContinuationTier,
    list_cap: usize,
    ledger_path: Option<&std::path::Path>,
) -> ResolvedSession

pub async fn apply_tier<F, Fut>(
    question: &mut Question,
    agent: &AgentKind,
    session_id: Option<&str>,
    via_acp: bool,
    list_cap: usize,
    ledger_path: Option<&std::path::Path>,
    summarize: F,
) where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Option<String>>,
```
resolve_session semantics (BINDING): (1) ledger lookup runs FIRST, before discover_agents, so an undiscovered agent can still resolve a ledger cwd; ledger record with Some(cwd) → project = that cwd, ledger_hit = true, and the reader.list() project re-scrape is SKIPPED; (2) ledger miss / record without cwd / ledger_path None → today's reader.list path, ledger_hit = false; (3) the transcript read (reader.read, fork-fallback safety net) is ALWAYS still attempted via store resolution, fail-soft. ledger_path = None must be byte-identical to today's behavior.
apply_tier semantics (BINDING): compute `let effective_tier = if entry.continuation == NativeResume && entry.resume_requires_ledger && !resolved.ledger_hit { Replay } else { entry.continuation };` and match on effective_tier. The existing cwd_usable existence/non-empty check STAYS and runs on the resolved project regardless of source (a ledger cwd can go stale; the degrade-to-replay arms depend on it).

=== C. DAEMON/CORE DATA SHAPES (new/changed) ===

C1. cue-core/src/config.rs CueSettings — new field (after attached_session):
```rust
/// Agent bridge: per-run model override for the attached agent (a vendor model
/// id, e.g. "composer-2.5" / "gpt-5.1-codex"). Applied via the registry row's
/// `model_flag`; a no-op for agents with no model flag. Cleared on detach.
#[serde(default)]
pub attached_model: Option<String>,
```
Default impl adds `attached_model: None`.

C2. cue-core/src/overlay.rs OverlayEvent::AgentAttachRequested — new optional field (back-compat: old payloads without it must still deserialize):
```rust
AgentAttachRequested {
    kind: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    model: Option<String>,
},
```
Update every match arm in cue-core and cue-daemon.

C3. cue-core/src/ai.rs — new field on BOTH structs:
```rust
// AnswerRequest:
#[serde(default)]
pub speed: Option<String>,   // "fast" | "balanced" | "deep" when the overlay speed picker set the mode
// ProviderRequestPayload: same field; from_request copies `request.speed.clone()`.
```
AnswerRequest::new initializes `speed: None`.

C4. cue-daemon/src/app.rs DriveOutcome — new field:
```rust
struct DriveOutcome { body: String, cost_usd: Option<f64>, session_id: Option<String>,
    /// The EFFECTIVE cwd the drive ran in: question.cwd after apply_continuation_tier,
    /// falling back to std::env::current_dir() (the inherited cwd). Recorded in the
    /// spawn-time session ledger so cwd-scoped resume never depends on store drift.
    cwd: Option<String>,
}
```

C5. Daemon-internal signature changes (daemon-owned, listed for completeness): `persist_attached_agent(daemon, agent: Option<String>, session: Option<String>, model: Option<String>)`; `handle_agent_attach(daemon, kind: &str, session_id: Option<&str>, model: Option<&str>)`; `resolve_answer_route(request, meeting, resume_session: Option<&str>, attached_model: Option<&str>, stream)`; `answer_with_agent(provider, payload, _meeting, resume_session: Option<&str>, attached_model: Option<&str>, stream, fallback_depth)`; `drive_answer_attempt(kind, label, payload, resume, model_override: &[String], effort_override: &[String], stream)`; `apply_continuation_tier(question, agent, session_id, via_acp, ledger_path: Option<std::path::PathBuf>)`.

=== D. CROSS-CUTTING SEMANTIC RULES (BINDING) ===
D1. Model seed: in answer_with_agent, before the retry loop (current app.rs:7356), when `attached_model` is Some(non-blank) AND `cue_agent_bridge::model_resolve::model_flag_for(&kind)` is Some(flag): `model_override = vec![flag.to_string(), chosen.to_string()]` AND `tried_models.push(chosen.to_string())` (so the ModelBlocked resolver never re-proposes the user's blocked pick and advances to fallbacks/BYOT). model_flag None → honest no-op (debug log, no error).
D2. Effort: computed once in answer_with_agent as `payload.speed.as_deref().map(|s| cue_agent_bridge::registry::effort_args_for(&kind, s)).unwrap_or_default()`; constant across retries. Prose speed instructions (mode_instructions fast/balanced/deep) REMAIN for ALL agents — effort args are additive, never a replacement.
D3. Drive call: `cue_agent_bridge::drive_with_overrides(kind.clone(), question, DriveOverrides { model_args: model_override.to_vec(), effort_args: effort_override.to_vec() })`.
D4. Ledger write point: inside the existing conversation-chaining persist block (app.rs:7499-7524), inside the `still_attached && attached_session != Some(new_id)` branch (changed-id gate = write-side dedup), regardless of persist_attached_agent success: append `SessionLedgerRecord::new(kind.clone(), new_id.clone(), outcome_cwd.clone())` to `daemon.paths.data_dir.join(cue_agent_bridge::sessions::ledger::AGENT_SESSION_LEDGER_FILE)`; on Err, warn!-and-continue (never fail the answer).
D5. Ledger read point: drive_answer_attempt computes `ledger_path = stream.as_ref().map(|s| s.daemon.paths.data_dir.join(AGENT_SESSION_LEDGER_FILE))` and passes it into apply_continuation_tier → apply_tier. stream None (headless) → None → today's behavior.
D6. Effective cwd capture: in drive_answer_attempt, AFTER apply_continuation_tier and BEFORE the Question is moved into drive_with_overrides: `let effective_cwd = question.cwd.clone().or_else(|| std::env::current_dir().ok().and_then(|p| p.to_str().map(String::from)));` — returned as DriveOutcome.cwd.
D7. Detach clears attached_model (persist_attached_agent(daemon, None, None, None)); the chaining persist PRESERVES it (pass settings.attached_model.clone()); the BYOT-disclosure re-run of handle_agent_attach passes model None (cloud rows have no model_flag; acceptable).

## Bridge implementer tasks (cue-agent-bridge)

All work in crates/cue-agent-bridge/** only. Implement per the contract; signatures are frozen.

B1. registry.rs — mechanism: add `pub struct EffortArgs { pub fast: &'static [&'static str], pub deep: &'static [&'static str] }` (Debug, Clone, Copy, PartialEq, Eq); add `pub effort_args: Option<EffortArgs>` and `pub resume_requires_ledger: bool` to AgentEntry (contract B4 doc comments); add `pub fn effort_args_for(agent: &AgentKind, speed: &str) -> Vec<String>` — trim + ascii-lowercase the tier, "fast" → row.fast, "deep" → row.deep, anything else (incl. "balanced") or no row/None → Vec::new(). Set the two new fields on EVERY row (defaults per contract B5).

B2. registry.rs — row edits (contract B5 values):
- Cursor: model_flag None → Some("--model"). Row comment: LIVE-VERIFIED 2026-07-03 — `cursor-agent --model composer-2.5` succeeded and the session blob recorded providerOptions.cursor.modelName="composer-2.5" (the JSON output does NOT echo the model); valid ids via `cursor-agent models` / --list-models; ids churn per release so fallback_models stays empty ("auto" is the only stable value). Continuation Replay → NativeResume + resume_requires_ledger: true. Row comment: `--resume=<id>` LIVE-VERIFIED (codeword PLUM recalled) but ONLY for CLI-minted ids from the session's original cwd — the store is ~/.cursor/chats/<md5(cwd)>/<id>/ (meta.json carries cwd); resume from another cwd, or of an id not in that store, exits 0 and SILENTLY mints an empty session — hence the ledger gate; GUI/vscdb ids stay replay.
- Copilot: model_flag None → Some("--model"). Comment: LIVE-VERIFIED 2026-07-03 — `copilot -p "reply with exactly OK" --model auto -s` → "OK", exit 0 (copilot 1.0.64, node 24); only "auto" is help-documented, model list is plan-dependent → fallback_models stays empty. effort_args per contract (`--effort` low/high; help-verified choices none|low|medium|high|xhigh|max).
- Antigravity: model_flag None → Some("--model"). Comment: LIVE-VERIFIED 2026-07-03 — `agy --model "Gemini 3.5 Flash (Low)" -p` OK; the CLI log shows the override propagated. Values are the exact DISPLAY LABELS from `agy models` (spaces + parens, one argv entry); backend-fetched, drift-prone → fallback_models stays empty; flag is version-gated (agy ≥ 1.0.5). Continuation stays Replay — update the row comment to the live refutation: `agy --conversation` only reaches ~/.gemini/antigravity-cli store conversations; every id Bluey's AntigravityIndex reader lists is from the ~/.gemini/antigravity desktop store and fails live with "trajectory not found" (exit 1), and the PlainText parser yields no session id, so NativeResume would hard-fail with nothing to chain.
- Codex: effort_args per contract B5, GATED on task B8's live smoke. Row comment cites the smoke result + that the `-c` mechanism is the same one answer_args already uses.

B3. Doc-comment updates for the tier flip:
- ContinuationTier enum docs (registry.rs ~256-271): add Cursor to the NativeResume list ("`--resume=<id>`, cwd-scoped AND ledger-gated — only Bluey-minted CLI sessions"); remove Cursor from the Replay list; rewrite the Antigravity Replay rationale to the live store-split refutation above.
- continuation/tier.rs module docs (lines ~6-11): NativeResume list gains "Cursor (ledger-gated)"; document the resume_requires_ledger degrade and the ledger-first cwd resolution.
- drive/cli.rs Antigravity resume_args comment (lines ~250-255): the claimed id equivalence is REFUTED for desktop-store ids (live 2026-07-03: `agy --conversation <desktop-store-id>` → "Error: failed to send message: trajectory not found", exit 1); only antigravity-cli-store conversation ids resume; Bluey currently lists only desktop-store ids, which is why the registry keeps Replay.
- Do NOT touch docs/AGENT-MODEL-SPEED-CONTROL.md beyond nothing — leave it (its Claude --effort claim is refuted but the doc is not yours to edit this round unless trivially annotating; skip it).

B4. drive layer: add DriveOverrides to drive/mod.rs + `pub use drive::DriveOverrides;` in lib.rs (contract B1); add DriveOptions.effort_override + Default + append-after-model_override in drive_with_options (contract B3); change drive_with_overrides to the contract B2 signature with the ACP gate `should_use_acp(&agent) && overrides.is_empty()` — when overrides are non-empty, skip ACP entirely (cloud check, then CLI with both override vecs); update the fn doc comment to state the forced-CLI rule and why (ACP has no model/effort param). The ACP branch's CLI fallback can now use DriveOptions::default() (overrides are empty on that branch by construction).

B5. sessions/ledger.rs: implement exactly the contract B6 API (const, record, ::new stamping unix-millis spawned_at fail-soft, append with OpenOptions create+append + single write_all of line+'\n', read_all with missing-file→empty + skip-malformed + keep-LAST dedup on (agent, session_id), lookup = read_all().find). Register `pub mod ledger;` in sessions/mod.rs. No fsync, no locking, no rotation (documented rationale in module docs: optimization layer; miss path = store re-scrape).

B6. continuation/tier.rs: implement ResolvedSession + the new resolve_session/apply_tier signatures and semantics from contract B7. Restructure resolve_session so the ledger lookup happens BEFORE discover_agents and its result survives the agent-not-discovered/no-store early paths (return ResolvedSession { project: ledger_cwd, transcript: None, ledger_hit: true } in that case). apply_tier computes effective_tier via the resume_requires_ledger && !ledger_hit degrade, keeps the cwd_usable guard and all existing arms otherwise byte-identical. Update continuation/mod.rs re-exports (add ResolvedSession).

B7. Tests (behavior-named, in the respective files):
- registry: test_cursor_row_is_ledger_gated_native_resume (continuation == NativeResume && resume_requires_ledger); test_model_flags_match_live_verified_clis (Cursor/Copilot/Antigravity == Some("--model"), Codex == Some("-m"), Gemini == Some("--model"), AntigravityIde/Windsurf/VsCode == None); test_effort_args_present_only_for_verified_clis (Copilot Some with exact tokens ["--effort","low"]/["--effort","high"]; Claude*/Gemini/Cursor/Antigravity None; Codex per B8 outcome); test_effort_args_for_maps_fast_and_deep_and_defaults_balanced_empty (fast/deep/balanced/"", "FAST " trimmed-case handling, Unknown agent → empty). Extend test_antigravity_ide_row_is_distinct_replay_only_surface if needed (IDE stays Replay) and fix ANY existing test asserting Cursor's old Replay tier (that behavior change is intended); the tier.rs continuation_bridge test is unaffected (Cursor has no continuation_via).
- ledger: test_ledger_append_then_read_round_trips_record; test_ledger_read_dedups_same_agent_session_keeping_latest; test_ledger_read_skips_corrupt_lines_and_returns_valid_ones; test_ledger_read_missing_file_returns_empty; test_ledger_lookup_distinguishes_same_session_id_across_agents (same id under ClaudeCode vs Codex).
- tier: test_resolve_session_ledger_hit_supplies_project_without_discovery (tempdir ledger with a record for AgentKind::Other("ledger-test") + Some(cwd) → project = cwd, ledger_hit = true, transcript None — deterministic on any machine because Other is never discovered); test_resolve_session_without_ledger_path_matches_existing_behavior (Other kind, ledger_path None → all-empty ResolvedSession); plus a unit test for the effective-tier degrade if you extract it as a pure helper (recommended: small pub(crate) fn + test_native_resume_degrades_to_replay_without_ledger_provenance).

B8. LIVE SMOKES (wrap in `timeout 180`, tiny prompts, ≤3 calls total):
- MANDATORY (gates the Codex effort row): `timeout 180 codex exec --skip-git-repo-check -c 'model_reasoning_effort="low"' "reply with exactly OK"`. Exit 0 + OK → keep the Codex effort_args row (comment: LIVE-VERIFIED <date>); any unknown-key/config error → effort_args: None + comment recording the failure.
- OPTIONAL (1 call, recommended): `timeout 180 copilot -p "reply with exactly OK" --effort low -s` under node 24 (~/.nvm/versions/node/v24.13.0/bin) to convert Copilot's effort row from help-verified to live-verified; annotate the row comment either way.

B9. Verify: cargo fmt; cargo clippy --target aarch64-apple-darwin -p cue-agent-bridge -- -D warnings; cargo test --target aarch64-apple-darwin -p cue-agent-bridge. No dead code, no .unwrap() outside tests, 100-char lines.

## Daemon implementer tasks (cue-daemon / cue-core)

All work in crates/cue-daemon/** and crates/cue-core/** only. You call new bridge API per the contract (it may not compile until the bridge half lands — code against the frozen signatures). Overlay/frontend UI is OUT of scope.

D1. cue-core/src/config.rs: add CueSettings.attached_model per contract C1 (+ Default). Extend the existing settings serde round-trip tests (the ones asserting attached_agent/attached_session at ~lines 225-241) with attached_model, plus a back-compat test that a settings JSON WITHOUT the field deserializes to None.

D2. cue-core/src/overlay.rs: add `#[serde(default)] model: Option<String>` to OverlayEvent::AgentAttachRequested per contract C2; update the match arms at ~lines 689/699 and any other exhaustive matches. Test: an AgentAttachRequested payload without "model" still deserializes (back-compat) and one with "model" carries it.

D3. cue-core/src/ai.rs: add `#[serde(default)] pub speed: Option<String>` to AnswerRequest (init None in ::new) and to ProviderRequestPayload; ProviderRequestPayload::from_request copies `speed: request.speed.clone()`. Doc comment: the overlay speed tier ("fast"|"balanced"|"deep") carried as DATA so the agent path can map it to per-run effort args; prose instructions still carry the same intent for flagless agents.

D4. cue-daemon/src/app.rs attach/persist wiring: change persist_attached_agent to the 4-arg contract C5 form (sets settings.attached_model = model); handle_agent_attach gains `model: Option<&str>` and passes it through (including the two-arg call inside the BYOT disclosure re-run at ~app.rs:3113 — pass None there, per contract D7); handle_agent_detach → persist_attached_agent(daemon, None, None, None); the conversation-chaining persist at ~app.rs:7510 passes settings.attached_model.clone() so chaining never clears the user's model; the AgentAttachRequested dispatch at ~app.rs:2445 forwards the new field.

D5. cue-daemon/src/app.rs answer_request_from_overlay (~8323): after the existing mode_instructions block, when the normalized mode is one of "fast"|"balanced"|"deep", set `request.speed = Some(<normalized>)`. Test: test_answer_request_from_overlay_sets_speed_only_for_speed_modes (fast/deep set it; "code"/None do not).

D6. cue-daemon/src/app.rs model-seed threading: in answer_with_provider_runtime (~6293) capture `attached_model = settings.attached_model.clone()` inside the existing attached-agent block (alongside resume_session); thread it as Option<&str> through resolve_answer_route (contract C5 signature; update its call site(s)) into answer_with_agent; in answer_with_agent, immediately after `let mut tried_models` (~7357), apply contract D1: seed model_override = vec![flag, chosen] via cue_agent_bridge::model_resolve::model_flag_for(&kind) and push chosen into tried_models; model_flag None → debug!-log no-op. Comment: the seed is overwritten by the ModelBlocked resolver on a 400 (fallback UX preserved) and forces the CLI route in the bridge (ACP has no model param).

D7. cue-daemon/src/app.rs effort + drive-call switch: in answer_with_agent compute `let effort_override: Vec<String> = payload.speed.as_deref().map(|s| cue_agent_bridge::registry::effort_args_for(&kind, s)).unwrap_or_default();` before the loop; drive_answer_attempt gains `effort_override: &[String]` (contract C5) and calls `cue_agent_bridge::drive_with_overrides(kind.clone(), question, cue_agent_bridge::DriveOverrides { model_args: model_override.to_vec(), effort_args: effort_override.to_vec() })` (contract D3) — this replaces the current Vec<String> third argument at ~7166. Prose mode_instructions stay untouched.

D8. cue-daemon/src/app.rs DriveOutcome.cwd: add the field per contract C4; in drive_answer_attempt capture effective_cwd per contract D6 (AFTER apply_continuation_tier, BEFORE the Question moves into the drive call) and return it in DriveOutcome; widen the loop break in answer_with_agent to `(outcome.body, outcome.cost_usd, outcome.session_id, outcome.cwd)`.

D9. cue-daemon/src/app.rs ledger wiring: apply_continuation_tier gains `ledger_path: Option<std::path::PathBuf>` and forwards `ledger_path.as_deref()` plus the new arg order into cue_agent_bridge::continuation::apply_tier (contract B7 signature); drive_answer_attempt computes ledger_path per contract D5 (stream.as_ref().map(|s| s.daemon.paths.data_dir.join(cue_agent_bridge::sessions::ledger::AGENT_SESSION_LEDGER_FILE)); None when stream is None — headless paths keep today's behavior). Write side per contract D4: inside the changed-id chaining branch (~7509-7521), build SessionLedgerRecord::new(kind.clone(), new_id.clone(), <DriveOutcome.cwd>) and cue_agent_bridge::sessions::ledger::append(...); warn!-and-continue on Err. The synchronous small-file append matches the existing precedent in that block (load_settings is already sync there). Do NOT touch the stale ACP comment at 7129-7133 and do NOT touch crates/cue-daemon/src/ledger.rs (unrelated in-flight decisions ledger).

D10. Tests + verify: extend/add daemon unit tests where pure logic exists — test_answer_card/speed test from D5, and (if drive_answer_attempt internals are not unit-testable) at minimum a test for the seed rule extracted as a pure helper (recommended: `fn seed_model_override(kind, attached_model) -> (Vec<String>, Option<String>)` returning the override + the tried-models entry, with tests: flagless agent → empty; Codex + model → ["-m", m]; blank → empty). Run: cargo fmt; PKG_CONFIG_PATH=/opt/homebrew/opt/openblas/lib/pkgconfig cargo clippy --target aarch64-apple-darwin -p cue-core -p cue-daemon -- -D warnings; same prefix for cargo test --target aarch64-apple-darwin -p cue-core -p cue-daemon. Expect cue-daemon to fail compile until the bridge half exists — verify cue-core standalone in the interim.
