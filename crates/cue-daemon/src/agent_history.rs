//! Agent-history retrieval — daemon orchestration (feature `local-memory`).
//!
//! The "borrow their reasoning" side-channel (PLAN Wave 2): every coding agent
//! Bluey can drive (Claude Code, Codex, Cursor, …) leaves its OWN past-session
//! transcripts on disk. This module builds and OWNS an in-memory retrieval index
//! ([`cue_rag::AgentHistoryIndex`]) over that cross-agent prose so the driven
//! agent can PULL relevant slices of it, read-only, as grounding for a meeting
//! question — exposed as the `search_agent_history` MCP tool.
//!
//! Discipline (all of it fail-soft; a retrieval hiccup must NEVER break the
//! answer path):
//! - **Consent-gated.** The whole feature is OFF unless BOTH the env flag
//!   [`enabled`] (`BLUEY_AGENT_HISTORY`, default OFF) AND the EXISTING
//!   session-history consent (`settings.allow_agent_session_history`) are on —
//!   reading the user's other agents' history is exactly what that consent
//!   governs, so we reuse it rather than add a parallel one.
//! - **Never blocks the caller.** The index builds LAZILY on the first search
//!   and refreshes on a TTL (`BLUEY_AGENT_HISTORY_TTL_SECS`, default 900). A
//!   stale/missing index spawns a background rebuild and serves whatever it has
//!   NOW (empty on the very first call) — [`AgentHistoryStore::search`] never
//!   awaits a full rebuild.
//! - **Single-flight rebuild.** An inflight `AtomicBool` + Drop guard (mirrors
//!   the conversation fold) means overlapping searches trigger at most one build.
//! - **Bounded + fail-soft per agent.** Total indexed chunks are capped
//!   (`BLUEY_AGENT_HISTORY_MAX_CHUNKS`, default 5000, most-recent sessions
//!   first) and one bad reader/session is skipped, never aborting the build.
//! - **Reuses the loaded embedder.** The build/query embed via the daemon's
//!   already-loaded `LocalBgeEmbedder` (shared from `FactsMemory`), never a
//!   second copy of the model.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use cue_agent_bridge::sessions::{list_with_health_check, reader_for};
use cue_agent_bridge::{discover_agents, AgentKind, DiscoveredAgent, Role};
use cue_rag::{AgentHistoryIndex, EmbeddingProvider, HistoryHit};

use crate::app::Daemon;

/// Env flag gating the whole feature (default OFF — reading other agents'
/// history is sensitive, opt-in for beta). Any truthy value ("1"/"true"/"on"/
/// "yes", case-insensitive) turns it on.
const ENV_ENABLE: &str = "BLUEY_AGENT_HISTORY";
/// Env override for the rebuild TTL in seconds (min-clamped).
const ENV_TTL_SECS: &str = "BLUEY_AGENT_HISTORY_TTL_SECS";
/// Env override for the total-chunk cap (min-clamped).
const ENV_MAX_CHUNKS: &str = "BLUEY_AGENT_HISTORY_MAX_CHUNKS";

/// Default rebuild TTL: 15 minutes.
const DEFAULT_TTL_SECS: u64 = 900;
/// Floor on the TTL so a misconfigured tiny value can't rebuild every search.
const MIN_TTL_SECS: u64 = 30;
/// Default total-chunk cap across all agents.
const DEFAULT_MAX_CHUNKS: usize = 5000;
/// Floor on the chunk cap so a misconfigured tiny value still indexes something.
const MIN_MAX_CHUNKS: usize = 100;

/// Recent sessions read per agent when building the index. Bounded so a giant
/// store (Cursor/Code/Claude can be gigabytes) never dominates the build; the
/// per-agent list is already most-recent-first, and the global chunk cap is the
/// real ceiling.
const SESSIONS_PER_AGENT: usize = 40;
/// Max turns decoded per session (the reader honors this — bounded read).
const MAX_TURNS_PER_SESSION: usize = 400;

/// Which agents' session history the index covers.
///
/// ALWAYS [`Attached`](HistoryScope::Attached): only the currently attached
/// agent's own sessions are searched — NEVER other agents you happen to have
/// installed. `search_agent_history` prefers the most-recent session first, then
/// widens to the SAME agent's other sessions, and stops there. Cross-agent search
/// was removed deliberately: a meeting answer must not silently pull another
/// agent's private reasoning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryScope {
    /// Only the currently attached agent's sessions.
    Attached,
}

/// The history scope is fixed to the attached agent — see [`HistoryScope`].
pub(crate) fn scope() -> HistoryScope {
    HistoryScope::Attached
}

/// Whether the env flag requests the feature (independent of consent). The
/// caller ANDs this with `settings.allow_agent_session_history`.
pub(crate) fn enabled() -> bool {
    std::env::var(ENV_ENABLE)
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "on" | "yes")
        })
        .unwrap_or(false)
}

/// Rebuild TTL from env (min-clamped), mirroring `ledger::interval_words`.
fn ttl_secs() -> u64 {
    std::env::var(ENV_TTL_SECS)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(|v| v.max(MIN_TTL_SECS))
        .unwrap_or(DEFAULT_TTL_SECS)
}

/// Total-chunk cap from env (min-clamped).
fn max_chunks() -> usize {
    std::env::var(ENV_MAX_CHUNKS)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .map(|v| v.max(MIN_MAX_CHUNKS))
        .unwrap_or(DEFAULT_MAX_CHUNKS)
}

/// Now, as epoch seconds (0 on the impossible pre-1970 clock).
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Owns the in-memory agent-history index + its freshness bookkeeping.
///
/// The index is behind an `RwLock` so searches share a read lock and the
/// background rebuild swaps a freshly-built index in under a brief write lock
/// (build happens OFF-lock; only the swap holds the write lock). `built_at` is
/// `0` until the first successful build; `inflight` single-flights rebuilds.
#[derive(Default)]
pub(crate) struct AgentHistoryStore {
    index: RwLock<AgentHistoryIndex>,
    /// Epoch seconds of the last successful build (`0` = never built).
    built_at: AtomicU64,
    /// True while a background rebuild is running (single-flight).
    inflight: AtomicBool,
}

impl AgentHistoryStore {
    /// A fresh, empty store (index built lazily on first search).
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Search the agent-history index for `query`, returning at most `limit`
    /// hits. Returns `vec![]` when the feature or consent is OFF. Ensures the
    /// index is fresh LAZILY: if never built or older than the TTL, spawns a
    /// background rebuild and serves whatever the index holds NOW (empty on the
    /// first call) — never blocks the caller on a full rebuild.
    pub(crate) async fn search(
        &self,
        daemon: &Arc<Daemon>,
        query: &str,
        limit: usize,
    ) -> Vec<HistoryHit> {
        if limit == 0 || !feature_on(daemon) {
            return Vec::new();
        }

        // Lazy freshness: kick a background rebuild if stale/never-built. This
        // does NOT await the build — we serve the current (possibly empty or
        // stale) index immediately.
        self.maybe_spawn_rebuild(daemon);

        // Embed the query with the shared daemon embedder (off-lock).
        let Some(embedder) = daemon_embedder(daemon).await else {
            return Vec::new();
        };
        let query_embedding = match embedder.embed_query(query).await {
            Ok(e) => e,
            Err(error) => {
                debug!("agent-history: query embed failed: {error}");
                return Vec::new();
            }
        };

        // Prefer the most-recent agent session FIRST, widening to the agent's
        // OTHER sessions only if it holds nothing relevant. Prefer the live
        // resume target (`attached_session`) when one is set; else the retained
        // `search_prefer_session` hint (the last session, kept after a fresh-per-
        // meeting drive cleared `attached_session`). Both share the index's
        // `session_id` id-space, so a direct match works; unset → plain search.
        let prefer = cue_core::load_settings(&daemon.paths)
            .ok()
            .and_then(|s| s.attached_session.or(s.search_prefer_session))
            .unwrap_or_default();

        // The active model's calibrated score floor (bge/arctic ~0.45, Gemma
        // ~0.20). Passing the model's own floor keeps a lower-scoring model's
        // correct hits instead of discarding them with a bge-tuned threshold.
        let floor = embedder.relevance_floor();

        // SHORT read lock: search, then release. `search` is pure CPU over the
        // in-memory index; no await under the lock.
        let guard = self.index.read().await;
        guard.search_prefer_session_with_floor(query, &query_embedding, limit, &prefer, floor)
    }

    /// Spawn a background rebuild iff the index is stale (older than the TTL) or
    /// never built, and no rebuild is already inflight. Fire-and-forget.
    fn maybe_spawn_rebuild(&self, daemon: &Arc<Daemon>) {
        let built_at = self.built_at.load(Ordering::SeqCst);
        let fresh = built_at != 0 && now_secs().saturating_sub(built_at) < ttl_secs();
        if fresh {
            return;
        }
        // We can't move `&self` into the task; the store lives on the Daemon, so
        // re-borrow it there. Single-flight is claimed here (before spawn) so two
        // concurrent searches don't both spawn a build.
        let daemon = Arc::clone(daemon);
        tokio::spawn(async move {
            daemon.agent_history.rebuild(&daemon).await;
        });
    }

    /// Rebuild the index from scratch: discover agents, read each drivable
    /// agent's recent sessions' prose turns, feed [`AgentHistoryIndex::add_session`]
    /// with the shared embedder, cap total chunks, and swap the fresh index in.
    /// Single-flight (an inflight flag + Drop guard, like the conversation fold);
    /// fail-soft per agent (one bad reader must not abort the build).
    pub(crate) async fn rebuild(self: &Arc<Self>, daemon: &Arc<Daemon>) {
        if !feature_on(daemon) {
            return;
        }
        if self.inflight.swap(true, Ordering::SeqCst) {
            return; // a rebuild is already running
        }
        // Clear the inflight flag on EVERY exit path (including panics).
        struct InflightClear(Arc<AgentHistoryStore>);
        impl Drop for InflightClear {
            fn drop(&mut self) {
                self.0.inflight.store(false, Ordering::SeqCst);
            }
        }
        let _guard = InflightClear(Arc::clone(self));

        let Some(embedder) = daemon_embedder(daemon).await else {
            debug!("agent-history: embedder not ready; rebuild deferred");
            return;
        };
        let cap = max_chunks();

        // Scope: the attached agent ONLY (see `HistoryScope`) — never other
        // agents. Resolved HERE, per rebuild, so re-attaching a different agent
        // re-scopes the index on the next TTL rebuild.
        let only_kind = match scope() {
            HistoryScope::Attached => {
                let settings = cue_core::load_settings(&daemon.paths).ok();
                let kind = settings
                    .as_ref()
                    .and_then(|s| crate::app::parse_attached_agent(s.attached_agent.as_deref()));
                if kind.is_none() {
                    // Attached-scope with nothing attached ⇒ nothing to index.
                    // Fail CLOSED (empty index) rather than silently widening to
                    // every agent, which is exactly what the default forbids.
                    debug!("agent-history: attached scope but no agent attached; index left empty");
                    self.built_at.store(now_secs(), Ordering::SeqCst);
                    *self.index.write().await = AgentHistoryIndex::new();
                    return;
                }
                kind
            }
        };

        // Discovery + session reads are blocking file/SQLite IO — collect the
        // raw prose off the async runtime, then embed on this task.
        let sessions = tokio::task::spawn_blocking(move || collect_recent_sessions(only_kind))
            .await
            .unwrap_or_else(|error| {
                warn!("agent-history: session-collect task panicked: {error}");
                Vec::new()
            });

        // Build a fresh index off-lock so searches keep serving the old one.
        let mut fresh = AgentHistoryIndex::new();
        let mut embed = |text: &str| -> Option<Vec<f32>> {
            match embedder.embed_passage_blocking(text) {
                Ok(e) => Some(e),
                Err(error) => {
                    debug!("agent-history: passage embed failed: {error}");
                    None
                }
            }
        };
        for session in sessions {
            if fresh.len() >= cap {
                break;
            }
            fresh.add_session(
                &session.agent,
                &session.session_id,
                session.epoch_secs,
                &session.turns,
                &mut embed,
            );
        }

        let chunk_count = fresh.len();
        // SHORT write lock: swap the fresh index in.
        {
            let mut guard = self.index.write().await;
            *guard = fresh;
        }
        self.built_at.store(now_secs(), Ordering::SeqCst);
        info!(
            chunks = chunk_count,
            "agent-history index (re)built (cross-agent session prose)"
        );
    }
}

/// True when BOTH the env flag and the session-history consent are on. This is
/// the single gate every entry point checks — reading the user's other agents'
/// history requires the same consent the agent-session reads already use.
fn feature_on(daemon: &Arc<Daemon>) -> bool {
    // Env override wins in BOTH directions (dev/test), mirroring the ledger's
    // `live_memory_enabled`: if `BLUEY_AGENT_HISTORY` is set, it decides; else
    // the `allow_agent_session_history` setting decides (default ON — it is the
    // user's own local session data and backs the core recall feature).
    if std::env::var(ENV_ENABLE).is_ok() {
        return enabled();
    }
    cue_core::load_settings(&daemon.paths)
        .map(|s| s.allow_agent_session_history)
        .unwrap_or(true)
}

/// Clone the shared, already-loaded embedder (bge/arctic/gemma, behind the
/// trait) from the daemon's facts memory (its ONNX session is `Arc`-internal,
/// so this is a cheap handle share, NOT a second model load). `None` until facts
/// memory has finished its background init.
async fn daemon_embedder(daemon: &Arc<Daemon>) -> Option<std::sync::Arc<dyn EmbeddingProvider>> {
    let memory = daemon.facts_memory.lock().await.clone()?;
    Some(memory.embedder())
}

/// One agent session's indexable content, collected off the async runtime.
struct SessionProse {
    agent: String,
    session_id: String,
    epoch_secs: u64,
    turns: Vec<String>,
}

/// Whether `candidate` belongs to the same agent "family" as the attached
/// `want` — i.e. reads the SAME underlying session store.
///
/// Exact-match everywhere except Claude: `ClaudeCode` (CLI), `ClaudeCodeApp`
/// (desktop) and `ClaudeCodeAgent` all drive the one `claude` engine and write
/// into the same `~/.claude/projects/**` transcript store (the App/Agent indexes
/// just point INTO those files). Treating them as distinct would mean attaching
/// the desktop app hides history the CLI wrote — the same conversations.
fn same_agent_family(want: &AgentKind, candidate: &AgentKind) -> bool {
    let claude = |k: &AgentKind| {
        matches!(
            k,
            AgentKind::ClaudeCode | AgentKind::ClaudeCodeApp | AgentKind::ClaudeCodeAgent
        )
    };
    if claude(want) && claude(candidate) {
        return true;
    }
    want == candidate
}

/// Collect the prose turns of recent sessions, most-recent-first across the set.
/// Pure blocking IO (file / read-only SQLite); fail-soft per agent and per
/// session. The caller applies the global chunk cap while indexing (so the
/// most-recent sessions win).
///
/// `only_kind` scopes the sweep to ONLY that agent's sessions — always
/// `Some(kind)` now ([`HistoryScope::Attached`]); cross-agent indexing was
/// removed. (`None` still means "no kind filter", but callers no longer pass it.)
/// All three Claude surfaces (CLI / App / Agent) share one `claude` engine and
/// transcript store, so attaching any of them matches the others — scoping to a
/// single variant would hide the same underlying history.
fn collect_recent_sessions(only_kind: Option<AgentKind>) -> Vec<SessionProse> {
    let mut out: Vec<SessionProse> = Vec::new();
    for agent in discover_agents() {
        if let Some(want) = only_kind.as_ref() {
            if !same_agent_family(want, &agent.kind) {
                continue;
            }
        }
        let Some(store) = agent.session_store.as_ref() else {
            continue;
        };
        // Read-only, health-checked listing (surfaces vendor format drift once).
        let refs = match list_with_health_check(store.format, store, SESSIONS_PER_AGENT) {
            Ok(refs) => refs,
            Err(error) => {
                debug!(
                    agent = %agent_label(&agent),
                    "agent-history: session list failed: {error:#}"
                );
                continue;
            }
        };
        let reader = reader_for(store.format);
        let label = agent_label(&agent);
        for r in refs {
            let epoch_secs = parse_epoch(&r.updated_at);
            let transcript = match reader.read(store, &r.id, MAX_TURNS_PER_SESSION) {
                Ok(t) => t,
                Err(error) => {
                    debug!(
                        agent = %label,
                        session = %r.id,
                        "agent-history: session read failed: {error:#}"
                    );
                    continue;
                }
            };
            let turns: Vec<String> = transcript
                .turns
                .into_iter()
                // Human-prose turns only: the driven agent grounds on the
                // reasoning dialogue (user asks + assistant reasons), not
                // system/tool scaffolding. The index applies its OWN self-prompt
                // / tool-dump / oversize filters on top of this.
                .filter(|t| matches!(t.role, Role::User | Role::Assistant))
                .map(|t| t.text)
                .filter(|t| !t.trim().is_empty())
                .collect();
            if turns.is_empty() {
                continue;
            }
            out.push(SessionProse {
                agent: label.clone(),
                session_id: r.id,
                epoch_secs,
                turns,
            });
        }
    }
    // Most-recent session first, so the global chunk cap keeps the freshest
    // reasoning when the corpus exceeds it.
    out.sort_by_key(|s| std::cmp::Reverse(s.epoch_secs));
    out
}

/// A stable, human label for an agent (its registry model label, e.g. "claude",
/// "codex"). Mirrors the daemon's own `agent_model_label` usage.
fn agent_label(agent: &DiscoveredAgent) -> String {
    cue_agent_bridge::registry::display_name_for(&agent.kind)
        .unwrap_or_else(|| format!("{:?}", agent.kind))
}

/// Parse a `SessionRef::updated_at` marker into epoch seconds. The file-backed
/// readers emit epoch-seconds strings; a few sources emit RFC3339. We only need
/// a monotonic recency key, so: try a bare integer first, then the epoch prefix
/// of an RFC3339 year (fallback `0` — treated as oldest, never fatal).
fn parse_epoch(updated_at: &str) -> u64 {
    let s = updated_at.trim();
    if let Ok(n) = s.parse::<u64>() {
        return n;
    }
    // RFC3339 like "2026-07-18T…": we have no date lib in-tree, so fall back to
    // 0 (this source simply sorts as oldest — recency ordering degrades
    // gracefully, correctness of retrieval is unaffected).
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_epoch_reads_integer_strings_and_defaults_rfc3339_to_zero() {
        assert_eq!(parse_epoch("1700000000"), 1_700_000_000);
        assert_eq!(parse_epoch("  42  "), 42);
        assert_eq!(parse_epoch("2026-07-18T10:00:00Z"), 0);
        assert_eq!(parse_epoch(""), 0);
        assert_eq!(parse_epoch("not-a-number"), 0);
    }

    #[test]
    fn ttl_and_cap_are_min_clamped_from_env() {
        // Defaults when unset.
        std::env::remove_var(ENV_TTL_SECS);
        std::env::remove_var(ENV_MAX_CHUNKS);
        assert_eq!(ttl_secs(), DEFAULT_TTL_SECS);
        assert_eq!(max_chunks(), DEFAULT_MAX_CHUNKS);

        // A too-small value is clamped up to the floor, never below.
        std::env::set_var(ENV_TTL_SECS, "1");
        std::env::set_var(ENV_MAX_CHUNKS, "5");
        assert_eq!(ttl_secs(), MIN_TTL_SECS);
        assert_eq!(max_chunks(), MIN_MAX_CHUNKS);

        // A sane value passes through.
        std::env::set_var(ENV_TTL_SECS, "1200");
        std::env::set_var(ENV_MAX_CHUNKS, "8000");
        assert_eq!(ttl_secs(), 1200);
        assert_eq!(max_chunks(), 8000);

        std::env::remove_var(ENV_TTL_SECS);
        std::env::remove_var(ENV_MAX_CHUNKS);
    }

    #[test]
    fn enabled_reads_truthy_env_values() {
        std::env::remove_var(ENV_ENABLE);
        assert!(!enabled());
        for truthy in ["1", "true", "TRUE", "on", "Yes"] {
            std::env::set_var(ENV_ENABLE, truthy);
            assert!(enabled(), "{truthy} should enable");
        }
        for falsy in ["0", "false", "off", "no", ""] {
            std::env::set_var(ENV_ENABLE, falsy);
            assert!(!enabled(), "{falsy:?} should NOT enable");
        }
        std::env::remove_var(ENV_ENABLE);
    }

    #[test]
    fn scope_is_always_attached_never_cross_agent() {
        // The scope is fixed to the attached agent — cross-agent history search
        // was removed, so there is exactly one scope and no env override widens it.
        assert_eq!(scope(), HistoryScope::Attached);
    }

    #[test]
    fn same_agent_family_groups_the_three_claude_surfaces_only() {
        use AgentKind::*;
        // All three Claude surfaces share the one `claude` engine + transcript
        // store, so attaching any of them must match the others.
        let claude = [ClaudeCode, ClaudeCodeApp, ClaudeCodeAgent];
        for want in &claude {
            for candidate in &claude {
                assert!(
                    same_agent_family(want, candidate),
                    "{want:?} should match {candidate:?} (same store)"
                );
            }
        }
        // Everything else is exact-match: attaching Claude must NOT pull in
        // Codex/Cursor/etc. — that is the whole point of the default scope.
        assert!(same_agent_family(&Codex, &Codex));
        assert!(!same_agent_family(&ClaudeCode, &Codex));
        assert!(!same_agent_family(&ClaudeCode, &Cursor));
        assert!(!same_agent_family(&Codex, &ClaudeCode));
        assert!(!same_agent_family(&Cursor, &Gemini));
    }
}
