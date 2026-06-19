import { useEffect, useState } from "react";
import { invoke } from "../lib/tauri";
import { Check, Eye, EyeOff, Mail, Trash2, ExternalLink, LogOut } from "lucide-react";
import { ConsentToggle } from "../components/ConsentToggle";

/**
 * Codex Stage 24: Settings page rewrite.
 *
 * Removed all BYOK Deepgram-key prompts (pre-Stage-18 era). New layout:
 *   - Account card (email, balance, "Open billing portal", "Sign out", "Delete account")
 *   - Privacy card (disguise picker — section migrated from Stage 18)
 *   - Visibility card (invisibility hotkey + tray reminder)
 *
 * Design tokens: aurora-glass system — neutral text on .glass panels, the
 * refined accent for primary actions, error tokens for destructive actions.
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
    <div className="min-h-screen p-6">
      <div className="mx-auto max-w-2xl space-y-6">
        <h1 className="text-title-1 text-text-primary">Settings</h1>
        <AccountCard />
        <PrivacyCard />
        <DisguiseCard />
        <VisibilityCard />
      </div>
    </div>
  );
}

function PrivacyCard() {
  return (
    <div className="glass rounded-xl p-5 space-y-4">
      <div>
        <h3 className="text-headline text-text-primary">Privacy</h3>
        <p className="text-footnote text-text-tertiary mt-1 leading-relaxed">
          Controls whether Bluey may read your coding agents&apos; prior
          sessions to list and continue them.
        </p>
      </div>
      <ConsentToggle />
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
    <div className="glass rounded-xl p-5 space-y-4">
      <h3 className="text-headline text-text-primary">Account</h3>
      {me ? (
        <div className="space-y-1 text-callout">
          <div className="flex items-center gap-2 text-text-secondary">
            <Mail className="h-4 w-4 text-text-tertiary" /> {me.email}
          </div>
          <div className="text-text-tertiary text-footnote">
            Balance: <span className="text-text-primary font-medium tabular-nums">
              ${(me.balance_cents / 100).toFixed(2)}
            </span>
            {me.trial_seconds_remaining > 0 && (
              <span className="ml-2 text-success">
                ({Math.round(me.trial_seconds_remaining / 60)} min trial left)
              </span>
            )}
          </div>
        </div>
      ) : loaded ? (
        <div className="space-y-2">
          <p className="text-callout text-text-tertiary">
            Sign in once in the browser. Bluey stores desktop tokens in the OS keychain.
          </p>
          <button
            onClick={signIn}
            className="inline-flex items-center gap-1 text-callout px-3 py-1.5 rounded-md bg-accent hover:bg-accent-hover text-white transition-colors duration-200"
          >
            Sign in or create account <ExternalLink className="h-3 w-3" />
          </button>
        </div>
      ) : (
        <p className="text-callout text-text-tertiary">Checking account…</p>
      )}
      {me && <div className="flex flex-wrap gap-2">
        <button
          onClick={openPortal}
          className="inline-flex items-center gap-1 text-callout px-3 py-1.5 rounded-md border border-hairline hover:border-hairline-strong text-text-secondary hover:text-text-primary transition-colors duration-200"
        >
          Manage billing <ExternalLink className="h-3 w-3" />
        </button>
        <button
          onClick={signOut}
          className="inline-flex items-center gap-1 text-callout px-3 py-1.5 rounded-md border border-hairline hover:border-hairline-strong text-text-secondary hover:text-text-primary transition-colors duration-200"
        >
          Sign out <LogOut className="h-3 w-3" />
        </button>
        <button
          onClick={deleteAccount}
          className="inline-flex items-center gap-1 text-callout px-3 py-1.5 rounded-md bg-error/10 border border-error/30 hover:bg-error/20 text-error transition-colors duration-200"
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
    <div className="glass rounded-xl p-5 space-y-4">
      <div>
        <h3 className="text-headline text-text-primary">Disguise</h3>
        <p className="text-footnote text-text-tertiary mt-1 leading-relaxed">
          How Bluey appears in your menu bar and to screen-shares.{" "}
          <a href="https://bluey.sh/docs/disguise" target="_blank" rel="noreferrer"
            className="text-accent-subtle-text underline">Why?</a>
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
                "w-full text-left rounded-lg border p-3 transition-colors duration-200 " +
                (active
                  ? "bg-accent-subtle border-accent"
                  : "bg-bg-input border-hairline hover:border-hairline-strong")
              }
            >
              <div className="flex items-center justify-between">
                <span className={"text-callout font-medium " + (active ? "text-accent-subtle-text" : "text-text-secondary")}>
                  {opt.label}
                </span>
                {active && <Check className="h-4 w-4 text-accent-subtle-text" />}
              </div>
              <p className="text-footnote text-text-tertiary mt-1">{opt.desc}</p>
            </button>
          );
        })}
      </div>
    </div>
  );
}

function VisibilityCard() {
  return (
    <div className="glass rounded-xl p-5 space-y-3">
      <h3 className="text-headline text-text-primary">Visibility</h3>
      <div className="flex items-start gap-3">
        <EyeOff className="h-4 w-4 text-text-tertiary mt-0.5 shrink-0" />
        <p className="text-footnote text-text-secondary leading-relaxed">
          <kbd className="px-1.5 py-0.5 rounded bg-bg-raised-2 text-text-primary text-caption font-mono">F19</kbd>{" "}
          toggles Bluey&apos;s overlay on or off instantly. The menu-bar icon
          and the {`"Invisible"`} tray entry do the same thing.
        </p>
      </div>
      <div className="flex items-start gap-3">
        <Eye className="h-4 w-4 text-text-tertiary mt-0.5 shrink-0" />
        <p className="text-footnote text-text-secondary leading-relaxed">
          When meeting apps (Zoom, Teams, Slack, Webex) are in front, Bluey
          can disguise itself automatically. We&apos;ll ask you once.
        </p>
      </div>
    </div>
  );
}
