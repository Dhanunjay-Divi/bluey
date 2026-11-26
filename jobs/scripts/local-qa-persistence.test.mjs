import test from "node:test";
import assert from "node:assert/strict";
import { createHash, createHmac } from "node:crypto";
import { request } from "node:http";
import { mkdtemp, realpath, rm, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { initializeLocalQaState, inspectLocalQaState, loadLocalQaSecrets } from "./local-qa-state.mjs";
import { startLocalQaObjectStore } from "./local-qa-object-store.mjs";

// Independent SigV4 signer; no verifier or signing shortcut from the service.
const sha = (value) => createHash("sha256").update(value).digest("hex");
const mac = (key, value) => createHmac("sha256", key).update(value).digest();
function send(endpoint, accessKey, secret, method, path, bytes = Buffer.alloc(0), contentType) {
  const host = new URL(endpoint).host;
  const timestamp = new Date().toISOString().replace(/[-:]|\.\d{3}/g, "");
  const date = timestamp.slice(0, 8), payload = sha(bytes), scope = `${date}/us-east-1/s3/aws4_request`;
  const signedHeaders = "host;x-amz-content-sha256;x-amz-date";
  const canonicalHeaders = `host:${host}\nx-amz-content-sha256:${payload}\nx-amz-date:${timestamp}\n`;
  const canonical = `${method}\n${path}\n\n${canonicalHeaders}\n${signedHeaders}\n${payload}`;
  let key = mac(`AWS4${secret}`, date);
  for (const part of ["us-east-1", "s3", "aws4_request"]) key = mac(key, part);
  const signature = mac(key, `AWS4-HMAC-SHA256\n${timestamp}\n${scope}\n${sha(canonical)}`).toString("hex");
  const headers = { host, "x-amz-content-sha256": payload, "x-amz-date": timestamp,
    authorization: `AWS4-HMAC-SHA256 Credential=${accessKey}/${scope}, SignedHeaders=${signedHeaders}, Signature=${signature}`,
    "content-length": String(bytes.length) };
  if (contentType) headers["content-type"] = contentType;
  return new Promise((resolve, reject) => {
    const req = request(endpoint + path, { method, headers }, (res) => {
      const chunks = []; res.on("data", (chunk) => chunks.push(chunk));
      res.on("end", () => resolve({ status: res.statusCode, body: Buffer.concat(chunks) }));
    });
    req.on("error", reject); req.end(bytes);
  });
}

test("durable state credentials reopen exact synthetic resume revisions after store restart", async (t) => {
  const parent = await realpath(await mkdtemp(join(tmpdir(), "local-qa-persistence-")));
  const path = join(parent, "bluey-jobs-qa-persistence"), services = [];
  // Only this test-created private parent is removed, after every service closes.
  t.after(async () => {
    for (const service of services) await service.close();
    await rm(parent, { recursive: true, force: true });
  });
  const first = await initializeLocalQaState(path);
  assert.deepEqual(await inspectLocalQaState(path), first);
  const firstSecrets = await loadLocalQaSecrets(path);
  const open = async (state, secrets) => {
    const service = await startLocalQaObjectStore({ directory: state.objectDirectory, qaId: state.qaId,
      bucket: state.bucket, prefix: state.prefix, accessKey: secrets.objectAccessKey, secret: secrets.objectSecret });
    services.push(service); return service;
  };
  let service = await open(first, firstSecrets);
  const pdf = Buffer.from("%PDF-1.4\nsynthetic qa fixture\n%%EOF\n"), text = Buffer.from("Synthetic QA resume revision\n");
  const key = (id, bytes, ext) => `/${first.bucket}/${first.prefix}/accounts/qa_account_1/jobs/resumes/${id}/sha256/${sha(bytes)}.${ext}`;
  const pdfKey = key("11111111-1111-4111-8111-111111111111", pdf, "pdf");
  const txtKey = key("22222222-2222-4222-8222-222222222222", text, "txt");
  const call = (method, objectPath, bytes, type, secret = firstSecrets.objectSecret) =>
    send(service.endpoint, firstSecrets.objectAccessKey, secret, method, objectPath, bytes, type);
  assert.equal((await call("PUT", pdfKey, pdf, "application/pdf")).status, 200);
  assert.equal((await call("PUT", txtKey, text, "text/plain")).status, 200);
  const wrongSecret = (firstSecrets.objectSecret[0] === "f" ? "e" : "f") + firstSecrets.objectSecret.slice(1);
  const rejected = await call("GET", pdfKey, undefined, undefined, wrongSecret);
  assert.equal(rejected.status, 403); assert.equal(rejected.body.length, 0);
  await service.close();
  assert.equal((await readdir(first.objectDirectory)).filter((name) => name.endsWith(".record")).length, 2);

  const restored = await inspectLocalQaState(path), restoredSecrets = await loadLocalQaSecrets(path);
  assert.deepEqual(restored, first);
  assert(restoredSecrets.objectAccessKey === firstSecrets.objectAccessKey, "stable access key changed");
  assert(restoredSecrets.objectSecret === firstSecrets.objectSecret, "stable object secret changed");
  service = await open(restored, restoredSecrets);
  const getPdf = await send(service.endpoint, restoredSecrets.objectAccessKey, restoredSecrets.objectSecret, "GET", pdfKey);
  const getText = await send(service.endpoint, restoredSecrets.objectAccessKey, restoredSecrets.objectSecret, "GET", txtKey);
  assert.equal(getPdf.status, 200); assert.deepEqual(getPdf.body, pdf);
  assert.equal(getText.status, 200); assert.deepEqual(getText.body, text);
  await service.close();
  assert.equal((await readdir(restored.objectDirectory)).filter((name) => name.endsWith(".record")).length, 2);
});
