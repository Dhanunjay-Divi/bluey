// The meeting overlay shell — the expanded panel: header (brand · via-agent ·
// tabs · close) over the active tab. First run shows onboarding; after that the
// live Ask loop. One window, state-driven views (no router) — lean by design.

import { useEffect, useRef, useState } from "react";
import { getClient } from "./lib";
import { useDragHeader } from "./lib/useDragHeader";
import { useCollapse } from "./lib/useCollapse";
import { useDataStore } from "./lib/dataStore";
import {
  Glass,
  Mark,
  ResizeGrip,
  SegmentedTabs,
  Waveform,
} from "./components/primitives";
import { Pill } from "./components/Pill";
import { AskScreen } from "./screens/AskScreen";
import { HistoryTab } from "./screens/HistoryTab";
import { AgentsScreen } from "./screens/AgentsScreen";
import { Onboarding } from "./screens/Onboarding";

// Audio controls live in the composer "+" menu now, not a separate tab.
// "History" holds TWO lenses under a sub-toggle: Meetings (my past meetings +
// their transcript/Q&A) and Sessions (resume a Claude/Cursor agent thread).
// "Agents" stays its own tab: attach/detach agents (+ its embedded past
// sessions). Three top tabs, two lenses inside History.
type Tab = "Ask" | "History" | "Agents";
const TABS: readonly Tab[] = ["Ask", "History", "Agents"];

export function App() {
  const client = getClient();
  const [onboarding, setOnboarding] = useState(
    () => !localStorage.getItem("bluey.onboarded"),
  );
  const [tab, setTab] = useState<Tab>("Ask");
  // Agents + attach/detach come from the shared SWR store (cached across tab
  // switches, kept live by the daemon's set_agents push) — no per-mount refetch.
  const { agents, attached, attach, detach } = useDataStore();
  const [connectors, setConnectors] = useState<string[]>([]);
  // Drag the frameless panel by its header (no titlebar to grab).
  const headerRef = useRef<HTMLDivElement>(null);
  useDragHeader(headerRef);

  // Collapse to a compact pill (X) / re-expand (click the pill).
  const { collapsed, collapse, expand } = useCollapse();
  const pillRef = useRef<HTMLDivElement>(null);
  useDragHeader(pillRef);

  // Real connectors for the attached agent — the footer lists the actual ready
  // ones, never hardcoded brand names.
  useEffect(() => {
    let live = true;
    if (!attached) {
      setConnectors([]);
      return;
    }
    client
      .connectors(attached.kind)
      .then(
        (cs) =>
          live && setConnectors(cs.filter((c) => c.ready).map((c) => c.name)),
      )
      .catch(() => live && setConnectors([]));
    return () => {
      live = false;
    };
  }, [client, attached]);

  return (
    // The panel is ALWAYS mounted — collapse hides it via display:none, it does
    // NOT unmount it. Unmounting used to destroy AskScreen's state (the collapse
    // bug); the session state now lives in MeetingProvider above <App/>, but we
    // also keep the panel mounted so expanding is an instant repaint with no
    // remount cost. The Pill (collapsed) and Onboarding (first run) render as
    // siblings, never replacing the panel subtree.
    <>
      {/* Collapsed: the ambient pill — live listening status + latest heard line
          + mic toggle, so the user rarely needs to expand mid-meeting. Click to
          expand. */}
      {collapsed && (
        <Pill
          client={client}
          attached={attached}
          onExpand={() => void expand()}
          dragRef={pillRef}
        />
      )}

      {/* First-run onboarding, rendered as a sibling and only while expanded. */}
      {onboarding && !collapsed && (
        <Onboarding
          agents={agents}
          onAttach={(k) => void attach(k)}
          onDone={() => {
            localStorage.setItem("bluey.onboarded", "1");
            setOnboarding(false);
          }}
        />
      )}

      {/* The panel FILLS the window (pinned to all edges with a small margin for
          the soft shadow) — like the interview overlay — so there's no empty
          space around it. The middle tab content flexes + scrolls inside.
          display:none (not visibility:hidden) when collapsed or onboarding so the
          heavy transcript list skips layout/paint and captures no pointer events,
          while AskScreen stays mounted. */}
      <div
        style={{
          position: "fixed",
          inset: 7,
          display: collapsed || onboarding ? "none" : "flex",
        }}
      >
        <Glass
          radius="var(--r-xl)"
          style={{
            flex: 1,
            display: "flex",
            flexDirection: "column",
            minHeight: 0,
          }}
        >
          {/* header — drag region for the frameless panel (drag to move) */}
          <div
            ref={headerRef}
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              padding: "13px 15px",
              cursor: "grab",
            }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: 9 }}>
              <span
                style={{ display: "inline-flex", alignItems: "center", gap: 8 }}
              >
                {attached ? <Waveform /> : <Mark />}
                <span
                  style={{
                    fontSize: 14,
                    fontWeight: 600,
                    letterSpacing: "-.01em",
                  }}
                >
                  Bluey
                </span>
              </span>
              <span style={{ fontSize: 12.5, color: "var(--ink-3)" }}>
                ·{" "}
                {attached ? (
                  <b style={{ color: "var(--tint-ink)", fontWeight: 540 }}>
                    {attached.displayName}
                  </b>
                ) : (
                  "managed"
                )}
              </span>
            </div>
            <SegmentedTabs tabs={TABS} value={tab} onChange={setTab} />
            <button
              aria-label="Collapse to pill"
              title="Collapse to pill"
              style={closeBtn}
              onClick={() => void collapse()}
            >
              ×
            </button>
          </div>

          {/* Middle: the active tab flexes to fill between header + footer and
            scrolls internally (so the panel fills the window, no empty space). */}
          <div
            style={{
              // Fills between header + footer; each tab's screen (AskScreen,
              // HistoryTab, AgentsScreen) is flex:1 and owns its OWN internal
              // scroll — so this frame must NOT scroll too (a double
              // scroll-in-scroll stranded lists at a short height and left dead
              // space at the bottom).
              flex: 1,
              minHeight: 0,
              display: "flex",
              flexDirection: "column",
            }}
          >
            {tab === "Ask" && <AskScreen agent={attached} />}
            {/* "History" holds the MEETINGS + SESSIONS lenses under a sub-toggle
                (the AGENT-SESSION lens also lives embedded in the Agents tab). */}
            {tab === "History" && (
              <HistoryTab
                attachedKind={attached?.kind ?? null}
                onResumeAgentThread={(kind, sid) => {
                  // Resume the thread on the agent the meeting ACTUALLY used
                  // (kind from the meeting link), not whatever is currently
                  // attached — a Claude id must not be resumed onto Cursor.
                  // Legacy links have no kind → fall back to the attached agent.
                  const resumeKind = kind ?? attached?.kind;
                  if (!resumeKind) return; // no kind and nothing attached — no-op
                  void attach(resumeKind, sid).then(() => setTab("Ask"));
                }}
                onResumeSession={(kind, sid) =>
                  void attach(kind, sid).then(() => setTab("Ask"))
                }
                onContinue={(id) => {
                  // Continue a past meeting into the ACTIVE slot. On success the
                  // provider has already (or will shortly) reseed via
                  // onMeetingReseed, so switching to Ask shows the continued
                  // meeting. On blocked, stay on History — the daemon already
                  // pushed the guidance Warning card (no extra UI needed).
                  void client.continueMeeting(id).then((r) => {
                    if (r.blocked) return;
                    setTab("Ask");
                  });
                }}
              />
            )}
            {tab === "Agents" && (
              <AgentsScreen
                agents={agents}
                onAttach={(k) => void attach(k)}
                onDetach={() => void detach()}
                onResumeSession={(kind, sid) =>
                  attach(kind, sid).then(() => setTab("Ask"))
                }
              />
            )}
          </div>

          {/* footer */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              padding: "9px 16px",
              borderTop: "1px solid var(--line)",
            }}
          >
            <span style={ftr}>
              {attached?.displayName ?? "Bluey"} ·{" "}
              <span style={{ color: "var(--tint-ink)" }}>
                runs on your machine
              </span>
            </span>
            <span style={ftr}>
              {connectors.length > 0 ? connectors.join(" · ") : "no connectors"}
            </span>
            <span style={ftr}>⌘↵ ask · ⌥ hide</span>
          </div>

          <ResizeGrip />
        </Glass>
      </div>
    </>
  );
}

const closeBtn = {
  width: 28,
  height: 28,
  borderRadius: 8,
  border: "none",
  background: "transparent",
  color: "var(--ink-3)",
  cursor: "pointer",
  fontSize: 16,
} as const;
const ftr = { fontSize: 10.5, color: "var(--ink-4)" } as const;
