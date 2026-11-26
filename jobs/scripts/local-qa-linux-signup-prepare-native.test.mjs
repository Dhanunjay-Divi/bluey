import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

import {
  parseLocalQaLinuxSignupPrepareNativeArgs,
  readLocalQaLinuxSignupPrepareReceipt,
} from "./local-qa-linux-signup-prepare-native.mjs";

const SCRIPT = fileURLToPath(new URL("./local-qa-linux-signup-prepare-native.mjs", import.meta.url));
const SPEC = "/tmp/bluey-test-runs-a1b2/run.1795250653.12345/spec.json";
const SHA = "a".repeat(64);

function receipt(evidence = {}) {
  return {
    event: "local_qa_linux_browser_sample_prepare_completed",
    sourceTree: "b".repeat(40),
    sourceInventorySha256: SHA,
    evidence: {
      browserSignup: true,
      candidateWorkspace: true,
      signupMailCount: 1,
      applicationCount: 1,
      submittedCount: 0,
      sampleResumeImported: true,
      reviewFirstPrepared: true,
      ...evidence,
    },
  };
}

test("pure parser accepts only the fixed digest-bound spec argv", () => {
  assert.deepEqual(
    parseLocalQaLinuxSignupPrepareNativeArgs(["--spec", SPEC, "--sha256", SHA]),
    { specFile: SPEC, specSha256: SHA },
  );
  for (const args of [
    ["--mode", "signup_sample_prepare", "--spec", SPEC, "--sha256", SHA],
    ["--spec", "relative.json", "--sha256", SHA],
    ["--spec", "/", "--sha256", SHA],
    ["--spec", `${SPEC}\n`, "--sha256", SHA],
    ["--spec", SPEC, "--sha256", "A".repeat(64)],
  ]) {
    assert.throws(() => parseLocalQaLinuxSignupPrepareNativeArgs(args), /local_qa_linux_c2c_native_refused/);
  }
});

test("invalid actor invocation exits before attempting the native runner", () => {
  const result = spawnSync(process.execPath, [SCRIPT, "--mode", "signup_sample_prepare"], {
    encoding: "utf8", timeout: 1_500, env: {},
  });
  assert.equal(result.error, undefined);
  assert.equal(result.status, 1);
  assert.equal(result.stdout, "");
  assert.equal(result.stderr, "local_qa_linux_c2c_native_refused\n");
});

test("receipt is closed to exactly one candidate application and no submission", () => {
  const accepted = receipt();
  assert.deepEqual(readLocalQaLinuxSignupPrepareReceipt(accepted), accepted);
  for (const changed of [
    receipt({ applicationCount: 0 }),
    receipt({ applicationCount: 2 }),
    receipt({ submittedCount: 1 }),
    receipt({ reviewFirstPrepared: false }),
    receipt({ accessToken: "synthetic-test-token" }),
  ]) {
    assert.throws(() => readLocalQaLinuxSignupPrepareReceipt(changed), /local_qa_linux_c2c_native_refused/);
  }
});
