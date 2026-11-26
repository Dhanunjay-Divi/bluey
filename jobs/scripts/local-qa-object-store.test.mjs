import test from "node:test";
import assert from "node:assert/strict";
import { createHash, createHmac, randomUUID } from "node:crypto";
import { request as httpRequest } from "node:http";
import { chmodSync, linkSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { initializeLocalQaObjectDirectory, LOCAL_QA_OBJECT_MARKER, startLocalQaObjectStore } from "./local-qa-object-store.mjs";

// Independent test signer. Never imports an implementation verifier or key helper.
const digest = (v) => createHash("sha256").update(v).digest("hex");
const hmac = (k, v) => createHmac("sha256", k).update(v).digest();
const config = { qaId: "11111111-1111-4111-8111-111111111111", bucket: "qa-" + "a".repeat(16),
  prefix: "qa-" + "b".repeat(16), accessKey: "qa" + "c".repeat(16), secret: "d".repeat(64) };
const mime = { txt: "text/plain", pdf: "application/pdf", docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document" };
const storesByDirectory = new Map();
function key(bytes, ext, account = "qa_account_1", id = randomUUID()) {
  return `/${config.bucket}/${config.prefix}/accounts/${account}/jobs/resumes/${id}/sha256/${digest(bytes)}.${ext}`;
}
function signed(endpoint, method, path, bytes = Buffer.alloc(0), type, options = {}) {
  const host = new URL(endpoint).host;
  const dateTime = options.date || new Date().toISOString().replace(/[-:]|\.\d{3}/g, "");
  const date = dateTime.slice(0, 8), payload = digest(bytes);
  const canonicalHeaders = `host:${host}\nx-amz-content-sha256:${payload}\nx-amz-date:${dateTime}\n`;
  const canonical = `${method}\n${path}\n\n${canonicalHeaders}\nhost;x-amz-content-sha256;x-amz-date\n${payload}`;
  const scope = `${date}/us-east-1/s3/aws4_request`;
  let signing = hmac(`AWS4${options.secret || config.secret}`, date);
  for (const part of ["us-east-1", "s3", "aws4_request"]) signing = hmac(signing, part);
  const signature = hmac(signing, `AWS4-HMAC-SHA256\n${dateTime}\n${scope}\n${digest(canonical)}`).toString("hex");
  const headers = { host, "x-amz-date": dateTime, "x-amz-content-sha256": payload,
    authorization: `AWS4-HMAC-SHA256 Credential=${config.accessKey}/${scope}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=${signature}`,
    "content-length": String(bytes.length) };
  if (type) headers["content-type"] = type;
  Object.assign(headers, options.headers || {});
  const wireHeaders = options.duplicateAuthorization
    ? [...Object.entries(headers).flat(), "authorization", headers.authorization] : headers;
  return new Promise((resolve, reject) => {
    const req = httpRequest(endpoint + path, { method, headers: wireHeaders }, (res) => {
      const chunks = []; res.on("data", (chunk) => chunks.push(chunk));
      res.on("end", () => resolve({ status: res.statusCode, headers: res.headers, body: Buffer.concat(chunks) }));
    });
    req.on("error", reject);
    // Exercise header admission without racing a multi-MiB write against an
    // early final rejection/connection close. This still requires a real 413.
    req.end(options.headersOnly ? undefined : bytes);
  });
}
function owned(t) {
  const parent = realpathSync(mkdtempSync(join(tmpdir(), "bluey-local-qa-object-test-"))); chmodSync(parent, 0o700);
  const directory = join(parent, "objects"); mkdirSync(directory, { mode: 0o700 });
  const initialized = initializeLocalQaObjectDirectory({ ...config, directory });
  assert.equal(initialized.markerFilename, LOCAL_QA_OBJECT_MARKER);
  storesByDirectory.set(directory, []);
  t.after(async () => {
    for (const store of storesByDirectory.get(directory)) await store.close();
    storesByDirectory.delete(directory);
    rmSync(parent, { recursive: true, force: true });
  });
  return directory;
}
async function start(t, directory) {
  const store = await startLocalQaObjectStore({ ...config, directory });
  storesByDirectory.get(directory).push(store);
  return store;
}

test("TXT PDF DOCX revisions, idempotent retry and delete survive restart", async (t) => {
  const dir = owned(t), values = [Buffer.from("synthetic txt"), Buffer.from("%PDF-synthetic"), Buffer.from("PK synthetic docx")];
  let store = await start(t, dir);
  assert(!JSON.stringify(store).includes(config.secret));
  const keys = values.map((v, i) => key(v, ["txt", "pdf", "docx"][i]));
  for (let i = 0; i < keys.length; i++) {
    const ext = ["txt", "pdf", "docx"][i];
    assert.equal((await signed(store.endpoint, "PUT", keys[i], values[i], mime[ext])).status, 200);
  }
  assert.equal((await signed(store.endpoint, "PUT", keys[0], values[0], mime.txt)).status, 200);
  await store.close(); store = await start(t, dir);
  for (let i = 0; i < keys.length; i++) {
    const result = await signed(store.endpoint, "GET", keys[i]);
    assert.equal(result.status, 200); assert.deepEqual(result.body, values[i]);
    assert.equal(result.headers["content-type"], mime[["txt", "pdf", "docx"][i]]);
  }
  assert.equal((await signed(store.endpoint, "DELETE", keys[1])).status, 204);
  await store.close(); store = await start(t, dir);
  assert.equal((await signed(store.endpoint, "GET", keys[1])).status, 404);
  assert.deepEqual((await signed(store.endpoint, "GET", keys[0])).body, values[0]);
});

test("rejects foreign paths, malformed signatures, dates, methods and content", async (t) => {
  const dir = owned(t), store = await start(t, dir), body = Buffer.from("private synthetic body"), path = key(body, "txt");
  const cases = [
    ["GET", `/${config.bucket}/${config.prefix}/accounts/other/jobs/resumes/`, Buffer.alloc(0), undefined, {}],
    ["GET", path + "?list-type=2", Buffer.alloc(0), undefined, {}],
    ["GET", path.replace("/accounts/", "/%61ccounts/"), Buffer.alloc(0), undefined, {}],
    ["GET", path, Buffer.alloc(0), undefined, { secret: "e".repeat(64) }],
    ["GET", path, Buffer.alloc(0), undefined, { date: "20200101T000000Z" }],
    ["GET", path, Buffer.alloc(0), undefined, { duplicateAuthorization: true }],
    ["GET", path.replace("/accounts/", "/../accounts/"), Buffer.alloc(0), undefined, {}],
    ["POST", path, Buffer.alloc(0), undefined, {}],
    ["PUT", path, body, mime.pdf, {}],
    ["PUT", path, Buffer.from("changed"), mime.txt, {}],
    ["PUT", path, body, mime.txt, { headers: { "x-amz-meta-extra": "no" } }],
  ];
  for (const [index, [method, target, value, type, options]] of cases.entries()) {
    const result = await signed(store.endpoint, method, target, value, type, options)
      .catch(error => { throw new Error(`synthetic_denial_case_${index}_${error.code || "transport"}`); });
    assert(result.status >= 400); assert.equal(result.body.length, 0);
    assert(!JSON.stringify(result.headers).includes(config.secret));
    assert(!JSON.stringify(result.headers).includes(body.toString()));
  }
});

test("oversized declared length returns 413 before body and never creates an object", async t => {
  const dir = owned(t), store = await start(t, dir), bytes = Buffer.alloc(10 * 1024 * 1024 + 1);
  const path = key(bytes, "txt");
  const result = await signed(store.endpoint, "PUT", path, bytes, mime.txt, { headersOnly: true });
  assert.equal(result.status, 413); assert.equal(result.body.length, 0);
  assert.equal((await signed(store.endpoint, "GET", path)).status, 404);
  assert.equal(store.counters().put, 0);
  assert.equal(readdirSync(dir).filter(name => name.endsWith(".record")).length, 0);
});

test("concurrent identical writes commit one exact record", async (t) => {
  const dir = owned(t), store = await start(t, dir), value = Buffer.from("concurrent"), path = key(value, "txt");
  const results = await Promise.all(Array.from({ length: 4 }, () => signed(store.endpoint, "PUT", path, value, mime.txt)));
  assert.deepEqual(results.map((x) => x.status), [200, 200, 200, 200]);
  assert.equal(readdirSync(dir).filter((x) => x.endsWith(".record")).length, 1);
  assert.deepEqual((await signed(store.endpoint, "GET", path)).body, value);
});

test("credential mismatch and record corruption fail closed after restart", async (t) => {
  const dir = owned(t); let store = await start(t, dir);
  const bytes = Buffer.from("corruption test"), path = key(bytes, "txt");
  assert.equal((await signed(store.endpoint, "PUT", path, bytes, mime.txt)).status, 200);
  await store.close();
  await assert.rejects(startLocalQaObjectStore({ ...config, secret: "e".repeat(64), directory: dir }), /local_qa_object_store_start_failed/);
  const record = join(dir, readdirSync(dir).find((x) => x.endsWith(".record")));
  const data = readFileSync(record); data[data.length - 1] ^= 1; writeFileSync(record, data);
  await assert.rejects(startLocalQaObjectStore({ ...config, directory: dir }), /local_qa_object_store_start_failed/);
});

test("symlink and hardlink substitution fail closed", async (t) => {
  const dir = owned(t); let store = await start(t, dir);
  const bytes = Buffer.from("links"), path = key(bytes, "txt");
  assert.equal((await signed(store.endpoint, "PUT", path, bytes, mime.txt)).status, 200); await store.close();
  const name = readdirSync(dir).find((x) => x.endsWith(".record")), record = join(dir, name);
  const alias = join(dir, "alias"); linkSync(record, alias);
  await assert.rejects(startLocalQaObjectStore({ ...config, directory: dir }), /local_qa_object_store_start_failed/);
  rmSync(alias); rmSync(record); symlinkSync("/dev/null", record);
  await assert.rejects(startLocalQaObjectStore({ ...config, directory: dir }), /local_qa_object_store_start_failed/);
});

test("closed metadata, ownership modes and root aliases are checked", async (t) => {
  const dir = owned(t); let store = await start(t, dir);
  const bytes = Buffer.from("header integrity"), path = key(bytes, "txt");
  assert.equal((await signed(store.endpoint, "PUT", path, bytes, mime.txt)).status, 200); await store.close();
  const record = join(dir, readdirSync(dir).find((x) => x.endsWith(".record")));
  const original = readFileSync(record), newline = original.indexOf(10);
  const meta = JSON.parse(original.subarray(0, newline).toString()); meta.size++;
  writeFileSync(record, Buffer.concat([Buffer.from(JSON.stringify(meta) + "\n"), original.subarray(newline + 1)]));
  await assert.rejects(startLocalQaObjectStore({ ...config, directory: dir }), /local_qa_object_store_start_failed/);
  writeFileSync(record, original); chmodSync(record, 0o644);
  await assert.rejects(startLocalQaObjectStore({ ...config, directory: dir }), /local_qa_object_store_start_failed/);
  chmodSync(record, 0o600);
  const parent = owned(t), alias = join(parent, "alias"); symlinkSync(dir, alias);
  await assert.rejects(startLocalQaObjectStore({ ...config, directory: alias }), /local_qa_object_store_start_failed/);
});

test("close stops the listener and never clears stored records", async (t) => {
  const dir = owned(t), store = await start(t, dir), bytes = Buffer.from("keep after close"), path = key(bytes, "txt");
  assert.equal((await signed(store.endpoint, "PUT", path, bytes, mime.txt)).status, 200);
  await store.close();
  await assert.rejects(signed(store.endpoint, "GET", path));
  assert.equal(readdirSync(dir).filter((x) => x.endsWith(".record")).length, 1);
});

test("lost established marker and records never cause silent reinitialization", async (t) => {
  const dir = owned(t), store = await start(t, dir), bytes = Buffer.from("persisted");
  assert.equal((await signed(store.endpoint, "PUT", key(bytes, "txt"), bytes, mime.txt)).status, 200);
  await store.close();
  for (const name of readdirSync(dir)) rmSync(join(dir, name));
  await assert.rejects(startLocalQaObjectStore({ ...config, directory: dir }), /local_qa_object_store_start_failed/);
  assert.deepEqual(readdirSync(dir), []);
});

test("initialization refuses an existing store or unknown nonempty directory", async (t) => {
  const dir = owned(t), marker = join(dir, LOCAL_QA_OBJECT_MARKER), before = readFileSync(marker);
  assert.throws(() => initializeLocalQaObjectDirectory({ ...config, directory: dir }), /local_qa_object_store_init_failed/);
  assert.deepEqual(readFileSync(marker), before);
  const other = owned(t); rmSync(join(other, LOCAL_QA_OBJECT_MARKER));
  writeFileSync(join(other, "unrelated"), "do not overwrite", { mode: 0o600 });
  assert.throws(() => initializeLocalQaObjectDirectory({ ...config, directory: other }), /local_qa_object_store_init_failed/);
  assert.equal(readFileSync(join(other, "unrelated"), "utf8"), "do not overwrite");
});

test("duplicate JSON fields and noncanonical headers fail closed", async (t) => {
  const dir = owned(t), store = await start(t, dir), bytes = Buffer.from("duplicate header");
  assert.equal((await signed(store.endpoint, "PUT", key(bytes, "txt"), bytes, mime.txt)).status, 200);
  await store.close();
  const record = join(dir, readdirSync(dir).find((x) => x.endsWith(".record")));
  const raw = readFileSync(record), split = raw.indexOf(10);
  const duplicated = raw.subarray(0, split).toString().replace('{"version":1,', '{"version":1,"version":1,');
  writeFileSync(record, Buffer.concat([Buffer.from(duplicated + "\n"), raw.subarray(split + 1)]));
  await assert.rejects(startLocalQaObjectStore({ ...config, directory: dir }), /local_qa_object_store_start_failed/);
});

test("object count quota admits 64 and rejects a 65th without deleting existing objects", async (t) => {
  const dir = owned(t), store = await start(t, dir), bytes = Buffer.from("q");
  const keys = Array.from({ length: 65 }, () => key(bytes, "txt"));
  for (const path of keys.slice(0, 64)) assert.equal((await signed(store.endpoint, "PUT", path, bytes, mime.txt)).status, 200);
  assert.equal((await signed(store.endpoint, "PUT", keys[64], bytes, mime.txt)).status, 413);
  assert.deepEqual((await signed(store.endpoint, "GET", keys[0])).body, bytes);
});

test("byte quota admits exactly 128 MiB and rejects additional bytes", async (t) => {
  const dir = owned(t), store = await start(t, dir), big = Buffer.alloc(10 * 1024 * 1024, 0x41);
  for (let i = 0; i < 12; i++) assert.equal((await signed(store.endpoint, "PUT", key(big, "pdf"), big, mime.pdf)).status, 200);
  const tail = Buffer.alloc(8 * 1024 * 1024, 0x42), tailKey = key(tail, "pdf");
  assert.equal((await signed(store.endpoint, "PUT", tailKey, tail, mime.pdf)).status, 200);
  const one = Buffer.from("x");
  assert.equal((await signed(store.endpoint, "PUT", key(one, "txt"), one, mime.txt)).status, 413);
  assert.equal((await signed(store.endpoint, "GET", tailKey)).body.length, tail.length);
});
