// Fixed-purpose entry for the sealed Linux signup + sample-resume + review-first
// mock-receipt actor. Admission and browser ownership stay in c2c-native.
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { runLocalQaLinuxBrowserSampleMockReceiptNative } from "./local-qa-linux-c2c-native.mjs";

const REFUSED = "local_qa_linux_c2c_native_refused";
const UNCLOSED = "local_qa_linux_c2c_native_unclosed";
const MAX_WALL_MS = 480_000;
const SHA256 = /^[0-9a-f]{64}$/;
const TREE = /^[0-9a-f]{40}$/;

const whole = (regex, value) => typeof value === "string" && regex.exec(value)?.[0] === value;
const absoluteCanonicalPath = value => typeof value === "string" && value !== "/"
  && resolve(value) === value && !/[\0\r\n]/.test(value);
const refuse = () => { throw new Error(REFUSED); };
const need = value => { if (!value) refuse(); };

/** Pure argv parser. Physical custody checks remain in the guarded native runner. */
export function parseLocalQaLinuxSignupMockReceiptNativeArgs(args) {
  try {
    need(Array.isArray(args) && args.length === 4 && args[0] === "--spec"
      && absoluteCanonicalPath(args[1]) && args[2] === "--sha256" && whole(SHA256, args[3]));
    return Object.freeze({ specFile: args[1], specSha256: args[3] });
  } catch {
    refuse();
  }
}

/** Accept only the complete exact mock-receipt receipt emitted by the fixed actor. */
export function readLocalQaLinuxSignupMockReceipt(receipt) {
  need(receipt && Object.getPrototypeOf(receipt) === Object.prototype
    && Object.keys(receipt).sort().join(",") === "event,evidence,sourceInventorySha256,sourceTree"
    && receipt.event === "local_qa_linux_browser_sample_mock_receipt_completed"
    && whole(TREE, receipt.sourceTree) && whole(SHA256, receipt.sourceInventorySha256));
  const data = receipt.evidence;
  need(data && Object.getPrototypeOf(data) === Object.prototype
    && Object.keys(data).sort().join(",")
      === "applicationCount,browserSignup,candidateWorkspace,mockReceiptObserved,sampleResumeImported,signupMailCount,submittedCount"
    && data.browserSignup === true && data.candidateWorkspace === true
    && data.sampleResumeImported === true && data.mockReceiptObserved === true
    && data.signupMailCount === 1 && data.applicationCount === 1 && data.submittedCount === 1);
  return Object.freeze({
    event: receipt.event,
    sourceTree: receipt.sourceTree,
    sourceInventorySha256: receipt.sourceInventorySha256,
    evidence: Object.freeze({
      browserSignup: true,
      candidateWorkspace: true,
      signupMailCount: 1,
      applicationCount: 1,
      submittedCount: 1,
      sampleResumeImported: true,
      mockReceiptObserved: true,
    }),
  });
}

export async function runLocalQaLinuxSignupMockReceiptNative({ specFile, specSha256, signal }) {
  try {
    const parsed = parseLocalQaLinuxSignupMockReceiptNativeArgs(["--spec", specFile, "--sha256", specSha256]);
    const receipt = await runLocalQaLinuxBrowserSampleMockReceiptNative({ ...parsed, signal });
    return readLocalQaLinuxSignupMockReceipt(receipt);
  } catch (error) {
    throw new Error(/unclosed$/i.test(error?.message ?? "") ? UNCLOSED : REFUSED);
  }
}

async function main(args) {
  const controller = new AbortController();
  const interrupt = () => controller.abort();
  process.once("SIGINT", interrupt);
  process.once("SIGTERM", interrupt);
  try {
    const parsed = parseLocalQaLinuxSignupMockReceiptNativeArgs(args);
    const signal = AbortSignal.any([controller.signal, AbortSignal.timeout(MAX_WALL_MS)]);
    const receipt = await runLocalQaLinuxSignupMockReceiptNative({ ...parsed, signal });
    process.stdout.write(`${JSON.stringify(receipt)}\n`);
  } catch (error) {
    process.stderr.write(`${error?.message === UNCLOSED ? UNCLOSED : REFUSED}\n`);
    process.exitCode = 1;
  } finally {
    process.removeListener("SIGINT", interrupt);
    process.removeListener("SIGTERM", interrupt);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main(process.argv.slice(2));
}
