import assert from "node:assert/strict";
import test from "node:test";
import { focusedPreparationPlan, prepareAndRunFocusedRunner } from "./prepare-focused-runner.mjs";

const root = "/private/tmp/bluey-test-runs-inert/run.1794548493.32694.31867127940";
const npm = "/private/tmp/inert-tool/lib/node_modules/npm/bin/npm-cli.js";

test("uses only the offline install and ordered automation build and runner no-emit check", () => {
  const jobs = `${root}/runner-gate-source/jobs`;
  const tsc = `${jobs}/node_modules/typescript/bin/tsc`;
  assert.deepEqual(focusedPreparationPlan(root, npm), [
    { name: "offline_install", cwd: jobs, args: [npm, "ci", "--offline", "--ignore-scripts",
      "--include=dev", "--no-audit", "--no-fund"], timeout: 120_000 },
    { name: "automation_build", cwd: `${jobs}/automation`, args: [tsc, "-p", "tsconfig.json"], timeout: 60_000 },
    { name: "runner_typecheck", cwd: `${jobs}/runner`, args: [tsc, "-p", "tsconfig.json", "--noEmit"], timeout: 60_000 },
  ]);
  assert.throws(() => focusedPreparationPlan("/tmp", npm), /focused_prepare_root/);
});

test("refuses to prepare without a private evidence writer and owned run root", async () => {
  await assert.rejects(prepareAndRunFocusedRunner({}), /focused_prepare_evidence_writer/);
  const previous = process.env.BLUEY_TEST_RUN_ROOT;
  delete process.env.BLUEY_TEST_RUN_ROOT;
  try {
    await assert.rejects(prepareAndRunFocusedRunner({ recordPrivateStep: assert.fail }), /focused_prepare_root/);
  } finally {
    if (previous !== undefined) process.env.BLUEY_TEST_RUN_ROOT = previous;
  }
});
