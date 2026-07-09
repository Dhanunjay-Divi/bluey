// Context-coverage meter — the onboarding "what can your agent reach" surface
// (the Context Intelligence Lab USP, first cut). Renders one chip per
// meeting-relevant source (calendar / slack / email / tickets / bluey memory):
// green dot = the attached agent already reaches it; hollow dot = missing,
// with the guided connect hint (the user runs it in THEIR agent — Bluey never
// holds credentials). Pure presentational; data comes from
// client.sourceCoverage().

import { useEffect, useState } from "react";
import { getClient } from "../lib";
import type { SourceCoverageInfo } from "../lib/types";

export function CoverageMeter({
  attachedKind,
}: {
  attachedKind: string | null;
}) {
  const client = getClient();
  const [sources, setSources] = useState<SourceCoverageInfo[] | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    if (!attachedKind) {
      setSources(null);
      return;
    }
    client
      .sourceCoverage()
      .then((s) => live && setSources(s))
      .catch(() => live && setSources([]));
    return () => {
      live = false;
    };
  }, [client, attachedKind]);

  if (!attachedKind || sources === null) return null;

  const connected = sources.filter((s) => s.connected).length;

  return (
    <div
      style={{
        border: "1px solid rgba(20,22,28,.08)",
        borderRadius: 12,
        padding: "10px 12px",
        display: "flex",
        flexDirection: "column",
        gap: 8,
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "baseline",
        }}
      >
        <span style={{ fontSize: 12, fontWeight: 600, color: "var(--ink-1)" }}>
          Context coverage
        </span>
        <span style={{ fontSize: 11, color: "var(--ink-3)" }}>
          {connected}/{sources.length} sources
        </span>
      </div>
      <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
        {sources.map((s) => (
          <button
            key={s.source}
            onClick={() => setExpanded(expanded === s.source ? null : s.source)}
            title={
              s.connected
                ? `Connected via ${s.via ?? "agent connector"}`
                : "Missing — tap for how to connect"
            }
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 5,
              fontSize: 11,
              padding: "3px 9px",
              borderRadius: 999,
              border: "1px solid rgba(20,22,28,.1)",
              background: s.connected ? "#e7f8f1" : "transparent",
              color: s.connected ? "var(--ok)" : "var(--ink-3)",
              cursor: "pointer",
            }}
          >
            <span
              style={{
                width: 6,
                height: 6,
                borderRadius: "50%",
                background: s.connected ? "var(--ok)" : "transparent",
                border: s.connected ? "none" : "1.5px solid var(--ink-4)",
              }}
            />
            {s.label}
          </button>
        ))}
      </div>
      {expanded &&
        (() => {
          const s = sources.find((x) => x.source === expanded);
          if (!s) return null;
          return (
            <div
              style={{
                fontSize: 11,
                color: "var(--ink-2)",
                background: "rgba(20,22,28,.04)",
                borderRadius: 8,
                padding: "8px 10px",
                lineHeight: 1.5,
              }}
            >
              {s.connected ? (
                <>
                  <b>{s.label}</b> is reachable via{" "}
                  <code>{s.via ?? "your agent"}</code>.
                </>
              ) : s.connectHint ? (
                <>
                  <b>{s.label}</b> isn’t connected. Add it in your agent (you
                  authorize there — Bluey never holds credentials):
                  <pre
                    style={{
                      margin: "6px 0 0",
                      padding: "6px 8px",
                      background: "rgba(20,22,28,.06)",
                      borderRadius: 6,
                      overflowX: "auto",
                      fontSize: 10.5,
                    }}
                  >
                    {s.connectHint}
                  </pre>
                </>
              ) : (
                <>
                  <b>{s.label}</b> isn’t connected. Add an MCP connector for it
                  in your agent to bring it into meetings.
                </>
              )}
            </div>
          );
        })()}
    </div>
  );
}
