import { createCipheriv, createHash, randomBytes } from "node:crypto";
import { copyFile, mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { describe, expect, it } from "vitest";
import { encryptFile } from "../src/crypto-envelope.js";
import {
  readResult,
  resultPath,
  stageResult,
  writeResult,
  type DurableResultContext,
} from "../src/result-store.js";

const PROFILE_SCOPE = createHash("sha256").update("account-a\0identity-a").digest("hex").slice(0, 40);
const OTHER_PROFILE_SCOPE = createHash("sha256").update("account-b\0identity-b").digest("hex").slice(0, 40);

describe("runner step result store", () => {
  it("round-trips an idempotent browser step result", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-result-"));
    const key = randomBytes(32);
    const result = { receipt: { status: "submitted" }, receiptPath: "/receipt.json" };
    const context = resultContext("run-123:resume:2");

    await writeResult(root, context, result, key);

    await expect(readResult(root, context, key)).resolves.toEqual(result);
    await expect(readResult(root, resultContext("run-123:resume:3"), key)).resolves.toBeUndefined();
    const encrypted = await readFile(resultPath(root, context));
    expect(encrypted.subarray(0, 8).toString("ascii")).toBe("BLUEYJP2");
    expect(encrypted.toString("utf8")).not.toContain("submitted");
    expect((await stat(resultPath(root, context))).mode & 0o777).toBe(0o600);
  });

  it("hashes request IDs instead of placing them in filesystem paths", () => {
    const path = resultPath("/tmp/runner", resultContext("../../another-account"));

    expect(path.startsWith("/tmp/runner/step-results/")).toBe(true);
    expect(path).not.toContain("another-account");
    expect(path).not.toContain(PROFILE_SCOPE);
  });

  it("requires a hashed tenant/profile scope", () => {
    expect(() => resultPath("/tmp/runner", {
      requestId: "run-123:initial",
      profileScope: "raw-account-id",
    })).toThrow(expect.objectContaining({ code: "invalid_result_scope" }));
  });

  it("does not expose a staged terminal result until it is committed", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-result-stage-"));
    const key = randomBytes(32);
    const result = { receipt: { status: "submitted" }, receiptPath: "/receipt.json" };
    const context = resultContext("run-123:resume:1");

    await stageResult(root, context, result, key);
    await expect(readResult(root, context, key)).resolves.toBeUndefined();

    await writeResult(root, context, result, key);
    await expect(readResult(root, context, key)).resolves.toEqual(result);
  });

  it("rejects ciphertext swapped between durable request scopes", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-result-swap-"));
    const key = randomBytes(32);
    const source = resultContext("run-source:initial");
    const target = resultContext("run-target:initial");
    await writeResult(root, source, { receipt: { status: "submitted" } }, key);

    await copyFile(resultPath(root, source), resultPath(root, target));

    await expect(readResult(root, target, key)).rejects.toMatchObject({ code: "authentication_failed" });
  });

  it("isolates the same request ID across tenant/profile scopes", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-result-account-scope-"));
    const key = randomBytes(32);
    const source = resultContext("shared-run:initial");
    const target = resultContext("shared-run:initial", OTHER_PROFILE_SCOPE);
    const sourceResult = { receipt: { status: "submitted" }, tenant: "a" };
    const targetResult = { receipt: { status: "failed" }, tenant: "b" };
    await writeResult(root, source, sourceResult, key);
    await writeResult(root, target, targetResult, key);

    expect(resultPath(root, source)).not.toBe(resultPath(root, target));
    await expect(readResult(root, source, key)).resolves.toEqual(sourceResult);
    await expect(readResult(root, target, key)).resolves.toEqual(targetResult);

    await copyFile(resultPath(root, source), resultPath(root, target));
    await expect(readResult(root, target, key)).rejects.toMatchObject({ code: "authentication_failed" });
  });

  it("rejects tampered authenticated ciphertext", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-result-tamper-"));
    const key = randomBytes(32);
    const context = resultContext("run-tamper:initial");
    const path = resultPath(root, context);
    await writeResult(root, context, { receipt: { status: "submitted" } }, key);
    const encrypted = await readFile(path);
    encrypted[20] ^= 0x01;
    await writeFile(path, encrypted, { mode: 0o600 });

    await expect(readResult(root, context, key)).rejects.toMatchObject({ code: "authentication_failed" });
  });

  it("rejects plaintext and unknown crypto envelope versions", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-result-envelope-"));
    const key = randomBytes(32);
    const plaintext = resultContext("run-plaintext:initial");
    await writeRawResult(root, plaintext, Buffer.from('{"receipt":{"status":"submitted"}}'));
    await expect(readResult(root, plaintext, key)).rejects.toMatchObject({ code: "invalid_envelope" });

    const unknown = resultContext("run-unknown:initial");
    await writeRawResult(root, unknown, Buffer.concat([Buffer.from("BLUEYJP9"), randomBytes(32)]));
    await expect(readResult(root, unknown, key))
      .rejects.toMatchObject({ code: "unsupported_envelope_version" });
  });

  it("rejects a decrypted result without the exact committed/staged envelope", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-result-payload-"));
    const key = randomBytes(32);
    const context = resultContext("run-pre-envelope:initial");
    const preFixResult = { receipt: { status: "submitted" }, receiptPath: "/receipt.json" };
    await writeV2ResultPayload(root, context, preFixResult, key);

    await expect(readResult(root, context, key))
      .rejects.toMatchObject({ code: "invalid_result_envelope" });
  });

  it("rejects unknown decrypted result envelope versions", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-result-payload-version-"));
    const key = randomBytes(32);
    const context = resultContext("run-payload-version:initial");
    await writeV2ResultPayload(root, context, {
      __blueyResultEnvelope: 999,
      committed: true,
      result: { receipt: { status: "submitted" } },
    }, key);

    await expect(readResult(root, context, key))
      .rejects.toMatchObject({ code: "unsupported_result_envelope_version" });
  });

  it("rejects legacy BLUEYJP1 cached submissions instead of treating them as committed", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-result-legacy-"));
    const key = randomBytes(32);
    const context = resultContext("run-legacy:initial");
    const preFixResult = { receipt: { status: "submitted" }, receiptPath: "/receipt.json" };
    await writeLegacyResult(root, context, preFixResult, key);

    await expect(readResult(root, context, key)).rejects.toMatchObject({ code: "legacy_envelope" });
  });

  it("durably replaces the canonical result without leaving staging files", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-result-replace-"));
    const key = randomBytes(32);
    const context = resultContext("run-replace:initial");
    const path = resultPath(root, context);
    await writeResult(root, context, { revision: 1 }, key);
    await writeResult(root, context, { revision: 2 }, key);

    await expect(readResult(root, context, key)).resolves.toEqual({ revision: 2 });
    expect(await readdir(dirname(path))).toEqual([basename(path)]);
    expect((await stat(path)).mode & 0o777).toBe(0o600);
  });
});

function resultContext(requestId: string, profileScope = PROFILE_SCOPE): DurableResultContext {
  return { requestId, profileScope };
}

async function writeRawResult(root: string, context: DurableResultContext, contents: Buffer): Promise<void> {
  const path = resultPath(root, context);
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, contents, { mode: 0o600 });
}

async function writeV2ResultPayload(
  root: string,
  context: DurableResultContext,
  payload: unknown,
  key: Buffer,
): Promise<void> {
  const requestScope = createHash("sha256").update(context.requestId).digest("hex");
  const plaintext = join(root, `${requestScope}.json`);
  await writeFile(plaintext, JSON.stringify(payload), { mode: 0o600 });
  try {
    await encryptFile(plaintext, resultPath(root, context), key, {
      purpose: "durable-result",
      scope: context.profileScope,
      requestScope,
    });
  } finally {
    await rm(plaintext, { force: true });
  }
}

async function writeLegacyResult(
  root: string,
  context: DurableResultContext,
  payload: unknown,
  key: Buffer,
): Promise<void> {
  const iv = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", key, iv);
  const plaintext = Buffer.from(JSON.stringify(payload));
  const ciphertext = Buffer.concat([cipher.update(plaintext), cipher.final()]);
  await writeRawResult(root, context, Buffer.concat([
    Buffer.from("BLUEYJP1"),
    iv,
    ciphertext,
    cipher.getAuthTag(),
  ]));
}
