import test from "node:test";
import assert from "node:assert/strict";
import { createHash, createHmac } from "node:crypto";
import { request as httpRequest } from "node:http";
import { chmodSync, mkdirSync, mkdtempSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { initializeLocalQaObjectDirectory, startLocalQaObjectStore } from "./local-qa-object-store.mjs";

const digest = (value) => createHash("sha256").update(value).digest("hex");
const hmac = (key, value) => createHmac("sha256", key).update(value).digest();
const config = Object.freeze({ qaId: "11111111-1111-4111-8111-111111111111", bucket: `qa-${"a".repeat(16)}`,
  prefix: `qa-${"b".repeat(16)}`, accessKey: `qa${"c".repeat(16)}`, secret: "d".repeat(64) });
const accountId = "22222222-2222-4222-8222-222222222222";
const applicationId = "33333333-3333-4333-8333-333333333333";
const foreignAccountId = "44444444-4444-4444-8444-444444444444";
const foreignApplicationId = "55555555-5555-4555-8555-555555555555";

function receiptEvidence(kind, bytes, account = accountId, application = applicationId, fingerprint = "e".repeat(64)) {
  const leaf = kind === "resume" ? "0-resume" : "1-screenshot";
  return `/${config.bucket}/${config.prefix}/accounts/${account}/context/jobs/${application}/receipts/${fingerprint}/${leaf}-${digest(bytes).slice(0, 20)}`;
}

function receiptBundle(bytes, application = applicationId, account = accountId, receipt = "f".repeat(64), bundle = "a".repeat(64)) {
  return `/${config.bucket}/${config.prefix}/accounts/${account}/jobs/applications/${digest(application)}/receipts/${receipt}/bundles/${bundle}/sha256/${digest(bytes)}.json`;
}

function resume(bytes) {
  return `/${config.bucket}/${config.prefix}/accounts/qa_account_1/jobs/resumes/${accountId}/sha256/${digest(bytes)}.txt`;
}

function signed(endpoint, method, path, bytes = Buffer.alloc(0), contentType) {
  const host = new URL(endpoint).host;
  const dateTime = new Date().toISOString().replace(/[-:]|\.\d{3}/g, "");
  const date = dateTime.slice(0, 8), payload = digest(bytes);
  const canonicalHeaders = `host:${host}\nx-amz-content-sha256:${payload}\nx-amz-date:${dateTime}\n`;
  const canonical = `${method}\n${path}\n\n${canonicalHeaders}\nhost;x-amz-content-sha256;x-amz-date\n${payload}`;
  const scope = `${date}/us-east-1/s3/aws4_request`;
  let signing = hmac(`AWS4${config.secret}`, date);
  for (const part of ["us-east-1", "s3", "aws4_request"]) signing = hmac(signing, part);
  const signature = hmac(signing, `AWS4-HMAC-SHA256\n${dateTime}\n${scope}\n${digest(canonical)}`).toString("hex");
  const headers = { host, "x-amz-date": dateTime, "x-amz-content-sha256": payload,
    authorization: `AWS4-HMAC-SHA256 Credential=${config.accessKey}/${scope}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=${signature}`,
    "content-length": String(bytes.length) };
  if (contentType) headers["content-type"] = contentType;
  return new Promise((resolve, reject) => {
    const request = httpRequest(endpoint + path, { method, headers }, response => {
      const chunks = []; response.on("data", chunk => chunks.push(chunk));
      response.on("end", () => resolve({ status: response.statusCode, body: Buffer.concat(chunks) }));
    });
    request.on("error", reject); request.end(bytes);
  });
}

function owned() {
  const parent = realpathSync(mkdtempSync(join(tmpdir(), "bluey-local-qa-receipt-test-")));
  chmodSync(parent, 0o700);
  const directory = join(parent, "objects"); mkdirSync(directory, { mode: 0o700 });
  initializeLocalQaObjectDirectory({ ...config, directory });
  return Object.freeze({ directory, cleanup: () => rmSync(parent, { recursive: true, force: true }) });
}

test("receipt paths are inert until exactly one private owner admission while resume storage remains available", async t => {
  const ownedStore = owned(), store = await startLocalQaObjectStore({ ...config, directory: ownedStore.directory });
  t.after(async () => { await store.close(); ownedStore.cleanup(); });
  const evidence = Buffer.from("synthetic receipt PDF"), receiptPath = receiptEvidence("resume", evidence);
  assert.equal((await signed(store.endpoint, "PUT", receiptPath, evidence, "application/pdf")).status, 403);
  assert.equal((await signed(store.endpoint, "PUT", resume(Buffer.from("synthetic resume")), Buffer.from("synthetic resume"), "text/plain")).status, 200);
  assert.throws(() => store.admitMockReceipt({ accountId: "not-a-uuid", applicationId }), /admission_refused/);
  assert.deepEqual(store.admitMockReceipt({ accountId, applicationId }), { accountId, applicationId });
  assert.throws(() => store.admitMockReceipt({ accountId, applicationId }), /admission_refused/);
  assert.equal(JSON.stringify(store).includes("admitMockReceipt"), false);
});

test("admitted receipt writes accept only the three exact signed receipt records and retain their custody", async t => {
  const ownedStore = owned(), store = await startLocalQaObjectStore({ ...config, directory: ownedStore.directory });
  t.after(async () => { await store.close(); ownedStore.cleanup(); });
  store.admitMockReceipt({ accountId, applicationId });
  const pdf = Buffer.from("synthetic receipt PDF"), png = Buffer.from("synthetic receipt PNG"), bundle = Buffer.from('{"synthetic":"receipt"}');
  const pdfPath = receiptEvidence("resume", pdf), pngPath = receiptEvidence("screenshot", png), bundlePath = receiptBundle(bundle);
  assert.equal((await signed(store.endpoint, "PUT", pdfPath, pdf, "application/pdf")).status, 200);
  assert.equal((await signed(store.endpoint, "PUT", pngPath, png, "image/png")).status, 200);
  assert.equal((await signed(store.endpoint, "PUT", bundlePath, bundle, "application/json")).status, 200);
  assert.deepEqual((await signed(store.endpoint, "GET", pdfPath)).body, pdf);
  assert.equal((await signed(store.endpoint, "PUT", pdfPath, pdf, "application/pdf")).status, 200);
  const fourth = receiptBundle(Buffer.from('{"next":true}'), applicationId, accountId, "b".repeat(64), "c".repeat(64));
  assert.equal((await signed(store.endpoint, "PUT", fourth, Buffer.from('{"next":true}'), "application/json")).status, 413);
  assert.equal((await signed(store.endpoint, "DELETE", pdfPath)).status, 403);
  assert.deepEqual((await signed(store.endpoint, "GET", pdfPath)).body, pdf);
});

test("receipt admission rejects foreign scope, wrong typed patterns and digest evidence", async t => {
  const ownedStore = owned(), store = await startLocalQaObjectStore({ ...config, directory: ownedStore.directory });
  t.after(async () => { await store.close(); ownedStore.cleanup(); });
  store.admitMockReceipt({ accountId, applicationId });
  const pdf = Buffer.from("synthetic receipt PDF"), path = receiptEvidence("resume", pdf);
  assert.equal((await signed(store.endpoint, "PUT", receiptEvidence("resume", pdf, foreignAccountId), pdf, "application/pdf")).status, 403);
  assert.equal((await signed(store.endpoint, "PUT", receiptEvidence("resume", pdf, accountId, foreignApplicationId), pdf, "application/pdf")).status, 403);
  assert.equal((await signed(store.endpoint, "PUT", path, pdf, "image/png")).status, 415);
  const mismatched = path.slice(0, -20) + "0".repeat(20);
  assert.equal((await signed(store.endpoint, "PUT", mismatched, pdf, "application/pdf")).status, 403);
  const foreignBundle = receiptBundle(Buffer.from("{}"), foreignApplicationId);
  assert.equal((await signed(store.endpoint, "PUT", foreignBundle, Buffer.from("{}"), "application/json")).status, 403);
  assert.equal((await signed(store.endpoint, "GET", receiptBundle(Buffer.from("{}")))).status, 404);
});
