// Segmented control for the two-product mode toggle (managed AI vs. your agent).
//
// Consumes `useAgentMode()`. "Managed AI" detaches any agent; "Your agent"
// either switches to agent mode (when one is attached) or signals the caller
// to open an agent picker via `onRequestAgentPick`. When an agent is attached,
// its display name is shown subtly beneath the control.

import { Bot, Sparkles } from "lucide-react";
import { useAgentMode } from "../lib/useAgentMode";

export function ModeToggle({
  onRequestAgentPick,
}: {
  onRequestAgentPick?: () => void;
}) {
  const { mode, attachedAgent, setMode } = useAgentMode();

  const handleManaged = () => {
    void setMode("managed");
  };

  const handleAgent = () => {
    if (attachedAgent) {
      void setMode("agent");
    } else {
      onRequestAgentPick?.();
    }
  };

  const segmentBase =
    "flex items-center gap-1.5 rounded-full px-3 py-1 text-footnote font-medium transition-colors duration-200";
  const activeSegment = "bg-accent-subtle text-accent-subtle-text";
  const inactiveSegment = "text-text-tertiary hover:text-text-secondary";

  return (
    <div className="inline-flex flex-col items-start gap-1">
      <div
        role="tablist"
        aria-label="Answer mode"
        className="glass inline-flex items-center gap-0.5 rounded-full border border-hairline bg-bg-raised p-0.5"
      >
        <button
          type="button"
          role="tab"
          aria-selected={mode === "managed"}
          onClick={handleManaged}
          className={`${segmentBase} ${mode === "managed" ? activeSegment : inactiveSegment}`}
        >
          <Sparkles size={14} aria-hidden="true" />
          Managed AI
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={mode === "agent"}
          onClick={handleAgent}
          className={`${segmentBase} ${mode === "agent" ? activeSegment : inactiveSegment}`}
        >
          <Bot size={14} aria-hidden="true" />
          Your agent
        </button>
      </div>
      {attachedAgent ? (
        <span className="px-2 text-caption text-text-tertiary">
          via {attachedAgent.display_name}
        </span>
      ) : null}
    </div>
  );
}
