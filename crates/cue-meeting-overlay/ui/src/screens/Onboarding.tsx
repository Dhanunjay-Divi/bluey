// First-run onboarding — the riskiest flow (permissions + attach + consent).
// Designed to reach the "aha" (first grounded answer) fast: one step at a time,
// privacy stated upfront, each step a single clear action. Progressive, not a
// scary wall of permissions.

import { useEffect, useRef, useState } from "react";
import { Glass, Mark } from "../components/primitives";
import { AgentLogo } from "../components/AgentLogo";
import {
  AgentInstallCard,
  type AgentInstallOffer,
} from "../components/AgentInstallCard";
import { getClient } from "../lib";
import type {
  AgentSummary,
  CalendarConnection,
  SetupStatus,
} from "../lib/types";

type Step =
  | "welcome"
  | "mic"
  | "attach"
  | "setup"
  | "calendar"
  | "consent"
  | "ready";
const ORDER: Step[] = [
  "welcome",
  "mic",
  "attach",
  "setup",
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
  // The daemon answers requestAgentInstall with a push_agent_install offer;
  // render the SAME consent card AskScreen uses so nothing installs unasked.
  const [installOffer, setInstallOffer] = useState<AgentInstallOffer | null>(
    null,
  );
  const [setup, setSetup] = useState<SetupStatus | null>(null);
  const idx = ORDER.indexOf(step);
  const next = () => setStep(ORDER[Math.min(idx + 1, ORDER.length - 1)]);
  const client = getClient();

  // Subscribe to daemon install offers (the reply to requestAgentInstall).
  useEffect(() => client.onAgentInstall((o) => setInstallOffer(o)), [client]);
  // Live setup status: the daemon pushes on request and on every change (model
  // download progress, an install/login finishing), so this screen reflects the
  // REAL state instead of assuming setup worked.
  useEffect(() => client.onSetupStatus((s) => setSetup(s)), [client]);
  useEffect(() => {
    if (step === "setup") client.requestSetupStatus();
  }, [step, client]);

  return (
    // Fill the window and center the card — the panel IS the window (no empty
    // box around it), matching the main shell + the interview overlay.
    <div style={{ position: "fixed", inset: 7, display: "flex" }}>
      <Glass
        radius="var(--r-xl)"
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          justifyContent: "center",
        }}
      >
        <div style={{ padding: "26px 26px 22px" }}>
          {/* progress dots */}
          <div style={{ display: "flex", gap: 6, marginBottom: 22 }}>
            {ORDER.map((s, i) => (
              <span
                key={s}
                style={{
                  height: 3,
                  flex: 1,
                  borderRadius: 2,
                  background:
                    i <= idx
                      ? "var(--brand, #c6613f)"
                      : "var(--line-2, #ded9cf)",
                  transition: ".25s",
                }}
              />
            ))}
          </div>

          {step === "welcome" && (
            <Body
              icon={<Mark size={40} />}
              title="Bluey for meetings"
              text="Answers come from your own coding agent and its connectors — grounded in your real repo, tickets and tools. Meeting data stays local by default; only providers and connectors you explicitly enable receive the context they need."
              cta="Get started"
              onCta={next}
            />
          )}

          {step === "mic" && (
            <Body
              icon={<Glyph>🎙</Glyph>}
              title="Allow audio access"
              text="Bluey uses System Audio Recording to hear the call and Microphone to hear you. macOS may ask once for each. On-device transcription is the default; audio is sent off-device only if you explicitly configure a cloud transcription provider."
              cta="Allow both"
              secondary="Skip for now"
              onCta={() => {
                // Request both explicitly, with copy that explains macOS owns
                // two independent grants. A system-audio prompt followed by a
                // microphone prompt is expected only on first use.
                client.startListening({ microphone: true, system: true });
                next();
              }}
              onSecondary={next}
            />
          )}

          {step === "attach" && (
            <div>
              <Glyph>⌘</Glyph>
              <h2 style={h2}>Attach your agent</h2>
              <p style={p}>
                Pick the coding agent you already use. Bluey drives your own
                session, so it knows your projects.
              </p>
              <div
                style={{
                  display: "flex",
                  flexDirection: "column",
                  gap: 7,
                  marginTop: 16,
                }}
              >
                {(agents ?? []).slice(0, 4).map((a) => {
                  // `needs_reauth` = the CLI is installed but signed out. Attaching
                  // it "works" and then every ask fails, so say so HERE and let the
                  // user fix it in-product (the daemon launches the agent's own
                  // login flow) instead of sending them to a terminal.
                  const needsLogin = a.capability === "needs_reauth";
                  return (
                    <button
                      key={a.kind}
                      onClick={() => {
                        // Signed out: launch the agent's own login flow instead
                        // of attaching something that fails on every ask. Stay
                        // on this step so the user can pick again once signed in.
                        if (needsLogin) {
                          client.requestAgentLogin(a.kind);
                          return;
                        }
                        onAttach(a.kind);
                        next();
                      }}
                      style={pickRow}
                    >
                      <span
                        style={{ display: "inline-flex", alignItems: "center" }}
                      >
                        <AgentLogo kind={a.kind} size={16} />
                      </span>
                      <span style={{ fontSize: 13, fontWeight: 540 }}>
                        {a.displayName}
                      </span>
                      <span
                        style={{
                          marginLeft: "auto",
                          fontSize: 11,
                          color: needsLogin
                            ? "var(--warn-ink, #d08700)"
                            : "var(--ink-3)",
                        }}
                      >
                        {needsLogin
                          ? "Sign in"
                          : `${a.sessionCount ?? 0} sessions`}
                      </span>
                      <span style={{ color: "var(--tint-ink)", fontSize: 13 }}>
                        →
                      </span>
                    </button>
                  );
                })}
                {agents === null && <p style={p}>Discovering your agents…</p>}
                {/* The empty case used to render NOTHING — a blank list with no
                    explanation and no way forward, which is what a brand-new Mac
                    sees. Bluey cannot answer without an agent, so this is the
                    single most important state in onboarding. */}
                {agents !== null && agents.length === 0 && (
                  <div>
                    <p style={p}>
                      No coding agent found on this Mac. Bluey answers{" "}
                      <em>through</em> your own agent, so you'll need one
                      installed.
                    </p>
                    <button
                      style={pickRow}
                      onClick={() => client.requestAgentInstall()}
                    >
                      <span style={{ fontSize: 13, fontWeight: 540 }}>
                        Install one for me
                      </span>
                      <span
                        style={{
                          marginLeft: "auto",
                          color: "var(--tint-ink)",
                          fontSize: 13,
                        }}
                      >
                        →
                      </span>
                    </button>
                    <button style={{ ...pickRow, marginTop: 7 }} onClick={next}>
                      <span style={{ fontSize: 13, color: "var(--ink-3)" }}>
                        I'll do it later
                      </span>
                      <span
                        style={{
                          marginLeft: "auto",
                          color: "var(--ink-3)",
                          fontSize: 13,
                        }}
                      >
                        →
                      </span>
                    </button>
                  </div>
                )}
              </div>
            </div>
          )}

          {installOffer && (
            <AgentInstallCard
              offer={installOffer}
              onInstall={() => {
                client.respondAgentInstall(installOffer.kind, true);
                setInstallOffer(null);
              }}
              onCancel={() => {
                client.respondAgentInstall(installOffer.kind, false);
                setInstallOffer(null);
              }}
            />
          )}

          {step === "setup" && (
            <div>
              <Glyph>⚙</Glyph>
              <h2 style={h2}>Getting everything ready</h2>
              <p style={p}>
                Bluey transcribes on this Mac and answers through your agent.
                Both need to be ready before a meeting.
              </p>
              <div
                style={{
                  display: "flex",
                  flexDirection: "column",
                  gap: 7,
                  marginTop: 16,
                }}
              >
                {setup === null && <p style={p}>Checking…</p>}
                {setup && (
                  <>
                    <SetupRow item={setup.model} />
                    <SetupRow
                      item={setup.agent}
                      onFix={
                        setup.agent.state === "missing"
                          ? () =>
                              client.requestAgentInstall(
                                setup.agent.kind ?? undefined,
                              )
                          : setup.agent.state === "needs_login" &&
                              setup.agent.kind
                            ? () =>
                                client.requestAgentLogin(
                                  setup.agent.kind as string,
                                )
                            : undefined
                      }
                      fixLabel={
                        setup.agent.state === "missing"
                          ? "Install"
                          : setup.agent.state === "needs_login"
                            ? "Sign in"
                            : undefined
                      }
                    />
                  </>
                )}
              </div>
              <button
                style={{
                  ...pickRow,
                  marginTop: 14,
                  opacity: setup?.allReady ? 1 : 0.55,
                }}
                onClick={next}
              >
                <span style={{ fontSize: 13, fontWeight: 540 }}>
                  {setup?.allReady ? "Continue" : "Continue anyway"}
                </span>
                <span
                  style={{
                    marginLeft: "auto",
                    color: "var(--tint-ink)",
                    fontSize: 13,
                  }}
                >
                  →
                </span>
              </button>
            </div>
          )}

          {step === "calendar" && (
            <CalendarAccounts onNext={next} onSkip={next} />
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
        <button onClick={onCta} style={primary}>
          {cta}
        </button>
        {secondary && (
          <button onClick={onSecondary} style={ghost}>
            {secondary}
          </button>
        )}
      </div>
    </div>
  );
}

// One cloud-calendar provider the onboarding step can connect.
type CalProvider = { id: string; label: string; clientIdEnv: string };
const CAL_PROVIDERS: CalProvider[] = [
  {
    id: "google",
    label: "Google Calendar",
    clientIdEnv: "BLUEY_GOOGLE_CLIENT_ID",
  },
  {
    id: "microsoft",
    label: "Microsoft Calendar",
    clientIdEnv: "BLUEY_MICROSOFT_CLIENT_ID",
  },
];

// Per-provider connect state. `email` is set once connected (from calendarStatus);
// `error` holds the daemon's failure message so the UI can render, not crash.
type CalState = {
  status: "idle" | "connecting" | "connected" | "disconnecting" | "error";
  configured?: boolean;
  email?: string;
  error?: string;
};

function calendarError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  return message.trim() || "Calendar connection failed. Please try again.";
}

function missingCalendarConfigGuidance(provider: CalProvider): string {
  return [
    `${provider.label} is unavailable in this install.`,
    "Install a calendar-enabled Bluey release.",
    `Developers and administrators can set ${provider.clientIdEnv} in the Bluey daemon environment and restart Bluey.`,
    "Use a registered public/desktop client ID—never a client secret.",
  ].join(" ");
}

// The calendar onboarding step: "Connect Google / Microsoft Calendar" buttons.
// Connecting is optional (Skip advances) — a cloud calendar lets Bluey warm up
// ahead of meetings, but a user with the macOS calendar connected doesn't need it.
export function CalendarAccounts({
  onNext,
  onSkip,
  management = false,
}: {
  onNext?: () => void;
  onSkip?: () => void;
  management?: boolean;
}) {
  const client = getClient();
  const [state, setState] = useState<Record<string, CalState>>({});
  const [loading, setLoading] = useState(true);
  const [statusError, setStatusError] = useState<string>();
  // React StrictMode replays mount effects in development. Reuse the same IPC
  // promise across that replay so one screen visit performs one Keychain read.
  const initialStatusRequest = useRef<Promise<CalendarConnection[]> | null>(
    null,
  );
  const anyConnected = CAL_PROVIDERS.some(
    (p) => state[p.id]?.status === "connected",
  );
  const busy = CAL_PROVIDERS.some((provider) => {
    const status = state[provider.id]?.status;
    return status === "connecting" || status === "disconnecting";
  });

  const applyStatus = (rows: CalendarConnection[]) => {
    setState((current) => {
      const next = { ...current };
      for (const provider of CAL_PROVIDERS) {
        const row = rows.find(
          (candidate) => candidate.provider === provider.id,
        );
        next[provider.id] = row?.connected
          ? {
              status: "connected",
              configured: row.configured,
              email: row.email || undefined,
            }
          : {
              status: row?.error ? "error" : "idle",
              configured: row?.configured,
              error: row?.error,
            };
      }
      return next;
    });
  };

  useEffect(() => {
    let active = true;
    setLoading(true);
    setStatusError(undefined);
    const request = initialStatusRequest.current ?? client.calendarStatus();
    initialStatusRequest.current = request;
    request
      .then((rows) => {
        if (active) applyStatus(rows);
      })
      .catch((error: unknown) => {
        if (active) setStatusError(calendarError(error));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [client]);

  const connect = async (id: string) => {
    if (busy) return;
    setStatusError(undefined);
    setState((s) => ({ ...s, [id]: { status: "connecting" } }));
    try {
      await client.calendarConnect(id);
      const rows = await client.calendarStatus();
      const connected = rows.find((row) => row.provider === id);
      if (!connected?.connected) {
        throw new Error(
          "Authorization finished, but Bluey could not verify the saved account. Try connecting again.",
        );
      }
      applyStatus(rows);
    } catch (error: unknown) {
      setState((s) => ({
        ...s,
        [id]: { status: "error", error: calendarError(error) },
      }));
    }
  };

  const disconnect = async (id: string) => {
    if (busy) return;
    setStatusError(undefined);
    setState((current) => ({
      ...current,
      [id]: {
        ...current[id],
        status: "disconnecting",
      },
    }));
    try {
      await client.calendarDisconnect(id);
      const rows = await client.calendarStatus();
      applyStatus(rows);
    } catch (error: unknown) {
      setState((current) => ({
        ...current,
        [id]: {
          ...current[id],
          status: "connected",
          error: calendarError(error),
        },
      }));
    }
  };

  return (
    <div>
      {!management && <Glyph>📅</Glyph>}
      <h2 style={management ? managementHeading : h2}>
        {management ? "Calendar accounts" : "Connect your calendar"}
      </h2>
      <p style={p}>
        {management
          ? "Reconnect or disconnect the Google and Microsoft accounts Bluey uses for meeting prep."
          : "Bluey can prepare your meeting context before a call starts. Your browser will open for Google or Microsoft consent, then return you here."}
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
          const disconnecting = st === "disconnecting";
          const connected = st === "connected";
          const configured = state[prov.id]?.configured !== false;
          const visibleError = configured
            ? state[prov.id]?.error
            : missingCalendarConfigGuidance(prov);
          return (
            <div key={prov.id}>
              <button
                onClick={() =>
                  connected ? disconnect(prov.id) : connect(prov.id)
                }
                disabled={loading || busy || !configured}
                style={{
                  ...pickRow,
                  cursor:
                    loading || busy || !configured ? "default" : "pointer",
                  opacity: connecting || disconnecting || !configured ? 0.7 : 1,
                }}
              >
                <span style={{ fontSize: 13, fontWeight: 540 }}>
                  {prov.label}
                </span>
                <span
                  style={{
                    marginLeft: "auto",
                    fontSize: 11,
                    color: connected
                      ? "var(--mint-ink,#2e7d63)"
                      : "var(--ink-3)",
                  }}
                >
                  {connecting
                    ? "Connecting…"
                    : disconnecting
                      ? "Disconnecting…"
                      : connected
                        ? `${state[prov.id]?.email ?? "Connected"} · Disconnect`
                        : !configured
                          ? "Not configured"
                          : loading
                            ? "Checking…"
                            : "Connect"}
                </span>
                {!connected && (
                  <span style={{ color: "var(--tint-ink)", fontSize: 13 }}>
                    →
                  </span>
                )}
              </button>
              {visibleError && (
                <p
                  style={{
                    fontSize: 11.5,
                    lineHeight: 1.5,
                    color: "var(--danger-ink,#c0392b)",
                    margin: "5px 2px 0",
                  }}
                >
                  {visibleError}
                </p>
              )}
            </div>
          );
        })}
      </div>
      {statusError && (
        <p
          style={{
            fontSize: 11.5,
            lineHeight: 1.5,
            color: "var(--danger-ink,#c0392b)",
            margin: "7px 2px 0",
          }}
        >
          Could not read calendar status: {statusError}
        </p>
      )}
      {!management && (
        <div style={{ display: "flex", gap: 9, marginTop: 20 }}>
          <button onClick={onNext} disabled={busy} style={primary}>
            {anyConnected ? "Continue" : "Next"}
          </button>
          {!anyConnected && !busy && (
            <button onClick={onSkip} disabled={loading} style={ghost}>
              Skip for now
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function Glyph({ children }: { children: React.ReactNode }) {
  return (
    <span
      style={{
        width: 42,
        height: 42,
        borderRadius: 12,
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        fontSize: 20,
        background: "rgba(198, 97, 63, 0.08)",
        color: "var(--brand, #c6613f)",
        boxShadow: "inset 0 0 0 1px rgba(198, 97, 63, 0.18)",
      }}
    >
      {children}
    </span>
  );
}

const h2 = {
  fontSize: 19,
  fontWeight: 600,
  letterSpacing: "-.02em",
  color: "var(--ink, #1c1a19)",
  margin: "16px 0 8px",
} as const;

const managementHeading = {
  ...h2,
  fontSize: 15,
  marginTop: 0,
} as const;

const p = {
  fontSize: 13.5,
  lineHeight: 1.6,
  color: "var(--ink-2, #3c3a38)",
} as const;

const primary = {
  fontSize: 13.5,
  fontWeight: 580,
  color: "var(--paper-2, #ffffff)",
  background: "var(--brand, #c6613f)",
  border: "none",
  borderRadius: 11,
  padding: "10px 18px",
  cursor: "pointer",
  boxShadow: "0 4px 14px -3px rgba(198, 97, 63, 0.4)",
  transition: "all 0.2s ease",
} as const;

const ghost = {
  fontSize: 13.5,
  fontWeight: 520,
  color: "var(--ink-2, #3c3a38)",
  background: "transparent",
  border: "1px solid var(--line-2, #ded9cf)",
  borderRadius: 11,
  padding: "10px 16px",
  cursor: "pointer",
  transition: "all 0.2s ease",
} as const;

const pickRow = {
  display: "flex",
  alignItems: "center",
  gap: 10,
  width: "100%",
  textAlign: "left",
  border: "1px solid var(--line, #ebe7df)",
  background: "var(--paper-2, #fdfcfb)",
  borderRadius: "var(--r, 12px)",
  padding: "11px 13px",
  cursor: "pointer",
  transition: "all 0.2s ease",
} as const;

/** One setup prerequisite row: status dot, detail, live progress, and (when the
 *  item is actionable) an in-product fix button — so the user never has to open
 *  a terminal to finish setup. */
function SetupRow({
  item,
  onFix,
  fixLabel,
}: {
  item: { state: string; detail: string; percent: number | null };
  onFix?: () => void;
  fixLabel?: string;
}) {
  const color =
    item.state === "ready"
      ? "var(--ok, #4e8d5b)"
      : item.state === "working"
        ? "var(--brand, #c6613f)"
        : "var(--ink-3, #8c867d)";
  return (
    <div
      style={{
        ...pickRow,
        cursor: "default",
        alignItems: "flex-start",
        flexDirection: "column",
        gap: 6,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          width: "100%",
          gap: 8,
        }}
      >
        <span
          style={{
            width: 7,
            height: 7,
            borderRadius: 4,
            background: color,
            flexShrink: 0,
          }}
        />
        <span style={{ fontSize: 12.5, color: "var(--ink, #1c1a19)" }}>
          {item.detail}
        </span>
        {onFix && fixLabel && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              onFix();
            }}
            style={{
              marginLeft: "auto",
              fontSize: 11.5,
              fontWeight: 560,
              color: "var(--brand, #c6613f)",
              background: "none",
              border: "none",
              cursor: "pointer",
            }}
          >
            {fixLabel}
          </button>
        )}
      </div>
      {item.percent !== null && (
        <div
          style={{
            width: "100%",
            height: 3,
            borderRadius: 2,
            background: "var(--line-2, #ded9cf)",
          }}
        >
          <div
            style={{
              width: `${item.percent}%`,
              height: "100%",
              borderRadius: 2,
              background: "var(--brand, #c6613f)",
              transition: ".3s",
            }}
          />
        </div>
      )}
    </div>
  );
}
