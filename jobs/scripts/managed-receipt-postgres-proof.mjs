import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { join } from "node:path";

const MODULE = "jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::";
const COMMON = Object.freeze([
  "phase657_owned_postgres_managed_request_start_dispatch_complete",
  "phase657_owned_postgres_managed_claim_complete",
  "phase657_owned_postgres_managed_effect_authorized",
  "phase657_owned_postgres_managed_irreversible_authorized",
  "phase658_owned_postgres_managed_connected_receipt_complete",
]);
const ACCEPTED = "phase660_owned_postgres_managed_start_accepted_receipt_replay_complete=true";
const RECOVERY = "phase660_owned_postgres_managed_receipt_terminal_update_recovery_complete=true";
const WARMUP = "phase660_owned_postgres_managed_receipt_interview_prep_complete=true";
const WARMUP_BROWSER = "phase660_owned_postgres_managed_receipt_interview_prep_browser_complete=true";
const ALL_WITNESSES = [...COMMON, ACCEPTED, RECOVERY, WARMUP, WARMUP_BROWSER];

export const RECEIPT_CASES = Object.freeze([
  Object.freeze({
    name: MODULE + "postgres_managed_start_dispatch_marks_delivery_unknown_when_configured",
    witnesses: COMMON,
  }),
  Object.freeze({
    name: MODULE + "postgres_managed_start_accepted_receipt_replay_is_read_only",
    witnesses: Object.freeze([...COMMON, ACCEPTED]),
  }),
  Object.freeze({
    name: MODULE + "postgres_managed_receipt_replay_recovers_terminal_update_failure",
    witnesses: Object.freeze([...COMMON, RECOVERY]),
  }),
]);

const INTERVIEW_PREP_CASES = Object.freeze([
  Object.freeze({
    name: MODULE + "postgres_managed_receipt_grounds_authenticated_interview_prep",
    witnesses: Object.freeze([...COMMON, WARMUP]),
  }),
]);

const INTERVIEW_PREP_BROWSER_CASES = Object.freeze([
  Object.freeze({
    name: MODULE + "postgres_managed_receipt_browser_downloads_interview_prep",
    witnesses: Object.freeze([...COMMON, WARMUP_BROWSER]),
  }),
]);

export function managedReceiptCases(profile = "baseline") {
  if (profile === "baseline") return RECEIPT_CASES;
  if (profile === "interview-prep") return INTERVIEW_PREP_CASES;
  assert.equal(profile, "interview-prep-browser", "unknown managed receipt proof profile");
  return INTERVIEW_PREP_BROWSER_CASES;
}

function keys(row, expected, label) {
  assert(row && typeof row === "object" && !Array.isArray(row), label);
  assert.deepEqual(Object.keys(row).sort(), [...expected].sort(), label);
}

// The caller selects an already qualified platform; never infer it from the
// transcript. This does not bind the executable or prove independent cleanup.
export function proveManagedReceiptPostgres(run, root, profile = "baseline", platform = "darwin-arm64") {
  const selected = managedReceiptCases(profile);
  assert(platform === "darwin-arm64" || platform === "linux-x64", "unknown managed receipt platform");
  const linux = platform === "linux-x64";
  const runRoot = /^\/tmp\/bluey-test-runs-[a-f0-9]{12}\/run\.[1-9][0-9]{0,15}\.[1-9][0-9]{0,9}\.[0-9]{3,12}$/;
  if (linux) assert.match(root, runRoot, "linux_ready_root");
  else {
    assert(root.startsWith("/private/"), "darwin_ready_root");
    assert.match(root.slice("/private".length), runRoot, "darwin_ready_root");
  }
  assert(run?.success === true && run.bad === false
    && run.terminal?.code === 0 && run.terminal?.signal === null, "wrapper_terminal");
  assert(typeof run.stdout === "string" && typeof run.stderr === "string", "wrapper_streams");
  assert(Buffer.byteLength(run.stdout) + Buffer.byteLength(run.stderr) <= 16 * 1024 * 1024, "stream_bound");
  for (const stream of [run.stdout, run.stderr]) {
    assert(!stream.includes("PostgreSQL case not selected"), "skipped_case");
    assert(!stream.includes("phase657_managed_claim_unexpected_response "), "claim_failure_diagnostic");
  }
  const lines = run.stdout.split("\n");
  assert.equal(lines.at(-1), "", "incomplete_stdout");
  const events = lines.flatMap((line, index) => {
    if (!line.startsWith('{"event":"nonprod_postgres_')) return [];
    const row = JSON.parse(line);
    assert.equal(JSON.stringify(row), line, "noncanonical_wrapper_event");
    return [{ row, index }];
  });
  assert.deepEqual(events.map(({ row }) => row.event), [
    "nonprod_postgres_ready", "nonprod_postgres_test_inventory",
    ...selected.flatMap(() => ["nonprod_postgres_case_started", "nonprod_postgres_case_finished"]),
    "nonprod_postgres_cases_finished", "nonprod_postgres_finished",
  ], "event_order");
  assert.equal(events.at(-1).index, lines.length - 2, "wrapper_finished_order");
  const ready = events[0].row;
  const readyKeys = ["event", "address", "port", "database", "dataDirectory", "systemIdentifier", "version", "vector"];
  keys(ready, linux ? [...readyKeys, "platform", "serverVersionNum"] : readyKeys, "ready_keys");
  assert.equal(ready.address, "127.0.0.1");
  assert(Number.isInteger(ready.port) && ready.port > 1023 && ready.port <= 65535, "ready_port");
  assert.equal(ready.database, "bluey_jobs_nonprod");
  assert.equal(ready.dataDirectory, join(root, "nonprod-postgres/data"), "ready_directory");
  assert.match(ready.systemIdentifier, /^[1-9][0-9]{0,19}$/);
  assert(BigInt(ready.systemIdentifier) <= 18_446_744_073_709_551_615n, "system_identifier_range");
  if (linux) {
    assert.equal(ready.platform, "linux-x64", "linux_ready_platform");
    assert.equal(ready.serverVersionNum, 170011, "linux_ready_server_version");
    assert.match(ready.version, /^PostgreSQL 17\.11(?: \([^()\r\n]+\))? on x86_64-[a-z0-9_-]+-linux-gnu, compiled by [^\r\n]+, 64-bit$/,
      "linux_ready_build");
    assert.equal(ready.vector, "0.8.7", "linux_ready_vector");
  } else {
    assert.match(ready.version, /^PostgreSQL 17\.10 \(Homebrew\) /);
    assert.equal(ready.vector, "0.8.3");
  }
  const inventory = events[1].row;
  keys(inventory, ["event", "names", "count", "sha256"], "inventory_keys");
  assert(Array.isArray(inventory.names) && inventory.names.length === inventory.count
    && inventory.count >= selected.length && inventory.count <= 20_000
    && new Set(inventory.names).size === inventory.count
    && inventory.names.every((name) => typeof name === "string" && name.length <= 1024
      && /^[A-Za-z_][A-Za-z0-9_:]*$/.test(name)), "inventory_shape");
  assert.equal(createHash("sha256").update(JSON.stringify(inventory.names)).digest("hex"),
    inventory.sha256, "inventory_digest");
  assert(selected.every(({ name }) => inventory.names.includes(name)), "inventory_cases");
  assert.deepEqual(events.at(-2).row, {
    event: "nonprod_postgres_cases_finished",
    tests: selected.length, passed: selected.length, failed: [],
  });
  assert.deepEqual(events.at(-1).row, {
    event: "nonprod_postgres_finished", status: 0, shutdownAttempted: true, serverExited: true,
  });

  const usedDatabases = new Set();
  const claimedWitnesses = new Set();
  const claimedSummaries = new Set();
  const cases = selected.map(({ name, witnesses }, index) => {
    const start = events[2 + index * 2];
    const end = events[3 + index * 2];
    keys(start.row, ["event", "test", "database"], "case_start_keys");
    assert.equal(start.row.test, name, "case_start_name");
    assert.deepEqual(end.row, {
      event: "nonprod_postgres_case_finished", test: name, exitCode: 0, signal: null,
    });
    assert.match(start.row.database, /^bluey_jobs_case_[a-f0-9]{24}$/);
    assert(!usedDatabases.has(start.row.database), "case_database_reused");
    usedDatabases.add(start.row.database);
    const expected = [`test ${name} ... ${witnesses[0]}`, ...witnesses.slice(1), "ok"];
    const segment = lines.slice(start.index + 1, end.index);
    const at = segment.indexOf(expected[0]);
    assert(at >= 0, "case_witness_anchor");
    assert.deepEqual(segment.slice(at, at + expected.length), expected, "case_witness_block");
    witnesses.forEach((_, i) => claimedWitnesses.add(start.index + 1 + at + i));
    const summaries = segment.flatMap((line, i) => line.startsWith("test result: ") ? [i] : []);
    assert.equal(summaries.length, 1, "case_summary_count");
    assert(summaries[0] >= at + expected.length, "case_summary_order");
    const match = /^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; ([0-9]+) filtered out; finished in ([0-9]+\.[0-9]+)s$/
      .exec(segment[summaries[0]]);
    assert(match && match[1] === String(inventory.count - 1) && Number.isFinite(Number(match[2])),
      "case_summary_inventory");
    claimedSummaries.add(start.index + 1 + summaries[0]);
    return { name, database: start.row.database, filtered: inventory.count - 1, witnesses };
  });
  lines.forEach((line, index) => {
    if (ALL_WITNESSES.some((witness) => line.includes(witness))) {
      assert(claimedWitnesses.has(index), "unexpected_witness");
    }
    if (line.startsWith("test result: ")) assert(claimedSummaries.has(index), "unexpected_summary");
  });
  assert(!ALL_WITNESSES.some((witness) => run.stderr.includes(witness)), "wrong_witness_stream");
  return { ...(profile !== "baseline" && { profile }), cases,
    ...(linux && { platform }),
    inventory: { count: inventory.count, sha256: inventory.sha256 },
    port: ready.port, wrapperObservedServerExit: true };
}
