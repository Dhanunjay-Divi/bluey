// Pipe-controlled, one-shot SMTP sink for the synthetic saved-account fixture.
// It deliberately has no configuration surface, file output, or HTTP listener.
import { startLocalQaSmtp } from "./local-qa-smtp.mjs";

const RECIPIENT = "candidate@bluey-qa.test";
const TIMEOUT_MS = 15_000;
const MAX_STDIN_BYTES = 16;

let sink;
let settled = false;
let timeout;

function write(stream, value) {
  return new Promise((resolve, reject) => stream.write(value, error => error ? reject(error) : resolve()));
}

async function closeSink() {
  if (timeout) clearTimeout(timeout);
  timeout = undefined;
  if (sink) await sink.close();
}

async function fail() {
  if (settled) return;
  settled = true;
  try {
    await closeSink();
  } finally {
    // Never include an OTP or input bytes in diagnostics.
    await write(process.stderr, "local_qa_smtp_test_child_failed\n").catch(() => undefined);
    process.exitCode = 1;
  }
}

async function complete() {
  if (settled) return;
  settled = true;
  try {
    const counters = sink.counters();
    if (counters.captured !== 1 || counters.denied !== 0 || counters.duplicate !== 0) throw new Error("smtp_capture_invalid");
    const code = sink.takeCode("signup");
    if (!/^[0-9]{6}$/.test(code || "") || sink.takeCode("signup") !== null) throw new Error("smtp_code_invalid");
    await closeSink();
    await write(process.stdout, `done ${code}\n`);
    process.exitCode = 0;
  } catch {
    settled = false;
    await fail();
  }
}

async function main() {
  sink = await startLocalQaSmtp({ recipient: RECIPIENT });
  await write(process.stdout, `ready ${sink.port}\n`);
  timeout = setTimeout(() => { void fail(); }, TIMEOUT_MS);
  timeout.unref();

  let input = "";
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", chunk => {
    if (settled || typeof chunk !== "string" || Buffer.byteLength(input) + Buffer.byteLength(chunk) > MAX_STDIN_BYTES) {
      void fail();
      return;
    }
    input += chunk;
    if (!"take\n".startsWith(input)) void fail();
  });
  process.stdin.once("end", () => { void (input === "take\n" ? complete() : fail()); });
  process.stdin.once("error", () => { void fail(); });
}

for (const signal of ["HUP", "INT", "TERM"]) process.once(signal, () => { void fail(); });
main().catch(() => { void fail(); });
