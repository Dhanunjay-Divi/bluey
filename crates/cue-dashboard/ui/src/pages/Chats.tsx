import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSessionEvents } from "../hooks/useSessionEvents";

interface Session {
  id: string;
  title: string;
  status: string;
  created_at: number;
  updated_at: number;
  token_count: number;
}

export function Chats() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

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
      await invoke("create_session", { title: null });
      await fetchSessions();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleArchive(id: string) {
    try {
      await invoke("archive_session", { id });
      await fetchSessions();
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    fetchSessions();
  }, [fetchSessions]);

  // D1.8: refresh session list when the daemon emits `session:created`.
  // Today create_session is called directly from this window, but Phase 2+
  // will emit events from the daemon side when sessions arrive through other
  // paths (e.g. CLI, hotkey, background jobs), and this hook will pick them up
  // without any change here.
  const handleSessionCreated = useCallback(() => {
    void fetchSessions();
  }, [fetchSessions]);
  useSessionEvents(handleSessionCreated);

  if (loading) {
    return <p className="text-zinc-500">Loading sessions...</p>;
  }

  if (error) {
    return <p className="text-red-400">Error: {error}</p>;
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-semibold">Sessions</h2>
        <button
          onClick={handleCreate}
          className="rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium hover:bg-blue-500"
        >
          New Session
        </button>
      </div>
      {sessions.length === 0 ? (
        <p className="text-zinc-500">No sessions yet. Create one to get started.</p>
      ) : (
        <ul className="space-y-2">
          {sessions.map((s) => (
            <li
              key={s.id}
              className="flex items-center justify-between rounded-md border border-zinc-800 bg-zinc-900 px-4 py-3"
            >
              <div>
                <p className="font-medium">{s.title}</p>
                <p className="text-xs text-zinc-500">
                  {new Date(s.created_at).toLocaleString()} · {s.token_count} tokens
                </p>
              </div>
              <div className="flex items-center gap-2">
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
                    onClick={() => handleArchive(s.id)}
                    className="rounded px-2 py-1 text-xs text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200"
                  >
                    Archive
                  </button>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
