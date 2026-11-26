import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { useLocation } from "react-router-dom";
import { invoke } from "../lib/tauri";
import { cloudSyncConsentCopy } from "../lib/cloudSyncConsent";
import {
  AudioLines,
  Check,
  Cloud,
  ExternalLink,
  Eye,
  EyeOff,
  FileAudio,
  GraduationCap,
  Keyboard,
  LogOut,
  Mail,
  Mic2,
  MousePointer2,
  RefreshCw,
  RotateCcw,
  Save,
  Trash2,
  Volume2,
} from "lucide-react";

/**
 * Codex Stage 24: Settings page rewrite.
 *
 * Removed all BYOK Deepgram-key prompts (pre-Stage-18 era). New layout:
 *   - Account card (email, balance, "Open billing portal", "Sign out", "Delete account")
 *   - Data controls (real cloud-sync preference + current retention/training states)
 *   - Screen-share privacy and explicit overlay visibility
 *
 * Design tokens: bg-zinc-950 surface, bg-zinc-900 cards, blue-500 brand,
 * red-500 destructive. Same palette as Onboarding.
 */

const platform = typeof navigator === "undefined" ? "" : navigator.userAgent;
const isWindows = platform.includes("Windows");

interface AccountMe {
  id: string;
  email: string;
  balance_cents: number;
  trial_seconds_remaining: number;
  auto_topup_enabled: boolean;
  auto_topup_threshold_cents: number;
  auto_topup_amount_cents: number;
}

interface ClearIgnoredMeetingAppsResult {
  apps: string[];
  detection_refreshed: boolean;
}

interface MeetingDetectionSettings {
  enabled: boolean;
  detection_refreshed: boolean;
}

export function Settings() {
  const location = useLocation();

  useEffect(() => {
    if (new URLSearchParams(location.search).get("section") !== "audio-meetings") {
      return;
    }
    const frame = window.requestAnimationFrame(() => {
      const target = document.getElementById("audio-meetings");
      target?.scrollIntoView({ block: "start" });
      target?.focus({ preventScroll: true });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [location.search]);

  return (
    <div className="text-zinc-100">
      <div className="mx-auto max-w-3xl space-y-6">
        <header>
          <h1 className="text-3xl font-semibold tracking-tight">Settings</h1>
          <p className="mt-2 max-w-2xl text-sm leading-6 text-zinc-400">
            Control what Bluey can hear, remember, sync, and show. Every capture mode remains
            separate and visible.
          </p>
        </header>
        <AccountCard />
        <AudioMeetingsCard />
        <OverlayInteractionCard />
        <DataControlsCard />
        <ShortcutsCard />
        <ScreenSharePrivacyCard />
      </div>
    </div>
  );
}

function AccountCard() {
  const [me, setMe] = useState<AccountMe | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleteText, setDeleteText] = useState("");
  const [acceptDataLoss, setAcceptDataLoss] = useState(false);
  const [acceptCreditLoss, setAcceptCreditLoss] = useState(false);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [accountError, setAccountError] = useState("");
  const deleteBusyRef = useRef(false);
  const deleteTriggerRef = useRef<HTMLButtonElement>(null);
  const deleteDialogRef = useRef<HTMLDivElement>(null);
  const deleteCancelRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    let cancelled = false;
    const loadAccount = (markLoaded: boolean) => {
      invoke<AccountMe | null>("account_me")
        .then((next) => {
          if (!cancelled) setMe(next);
        })
        .catch((nextError) => {
          if (!cancelled && markLoaded) {
            setAccountError(`Bluey could not load your account: ${String(nextError)}`);
          }
        })
        .finally(() => {
          if (!cancelled && markLoaded) setLoaded(true);
        });
    };
    loadAccount(true);
    const id = window.setInterval(() => loadAccount(false), 10_000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, []);

  useEffect(() => {
    deleteBusyRef.current = deleteBusy;
  }, [deleteBusy]);

  useEffect(() => {
    if (!deleteOpen) return;
    const frame = window.requestAnimationFrame(() => deleteCancelRef.current?.focus());
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !deleteBusyRef.current) {
        event.preventDefault();
        setDeleteOpen(false);
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = Array.from(
        deleteDialogRef.current?.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
        ) ?? [],
      ).filter((element) => !element.hasAttribute("hidden"));
      if (focusable.length === 0) {
        event.preventDefault();
        deleteDialogRef.current?.focus();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      window.cancelAnimationFrame(frame);
      document.removeEventListener("keydown", onKeyDown);
      deleteTriggerRef.current?.focus();
    };
  }, [deleteOpen]);

  async function signIn() {
    setAccountError("");
    try {
      const url = await invoke<string>("get_signin_url");
      window.open(url, "_blank");
    } catch (e) {
      setAccountError(`Bluey could not open sign-in: ${String(e)}`);
    }
  }

  async function openPortal() {
    setAccountError("");
    try {
      const url = await invoke<string>("billing_portal_url");
      window.open(url, "_blank");
    } catch (e) {
      setAccountError(`Bluey could not open credit management: ${String(e)}`);
    }
  }
  async function signOut() {
    setAccountError("");
    try {
      await invoke("sign_out");
      window.location.href = "/";
    } catch (e) {
      setAccountError(`Bluey could not sign out: ${String(e)}`);
    }
  }
  async function deleteAccount() {
    if (!acceptDataLoss || !acceptCreditLoss || deleteText !== "DELETE") return;
    setAccountError("");
    setDeleteBusy(true);
    try {
      const outcome = await invoke<{ deleted: boolean; state: string; message: string }>(
        "delete_account_now",
      );
      if (outcome.deleted) {
        window.location.href = "/";
      } else {
        setAccountError(outcome.message);
      }
    } catch (e) {
      setAccountError(`Your account was not deleted: ${String(e)}`);
    } finally {
      setDeleteBusy(false);
    }
  }

  const balanceClass = me
    ? me.balance_cents < 500
      ? "text-red-300"
      : me.balance_cents < 1_000
        ? "text-amber-300"
        : "text-zinc-200"
    : "text-zinc-200";

  return (
    <div className="rounded-xl bg-zinc-900 border border-zinc-800 p-5 space-y-4">
      <h3 className="font-semibold text-zinc-100">Account and credits</h3>
      {accountError ? (
        <p
          role="alert"
          className="rounded-lg border border-red-500/35 bg-red-950/35 px-3 py-2 text-sm leading-5 text-red-100"
        >
          {accountError}
        </p>
      ) : null}
      {me ? (
        <div className="space-y-3 text-sm">
          <div className="flex items-center gap-2 text-zinc-300">
            <Mail className="h-4 w-4 text-zinc-500" /> {me.email}
          </div>
          <div className="grid gap-2 rounded-lg border border-zinc-800 bg-zinc-950 p-3">
            <div>
              <span className="block text-[11px] font-semibold uppercase tracking-wide text-zinc-500">
                Credits balance
              </span>
              <span className={`${balanceClass} mt-1 block text-2xl font-semibold tabular-nums`}>
                ${(me.balance_cents / 100).toFixed(2)}
              </span>
            </div>
            <p className="text-xs leading-5 text-zinc-500">
              {me.trial_seconds_remaining > 0
                ? `${Math.round(me.trial_seconds_remaining / 60)} trial minutes left. Credits are used after the trial when paid cloud work is needed.`
                : "Paid cloud work pauses at $0 until you add credits."}
            </p>
            <p className="text-xs leading-5 text-zinc-500">
              {me.auto_topup_enabled
                ? `Auto Reload is on: adds $${(me.auto_topup_amount_cents / 100).toFixed(2)} below $${(me.auto_topup_threshold_cents / 100).toFixed(2)}.`
                : "Auto Reload is off. Add credits manually from the web account page."}
            </p>
          </div>
        </div>
      ) : loaded ? (
        <div className="space-y-2">
          <p className="text-sm text-zinc-400">
            {cloudSyncConsentCopy.signedOutAccount}
          </p>
          <button
            onClick={signIn}
            className="inline-flex items-center gap-1 text-sm px-3 py-1.5 rounded-md bg-blue-500 hover:bg-blue-400 text-white"
          >
            Sign in or create account <ExternalLink className="h-3 w-3" />
          </button>
        </div>
      ) : (
        <p className="text-sm text-zinc-500">Checking account…</p>
      )}
      {me && <div className="flex flex-wrap gap-2">
        <button
          onClick={openPortal}
          className="inline-flex items-center gap-1 text-sm px-3 py-1.5 rounded-md bg-zinc-800 hover:bg-zinc-700 text-zinc-100"
        >
          Manage credits <ExternalLink className="h-3 w-3" />
        </button>
        <button
          onClick={signOut}
          className="inline-flex items-center gap-1 text-sm px-3 py-1.5 rounded-md bg-zinc-800 hover:bg-zinc-700 text-zinc-100"
        >
          Sign out <LogOut className="h-3 w-3" />
        </button>
        <button
          ref={deleteTriggerRef}
          onClick={() => {
            setAccountError("");
            setDeleteOpen(true);
          }}
          className="inline-flex items-center gap-1 text-sm px-3 py-1.5 rounded-md bg-red-500/10 border border-red-500/30 hover:bg-red-500/20 text-red-300"
        >
          Delete account <Trash2 className="h-3 w-3" />
        </button>
      </div>}
      {deleteOpen && me && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4">
          <div
            ref={deleteDialogRef}
            role="dialog"
            aria-modal="true"
            aria-labelledby="delete-account-title"
            aria-describedby="delete-account-description"
            tabIndex={-1}
            className="w-full max-w-lg rounded-xl border border-red-500/30 bg-zinc-950 p-5 shadow-2xl"
          >
            <div className="flex items-start gap-3">
              <Trash2 className="mt-1 h-5 w-5 text-red-300" />
              <div>
                <h3 id="delete-account-title" className="text-lg font-semibold text-zinc-100">
                  Delete Bluey account?
                </h3>
                <p
                  id="delete-account-description"
                  className="mt-1 text-sm leading-relaxed text-zinc-400"
                >
                  This permanently deletes {me.email}, synced sessions, files, transcripts,
                  generated answers, usage history, and account records. Unused Bluey credits
                  are lost when the account is deleted.
                </p>
              </div>
            </div>
            <div className="mt-4 space-y-3 text-sm text-zinc-300">
              <label className="flex gap-2">
                <input
                  type="checkbox"
                  checked={acceptDataLoss}
                  onChange={(event) => setAcceptDataLoss(event.target.checked)}
                />
                <span>I understand this deletes account files, saved sessions, history, and data.</span>
              </label>
              <label className="flex gap-2">
                <input
                  type="checkbox"
                  checked={acceptCreditLoss}
                  onChange={(event) => setAcceptCreditLoss(event.target.checked)}
                />
                <span>I understand unused credits are lost after deletion.</span>
              </label>
              <label className="block">
                <span className="mb-1 block text-xs font-semibold uppercase tracking-wide text-zinc-500">
                  Type DELETE to confirm
                </span>
                <input
                  value={deleteText}
                  onChange={(event) => setDeleteText(event.target.value)}
                  className="w-full rounded-md border border-zinc-700 bg-zinc-900 px-3 py-2 text-zinc-100 outline-none focus:border-red-400"
                  placeholder="DELETE"
                />
              </label>
            </div>
            <div className="mt-5 flex flex-wrap justify-end gap-2">
              {accountError ? (
                <p role="alert" className="w-full text-sm leading-5 text-red-200">
                  {accountError}
                </p>
              ) : null}
              <button
                ref={deleteCancelRef}
                type="button"
                onClick={() => setDeleteOpen(false)}
                className="rounded-md bg-zinc-800 px-3 py-2 text-sm text-zinc-100 hover:bg-zinc-700"
              >
                Cancel
              </button>
              <button
                type="button"
                disabled={deleteBusy || !acceptDataLoss || !acceptCreditLoss || deleteText !== "DELETE"}
                onClick={deleteAccount}
                className="rounded-md border border-red-500/40 bg-red-500/15 px-3 py-2 text-sm text-red-200 hover:bg-red-500/25 disabled:cursor-not-allowed disabled:opacity-40"
              >
                {deleteBusy ? "Deleting..." : "Delete account"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

interface DataControls {
  cloud_sync_enabled: boolean;
  raw_audio_retained: boolean;
  training_enabled: boolean;
}

interface ContextWatchControls {
  semantic_first: boolean;
  screenshot_fallback: boolean;
  interval_secs: number;
  max_local_items: number;
  excluded_apps: string[];
  excluded_domains: string[];
}

function DataControlsCard() {
  const [controls, setControls] = useState<DataControls | null>(null);
  const [contextWatch, setContextWatch] = useState<ContextWatchControls | null>(null);
  const [excludedAppsDraft, setExcludedAppsDraft] = useState("");
  const [excludedDomainsDraft, setExcludedDomainsDraft] = useState("");
  const [busy, setBusy] = useState<"cloud" | "context" | null>(null);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");

  useEffect(() => {
    Promise.all([
      invoke<DataControls>("get_data_controls"),
      invoke<ContextWatchControls>("get_context_watch_settings"),
    ])
      .then(([nextControls, nextContextWatch]) => {
        setControls(nextControls);
        setContextWatch(nextContextWatch);
        setExcludedAppsDraft(nextContextWatch.excluded_apps.join("\n"));
        setExcludedDomainsDraft(nextContextWatch.excluded_domains.join("\n"));
      })
      .catch((e) => setError(String(e)));
  }, []);

  async function updateCloudSync(enabled: boolean) {
    if (!controls || busy) return;
    setBusy("cloud");
    setError("");
    setMessage("");
    try {
      const next = await invoke<DataControls>("set_cloud_sync_enabled", { enabled });
      setControls(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function saveContextWatch() {
    if (!contextWatch || busy) return;
    setBusy("context");
    setError("");
    setMessage("");
    const settings: ContextWatchControls = {
      ...contextWatch,
      interval_secs: Math.min(300, Math.max(3, Math.round(contextWatch.interval_secs))),
      max_local_items: Math.min(500, Math.max(10, Math.round(contextWatch.max_local_items))),
      excluded_apps: parseExclusionDraft(excludedAppsDraft),
      excluded_domains: parseExclusionDraft(excludedDomainsDraft),
    };
    try {
      const saved = await invoke<ContextWatchControls>("update_context_watch_settings", {
        settings,
      });
      setContextWatch(saved);
      setExcludedAppsDraft(saved.excluded_apps.join("\n"));
      setExcludedDomainsDraft(saved.excluded_domains.join("\n"));
      setMessage(
        "Context privacy policy saved. Interval changes apply the next time Context mode starts.",
      );
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="rounded-lg bg-zinc-900 border border-zinc-800 p-5 space-y-4">
      <div>
        <h3 className="font-semibold text-zinc-100">Data controls</h3>
        <p className="mt-1 text-xs leading-relaxed text-zinc-400">
          {cloudSyncConsentCopy.dataControls}
        </p>
      </div>
      <div className="divide-y divide-zinc-800 rounded-lg border border-zinc-800 bg-zinc-950">
        <label className="flex cursor-pointer items-start justify-between gap-4 p-4">
          <span className="flex min-w-0 gap-3">
            <Cloud className="mt-0.5 h-4 w-4 shrink-0 text-blue-300" />
            <span>
              <span className="block text-sm font-medium text-zinc-200">Cloud session sync</span>
              <span className="mt-1 block text-xs leading-5 text-zinc-500">
                {cloudSyncConsentCopy.toggle}
              </span>
            </span>
          </span>
          <input
            type="checkbox"
            className="mt-0.5 h-4 w-4 shrink-0 accent-blue-500"
            checked={controls?.cloud_sync_enabled ?? false}
            disabled={!controls || busy !== null}
            onChange={(event) => updateCloudSync(event.target.checked)}
          />
        </label>
        <div className="space-y-4 p-4">
          <div className="flex items-start gap-3">
            <Eye className="mt-0.5 h-4 w-4 shrink-0 text-violet-300" />
            <div>
              <span className="block text-sm font-medium text-zinc-200">
                Context mode privacy
              </span>
              <span className="mt-1 block text-xs leading-5 text-zinc-500">
                Context mode is always started separately from listening. It reads supported
                browser-page text first, skips configured sources before storage, and keeps a
                bounded number of its own observations.
              </span>
            </div>
          </div>

          {contextWatch ? (
            <div className="grid gap-4 pl-7 sm:grid-cols-2">
              <label className="flex items-start justify-between gap-4 rounded-md border border-zinc-800 bg-zinc-900/70 p-3 sm:col-span-2">
                <span>
                  <span className="block text-xs font-semibold text-zinc-200">
                    Screenshot fallback
                  </span>
                  <span className="mt-1 block text-[11px] leading-5 text-zinc-500">
                    If readable browser text is unavailable during explicitly started Context
                    mode, allow a screenshot of the visible display. Off is the safer default.
                  </span>
                </span>
                <input
                  type="checkbox"
                  checked={contextWatch.screenshot_fallback}
                  disabled={busy !== null}
                  onChange={(event) =>
                    setContextWatch((current) =>
                      current
                        ? { ...current, screenshot_fallback: event.target.checked }
                        : current,
                    )
                  }
                  className="mt-0.5 h-4 w-4 shrink-0 accent-violet-400"
                />
              </label>

              <label className="text-xs text-zinc-400">
                <span className="mb-1.5 block font-semibold text-zinc-300">
                  Default check interval
                </span>
                <select
                  value={contextWatch.interval_secs}
                  disabled={busy !== null}
                  onChange={(event) =>
                    setContextWatch((current) =>
                      current ? { ...current, interval_secs: Number(event.target.value) } : current,
                    )
                  }
                  className="min-h-10 w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 text-sm text-zinc-200"
                >
                  <option value={3}>Every 3 seconds</option>
                  <option value={12}>Every 12 seconds</option>
                  <option value={30}>Every 30 seconds</option>
                  <option value={60}>Every minute</option>
                  <option value={120}>Every 2 minutes</option>
                  <option value={300}>Every 5 minutes</option>
                </select>
              </label>

              <label className="text-xs text-zinc-400">
                <span className="mb-1.5 block font-semibold text-zinc-300">
                  Local observation limit
                </span>
                <input
                  type="number"
                  min={10}
                  max={500}
                  value={contextWatch.max_local_items}
                  disabled={busy !== null}
                  onChange={(event) =>
                    setContextWatch((current) =>
                      current
                        ? { ...current, max_local_items: Number(event.target.value) }
                        : current,
                    )
                  }
                  className="min-h-10 w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 text-sm text-zinc-200"
                />
                <span className="mt-1 block text-[11px] leading-4 text-zinc-600">
                  Old Context-mode observations are pruned beyond this bound. Other attached
                  session files are not counted.
                </span>
              </label>

              <label className="text-xs text-zinc-400">
                <span className="mb-1.5 block font-semibold text-zinc-300">
                  Excluded apps
                </span>
                <textarea
                  rows={4}
                  value={excludedAppsDraft}
                  disabled={busy !== null}
                  onChange={(event) => setExcludedAppsDraft(event.target.value)}
                  placeholder={"1Password\nSlack"}
                  className="w-full resize-y rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-200 outline-none placeholder:text-zinc-700 focus:border-violet-400"
                />
                <span className="mt-1 block text-[11px] leading-4 text-zinc-600">
                  One application name or bundle/process identity per line.
                </span>
              </label>

              <label className="text-xs text-zinc-400">
                <span className="mb-1.5 block font-semibold text-zinc-300">
                  Excluded domains
                </span>
                <textarea
                  rows={4}
                  value={excludedDomainsDraft}
                  disabled={busy !== null}
                  onChange={(event) => setExcludedDomainsDraft(event.target.value)}
                  placeholder={"accounts.example.com\nmail.example.com"}
                  className="w-full resize-y rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-200 outline-none placeholder:text-zinc-700 focus:border-violet-400"
                />
                <span className="mt-1 block text-[11px] leading-4 text-zinc-600">
                  One hostname per line. If a URL cannot be verified while this list is set,
                  readable-page capture is rejected.
                </span>
              </label>

              <div className="flex flex-wrap items-center justify-between gap-3 sm:col-span-2">
                <p className="text-[11px] leading-5 text-zinc-600">
                  Page-text-first is always on. These settings do not start Context mode or
                  listening.
                </p>
                <button
                  type="button"
                  disabled={busy !== null}
                  onClick={() => void saveContextWatch()}
                  className="inline-flex min-h-9 items-center gap-2 rounded-md bg-violet-400 px-3 text-xs font-semibold text-zinc-950 hover:bg-violet-300 disabled:opacity-50"
                >
                  <Save className="h-3.5 w-3.5" />
                  {busy === "context" ? "Saving..." : "Save context policy"}
                </button>
              </div>
            </div>
          ) : (
            <p className="pl-7 text-xs text-zinc-600">Loading Context-mode policy...</p>
          )}
        </div>
        <div className="flex items-start justify-between gap-4 p-4">
          <span className="flex min-w-0 gap-3">
            <FileAudio className="mt-0.5 h-4 w-4 shrink-0 text-emerald-300" />
            <span>
              <span className="block text-sm font-medium text-zinc-200">Raw audio retention</span>
              <span className="mt-1 block text-xs leading-5 text-zinc-500">
                Audio is processed for transcription. Bluey does not keep a raw-audio library in this release.
              </span>
            </span>
          </span>
          <strong className="shrink-0 text-xs font-semibold text-emerald-300">
            {controls?.raw_audio_retained ? "On" : "Off"}
          </strong>
        </div>
        <div className="flex items-start justify-between gap-4 p-4">
          <span className="flex min-w-0 gap-3">
            <GraduationCap className="mt-0.5 h-4 w-4 shrink-0 text-amber-300" />
            <span>
              <span className="block text-sm font-medium text-zinc-200">Model training</span>
              <span className="mt-1 block text-xs leading-5 text-zinc-500">
                Submitted prompts, transcripts, files, screenshots, audio, and answers are not used to train models.
              </span>
            </span>
          </span>
          <strong className="shrink-0 text-xs font-semibold text-emerald-300">
            {controls?.training_enabled ? "On" : "Off"}
          </strong>
        </div>
      </div>
      {message ? <p className="text-xs text-emerald-300" role="status">{message}</p> : null}
      {error && <p className="text-xs text-red-300" role="status">Could not update data controls: {error}</p>}
    </div>
  );
}

function parseExclusionDraft(value: string): string[] {
  return value
    .split(/[\n,]/)
    .map((entry) => entry.trim())
    .filter(Boolean);
}

function AudioMeetingsCard() {
  const [devices, setDevices] = useState<string[]>([]);
  const [selected, setSelected] = useState("");
  const [meetingDetectionEnabled, setMeetingDetectionEnabled] = useState(true);
  const [ignoredApps, setIgnoredApps] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [detectionBusy, setDetectionBusy] = useState(false);
  const [ignoredBusy, setIgnoredBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  async function load() {
    setLoading(true);
    setError("");
    try {
      const [nextDevices, settings, detection, nextIgnoredApps] = await Promise.all([
        invoke<string[]>("list_audio_devices"),
        invoke<Record<string, string>>("load_settings"),
        invoke<MeetingDetectionSettings>("get_meeting_detection_settings"),
        invoke<string[]>("get_meeting_detection_ignored_apps"),
      ]);
      setDevices(nextDevices);
      setSelected(settings["audio.mic_device"] ?? "");
      setMeetingDetectionEnabled(detection.enabled);
      setIgnoredApps(nextIgnoredApps);
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setLoading(false);
    }
  }

  async function setMeetingDetection(enabled: boolean) {
    if (detectionBusy) return;
    const previous = meetingDetectionEnabled;
    setMeetingDetectionEnabled(enabled);
    setDetectionBusy(true);
    setMessage("");
    setError("");
    try {
      const result = await invoke<MeetingDetectionSettings>(
        "set_meeting_detection_enabled",
        { enabled },
      );
      setMeetingDetectionEnabled(result.enabled);
      setMessage(
        result.detection_refreshed
          ? result.enabled
            ? "Meeting suggestions enabled. Native detection is active."
            : "Meeting suggestions disabled. Native detection and pending notifications stopped."
          : "Preference saved. Bluey will apply it when the local service reconnects.",
      );
    } catch (nextError) {
      setMeetingDetectionEnabled(previous);
      setError(String(nextError));
    } finally {
      setDetectionBusy(false);
    }
  }

  useEffect(() => {
    void load();
  }, []);

  async function save() {
    if (busy) return;
    setBusy(true);
    setMessage("");
    setError("");
    try {
      await invoke("save_settings", {
        settings: { "audio.mic_device": selected },
      });
      setMessage("Microphone preference saved. New listening sessions will use it.");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setBusy(false);
    }
  }

  function openPrivacySettings(source: "microphone" | "system") {
    invoke("open_privacy_settings", { source }).catch((nextError) => {
      setError(String(nextError));
    });
  }

  async function clearIgnoredApps() {
    if (ignoredBusy || ignoredApps.length === 0) return;
    setIgnoredBusy(true);
    setMessage("");
    setError("");
    try {
      const result = await invoke<ClearIgnoredMeetingAppsResult>(
        "clear_meeting_detection_ignored_apps",
      );
      setIgnoredApps(result.apps);
      setMessage(
        result.detection_refreshed
          ? "Ignored meeting apps cleared. Meeting detection refreshed."
          : "Ignored meeting apps cleared. The detector will use the new list after it reconnects.",
      );
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setIgnoredBusy(false);
    }
  }

  return (
    <section
      id="audio-meetings"
      tabIndex={-1}
      className="scroll-mt-4 space-y-4 rounded-xl border border-zinc-800 bg-zinc-900 p-5 outline-none focus-visible:ring-2 focus-visible:ring-cyan-400"
      aria-labelledby="audio-settings-title"
    >
      <div className="flex items-start gap-3">
        <span className="grid h-9 w-9 shrink-0 place-items-center rounded-md border border-cyan-400/25 bg-cyan-400/10 text-cyan-200">
          <AudioLines aria-hidden="true" size={18} />
        </span>
        <div>
          <h2 id="audio-settings-title" className="font-semibold text-zinc-100">
            Audio and meetings
          </h2>
          <p className="mt-1 text-xs leading-5 text-zinc-400">
            Bluey listens to microphone and system audio only after you start a live session.
            Listening always follows the control or shortcut you choose.
          </p>
        </div>
      </div>

      <div className="grid gap-3 rounded-lg border border-zinc-800 bg-zinc-950 p-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
        <label className="grid gap-1.5 text-sm text-zinc-300">
          <span className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">
            <Mic2 aria-hidden="true" size={14} />
            Preferred microphone
          </span>
          <select
            value={selected}
            disabled={loading || busy}
            onChange={(event) => setSelected(event.target.value)}
            className="min-h-10 w-full rounded-md border border-zinc-700 bg-zinc-900 px-3 text-sm text-zinc-100 outline-none focus:border-cyan-400"
          >
            <option value="">System default</option>
            {devices.map((device) => (
              <option key={device} value={device}>
                {device}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          disabled={loading || busy}
          onClick={() => void save()}
          className="inline-flex min-h-10 items-center justify-center gap-2 rounded-md bg-cyan-400 px-4 text-sm font-semibold text-zinc-950 hover:bg-cyan-300 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Save aria-hidden="true" size={15} />
          {busy ? "Saving..." : "Save"}
        </button>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <PermissionShortcut
          icon={<Mic2 size={16} />}
          title="Microphone access"
          body="Required for your side of a conversation."
          onClick={() => openPrivacySettings("microphone")}
        />
        <PermissionShortcut
          icon={<Volume2 size={16} />}
          title="System audio access"
          body="Required for the other side of a call."
          onClick={() => openPrivacySettings("system")}
        />
      </div>

      <div className="rounded-lg border border-zinc-800 bg-zinc-950 p-4">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="max-w-xl">
            <h3 className="text-sm font-semibold text-zinc-200">Meeting suggestions</h3>
            <p id="meeting-detection-description" className="mt-1 text-xs leading-5 text-zinc-500">
              When enabled, Bluey checks foreground app identity, call-related window metadata,
              and whether microphone or system audio is active to suggest starting a session.
              It does not record audio or capture page content during detection.
            </p>
          </div>
          <label className="inline-flex min-h-10 cursor-pointer items-center gap-3 rounded-md border border-zinc-700 px-3 text-xs font-semibold text-zinc-200">
            <input
              type="checkbox"
              checked={meetingDetectionEnabled}
              disabled={loading || detectionBusy}
              aria-describedby="meeting-detection-description"
              onChange={(event) => void setMeetingDetection(event.target.checked)}
              className="h-4 w-4 accent-cyan-400"
            />
            {detectionBusy
              ? "Updating..."
              : meetingDetectionEnabled
                ? "Suggestions on"
                : "Suggestions off"}
          </label>
        </div>
      </div>

      <div className="rounded-lg border border-zinc-800 bg-zinc-950 p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h3 className="text-sm font-semibold text-zinc-200">
              Ignored meeting apps
              <span className="ml-2 rounded-full border border-zinc-700 px-2 py-0.5 text-[11px] text-zinc-400">
                {ignoredApps.length}
              </span>
            </h3>
            <p className="mt-1 max-w-xl text-xs leading-5 text-zinc-500">
              Choosing Ignore on a meeting notification stores only that app&apos;s process or
              bundle identity. It does not store window titles, pages, transcripts, or audio.
            </p>
          </div>
          <button
            type="button"
            disabled={loading || ignoredBusy || ignoredApps.length === 0}
            onClick={() => void clearIgnoredApps()}
            className="inline-flex min-h-9 items-center justify-center gap-2 rounded-md border border-zinc-700 px-3 text-xs font-semibold text-zinc-200 hover:border-zinc-500 hover:bg-zinc-900 disabled:cursor-not-allowed disabled:opacity-50"
          >
            <RotateCcw
              aria-hidden="true"
              className={ignoredBusy ? "animate-spin" : undefined}
              size={14}
            />
            {ignoredBusy ? "Clearing..." : "Clear ignored apps"}
          </button>
        </div>
        {loading ? (
          <p className="mt-3 text-xs text-zinc-500">Loading ignored apps...</p>
        ) : ignoredApps.length > 0 ? (
          <ul className="mt-3 grid gap-2 sm:grid-cols-2" aria-label="Ignored meeting applications">
            {ignoredApps.map((app) => (
              <li
                key={app}
                className="min-w-0 break-all rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-xs text-zinc-300"
              >
                {app}
              </li>
            ))}
          </ul>
        ) : (
          <p className="mt-3 text-xs text-zinc-500">
            {meetingDetectionEnabled
              ? "No meeting apps are ignored. Detection notifications remain eligible for supported apps."
              : "Meeting suggestions are off. No native meeting evidence is sampled."}
          </p>
        )}
      </div>

      {message ? (
        <p role="status" className="flex items-center gap-2 text-xs text-emerald-300">
          <Check aria-hidden="true" size={14} />
          {message}
        </p>
      ) : null}
      {error ? (
        <p role="alert" className="text-xs text-red-300">
          Audio settings could not be updated: {error}
        </p>
      ) : null}
    </section>
  );
}

function PermissionShortcut({
  icon,
  title,
  body,
  onClick,
}: {
  icon: ReactNode;
  title: string;
  body: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex min-h-20 items-start gap-3 rounded-lg border border-zinc-800 bg-zinc-950 p-3 text-left hover:border-zinc-600 hover:bg-zinc-900"
    >
      <span className="mt-0.5 text-cyan-300">{icon}</span>
      <span>
        <strong className="block text-sm text-zinc-200">{title}</strong>
        <span className="mt-1 block text-xs leading-5 text-zinc-500">{body}</span>
        <span className="mt-1.5 inline-flex items-center gap-1 text-xs font-semibold text-cyan-200">
          Open system settings
          <ExternalLink aria-hidden="true" size={11} />
        </span>
      </span>
    </button>
  );
}

function OverlayInteractionCard() {
  const [passthrough, setPassthrough] = useState(true);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    invoke<boolean>("get_mouse_passthrough")
      .then(setPassthrough)
      .catch((nextError) => setError(String(nextError)))
      .finally(() => setLoading(false));
  }, []);

  async function update(enabled: boolean) {
    if (busy) return;
    const previous = passthrough;
    setPassthrough(enabled);
    setBusy(true);
    setError("");
    try {
      await invoke("set_mouse_passthrough", { enabled });
    } catch (nextError) {
      setPassthrough(previous);
      setError(String(nextError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="space-y-4 rounded-xl border border-zinc-800 bg-zinc-900 p-5">
      <div>
        <h2 className="font-semibold text-zinc-100">Overlay interaction</h2>
        <p className="mt-1 text-xs leading-5 text-zinc-400">
          Keep the overlay visible without blocking the app underneath, or make it directly clickable.
          This setting does not change listening.
        </p>
      </div>
      <label className="flex cursor-pointer items-start justify-between gap-4 rounded-lg border border-zinc-800 bg-zinc-950 p-4">
        <span className="flex min-w-0 gap-3">
          <MousePointer2 className="mt-0.5 h-4 w-4 shrink-0 text-cyan-300" />
          <span>
            <span className="block text-sm font-medium text-zinc-200">Click through the overlay</span>
            <span className="mt-1 block text-xs leading-5 text-zinc-500">
              Pointer input passes to the application underneath until you switch interaction mode.
            </span>
          </span>
        </span>
        <input
          type="checkbox"
          className="mt-0.5 h-4 w-4 shrink-0 accent-cyan-400"
          checked={passthrough}
          disabled={loading || busy}
          onChange={(event) => void update(event.target.checked)}
        />
      </label>
      {error ? (
        <p role="alert" className="text-xs text-red-300">
          Overlay interaction could not be saved: {error}
        </p>
      ) : null}
    </section>
  );
}

interface KeybindEntry {
  action: string;
  accelerator: string;
}

const KEYBIND_LABELS: Record<string, { title: string; body: string }> = {
  toggle_listening: {
    title: "Start or stop listening",
    body: "Controls microphone and system-audio capture.",
  },
  push_to_talk: {
    title: "Push to talk",
    body: "Temporarily starts the listening action.",
  },
  toggle_overlay: {
    title: "Show or hide overlay",
    body: "Changes overlay visibility without changing audio.",
  },
  toggle_dashboard: {
    title: "Show or hide dashboard",
    body: "Returns to this control center.",
  },
};

function ShortcutsCard() {
  const [entries, setEntries] = useState<KeybindEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyAction, setBusyAction] = useState("");
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  async function load() {
    setLoading(true);
    setError("");
    try {
      setEntries(await invoke<KeybindEntry[]>("list_keybinds"));
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void load();
  }, []);

  async function save(entry: KeybindEntry) {
    if (busyAction) return;
    setBusyAction(entry.action);
    setMessage("");
    setError("");
    try {
      await invoke("set_keybind", {
        action: entry.action,
        accelerator: entry.accelerator,
      });
      setMessage(`${KEYBIND_LABELS[entry.action]?.title ?? entry.action} saved. Restart Bluey to apply it everywhere.`);
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setBusyAction("");
    }
  }

  async function reset() {
    if (busyAction) return;
    setBusyAction("reset");
    setMessage("");
    setError("");
    try {
      await invoke("reset_keybinds");
      await load();
      setMessage("Default shortcuts restored. Restart Bluey to apply them everywhere.");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setBusyAction("");
    }
  }

  return (
    <section className="space-y-4 rounded-xl border border-zinc-800 bg-zinc-900 p-5" aria-labelledby="shortcut-settings-title">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 id="shortcut-settings-title" className="font-semibold text-zinc-100">
            Keyboard shortcuts
          </h2>
          <p className="mt-1 text-xs leading-5 text-zinc-400">
            Use platform shortcut syntax such as Ctrl+Alt+L or CmdOrCtrl+Shift+H.
          </p>
        </div>
        <button
          type="button"
          disabled={loading || Boolean(busyAction)}
          onClick={() => void reset()}
          className="inline-flex min-h-9 items-center gap-2 rounded-md border border-zinc-700 px-3 text-xs font-semibold text-zinc-300 hover:bg-zinc-800 disabled:opacity-50"
        >
          <RotateCcw aria-hidden="true" size={14} />
          Reset defaults
        </button>
      </div>

      <div className="divide-y divide-zinc-800 overflow-hidden rounded-lg border border-zinc-800 bg-zinc-950">
        {loading ? (
          <p className="p-4 text-sm text-zinc-500">Loading shortcuts...</p>
        ) : (
          entries.map((entry) => {
            const copy = KEYBIND_LABELS[entry.action] ?? {
              title: entry.action.replace(/_/g, " "),
              body: "Bluey keyboard action.",
            };
            return (
              <div
                key={entry.action}
                className="grid gap-3 p-4 sm:grid-cols-[minmax(0,1fr)_minmax(180px,240px)_auto] sm:items-center"
              >
                <span className="flex min-w-0 gap-3">
                  <Keyboard aria-hidden="true" className="mt-0.5 shrink-0 text-zinc-500" size={16} />
                  <span>
                    <strong className="block text-sm font-medium text-zinc-200">{copy.title}</strong>
                    <span className="mt-1 block text-xs leading-5 text-zinc-500">{copy.body}</span>
                  </span>
                </span>
                <label>
                  <span className="sr-only">{copy.title} shortcut</span>
                  <input
                    value={entry.accelerator}
                    onChange={(event) =>
                      setEntries((current) =>
                        current.map((item) =>
                          item.action === entry.action
                            ? { ...item, accelerator: event.target.value }
                            : item,
                        ),
                      )
                    }
                    className="min-h-10 w-full rounded-md border border-zinc-700 bg-zinc-900 px-3 font-mono text-xs text-zinc-100 outline-none focus:border-cyan-400"
                  />
                </label>
                <button
                  type="button"
                  disabled={Boolean(busyAction)}
                  onClick={() => void save(entry)}
                  className="inline-flex min-h-10 items-center justify-center gap-1.5 rounded-md border border-zinc-700 px-3 text-xs font-semibold text-zinc-200 hover:border-zinc-500 hover:bg-zinc-800 disabled:opacity-50"
                >
                  {busyAction === entry.action ? (
                    <RefreshCw aria-hidden="true" className="animate-spin" size={13} />
                  ) : (
                    <Save aria-hidden="true" size={13} />
                  )}
                  Save
                </button>
              </div>
            );
          })
        )}
      </div>
      {message ? (
        <p role="status" className="flex items-center gap-2 text-xs text-emerald-300">
          <Check aria-hidden="true" size={14} />
          {message}
        </p>
      ) : null}
      {error ? (
        <p role="alert" className="text-xs text-red-300">
          Shortcuts could not be updated: {error}
        </p>
      ) : null}
    </section>
  );
}

function ScreenSharePrivacyCard() {
  const [hidden, setHidden] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    invoke<boolean>("invisibility_state").then(setHidden).catch(() => {});
  }, []);

  async function updateVisibility() {
    if (busy) return;
    setBusy(true);
    try {
      setHidden(await invoke<boolean>("invisibility_toggle"));
    } catch (e) {
      console.warn("overlay visibility toggle failed", e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="rounded-lg bg-zinc-900 border border-zinc-800 p-5 space-y-4">
      <div>
        <h3 className="font-semibold text-zinc-100">Screen-share privacy</h3>
        <p className="mt-1 text-xs leading-relaxed text-zinc-400">
          Bluey requests capture exclusion where the operating system supports it. This is best effort, not an invisibility or security guarantee.
        </p>
      </div>
      <label className="flex cursor-pointer items-start justify-between gap-4 rounded-lg border border-zinc-800 bg-zinc-950 p-4">
        <span className="flex min-w-0 gap-3">
          {hidden ? <EyeOff className="mt-0.5 h-4 w-4 shrink-0 text-zinc-400" /> : <Eye className="mt-0.5 h-4 w-4 shrink-0 text-blue-300" />}
          <span>
            <span className="block text-sm font-medium text-zinc-200">Hide overlay now</span>
            <span className="mt-1 block text-xs leading-5 text-zinc-500">
              This changes only overlay visibility. Listening and background state remain separately controlled.
            </span>
          </span>
        </span>
        <input
          type="checkbox"
          className="mt-0.5 h-4 w-4 shrink-0 accent-blue-500"
          checked={hidden}
          disabled={busy}
          onChange={updateVisibility}
        />
      </label>
      <p className="text-xs leading-5 text-zinc-500">
        Use <kbd className="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-[11px] text-zinc-100">F19</kbd> or the {isWindows ? "tray" : "menu-bar"} command for the same show/hide control. Test your meeting app before sharing.
      </p>
    </div>
  );
}
