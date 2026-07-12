import { useEffect, useState } from "react";
import { invoke } from "../lib/tauri";
import { Cloud, Eye, EyeOff, FileAudio, GraduationCap, Mail, Trash2, ExternalLink, LogOut } from "lucide-react";

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

export function Settings() {
  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100 p-6">
      <div className="mx-auto max-w-2xl space-y-6">
        <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
        <AccountCard />
        <DataControlsCard />
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
  useEffect(() => {
    let cancelled = false;
    const loadAccount = (markLoaded: boolean) => {
      invoke<AccountMe | null>("account_me")
        .then((next) => {
          if (!cancelled) setMe(next);
        })
        .catch(() => {})
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

  async function signIn() {
    try {
      const url = await invoke<string>("get_signin_url");
      window.open(url, "_blank");
    } catch (e) {
      console.warn("get_signin_url failed", e);
    }
  }

  async function openPortal() {
    try {
      const url = await invoke<string>("billing_portal_url");
      window.open(url, "_blank");
    } catch (e) {
      console.warn("billing_portal failed", e);
    }
  }
  async function signOut() {
    try { await invoke("sign_out"); window.location.href = "/"; }
    catch (e) { console.warn("sign_out failed", e); }
  }
  async function deleteAccount() {
    if (!acceptDataLoss || !acceptCreditLoss || deleteText !== "DELETE") return;
    setDeleteBusy(true);
    try {
      await invoke("delete_account_now");
      window.location.href = "/";
    }
    catch (e) { console.warn("delete_account failed", e); }
    finally { setDeleteBusy(false); }
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
            Sign in once in the browser for cloud answers, credits, and saved-session sync. You can turn sync off below.
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
          onClick={() => setDeleteOpen(true)}
          className="inline-flex items-center gap-1 text-sm px-3 py-1.5 rounded-md bg-red-500/10 border border-red-500/30 hover:bg-red-500/20 text-red-300"
        >
          Delete account <Trash2 className="h-3 w-3" />
        </button>
      </div>}
      {deleteOpen && me && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4">
          <div className="w-full max-w-lg rounded-xl border border-red-500/30 bg-zinc-950 p-5 shadow-2xl">
            <div className="flex items-start gap-3">
              <Trash2 className="mt-1 h-5 w-5 text-red-300" />
              <div>
                <h3 className="text-lg font-semibold text-zinc-100">Delete Bluey account?</h3>
                <p className="mt-1 text-sm leading-relaxed text-zinc-400">
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
              <button
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

function DataControlsCard() {
  const [controls, setControls] = useState<DataControls | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    invoke<DataControls>("get_data_controls")
      .then(setControls)
      .catch((e) => setError(String(e)));
  }, []);

  async function updateCloudSync(enabled: boolean) {
    if (!controls || busy) return;
    setBusy(true);
    setError("");
    try {
      const next = await invoke<DataControls>("set_cloud_sync_enabled", { enabled });
      setControls(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="rounded-lg bg-zinc-900 border border-zinc-800 p-5 space-y-4">
      <div>
        <h3 className="font-semibold text-zinc-100">Data controls</h3>
        <p className="mt-1 text-xs leading-relaxed text-zinc-400">
          Verified sign-in enables saved-session sync for reliable history. You can turn it off here and keep future sessions local.
        </p>
      </div>
      <div className="divide-y divide-zinc-800 rounded-lg border border-zinc-800 bg-zinc-950">
        <label className="flex cursor-pointer items-start justify-between gap-4 p-4">
          <span className="flex min-w-0 gap-3">
            <Cloud className="mt-0.5 h-4 w-4 shrink-0 text-blue-300" />
            <span>
              <span className="block text-sm font-medium text-zinc-200">Cloud session sync</span>
              <span className="mt-1 block text-xs leading-5 text-zinc-500">
                Upload new session transcripts, answers, and approved attachments for signed-in restore and memory.
              </span>
            </span>
          </span>
          <input
            type="checkbox"
            className="mt-0.5 h-4 w-4 shrink-0 accent-blue-500"
            checked={controls?.cloud_sync_enabled ?? false}
            disabled={!controls || busy}
            onChange={(event) => updateCloudSync(event.target.checked)}
          />
        </label>
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
      {error && <p className="text-xs text-red-300" role="status">Could not update data controls: {error}</p>}
    </div>
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
