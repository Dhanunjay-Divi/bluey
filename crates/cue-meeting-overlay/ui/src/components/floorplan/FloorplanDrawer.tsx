// The Open Floor Plan drawer — a slide-over sheet for History & Agents.
//
// The floorplan replaces the glass tab panel, so History (past meetings + agent
// sessions) and Agents (attach/select) are reached here instead: the top-bar
// layers button opens this sheet OVER the document. It reuses the existing,
// self-contained HistoryTab + AgentsScreen leaf screens verbatim — same props,
// same data (useDataStore) — so nothing is re-implemented; only the chrome (the
// slide-over frame + a History/Agents switch) is new. A scrim behind it closes
// on click; Escape closes too. Warm-paper themed like the rest of the floorplan.

import { useEffect, useState } from "react";
import type { AgentSummary, MeetingSummary } from "../../lib/types";
import { HistoryTab } from "../../screens/HistoryTab";
import { AgentsScreen } from "../../screens/AgentsScreen";
import { CloseIcon } from "../icons";

type Panel = "history" | "agents";

export function FloorplanDrawer({
  initial,
  onClose,
  agents,
  attachedKind,
  onAttach,
  onDetach,
  onResumeSession,
  onResumeAgentThread,
  onContinue,
}: {
  initial: Panel;
  onClose: () => void;
  agents: AgentSummary[] | null;
  attachedKind: string | null;
  onAttach: (kind: string) => void;
  onDetach: () => void;
  onResumeSession: (kind: string, sessionId: string) => void;
  onResumeAgentThread: (kind: string | undefined, sessionId: string) => void;
  onContinue: (meeting: MeetingSummary) => void;
}) {
  const [panel, setPanel] = useState<Panel>(initial);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="fp-drawer-root">
      <div className="fp-drawer-scrim" onClick={onClose} aria-hidden />
      <div
        className="fp-drawer"
        role="dialog"
        aria-modal="true"
        aria-label="History and Agents"
      >
        <div className="fp-drawer-head">
          <div className="fp-drawer-switch">
            <button
              className={`fp-drawer-tab${panel === "history" ? " is-on" : ""}`}
              onClick={() => setPanel("history")}
            >
              History
            </button>
            <button
              className={`fp-drawer-tab${panel === "agents" ? " is-on" : ""}`}
              onClick={() => setPanel("agents")}
            >
              Agents
            </button>
          </div>
          <button
            className="fp-drawer-close"
            aria-label="Close"
            onClick={onClose}
          >
            <CloseIcon size={16} />
          </button>
        </div>

        <div className="fp-drawer-body">
          {panel === "history" ? (
            <HistoryTab
              attachedKind={attachedKind}
              onResumeAgentThread={onResumeAgentThread}
              onResumeSession={onResumeSession}
              onContinue={onContinue}
            />
          ) : (
            <AgentsScreen
              agents={agents}
              onAttach={onAttach}
              onDetach={onDetach}
              onResumeSession={onResumeSession}
            />
          )}
        </div>
      </div>
    </div>
  );
}
