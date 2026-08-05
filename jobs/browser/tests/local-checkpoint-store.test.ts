import { execFile } from "node:child_process";
import { copyFile, mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import { afterEach, describe, expect, it } from "vitest";
import { restartDisposition } from "@bluey/jobs-automation";
import { acquireFinalSubmitAuthority, finalSubmitMarkerExists } from "../src/irreversible-submit.js";
import { scopedLocalRunAuthorization } from "../src/local-capabilities.js";
import {
  LocalCheckpointStore,
  type LocalRunCheckpoint,
} from "../src/local-checkpoint-store.js";

const temporaryDirectories: string[] = [];
const execFileAsync = promisify(execFile);
const SECURE_STORE_ENVIRONMENT = [
  "BLUEY_USE_OS_KEYCHAIN",
  "BLUEY_USE_SECURE_STORE",
  "BLUEY_LEGACY_KEYRING_FALLBACK",
  "BLUEY_ALLOW_PLAINTEXT_TOKENS",
] as const;

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, {
    recursive: true,
    force: true,
  })));
});

describe("encrypted local browser checkpoints", () => {
  it("survives a hard store restart without plaintext at rest", async () => {
    const root = await temporaryDirectory();
    const first = await LocalCheckpointStore.open(root);
    const checkpoint = fixture();
    const scope = first.scopeFor(checkpoint.request);
    await first.write(checkpoint);

    const encryptedPath = join(first.directory, `${scope}.json.enc`);
    const encrypted = await readFile(encryptedPath);
    expect(encrypted.subarray(0, 8).toString("ascii")).toBe("BLUEYLJ1");
    expect(encrypted.includes(Buffer.from("person@example.test"))).toBe(false);
    expect(encrypted.includes(Buffer.from("scoped-resume-capability"))).toBe(false);
    await expectPrivatePath(encryptedPath, 0o600);
    await expectPrivatePath(join(root, "recovery", "checkpoint-key-v1"), 0o600);
    await expectPrivatePath(join(root, "recovery"), 0o700);
    await expectPrivatePath(first.directory, 0o700);

    const restarted = await LocalCheckpointStore.open(root);
    await expect(restarted.read(scope)).resolves.toEqual(checkpoint);
  });

  it("binds ciphertext to its account, identity, and run scope", async () => {
    const root = await temporaryDirectory();
    const store = await LocalCheckpointStore.open(root);
    const source = fixture();
    const target = fixture({ runId: "run-other" });
    const sourceScope = store.scopeFor(source.request);
    const targetScope = store.scopeFor(target.request);
    await store.write(source);
    await copyFile(
      join(store.directory, `${sourceScope}.json.enc`),
      join(store.directory, `${targetScope}.json.enc`),
    );

    await expect(store.read(targetScope)).rejects.toThrow("authentication failed");
  });

  it("lists and removes only canonical encrypted checkpoints", async () => {
    const root = await temporaryDirectory();
    const store = await LocalCheckpointStore.open(root);
    const checkpoint = fixture();
    const scope = store.scopeFor(checkpoint.request);
    await store.write(checkpoint);

    await expect(store.list()).resolves.toEqual([{ scope, checkpoint }]);
    await store.remove(scope);
    await expect(store.list()).resolves.toEqual([]);
  });

  it("round-trips the non-PII manual-submission side-effect reason", async () => {
    const root = await temporaryDirectory();
    const store = await LocalCheckpointStore.open(root);
    const checkpoint: LocalRunCheckpoint = {
      ...fixture(),
      phase: "side_effect_unknown",
      workflow: {
        status: "side_effect_unknown",
        adapter: "greenhouse",
        approvedSubmitActionConsumed: true,
        sideEffectReason: "manual_submission_observed",
      },
    };
    const scope = store.scopeFor(checkpoint.request);

    await store.write(checkpoint);

    await expect(store.read(scope)).resolves.toEqual(checkpoint);
  });

  it("loads legacy checkpoints without manufacturing submit authority", async () => {
    const root = await temporaryDirectory();
    const store = await LocalCheckpointStore.open(root);
    const checkpoint = fixture();
    delete checkpoint.delivery.capabilities.submit;
    const scope = store.scopeFor(checkpoint.request);
    await store.write(checkpoint);

    const restored = await store.read(scope);
    expect(restored?.delivery.capabilities.submit).toBeUndefined();
    expect(() => scopedLocalRunAuthorization(
      restored!.delivery.capabilities,
      "submit",
      Date.parse("2026-07-16T12:02:00.000Z"),
    )).toThrow("unavailable");
  });

  it("restores before the durable submit marker and fails closed after it", async () => {
    const root = await temporaryDirectory();
    const runDirectory = join(root, "run");
    await mkdir(runDirectory, { recursive: true });
    const store = await LocalCheckpointStore.open(root);
    const checkpoint = {
      ...fixture(),
      phase: "prepared" as const,
      workflow: { status: "prepared" as const, adapter: "greenhouse" },
    };
    const scope = store.scopeFor(checkpoint.request);
    await store.write(checkpoint);
    const restarted = await LocalCheckpointStore.open(root);
    const rehydrated = await restarted.read(scope);
    expect(rehydrated).toBeDefined();
    expect(restartDisposition(
      rehydrated!.phase,
      rehydrated!.expiresAtMs,
      Date.parse("2026-07-16T12:02:00.000Z"),
      false,
    )).toBe("restore");

    await acquireFinalSubmitAuthority(runDirectory);
    expect(await finalSubmitMarkerExists(runDirectory)).toBe(true);
    expect(restartDisposition(
      rehydrated!.phase,
      rehydrated!.expiresAtMs,
      Date.parse("2026-07-16T12:02:00.000Z"),
      true,
    )).toBe("side_effect_unknown");
  });

  it("never loads Windows safeStorage when every secure-store switch is disabled", async () => {
    const root = await temporaryDirectory();
    let safeStorageLoads = 0;

    await withSecureStoreEnvironment("0", async () => {
      const store = await LocalCheckpointStore.open(root, {
        loadWindowsSafeStorage: async () => {
          safeStorageLoads += 1;
          throw new Error("safeStorage must not be loaded while opted out");
        },
      });
      const checkpoint = fixture();
      const scope = store.scopeFor(checkpoint.request);
      await store.write(checkpoint);
      await expect(store.read(scope)).resolves.toEqual(checkpoint);
    });

    expect(safeStorageLoads).toBe(0);
    const key = await readFile(join(root, "recovery", "checkpoint-key-v1"));
    expect(key).toHaveLength(32);
    expect(key.subarray(0, 7).toString("ascii")).not.toBe("BLUEYLK1");
    await expectPrivatePath(join(root, "recovery", "checkpoint-key-v1"), 0o600);
  });

  it("fails closed on a legacy DPAPI key envelope after secure-store opt-out", async () => {
    const root = await temporaryDirectory();
    const recovery = join(root, "recovery");
    await mkdir(recovery, { recursive: true });
    await writeFile(
      join(recovery, "checkpoint-key-v1"),
      Buffer.concat([Buffer.from("BLUEYLK1"), Buffer.alloc(48, 7)]),
    );
    let safeStorageLoads = 0;

    await withSecureStoreEnvironment("0", async () => {
      await expect(LocalCheckpointStore.open(root, {
        loadWindowsSafeStorage: async () => {
          safeStorageLoads += 1;
          throw new Error("safeStorage must not be loaded while opted out");
        },
      })).rejects.toThrow("Invalid local checkpoint key file");
    });

    expect(safeStorageLoads).toBe(0);
  });
});

function fixture(requestOverrides: Record<string, unknown> = {}): LocalRunCheckpoint {
  const expiresAtMs = Date.parse("2026-07-17T12:00:00.000Z");
  return {
    version: 1,
    phase: "needs_input",
    createdAtMs: Date.parse("2026-07-16T12:00:00.000Z"),
    updatedAtMs: Date.parse("2026-07-16T12:01:00.000Z"),
    expiresAtMs,
    request: {
      accountId: "account-123",
      applicationIdentityId: "identity-123",
      runId: "run-123",
      applicationId: "application-123",
      url: "https://jobs.example.test/apply",
      packet: {
        applicationId: "application-123",
        jobId: "job-123",
        resumeVersionId: "resume-123",
        applicationEmail: "person@example.test",
        answers: { private_question: "private answer" },
        verifiedClaimIds: [],
      },
      ...requestOverrides,
    },
    delivery: {
      apiOrigin: "https://bluey.example.test",
      capabilities: {
        result: "scoped-result-capability",
        resume: "scoped-resume-capability",
        submit: "scoped-submit-capability",
        expiresAtMs,
        runId: String(requestOverrides.runId || "run-123"),
      },
    },
    browser: { url: "https://jobs.example.test/apply/review" },
    workflow: { status: "needs_input", adapter: "greenhouse" },
    providerFinalReview: { adapter: "greenhouse", adapterVersion: "2026.07.1" },
    events: [{ event: "needs_input", details: {}, at: "2026-07-16T12:01:00.000Z" }],
  };
}

async function temporaryDirectory(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "bluey-local-checkpoint-"));
  temporaryDirectories.push(path);
  return path;
}

async function expectPrivatePath(path: string, posixMode: number): Promise<void> {
  if (process.platform !== "win32") {
    expect((await stat(path)).mode & 0o777).toBe(posixMode);
    return;
  }
  const systemRoot = process.env.SystemRoot || process.env.WINDIR || "C:\\Windows";
  const options = { windowsHide: true, timeout: 10_000, maxBuffer: 64 * 1024 } as const;
  const [{ stdout: acl }, { stdout: account }] = await Promise.all([
    execFileAsync(join(systemRoot, "System32", "icacls.exe"), [path], options),
    execFileAsync(join(systemRoot, "System32", "whoami.exe"), [], options),
  ]);
  expect(acl.toLowerCase()).toContain(account.trim().toLowerCase());
  expect(acl).not.toContain("(I)");
}

async function withSecureStoreEnvironment(
  value: string,
  action: () => Promise<void>,
): Promise<void> {
  const previous = new Map(SECURE_STORE_ENVIRONMENT.map((name) => [name, process.env[name]]));
  for (const name of SECURE_STORE_ENVIRONMENT) process.env[name] = value;
  try {
    await action();
  } finally {
    for (const name of SECURE_STORE_ENVIRONMENT) {
      const old = previous.get(name);
      if (old === undefined) delete process.env[name];
      else process.env[name] = old;
    }
  }
}
