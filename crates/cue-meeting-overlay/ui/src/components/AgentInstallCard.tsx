// One-click install offer for a missing agent CLI. The daemon pushes this
// (push_agent_install) when the attached agent's CLI isn't on PATH but has a
// vetted install recipe. Install runs the exact command shown; Bluey NEVER signs
// the user in — after install they sign in manually and ask again.

export type AgentInstallOffer = {
  kind: string;
  displayName: string;
  command: string;
  prerequisite?: string;
};

export function AgentInstallCard({
  offer,
  onInstall,
  onCancel,
}: {
  offer: AgentInstallOffer;
  onInstall: () => void;
  onCancel: () => void;
}) {
  return (
    <div style={wrap}>
      <div style={title}>Install {offer.displayName}</div>
      <div style={body}>
        Bluey answers through your own {offer.displayName} CLI, but it isn't
        installed yet. Install it, then sign in and ask again.
      </div>
      <code style={cmd}>{offer.command}</code>
      {offer.prerequisite && (
        <div style={note}>Requires {offer.prerequisite} on your machine.</div>
      )}
      <div style={{ display: "flex", gap: 8, marginTop: 12 }}>
        <button style={primary} onClick={onInstall}>
          Install
        </button>
        <button style={ghost} onClick={onCancel}>
          Not now
        </button>
      </div>
    </div>
  );
}

const wrap = {
  margin: "8px 12px 4px",
  padding: "13px 15px",
  borderRadius: "var(--r)",
  border: "1px solid var(--line)",
  background: "var(--glass-2)",
} as const;
const title = {
  fontSize: 13.5,
  fontWeight: 600,
  color: "var(--ink)",
  marginBottom: 5,
} as const;
const body = {
  fontSize: 12.5,
  lineHeight: 1.5,
  color: "var(--ink-2)",
} as const;
const cmd = {
  display: "block",
  marginTop: 9,
  padding: "7px 9px",
  borderRadius: 8,
  background: "var(--glass)",
  border: "1px solid var(--line-2)",
  fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
  fontSize: 11.5,
  color: "var(--ink)",
  overflowX: "auto",
  whiteSpace: "nowrap",
} as const;
const note = {
  marginTop: 7,
  fontSize: 11,
  color: "var(--ink-3)",
} as const;
const primary = {
  fontSize: 12.5,
  fontWeight: 540,
  color: "#fff",
  background: "linear-gradient(140deg,var(--tint),#8f7af5)",
  border: "none",
  borderRadius: 9,
  padding: "8px 16px",
  cursor: "pointer",
} as const;
const ghost = {
  fontSize: 12.5,
  fontWeight: 500,
  color: "var(--ink-2)",
  background: "transparent",
  border: "1px solid var(--line-2)",
  borderRadius: 9,
  padding: "8px 14px",
  cursor: "pointer",
} as const;
