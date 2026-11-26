// In-memory OTP delivery for one explicit synthetic recipient. No relay, disk
// mailbox, public HTTP reader, credentials, TLS or general-purpose SMTP service.
import { createServer } from "node:net";
import { createHash } from "node:crypto";

const LIMIT = 64 * 1024;
const FROM = "no-reply@bluey-qa.test";
const error = () => new Error("local_qa_smtp_refused");
function requireThat(value) { if (!value) throw error(); }
function headers(text) {
  requireThat(text.length <= 8192 && !/[\0\r]/.test(text));
  const unfolded = text.replace(/\n[ \t]+/g, " ").split("\n"), result = new Map();
  requireThat(unfolded.length <= 40);
  for (const line of unfolded) {
    const match = /^([A-Za-z0-9-]{1,64}):[ \t]*([^\n]*)$/.exec(line);
    requireThat(match && !result.has(match[1].toLowerCase()));
    result.set(match[1].toLowerCase(), match[2].trim());
  }
  return result;
}
function splitEntity(text) {
  const index = text.indexOf("\n\n"); requireThat(index > 0);
  return { headers: headers(text.slice(0, index)), body: text.slice(index + 2) };
}
function decodePlain(entity) {
  requireThat(/^text\/plain(?:; charset="?utf-8"?)?$/i.test(entity.headers.get("content-type") || ""));
  const encoding = (entity.headers.get("content-transfer-encoding") || "7bit").toLowerCase();
  let body = entity.body;
  if (encoding === "base64") {
    const compact = body.replaceAll("\n", "");
    requireThat(/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(compact));
    const bytes = Buffer.from(compact, "base64");
    requireThat(bytes.toString("base64") === compact); body = bytes.toString("utf8"); bytes.fill(0);
  } else if (encoding === "quoted-printable") {
    body = body.replace(/=\n/g, ""); requireThat(!/=(?![A-Fa-f0-9]{2})/.test(body));
    body = body.replace(/=([A-Fa-f0-9]{2})/g, (_, hex) => String.fromCharCode(parseInt(hex, 16)));
  } else requireThat(encoding === "7bit" || encoding === "8bit");
  requireThat(!/[^\x09\x0a\x0d\x20-\x7e]/.test(body));
  return body.replaceAll("\r\n", "\n").replace(/\n$/, "");
}
function mailbox(value, expected) {
  return value === expected || /^(?:[A-Za-z0-9 ._-]{1,80}|"[A-Za-z0-9 ._-]{1,80}") <[^<>]+>$/.test(value || "")
    && value.slice(value.indexOf("<") + 1, -1) === expected;
}
function parseOtp(raw, recipient) {
  const top = splitEntity(raw);
  requireThat(mailbox(top.headers.get("from"), FROM) && mailbox(top.headers.get("to"), recipient));
  requireThat(!top.headers.has("cc") && !top.headers.has("bcc") && !top.headers.has("resent-to"));
  const subject = top.headers.get("subject"), kind = subject === "Your Bluey verification code" ? "signup"
    : subject === "Verify your Bluey Jobs application email" ? "identity" : null;
  requireThat(kind);
  const content = top.headers.get("content-type") || "";
  const boundary = /^multipart\/alternative;\s*boundary=(?:"([A-Za-z0-9=_-]{1,100})"|([A-Za-z0-9=_-]{1,100}))$/i.exec(content);
  requireThat(boundary && top.headers.get("mime-version") === "1.0");
  const separator = "--" + (boundary[1] || boundary[2]);
  const lines = top.body.split("\n"), starts = [];
  for (let i = 0; i < lines.length; i++) if (lines[i] === separator || lines[i] === separator + "--") starts.push(i);
  requireThat(starts.length === 3 && lines[starts[0]] === separator && lines[starts[1]] === separator
    && lines[starts[2]] === separator + "--" && lines.slice(starts[2] + 1).every(line => line === ""));
  const parts = [splitEntity(lines.slice(starts[0] + 1, starts[1]).join("\n")),
    splitEntity(lines.slice(starts[1] + 1, starts[2]).join("\n"))];
  const plain = parts.filter(part => /^text\/plain(?:;|$)/i.test(part.headers.get("content-type") || ""));
  requireThat(plain.length === 1 && parts.filter(part => /^text\/html(?:;|$)/i.test(part.headers.get("content-type") || "")).length === 1);
  const text = decodePlain(plain[0]);
  const prefix = kind === "signup" ? "Welcome to Bluey." : "Verify this application email for Bluey Jobs.";
  const suffix = kind === "signup" ? "If you did not request this, you can ignore this email."
    : "If you did not add this address to Bluey Jobs, you can ignore this email.";
  const introduction = prefix + "\n\nYour verification code is:\n\n";
  const match = /^([0-9]{6})\n\nThis code expires in ([1-9]|10) minutes\. (.*)$/.exec(text.slice(introduction.length));
  requireThat(text.startsWith(introduction) && match && match[3] === suffix);
  return { kind, code: match[1], ttl: Number(match[2]) * 60_000 };
}

export async function startLocalQaSmtp({ recipient, port = 0 }) {
  requireThat(typeof recipient === "string" && /^[a-z0-9][a-z0-9._+-]{0,63}@bluey-qa\.test$/.test(recipient)
    && Number.isInteger(port) && port >= 0 && port <= 65535);
  const codes = new Map(), seen = new Map(), sockets = new Set();
  const counts = { captured: 0, denied: 0, duplicate: 0 };
  let closed = false, closing;
  const server = createServer(socket => {
    if (closed || socket.remoteAddress !== "127.0.0.1" || sockets.size >= 4) { socket.destroy(); return; }
    sockets.add(socket); socket.setTimeout(10_000, () => socket.destroy());
    const deadline = setTimeout(() => socket.destroy(), 30_000);
    let buffer = Buffer.alloc(0), greeted = false, sender = false, receiver = false, inData = false;
    let data = [], size = 0, total = 0;
    const reset = () => { sender = false; receiver = false; inData = false; data = []; size = 0; };
    const reply = text => { if (!socket.destroyed) socket.write(text + "\r\n"); };
    const deny = (status = "550 Message refused") => { counts.denied++; reset(); reply(status); };
    socket.on("error", () => {});
    socket.on("close", () => { clearTimeout(deadline); buffer.fill(0); buffer = Buffer.alloc(0); reset(); sockets.delete(socket); });
    socket.on("data", bytes => {
      total += bytes.length;
      if (total > 2 * LIMIT || buffer.length + bytes.length > LIMIT + 1024) { deny("552 Limit exceeded"); socket.end(); return; }
      buffer = Buffer.concat([buffer, bytes]);
      for (;;) {
        const index = buffer.indexOf("\r\n");
        if (index === -1) { if (buffer.length > (inData ? 2048 : 512)) socket.destroy(); break; }
        const line = buffer.subarray(0, index).toString("utf8"); buffer = buffer.subarray(index + 2);
        if (line.length > (inData ? 2048 : 512) || /[\0\r\n]/.test(line)) { deny(); socket.end(); break; }
        if (inData) {
          if (line !== ".") {
            const value = line.startsWith("..") ? line.slice(1) : line;
            if (line.startsWith(".") && !line.startsWith("..")) { deny(); socket.end(); break; }
            size += Buffer.byteLength(value) + 2;
            if (size > LIMIT) { deny("552 Limit exceeded"); socket.end(); break; }
            data.push(value); continue;
          }
          try {
            const raw = data.join("\n"), message = parseOtp(raw, recipient), now = Date.now();
            for (const [key, expiry] of seen) if (expiry <= now) seen.delete(key);
            const digest = createHash("sha256").update(raw).digest("hex");
            if (seen.has(digest)) counts.duplicate++;
            else {
              requireThat(seen.size < 16); seen.set(digest, now + message.ttl);
              codes.set(message.kind, { code: message.code, expires: now + message.ttl }); counts.captured++;
            }
            reset(); reply("250 Captured locally");
          } catch { deny(); }
        } else if (/^(EHLO|HELO) [A-Za-z0-9._:[\]-]{1,255}$/.test(line)) {
          reset(); greeted = true; reply("250-localhost\r\n250 SIZE 65536");
        } else if (line === "QUIT") { reply("221 Closing"); socket.end(); break; }
        else if (line === "NOOP") reply("250 OK");
        else if (line === "RSET" && greeted) { reset(); reply("250 Reset"); }
        else if (line.startsWith("MAIL FROM:")) {
          reset();
          const match = /^MAIL FROM:<([^<>]+)>(?: SIZE=([1-9][0-9]{0,5}))?$/.exec(line);
          if (!greeted || !match || match[1] !== FROM || Number(match[2] || 1) > LIMIT) deny();
          else { sender = true; reply("250 Sender accepted"); }
        } else if (line.startsWith("RCPT TO:")) {
          if (!sender || receiver || line !== `RCPT TO:<${recipient}>`) deny();
          else { receiver = true; reply("250 Recipient accepted"); }
        } else if (line === "DATA" && sender && receiver) { inData = true; reply("354 End with dot"); }
        else deny("502 Command refused");
      }
    });
    reply("220 localhost Local QA OTP sink");
  });
  await new Promise((resolve, reject) => { server.once("error", reject); server.listen(port, "127.0.0.1", () => { server.off("error", reject); resolve(); }); });
  const address = server.address(); requireThat(address && typeof address !== "string" && address.address === "127.0.0.1");
  const api = { host: "127.0.0.1", port: address.port, counters: () => ({ ...counts }), close: () => {
    if (!closing) { closed = true; codes.clear(); seen.clear(); for (const socket of sockets) socket.destroy();
      closing = new Promise(resolve => server.close(resolve)); }
    return closing;
  } };
  Object.defineProperty(api, "takeCode", { value: kind => {
    requireThat(kind === "signup" || kind === "identity");
    const value = codes.get(kind); codes.delete(kind);
    return !closed && value && value.expires > Date.now() ? value.code : null;
  } });
  Object.defineProperty(api, "env", { value: Object.freeze({ BLUEY_MAIL_TRANSPORT: "smtp", BLUEY_SMTP_HOST: "127.0.0.1",
    BLUEY_SMTP_PORT: String(address.port), BLUEY_SMTP_STARTTLS: "false", BLUEY_SMTP_FROM: `Bluey Local QA <${FROM}>`,
    BLUEY_DEV_LOG_AUTH_LINKS: "0" }) });
  return Object.freeze(api);
}
