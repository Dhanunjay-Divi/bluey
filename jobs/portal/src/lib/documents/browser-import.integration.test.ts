import { accessSync, constants, statSync } from "node:fs";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { spawn } from "node:child_process";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { describe, expect, it } from "vitest";

const executable = chromium.executablePath();
const fixtureScript = fileURLToPath(
  new URL("../../../nonprod/document-import.integration.mjs", import.meta.url),
);
const hasConfiguredChromium = (() => {
  try {
    accessSync(executable, constants.X_OK);
    return statSync(executable).isFile();
  } catch {
    return false;
  }
})();

describe("written production resume import in private Chromium", () => {
  // Qualification installs this exact engine and rejects all skips. Never use an owner profile or system Chrome.
  it.skipIf(!hasConfiguredChromium)(
    "imports PDF, DOCX symbols and TXT while denying malformed and oversized input",
    async () => {
      const root = await mkdtemp(join(tmpdir(), "bluey-document-import-"));
      let cleanupProven = false;
      try {
        const home = join(root, "home");
        const temp = join(root, "tmp");
        await mkdir(home, { mode: 0o700 });
        await mkdir(temp, { mode: 0o700 });
        const child = spawn(process.execPath, [fixtureScript], {
          cwd: dirname(fixtureScript),
          env: {
            PATH: `${dirname(process.execPath)}:/usr/bin:/bin`,
            HOME: home,
            XDG_CONFIG_HOME: join(home, "config"),
            TMPDIR: temp,
            TMP: temp,
            TEMP: temp,
            LANG: "en_US.UTF-8",
            TZ: "UTC",
            CI: "1",
            NO_COLOR: "1",
            BLUEY_DOCUMENT_FIXTURE_ROOT: root,
            BLUEY_DOCUMENT_CHROMIUM_EXECUTABLE: executable,
          },
          stdio: ["ignore", "pipe", "pipe"],
        });
        let timedOut = false;
        let oversizedOutput = false;
        const termTimer = setTimeout(() => {
          timedOut = true;
          child.kill("SIGTERM");
        }, 75_000);
        const killTimer = setTimeout(() => child.kill("SIGKILL"), 90_000);
        let stdout = "";
        let stderr = "";
        child.stdout.on("data", (data: Buffer) => {
          stdout += data;
          if (stdout.length > 1_048_576) {
            oversizedOutput = true;
            stdout = stdout.slice(-1_048_576);
            child.kill("SIGTERM");
          }
        });
        child.stderr.on("data", (data: Buffer) => {
          stderr += data;
          if (stderr.length > 16_384) {
            oversizedOutput = true;
            stderr = stderr.slice(-16_384);
            child.kill("SIGTERM");
          }
        });
        let closed;
        try {
          closed = await new Promise<{ code: number | null; signal: NodeJS.Signals | null }>((resolve, reject) => {
            child.once("error", reject);
            child.once("close", (code, signal) => resolve({ code, signal }));
          });
        } finally {
          clearTimeout(termTimer);
          clearTimeout(killTimer);
        }
        expect(timedOut).toBe(false);
        expect(oversizedOutput).toBe(false);
        expect(closed, `${stdout}\n${stderr}`).toEqual({ code: 0, signal: null });
        const reports = stdout.split("\n").filter((line) => (
          line.startsWith('{"event":"document_import_qualification",')
        ));
        expect(reports).toHaveLength(1);
        const report: unknown = JSON.parse(reports[0]);
        expect(report).toMatchObject({
          event: "document_import_qualification",
          cases: ["pdf", "docx-symbol", "txt", "malformed", "oversize"],
          passed: 5,
          unexpectedRequests: 0,
          pageErrors: 0,
          realPdfWorker: true,
          browserClosed: true,
          engineClosed: true,
          serverClosed: true,
          listenerClosed: true,
        });
        cleanupProven = true;
        console.info(reports[0]);
      } finally {
        if (cleanupProven) await rm(root, { recursive: true, force: true });
        else console.error(`Document fixture retained for controller cleanup: ${root}`);
      }
    },
    100_000,
  );
});
