#!/usr/bin/env node
/** Deterministically materialize the narrow Jobs shared-auth static pair. */
import { createHash } from "node:crypto";
import {
  constants, lstat, mkdir, open, readdir, realpath, stat,
} from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const HTML_PATH = join(ROOT, "web/index.html");
const JS_PATH = join(ROOT, "web/assets/bluey-site.js");
const MAX_HTML = 1 << 20;
const MAX_JS = 4 << 20;
const MANIFEST = "manifest.json";
const HTML_OUTPUT = "web/index.html";
const FINAL_SCRIPT = /([ \t]*<script\s+src=(['"])(\/assets\/bluey-site\.js\?v=\d{10})\2\s*>\s*<\/script>\s*<\/body>\s*<\/html>\s*)$/s;
const SHARED_SITE_REFERENCE = /\/assets\/bluey-site(?:-[a-f0-9]{64})?\.js(?:\?[^'"<>\s]*)?/g;

function fail(reason) {
  throw new Error(`jobs-static-auth-pair:${reason}`);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function utf8(bytes, name) {
  if (bytes.length >= 3 && bytes.subarray(0, 3).equals(Buffer.from([0xef, 0xbb, 0xbf]))) {
    fail(`${name}_bom`);
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    fail(`${name}_utf8`);
  }
}

function boundedBytes(value, maximum, name) {
  if (!Buffer.isBuffer(value) || value.length < 1 || value.length > maximum) fail(`${name}_size`);
  return value;
}

function canonical(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function sourceLink(html) {
  const references = html.match(SHARED_SITE_REFERENCE) ?? [];
  if (references.length !== 1 || !/^\/assets\/bluey-site\.js\?v=\d{10}$/.test(references[0])) {
    fail("legacy_script_reference");
  }
  const match = FINAL_SCRIPT.exec(html);
  if (!match || match[3] !== references[0]) fail("legacy_script_final");
  const start = match.index + match[0].indexOf(match[3]);
  if (html.lastIndexOf("<!--", start) > html.lastIndexOf("-->", start)) fail("legacy_script_final");
  return { start, end: start + match[3].length, path: match[3] };
}

export function buildPair({ html, javascript }) {
  const htmlBytes = boundedBytes(html, MAX_HTML, "html");
  const jsBytes = boundedBytes(javascript, MAX_JS, "javascript");
  const htmlText = utf8(htmlBytes, "html");
  utf8(jsBytes, "javascript");
  const legacy = sourceLink(htmlText);
  const javascriptSha256 = sha256(jsBytes);
  const immutablePath = `/assets/bluey-site-${javascriptSha256}.js`;
  const outputHtml = Buffer.from(
    `${htmlText.slice(0, legacy.start)}${immutablePath}${htmlText.slice(legacy.end)}`,
    "utf8",
  );
  const scriptOutput = `web${immutablePath}`;
  const manifest = {
    schema: "bluey-jobs-static-auth-pair-v1",
    htmlSource: { path: "web/index.html", bytes: htmlBytes.length, sha256: sha256(htmlBytes) },
    legacySource: {
      path: "web/assets/bluey-site.js",
      versionedPath: legacy.path,
      bytes: jsBytes.length,
      sha256: javascriptSha256,
    },
    files: {
      [HTML_OUTPUT]: { bytes: outputHtml.length, sha256: sha256(outputHtml) },
      [scriptOutput]: { bytes: jsBytes.length, sha256: javascriptSha256 },
    },
  };
  return { manifest, files: new Map([[scriptOutput, jsBytes], [HTML_OUTPUT, outputHtml]]) };
}

function identity(row) {
  return [row.dev, row.ino, row.mode, row.uid, row.gid, row.nlink, row.size, row.mtimeMs, row.ctimeMs];
}

async function boundedRegular(path, maximum, reason) {
  const before = await lstat(path).catch(() => fail(reason));
  if (!before.isFile() || before.isSymbolicLink() || before.size < 1 || before.size > maximum) fail(reason);
  const handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW).catch(() => fail(reason));
  try {
    const opened = await handle.stat();
    if (JSON.stringify(identity(before)) !== JSON.stringify(identity(opened))) fail(reason);
    const buffer = Buffer.alloc(before.size + 1);
    let offset = 0;
    while (offset < buffer.length) {
      const { bytesRead } = await handle.read(buffer, offset, buffer.length - offset, null);
      if (bytesRead === 0) break;
      offset += bytesRead;
    }
    const closed = await handle.stat();
    const after = await lstat(path).catch(() => fail(reason));
    if (offset !== before.size || JSON.stringify(identity(before)) !== JSON.stringify(identity(closed))
        || JSON.stringify(identity(before)) !== JSON.stringify(identity(after))) fail(reason);
    return buffer.subarray(0, offset);
  } finally {
    await handle.close();
  }
}

async function readFixedSource() {
  const [html, javascript] = await Promise.all([
    boundedRegular(HTML_PATH, MAX_HTML, "html_source"),
    boundedRegular(JS_PATH, MAX_JS, "javascript_source"),
  ]);
  return { html, javascript };
}

async function writeExclusive(path, bytes) {
  const handle = await open(path, "wx", 0o600);
  try {
    await handle.writeFile(bytes);
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function ensureNewOutput(out) {
  if (!isAbsolute(out)) fail("out_absolute");
  const parent = dirname(resolve(out));
  const parentReal = await realpath(parent).catch(() => fail("out_parent"));
  const leaf = out.slice(out.lastIndexOf("/") + 1);
  if (!leaf || leaf === "." || leaf === "..") fail("out_path");
  const target = join(parentReal, leaf);
  if (relative(parentReal, target).startsWith("..")) fail("out_path");
  await mkdir(target, { mode: 0o700 });
  const row = await stat(target);
  if (!row.isDirectory() || (row.mode & 0o777) !== 0o700) fail("out_mode");
  return target;
}

export async function writePair(out, pair) {
  const root = await ensureNewOutput(out);
  for (const [name, bytes] of pair.files) {
    const path = join(root, name);
    await mkdir(dirname(path), { recursive: true, mode: 0o700 });
    await writeExclusive(path, bytes);
  }
  await writeExclusive(join(root, MANIFEST), Buffer.from(canonical(pair.manifest)));
  await verifyPair(root, pair);
}

async function regularBytes(path, maximum) {
  return boundedRegular(path, maximum, "output_file");
}

async function names(directory) {
  return (await readdir(directory, { withFileTypes: true })).sort((left, right) => left.name.localeCompare(right.name));
}

async function exactInventory(root, scriptOutput) {
  const rootEntries = await names(root);
  const rootByName = new Map(rootEntries.map((entry) => [entry.name, entry]));
  if (rootEntries.length !== 2 || !rootByName.get(MANIFEST)?.isFile()
      || !rootByName.get("web")?.isDirectory()) fail("output_leaves");
  const web = await names(join(root, "web"));
  const webByName = new Map(web.map((entry) => [entry.name, entry]));
  if (web.length !== 2 || !webByName.get("assets")?.isDirectory()
      || !webByName.get("index.html")?.isFile()) fail("output_leaves");
  const assets = await names(join(root, "web/assets"));
  if (assets.length !== 1 || assets[0].name !== scriptOutput.slice("web/assets/".length)
      || !assets[0].isFile()) fail("output_leaves");
}

export async function verifyPair(out, expected) {
  const expectedPair = expected ?? buildPair(await readFixedSource());
  if (!isAbsolute(out)) fail("verify_absolute");
  const input = await lstat(out).catch(() => fail("verify_root"));
  if (input.isSymbolicLink()) fail("verify_root_link");
  const root = await realpath(out).catch(() => fail("verify_root"));
  const scriptOutput = [...expectedPair.files.keys()].find((name) => name !== HTML_OUTPUT);
  await exactInventory(root, scriptOutput);
  const manifestRaw = await regularBytes(join(root, MANIFEST), MAX_HTML);
  if (!manifestRaw.equals(Buffer.from(canonical(expectedPair.manifest)))) fail("manifest_bytes");
  for (const [name, bytes] of expectedPair.files) {
    const actual = await regularBytes(join(root, name), MAX_JS);
    if (!actual.equals(bytes)) fail("output_content");
  }
  return expectedPair.manifest;
}

async function main(argv) {
  if (argv.length !== 2 || !["--out", "--verify"].includes(argv[0])) fail("usage");
  const pair = buildPair(await readFixedSource());
  if (argv[0] === "--out") await writePair(argv[1], pair);
  else await verifyPair(argv[1], pair);
  process.stdout.write(`${JSON.stringify(pair.manifest)}\n`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch(() => {
    process.stderr.write("Jobs static auth pair refused.\n");
    process.exitCode = 1;
  });
}
