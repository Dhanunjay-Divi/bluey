import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { chmod, lstat, mkdtemp, mkdir, writeFile, rm, symlink, realpath } from "node:fs/promises";
import { createServer, request } from "node:http";
import { createServer as createNetServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startLocalQaGateway, LOCAL_QA_GATEWAY_BACKEND_FENCE_VERSION } from "./local-qa-gateway.mjs";

const digest = text => createHash("sha256").update(text).digest("hex");
function call(port, path, { method = "GET", headers = {}, body = "" } = {}) {
  return new Promise((resolve, reject) => {
    const req = request({ host: "127.0.0.1", port, path, method, headers }, res => {
      const chunks = []; res.on("data", chunk => chunks.push(chunk)); res.on("end", () => resolve({ status: res.statusCode, contentType: res.headers["content-type"], csp: res.headers["content-security-policy"], body: Buffer.concat(chunks).toString() }));
    }); req.on("error", reject); req.end(body);
  });
}
test("pinned gateway serves static assets and narrowly proxies same-origin requests", async () => {
  const root = await realpath(await mkdtemp(join(tmpdir(), "local-qa-gateway-test-")));
  let gateway, backend;
  try {
    await mkdir(join(root, "jobs")); await mkdir(join(root, "assets"));
    for (const [path, content] of [["index.html", "login"], ["jobs/index.html", "jobs"], ["assets/site.js", "script"]]) await writeFile(join(root, path), content);
    let captured, upstreamClosed = false;
    backend = createServer((req, res) => { const chunks = []; req.on("data", c => chunks.push(c)); req.on("end", () => { captured = { method: req.method, path: req.url, body: Buffer.concat(chunks).toString(), auth: req.headers.authorization }; if (req.url === "/auth/refresh") { res.writeHead(302, { location: "https://example.invalid/" }); res.end(); } else if (req.url === "/account/me") { res.writeHead(200, { "content-type": "application/json" }); const interval = setInterval(() => res.write(" "), 250); res.on("close", () => { clearInterval(interval); upstreamClosed = true; }); } else { res.writeHead(200, { "content-type": "application/json" }); res.end("{\"ok\":true}"); } }); });
    await new Promise(resolve => backend.listen(0, "127.0.0.1", resolve));
    gateway = await startLocalQaGateway({ webDirectory: root, manifest: { "/index.html": digest("login"), "/jobs/index.html": digest("jobs"), "/assets/site.js": digest("script") }, backendPort: backend.address().port,
      verifyBackend: async context => { assert.deepEqual(context, { host: "127.0.0.1", port: backend.address().port }); assert.equal(Object.isFrozen(context), true); return backend.listening; } });
    const port = Number(new URL(gateway.origin).port);
    assert.equal((await call(port, "/login?mode=signup&next=%2Fjobs")).body, "login");
    assert.equal((await call(port, "/jobs/profile")).body, "jobs");
    assert.equal((await call(port, "/assets/site.js?v=123")).body, "script");
    const version = digest("script").slice(0, 12);
    const contentVersion = await call(port, `/assets/site.js?v=${version}`);
    assert.equal(contentVersion.status, 200);
    assert.equal(contentVersion.body, "script");
    for (const path of [
      `/assets/site.js?v=${digest("jobs").slice(0, 12)}`,
      `/assets/site.js?v=${version.toUpperCase()}`,
      `/assets/site.js?v=${version}&other=1`,
      `/assets/missing.js?v=${version}`,
      `/jobs/index.html?v=${digest("jobs").slice(0, 12)}`,
    ]) assert.equal((await call(port, path)).status, 400, path);
    assert.equal(captured, undefined, "static version lookups never proxy upstream");
    assert.equal((await call(port, "/")).status, 302);
    const headers = { origin: gateway.origin, authorization: "Bearer synthetic", "content-type": "application/json" };
    assert.equal((await call(port, "/api/jobs/profile", { method: "PUT", headers, body: "{}" })).status, 200);
    assert.deepEqual(captured, { method: "PUT", path: "/api/jobs/profile", body: "{}", auth: "Bearer synthetic" });
    for (const [path, options] of [["/api/jobs/profile", { method: "PUT", body: "{}" }], ["/api/jobs/profile", { method: "PUT", headers: { origin: "https://example.invalid" } }], ["/api/jobs/applications", { method: "POST", headers }], ["/assets/%73ite.js", {}], ["/assets/site.js?other=1", {}], ["/jobs/../index.html", {}], ["/api/jobs/workspace", { headers: { host: "evil.invalid" } }], ["/api/jobs/profile", { method: "PATCH", headers }]]) assert.equal((await call(port, path, options)).status, 400);
    assert.equal((await call(port, "/auth/refresh", { method: "POST", headers, body: "{}" })).status, 502);
    await assert.rejects(call(port, "/api/jobs/resume-source", { method: "POST", headers, body: "x".repeat(15 * 1024 * 1024 + 1) }));
    const beforeTrickle = captured;
    const bodyStarted = Date.now();
    await new Promise((resolve, reject) => {
      const client = request({ host: "127.0.0.1", port, path: "/api/jobs/profile", method: "PUT", headers: { ...headers, "content-length": "1000" } });
      const interval = setInterval(() => client.write("x"), 250);
      const limit = setTimeout(() => { clearInterval(interval); client.destroy(); reject(new Error("body deadline failed")); }, 35000);
      client.on("response", response => { response.resume(); });
      client.on("error", () => {});
      client.on("close", () => { clearInterval(interval); clearTimeout(limit); resolve(); });
    });
    assert.ok(Date.now() - bodyStarted < 35000, "absolute deadline bounds a trickling request body");
    assert.equal(captured, beforeTrickle, "incomplete body never reaches backend");
    const started = Date.now();
    assert.equal((await call(port, "/account/me")).status, 502);
    assert.ok(Date.now() - started < 19000, "absolute deadline bounds a trickling response");
    assert.equal(upstreamClosed, true);
    upstreamClosed = false;
    await new Promise((resolve, reject) => {
      const client = request({ host: "127.0.0.1", port, path: "/account/me" });
      client.on("error", () => {}); client.end();
      setTimeout(() => client.destroy(), 500);
      const deadline = setTimeout(() => { clearInterval(poll); reject(new Error("upstream cancellation timed out")); }, 3000);
      const poll = setInterval(() => { if (upstreamClosed) { clearInterval(poll); clearTimeout(deadline); resolve(); } }, 25);
    });
  } finally {
    if (gateway) await gateway.close();
    if (backend) await new Promise(resolve => backend.close(resolve));
    await rm(root, { recursive: true });
  }
});

test("static digest mismatch refuses before listener creation", async () => {
  const root = await realpath(await mkdtemp(join(tmpdir(), "local-qa-gateway-test-")));
  try {
    await mkdir(join(root, "jobs")); await writeFile(join(root, "index.html"), "x"); await writeFile(join(root, "jobs/index.html"), "y");
    await assert.rejects(startLocalQaGateway({ webDirectory: root, manifest: { "/index.html": "0".repeat(64), "/jobs/index.html": digest("y") }, backendPort: 1 }), /local_qa_gateway_refused/);
  } finally { await rm(root, { recursive: true }); }
});

test("candidate readback serves only its measured résumé and allows repeat candidate sign-ins", async t => {
  const root = await realpath(await mkdtemp(join(tmpdir(), "local-qa-candidate-gateway-")));
  const accountId = "11111111-1111-4111-8111-111111111111";
  const resumeId = "22222222-2222-4222-8222-222222222222";
  const email = "candidate@bluey-qa.test";
  const loginBody = JSON.stringify({ email, password: `Qa-${"a".repeat(48)}` });
  const resume = `/api/jobs/resume-versions/${resumeId}`;
  let verify = true, invalidateOnReply = false, identity = accountId, admin = false, loginCount = 0;
  const seen = [];
  const backend = createServer(async (req, res) => {
    for await (const _ of req) { /* consume bounded fixture request */ }
    seen.push(req.url);
    if (invalidateOnReply) verify = false;
    res.setHeader("content-type", "application/json");
    res.end(JSON.stringify(req.url === "/auth/login" ? { account: {
      id: identity, email, is_admin: admin, is_temporary: false,
    }, access_token: `jwt.body.signature_${++loginCount}` } : { ok: true }));
  });
  await new Promise(resolve => backend.listen(0, "127.0.0.1", resolve));
  let gateway;
  t.after(async () => {
    await gateway?.close();
    await new Promise(resolve => backend.close(resolve));
    await rm(root, { recursive: true });
  });
  await mkdir(join(root, "jobs"));
  await writeFile(join(root, "index.html"), "login");
  await writeFile(join(root, "jobs/index.html"), "jobs");
  const base = { webDirectory: root, manifest: { "/index.html": digest("login"),
    "/jobs/index.html": digest("jobs") }, backendPort: backend.address().port, verifyBackend: () => verify };
  await assert.rejects(startLocalQaGateway({ ...base, candidateReadback: { accountId, resumeId, write: true } }),
    /local_qa_gateway_refused/);
  gateway = await startLocalQaGateway({ ...base, candidateReadback: { accountId, resumeId },
    expiresAtMs: Date.now() + 120000 });
  const port = Number(new URL(gateway.origin).port);
  const login = (body = loginBody) => call(port, "/auth/login", { method: "POST",
    headers: { origin: gateway.origin, "content-type": "application/json",
      "content-length": String(Buffer.byteLength(body)) }, body });
  const auth = count => ({ authorization: `Bearer jwt.body.signature_${count}` });
  const document = await call(port, "/jobs/applications");
  assert.equal(document.status, 200);
  assert.match(document.csp, /connect-src 'self'/);
  assert.equal((await call(port, resume, { headers: auth(1) })).status, 400);
  assert.equal((await login(loginBody.replace(email, "operator@bluey-qa.test"))).status, 400);
  assert.deepEqual(seen, []);
  assert.equal((await login()).status, 200);
  assert.equal((await login()).status, 200);
  for (const count of [1, 2]) {
    assert.equal((await call(port, "/api/jobs/workspace", { headers: auth(count) })).status, 200);
    assert.equal((await call(port, resume, { headers: auth(count) })).status, 200);
  }
  const accepted = seen.length;
  const blocked = [
    ["GET", resume.replace(resumeId, accountId)], ["GET", `${resume}?other=1`],
    ["GET", `/api/jobs/matches/${resumeId}`],
    ["GET", "/api/jobs/communication-actions?limit=100"],
    ["POST", "/api/jobs/applications"], ["PUT", "/api/jobs/profile"],
    ["POST", "/api/jobs/applications/approve"], ["POST", "/auth/refresh"],
    ["POST", "/auth/signup/start"], ["GET", "/admin/jobs/public-beta"],
  ];
  for (const [method, path] of blocked) {
    assert.equal((await call(port, path, { method, headers: { ...auth(1), origin: gateway.origin } })).status, 400);
  }
  assert.equal(seen.length, accepted);
  identity = resumeId;
  assert.equal((await login()).status, 502);
  assert.equal((await call(port, resume, { headers: auth(3) })).status, 400);
  identity = accountId; admin = true;
  assert.equal((await login()).status, 502);
  assert.equal((await call(port, resume, { headers: auth(4) })).status, 400);
  admin = false;
  invalidateOnReply = true;
  assert.equal((await login()).status, 502, "outgoing session refused when backend ownership changed");
  verify = true; invalidateOnReply = false;
  assert.equal((await call(port, resume, { headers: auth(5) })).status, 400);
  verify = false;
  const beforeRefusedOwner = seen.length;
  assert.equal((await call(port, resume, { headers: auth(2) })).status, 502);
  assert.equal(seen.length, beforeRefusedOwner);
});

test("C2C restart admits one exact match and only empty authenticated communications lists", async t => {
  const root = await realpath(await mkdtemp(join(tmpdir(), "local-qa-c2c-detail-")));
  const accountId = "11111111-1111-4111-8111-111111111111";
  const resumeId = "22222222-2222-4222-8222-222222222222";
  const c2cMatchId = "33333333-3333-4333-8333-333333333333";
  const c2cApplicationId = "44444444-4444-4444-8444-444444444444";
  const email = "candidate@bluey-qa.test", token = "synthetic.bound.token";
  const match = `/api/jobs/matches/${c2cMatchId}`;
  const communications = "/api/jobs/communication-actions?limit=100";
  const applicationCommunications = `${communications}&application_id=${c2cApplicationId}`;
  let communicationBody = [];
  const seen = [];
  const backend = createServer(async (req, res) => {
    for await (const _ of req) { /* consume admitted synthetic request */ }
    seen.push([req.method, req.url]);
    res.setHeader("content-type", "application/json");
    res.end(JSON.stringify(req.url === "/auth/login" ? { account: {
      id: accountId, email, is_admin: false, is_temporary: false,
    }, access_token: token } : req.url === match ? { id: c2cMatchId, employment_type: "contract c2c" }
      : communicationBody));
  });
  let gateway;
  t.after(async () => {
    await gateway?.close();
    await new Promise(resolve => backend.close(resolve));
    await rm(root, { recursive: true });
  });
  await new Promise(resolve => backend.listen(0, "127.0.0.1", resolve));
  await mkdir(join(root, "jobs"));
  await writeFile(join(root, "index.html"), "login");
  await writeFile(join(root, "jobs/index.html"), "jobs");
  const base = { webDirectory: root, manifest: { "/index.html": digest("login"),
    "/jobs/index.html": digest("jobs") }, backendPort: backend.address().port,
    verifyBackend: () => backend.listening, expiresAtMs: Date.now() + 120000 };
  const bound = { accountId, resumeId, c2cMatchId, c2cApplicationId };
  for (const binding of [{ ...bound, resumeId: null }, { ...bound, c2cMatchId: "wrong/path" },
    { ...bound, c2cApplicationId: "wrong/id" }, { accountId, resumeId, c2cMatchId }]) {
    await assert.rejects(startLocalQaGateway({ ...base, candidateReadback: binding }), /local_qa_gateway_refused/);
  }
  gateway = await startLocalQaGateway({ ...base, candidateReadback: bound });
  const port = Number(new URL(gateway.origin).port);
  const headers = { authorization: `Bearer ${token}`, origin: gateway.origin };
  assert.equal((await call(port, match, { headers })).status, 400);
  assert.equal((await call(port, communications, { headers })).status, 400);
  const loginBody = JSON.stringify({ email, password: `Qa-${"a".repeat(48)}` });
  assert.equal((await call(port, "/auth/login", { method: "POST", body: loginBody,
    headers: { origin: gateway.origin, "content-type": "application/json",
      "content-length": String(Buffer.byteLength(loginBody)) } })).status, 200);
  assert.equal((await call(port, match, { headers })).status, 200);
  for (const path of [communications, applicationCommunications]) {
    const reply = await call(port, path, { headers });
    assert.equal(reply.status, 200);
    assert.equal(reply.body, "[]");
  }
  for (const [method, path] of [["GET", match.replace(c2cMatchId, accountId)], ["GET", `${match}?fresh=1`],
    ["HEAD", match], ["POST", match], ["PUT", match], ["DELETE", match], ["POST", "/api/jobs/applications"],
    ["GET", communications.split("?")[0]], ["GET", communications.replace("100", "99")],
    ["GET", applicationCommunications.replace(c2cApplicationId, accountId)],
    ["GET", `${communications}&other=1`], ["HEAD", communications], ["POST", communications],
    ["GET", `/api/jobs/communication-actions/${c2cApplicationId}`]]) {
    assert.equal((await call(port, path, { method, headers })).status, 400);
  }
  assert.deepEqual(seen, [["POST", "/auth/login"], ["GET", match],
    ["GET", communications], ["GET", applicationCommunications]]);
  for (const body of [[{ synthetic: "never-forward" }], { actions: [] }, "not-json-array"]) {
    communicationBody = body;
    const reply = await call(port, applicationCommunications, { headers });
    assert.equal(reply.status, 502);
    assert.equal(reply.body, "Request refused\n");
  }
});

test("candidate gateway requires and enforces its bounded expiry", async t => {
  const root = await realpath(await mkdtemp(join(tmpdir(), "local-qa-candidate-expiry-")));
  const accountId = "11111111-1111-4111-8111-111111111111";
  const candidateReadback = { accountId, resumeId: null };
  await mkdir(join(root, "jobs"));
  await writeFile(join(root, "index.html"), "login");
  await writeFile(join(root, "jobs/index.html"), "jobs");
  const base = { webDirectory: root, manifest: { "/index.html": digest("login"),
    "/jobs/index.html": digest("jobs") }, backendPort: 1 };
  t.after(() => rm(root, { recursive: true }));
  for (const expiresAtMs of [undefined, NaN, Date.now() - 1, Date.now() + 2 * 60 * 60 * 1000 + 1000]) {
    await assert.rejects(startLocalQaGateway({ ...base, candidateReadback, expiresAtMs }),
      /local_qa_gateway_refused/);
  }
  await assert.rejects(startLocalQaGateway({ ...base, expiresAtMs: Date.now() + 1000 }),
    /local_qa_gateway_refused/);

  const gateway = await startLocalQaGateway({ ...base, candidateReadback,
    expiresAtMs: Date.now() + 150 });
  const port = Number(new URL(gateway.origin).port);
  assert.equal((await call(port, "/jobs/")).status, 200);
  await new Promise(resolve => setTimeout(resolve, 250));
  await assert.rejects(call(port, "/jobs/"));
  await gateway.close();
  await gateway.close();
});

test("inherited candidate listener accepts IPv4 loopback and refuses a Unix listener", async t => {
  const root = await realpath(await mkdtemp(join(tmpdir(), "jfd-")));
  const backend = createNetServer(socket => socket.destroy());
  const backendPath = join(root, "a");
  const wrongListenerPath = join(root, "b");
  t.after(async () => {
    if (backend.listening) await new Promise(resolve => backend.close(resolve));
    await rm(root, { recursive: true });
  });
  // Darwin's sockaddr_un has 104 path bytes including NUL. Node can report
  // successful listen while binding a truncated name, so check before bind.
  assert.ok([backendPath, wrongListenerPath].every(path => Buffer.byteLength(path) <= 103),
    "owned Unix listener fixture paths must fit the portable socket limit");
  await mkdir(join(root, "jobs"));
  await writeFile(join(root, "index.html"), "login");
  await writeFile(join(root, "jobs/index.html"), "jobs");
  await new Promise((resolve, reject) => {
    backend.once("error", reject);
    backend.listen(backendPath, resolve);
  });
  await chmod(backendPath, 0o600);
  const { uid, dev, ino } = await lstat(backendPath);
  const base = {
    webDirectory: root,
    manifest: { "/index.html": digest("login"), "/jobs/index.html": digest("jobs") },
    backendUnixSocket: { path: backendPath, root, uid, dev, ino, mode: 0o600 },
    candidateReadback: { accountId: "11111111-1111-4111-8111-111111111111", resumeId: null },
    ownedLoopbackFd: 3,
  };
  const childSource = `
    import { startLocalQaGateway } from ${JSON.stringify(new URL("./local-qa-gateway.mjs", import.meta.url).href)};
    const base = JSON.parse(process.argv[1]);
    try {
      const gateway = await startLocalQaGateway({ ...base, expiresAtMs: Date.now() + 5000,
        connectOwnedUnix() { throw new Error("unexpected_backend_connection"); } });
      await gateway.close();
      process.stdout.write(gateway.origin + "\\n");
    } catch (error) {
      process.stdout.write(error.name + ":" + error.message + "\\n");
    }
  `;
  const listenerOptions = [{ port: 0, host: "127.0.0.1" }, { path: wrongListenerPath }];
  for (const options of listenerOptions) {
    const listener = createNetServer(socket => socket.destroy());
    try {
      await new Promise((resolve, reject) => {
        listener.once("error", reject);
        listener.listen(options, resolve);
      });
      // Only the test uses Node's underlying listener FD to simulate the native parent.
      const fd = listener._handle?.fd;
      assert.ok(Number.isInteger(fd) && fd >= 0);
      const address = listener.address();
      const expected = options.host ? `http://127.0.0.1:${address.port}\n`
        : "Error:local_qa_gateway_refused\n";
      const observed = await new Promise((resolve, reject) => {
        const child = spawn(process.execPath, ["--input-type=module", "--eval", childSource,
          JSON.stringify(base)], {
          env: {}, stdio: ["ignore", "pipe", "pipe", fd], timeout: 10000, killSignal: "SIGKILL",
        });
        let stdout = "", stderr = "";
        child.stdout.setEncoding("utf8").on("data", chunk => { stdout += chunk; });
        child.stderr.setEncoding("utf8").on("data", chunk => { stderr += chunk; });
        child.once("error", reject);
        child.once("close", (code, signal) => resolve({ code, signal, stdout, stderr }));
      });
      assert.deepEqual(observed, { code: 0, signal: null, stderr: "", stdout: expected });
    } finally {
      if (listener.listening) await new Promise(resolve => listener.close(resolve));
    }
  }
});

test("candidate gateway does not send an in-flight response after expiry", async t => {
  const root = await realpath(await mkdtemp(join(tmpdir(), "local-qa-candidate-inflight-")));
  const accountId = "11111111-1111-4111-8111-111111111111";
  let backend;
  await mkdir(join(root, "jobs"));
  await writeFile(join(root, "index.html"), "login");
  await writeFile(join(root, "jobs/index.html"), "jobs");
  backend = createServer(async (req, res) => {
    for await (const _ of req) { /* consume request */ }
    res.setHeader("content-type", "application/json");
    if (req.url === "/auth/login") {
      res.end(JSON.stringify({ account: { id: accountId, email: "candidate@bluey-qa.test",
        is_admin: false, is_temporary: false }, access_token: "dummy.body.signature" }));
    } else {
      await new Promise(resolve => setTimeout(resolve, 500));
      res.end("{}");
    }
  });
  await new Promise(resolve => backend.listen(0, "127.0.0.1", resolve));
  let gateway;
  t.after(async () => {
    await gateway?.close();
    await new Promise(resolve => backend.close(resolve));
    await rm(root, { recursive: true });
  });
  gateway = await startLocalQaGateway({ webDirectory: root,
    manifest: { "/index.html": digest("login"), "/jobs/index.html": digest("jobs") },
    backendPort: backend.address().port, verifyBackend: () => true,
    candidateReadback: { accountId, resumeId: null }, expiresAtMs: Date.now() + 250 });
  const origin = gateway.origin;
  const loginBody = JSON.stringify({ email: "candidate@bluey-qa.test", password: `Qa-${"a".repeat(48)}` });
  assert.equal((await call(Number(new URL(origin).port), "/auth/login", { method: "POST",
    headers: { origin, "content-type": "application/json", "content-length": String(loginBody.length) },
    body: loginBody })).status, 200);
  await assert.rejects(call(Number(new URL(gateway.origin).port), "/account/me",
    { headers: { authorization: "Bearer dummy.body.signature" } }));
});

test("pinned module-worker assets are served with JavaScript MIME", async () => {
  const root = await realpath(await mkdtemp(join(tmpdir(), "local-qa-gateway-module-test-")));
  let gateway;
  try {
    await mkdir(join(root, "jobs"));
    const files = { "/index.html": "login", "/jobs/index.html": "jobs", "/jobs/pdf.worker.mjs": "export const worker = true;\n" };
    for (const [path, bytes] of Object.entries(files)) await writeFile(join(root, path.slice(1)), bytes);
    gateway = await startLocalQaGateway({ webDirectory: root,
      manifest: Object.fromEntries(Object.entries(files).map(([path, bytes]) => [path, digest(bytes)])),
      backendPort: 1 });
    const response = await call(Number(new URL(gateway.origin).port), "/jobs/pdf.worker.mjs");
    assert.equal(response.status, 200);
    assert.equal(response.contentType, "text/javascript; charset=utf-8");
    assert.equal(response.body, files["/jobs/pdf.worker.mjs"]);
  } finally {
    if (gateway) await gateway.close();
    await rm(root, { recursive: true });
  }
});

test("static symlinks refuse before listener creation", async () => {
  const root = await realpath(await mkdtemp(join(tmpdir(), "local-qa-gateway-test-")));
  try {
    await mkdir(join(root, "jobs")); await writeFile(join(root, "index.html"), "x"); await symlink("../index.html", join(root, "jobs/index.html"));
    await assert.rejects(startLocalQaGateway({ webDirectory: root, manifest: { "/index.html": digest("x"), "/jobs/index.html": digest("x") }, backendPort: 1 }));
  } finally { await rm(root, { recursive: true }); }
});

test("each upstream request requires a fresh bounded backend-owner approval", async () => {
  assert.equal(LOCAL_QA_GATEWAY_BACKEND_FENCE_VERSION, 1);
  const root = await realpath(await mkdtemp(join(tmpdir(), "local-qa-gateway-owner-test-")));
  let gateway, backend;
  try {
    await mkdir(join(root, "jobs"));
    await writeFile(join(root, "index.html"), "login");
    await writeFile(join(root, "jobs/index.html"), "jobs");
    let forwarded = 0, verified = 0, allow = true;
    backend = createServer((_req, res) => { forwarded++; res.end("{}"); });
    await new Promise(resolve => backend.listen(0, "127.0.0.1", resolve));
    const base = { webDirectory: root, manifest: { "/index.html": digest("login"), "/jobs/index.html": digest("jobs") }, backendPort: backend.address().port };
    gateway = await startLocalQaGateway(base);
    assert.equal((await call(Number(new URL(gateway.origin).port), "/jobs/")).status, 200);
    assert.equal((await call(Number(new URL(gateway.origin).port), "/account/me")).status, 502);
    assert.equal(forwarded, 0);
    await gateway.close();
    gateway = await startLocalQaGateway({ ...base, verifyBackend: async context => {
      assert.deepEqual(Object.keys(context), ["host", "port"]); assert.equal(Object.isFrozen(context), true);
      assert.equal(context.host, "127.0.0.1"); assert.equal(context.port, base.backendPort);
      verified++; if (allow === "throw") throw new Error("fixed synthetic refusal");
      if (allow === "wait") return new Promise(() => {});
      return allow;
    } });
    const port = Number(new URL(gateway.origin).port), headers = { authorization: "Bearer synthetic-do-not-forward-on-refusal" };
    assert.equal((await call(port, "/account/me", { headers })).status, 200);
    assert.equal(forwarded, 1);
    for (const decision of [false, "throw", "wait"]) {
      allow = decision;
      assert.equal((await call(port, "/account/me", { headers })).status, 502);
      assert.equal(forwarded, 1, "refused verification must not forward a request");
    }
    assert.equal(verified, 4, "approval is checked again for each request");
  } finally {
    if (gateway) await gateway.close();
    if (backend) await new Promise(resolve => backend.close(resolve));
    await rm(root, { recursive: true });
  }
});
