// History tab — prior sessions grouped by project, resume one to continue it
// (true in-place resume via the backend). Loading / empty / consent states.

import { useEffect, useState } from "react";
import { getClient } from "../lib";
import type { AgentSessionSummary } from "../lib/types";

export function HistoryScreen({
  kind,
  onResume,
}: {
  kind: string | null;
  onResume: (sessionId: string) => void;
}) {
  const [sessions, setSessions] = useState<AgentSessionSummary[] | null>(null);

  useEffect(() => {
    let live = true;
    if (!kind) {
      setSessions([]);
      return;
    }
    setSessions(null);
    getClient()
      .sessions(kind)
      .then((s) => live && setSessions(s))
      .catch(() => live && setSessions([]));
    return () => {
      live = false;
    };
  }, [kind]);

  if (!kind) return <Empty text="Attach an agent to see its sessions." />;
  if (sessions === null) return <Loading text="Loading sessions…" />;
  if (sessions.length === 0) return <Empty text="No sessions yet. Enable history in settings to surface them." />;

  const groups = groupByProject(sessions);

  return (
    <div style={{ padding: "6px 12px 12px", maxHeight: 480, overflowY: "auto" }}>
      {Object.entries(groups).map(([project, rows]) => (
        <div key={project} style={{ marginBottom: 12 }}>
          <div style={{ fontSize: 10, fontWeight: 680, letterSpacing: ".08em", color: "var(--ink-3)", padding: "8px 4px 6px" }}>
            {project === "—" ? "NO PROJECT" : leaf(project).toUpperCase()}
          </div>
          {rows.map((s) => (
            <button key={s.id} onClick={() => onResume(s.id)} style={sessionRow}>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 13, fontWeight: 500, color: "var(--ink)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {s.title ?? `Session ${s.id.slice(0, 8)}`}
                </div>
                <div style={{ fontSize: 11, color: "var(--ink-3)", marginTop: 1 }}>{relativeTime(s.updatedAt)}</div>
              </div>
              <span style={continueBtn}>Continue →</span>
            </button>
          ))}
        </div>
      ))}
    </div>
  );
}

function groupByProject(sessions: AgentSessionSummary[]): Record<string, AgentSessionSummary[]> {
  const out: Record<string, AgentSessionSummary[]> = {};
  for (const s of sessions) {
    const k = s.project ?? "—";
    (out[k] ??= []).push(s);
  }
  return out;
}
const leaf = (p: string) => p.replace(/\/+$/, "").split("/").filter(Boolean).pop() ?? p;
function relativeTime(v: string): string {
  const n = Number(v);
  if (!Number.isFinite(n) || n <= 0) return "recently";
  const secs = Math.max(0, Math.floor(Date.now() / 1000) - n);
  if (secs < 3600) return `${Math.max(1, Math.floor(secs / 60))}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}

function Loading({ text }: { text: string }) {
  return (
    <div style={{ padding: "40px 16px", textAlign: "center", color: "var(--ink-3)", fontSize: 13, display: "flex", flexDirection: "column", alignItems: "center", gap: 12 }}>
      <span style={{ width: 18, height: 18, borderRadius: "50%", background: "conic-gradient(var(--violet),var(--blue),var(--mint),var(--violet))", animation: "aurora-spin 1.4s linear infinite" }} />
      {text}
    </div>
  );
}
function Empty({ text }: { text: string }) {
  return <div style={{ padding: "40px 16px", textAlign: "center", color: "var(--ink-3)", fontSize: 13 }}>{text}</div>;
}

const sessionRow = {
  display: "flex",
  alignItems: "center",
  gap: 10,
  width: "100%",
  textAlign: "left",
  border: "1px solid var(--line)",
  background: "var(--glass-2)",
  borderRadius: "var(--r)",
  padding: "10px 12px",
  marginBottom: 6,
  cursor: "pointer",
} as const;
const continueBtn = { fontSize: 11.5, color: "var(--tint-ink)", fontWeight: 540, flex: "none" } as const;
