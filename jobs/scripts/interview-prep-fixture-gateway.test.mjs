import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { test } from "node:test";
import { startInterviewPrepFixtureGateway } from "../portal/nonprod/interview-prep-fixture-gateway.mjs";

const accountId = "ui_fixture_00000000-0000-4000-8000-000000000000";
const accessToken = "fixture.payload.synthetic";
const applicationId = "fixture-application";
const resumeVersionId = "fixture-resume";
const receiptFingerprint = "a".repeat(64);
const prepPath = `/api/jobs/applications/${applicationId}/interview-prep`;
const validBody = JSON.stringify({ prepare: true, receipt_fingerprint: receiptFingerprint });
const headers = { authorization: `Bearer ${accessToken}` };
const post = { method: "POST", headers: { ...headers, "content-type": "application/json" }, body: validBody };

async function fixture(t, reply = (_req, res) => res.writeHead(200, {
  "content-type": "application/json", "x-test-private": "never forward",
}).end('{"peer":true}')) {
  const received = [];
  const backend = createServer(async (req, res) => {
    const chunks = [];
    for await (const chunk of req) chunks.push(chunk);
    received.push({ method: req.method, url: req.url, headers: req.headers,
      body: Buffer.concat(chunks).toString() });
    reply(req, res);
  });
  await new Promise((resolve, reject) => {
    backend.once("error", reject);
    backend.listen(0, "127.0.0.1", resolve);
  });
  t.after(() => new Promise((resolve) => { backend.close(resolve); backend.closeAllConnections(); }));
  const html = Buffer.from("<!doctype html><title>Receipt fixture</title>");
  const config = { jobsPort: backend.address().port, accountId, accessToken, balanceCents: 0,
    applicationId, resumeVersionId, receiptFingerprint,
    assetBytes: new Map([["/jobs/index.html", html], ["/jobs/assets/synthetic.js", Buffer.from("void 0;")],
      ["/jobs/assets/worker.mjs", Buffer.from("void 0;")], ["/jobs/us-locations.json", Buffer.from("[]")]]) };
  const gateway = await startInterviewPrepFixtureGateway(config);
  t.after(() => gateway.close());
  const call = (path, init) => fetch(gateway.origin + path, { signal: AbortSignal.timeout(3000), ...init });
  return { received, config, gateway, call, html };
}

test("serves an immutable portal snapshot and exact synthetic account read", async (t) => {
  const { call, config, gateway, received, html } = await fixture(t);
  const text = html.toString();
  html.fill(0);
  config.assetBytes.clear();
  const portal = await call("/jobs/applications");
  assert.equal(portal.status, 200);
  assert.equal(await portal.text(), text);
  assert.match(portal.headers.get("content-security-policy"), /connect-src 'self'/);
  assert.equal((await call("/jobs/assets/synthetic.js")).status, 200);
  assert.equal((await call("/jobs/assets/worker.mjs")).headers.get("content-type"), "text/javascript");
  assert.deepEqual(await (await call("/jobs/us-locations.json")).json(), []);
  assert.equal((await call("/jobs/assets/synthetic.js?extra=1")).status, 404);
  assert.equal((await call("/assets/favicon-512.png")).status, 204);
  assert.equal((await call("/account/me")).status, 401);
  assert.deepEqual(await (await call("/account/me", { headers })).json(), {
    email: `${accountId}@example.test`, balance_cents: 0,
  });
  assert.deepEqual(received, []);
  assert.equal(gateway.census().staticResponses, 4);
});

test("only forwards the five owned read targets and the first exact prep POST", async (t) => {
  const { call, gateway, received } = await fixture(t);
  const reads = ["/api/jobs/beta-access", "/api/jobs/workspace",
    "/api/jobs/communication-actions?limit=100",
    `/api/jobs/communication-actions?limit=100&application_id=${applicationId}`,
    `/api/jobs/resume-versions/${resumeVersionId}`];
  for (const path of reads) {
    const response = await call(path, { headers: { ...headers, cookie: "private=fixture" } });
    assert.equal(response.status, 200);
    assert.equal(response.headers.get("x-test-private"), null);
    assert.deepEqual(await response.json(), { peer: true });
  }
  const prep = await Promise.all([call(prepPath, post), call(prepPath, post)]);
  assert.deepEqual(prep.map((res) => res.status).sort(), [200, 409]);
  assert.deepEqual(received.map(({ url }) => url), [...reads, prepPath]);
  assert(received.every(({ headers: seen }) => seen.authorization === headers.authorization
    && seen.cookie === undefined));
  assert.equal(received.at(-1).body, validBody);
  assert.equal(received.at(-1).method, "POST");
  assert.deepEqual(gateway.census(), {
    prepForwardAttempts: 1, readForwardAttempts: 5, staticResponses: 0,
    prepRequestSha256: createHash("sha256").update(`POST\n${prepPath}\n${validBody}`).digest("hex"),
  });
  await gateway.close();
  await gateway.close();
  await assert.rejects(call("/jobs/applications"));
});

test("refuses different sessions, wrong consent and non-fixture routes before forwarding", async (t) => {
  const { call, gateway, received } = await fixture(t);
  const denies = [
    ["/api/jobs/workspace", { headers: { authorization: "Bearer wrong" } }, 401],
    ["/api/jobs/workspace", { headers: { ...headers, origin: "https://fixture.example.test" } }, 403],
    ["/api/jobs/workspace?extra=1", { headers }, 404],
    ["/api/jobs/communication-actions?limit=100&application_id=wrong", { headers }, 404],
    ["/api/jobs/resume-versions/other", { headers }, 404],
    ["/api/jobs/applications/other/interview-prep", post, 404],
    ["/api/jobs/applications/fixture-application/approve", post, 404],
    ["/api/jobs/internal/lease", post, 404],
    ["/auth/login", post, 404],
    [prepPath, { ...post, headers: { "content-type": "application/json" } }, 401],
    [prepPath, { ...post, body: JSON.stringify({ prepare: false, receipt_fingerprint: receiptFingerprint }) }, 400],
    [prepPath, { ...post, body: validBody.replace(/a{64}/, "b".repeat(64)) }, 400],
  ];
  for (const [path, init, status] of denies) assert.equal((await call(path, init)).status, status);
  assert.deepEqual(received, []);
  assert.deepEqual(gateway.census(), {
    prepForwardAttempts: 0, readForwardAttempts: 0, staticResponses: 0, prepRequestSha256: null,
  });
});

test("candidate workspace read leaves receipt-worker authority behind the gateway", async (t) => {
  const { call, received, gateway } = await fixture(t);
  const workerHeaders = {
    "x-bluey-jobs-worker-id": "synthetic-worker-only",
    "x-bluey-jobs-worker-scope": "receipt",
    "x-bluey-jobs-worker-signature": "synthetic-signature-never-forwarded",
  };
  const receiptPath = `/api/jobs/internal/applications/${applicationId}/receipt`;
  for (const authorization of [headers.authorization, "Bearer synthetic-worker-only"]) {
    const denied = await call(receiptPath, {
      method: "POST",
      headers: { ...workerHeaders, authorization, "content-type": "application/json" },
      body: "{}",
    });
    assert.equal(denied.status, 404);
  }
  assert.deepEqual(received, []);
  const read = await call("/api/jobs/workspace", { headers: { ...headers, ...workerHeaders } });
  assert.equal(read.status, 200);
  assert.equal(read.headers.get("cache-control"), "no-store");
  assert.equal(received.length, 1);
  assert.equal(received[0].headers.authorization, headers.authorization);
  for (const name of Object.keys(workerHeaders)) assert.equal(received[0].headers[name], undefined);
  assert.equal(gateway.census().prepForwardAttempts, 0);
});

test("a lost upstream response cannot reopen the once-only prep gate", async (t) => {
  const { call, gateway, received } = await fixture(t, (_req, res) => res.destroy());
  assert.equal((await call(prepPath, post)).status, 502);
  assert.equal((await call(prepPath, post)).status, 409);
  assert.equal(received.length, 1);
  assert.equal(gateway.census().prepForwardAttempts, 1);
});

test("redirects do not forward account credentials or reopen prep", async (t) => {
  const { call, received } = await fixture(t, (_req, res) =>
    res.writeHead(302, { location: "/api/jobs/workspace", "content-type": "application/json" }).end("{}"));
  assert.equal((await call(prepPath, post)).status, 502);
  assert.equal((await call(prepPath, post)).status, 409);
  assert.deepEqual(received.map(({ url }) => url), [prepPath]);
});

test("returns an admissible Jobs API refusal without attempting a second prep", async (t) => {
  const { call, received } = await fixture(t, (_req, res) =>
    res.writeHead(409, { "content-type": "text/plain; charset=utf-8" }).end("stale fixture receipt"));
  const reply = await call(prepPath, post);
  assert.equal(reply.status, 409);
  assert.equal(await reply.text(), "stale fixture receipt");
  assert.equal((await call(prepPath, post)).status, 409);
  assert.deepEqual(received.map(({ url }) => url), [prepPath]);
});

test("close interrupts pending upstream work and refuses new connections", { timeout: 4000 }, async (t) => {
  const arrival = Promise.withResolvers();
  const disconnected = Promise.withResolvers();
  const { call, gateway, received } = await fixture(t, (_req, res) => {
    res.once("close", disconnected.resolve);
    arrival.resolve();
  });
  const failure = assert.rejects(call(prepPath, post));
  await arrival.promise;
  await gateway.close();
  await disconnected.promise;
  await failure;
  await assert.rejects(call("/jobs/applications"));
  assert.equal(received.length, 1);
});

test("refuses non-fixture owner or an unsafe static inventory", async (t) => {
  const { config } = await fixture(t);
  await assert.rejects(startInterviewPrepFixtureGateway({ ...config, accountId: "other" }),
    { message: "prep_gateway_binding" });
  await assert.rejects(startInterviewPrepFixtureGateway({
    ...config, assetBytes: new Map([["/jobs/../index.html", Buffer.from("unsafe")]]),
  }), { message: "prep_gateway_binding" });
});
