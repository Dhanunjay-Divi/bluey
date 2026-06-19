// Home — the mode surface.
//
// The first thing the user sees: which "brain" answers Bluey. Two products,
// one toggle — Bluey's managed AI, or the user's own coding agent driving with
// its own tools and connectors. The toggle and the active-mode summary live
// here so switching is a deliberate, visible choice rather than a buried
// setting. Picking an agent happens on /agents (the ModeToggle routes there).

import { useNavigate } from "react-router-dom";
import { Sparkles, Bot, ChevronRight } from "lucide-react";
import { ModeToggle } from "../components/ModeToggle";
import { useAgentMode } from "../lib/useAgentMode";

export function Home() {
  const navigate = useNavigate();
  const { mode, attachedAgent } = useAgentMode();

  return (
    <div className="mx-auto max-w-3xl space-y-8">
      <header className="space-y-2">
        <h1 className="text-title-1 text-text-primary">Welcome back</h1>
        <p className="text-callout text-text-secondary">
          Choose how Bluey answers. Switch anytime — your conversations carry
          over either way.
        </p>
      </header>

      <section className="glass-strong space-y-5 rounded-2xl p-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="space-y-1">
            <h2 className="text-headline text-text-primary">Answer engine</h2>
            <p className="text-footnote text-text-tertiary">
              {mode === "managed"
                ? "Bluey's managed AI is answering."
                : attachedAgent
                  ? `Your agent (${attachedAgent.display_name}) is answering.`
                  : "Your own coding agent answers."}
            </p>
          </div>
          <ModeToggle onRequestAgentPick={() => navigate("/agents")} />
        </div>

        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <ModeCard
            active={mode === "managed"}
            icon={<Sparkles size={16} />}
            title="Managed AI"
            body="Bluey's hosted models answer instantly. Metered to your balance, nothing to set up."
          />
          <ModeCard
            active={mode === "agent"}
            icon={<Bot size={16} />}
            title="Your agent"
            body="Your own coding agent drives — its tools, its connectors, its sessions. Pick one to begin."
            action={
              mode === "agent" ? undefined : (
                <button
                  type="button"
                  onClick={() => navigate("/agents")}
                  className="mt-3 inline-flex items-center gap-1 rounded-md border border-hairline px-3 py-1.5 text-subhead font-medium text-text-secondary transition-colors duration-200 hover:border-hairline-strong hover:text-text-primary"
                >
                  Choose an agent
                  <ChevronRight className="h-3.5 w-3.5" />
                </button>
              )
            }
          />
        </div>
      </section>
    </div>
  );
}

interface ModeCardProps {
  active: boolean;
  icon: React.ReactNode;
  title: string;
  body: string;
  action?: React.ReactNode;
}

function ModeCard({ active, icon, title, body, action }: ModeCardProps) {
  return (
    <div
      className={
        "rounded-xl p-4 transition-colors duration-200 " +
        (active
          ? "glass-strong border-accent"
          : "glass hover:border-hairline-strong")
      }
    >
      <div className="flex items-center gap-2">
        <span
          className={
            active ? "text-accent-subtle-text" : "text-text-tertiary"
          }
        >
          {icon}
        </span>
        <h3 className="text-subhead font-medium text-text-primary">{title}</h3>
        {active && (
          <span className="ml-auto rounded-full bg-accent-subtle px-2 py-0.5 text-caption text-accent-subtle-text">
            Active
          </span>
        )}
      </div>
      <p className="mt-2 text-footnote text-text-tertiary">{body}</p>
      {action}
    </div>
  );
}
