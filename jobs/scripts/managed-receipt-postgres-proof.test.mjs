import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { managedReceiptCases, proveManagedReceiptPostgres, RECEIPT_CASES } from "./managed-receipt-postgres-proof.mjs";
import {
  MANAGED_RECEIPT_CARGO_COMMAND, renderManagedReceiptPostgresWrapper,
} from "./render-managed-receipt-postgres-wrapper.mjs";
import { verifyReceiptSourceBindings } from "./managed-receipt-source-preflight.mjs";
import { stageManagedReceiptSource } from "./stage-managed-receipt-source.mjs";

const ROOT = "/private/tmp/bluey-test-runs-0123456789ab/run.1794466886.48891.16299199040";
const CASES = RECEIPT_CASES.map(({ name }) => name);
const INTERVIEW_PREP = "interview-prep";
const INTERVIEW_PREP_BROWSER = "interview-prep-browser";
const [warmupCase] = managedReceiptCases(INTERVIEW_PREP);
const [browserCase] = managedReceiptCases(INTERVIEW_PREP_BROWSER);
const names = [...CASES, "db::jobs::tests::other"];
const db = (i) => `bluey_jobs_case_${String(i + 1).padStart(24, "0")}`;
const inventory = (names) => JSON.stringify({ event: "nonprod_postgres_test_inventory",
  names, count: names.length, sha256: createHash("sha256").update(JSON.stringify(names)).digest("hex") });
const summary = "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 1.00s";
const buildSample = (selected, fullInventory) => [
  JSON.stringify({ event: "nonprod_postgres_ready", address: "127.0.0.1", port: 53790,
    database: "bluey_jobs_nonprod", dataDirectory: `${ROOT}/nonprod-postgres/data`,
    systemIdentifier: "7707177940447533940", version: "PostgreSQL 17.10 (Homebrew) on Darwin", vector: "0.8.3" }),
  inventory(fullInventory),
  ...selected.flatMap(({ name, witnesses }, i) => [
    JSON.stringify({ event: "nonprod_postgres_case_started", test: name, database: db(i) }),
    "running 1 test", `test ${name} ... ${witnesses[0]}`, ...witnesses.slice(1), "ok",
    summary.replace("3 filtered out", `${fullInventory.length - 1} filtered out`),
    JSON.stringify({ event: "nonprod_postgres_case_finished", test: name, exitCode: 0, signal: null }),
  ]),
  JSON.stringify({ event: "nonprod_postgres_cases_finished", tests: selected.length, passed: selected.length, failed: [] }),
  JSON.stringify({ event: "nonprod_postgres_finished", status: 0, shutdownAttempted: true, serverExited: true }),
  "",
].join("\n");
const sample = buildSample(RECEIPT_CASES, names);
const warmupSample = buildSample([warmupCase], [...names, warmupCase.name]);
const browserSample = buildSample([browserCase], [...names, browserCase.name]);
const run = (stdout = sample, stderr = "") =>
  ({ success: true, bad: false, terminal: { code: 0, signal: null }, stdout, stderr });
const recovery = RECEIPT_CASES[2].witnesses.at(-1);
const accepted = RECEIPT_CASES[1].witnesses.at(-1);

test("binds three exact case bodies to separate databases and the full inventory", () => {
  const proof = proveManagedReceiptPostgres(run(), ROOT);
  assert.deepEqual(proof.cases.map(({ name, database, filtered }) => [name, database, filtered]),
    CASES.map((name, i) => [name, db(i), 3]));
  assert.equal(proof.wrapperObservedServerExit, true);
});

test("selects one warmup body while keeping the full inventory and independent profile", () => {
  const proof = proveManagedReceiptPostgres(run(warmupSample), ROOT, INTERVIEW_PREP);
  assert.equal(proof.profile, INTERVIEW_PREP);
  assert.deepEqual(proof.cases.map(({ name, database, filtered }) => [name, database, filtered]),
    [[warmupCase.name, db(0), 4]]);
  assert.throws(() => proveManagedReceiptPostgres(run(warmupSample), ROOT), /event_order/);
  assert.throws(() => proveManagedReceiptPostgres(run(sample), ROOT, INTERVIEW_PREP), /event_order/);
  assert.throws(() => proveManagedReceiptPostgres(run(warmupSample), ROOT, "cli-supplied-case"), /unknown.*profile/);
});

test("selects the browser interview-prep body without broadening the API-only profile", () => {
  const proof = proveManagedReceiptPostgres(run(browserSample), ROOT, INTERVIEW_PREP_BROWSER);
  assert.equal(proof.profile, INTERVIEW_PREP_BROWSER);
  assert.deepEqual(proof.cases.map(({ name, database, filtered }) => [name, database, filtered]),
    [[browserCase.name, db(0), 4]]);
  assert.throws(() => proveManagedReceiptPostgres(run(browserSample), ROOT, INTERVIEW_PREP), /inventory_cases/);
  assert.throws(() => proveManagedReceiptPostgres(run(warmupSample), ROOT, INTERVIEW_PREP_BROWSER), /inventory_cases/);
});

test("Linux warmup proof requires the independently selected root, engine and extension profile", () => {
  const linuxRoot = ROOT.slice("/private".length);
  const fixture = warmupSample.split("\n");
  const ready = { ...JSON.parse(fixture[0]), dataDirectory: `${linuxRoot}/nonprod-postgres/data`,
    version: "PostgreSQL 17.11 (fixture build) on x86_64-pc-linux-gnu, compiled by fixture GCC, 64-bit",
    vector: "0.8.7", platform: "linux-x64", serverVersionNum: 170011 };
  const transcript = (event = ready) => run([JSON.stringify(event), ...fixture.slice(1)].join("\n"));
  const verify = (event) => proveManagedReceiptPostgres(transcript(event), linuxRoot, INTERVIEW_PREP, "linux-x64");
  assert.equal(verify().platform, "linux-x64");
  assert.equal(verify().cases[0].name, warmupCase.name);
  assert.throws(() => proveManagedReceiptPostgres(transcript(), linuxRoot, INTERVIEW_PREP), /darwin_ready_root/);
  assert.throws(() => proveManagedReceiptPostgres(transcript(), ROOT, INTERVIEW_PREP, "linux-x64"), /linux_ready_root/);
  assert.throws(() => proveManagedReceiptPostgres(transcript(), linuxRoot, INTERVIEW_PREP, "auto"), /unknown.*platform/);
  assert.throws(() => verify({ ...ready, dataDirectory: `${ROOT}/nonprod-postgres/data` }), /ready_directory/);
  assert.throws(() => verify({ ...ready, serverVersionNum: undefined }), /ready_keys/);
  assert.throws(() => verify({ ...ready, serverVersionNum: "170011" }), /linux_ready_server_version/);
  assert.throws(() => verify({ ...ready, serverVersionNum: 170010 }), /linux_ready_server_version/);
  assert.throws(() => verify({ ...ready, version: ready.version.replace("17.11", "17.10") }), /linux_ready_build/);
  assert.throws(() => verify({ ...ready, version: ready.version.replace("x86_64", "aarch64") }), /linux_ready_build/);
  assert.throws(() => verify({ ...ready, vector: "0.8.3" }), /linux_ready_vector/);
  assert.throws(() => verify({ ...ready, platform: "linux-arm64" }), /linux_ready_platform/);
});

test("Linux browser warmup proof requires its dedicated completion witness", () => {
  const linuxRoot = ROOT.slice("/private".length);
  const fixture = browserSample.split("\n");
  const ready = { ...JSON.parse(fixture[0]), dataDirectory: `${linuxRoot}/nonprod-postgres/data`,
    version: "PostgreSQL 17.11 (fixture build) on x86_64-pc-linux-gnu, compiled by fixture GCC, 64-bit",
    vector: "0.8.7", platform: "linux-x64", serverVersionNum: 170011 };
  const transcript = (event = ready, stdout = browserSample) => run([
    JSON.stringify(event), ...stdout.split("\n").slice(1),
  ].join("\n"));
  const proof = proveManagedReceiptPostgres(transcript(), linuxRoot, INTERVIEW_PREP_BROWSER, "linux-x64");
  assert.equal(proof.platform, "linux-x64");
  assert.equal(proof.cases[0].name, browserCase.name);
  const marker = browserCase.witnesses.at(-1);
  assert.throws(() => proveManagedReceiptPostgres(
    transcript(ready, browserSample.replace(`${marker}\n`, "")), linuxRoot, INTERVIEW_PREP_BROWSER, "linux-x64",
  ), /case_witness_block/);
  assert.throws(() => proveManagedReceiptPostgres(
    transcript(ready, browserSample.replace(marker, warmupCase.witnesses.at(-1))), linuxRoot,
    INTERVIEW_PREP_BROWSER, "linux-x64",
  ), /case_witness_block/);
});

test("rejects a missing, borrowed, duplicate or wrong-stream warmup marker", () => {
  const marker = warmupCase.witnesses.at(-1);
  assert.throws(() => proveManagedReceiptPostgres(run(warmupSample.replace(`${marker}\n`, "")), ROOT, INTERVIEW_PREP),
    /case_witness_block/);
  assert.throws(() => proveManagedReceiptPostgres(run(warmupSample.replace(marker, accepted)), ROOT, INTERVIEW_PREP),
    /case_witness_block/);
  assert.throws(() => proveManagedReceiptPostgres(run(warmupSample.replace(`running 1 test`, `running 1 test\n${marker}`)),
    ROOT, INTERVIEW_PREP), /unexpected_witness/);
  assert.throws(() => proveManagedReceiptPostgres(run(warmupSample, marker), ROOT, INTERVIEW_PREP), /wrong_witness_stream/);
  assert.throws(() => proveManagedReceiptPostgres(run(sample.replace("running 1 test", `running 1 test\n${marker}`)), ROOT),
    /unexpected_witness/);
  assert.throws(() => proveManagedReceiptPostgres(run(warmupSample.replace("4 filtered out", "0 filtered out")),
    ROOT, INTERVIEW_PREP), /case_summary_inventory/);
});

for (const [label, stdout, reason] of [
  ["missing recovery witness", sample.replace(`${recovery}\n`, ""), /case_witness_block/],
  ["wrong case witness", sample.replace(recovery, accepted), /case_witness_block/],
  ["recovery witness in another case", sample.replace(accepted, `${accepted}\n${recovery}`), /case_witness_block/],
  ["duplicate recovery witness", sample.replace("running 1 test", `running 1 test\n${recovery}`), /unexpected_witness/],
  ["reused database", sample.replace(db(2), db(1)), /case_database_reused/],
  ["reordered cases", sample.replace(`"test":"${CASES[0]}"`, `"test":"${CASES[1]}"`), /case_start_name/],
  ["missing case event", sample.replace(/^[^\n]*"event":"nonprod_postgres_case_started"[^\n]*\n/m, ""), /event_order/],
  ["partial inventory", sample.replace(inventory(names), inventory(names.filter((name) => name !== CASES[2]))), /inventory_cases/],
  ["wrong filtered count", sample.replace("3 filtered out", "2 filtered out"), /case_summary_inventory/],
  ["failed shutdown", sample.replace('"serverExited":true', '"serverExited":false'), /Expected values/],
  ["duplicate event field", sample.replace('"serverExited":true', '"serverExited":false,"serverExited":true'),
    /noncanonical_wrapper_event/],
  ["trailing controller output", `${sample}controller footer\n`, /wrapper_finished_order/],
]) {
  test(`rejects ${label}`, () => assert.throws(() => proveManagedReceiptPostgres(run(stdout), ROOT), reason));
}

test("rejects skipped body and a witness on stderr", () => {
  assert.throws(() => proveManagedReceiptPostgres(run(sample, "PostgreSQL case not selected"), ROOT), /skipped_case/);
  assert.throws(() => proveManagedReceiptPostgres(run(sample, recovery), ROOT), /wrong_witness_stream/);
});

test("renders the exact three selectors and one full inventory against the current launcher", () => {
  const base = readFileSync(new URL("./run-nonprod-postgres-tests.mjs", import.meta.url), "utf8");
  const dispatch = readFileSync(new URL("../../server/src/jobs_workflow_dispatch.rs", import.meta.url), "utf8");
  const feature = /#\[cfg\(all\(test, debug_assertions, feature = "([^"]+)"\)\)\]\n#\[path = "jobs_managed_request_start_postgres_test.rs"\]/.exec(dispatch);
  assert(feature, "owned receipt tests must be feature-gated");
  assert.equal(MANAGED_RECEIPT_CARGO_COMMAND[MANAGED_RECEIPT_CARGO_COMMAND.indexOf("--features") + 1], feature[1]);
  const rendered = renderManagedReceiptPostgresWrapper(base);
  assert(rendered.includes(`const receiptCases = Object.freeze(${JSON.stringify(CASES)});`));
  assert.equal(rendered.split('...args, "--", "--list"').length, 2);
  assert(!rendered.includes('...args, filter, "--", "--list"'));
  assert(rendered.includes('...args, name, "--", "--exact", "--nocapture", "--test-threads=1"'));
  assert.throws(() => renderManagedReceiptPostgresWrapper(rendered), /unique filtered inventory preimage/);
  assert.throws(() => renderManagedReceiptPostgresWrapper(base.replace("let passed = 0;", "let passed = 1;")),
    /unique case execution preimage/);
});

function checkRenderedAdmission(profile, t) {
  const cwd = mkdtempSync(join(tmpdir(), "bluey-receipt-wrapper-test-"));
  t.after(() => rmSync(cwd, { recursive: true }));
  const scripts = join(cwd, "jobs", "scripts");
  mkdirSync(scripts, { recursive: true });
  const wrapper = join(scripts, "run-nonprod-postgres-tests.mjs");
  const base = readFileSync(new URL("./run-nonprod-postgres-tests.mjs", import.meta.url), "utf8");
  writeFileSync(wrapper, renderManagedReceiptPostgresWrapper(base, profile));
  writeFileSync(join(scripts, "nonprod-postgres-identity.mjs"),
    readFileSync(new URL("./nonprod-postgres-identity.mjs", import.meta.url)));
  const cargo = MANAGED_RECEIPT_CARGO_COMMAND;
  const invoke = (names, command, childCwd = cwd) => {
    const argv = ["--pg-bin", cwd, ...names.flatMap((name) => ["--case-filter", name]), "--", ...command];
    // No child receives credentials or an owned test root. Even the valid
    // argv control must stop while resolving a deliberately nonexistent root.
    const child = spawnSync(process.execPath, [wrapper, ...argv], {
      cwd: childCwd, env: { BLUEY_TEST_RUN_ROOT: join(cwd, "absent") },
      encoding: "utf8", timeout: 5_000, maxBuffer: 64 * 1024,
    });
    assert.ifError(child.error);
    assert.equal(child.signal, null);
    assert.equal(child.status, 1);
    assert.equal(child.stdout, "");
    return child.stderr;
  };
  const selected = managedReceiptCases(profile).map(({ name }) => name);
  const other = profile === "baseline" ? [warmupCase.name] : CASES;
  assert.match(invoke(other, cargo), /exact managed receipt selectors required/);
  assert.match(invoke(selected.slice(0, -1), cargo), /exact managed receipt selectors required/);
  assert.match(invoke([...selected, ...other], cargo), /exact managed receipt selectors required/);
  assert.match(invoke(selected, cargo.filter((_, i) => i !== cargo.indexOf("--features")
    && i !== cargo.indexOf("--features") + 1)), /exact managed receipt library command required/);
  assert.match(invoke(selected, [...cargo.slice(0, -1), "8"]), /exact managed receipt library command required/);
  assert.match(invoke(selected, cargo, scripts), /must run from its staged source root/);
  assert.match(invoke(selected, cargo), /ENOENT[^\n]*absent/);
}
for (const profile of ["baseline", INTERVIEW_PREP, INTERVIEW_PREP_BROWSER]) {
  test(`emitted ${profile} wrapper refuses invalid inputs before creating PostgreSQL state`, (t) =>
    checkRenderedAdmission(profile, t));
}

test("only the rendered browser profile checks and forwards the outer marker before cluster creation", () => {
  const source = readFileSync(new URL("./run-nonprod-postgres-tests.mjs", import.meta.url), "utf8");
  const marker = "BLUEY_INTERVIEW_PREP_OUTER_FENCE";
  for (const profile of ["baseline", INTERVIEW_PREP]) {
    assert(!renderManagedReceiptPostgresWrapper(source, profile).includes(marker));
  }
  const browser = renderManagedReceiptPostgresWrapper(source, INTERVIEW_PREP_BROWSER);
  const requirement = browser.indexOf(`assert.equal(process.env.${marker}, "qualified-linux-interview-prep-fixture-v1"`);
  assert(requirement > browser.indexOf('assert(process.platform === "linux" && process.arch === "x64"'));
  assert(browser.indexOf(`env.${marker} = process.env.${marker};`) > requirement);
  assert(browser.indexOf('mkdirSync(work, { mode: 0o700 });') > requirement);
});

test("binds the current driver and all three Rust selectors before rendering", () => {
  const read = (path, encoding) => readFileSync(new URL(path, import.meta.url), encoding);
  const input = {
    fixture: read("../../server/src/jobs_managed_request_start_postgres_test.rs", "utf8"),
    driver: read("../runner/tests/managed-receipt-journey-driver.mjs"),
    registry: read("../../server/src/db/owned_postgres_test.rs", "utf8"),
    dispatch: read("../../server/src/jobs_workflow_dispatch.rs", "utf8"),
    browserSupport: {
      DRIVER: read("../portal/nonprod/interview-prep-fixture-driver.mjs"),
      GATEWAY: read("../portal/nonprod/interview-prep-fixture-gateway.mjs"),
      WITNESS: read("../portal/nonprod/interview-prep-witness.mjs"),
    },
  };
  assert.equal(verifyReceiptSourceBindings(input), createHash("sha256").update(input.driver).digest("hex"));
  assert.throws(() => verifyReceiptSourceBindings({ ...input, driver: Buffer.concat([input.driver, Buffer.from("\n")]) }),
    /receipt driver does not match fixture pin/);
  assert.throws(() => verifyReceiptSourceBindings({ ...input, registry: input.registry.replace(CASES[2], "") }),
    /receipt case not uniquely registered/);
  assert.throws(() => verifyReceiptSourceBindings({ ...input, fixture: input.fixture.replace(CASES[2], "") }),
    /receipt fixture selector missing or duplicated/);
  assert.throws(() => verifyReceiptSourceBindings({
    ...input, fixture: input.fixture.replace(`async fn ${CASES[2].split("::").at(-1)}()`, "async fn omitted()"),
  }), /receipt case body missing or duplicated/);
  assert.throws(() => verifyReceiptSourceBindings({ ...input, dispatch: input.dispatch.replace("integration-test-support", "other") }),
    /receipt command does not select the fixture feature/);
  assert.equal(verifyReceiptSourceBindings(input, INTERVIEW_PREP), createHash("sha256").update(input.driver).digest("hex"));
  assert.throws(() => verifyReceiptSourceBindings({ ...input, registry: input.registry.replace(warmupCase.name, "") },
    INTERVIEW_PREP), /receipt case not uniquely registered/);
  assert.throws(() => verifyReceiptSourceBindings({ ...input, fixture: input.fixture.replace(warmupCase.witnesses.at(-1), "") },
    INTERVIEW_PREP), /warmup witness not uniquely registered/);
  assert.equal(verifyReceiptSourceBindings(input, INTERVIEW_PREP_BROWSER),
    createHash("sha256").update(input.driver).digest("hex"));
  assert.throws(() => verifyReceiptSourceBindings({
    ...input, browserSupport: { ...input.browserSupport, WITNESS: Buffer.from("changed") },
  }, INTERVIEW_PREP_BROWSER), /browser support does not match fixture pin/);
  assert.throws(() => verifyReceiptSourceBindings({
    ...input, fixture: input.fixture.replace(browserCase.witnesses.at(-1), ""),
  }, INTERVIEW_PREP_BROWSER), /warmup witness not uniquely registered/);
});

test("staging refuses a warmup manifest unless the caller explicitly selects that profile", () => {
  const manifestBytes = Buffer.from(`${JSON.stringify({ profile: INTERVIEW_PREP })}\n`);
  const expectedSha256 = createHash("sha256").update(manifestBytes).digest("hex");
  assert.throws(() => stageManagedReceiptSource({ checkout: ROOT, manifestBytes, expectedSha256 }),
    /receipt manifest profile mismatch/);
});
