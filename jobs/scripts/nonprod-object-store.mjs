// Narrow, in-memory protocol fixture for the production ObjectStorage client.
// This is not an S3/MinIO implementation or cloud/R2 qualification. SigV4:
// https://docs.aws.amazon.com/IAM/latest/UserGuide/reference_sigv-create-signed-request.html
import { createHash, createHmac, randomBytes, timingSafeEqual } from "node:crypto";
import { createServer } from "node:http";

const MAX_BYTES = 1024 * 1024;
const MAX_CLOCK_SKEW_MS = 5 * 60 * 1000;
const SIGNED_HEADERS = "host;x-amz-content-sha256;x-amz-date";
const UUID = "[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";
const ACCOUNT = new RegExp(`^ui_fixture_${UUID}$`);
const EMPTY_HASH = createHash("sha256").update("").digest("hex");

function deny(status = 403) {
  const error = new Error("nonprod_object_store_request_denied");
  error.status = status;
  throw error;
}
function sha256(bytes) { return createHash("sha256").update(bytes).digest("hex"); }
function mac(key, text) { return createHmac("sha256", key).update(text).digest(); }
function amzDate(milliseconds) {
  return new Date(milliseconds).toISOString().replace(/[-:]|\.\d{3}/g, "");
}
function signedTimestamp(value) {
  if (typeof value !== "string" || !/^\d{8}T\d{6}Z$/.test(value)) deny();
  const iso = `${value.slice(0, 4)}-${value.slice(4, 6)}-${value.slice(6, 8)}T${value.slice(9, 11)}:${value.slice(11, 13)}:${value.slice(13, 15)}Z`;
  const milliseconds = Date.parse(iso);
  if (!Number.isFinite(milliseconds) || amzDate(milliseconds) !== value
    || Math.abs(Date.now() - milliseconds) > MAX_CLOCK_SKEW_MS) deny();
  return value.slice(0, 8);
}
function uniqueHeader(request, name) {
  let count = 0;
  for (let index = 0; index < request.rawHeaders.length; index += 2) {
    if (request.rawHeaders[index].toLowerCase() === name) count += 1;
  }
  const value = request.headers[name];
  if (count !== 1 || typeof value !== "string") deny();
  return value;
}

export async function startNonprodObjectStore({ accountId } = {}) {
  if (typeof accountId !== "string" || !ACCOUNT.test(accountId)) {
    throw new Error("nonprod_object_store_synthetic_account_required");
  }
  const bucket = `fixture-${randomBytes(12).toString("hex")}`;
  const prefix = `phase633-${randomBytes(12).toString("hex")}`;
  const accessKey = `fixture${randomBytes(16).toString("hex")}`;
  const secret = randomBytes(32).toString("hex");
  const region = "us-east-1";
  const basePath = `/${bucket}/${prefix}/accounts/${accountId}/jobs/resumes/`;
  const objectSuffix = new RegExp(`^(${UUID})/sha256/([0-9a-f]{64})\\.txt$`);
  const objects = new Map();
  const pendingBytes = new Set();
  const sockets = new Set();
  const counts = { put: 0, get: 0, denied: 0 };
  let host;
  let closed = false;
  let closing;

  async function handle(request, response) {
    let body;
    const chunks = [];
    try {
      if (closed) deny(503);
      if (request.method !== "PUT" && request.method !== "GET") deny(405);
      // Use the exact request target, never URL-normalized paths: encoded
      // separators, traversal, queries and absolute-form targets are forbidden.
      const path = request.url;
      if (typeof path !== "string" || !path.startsWith(basePath)) deny();
      const match = objectSuffix.exec(path.slice(basePath.length));
      if (!match || path.length > 512) deny();
      if (uniqueHeader(request, "host") !== host) deny();
      const payloadHash = uniqueHeader(request, "x-amz-content-sha256");
      if (!/^[0-9a-f]{64}$/.test(payloadHash)) deny();
      const timestamp = uniqueHeader(request, "x-amz-date");
      const date = signedTimestamp(timestamp);
      for (const name of Object.keys(request.headers)) {
        if (name.startsWith("x-amz-") && name !== "x-amz-content-sha256" && name !== "x-amz-date") deny();
      }
      if (request.headers["transfer-encoding"] !== undefined || request.headers.trailer !== undefined) deny();
      const authorization = uniqueHeader(request, "authorization");
      const parsed = /^AWS4-HMAC-SHA256 Credential=([A-Za-z0-9]+)\/(\d{8})\/us-east-1\/s3\/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=([0-9a-f]{64})$/.exec(authorization);
      if (!parsed || parsed[1] !== accessKey || parsed[2] !== date) deny();
      const scope = `${date}/${region}/s3/aws4_request`;
      const headers = `host:${host}\nx-amz-content-sha256:${payloadHash}\nx-amz-date:${timestamp}\n`;
      const canonical = [request.method, path, "", headers, SIGNED_HEADERS, payloadHash].join("\n");
      const stringToSign = ["AWS4-HMAC-SHA256", timestamp, scope, sha256(canonical)].join("\n");
      let key = mac(`AWS4${secret}`, date);
      for (const component of [region, "s3", "aws4_request"]) {
        const previous = key;
        key = mac(previous, component);
        previous.fill(0);
      }
      const expected = mac(key, stringToSign);
      key.fill(0);
      const authenticated = timingSafeEqual(expected, Buffer.from(parsed[3], "hex"));
      expected.fill(0);
      if (!authenticated) deny();
      let expectedLength = 0;
      if (request.method === "PUT") {
        const length = uniqueHeader(request, "content-length");
        if (!/^[1-9][0-9]{0,6}$/.test(length)) deny(413);
        expectedLength = Number(length);
        if (expectedLength > MAX_BYTES) deny(413);
        if (uniqueHeader(request, "content-type") !== "text/plain") deny(415);
        if (match[2] !== payloadHash) deny();
      } else if (payloadHash !== EMPTY_HASH
        || (request.headers["content-length"] !== undefined && request.headers["content-length"] !== "0")) deny();
      let total = 0;
      for await (const chunk of request) {
        total += chunk.length;
        if (total > expectedLength || total > MAX_BYTES || closed) deny(413);
        chunks.push(chunk);
        pendingBytes.add(chunk);
      }
      if (closed || total !== expectedLength) deny();
      body = Buffer.concat(chunks, total);
      pendingBytes.add(body);
      if (sha256(body) !== payloadHash) deny();
      if (request.method === "PUT") {
        if (objects.size && !objects.has(path)) deny(409);
        // The path binds bytes by hash. Exact PUT retries are allowed, but
        // neither another object nor changed bytes can replace the one source.
        const existing = objects.get(path);
        if (existing && (existing.length !== body.length || !timingSafeEqual(existing, body))) deny(409);
        if (!existing) objects.set(path, Buffer.from(body));
        counts.put += 1;
        response.writeHead(200, { "content-length": "0" }).end();
      } else {
        const stored = objects.get(path);
        if (!stored) deny(404);
        counts.get += 1;
        response.writeHead(200, { "content-type": "text/plain", "content-length": String(stored.length) });
        response.end(stored);
      }
    } catch (error) {
      counts.denied += 1;
      // Never log paths, headers, credentials, request bodies or raw errors.
      if (!response.headersSent && !response.destroyed) {
        response.writeHead([403, 404, 405, 409, 413, 415, 503].includes(error?.status) ? error.status : 400,
          { "content-length": "0", connection: "close" }).end();
      } else response.destroy();
      request.resume();
    } finally {
      for (const bytes of [...chunks, ...(body ? [body] : [])]) {
        bytes.fill(0);
        pendingBytes.delete(bytes);
      }
    }
  }

  const server = createServer({ maxHeaderSize: 8192, headersTimeout: 5000, requestTimeout: 5000 },
    (request, response) => { void handle(request, response); });
  server.maxConnections = 8;
  server.keepAliveTimeout = 1000;
  server.setTimeout(5000, (socket) => socket.destroy());
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
  });
  server.on("clientError", (_error, socket) => {
    counts.denied += 1;
    socket.destroy();
  });
  async function close() {
    if (closing) return closing;
    closed = true;
    for (const bytes of objects.values()) bytes.fill(0);
    for (const bytes of pendingBytes) bytes.fill(0);
    objects.clear();
    pendingBytes.clear();
    closing = new Promise((resolve, reject) => {
      server.close((error) => { if (error && error.code !== "ERR_SERVER_NOT_RUNNING") reject(new Error("nonprod_object_store_close_failed")); else resolve(); });
      for (const socket of sockets) socket.destroy();
    });
    return closing;
  }
  try {
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(0, "127.0.0.1", () => { server.off("error", reject); resolve(); });
    });
    const address = server.address();
    if (!address || typeof address === "string" || address.address !== "127.0.0.1") {
      throw new Error("nonprod_object_store_bind_failed");
    }
    host = `127.0.0.1:${address.port}`;
    const endpoint = `http://${host}`;
    return Object.freeze({
      endpoint,
      env: Object.freeze({
        BLUEY_OBJECT_ENDPOINT_URL: endpoint,
        BLUEY_OBJECT_BUCKET: bucket,
        BLUEY_OBJECT_ACCESS_KEY_ID: accessKey,
        BLUEY_OBJECT_SECRET_ACCESS_KEY: secret,
        BLUEY_OBJECT_REGION: region,
        BLUEY_OBJECT_KEY_PREFIX: prefix,
        BLUEY_OBJECT_MAX_BYTES: String(MAX_BYTES),
      }),
      counters: () => ({ ...counts, objects: objects.size }),
      assertStoredBytes(expected) {
        if (closed || !Buffer.isBuffer(expected) || expected.length === 0 || objects.size !== 1) {
          throw new Error("nonprod_object_store_expected_one_source");
        }
        const stored = objects.values().next().value;
        if (stored.length !== expected.length || !timingSafeEqual(stored, expected)) {
          throw new Error("nonprod_object_store_bytes_mismatch");
        }
      },
      close,
    });
  } catch {
    await close();
    throw new Error("nonprod_object_store_start_failed");
  }
}
