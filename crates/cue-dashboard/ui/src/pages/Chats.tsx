import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "../lib/tauri";
import { useNavigate } from "react-router-dom";
import { useSessionEvents } from "../hooks/useSessionEvents";
import { useActiveSession } from "../hooks/useActiveSession";
import {
  newSessionActionErrorMessage,
  startNewSession,
} from "../lib/sessionActions";
import {
  Archive,
  ArrowRight,
  Clock3,
  MessageSquarePlus,
  RefreshCw,
  Search,
  Trash2,
} from "lucide-react";

interface Session {
  id: string;
  title: string;
  status: string;
  created_at: number;
  updated_at: number;
  token_count: number;
}

type Filter = "all" | "active" | "paused" | "archived";

export function Chats() {
  const navigate = useNavigate();
  const { activeId } = useActiveSession();

  const [sessions, setSessions] = useState<Session[]>([]);
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");

  const fetchSessions = useCallback(async () => {
    try {
      setError(null);
      const result = await invoke<Session[]>("list_sessions");
      setSessions(result);
    } catch (e) {
      setError(`Saved sessions could not be loaded: ${String(e)}`);
    } finally {
      setLoading(false);
    }
  }, []);

  async function handleCreate() {
    if (creating) return;
    setCreating(true);
    setError(null);
    try {
      await startNewSession({ navigate });
    } catch (e) {
      await fetchSessions();
      setError(newSessionActionErrorMessage(e));
    } finally {
      setCreating(false);
    }
  }

  async function handleArchive(id: string, event: React.MouseEvent) {
    event.stopPropagation();
    try {
      await invoke("archive_session", { id });
      await fetchSessions();
    } catch (e) {
      setError(`Bluey could not archive this session: ${String(e)}`);
    }
  }

  async function handleDelete(id: string, title: string, event: React.MouseEvent) {
    event.stopPropagation();
    if (!confirm(`Delete session "${title}"? This cannot be undone.`)) return;
    try {
      await invoke("delete_session", { id });
      await fetchSessions();
    } catch (e) {
      setError(`Bluey could not delete this session: ${String(e)}`);
    }
  }

  useEffect(() => {
    fetchSessions();
  }, [fetchSessions]);

  // D1.8: refresh on session:created event (from daemon or other windows).
  const handleSessionCreated = useCallback(() => {
    void fetchSessions();
  }, [fetchSessions]);
  useSessionEvents(handleSessionCreated);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return [...sessions]
      .filter((s) => {
        if (filter !== "all" && s.status !== filter) return false;
        if (q && !s.title.toLowerCase().includes(q)) return false;
        return true;
      })
      .sort((a, b) => (b.updated_at || b.created_at) - (a.updated_at || a.created_at));
  }, [sessions, filter, query]);

  return (
    <div className="mx-auto max-w-6xl space-y-5">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="text-xs font-semibold uppercase tracking-[0.16em] text-cyan-300">
            Durable work history
          </p>
          <h1 className="mt-1 text-3xl font-semibold tracking-tight">Saved sessions</h1>
          <p className="mt-1 text-sm text-zinc-500">
            Continue, rename, export, archive, or remove work owned by the current desktop account.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void handleCreate()}
          disabled={creating}
          aria-busy={creating}
          className="inline-flex min-h-10 items-center gap-2 rounded-md bg-cyan-400 px-4 text-sm font-semibold text-zinc-950 hover:bg-cyan-300 disabled:cursor-not-allowed disabled:opacity-60"
        >
          <MessageSquarePlus aria-hidden="true" size={16} />
          {creating ? "Starting..." : "New session"}
        </button>
      </div>

      {error ? (
        <div role="alert" className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-red-500/35 bg-red-950/35 px-4 py-3 text-sm text-red-100">
          <span>{error}</span>
          <button
            type="button"
            onClick={() => void fetchSessions()}
            className="inline-flex min-h-8 items-center gap-1.5 rounded-md border border-red-400/30 px-3 text-xs font-semibold hover:bg-red-400/10"
          >
            <RefreshCw aria-hidden="true" size={13} />
            Refresh
          </button>
        </div>
      ) : null}

      <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_auto]">
        <label className="relative">
          <span className="sr-only">Search saved sessions by title</span>
          <Search
            aria-hidden="true"
            className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500"
            size={16}
          />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search by session title..."
            className="min-h-11 w-full rounded-md border border-zinc-800 bg-zinc-900 pl-10 pr-3 text-sm placeholder-zinc-600 focus:border-cyan-400 focus:outline-none"
          />
        </label>
        <div
          className="flex items-center gap-1 overflow-x-auto rounded-md border border-zinc-800 bg-zinc-900 p-1 text-xs"
          aria-label="Filter sessions"
        >
          {(["all", "active", "paused", "archived"] as Filter[]).map((f) => (
            <button
              type="button"
              key={f}
              onClick={() => setFilter(f)}
              aria-pressed={filter === f}
              className={`min-h-8 whitespace-nowrap rounded px-3 capitalize ${
                filter === f ? "bg-cyan-400/10 text-cyan-200" : "text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200"
              }`}
            >
              {f}
            </button>
          ))}
        </div>
      </div>

      {loading ? (
        <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-4 py-10 text-center text-sm text-zinc-500">
          Loading saved sessions...
        </div>
      ) : filtered.length === 0 ? (
        <div className="rounded-xl border border-dashed border-zinc-700 bg-zinc-900/60 px-6 py-12 text-center">
          <MessageSquarePlus aria-hidden="true" className="mx-auto text-zinc-600" size={30} />
          <p className="mt-3 text-sm font-medium text-zinc-200">
            {sessions.length === 0 ? "No sessions yet" : "No sessions match this view"}
          </p>
          <p className="mt-1 text-xs leading-5 text-zinc-500">
            {sessions.length === 0
              ? "Start one before a meeting, coding task, or research pass."
              : "Change the title search or status filter."}
          </p>
        </div>
      ) : (
        <ul className="space-y-3">
          {filtered.map((s) => {
            const isActive = s.id === activeId;
            return (
              <li
                key={s.id}
                className={`grid gap-3 rounded-xl border px-4 py-4 transition-colors sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center ${
                  isActive
                    ? "border-cyan-500/40 bg-cyan-950/20"
                    : "border-zinc-800 bg-zinc-900 hover:border-zinc-700"
                }`}
              >
                <button
                  type="button"
                  onClick={() => navigate(`/session/${s.id}`)}
                  className="min-w-0 text-left"
                  aria-label={`Open ${s.title}`}
                >
                  <span className="flex items-center gap-2">
                    <span className="truncate font-medium text-zinc-100">{s.title}</span>
                    {isActive && (
                      <span className="shrink-0 rounded-full bg-emerald-400/10 px-2 py-0.5 text-[11px] font-semibold text-emerald-200">
                        live
                      </span>
                    )}
                  </span>
                  <span className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-zinc-500">
                    <span className="inline-flex items-center gap-1.5">
                      <Clock3 aria-hidden="true" size={13} />
                      Updated {formatTime(s.updated_at || s.created_at)}
                    </span>
                    <span>{s.status === "archived" ? "Archived history" : "Saved history"}</span>
                  </span>
                </button>
                <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                  <span
                    className={`rounded-full px-2 py-0.5 text-xs ${
                      s.status === "active"
                        ? "bg-emerald-400/10 text-emerald-300"
                        : s.status === "archived"
                          ? "bg-zinc-700 text-zinc-400"
                          : "bg-amber-400/10 text-amber-300"
                    }`}
                  >
                    {s.status}
                  </span>
                  {s.status !== "archived" && (
                    <button
                      type="button"
                      onClick={(e) => handleArchive(s.id, e)}
                      aria-label={`Archive ${s.title}`}
                      className="inline-flex min-h-8 items-center gap-1.5 rounded-md border border-zinc-700 px-2.5 text-xs text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200"
                    >
                      <Archive aria-hidden="true" size={13} />
                      Archive
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={(e) => handleDelete(s.id, s.title, e)}
                    aria-label={`Delete ${s.title}`}
                    className="inline-flex min-h-8 items-center gap-1.5 rounded-md border border-red-500/20 px-2.5 text-xs text-red-300 hover:bg-red-500/10"
                  >
                    <Trash2 aria-hidden="true" size={13} />
                    Delete
                  </button>
                  <button
                    type="button"
                    onClick={() => navigate(`/session/${s.id}`)}
                    aria-label={`Continue ${s.title}`}
                    className="grid h-8 w-8 place-items-center rounded-md text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200"
                  >
                    <ArrowRight aria-hidden="true" size={15} />
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

function formatTime(ms: number): string {
  const date = new Date(ms);
  if (Number.isNaN(date.getTime())) return "at an unknown time";
  return date.toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}
