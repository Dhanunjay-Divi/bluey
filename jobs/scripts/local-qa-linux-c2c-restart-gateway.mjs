// Owned, candidate-only gateway for the saved Linux C2C test (not a login handoff).
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open, realpath } from "node:fs/promises";
import { resolve } from "node:path";
import {
  assertLocalQaCandidateResume, bindLocalQaCandidateReadback, requireLocalQaExpectedDraft,
} from "./local-qa-candidate-readback.mjs";
import { startLocalQaGateway } from "./local-qa-gateway.mjs";
import { exerciseLocalQaLinuxC2cBrowser } from "./local-qa-linux-c2c-browser.mjs";

const REFUSED = "local_qa_linux_c2c_restart_gateway_refused";
const UNCLOSED = "local_qa_linux_c2c_restart_gateway_unclosed";
const MAX_MANIFEST = 65_536;
const MAX_WINDOW = 180_000;
const EMPLOYER = /^https:\/\/boards\.greenhouse\.io\/blueynonprod\/jobs\/durable-c2c-qa-[0-9a-f]{32}$/;
const stamp = s => [s.dev, s.ino, s.uid, s.mode, s.nlink, s.size, s.mtimeNs, s.ctimeNs].join(":");
const whole = (regex, value) => typeof value === "string" && regex.exec(value)?.[0] === value;
function need(value) { if (!value) throw new Error(REFUSED); }
function exact(value, keys) {
  return value && Object.getPrototypeOf(value) === Object.prototype && Object.keys(value).sort().join(",") === keys;
}

export function validateLocalQaLinuxC2cBrowserBinding(input, excludedPorts) {
  need(exact(input, "openContext,port,webDirectory") && Number.isSafeInteger(input.port)
    && input.port > 1024 && input.port <= 65535 && ![5187, 5432, ...excludedPorts].includes(input.port)
    && typeof input.webDirectory === "string" && input.webDirectory !== "/"
    && resolve(input.webDirectory) === input.webDirectory && !/[\0\r\n]/.test(input.webDirectory)
    && typeof input.openContext === "function");
  return Object.freeze({ ...input });
}

export function bindLocalQaLinuxC2cRestartWorkspace(accountId, workspace, preparedDraft) {
  const draft = requireLocalQaExpectedDraft(preparedDraft);
  need(draft.fixtureMode === "c2c");
  const readback = bindLocalQaCandidateReadback(accountId, workspace, draft);
  const employerUrl = workspace.matches[0]?.canonical_url;
  need(whole(EMPLOYER, employerUrl) && workspace.applications[0]?.receipt?.job_snapshot?.canonical_url === employerUrl);
  return Object.freeze({ readback, preparedDraft: draft, employerUrl });
}

export async function loadManifest(path, expected) {
  need(await realpath(path) === path);
  const fd = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = await fd.stat({ bigint: true });
    need(before.isFile() && (before.uid === BigInt(process.getuid()) || before.uid === 0n)
      && before.nlink === 1n && (before.mode & 0o7022n) === 0n
      && before.size > 0n && before.size <= BigInt(MAX_MANIFEST));
    const buffer = Buffer.alloc(MAX_MANIFEST + 1);
    let size = 0;
    for (;;) {
      const { bytesRead } = await fd.read(buffer, size, buffer.length - size, null);
      size += bytesRead;
      need(size <= MAX_MANIFEST);
      if (bytesRead === 0) break;
    }
    const bytes = buffer.subarray(0, size);
    need(BigInt(size) === before.size && stamp(before) === stamp(await fd.stat({ bigint: true }))
      && stamp(before) === stamp(await lstat(path, { bigint: true }))
      && createHash("sha256").update(bytes).digest("hex") === expected);
    const value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
    need(value && Object.getPrototypeOf(value) === Object.prototype && Buffer.from(JSON.stringify(value) + "\n").equals(bytes));
    return Object.freeze(value);
  } finally { await fd.close(); }
}

export async function runLocalQaLinuxC2cRestartGateway({ browser, buildBinding, backendPort, preparedDraft,
  accountId, credentials, assertOwned, readWorkspace, readResume, readCensus, readMailAttempts, signal }) {
  let gateway, context;
  try {
    signal?.throwIfAborted();
    need(await assertOwned() === true);
    const manifest = await loadManifest(buildBinding.manifestFile, buildBinding.manifestSha256);
    const workspace = await readWorkspace();
    const bound = bindLocalQaLinuxC2cRestartWorkspace(accountId, workspace, preparedDraft);
    assertLocalQaCandidateResume(workspace, await readResume(bound.readback.resumeId));
    need(await assertOwned() === true && await readMailAttempts() === 0);
    signal?.throwIfAborted();
    gateway = await startLocalQaGateway({
      webDirectory: browser.webDirectory, manifest, backendPort, port: browser.port, candidateReadback: bound.readback,
      expiresAtMs: Date.now() + MAX_WINDOW,
      verifyBackend: async target => target.host === "127.0.0.1" && target.port === backendPort && await assertOwned() === true,
    });
    need(gateway.origin === `http://127.0.0.1:${browser.port}` && await assertOwned() === true);
    context = await browser.openContext(Object.freeze({ origin: gateway.origin, serviceWorkers: "block", signal }));
    // The driver owns the context from the first instruction and always closes it.
    const browserContext = context;
    context = undefined;
    const evidence = await exerciseLocalQaLinuxC2cBrowser({ context: browserContext, origin: gateway.origin,
      credentials, preparedDraft: bound.preparedDraft, employerUrl: bound.employerUrl, assertOwned, readCensus,
      readWorkspace, readMailAttempts, signal });
    need(await assertOwned() === true);
    return evidence;
  } catch (error) { throw new Error(/unclosed$/i.test(error?.message ?? "") ? UNCLOSED : REFUSED); }
  finally {
    try {
      if (context) await context.close();
      if (gateway) await gateway.close();
    } catch { throw new Error(UNCLOSED); }
  }
}
