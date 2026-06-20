//! Format-drift canary — the production-maintenance backbone for the session
//! readers (H3).
//!
//! Session reading has no uniform vendor protocol, so every reader reverse-
//! engineers an undocumented on-disk format. Vendors change those formats on
//! roughly every major/minor, and the change is silent: a fail-soft reader just
//! returns fewer (or zero) sessions, which looks identical to "the user has no
//! sessions." This test is the early-warning system: it locates the REAL stores
//! present on this machine and asserts each reader still understands them —
//! `is_total_drift()` is false, and when a store has real content the parse
//! ratio clears a floor. If a vendor ships a breaking format change, THIS test
//! goes red in CI, naming the agent and the store, instead of the breakage
//! reaching users as silently-missing history.
//!
//! It is deliberately tolerant about absence: an agent with no store on the
//! current machine is SKIPPED (logged), never failed — CI runners and dev boxes
//! have different agents installed. The test only asserts on stores that
//! actually exist here, so it is a true canary (catches real drift on whatever
//! is installed) without being flaky on minimal environments.

use cue_agent_bridge::{discover_agents, reader_for, ReaderHealth};

/// Parse-ratio floor for a store that has real content. Below this, the reader
/// is understanding so little of a populated store that a format change is the
/// likely cause — worth a human look even short of total drift. Generous on
/// purpose (legitimately-empty drafts are already excluded from the ratio by the
/// per-reader `health()` overrides), so a healthy store sits at or near 1.0.
const PARSE_RATIO_FLOOR: f64 = 0.5;

#[test]
fn real_stores_on_this_machine_are_not_drifted() {
    let agents = discover_agents();

    let mut probed = 0usize;
    let mut skipped: Vec<String> = Vec::new();

    for agent in &agents {
        let Some(store) = agent.session_store.as_ref() else {
            skipped.push(format!("{:?} (no session store on disk)", agent.kind));
            continue;
        };
        let reader = reader_for(store.format);
        let health = reader.health(store);

        // The core canary assertion: a store that exists on disk must never look
        // like total drift (raw records present, none understood). If this fires,
        // the on-disk format for this agent very likely changed.
        assert!(
            !health.is_total_drift(),
            "FORMAT DRIFT for {:?} at {}: the store holds raw records but the \
             reader parsed NONE — the on-disk format likely changed. health={:?}",
            agent.kind,
            store.path.display(),
            health,
        );

        match health {
            ReaderHealth::EmptyStore => {
                skipped.push(format!("{:?} (store present but empty)", agent.kind));
            }
            ReaderHealth::Parsed { parsed, raw_total } => {
                let ratio = health.parse_ratio();
                println!(
                    "canary: {:?} at {} — parsed {parsed}/{raw_total} (ratio {ratio:.2})",
                    agent.kind,
                    store.path.display(),
                );
                // Only enforce the floor on stores with real content; an
                // all-empty-draft store reports raw_total 0 → EmptyStore above.
                if raw_total > 0 {
                    assert!(
                        ratio >= PARSE_RATIO_FLOOR,
                        "LOW PARSE RATIO for {:?} at {}: parsed {parsed}/{raw_total} \
                         (ratio {ratio:.2}) is below the {PARSE_RATIO_FLOOR} floor — \
                         the reader is understanding too little of a populated store, \
                         which usually means a partial format change.",
                        agent.kind,
                        store.path.display(),
                    );
                    probed += 1;
                }
            }
        }
    }

    println!(
        "canary: probed {probed} real store(s) with content; skipped {}: {}",
        skipped.len(),
        skipped.join(", ")
    );

    // The test passes cleanly on a machine with no agents installed (pure CI
    // runner) — it is a canary, not a presence requirement. It only ever FAILS
    // when a store that IS present has drifted. `probed == 0` is therefore a
    // valid (skipped-everything) outcome, not an error.
}
