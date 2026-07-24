// The detected-question dock — rises from the bottom of the document (beside
// the floating stack) when Bluey catches a question meant for you. Confirm to
// send it to your agent; the reasoning then streams into an inline Q&A block in
// the timeline. Mirrors the glass "for-me" hero card, relocated to the bottom.

export function DetectedDock({
  question,
  title,
  agentName,
  onAsk,
  onDismiss,
}: {
  question: string;
  title?: string;
  agentName?: string;
  onAsk: () => void;
  onDismiss: () => void;
}) {
  return (
    <div className="fp-dock" role="dialog" aria-label="Detected question">
      <div className="fp-dock-eyebrow">
        <span className="fp-dock-pulse" aria-hidden />
        {title ?? "Looks like a question for you"}
      </div>
      <p className="fp-dock-q">{question}</p>
      <div className="fp-dock-btns">
        <button className="fp-btn fp-btn-primary" onClick={onAsk}>
          Ask {agentName ?? "your agent"}
        </button>
        <button className="fp-btn fp-btn-ghost" onClick={onDismiss}>
          Dismiss
        </button>
      </div>
    </div>
  );
}
