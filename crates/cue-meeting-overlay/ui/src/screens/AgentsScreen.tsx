// Agents tab — discover installed agents, see capability + connector readiness,
// attach/detach. Capability pills mirror the backend's honest states.

import type { AgentSummary } from "../lib/types";

const CAP: Record<string, { label: string; bg: string; fg: string }> = {
  drive: { label: "Ready", bg: "#e7f8f1", fg: "var(--ok)" },
  read_only: { label: "Read-only", bg: "rgba(20,22,28,.05)", fg: "var(--ink-2)" },
  needs_trust: { label: "Needs trust", bg: "#fff3e3", fg: "#c8841f" },
  needs_reauth: { label: "Needs reauth", bg: "#ffeef0", fg: "#d6536a" },
  cloud_blocked: { label: "Cloud only", bg: "rgba(20,22,28,.05)", fg: "var(--ink-3)" },
};

export function AgentsScreen({
  agents,
  onAttach,
  onDetach,
}: {
  agents: AgentSummary[] | null;
  onAttach: (kind: string) => void;
  onDetach: () => void;
}) {
  if (agents === null) {
    return (
      <div style={{ padding: "40px 16px", textAlign: "center", color: "var(--ink-3)", fontSize: 13, display: "flex", flexDirection: "column", alignItems: "center", gap: 12 }}>
        <span style={{ width: 18, height: 18, borderRadius: "50%", background: "conic-gradient(var(--violet),var(--blue),var(--mint),var(--violet))", animation: "aurora-spin 1.4s linear infinite" }} />
        Discovering your agents…
      </div>
    );
  }
  return (
    <div style={{ padding: "8px 12px 12px", maxHeight: 480, overflowY: "auto", display: "flex", flexDirection: "column", gap: 8 }}>
      {agents.map((a) => {
        const cap = CAP[a.capability] ?? CAP.read_only;
        return (
          <div key={a.kind} style={{ display: "flex", alignItems: "center", gap: 11, padding: "11px 13px", borderRadius: "var(--r-lg)", background: a.attached ? "var(--glass-solid)" : "var(--glass-2)", border: a.attached ? "1px solid var(--tint-wash)" : "1px solid var(--line)" }}>
            <span style={{ width: 30, height: 30, borderRadius: 9, background: "linear-gradient(150deg,#eef0ff,#e7f6f1)", color: "var(--tint-ink)", display: "inline-flex", alignItems: "center", justifyContent: "center", fontSize: 14, flex: "none", boxShadow: "inset 0 0 0 1px rgba(255,255,255,.6)" }}>⌘</span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 13, fontWeight: 560, display: "flex", alignItems: "center", gap: 8 }}>
                {a.displayName}
                {a.attached && <span style={{ fontSize: 10, color: "var(--tint-ink)", background: "var(--tint-wash)", padding: "2px 7px", borderRadius: "var(--r-pill)" }}>attached</span>}
              </div>
              <div style={{ fontSize: 11, color: "var(--ink-3)", marginTop: 1 }}>
                {a.readyConnectorCount}/{a.connectorCount} connectors{a.sessionCount != null ? ` · ${a.sessionCount} sessions` : ""}
              </div>
            </div>
            <span style={{ fontSize: 10, fontWeight: 560, padding: "3px 9px", borderRadius: "var(--r-pill)", background: cap.bg, color: cap.fg, flex: "none" }}>{cap.label}</span>
            <button onClick={() => (a.attached ? onDetach() : onAttach(a.kind))} style={a.attached ? detachBtn : attachBtn}>
              {a.attached ? "Stop" : "Use"}
            </button>
          </div>
        );
      })}
    </div>
  );
}

const attachBtn = {
  fontSize: 12,
  fontWeight: 540,
  color: "#fff",
  background: "linear-gradient(140deg,var(--tint),#8f7af5)",
  border: "none",
  borderRadius: 9,
  padding: "6px 13px",
  cursor: "pointer",
  flex: "none",
  boxShadow: "0 4px 12px -3px rgba(111,106,240,.5)",
} as const;
const detachBtn = {
  fontSize: 12,
  fontWeight: 540,
  color: "var(--ink-2)",
  background: "var(--glass-solid)",
  border: "1px solid var(--line-2)",
  borderRadius: 9,
  padding: "6px 13px",
  cursor: "pointer",
  flex: "none",
} as const;
