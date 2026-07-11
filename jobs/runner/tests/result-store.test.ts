import { randomBytes } from "node:crypto";
import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { readResult, resultPath, writeResult } from "../src/result-store.js";

describe("runner step result store", () => {
  it("round-trips an idempotent browser step result", async () => {
    const root = await mkdtemp(join(tmpdir(), "bluey-jobs-result-"));
    const key = randomBytes(32);
    const result = { receipt: { status: "submitted" }, receiptPath: "/receipt.json" };

    await writeResult(root, "run-123:resume:2", result, key);

    await expect(readResult(root, "run-123:resume:2", key)).resolves.toEqual(result);
    await expect(readResult(root, "run-123:resume:3", key)).resolves.toBeUndefined();
    await expect(readFile(resultPath(root, "run-123:resume:2"), "utf8")).resolves.not.toContain("submitted");
  });

  it("hashes request IDs instead of placing them in filesystem paths", () => {
    const path = resultPath("/tmp/runner", "../../another-account");

    expect(path.startsWith("/tmp/runner/step-results/")).toBe(true);
    expect(path).not.toContain("another-account");
  });
});
