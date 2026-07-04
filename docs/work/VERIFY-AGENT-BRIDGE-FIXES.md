# Verify results — agent-bridge research fixes (live-CLI verification)

> Phase-1 output of workflow wf_3f3c0007-739 (agent/agent-bridge-fixes).
> Each block is a verifier agent's structured result. Working tree was clean at capture (read-only phase).

## agent-a25bbb92a5e0bf198.jsonl — verdict: **partial**

**Summary:** Both fixes are live-verified as real, with one critical correction to the research. (1) MODEL FLAG: cursor-agent supports --model <id> per-run (help-documented; `cursor-agent models` lists valid ids); live smoke with --model composer-2.5 succeeded and the on-disk session blob records providerOptions.cursor.modelName="composer-2.5", proving the model was applied — setting registry.rs Cursor model_flag: Some("--model") is safe. (2) PER-ID RESUME: --resume=<uuid> works headlessly and recalled the PLUM codeword when run from the SAME cwd (recall proven in the stored reasoning trace), and the one-shot JSON result object carries session_id which cli.rs push_cursor_line (lines 1001-1009) already emits as AnswerChunk::Started{session_id} — so the Replay→NativeResume tier flip will chain ids end-to-end. HOWEVER, resume is cwd-SCOPED, not cwd-independent: the CLI store lives at ~/.cursor/chats/<md5(cwd)>/<id>/ and resuming from a different cwd ($HOME) silently created a NEW EMPTY session under md5($HOME) with the same id — no error, total context loss. The research's suggestion that tier.rs's cwd-degrade is over-conservative for Cursor is REFUTED: correct cwd resolution (available in the CLI store's meta.json "cwd" field) is mandatory for the tier flip to be safe.

**Facts:**

```json
{
  "model_flag": "--model <model> (space form; per cursor-agent --help; also --list-models and `cursor-agent models` subcommand to enumerate valid ids)",
  "model_smoke_ok": true,
  "valid_model_used": "composer-2.5 (confirmed applied via store.db blob providerOptions.cursor.modelName; note: the CLI's JSON output does NOT echo the model \u2014 verification required reading ~/.cursor/chats/<md5(cwd)>/<id>/store.db)",
  "resume_codeword_ok": true,
  "resume_codeword_detail": "In-cwd --resume=<id> recalled PLUM \u2014 proven by the stored reasoning ('they asked me to remember the codeword \"PLUM\" ... which is PLUM'), though that turn's JSON result field was \"\" because the final text surfaced only as redacted reasoning ([REDACTED] in the transcript jsonl); a later resume turn returned normal non-empty result text",
  "session_id_source": "The one-shot JSON result object itself: {type:\"result\",...,\"session_id\":\"<uuid>\"} \u2014 no store scrape needed; on disk the session lives at ~/.cursor/chats/<md5(cwd)>/<uuid>/{meta.json,store.db} (meta.json carries the original cwd) plus a transcript mirror at ~/.cursor/projects/<slugified-cwd>/agent-transcripts/<uuid>.jsonl",
  "cwd_independent": false,
  "cwd_detail": "Resume from $HOME with the same id succeeded (exit 0, same session_id echoed) but had NO prior context and created a fresh empty store at ~/.cursor/chats/b8e31af6b739962aafc2c0f605700e01/<id> (= md5($HOME)); tier.rs's set-cwd-from-store + degrade-to-replay-when-cwd-unusable is REQUIRED for Cursor, not over-conservative",
  "parser_emits_id": true,
  "parser_evidence": "crates/cue-agent-bridge/src/drive/cli.rs push_cursor_line, lines 1001-1009: `if !self.started { self.started = true; self.session_id = v.get(\"session_id\").and_then(|s| s.as_str()).map(|s| s.to_string()); out.push(AnswerChunk::Started { session_id: self.session_id.clone() }); }` \u2014 id capture already works, no parser change needed for the tier flip",
  "registry_current_state": "crates/cue-agent-bridge/src/registry.rs Cursor row (KindTag::Cursor): model_flag: None (~line 535), continuation: ContinuationTier::Replay (~line 567); resume_args: &[\"--resume={id}\"] already present in cli.rs spec table (~line 190)"
}
```

**Hazards:**
- Wrong-cwd resume FAILS SILENTLY: cursor-agent --resume=<id> from a different cwd exits success and echoes the same session_id but mints a brand-new EMPTY session under md5(new-cwd) — any error-based degrade check will never fire; the tier flip is only safe if the drive always sets question.cwd to the session's original cwd
- Project-path resolution source: Cursor CLI sessions live in ~/.cursor/chats/<md5(cwd)>/<id>/meta.json (which has a "cwd" field), NOT in the GUI state.vscdb that sessions/vscdb.rs reads — if the NativeResume arm resolves the project path only via the vscdb store, CLI-minted ids will resolve no project path and either permanently degrade to replay or (worse) resume under the daemon's cwd and silently lose context
- Successful resume turns can return result:"" (our PLUM turn: the answer surfaced only as redacted reasoning, transcript text was literally [REDACTED]); push_cursor_line then emits Started+Done with zero Delta — downstream must tolerate empty answers rather than treating them as failure
- The Cursor JSON output does not echo which model ran, so model application cannot be asserted from parser output alone (only from the session store blob)
- Model ids churn with releases (current list is versioned: composer-2.5, gpt-5.3-codex, claude-opus-4-8-thinking-high, ...); hardcoding a model value is unsafe — validate via `cursor-agent models`/--list-models, or use "auto" which is always present
- Resume flag syntax: the equals form --resume=<id> is what was live-verified (help shows `--resume [chatId]` with optional value; bare --resume opens an interactive picker, which would hang a headless drive) — keep the {id} substitution embedded in the token as registry resume_args already does

---

## agent-a6592fb4807e1f18d.jsonl — verdict: **confirmed**

**Summary:** Recon confirmed the task's premises end-to-end. MODEL: model_override (app.rs:7356) is empty until the ModelBlocked resolver fills it (7431); it flows drive_answer_attempt → drive_with_overrides (lib.rs:102) → DriveOptions.model_override → appended last to argv in drive_with_options (cli.rs:586). Question has only {prompt, context, resume, cwd}. The seed seam is app.rs:7356 (registry model_flag + chosen value, also pre-fill tried_models). CHOICE SOURCE: CueSettings has attached_agent/attached_session but no attached_model; the overlay has a speed picker (fast/balanced/deep → mode) and dormant provider/model type plumbing but NO model-picker UI, and on the agent path the route is replaced with ProviderSelector::agent(label) so overlay model picks are discarded. SPEED: fast/balanced/deep is prose-only (mode_instructions app.rs:8348) and reaches the agent solely as a System turn in the prompt. Live CLI verification: copilot has --effort/--reasoning-effort (none..max); codex has generic -c (model_reasoning_effort key documented); cursor-agent embeds effort in --model bracket syntax; claude 2.0.42, gemini, and agy have NO effort flag — refuting the research doc's Claude --effort claim. Registry understates Cursor/Copilot (model_flag None despite live --model). Biggest hazard: ACP is default-on and ignores model_override, so any seeded model/effort is a no-op on ACP-routed turns unless that route is gated or extended.

**Facts:**

```json
{
  "question_fields": "cue-agent-bridge/src/drive/mod.rs:66-82 `Question { prompt: String, context: Option<Transcript>, resume: Option<String>, cwd: Option<String> }` \u2014 NO model or effort field. Model rides separately in DriveOptions.model_override (drive/cli.rs:288), an exact argv token pair appended verbatim.",
  "override_seam": "app.rs answer_with_agent:7356 `let mut model_override: Vec<String> = Vec::new();` \u2014 filled ONLY at 7431 by the ModelBlocked resolver (decide_model_block \u2192 ModelLoopStep::RetryWithModel{model_flag_args}). Flow: 7364 drive_answer_attempt(&model_override) \u2192 7166 cue_agent_bridge::drive_with_overrides(kind, question, model_override.to_vec()) \u2192 lib.rs:102 drive_with_overrides: CLI branch puts it in DriveOptions.model_override \u2192 drive/cli.rs:546 drive_with_options appends each token LAST (cli.rs:586-588) after build_argv_with_mode (base argv cli.rs:321 + answer_args + mcp_allow, cli.rs:475-513). EXACT SEAM for a user pick: before the loop at app.rs:7356, seed `model_override = vec![model_flag_for(&kind), chosen_model]` using cue_agent_bridge::model_resolve::model_flag_for (model_resolve.rs:380) \u2014 and push the chosen model into `tried_models` (7357) so the block-resolver won't retry it. Registry model_flag today: ClaudeCode/App/Agent Some(\"--model\"), Codex Some(\"-m\"), Gemini Some(\"--model\"), Aider Some(\"--model\"); Cursor/Copilot/Antigravity/AntigravityIde/Windsurf/VsCode None \u2014 Cursor and Copilot are WRONG per live --help (both accept --model; doc \u00a7'Registry bugs' agrees).",
  "overlay_picker_exists": "Speed picker: YES \u2014 crates/cue-meeting-overlay/ui/src/components/Composer.tsx MODES fast/balanced/deep (commit 1ead3a9), forwarded as `mode` through client.ask \u2192 tauriClient.ts:498-506 \u2192 OverlayEvent::AskRequested. Model picker: NO \u2014 types.ts:95-98 AskOpts has optional provider?/model? fields ('carried in type plumbing for later') but no component renders a model chooser (only comment-level 'model' hits in AskScreen.tsx). Moreover any overlay-sent provider/model is DISCARDED on the agent path: answer_with_provider_runtime app.rs:6294-6299 replaces request.route with ProviderSelector::agent(agent_model_label(kind)) whenever settings.attached_agent is set, so provider.model carries the agent LABEL (e.g. 'claude_code'), which answer_with_agent:7296-7301 parses back into AgentKind. There is no channel today for an actual model choice on the agent route.",
  "settings_gap": "cue-core/src/config.rs CueSettings (line 46): has attached_agent (76) and attached_session (81), NO attached_model field. persist_attached_agent (app.rs:3131) persists agent+session only; attach/detach IPC (OverlayEvent::AgentAttachRequested{kind, session_id}, app.rs:2445) carries no model.",
  "effort_flags_by_cli": {
    "claude (2.0.42, live --help)": "NO effort/thinking/reasoning flag headless. Only --model <alias|full-id> and --fallback-model. NOTE: docs/AGENT-MODEL-SPEED-CONTROL.md claims `--effort low..max` for Claude \u2014 REFUTED on the installed 2.0.42; wiring it would break every Claude drive.",
    "codex (codex-cli 0.137.0, `codex exec --help`)": "No dedicated effort flag; generic `-c <key=value>` config override IS in help. The known key is `-c model_reasoning_effort=\"minimal|low|medium|high|xhigh\"` (documented Codex config + research doc; key itself not listed in help, not live-driven here). Also `-m/--model`.",
    "gemini (live --help)": "None. Only -m/--model. Speed = model choice (-flash fast / -pro deep).",
    "cursor-agent (live --help)": "No separate flag. Effort embeds in the model string: `--model 'claude-opus-4-8[context=1m,effort=high,fast=false]'` (verified in help) or a `-thinking` model variant. Couples effort to model choice.",
    "copilot (live --help via node v24)": "`--effort, --reasoning-effort <level>` choices none|low|medium|high|xhigh|max \u2014 VERIFIED. Plus --model. CAVEAT: `copilot` on PATH dies under default node v23.7.0 ('requires Node.js v24'); binary lives under ~/.nvm/versions/node/v24.13.0 \u2014 the daemon's spawn needs Node-pinned runtime resolution.",
    "agy (live --help)": "--model only (display-string values, list via `agy models`), no effort flag. Doc: -p historically rejects --model pre-1.0.5 and drops stdout under non-TTY."
  },
  "mode_reaches_agent_path": "YES, but as PROSE only. Overlay speed \u2192 OverlayEvent::AskRequested.mode \u2192 answer_request_from_overlay (app.rs:8323) \u2192 request.with_instructions(mode_instructions(mode)) (8341-8342; fast/balanced/deep arms at 8365-8373 are pure prose) \u2192 merge_answer_instructions (6327) \u2192 ProviderRequestPayload.instructions \u2192 agent_question_from_payload (app.rs:6985-6999) turns it into a System Turn in Question.context \u2192 flattened into the single prompt argv entry. It never reaches the drive layer as data: DriveOptions.mode is DriveMode::{Answer,ProposeFix,ApplyFix} (write posture), unrelated to speed. No structured speed field exists on AnswerRequest/ProviderRequestPayload.",
  "minimal_model_wiring": "1) cue-core config.rs: add `#[serde(default)] pub attached_model: Option<String>` to CueSettings (clear on detach, like attached_session). 2) IPC: extend AgentAttachRequested (or add AgentModelSelected) + persist_attached_agent (app.rs:3131) to persist it; overlay UI: a picker feeding the existing AskOpts.model plumbing or the attach flow. 3) app.rs answer_with_agent: read settings (already loaded at 6293 in answer_with_provider_runtime \u2014 cheapest is to thread the model alongside resume_session into resolve_answer_route \u2192 answer_with_agent), then at 7356 seed `model_override = vec![flag.into(), chosen]` from model_resolve::model_flag_for(&kind), and push chosen into tried_models. Everything downstream (drive_with_overrides \u2192 DriveOptions.model_override \u2192 argv append at cli.rs:586) already works untouched. 4) registry.rs: fix Cursor + Copilot model_flag None \u2192 Some(\"--model\") (live-verified; doc-endorsed). Fallback UX: keep the ModelBlocked resolver overwriting the seed on a 400.",
  "minimal_speed_wiring": "1) registry.rs AgentEntry: add effort data \u2014 simplest is `effort_flag: Option<&'static str>` + `effort_style` (Separate vs ConfigKv) or a direct per-tier args table; populate ONLY live-verified rows: Copilot (\"--effort\", separate), Codex (\"-c\", kv `model_reasoning_effort=<tier>`). Claude/Gemini/Cursor/agy \u2192 None (prose fallback, which already works today). 2) drive/cli.rs DriveOptions: add `effort_override: Vec<String>` mirroring model_override, appended right after it in drive_with_options. 3) lib.rs drive_with_overrides: accept it (or fold model+effort into one `extra_args`/overrides struct). 4) app.rs: thread the raw speed tier as DATA to answer_with_agent \u2014 today it only survives as prose in payload.instructions; minimal is a `speed: Option<String>` on AnswerRequest+ProviderRequestPayload set in answer_request_from_overlay when mode \u2208 {fast,balanced,deep}, then answer_with_agent maps tier\u2192args via the registry (fast\u2192low, balanced\u2192none/default, deep\u2192high) and keeps the existing prose for flagless agents. This matches doc \u00a7'The fix' items 1-3."
}
```

**Hazards:**
- ACP is default-ON for ACP-capable agents (should_use_acp, lib.rs:235; ClaudeCode/Cursor/Gemini at least have specs) and drive_with_overrides IGNORES model_override on the ACP branch (honored only in the pre-first-token CLI fallback, lib.rs:107-123). The ACP schema has no model/effort field (doc §ACP path). A seeded user model/effort is therefore a silent NO-OP on most default-routed turns unless the CLI route is forced (BLUEY_USE_ACP=0), the seam also gates ACP routing, or a per-bridge ACP model param is added — the single biggest trap for this design.
- docs/AGENT-MODEL-SPEED-CONTROL.md asserts `claude --effort low..max` — the installed claude 2.0.42 has NO such flag (full --help inspected). Wiring effort_flag for Claude per the doc would make every Claude drive fail with an unknown-option error. Treat the doc's Claude effort row as future/version-gated.
- Overlay-sent provider/model is silently discarded whenever an agent is attached (route replaced at app.rs:6296), and ProviderSelector.model is overloaded to carry the agent kind label — a UI model pick cannot reuse those fields without disambiguating label-vs-model; persisting the choice in CueSettings sidesteps this.
- copilot on PATH fails under the default node v23.7.0 (requires v24; real binary under ~/.nvm/versions/node/v24.13.0). Any drive/effort wiring for Copilot inherits the known Node-pinned runtime-resolution requirement.
- Codex effort is a `-c key=value` config override, not a plain flag — the effort renderer needs a kv style, and ChatGPT-plan accounts 400 on many models: a user-picked model MUST be recorded in tried_models before the loop or the block-resolver can re-try/loop on it.
- User-picked model strings pass through to argv verbatim; the fallback resolver only fires on recognized model-block error text, so a typo'd model surfaces as a raw CLI failure card, not a graceful fallback.
- cursor-agent effort rides inside the --model bracket syntax, so speed control for Cursor couples to (and would overwrite) the model choice — safer to leave Cursor prose-only in v1.
- agy --model is version-gated (>=1.0.5), takes display strings not slugs, and old builds drop stdout under non-TTY — gate before wiring.
- mode/speed prose and session answer rules are merged into one instructions string (merge_answer_instructions, app.rs:6327); do not attempt to parse the tier back out of instructions — thread it as a separate structured field.

---

## agent-a6b4218b8c3076188.jsonl — verdict: **confirmed**

**Summary:** Recon complete for the spawn-time session ledger. The write point is the existing chaining-persist block in cue-daemon/src/app.rs (~7499-7524, inside answer_with_agent), where agent kind, session id, and the daemon handle (paths) are already in scope — but the cwd the drive actually used is NOT (it lives in drive_answer_attempt's local Question and must be threaded out via a new DriveOutcome.cwd field). The read point is resolve_session in cue-agent-bridge/src/continuation/tier.rs:55-86, where a ledger lookup should replace the reader.list() re-scrape for the project cwd (transcript read stays as the fork-fallback safety net). Recommended location: cue-agent-bridge/src/sessions/ledger.rs with the storage path injected by the daemon (option a) — matches the existing idiom where the daemon injects scalars/closures (AGENT_SESSION_LIST_CAP, summarize) and the bridge owns all session-store file IO; keeps write/read format in one module. Storage: {data_dir}/agent-session-ledger.jsonl next to sessions.db / rag_vectors.db in the 0o700-private data dir. CRITICAL naming hazard: crates/cue-daemon/src/ledger.rs and crates/cue-core/src/ledger.rs ALREADY EXIST (untracked, in-flight) as the unrelated meeting decisions ledger (docs/LEDGER-PLAN.md) — the session ledger must not be named plain `ledger` in the daemon.

**Facts:**

```json
{
  "write_points": {
    "capture": "app.rs:7200-7210 (inside drive_answer_attempt): AnswerChunk::Started { session_id } \u2014 last non-empty id kept in latest_session, returned as DriveOutcome.session_id (app.rs:7263-7267). Covers both CLI and ACP drives uniformly since both emit Started.",
    "persist": "app.rs:7489-7524 (inside answer_with_agent): the CONVERSATION CHAINING block. In scope: kind (AgentKind, cloned), session_id (Option<String> from the loop break at 7374), label, daemon = stream.as_ref().map(|s| s.daemon.clone()) (Arc<Daemon> with daemon.paths: AppPaths at app.rs:882). The ledger append belongs INSIDE the `still_attached && settings.attached_session != Some(new_id)` branch (7509-7521), right next to persist_attached_agent \u2014 appending only when the id CHANGED gives natural write-side dedup for chained turns reusing an id.",
    "cwd_gap": "The cwd the drive actually used is NOT accessible at the persist site. question.cwd is set by apply_continuation_tier (app.rs:7136) inside drive_answer_attempt and the Question is MOVED into cue_agent_bridge::drive_with_overrides at app.rs:7166-7170. DriveOutcome (app.rs:7065-7075) carries only body/cost_usd/session_id. Fix: add `cwd: Option<String>` to DriveOutcome, populated by cloning question.cwd just before the drive call (~app.rs:7157). When question.cwd is None the child inherits the daemon's cwd (drive/cli.rs:289-293, :559-562) \u2014 record the EFFECTIVE cwd: question.cwd.clone().or_else(|| std::env::current_dir().ok().and_then(|p| p.to_str().map(String::from))), because a cwd-scoped resume (Claude) later needs the real directory, not None.",
    "gating": "The persist block is gated on: (1) stream present (headless paths with no OverlayAnswerStream never ledger), (2) agent_chains_by_session_id(&kind) (app.rs:349 \u2014 NativeResume tiers only; correct, Replay ids aren't resumable), (3) still_attached. Sessions spawned by the summarize closure (drive_and_collect in apply_continuation_tier, app.rs:7046-7058) are never ledgered \u2014 acceptable, they're throwaway compaction turns."
  },
  "read_point": {
    "location": "cue-agent-bridge/src/continuation/tier.rs resolve_session (lines 55-86). The project cwd is currently recovered via reader.list(store, list_cap) re-scrape (lines 70-74) \u2014 the exact drift-exposed read the ledger closes.",
    "design": "Ledger lookup goes FIRST, before discover_agents/store resolution for the project: `let project = ledger_path.and_then(|p| ledger::lookup(p, agent, session_id)).and_then(|r| r.cwd).or_else(|| reader.list(...) re-scrape as today)`. Concretely: (1) ledger hit with Some(cwd) -> use it, SKIP reader.list entirely; (2) miss or record without cwd -> existing reader.list path unchanged. The transcript read (reader.read, lines 81-83) is NOT replaced \u2014 it remains the Replay context / NativeResume fork-fallback safety net, so store/reader resolution still happens for it. The existing cwd_usable existence check in apply_tier (lines 125-131) MUST stay: a ledger cwd can go stale (moved/deleted project), and the degrade-to-replay branches depend on it.",
    "threading": "apply_tier (tier.rs:96) gains the same `ledger_path: Option<&Path>` parameter and forwards it to resolve_session. Daemon call site: apply_continuation_tier (app.rs:7033-7062) \u2014 currently has no daemon handle, so either pass the path in from drive_answer_attempt (which can derive it via stream.as_ref().map(|s| s.daemon.paths.data_dir.join(...))) or add a `ledger_path: Option<PathBuf>` parameter to apply_continuation_tier. None => today's behavior (fail-soft)."
  },
  "recommended_location": "Option (a): cue-agent-bridge/src/sessions/ledger.rs with the storage PATH injected by the caller. Rationale against the code idioms: (1) the bridge already owns ALL session-store file IO (sessions/mod.rs readers: jsonl/vscdb/json_files/antigravity) and its fail-soft/bounded/read-only invariants \u2014 a JSONL ledger reader/writer is the same species; (2) the daemon's existing injection idiom into the bridge is scalars and closures (AGENT_SESSION_LIST_CAP at app.rs:2747 into apply_tier's list_cap; the summarize closure at app.rs:7046), never a config struct \u2014 a Path parameter fits; (3) grep confirms NO PathBuf/Path currently crosses into continuation/*, so either option adds the first path-crossing, but (a) keeps serialization format + append + read + dedup in ONE module next to its consumers, whereas option (b)'s lookup callback would split the format into cue-daemon (where a `ledger` module name ALREADY COLLIDES with the in-flight decisions ledger at crates/cue-daemon/src/ledger.rs) and force the write-side and read-side to agree across crates via a closure contract; (4) the bridge keeps zero cue-core dependency (Cargo.toml verified: no cue-core; only serde/serde_json/anyhow needed, all already deps). The daemon composes the path from cue-core AppPaths and hands it down \u2014 the dependency arrow stays daemon->bridge.",
  "exact_signatures": {
    "record": "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] pub struct SessionLedgerRecord { pub agent: AgentKind, pub session_id: String, pub cwd: Option<String>, pub spawned_at: u64 } \u2014 AgentKind already derives serde with rename_all = \"snake_case\" (confirmed via parse_attached_agent, app.rs:243-246), so the JSONL agent field is the same stable snake_case string used in settings.attached_agent. spawned_at = unix millis (u64).",
    "append": "pub fn append(path: &std::path::Path, record: &SessionLedgerRecord) -> anyhow::Result<()> \u2014 OpenOptions::new().create(true).append(true), serde_json::to_string + one write_all of line + \\n (single write syscall).",
    "read_all": "pub fn read_all(path: &std::path::Path) -> Vec<SessionLedgerRecord> \u2014 missing file => empty Vec; per-line serde_json::from_str, malformed lines skipped (fail-soft, matching every SessionReader); dedup on (agent, session_id) keeping the LAST occurrence (append order = keep-latest).",
    "lookup": "pub fn lookup(path: &std::path::Path, agent: &AgentKind, session_id: &str) -> Option<SessionLedgerRecord> \u2014 read_all then find; O(n) is fine at MVP sizes.",
    "resolve_session_change": "pub async fn resolve_session(agent: &AgentKind, session_id: &str, tier: ContinuationTier, list_cap: usize, ledger_path: Option<&std::path::Path>) -> (Option<String>, Option<Transcript>)",
    "apply_tier_change": "pub async fn apply_tier<F, Fut>(question: &mut Question, agent: &AgentKind, session_id: Option<&str>, via_acp: bool, list_cap: usize, ledger_path: Option<&std::path::Path>, summarize: F) \u2014 forwarded to resolve_session.",
    "drive_outcome_change": "struct DriveOutcome { body: String, cost_usd: Option<f64>, session_id: Option<String>, cwd: Option<String> } (app.rs:7065) \u2014 cwd = the effective drive cwd, cloned before the Question is moved at app.rs:7166."
  },
  "storage_path": "daemon.paths.data_dir.join(\"agent-session-ledger.jsonl\") \u2014 AppPaths (cue-core/src/app_paths.rs:10-17) exposes data_dir (~/Library/Application Support/bluey on macOS, BLUEY_DATA_DIR override honored, created 0o700 by ensure()); data_dir already hosts sessions.db (app.rs:11560), rag_vectors.db (app.rs:11520), page-context/ (app.rs:9041), captures/ (app.rs:9069). File name deliberately says agent-session- to disambiguate from the unrelated decisions ledger.",
  "format": {
    "encoding": "JSONL, one serde_json record per line, append-only.",
    "concurrency": "Single daemon process; asks CAN overlap (answer_with_agent per ask), but each append is one O_APPEND write_all of a <200-byte line \u2014 whole-write interleaving on APFS in practice, and the corrupt-line-skip reader makes a torn tail line non-fatal (record simply missed -> falls back to the existing re-scrape path). No in-process mutex needed at MVP; note the persist site already does synchronous small-file IO in async context (load_settings at app.rs:7504), so a plain std::fs append there matches the existing precedent.",
    "fsync": "None per append. Justification: the ledger is a resume OPTIMIZATION with the vendor-store re-scrape as the miss path \u2014 losing the tail line on power loss degrades to today's behavior, never loses user data. fsync per turn would add latency to the live answer path for no correctness gain.",
    "growth": "Bounded in practice: one line per NEW session id per chained conversation (~150 bytes); appends are further suppressed by the changed-id gate at the write point. 100 new sessions/day for a year is ~5 MB; read is a once-per-continuation O(n) scan. Keep-latest dedup happens on READ (no compaction pass); no rotation at MVP \u2014 revisit only if a size check ever shows multi-MB files.",
    "corrupt_lines": "Reader skips any line that fails serde_json::from_str and continues \u2014 same fail-soft invariant as every SessionReader (sessions/mod.rs doc: 'a malformed line / row / file is skipped; the rest still returns')."
  },
  "tests": [
    "test_ledger_append_then_read_round_trips_record \u2014 append one record, read_all returns it field-for-field",
    "test_ledger_read_dedups_same_agent_session_keeping_latest \u2014 two appends with same (agent, session_id) but different cwd/spawned_at; read_all/lookup return only the later one",
    "test_ledger_read_skips_corrupt_lines_and_returns_valid_ones \u2014 hand-write a file with a valid line, a garbage line, and a valid line; both valid records returned",
    "test_ledger_read_missing_file_returns_empty \u2014 nonexistent path -> empty Vec, no error",
    "test_ledger_lookup_distinguishes_same_session_id_across_agents \u2014 same id under ClaudeCode and Codex resolve to their own records",
    "test_resolve_session_prefers_ledger_cwd_over_store_rescrape \u2014 tempdir vendor-store fixture whose SessionRef.project differs from the ledger record's cwd; with ledger_path=Some, resolve_session returns the ledger cwd and (observable via the fixture) never needed the list scan",
    "test_resolve_session_falls_back_to_store_on_ledger_miss \u2014 ledger present but no matching (agent, session_id) -> project comes from reader.list as today",
    "test_resolve_session_none_ledger_path_matches_existing_behavior \u2014 ledger_path=None is byte-identical to the current path (regression guard for all existing callers)"
  ]
}
```

**Hazards:**
- NAME COLLISION (blocking): crates/cue-daemon/src/ledger.rs and crates/cue-core/src/ledger.rs already exist as UNTRACKED in-flight files — they are the unrelated meeting DECISIONS ledger (docs/LEDGER-PLAN.md, LedgerState/parse_and_verify). The session ledger must live at cue-agent-bridge/src/sessions/ledger.rs and use distinct names (SessionLedgerRecord, agent-session-ledger.jsonl); do not touch or rename the decisions-ledger files, they belong to a parallel effort on this dirty working tree.
- cwd is NOT in scope at the persist site (app.rs:7499) — it dies inside drive_answer_attempt when the Question is moved at app.rs:7166. DriveOutcome must grow a cwd field; forgetting this yields a ledger of {agent, id, None} records that cannot drift-proof cwd-scoped resume (Claude), the whole point.
- Recording cwd=None for fresh (non-continuation) spawns silently binds the session to the daemon's launch cwd; if the daemon later runs from elsewhere, a ledger resume would target the wrong ~/.claude/projects/<encoded-cwd> dir. Record the EFFECTIVE cwd (env::current_dir fallback) at drive time.
- Ledger cwd can go stale (project moved/deleted). apply_tier's cwd_usable existence check (tier.rs:125-131) must remain AFTER the ledger lookup — ledger replaces the re-scrape, not the usability guard, or NativeResume silently starts fresh (the documented context-loss failure).
- Headless/CLI ask paths without an OverlayAnswerStream never reach the persist block (stream is None at app.rs:7501), so those spawns are never ledgered — the re-scrape fallback must stay, this is an optimization layer, not a replacement.
- Stale comment risk: app.rs:7129-7133 claims an ACP id-space mismatch, but MEMORY (project_true_resume_mechanism) and tier.rs:135-156 say true ACP resume works and the on-disk id IS the key. Recording the Started id is correct either way (it is what --resume/session-load consume), but do not 'fix' that comment as part of the ledger change — out of scope.
- Signature changes to public bridge fns (resolve_session, apply_tier) ripple to all call sites and must compile under clippy -D warnings with --target aarch64-apple-darwin and PKG_CONFIG_PATH=/opt/homebrew/opt/openblas/lib/pkgconfig for cue-daemon; an Option<&Path> default-None keeps every existing caller semantically unchanged.
- If two daemon instances ever run (e.g. dev + prod with the same BLUEY_DATA_DIR), cross-process appends interleave; whole-line O_APPEND writes plus corrupt-line skip make this degrade soft, but do not add read-side caching that assumes single-writer.

---

## agent-add348c96f4013a15.jsonl — verdict: **partial**

**Summary:** Model flag CONFIRMED: agy has --model taking the exact display labels from `agy models` (e.g. "Gemini 3.5 Flash (Low)"); live smoke returned OK and the CLI log proves propagation ("Propagating selected model override to backend: label=Gemini 3.5 Flash (Low)"). Per-conversation resume CONFIRMED mechanically: codeword MANGO stored via `agy -p`, recovered via `agy --conversation <id> -p` from a different cwd. BUT the resume fix as planned is undermined by a store split the research missed: the agy CLI reads/writes conversations in ~/.gemini/antigravity-cli (SQLite conversation_summaries.db index), NOT ~/.gemini/antigravity (the agyhub_summaries_proto.pb desktop store Bluey's AntigravityIndex reader and data_dir_globs target). A desktop-store id from Bluey's own session list fails live with "trajectory not found" (exit 1) — so after flipping tier to NativeResume, attaching a Bluey-listed Antigravity session will hard-fail, and since agy parses as PlainText (Started{session_id:None}, cli.rs:832), fresh-conversation chaining is also inert. The tier flip only helps if the sessions reader additionally learns the antigravity-cli store, or ids are minted/captured some other way.

**Facts:**

```json
{
  "model_flag": "--model (single flag; value is the exact display label from `agy models`, e.g. \"Gemini 3.5 Flash (Low)\" \u2014 spaces and parens, must be one argv entry)",
  "model_value_known": true,
  "model_values": [
    "Gemini 3.5 Flash (Medium)",
    "Gemini 3.5 Flash (High)",
    "Gemini 3.5 Flash (Low)",
    "Gemini 3.1 Pro (Low)",
    "Gemini 3.1 Pro (High)",
    "Claude Sonnet 4.6 (Thinking)",
    "Claude Opus 4.6 (Thinking)",
    "GPT-OSS 120B (Medium)"
  ],
  "model_flag_verified_live": "agy --model \"Gemini 3.5 Flash (Low)\" -p \u2192 OK; log ~/.gemini/antigravity-cli/log/cli-20260703_121614.log shows printmode model=\"Gemini 3.5 Flash (Low)\" and 'Propagating selected model override to backend: label=\"Gemini 3.5 Flash (Low)\"'",
  "resume_codeword_ok": true,
  "resume_test_ids": {
    "cli_store_id_resumed_ok": "6da73f7c-bc0e-484b-9aa2-a01967f7b90d",
    "desktop_store_id_rejected": "544d284d-cacc-4972-847b-5e75724a9311 \u2192 'Error: failed to send message: trajectory not found', exit 1"
  },
  "id_source": "NEW conversations minted by `agy -p` land in ~/.gemini/antigravity-cli/conversations/<uuid>.db (index: conversation_summaries.db SQLite) \u2014 NOT in ~/.gemini/antigravity (desktop store, agyhub_summaries_proto.pb index) that crates/cue-agent-bridge/src/sessions/antigravity.rs and registry data_dir_globs [\".gemini/antigravity\"] read. The id is also logged (not printed) as 'Print mode: conversation=<uuid>' in ~/.gemini/antigravity-cli/log/cli-*.log; stdout carries only the answer text.",
  "cwd_independent": true,
  "cwd_note": "store run from /Users/ms/Developer/Bluey, resume from the scratchpad dir \u2192 MANGO; CLI store is global under ~/.gemini/antigravity-cli, no per-cwd keying",
  "parser_emits_id": false,
  "parser_evidence": "OutputParser::PlainText at crates/cue-agent-bridge/src/drive/cli.rs:832 yields AnswerChunk::Started { session_id: None }; app.rs:7206-7209 only records non-empty ids, app.rs:7499-7524 only persists when latest_session is Some \u2014 so nothing is ever persisted for agy",
  "chaining_after_flip": "Fresh agy conversation: turn 2 will NOT chain (no id reaches Started; each ask mints a new CLI-store conversation). Attached existing session (settings.attached_session): works ONLY if the id is an antigravity-cli-store conversation; every id Bluey's current AntigravityIndex reader lists comes from the desktop store and fails live with 'trajectory not found' \u2014 and that phrase is NOT matched by is_resume_recoverable_error (crates/cue-agent-bridge/src/continuation/recoverable.rs:25 \u2014 no 'session'/'conversation ... not found' substring), so app.rs:7379 will NOT retry fresh; the user sees a hard error.",
  "extra_option": "`agy --continue` / `-c` (resume most recent conversation) exists and could chain fresh turn 2 without an id, at the cost of a race if the user runs agy concurrently elsewhere"
}
```

**Hazards:**
- STORE SPLIT (blocker-grade for the resume fix as scoped): agy CLI conversations live in ~/.gemini/antigravity-cli (conversation_summaries.db SQLite index); Bluey's AntigravityIndex reader + data_dir_globs target ~/.gemini/antigravity (agyhub_summaries_proto.pb). Verified live: --conversation with a desktop-store id fails 'trajectory not found'. Flipping tier to NativeResume without teaching the sessions layer the antigravity-cli store makes attach-then-ask hard-fail for every session Bluey can list today.
- is_resume_recoverable_error (crates/cue-agent-bridge/src/continuation/recoverable.rs) does not match agy's 'trajectory not found' phrasing, so a failed agy resume will NOT fall back to a fresh drive (app.rs:7379 gate) — add the phrase to the not-resumable class before/with the tier flip.
- Stale comment at crates/cue-agent-bridge/src/drive/cli.rs:250-252 claims 'verified' id equivalence between the store Bluey reads and `agy --conversation` — refuted live for the desktop store; fix the comment when applying the change.
- PlainText parser means no session id ever reaches the daemon for agy: after the flip, daemon turn-chaining (app.rs:7203/7499) stays inert for fresh conversations. Honest limit, not a blocker; possible mitigations: snapshot-diff ~/.gemini/antigravity-cli/conversations around the drive, parse 'Print mode: conversation=<uuid>' from the newest cli-*.log, or use --continue for turn 2 (race-prone).
- --model values are backend-fetched display labels (spaces + parentheses) that can drift with Antigravity's server-side model list; behavior on an invalid label was NOT tested (would cost a live call). If fallback_models entries are added, use the exact labels above and ensure the model_override argv appends them as a single value entry after --model.
- agy models / drives require auth; runs succeed via silent auth here, but a signed-out state produces 'You are not logged into Antigravity' errors (seen in cli-20260703_121455.log warnings) — registry row has login_command: None, so no guided recovery exists for agy.

---

## agent-afcb497f6317559f8.jsonl — verdict: **confirmed**

**Summary:** The standalone GitHub Copilot CLI (v1.0.64, run under node v24.13.0 via ~/.nvm) accepts a per-run `--model <model>` flag ("Set the AI model to use (use 'auto' to let Copilot pick automatically)"). Live smoke `copilot -p "reply with exactly OK" --model auto -s` returned exactly "OK", exit 0. Registry change model_flag: None -> Some("--model") at crates/cue-agent-bridge/src/registry.rs:717 is safe: model_flag is consumed only via model_resolve.rs::model_flag_for (line 380) inside plan_resolution (line 419), which emits [flag, model] only when a non-exhausted fallback_models entry exists; with the Copilot row's fallback_models: &[] the flag is inert (resolution falls through to BYOT). The drive layer never reads model_flag — Copilot's DriveSpec (drive/cli.rs:146-163) is static, and the only model injection point is DriveOptions::model_override (empty by default, cli.rs:302, appended cli.rs:586), populated exclusively from plan_resolution's RetryWithModel output (prove_drive.rs:196, cue-daemon/src/app.rs:7431, cue-cli/src/app.rs:2732).

**Facts:**

```json
{
  "model_flag": "--model",
  "flag_syntax": "--model <model> (space-separated value; help text: \"Set the AI model to use (use 'auto' to let Copilot pick automatically)\")",
  "allowed_values": "Not enumerated by --help. Only documented value is 'auto'; help example shows a concrete model ('copilot --model gpt-5.4'). Actual model list is account/plan-dependent and not printed in help.",
  "smoke_ok": true,
  "smoke_command": "copilot -p \"reply with exactly OK\" --model auto -s (copilot 1.0.64, node v24.13.0 via ~/.nvm/versions/node/v24.13.0/bin) -> stdout 'OK', exit 0",
  "consumer_proof": "model_flag is read only by model_resolve.rs: model_flag_for (crates/cue-agent-bridge/src/model_resolve.rs:380-384) -> plan_resolution (lines 402-432) where `if let Some(flag) = flag { if let Some(next) = fallbacks.iter().copied().find(|m| !is_exhausted(m)) { return ModelProposal::RetryWithModel { model_flag_args: vec![flag.to_string(), next.to_string()], .. } } }` \u2014 with Copilot's fallback_models: &[] (registry.rs:716) the inner find is always None, so the flag is never emitted and resolution proposes BYOT. Drive layer: Copilot DriveSpec (drive/cli.rs:146-163) never references model_flag; the only argv injection is DriveOptions::model_override (default Vec::new(), cli.rs:302) appended at cli.rs:586-588, and every setter (prove_drive.rs:196, cue-daemon/src/app.rs:7431, cue-cli/src/app.rs:2732) populates it solely from plan_resolution's RetryWithModel::model_flag_args. Therefore Some(\"--model\") is inert until a fallback/explicit model is requested.",
  "registry_row": "crates/cue-agent-bridge/src/registry.rs:713-772 (KindTag::Copilot; model_flag at line 717, fallback_models at line 716)",
  "node_requirement": "Copilot needs Node >= 24; default node on this machine is v23.7.0 \u2014 used ~/.nvm/versions/node/v24.13.0/bin prefix (mirrors runtime_resolve.rs behavior)"
}
```

**Hazards:**
- Allowed model values are NOT enumerated in --help and are account/plan-dependent; if fallback_models entries are later added for Copilot, each value needs its own live verification (an invalid --model value's failure mode — error vs silent fallback — was not tested).
- The smoke used --model auto (the only help-documented value); a concrete model id was not live-tested to avoid burning quota, so treat specific model ids as unverified.
- The drive layer appends model_override AFTER all other flags (cli.rs:586 'Appended after the prompt and other flags'), verified live only for Codex's -m; the smoke passed --model mid-argv (after -p, before -s), which works, but a trailing position after --resume=<id> has not been live-tested for Copilot.

---

