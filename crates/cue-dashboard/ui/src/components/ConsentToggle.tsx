// ConsentToggle — the session-history consent switch.
//
// A self-contained, reusable row for granting Bluey permission to read a
// coding agent's prior sessions (so it can list and continue them). The
// backend exposes no getter for the current value, so this toggle is
// write-only intent: it starts off, and flipping it sends the new value via
// `setSessionHistoryConsent`. The flip is optimistic and reverts on error.
//
// No Radix dependency is assumed: the switch is an accessible <button> with
// `role="switch"` + `aria-checked`, keyboard-toggleable, with a visible focus
// ring. The integrator drops this onto the Settings page.

import { useState } from "react";
import { setSessionHistoryConsent } from "../lib/agentApi";

export function ConsentToggle() {
  const [enabled, setEnabled] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function toggle() {
    if (busy) return;
    const next = !enabled;
    // Optimistically reflect the user's intent, then revert if the write fails.
    setEnabled(next);
    setBusy(true);
    setError(null);
    try {
      await setSessionHistoryConsent(next);
    } catch (e) {
      setEnabled(!next);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="glass rounded-lg p-4">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 space-y-1">
          <p className="text-callout text-text-primary">
            Read agents&apos; prior sessions
          </p>
          <p className="text-footnote text-text-tertiary">
            Lets Bluey list and continue your existing conversations with coding
            agents. Off by default; nothing is read until you turn this on.
          </p>
          {error && (
            <p className="text-caption text-error">
              Could not update setting: {error}
            </p>
          )}
        </div>

        <button
          type="button"
          role="switch"
          aria-checked={enabled}
          aria-label="Read agents' prior sessions"
          onClick={() => void toggle()}
          disabled={busy}
          className={
            "relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-bg-base disabled:opacity-50 " +
            (enabled ? "bg-accent" : "bg-bg-raised-2 border border-hairline")
          }
        >
          <span
            className={
              "inline-block h-4 w-4 transform rounded-full bg-white shadow transition-transform duration-200 " +
              (enabled ? "translate-x-6" : "translate-x-1")
            }
          />
        </button>
      </div>
    </div>
  );
}
