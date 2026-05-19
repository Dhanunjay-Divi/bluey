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
      className="flex items-center gap-2 text-[10px] tracking-wide"
      title={JSON.stringify(meta, null, 2)}
    >
      <span className={`px-1.5 py-0.5 rounded font-semibold uppercase ${laneClass}`}>
        {lanePrettyName}
      </span>
      <span className="text-zinc-400">{meta.task_type.replace(/_/g, " ")}</span>
      <span className="text-zinc-500">·</span>
      <span className="text-zinc-400">
        {meta.provider_name}/{shortenModel(meta.model)}
      </span>
      <span className="text-zinc-500">·</span>
      <span className="text-zinc-400">{Math.round(meta.confidence * 100)}%</span>
      {refined ? (
        <span className="ml-1 px-1.5 py-0.5 rounded bg-purple-700 text-purple-100 font-semibold">
          REFINED
        </span>
      ) : null}
    </div>
  );
}

function laneStyles(lane: string): string {
  switch (lane) {
    case "instant":
      return "bg-emerald-700 text-emerald-100";
    case "balanced":
      return "bg-blue-700 text-blue-100";
    case "deep":
      return "bg-purple-700 text-purple-100";
    default:
      return "bg-zinc-700 text-zinc-200";
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
