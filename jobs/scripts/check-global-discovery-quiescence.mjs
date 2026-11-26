#!/usr/bin/env node

import { readSync } from "node:fs";
import { pathToFileURL } from "node:url";

const FORMAT = "bluey-jobs-global-quiescence-v2";
const MAX_CENSUS_BYTES = 512;

// One read-only statement so both aggregates see the same database snapshot.
// Expired and partially populated leases still count: neither proves that an
// old run or worker has finished. No source identifiers or lease data leave SQL.
export const GLOBAL_DISCOVERY_CENSUS_SQL = `WITH source_census AS (
    SELECT COUNT(*) AS total,
           COUNT(CASE WHEN lease_owner IS NOT NULL OR lease_token IS NOT NULL
                       OR lease_expires_at_ms IS NOT NULL THEN 1 END) AS leased,
           COUNT(CASE WHEN status IS NULL OR status NOT IN ('active', 'disabled')
                       OR health IS NULL OR health NOT IN ('waiting', 'healthy', 'degraded', 'paused')
                      THEN 1 END) AS unclear
    FROM jobs_global_discovery_sources
), run_census AS (
    SELECT COUNT(*) AS total,
           COUNT(CASE WHEN status = 'running' THEN 1 END) AS running,
           COUNT(CASE WHEN status IS NULL OR status NOT IN ('running', 'completed', 'failed')
                       THEN 1 END) AS unknown,
           COUNT(CASE WHEN status IN ('completed', 'failed')
                       AND (started_at_ms IS NULL OR started_at_ms <= 0 OR completed_at_ms IS NULL
                            OR completed_at_ms < started_at_ms) THEN 1 END) AS invalid_terminal
    FROM jobs_global_ingestion_runs
)
SELECT '${FORMAT} '
       || CAST(source_census.total AS TEXT) || ' ' || CAST(source_census.leased AS TEXT) || ' '
       || CAST(source_census.unclear AS TEXT) || ' '
       || CAST(run_census.total AS TEXT) || ' ' || CAST(run_census.running AS TEXT) || ' '
       || CAST(run_census.unknown AS TEXT) || ' ' || CAST(run_census.invalid_terminal AS TEXT) AS census
FROM source_census CROSS JOIN run_census;
`;

export function checkGlobalDiscoveryCensus(raw) {
  const fields = /^bluey-jobs-global-quiescence-v2((?: (?:0|[1-9][0-9]{0,15})){7})\n$/.exec(raw);
  if (!fields || fields[0] !== raw) throw new Error("invalid_global_discovery_census");
  const counts = fields[1].slice(1).split(" ").map(Number);
  if (!counts.every(Number.isSafeInteger)) throw new Error("invalid_global_discovery_census_count");
  const [sources, sourcesWithLease, sourcesWithUnclearState, runs, running, unknownStatus, invalidTerminal] = counts;
  const unresolvedRuns = running + unknownStatus + invalidTerminal;
  if (sourcesWithLease > sources || sourcesWithUnclearState > sources
      || !Number.isSafeInteger(unresolvedRuns) || unresolvedRuns > runs
      || (sources === 0 && runs !== 0)) {
    throw new Error("inconsistent_global_discovery_census");
  }
  return {
    schema: FORMAT,
    status: sourcesWithLease || sourcesWithUnclearState || unresolvedRuns
      ? "hold" : sources ? "no_inflight_rows" : "no_sources",
    sources,
    sourcesWithLease,
    sourcesWithUnclearState,
    runs,
    running,
    unknownStatus,
    invalidTerminal,
    activationAuthorized: false,
  };
}

function readCensus() {
  const bytes = Buffer.alloc(MAX_CENSUS_BYTES + 1);
  let offset = 0;
  while (offset < bytes.length) {
    const count = readSync(0, bytes, offset, bytes.length - offset, null);
    if (count === 0) break;
    offset += count;
  }
  if (offset > MAX_CENSUS_BYTES) throw new Error("oversized_global_discovery_census");
  return bytes.subarray(0, offset).toString("utf8");
}

function main(args) {
  if (args.length !== 1) return 64;
  if (args[0] === "--sql") {
    process.stdout.write(GLOBAL_DISCOVERY_CENSUS_SQL);
    return 0;
  }
  if (args[0] !== "--check") return 64;
  try {
    const result = checkGlobalDiscoveryCensus(readCensus());
    process.stdout.write(`${JSON.stringify(result)}\n`);
    return result.status === "no_inflight_rows" ? 0 : 2;
  } catch {
    // Never echo the input: an operator could pass the wrong file or stream.
    process.stderr.write("Global discovery census was not valid. Hold the decoder rollout.\n");
    return 65;
  }
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) {
  process.exitCode = main(process.argv.slice(2));
}
