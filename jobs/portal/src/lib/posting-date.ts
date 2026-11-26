const DAY_MS = 86_400_000;

export function isRecentPosting(
  job: { availability_status: string; posted_at_ms?: number | null },
  maximumAgeDays: number,
  nowMs = Date.now(),
): boolean {
  const timestamp = job.posted_at_ms;
  if (timestamp == null) return job.availability_status !== "expired";
  if (!Number.isFinite(timestamp)) return false;
  const ageMs = nowMs - timestamp;
  return Number.isFinite(ageMs)
    && ageMs >= -DAY_MS
    && ageMs <= Math.max(1, maximumAgeDays) * DAY_MS;
}

export function postingAgeLabel(
  job: { posted_at_ms?: number | null },
  nowMs = Date.now(),
): string {
  const timestamp = job.posted_at_ms;
  if (timestamp == null) return "Posting date not listed";
  if (!Number.isFinite(timestamp)) return "Posting date needs verification";
  const ageMs = nowMs - timestamp;
  if (!Number.isFinite(ageMs) || ageMs < -DAY_MS) return "Posting date needs verification";
  if (ageMs < 0) return "Posting date is in the future";
  const ageDays = Math.floor(ageMs / DAY_MS);
  if (ageDays === 0) return "Posted today";
  if (ageDays === 1) return "Posted yesterday";
  return `Posted ${ageDays} days ago`;
}

export function isMatchVisibleByState(
  job: { status: string; availability_status: string; posted_at_ms?: number | null },
  maximumAgeDays: number,
  passed: boolean,
  showPassed: boolean,
): boolean {
  return (showPassed ? passed : !passed)
    && job.status !== "skipped"
    && job.availability_status !== "expired"
    && (showPassed || isRecentPosting(job, maximumAgeDays));
}
