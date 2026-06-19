import { useEffect, useState } from "react";
import { invoke } from "../lib/tauri";
import { CreditCard } from "lucide-react";

interface BalanceSnapshot {
  balance_cents: number;
  balance_label: string;
  trial_seconds_remaining: number;
  auto_topup_enabled: boolean;
  auto_topup_threshold_cents: number;
  auto_topup_amount_cents: number;
  low_balance_warning: boolean;
}

export function BalanceIndicator() {
  const [snapshot, setSnapshot] = useState<BalanceSnapshot | null>(null);
  const [status, setStatus] = useState<"idle" | "ready" | "signed-out" | "error">("idle");

  useEffect(() => {
    let cancelled = false;
    const poll = () => {
      invoke<BalanceSnapshot | null>("get_balance_snapshot")
        .then((next) => {
          if (cancelled) return;
          setSnapshot(next);
          setStatus(next ? "ready" : "signed-out");
        })
        .catch(() => {
          if (cancelled) return;
          setStatus("error");
        });
    };
    poll();
    const id = setInterval(poll, 30_000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  const label =
    status === "ready" && snapshot
      ? snapshot.balance_label
      : status === "signed-out"
        ? "Sign in"
        : status === "error"
          ? "Balance --"
          : "Checking";

  const tone = snapshot?.low_balance_warning
    ? "border border-warning/40 bg-warning/10 text-warning"
    : "border border-hairline bg-bg-raised/85 text-text-primary";

  return (
    <div
      className={`absolute right-5 top-4 z-20 flex items-center gap-2 rounded-full px-3 py-1.5 text-footnote font-semibold shadow-lg backdrop-blur ${tone}`}
      title={tooltip(snapshot, status)}
    >
      <CreditCard
        size={14}
        className={snapshot?.low_balance_warning ? "text-warning" : "text-accent-subtle-text"}
      />
      <span>{label}</span>
      {snapshot?.auto_topup_enabled ? (
        <span className="text-caption text-text-tertiary">auto</span>
      ) : null}
    </div>
  );
}

function tooltip(snapshot: BalanceSnapshot | null, status: string): string {
  if (!snapshot) {
    return status === "signed-out"
      ? "Bluey account is not signed in."
      : "Balance unavailable.";
  }
  const threshold = formatCents(snapshot.auto_topup_threshold_cents);
  const amount = formatCents(snapshot.auto_topup_amount_cents);
  const trialMinutes = Math.floor(snapshot.trial_seconds_remaining / 60);
  return [
    `Balance: ${snapshot.balance_label}`,
    snapshot.auto_topup_enabled
      ? `Auto top-up: ${amount} under ${threshold}`
      : "Auto top-up: off",
    `Trial time: ${trialMinutes} min`,
  ].join("\n");
}

function formatCents(cents: number): string {
  const sign = cents < 0 ? "-" : "";
  const abs = Math.abs(cents);
  return `${sign}$${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, "0")}`;
}
