// Local QA resume-object subset only; not S3, R2, a production store or a
// provider qualification. The root is trusted to the current OS user. Node's
// path APIs cannot provide hostile-same-user openat/rename race guarantees.
import { createHash, createHmac, randomBytes, timingSafeEqual } from "node:crypto";
import { createServer } from "node:http";
import { constants, closeSync, fstatSync, fsyncSync, lstatSync, openSync, readFileSync,
  readdirSync, realpathSync, renameSync, unlinkSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join, resolve } from "node:path";

const MAX_OBJECT = 10 * 1024 * 1024;
const MAX_TOTAL = 128 * 1024 * 1024;
const MAX_RECORDS = 64;
const MAX_HEADER = 4096;
const EMPTY = createHash("sha256").update("").digest("hex");
const SIGNED = "host;x-amz-content-sha256;x-amz-date";
const UUID = "[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";
export const LOCAL_QA_OBJECT_MARKER = ".local-qa-object-store.json";
const MIME = Object.freeze({ txt: "text/plain", pdf: "application/pdf",
  docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  receipt: "application/json", screenshot: "image/png" });
const hash = (value) => createHash("sha256").update(value).digest("hex");
const mac = (key, value) => createHmac("sha256", key).update(value).digest();
const fail = (status = 403) => { throw Object.assign(new Error("local_qa_object_refused"), { status }); };
const identity = (s) => [s.dev, s.ino, s.uid, s.mode, s.nlink, s.size, s.mtimeMs, s.ctimeMs];
const same = (a, b) => identity(a).every((value, i) => value === identity(b)[i]);

function privateFile(path, maximum) {
  const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const before = fstatSync(fd);
    if (!before.isFile() || before.uid !== process.getuid() || before.nlink !== 1 ||
      (before.mode & 0o7777) !== 0o600 || before.size > maximum) fail(500);
    const bytes = readFileSync(fd);
    if (bytes.length !== before.size || !same(before, fstatSync(fd)) || !same(before, lstatSync(path))) fail(500);
    return bytes;
  } finally { closeSync(fd); }
}

function header(request, name) {
  let count = 0;
  for (let i = 0; i < request.rawHeaders.length; i += 2) {
    if (request.rawHeaders[i].toLowerCase() === name) count++;
  }
  const value = request.headers[name];
  if (count !== 1 || typeof value !== "string") fail();
  return value;
}

function timestamp(value) {
  if (!/^\d{8}T\d{6}Z$/.test(value)) fail();
  const iso = `${value.slice(0, 4)}-${value.slice(4, 6)}-${value.slice(6, 8)}T${value.slice(9, 11)}:${value.slice(11, 13)}:${value.slice(13, 15)}Z`;
  const ms = Date.parse(iso);
  if (!Number.isFinite(ms) || new Date(ms).toISOString().replace(/[-:]|\.\d{3}/g, "") !== value ||
      Math.abs(Date.now() - ms) > 300_000) fail();
  return value.slice(0, 8);
}

function markerFor({ directory, qaId, bucket, prefix, accessKey, secret, port = 0 }) {
  if (typeof directory !== "string" || !isAbsolute(directory) || resolve(directory) !== directory ||
      !new RegExp(`^${UUID}$`).test(qaId) || !/^qa-[0-9a-f]{16,64}$/.test(bucket) ||
      !/^qa-[0-9a-f]{16,64}$/.test(prefix) || !/^qa[0-9a-f]{16,64}$/.test(accessKey) ||
      !/^[0-9a-f]{64}$/.test(secret) || !Number.isInteger(port) || port < 0 || port > 65535) fail(500);
  return Buffer.from(JSON.stringify({ version: 1, qaId, bucket, prefix,
    credentialSha256: hash(JSON.stringify([accessKey, secret])) }) + "\n");
}

// Called only while creating a new, not-yet-committed parent QA state. Never
// use this to recover an established store with a missing marker.
export function initializeLocalQaObjectDirectory(options) {
  let rootFd;
  try {
    const marker = markerFor(options), { directory } = options;
    if (realpathSync(directory) !== directory) fail(500);
    const parent = lstatSync(dirname(directory));
    rootFd = openSync(directory, constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
    const root = fstatSync(rootFd), named = lstatSync(directory);
    if (!parent.isDirectory() || parent.isSymbolicLink() || parent.uid !== process.getuid() ||
        (parent.mode & 0o7777) !== 0o700 || !root.isDirectory() || root.uid !== process.getuid() ||
        (root.mode & 0o7777) !== 0o700 || root.dev !== named.dev || root.ino !== named.ino ||
        readdirSync(directory).length !== 0) fail(500);
    const fd = openSync(join(directory, LOCAL_QA_OBJECT_MARKER),
      constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
    try { writeFileSync(fd, marker); fsyncSync(fd); } finally { closeSync(fd); }
    fsyncSync(rootFd);
    if (!privateFile(join(directory, LOCAL_QA_OBJECT_MARKER), MAX_HEADER).equals(marker)) fail(500);
    return Object.freeze({ markerFilename: LOCAL_QA_OBJECT_MARKER, markerSha256: hash(marker) });
  } catch { throw new Error("local_qa_object_store_init_failed"); }
  finally { if (rootFd !== undefined) closeSync(rootFd); }
}

export async function startLocalQaObjectStore({ directory, qaId, bucket, prefix, accessKey, secret, port = 0 }) {
  let server, lockPath, lockIdentity, rootFd, rootIdentity, parentIdentity, markerBytes, closing;
  const sockets = new Set(), pending = new Set(), counts = { put: 0, get: 0, delete: 0, denied: 0 };
  let closed = false, queue = Promise.resolve(), host, receiptAdmission = null, receiptAdmissionUsed = false;
  const run = (fn) => { const result = queue.then(fn); queue = result.catch(() => {}); return result; };
  const syncRoot = () => fsyncSync(rootFd);
  function verifyRoot() {
    const named = lstatSync(directory), fd = fstatSync(rootFd);
    const parent = lstatSync(dirname(directory));
    if (!named.isDirectory() || named.isSymbolicLink() || named.uid !== process.getuid() ||
        (named.mode & 0o7777) !== 0o700 || named.dev !== rootIdentity.dev || named.ino !== rootIdentity.ino ||
        fd.dev !== rootIdentity.dev || fd.ino !== rootIdentity.ino || realpathSync(directory) !== directory ||
        !parent.isDirectory() || parent.isSymbolicLink() || parent.uid !== process.getuid() ||
        (parent.mode & 0o7777) !== 0o700 || parent.dev !== parentIdentity.dev || parent.ino !== parentIdentity.ino) fail(500);
    const marker = privateFile(join(directory, LOCAL_QA_OBJECT_MARKER), MAX_HEADER);
    if (!marker.equals(markerBytes)) fail(500);
    const lock = lstatSync(lockPath);
    if (!lock.isFile() || lock.isSymbolicLink() || lock.uid !== process.getuid() || lock.nlink !== 1 ||
        (lock.mode & 0o7777) !== 0o600 || lock.dev !== lockIdentity.dev || lock.ino !== lockIdentity.ino) fail(500);
  }
  function parsePath(path) {
    if (typeof path !== "string" || path.length > 512 || path.includes("%") || path.includes("?") || path.includes("#")) fail();
    const resume = new RegExp(`^/${bucket}/${prefix}/accounts/([A-Za-z0-9_-]{1,128})/jobs/resumes/(${UUID})/sha256/([0-9a-f]{64})\\.(txt|pdf|docx)$`).exec(path);
    if (resume) return { family: "resume", path, digest: resume[3], mime: MIME[resume[4]] };
    const evidence = new RegExp(`^/${bucket}/${prefix}/accounts/(${UUID})/context/jobs/(${UUID})/receipts/([0-9a-f]{64})/(0-resume|1-screenshot)-([0-9a-f]{20})$`).exec(path);
    if (evidence) return { family: "receipt", path, accountId: evidence[1], applicationId: evidence[2],
      applicationScope: evidence[3], kind: evidence[4], digestPrefix: evidence[5],
      mime: evidence[4] === "0-resume" ? MIME.pdf : MIME.screenshot };
    const bundle = new RegExp(`^/${bucket}/${prefix}/accounts/(${UUID})/jobs/applications/([0-9a-f]{64})/receipts/([0-9a-f]{64})/bundles/([0-9a-f]{64})/sha256/([0-9a-f]{64})\\.json$`).exec(path);
    if (bundle) return { family: "receipt", path, accountId: bundle[1], applicationScope: bundle[2],
      receiptScope: bundle[3], bundleScope: bundle[4], digest: bundle[5], kind: "bundle", mime: MIME.receipt };
    fail();
  }
  function receiptAllowed(key) {
    return key.family === "receipt" && receiptAdmission !== null
      && key.accountId === receiptAdmission.accountId
      && (key.kind === "bundle"
        ? key.applicationScope === hash(receiptAdmission.applicationId)
        : key.applicationId === receiptAdmission.applicationId);
  }
  function admitMockReceipt({ accountId, applicationId } = {}) {
    if (closed || receiptAdmissionUsed || typeof accountId !== "string" || typeof applicationId !== "string"
      || !new RegExp(`^${UUID}$`).test(accountId) || !new RegExp(`^${UUID}$`).test(applicationId)) {
      throw new Error("local_qa_object_receipt_admission_refused");
    }
    receiptAdmissionUsed = true;
    receiptAdmission = Object.freeze({ accountId, applicationId });
    return Object.freeze({ accountId, applicationId });
  }
  function readRecord(name) {
    const data = privateFile(join(directory, name), MAX_OBJECT + MAX_HEADER + 1);
    const split = data.indexOf(10);
    if (split < 1 || split > MAX_HEADER) fail(500);
    let meta;
    const rawHeader = data.subarray(0, split).toString("utf8");
    try { meta = JSON.parse(rawHeader); } catch { fail(500); }
    if (!meta || Object.keys(meta).sort().join(",") !== "digest,mime,path,size,version" || meta.version !== 1 ||
        !Number.isSafeInteger(meta.size) || meta.size <= 0 || meta.size > MAX_OBJECT ||
        typeof meta.path !== "string" || typeof meta.digest !== "string" || typeof meta.mime !== "string" ||
        JSON.stringify({ version: meta.version, path: meta.path, digest: meta.digest, mime: meta.mime, size: meta.size }) !== rawHeader) fail(500);
    const key = parsePath(meta.path), body = data.subarray(split + 1);
    if (name !== `${hash(meta.path)}.record` || (key.digest !== undefined && key.digest !== meta.digest)
        || (key.digestPrefix !== undefined && !meta.digest.startsWith(key.digestPrefix)) || key.mime !== meta.mime ||
        body.length !== meta.size || hash(body) !== meta.digest) fail(500);
    return { meta, data, body };
  }
  function inventory() {
    verifyRoot();
    let total = 0, count = 0, receiptCount = 0;
    for (const name of readdirSync(directory)) {
      if (name === LOCAL_QA_OBJECT_MARKER || name === ".local-qa-object-store.lock") continue;
      if (!/^[0-9a-f]{64}\.record$/.test(name)) fail(500);
      const record = readRecord(name); count++; total += record.meta.size;
      receiptCount += Number(parsePath(record.meta.path).family === "receipt"); record.data.fill(0);
      if (count > MAX_RECORDS || total > MAX_TOTAL) fail(500);
    }
    return { count, total, receiptCount };
  }
  async function handle(request, response) {
    let body; const chunks = [];
    try {
      if (closed) fail(503);
      if (request.socket.remoteAddress !== "127.0.0.1" || header(request, "host") !== host) fail();
      if (!["PUT", "GET", "DELETE"].includes(request.method)) fail(405);
      const key = parsePath(request.url), payload = header(request, "x-amz-content-sha256");
      if (key.family === "receipt" && !receiptAllowed(key)) fail();
      if (!/^[0-9a-f]{64}$/.test(payload)) fail();
      const dateTime = header(request, "x-amz-date"), date = timestamp(dateTime);
      for (const name of Object.keys(request.headers)) {
        if (name.startsWith("x-amz-") && name !== "x-amz-content-sha256" && name !== "x-amz-date") fail();
      }
      if (request.headers["transfer-encoding"] !== undefined || request.headers.trailer !== undefined) fail();
      const auth = header(request, "authorization");
      const match = /^AWS4-HMAC-SHA256 Credential=([A-Za-z0-9]+)\/(\d{8})\/us-east-1\/s3\/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=([0-9a-f]{64})$/.exec(auth);
      if (!match || match[1] !== accessKey || match[2] !== date) fail();
      const scope = `${date}/us-east-1/s3/aws4_request`;
      const headers = `host:${host}\nx-amz-content-sha256:${payload}\nx-amz-date:${dateTime}\n`;
      const canonical = `${request.method}\n${key.path}\n\n${headers}\n${SIGNED}\n${payload}`;
      const signed = `AWS4-HMAC-SHA256\n${dateTime}\n${scope}\n${hash(canonical)}`;
      let signingKey = mac(`AWS4${secret}`, date);
      for (const part of ["us-east-1", "s3", "aws4_request"]) { const old = signingKey; signingKey = mac(old, part); old.fill(0); }
      const expected = mac(signingKey, signed); signingKey.fill(0);
      const valid = timingSafeEqual(expected, Buffer.from(match[3], "hex")); expected.fill(0);
      if (!valid) fail();
      let length = 0;
      if (request.method === "PUT") {
        const value = header(request, "content-length");
        if (!/^[1-9][0-9]{0,7}$/.test(value) || Number(value) > MAX_OBJECT) fail(413);
        length = Number(value);
        if (header(request, "content-type") !== key.mime) fail(415);
        if ((key.digest !== undefined && payload !== key.digest)
            || (key.digestPrefix !== undefined && !payload.startsWith(key.digestPrefix))) fail();
      } else if (payload !== EMPTY || (request.headers["content-length"] !== undefined && header(request, "content-length") !== "0")) fail();
      let received = 0;
      for await (const chunk of request) {
        received += chunk.length; if (received > length || received > MAX_OBJECT || closed) fail(413);
        chunks.push(chunk); pending.add(chunk);
      }
      if (closed || received !== length) fail();
      body = Buffer.concat(chunks, received); pending.add(body);
      if (hash(body) !== payload) fail();
      const result = await run(() => {
        if (closed) fail(503);
        const { count, total, receiptCount } = inventory(), name = `${hash(key.path)}.record`, path = join(directory, name);
        let existing;
        try { existing = readRecord(name); } catch (error) { if (error?.code !== "ENOENT") throw error; }
        if (request.method === "GET") {
          if (!existing) fail(404);
          if (existing.meta.path !== key.path) { existing.data.fill(0); fail(500); }
          return { bytes: existing.data, offset: existing.data.length - existing.meta.size, mime: existing.meta.mime };
        }
        if (request.method === "DELETE") {
          if (key.family === "receipt") fail();
          if (existing) { if (existing.meta.path !== key.path) { existing.data.fill(0); fail(500); }
            existing.data.fill(0); verifyRoot(); unlinkSync(path); syncRoot(); }
          return {};
        }
        if (existing) {
          const exact = existing.meta.path === key.path && existing.meta.mime === key.mime && existing.body.equals(body);
          existing.data.fill(0); if (!exact) fail(409); return {};
        }
        if (count >= MAX_RECORDS || total + length > MAX_TOTAL || (key.family === "receipt" && receiptCount >= 3)) fail(413);
        const meta = Buffer.from(JSON.stringify({ version: 1, path: key.path, digest: payload, mime: key.mime, size: length }) + "\n");
        const tmp = join(directory, `.pending-${randomBytes(16).toString("hex")}`); let fd, writtenIdentity, recordBytes;
        try {
          fd = openSync(tmp, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
          writtenIdentity = fstatSync(fd); recordBytes = Buffer.concat([meta, body]);
          writeFileSync(fd, recordBytes); fsyncSync(fd); closeSync(fd); fd = undefined;
          verifyRoot(); const current = lstatSync(tmp);
          if (!current.isFile() || current.uid !== process.getuid() || current.nlink !== 1 ||
              current.dev !== writtenIdentity.dev || current.ino !== writtenIdentity.ino || (current.mode & 0o7777) !== 0o600) fail(500);
          try { lstatSync(path); fail(409); } catch (error) { if (error?.code !== "ENOENT") throw error; }
          renameSync(tmp, path); syncRoot();
        } finally {
          if (fd !== undefined) closeSync(fd);
          try { const current = lstatSync(tmp); if (writtenIdentity && current.dev === writtenIdentity.dev && current.ino === writtenIdentity.ino) unlinkSync(tmp); }
          catch (error) { if (error?.code !== "ENOENT") throw error; }
          meta.fill(0); recordBytes?.fill(0);
        }
        return {};
      });
      counts[request.method.toLowerCase()]++;
      if (result.bytes) {
        const bytes = result.bytes; response.writeHead(200, { "content-type": result.mime, "content-length": bytes.length - result.offset });
        response.end(bytes.subarray(result.offset), () => bytes.fill(0));
        response.on("close", () => bytes.fill(0));
      } else response.writeHead(request.method === "DELETE" ? 204 : 200, { "content-length": "0" }).end();
    } catch (error) {
      counts.denied++;
      if (!response.headersSent && !response.destroyed) response.writeHead([403, 404, 405, 409, 413, 415, 503].includes(error?.status) ? error.status : 500,
        { "content-length": "0", connection: "close" }).end();
      else response.destroy();
      request.resume();
    } finally { for (const bytes of [...chunks, ...(body ? [body] : [])]) { bytes.fill(0); pending.delete(bytes); } }
  }
  async function close() {
    if (closing) return closing;
    closed = true; for (const socket of sockets) socket.destroy();
    closing = (async () => {
      if (server) await new Promise((resolveClose) => server.close(() => resolveClose()));
      await queue; for (const bytes of pending) bytes.fill(0); pending.clear();
      if (rootFd !== undefined) {
        try { verifyRoot(); unlinkSync(lockPath); syncRoot(); }
        finally { closeSync(rootFd); rootFd = undefined; }
      }
    })();
    return closing;
  }
  try {
    markerBytes = markerFor({ directory, qaId, bucket, prefix, accessKey, secret, port });
    if (realpathSync(directory) !== directory) fail(500);
    parentIdentity = lstatSync(dirname(directory));
    if (!parentIdentity.isDirectory() || parentIdentity.isSymbolicLink() || parentIdentity.uid !== process.getuid() ||
        (parentIdentity.mode & 0o7777) !== 0o700) fail(500);
    rootFd = openSync(directory, constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
    rootIdentity = fstatSync(rootFd);
    if (!rootIdentity.isDirectory() || rootIdentity.uid !== process.getuid() || (rootIdentity.mode & 0o7777) !== 0o700) fail(500);
    const markerPath = join(directory, LOCAL_QA_OBJECT_MARKER);
    if (!privateFile(markerPath, MAX_HEADER).equals(markerBytes)) fail(500);
    lockPath = join(directory, ".local-qa-object-store.lock");
    const lockFd = openSync(lockPath, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
    try { lockIdentity = fstatSync(lockFd); fsyncSync(lockFd); } finally { closeSync(lockFd); } syncRoot();
    inventory();
    server = createServer({ maxHeaderSize: 8192, headersTimeout: 5000, requestTimeout: 5000 },
      (request, response) => { void handle(request, response); });
    server.maxConnections = 8; server.keepAliveTimeout = 1000; server.setTimeout(5000, (socket) => socket.destroy());
    server.on("connection", (socket) => { sockets.add(socket); socket.on("close", () => sockets.delete(socket)); });
    server.on("clientError", (_error, socket) => { counts.denied++; socket.destroy(); });
    await new Promise((resolveListen, reject) => { server.once("error", reject); server.listen(port, "127.0.0.1", () => { server.off("error", reject); resolveListen(); }); });
    const address = server.address(); if (!address || typeof address === "string" || address.address !== "127.0.0.1") fail(500);
    host = `127.0.0.1:${address.port}`; const endpoint = `http://${host}`;
    const env = Object.freeze({ BLUEY_OBJECT_ENDPOINT_URL: endpoint, BLUEY_OBJECT_BUCKET: bucket,
      BLUEY_OBJECT_ACCESS_KEY_ID: accessKey, BLUEY_OBJECT_SECRET_ACCESS_KEY: secret,
      BLUEY_OBJECT_REGION: "us-east-1", BLUEY_OBJECT_KEY_PREFIX: prefix, BLUEY_OBJECT_MAX_BYTES: String(MAX_OBJECT) });
    const api = { endpoint, counters: () => ({ ...counts }), close };
    // Credentials are available only to an explicit caller, not default JSON/log serialization.
    Object.defineProperty(api, "env", { value: env, enumerable: false });
    // This owner-only capability cannot be configured or invoked over HTTP.
    Object.defineProperty(api, "admitMockReceipt", { value: admitMockReceipt, enumerable: false });
    return Object.freeze(api);
  } catch {
    if (server) { for (const socket of sockets) socket.destroy(); await new Promise((done) => server.close(() => done())); }
    if (rootFd !== undefined) {
      if (lockIdentity) { try { verifyRoot(); unlinkSync(lockPath); syncRoot(); } catch { /* retain ambiguous evidence */ } }
      closeSync(rootFd);
    }
    throw new Error("local_qa_object_store_start_failed");
  }
}
