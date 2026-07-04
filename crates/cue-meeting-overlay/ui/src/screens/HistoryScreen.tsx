// History tab — prior sessions grouped by project, resume one to continue it
// (true in-place resume via the backend). Loading / empty / consent states.

import { useEffect } from "react";
import type { AgentSessionSummary } from "../lib/types";
import { useDataStore } from "../lib/dataStore";

export function HistoryScreen({
  kind,
  onResume,
}: {
  kind: string | null;
  onResume: (sessionId: string) => void;
}) {
  // Sessions come from the shared SWR store, keyed per agent kind — so the SAME
  // fetched list is reused across the Agents tab's embedded PAST SESSIONS list
  // and the History>Sessions lens (no double fetch). ensureSessions lazily loads
  // this kind once (null = first-load spinner); revalidateSessions refreshes an
  // already-loaded kind in the background on every focus, so a newly-created
  // thread surfaces without an app restart. sessionsFor is a pure getter.
  const { sessionsFor, ensureSessions, revalidateSessions } = useDataStore();

  // Mounting this lens (flipping to History>Sessions, or opening the Agents tab)
  // = "revalidate on focus": ensureSessions handles the very first load with a
  // spinner; revalidateSessions background-refreshes when the kind is already
  // cached. Cached rows stay visible meanwhile (stale-while-revalidate).
  useEffect(() => {
    if (!kind) return;
    ensureSessions(kind);
    revalidateSessions(kind);
  }, [kind, ensureSessions, revalidateSessions]);

  const sessions = kind ? sessionsFor(kind) : [];

  if (!kind) return <Empty text="Attach an agent to see its sessions." />;
  if (sessions === null) return <Loading text="Loading sessions…" />;
  if (sessions.length === 0)
    return (
      <Empty text="No sessions yet. Enable history in settings to surface them." />
    );

  const groups = groupByProject(sessions);

  return (
    <div
      style={{
        padding: "6px 12px 12px",
        // Fill the parent + scroll internally (was maxHeight:480 → dead space
        // below on taller windows).
        flex: 1,
        minHeight: 0,
        overflowY: "auto",
      }}
    >
      {Object.entries(groups).map(([project, rows]) => (
        <div key={project} style={{ marginBottom: 12 }}>
          <div
            style={{
              fontSize: 10,
              fontWeight: 680,
              letterSpacing: ".08em",
              color: "var(--ink-3)",
              padding: "8px 4px 6px",
            }}
          >
            {project === "—" ? "NO PROJECT" : leaf(project).toUpperCase()}
          </div>
          {rows.map((s) => (
            <button
              key={s.id}
              onClick={() => onResume(s.id)}
              style={sessionRow}
            >
              <div style={{ flex: 1, minWidth: 0 }}>
                <div
                  style={{
                    fontSize: 13,
                    fontWeight: 500,
                    color: "var(--ink)",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {s.title ?? `Session ${s.id.slice(0, 8)}`}
                </div>
                <div
                  style={{ fontSize: 11, color: "var(--ink-3)", marginTop: 1 }}
                >
                  {relativeTime(s.updatedAt)}
                </div>
              </div>
              <span style={continueBtn}>Continue →</span>
            </button>
          ))}
        </div>
      ))}
    </div>
  );
}

function groupByProject(
  sessions: AgentSessionSummary[],
): Record<string, AgentSessionSummary[]> {
  const out: Record<string, AgentSessionSummary[]> = {};
  for (const s of sessions) {
    const k = s.project ?? "—";
    (out[k] ??= []).push(s);
  }
  return out;
}
const leaf = (p: string) =>
  p.replace(/\/+$/, "").split("/").filter(Boolean).pop() ?? p;
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
    <div
      style={{
        padding: "40px 16px",
        textAlign: "center",
        color: "var(--ink-3)",
        fontSize: 13,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: 12,
      }}
    >
      <span
        style={{
          width: 18,
          height: 18,
          borderRadius: "50%",
          background:
            "conic-gradient(var(--violet),var(--blue),var(--mint),var(--violet))",
          animation: "aurora-spin 1.4s linear infinite",
        }}
      />
      {text}
    </div>
  );
}
function Empty({ text }: { text: string }) {
  return (
    <div
      style={{
        padding: "40px 16px",
        textAlign: "center",
        color: "var(--ink-3)",
        fontSize: 13,
      }}
    >
      {text}
    </div>
  );
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
const continueBtn = {
  fontSize: 11.5,
  color: "var(--tint-ink)",
  fontWeight: 540,
  flex: "none",
} as const;
