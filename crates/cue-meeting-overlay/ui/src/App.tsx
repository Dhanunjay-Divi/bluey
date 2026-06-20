// The meeting overlay shell — the expanded panel: header (brand · via-agent ·
// tabs · close) over the active tab. First run shows onboarding; after that the
// live Ask loop. One window, state-driven views (no router) — lean by design.

import { useEffect, useMemo, useState } from "react";
import { getClient } from "./lib";
import type { AgentSummary } from "./lib/types";
import { Glass, Mark, SegmentedTabs, Waveform } from "./components/primitives";
import { AskScreen } from "./screens/AskScreen";
import { HistoryScreen } from "./screens/HistoryScreen";
import { AgentsScreen } from "./screens/AgentsScreen";
import { Onboarding } from "./screens/Onboarding";

type Tab = "Ask" | "History" | "Agents";
const TABS: readonly Tab[] = ["Ask", "History", "Agents"];

export function App() {
  const client = getClient();
  const [onboarding, setOnboarding] = useState(() => !localStorage.getItem("bluey.onboarded"));
  const [tab, setTab] = useState<Tab>("Ask");
  const [agents, setAgents] = useState<AgentSummary[] | null>(null);

  useEffect(() => {
    let live = true;
    client.listAgents().then((a) => live && setAgents(a)).catch(() => live && setAgents([]));
    return () => { live = false; };
  }, [client]);

  const attached = useMemo(() => agents?.find((a) => a.attached) ?? null, [agents]);

  const attach = (kind: string, sessionId?: string) =>
    client.attach(kind, sessionId).then(setAgents);
  const detach = () => client.detach().then(setAgents);

  if (onboarding) {
    return (
      <Onboarding
        agents={agents}
        onAttach={(k) => void attach(k)}
        onDone={() => {
          localStorage.setItem("bluey.onboarded", "1");
          setOnboarding(false);
        }}
      />
    );
  }

  return (
    <div style={{ minHeight: "100%", display: "flex", alignItems: "center", justifyContent: "center", padding: 24 }}>
      <Glass radius="var(--r-xl)" style={{ width: 482 }}>
        {/* header */}
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", padding: "13px 15px" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 9 }}>
            <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
              {attached ? <Waveform /> : <Mark />}
              <span style={{ fontSize: 14, fontWeight: 600, letterSpacing: "-.01em" }}>Bluey</span>
            </span>
            <span style={{ fontSize: 12.5, color: "var(--ink-3)" }}>
              · {attached ? <b style={{ color: "var(--tint-ink)", fontWeight: 540 }}>{attached.displayName}</b> : "managed"}
            </span>
          </div>
          <SegmentedTabs tabs={TABS} value={tab} onChange={setTab} />
          <button aria-label="Collapse" style={closeBtn}>×</button>
        </div>

        {tab === "Ask" && <AskScreen agent={attached} />}
        {tab === "History" && (
          <HistoryScreen
            kind={attached?.kind ?? null}
            onResume={(sid) => attached && attach(attached.kind, sid).then(() => setTab("Ask"))}
          />
        )}
        {tab === "Agents" && <AgentsScreen agents={agents} onAttach={(k) => void attach(k)} onDetach={() => void detach()} />}

        {/* footer */}
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", padding: "9px 16px", borderTop: "1px solid var(--line)" }}>
          <span style={ftr}>{attached?.displayName ?? "Bluey"} · <span style={{ color: "var(--tint-ink)" }}>runs on your machine</span></span>
          <span style={ftr}>Jira · GitHub · Supabase</span>
          <span style={ftr}>⌘↵ ask · ⌥ hide</span>
        </div>
      </Glass>
    </div>
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
