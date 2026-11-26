// Dedicated receipt-test gateway. The caller owns the exact loopback Jobs
// listener, verified asset bytes, ephemeral synthetic session and OS fence.
// This is not the saved-account gateway or a production login flow.
import { createServer, request } from "node:http";
import { createHash } from "node:crypto";
import { extname } from "node:path";

const host = "127.0.0.1";
const maxReply = 4 * 1024 * 1024;
const deadline = 10_000;
const types = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript",
  ".css": "text/css", ".json": "application/json", ".svg": "image/svg+xml",
  ".png": "image/png", ".woff2": "font/woff2" };
const staticHeaders = {
  "cache-control": "no-store",
  "x-content-type-options": "nosniff",
  "referrer-policy": "no-referrer",
  "content-security-policy": "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; "
    + "connect-src 'self'; img-src 'self' data: blob:; font-src 'self'; object-src 'none'; "
    + "frame-ancestors 'none'; form-action 'none'; base-uri 'none'",
};

function refuse(message = "prep_gateway_binding") { throw new Error(message); }
function tokenId(value) { return typeof value === "string" && /^[a-zA-Z0-9_-]{1,128}$/.test(value); }

function snapshot(assets) {
  if (!(assets instanceof Map) || assets.size < 1 || assets.size > 64) refuse();
  const files = new Map();
  let total = 0;
  for (const [key, bytes] of assets) {
    const type = typeof key === "string" && types[extname(key)];
    if (!type || !/^\/jobs\/[a-zA-Z0-9._/-]+$/.test(key)
      || key.split("/").slice(1).some((part) => !part || part === "." || part === "..")
      || !Buffer.isBuffer(bytes) || bytes.length === 0 || bytes.length > maxReply) refuse();
    total += bytes.length;
    if (total > maxReply * 4) refuse();
    files.set(key, { type, bytes: Buffer.from(bytes) });
  }
  if (!files.has("/jobs/index.html")) refuse();
  return files;
}

async function readBounded(stream, cap) {
  let size = 0;
  const chunks = [];
  for await (const bytes of stream) {
    size += bytes.length;
    if (size > cap) refuse("prep_gateway_body_size");
    chunks.push(bytes);
  }
  return Buffer.concat(chunks, size);
}

function forward(port, path, method, bearer, body, signal) {
  return new Promise((resolve, reject) => {
    const upstream = request({
      host, port, path, method, agent: false, signal,
      headers: { authorization: bearer, accept: "application/json", ...(body && {
        "content-type": "application/json", "content-length": body.length,
      }) },
    }, async (response) => {
      try {
        const status = response.statusCode;
        const type = response.headers["content-type"];
        if (!Number.isInteger(status) || status < 200 || status > 599 || (status >= 300 && status < 400)
          || !/^application\/json(?:;\s*charset=utf-8)?$|^text\/plain(?:;\s*charset=utf-8)?$/i.test(type ?? "")) {
          response.destroy();
          refuse("prep_gateway_upstream_shape");
        }
        resolve({ status, type, body: await readBounded(response, maxReply) });
      } catch (error) { reject(error); }
    });
    upstream.once("error", reject);
    upstream.end(body);
  });
}

export async function startInterviewPrepFixtureGateway({
  jobsPort, assetBytes, accountId, accessToken, balanceCents, applicationId, resumeVersionId, receiptFingerprint,
}) {
  if (!Number.isInteger(jobsPort) || jobsPort < 1024 || jobsPort > 65535
    || !/^ui_fixture_[0-9a-f-]{36}$/.test(accountId)
    || typeof accessToken !== "string" || !/^[a-zA-Z0-9_-]{1,1024}\.[a-zA-Z0-9_-]{1,2048}\.[a-zA-Z0-9_-]{1,1024}$/.test(accessToken)
    || !Number.isSafeInteger(balanceCents) || balanceCents < 0
    || !tokenId(applicationId) || !tokenId(resumeVersionId) || !/^[a-f0-9]{64}$/.test(receiptFingerprint)) refuse();
  const assets = snapshot(assetBytes);
  const bearer = `Bearer ${accessToken}`;
  const account = Buffer.from(JSON.stringify({ email: `${accountId}@example.test`, balance_cents: balanceCents }));
  const expectedBody = Buffer.from(JSON.stringify({ prepare: true, receipt_fingerprint: receiptFingerprint }));
  const prepPath = `/api/jobs/applications/${applicationId}/interview-prep`;
  const reads = new Set(["/api/jobs/beta-access", "/api/jobs/workspace", "/api/jobs/communication-actions?limit=100",
    `/api/jobs/communication-actions?limit=100&application_id=${applicationId}`,
    `/api/jobs/resume-versions/${resumeVersionId}`]);
  const stopped = new AbortController();
  let observedPrep = 0, forwardedReads = 0, servedStatic = 0;
  let prepRequestSha256 = null;
  let origin;
  const send = (response, status, body = "", type = "text/plain") => {
    response.writeHead(status, { ...staticHeaders, "content-type": type }).end(body);
  };
  const server = createServer({ requestTimeout: deadline, headersTimeout: deadline, maxHeaderSize: 12 * 1024 },
    async (req, res) => {
      req.setTimeout(deadline, () => req.destroy());
      const path = req.url;
      const sameOrigin = req.headers.origin === undefined || req.headers.origin === origin;
      if (stopped.signal.aborted || req.headers.host !== origin.slice(7) || !sameOrigin) return send(res, 403);
      const get = req.method === "GET" && req.headers["transfer-encoding"] === undefined
        && (req.headers["content-length"] === undefined || req.headers["content-length"] === "0");
      const post = req.method === "POST" && path === prepPath;
      if (get) {
        const entry = assets.get(path === "/jobs/applications" ? "/jobs/index.html" : path);
        if (entry) {
          servedStatic += 1;
          return send(res, 200, entry.bytes, entry.type);
        }
        if (path === "/assets/favicon-512.png" || path === "/favicon.ico") return send(res, 204);
      }
      const accountRead = get && path === "/account/me";
      const apiRead = get && reads.has(path);
      if (!accountRead && !apiRead && !post) return send(res, 404);
      if (req.headers.authorization !== bearer) return send(res, 401);
      if (accountRead) return send(res, 200, account, "application/json");
      let body;
      try {
        if (post) {
          if (req.headers["content-type"] !== "application/json"
            || req.headers["transfer-encoding"] !== undefined
            || req.headers["content-length"] !== String(expectedBody.length)) return send(res, 400);
          body = await readBounded(req, expectedBody.length);
          if (!body.equals(expectedBody)) return send(res, 400);
          if (observedPrep !== 0) return send(res, 409);
          observedPrep += 1; // Count before forwarding; failure cannot reopen the gate.
          prepRequestSha256 = createHash("sha256").update(`POST\n${path}\n`).update(body).digest("hex");
        } else forwardedReads += 1;
        const signal = AbortSignal.any([stopped.signal, AbortSignal.timeout(deadline)]);
        const result = await forward(jobsPort, path, req.method, bearer, body, signal);
        return send(res, result.status, result.body, result.type);
      } catch {
        if (!res.destroyed) return send(res, 502);
      }
    });
  // CONNECT and Upgrade are not part of the fixture and have no forwarding handler.
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, host, () => {
      server.off("error", reject);
      origin = `http://${host}:${server.address().port}`;
      resolve();
    });
  });
  let closing;
  return { origin, census: () => ({ prepForwardAttempts: observedPrep, readForwardAttempts: forwardedReads,
    staticResponses: servedStatic, prepRequestSha256 }),
    close() {
      if (closing) return closing;
      stopped.abort();
      closing = new Promise((resolve, reject) => {
        server.close((error) => error ? reject(error) : resolve());
        server.closeAllConnections();
      });
      return closing;
    } };
}
