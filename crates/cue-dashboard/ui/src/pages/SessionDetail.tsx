import { useCallback, useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useActiveSession } from "../hooks/useActiveSession";

interface Session {
  id: string;
  title: string;
  status: string;
  created_at: number;
  updated_at: number;
  token_count: number;
}

interface Turn {
  id: string;
  session_id: string;
  turn_index: number;
  user_message: string;
  model_response: string;
  lane: string;
  provider: string;
  model: string;
  created_at: number;
  duration_ms: number | null;
  input_tokens: number | null;
  output_tokens: number | null;
  cost_cents: number | null;
}

export function SessionDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { activeId, setActive } = useActiveSession();

  const [session, setSession] = useState<Session | null>(null);
  const [turns, setTurns] = useState<Turn[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");

  const load = useCallback(async () => {
    if (!id) return;
    try {
      setError(null);
      const [s, ts] = await Promise.all([
        invoke<Session | null>("get_session", { id }),
        invoke<Turn[]>("list_turns", { sessionId: id }),
      ]);
      if (!s) {
        setError(`Session ${id} not found.`);
        setSession(null);
      } else {
        setSession(s);
        setTitleDraft(s.title);
      }
      setTurns(ts);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [id]);

  useEffect(() => {
    load();
  }, [load]);

  // Mark this session active whenever the route points at it.
  useEffect(() => {
    if (id && id !== activeId) {
      void setActive(id);
    }
  }, [id, activeId, setActive]);

  async function saveTitle() {
    if (!session) return;
    const trimmed = titleDraft.trim();
    if (!trimmed || trimmed === session.title) {
      setEditingTitle(false);
      setTitleDraft(session.title);
      return;
    }
    try {
      await invoke("update_session_title", { id: session.id, title: trimmed });
      setEditingTitle(false);
      await load();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleArchive() {
    if (!session) return;
    try {
      await invoke("archive_session", { id: session.id });
      await load();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleDelete() {
    if (!session) return;
    if (!confirm(`Delete session "${session.title}"? This cannot be undone.`)) {
      return;
    }
    try {
      await invoke("delete_session", { id: session.id });
      navigate("/chats");
    } catch (e) {
      setError(String(e));
    }
  }

  if (loading) return <p className="text-zinc-500">Loading session...</p>;
  if (error) return <p className="text-red-400">Error: {error}</p>;
  if (!session) return null;

  return (
    <div className="space-y-6">
      <div className="flex items-start justify-between gap-4">
        <div className="flex-1 min-w-0">
          {editingTitle ? (
            <div className="flex items-center gap-2">
              <input
                autoFocus
                value={titleDraft}
                onChange={(e) => setTitleDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") saveTitle();
                  if (e.key === "Escape") {
                    setEditingTitle(false);
                    setTitleDraft(session.title);
                  }
                }}
                className="w-full rounded-md border border-zinc-700 bg-zinc-900 px-3 py-2 text-xl font-semibold"
              />
              <button
                onClick={saveTitle}
                className="rounded-md bg-blue-600 px-3 py-2 text-sm font-medium hover:bg-blue-500"
              >
                Save
              </button>
            </div>
          ) : (
            <button
              onClick={() => setEditingTitle(true)}
              title="Click to rename"
              className="text-left text-xl font-semibold hover:text-zinc-300"
            >
              {session.title}
            </button>
          )}
          <p className="mt-1 text-xs text-zinc-500">
            ID {session.id.slice(0, 8)} · created{" "}
            {new Date(session.created_at).toLocaleString()} · {session.token_count} tokens
          </p>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <span
            className={`rounded-full px-2 py-0.5 text-xs ${
              session.status === "active"
                ? "bg-green-900/50 text-green-400"
                : session.status === "archived"
                  ? "bg-zinc-700 text-zinc-400"
                  : "bg-yellow-900/50 text-yellow-400"
            }`}
          >
            {session.status}
          </span>
          {activeId === session.id && (
            <span className="rounded-full bg-blue-900/40 px-2 py-0.5 text-xs text-blue-300">
              active
            </span>
          )}
          {session.status !== "archived" && (
            <button
              onClick={handleArchive}
              className="rounded px-2 py-1 text-xs text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200"
            >
              Archive
            </button>
          )}
          <button
            onClick={handleDelete}
            className="rounded px-2 py-1 text-xs text-red-400 hover:bg-red-950 hover:text-red-300"
          >
            Delete
          </button>
        </div>
      </div>

      <div>
        <h3 className="mb-2 text-sm font-medium text-zinc-400">Turns</h3>
        {turns.length === 0 ? (
          <p className="text-sm text-zinc-600">
            No turns yet. This session has no conversation history.
          </p>
        ) : (
          <ul className="space-y-3">
            {turns.map((t) => (
              <li
                key={t.id}
                className="rounded-md border border-zinc-800 bg-zinc-900 px-4 py-3 text-sm"
              >
                <div className="mb-1 flex items-center gap-2 text-xs text-zinc-500">
                  <span className="rounded bg-zinc-800 px-1.5 py-0.5">{t.lane}</span>
                  <span>
                    {t.provider}/{t.model}
                  </span>
                  {t.duration_ms !== null && <span>{t.duration_ms}ms</span>}
                </div>
                <p className="text-zinc-300">
                  <span className="text-zinc-500">user:</span> {t.user_message}
                </p>
                <p className="mt-1 text-zinc-300">
                  <span className="text-zinc-500">model:</span> {t.model_response}
                </p>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
