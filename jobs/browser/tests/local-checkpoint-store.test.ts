import { copyFile, mkdir, mkdtemp, readFile, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { restartDisposition } from "@bluey/jobs-automation";
import { acquireFinalSubmitAuthority, finalSubmitMarkerExists } from "../src/irreversible-submit.js";
import {
  LocalCheckpointStore,
  type LocalRunCheckpoint,
} from "../src/local-checkpoint-store.js";

const temporaryDirectories: string[] = [];

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
    expect((await stat(encryptedPath)).mode & 0o777).toBe(0o600);
    expect((await stat(join(root, "recovery", "checkpoint-key-v1"))).mode & 0o777).toBe(0o600);
    expect((await stat(join(root, "recovery"))).mode & 0o777).toBe(0o700);
    expect((await stat(first.directory)).mode & 0o777).toBe(0o700);

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
