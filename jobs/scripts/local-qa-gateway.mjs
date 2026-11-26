// Local QA only: a pinned static snapshot and a deliberately narrow API boundary.
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { open, lstat, realpath } from "node:fs/promises";
import { createServer, request as httpRequest } from "node:http";
import { Socket } from "node:net";
import { dirname, extname, join, resolve, sep } from "node:path";
import { performance } from "node:perf_hooks";

const MAX_BODY = 15 * 1024 * 1024, MAX_RESPONSE = 16 * 1024 * 1024;
export const LOCAL_QA_GATEWAY_BACKEND_FENCE_VERSION = 1;
const MIME = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".mjs": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8", ".json": "application/json", ".svg": "image/svg+xml", ".png": "image/png", ".webp": "image/webp", ".woff2": "font/woff2", ".ico": "image/x-icon" };
const UUID = "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}";
const EXACT_UUID = new RegExp(`^${UUID}$`);
const CANDIDATE_EMAIL = "candidate@bluey-qa.test";
const CANDIDATE_MAX_LIFETIME_MS = 2 * 60 * 60 * 1000;
const CANDIDATE_EXPIRED = "local_qa_candidate_expired";
const ACCESS_TOKEN = /^[A-Za-z0-9_-]{1,1024}\.[A-Za-z0-9_-]{1,2048}\.[A-Za-z0-9_-]{1,2048}$/;
const CANDIDATE_READS = new Set(["/account/me", "/api/jobs/beta-access", "/api/jobs/workspace",
  "/api/jobs/taxonomy", "/api/jobs/resume-source"]);
const COMMUNICATION_LIST = "/api/jobs/communication-actions";
const EXACT = new Map([
  ["POST", new Set(["/auth/signup/start", "/auth/signup/confirm", "/auth/login", "/auth/refresh", "/auth/logout", "/api/jobs/onboarding/complete", "/api/jobs/resume-source", "/api/jobs/facts", "/api/jobs/tracks", "/api/jobs/matches", "/api/jobs/matches/page"])],
  ["GET", new Set(["/auth/captcha/config", "/account/me", "/api/jobs/beta-access", "/api/jobs/workspace", "/api/jobs/taxonomy", "/api/jobs/resume-source"])],
  ["PUT", new Set(["/api/jobs/profile", "/api/jobs/preferences"])],
]);
function apiAllowed(method, path) {
  return EXACT.get(method)?.has(path) ||
    ((method === "PUT" || method === "DELETE") && new RegExp(`^/api/jobs/tracks/${UUID}$`).test(path)) ||
    (method === "DELETE" && new RegExp(`^/api/jobs/facts/${UUID}$`).test(path)) ||
    (method === "GET" && new RegExp(`^/api/jobs/matches/${UUID}$`).test(path));
}
function requireValid(value) { if (!value) throw new Error("local_qa_gateway_refused"); }
function sha(bytes) { return createHash("sha256").update(bytes).digest("hex"); }

// The in-process owner supplies IDs measured in its private QA store. No
// portal parameter, environment variable, or CLI option grants this mode.
function candidateBoundary(binding) {
  if (binding === undefined) return null;
  requireValid(binding && Object.getPrototypeOf(binding) === Object.prototype);
  const keys = Object.keys(binding).sort().join(",");
  const c2c = keys === "accountId,c2cApplicationId,c2cMatchId,resumeId";
  requireValid((keys === "accountId,resumeId" || c2c)
    && typeof binding.accountId === "string" && EXACT_UUID.test(binding.accountId)
    && (binding.resumeId === null || typeof binding.resumeId === "string" && EXACT_UUID.test(binding.resumeId)));
  const { accountId, resumeId } = binding;
  const resumePath = resumeId === null ? null : `/api/jobs/resume-versions/${resumeId}`;
  // Only the persisted C2C-draft profile can enable its exact match detail.
  // Normal saved-account checks retain the previous read surface.
  let c2cMatchPath = null, c2cCommunicationQuery = null;
  if (c2c) {
    const { c2cMatchId, c2cApplicationId } = binding;
    requireValid(resumePath !== null && typeof c2cMatchId === "string" && EXACT_UUID.test(c2cMatchId)
      && typeof c2cApplicationId === "string" && EXACT_UUID.test(c2cApplicationId));
    c2cMatchPath = `/api/jobs/matches/${c2cMatchId}`;
    c2cCommunicationQuery = `limit=100&application_id=${c2cApplicationId}`;
  }
  function communicationRead(method, path, query) {
    return c2c && method === "GET" && path === COMMUNICATION_LIST
      && (query === "limit=100" || query === c2cCommunicationQuery);
  }
  const sessions = new Set();
  return {
    alone(method, path, query) {
      if (query !== undefined) return communicationRead(method, path, query);
      return (method === "POST" && path === "/auth/login")
        || (method === "GET" && (CANDIDATE_READS.has(path) || path === resumePath || path === c2cMatchPath));
    },
    acceptCommunicationRead(method, path, query, reply, payload) {
      if (!communicationRead(method, path, query)) return;
      // A fresh synthetic account must have no communication actions. Reject
      // an unexpected response without exposing its contents to the browser.
      requireValid(reply.statusCode === 200 && /^application\/json(?:;\s*charset=utf-8)?$/i.test(reply.headers["content-type"] ?? "")
        && payload.length <= 32 && payload.toString("utf8") === "[]");
    },
    authorize(method, headers, body) {
      if (method === "POST") {
        requireValid(headers.authorization === undefined && body.length > 0 && body.length <= 256
          && /^application\/json(?:; charset=utf-8)?$/i.test(headers["content-type"] ?? ""));
        const input = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(body));
        requireValid(input && Object.getPrototypeOf(input) === Object.prototype
          && Object.keys(input).sort().join(",") === "email,password"
          && body.equals(Buffer.from(JSON.stringify(input)))
          && input.email === CANDIDATE_EMAIL && typeof input.password === "string"
          && /^Qa-[0-9a-f]{48}$/.test(input.password));
      } else {
        requireValid(body.length === 0 && typeof headers.authorization === "string"
          && headers.authorization.startsWith("Bearer ") && ACCESS_TOKEN.test(headers.authorization.slice(7))
          && sessions.has(sha(headers.authorization.slice(7))));
      }
    },
    acceptLogin(reply, payload) {
      if (reply.statusCode !== 200) return;
      requireValid(/^application\/json(?:;\s*charset=utf-8)?$/i.test(reply.headers["content-type"] ?? "")
        && payload.length <= 65536);
      const value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(payload));
      requireValid(value?.account?.id === accountId && value.account.email === CANDIDATE_EMAIL
        && value.account.is_admin === false && value.account.is_temporary === false
        && typeof value.access_token === "string" && ACCESS_TOKEN.test(value.access_token));
      const key = sha(value.access_token);
      sessions.delete(key);
      if (sessions.size >= 8) sessions.delete(sessions.values().next().value);
      sessions.add(key);
    },
    close() { sessions.clear(); },
  };
}
async function staticSnapshot(root, manifest) {
  requireValid(typeof root === "string" && resolve(root) === root && await realpath(root) === root && manifest && typeof manifest === "object" && !Array.isArray(manifest));
  const entries = Object.entries(manifest); requireValid(entries.length > 0 && entries.length <= 256);
  const result = new Map(); let total = 0;
  for (const [url, digest] of entries) {
    requireValid(/^\/(?:index\.html|assets\/[A-Za-z0-9._/-]+|jobs\/[A-Za-z0-9._/-]+)$/.test(url) && !url.includes("..") && !url.includes("//") && /^[0-9a-f]{64}$/.test(digest) && MIME[extname(url)]);
    const path = join(root, url.slice(1)); let current = root;
    for (const part of url.slice(1).split("/")) { current = join(current, part); requireValid(!(await lstat(current)).isSymbolicLink()); }
    requireValid((await realpath(path)).startsWith(root + sep));
    const file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    try {
      const before = await file.stat(); requireValid(before.isFile() && before.size <= 16 * 1024 * 1024);
      const bytes = await file.readFile(), after = await file.stat();
      requireValid(before.dev === after.dev && before.ino === after.ino && before.size === after.size && bytes.length === before.size && sha(bytes) === digest);
      total += bytes.length; requireValid(total <= 64 * 1024 * 1024);
      result.set(url, { bytes, digest, type: MIME[extname(url)] });
    } finally { await file.close(); }
  }
  requireValid(result.has("/index.html") && result.has("/jobs/index.html"));
  return result;
}
function fixed(response, status) { if (!response.headersSent) response.writeHead(status, { "content-type": "text/plain; charset=utf-8", "cache-control": "no-store", "x-content-type-options": "nosniff" }); response.end("Request refused\n"); }
function collect(stream, limit, timeoutMs = 0) {
  return new Promise((resolveBody, reject) => {
    const chunks = []; let size = 0, settled = false;
    const timer = timeoutMs ? setTimeout(() => fail("timeout"), timeoutMs) : null;
    function fail(reason) { if (settled) return; settled = true; clearTimeout(timer); reject(new Error(reason)); stream.destroy(); }
    stream.on("data", chunk => { size += chunk.length; if (size > limit) fail("limit"); else chunks.push(chunk); });
    stream.once("end", () => { if (!settled) { settled = true; clearTimeout(timer); resolveBody(Buffer.concat(chunks)); } });
    stream.once("error", () => fail("stream")); stream.once("aborted", () => fail("aborted"));
  });
}
async function unixBackendTarget(binding) {
  requireValid(binding && Object.getPrototypeOf(binding) === Object.prototype
    && Object.keys(binding).sort().join(",") === "dev,ino,mode,path,root,uid");
  requireValid(typeof binding.path === "string" && typeof binding.root === "string"
    && resolve(binding.path) === binding.path && resolve(binding.root) === binding.root
    && dirname(binding.path) === binding.root
    && typeof process.getuid === "function" && binding.uid === process.getuid() && binding.mode === 0o600
    && [binding.uid, binding.mode, binding.dev, binding.ino].every(Number.isSafeInteger));
  requireValid(await realpath(binding.root) === binding.root);
  const root = await lstat(binding.root, { bigint: true });
  requireValid(root.isDirectory() && root.uid === BigInt(binding.uid) && (root.mode & 0o7777n) === 0o700n);
  const socket = await lstat(binding.path, { bigint: true });
  requireValid(socket.isSocket() && socket.uid === BigInt(binding.uid)
    && Number(socket.mode & 0o7777n) === binding.mode
    && Number(socket.dev) === binding.dev && Number(socket.ino) === binding.ino);
  return Object.freeze({ kind: "unix", path: binding.path, root: binding.root, uid: binding.uid,
    mode: binding.mode, dev: binding.dev, ino: binding.ino });
}
async function verifyBackendOwner(verifier, target) {
  requireValid(typeof verifier === "function");
  let timer;
  try {
    const result = await Promise.race([
      Promise.resolve().then(() => verifier(target)),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error("backend_verification_timeout")), 15000); }),
    ]);
    requireValid(result === true);
  } finally { clearTimeout(timer); }
}
export async function startLocalQaGateway({ webDirectory, manifest, backendPort, backendUnixSocket, verifyBackend,
  connectOwnedUnix, ownedLoopbackFd, port = 0, candidateReadback, expiresAtMs }) {
  requireValid(Number.isInteger(port) && port >= 0 && port <= 65535);
  const unixBackend = backendUnixSocket !== undefined;
  requireValid(unixBackend
    ? backendPort === undefined && typeof connectOwnedUnix === "function"
    : connectOwnedUnix === undefined && Number.isInteger(backendPort) && backendPort > 0 && backendPort <= 65535);
  const candidate = candidateBoundary(candidateReadback);
  requireValid(!unixBackend || candidate);
  // A strict Unix run can inherit an already-bound listener from its owner.
  // The legacy TCP lane and ordinary ephemeral bind behavior stay unchanged.
  const useOwnedListener = ownedLoopbackFd !== undefined;
  requireValid(!useOwnedListener || (unixBackend && port === 0 && Number.isInteger(ownedLoopbackFd)
    && ownedLoopbackFd >= 3 && ownedLoopbackFd <= 255));
  const backend = unixBackend
    ? await unixBackendTarget(backendUnixSocket)
    : Object.freeze({ host: "127.0.0.1", port: backendPort });
  const unixIdentity = unixBackend ? Object.freeze({
    path: backend.path, root: backend.root, uid: backend.uid,
    mode: backend.mode, dev: backend.dev, ino: backend.ino,
  }) : null;
  async function verifyPinnedBackend() {
    await verifyBackendOwner(verifyBackend, backend);
    if (unixIdentity) await unixBackendTarget(unixIdentity);
  }
  const usedUnixStreams = new WeakSet();
  async function ownedUnixStream() {
    // Path metadata does not authenticate the peer of a newly opened socket.
    // Only a trusted in-process broker may connect and attest the already-held
    // stream. This gateway never opens the pathname or reuses a connection.
    const deadline = AbortSignal.timeout(8000);
    let closeOnTimeout;
    const pending = Promise.resolve().then(() => connectOwnedUnix(backend, deadline)).then(stream => {
      if (!(stream instanceof Socket && stream.readyState === "open"
          && !stream.localAddress && !stream.remoteAddress) || deadline.aborted || usedUnixStreams.has(stream)) {
        if (stream instanceof Socket) stream.destroy();
        throw new Error("local_qa_gateway_refused");
      }
      usedUnixStreams.add(stream);
      return stream;
    });
    try {
      return await Promise.race([pending, new Promise((_, reject) => {
        closeOnTimeout = () => reject(new Error("local_qa_gateway_refused"));
        deadline.addEventListener("abort", closeOnTimeout, { once: true });
      })]);
    } finally {
      if (closeOnTimeout) deadline.removeEventListener("abort", closeOnTimeout);
    }
  }
  const startedAtMs = Date.now(), startedAtMonotonicMs = performance.now();
  if (candidate) {
    requireValid(Number.isSafeInteger(expiresAtMs) && expiresAtMs > startedAtMs
      && expiresAtMs - startedAtMs <= CANDIDATE_MAX_LIFETIME_MS);
  } else requireValid(expiresAtMs === undefined);
  const lifetimeMs = candidate ? expiresAtMs - startedAtMs : null;
  const candidateExpired = () => candidate && (Date.now() >= expiresAtMs
    || performance.now() - startedAtMonotonicMs >= lifetimeMs);
  function requireCandidateLive() {
    if (candidateExpired()) throw new Error(CANDIDATE_EXPIRED);
  }
  const staticFiles = await staticSnapshot(webDirectory, manifest);
  requireCandidateLive();
  const sockets = new Set(), upstreams = new Set(); let origin;
  const server = createServer(async (req, res) => {
    let stagedStream;
    try {
      requireCandidateLive();
      requireValid(req.socket.remoteAddress === "127.0.0.1" && req.headers.host === origin.slice(7) && (!req.headers.origin || req.headers.origin === origin));
      const seen = new Set(); for (let i = 0; i < req.rawHeaders.length; i += 2) { const name = req.rawHeaders[i].toLowerCase(); requireValid(!seen.has(name)); seen.add(name); }
      requireValid(typeof req.url === "string" && req.url.length <= 2048 && /^\/[\x21-\x7e]*$/.test(req.url) && !req.url.includes("#") && !req.url.includes("\\"));
      const [path, query, ...extra] = req.url.split("?"); requireValid(!extra.length && !path.includes("%") && !path.includes("..") && !path.includes("//"));
      const method = req.method;
      if (method === "GET" && path === "/" && query === undefined) { res.writeHead(302, { location: "/jobs/", "cache-control": "no-store" }); res.end(); return; }
      if (candidate ? candidate.alone(method, path, query) : apiAllowed(method, path)) {
        requireValid((candidate || query === undefined) && (method === "GET" || req.headers.origin === origin));
        const headers = {}; for (const name of ["authorization", "content-type", "x-bluey-jobs-taxonomy-version", "x-bluey-jobs-taxonomy-sha256"]) if (req.headers[name] !== undefined) headers[name] = req.headers[name];
        requireValid(!req.headers["content-encoding"] && !req.headers.cookie && !req.headers["transfer-encoding"]);
        const body = await collect(req, candidate ? 256 : MAX_BODY, 30000);
        requireCandidateLive();
        if (candidate) candidate.authorize(method, headers, body);
        headers["content-length"] = String(body.length);
        try {
          await verifyPinnedBackend();
          if (unixIdentity) stagedStream = await ownedUnixStream();
          requireCandidateLive();
          if (res.destroyed || req.socket.destroyed) {
            stagedStream?.destroy();
            return;
          }
        }
        catch {
          stagedStream?.destroy();
          if (candidateExpired()) res.destroy();
          else if (!res.destroyed) fixed(res, 502);
          return;
        }
        const liveStream = stagedStream;
        const upstream = httpRequest({ ...(liveStream
          ? { createConnection: () => liveStream }
          : { host: backend.host, port: backend.port }), method, path: req.url, headers, timeout: 15000 }, async reply => {
          try {
            requireValid(!reply.headers.location && !(reply.statusCode >= 300 && reply.statusCode < 400) && !reply.headers["set-cookie"]);
            const payload = await collect(reply, MAX_RESPONSE);
            requireCandidateLive();
            if (candidate) {
              await verifyPinnedBackend();
              requireCandidateLive();
              if (res.destroyed || req.socket.destroyed) return;
              if (method === "POST") candidate.acceptLogin(reply, payload);
              candidate.acceptCommunicationRead(method, path, query, reply, payload);
            }
            requireCandidateLive();
            const responseHeaders = { "cache-control": "no-store", "x-content-type-options": "nosniff", "content-length": String(payload.length) };
            if (reply.headers["content-type"]) responseHeaders["content-type"] = reply.headers["content-type"];
            res.writeHead(reply.statusCode || 502, responseHeaders); res.end(payload);
          } catch {
            reply.destroy();
            if (candidateExpired()) res.destroy();
            else fixed(res, 502);
          }
        });
        const deadline = setTimeout(() => upstream.destroy(), 15000);
        stagedStream = undefined;
        upstreams.add(upstream); upstream.once("close", () => upstreams.delete(upstream));
        res.once("close", () => clearTimeout(deadline));
        upstream.once("timeout", () => upstream.destroy());
        upstream.once("error", () => {
          if (candidateExpired()) res.destroy();
          else if (!res.destroyed) fixed(res, 502);
        });
        res.once("close", () => { if (!res.writableEnded) upstream.destroy(); });
        if (liveStream) {
          upstream.once("socket", socket => {
            if (socket !== liveStream || socket.destroyed || res.destroyed || candidateExpired()) {
              upstream.destroy(new Error("local_qa_gateway_refused")); liveStream.destroy(); return;
            }
            // Node's child stdio adoption pauses reads. Resume only after
            // HTTP attaches its parser to the exact attested stream.
            socket.resume();
            upstream.end(body);
          });
        } else {
          upstream.end(body);
        }
        return;
      }
      if (candidate) requireValid(!/^\/(?:auth|account|api|admin)(?:\/|$)/.test(path));
      const staticDigest = staticFiles.get(path)?.digest;
      const versionedAsset = path.startsWith("/assets/") && staticDigest !== undefined
        && (/^v=[0-9]{1,20}$/.test(query) || query === `v=${staticDigest.slice(0, 12)}`);
      requireValid(method === "GET" && (!query || versionedAsset
        || (path === "/login" && /^(?:mode=signup&)?next=%2Fjobs(?:%2F)?$/.test(query))));
      let key = path === "/login" ? "/index.html" : path === "/jobs" || path === "/jobs/" || /^\/jobs\/[A-Za-z0-9_-]+\/?$/.test(path) ? "/jobs/index.html" : path;
      const asset = staticFiles.get(key); requireValid(asset);
      const candidateDocument = candidate && asset.type.startsWith("text/html");
      res.writeHead(200, { "content-type": asset.type, "content-length": String(asset.bytes.length),
        "cache-control": "no-store", "x-content-type-options": "nosniff",
        ...(candidateDocument ? { "content-security-policy": [
          "default-src 'none'", "script-src 'self'", "style-src 'self' 'unsafe-inline'",
          "img-src 'self' data:", "font-src 'self'", "connect-src 'self'",
          "base-uri 'none'", "form-action 'self'", "frame-ancestors 'none'",
        ].join("; "), "referrer-policy": "same-origin" } : {}) });
      res.end(asset.bytes);
    } catch {
      stagedStream?.destroy();
      if (candidateExpired()) res.destroy();
      else if (!res.destroyed) fixed(res, 400);
    }
  });
  server.headersTimeout = 10000; server.requestTimeout = 30000; server.keepAliveTimeout = 2000; server.maxHeadersCount = 64; server.maxConnections = 32;
  server.on("connection", socket => { sockets.add(socket); socket.setTimeout(30000, () => socket.destroy()); socket.once("close", () => sockets.delete(socket)); });
  await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    if (useOwnedListener) server.listen({ fd: ownedLoopbackFd, exclusive: true }, resolveListen);
    else server.listen(port, "127.0.0.1", resolveListen);
  });
  const address = server.address();
  if (address?.address !== "127.0.0.1" || address.family !== "IPv4" || address.port <= 0) {
    for (const socket of sockets) socket.destroy();
    await new Promise(resolveClose => server.close(resolveClose));
    throw new Error("local_qa_gateway_refused");
  }
  origin = `http://127.0.0.1:${address.port}`;
  let closePromise, deadlineTimer;
  function close() {
    if (closePromise) return closePromise;
    closePromise = (async () => {
      clearTimeout(deadlineTimer);
      candidate?.close();
      for (const upstream of upstreams) upstream.destroy();
      for (const socket of sockets) socket.destroy();
      await new Promise((resolveClose, reject) => server.close(error => error ? reject(error) : resolveClose()));
    })();
    return closePromise;
  }
  if (candidate) {
    if (candidateExpired()) {
      await close();
      throw new Error(CANDIDATE_EXPIRED);
    }
    function scheduleDeadlineCheck() {
      if (candidateExpired()) { void close().catch(() => {}); return; }
      const wallRemaining = expiresAtMs - Date.now();
      const monotonicRemaining = lifetimeMs - (performance.now() - startedAtMonotonicMs);
      deadlineTimer = setTimeout(scheduleDeadlineCheck,
        Math.max(0, Math.min(1000, wallRemaining, monotonicRemaining)));
    }
    scheduleDeadlineCheck();
  }
  return { origin, close };
}
