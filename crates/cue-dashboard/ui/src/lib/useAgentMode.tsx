// Two-product mode toggle: managed Bluey AI vs. the user's own coding agent.
//
// `"managed"` is the default — Bluey's hosted AI answers, billing/balance is
// shown, and no agent is attached. `"agent"` means a discovered coding agent
// (an `AgentSummary`) is attached and drives answers instead.
//
// This module owns the shared mode + agent-list state via React context. On
// mount the provider calls `listAgents()` once and derives the initial mode
// from whether any agent reports `attached === true`. All async calls funnel
// their failures into `error` state so nothing throws uncaught.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import {
  attachAgent,
  detachAgent,
  listAgents,
  type AgentSummary,
} from "./agentApi";

export type AgentMode = "managed" | "agent";

export interface AgentModeContextValue {
  /** Current product mode. */
  mode: AgentMode;
  /** Every discovered agent, as last fetched. */
  agents: AgentSummary[];
  /** The currently attached agent, or null in managed mode. */
  attachedAgent: AgentSummary | null;
  /** True while an agent request is in flight. */
  loading: boolean;
  /** Last error message from an agent request, or null. */
  error: string | null;
  /** Re-fetch the agent list and recompute attached agent + mode. */
  refresh: () => Promise<void>;
  /** Attach an agent (optionally resuming a session) and switch to agent mode. */
  attach: (kind: string, sessionId?: string) => Promise<void>;
  /** Detach the active agent and switch back to managed mode. */
  detach: () => Promise<void>;
  /** Switch mode: "managed" detaches; "agent" is a no-op until one is attached. */
  setMode: (next: AgentMode) => Promise<void>;
}

const AgentModeContext = createContext<AgentModeContextValue | null>(null);

function findAttached(agents: AgentSummary[]): AgentSummary | null {
  return agents.find((agent) => agent.attached) ?? null;
}

function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return "Agent request failed.";
}

export function AgentModeProvider({ children }: { children: React.ReactNode }) {
  const [agents, setAgents] = useState<AgentSummary[]>([]);
  const [mode, setModeState] = useState<AgentMode>("managed");
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);

  const applyAgents = useCallback((next: AgentSummary[]) => {
    setAgents(next);
    setModeState(findAttached(next) ? "agent" : "managed");
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      applyAgents(await listAgents());
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setLoading(false);
    }
  }, [applyAgents]);

  const attach = useCallback(
    async (kind: string, sessionId?: string) => {
      setLoading(true);
      setError(null);
      try {
        const next = await attachAgent(kind, sessionId);
        setAgents(next);
        // Trust the explicit user action: agent mode is intended even if the
        // refreshed list hasn't flipped `attached` yet.
        setModeState("agent");
      } catch (err) {
        setError(errorMessage(err));
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  const detach = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await detachAgent();
      setAgents(next);
      setModeState("managed");
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setLoading(false);
    }
  }, []);

  const attachedAgent = useMemo(() => findAttached(agents), [agents]);

  const setMode = useCallback(
    async (next: AgentMode) => {
      if (next === "managed") {
        await detach();
        return;
      }
      // Switching to "agent" only sticks once something is attached; the Agents
      // page owns the actual attach flow, so this is a no-op otherwise.
      if (attachedAgent) {
        setModeState("agent");
      }
    },
    [attachedAgent, detach],
  );

  // Discover agents once on mount; failures land in `error`, not uncaught.
  useEffect(() => {
    void refresh();
  }, [refresh]);

  const value = useMemo<AgentModeContextValue>(
    () => ({
      mode,
      agents,
      attachedAgent,
      loading,
      error,
      refresh,
      attach,
      detach,
      setMode,
    }),
    [mode, agents, attachedAgent, loading, error, refresh, attach, detach, setMode],
  );

  return (
    <AgentModeContext.Provider value={value}>
      {children}
    </AgentModeContext.Provider>
  );
}

export function useAgentMode(): AgentModeContextValue {
  const ctx = useContext(AgentModeContext);
  if (!ctx) {
    throw new Error("useAgentMode must be used within an <AgentModeProvider>.");
  }
  return ctx;
}
