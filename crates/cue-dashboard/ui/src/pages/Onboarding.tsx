import { useEffect, useState } from "react";
import { invoke } from "../lib/tauri";
import { listen } from "@tauri-apps/api/event";
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
 * Design tokens (aurora-glass system, defined in src/index.css):
 *   surface: app shell aurora (no wrapper bg)
 *   card:    .glass / .glass-strong (frosted panels)
 *   accent:  text-accent-subtle-text / bg-accent (one refined blue)
 *   text:    text-text-primary / text-text-secondary / text-text-tertiary
 */

type Step = "welcome" | "authorizing" | "linked" | "error";

interface DeepLinkResult {
  success: boolean;
  email?: string | null;
  error?: string | null;
}

export function Onboarding({ onComplete }: { onComplete?: () => void }) {
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
      // Daemon hits the bluey.sh/link landing page; the user signs in
      // there, the page mints a /auth/link/mint code, then redirects
      // to bluey://link?code=... — which the deep-link handler in
      // lib.rs catches and turns into the "deep_link_login" event we
      // listen for above.
      const url = await invoke<string>("get_signin_url");
      window.open(url, "_blank");
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
    <div className="min-h-screen text-text-primary flex items-center justify-center p-6">
      <div className="w-full max-w-md">
        {step === "welcome" && <WelcomeStep onSignIn={startSignIn} />}
        {step === "authorizing" && <AuthorizingStep onCancel={cancel} />}
        {step === "linked" && email && (
          <LinkedStep email={email} onComplete={onComplete} />
        )}
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
        <div className="inline-flex h-12 w-12 items-center justify-center rounded-2xl bg-accent-subtle text-accent-subtle-text">
          <BlueyMark />
        </div>
        <h1 className="text-title-1">Welcome to Bluey</h1>
        <p className="text-callout text-text-tertiary leading-relaxed">
          A quiet AI copilot that listens, suggests answers, and stays out of the way.
        </p>
      </div>

      <div className="glass rounded-xl p-4 space-y-3">
        <div className="flex items-start gap-3">
          <EyeOff className="h-4 w-4 text-text-tertiary mt-0.5 shrink-0" />
          <div className="text-footnote text-text-secondary leading-relaxed">
            <span className="font-medium text-text-primary">Hidden by default.</span>{" "}
            Bluey runs disguised in your menu bar so screen-shares and meeting
            recordings never see it. You can toggle visibility anytime.
          </div>
        </div>
        <div className="flex items-start gap-3">
          <Shield className="h-4 w-4 text-text-tertiary mt-0.5 shrink-0" />
          <div className="text-footnote text-text-secondary leading-relaxed">
            <span className="font-medium text-text-primary">Sign in is browser-based.</span>{" "}
            We&apos;ll open your browser; you sign in once. No passwords or codes
            to type into Bluey.
          </div>
        </div>
      </div>

      <button
        onClick={onSignIn}
        className="w-full inline-flex items-center justify-center gap-2 rounded-lg bg-accent hover:bg-accent-hover text-white font-medium py-2.5 transition-colors duration-200"
      >
        Sign in with browser
        <ArrowRight className="h-4 w-4" />
      </button>

      <p className="text-footnote text-text-tertiary text-center">
        New to Bluey? You&apos;ll create an account in the same flow.
      </p>
    </div>
  );
}

function AuthorizingStep({ onCancel }: { onCancel: () => void }) {
  return (
    <div className="space-y-6 text-center">
      <div className="inline-flex h-12 w-12 items-center justify-center rounded-2xl bg-accent-subtle text-accent-subtle-text">
        <Loader2 className="h-6 w-6 animate-spin" />
      </div>
      <div className="space-y-2">
        <h2 className="text-title-3">Waiting for browser…</h2>
        <p className="text-callout text-text-tertiary leading-relaxed">
          Complete sign-in in your browser. We&apos;ll detect it automatically
          when you&apos;re done.
        </p>
      </div>
      <div className="glass rounded-lg p-3 text-footnote text-text-tertiary leading-relaxed">
        Browser didn&apos;t open?{" "}
        <button
          onClick={onCancel}
          className="text-accent-subtle-text hover:text-text-primary underline transition-colors duration-200"
        >
          Try again
        </button>
        .
      </div>
    </div>
  );
}

function LinkedStep({ email, onComplete }: { email: string; onComplete?: () => void }) {
  async function finish() {
    try {
      await invoke("complete_onboarding");
    } catch (e) {
      console.warn("complete_onboarding failed", e);
    }
    if (onComplete) {
      onComplete();
    } else {
      window.location.href = "/";
    }
  }
  return (
    <div className="space-y-6 text-center">
      <div className="inline-flex h-12 w-12 items-center justify-center rounded-2xl bg-success/10 text-success">
        <Check className="h-6 w-6" />
      </div>
      <div className="space-y-2">
        <h2 className="text-title-3">You&apos;re in.</h2>
        <p className="text-callout text-text-tertiary leading-relaxed">
          Signed in as <span className="text-text-primary font-medium">{email}</span>.
        </p>
      </div>
      <div className="glass rounded-xl p-4 space-y-3 text-left">
        <div className="flex items-start gap-3">
          <Eye className="h-4 w-4 text-text-tertiary mt-0.5 shrink-0" />
          <div className="text-footnote text-text-secondary leading-relaxed">
            <span className="font-medium text-text-primary">Press F19</span> (or
            the menu-bar icon) anytime to show or hide Bluey.
          </div>
        </div>
      </div>
      <button
        onClick={finish}
        className="w-full inline-flex items-center justify-center gap-2 rounded-lg bg-accent hover:bg-accent-hover text-white font-medium py-2.5 transition-colors duration-200"
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
      <div className="inline-flex h-12 w-12 items-center justify-center rounded-2xl bg-error/10 text-error">
        <span className="text-xl">!</span>
      </div>
      <div className="space-y-2">
        <h2 className="text-title-3">Sign-in didn&apos;t complete</h2>
        <p className="text-callout text-text-tertiary leading-relaxed break-words">
          {error}
        </p>
      </div>
      <button
        onClick={onRetry}
        className="w-full inline-flex items-center justify-center gap-2 rounded-lg bg-accent hover:bg-accent-hover text-white font-medium py-2.5 transition-colors duration-200"
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
