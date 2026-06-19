import type { RouterMeta } from "./responseReducer";

/**
 * Compact lane indicator shown above each in-flight Bluey Auto card.
 *
 * Renders the latency lane (instant/balanced/deep), the chosen provider +
 * model, and the classifier confidence. Clicking the badge surfaces the full
 * RouterMeta JSON in a tooltip so power users can see what Bluey decided.
 *
 * R14.5 — fields come from the daemon's RouterMeta which mirrors
 * cue_router::TaskClassification + cue_router::ProviderRoute.
 */
export function LaneBadge({ meta, refined }: { meta: RouterMeta; refined?: boolean }) {
  const laneClass = laneStyles(meta.latency_lane);
  const lanePrettyName = prettyLane(meta.latency_lane);

  return (
    <div
      className="flex items-center gap-2 text-caption tracking-wide"
      title={JSON.stringify(meta, null, 2)}
    >
      <span className={`px-1.5 py-0.5 rounded font-semibold uppercase ${laneClass}`}>
        {lanePrettyName}
      </span>
      <span className="text-text-tertiary">{meta.task_type.replace(/_/g, " ")}</span>
      <span className="text-text-quaternary">·</span>
      <span className="text-text-tertiary">
        {meta.provider_name}/{shortenModel(meta.model)}
      </span>
      <span className="text-text-quaternary">·</span>
      <span className="text-text-tertiary">{Math.round(meta.confidence * 100)}%</span>
      {refined ? (
        <span className="ml-1 px-1.5 py-0.5 rounded bg-warning/10 text-warning font-semibold">
          REFINED
        </span>
      ) : null}
    </div>
  );
}

function laneStyles(lane: string): string {
  switch (lane) {
    case "instant":
      return "bg-success/10 text-success";
    case "balanced":
      return "bg-accent-subtle text-accent-subtle-text";
    case "deep":
      return "bg-warning/10 text-warning";
    default:
      return "bg-bg-raised-2 text-text-tertiary border border-hairline";
  }
}

function prettyLane(lane: string): string {
  switch (lane) {
    case "instant":
      return "instant";
    case "balanced":
      return "balanced";
    case "deep":
      return "deep";
    default:
      return lane;
  }
}

function shortenModel(model: string): string {
  // gpt-4o-mini -> gpt-4o-mini (already short)
  // claude-3-5-sonnet-latest -> 3-5-sonnet
  // claude-3-7-sonnet-latest -> 3-7-sonnet
  if (model.startsWith("claude-")) {
    return model.replace(/^claude-/, "").replace(/-latest$/, "");
  }
  return model;
}
