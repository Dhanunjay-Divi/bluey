import { useCallback, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { useNavigate } from "react-router-dom";
import {
  ArrowRight,
  CircleDollarSign,
  Clock3,
  ExternalLink,
  Headphones,
  LogIn,
  MessageSquarePlus,
  Search,
  Settings,
  Sparkles,
} from "lucide-react";
import { invoke } from "../lib/tauri";

interface Session {
  id: string;
  title: string;
  status: string;
  created_at: number;
  updated_at: number;
  token_count: number;
}

interface AccountMe {
  email: string;
  balance_cents: number;
  trial_seconds_remaining: number;
  auto_topup_enabled?: boolean;
  auto_topup_threshold_cents?: number;
  auto_topup_amount_cents?: number;
}

export function Home() {
  const navigate = useNavigate();
  const [sessions, setSessions] = useState<Session[]>([]);
  const [account, setAccount] = useState<AccountMe | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const load = useCallback(async (silent = false) => {
    if (!silent) setLoading(true);
    if (!silent) setError(null);
    const [sessionResult, accountResult, activeResult] = await Promise.allSettled([
      invoke<Session[]>("list_sessions"),
      invoke<AccountMe | null>("account_me"),
      invoke<string | null>("get_active_session"),
    ]);
    if (sessionResult.status === "fulfilled") {
      setSessions(sessionResult.value);
    } else if (!silent) {
      setError(String(sessionResult.reason));
    }
    if (accountResult.status === "fulfilled") {
      setAccount(accountResult.value);
    }
    if (activeResult.status === "fulfilled") {
      setActiveId(activeResult.value);
    }
    if (!silent) setLoading(false);
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    const id = window.setInterval(() => {
      void load(true);
    }, 10_000);
    return () => window.clearInterval(id);
  }, [load]);

  const recentSessions = useMemo(
    () =>
      [...sessions]
        .sort((a, b) => (b.updated_at || b.created_at) - (a.updated_at || a.created_at))
        .slice(0, 4),
    [sessions],
  );

  const activeSession = sessions.find((session) => session.id === activeId) ?? recentSessions[0];
  const balanceTone = account
    ? account.balance_cents < 500
      ? "text-red-300"
      : account.balance_cents < 1_000
        ? "text-amber-300"
        : "text-cyan-200"
    : "text-zinc-200";

  async function createSession() {
    setCreating(true);
    setError(null);
    try {
      const session = await invoke<Session>("create_session", { title: null });
      navigate(`/session/${session.id}`);
    } catch (err) {
      setError(String(err));
    } finally {
      setCreating(false);
    }
  }

  async function signIn() {
    try {
      const url = await invoke<string>("get_signin_url");
      window.open(url, "_blank");
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <section className="grid gap-4 rounded-lg border border-zinc-800 bg-zinc-900/70 p-5 shadow-2xl shadow-black/20 md:grid-cols-[minmax(0,1fr)_auto] md:items-end">
        <div className="min-w-0 space-y-3">
          <div className="inline-flex items-center gap-2 rounded-full border border-cyan-400/25 bg-cyan-400/10 px-3 py-1 text-xs font-semibold text-cyan-200">
            <Sparkles className="h-3.5 w-3.5" />
            Bluey desktop dashboard
          </div>
          <div>
            <h1 className="text-3xl font-semibold tracking-tight text-zinc-50 md:text-4xl">
              Start with one session.
            </h1>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-zinc-400">
              Bluey keeps the current conversation, screen context, files, answers, and saved history together.
              Use the first action below when you are not sure where to start.
            </p>
          </div>
        </div>
        <div className="flex flex-wrap gap-2 md:justify-end">
          <button
            type="button"
            onClick={createSession}
            disabled={creating}
            className="inline-flex min-h-10 items-center justify-center gap-2 rounded-md bg-blue-500 px-4 text-sm font-semibold text-white hover:bg-blue-400 disabled:cursor-not-allowed disabled:opacity-60"
          >
            <MessageSquarePlus className="h-4 w-4" />
            {creating ? "Starting..." : "New session"}
          </button>
          <button
            type="button"
            onClick={() => navigate("/chats")}
            className="inline-flex min-h-10 items-center justify-center gap-2 rounded-md border border-zinc-700 px-4 text-sm font-semibold text-zinc-200 hover:border-zinc-500 hover:bg-zinc-800"
          >
            Saved sessions
            <ArrowRight className="h-4 w-4" />
          </button>
        </div>
      </section>

      {error ? (
        <div className="rounded-lg border border-red-500/35 bg-red-950/35 px-4 py-3 text-sm text-red-100">
          {error}
        </div>
      ) : null}

      <section className="grid gap-4 md:grid-cols-3">
        <ActionCard
          icon={<MessageSquarePlus className="h-5 w-5" />}
          title="1. Open a session"
          body={activeSession ? activeSession.title : "Create a session before a meeting, coding task, or research pass."}
          cta={activeSession ? "Continue session" : "Create session"}
          onClick={activeSession ? () => navigate(`/session/${activeSession.id}`) : createSession}
        />
        <ActionCard
          icon={<Headphones className="h-5 w-5" />}
          title="2. Ask from work"
          body="Use Bluey from the overlay while the conversation, document, or screen is fresh."
          cta="Live transcript"
          onClick={() => navigate("/live")}
        />
        <ActionCard
          icon={<Search className="h-5 w-5" />}
          title="3. Find it later"
          body={`${sessions.length} local ${sessions.length === 1 ? "session" : "sessions"} available on this machine.`}
          cta="Search sessions"
          onClick={() => navigate("/search")}
        />
      </section>

      <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_360px]">
        <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-4">
          <div className="mb-4 flex items-center justify-between gap-3">
            <div>
              <h2 className="text-lg font-semibold text-zinc-100">Recent sessions</h2>
              <p className="text-sm text-zinc-500">Newest local work on this desktop.</p>
            </div>
            <button
              type="button"
              onClick={() => navigate("/chats")}
              className="rounded-md border border-zinc-700 px-3 py-2 text-xs font-semibold text-zinc-300 hover:border-zinc-500 hover:bg-zinc-800"
            >
              View all
            </button>
          </div>
          {loading ? (
            <p className="rounded-md border border-zinc-800 bg-zinc-950 px-4 py-6 text-center text-sm text-zinc-500">
              Loading sessions...
            </p>
          ) : recentSessions.length ? (
            <div className="grid gap-2">
              {recentSessions.map((session) => (
                <button
                  key={session.id}
                  type="button"
                  onClick={() => navigate(`/session/${session.id}`)}
                  className="grid min-h-16 grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-md border border-zinc-800 bg-zinc-950 px-4 py-3 text-left hover:border-zinc-600 hover:bg-zinc-900"
                >
                  <span className="min-w-0">
                    <span className="flex items-center gap-2">
                      <span className="truncate text-sm font-medium text-zinc-100">{session.title}</span>
                      {session.id === activeId ? (
                        <span className="rounded-full bg-blue-500/15 px-2 py-0.5 text-[11px] font-semibold text-blue-200">
                          active
                        </span>
                      ) : null}
                    </span>
                    <span className="mt-1 flex items-center gap-2 text-xs text-zinc-500">
                      <Clock3 className="h-3.5 w-3.5" />
                      {formatTime(session.updated_at || session.created_at)}
                    </span>
                  </span>
                  <ArrowRight className="h-4 w-4 text-zinc-500" />
                </button>
              ))}
            </div>
          ) : (
            <div className="rounded-md border border-zinc-800 bg-zinc-950 px-4 py-6 text-center">
              <p className="text-sm font-medium text-zinc-200">No sessions yet.</p>
              <p className="mt-1 text-sm text-zinc-500">Start one and Bluey will keep its local history here.</p>
            </div>
          )}
        </div>

        <aside className="space-y-4">
          <div className="rounded-lg border border-zinc-800 bg-zinc-900 p-4">
            <div className="mb-3 flex items-center gap-2">
              <CircleDollarSign className="h-5 w-5 text-cyan-300" />
              <h2 className="text-lg font-semibold text-zinc-100">Account and credits</h2>
            </div>
            {account ? (
              <div className="space-y-3">
                <div>
                  <p className="truncate text-sm font-medium text-zinc-100">{account.email}</p>
                  <p className="mt-1 text-sm text-zinc-500">Signed in on this desktop.</p>
                </div>
                <div className="rounded-md border border-zinc-800 bg-zinc-950 p-3">
                  <span className="text-xs font-semibold uppercase tracking-wide text-zinc-500">
                    Credits balance
                  </span>
                  <strong className={`mt-1 block text-2xl font-semibold tabular-nums ${balanceTone}`}>
                    {formatCents(account.balance_cents)}
                  </strong>
                  <span className="mt-1 block text-xs leading-5 text-zinc-500">
                    {account.trial_seconds_remaining > 0
                      ? `${Math.round(account.trial_seconds_remaining / 60)} trial minutes left before credits are used.`
                      : "Paid cloud work pauses at $0 until you add credits."}
                  </span>
                </div>
                <div className="flex flex-wrap gap-2">
                  <button
                    type="button"
                    onClick={() => window.open("https://bluey.sh/account", "_blank")}
                    className="inline-flex min-h-9 items-center gap-2 rounded-md bg-zinc-800 px-3 text-sm font-semibold text-zinc-100 hover:bg-zinc-700"
                  >
                    Add credits
                    <ExternalLink className="h-3.5 w-3.5" />
                  </button>
                  <button
                    type="button"
                    onClick={() => navigate("/settings")}
                    className="inline-flex min-h-9 items-center gap-2 rounded-md border border-zinc-700 px-3 text-sm font-semibold text-zinc-300 hover:border-zinc-500 hover:bg-zinc-800"
                  >
                    Settings
                    <Settings className="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
            ) : (
              <div className="space-y-3">
                <p className="text-sm leading-6 text-zinc-400">
                  Sign in to use cloud answers and credits. Saved-session sync stays off until you enable it in Settings.
                </p>
                <button
                  type="button"
                  onClick={signIn}
                  className="inline-flex min-h-9 items-center gap-2 rounded-md bg-blue-500 px-3 text-sm font-semibold text-white hover:bg-blue-400"
                >
                  Sign in
                  <LogIn className="h-3.5 w-3.5" />
                </button>
              </div>
            )}
          </div>
        </aside>
      </section>
    </div>
  );
}

function ActionCard({
  icon,
  title,
  body,
  cta,
  onClick,
}: {
  icon: ReactNode;
  title: string;
  body: string;
  cta: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="group grid min-h-44 content-between rounded-lg border border-zinc-800 bg-zinc-900 p-4 text-left hover:border-cyan-400/40 hover:bg-zinc-900/80"
    >
      <span>
        <span className="mb-3 inline-grid h-10 w-10 place-items-center rounded-md border border-cyan-400/25 bg-cyan-400/10 text-cyan-200">
          {icon}
        </span>
        <span className="block text-base font-semibold text-zinc-100">{title}</span>
        <span className="mt-2 block text-sm leading-6 text-zinc-400">{body}</span>
      </span>
      <span className="mt-4 inline-flex items-center gap-2 text-sm font-semibold text-cyan-200 group-hover:text-cyan-100">
        {cta}
        <ArrowRight className="h-4 w-4" />
      </span>
    </button>
  );
}

function formatTime(ms: number): string {
  if (!ms) return "not saved yet";
  const date = new Date(ms);
  if (Number.isNaN(date.getTime())) return "unknown time";
  return date.toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function formatCents(cents: number): string {
  const sign = cents < 0 ? "-" : "";
  const abs = Math.abs(cents);
  return `${sign}$${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, "0")}`;
}
