#!/usr/bin/env node

import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const portalRoot = join(root, "jobs/portal/src");
const assetsRoot = join(root, "web/jobs/assets");
const portalPackage = readFileSync(join(root, "jobs/portal/package.json"), "utf8");
const source = textFiles(portalRoot).map(readFile).join("\n");
const bundle = textFiles(assetsRoot).map(readFile).join("\n");

assert.doesNotMatch(portalPackage, /@bluey\/jobs-automation/);
assert.doesNotMatch(source, /@bluey\/jobs-automation/);

for (const secret of [
  "BLUEY_JOBS_WORKER_SIGNING_KEY",
  "BLUEY_JOBS_LOCAL_RUN_CAPABILITY_KEY",
  "x-bluey-jobs-worker-signature",
  "You are Bluey's interview coach",
  "Never invent employers, projects, tools, metrics",
  "No submitted, verified claim directly supports",
  "Coaching rule: cite the source IDs",
]) {
  assert.equal(bundle.includes(secret), false, `browser bundle leaks protected Jobs logic: ${secret}`);
}

const sourceMaps = walk(assetsRoot).filter((path) => path.endsWith(".map"));
assert.deepEqual(sourceMaps, [], `Jobs source maps must not ship: ${sourceMaps.join(", ")}`);

console.log("Bluey Jobs client/server boundary checks passed");

function readFile(path) {
  return readFileSync(path, "utf8");
}

function textFiles(path) {
  return walk(path).filter((file) => [".js", ".jsx", ".mjs", ".ts", ".tsx"].includes(extname(file)));
}

function walk(path) {
  if (!existsSync(path)) return [];
  return readdirSync(path).flatMap((entry) => {
    const child = join(path, entry);
    return statSync(child).isDirectory() ? walk(child) : [child];
  });
}
