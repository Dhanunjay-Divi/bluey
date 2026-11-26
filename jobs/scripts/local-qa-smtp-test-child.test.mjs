import assert from "node:assert/strict";
import { createConnection } from "node:net";
import { spawn } from "node:child_process";
import { once } from "node:events";
import test from "node:test";

const childPath = new URL("./local-qa-smtp-test-child.mjs", import.meta.url);
const recipient = "candidate@bluey-qa.test";
const from = "no-reply@bluey-qa.test";

function startChild() {
  const child = spawn(process.execPath, [childPath.pathname], {
    env: {},
    stdio: ["pipe", "pipe", "pipe"],
  });
  child.stdoutText = "";
  child.stdout.on("data", chunk => { child.stdoutText += chunk.toString("utf8"); });
  return child;
}

async function readReady(child) {
  for (;;) {
    const match = /^ready ([1-9][0-9]{0,4})\n$/.exec(child.stdoutText);
    if (match) return Number(match[1]);
    if (child.stdoutText.length > 32) break;
    await Promise.race([once(child.stdout, "data"), once(child, "close")]);
  }
  throw new Error("missing_ready");
}

function message() {
  return `From: Bluey Local QA <${from}>\r\nTo: ${recipient}\r\nSubject: Your Bluey verification code\r\nMIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary="qa-boundary"\r\n\r\n--qa-boundary\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nWelcome to Bluey.\r\n\r\nYour verification code is:\r\n\r\n012345\r\n\r\nThis code expires in 10 minutes. If you did not request this, you can ignore this email.\r\n--qa-boundary\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>synthetic</p>\r\n--qa-boundary--\r\n`;
}

async function deliver(port) {
  const socket = createConnection({ host: "127.0.0.1", port });
  let buffer = "";
  const lines = [];
  socket.setTimeout(5_000, () => socket.destroy(new Error("smtp_timeout")));
  socket.on("data", chunk => {
    buffer += chunk.toString("utf8");
    for (;;) {
      const index = buffer.indexOf("\r\n");
      if (index < 0) return;
      lines.push(buffer.slice(0, index));
      buffer = buffer.slice(index + 2);
    }
  });
  const next = async () => {
    while (!lines.length) await Promise.race([once(socket, "data"), once(socket, "error").then(([error]) => Promise.reject(error))]);
    return lines.shift();
  };
  await once(socket, "connect");
  assert.match(await next(), /^220 /);
  const command = async value => { socket.write(`${value}\r\n`); return next(); };
  assert.match(await command("EHLO qa.local"), /^250-localhost$/);
  assert.match(await next(), /^250 SIZE /);
  assert.match(await command(`MAIL FROM:<${from}>`), /^250 /);
  assert.match(await command(`RCPT TO:<${recipient}>`), /^250 /);
  assert.match(await command("DATA"), /^354 /);
  socket.write(message()); socket.write(".\r\n");
  assert.match(await next(), /^250 /);
  socket.end();
}

async function closed(child) {
  const [code, signal] = await once(child, "close");
  return { code, signal };
}

test("SMTP child exposes one OTP only after a real loopback delivery", { timeout: 8000 }, async () => {
  const child = startChild();
  const stderr = [];
  child.stderr.on("data", chunk => stderr.push(chunk));
  const port = await readReady(child);
  await deliver(port);
  child.stdin.end("take\n");
  const result = await closed(child);
  assert.deepEqual(result, { code: 0, signal: null });
  assert.match(child.stdoutText, /^ready [1-9][0-9]{0,4}\ndone 012345\n$/);
  assert.equal(Buffer.concat(stderr).toString("utf8"), "");
});

for (const [name, input] of [["EOF", null], ["malformed input", "take\nextra"]]) {
  test(`SMTP child fails closed on ${name}`, { timeout: 8000 }, async () => {
    const child = startChild();
    const stderr = [];
    child.stderr.on("data", chunk => stderr.push(chunk));
    await readReady(child);
    if (input === null) child.stdin.end();
    else child.stdin.end(input);
    const result = await closed(child);
    assert.deepEqual(result, { code: 1, signal: null });
    assert.match(child.stdoutText, /^ready [1-9][0-9]{0,4}\n$/);
    assert.equal(Buffer.concat(stderr).toString("utf8"), "local_qa_smtp_test_child_failed\n");
  });
}
