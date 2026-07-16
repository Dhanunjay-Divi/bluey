import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent, ReactNode } from "react";
import { useNavigate } from "react-router-dom";
import {
  Activity,
  ArrowRight,
  Cloud,
  Database,
  FileSearch,
  FolderKanban,
  History,
  Pause,
  Play,
  Radio,
  ScanText,
  Search,
  ShieldCheck,
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

interface DataControls {
  cloud_sync_enabled: boolean;
  raw_audio_retained: boolean;
  training_enabled: boolean;
}

interface ContextModeStatus {
  active: boolean;
  interval_secs: number | null;
  context_items: number;
}

interface ContextWatchSettings {
  interval_secs: number;
}

interface ContextItemSummary {
  id: string;
  title: string;
  kind: string;
  processing_status: string;
  created_at: string;
  context_mode_observation: boolean;
}

export function Context() {
  const navigate = useNavigate();
  const [sessions, setSessions] = useState<Session[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [controls, setControls] = useState<DataControls | null>(null);
  const [modeStatus, setModeStatus] = useState<ContextModeStatus | null>(null);
  const [contextInterval, setContextInterval] = useState(12);
  const [contextItems, setContextItems] = useState<ContextItemSummary[]>([]);
  const [modeBusy, setModeBusy] = useState<"start" | "stop" | "capture" | null>(null);
  const [captureCountdown, setCaptureCountdown] = useState<number | null>(null);
  const [modeMessage, setModeMessage] = useState("");
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const captureArmGeneration = useRef(0);

  useEffect(() => {
    Promise.all([
      invoke<Session[]>("list_sessions"),
      invoke<string | null>("get_active_session"),
      invoke<DataControls>("get_data_controls"),
      invoke<ContextModeStatus>("daemon_context_status"),
      invoke<ContextWatchSettings>("get_context_watch_settings"),
      invoke<ContextItemSummary[]>("daemon_context_items"),
    ])
      .then(([
        nextSessions,
        nextActiveId,
        nextControls,
        nextModeStatus,
        nextContextWatch,
        nextContextItems,
      ]) => {
        setSessions(nextSessions);
        setActiveId(nextActiveId);
        setControls(nextControls);
        setModeStatus(nextModeStatus);
        setContextInterval(nextModeStatus.interval_secs ?? nextContextWatch.interval_secs);
        setContextItems(nextContextItems);
      })
      .catch((nextError) => setError(String(nextError)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    if (!modeStatus?.active) return;
    const refresh = () => {
      Promise.all([
        invoke<ContextModeStatus>("daemon_context_status"),
        invoke<ContextItemSummary[]>("daemon_context_items"),
      ]).then(([nextStatus, nextItems]) => {
        setModeStatus(nextStatus);
        setContextItems(nextItems);
      }).catch(() => {
        // The visible status remains usable during a transient daemon restart.
      });
    };
    const timer = window.setInterval(
      refresh,
      Math.max(3, modeStatus.interval_secs ?? 12) * 1000,
    );
    return () => window.clearInterval(timer);
  }, [modeStatus?.active, modeStatus?.interval_secs]);

  const recent = useMemo(
    () =>
      [...sessions]
        .sort((a, b) => (b.updated_at || b.created_at) - (a.updated_at || a.created_at))
        .slice(0, 5),
    [sessions],
  );
  const active = sessions.find((session) => session.id === activeId) ?? null;
  const observations = useMemo(
    () =>
      contextItems
        .filter((item) => item.context_mode_observation)
        .sort((a, b) => Number(b.created_at) - Number(a.created_at))
        .slice(0, 4),
    [contextItems],
  );

  function searchMemory(event: FormEvent) {
    event.preventDefault();
    const trimmed = query.trim();
    navigate(trimmed ? `/search?q=${encodeURIComponent(trimmed)}` : "/search");
  }

  const refreshContextItems = useCallback(async () => {
    const [nextStatus, nextItems] = await Promise.all([
      invoke<ContextModeStatus>("daemon_context_status"),
      invoke<ContextItemSummary[]>("daemon_context_items"),
    ]);
    setModeStatus(nextStatus);
    setContextItems(nextItems);
  }, []);

  async function startContextMode() {
    if (modeBusy || captureCountdown !== null) return;
    setModeBusy("start");
    setError("");
    setModeMessage("");
    try {
      const next = await invoke<ContextModeStatus>("daemon_context_start", {
        intervalSecs: modeStatus?.interval_secs ?? contextInterval,
      });
      setModeStatus(next);
      setModeMessage(
        `Learning is on. Bluey checks the active supported browser for changed readable text every ${next.interval_secs ?? contextInterval} seconds.`,
      );
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setModeBusy(null);
    }
  }

  async function stopContextMode() {
    if (modeBusy || captureCountdown !== null) return;
    setModeBusy("stop");
    setError("");
    setModeMessage("");
    try {
      const next = await invoke<ContextModeStatus>("daemon_context_stop");
      setModeStatus(next);
      setModeMessage("Learning is paused. Bluey will not add new background observations.");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setModeBusy(null);
    }
  }

  const performActivePageCapture = useCallback(async (generation: number) => {
    if (generation !== captureArmGeneration.current) return;
    setCaptureCountdown(null);
    setModeBusy("capture");
    setError("");
    setModeMessage("Reading the active supported browser page now…");
    try {
      await invoke<ContextModeStatus>("daemon_capture_active_page");
      await refreshContextItems();
      setModeMessage("The active supported browser page was added to this session.");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setModeBusy(null);
    }
  }, [refreshContextItems]);

  useEffect(() => {
    if (captureCountdown === null) return;
    const generation = captureArmGeneration.current;
    const timeout = window.setTimeout(() => {
      if (generation !== captureArmGeneration.current) return;
      if (captureCountdown > 1) {
        setCaptureCountdown(captureCountdown - 1);
      } else {
        void performActivePageCapture(generation);
      }
    }, 1_000);
    return () => window.clearTimeout(timeout);
  }, [captureCountdown, performActivePageCapture]);

  function armActivePageCapture() {
    if (modeBusy || captureCountdown !== null) return;
    captureArmGeneration.current += 1;
    setError("");
    setModeMessage("");
    setCaptureCountdown(3);
  }

  function cancelActivePageCapture() {
    captureArmGeneration.current += 1;
    setCaptureCountdown(null);
    setModeMessage("Page capture cancelled. Nothing was added.");
  }

  const modeLocked = modeBusy !== null || captureCountdown !== null;

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <header className="space-y-2">
        <div className="inline-flex items-center gap-2 text-xs font-semibold uppercase tracking-[0.16em] text-cyan-300">
          <Database aria-hidden="true" size={15} />
          Context and memory
        </div>
        <h1 className="text-3xl font-semibold tracking-tight text-zinc-50">
          Bluey learns the work you choose to share.
        </h1>
        <p className="max-w-3xl text-sm leading-6 text-zinc-400">
          Turn on Context mode when you want Bluey to follow changed readable browser-page text,
          remember it inside the current session, and use it in later answers. It stays separate
          from listening and optional cloud sync.
        </p>
      </header>

      {error ? (
        <div role="alert" className="rounded-lg border border-red-500/35 bg-red-950/35 px-4 py-3 text-sm text-red-100">
          Context could not be updated: {error}
        </div>
      ) : null}

      <section className="overflow-hidden rounded-xl border border-violet-400/25 bg-[radial-gradient(circle_at_top_left,rgba(139,92,246,0.18),transparent_46%),rgba(24,24,27,0.95)]">
        <div className="grid gap-5 p-5 lg:grid-cols-[minmax(0,1fr)_360px]">
          <div>
            <div className="flex flex-wrap items-center gap-3">
              <span
                className={`inline-flex h-11 w-11 items-center justify-center rounded-lg border ${
                  modeStatus?.active
                    ? "border-emerald-400/35 bg-emerald-400/10 text-emerald-200"
                    : "border-zinc-700 bg-zinc-950 text-zinc-400"
                }`}
              >
                <Sparkles aria-hidden="true" size={20} />
              </span>
              <div>
                <div className="flex items-center gap-2">
                  <span
                    aria-hidden="true"
                    className={`h-2 w-2 rounded-full ${
                      modeStatus?.active ? "animate-pulse bg-emerald-400" : "bg-zinc-600"
                    }`}
                  />
                  <p className="text-xs font-semibold uppercase tracking-[0.14em] text-zinc-400">
                    {modeStatus?.active ? "Learning about your work" : "Context mode paused"}
                  </p>
                </div>
                <h2 className="mt-1 text-xl font-semibold text-zinc-50">
                  {modeStatus?.active
                    ? `Watching for meaningful changes every ${modeStatus.interval_secs ?? 12}s`
                    : "Start only when you want contextual memory"}
                </h2>
              </div>
            </div>
            <p className="mt-4 max-w-2xl text-sm leading-6 text-zinc-400">
              Bluey reads supported browser text first, skips your excluded apps and domains,
              deduplicates unchanged pages, and keeps a bounded observation history. Screenshot
              fallback remains off unless you explicitly enable it in Data controls.
            </p>
            <div className="mt-5 flex flex-wrap gap-2">
              {modeStatus?.active ? (
                <button
                  type="button"
                  disabled={modeLocked}
                  onClick={() => void stopContextMode()}
                  className="inline-flex min-h-10 items-center gap-2 rounded-md bg-zinc-100 px-4 text-sm font-semibold text-zinc-950 hover:bg-white disabled:opacity-50"
                >
                  <Pause aria-hidden="true" size={15} />
                  {modeBusy === "stop" ? "Pausing..." : "Pause learning"}
                </button>
              ) : (
                <button
                  type="button"
                  disabled={modeLocked || loading}
                  onClick={() => void startContextMode()}
                  className="inline-flex min-h-10 items-center gap-2 rounded-md bg-violet-300 px-4 text-sm font-semibold text-zinc-950 hover:bg-violet-200 disabled:opacity-50"
                >
                  <Play aria-hidden="true" size={15} />
                  {modeBusy === "start" ? "Starting..." : "Start learning"}
                </button>
              )}
              <button
                type="button"
                disabled={modeLocked || loading}
                onClick={armActivePageCapture}
                className="inline-flex min-h-10 items-center gap-2 rounded-md border border-zinc-700 bg-zinc-950 px-4 text-sm font-semibold text-zinc-200 hover:border-violet-400/60 hover:text-white disabled:opacity-50"
              >
                <ScanText aria-hidden="true" size={15} />
                {modeBusy === "capture"
                  ? "Reading page…"
                  : captureCountdown !== null
                    ? `Capture in ${captureCountdown}s`
                    : "Capture this page once"}
              </button>
              <button
                type="button"
                onClick={() => navigate("/settings")}
                className="inline-flex min-h-10 items-center gap-2 rounded-md px-3 text-sm font-semibold text-violet-200 hover:text-violet-100"
              >
                Privacy controls
                <ArrowRight aria-hidden="true" size={14} />
              </button>
            </div>
            {captureCountdown !== null ? (
              <div
                role="status"
                className="mt-3 flex flex-wrap items-center justify-between gap-3 rounded-lg border border-violet-400/30 bg-violet-400/10 px-3 py-2.5 text-xs leading-5 text-violet-100"
              >
                <span>
                  Switch now to Chrome, Edge, Brave, Arc, Chromium, or Safari and leave the
                  target tab frontmost. Bluey reads it when the countdown finishes.
                </span>
                <button
                  type="button"
                  onClick={cancelActivePageCapture}
                  className="min-h-8 rounded-md border border-violet-300/35 px-3 font-semibold hover:bg-violet-300/10"
                >
                  Cancel capture
                </button>
              </div>
            ) : null}
            {modeMessage ? (
              <p role="status" className="mt-3 text-xs leading-5 text-emerald-300">
                {modeMessage}
              </p>
            ) : null}
          </div>

          <div className="rounded-lg border border-zinc-800 bg-zinc-950/80 p-4">
            <div className="flex items-center justify-between gap-3">
              <div>
                <p className="text-xs font-semibold uppercase tracking-[0.12em] text-zinc-500">
                  Recent observations
                </p>
                <p className="mt-1 text-sm text-zinc-300">
                  {modeStatus?.context_items ?? contextItems.length} context item
                  {(modeStatus?.context_items ?? contextItems.length) === 1 ? "" : "s"} in this session
                </p>
              </div>
              <Activity aria-hidden="true" className="text-violet-300" size={19} />
            </div>
            {observations.length ? (
              <ul className="mt-3 space-y-2">
                {observations.map((item) => (
                  <li key={item.id} className="rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2.5">
                    <p className="truncate text-sm font-medium text-zinc-200">{item.title}</p>
                    <p className="mt-1 text-[11px] text-zinc-500">
                      {titleCase(item.kind)} · {titleCase(item.processing_status)} ·{" "}
                      {formatEpoch(item.created_at)}
                    </p>
                  </li>
                ))}
              </ul>
            ) : (
              <div className="mt-3 rounded-md border border-dashed border-zinc-800 px-3 py-5 text-center">
                <ScanText aria-hidden="true" className="mx-auto text-zinc-600" size={22} />
                <p className="mt-2 text-xs font-medium text-zinc-300">No page observations yet</p>
                <p className="mt-1 text-[11px] leading-4 text-zinc-600">
                  Open a supported browser page, then capture it once or start Context mode.
                </p>
              </div>
            )}
          </div>
        </div>
      </section>

      <section className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
        <ScopeCard
          icon={<Radio size={19} />}
          title="Live now"
          value={active ? active.title : "No active session"}
          body="Microphone and system audio appear only after you start listening."
          tone={active ? "live" : "neutral"}
          action="Open live view"
          onClick={() => navigate("/live")}
        />
        <ScopeCard
          icon={<History size={19} />}
          title="Session memory"
          value={loading ? "Checking..." : `${sessions.length} saved locally`}
          body="Raw transcript and answer history stays tied to its session."
          action="Browse sessions"
          onClick={() => navigate("/chats")}
        />
        <ScopeCard
          icon={<Cloud size={19} />}
          title="Cloud memory"
          value={controls?.cloud_sync_enabled ? "Sync enabled" : "Sync off"}
          body="Signed-in restore is a separate, reversible Settings choice."
          tone={controls?.cloud_sync_enabled ? "cloud" : "neutral"}
          action="Review data controls"
          onClick={() => navigate("/settings")}
        />
        <ScopeCard
          icon={<FolderKanban size={19} />}
          title="Projects"
          value="Nothing connected automatically"
          body="Files, repositories, tickets, and Jobs tracks require an explicit workflow."
          action="Open Bluey Jobs"
          onClick={() => window.open("https://bluey.sh/jobs", "_blank")}
        />
      </section>

      <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_340px]">
        <div className="rounded-xl border border-zinc-800 bg-zinc-900 p-5">
          <div className="flex flex-wrap items-end justify-between gap-3">
            <div>
              <h2 className="text-lg font-semibold text-zinc-100">Search remembered work</h2>
              <p className="mt-1 text-sm text-zinc-500">
                Search transcript text across sessions owned by the current desktop account.
              </p>
            </div>
            <ShieldCheck className="text-emerald-300" aria-hidden="true" size={21} />
          </div>
          <form onSubmit={searchMemory} className="mt-4 flex gap-2">
            <label className="relative min-w-0 flex-1">
              <span className="sr-only">Search session memory</span>
              <Search
                aria-hidden="true"
                className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500"
                size={16}
              />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                className="min-h-11 w-full rounded-md border border-zinc-700 bg-zinc-950 pl-10 pr-3 text-sm text-zinc-100 outline-none placeholder:text-zinc-600 focus:border-cyan-400"
                placeholder="Decision, technology, customer, action item..."
              />
            </label>
            <button
              type="submit"
              className="inline-flex min-h-11 items-center gap-2 rounded-md bg-cyan-400 px-4 text-sm font-semibold text-zinc-950 hover:bg-cyan-300"
            >
              Search
              <ArrowRight aria-hidden="true" size={15} />
            </button>
          </form>

          <div className="mt-5">
            <div className="mb-2 flex items-center justify-between">
              <h3 className="text-xs font-semibold uppercase tracking-[0.14em] text-zinc-500">
                Recent session memory
              </h3>
              <button
                type="button"
                onClick={() => navigate("/chats")}
                className="text-xs font-semibold text-cyan-200 hover:text-cyan-100"
              >
                View all
              </button>
            </div>
            {loading ? (
              <p className="rounded-md border border-zinc-800 bg-zinc-950 px-4 py-5 text-sm text-zinc-500">
                Loading saved memory...
              </p>
            ) : recent.length ? (
              <ul className="divide-y divide-zinc-800 overflow-hidden rounded-md border border-zinc-800 bg-zinc-950">
                {recent.map((session) => (
                  <li key={session.id}>
                    <button
                      type="button"
                      onClick={() => navigate(`/session/${session.id}`)}
                      className="flex min-h-16 w-full items-center justify-between gap-3 px-4 py-3 text-left hover:bg-zinc-900"
                    >
                      <span className="min-w-0">
                        <span className="flex items-center gap-2">
                          <span className="truncate text-sm font-medium text-zinc-100">{session.title}</span>
                          {session.id === activeId ? (
                            <span className="rounded-full bg-emerald-400/10 px-2 py-0.5 text-[10px] font-semibold text-emerald-200">
                              live
                            </span>
                          ) : null}
                        </span>
                        <span className="mt-1 block text-xs text-zinc-500">
                          Raw session history · {formatTime(session.updated_at || session.created_at)}
                        </span>
                      </span>
                      <ArrowRight aria-hidden="true" className="shrink-0 text-zinc-600" size={15} />
                    </button>
                  </li>
                ))}
              </ul>
            ) : (
              <div className="rounded-md border border-zinc-800 bg-zinc-950 px-4 py-6 text-center">
                <FileSearch aria-hidden="true" className="mx-auto text-zinc-600" size={26} />
                <p className="mt-2 text-sm font-medium text-zinc-200">No saved session memory yet</p>
                <p className="mt-1 text-xs text-zinc-500">Start a visible live session to build one.</p>
              </div>
            )}
          </div>
        </div>

        <aside className="rounded-xl border border-zinc-800 bg-zinc-900 p-5">
          <h2 className="text-lg font-semibold text-zinc-100">Memory boundaries</h2>
          <div className="mt-4 space-y-4 text-sm">
            <Boundary
              title="Raw audio"
              value={controls?.raw_audio_retained ? "Retained" : "Not retained"}
              body="Audio is processed for transcription; this release does not keep a raw-audio library by default."
            />
            <Boundary
              title="Model training"
              value={controls?.training_enabled ? "Enabled" : "Off"}
              body="Bluey account content is not used to train models under the current data controls."
            />
            <Boundary
              title="Derived memory"
              value="Shown when available"
              body="Summaries and retrieved context should remain distinguishable from raw session evidence."
            />
          </div>
        </aside>
      </section>
    </div>
  );
}

function ScopeCard({
  icon,
  title,
  value,
  body,
  action,
  onClick,
  tone = "neutral",
}: {
  icon: ReactNode;
  title: string;
  value: string;
  body: string;
  action: string;
  onClick: () => void;
  tone?: "neutral" | "live" | "cloud";
}) {
  const color =
    tone === "live"
      ? "border-emerald-400/25 bg-emerald-400/10 text-emerald-200"
      : tone === "cloud"
        ? "border-cyan-400/25 bg-cyan-400/10 text-cyan-200"
        : "border-zinc-700 bg-zinc-950 text-zinc-300";
  return (
    <article className="flex min-h-56 flex-col rounded-xl border border-zinc-800 bg-zinc-900 p-4">
      <span className={`inline-grid h-10 w-10 place-items-center rounded-md border ${color}`}>{icon}</span>
      <h2 className="mt-4 text-sm font-semibold text-zinc-100">{title}</h2>
      <strong className="mt-1 text-base text-zinc-50">{value}</strong>
      <p className="mt-2 flex-1 text-xs leading-5 text-zinc-500">{body}</p>
      <button
        type="button"
        onClick={onClick}
        className="mt-4 inline-flex items-center gap-1.5 self-start text-xs font-semibold text-cyan-200 hover:text-cyan-100"
      >
        {action}
        <ArrowRight aria-hidden="true" size={13} />
      </button>
    </article>
  );
}

function Boundary({ title, value, body }: { title: string; value: string; body: string }) {
  return (
    <div className="border-b border-zinc-800 pb-4 last:border-b-0 last:pb-0">
      <div className="flex items-center justify-between gap-3">
        <h3 className="font-medium text-zinc-200">{title}</h3>
        <span className="shrink-0 text-xs font-semibold text-emerald-300">{value}</span>
      </div>
      <p className="mt-1 text-xs leading-5 text-zinc-500">{body}</p>
    </div>
  );
}

function formatTime(ms: number): string {
  const date = new Date(ms);
  if (Number.isNaN(date.getTime())) return "unknown time";
  return date.toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function formatEpoch(value: string): string {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? formatTime(parsed) : "unknown time";
}

function titleCase(value: string): string {
  return value
    .replace(/[_-]+/g, " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}
