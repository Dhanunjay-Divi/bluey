// First-run onboarding — the riskiest flow (permissions + attach + consent).
// Designed to reach the "aha" (first grounded answer) fast: one step at a time,
// privacy stated upfront, each step a single clear action. Progressive, not a
// scary wall of permissions.

import { useState } from "react";
import { Glass, Mark } from "../components/primitives";
import { AgentLogo } from "../components/AgentLogo";
import { getClient } from "../lib";
import type { AgentSummary } from "../lib/types";

type Step = "welcome" | "mic" | "attach" | "calendar" | "consent" | "ready";
const ORDER: Step[] = [
  "welcome",
  "mic",
  "attach",
  "calendar",
  "consent",
  "ready",
];

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
                    <span style={{ display: "inline-flex", alignItems: "center" }}>
                      <AgentLogo kind={a.kind} size={16} />
                    </span>
                    <span style={{ fontSize: 13, fontWeight: 540 }}>{a.displayName}</span>
                    <span style={{ marginLeft: "auto", fontSize: 11, color: "var(--ink-3)" }}>{a.sessionCount ?? 0} sessions</span>
                    <span style={{ color: "var(--tint-ink)", fontSize: 13 }}>→</span>
                  </button>
                ))}
                {agents === null && <p style={p}>Discovering your agents…</p>}
              </div>
            </div>
          )}

          {step === "calendar" && (
            <CalendarStep onNext={next} onSkip={next} />
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

// One cloud-calendar provider the onboarding step can connect.
type CalProvider = { id: string; label: string };
const CAL_PROVIDERS: CalProvider[] = [
  { id: "google", label: "Google Calendar" },
  { id: "microsoft", label: "Microsoft Calendar" },
];

// Per-provider connect state. `email` is set once connected (from calendarStatus);
// `error` holds the daemon's failure message so the UI can render, not crash.
type CalState = {
  status: "idle" | "connecting" | "connected" | "error";
  email?: string;
  error?: string;
};

// The calendar onboarding step: "Connect Google / Microsoft Calendar" buttons.
// Connecting is optional (Skip advances) — a cloud calendar lets Bluey warm up
// ahead of meetings, but a user with the macOS calendar connected doesn't need it.
function CalendarStep({
  onNext,
  onSkip,
}: {
  onNext: () => void;
  onSkip: () => void;
}) {
  const client = getClient();
  const [state, setState] = useState<Record<string, CalState>>({});
  const anyConnected = CAL_PROVIDERS.some(
    (p) => state[p.id]?.status === "connected",
  );

  const connect = async (id: string) => {
    setState((s) => ({ ...s, [id]: { status: "connecting" } }));
    try {
      await client.calendarConnect(id);
      // Pull the connected email for the label; tolerate a status read failing.
      let email: string | undefined;
      try {
        const rows = await client.calendarStatus();
        email = rows.find((r) => r.provider === id)?.email || undefined;
      } catch {
        email = undefined;
      }
      setState((s) => ({ ...s, [id]: { status: "connected", email } }));
    } catch (e) {
      // Render the daemon's error (e.g. "cloud calendar not built", a timeout,
      // or an OAuth failure) instead of throwing out of the click handler.
      const error = e instanceof Error ? e.message : String(e);
      setState((s) => ({ ...s, [id]: { status: "error", error } }));
    }
  };

  return (
    <div>
      <Glyph>📅</Glyph>
      <h2 style={h2}>Connect your calendar</h2>
      <p style={p}>
        Bluey warms up an answer ahead of each meeting from your calendar. Connect
        a cloud calendar, or skip if your calendar is already on this Mac.
      </p>
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 7,
          marginTop: 16,
        }}
      >
        {CAL_PROVIDERS.map((prov) => {
          const st = state[prov.id]?.status ?? "idle";
          const connecting = st === "connecting";
          const connected = st === "connected";
          return (
            <div key={prov.id}>
              <button
                onClick={() => connect(prov.id)}
                disabled={connecting || connected}
                style={{
                  ...pickRow,
                  cursor: connecting || connected ? "default" : "pointer",
                  opacity: connecting ? 0.7 : 1,
                }}
              >
                <span style={{ fontSize: 13, fontWeight: 540 }}>
                  {prov.label}
                </span>
                <span
                  style={{
                    marginLeft: "auto",
                    fontSize: 11,
                    color: connected ? "var(--mint-ink,#2e7d63)" : "var(--ink-3)",
                  }}
                >
                  {connecting
                    ? "Connecting…"
                    : connected
                      ? (state[prov.id]?.email ?? "Connected")
                      : ""}
                </span>
                {!connected && (
                  <span style={{ color: "var(--tint-ink)", fontSize: 13 }}>→</span>
                )}
              </button>
              {st === "error" && (
                <p
                  style={{
                    fontSize: 11.5,
                    lineHeight: 1.5,
                    color: "var(--danger-ink,#c0392b)",
                    margin: "5px 2px 0",
                  }}
                >
                  {state[prov.id]?.error ?? "Connection failed."}
                </p>
              )}
            </div>
          );
        })}
      </div>
      <div style={{ display: "flex", gap: 9, marginTop: 20 }}>
        <button onClick={onNext} style={primary}>
          {anyConnected ? "Continue" : "Next"}
        </button>
        {!anyConnected && (
          <button onClick={onSkip} style={ghost}>
            Skip for now
          </button>
        )}
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
