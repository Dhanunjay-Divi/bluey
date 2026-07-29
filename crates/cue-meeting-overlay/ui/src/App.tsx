// The meeting overlay shell — the expanded panel: header (brand · via-agent ·
// tabs · close) over the active tab. First run shows onboarding; after that the
// live Ask loop. One window, state-driven views (no router) — lean by design.

import { useEffect, useRef, useState } from "react";
import { getClient } from "./lib";
import { useDragHeader } from "./lib/useDragHeader";
import { useCollapse, useOnboardingWindowSize } from "./lib/useCollapse";
import { useDataStore } from "./lib/dataStore";
import type { AgentConnectorInfo } from "./lib/types";
import {
  Glass,
  Mark,
  ResizeGrip,
  SegmentedTabs,
  Waveform,
} from "./components/primitives";
import { EyeIcon } from "./components/icons";
import { Pill } from "./components/Pill";
import { FloorplanPill } from "./components/FloorplanPill";
import { ResizeBorder } from "./components/floorplan/ResizeBorder";
import { FloorplanDrawer } from "./components/floorplan/FloorplanDrawer";
import { LiveTranscriptBar } from "./components/LiveTranscriptBar";
import { AskScreen } from "./screens/AskScreen";
import { OpenFloorScreen } from "./screens/OpenFloorScreen";
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

/** Whether the Open Floor Plan redesign is active. Default ON; force the old
 *  glass UI with `?glass` in the URL or `localStorage['bluey.ui'] = 'glass'`.
 *  Sets `data-floorplan` on the root so the warm-paper token layer skins the UI. */
function useFloorplanFlag(): boolean {
  const on = (() => {
    if (typeof window === "undefined") return true;
    if (new URLSearchParams(window.location.search).has("glass")) return false;
    if (localStorage.getItem("bluey.ui") === "glass") return false;
    return true;
  })();
  useEffect(() => {
    const root = document.documentElement;
    if (on) root.setAttribute("data-floorplan", "");
    else root.removeAttribute("data-floorplan");
    return () => root.removeAttribute("data-floorplan");
  }, [on]);
  return on;
}

export function App() {
  const client = getClient();
  // KNOWN LIMITATION: first-run state lives ONLY in the WebView's localStorage
  // (~/Library/WebKit/cue-meeting-overlay), not in daemon settings. So it is
  // out of sync with the rest of the install in both directions: deleting
  // ~/.bluey + Application Support does NOT reset onboarding (the flag
  // survives, and the next launch skips straight to the main app), and clearing
  // the WebView store re-triggers onboarding for an already-set-up user.
  // A factory reset must clear this WebView store explicitly; the real fix is
  // to move this flag into daemon settings so there is one source of truth.
  const [onboarding, setOnboarding] = useState(
    () => !localStorage.getItem("bluey.onboarded"),
  );
  const [tab, setTab] = useState<Tab>("Ask");
  // The Open Floor Plan redesign (default on; ?glass forces the old UI).
  const floorplan = useFloorplanFlag();
  // Agents + attach/detach come from the shared SWR store (cached across tab
  // switches, kept live by the daemon's set_agents push) — no per-mount refetch.
  const { agents, attached, attach, detach } = useDataStore();
  // The attached agent's FULL MCP connector list — feeds the AgentBar chips +
  // the "+N" popover in the Open Floor Plan. Kept whole (name/ready/authTier)
  // so the popover can show every connector, ready dot and all.
  const [connectors, setConnectors] = useState<AgentConnectorInfo[]>([]);
  useEffect(() => {
    let live = true;
    if (!attached) {
      setConnectors([]);
      return;
    }
    client
      .connectors(attached.kind)
      .then((cs) => live && setConnectors(cs))
      .catch(() => live && setConnectors([]));
    return () => {
      live = false;
    };
  }, [attached, client]);
  // The Open Floor Plan drawer: a slide-over sheet with History (past meetings +
  // agent sessions) and Agents (attach/select). null = closed. This is how those
  // surfaces are reached now that the floorplan replaces the glass tab panel.
  const [drawer, setDrawer] = useState<null | "history" | "agents">(null);
  // A meeting the user tried to continue while audio is still recording. The
  // daemon BLOCKS the switch (a live recording is never lost) — we surface a
  // clear notice with a one-tap "Stop audio & continue" so the user isn't left
  // wondering why continue "did nothing". null = no pending blocked continue.
  const [blockedContinue, setBlockedContinue] = useState<
    null | { id: string; agentKind?: string; agentSessionId?: string }
  >(null);
  // Drag the frameless panel by its header (no titlebar to grab).
  const headerRef = useRef<HTMLDivElement>(null);
  useDragHeader(headerRef);

  // Perform a continue, surfacing the blocked case as a visible notice instead
  // of silently returning. `after` runs on success (re-attach etc.).
  const doContinue = (
    meeting: { id: string; agentKind?: string; agentSessionId?: string },
    after: () => void,
  ) => {
    void client.continueMeeting(meeting.id).then((r) => {
      if (r.blocked) {
        setBlockedContinue({
          id: meeting.id,
          agentKind: meeting.agentKind,
          agentSessionId: meeting.agentSessionId,
        });
        return;
      }
      after();
    });
  };

  // Collapse to a compact pill (X) / re-expand (click the pill).
  const { collapsed, collapse, expand, setPillSize } = useCollapse();
  const pillRef = useRef<HTMLDivElement>(null);
  // The pill is conditionally mounted. Re-run the hook after collapse commits so
  // its ref points at a real element; on the initial expanded render it is null.
  const pillWasDragged = useDragHeader(pillRef, collapsed);

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
      {collapsed &&
        (floorplan ? (
          <FloorplanPill
            client={client}
            attached={attached}
            onExpand={() => void expand()}
            setPillSize={(s) => void setPillSize(s)}
            dragRef={pillRef}
            didDragRef={pillWasDragged}
          />
        ) : (
          <Pill
            client={client}
            attached={attached}
            onExpand={() => void expand()}
            dragRef={pillRef}
            didDragRef={pillWasDragged}
          />
        ))}

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

      {/* OPEN FLOOR PLAN — the redesigned full-panel document. Self-contained
          (own top bar + agent bar + canvas + floating stack), so it replaces the
          glass panel entirely rather than living inside the tab body. History &
          Agents are reached via its drawer button (onOpenDrawer). */}
      {floorplan && !collapsed && !onboarding && (
        <div style={{ position: "fixed", inset: 0, display: "flex" }}>
          <Glass
            radius="var(--r-xl)"
            style={{
              flex: 1,
              display: "flex",
              flexDirection: "column",
              minHeight: 0,
            }}
          >
            <OpenFloorScreen
              agent={attached}
              connectors={connectors}
              onTurnOff={() => client.turnOff()}
              onCollapse={() => void collapse()}
              onOpenDrawer={() => setDrawer("history")}
            />
            {/* Resize from ANY edge/corner (the frameless NSPanel has no OS
                resize frame — these do it manually). Replaces the lone grip. */}
            <ResizeBorder />

            {/* Slide-over sheet: History (meetings + agent sessions) & Agents.
                The callbacks mirror the glass tab wiring exactly — attach-based
                resume + continueMeeting — but close the drawer instead of
                switching a tab. */}
            {drawer && (
              <FloorplanDrawer
                initial={drawer}
                onClose={() => setDrawer(null)}
                agents={agents}
                attachedKind={attached?.kind ?? null}
                onAttach={(k) => void attach(k)}
                onDetach={() => void detach()}
                onResumeSession={(kind, sid) =>
                  void attach(kind, sid).then(() => setDrawer(null))
                }
                onResumeAgentThread={(kind, sid) => {
                  const resumeKind = kind ?? attached?.kind;
                  if (!resumeKind) return;
                  void attach(resumeKind, sid).then(() => setDrawer(null));
                }}
                onContinue={(meeting) => {
                  doContinue(meeting, () => {
                    if (meeting.agentSessionId) {
                      const resumeKind = meeting.agentKind ?? attached?.kind;
                      if (resumeKind) {
                        void attach(resumeKind, meeting.agentSessionId);
                      }
                    }
                    setDrawer(null);
                  });
                }}
              />
            )}
          </Glass>
        </div>
      )}

      {/* The GLASS panel FILLS the window (pinned to all edges with a small margin
          for the soft shadow). Shown when the floorplan flag is OFF (?glass). The
          middle tab content flexes + scrolls inside. display:none (not
          visibility:hidden) when collapsed/onboarding so the heavy transcript list
          skips layout/paint, while AskScreen stays mounted. */}
      <div
        style={{
          position: "fixed",
          inset: 0,
          display: collapsed || onboarding || floorplan ? "none" : "flex",
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
                  // not just the transcript. On blocked (a live recording), surface
                  // a clear "stop audio first" notice via doContinue instead of
                  // silently doing nothing.
                  doContinue(meeting, () => {
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

      {/* BLOCKED-CONTINUE notice: the daemon refuses to switch meetings while a
          recording is live (a live meeting is never lost). Tell the user clearly
          and offer a one-tap "Stop audio & continue" so they don't have to hunt
          for the mic/system toggles. */}
      {blockedContinue && (
        <div
          style={{
            position: "fixed",
            left: 0,
            right: 0,
            bottom: 16,
            display: "flex",
            justifyContent: "center",
            zIndex: 9999,
            pointerEvents: "none",
          }}
        >
          <div
            style={{
              pointerEvents: "auto",
              maxWidth: 420,
              background: "var(--paper, #fff)",
              color: "var(--ink, #1c1a19)",
              borderRadius: 12,
              padding: "12px 14px",
              boxShadow: "0 8px 28px rgba(28,26,25,0.22)",
              fontFamily: "var(--font)",
              fontSize: 13,
              lineHeight: 1.4,
            }}
          >
            <div style={{ fontWeight: 600, marginBottom: 4 }}>
              Still recording
            </div>
            <div style={{ opacity: 0.8, marginBottom: 10 }}>
              Turn off the mic and system audio to continue a different meeting —
              your current recording won’t be lost.
            </div>
            <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
              <button
                style={{
                  border: "none",
                  background: "transparent",
                  color: "var(--ink-3, #6b6560)",
                  fontSize: 13,
                  cursor: "pointer",
                }}
                onClick={() => setBlockedContinue(null)}
              >
                Not now
              </button>
              <button
                style={{
                  border: "none",
                  background: "var(--fp-live, #d1483a)",
                  color: "#fff",
                  borderRadius: 8,
                  padding: "6px 12px",
                  fontSize: 13,
                  fontWeight: 600,
                  cursor: "pointer",
                }}
                onClick={() => {
                  const pending = blockedContinue;
                  setBlockedContinue(null);
                  // Stop audio, then retry the continue now that nothing is live.
                  client.stopListening();
                  setTimeout(() => {
                    if (!pending) return;
                    doContinue(pending, () => {
                      if (pending.agentSessionId) {
                        const resumeKind = pending.agentKind ?? attached?.kind;
                        if (resumeKind) {
                          void attach(resumeKind, pending.agentSessionId);
                        }
                      }
                      setDrawer(null);
                      setTab("Ask");
                    });
                  }, 400);
                }}
              >
                Stop audio & continue
              </button>
            </div>
          </div>
        </div>
      )}
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
