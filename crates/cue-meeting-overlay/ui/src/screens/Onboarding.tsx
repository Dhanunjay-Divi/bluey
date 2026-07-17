// First-run onboarding — the riskiest flow (permissions + attach + consent).
// Designed to reach the "aha" (first grounded answer) fast: one step at a time,
// privacy stated upfront, each step a single clear action. Progressive, not a
// scary wall of permissions.

import { useState } from "react";
import { Glass, Mark } from "../components/primitives";
import { getClient } from "../lib";
import type { AgentSummary } from "../lib/types";

type Step = "welcome" | "mic" | "attach" | "consent" | "ready";
const ORDER: Step[] = ["welcome", "mic", "attach", "consent", "ready"];

export function Onboarding({
  agents,
  onAttach,
  onDone,
}: {
  agents: AgentSummary[] | null;
  onAttach: (kind: string) => void;
  onDone: () => void;
}) {
  const [step, setStep] = useState<Step>("welcome");
  const idx = ORDER.indexOf(step);
  const next = () => setStep(ORDER[Math.min(idx + 1, ORDER.length - 1)]);
  const client = getClient();

  return (
    // Fill the window and center the card — the panel IS the window (no empty
    // box around it), matching the main shell + the interview overlay.
    <div style={{ position: "fixed", inset: 7, display: "flex" }}>
      <Glass radius="var(--r-xl)" style={{ flex: 1, display: "flex", flexDirection: "column", justifyContent: "center" }}>
        <div style={{ padding: "26px 26px 22px" }}>
          {/* progress dots */}
          <div style={{ display: "flex", gap: 6, marginBottom: 22 }}>
            {ORDER.map((s, i) => (
              <span key={s} style={{ height: 3, flex: 1, borderRadius: 2, background: i <= idx ? "var(--tint)" : "var(--line-2)", transition: ".25s" }} />
            ))}
          </div>

          {step === "welcome" && (
            <Body
              icon={<Mark size={40} />}
              title="Bluey for meetings"
              text="Answers come from your own coding agent and its connectors — grounded in your real repo, tickets and tools. Everything runs on this machine. Nothing leaves."
              cta="Get started"
              onCta={next}
            />
          )}

          {step === "mic" && (
            <Body
              icon={<Glyph>🎙</Glyph>}
              title="Let Bluey hear the call"
              text="Bluey listens to your system audio locally to catch questions as they come up. Audio is transcribed on your machine and never uploaded."
              cta="Allow microphone"
              secondary="Skip for now"
              onCta={() => {
                // Actually start capture — this triggers the OS permission
                // prompt and begins listening, mirroring the composer mic
                // button (v1 captures SYSTEM audio: the other people on the
                // call). Without this the step was cosmetic (next() only).
                client.startListening({ microphone: false, system: true });
                next();
              }}
              onSecondary={next}
            />
          )}

          {step === "attach" && (
            <div>
              <Glyph>⌘</Glyph>
              <h2 style={h2}>Attach your agent</h2>
              <p style={p}>Pick the coding agent you already use. Bluey drives your own session, so it knows your projects.</p>
              <div style={{ display: "flex", flexDirection: "column", gap: 7, marginTop: 16 }}>
                {(agents ?? []).slice(0, 4).map((a) => (
                  <button key={a.kind} onClick={() => { onAttach(a.kind); next(); }} style={pickRow}>
                    <span style={{ fontSize: 13, fontWeight: 540 }}>{a.displayName}</span>
                    <span style={{ marginLeft: "auto", fontSize: 11, color: "var(--ink-3)" }}>{a.sessionCount ?? 0} sessions</span>
                    <span style={{ color: "var(--tint-ink)", fontSize: 13 }}>→</span>
                  </button>
                ))}
                {agents === null && <p style={p}>Discovering your agents…</p>}
              </div>
            </div>
          )}

          {step === "consent" && (
            <Body
              icon={<Glyph>📂</Glyph>}
              title="Use your past sessions"
              text="Allow Bluey to read your agent's prior session history so answers carry your project context. Read-only, on this machine, and you can turn it off anytime."
              cta="Allow & continue"
              secondary="Not now"
              onCta={() => {
                // Persist the consent (read prior agent sessions) before
                // advancing — the step was cosmetic (next() only) so a new
                // user finished setup with nothing granted.
                void client.setSessionHistoryConsent(true);
                next();
              }}
              onSecondary={next}
            />
          )}

          {step === "ready" && (
            <Body
              icon={<Glyph>✦</Glyph>}
              title="You're ready"
              text="Bluey is listening. When a question comes up in your meeting, you'll get a grounded answer from your own agent — private, until you choose to share it."
              cta="Enter meeting mode"
              onCta={onDone}
            />
          )}
        </div>
      </Glass>
    </div>
  );
}

function Body({
  icon,
  title,
  text,
  cta,
  secondary,
  onCta,
  onSecondary,
}: {
  icon: React.ReactNode;
  title: string;
  text: string;
  cta: string;
  secondary?: string;
  onCta: () => void;
  onSecondary?: () => void;
}) {
  return (
    <div>
      {icon}
      <h2 style={h2}>{title}</h2>
      <p style={p}>{text}</p>
      <div style={{ display: "flex", gap: 9, marginTop: 20 }}>
        <button onClick={onCta} style={primary}>{cta}</button>
        {secondary && <button onClick={onSecondary} style={ghost}>{secondary}</button>}
      </div>
    </div>
  );
}

function Glyph({ children }: { children: React.ReactNode }) {
  return (
    <span style={{ width: 40, height: 40, borderRadius: 12, display: "inline-flex", alignItems: "center", justifyContent: "center", fontSize: 19, background: "linear-gradient(150deg,#eef0ff,#e7f6f1)", color: "var(--tint-ink)", boxShadow: "inset 0 0 0 1px rgba(255,255,255,.6)" }}>
      {children}
    </span>
  );
}

const h2 = { fontSize: 19, fontWeight: 600, letterSpacing: "-.02em", color: "var(--ink)", margin: "16px 0 8px" } as const;
const p = { fontSize: 13.5, lineHeight: 1.6, color: "var(--ink-2)" } as const;
const primary = {
  fontSize: 13.5,
  fontWeight: 540,
  color: "#fff",
  background: "linear-gradient(140deg,var(--tint),#8f7af5)",
  border: "none",
  borderRadius: 11,
  padding: "10px 18px",
  cursor: "pointer",
  boxShadow: "0 5px 14px -3px rgba(111,106,240,.5)",
} as const;
const ghost = {
  fontSize: 13.5,
  fontWeight: 500,
  color: "var(--ink-2)",
  background: "transparent",
  border: "1px solid var(--line-2)",
  borderRadius: 11,
  padding: "10px 16px",
  cursor: "pointer",
} as const;
const pickRow = {
  display: "flex",
  alignItems: "center",
  gap: 10,
  width: "100%",
  textAlign: "left",
  border: "1px solid var(--line)",
  background: "var(--glass-2)",
  borderRadius: "var(--r)",
  padding: "11px 13px",
  cursor: "pointer",
} as const;
