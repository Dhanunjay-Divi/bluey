import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { decodeComposeRequest, MAX_REQUEST_BYTES, REQUEST_SCHEMA } from "./local-qa-linux-signup-mock-receipt-compose-entry.mjs";

const here = new URL(".", import.meta.url);

function request(overrides = {}) {
  return Buffer.from(JSON.stringify({
    schema: REQUEST_SCHEMA, reservationSha256: "a".repeat(64), systemctlSha256: "b".repeat(64),
    waiting: {}, application: {}, browser: {}, postgres: {}, ports: {}, python: {},
    codePlanBase64: "AQ==", chromiumInventoryBase64: "Ag==", ...overrides,
  }) + "\n");
}

test("decode accepts only one compact bounded request and canonical base64 file bytes", () => {
  const value = decodeComposeRequest(request());
  assert.deepEqual(value.codePlanBytes, Buffer.from([1]));
  assert.deepEqual(value.chromiumInventoryBytes, Buffer.from([2]));
  for (const raw of [Buffer.alloc(0), Buffer.from("{}\n"), request({ codePlanBase64: "AQ=" }),
    request({ path: "/tmp/untrusted" }), Buffer.alloc(MAX_REQUEST_BYTES + 1, 0x20)]) {
    assert.throws(() => decodeComposeRequest(raw), /compose_refused/);
  }
});

test("sealed actor MJS modules form a portable closure with the entry and spec", async () => {
  assert.ok(process.env.MOCK_RECEIPT_EVIDENCE_ROOT);
  const packet = join(process.env.MOCK_RECEIPT_EVIDENCE_ROOT, "jobs-signup-mock-receipt-source.Lu4NPZR1",
    "signup-mock-receipt-source-candidate.tar");
  const stage = await mkdtemp(join(tmpdir(), "bluey-mock-compose-"));
  try {
    const members = execFileSync("/usr/bin/tar", ["-tf", packet], { encoding: "utf8", maxBuffer: 64 << 10 })
      .trim().split("\n").filter((name) => /^actor\/.+\.mjs$/.test(name));
    // The sealed actor packet has 39 files; exactly 36 are JavaScript modules.
    assert.equal(members.length, 36);
    for (const member of members) {
      const name = member.slice("actor/".length);
      const bytes = execFileSync("/usr/bin/tar", ["-xOf", packet, member], { maxBuffer: 128 << 10 });
      await writeFile(join(stage, name), bytes, { mode: 0o600 });
    }
    for (const name of ["local-qa-linux-signup-mock-receipt-compose-entry.mjs", "local-qa-linux-signup-mock-receipt-spec.mjs"]) {
      await writeFile(join(stage, name), await readFile(new URL(name, here)), { mode: 0o600 });
    }
    const run = spawnSync(process.execPath, [join(stage, "local-qa-linux-signup-mock-receipt-compose-entry.mjs")], {
      input: Buffer.from("{}\n"), encoding: null, timeout: 5_000,
      env: { PATH: "/usr/bin:/bin", HOME: stage, TMPDIR: stage, LC_ALL: "C" },
    });
    assert.equal(run.status, 1);
    assert.equal(run.stdout.length, 0);
    assert.equal(run.stderr.length, 0);
  } finally {
    await rm(stage, { recursive: true, force: true });
  }
});
