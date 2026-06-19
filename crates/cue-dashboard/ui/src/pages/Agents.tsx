// Agents — the coding-agent picker.
//
// Lists every discovered coding agent (Claude Code, Codex, Gemini, Copilot, …)
// and lets the user pick which one should drive answers. Picking an agent
// attaches it (via `useAgentMode().attach`) and switches the app into agent
// mode; "View sessions" deep-links to the per-agent resume picker so a prior
// conversation can be continued instead.
//
// Data + attach state both come from `useAgentMode()` so the card grid always
// reflects the shared attach flags the moment an attach/detach lands. The hook
// fetches the list once on mount and funnels every failure into `error`.

import { useNavigate } from "react-router-dom";
import { Check, ChevronRight, Loader2 } from "lucide-react";
import { useAgentMode } from "../lib/useAgentMode";
import type { AgentSummary } from "../lib/agentTypes";

/** Visual treatment for a capability pill: a label + subtle tone classes. */
interface CapabilityStyle {
  label: string;
  className: string;
}

/**
 * Map a snake_case `capability` to a muted pill. Tones stay subtle (tinted
 * backgrounds, never neon): drive reads as ready/blue, read-only as plain
 * tertiary, the two "needs …" states as warning, cloud-blocked as error.
 */
function capabilityStyle(capability: string): CapabilityStyle {
  switch (capability) {
    case "drive":
      return {
        label: "Ready",
        className: "bg-accent-subtle text-accent-subtle-text",
      };
    case "read_only":
      return {
        label: "Read-only",
        className: "bg-bg-raised-2 text-text-tertiary border border-hairline",
      };
    case "needs_trust":
      return {
        label: "Needs trust",
        className: "bg-warning/10 text-warning",
      };
    case "needs_reauth":
      return {
        label: "Needs re-auth",
        className: "bg-warning/10 text-warning",
      };
    case "cloud_blocked":
      return {
        label: "Cloud blocked",
        className: "bg-error/10 text-error",
      };
    default:
      return {
        label: capability,
        className: "bg-bg-raised-2 text-text-tertiary border border-hairline",
      };
  }
}

export function Agents() {
  const navigate = useNavigate();
  const { agents, attachedAgent, loading, error, attach, detach } =
    useAgentMode();

  return (
    <div className="space-y-6">
      <header className="space-y-1">
        <h1 className="text-title-2 text-text-primary">Coding agents</h1>
        <p className="text-callout text-text-secondary">
          Pick which of your coding agents should drive Bluey&apos;s answers.
          The agent you choose answers using its own tools and connectors.
        </p>
      </header>

      {error && (
        <div className="glass rounded-lg p-4 text-footnote text-error">
          Could not load agents: {error}
        </div>
      )}

      {loading && agents.length === 0 ? (
        <div className="flex items-center gap-2 text-footnote text-text-tertiary">
          <Loader2 className="h-4 w-4 animate-spin" />
          Discovering coding agents…
        </div>
      ) : agents.length === 0 ? (
        <div className="glass rounded-xl p-8 text-center">
          <p className="text-headline text-text-primary">
            No coding agents detected
          </p>
          <p className="mx-auto mt-2 max-w-md text-footnote text-text-tertiary">
            Bluey looks for installed coding agents like Claude Code, Codex,
            Gemini, and Copilot. Install or sign in to one, then return here to
            let it drive your answers.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-3">
          {agents.map((agent) => (
            <AgentCard
              key={agent.kind}
              agent={agent}
              active={attachedAgent?.kind === agent.kind}
              busy={loading}
              onUse={() => void attach(agent.kind)}
              onStop={() => void detach()}
              onViewSessions={() => navigate(`/agents/${agent.kind}`)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

interface AgentCardProps {
  agent: AgentSummary;
  active: boolean;
  busy: boolean;
  onUse: () => void;
  onStop: () => void;
  onViewSessions: () => void;
}

function AgentCard({
  agent,
  active,
  busy,
  onUse,
  onStop,
  onViewSessions,
}: AgentCardProps) {
  const cap = capabilityStyle(agent.capability);

  return (
    <div
      className={
        "flex flex-col gap-4 rounded-xl p-5 transition-colors duration-200 " +
        (active
          ? "glass-strong border-accent"
          : "glass hover:border-hairline-strong")
      }
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 space-y-1">
          <div className="flex items-center gap-2">
            <h2 className="truncate text-headline text-text-primary">
              {agent.display_name}
            </h2>
            {active && (
              <span className="inline-flex shrink-0 items-center gap-1 rounded-full bg-accent-subtle px-2 py-0.5 text-caption text-accent-subtle-text">
                <Check className="h-3 w-3" />
                Active
              </span>
            )}
          </div>
          <span
            className={
              "inline-block rounded-full px-2 py-0.5 text-caption " +
              cap.className
            }
          >
            {cap.label}
          </span>
        </div>
      </div>

      <div className="space-y-0.5 text-footnote text-text-tertiary">
        <p>
          {agent.ready_connector_count}/{agent.connector_count} connectors ready
        </p>
        {agent.session_count !== null && (
          <p>
            {agent.session_count}{" "}
            {agent.session_count === 1 ? "session" : "sessions"}
          </p>
        )}
      </div>

      <div className="mt-auto flex items-center gap-2 pt-1">
        {active ? (
          <button
            type="button"
            onClick={onStop}
            disabled={busy}
            className="rounded-md border border-hairline px-3 py-1.5 text-subhead font-medium text-text-secondary transition-colors duration-200 hover:border-hairline-strong hover:text-text-primary disabled:opacity-50"
          >
            Stop using
          </button>
        ) : (
          <button
            type="button"
            onClick={onUse}
            disabled={busy}
            className="rounded-md bg-accent px-3 py-1.5 text-subhead font-medium text-white transition-colors duration-200 hover:bg-accent-hover disabled:opacity-50"
          >
            Use this agent
          </button>
        )}
        <button
          type="button"
          onClick={onViewSessions}
          className="inline-flex items-center gap-1 rounded-md border border-hairline px-3 py-1.5 text-subhead font-medium text-text-secondary transition-colors duration-200 hover:border-hairline-strong hover:text-text-primary"
        >
          View sessions
          <ChevronRight className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  );
}
