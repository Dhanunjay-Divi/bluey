import { constants } from "node:fs";
import { lstat, open, realpath } from "node:fs/promises";
import { builtinModules } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "vite";

const PORTAL = fileURLToPath(new URL("..", import.meta.url));
export const ROOT_TEMPLATE = '<div id="root"><!--bluey-jobs-public-entry--></div>';
export const HTML_LIMIT = 256 * 1024;
const RENDERER_LIMIT = 4 * 1024 * 1024;
const SVG_LIMIT = 16 * 1024;
export const RENDERER_BUILTINS = Object.freeze([
  "node:async_hooks", "node:crypto", "node:stream", "node:util",
]);
// Platforms without O_NOFOLLOW still require matching lstat/fd identities and
// regular-file types before any write; a symlink never supplies write authority.
const noFollow = constants.O_NOFOLLOW ?? 0;

function requireCondition(condition, code) {
  if (!condition) throw new Error(`Jobs public prerender: ${code}`);
}

function sameFile(first, second) {
  return first.dev === second.dev && first.ino === second.ino
    && first.uid === second.uid && first.mode === second.mode && first.nlink === second.nlink;
}

function sameSnapshot(first, second) {
  return sameFile(first, second) && first.size === second.size
    && first.mtimeMs === second.mtimeMs && first.ctimeMs === second.ctimeMs;
}

function attribute(tag, name) {
  const matches = [...tag.matchAll(new RegExp(`\\s${name}\\s*=\\s*(["'])(.*?)\\1`, "gi"))];
  return matches.length === 1 ? matches[0][2] : null;
}

export async function readRegular(filename, limit) {
  requireCondition(await realpath(filename) === filename, "noncanonical input");
  const handle = await open(filename, constants.O_RDONLY | noFollow);
  try {
    const before = await handle.stat();
    requireCondition(before.isFile() && before.nlink === 1 && before.size > 0
      && before.size <= limit, "invalid input size or type");
    const bytes = await handle.readFile();
    requireCondition(bytes.length === before.size && sameSnapshot(before, await handle.stat())
      && sameSnapshot(before, await lstat(filename)), "input changed");
    return { bytes, identity: before };
  } finally {
    await handle.close();
  }
}

export function insertPublicEntry(template, markup) {
  requireCondition(Buffer.byteLength(template) <= HTML_LIMIT, "HTML too large");
  requireCondition(template.split(ROOT_TEMPLATE).length === 2
    && (template.match(/\bid\s*=\s*["']root["']/g) ?? []).length === 1,
  "missing or ambiguous root");
  // React may emit image preload links before the main element. They undergo
  // the same local-resource validation below; they are not a second root.
  requireCondition(typeof markup === "string"
    && /^(?:<link\b[^>]*\/>)*<main class="jobs-entry">/.test(markup)
    && Buffer.byteLength(markup) <= 128 * 1024 && !/<(?:script|iframe)\b/i.test(markup),
  "invalid public markup");
  const html = template.replace(ROOT_TEMPLATE,
    `<div id="root" data-bluey-jobs-prerendered="true">${markup}</div>`);
  requireCondition(Buffer.byteLength(html) <= HTML_LIMIT, "HTML too large");
  return html;
}

// Only resource elements are checked; ordinary /login, /terms and fragment links
// remain links, not local files. No remote resource dependency is admitted.
export async function validatePublicAssets(html, outputDirectory, webDirectory) {
  requireCondition(typeof html === "string" && Buffer.byteLength(html) <= HTML_LIMIT, "HTML too large");
  const tags = html.match(/<(?:script|link|img|source)\b[^>]*>/gi) ?? [];
  requireCondition(tags.length <= 64, "too many resource elements");
  let script = false;
  let stylesheet = false;
  for (const tag of tags) {
    requireCondition(!/\bsrcset\s*=/i.test(tag), "unexpected resource list");
    const reference = attribute(tag, /^<link\b/i.test(tag) ? "href" : "src");
    requireCondition(reference !== null, "missing resource reference");
    const inlineImage = /^<img\b/i.test(tag) || (/^<link\b/i.test(tag)
      && attribute(tag, "rel") === "preload" && attribute(tag, "as") === "image");
    if (inlineImage && /^data:image\/svg\+xml;base64,[A-Za-z0-9+/]+=*$/.test(reference)) {
      requireCondition(Buffer.byteLength(reference) <= SVG_LIMIT * 2, "inline image too large");
      const encoded = reference.slice("data:image/svg+xml;base64,".length);
      const decoded = Buffer.from(encoded, "base64");
      requireCondition(decoded.length > 0 && decoded.length <= SVG_LIMIT
        && decoded.toString("base64") === encoded, "invalid inline image");
      continue;
    }
    requireCondition(!/[?#%\\\s]/.test(reference)
      && reference.startsWith("/")
      && reference.slice(1).split("/").every((part) => part !== "" && part !== "." && part !== "..")
      && path.posix.normalize(reference) === reference, "invalid resource reference");
    const inJobs = reference.startsWith("/jobs/assets/");
    requireCondition(inJobs || reference === "/assets/favicon-512.png", "unresolved resource reference");
    const base = inJobs ? outputDirectory : webDirectory;
    const relative = inJobs ? reference.slice("/jobs/".length) : reference.slice(1);
    const filename = path.resolve(base, relative);
    requireCondition(filename.startsWith(`${base}${path.sep}`)
      && await realpath(filename) === filename, "noncanonical resource");
    const stat = await lstat(filename);
    requireCondition(stat.isFile() && stat.size > 0, "missing resource file");
    script ||= /^<script\b/i.test(tag) && inJobs && reference.endsWith(".js");
    stylesheet ||= /^<link\b/i.test(tag) && attribute(tag, "rel") === "stylesheet"
      && inJobs && reference.endsWith(".css");
  }
  requireCondition(script && stylesheet, "missing client entry or stylesheet");
}

export function validateRendererBundle(result) {
  requireCondition(result && !Array.isArray(result) && "output" in result && result.output.length === 1,
    "unexpected renderer bundle");
  const chunk = result.output[0];
  requireCondition(chunk.type === "chunk" && chunk.isEntry && chunk.dynamicImports.length === 0
    && Buffer.byteLength(chunk.code) <= RENDERER_LIMIT, "unresolved or oversized renderer");
  const observedImports = [...chunk.imports].sort();
  if (observedImports.length !== RENDERER_BUILTINS.length
    || !observedImports.every((name, index) => name === RENDERER_BUILTINS[index])) {
    const known = new Set(builtinModules.flatMap((name) => [name, `node:${name}`]));
    const observed = chunk.imports.filter((name) => known.has(name) && /^[a-z_:/]{1,64}$/.test(name));
    throw new Error(`Jobs public prerender: unapproved imports; builtin names=${observed.slice(0, 16).join(",")}`);
  }
  return chunk.code;
}

export async function renderPublicEntry() {
  const allowedImages = new Set([
    path.resolve(PORTAL, "../../web/assets/bluey-logo.svg"),
    path.resolve(PORTAL, "../../web/assets/bluey-wordmark.svg"),
  ]);
  const result = await build({
    configFile: false,
    root: PORTAL,
    base: "/jobs/",
    logLevel: "error",
    publicDir: false,
    envFile: false,
    define: { "process.env.NODE_ENV": '"production"' },
    esbuild: { jsx: "automatic" },
    plugins: [{
      name: "jobs-public-inline-brand-assets",
      enforce: "pre",
      async load(id) {
        if (!allowedImages.has(id)) return null;
        const { bytes } = await readRegular(id, SVG_LIMIT);
        return `export default ${JSON.stringify(`data:image/svg+xml;base64,${bytes.toString("base64")}`)};`;
      },
    }],
    ssr: { noExternal: true },
    build: {
      ssr: path.join(PORTAL, "scripts/prerender-entry.tsx"),
      target: "es2022",
      write: false,
      minify: false,
      sourcemap: false,
      rollupOptions: { output: {
        format: "es",
        inlineDynamicImports: true,
        paths: { async_hooks: "node:async_hooks", crypto: "node:crypto", stream: "node:stream", util: "node:util" },
      } },
    },
  });
  const code = validateRendererBundle(result);
  // The sole bounded chunk has only builtin externals. No temporary module,
  // dependency lookup outside the build, or renderer directory is created.
  const renderer = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
  requireCondition(typeof renderer.renderPublicEntry === "function", "missing renderer export");
  return renderer.renderPublicEntry();
}

export async function prerender() {
  const webDirectory = await realpath(path.resolve(PORTAL, "../../web"));
  const outputDirectory = path.join(webDirectory, "jobs");
  const filename = path.join(outputDirectory, "index.html");
  await prerenderFile(filename, outputDirectory, webDirectory);
  await verifyPrerenderedEntry();
}

export async function prerenderFile(filename, outputDirectory, webDirectory) {
  requireCondition(filename === path.join(outputDirectory, "index.html")
    && outputDirectory === path.join(webDirectory, "jobs"), "unexpected output path");
  const template = await readRegular(filename, HTML_LIMIT);
  const html = insertPublicEntry(template.bytes.toString("utf8"), await renderPublicEntry());
  await validatePublicAssets(html, outputDirectory, webDirectory);
  await writePrerenderedHtml(filename, template, html);
}

export async function writePrerenderedHtml(filename, template, html) {
  requireCondition(Buffer.byteLength(html) <= HTML_LIMIT, "HTML too large");
  // Open the existing client-built file without following a substituted link;
  // all rendering/asset checks finish before any generated output is changed.
  const handle = await open(filename, constants.O_RDWR | noFollow);
  try {
    const current = await handle.stat();
    requireCondition(current.isFile() && await realpath(filename) === filename
      && sameSnapshot(template.identity, current)
      && sameSnapshot(current, await lstat(filename))
      && (await handle.readFile()).equals(template.bytes)
      && sameSnapshot(current, await handle.stat()) && sameSnapshot(current, await lstat(filename)),
    "client HTML changed");
    await handle.truncate(0);
    const bytes = Buffer.from(html);
    const written = await handle.write(bytes, 0, bytes.length, 0);
    requireCondition(written.bytesWritten === bytes.length, "incomplete HTML write");
    await handle.sync();
    const completed = await handle.stat();
    requireCondition(sameFile(current, completed) && completed.size === bytes.length,
      "written HTML identity changed");
    const readBack = Buffer.alloc(bytes.length);
    const read = await handle.read(readBack, 0, readBack.length, 0);
    requireCondition(read.bytesRead === bytes.length && readBack.equals(bytes)
      && sameSnapshot(completed, await handle.stat()) && sameSnapshot(completed, await lstat(filename))
      && await realpath(filename) === filename,
    "HTML read-back or path changed");
  } finally {
    await handle.close();
  }
}

// Post-build verification can import this without invoking the renderer/build.
export async function verifyPrerenderedEntry() {
  const webDirectory = await realpath(path.resolve(PORTAL, "../../web"));
  const outputDirectory = path.join(webDirectory, "jobs");
  const html = (await readRegular(path.join(outputDirectory, "index.html"), HTML_LIMIT)).bytes.toString("utf8");
  requireCondition((html.match(/<div id="root" data-bluey-jobs-prerendered="true">/g) ?? []).length === 1
    && !html.includes("<!--bluey-jobs-public-entry-->"), "missing prerendered entry");
  await validatePublicAssets(html, outputDirectory, webDirectory);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    await prerender();
  } catch (error) {
    // An imported data: module can put its entire source in a native stack.
    // Emit only our bounded validation messages, never that stack or payload.
    const message = error instanceof Error && /^Jobs public prerender: [^\r\n]{1,2048}$/.test(error.message)
      ? error.message : "Jobs public prerender: build or render failed";
    console.error(message);
    process.exitCode = 1;
  }
}
