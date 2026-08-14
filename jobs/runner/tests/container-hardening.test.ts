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
    ).toHaveLength(3);
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
    expect(dockerfile).toContain("! -name 'chromium_headless_shell-*'");
    expect(dockerfile).toContain("! -name 'ffmpeg-*'");
    expect(dockerfile).not.toContain("! -name 'chromium-*'");
    expect(dockerfile).toContain(
      'CMD ["/usr/local/bin/node", "runner/dist/server.js"]',
    );
    expect(dockerfile).toMatch(/\nUSER pwuser\n/);
    expect(dockerfile).not.toContain("--no-sandbox");
    expect(dockerfile).not.toMatch(/\nUSER (?:0|root)\n/);

    const server = await readFile(
      new URL("../src/server.ts", import.meta.url),
      "utf8",
    );
    expect(server).toContain("process.umask(0o077)");
    expect(server.match(/headless: true/g)).toHaveLength(2);
    expect(server).not.toContain("headless: false");
    const startRecovery = server.indexOf(
      "const recovered = await recoverSubmittedCheckpointForRequest(\n" +
        "          paths.scope",
    );
    const startAuthority = server.indexOf(
      "if (!managedCloudRuntimeReady)",
      startRecovery,
    );
    const startLease = server.indexOf("const { lease, execution }", startRecovery);
    expect(startRecovery).toBeGreaterThan(-1);
    expect(startAuthority).toBeGreaterThan(startRecovery);
    expect(startLease).toBeGreaterThan(startAuthority);
    const resumeRecovery = server.indexOf(
      "const recovered = await recoverSubmittedCheckpointForRequest(\n" +
        "            resolution.profileScope",
    );
    const resumeAuthority = server.indexOf(
      "if (!managedCloudRuntimeReady)",
      resumeRecovery,
    );
    const resumeRestore = server.indexOf("const active =", resumeRecovery);
    expect(resumeRecovery).toBeGreaterThan(-1);
    expect(resumeAuthority).toBeGreaterThan(resumeRecovery);
    expect(resumeRestore).toBeGreaterThan(resumeAuthority);
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
        "BLUEY_JOBS_RUNNER_PROCESS_RUNTIME_GRANT_ID",
        "BLUEY_JOBS_RUNNER_PROCESS_RUNTIME_GRANT_TOKEN",
        "BLUEY_JOBS_RUNNER_IMAGE_SHA256",
        "BLUEY_JOBS_AUTOMATION_BUNDLE_SHA256",
        "BLUEY_JOBS_PLAYWRIGHT_VERSION",
        "BLUEY_JOBS_CHROMIUM_REVISION",
        "BLUEY_JOBS_CHROMIUM_EXECUTABLE_SHA256",
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
    const dockerIgnoreLines = dockerIgnore.split(/\r?\n/);
    expect(dockerIgnoreLines[0]).toBe("**");
    expect(dockerIgnoreLines).toContain("!runner/native-storage/src/**");
    expect(dockerIgnoreLines).toContain(
      "!scripts/managed-cloud-release-gate.mjs",
    );
    expect(dockerIgnoreLines.some((line) => line.includes("target"))).toBe(false);
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
