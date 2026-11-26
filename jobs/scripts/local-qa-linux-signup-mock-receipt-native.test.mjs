import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

import {
  parseLocalQaLinuxSignupMockReceiptNativeArgs,
  readLocalQaLinuxSignupMockReceipt,
} from "./local-qa-linux-signup-mock-receipt-native.mjs";

const SCRIPT = fileURLToPath(new URL("./local-qa-linux-signup-mock-receipt-native.mjs", import.meta.url));
const SPEC = "/tmp/bluey-test-runs-a1b2/run.1795250653.12345/spec.json";
const SHA = "a".repeat(64);

function receipt(evidence = {}) {
  return {
    event: "local_qa_linux_browser_sample_mock_receipt_completed",
    sourceTree: "b".repeat(40),
    sourceInventorySha256: SHA,
    evidence: {
      browserSignup: true,
      candidateWorkspace: true,
      signupMailCount: 1,
      applicationCount: 1,
      submittedCount: 1,
      sampleResumeImported: true,
      mockReceiptObserved: true,
      ...evidence,
    },
  };
}

test("pure parser accepts only the fixed digest-bound spec argv", () => {
  assert.deepEqual(
    parseLocalQaLinuxSignupMockReceiptNativeArgs(["--spec", SPEC, "--sha256", SHA]),
    { specFile: SPEC, specSha256: SHA },
  );
  for (const args of [
    ["--mode", "signup_sample_mock_receipt", "--spec", SPEC, "--sha256", SHA],
    ["--spec", "relative.json", "--sha256", SHA],
    ["--spec", "/", "--sha256", SHA],
    ["--spec", `${SPEC}\n`, "--sha256", SHA],
    ["--spec", SPEC, "--sha256", "A".repeat(64)],
  ]) assert.throws(() => parseLocalQaLinuxSignupMockReceiptNativeArgs(args), /local_qa_linux_c2c_native_refused/);
});

test("invalid actor invocation exits before attempting the native runner", () => {
  const result = spawnSync(process.execPath, [SCRIPT, "--mode", "signup_sample_mock_receipt"], {
    encoding: "utf8", timeout: 1_500, env: {},
  });
  assert.equal(result.error, undefined);
  assert.equal(result.status, 1);
  assert.equal(result.stdout, "");
  assert.equal(result.stderr, "local_qa_linux_c2c_native_refused\n");
});

test("receipt is closed to one signed-up candidate, one prepared application, and one mock submission", () => {
  const accepted = receipt();
  assert.deepEqual(readLocalQaLinuxSignupMockReceipt(accepted), accepted);
  for (const changed of [
    receipt({ applicationCount: 0 }),
    receipt({ applicationCount: 2 }),
    receipt({ submittedCount: 0 }),
    receipt({ mockReceiptObserved: false }),
    receipt({ workerToken: "out-of-scope" }),
  ]) assert.throws(() => readLocalQaLinuxSignupMockReceipt(changed), /local_qa_linux_c2c_native_refused/);
});
