//! **MCP probe** — prove, per agent, that the user's own MCP connectors really
//! fire when the agent is driven, and that *live* data (data the model could not
//! have known from training) comes back.
//!
//! The product promise is "your agent answers WITH your own connectors/tools."
//! [`prove`](crate::prove) shows a connector is *configured*; [`prove_drive`]
//! shows the agent *answers at all*. Neither shows a tool actually executed.
//! This module closes that gap: it auto-picks a read-only probe from the agent's
//! own configured connectors, drives the agent through the **production** path
//! ([`crate::drive`] — which already appends the agent's scoped
//! `--allowed-mcp-server-names` allow-list in Answer mode so its MCP read-tools
//! fire headless), and classifies whether the answer looks like real tool output
//! or a model-only dodge.
//!
//! Design (mirrors the rest of the crate):
//! - **Data-driven, not `if agent ==`.** The connector→probe mapping is a static
//!   table ([`PROBE_RULES`]) keyed by connector-name pattern, each row carrying a
//!   read-only question and a [`Verifier`] spec. Adding a connector family is a
//!   new row, never a new code path.
//! - **Read-only only.** Every probe question is side-effect-free (list / get /
//!   read). There is intentionally no write/delete/send probe anywhere in the
//!   table — a connector with no safe read probe is honestly skipped.
//! - **Bluey's own AI is never used.** The probe is answered by *the user's*
//!   agent firing *the user's* connectors; this module only selects, drives, and
//!   classifies.
//!
//! `run_mcp_probe` is REAL: no mock, no fixture, no asserted request shape. It
//! spawns the agent and reports what actually came back. It therefore spends a
//! real model call and is never part of any read-only `prove_all()`; it runs
//! only on explicit request, exactly like [`prove_drive`].
//!
//! [`prove_drive`]: crate::prove_drive

use std::time::{Duration, Instant};

use futures_util::StreamExt;

use crate::connectors::Connector;
use crate::{discover_agents, AgentKind, AnswerChunk, Question};

/// Per-probe wall-clock cap. A tool round-trip (web search, a DB list, a price
/// lookup) is slower than a one-word echo but still completes well within this;
/// past it we record a timeout rather than hang.
const PROBE_TIMEOUT: Duration = Duration::from_secs(90);

/// Max chars of answer text retained in a result (keeps reports/logs bounded;
/// the full body is never needed once classified).
const ANSWER_CAP: usize = 400;

// ---------------------------------------------------------------------------
// Verification spec
// ---------------------------------------------------------------------------

/// How to decide whether an answer is *live tool output* rather than a
/// model-only dodge.
///
/// Kept as **data** (an enum), not a `fn` pointer, so [`PROBE_RULES`] stays a
/// pure `const` table and every verifier is unit-testable in isolation. The
/// shared model-dodge guard (see [`looks_like_model_dodge`]) is applied to
/// *every* variant by [`Verifier::verify`], so an answer like "I don't have
/// access to real-time data" fails regardless of which rule matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verifier {
    /// Answer must contain a version-like token (`\d+\.\d+`, e.g. `1.96.0`).
    /// Used for web-search probes: the model alone cannot reliably know the
    /// *current* stable version, so a concrete version token is strong evidence
    /// the search tool actually ran.
    VersionToken,
    /// Answer must contain a price-like number (a bare decimal such as
    /// `214.30`, optionally `$`-prefixed). Used for market-data probes.
    PriceNumber,
    /// Answer must be non-trivial free text of at least `min_len` chars after
    /// trimming (used when the live payload is prose, e.g. a repo description).
    NonEmptyText { min_len: usize },
    /// Answer must look like a comma-separated list (≥2 comma-separated,
    /// non-empty items). Used for "list the table names" style probes.
    CommaSeparatedList,
}

impl Verifier {
    /// Decide whether `answer` is live tool output under this spec.
    ///
    /// Two gates, both must pass:
    /// 1. The answer must NOT look like a model-only dodge ("I can't access
    ///    real-time data", "as an AI…", "I don't have a … tool"). This gate is
    ///    shared across every variant so a refusal never counts as a pass.
    /// 2. The answer must match this variant's positive shape.
    pub fn verify(&self, answer: &str) -> bool {
        let trimmed = answer.trim();
        if trimmed.is_empty() {
            return false;
        }
        if looks_like_model_dodge(trimmed) {
            return false;
        }
        match self {
            Verifier::VersionToken => contains_version_token(trimmed),
            Verifier::PriceNumber => contains_price_number(trimmed),
            Verifier::NonEmptyText { min_len } => trimmed.chars().count() >= *min_len,
            Verifier::CommaSeparatedList => looks_like_comma_list(trimmed),
        }
    }
}

// ---------------------------------------------------------------------------
// The connector → probe table (data, not branches)
// ---------------------------------------------------------------------------

/// How a [`ProbeRule`] matches a connector name.
///
/// Connector names vary across agents for the same underlying service
/// (`perplexity` vs `perplexity-ask`; `supabase-divini` vs `supabase-cookwise`),
/// so rules match by pattern, not exact string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameMatch {
    /// Case-insensitive exact match on the connector name.
    Exact(&'static str),
    /// Case-insensitive substring match (e.g. `"supabase"` matches
    /// `supabase-whatsapp-bot`). Anchored loosely on purpose.
    Contains(&'static str),
}

impl NameMatch {
    fn matches(&self, name: &str) -> bool {
        let lname = name.to_ascii_lowercase();
        match self {
            NameMatch::Exact(s) => lname == s.to_ascii_lowercase(),
            NameMatch::Contains(s) => lname.contains(&s.to_ascii_lowercase()),
        }
    }
}

/// One row of the probe table: which connectors it applies to, the read-only
/// question to ask, how to verify the answer, and a clarity rank used to pick
/// the *best* probe when an agent has several testable connectors.
#[derive(Debug, Clone, Copy)]
pub struct ProbeRule {
    /// Connector-name patterns this rule covers. A connector matches the rule if
    /// it matches ANY pattern here.
    pub patterns: &'static [NameMatch],
    /// The read-only question to put to the agent. Phrased to force the tool and
    /// to elicit a terse, machine-checkable answer.
    pub question: &'static str,
    /// How to recognize a live answer.
    pub verifier: Verifier,
    /// Higher = clearer live-vs-training signal. Web search ranks highest
    /// because the *current* stable version is the canonical "model can't know
    /// this from training" probe; market data is next; opaque list/description
    /// probes rank lower. [`pick_probe`] prefers the highest-rank testable
    /// connector.
    pub clarity: u8,
    /// A short, stable family label for reporting (e.g. `"web-search"`).
    pub family: &'static str,
}

/// The probe table. **Adding a testable connector family = adding a row here.**
///
/// All probes are READ-ONLY (list / get / read). There is deliberately no write,
/// delete, or send probe. Connectors with no safe read probe (e.g. `retellai`,
/// whose useful surface places calls / mutates agents) are simply absent — they
/// fall through to "no testable connector" and are honestly skipped rather than
/// poked with a risky call.
pub const PROBE_RULES: &[ProbeRule] = &[
    // Web search — the clearest signal. The model cannot reliably know the
    // CURRENT stable Rust version from training, so a concrete version token is
    // strong evidence the search tool actually fired. (PROVEN by hand:
    // antigravity + perplexity-ask answered "1.96.0, May 28, 2026".)
    ProbeRule {
        patterns: &[
            NameMatch::Contains("perplexity"),
            NameMatch::Contains("web-search"),
            NameMatch::Contains("websearch"),
            NameMatch::Contains("brave-search"),
            NameMatch::Contains("tavily"),
        ],
        question: "Use your web search tool to find the latest stable Rust compiler \
                   version and its release date. Reply with just the version and date.",
        verifier: Verifier::VersionToken,
        clarity: 100,
        family: "web-search",
    },
    // Market data — a live price is a moving number the model cannot know.
    ProbeRule {
        patterns: &[NameMatch::Contains("tradingview")],
        question: "Use your TradingView tool to get the current price of AAPL. \
                   Reply with just the number.",
        verifier: Verifier::PriceNumber,
        clarity: 80,
        family: "market-data",
    },
    // GitHub — a real repo description is live API output. `rust-lang/rust` is a
    // safe, public, read-only target.
    ProbeRule {
        patterns: &[NameMatch::Exact("github")],
        question: "Use your GitHub tool to report the description of the repository \
                   'rust-lang/rust'. Reply with just the description text.",
        verifier: Verifier::NonEmptyText { min_len: 12 },
        clarity: 60,
        family: "github",
    },
    // Context7 — read-only docs lookup. Ask it to resolve a well-known library;
    // a real lookup returns prose the model wouldn't invent verbatim.
    ProbeRule {
        patterns: &[NameMatch::Contains("context7")],
        question: "Use your Context7 tool to fetch a one-sentence description of the \
                   'react' library. Reply with just that sentence.",
        verifier: Verifier::NonEmptyText { min_len: 12 },
        clarity: 40,
        family: "docs",
    },
    // Supabase — list table names. Read-only and side-effect-free. Lowest clarity
    // because the shape (a comma list) is the weakest live signal, but it still
    // proves the DB tool answered with this project's real schema.
    ProbeRule {
        patterns: &[NameMatch::Contains("supabase")],
        question: "Use your Supabase tool to list the table names in the database. \
                   Reply with a comma-separated list of table names only.",
        verifier: Verifier::CommaSeparatedList,
        clarity: 30,
        family: "supabase",
    },
];

/// Find the probe rule (if any) that covers a connector by name. Returns the
/// FIRST matching row in table order; table order is therefore significant only
/// among rules that could match the same name (none do today — the patterns are
/// disjoint), so in practice the match is unambiguous.
fn rule_for_connector(name: &str) -> Option<&'static ProbeRule> {
    PROBE_RULES
        .iter()
        .find(|r| r.patterns.iter().any(|p| p.matches(name)))
}

// ---------------------------------------------------------------------------
// Probe selection
// ---------------------------------------------------------------------------

/// A concrete, runnable probe: which connector it exercises, the family label,
/// the exact question, and how the answer is verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpProbe {
    /// The agent's own connector name this probe drives through (e.g.
    /// `perplexity-ask`). Surfaced so callers/reports can name the exact tool.
    pub connector: String,
    /// Stable family label from the matched rule (e.g. `"web-search"`).
    pub family: &'static str,
    /// The read-only question put to the agent.
    pub question: &'static str,
    /// How the answer is classified as live vs. dodge.
    pub verifier: Verifier,
}

/// Read the agent's real, configured MCP connectors (shape + tier only — never
/// secrets), reusing the crate's discovery + connector reader.
///
/// This is the *same* path the drive layer uses to build its scoped MCP
/// allow-list ([`crate::drive`]'s `mcp_allow_args_for_agent`): locate the agent
/// among [`discover_agents`], follow its `connector_config_path`, and parse it
/// with [`crate::read_connectors`]. Fully read-only and fail-soft — an agent
/// that is not installed, has no config, or has an unparseable config yields an
/// empty `Vec`.
pub fn connectors_for(agent: &AgentKind) -> Vec<Connector> {
    discover_agents()
        .into_iter()
        .find(|d| &d.kind == agent)
        .and_then(|d| d.connector_config_path)
        .map(|cfg| crate::read_connectors(&cfg))
        .unwrap_or_default()
}

/// Choose the best testable probe for an agent from its own configured
/// connectors, or `None` if none of them have a known read-only probe (an honest
/// skip — not every agent has a probeable connector).
///
/// Selection is data-driven: every connector is mapped through [`PROBE_RULES`],
/// and the candidate with the highest [`ProbeRule::clarity`] wins (ties broken
/// by configured order, which is stable). This naturally prefers a web-search /
/// perplexity connector — the clearest live-vs-training signal — and falls back
/// to the next-clearest known-testable connector otherwise.
pub fn pick_probe(agent: &AgentKind) -> Option<McpProbe> {
    pick_probe_from(&connectors_for(agent))
}

/// Pure core of [`pick_probe`]: choose the best probe from an explicit connector
/// list. Separated from discovery IO so selection is unit-testable against
/// synthetic connector lists (the real per-agent lists are machine-specific).
pub fn pick_probe_from(connectors: &[Connector]) -> Option<McpProbe> {
    connectors
        .iter()
        .filter_map(|c| rule_for_connector(&c.name).map(|rule| (c, rule)))
        // Highest clarity wins; `max_by_key` keeps the LAST max on ties, so to
        // make ties resolve to the first-configured connector we compare on
        // (clarity, reversed index) — but a plain stable pick is enough here:
        // iterate and keep the first strictly-greater candidate.
        .fold(
            None::<(&Connector, &'static ProbeRule)>,
            |best, cand| match best {
                Some((_, brule)) if brule.clarity >= cand.1.clarity => best,
                _ => Some(cand),
            },
        )
        .map(|(c, rule)| McpProbe {
            connector: c.name.clone(),
            family: rule.family,
            question: rule.question,
            verifier: rule.verifier,
        })
}

// ---------------------------------------------------------------------------
// The real probe run
// ---------------------------------------------------------------------------

/// The honest outcome of one real MCP probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpProbeResult {
    /// The connector fired and returned live data: the agent answered AND the
    /// answer passed verification (looks like real tool output, not a dodge).
    Fired {
        /// The connector that was exercised.
        connector: String,
        /// Family label of the probe (e.g. `"web-search"`).
        family: &'static str,
        /// The (truncated) real answer text.
        answer: String,
        elapsed_ms: u64,
    },
    /// The agent answered, but the answer does NOT look like live tool output
    /// (failed verification or read like a model-only dodge). The connector may
    /// not have fired, or fired and returned nothing usable. Honest "alive but
    /// unproven" — never silently upgraded to a pass.
    NoLiveData {
        connector: String,
        family: &'static str,
        answer: String,
        elapsed_ms: u64,
    },
    /// The agent has no connector with a known read-only probe. No model call
    /// was made (honest skip).
    NoProbe,
    /// The drive itself failed (not signed in, rate limited, binary missing,
    /// timeout, …). `reason` is the REAL error text from the agent/CLI.
    Failed { reason: String, elapsed_ms: u64 },
}

impl McpProbeResult {
    /// A short status marker for matrices/reports.
    pub fn marker(&self) -> &'static str {
        match self {
            McpProbeResult::Fired { .. } => "FIRED ✅",
            McpProbeResult::NoLiveData { .. } => "no live data 🟡",
            McpProbeResult::NoProbe => "no probe ⬜",
            McpProbeResult::Failed { .. } => "FAILED 🔴",
        }
    }
}

/// Drive the user's agent with an auto-picked read-only MCP probe and classify
/// what comes back. **REAL**: spawns the agent through the production
/// [`crate::drive`] path (which appends the agent's scoped
/// `--allowed-mcp-server-names` allow-list in Answer mode, so MCP read-tools
/// fire headless) and consumes the real streamed answer.
///
/// Spends a real model call against the user's account — call only on explicit
/// request. Never panics: every failure mode (no probe, spawn failure, terminal
/// error, timeout, empty answer) is folded into an [`McpProbeResult`].
pub async fn run_mcp_probe(agent: &AgentKind) -> McpProbeResult {
    let Some(probe) = pick_probe(agent) else {
        return McpProbeResult::NoProbe;
    };
    run_picked_probe(agent.clone(), probe).await
}

/// Run an already-picked probe against an agent. Split out so a caller (e.g. the
/// prove-drive matrix) that already selected a probe — or that wants to report
/// the chosen connector before paying for the call — can reuse the exact drive +
/// classify logic without re-running selection.
pub async fn run_picked_probe(agent: AgentKind, probe: McpProbe) -> McpProbeResult {
    let started = Instant::now();
    let question = Question::new(probe.question);

    // Production drive path. In Answer mode (the default) it appends the agent's
    // own scoped MCP allow-list, so its read-tools are auto-approved headless
    // while file/shell write tools stay gated.
    let stream = match crate::drive(agent, question).await {
        Ok(s) => s,
        Err(e) => {
            return McpProbeResult::Failed {
                reason: format!("could not start: {e:#}"),
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
        }
    };

    futures_util::pin_mut!(stream);
    let mut body = String::new();

    loop {
        let Some(remaining) = PROBE_TIMEOUT.checked_sub(started.elapsed()) else {
            return McpProbeResult::Failed {
                reason: format!(
                    "timed out after {}s with no terminal event",
                    PROBE_TIMEOUT.as_secs()
                ),
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
        };

        match tokio::time::timeout(remaining, stream.next()).await {
            Err(_) => {
                return McpProbeResult::Failed {
                    reason: format!("timed out after {}s", PROBE_TIMEOUT.as_secs()),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                };
            }
            Ok(None) => break,
            Ok(Some(chunk)) => match chunk {
                AnswerChunk::Started { .. } => {}
                AnswerChunk::Delta(d) => body.push_str(&d),
                AnswerChunk::Done { .. } => break,
                AnswerChunk::Error(message) => {
                    return McpProbeResult::Failed {
                        reason: message,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    };
                }
            },
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return McpProbeResult::Failed {
            reason: "returned no answer (empty stream)".to_string(),
            elapsed_ms,
        };
    }

    let answer = truncate(trimmed, ANSWER_CAP);
    if probe.verifier.verify(trimmed) {
        McpProbeResult::Fired {
            connector: probe.connector,
            family: probe.family,
            answer,
            elapsed_ms,
        }
    } else {
        McpProbeResult::NoLiveData {
            connector: probe.connector,
            family: probe.family,
            answer,
            elapsed_ms,
        }
    }
}

// ---------------------------------------------------------------------------
// Verification primitives (hand-rolled — no `regex` dep added)
// ---------------------------------------------------------------------------

/// True when `text` reads like the model declining / hedging instead of using a
/// tool. This is the central guard that stops a polished refusal ("I don't have
/// access to real-time data") from ever counting as a live answer.
///
/// Conservative on purpose: it only fires on phrases that are *characteristic of
/// a non-tool answer*. A real tool answer ("1.96.0, released May 28 2026") never
/// contains these, so the guard does not produce false negatives on live data.
fn looks_like_model_dodge(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    const DODGE_MARKERS: &[&str] = &[
        "don't have access",
        "do not have access",
        "don't have real-time",
        "do not have real-time",
        "no access to real-time",
        "can't access",
        "cannot access",
        "unable to access",
        "can't browse",
        "cannot browse",
        "unable to browse",
        "don't have the ability",
        "do not have the ability",
        "as an ai",
        "as a language model",
        "i cannot provide real-time",
        "i can't provide real-time",
        "my training data",
        "my knowledge cutoff",
        "knowledge cutoff",
        "as of my last update",
        "i don't have a tool",
        "i do not have a tool",
        "no tool available",
        "don't have that tool",
        "do not have that tool",
        "isn't configured",
        "is not configured",
        "not connected",
    ];
    DODGE_MARKERS.iter().any(|m| t.contains(m))
}

/// True when `text` contains a version-like token: a run of digits, a `.`, and
/// at least one more digit (`\d+\.\d+`). Matches `1.96`, `1.96.0`, `v2.0.1`.
fn contains_version_token(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            // consume the integer part
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            // need a dot directly after the digits, then another digit
            if i < bytes.len() && bytes[i] == b'.' {
                let after_dot = i + 1;
                if after_dot < bytes.len() && bytes[after_dot].is_ascii_digit() {
                    return true;
                }
            }
            // not a version; ensure forward progress past this digit run
            if i == start {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    false
}

/// True when `text` contains a price-like number: a decimal with a fractional
/// part (`214.30`, `$214.3`) OR a `$`-prefixed integer (`$214`). A bare integer
/// with no `$` is NOT accepted, so a dodge that merely mentions a year ("2026")
/// or a count is not misread as a price. A version token like `1.96.0` would
/// satisfy "has a decimal", so prices are only accepted when not part of a
/// dotted-triple version — but in practice the price probe's answer is a single
/// number, and the model-dodge guard already runs first.
fn contains_price_number(text: &str) -> bool {
    let bytes = text.as_bytes();
    // Case A: a decimal number `<digits>.<digits>` (e.g. 214.30).
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                let after = i + 1;
                if after < bytes.len() && bytes[after].is_ascii_digit() {
                    return true;
                }
            }
            if i == start {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    // Case B: a `$`-prefixed integer (e.g. $214) with no decimal.
    let chars: Vec<char> = text.chars().collect();
    for (idx, &c) in chars.iter().enumerate() {
        if c == '$' {
            // skip optional spaces
            let mut j = idx + 1;
            while j < chars.len() && chars[j] == ' ' {
                j += 1;
            }
            if j < chars.len() && chars[j].is_ascii_digit() {
                return true;
            }
        }
    }
    false
}

/// True when `text` looks like a comma-separated list of at least two non-empty
/// items (the shape of a "list the table names" answer). Tolerates a trailing
/// period and surrounding whitespace.
fn looks_like_comma_list(text: &str) -> bool {
    let cleaned = text.trim().trim_end_matches('.');
    let items: Vec<&str> = cleaned
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    items.len() >= 2
}

/// Collapse whitespace to single spaces and cap to `max` chars (char-boundary
/// safe), for bounded answers in results/reports.
fn truncate(text: &str, max: usize) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        one_line
    } else {
        let mut t: String = one_line.chars().take(max).collect();
        t.push('…');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::{AuthTier, Connector, Transport};

    /// Build a synthetic stdio connector with the given name (transport/tier are
    /// irrelevant to probe selection, which keys on the name only).
    fn conn(name: &str) -> Connector {
        Connector {
            name: name.to_string(),
            transport: Transport::Stdio {
                command: "x".to_string(),
                args: vec![],
            },
            auth_tier: AuthTier::None_,
        }
    }

    // ---- probe selection: real per-agent connector sets -----------------

    #[test]
    fn picks_perplexity_for_antigravity_connector_set() {
        // antigravity's real connectors on this machine (from
        // `bluey agent connectors antigravity`).
        let connectors = [
            conn("github"),
            conn("perplexity-ask"),
            conn("supabase-divini"),
            conn("supabase-heyloo"),
            conn("supabase-itvg"),
            conn("supabase-optimcruit-ai"),
            conn("supabase-whatsapp-bot"),
            conn("tradingview"),
        ];
        let probe = pick_probe_from(&connectors).expect("a probe should be picked");
        // Web search is the clearest signal → perplexity-ask wins over github,
        // tradingview, and the supabases.
        assert_eq!(probe.connector, "perplexity-ask");
        assert_eq!(probe.family, "web-search");
        assert_eq!(probe.verifier, Verifier::VersionToken);
    }

    #[test]
    fn picks_github_for_gemini_connector_set() {
        // gemini's only real connector on this machine is github.
        let connectors = [conn("github")];
        let probe = pick_probe_from(&connectors).expect("a probe should be picked");
        assert_eq!(probe.connector, "github");
        assert_eq!(probe.family, "github");
        assert_eq!(probe.verifier, Verifier::NonEmptyText { min_len: 12 });
    }

    #[test]
    fn picks_perplexity_for_cursor_connector_set() {
        // cursor's real connectors include perplexity, context7, tradingview,
        // supabase-cookwise, retellai — web search must still win.
        let connectors = [
            conn("context7"),
            conn("perplexity"),
            conn("retellai-mcp-server"),
            conn("supabase-cookwise"),
            conn("tradingview"),
        ];
        let probe = pick_probe_from(&connectors).expect("a probe should be picked");
        assert_eq!(probe.connector, "perplexity");
        assert_eq!(probe.family, "web-search");
    }

    #[test]
    fn prefers_higher_clarity_regardless_of_order() {
        // tradingview appears BEFORE perplexity here; web search must still win
        // because selection is by clarity, not position.
        let connectors = [conn("tradingview"), conn("perplexity-ask"), conn("github")];
        let probe = pick_probe_from(&connectors).unwrap();
        assert_eq!(probe.family, "web-search");
    }

    #[test]
    fn falls_back_to_market_data_when_no_web_search() {
        let connectors = [conn("github"), conn("tradingview")];
        let probe = pick_probe_from(&connectors).unwrap();
        // tradingview (clarity 80) beats github (clarity 60).
        assert_eq!(probe.family, "market-data");
        assert_eq!(probe.connector, "tradingview");
    }

    #[test]
    fn none_when_no_testable_connector() {
        // retellai has no safe read-only probe (its surface mutates / places
        // calls) → honest skip.
        let connectors = [conn("retellai-mcp-server")];
        assert!(pick_probe_from(&connectors).is_none());
    }

    #[test]
    fn none_for_empty_connector_set() {
        assert!(pick_probe_from(&[]).is_none());
    }

    #[test]
    fn supabase_variants_all_match_by_substring() {
        for name in [
            "supabase-divini",
            "supabase-whatsapp-bot",
            "supabase-cookwise",
            "supabase DIVINI MCP",
        ] {
            let probe = pick_probe_from(&[conn(name)])
                .unwrap_or_else(|| panic!("{name} should map to a probe"));
            assert_eq!(probe.family, "supabase", "for {name}");
            assert_eq!(probe.verifier, Verifier::CommaSeparatedList);
        }
    }

    // ---- verification: accepts real tool output, rejects model dodge ----

    #[test]
    fn version_verifier_accepts_real_looking_answer() {
        let v = Verifier::VersionToken;
        // The exact hand-proven antigravity answer.
        assert!(v.verify("1.96.0, May 28 2026"));
        assert!(v.verify("The latest stable Rust is 1.96.0 (released 2026-05-28)."));
        assert!(v.verify("v1.85"));
    }

    #[test]
    fn version_verifier_rejects_model_only_dodge() {
        let v = Verifier::VersionToken;
        // The canonical dodge — even though it contains no version, the dodge
        // guard alone must reject it.
        assert!(!v.verify("I don't have access to real-time data."));
        assert!(!v.verify(
            "As an AI, I cannot provide real-time information. As of my last update, \
             the version was around 1.80."
        ));
        // A bare refusal with no number.
        assert!(!v.verify("I'm unable to browse the web."));
        // Empty / whitespace.
        assert!(!v.verify("   "));
    }

    #[test]
    fn dodge_guard_overrides_a_version_token_in_a_refusal() {
        // Even if a refusal mentions a stale version number, the dodge markers
        // ("knowledge cutoff", "as of my last update") must veto it — we are not
        // fooled by a number embedded in a hedge.
        let v = Verifier::VersionToken;
        assert!(!v.verify(
            "As of my last update (knowledge cutoff), the latest was 1.80.0, but I \
             can't access real-time data to confirm the current version."
        ));
    }

    #[test]
    fn price_verifier_accepts_numbers_rejects_dodge() {
        let v = Verifier::PriceNumber;
        assert!(v.verify("214.30"));
        assert!(v.verify("$214"));
        assert!(v.verify("The current price is $213.55."));
        assert!(!v.verify("I can't access real-time market data."));
        // A lone year is not a price.
        assert!(!v.verify("2026"));
    }

    #[test]
    fn nonempty_text_verifier_accepts_description_rejects_dodge() {
        let v = Verifier::NonEmptyText { min_len: 12 };
        assert!(v.verify("Empowering everyone to build reliable and efficient software."));
        // Too short to be a real description.
        assert!(!v.verify("ok"));
        // A refusal of adequate length is still rejected by the dodge guard.
        assert!(!v.verify("I do not have access to that tool right now, sorry."));
    }

    #[test]
    fn comma_list_verifier_accepts_list_rejects_prose_and_dodge() {
        let v = Verifier::CommaSeparatedList;
        assert!(v.verify("users, sessions, messages, audit_log"));
        assert!(v.verify("users, sessions.")); // trailing period tolerated
                                               // A single token is not a list.
        assert!(!v.verify("users"));
        // A dodge is rejected even if it happens to contain a comma.
        assert!(!v.verify("I'm sorry, I cannot access the database."));
    }

    // ---- verification primitives (unit) ---------------------------------

    #[test]
    fn version_token_scanner_matches_expected_shapes() {
        assert!(contains_version_token("1.96"));
        assert!(contains_version_token("1.96.0"));
        assert!(contains_version_token("rustc 1.85.1 (abc 2026)"));
        assert!(!contains_version_token("version one point nine"));
        assert!(!contains_version_token("2026")); // no dotted fraction
        assert!(!contains_version_token("ends with dot 1."));
        assert!(!contains_version_token(""));
    }

    #[test]
    fn price_scanner_distinguishes_price_from_plain_integer() {
        assert!(contains_price_number("214.30"));
        assert!(contains_price_number("$5"));
        assert!(!contains_price_number("100")); // bare integer, no $ and no decimal
        assert!(!contains_price_number("no number here"));
    }

    #[test]
    fn dodge_detector_flags_common_refusals_only() {
        assert!(looks_like_model_dodge(
            "I don't have access to real-time data"
        ));
        assert!(looks_like_model_dodge(
            "As an AI language model, I cannot browse."
        ));
        assert!(looks_like_model_dodge("That MCP server is not connected."));
        // A genuine tool answer must NOT be flagged.
        assert!(!looks_like_model_dodge("1.96.0, released May 28 2026"));
        assert!(!looks_like_model_dodge("users, sessions, messages"));
    }

    // ---- result markers --------------------------------------------------

    #[test]
    fn result_markers_are_distinct() {
        let fired = McpProbeResult::Fired {
            connector: "perplexity-ask".into(),
            family: "web-search",
            answer: "1.96.0".into(),
            elapsed_ms: 10,
        };
        let nolive = McpProbeResult::NoLiveData {
            connector: "perplexity-ask".into(),
            family: "web-search",
            answer: "I can't access real-time data".into(),
            elapsed_ms: 10,
        };
        assert_eq!(fired.marker(), "FIRED ✅");
        assert_eq!(nolive.marker(), "no live data 🟡");
        assert_eq!(McpProbeResult::NoProbe.marker(), "no probe ⬜");
        assert_ne!(fired.marker(), nolive.marker());
    }

    #[test]
    fn every_probe_rule_question_names_a_tool_and_is_read_only() {
        // Guardrail: each rule's question must instruct tool use and must not
        // contain any write/destructive verb. This keeps the table honest as
        // rows are added.
        const FORBIDDEN: &[&str] = &[
            "delete", "drop ", "insert", "update ", "write", "send", "create", "remove", "truncate",
        ];
        for rule in PROBE_RULES {
            let q = rule.question.to_ascii_lowercase();
            assert!(
                q.contains("use your"),
                "rule {} must instruct tool use: {:?}",
                rule.family,
                rule.question
            );
            for bad in FORBIDDEN {
                assert!(
                    !q.contains(bad),
                    "rule {} has a non-read-only verb {:?}: {:?}",
                    rule.family,
                    bad,
                    rule.question
                );
            }
        }
    }

    #[test]
    fn truncate_collapses_and_caps() {
        assert_eq!(truncate("  a   b ", 80), "a b");
        let long = "x".repeat(600);
        assert!(truncate(&long, ANSWER_CAP).ends_with('…'));
    }
}
