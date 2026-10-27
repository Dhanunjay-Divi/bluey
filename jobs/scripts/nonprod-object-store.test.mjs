import assert from "node:assert/strict";
import { createHash, createHmac, randomUUID } from "node:crypto";
import { request as httpRequest } from "node:http";
import { createConnection } from "node:net";
import { test } from "node:test";
import { startNonprodObjectStore } from "./nonprod-object-store.mjs";

const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
const hmac = (key, data) => createHmac("sha256", key).update(data).digest();
const timestamp = (date = new Date()) => date.toISOString().replace(/[-:]|\.\d{3}/g, "");

// Separate test-side signer; the fixture exports no signing/verifier shortcut.
function sign(store, path, method, body = Buffer.alloc(0), overrides = {}) {
  const env = store.env;
  const dateTime = overrides.timestamp || timestamp();
  const day = dateTime.slice(0, 8);
  const hash = overrides.payloadHash || digest(body);
  const host = overrides.host || new URL(store.endpoint).host;
  const keyId = overrides.accessKey || env.BLUEY_OBJECT_ACCESS_KEY_ID;
  const scope = `${day}/us-east-1/s3/aws4_request`;
  const names = "host;x-amz-content-sha256;x-amz-date";
  const canonical = `${method}\n${path}\n\nhost:${host}\nx-amz-content-sha256:${hash}\nx-amz-date:${dateTime}\n\n${names}\n${hash}`;
  const key = [day, "us-east-1", "s3", "aws4_request"]
    .reduce((previous, field) => hmac(previous, field), `AWS4${env.BLUEY_OBJECT_SECRET_ACCESS_KEY}`);
  const signature = hmac(key, `AWS4-HMAC-SHA256\n${dateTime}\n${scope}\n${digest(canonical)}`).toString("hex");
  return {
    host, "x-amz-date": dateTime, "x-amz-content-sha256": hash,
    authorization: `AWS4-HMAC-SHA256 Credential=${keyId}/${scope}, SignedHeaders=${names}, Signature=${signature}`,
    ...(method === "PUT" ? { "content-type": "text/plain", "content-length": String(body.length) } : {}),
  };
}
function sourcePath(store, accountId, bytes, id = randomUUID()) {
  return `/${store.env.BLUEY_OBJECT_BUCKET}/${store.env.BLUEY_OBJECT_KEY_PREFIX}/accounts/${accountId}/jobs/resumes/${id}/sha256/${digest(bytes)}.txt`;
}
async function send(store, path, method, body, headers) {
  return new Promise((resolve, reject) => {
    const request = httpRequest(`${store.endpoint}`, { path, method, headers, agent: false }, (response) => {
      const chunks = [];
      response.on("data", (chunk) => chunks.push(chunk));
      response.on("end", () => resolve({ status: response.statusCode, headers: response.headers, bytes: Buffer.concat(chunks) }));
      response.on("error", reject);
    });
    request.setTimeout(3000, () => request.destroy(new Error("fixture_test_request_timeout")));
    request.on("error", reject);
    request.end(body);
  });
}
async function withStore(run) {
  const account = `ui_fixture_${randomUUID()}`;
  const store = await startNonprodObjectStore({ accountId: account });
  try { await run(store, account); } finally { await store.close(); }
}

test("synthetic account identity is required before binding a socket", async () => {
  for (const accountId of [undefined, "owner", "../account", `ui_isolated_${randomUUID()}`]) {
    await assert.rejects(startNonprodObjectStore({ accountId }), /synthetic_account_required/);
  }
});

test("signed source upload and exact readback retain one bounded object", async () => {
  await withStore(async (store, account) => {
    const bytes = Buffer.from("Synthetic source resume\nSoftware Engineer\n");
    const path = sourcePath(store, account, bytes);
    assert.equal((await send(store, path, "PUT", bytes, sign(store, path, "PUT", bytes))).status, 200);
    for (let index = 0; index < 2; index += 1) {
      const response = await send(store, path, "GET", undefined, sign(store, path, "GET"));
      assert.equal(response.status, 200);
      assert.equal(response.headers["content-type"], "text/plain");
      assert.equal(digest(response.bytes), digest(bytes));
    }
    store.assertStoredBytes(bytes);
    assert.deepEqual(store.counters(), { put: 1, get: 2, denied: 0, objects: 1 });
    assert.throws(() => store.assertStoredBytes(Buffer.from("wrong")), /bytes_mismatch/);
  });
});

test("signature, body hash, credential, date and signed-header changes deny without storage", async () => {
  await withStore(async (store, account) => {
    const bytes = Buffer.from("Synthetic bytes");
    const path = sourcePath(store, account, bytes);
    const good = sign(store, path, "PUT", bytes);
    const alteredSignature = good.authorization.slice(0, -1) + (good.authorization.endsWith("0") ? "1" : "0");
    const requests = [
      { headers: { ...good, authorization: alteredSignature } },
      { headers: sign(store, path, "PUT", bytes, { accessKey: "wrongcredential" }) },
      { headers: sign(store, path, "PUT", bytes, { timestamp: timestamp(new Date(Date.now() - 3600_000)) }) },
      { headers: sign(store, path, "PUT", bytes, { timestamp: timestamp(new Date(Date.now() + 3600_000)) }) },
      { headers: { ...good, "x-amz-security-token": "not-supported" } },
      { headers: { ...good, authorization: good.authorization.replace("us-east-1", "us-west-2") } },
      { headers: good, body: Buffer.from("Synthetic bytez") },
      { headers: sign(store, path, "PUT", bytes, { payloadHash: "0".repeat(64) }) },
    ];
    for (const item of requests) {
      assert.equal((await send(store, path, "PUT", item.body || bytes, item.headers)).status, 403);
    }
    assert.deepEqual(store.counters(), { put: 0, get: 0, denied: requests.length, objects: 0 });
  });
});

test("foreign namespaces, traversal, query strings and host changes are rejected", async () => {
  await withStore(async (store, account) => {
    const bytes = Buffer.from("Synthetic bytes");
    const path = sourcePath(store, account, bytes);
    const paths = [path.replace(account, `ui_fixture_${randomUUID()}`),
      path.replace(store.env.BLUEY_OBJECT_BUCKET, "otherbucket"),
      path.replace(store.env.BLUEY_OBJECT_KEY_PREFIX, "otherprefix"),
      `${path}?versionId=1`, path.replace("/jobs/", "/../jobs/"),
      path.replace("/jobs/", "/%2E%2E/jobs/"), path.replace("/jobs/", "/jobs%2F"),
      `http://127.0.0.1${path}`];
    for (const badPath of paths) {
      assert.equal((await send(store, badPath, "PUT", bytes, sign(store, badPath, "PUT", bytes))).status, 403);
    }
    assert.equal((await send(store, path, "PUT", bytes, sign(store, path, "PUT", bytes, { host: "localhost:1" }))).status, 403);
    assert.deepEqual(store.counters(), { put: 0, get: 0, denied: paths.length + 1, objects: 0 });
  });
});

test("unsupported methods, content types, oversized bodies and a second object fail closed", async () => {
  await withStore(async (store, account) => {
    const bytes = Buffer.from("Synthetic bytes");
    const path = sourcePath(store, account, bytes);
    for (const method of ["DELETE", "POST", "HEAD"]) {
      assert.equal((await send(store, path, method, undefined, sign(store, path, method))).status, 405);
    }
    const headers = sign(store, path, "PUT", bytes);
    assert.equal((await send(store, path, "PUT", bytes, { ...headers, "content-type": "application/pdf" })).status, 415);
    const large = Buffer.alloc(1024 * 1024 + 1, 65);
    const largePath = sourcePath(store, account, large);
    assert.equal((await send(store, largePath, "PUT", large, sign(store, largePath, "PUT", large))).status, 413);
    assert.equal((await send(store, path, "PUT", bytes, headers)).status, 200);
    const secondPath = sourcePath(store, account, bytes);
    assert.equal((await send(store, secondPath, "PUT", bytes, sign(store, secondPath, "PUT", bytes))).status, 409);
    store.assertStoredBytes(bytes);
    assert.deepEqual(store.counters(), { put: 1, get: 0, denied: 6, objects: 1 });
  });
});

test("close is repeatable, clears bytes and makes the owned listener unavailable", async () => {
  const account = `ui_fixture_${randomUUID()}`;
  const store = await startNonprodObjectStore({ accountId: account });
  const bytes = Buffer.from("Synthetic bytes");
  const path = sourcePath(store, account, bytes);
  let idle;
  let idleClosed;
  try {
    assert.equal((await send(store, path, "PUT", bytes, sign(store, path, "PUT", bytes))).status, 200);
    idle = createConnection({ host: "127.0.0.1", port: Number(new URL(store.endpoint).port) });
    idle.on("error", () => {});
    idleClosed = new Promise((resolve) => idle.once("close", resolve));
    await new Promise((resolve, reject) => { idle.once("connect", resolve); idle.once("error", reject); });
  } finally { await store.close(); }
  await idleClosed;
  assert.equal(idle.destroyed, true);
  await store.close();
  assert.equal(store.counters().objects, 0);
  assert.throws(() => store.assertStoredBytes(bytes), /expected_one_source/);
  await assert.rejects(send(store, path, "GET", undefined, sign(store, path, "GET")), { code: "ECONNREFUSED" });
});

test("a missing exact object denies and exact PUT retries never create another object", async () => {
  await withStore(async (store, account) => {
    const bytes = Buffer.from("Synthetic bytes");
    const path = sourcePath(store, account, bytes);
    assert.equal((await send(store, path, "GET", undefined, sign(store, path, "GET"))).status, 404);
    for (let index = 0; index < 2; index += 1) {
      assert.equal((await send(store, path, "PUT", bytes, sign(store, path, "PUT", bytes))).status, 200);
    }
    store.assertStoredBytes(bytes);
    assert.deepEqual(store.counters(), { put: 2, get: 0, denied: 1, objects: 1 });
  });
});
