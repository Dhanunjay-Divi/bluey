import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { focusedVitestFailureEvidence, runFocusedVitest } from "./run-focused-vitest.mjs";
import { FOCUSED_RUNNER_CASES } from "./verify-focused-vitest-report.mjs";

const root = process.env.BLUEY_TEST_RUN_ROOT;

test("binds a freshly produced report to a real child exit and rejects failed terminal or report custody", () => {
  assert(root, "run this test through the canonical launcher with a private run parent");
  const jobs = join(root, "runner-gate-source/jobs");
  const runner = join(jobs, "runner");
  const vitest = join(jobs, "node_modules/vitest");
  mkdirSync(runner, { recursive: true });
  mkdirSync(vitest, { recursive: true });
  writeFileSync(join(vitest, "package.json"), JSON.stringify({ version: "4.1.12" }), { flag: "wx" });
  const cli = join(vitest, "vitest.mjs");
  const synthetic = `
    import assert from "node:assert/strict";
    import { existsSync, writeFileSync, symlinkSync } from "node:fs";
    import { join } from "node:path";
    const files = ${JSON.stringify(FOCUSED_RUNNER_CASES)};
    const path = process.argv.at(-1).slice("--outputFile=".length);
    assert(!existsSync(path));
    assert.deepEqual(process.argv.slice(2, -1), ["run", ...Object.keys(files), "--pool=threads",
      "--maxWorkers=2", "--reporter=default", "--reporter=json"]);
    assert.equal(process.cwd(), ${JSON.stringify(runner)});
    assert.equal(process.env.TMPDIR, ${JSON.stringify(join(root, "tmp"))});
    assert.equal(process.env.BLUEY_FOCUSED_TEST_SENTINEL, undefined);
    assert.equal(process.env.BLUEY_DATABASE_URL, undefined);
    const report = {
      success:true, numTotalTests:73, numPassedTests:73, numFailedTests:0, numPendingTests:0, numTodoTests:0,
      numTotalTestSuites:8, numPassedTestSuites:8, numFailedTestSuites:0, numPendingTestSuites:0,
      snapshot:{failure:false}, testResults:Object.entries(files).map(([file,count]) => ({
        name:join(process.cwd(),file), status:"passed", message:"",
        assertionResults:Array.from({length:count}, (_,i)=>({status:"passed",failureMessages:[],fullName:"case "+i})),
      })),
    };
  `;
  const prior = process.env.BLUEY_FOCUSED_TEST_SENTINEL;
  process.env.BLUEY_FOCUSED_TEST_SENTINEL = "synthetic-parent-only";
  try {
    writeFileSync(cli, `${synthetic}\nwriteFileSync(path, JSON.stringify(report), {flag:"wx"});`, { flag: "wx" });
    const first = runFocusedVitest();
    assert.equal(first.passed, 73);
    assert.deepEqual(first.terminal, { code: 0, signal: null, error: null });
    assert.notEqual(runFocusedVitest().args.at(-1), first.args.at(-1));
    const diagnostic = "synthetic-only diagnostic";
    writeFileSync(cli, `${synthetic}\nwriteFileSync(path, JSON.stringify(report));
      console.error(${JSON.stringify(diagnostic)}); process.exitCode=7;`);
    assert.throws(() => runFocusedVitest(), (error) => {
      assert.equal(error.message, "vitest_terminal");
      assert.deepEqual(Object.keys(error), []);
      assert(!error.stack.includes(diagnostic));
      assert.equal(JSON.stringify(error), "{}");
      const evidence = focusedVitestFailureEvidence(error);
      assert.deepEqual(evidence.terminal, { code: 7, signal: null, error: null });
      assert.equal(evidence.streamsComplete, true);
      assert.equal(evidence.streams.stderr.bytes.toString(), `${diagnostic}\n`);
      assert.equal(evidence.reportStatus, "captured");
      assert.equal(evidence.verified, false);
      assert.equal(JSON.parse(evidence.report.toString()).numPassedTests, 73);
      evidence.report.fill(0);
      evidence.streams.stderr.bytes.fill(0);
      assert.equal(focusedVitestFailureEvidence(error).streams.stderr.bytes.toString(), `${diagnostic}\n`);
      assert.equal(JSON.parse(focusedVitestFailureEvidence(error).report.toString()).numPassedTests, 73);
      return true;
    });
    writeFileSync(cli, `${synthetic}\nwriteFileSync(path+".other", JSON.stringify(report)); symlinkSync(path+".other",path);`);
    assert.throws(() => runFocusedVitest(), (error) => {
      assert.equal(error.message, "vitest_report_custody_rejected");
      assert.equal(focusedVitestFailureEvidence(error).report, undefined);
      return true;
    });
    writeFileSync(cli, `${synthetic}\nwriteFileSync(path, JSON.stringify(report)); console.error("inert warning");`);
    assert.throws(() => runFocusedVitest(), /vitest_report_verification/);
    assert.equal(focusedVitestFailureEvidence(new Error("other")), undefined);
  } finally {
    if (prior === undefined) delete process.env.BLUEY_FOCUSED_TEST_SENTINEL;
    else process.env.BLUEY_FOCUSED_TEST_SENTINEL = prior;
  }
});
