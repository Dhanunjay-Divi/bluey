import { afterEach, describe, expect, it, vi } from "vitest";

import { globalDiscoveryWorkerFromEnvironment } from "../src/global-discovery-worker.js";

afterEach(() => {
  vi.unstubAllEnvs();
});

describe("global discovery worker startup", () => {
  it("refuses to start without an explicit approved source allowlist", () => {
    vi.stubEnv("BLUEY_JOBS_GLOBAL_DISCOVERY_SOURCE_FAMILIES", "");

    expect(() => globalDiscoveryWorkerFromEnvironment()).toThrow(
      /SOURCE_FAMILIES is required/,
    );
  });

  it("accepts a normalized explicit source allowlist", () => {
    vi.stubEnv("BLUEY_JOBS_GLOBAL_DISCOVERY_SOURCE_FAMILIES", " Lever,greenhouse,LEVER ");
    vi.stubEnv(
      "BLUEY_JOBS_WORKER_SIGNING_KEY",
      "global-discovery-test-signing-key-0123456789",
    );

    expect(() => globalDiscoveryWorkerFromEnvironment()).not.toThrow();
  });
});
