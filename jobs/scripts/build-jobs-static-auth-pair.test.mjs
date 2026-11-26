import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { buildPair, verifyPair, writePair } from "./build-jobs-static-auth-pair.mjs";

const js = Buffer.from("window.__jobs_auth_pair__ = true;\n");
const oneLine = Buffer.from('<html><body><script src="/assets/bluey-site.js?v=2026112201"></script></body></html>\n');
const multiline = Buffer.from('<html><body><script\n  src="/assets/bluey-site.js?v=2026112201"\n></script>\n</body></html>\n');

async function temporary() {
  return mkdtemp(join(tmpdir(), "bluey-static-auth-pair-"));
}

test("builds the same immutable pair for one-line and multiline legacy tags", () => {
  const first = buildPair({ html: oneLine, javascript: js });
  const second = buildPair({ html: multiline, javascript: js });
  const [firstPath] = [...first.files.keys()].filter((name) => name !== "web/index.html");
  const [secondPath] = [...second.files.keys()].filter((name) => name !== "web/index.html");
  assert.equal(firstPath, secondPath);
  assert.match(firstPath, /^web\/assets\/bluey-site-[a-f0-9]{64}\.js$/);
  assert.equal(first.manifest.legacySource.versionedPath, "/assets/bluey-site.js?v=2026112201");
  assert.ok(first.files.get("web/index.html").includes(firstPath.slice(3)));
  assert.ok(second.files.get("web/index.html").includes(secondPath.slice(3)));
});

test("writes exactly the pair and independently detects changed output", async () => {
  const root = await temporary();
  const out = join(root, "pair");
  try {
    const pair = buildPair({ html: oneLine, javascript: js });
    await writePair(out, pair);
    assert.deepEqual(await verifyPair(out, pair), pair.manifest);
    const html = join(out, "web/index.html");
    await writeFile(html, Buffer.concat([await readFile(html), Buffer.from(" ")]));
    await assert.rejects(verifyPair(out, pair), /output_content/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("refuses ambiguous, wrong, oversized, and non-UTF8 source scope", () => {
  assert.throws(() => buildPair({ html: Buffer.from("<html></html>"), javascript: js }), /legacy_script_reference/);
  assert.throws(() => buildPair({ html: Buffer.concat([oneLine, oneLine]), javascript: js }), /legacy_script_reference/);
  assert.throws(() => buildPair({ html: Buffer.from('<script src="/assets/other.js?v=2026112201"></script>'), javascript: js }), /legacy_script_reference/);
  assert.throws(() => buildPair({ html: Buffer.from('<body data-src="/assets/bluey-site.js?v=2026112201"></body>'), javascript: js }), /legacy_script_final/);
  assert.throws(() => buildPair({ html: Buffer.from('<!-- /assets/bluey-site.js?v=2026112201 --><body></body>'), javascript: js }), /legacy_script_final/);
  assert.throws(() => buildPair({ html: Buffer.from('<!-- <script src="/assets/bluey-site.js?v=2026112201"></script></body></html>'), javascript: js }), /legacy_script_final/);
  assert.throws(() => buildPair({ html: Buffer.from([0xff]), javascript: js }), /html_utf8/);
  assert.throws(() => buildPair({ html: Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), oneLine]), javascript: js }), /html_bom/);
  assert.throws(() => buildPair({ html: oneLine, javascript: Buffer.alloc((4 << 20) + 1) }), /javascript_size/);
});

test("refuses extra output leaves and existing output roots", async () => {
  const root = await temporary();
  const out = join(root, "pair");
  try {
    const pair = buildPair({ html: oneLine, javascript: js });
    await writePair(out, pair);
    await writeFile(join(out, "extra"), "unexpected");
    await assert.rejects(verifyPair(out, pair), /output_leaves/);
    await assert.rejects(writePair(out, pair));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
