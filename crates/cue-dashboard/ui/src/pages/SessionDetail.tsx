import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  Archive,
  ArrowLeft,
  Check,
  Clipboard,
  Clock3,
  MessageSquareText,
  Pencil,
  Radio,
  Search,
  Trash2,
} from "lucide-react";
import { invoke } from "../lib/tauri";
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
  const [loadError, setLoadError] = useState("");
  const [actionError, setActionError] = useState("");
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  const [query, setQuery] = useState("");
  const [copyStatus, setCopyStatus] = useState("");
  const [busyAction, setBusyAction] = useState("");

  const load = useCallback(async () => {
    if (!id) return;
    setLoading(true);
    setLoadError("");
    try {
      const [nextSession, nextTurns] = await Promise.all([
        invoke<Session | null>("get_session", { id }),
        invoke<Turn[]>("list_turns", { sessionId: id }),
      ]);
      if (!nextSession) {
        setLoadError("This session is unavailable or belongs to another desktop account.");
        setSession(null);
      } else {
        setSession(nextSession);
        setTitleDraft(nextSession.title);
      }
      setTurns(nextTurns);
    } catch (nextError) {
      setLoadError(String(nextError));
    } finally {
      setLoading(false);
    }
  }, [id]);

  useEffect(() => {
    void load();
  }, [load]);

  const visibleTurns = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return turns;
    return turns.filter(
      (turn) =>
        turn.user_message.toLowerCase().includes(normalized) ||
        turn.model_response.toLowerCase().includes(normalized),
    );
  }, [query, turns]);

  async function saveTitle() {
    if (!session) return;
    const trimmed = titleDraft.trim();
    if (!trimmed || trimmed === session.title) {
      setEditingTitle(false);
      setTitleDraft(session.title);
      return;
    }
    setBusyAction("title");
    setActionError("");
    try {
      await invoke("update_session_title", { id: session.id, title: trimmed });
      setEditingTitle(false);
      await load();
    } catch (nextError) {
      setActionError(String(nextError));
    } finally {
      setBusyAction("");
    }
  }

  async function archiveSession() {
    if (!session) return;
    setBusyAction("archive");
    setActionError("");
    try {
      await invoke("archive_session", { id: session.id });
      await load();
    } catch (nextError) {
      setActionError(String(nextError));
    } finally {
      setBusyAction("");
    }
  }

  async function deleteSession() {
    if (!session) return;
    if (!confirm(`Delete session "${session.title}"? This cannot be undone.`)) return;
    setBusyAction("delete");
    setActionError("");
    try {
      await invoke("delete_session", { id: session.id });
      navigate("/chats");
    } catch (nextError) {
      setActionError(String(nextError));
      setBusyAction("");
    }
  }

  async function copySession() {
    if (!session) return;
    setBusyAction("copy");
    setActionError("");
    setCopyStatus("");
    try {
      const content = await invoke<string>("export_session_to_clipboard", {
        id: session.id,
        format: "markdown",
      });
      await navigator.clipboard.writeText(content);
      setCopyStatus("Session copied as Markdown.");
    } catch (nextError) {
      setActionError(String(nextError));
    } finally {
      setBusyAction("");
    }
  }

  async function copyAnswer(turn: Turn) {
    setCopyStatus("");
    try {
      await navigator.clipboard.writeText(turn.model_response);
      setCopyStatus(`Answer ${turn.turn_index + 1} copied.`);
    } catch (nextError) {
      setActionError(String(nextError));
    }
  }

  async function continueLive() {
    if (!session || busyAction) return;
    setBusyAction("continue");
    setActionError("");
    try {
      await setActive(session.id);
      navigate("/live");
    } catch (nextError) {
      setActionError(`Bluey could not continue this session: ${String(nextError)}`);
    } finally {
      setBusyAction("");
    }
  }

  if (loading) {
    return (
      <div className="mx-auto max-w-5xl rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-sm text-zinc-500">
        Loading session history...
      </div>
    );
  }

  if (loadError || !session) {
    return (
      <div className="mx-auto max-w-xl rounded-xl border border-red-500/30 bg-red-950/30 p-6 text-center">
        <p role="alert" className="text-sm text-red-100">
          {loadError || "Session unavailable."}
        </p>
        <div className="mt-4 flex justify-center gap-2">
          <button
            type="button"
            onClick={() => void load()}
            className="rounded-md border border-red-400/30 px-3 py-2 text-sm text-red-100 hover:bg-red-400/10"
          >
            Try again
          </button>
          <button
            type="button"
            onClick={() => navigate("/chats")}
            className="rounded-md bg-zinc-800 px-3 py-2 text-sm text-zinc-100 hover:bg-zinc-700"
          >
            Saved sessions
          </button>
        </div>
      </div>
    );
  }

  const isActive = activeId === session.id;

  return (
    <div className="mx-auto max-w-5xl space-y-5">
      <button
        type="button"
        onClick={() => navigate("/chats")}
        className="inline-flex min-h-9 items-center gap-2 rounded-md text-sm font-semibold text-zinc-400 hover:text-zinc-100"
      >
        <ArrowLeft aria-hidden="true" size={15} />
        Saved sessions
      </button>

      <header className="rounded-xl border border-zinc-800 bg-zinc-900 p-5">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0 flex-1">
            {editingTitle ? (
              <div className="flex flex-wrap items-center gap-2">
                <input
                  autoFocus
                  value={titleDraft}
                  onChange={(event) => setTitleDraft(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") void saveTitle();
                    if (event.key === "Escape") {
                      setEditingTitle(false);
                      setTitleDraft(session.title);
                    }
                  }}
                  className="min-h-11 min-w-0 flex-1 rounded-md border border-zinc-700 bg-zinc-950 px-3 text-xl font-semibold text-zinc-100 outline-none focus:border-cyan-400"
                />
                <button
                  type="button"
                  disabled={busyAction === "title"}
                  onClick={() => void saveTitle()}
                  className="min-h-11 rounded-md bg-cyan-400 px-4 text-sm font-semibold text-zinc-950 hover:bg-cyan-300 disabled:opacity-50"
                >
                  Save
                </button>
              </div>
            ) : (
              <button
                type="button"
                onClick={() => setEditingTitle(true)}
                className="group flex max-w-full items-center gap-2 text-left"
                aria-label={`Rename ${session.title}`}
              >
                <h1 className="truncate text-2xl font-semibold tracking-tight text-zinc-50">
                  {session.title}
                </h1>
                <Pencil aria-hidden="true" className="shrink-0 text-zinc-600 group-hover:text-cyan-300" size={15} />
              </button>
            )}
            <p className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-zinc-500">
              <span className="inline-flex items-center gap-1.5">
                <Clock3 aria-hidden="true" size={13} />
                Updated {formatDate(session.updated_at || session.created_at)}
              </span>
              <span>{turns.length} {turns.length === 1 ? "turn" : "turns"}</span>
              <span>Saved session history</span>
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <span
              className={`rounded-full px-2.5 py-1 text-xs font-semibold ${
                isActive
                  ? "bg-emerald-400/10 text-emerald-200"
                  : session.status === "archived"
                    ? "bg-zinc-800 text-zinc-400"
                    : "bg-amber-400/10 text-amber-200"
              }`}
            >
              {isActive ? "Active" : session.status}
            </span>
            <button
              type="button"
              disabled={Boolean(busyAction)}
              aria-busy={busyAction === "continue"}
              onClick={() => void continueLive()}
              className="inline-flex min-h-9 items-center gap-2 rounded-md bg-cyan-400 px-3 text-xs font-semibold text-zinc-950 hover:bg-cyan-300 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {busyAction === "continue" ? (
                <span
                  aria-hidden="true"
                  className="h-3.5 w-3.5 animate-spin rounded-full border-2 border-zinc-950/30 border-t-zinc-950"
                />
              ) : (
                <Radio aria-hidden="true" size={14} />
              )}
              {busyAction === "continue" ? "Continuing..." : isActive ? "Open live" : "Continue live"}
            </button>
            <button
              type="button"
              disabled={Boolean(busyAction)}
              onClick={() => void copySession()}
              className="inline-flex min-h-9 items-center gap-2 rounded-md border border-zinc-700 px-3 text-xs font-semibold text-zinc-200 hover:bg-zinc-800 disabled:opacity-50"
            >
              <Clipboard aria-hidden="true" size={14} />
              {busyAction === "copy" ? "Copying..." : "Copy Markdown"}
            </button>
            {session.status !== "archived" ? (
              <button
                type="button"
                disabled={Boolean(busyAction)}
                onClick={() => void archiveSession()}
                className="inline-flex min-h-9 items-center gap-2 rounded-md border border-zinc-700 px-3 text-xs font-semibold text-zinc-300 hover:bg-zinc-800 disabled:opacity-50"
              >
                <Archive aria-hidden="true" size={14} />
                Archive
              </button>
            ) : null}
            <button
              type="button"
              disabled={Boolean(busyAction)}
              onClick={() => void deleteSession()}
              className="inline-flex min-h-9 items-center gap-2 rounded-md border border-red-500/25 px-3 text-xs font-semibold text-red-300 hover:bg-red-500/10 disabled:opacity-50"
            >
              <Trash2 aria-hidden="true" size={14} />
              Delete
            </button>
          </div>
        </div>
        {actionError ? (
          <p role="alert" className="mt-4 rounded-md border border-red-500/25 bg-red-500/10 px-3 py-2 text-xs text-red-200">
            {actionError}
          </p>
        ) : null}
        {copyStatus ? (
          <p role="status" className="mt-4 flex items-center gap-2 text-xs text-emerald-300">
            <Check aria-hidden="true" size={14} />
            {copyStatus}
          </p>
        ) : null}
      </header>

      <section className="space-y-3" aria-labelledby="conversation-title">
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div>
            <h2 id="conversation-title" className="text-lg font-semibold text-zinc-100">
              Conversation
            </h2>
            <p className="mt-1 text-xs text-zinc-500">
              Questions and completed answers are shown without internal provider diagnostics.
            </p>
          </div>
          <label className="relative w-full sm:w-72">
            <span className="sr-only">Search within this session</span>
            <Search
              aria-hidden="true"
              className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500"
              size={15}
            />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              className="min-h-10 w-full rounded-md border border-zinc-800 bg-zinc-900 pl-9 pr-3 text-sm text-zinc-100 outline-none placeholder:text-zinc-600 focus:border-cyan-400"
              placeholder="Search this session..."
            />
          </label>
        </div>

        {turns.length === 0 ? (
          <div className="rounded-xl border border-dashed border-zinc-700 bg-zinc-900/50 px-5 py-12 text-center">
            <MessageSquareText aria-hidden="true" className="mx-auto text-zinc-600" size={30} />
            <p className="mt-3 text-sm font-medium text-zinc-200">No conversation yet</p>
            <p className="mt-1 text-xs text-zinc-500">
              Continue live to listen, attach context, and ask Bluey.
            </p>
          </div>
        ) : visibleTurns.length === 0 ? (
          <p className="rounded-xl border border-zinc-800 bg-zinc-900 px-4 py-8 text-center text-sm text-zinc-500">
            No questions or answers match “{query}”.
          </p>
        ) : (
          <ol className="space-y-4">
            {visibleTurns.map((turn) => (
              <li key={turn.id} className="overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900">
                <div className="border-b border-zinc-800 bg-zinc-950/55 px-4 py-3">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <span className="text-xs font-semibold text-zinc-500">
                      Question {turn.turn_index + 1} · {formatDate(turn.created_at)}
                    </span>
                    {turn.duration_ms !== null ? (
                      <span className="text-[11px] text-zinc-600">
                        Answered in {formatDuration(turn.duration_ms)}
                      </span>
                    ) : null}
                  </div>
                  <p className="mt-2 whitespace-pre-wrap text-sm leading-6 text-zinc-200">
                    {turn.user_message}
                  </p>
                </div>
                <div className="px-4 py-4">
                  <div className="mb-2 flex items-center justify-between gap-2">
                    <span className="text-xs font-semibold uppercase tracking-[0.14em] text-cyan-300">
                      Bluey
                    </span>
                    <button
                      type="button"
                      onClick={() => void copyAnswer(turn)}
                      className="inline-flex min-h-8 items-center gap-1.5 rounded-md px-2 text-xs font-semibold text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100"
                    >
                      <Clipboard aria-hidden="true" size={13} />
                      Copy
                    </button>
                  </div>
                  <p className="whitespace-pre-wrap text-sm leading-6 text-zinc-300">
                    {turn.model_response}
                  </p>
                </div>
              </li>
            ))}
          </ol>
        )}
      </section>
    </div>
  );
}

function formatDate(ms: number): string {
  const date = new Date(ms);
  if (Number.isNaN(date.getTime())) return "unknown time";
  return date.toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function formatDuration(ms: number): string {
  if (ms < 1_000) return `${Math.max(1, Math.round(ms))} ms`;
  return `${(ms / 1_000).toFixed(ms < 10_000 ? 1 : 0)} s`;
}
