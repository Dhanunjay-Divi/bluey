import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  authorizedFinalSubmitHooks,
  type FinalSubmitFetch,
} from "../src/authorized-final-submit.js";
import { finalSubmitMarkerExists } from "../src/irreversible-submit.js";
import type { LocalRunDelivery } from "../src/local-run-contracts.js";

const temporaryDirectories: string[] = [];

afterEach(async () => {
  vi.useRealTimers();
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, {
    recursive: true,
    force: true,
  })));
});

describe("authorized final submit", () => {
  it("rejects an expired submit capability before network or marker acquisition", async () => {
    const runDirectory = await temporaryRunDirectory();
    const fetchMock = vi.fn<FinalSubmitFetch>();
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() - 1),
      fetchMock,
    );

    await expect(hooks.beforeFinalSubmit()).rejects.toMatchObject({
      code: "launch_expired",
    });
    expect(fetchMock).not.toHaveBeenCalled();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("rechecks expiry after the live-authority request returns", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
    const runDirectory = await temporaryRunDirectory();
    const fetchMock = vi.fn<FinalSubmitFetch>(async () => {
      vi.setSystemTime(2_000);
      return Response.json({ authorized: true, authorizedAtMs: 1_000 });
    });
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(2_000),
      fetchMock,
    );

    await expect(hooks.beforeFinalSubmit()).rejects.toMatchObject({ code: "launch_expired" });
    expect(fetchMock).toHaveBeenCalledOnce();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("fails closed on an explicit live-authority denial", async () => {
    const runDirectory = await temporaryRunDirectory();
    const fetchMock = vi.fn<FinalSubmitFetch>(async () => new Response(null, { status: 403 }));
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      fetchMock,
    );

    await expect(hooks.beforeFinalSubmit()).rejects.toMatchObject({ code: "launch_expired" });
    expect(fetchMock).toHaveBeenCalledOnce();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("fails closed on a live-authority network error", async () => {
    const runDirectory = await temporaryRunDirectory();
    const fetchMock = vi.fn<FinalSubmitFetch>(async () => {
      throw new TypeError("network unavailable");
    });
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      fetchMock,
    );

    await expect(hooks.beforeFinalSubmit()).rejects.toMatchObject({ code: "launch_expired" });
    expect(fetchMock).toHaveBeenCalledOnce();
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it.each([
    ["denied body", { authorized: false, authorizedAtMs: Date.now() }],
    ["missing timestamp", { authorized: true }],
    ["invalid timestamp", { authorized: true, authorizedAtMs: "now" }],
  ])("fails closed on malformed success: %s", async (_label, responseBody) => {
    const runDirectory = await temporaryRunDirectory();
    const fetchMock = vi.fn<FinalSubmitFetch>(async () => Response.json(responseBody));
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(Date.now() + 60_000),
      fetchMock,
    );

    await expect(hooks.beforeFinalSubmit()).rejects.toMatchObject({ code: "launch_expired" });
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });

  it("posts only the scoped submit capability before acquiring the durable marker", async () => {
    const runDirectory = await temporaryRunDirectory();
    const expiresAtMs = Date.now() + 60_000;
    const fetchMock = vi.fn<FinalSubmitFetch>(async () => Response.json({
      authorized: true,
      authorizedAtMs: Date.now(),
    }));
    const hooks = authorizedFinalSubmitHooks(
      runDirectory,
      requestBindings(),
      delivery(expiresAtMs),
      fetchMock,
    );

    await hooks.beforeFinalSubmit();
    expect(fetchMock).toHaveBeenCalledOnce();
    const [url, init] = fetchMock.mock.calls[0]!;
    expect(url).toBe("https://bluey.sh/api/jobs/local-runs/run-123/authorize-submit");
    expect(init).toMatchObject({
      method: "POST",
      headers: {
        Accept: "application/json",
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ capability: capability("submit", expiresAtMs) }),
      cache: "no-store",
      credentials: "omit",
      redirect: "error",
    });
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
      submit: capability("submit", expiresAtMs),
    },
  };
}

function capability(operation: "result" | "resume" | "submit", expiresAtMs: number): string {
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
