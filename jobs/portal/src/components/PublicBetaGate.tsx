import {
  AlertTriangle,
  ArrowLeft,
  BadgeCheck,
  CirclePause,
  Clock3,
  LogOut,
  RefreshCw,
  ShieldAlert,
  UsersRound,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { JobsBetaAccess, JobsBetaAccessReason } from "../api";
import blueyIcon from "../../../../web/assets/bluey-logo.svg";
import blueyWordmark from "../../../../web/assets/bluey-wordmark.svg";

type BlockedReason = Exclude<JobsBetaAccessReason, "admitted">;

interface GateCopy {
  title: string;
  detail: string;
  Icon: LucideIcon;
  tone: "accent" | "amber" | "red";
}

const GATE_COPY: Record<BlockedReason, GateCopy> = {
  verification_required: {
    title: "Verify your Bluey account",
    detail:
      "Bluey Jobs is open to verified Bluey accounts. Finish account verification, then try again.",
    Icon: BadgeCheck,
    tone: "accent",
  },
  not_open: {
    title: "Public beta enrollment is not open yet",
    detail:
      "Bluey Jobs is not accepting new public-beta accounts right now. Core Bluey remains available.",
    Icon: Clock3,
    tone: "accent",
  },
  window_closed: {
    title: "Public beta enrollment is closed",
    detail:
      "This public beta window has ended. You can keep using core Bluey and check here again later.",
    Icon: CirclePause,
    tone: "amber",
  },
  capacity_reached: {
    title: "The public beta is full",
    detail:
      "The current public release has reached capacity while we learn, improve, and protect reliability.",
    Icon: UsersRound,
    tone: "amber",
  },
  denied: {
    title: "Bluey Jobs access is unavailable",
    detail:
      "This Bluey account cannot use the Jobs public beta right now. Contact support if this seems wrong.",
    Icon: ShieldAlert,
    tone: "red",
  },
  suspended: {
    title: "Bluey Jobs is temporarily paused",
    detail:
      "The public beta is paused right now. Please try again later or continue in core Bluey.",
    Icon: CirclePause,
    tone: "amber",
  },
  unavailable: {
    title: "Bluey Jobs is temporarily unavailable",
    detail:
      "We could not safely confirm public-beta access, so no Jobs workspace was loaded. Please try again.",
    Icon: AlertTriangle,
    tone: "red",
  },
};

export interface PublicBetaGateProps {
  betaAccess: Exclude<JobsBetaAccess, { access: "admitted" }>;
  onRetry: () => void;
  onSignOut: () => void;
}

export function PublicBetaGate({
  betaAccess,
  onRetry,
  onSignOut,
}: PublicBetaGateProps) {
  const reason: BlockedReason = betaAccess.reason;
  const { title, detail, Icon, tone } = GATE_COPY[reason];
  const verificationRequired = reason === "verification_required";
  const denied = reason === "denied";

  return (
    <div className="public-beta-gate">
      <a className="jobs-skip-link" href="#public-beta-content">
        Skip to public beta status
      </a>
      <header className="entry-header">
        <a className="brand-lockup" href="/" aria-label="Bluey home">
          <img className="brand-icon" src={blueyIcon} alt="" />
          <img className="brand-wordmark" src={blueyWordmark} alt="" />
          <b>jobs</b>
        </a>
        <nav aria-label="Bluey navigation">
          <a href="/">Bluey</a>
          <a href="/account">Account</a>
        </nav>
        <button
          className="button secondary compact"
          type="button"
          onClick={onSignOut}
        >
          <LogOut aria-hidden="true" size={15} />
          Sign out
        </button>
      </header>

      <main id="public-beta-content" className="public-beta-main" tabIndex={-1}>
        <section
          className="public-beta-card"
          aria-labelledby="public-beta-title"
        >
          <span className={`public-beta-icon ${tone}`}>
            <Icon aria-hidden="true" size={26} />
          </span>
          <p className="eyebrow">BLUEY JOBS PUBLIC BETA</p>
          <h1 id="public-beta-title">{title}</h1>
          <p className="public-beta-detail">{detail}</p>
          <div className="public-beta-actions">
            {verificationRequired && (
              <a className="button primary" href="/account">
                Open Bluey account
              </a>
            )}
            {denied && (
              <a className="button primary" href="mailto:hello@bluey.sh">
                Contact support
              </a>
            )}
            <button
              className={`button ${verificationRequired || denied ? "secondary" : "primary"}`}
              type="button"
              onClick={onRetry}
            >
              <RefreshCw aria-hidden="true" size={15} />
              Try again
            </button>
            <a className="button secondary" href="/">
              <ArrowLeft aria-hidden="true" size={15} />
              Back to Bluey
            </a>
          </div>
          <p className="public-beta-note">
            Bluey Jobs is releasing in measured public stages. Access checks
            never reveal account or capacity details.
          </p>
        </section>
      </main>

      <footer className="entry-footer public-beta-footer">
        <span>Bluey Jobs public beta</span>
        <p>Core Bluey remains available.</p>
        <nav aria-label="Legal and support">
          <a href="/terms">Terms</a>
          <a href="/privacy">Privacy</a>
          <a href="mailto:hello@bluey.sh">Help</a>
        </nav>
      </footer>
    </div>
  );
}
