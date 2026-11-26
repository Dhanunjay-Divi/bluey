// Base SHA-256: 404aacaa26c8afab9762a3fe3f35278f0869b2f6ea2ab955c9b5feb05a556d8a
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import test from "node:test";

const script = fileURLToPath(new URL("./nonprod-jobs-mcp-stdio.mjs", import.meta.url));
const CLOSED_ENV = Object.freeze({ PATH: "/usr/bin:/bin", HOME: "/nonexistent", LANG: "C", TZ: "UTC" });
const initialize = Object.freeze({
  jsonrpc: "2.0",
  id: 1,
  method: "initialize",
  params: {
    protocolVersion: "2025-11-25",
    capabilities: {},
    clientInfo: { name: "bluey-nonprod-test", version: "1.0.0" },
  },
});

const request = (id, method, params) => ({
  jsonrpc: "2.0", id, method, ...(params === undefined ? {} : { params }),
});
const notification = (method, params = {}) => ({ jsonrpc: "2.0", method, params });

function run(account, records) {
  const input = `${records.map((record) => typeof record === "string" ? record : JSON.stringify(record)).join("\n")}\n`;
  const child = spawnSync(process.execPath, [script, `--synthetic-account=${account}`], {
    input,
    encoding: "utf8",
    timeout: 5_000,
    maxBuffer: 128 * 1024,
    env: CLOSED_ENV,
  });
  assert.ifError(child.error);
  assert.equal(child.signal, null, "child closes after stdin EOF");
  assert.equal(child.status, 0, child.stderr);
  assert.equal(child.stderr, "", "protocol logs must not share stderr");
  assert(!child.stdout.includes("BLUEY_TEST_SECRET"));
  return child.stdout === "" ? [] : child.stdout.trimEnd().split("\n").map((line) => JSON.parse(line));
}

function calls(account, ...argumentsList) {
  return run(account, [
    initialize,
    notification("notifications/initialized"),
    ...argumentsList.map((arguments_, index) => request(index + 2, "tools/call", {
      name: "get_job_matches", arguments: arguments_,
    })),
  ]);
}

function assertPage(response, trackId, count) {
  const { isError, structuredContent, content } = response.result;
  assert.equal(isError, false);
  assert.deepEqual(JSON.parse(content[0].text), structuredContent);
  assert.deepEqual(Object.keys(structuredContent).sort(),
    ["fixtureOnly", "matches", "nextCursor", "shortlistTruncated", "trackId"]);
  assert.equal(structuredContent.fixtureOnly, true);
  assert.equal(structuredContent.trackId, trackId);
  assert.equal(structuredContent.matches.length, count);
  assert.equal(structuredContent.nextCursor, null);
  assert.equal(structuredContent.shortlistTruncated, false);
}

test("the process-selected account derives Track scope and returns one terminal fixture page", () => {
  const alpha = calls("alpha", { limit: 1 });
  const beta = calls("beta", {});
  assertPage(alpha[1], "track-alpha", 1);
  assertPage(beta[1], "track-beta", 1);
  assert.notEqual(alpha[1].result.structuredContent.matches[0].jobId,
    beta[1].result.structuredContent.matches[0].jobId);
});

test("the listed tool declares the closed start-or-cursor ABI and bounded terminal response", () => {
  const output = run("alpha", [initialize, notification("notifications/initialized"), request(2, "tools/list", {})]);
  const [tool] = output[1].result.tools;
  assert.equal(tool.name, "get_job_matches");
  assert.equal(tool.inputSchema.oneOf.length, 2);
  assert.deepEqual(tool.inputSchema.oneOf[0].properties.limit, { type: "integer", minimum: 1, maximum: 10 });
  assert.deepEqual(tool.inputSchema.oneOf[1].required, ["cursor"]);
  assert.deepEqual(tool.outputSchema.properties.nextCursor.oneOf, [
    { type: "null" },
    { type: "string", minLength: 71, maxLength: 71, pattern: "^bjmcp1_[0-9a-f]{64}$" },
  ]);
  assert.deepEqual(tool.outputSchema.properties.shortlistTruncated, { type: "boolean" });
  assert.equal(tool.annotations.readOnlyHint, true);
});

test("caller-selected Track, account, mixed pages and malformed cursors are invalid", () => {
  const output = calls("alpha",
    { trackId: "track-alpha" },
    { accountId: "alpha" },
    { limit: 1, cursor: `bjmcp1_${"a".repeat(64)}` },
    { cursor: "not-a-cursor" },
    { cursor: `bjmcp1_${"A".repeat(64)}` },
    { cursor: `bjmcp1_${"a".repeat(43)}` },
    { limit: 0 });
  for (const response of output.slice(1)) {
    assert.equal(response.error.code, -32602);
    assert.equal(response.error.message, "Invalid match parameters");
  }
});

test("a syntactically valid cursor is generically unavailable and never echoed", () => {
  const cursor = `bjmcp1_${"a".repeat(64)}`;
  const output = calls("alpha", { cursor });
  const response = output[1];
  assert.equal(response.error, undefined);
  assert.equal(response.result.isError, true);
  assert.equal(response.result.structuredContent, undefined);
  assert.equal(response.result.content[0].text, "Career Track unavailable");
  assert(!JSON.stringify(response).includes(cursor));
});

test("transport remains closed before initialization and after malformed or oversized frames", () => {
  const pre = run("alpha", [request(1, "tools/list", {})]);
  assert.equal(pre[0].error.code, -32002);
  const malformed = run("alpha", ["{not-json", initialize, notification("notifications/initialized"), request(2, "tools/list", {})]);
  assert.equal(malformed[0].error.code, -32700);
  assert.equal(malformed[2].result.tools[0].name, "get_job_matches");
  const oversized = run("alpha", ["x".repeat(8193), initialize, notification("notifications/initialized"), request(2, "tools/list", {})]);
  assert.equal(oversized[0].error.code, -32700);
  assert.equal(oversized[2].result.tools[0].name, "get_job_matches");
});

test("the finite frame budget and fixture-account argv admission are unchanged", () => {
  const records = Array.from({ length: 256 }, () => notification("notifications/unknown"));
  const output = run("alpha", [...records, request(1, "ping")]);
  assert.equal(output.length, 1);
  assert.equal(output[0].error.code, -32000);

  for (const argv of [[], ["--synthetic-account=gamma"], ["--synthetic-account=alpha", "extra"]]) {
    const child = spawnSync(process.execPath, [script, ...argv], { encoding: "utf8", env: CLOSED_ENV, timeout: 5_000 });
    assert.equal(child.status, 64);
    assert.equal(child.stdout, "");
    assert.equal(child.stderr, "");
  }
});
