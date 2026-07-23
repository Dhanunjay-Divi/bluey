// The meeting overlay shell — the expanded panel: header (brand · via-agent ·
// tabs · close) over the active tab. First run shows onboarding; after that the
// live Ask loop. One window, state-driven views (no router) — lean by design.

import { useRef, useState } from "react";
import { getClient } from "./lib";
import { useDragHeader } from "./lib/useDragHeader";
import { useCollapse, useOnboardingWindowSize } from "./lib/useCollapse";
import { useDataStore } from "./lib/dataStore";
import {
  Glass,
  Mark,
  ResizeGrip,
  SegmentedTabs,
  Waveform,
} from "./components/primitives";
import { EyeIcon } from "./components/icons";
import { Pill } from "./components/Pill";
import { LiveTranscriptBar } from "./components/LiveTranscriptBar";
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
  // KNOWN LIMITATION: first-run state lives ONLY in the WebView's localStorage
  // (~/Library/WebKit/cue-meeting-overlay), not in daemon settings. So it is
  // out of sync with the rest of the install in both directions: deleting
  // ~/.bluey + Application Support does NOT reset onboarding (the flag
  // survives, and the next launch skips straight to the main app), and clearing
  // the WebView store re-triggers onboarding for an already-set-up user.
  // `install.sh --reset` clears the WebView store too; the real fix is to move
  // this flag into daemon settings so there is one source of truth.
  const [onboarding, setOnboarding] = useState(
    () => !localStorage.getItem("bluey.onboarded"),
  );
  const [tab, setTab] = useState<Tab>("Ask");
  // Agents + attach/detach come from the shared SWR store (cached across tab
  // switches, kept live by the daemon's set_agents push) — no per-mount refetch.
  const { agents, attached, attach, detach } = useDataStore();
  // Drag the frameless panel by its header (no titlebar to grab).
  const headerRef = useRef<HTMLDivElement>(null);
  useDragHeader(headerRef);

  // Collapse to a compact pill (X) / re-expand (click the pill).
  const { collapsed, collapse, expand } = useCollapse();
  const pillRef = useRef<HTMLDivElement>(null);
  useDragHeader(pillRef);

  // First-run onboarding is a compact centered card — shrink the OS window to it
  // while onboarding, then restore the full panel when it's done (fixes the card
  // rendering full-size in a large empty window).
  useOnboardingWindowSize(onboarding, collapsed);

  // (The connector list that fed the old status footer was dropped with it —
  // the bottom row is the live transcript now. Connectors remain visible in the
  // Agents tab, which is where they're actionable.)

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
            {/* Two distinct exits, side by side: the EYE hides the panel (Bluey
                keeps running — collapse to the pill), the × turns Bluey OFF. */}
            <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
              <button
                aria-label="Hide panel"
                title="Hide — Bluey keeps running"
                style={closeBtn}
                onClick={() => void collapse()}
              >
                <EyeIcon size={15} />
              </button>
              <button
                aria-label="Turn Bluey off"
                title="Turn Bluey off"
                style={closeBtn}
                onClick={() => client.turnOff()}
              >
                ×
              </button>
            </div>
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
                onContinue={(meeting) => {
                  // Continue a past meeting into the ACTIVE slot AND re-attach the
                  // agent thread it used — so continuing restores the FULL context
                  // (transcript + Q&A via the reseed, AND the agent conversation),
                  // not just the transcript. On success the provider reseeds via
                  // onMeetingReseed, so switching to Ask shows the continued
                  // meeting. On blocked (a live recording), stay on History — the
                  // daemon already pushed the guidance Warning card.
                  void client.continueMeeting(meeting.id).then((r) => {
                    if (r.blocked) return;
                    // Re-attach the meeting's linked agent thread on the agent it
                    // ACTUALLY used (kind from the link; fall back to the attached
                    // agent for legacy links with no kind). No link → just show the
                    // meeting content.
                    if (meeting.agentSessionId) {
                      const resumeKind = meeting.agentKind ?? attached?.kind;
                      if (resumeKind) {
                        void attach(resumeKind, meeting.agentSessionId);
                      }
                    }
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

          {/* Bottom row: LIVE TRANSCRIPTION (replaces the old static status
              footer — that line never changed, so it wasted a permanent row on
              something read once; the transcript is what's actually live). Shows
              only the newest spoken line, expandable to a scrollable history. */}
          <LiveTranscriptBar />

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
