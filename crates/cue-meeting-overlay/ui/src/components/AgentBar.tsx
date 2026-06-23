// The attached-agent + MCP connector bar — makes the differentiator obvious:
// "this is YOUR agent, resumed, with YOUR connectors." Not a generic chatbot.

import type { AgentSummary } from "../lib/types";

export function AgentBar({
  agent,
  connectorNames,
  extraCount,
}: {
  agent: AgentSummary | null;
  connectorNames: string[];
  extraCount: number;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        margin: "0 12px 4px",
        padding: "11px 13px",
        borderRadius: "var(--r-lg)",
        background: "var(--glass-2)",
        border: "1px solid var(--line)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span
          style={{
            width: 30,
            height: 30,
            borderRadius: 9,
            background: "linear-gradient(150deg,#eef0ff,#e7f6f1)",
            color: "var(--tint-ink)",
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 15,
            boxShadow: "inset 0 0 0 1px rgba(255,255,255,.6)",
          }}
        >
          ⌘
        </span>
        <div>
          <div style={{ fontSize: 13, fontWeight: 560 }}>
            {agent ? agent.displayName : "No agent attached"}
            {agent && <span style={{ color: "var(--ink-3)", fontWeight: 430 }}> · your session</span>}
          </div>
          <div style={{ fontSize: 11, color: "var(--ink-3)", marginTop: 1 }}>
            {agent
              ? `${agent.readyConnectorCount}/${agent.connectorCount} connectors${agent.sessionCount != null ? ` · ${agent.sessionCount} sessions` : ""}`
              : "attach one to answer from your own agent"}
          </div>
        </div>
      </div>
      <div style={{ display: "flex", gap: 6 }}>
        {connectorNames.map((n) => (
          <span
            key={n}
            style={{
              fontSize: 11,
              color: "var(--ink-2)",
              background: "var(--glass-solid)",
              border: "1px solid var(--line)",
              padding: "3px 9px",
              borderRadius: "var(--r-pill)",
              display: "inline-flex",
              alignItems: "center",
              gap: 5,
            }}
          >
            <span style={{ width: 5, height: 5, borderRadius: "50%", background: "var(--mint)" }} />
            {n}
          </span>
        ))}
        {extraCount > 0 && (
          <span
            style={{
              fontSize: 11,
              color: "var(--ink-3)",
              background: "var(--glass-solid)",
              border: "1px solid var(--line)",
              padding: "3px 9px",
              borderRadius: "var(--r-pill)",
            }}
          >
            +{extraCount}
          </span>
        )}
      </div>
    </div>
  );
}
