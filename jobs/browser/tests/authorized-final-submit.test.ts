import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { authorizedFinalSubmitHooks } from "../src/authorized-final-submit.js";
import { finalSubmitMarkerExists } from "../src/irreversible-submit.js";
import type { LocalRunDelivery } from "../src/local-run-contracts.js";

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, {
    recursive: true,
    force: true,
  })));
});

describe("authorized final submit", () => {
  it("fails before marker acquisition when claimed run authority has expired", async () => {
    const runDirectory = await temporaryRunDirectory();
    const hooks = authorizedFinalSubmitHooks(runDirectory, delivery(Date.now() - 1));

    await expect(hooks.beforeFinalSubmit()).rejects.toMatchObject({
      code: "launch_expired",
    });
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("acquires durable authority while the claimed run is still valid", async () => {
    const runDirectory = await temporaryRunDirectory();
    const hooks = authorizedFinalSubmitHooks(runDirectory, delivery(Date.now() + 60_000));

    await hooks.beforeFinalSubmit();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(true);
  });
});

function delivery(expiresAtMs: number): LocalRunDelivery {
  return {
    apiOrigin: "https://bluey.sh",
    capabilities: {
      runId: "run-123",
      expiresAtMs,
      result: capability("result", expiresAtMs),
      resume: capability("resume", expiresAtMs),
    },
  };
}

function capability(operation: "result" | "resume", expiresAtMs: number): string {
  const payload = Buffer.from(JSON.stringify({
    version: 1,
    audience: "bluey-jobs-local-run",
    account_id: "account-123",
    application_id: "application-123",
    run_id: "run-123",
    browser_profile_id: "profile-123",
    operation,
    expires_at_ms: expiresAtMs,
    nonce: "n".repeat(32),
  })).toString("base64url");
  return `${payload}.${"a".repeat(64)}`;
}

async function temporaryRunDirectory(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "bluey-authorized-submit-"));
  temporaryDirectories.push(root);
  const runDirectory = join(root, "run");
  await mkdir(runDirectory, { recursive: true });
  return runDirectory;
}
