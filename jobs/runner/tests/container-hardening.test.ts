import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";

describe("runner container storage boundary", () => {
  it("runs as the dedicated Playwright user with one owner-private persistent root", async () => {
    const dockerfile = await readFile(
      new URL("../Dockerfile", import.meta.url),
      "utf8",
    );

    expect(dockerfile).toContain(
      "install -d -o pwuser -g pwuser -m 0700 /var/lib/bluey-jobs-runner",
    );
    expect(dockerfile).toContain(
      "FROM rust:1.95.0-bookworm@sha256:",
    );
    expect(dockerfile).toContain(
      "FROM mcr.microsoft.com/playwright:v1.61.1-noble@sha256:",
    );
    expect(
      dockerfile.match(/^FROM [^\n]+@sha256:[0-9a-f]{64}(?: AS [^\n]+)?$/gm),
    ).toHaveLength(2);
    expect(dockerfile).toContain("cargo build --locked --release");
    expect(dockerfile).toContain(
      "target/release/libbluey_jobs_runner_native_storage.so",
    );
    expect(dockerfile).toContain(
      "/app/runner/dist/native/bluey_jobs_runner_native_storage.node",
    );
    expect(dockerfile).toContain(
      "ENV BLUEY_JOBS_RUNNER_DATA=/var/lib/bluey-jobs-runner",
    );
    expect(dockerfile).toMatch(/\nUSER pwuser\n/);
    expect(dockerfile).not.toContain("--no-sandbox");
    expect(dockerfile).not.toMatch(/\nUSER (?:0|root)\n/);

    const server = await readFile(
      new URL("../src/server.ts", import.meta.url),
      "utf8",
    );
    expect(server).toContain("process.umask(0o077)");
  });

  it("keeps the deployment templates aligned with the image root and volume authority", async () => {
    const operations = await readFile(
      new URL("../../OPERATIONS.md", import.meta.url),
      "utf8",
    );
    const runnerEnvironment = await readFile(
      new URL("../../../ops/bluey-jobs-runner.env.example", import.meta.url),
      "utf8",
    );
    const dockerIgnore = await readFile(
      new URL("../../.dockerignore", import.meta.url),
      "utf8",
    );

    for (const source of [operations, runnerEnvironment]) {
      expect(source).toContain(
        "BLUEY_JOBS_RUNNER_DATA=/var/lib/bluey-jobs-runner",
      );
      for (const variable of [
        "BLUEY_JOBS_RUNNER_ADMISSION_GRANT_ID",
        "BLUEY_JOBS_RUNNER_ADMISSION_GRANT_TOKEN",
        "BLUEY_JOBS_RUNNER_PROVIDER_RESOURCE_ID",
        "BLUEY_JOBS_RUNNER_RESOURCE_FINGERPRINT",
        "BLUEY_JOBS_RUNNER_BUILD_ID",
        "BLUEY_JOBS_RUNNER_SERVER_COMMAND_KEYS",
      ]) {
        expect(source).toContain(variable);
      }
    }
    expect(operations).not.toContain(
      "BLUEY_JOBS_RUNNER_DATA=/var/lib/bluey-jobs\n",
    );
    expect(runnerEnvironment).toContain(
      "BLUEY_JOBS_RUNNER_SERVER_COMMAND_KEYS='{\"",
    );
    expect(dockerIgnore.split(/\r?\n/)).toContain("**/target");
  });

  it("loads and smokes the Darwin addon in CI and release gates", async () => {
    const sources = await Promise.all(
      ["jobs-ci.yml", "release.yml"].map((workflow) =>
        readFile(
          new URL(`../../../.github/workflows/${workflow}`, import.meta.url),
          "utf8",
        ),
      ),
    );
    for (const source of sources) {
      expect(source).toContain(
        "libbluey_jobs_runner_native_storage.dylib",
      );
      expect(source).toContain(
        "jobs/runner/dist/native/bluey_jobs_runner_native_storage.node",
      );
      expect(source).toContain("jobs/scripts/native-runner-addon-smoke.mjs");
      expect(source).toContain("BLUEY_JOBS_RUNNER_NATIVE_SMOKE_ROOT");
    }
  });
});
