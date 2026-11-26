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
    expect(dockerfile).toContain('test "$(command -v node)" = /usr/bin/node');
    expect(dockerfile).toContain(
      "install -m 0555 /usr/bin/node /usr/local/bin/node",
    );
    expect(dockerfile).toContain("test -f /usr/local/bin/node");
    expect(dockerfile).toContain("! test -L /usr/local/bin/node");
    expect(dockerfile).toContain(
      "/usr/local/bin/node /tmp/managed-cloud-release-gate.mjs runtime-measurement",
    );
    expect(dockerfile).toContain(
      'CMD ["/usr/local/bin/node", "runner/dist/server.js"]',
    );
    expect(dockerfile).toContain("rm -rf /usr/lib/node_modules");
    expect(dockerfile).toContain("rm -f /usr/bin/corepack /usr/bin/npm /usr/bin/npx");
    expect(dockerfile).toContain("/usr/bin/node /usr/bin/yarn /usr/bin/yarnpkg");
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
    const startRoute = server.slice(
      server.indexOf('if (request.url === "/runs"'),
      server.indexOf("const resume = request.url?.match"),
    );
    expect(startRoute).toContain(
      "serializedProfileMutation(paths.scope, () =>\n        recoverOrExecuteRun<CloudRunResult>({",
    );
    expect(startRoute).toContain("execute: startFreshRun,");
    expect(startRoute).toContain("readCommitted: () => readMutationResult(resultContext, input),");
    expect(startRoute).toContain("recoverSubmittedCheckpoint: () => recoverSubmittedCheckpointForRequest(");
    expect(startRoute).toMatch(
      /async function startFreshRun\(\)[\s\S]*?if \(!managedCloudRuntimeReady\)[\s\S]*?await beginLeasedRun\(/,
    );
    const resumeRoute = server.slice(
      server.indexOf('if (resume && request.method === "POST")'),
      server.indexOf("const release = request.url?.match"),
    );
    expect(resumeRoute).toContain([
      "serializedBrowserSessionMutation(",
      "        resume[1]!,",
      "        resolution.profileScope,",
      "        () => recoverOrExecuteRun<CloudRunResult | undefined>({",
    ].join("\n"));
    expect(resumeRoute).toContain("execute: resumeRun,");
    expect(resumeRoute).toContain("readCommitted: () => readMutationResult(resultContext, binding),");
    expect(resumeRoute).toContain("recoverSubmittedCheckpoint: () => recoverSubmittedCheckpointForRequest(");
    expect(resumeRoute).toMatch(/const resumeRun =[\s\S]*?if \(!managedCloudRuntimeReady\)[\s\S]*?restoreCloudRunCheckpoint\(/);
    expect(resumeRoute).toContain('return json(response, 404, { error: "Browser run not found" });');
  });

  it("explicitly sandboxes and sanitizes every production persistent launch", async () => {
    for (const [path, expectedCalls] of [
      ["../../browser/src/browser-context-registry.ts", 1],
      ["../src/server.ts", 2],
    ] as const) {
      const source = await readFile(new URL(path, import.meta.url), "utf8");
      expect(source.match(/chromium\.launchPersistentContext\(/g)).toHaveLength(
        expectedCalls,
      );
      const launches = Array.from(
        source.matchAll(
          /chromium\.launchPersistentContext\([^,\n]+,\s*\{([\s\S]*?)\n\s*\}\);/g,
        ),
      );
      expect(launches).toHaveLength(expectedCalls);
      for (const launch of launches) {
        const options = launch[1] ?? "";
        expect(options).toMatch(/^\s*chromiumSandbox: true,/);
        expect(options.match(/\bchromiumSandbox:/g)).toHaveLength(1);
        expect(options).toMatch(
          /\n\s*env: browserLaunchEnvironment\(process\.env, process\.platform\),/,
        );
        expect(options.match(/\benv:/g)).toHaveLength(1);
      }
    }
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
      expect(source).toContain(
        "BLUEY_JOBS_SOURCE_COMMIT: ${{ github.event.pull_request.head.sha || github.sha }}",
      );
      expect(source).toContain(
        "ref: ${{ github.event.pull_request.head.sha || github.sha }}",
      );
      expect(source).toContain(
        '--build-arg "BLUEY_JOBS_SOURCE_COMMIT=$BLUEY_JOBS_SOURCE_COMMIT"',
      );
      expect(source).not.toContain(
        '--build-arg "BLUEY_JOBS_SOURCE_COMMIT=$GITHUB_SHA"',
      );
      expect(source).toContain("--entrypoint /usr/local/bin/node");
    }
  });
});
