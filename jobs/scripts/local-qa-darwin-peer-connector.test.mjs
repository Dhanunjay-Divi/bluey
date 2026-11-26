import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { once } from "node:events";
import { chmod, lstat, readFile, realpath } from "node:fs/promises";
import { createServer } from "node:net";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { connectOwnedUnix } from "./local-qa-darwin-peer-connector.mjs";

const interpreterPath = "/Library/Developer/CommandLineTools/Library/Frameworks/Python3.framework/Versions/3.9/bin/python3.9";
const interpreterSha256 = "6e7ae61f68a3838094fc56590f84c52069a97d7816f6bb79e0e85995b340e464";
const helperPath = fileURLToPath(new URL("./local-qa-darwin-peer.py", import.meta.url));

async function binding(root, path, pid) {
  const info = await lstat(path, { bigint: true });
  return Object.freeze({
    dev: Number(info.dev), ino: Number(info.ino), mode: Number(info.mode & 0o7777n),
    path, pid, root, uid: Number(info.uid),
  });
}

test("the Darwin connector verifies the already-held stream before forwarding", { timeout: 12000 }, async () => {
  assert.equal(process.platform, "darwin");
  const root = await realpath(process.env.BLUEY_TEST_RUN_ROOT);
  assert.equal(root, process.env.BLUEY_TEST_RUN_ROOT);
  const path = join(root, "peer-node");
  const helperSha256 = createHash("sha256").update(await readFile(helperPath)).digest("hex");
  assert.equal(helperSha256, "aa1f5954a8c4ba0c14b06342216b9b969f7bcf21d3d3d1553d2867127567417d");
  const peers = new Set(), phases = [];
  let bytesFromClient = 0;
  const backend = createServer(socket => {
    peers.add(socket);
    socket.once("close", () => peers.delete(socket));
    socket.on("error", () => {});
    socket.on("data", bytes => { bytesFromClient += bytes.length; });
    socket.write("unchanged");
  });
  const listening = once(backend, "listening");
  backend.listen(path);
  await listening;
  let live;
  try {
    await chmod(path, 0o600);
    const verify = Object.freeze({ interpreterPath, interpreterSha256, helperPath, helperSha256,
      assertOwner: async (_candidate, phase) => { phases.push(phase); return true; } });
    const present = await binding(root, path, process.pid);
    live = await connectOwnedUnix(present, verify, AbortSignal.timeout(4000));
    const data = once(live, "data", { signal: AbortSignal.timeout(3000) });
    live.resume();
    const [received] = await data;
    assert.equal(received.toString(), "unchanged");
    assert.deepEqual(phases, ["before_connect", "before_release"]);
    live.destroy();
    phases.length = 0;
    const other = await binding(root, path, process.ppid);
    assert.notEqual(other.pid, present.pid);
    await assert.rejects(connectOwnedUnix(other, verify, AbortSignal.timeout(4000)), /local_qa_peer_refused/);
    assert.deepEqual(phases, ["before_connect"]);
    assert.equal(bytesFromClient, 0);
  } finally {
    live?.destroy();
    for (const socket of peers) socket.destroy();
    await new Promise((close, reject) => backend.close(error => error ? reject(error) : close()));
  }
});
