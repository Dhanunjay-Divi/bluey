import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "../lib/tauri";
import { useNavigate } from "react-router-dom";
import { useSessionEvents } from "../hooks/useSessionEvents";
import { useActiveSession } from "../hooks/useActiveSession";

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
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");

  const fetchSessions = useCallback(async () => {
    try {
      setError(null);
      const result = await invoke<Session[]>("list_sessions");
      setSessions(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  async function handleCreate() {
    try {
      const s = await invoke<Session>("create_session", { title: null });
      await fetchSessions();
      navigate(`/session/${s.id}`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleArchive(id: string, event: React.MouseEvent) {
    event.stopPropagation();
    try {
      await invoke("archive_session", { id });
      await fetchSessions();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleDelete(id: string, title: string, event: React.MouseEvent) {
    event.stopPropagation();
    if (!confirm(`Delete session "${title}"? This cannot be undone.`)) return;
    try {
      await invoke("delete_session", { id });
      await fetchSessions();
    } catch (e) {
      setError(String(e));
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
    return sessions.filter((s) => {
      if (filter !== "all" && s.status !== filter) return false;
      if (q && !s.title.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [sessions, filter, query]);

  if (loading) return <p className="text-zinc-500">Loading sessions...</p>;
  if (error) return <p className="text-red-400">Error: {error}</p>;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h2 className="text-xl font-semibold">Saved sessions</h2>
          <p className="mt-1 text-sm text-zinc-500">
            Local sessions on this desktop. Sign in to sync them to your account.
          </p>
        </div>
        <button
          onClick={handleCreate}
          className="rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium hover:bg-blue-500"
        >
          New session
        </button>
      </div>

      <div className="flex items-center gap-3">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search by title..."
          className="flex-1 rounded-md border border-zinc-800 bg-zinc-900 px-3 py-1.5 text-sm placeholder-zinc-600 focus:border-zinc-600 focus:outline-none"
        />
        <div className="flex items-center gap-1 rounded-md border border-zinc-800 bg-zinc-900 p-0.5 text-xs">
          {(["all", "active", "paused", "archived"] as Filter[]).map((f) => (
            <button
              key={f}
              onClick={() => setFilter(f)}
              className={`rounded px-2 py-1 capitalize ${
                filter === f ? "bg-zinc-800 text-zinc-200" : "text-zinc-500 hover:text-zinc-300"
              }`}
            >
              {f}
            </button>
          ))}
        </div>
      </div>

      {filtered.length === 0 ? (
        <p className="text-zinc-500">
          {sessions.length === 0
            ? "No sessions yet. Start one before a meeting, coding task, or research pass."
            : "No sessions match the current filter."}
        </p>
      ) : (
        <ul className="space-y-2">
          {filtered.map((s) => {
            const isActive = s.id === activeId;
            return (
              <li
                key={s.id}
                onClick={() => navigate(`/session/${s.id}`)}
                className={`flex items-center justify-between rounded-md border px-4 py-3 transition-colors cursor-pointer ${
                  isActive
                    ? "border-blue-700 bg-blue-950/30"
                    : "border-zinc-800 bg-zinc-900 hover:bg-zinc-850 hover:border-zinc-700"
                }`}
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <p className="truncate font-medium">{s.title}</p>
                    {isActive && (
                      <span className="shrink-0 rounded-full bg-blue-900/40 px-2 py-0.5 text-xs text-blue-300">
                        active
                      </span>
                    )}
                  </div>
                  <p className="text-xs text-zinc-500">
                    {new Date(s.created_at).toLocaleString()} · {s.token_count} tokens
                  </p>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  <span
                    className={`rounded-full px-2 py-0.5 text-xs ${
                      s.status === "active"
                        ? "bg-green-900/50 text-green-400"
                        : s.status === "archived"
                          ? "bg-zinc-700 text-zinc-400"
                          : "bg-yellow-900/50 text-yellow-400"
                    }`}
                  >
                    {s.status}
                  </span>
                  {s.status !== "archived" && (
                    <button
                      onClick={(e) => handleArchive(s.id, e)}
                      className="rounded px-2 py-1 text-xs text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200"
                    >
                      Archive
                    </button>
                  )}
                  <button
                    onClick={(e) => handleDelete(s.id, s.title, e)}
                    className="rounded px-2 py-1 text-xs text-red-400 hover:bg-red-950 hover:text-red-300"
                  >
                    Delete
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
