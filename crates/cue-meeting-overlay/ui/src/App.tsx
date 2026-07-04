// The meeting overlay shell — the expanded panel: header (brand · via-agent ·
// tabs · close) over the active tab. First run shows onboarding; after that the
// live Ask loop. One window, state-driven views (no router) — lean by design.

import { useEffect, useMemo, useRef, useState } from "react";
import { getClient } from "./lib";
import { useDragHeader } from "./lib/useDragHeader";
import { useCollapse } from "./lib/useCollapse";
import type { AgentSummary } from "./lib/types";
import {
  Glass,
  Mark,
  ResizeGrip,
  SegmentedTabs,
  Waveform,
} from "./components/primitives";
import { Pill } from "./components/Pill";
import { AskScreen } from "./screens/AskScreen";
import { MeetingsScreen } from "./screens/MeetingsScreen";
import { AgentsScreen } from "./screens/AgentsScreen";
import { Onboarding } from "./screens/Onboarding";

// Audio controls live in the composer "+" menu now, not a separate tab.
// "Meetings" is the MEETINGS lens (my past meetings + their transcript/Q&A);
// the AGENT-SESSION lens (resume a Claude/Cursor thread) lives under "Agents".
// Two clearly-named, distinct history surfaces.
type Tab = "Ask" | "Meetings" | "Agents";
const TABS: readonly Tab[] = ["Ask", "Meetings", "Agents"];

export function App() {
  const client = getClient();
  const [onboarding, setOnboarding] = useState(
    () => !localStorage.getItem("bluey.onboarded"),
  );
  const [tab, setTab] = useState<Tab>("Ask");
  const [agents, setAgents] = useState<AgentSummary[] | null>(null);
  const [connectors, setConnectors] = useState<string[]>([]);
  // Drag the frameless panel by its header (no titlebar to grab).
  const headerRef = useRef<HTMLDivElement>(null);
  useDragHeader(headerRef);

  // Collapse to a compact pill (X) / re-expand (click the pill).
  const { collapsed, collapse, expand } = useCollapse();
  const pillRef = useRef<HTMLDivElement>(null);
  useDragHeader(pillRef);

  useEffect(() => {
    let live = true;
    client
      .listAgents()
      .then((a) => live && setAgents(a))
      .catch(() => live && setAgents([]));
    return () => {
      live = false;
    };
  }, [client]);

  const attached = useMemo(
    () => agents?.find((a) => a.attached) ?? null,
    [agents],
  );

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

  const attach = (kind: string, sessionId?: string, model?: string) =>
    client.attach(kind, sessionId, model).then(setAgents);
  const detach = () => client.detach().then(setAgents);

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
              flex: 1,
              minHeight: 0,
              overflowY: "auto",
              display: "flex",
              flexDirection: "column",
            }}
          >
            {tab === "Ask" && <AskScreen agent={attached} />}
            {/* "Meetings" is the MEETINGS lens (my past meetings). The
                AGENT-SESSION lens lives under the Agents tab. */}
            {tab === "Meetings" && (
              <MeetingsScreen
                onResumeAgentThread={(kind, sid) => {
                  // Resume the thread on the agent the meeting ACTUALLY used
                  // (kind from the meeting link), not whatever is currently
                  // attached — a Claude id must not be resumed onto Cursor.
                  // Legacy links have no kind → fall back to the attached agent.
                  const resumeKind = kind ?? attached?.kind;
                  if (!resumeKind) return; // no kind and nothing attached — no-op
                  void attach(resumeKind, sid).then(() => setTab("Ask"));
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
