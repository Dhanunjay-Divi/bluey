import assert from "node:assert/strict";
import test from "node:test";
import { createConnection } from "node:net";
import { startLocalQaSmtp } from "./local-qa-smtp.mjs";

const recipient = "synthetic@bluey-qa.test", from = "no-reply@bluey-qa.test";
function message({ kind = "signup", encoding = "7bit", code = "012345", extra = "", bodyExtra = "" } = {}) {
  const title = kind === "signup" ? "Your Bluey verification code" : "Verify your Bluey Jobs application email";
  const intro = kind === "signup" ? "Welcome to Bluey." : "Verify this application email for Bluey Jobs.";
  const suffix = kind === "signup" ? "If you did not request this, you can ignore this email."
    : "If you did not add this address to Bluey Jobs, you can ignore this email.";
  let text = `${intro}\r\n\r\nYour verification code is:\r\n\r\n${code}\r\n\r\nThis code expires in 10 minutes. ${suffix}${bodyExtra}`;
  if (encoding === "base64") text = Buffer.from(text).toString("base64");
  else if (encoding === "quoted-printable") text = text.replace("verification", "verifica=\r\ntion").replaceAll(" ", "=20");
  // lettre/email-encoding quotes a display name containing spaces.
  return `From: "Bluey Local QA" <${from}>\r\nTo: ${recipient}\r\nSubject: ${title}\r\nMIME-Version: 1.0\r\n${extra}`
    + 'Content-Type: multipart/alternative;\r\n boundary="qa-boundary"\r\n\r\n'
    + `--qa-boundary\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: ${encoding}\r\n\r\n${text}\r\n`
    + '--qa-boundary\r\nContent-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: 7bit\r\n\r\n<p>synthetic</p>\r\n--qa-boundary--\r\n';
}
async function client(t, sink) {
  const socket = createConnection({ host: sink.host, port: sink.port });
  t.after(() => socket.destroy());
  let buffer = "", current = [], ended = false;
  const messages = [], waiters = [];
  const flush = () => { while (messages.length && waiters.length) waiters.shift().resolve(messages.shift()); };
  socket.on("data", bytes => {
    buffer += bytes.toString("utf8");
    while (buffer.includes("\r\n")) {
      const index = buffer.indexOf("\r\n"), line = buffer.slice(0, index); buffer = buffer.slice(index + 2);
      current.push(line);
      if (/^[0-9]{3} /.test(line)) { messages.push(current.join("\n")); current = []; flush(); }
    }
  });
  const end = () => { ended = true; while (waiters.length) waiters.shift().reject(new Error("synthetic_smtp_closed")); };
  socket.on("error", end); socket.on("close", end);
  const next = () => {
    if (messages.length) return Promise.resolve(messages.shift());
    if (ended) return Promise.reject(new Error("synthetic_smtp_closed"));
    return new Promise((resolve, reject) => waiters.push({ resolve, reject }));
  };
  assert.match(await next(), /^220 /);
  return { next, command: async line => { socket.write(line + "\r\n"); return next(); },
    data: async (raw, chunked = false) => {
      if (chunked) for (let i = 0; i < raw.length; i += 7) socket.write(raw.slice(i, i + 7));
      else socket.write(raw);
      socket.write(".\r\n"); return next();
    } };
}
async function envelope(t, sink) {
  const smtp = await client(t, sink);
  assert.match(await smtp.command("EHLO synthetic.local"), /^250-localhost\n250 SIZE/);
  assert.match(await smtp.command(`MAIL FROM:<${from}>`), /^250 /);
  assert.match(await smtp.command(`RCPT TO:<${recipient}>`), /^250 /);
  assert.match(await smtp.command("DATA"), /^354 /);
  return smtp;
}
async function fixture(t) {
  const sink = await startLocalQaSmtp({ recipient });
  t.after(() => sink.close()); return sink;
}

test("ordinary synthetic signup/identity MIME deliveries retain leading-zero codes only in memory", async t => {
  const sink = await fixture(t);
  for (const kind of ["signup", "identity"]) {
    for (const encoding of ["7bit", "quoted-printable", "base64"]) {
      const smtp = await envelope(t, sink);
      const code = String(1230 + sink.counters().captured).padStart(6, "0");
      assert.match(await smtp.data(message({ kind, encoding, code }), true), /^250 /);
      assert(!JSON.stringify(sink).includes(code));
      assert.equal(sink.takeCode(kind), code); assert.equal(sink.takeCode(kind), null);
      assert.match(await smtp.command("QUIT"), /^221 /);
    }
  }
  assert.equal(sink.counters().captured, 6);
});

test("foreign/additional recipients, wrong senders and unsupported commands never capture", async t => {
  const sink = await fixture(t), smtp = await client(t, sink);
  assert.match(await smtp.command("DATA"), /^502 /);
  assert.match(await smtp.command("EHLO synthetic.local"), /^250-/);
  assert.match(await smtp.command("AUTH LOGIN"), /^502 /);
  assert.match(await smtp.command("STARTTLS"), /^502 /);
  assert.match(await smtp.command("MAIL FROM:<external@example.com>"), /^550 /);
  assert.match(await smtp.command(`MAIL FROM:<${from}>`), /^250 /);
  assert.match(await smtp.command("RCPT TO:<external@example.com>"), /^550 /);
  assert.match(await smtp.command(`MAIL FROM:<${from}>`), /^250 /);
  assert.match(await smtp.command(`RCPT TO:<${recipient}>`), /^250 /);
  assert.match(await smtp.command(`RCPT TO:<${recipient}>`), /^550 /);
  assert.match(await smtp.command("DATA"), /^502 /);
  assert.equal(sink.counters().captured, 0); assert.equal(sink.takeCode("signup"), null);
});

test("unsupported/ambiguous MIME and incomplete DATA do not expose a code", async t => {
  const sink = await fixture(t);
  const cases = [message({ extra: `To: ${recipient}\r\n` }), message({ extra: "Cc: other@bluey-qa.test\r\n" }),
    message().replace(`"Bluey Local QA" <${from}>`, '"External Sender" <external@example.com>'),
    message({ code: "1234567" }), message({ bodyExtra: " extra" }), message({ encoding: "unknown" }),
    message().replace("Your Bluey verification code", "Reset your Bluey password"),
    message().replace("--qa-boundary--", "--wrong-boundary--")];
  for (const raw of cases) {
    const smtp = await envelope(t, sink);
    assert.match(await smtp.data(raw), /^550 /);
    assert.match(await smtp.command("QUIT"), /^221 /);
  }
  const pending = await envelope(t, sink); // disconnect before DATA terminator via test cleanup
  assert.equal(sink.takeCode("signup"), null); assert.equal(sink.counters().captured, 0);
  assert(pending);
});

test("exact duplicate DATA is an idempotent delivery, not a reusable captured code", async t => {
  const sink = await fixture(t), raw = message();
  assert.match(await (await envelope(t, sink)).data(raw), /^250 /);
  assert.equal(sink.takeCode("signup"), "012345");
  assert.match(await (await envelope(t, sink)).data(raw), /^250 /);
  assert.equal(sink.takeCode("signup"), null);
  assert.deepEqual(sink.counters(), { captured: 1, denied: 0, duplicate: 1 });
});

test("close destroys owned sessions and clears private codes without sending mail", async t => {
  const sink = await fixture(t);
  assert.match(await (await envelope(t, sink)).data(message()), /^250 /);
  await sink.close(); await sink.close();
  assert.equal(sink.takeCode("signup"), null);
  await assert.rejects(client(t, sink));
});

test("configuration refuses non-synthetic recipients and invalid ports", async () => {
  for (const config of [{ recipient: "real@example.com" }, { recipient, port: -1 }, { recipient, port: 65536 }]) {
    await assert.rejects(startLocalQaSmtp(config), /local_qa_smtp_refused/);
  }
});

test("oversized DATA is bounded and never captured", async t => {
  const sink = await fixture(t), smtp = await envelope(t, sink);
  const raw = Array.from({ length: 600 }, () => "x".repeat(128) + "\r\n").join("");
  const result = await smtp.data(raw).catch(() => "closed");
  assert(result === "closed" || /^552 /.test(result));
  assert.equal(sink.counters().captured, 0); assert(sink.counters().denied > 0);
});
