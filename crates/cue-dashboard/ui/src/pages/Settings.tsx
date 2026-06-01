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

const DISGUISE_OPTIONS = [
  { value: "none", label: "Off (visible as Bluey)", desc: "Bluey shows up as itself in your menu bar." },
  { value: "activity", label: "Activity Monitor", desc: "Recommended. Looks like the system process viewer." },
  { value: "terminal", label: "Terminal", desc: "Looks like an open terminal window." },
  { value: "settings", label: "System Settings", desc: "Looks like an open settings pane." },
];

interface AccountMe {
  id: string;
  email: string;
  balance_cents: number;
  trial_seconds_remaining: number;
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
    if (!confirm("This will permanently delete your account. Continue?")) return;
    if (prompt("Type DELETE to confirm") !== "DELETE") return;
    try { await invoke("delete_account_now"); window.location.href = "/"; }
    catch (e) { console.warn("delete_account failed", e); }
  }

  return (
    <div className="rounded-xl bg-zinc-900 border border-zinc-800 p-5 space-y-4">
      <h3 className="font-semibold text-zinc-100">Account</h3>
      {me ? (
        <div className="space-y-1 text-sm">
          <div className="flex items-center gap-2 text-zinc-300">
            <Mail className="h-4 w-4 text-zinc-500" /> {me.email}
          </div>
          <div className="text-zinc-400 text-xs">
            Balance: <span className="text-zinc-200 font-medium tabular-nums">
              ${(me.balance_cents / 100).toFixed(2)}
            </span>
            {me.trial_seconds_remaining > 0 && (
              <span className="ml-2 text-emerald-400">
                ({Math.round(me.trial_seconds_remaining / 60)} min trial left)
              </span>
            )}
          </div>
        </div>
      ) : loaded ? (
        <div className="space-y-2">
          <p className="text-sm text-zinc-400">
            Sign in once in the browser. Bluey stores desktop tokens in the OS keychain.
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
          Manage billing <ExternalLink className="h-3 w-3" />
        </button>
        <button
          onClick={signOut}
          className="inline-flex items-center gap-1 text-sm px-3 py-1.5 rounded-md bg-zinc-800 hover:bg-zinc-700 text-zinc-100"
        >
          Sign out <LogOut className="h-3 w-3" />
        </button>
        <button
          onClick={deleteAccount}
          className="inline-flex items-center gap-1 text-sm px-3 py-1.5 rounded-md bg-red-500/10 border border-red-500/30 hover:bg-red-500/20 text-red-300"
        >
          Delete account <Trash2 className="h-3 w-3" />
        </button>
      </div>}
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
          How Bluey appears in your menu bar and to screen-shares.{" "}
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
          toggles Bluey&apos;s overlay on or off instantly. The menu-bar icon
          and the {`"Invisible"`} tray entry do the same thing.
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
