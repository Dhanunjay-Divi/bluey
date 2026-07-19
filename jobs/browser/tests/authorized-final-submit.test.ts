import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
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
    const authorize = vi.fn(async () => undefined);
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() - 1),
      authorize,
    );

    await expect(hooks.beforeFinalSubmit()).rejects.toMatchObject({
      code: "launch_expired",
    });
    expect(authorize).not.toHaveBeenCalled();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("fails before marker acquisition when live authority denies or is unavailable", async () => {
    const runDirectory = await temporaryRunDirectory();
    const denial = new Error("live authority denied");
    const authorize = vi.fn(async () => {
      throw denial;
    });
    const expiresAtMs = Date.now() + 60_000;
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(expiresAtMs),
      authorize,
    );

    await expect(hooks.beforeFinalSubmit()).rejects.toBe(denial);
    expect(authorize).toHaveBeenCalledOnce();
    expect(authorize).toHaveBeenCalledWith(Object.freeze({
      ...requestBindings(),
      apiOrigin: "https://bluey.sh",
      capability: capability("result", expiresAtMs),
      expiresAtMs,
    }));
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("acquires durable authority while the claimed run is still valid", async () => {
    const runDirectory = await temporaryRunDirectory();
    const authorize = vi.fn(async () => undefined);
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      authorize,
    );

    await hooks.beforeFinalSubmit();
    expect(authorize).toHaveBeenCalledOnce();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(true);
  });
});

function requestBindings() {
  return {
    accountId: "account-123",
    applicationId: "application-123",
    applicationIdentityId: "identity-123",
    runId: "run-123",
  };
}

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
