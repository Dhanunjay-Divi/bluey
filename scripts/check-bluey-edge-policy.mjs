#!/usr/bin/env node

import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(join(root, path), "utf8");

const robots = read("web/robots.txt");
const caddy = read("ops/Caddyfile.example");
const index = read("web/index.html");
const sitemap = read("web/sitemap.xml");
const vite = read("jobs/portal/vite.config.ts");

assert.match(robots, /Content-Signal:\s*search=yes,\s*ai-input=no,\s*ai-train=no/i);
for (const crawler of ["GPTBot", "ClaudeBot", "Google-Extended", "PerplexityBot", "CCBot"]) {
  assert.match(robots, new RegExp(`User-agent: ${crawler}\\nDisallow: /`, "i"));
}
assert.doesNotMatch(index, /llms\.txt/i);
assert.doesNotMatch(sitemap, /llms\.txt/i);
assert.match(index, /security@bluey\.sh/i);
assert.match(index, /automated means to extract, reproduce, index, benchmark/i);
assert.equal(existsSync(join(root, "web/llms.txt")), false, "web/llms.txt must not ship");

assert.match(caddy, /handle \/llms\.txt\s*\{\s*respond 410\s*\}/s);
assert.match(caddy, /handle \/JobApply\s*\{\s*redir \/jobs 308\s*\}/s);
assert.match(caddy, /@bluey_jobs_internal path \/api\/jobs\/internal\/\*/);
assert.match(caddy, /@bluey_scrape_canary path \/\.well-known\/bluey-integrity-/);
assert.match(caddy, /handle @bluey_scrape_canary\s*\{[\s\S]*?X-Bluey-Scrape-Canary "hit"[\s\S]*?respond 404\s*\}/);
assert.match(caddy, /handle @bluey_jobs_internal\s*\{\s*respond 404\s*\}/s);
assert.match(caddy, /@bluey_assets path \/assets\/\* \/jobs\/assets\/\*/);
assert.match(caddy, /header @bluey_noindex X-Robots-Tag "noindex, nofollow, noarchive, nosnippet"/);
assert.match(caddy, /-Server/);
assert.ok(
  caddy.indexOf("handle @bluey_jobs_internal") < caddy.indexOf("handle @bluey_jobs_api"),
  "private Jobs routes must be rejected before the public Jobs proxy",
);
const assetsHandler = caddy.indexOf("handle @bluey_assets {");
const jobsSpaHandler = caddy.indexOf("handle @bluey_jobs {");
assert.ok(
  assetsHandler >= 0 && jobsSpaHandler >= 0 && assetsHandler < jobsSpaHandler,
  "asset requests must be resolved before Jobs SPA fallback",
);

assert.match(vite, /sourcemap:\s*false/);
const shippedMaps = walk(join(root, "web")).filter((path) => path.endsWith(".map"));
assert.deepEqual(shippedMaps, [], `source maps must not ship: ${shippedMaps.join(", ")}`);

console.log("Bluey edge/crawler policy checks passed");

function walk(path) {
  if (!existsSync(path)) return [];
  return readdirSync(path).flatMap((entry) => {
    const child = join(path, entry);
    return statSync(child).isDirectory() ? walk(child) : [child];
  });
}
