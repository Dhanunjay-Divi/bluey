import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Loader2, Shield, ArrowRight, Check, Eye, EyeOff } from "lucide-react";

/**
 * First-run onboarding wizard. Codex Stage 18 — deep-link Option A.
 *
 * Three focused steps, no in-app password prompt:
 *   1. Welcome — explains what Bluey is + a single "Sign in with browser" CTA.
 *      Discloses by default that the overlay runs disguised so customers
 *      know up front (transparency = trust).
 *   2. Authorizing — opens the browser, polls for the deep-link callback
 *      (handled by tauri-plugin-deep-link in lib.rs which emits
 *      "deep_link_login"), shows a clean spinner with cancel option.
 *   3. Linked — short success state with a "Get started" button.
 *
 * Design tokens locked here (used throughout the dashboard):
 *   surface: bg-zinc-950
 *   card:    bg-zinc-900 border-zinc-800
 *   accent:  text-blue-400 / bg-blue-500 (Bluey brand)
 *   text:    text-zinc-100 (primary), text-zinc-400 (secondary), text-zinc-500 (tertiary)
 */

type Step = "welcome" | "authorizing" | "linked" | "error";

interface DeepLinkResult {
  success: boolean;
  email?: string | null;
  error?: string | null;
}

export function Onboarding() {
  const [step, setStep] = useState<Step>("welcome");
  const [email, setEmail] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Subscribe to the deep_link_login event the Tauri lib.rs handler emits.
  useEffect(() => {
    const unlisten = listen<DeepLinkResult>("deep_link_login", (event) => {
      const r = event.payload;
      if (r.success) {
        setEmail(r.email ?? null);
        setStep("linked");
      } else {
        setError(r.error ?? "unknown error");
        setStep("error");
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  async function startSignIn() {
    setError(null);
    setStep("authorizing");
    try {
      // Daemon hits the bluey.dev/link landing page; the user signs in
      // there, the page mints a /auth/link/mint code, then redirects
      // to bluey://link?code=... — which the deep-link handler in
      // lib.rs catches and turns into the "deep_link_login" event we
      // listen for above.
      const url = await invoke<string>("get_signin_url");
      await openUrl(url);
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error).message);
      setStep("error");
    }
  }

  function cancel() {
    setStep("welcome");
    setError(null);
  }

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100 flex items-center justify-center p-6">
      <div className="w-full max-w-md">
        {step === "welcome" && <WelcomeStep onSignIn={startSignIn} />}
        {step === "authorizing" && <AuthorizingStep onCancel={cancel} />}
        {step === "linked" && email && <LinkedStep email={email} />}
        {step === "error" && error && (
          <ErrorStep error={error} onRetry={startSignIn} />
        )}
      </div>
    </div>
  );
}

function WelcomeStep({ onSignIn }: { onSignIn: () => void }) {
  return (
    <div className="space-y-8">
      <div className="text-center space-y-3">
        <div className="inline-flex h-12 w-12 items-center justify-center rounded-2xl bg-blue-500/15 text-blue-400">
          <BlueyMark />
        </div>
        <h1 className="text-2xl font-semibold tracking-tight">Welcome to Bluey</h1>
        <p className="text-sm text-zinc-400 leading-relaxed">
          A quiet AI copilot that listens, suggests answers, and stays out of the way.
        </p>
      </div>

      <div className="rounded-xl bg-zinc-900 border border-zinc-800 p-4 space-y-3">
        <div className="flex items-start gap-3">
          <EyeOff className="h-4 w-4 text-zinc-400 mt-0.5 shrink-0" />
          <div className="text-xs text-zinc-300 leading-relaxed">
            <span className="font-medium text-zinc-100">Hidden by default.</span>{" "}
            Bluey runs disguised in your menu bar so screen-shares and meeting
            recordings never see it. You can toggle visibility anytime.
          </div>
        </div>
        <div className="flex items-start gap-3">
          <Shield className="h-4 w-4 text-zinc-400 mt-0.5 shrink-0" />
          <div className="text-xs text-zinc-300 leading-relaxed">
            <span className="font-medium text-zinc-100">Sign in is browser-based.</span>{" "}
            We&apos;ll open your browser; you sign in once. No passwords or codes
            to type into Bluey.
          </div>
        </div>
      </div>

      <button
        onClick={onSignIn}
        className="w-full inline-flex items-center justify-center gap-2 rounded-lg bg-blue-500 hover:bg-blue-400 text-white font-medium py-2.5 transition-colors"
      >
        Sign in with browser
        <ArrowRight className="h-4 w-4" />
      </button>

      <p className="text-xs text-zinc-500 text-center">
        New to Bluey? You&apos;ll create an account in the same flow.
      </p>
    </div>
  );
}

function AuthorizingStep({ onCancel }: { onCancel: () => void }) {
  return (
    <div className="space-y-6 text-center">
      <div className="inline-flex h-12 w-12 items-center justify-center rounded-2xl bg-blue-500/15 text-blue-400">
        <Loader2 className="h-6 w-6 animate-spin" />
      </div>
      <div className="space-y-2">
        <h2 className="text-xl font-semibold">Waiting for browser…</h2>
        <p className="text-sm text-zinc-400 leading-relaxed">
          Complete sign-in in your browser. We&apos;ll detect it automatically
          when you&apos;re done.
        </p>
      </div>
      <div className="rounded-lg bg-zinc-900 border border-zinc-800 p-3 text-xs text-zinc-500 leading-relaxed">
        Browser didn&apos;t open?{" "}
        <button
          onClick={onCancel}
          className="text-blue-400 hover:text-blue-300 underline"
        >
          Try again
        </button>
        .
      </div>
    </div>
  );
}

function LinkedStep({ email }: { email: string }) {
  async function finish() {
    try {
      await invoke("complete_onboarding");
    } catch (e) {
      console.warn("complete_onboarding failed", e);
    }
    window.location.href = "/";
  }
  return (
    <div className="space-y-6 text-center">
      <div className="inline-flex h-12 w-12 items-center justify-center rounded-2xl bg-emerald-500/15 text-emerald-400">
        <Check className="h-6 w-6" />
      </div>
      <div className="space-y-2">
        <h2 className="text-xl font-semibold">You&apos;re in.</h2>
        <p className="text-sm text-zinc-400 leading-relaxed">
          Signed in as <span className="text-zinc-200 font-medium">{email}</span>.
        </p>
      </div>
      <div className="rounded-xl bg-zinc-900 border border-zinc-800 p-4 space-y-3 text-left">
        <div className="flex items-start gap-3">
          <Eye className="h-4 w-4 text-zinc-400 mt-0.5 shrink-0" />
          <div className="text-xs text-zinc-300 leading-relaxed">
            <span className="font-medium text-zinc-100">Press F19</span> (or
            the menu-bar icon) anytime to show or hide Bluey.
          </div>
        </div>
      </div>
      <button
        onClick={finish}
        className="w-full inline-flex items-center justify-center gap-2 rounded-lg bg-blue-500 hover:bg-blue-400 text-white font-medium py-2.5 transition-colors"
      >
        Get started
        <ArrowRight className="h-4 w-4" />
      </button>
    </div>
  );
}

function ErrorStep({ error, onRetry }: { error: string; onRetry: () => void }) {
  return (
    <div className="space-y-6 text-center">
      <div className="inline-flex h-12 w-12 items-center justify-center rounded-2xl bg-red-500/15 text-red-400">
        <span className="text-xl">!</span>
      </div>
      <div className="space-y-2">
        <h2 className="text-xl font-semibold">Sign-in didn&apos;t complete</h2>
        <p className="text-sm text-zinc-400 leading-relaxed break-words">
          {error}
        </p>
      </div>
      <button
        onClick={onRetry}
        className="w-full inline-flex items-center justify-center gap-2 rounded-lg bg-blue-500 hover:bg-blue-400 text-white font-medium py-2.5 transition-colors"
      >
        Try again
      </button>
    </div>
  );
}

function BlueyMark() {
  // Minimal monogram while we wait on a final brand asset.
  return (
    <svg
      viewBox="0 0 24 24"
      width="22"
      height="22"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <circle cx="12" cy="12" r="9" />
      <path d="M8 13.5c1.2 1.2 2.6 1.8 4 1.8s2.8-.6 4-1.8" />
      <circle cx="9" cy="10" r="0.8" fill="currentColor" />
      <circle cx="15" cy="10" r="0.8" fill="currentColor" />
    </svg>
  );
}
