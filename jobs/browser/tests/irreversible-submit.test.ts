import { mkdtemp, mkdir, readFile, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  FINAL_SUBMIT_MARKER_FILE,
  acquireFinalSubmitAuthority,
  durableFinalSubmitHooks,
  finalSubmitMarkerExists,
  recordReconciledSubmitConfirmation,
} from "../src/irreversible-submit.js";
import { classifyLocalFailure, LocalBrowserError } from "../src/local-failure.js";
import { identityProfileDirectory } from "../src/profile.js";

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, {
    recursive: true,
    force: true,
  })));
});

describe("irreversible submit authority", () => {
  it("atomically rejects duplicate marker acquisition", async () => {
    const runDirectory = await temporaryRunDirectory();
    const now = () => new Date("2026-07-12T12:00:00.000Z");

    const attempts = await Promise.allSettled([
      acquireFinalSubmitAuthority(runDirectory, now),
      acquireFinalSubmitAuthority(runDirectory, now),
    ]);

    expect(attempts.filter((attempt) => attempt.status === "fulfilled")).toHaveLength(1);
    const rejection = attempts.find((attempt) => attempt.status === "rejected");
    expect(rejection).toMatchObject({
      status: "rejected",
      reason: { code: "submit_authority_exists" },
    });
    const markerPath = join(runDirectory, FINAL_SUBMIT_MARKER_FILE);
    if (process.platform === "win32") {
      await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(true);
      await expect(readFile(markerPath, "utf8")).resolves.toBe(
        '{"phase":"authority_acquired","at":"2026-07-12T12:00:00.000Z"}\n',
      );
    } else {
      expect((await stat(markerPath)).mode & 0o777).toBe(0o600);
    }
  });

  it("persists authority across a simulated process restart", async () => {
    const runDirectory = await temporaryRunDirectory();
    await acquireFinalSubmitAuthority(runDirectory);

    const restartedHooks = durableFinalSubmitHooks(runDirectory);

    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(true);
    await expect(restartedHooks.beforeFinalSubmit()).rejects.toMatchObject({
      code: "submit_authority_exists",
    });
  });

  it("persists explicit manual confirmation without creating duplicate authority", async () => {
    const runDirectory = await temporaryRunDirectory();
    await recordReconciledSubmitConfirmation(
      runDirectory,
      () => new Date("2026-07-12T12:00:00.000Z"),
    );
    await recordReconciledSubmitConfirmation(runDirectory);

    const marker = await readFile(join(runDirectory, FINAL_SUBMIT_MARKER_FILE), "utf8");
    expect(marker.trim().split("\n")).toHaveLength(1);
    expect(JSON.parse(marker)).toEqual({
      phase: "confirmation_reconciled",
      at: "2026-07-12T12:00:00.000Z",
    });
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(true);
  });

  it("stores only bounded non-PII phases and timestamps", async () => {
    const root = await temporaryDirectory();
    const secrets = [
      "person@example.com",
      "https://jobs.example.test/apply?token=secret-token",
      "resume-private.pdf",
      "My private application answer",
      "session-cookie-value",
    ];
    const runDirectory = join(
      identityProfileDirectory(root, "secret-account", secrets[0]),
      "runs",
      "run-private",
    );
    await mkdir(runDirectory, { recursive: true });
    const authority = await acquireFinalSubmitAuthority(
      runDirectory,
      () => new Date("2026-07-12T12:00:00.000Z"),
    );
    await authority.recordActivation("activated");

    const markerPath = join(runDirectory, FINAL_SUBMIT_MARKER_FILE);
    const bytes = await readFile(markerPath, "utf8");
    const records = bytes.trim().split("\n").map((line) => JSON.parse(line) as Record<string, unknown>);

    expect(Buffer.byteLength(bytes)).toBeLessThanOrEqual(512);
    expect(records).toHaveLength(2);
    expect(records.map((record) => Object.keys(record).sort())).toEqual([
      ["at", "phase"],
      ["at", "phase"],
    ]);
    expect(records.map((record) => record.phase)).toEqual([
      "authority_acquired",
      "activation_observed",
    ]);
    for (const secret of secrets) {
      expect(markerPath).not.toContain(secret);
      expect(bytes).not.toContain(secret);
    }
  });

  it("classifies failures before and after marker creation", async () => {
    const runDirectory = await temporaryRunDirectory();
    const rawException = new Error("person@example.com token=private");

    const before = await classifyLocalFailure(runDirectory, rawException);
    expect(before).toEqual({
      status: "failed",
      code: "browser_execution_failed",
      message: "Bluey Browser could not finish this application safely.",
      preservePage: false,
    });
    expect(JSON.stringify(before)).not.toContain(rawException.message);

    await acquireFinalSubmitAuthority(runDirectory);
    const after = await classifyLocalFailure(runDirectory, rawException);
    expect(after).toEqual({
      status: "side_effect_unknown",
      code: "submit_outcome_unknown",
      message: "Bluey cannot confirm whether the employer received this application. Review the preserved browser; Bluey will not submit again automatically.",
      preservePage: true,
    });
    expect(JSON.stringify(after)).not.toContain(rawException.message);
  });

  it("preserves uncertainty when confirmed submission evidence cannot be marked", async () => {
    const runDirectory = await temporaryRunDirectory();
    const result = await classifyLocalFailure(
      runDirectory,
      new LocalBrowserError("submit_outcome_unknown"),
    );

    expect(result).toMatchObject({
      status: "side_effect_unknown",
      code: "submit_outcome_unknown",
      preservePage: true,
    });
    await expect(finalSubmitMarkerExists(runDirectory)).resolves.toBe(false);
  });
});

async function temporaryRunDirectory(): Promise<string> {
  const root = await temporaryDirectory();
  const runDirectory = join(root, "identity-hash", "runs", "run-123");
  await mkdir(runDirectory, { recursive: true });
  return runDirectory;
}

async function temporaryDirectory(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "bluey-submit-marker-"));
  temporaryDirectories.push(path);
  return path;
}
