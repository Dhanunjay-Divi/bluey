// The shared SWR (stale-while-revalidate) data store for discovery data —
// agents, past meetings, and per-agent-kind sessions. It sits ABOVE <App/> (see
// main.tsx, beside MeetingProvider), so switching tabs, collapsing, or first-run
// onboarding never unmounts it and never throws away already-loaded data.
//
// The contract, applied uniformly to every dataset:
//   • READS are pure — a consumer that reads `agents` / `meetings` /
//     sessionsFor(kind) gets the CURRENT cached value instantly (null only until
//     the very first load resolves; that null is the ONLY spinner state).
//   • REVALIDATION is background — revalidateX() fires a fetch, and when it
//     resolves the store updates and every consumer re-renders with the fresh
//     value. On failure the STALE value is kept (stale-over-blank), never nulled.
//   • An in-flight guard per dataset dedupes concurrent revalidations so a rapid
//     sequence of tab focuses can't stack duplicate requests.
//
// The store is ALSO live: it owns the single onAgents subscription, so a daemon
// PUSH of set_agents (the SWR background full-refresh filling session counts, or
// an attach/detach flag-flip, or a consent refresh) updates the agent list with
// no user action. listAgents() (fired by revalidateAgents) also returns via a
// set_agents line, so onAgents fires too — harmless, setAgents is
// idempotent-by-value; the store simply receives the same list twice.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { getClient } from ".";
import type {
  AgentSessionSummary,
  AgentSummary,
  MeetingSummary,
} from "./types";

interface DataStoreValue {
  /** The discovered agents, or null until the first load resolves (spinner). */
  agents: AgentSummary[] | null;
  /** The currently-attached agent, derived from `agents`. */
  attached: AgentSummary | null;
  /** Past meetings for the MEETINGS lens, or null until first load (spinner). */
  meetings: MeetingSummary[] | null;
  /** Pure getter: the cached sessions for `kind`, or null when not-yet-loaded
   *  (spinner) — for a null kind it returns []. Never triggers a fetch itself;
   *  call {@link ensureSessions} to lazily load a kind. */
  sessionsFor: (kind: string | null) => AgentSessionSummary[] | null;
  /** Lazily fetch a kind's sessions once (no-op if already loaded or in flight).
   *  Consumers call this in an effect keyed on the kind, then read sessionsFor. */
  ensureSessions: (kind: string) => void;
  /** Background-revalidate a kind's sessions (e.g. when the Sessions lens gains
   *  focus). Unlike {@link ensureSessions} this re-fetches even an already-loaded
   *  kind, so a newly-created thread surfaces without an app restart. Keeps the
   *  stale list on failure. */
  revalidateSessions: (kind: string) => void;
  /** Attach an agent (optionally resuming a session / pinning a model). Resolves
   *  once the store has the fresh agent list applied; callers can `.then(...)`. */
  attach: (kind: string, sessionId?: string, model?: string) => Promise<void>;
  /** Detach the current agent. Resolves once the fresh list is applied. */
  detach: () => Promise<void>;
  /** Background-revalidate the agent list (e.g. when a lens gains focus). */
  revalidateAgents: () => void;
  /** Background-revalidate past meetings (e.g. when the Meetings lens shows). */
  revalidateMeetings: () => void;
}

const DataStoreContext = createContext<DataStoreValue | null>(null);

export function DataProvider({ children }: { children: ReactNode }) {
  const client = getClient();

  const [agents, setAgents] = useState<AgentSummary[] | null>(null);
  const [meetings, setMeetings] = useState<MeetingSummary[] | null>(null);
  // key = agent kind; value = that kind's sessions. A MISSING key means
  // never-fetched-for-that-kind; a present key (even []) means loaded.
  const [sessions, setSessions] = useState<Map<string, AgentSessionSummary[]>>(
    () => new Map(),
  );

  // In-flight guards — dedupe concurrent revalidations so background refreshes
  // can't stack duplicate requests for the same dataset.
  const agentsInflight = useRef(false);
  const meetingsInflight = useRef(false);
  const sessionsInflight = useRef<Set<string>>(new Set());
  // Guards the first-mount seed against React 19 StrictMode's double-invoke.
  const didSeed = useRef(false);

  const attached = useMemo(
    () => agents?.find((a) => a.attached) ?? null,
    [agents],
  );

  // ---- SWR revalidators (guard → fetch → on-resolve set+clear / on-reject keep) ----
  const revalidateAgents = useCallback(() => {
    if (agentsInflight.current) return;
    agentsInflight.current = true;
    client
      .listAgents()
      .then((a) => {
        // onAgents (the live sub below) also fires for this reply; setting here
        // too avoids a first-paint lag and is idempotent-by-value.
        setAgents(a);
        agentsInflight.current = false;
      })
      .catch(() => {
        // Keep the stale value (stale-over-blank); just release the guard.
        agentsInflight.current = false;
      });
  }, [client]);

  const revalidateMeetings = useCallback(() => {
    if (meetingsInflight.current) return;
    meetingsInflight.current = true;
    client
      .meetings()
      .then((m) => {
        setMeetings(m);
        meetingsInflight.current = false;
      })
      .catch(() => {
        meetingsInflight.current = false;
      });
  }, [client]);

  const revalidateSessions = useCallback(
    (kind: string) => {
      if (sessionsInflight.current.has(kind)) return;
      sessionsInflight.current.add(kind);
      client
        .sessions(kind)
        .then((rows) => {
          setSessions((prev) => new Map(prev).set(kind, rows));
          sessionsInflight.current.delete(kind);
        })
        .catch(() => {
          // Keep any stale value for this kind; just release the guard.
          sessionsInflight.current.delete(kind);
        });
    },
    [client],
  );

  // ---- first-mount seed (null → value): fetch agents + meetings once ----
  useEffect(() => {
    if (didSeed.current) return;
    didSeed.current = true;
    revalidateAgents();
    revalidateMeetings();
  }, [revalidateAgents, revalidateMeetings]);

  // ---- live subscription (single owner): daemon-pushed set_agents ----
  useEffect(() => client.onAgents((next) => setAgents(next)), [client]);

  // ---- public getters + imperative helpers ----
  const sessionsFor = useCallback(
    (kind: string | null): AgentSessionSummary[] | null => {
      if (!kind) return [];
      return sessions.get(kind) ?? null;
    },
    [sessions],
  );

  const ensureSessions = useCallback(
    (kind: string) => {
      if (sessions.has(kind) || sessionsInflight.current.has(kind)) return;
      revalidateSessions(kind);
    },
    [sessions, revalidateSessions],
  );

  const attach = useCallback(
    (kind: string, sessionId?: string, model?: string): Promise<void> =>
      // The daemon confirms by re-pushing the full agent list (caught by the
      // onAgents sub); we also apply the resolved value here to avoid a
      // first-paint lag. Idempotent with the push.
      client.attach(kind, sessionId, model).then((a) => {
        setAgents(a);
      }),
    [client],
  );

  const detach = useCallback(
    (): Promise<void> =>
      client.detach().then((a) => {
        setAgents(a);
      }),
    [client],
  );

  const value: DataStoreValue = {
    agents,
    attached,
    meetings,
    sessionsFor,
    ensureSessions,
    revalidateSessions,
    attach,
    detach,
    revalidateAgents,
    revalidateMeetings,
  };

  return (
    <DataStoreContext.Provider value={value}>
      {children}
    </DataStoreContext.Provider>
  );
}

/** Read the shared SWR data store. Must be called under <DataProvider>. */
export function useDataStore(): DataStoreValue {
  const ctx = useContext(DataStoreContext);
  if (!ctx) {
    throw new Error("useDataStore must be used within a DataProvider");
  }
  return ctx;
}
