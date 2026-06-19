// AgentSessions — prior sessions for one agent, grouped by project, with resume.
//
// Reads the `:kind` route param, fetches that agent's prior sessions via the
// raw `listAgentSessions(kind)` API, and groups them by their `project` path so
// the user can find the conversation they want and continue it. "Continue"
// attaches the agent pinned to that session (via `useAgentMode().attach`) and
// routes to the answers view so the next answer resumes the thread.
//
// The backend gates session history behind a consent flag and returns an empty
// list when it is off, so the empty state nudges the user toward Settings
// rather than implying they have no history.

import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { ArrowLeft, Loader2 } from "lucide-react";
import { listAgentSessions } from "../lib/agentApi";
import type { AgentSessionSummary } from "../lib/agentTypes";
import { useAgentMode } from "../lib/useAgentMode";

/** A bucket of sessions that share one project (or the null/"No project" group). */
interface ProjectGroup {
  /** Full project path, or null for sessions without one. */
  project: string | null;
  /** Short label shown as the section header (basename, or "No project"). */
  label: string;
  sessions: AgentSessionSummary[];
}

/** Last non-empty path segment of a project path, falling back to the path. */
function projectBasename(project: string | null): string {
  if (!project) return "No project";
  return project.split("/").filter(Boolean).pop() ?? project;
}

/**
 * Render a session's `updated_at` human-friendly. The field is best-effort
 * (RFC3339 from some sources, a bare epoch-seconds string from others), so:
 * an all-digit string is treated as epoch seconds; anything `Date` can parse is
 * formatted as a short locale date+time; otherwise the raw string is shown.
 */
function formatUpdatedAt(s: string): string {
  const raw = s.trim();
  if (!raw) return "";
  const date = /^\d+$/.test(raw) ? new Date(Number(raw) * 1000) : new Date(raw);
  if (Number.isNaN(date.getTime())) return raw;
  return date.toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

/** Group sessions by project; named projects first (A→Z), "No project" last. */
function groupByProject(sessions: AgentSessionSummary[]): ProjectGroup[] {
  const buckets = new Map<string | null, AgentSessionSummary[]>();
  for (const session of sessions) {
    const key = session.project;
    const existing = buckets.get(key);
    if (existing) {
      existing.push(session);
    } else {
      buckets.set(key, [session]);
    }
  }

  const groups: ProjectGroup[] = Array.from(buckets.entries()).map(
    ([project, groupSessions]) => ({
      project,
      label: projectBasename(project),
      sessions: groupSessions,
    }),
  );

  groups.sort((a, b) => {
    if (a.project === null) return 1;
    if (b.project === null) return -1;
    return a.label.localeCompare(b.label);
  });

  return groups;
}

export function AgentSessions() {
  const { kind = "" } = useParams<{ kind: string }>();
  const navigate = useNavigate();
  const { attach } = useAgentMode();

  const [sessions, setSessions] = useState<AgentSessionSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [continuing, setContinuing] = useState<string | null>(null);

  const fetchSessions = useCallback(async () => {
    if (!kind) return;
    setLoading(true);
    setError(null);
    try {
      setSessions(await listAgentSessions(kind));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [kind]);

  useEffect(() => {
    void fetchSessions();
  }, [fetchSessions]);

  const groups = useMemo(() => groupByProject(sessions), [sessions]);

  async function handleContinue(session: AgentSessionSummary) {
    setContinuing(session.id);
    try {
      await attach(kind, session.id);
      navigate("/responses");
    } catch (e) {
      setError(String(e));
      setContinuing(null);
    }
  }

  return (
    <div className="space-y-6">
      <div className="space-y-3">
        <button
          type="button"
          onClick={() => navigate("/agents")}
          className="inline-flex items-center gap-1 text-footnote text-text-tertiary transition-colors duration-200 hover:text-text-secondary"
        >
          <ArrowLeft className="h-3.5 w-3.5" />
          Agents
        </button>
        <h1 className="text-title-2 text-text-primary">Sessions</h1>
      </div>

      {error && (
        <div className="glass rounded-lg p-4 text-footnote text-error">
          Could not load sessions: {error}
        </div>
      )}

      {loading ? (
        <div className="flex items-center gap-2 text-footnote text-text-tertiary">
          <Loader2 className="h-4 w-4 animate-spin" />
          Loading sessions…
        </div>
      ) : groups.length === 0 ? (
        <div className="glass rounded-xl p-8 text-center">
          <p className="text-headline text-text-primary">No sessions found</p>
          <p className="mx-auto mt-2 max-w-md text-footnote text-text-tertiary">
            If you have prior conversations, enable session history in Settings.
          </p>
        </div>
      ) : (
        <div className="space-y-8">
          {groups.map((group) => (
            <section
              key={group.project ?? "__no_project__"}
              className="space-y-3"
            >
              <div className="space-y-0.5">
                <h2 className="text-callout font-medium text-text-secondary">
                  {group.label}
                </h2>
                {group.project && (
                  <p className="truncate text-caption text-text-tertiary">
                    {group.project}
                  </p>
                )}
              </div>
              <ul className="space-y-2">
                {group.sessions.map((session) => (
                  <li
                    key={session.id}
                    className="glass flex items-center justify-between gap-3 rounded-lg p-3 transition-colors duration-200 hover:border-hairline-strong"
                  >
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-callout text-text-primary">
                        {session.title ?? "Untitled session"}
                      </p>
                      <p className="text-caption text-text-tertiary">
                        {formatUpdatedAt(session.updated_at)}
                      </p>
                    </div>
                    <button
                      type="button"
                      onClick={() => void handleContinue(session)}
                      disabled={continuing !== null}
                      className="shrink-0 rounded-md bg-accent px-3 py-1.5 text-subhead font-medium text-white transition-colors duration-200 hover:bg-accent-hover disabled:opacity-50"
                    >
                      {continuing === session.id ? "Continuing…" : "Continue"}
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}
