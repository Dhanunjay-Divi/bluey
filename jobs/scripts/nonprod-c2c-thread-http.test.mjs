// Detached HTTP contract fixture. No registration in the Jobs router.
import assert from "node:assert/strict";
import { createHash, randomBytes } from "node:crypto";
import { lstatSync, readFileSync, realpathSync } from "node:fs";
import { createServer, request } from "node:http";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";

const root = process.env.BLUEY_TEST_RUN_ROOT;
assert.match(root ?? "", /^\/(?:private\/)?tmp\/bluey-test-runs-[^/]+\/run\.[0-9]+\.[0-9]+\.[A-Za-z0-9]+$/);
assert.equal(realpathSync(root), root);
const rootInfo = lstatSync(root);
assert.ok(rootInfo.isDirectory() && rootInfo.uid === process.getuid() && (rootInfo.mode & 0o777) === 0o700);
const socket = join(root, "tmp", "c2c-thread-http.sock");
const now = 1_800_000_000_000;
const ownerLink = "d".repeat(32), otherLink = "e".repeat(32), outlookLink = "1".repeat(32);
const ownerToken = randomBytes(32).toString("base64url"), otherToken = randomBytes(32).toString("base64url");
const tokenKey = token => createHash("sha256").update(token).digest("hex");
const principals = new Map([[tokenKey(ownerToken), "owner"], [tokenKey(otherToken), "other"]]);
const here = name => new URL(name, import.meta.url);
const query = readFileSync(here("./nonprod-c2c-thread-read.select.sql"), "utf8");
assert.equal(tokenKey(query), "f9234005129fe19b9afbfd6d1e81a1333174597d77fdb76ec3f768d10114b85f");
const fixture = readFileSync(here("./nonprod-c2c-thread-read.test.mjs"), "utf8");
assert.equal(tokenKey(fixture), "146afdd32e227832107c7874fcc0be33361e0fd8e34b01767b38aeb3a5260c9e");
const template = /^function fixture\(db\) \{\n  db\.exec\(`([\s\S]*?)`\);/m.exec(fixture)?.[1];
assert.ok(template);
const seed = template
  .replace("${intentDDL}", readFileSync(here("../../infra/sqlite/server-runtime/043_account_deletion_intents.sql"), "utf8"))
  .replaceAll(/\$\{now ([+-]) ([0-9]+)\}/g,
    (_, operator, offset) => String(now + Number(offset) * (operator === "+" ? 1 : -1)))
  .replace("'owner-link'", `'${ownerLink}'`).replace("'other-link'", `'${otherLink}'`)
  .replace("'out-link'", `'${outlookLink}'`);
assert.ok(!seed.includes("${"));

function reply(res, status, payload) {
  const body = JSON.stringify(payload) + "\n";
  res.writeHead(status, {
    "Cache-Control": "private, no-store",
    "Content-Type": "application/json",
    "Content-Length": Buffer.byteLength(body),
    "X-Content-Type-Options": "nosniff",
    Vary: "Authorization",
  });
  res.end(body);
}

function fixtureHandler(db, inspected) {
  const exact = db.prepare(query);
  const unavailable = { error: "unavailable" };
  return (req, res) => {
    // Do not normalize a caller path or substitute a broad listing lookup.
    const link = /^\/fixture\/c2c\/threads\/([0-9a-f]{32})$/.exec(req.url ?? "")?.[1];
    if (!link || req.method !== "GET" || req.headers["transfer-encoding"] !== undefined ||
        (req.headers["content-length"] !== undefined && req.headers["content-length"] !== "0")) {
      reply(res, 404, unavailable);
      return;
    }
    const inputs = req.headersDistinct.authorization;
    const token = inputs?.length === 1 && /^Bearer ([A-Za-z0-9_-]{43})$/.exec(inputs[0])?.[1];
    const account = token && principals.get(tokenKey(token));
    if (!account) {
      reply(res, 401, { error: "authentication_required" });
      return;
    }
    inspected();
    const rows = exact.all({ $1: account, $2: link, $3: now });
    if (rows.length === 0 || rows.length > 250 || rows.some(row =>
      !["gmail", "outlook_email"].includes(row.provider) || row.direction !== "inbound" ||
      !Number.isSafeInteger(row.occurred_at_ms) || row.occurred_at_ms < 0 || row.occurred_at_ms > now)) {
      reply(res, 404, unavailable);
      return;
    }
    reply(res, 200, { messages: rows.map(({ provider, direction, occurred_at_ms }) => ({
      provider, direction, occurredAtMs: occurred_at_ms,
    })) });
  };
}

const route = id => `/fixture/c2c/threads/${id}`;
function call(path, { token, headers, method = "GET", body } = {}) {
  return new Promise((resolve, reject) => {
    const client = request({ socketPath: socket, path, method, timeout: 3000, agent: false,
      headers: { ...(token ? { authorization: `Bearer ${token}` } : {}), ...headers } }, response => {
      let raw = "";
      response.setEncoding("utf8");
      response.on("data", chunk => { raw += chunk; });
      response.on("error", reject);
      response.on("end", () => {
        try { resolve({ code: response.statusCode, headers: response.headers, raw, body: JSON.parse(raw) }); }
        catch (error) { reject(error); }
      });
    });
    client.on("error", reject);
    client.on("timeout", () => client.destroy(new Error("fixture request timeout")));
    client.end(body);
  });
}

test("fixture HTTP preserves authenticated owner scope, output privacy and committed revocation", async () => {
  const db = new DatabaseSync(":memory:");
  let server;
  let queries = 0;
  try {
    db.exec(seed);
    server = createServer({ requestTimeout: 3000, headersTimeout: 3000, maxHeaderSize: 4096 },
      fixtureHandler(db, () => { queries++; }));
    server.on("checkContinue", (_req, res) => reply(res, 404, { error: "unavailable" }));
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(socket, resolve);
    });
    assert.ok(lstatSync(socket).isSocket());
    const changes = () => db.prepare("SELECT total_changes() AS n").get().n;
    const baseline = changes();
    const owner = { token: ownerToken };
    const other = { token: otherToken };
    const allowed = await call(route(ownerLink), owner);
    assert.equal(allowed.code, 200);
    assert.deepEqual(allowed.body, { messages: [{ provider: "gmail", direction: "inbound", occurredAtMs: now - 10 }] });
    assert.equal(allowed.headers["cache-control"], "private, no-store");
    assert.equal(allowed.headers.vary, "Authorization");
    assert.equal(allowed.headers.etag, undefined);
    assert.equal(allowed.headers["set-cookie"], undefined);
    const foreign = await call(route(otherLink), { ...owner, headers: { "x-account-id": "other" } });
    const absent = await call(route("a".repeat(32)), owner);
    assert.equal(foreign.code, 404);
    assert.equal(absent.code, 404);
    assert.equal(foreign.raw, absent.raw);
    assert.equal(foreign.headers["cache-control"], allowed.headers["cache-control"]);
    assert.equal((await call(route(ownerLink), other)).raw, absent.raw);
    const allowedQueries = queries;
    for (const options of [{}, { token: randomBytes(32).toString("base64url") },
      { headers: { authorization: [`Bearer ${ownerToken}`, `Bearer ${otherToken}`] } }]) {
      const denied = await call(route(ownerLink), options);
      assert.equal(denied.code, 401);
      assert.deepEqual(denied.body, { error: "authentication_required" });
    }
    for (const [path, options] of [
      [route(ownerLink) + "?account_id=other", owner], [route(ownerLink) + "/", owner],
      [route("%64" + ownerLink.slice(1)), owner], [route(ownerLink.toUpperCase()), owner],
      [route(ownerLink), { ...owner, method: "POST" }],
      [route(ownerLink), { ...owner, body: "x", headers: { "content-length": 1 } }],
      [route(ownerLink), { ...owner, body: "x", headers: { "transfer-encoding": "chunked" } }],
    ]) {
      const denied = await call(path, options);
      assert.equal(denied.code, 404);
      assert.equal(denied.raw, absent.raw);
    }
    assert.equal(queries, allowedQueries, "bad authentication and malformed requests must not run SQL");
    assert.equal(changes(), baseline);
    db.prepare(`INSERT INTO fixture_thread_evidence
      VALUES ('owner','conn','gmail','thread','expired','outbound',?,?)`).run(now - 1, now);
    assert.equal((await call(route(ownerLink), owner)).raw, absent.raw);
    db.exec("DELETE FROM fixture_thread_evidence WHERE private_order_key='expired'");
    assert.equal((await call(route(ownerLink), owner)).code, 200);
    db.prepare("UPDATE fixture_thread_links SET revoked_at_ms=? WHERE id=?").run(now, ownerLink);
    assert.equal((await call(route(ownerLink), owner)).raw, absent.raw);
    assert.equal((await call(route(otherLink), other)).code, 200);
    db.prepare(`INSERT INTO account_deletion_intents
      (account_id,requested_at_ms,last_checked_at_ms,fresh_upload_cutoff_ms) VALUES (?,?,?,?)`)
      .run("owner", now, now, now);
    assert.equal((await call(route(outlookLink), owner)).raw, absent.raw);
    assert.equal((await call(route(otherLink), other)).code, 200);
    assert.equal(changes(), baseline + 4, "only test-coordinator mutations are allowed");
  } finally {
    try {
      if (server?.listening) await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    } finally { db.close(); }
  }
});
