import { useEffect, useState } from "react";
import { invoke } from "../lib/tauri";
import { Check, Eye, EyeOff, Mail, Trash2, ExternalLink, LogOut } from "lucide-react";

/**
 * Codex Stage 24: Settings page rewrite.
 *
 * Removed all BYOK Deepgram-key prompts (pre-Stage-18 era). New layout:
 *   - Account card (email, balance, "Open billing portal", "Sign out", "Delete account")
 *   - Privacy card (disguise picker — section migrated from Stage 18)
 *   - Visibility card (invisibility hotkey + tray reminder)
 *
 * Design tokens: bg-zinc-950 surface, bg-zinc-900 cards, blue-500 brand,
 * red-500 destructive. Same palette as Onboarding.
 */

const platform = typeof navigator === "undefined" ? "" : navigator.userAgent;
const isWindows = platform.includes("Windows");
const isMac = platform.includes("Mac");

const DISGUISE_OPTIONS = [
  {
    value: "none",
    label: "Off (visible as Bluey)",
    desc: `Bluey shows up as itself in your ${isWindows ? "taskbar tray" : "menu bar"}.`,
  },
  {
    value: "activity",
    label: isWindows ? "Task Manager" : "Activity Monitor",
    desc: "Recommended. Uses the system process viewer identity.",
  },
  {
    value: "terminal",
    label: isWindows ? "Command Prompt" : "Terminal",
    desc: "Uses the platform terminal identity.",
  },
  {
    value: "settings",
    label: isMac ? "System Settings" : "Settings",
    desc: "Uses the platform settings identity.",
  },
];

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
        <DisguiseCard />
        <VisibilityCard />
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
    invoke<AccountMe | null>("account_me")
      .then(setMe)
      .catch(() => {})
      .finally(() => setLoaded(true));
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
            Sign in once in the browser for cloud answers, credits, and saved-session sync.
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

function DisguiseCard() {
  const [mode, setMode] = useState<string>("activity");
  useEffect(() => {
    invoke<string>("get_disguise").then(setMode).catch(() => {});
  }, []);
  async function update(next: string) {
    setMode(next);
    try { await invoke("set_disguise", { mode: next }); }
    catch (e) { console.warn("set_disguise failed", e); }
  }
  return (
    <div className="rounded-xl bg-zinc-900 border border-zinc-800 p-5 space-y-4">
      <div>
        <h3 className="font-semibold text-zinc-100">Disguise</h3>
        <p className="text-xs text-zinc-400 mt-1 leading-relaxed">
          How Bluey appears in your {isWindows ? "taskbar tray" : "menu bar"} and to screen-shares.{" "}
          <a href="https://bluey.sh/docs/disguise" target="_blank" rel="noreferrer"
            className="text-blue-400 hover:text-blue-300 underline">Why?</a>
        </p>
      </div>
      <div className="space-y-2">
        {DISGUISE_OPTIONS.map((opt) => {
          const active = opt.value === mode;
          return (
            <button
              key={opt.value}
              onClick={() => update(opt.value)}
              className={
                "w-full text-left rounded-lg border p-3 transition-colors " +
                (active
                  ? "bg-blue-500/10 border-blue-500/40"
                  : "bg-zinc-950 border-zinc-800 hover:border-zinc-700")
              }
            >
              <div className="flex items-center justify-between">
                <span className={"text-sm font-medium " + (active ? "text-blue-300" : "text-zinc-200")}>
                  {opt.label}
                </span>
                {active && <Check className="h-4 w-4 text-blue-400" />}
              </div>
              <p className="text-xs text-zinc-500 mt-1">{opt.desc}</p>
            </button>
          );
        })}
      </div>
    </div>
  );
}

function VisibilityCard() {
  return (
    <div className="rounded-xl bg-zinc-900 border border-zinc-800 p-5 space-y-3">
      <h3 className="font-semibold text-zinc-100">Visibility</h3>
      <div className="flex items-start gap-3">
        <EyeOff className="h-4 w-4 text-zinc-400 mt-0.5 shrink-0" />
        <p className="text-xs text-zinc-300 leading-relaxed">
          <kbd className="px-1.5 py-0.5 rounded bg-zinc-800 text-zinc-100 text-[11px] font-mono">F19</kbd>{" "}
          toggles Bluey&apos;s overlay on or off instantly. The tray icon and
          the {`"Invisible"`} tray entry do the same thing.
        </p>
      </div>
      <div className="flex items-start gap-3">
        <Eye className="h-4 w-4 text-zinc-400 mt-0.5 shrink-0" />
        <p className="text-xs text-zinc-300 leading-relaxed">
          When meeting apps (Zoom, Teams, Slack, Webex) are in front, Bluey
          can disguise itself automatically. We&apos;ll ask you once.
        </p>
      </div>
    </div>
  );
}
