import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { test } from "node:test";
import { getNonprodJson, redactedChildLiveness } from "./nonprod-http-read.mjs";

async function withServer(handler, action) {
  const server = createServer(handler);
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  try { return await action(`http://127.0.0.1:${server.address().port}`); }
  finally { await new Promise((resolve) => server.close(resolve)); }
}

async function rejected(promise) {
  let failure;
  try { await promise; } catch (error) { failure = error; }
  assert(failure instanceof Error, "request unexpectedly passed");
  return failure;
}

test("401 429 and 500 retain only finite redacted response evidence", async () => {
  const token = "fixture-token-must-not-appear";
  const operations = ["account_before", "workspace_before", "resume_before",
    "account_after", "workspace_after", "resume_after", "isolated_workspace"];
  const statuses = [401, 429, 500];
  const cases = operations.map((operation, index) => [statuses[index % statuses.length],
    operation, index === 1 ? Buffer.from([0, 255, 10, 13])
      : Buffer.from(`private response ${index}`)]);
  for (const [status, operation, body] of cases) {
    await withServer((_request, response) => {
      response.writeHead(status, { "content-type": "application/octet-stream" });
      response.end(body);
    }, async (origin) => {
      const error = await rejected(getNonprodJson(`${origin}/private`, token, operation));
      assert.equal(error.message, "human_http_read_status");
      assert.deepEqual(error.httpReadDiagnostic, {
        operation, status, response_bytes: body.length,
        response_sha256: createHash("sha256").update(body).digest("hex"),
      });
      const serialized = JSON.stringify(error.httpReadDiagnostic);
      assert(!serialized.includes(token));
      assert(!serialized.includes(body.toString("utf8")));
      assert.deepEqual(Object.keys(error.httpReadDiagnostic).sort(),
        ["operation", "response_bytes", "response_sha256", "status"]);
    });
  }
});

test("invalid operations are refused before a request", async () => {
  let requests = 0;
  await withServer((_request, response) => {
    requests += 1; response.writeHead(500).end("unused");
  }, async (origin) => {
    for (const operation of ["", "account", "workspace/../../token", 7, {}]) {
      const error = await rejected(Promise.resolve()
        .then(() => getNonprodJson(origin, "token", operation)));
      assert.equal(error.code, "ERR_ASSERTION");
      assert.match(error.message, /^http_read_operation/);
    }
  });
  assert.equal(requests, 0);
});

test("oversize and non-json responses fail without response contents", async () => {
  await withServer((request, response) => {
    if (request.url === "/oversize") response.end(Buffer.alloc(1024 * 1024 + 1, 65));
    else response.end("not-json-private-content");
  }, async (origin) => {
    const oversize = await rejected(getNonprodJson(`${origin}/oversize`, "token",
      "workspace_before"));
    assert.equal(oversize.message, "workspace_body_limit");
    assert.equal(oversize.httpReadDiagnostic, undefined);
    const invalid = await rejected(getNonprodJson(`${origin}/invalid`, "token",
      "workspace_before"));
    assert.equal(invalid.message, "workspace_json");
    assert.equal(invalid.httpReadDiagnostic, undefined);
    assert.deepEqual(Object.keys(oversize), []);
    assert.deepEqual(Object.keys(invalid), []);
    assert(!oversize.message.includes("private-content"));
    assert(!invalid.message.includes("private-content"));
  });
});

test("successful JSON and legacy untagged failures retain existing behavior", async () => {
  await withServer((request, response) => {
    if (request.url === "/ok") response.end('{"ok":true}');
    else response.writeHead(503).end("private unavailable");
  }, async (origin) => {
    assert.deepEqual(await getNonprodJson(`${origin}/ok`, "token"), { ok: true });
    const error = await rejected(getNonprodJson(`${origin}/failure`, "token"));
    assert.equal(error.message, "workspace_status");
    assert.equal(error.httpReadDiagnostic, undefined);
  });
});

test("child liveness excludes process identity command and stderr", () => {
  const secret = "must-not-appear";
  const evidence = redactedChildLiveness([
    { name: "bluey-jobs-api", processHandle: { pid: 4001, exitCode: null,
      signalCode: null, command: secret } },
    { name: "bluey-server", processHandle: { pid: 4002, exitCode: 1,
      signalCode: null, stderr: secret } },
    { name: "packet-fixture", processHandle: { pid: 4003, exitCode: 0,
      signalCode: null } },
    { name: secret, processHandle: { pid: 4004, exitCode: null, signalCode: null } },
  ]);
  assert.deepEqual(evidence, [
    { component: "jobs_api", running: true, exit_code: null, signal: null },
    { component: "account_api", running: false, exit_code: 1, signal: null },
    { component: "packet_fixture", running: false, exit_code: 0, signal: null },
  ]);
  assert(!JSON.stringify(evidence).includes(secret));
  assert(!JSON.stringify(evidence).includes("400"));
});
