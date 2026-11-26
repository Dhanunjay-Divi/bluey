import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { once } from "node:events";
import { chmod, lstat, mkdir, mkdtemp, realpath, rm, writeFile } from "node:fs/promises";
import { createServer, request } from "node:http";
import { createConnection } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startLocalQaGateway } from "./local-qa-gateway.mjs";

const digest = text => createHash("sha256").update(text).digest("hex");
async function socketBinding(root, path) {
  const stat = await lstat(path, { bigint: true });
  return { root, path, uid: Number(stat.uid), mode: Number(stat.mode & 0o7777n),
    dev: Number(stat.dev), ino: Number(stat.ino) };
}
function call(port, path, body) {
  return new Promise((resolve, reject) => {
    const req = request({ host: "127.0.0.1", port, path, method: "POST",
      headers: { origin: `http://127.0.0.1:${port}`, "content-type": "application/json", "content-length": String(Buffer.byteLength(body)) } }, res => {
      const chunks=[]; res.on("data", x => chunks.push(x)); res.on("end", () => resolve([res.statusCode, Buffer.concat(chunks)])); });
    req.on("error", reject); req.end(body);
  });
}
test("candidate-only gateway proxies through an exact private Unix socket", async () => {
  // Keep Darwin's sockaddr_un path in range even under the canonical launcher.
  const root = await realpath(await mkdtemp(join(tmpdir(), "q-")));
  const web = join(root, "web"), socket = join(root, "a");
  let backend, gateway;
  const backendSockets = new Set();
  try {
    await chmod(root, 0o700); await mkdir(join(web, "jobs"), { recursive: true }); await writeFile(join(web, "index.html"), "login"); await writeFile(join(web, "jobs/index.html"), "jobs");
    backend = createServer(async (req, res) => { for await (const _ of req) {} res.setHeader("content-type", "application/json"); res.end(JSON.stringify({ account:{id:"11111111-1111-4111-8111-111111111111",email:"candidate@bluey-qa.test",is_admin:false,is_temporary:false},access_token:"a.b.c" })); });
    backend.on("connection", socket => { backendSockets.add(socket); socket.once("close", () => backendSockets.delete(socket)); });
    const listening = once(backend, "listening");
    backend.listen(socket);
    await listening;
    await chmod(socket, 0o600);
    const binding = await socketBinding(root, socket); let checks=0;
    // This mock owns the backend in-process. It is not the saved-account
    // peer-attestation broker that a real candidate run would require.
    const connectMock = async (target, signal) => {
      assert.equal(target.path, socket);
      const stream = createConnection({ path: socket, signal });
      await once(stream, "connect");
      return stream;
    };
    await assert.rejects(startLocalQaGateway({ webDirectory:web, manifest:{"/index.html":digest("login"),"/jobs/index.html":digest("jobs")}, backendUnixSocket:binding,
      candidateReadback:{accountId:"11111111-1111-4111-8111-111111111111",resumeId:null}, expiresAtMs:Date.now()+30_000,
      verifyBackend: async () => true }), /local_qa_gateway_refused/);
    gateway = await startLocalQaGateway({ webDirectory:web, manifest:{"/index.html":digest("login"),"/jobs/index.html":digest("jobs")}, backendUnixSocket:binding,
      candidateReadback:{accountId:"11111111-1111-4111-8111-111111111111",resumeId:null}, expiresAtMs:Date.now()+30_000,
      connectOwnedUnix: connectMock,
      verifyBackend: async target => { checks++; return target.kind==="unix" && target.path===socket; } });
    const port = Number(new URL(gateway.origin).port);
    assert.equal((await call(port, "/auth/login", JSON.stringify({email:"candidate@bluey-qa.test",password:`Qa-${"a".repeat(48)}`})))[0], 200);
    assert.ok(checks >= 2);
    const input = JSON.stringify({ email: "candidate@bluey-qa.test", password: `Qa-${"a".repeat(48)}` });
    const pinnedInode = binding.ino;
    binding.ino++;
    assert.equal((await call(port, "/auth/login", input))[0], 200, "caller mutation cannot retarget a started gateway");
    binding.ino = pinnedInode;
    await chmod(socket, 0o660);
    assert.equal((await call(port, "/auth/login", input))[0], 502, "mode drift refuses before proxying");
    await chmod(socket, 0o600);
    await assert.rejects(startLocalQaGateway({ webDirectory:web, manifest:{"/index.html":digest("login"),"/jobs/index.html":digest("jobs")}, backendPort:0, backendUnixSocket:binding,
      candidateReadback:{accountId:"11111111-1111-4111-8111-111111111111",resumeId:null},expiresAtMs:Date.now()+30_000,verifyBackend:async()=>true,connectOwnedUnix:connectMock }), /local_qa_gateway_refused/);
    await assert.rejects(startLocalQaGateway({ webDirectory:web, manifest:{"/index.html":digest("login"),"/jobs/index.html":digest("jobs")}, backendUnixSocket:{...binding,ino:binding.ino+1},
      candidateReadback:{accountId:"11111111-1111-4111-8111-111111111111",resumeId:null},expiresAtMs:Date.now()+30_000,verifyBackend:async()=>true,connectOwnedUnix:connectMock }), /local_qa_gateway_refused/);
  } finally {
    await gateway?.close();
    for (const socket of backendSockets) socket.destroy();
    if (backend?.listening) await new Promise((resolve, reject) => backend.close(error => error ? reject(error) : resolve()));
    await rm(root,{recursive:true,force:true});
  }
});
