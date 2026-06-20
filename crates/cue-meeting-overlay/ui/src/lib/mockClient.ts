// Mock adapter — runs the WHOLE meeting experience with no daemon, so the UI is
// demoable/verifiable instantly and the design can be reviewed end-to-end. It
// reproduces the real backend's behavior we proved live: an attached agent that
// resumes, a slow answer that fires MCP tools mid-stream, and grounded source
// rows. Swap this for the Tauri adapter to go live — same MeetingClient.

import type { AskHandle, MeetingClient } from "./client";
import type {
  AgentConnectorInfo,
  AgentSessionSummary,
  AgentSummary,
  AnswerChunk,
  TranscriptLine,
} from "./types";

const AGENTS: AgentSummary[] = [
  { kind: "claude_code", displayName: "Claude Code", capability: "drive", connectorCount: 4, readyConnectorCount: 3, sessionCount: 473, attached: true },
  { kind: "codex", displayName: "Codex", capability: "drive", connectorCount: 5, readyConnectorCount: 5, sessionCount: 298, attached: false },
  { kind: "cursor", displayName: "Cursor", capability: "drive", connectorCount: 7, readyConnectorCount: 6, sessionCount: 115, attached: false },
  { kind: "copilot", displayName: "GitHub Copilot CLI", capability: "drive", connectorCount: 1, readyConnectorCount: 1, sessionCount: 89, attached: false },
  { kind: "gemini", displayName: "Gemini CLI", capability: "drive", connectorCount: 1, readyConnectorCount: 1, sessionCount: 168, attached: false },
  { kind: "antigravity", displayName: "Antigravity", capability: "drive", connectorCount: 8, readyConnectorCount: 8, sessionCount: 110, attached: false },
  { kind: "antigravity_ide", displayName: "Antigravity IDE", capability: "read_only", connectorCount: 8, readyConnectorCount: 8, sessionCount: 1, attached: false },
];

const SESSIONS: AgentSessionSummary[] = [
  { id: "637ec7a7", title: "Payments API — auth migration review", updatedAt: "1781990000", project: "/Users/you/dev/acme/payments-api" },
  { id: "8732530c", title: "Bluey spine planning", updatedAt: "1781900000", project: "/Users/you/dev/bluey" },
  { id: "0d2907a8", title: "Staging 500s — incident debrief", updatedAt: "1781800000", project: "/Users/you/dev/acme/payments-api" },
  { id: "5b2dafb7", title: "Rate-limiter design", updatedAt: "1781700000", project: "/Users/you/dev/acme/gateway" },
];

const CONNECTORS: AgentConnectorInfo[] = [
  { name: "Jira", authTier: "hosted_oauth", ready: true },
  { name: "GitHub", authTier: "hosted_oauth", ready: true },
  { name: "Supabase", authTier: "env_auth", ready: true },
  { name: "Perplexity", authTier: "env_auth", ready: false },
];

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
let attachedKind = "claude_code";

export function createMockClient(): MeetingClient {
  const transcriptCbs = new Set<(l: TranscriptLine) => void>();

  // Emit a sample live transcript line shortly after mount so the idle→live
  // demo flows on its own (mirrors system-audio → STT).
  setTimeout(() => {
    transcriptCbs.forEach((cb) =>
      cb({
        source: "system",
        speaker: "Priya",
        final: true,
        text: "…before we lock the launch date — why did the auth migration break staging last week, and is it actually fixed?",
      }),
    );
  }, 1200);

  return {
    async listAgents() {
      await delay(120);
      return AGENTS.map((a) => ({ ...a, attached: a.kind === attachedKind }));
    },
    async attach(kind) {
      await delay(180);
      attachedKind = kind;
      return AGENTS.map((a) => ({ ...a, attached: a.kind === kind }));
    },
    async detach() {
      await delay(120);
      attachedKind = "";
      return AGENTS.map((a) => ({ ...a, attached: false }));
    },
    async sessions() {
      await delay(150);
      return SESSIONS;
    },
    async connectors() {
      await delay(120);
      return CONNECTORS;
    },
    async setSessionHistoryConsent() {
      await delay(80);
    },
    onTranscript(cb) {
      transcriptCbs.add(cb);
      return () => transcriptCbs.delete(cb);
    },
    ask(_question, onChunk): AskHandle {
      let cancelled = false;
      const run = async () => {
        // The honest slow pause: the agent is driving + doing MCP round-trips.
        await delay(900);
        if (cancelled) return;
        const steps: AnswerChunk[] = [
          { text: "Yes — it's fixed. " },
          { tool: "jira_search" },
          { source: { kind: "jira", label: "CUE-1423 “Staging auth 500s”", note: "resolved 3d ago" } },
          { text: "PR #482 renamed the auth.sessions column but the migration ran before deploy — a 12-min window of stale reads. " },
          { tool: "github_pulls" },
          { source: { kind: "github", label: "#488 backfill guard", note: "merged → staging green 3 days" } },
          { text: "The backfill guard since makes it backward-compatible, so the launch isn't blocked by this." },
          { done: true },
        ];
        for (const s of steps) {
          if (cancelled) return;
          await delay(s.text ? 260 : 200);
          onChunk(s);
        }
      };
      void run();
      return { cancel: () => (cancelled = true) };
    },
  };
}
