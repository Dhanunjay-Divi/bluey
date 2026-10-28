// Owned synthetic document qualification only: no API, accounts, or external origins.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { createConnection } from "node:net";
import { lstat, mkdir, readFile, realpath } from "node:fs/promises";
import { dirname, extname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { Document, Packer, Paragraph, SymbolRun, TextRun } from "docx";
import { PDFDocument, StandardFonts } from "pdf-lib";
import { chromium } from "playwright";
import { build } from "vite";

const portalRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const entryId = "\0bluey-owned-document-import";
const expectedCases = ["pdf", "docx-symbol", "txt", "malformed", "oversize"];
const text = "Synthetic Candidate built reliable document validation and offline processing systems.";
const cases = [];
const unexpectedRequests = [];
const pageErrors = [];
const workerUrls = [];
const sockets = new Set();
let browser;
let browserServer;
let engineCompletion;
let server;
let browserClosed = false;
let engineClosed = false;
let serverClosed = false;
let cleanupPromise;

async function bounded(promise, label, milliseconds = 5_000) {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`timeout_${label}`)), milliseconds);
    })]);
  } finally { clearTimeout(timer); }
}

async function assertLoopbackClosed(port) {
  await new Promise((resolve, reject) => {
    const socket = createConnection({ host: "127.0.0.1", port });
    socket.setTimeout(2_000);
    socket.once("connect", () => { socket.destroy(); reject(new Error("fixture_listener_survived")); });
    socket.once("timeout", () => { socket.destroy(); reject(new Error("fixture_listener_check_timeout")); });
    socket.once("error", (error) => error.code === "ECONNREFUSED" ? resolve() : reject(error));
  });
}

function cleanup() {
  return cleanupPromise ??= (async () => {
    const outcomes = await Promise.allSettled([
      (async () => {
        if (!browser) return;
        await bounded(browser.close(), "browser_connection_close");
        browserClosed = !browser.isConnected();
      })(),
      (async () => {
        if (!browserServer) return;
        let gracefulError;
        try { await bounded(browserServer.close(), "engine_close"); }
        catch (error) {
          gracefulError = error;
          await bounded(browserServer.kill(), "engine_kill");
        }
        await bounded(engineCompletion, "engine_process_close");
        engineClosed = true;
        // Forced shutdown is cleanup, never a passing qualification result.
        if (gracefulError) throw gracefulError;
      })(),
      (async () => {
        if (!server) return;
        const closing = new Promise((resolve, reject) => {
          server.close((error) => error ? reject(error) : resolve());
        });
        for (const socket of sockets) socket.destroy();
        await bounded(closing, "http_close");
        serverClosed = !server.listening;
      })(),
    ]);
    const failure = outcomes.find((outcome) => outcome.status === "rejected");
    if (failure) throw failure.reason;
  })();
}

for (const signal of ["SIGTERM", "SIGINT", "SIGHUP"]) {
  process.once(signal, () => {
    void cleanup().then(() => process.exit(1), () => process.exit(1));
  });
}

async function main() {
  const requestedRoot = process.env.BLUEY_DOCUMENT_FIXTURE_ROOT;
  const executablePath = process.env.BLUEY_DOCUMENT_CHROMIUM_EXECUTABLE;
  assert.ok(requestedRoot && executablePath, "explicit_fixture_paths_required");
  const rootInfo = await lstat(requestedRoot);
  assert.ok(rootInfo.isDirectory() && !rootInfo.isSymbolicLink(), "fixture_root_directory");
  assert.equal(rootInfo.uid, process.getuid(), "fixture_root_owner");
  assert.equal(rootInfo.mode & 0o077, 0, "fixture_root_private");
  const root = await realpath(requestedRoot);
  const outDir = join(root, "built");
  await mkdir(outDir, { mode: 0o700 });
  const output = await build({
    root: portalRoot,
    configFile: join(portalRoot, "vite.config.ts"),
    envDir: false,
    publicDir: false,
    plugins: [{
      name: "bluey-owned-document-import-fixture",
      resolveId(id) { if (id === entryId) return entryId; },
      load(id) {
        if (id === entryId) {
          return `import { importResume, MAX_RESUME_BYTES } from ${JSON.stringify(join(portalRoot, "src/lib/documents/import.ts"))};
globalThis.blueyDocumentFixture = { importResume, MAX_RESUME_BYTES };`;
        }
      },
    }],
    build: {
      outDir,
      emptyOutDir: false,
      rollupOptions: { input: entryId },
    },
  });
  assert.ok(!Array.isArray(output) && "output" in output, "single_written_build");
  const assets = new Map();
  let entryPath;
  for (const item of output.output) {
    const path = join(outDir, item.fileName);
    assert.ok(!relative(outDir, path).startsWith(".."), "owned_build_path");
    const url = `/jobs/${item.fileName}`;
    assets.set(url, await readFile(path));
    if (item.type === "chunk" && item.isEntry) {
      assert.equal(entryPath, undefined, "single_fixture_entry");
      entryPath = url;
    }
  }
  assert.ok(entryPath, "written_fixture_entry");
  const workerPaths = [...assets.keys()].filter((path) => /pdf\.worker\.min-.*\.mjs$/.test(path));
  assert.equal(workerPaths.length, 1, "real_worker_asset");
  assert.ok(assets.get(workerPaths[0]).length > 1_000_000, "real_worker_not_url_shim");
  assets.set("/jobs/fixture.html", Buffer.from(
    `<!doctype html><html><head><link rel="icon" href="data:,"></head><body><script type="module" src="${entryPath}"></script></body></html>`,
  ));
  server = createServer((request, response) => {
    const body = assets.get(request.url);
    if (request.method !== "GET" || !body) {
      unexpectedRequests.push("server_request");
      response.writeHead(403).end();
      return;
    }
    const extension = extname(request.url);
    const contentType = extension === ".html" ? "text/html" : extension === ".css" ? "text/css" : "text/javascript";
    response.writeHead(200, {
      "content-type": `${contentType}; charset=utf-8`,
      "content-length": body.length,
      "cache-control": "no-store",
    });
    response.end(body);
  });
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.once("close", () => sockets.delete(socket));
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const port = server.address().port;
  const origin = `http://127.0.0.1:${port}`;
  browserServer = await chromium.launchServer({ executablePath, headless: true, timeout: 15_000 });
  const engineProcess = browserServer.process();
  engineCompletion = new Promise((resolve) => engineProcess.once("close", resolve));
  const endpoint = new URL(browserServer.wsEndpoint());
  assert.ok(["127.0.0.1", "localhost", "[::1]"].includes(endpoint.hostname), "owned_browser_endpoint");
  browser = await chromium.connect(browserServer.wsEndpoint(), { timeout: 15_000 });
  const context = await browser.newContext({ serviceWorkers: "block" });
  await context.route("**/*", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    if (url.origin !== origin || request.method() !== "GET" || !assets.has(url.pathname + url.search)) {
      unexpectedRequests.push("browser_request");
      await route.abort("blockedbyclient");
      return;
    }
    await route.continue();
  });
  const page = await context.newPage();
  page.setDefaultTimeout(10_000);
  page.on("pageerror", () => pageErrors.push("page_error"));
  page.on("worker", (worker) => workerUrls.push(worker.url()));
  await page.goto(`${origin}/jobs/fixture.html`, { waitUntil: "networkidle" });
  await page.waitForFunction(() => Boolean(globalThis.blueyDocumentFixture));

  const pdf = await PDFDocument.create();
  const font = await pdf.embedFont(StandardFonts.Helvetica);
  pdf.addPage([612, 792]).drawText(text, { font, size: 10, x: 40, y: 740 });
  const pdfBytes = Buffer.from(await pdf.save());
  const docxBytes = await Packer.toBuffer(new Document({
    sections: [{ children: [new Paragraph({ children: [
      new TextRun(text),
      new SymbolRun({ char: "F0A7", symbolfont: "Wingdings" }),
    ] })] }],
  }));
  const txtBytes = Buffer.from(text);
  const fixtures = [
    { name: "synthetic.pdf", type: "application/pdf", bytes: pdfBytes, kind: "pdf" },
    { name: "synthetic.docx", type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document", bytes: docxBytes, kind: "docx" },
    { name: "synthetic.txt", type: "text/plain", bytes: txtBytes, kind: "txt" },
  ];
  for (const fixture of fixtures) {
    const imported = await page.evaluate(async ({ name, type, bytes }) => {
      const file = new File([new Uint8Array(bytes)], name, { type });
      return globalThis.blueyDocumentFixture.importResume(file);
    }, { name: fixture.name, type: fixture.type, bytes: [...fixture.bytes] });
    assert.equal(imported.name, fixture.name, "imported_name");
    assert.equal(imported.file_type, fixture.kind, "imported_type");
    assert.ok(imported.text.includes(text), "complete_expected_text");
    if (fixture.kind === "pdf") {
      assert.equal(imported.page_count, 1, "pdf_page_count");
      assert.ok(workerUrls.includes(`${origin}${workerPaths[0]}`), "real_pdf_worker_observed");
    }
    if (fixture.kind === "docx") assert.ok(imported.text.includes("▪"), "wingdings_symbol_preserved");
    cases.push(expectedCases[cases.length]);
  }
  const malformed = await page.evaluate(async () => {
    try {
      await globalThis.blueyDocumentFixture.importResume(new File(["not a PDF"], "bad.pdf"));
      return "unexpected_success";
    } catch (error) { return error instanceof Error ? error.message : "unknown_error"; }
  });
  assert.equal(malformed, "That file is not a valid PDF. Choose the original PDF instead of a renamed file.");
  cases.push("malformed");
  const oversized = await page.evaluate(async () => {
    try {
      const fixture = globalThis.blueyDocumentFixture;
      await fixture.importResume(new File([new ArrayBuffer(fixture.MAX_RESUME_BYTES + 1)], "large.txt"));
      return "unexpected_success";
    } catch (error) { return error instanceof Error ? error.message : "unknown_error"; }
  });
  assert.equal(oversized, "That resume is larger than 10 MB. Choose a smaller PDF, DOCX, or TXT file.");
  cases.push("oversize");
  assert.deepEqual(cases, expectedCases, "all_document_cases");
  assert.deepEqual(unexpectedRequests, [], "no_unexpected_requests");
  assert.deepEqual(pageErrors, [], "no_page_errors");
  await context.close();
  await cleanup();
  assert.ok(browserClosed && engineClosed && serverClosed, "resources_closed");
  await assertLoopbackClosed(port);
  console.log(JSON.stringify({
    event: "document_import_qualification",
    cases,
    passed: cases.length,
    unexpectedRequests: unexpectedRequests.length,
    pageErrors: pageErrors.length,
    realPdfWorker: true,
    browserClosed,
    engineClosed,
    serverClosed,
    listenerClosed: true,
    fixtureHashes: fixtures.map(({ kind, bytes }) => ({ kind, sha256: createHash("sha256").update(bytes).digest("hex") })),
  }));
}

try {
  await main();
} finally {
  await cleanup();
}
