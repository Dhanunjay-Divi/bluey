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

  if (loading) return <p className="text-callout text-text-tertiary">Loading sessions...</p>;
  if (error) return <p className="text-callout text-error">Error: {error}</p>;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-4">
        <h2 className="text-title-2 text-text-primary">Sessions</h2>
        <button
          onClick={handleCreate}
          className="rounded-md bg-accent px-3 py-1.5 text-subhead text-white transition-colors duration-200 hover:bg-accent-hover"
        >
          New Session
        </button>
      </div>

      <div className="flex items-center gap-3">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search by title..."
          className="flex-1 rounded-md border border-hairline bg-bg-input px-3 py-1.5 text-callout placeholder-text-quaternary transition-colors duration-200 focus:border-hairline-strong focus:outline-none"
        />
        <div className="flex items-center gap-1 rounded-md border border-hairline bg-bg-input p-0.5 text-footnote">
          {(["all", "active", "paused", "archived"] as Filter[]).map((f) => (
            <button
              key={f}
              onClick={() => setFilter(f)}
              className={`rounded px-2 py-1 capitalize transition-colors duration-200 ${
                filter === f
                  ? "bg-bg-raised-2 text-text-primary"
                  : "text-text-tertiary hover:text-text-secondary"
              }`}
            >
              {f}
            </button>
          ))}
        </div>
      </div>

      {filtered.length === 0 ? (
        <p className="text-callout text-text-tertiary">
          {sessions.length === 0
            ? "No sessions yet. Create one to get started."
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
                className={`flex items-center justify-between rounded-md px-4 py-3 transition-colors duration-200 cursor-pointer ${
                  isActive
                    ? "glass-strong border-accent"
                    : "glass hover:bg-bg-raised-2 hover:border-hairline-strong"
                }`}
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <p className="truncate text-headline text-text-primary">{s.title}</p>
                    {isActive && (
                      <span className="shrink-0 rounded-full bg-accent-subtle px-2 py-0.5 text-caption text-accent-subtle-text">
                        active
                      </span>
                    )}
                  </div>
                  <p className="text-footnote text-text-tertiary">
                    {new Date(s.created_at).toLocaleString()} · {s.token_count} tokens
                  </p>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  <span
                    className={`rounded-full px-2 py-0.5 text-caption ${
                      s.status === "active"
                        ? "bg-success/10 text-success"
                        : s.status === "archived"
                          ? "bg-bg-raised-2 text-text-tertiary"
                          : "bg-warning/10 text-warning"
                    }`}
                  >
                    {s.status}
                  </span>
                  {s.status !== "archived" && (
                    <button
                      onClick={(e) => handleArchive(s.id, e)}
                      className="rounded px-2 py-1 text-footnote text-text-tertiary transition-colors duration-200 hover:bg-bg-raised-2 hover:text-text-primary"
                    >
                      Archive
                    </button>
                  )}
                  <button
                    onClick={(e) => handleDelete(s.id, s.title, e)}
                    className="rounded px-2 py-1 text-footnote text-error transition-colors duration-200 hover:bg-error/10"
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
