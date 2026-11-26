#!/usr/bin/env node

import { readSync } from "node:fs";
import { pathToFileURL } from "node:url";

const FORMAT = "bluey-jobs-submission-drain-v1";
const MAX_BYTES = 512;
const DAY_MS = 86_400_000;
const FIELDS = [
  "observedAtMs", "localClickStarted", "localUnknown", "cloudClickStarted", "cloudUnknown",
  "cloudAwaitingPublication", "activeEvidenceReservations", "unknownApplications",
  "unknownAttemptReservations", "agedExecutionRows", "invalidExecutionClocks",
];
const WIRE = /^bluey-jobs-submission-drain-v1((?: (?:0|[1-9][0-9]{0,15})){11})\n$/;

// Observation only. One statement so the run and publication rows come from
// the same database snapshot. Never connect to a database from this module.
export function submissionDrainSql(dialect) {
  const clock = dialect === "postgres"
    ? "CAST(FLOOR(EXTRACT(EPOCH FROM statement_timestamp()) * 1000) AS BIGINT)"
    : dialect === "sqlite"
      ? "CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)"
      : null;
  if (!clock) throw new Error("invalid_submission_drain_dialect");
  return `WITH observation AS (
  SELECT ${clock} AS observed_at_ms
), execution_rows AS (
  SELECT CASE ticket.status WHEN 'click_started' THEN 1 ELSE 2 END AS kind,
         ticket.updated_at_ms
    FROM jobs_local_run_tickets AS ticket
   WHERE ticket.status IN ('click_started', 'side_effect_unknown')
  UNION ALL
  SELECT CASE lease.phase WHEN 'click_started' THEN 3 WHEN 'side_effect_unknown' THEN 4 ELSE 5 END,
         lease.updated_at_ms
    FROM jobs_execution_leases AS lease
   WHERE lease.phase IN ('click_started', 'side_effect_unknown')
      OR (lease.phase = 'submitted' AND NOT EXISTS (
        SELECT 1 FROM jobs_submission_evidence_capacity AS evidence
        JOIN jobs_applications AS application
          ON application.id = evidence.application_id AND application.account_id = evidence.account_id
       WHERE evidence.account_id = lease.account_id AND evidence.application_id = lease.application_id
         AND evidence.run_id = lease.run_id AND evidence.runner = 'cloud'
         AND evidence.state = 'committed' AND evidence.completed_at_ms > 0
         AND application.state = 'submitted' AND application.submitted_at_ms > 0
      ))
), execution_counts AS (
  SELECT COUNT(CASE WHEN kind = 1 THEN 1 END) AS local_click,
         COUNT(CASE WHEN kind = 2 THEN 1 END) AS local_unknown,
         COUNT(CASE WHEN kind = 3 THEN 1 END) AS cloud_click,
         COUNT(CASE WHEN kind = 4 THEN 1 END) AS cloud_unknown,
         COUNT(CASE WHEN kind = 5 THEN 1 END) AS cloud_awaiting_publication,
         COUNT(CASE WHEN updated_at_ms BETWEEN 1 AND observed_at_ms - ${DAY_MS} THEN 1 END) AS aged,
         COUNT(CASE WHEN updated_at_ms IS NULL OR updated_at_ms <= 0 OR updated_at_ms > observed_at_ms
                    THEN 1 END) AS invalid_clocks
    FROM execution_rows CROSS JOIN observation
)
SELECT '${FORMAT} '
  || CAST(observed_at_ms AS TEXT) || ' ' || CAST(local_click AS TEXT) || ' ' || CAST(local_unknown AS TEXT)
  || ' ' || CAST(cloud_click AS TEXT) || ' ' || CAST(cloud_unknown AS TEXT)
  || ' ' || CAST(cloud_awaiting_publication AS TEXT)
  || ' ' || CAST((SELECT COUNT(*) FROM jobs_submission_evidence_capacity WHERE state = 'active') AS TEXT)
  || ' ' || CAST((SELECT COUNT(*) FROM jobs_applications WHERE state = 'side_effect_unknown') AS TEXT)
  || ' ' || CAST((SELECT COUNT(*) FROM jobs_attempt_reservations WHERE status = 'side_effect_unknown') AS TEXT)
  || ' ' || CAST(aged AS TEXT) || ' ' || CAST(invalid_clocks AS TEXT) AS census
FROM observation CROSS JOIN execution_counts;
`;
}

export function checkSubmissionDrainCensus(raw) {
  const match = WIRE.exec(raw);
  if (!match || match[0] !== raw) throw new Error("invalid_submission_drain_census");
  const counts = match[1].slice(1).split(" ").map(Number);
  if (counts[0] <= 0 || !counts.every(Number.isSafeInteger)) {
    throw new Error("invalid_submission_drain_count");
  }
  const executionRows = counts.slice(1, 6).reduce((total, count) => total + count, 0);
  const diagnosticRows = counts[9] + counts[10];
  if (!Number.isSafeInteger(executionRows) || diagnosticRows > executionRows) {
    throw new Error("inconsistent_submission_drain_census");
  }
  return {
    schema: FORMAT,
    status: counts.slice(1, 9).some((count) => count > 0) ? "pending" : "no_unresolved_rows",
    ...Object.fromEntries(FIELDS.map((field, index) => [field, counts[index]])),
    releaseAuthorized: false,
  };
}

function readCensus() {
  const buffer = Buffer.alloc(MAX_BYTES + 1);
  let length = 0;
  while (length < buffer.length) {
    const read = readSync(0, buffer, length, buffer.length - length, null);
    if (read === 0) break;
    length += read;
  }
  if (length > MAX_BYTES) throw new Error("oversized_submission_drain_census");
  return buffer.subarray(0, length).toString("utf8");
}

function main(args) {
  if (args.length === 2 && args[0] === "--sql" && ["postgres", "sqlite"].includes(args[1])) {
    process.stdout.write(submissionDrainSql(args[1]));
    return 0;
  }
  if (args.length !== 1 || args[0] !== "--check") return 64;
  try {
    const result = checkSubmissionDrainCensus(readCensus());
    process.stdout.write(`${JSON.stringify(result)}\n`);
    return result.status === "pending" ? 2 : 0;
  } catch {
    // A wrong file may contain private data. Do not echo any of the input.
    process.stderr.write("Jobs submission drain census is invalid. Keep recovery dependencies in place.\n");
    return 65;
  }
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) {
  process.exitCode = main(process.argv.slice(2));
}
