import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { parseLocalQaLinuxSignupSampleNativeArgs,
  readLocalQaLinuxSignupSampleReceipt } from "./local-qa-linux-signup-sample-native.mjs";

const SCRIPT = fileURLToPath(new URL("./local-qa-linux-signup-sample-native.mjs", import.meta.url));
const SPEC = "/tmp/bluey-test-runs-a1b2/run.1795250653.12345/spec.json";
const SHA = "a".repeat(64);

test("pure parser accepts only the fixed spec/hash argv", () => {
  assert.deepEqual(
    parseLocalQaLinuxSignupSampleNativeArgs(["--spec", SPEC, "--sha256", SHA]),
    { specFile: SPEC, specSha256: SHA },
  );
  for (const args of [
    ["--mode", "signup_sample_resume", "--spec", SPEC, "--sha256", SHA],
    ["--spec", "relative.json", "--sha256", SHA],
    ["--spec", "/", "--sha256", SHA],
    ["--spec", `${SPEC}\n`, "--sha256", SHA],
    ["--spec", SPEC, "--sha256", "A".repeat(64)],
  ]) {
    assert.throws(() => parseLocalQaLinuxSignupSampleNativeArgs(args), /local_qa_linux_c2c_native_refused/);
  }
});

test("invalid actor invocation exits before attempting the native runner", () => {
  const result = spawnSync(process.execPath, [SCRIPT, "--mode", "signup_sample_resume"], {
    encoding: "utf8",
    timeout: 1_500,
    env: {},
  });
  assert.equal(result.error, undefined);
  assert.equal(result.status, 1);
  assert.equal(result.stdout, "");
  assert.equal(result.stderr, "local_qa_linux_c2c_native_refused\n");
});

test("actor emits only closed non-secret signup and import evidence", () => {
  const evidence = { browserSignup: true, candidateWorkspace: true, signupMailCount: 1,
    applicationCount: 0, submittedCount: 0, sampleResumeImported: true };
  const receipt = { event: "local_qa_linux_browser_sample_resume_completed", sourceTree: "b".repeat(40),
    sourceInventorySha256: SHA, evidence };
  assert.deepEqual(readLocalQaLinuxSignupSampleReceipt(receipt), receipt);
  for (const changed of [{ ...evidence, accessToken: "synthetic-test-token" }, { ...evidence, submittedCount: 1 },
    { ...evidence, signupMailCount: true }]) {
    assert.throws(() => readLocalQaLinuxSignupSampleReceipt({ ...receipt, evidence: changed }),
      /local_qa_linux_c2c_native_refused/);
  }
});
