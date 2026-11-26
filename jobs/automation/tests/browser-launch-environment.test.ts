import { describe, expect, it } from "vitest";
import { browserLaunchEnvironment } from "../src/browser-launch-environment.js";

const COMMON_KEYS = [
  "PATH",
  "HOME",
  "TMPDIR",
  "TMP",
  "TEMP",
  "LANG",
  "LC_ALL",
  "LC_CTYPE",
  "TZ",
];
const OS_KEYS = {
  darwin: ["__CF_USER_TEXT_ENCODING"],
  win32: [
    "SystemRoot",
    "WINDIR",
    "SystemDrive",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
  ],
  linux: [
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "XDG_RUNTIME_DIR",
    "XDG_CONFIG_HOME",
    "XDG_CACHE_HOME",
    "XDG_DATA_HOME",
  ],
};
const PLATFORMS = ["darwin", "win32", "linux"] as const;
const EXCLUDED_KEYS = [
  "BLUEY_JOBS_RUNNER_TOKEN",
  "BLUEY_JOBS_PROFILE_ENCRYPTION_KEY",
  "BLUEY_JOBS_WORKER_SIGNING_KEY",
  "BLUEY_JOBS_RUNNER_ADMISSION_GRANT_TOKEN",
  "BLUEY_JOBS_RUNNER_PROCESS_RUNTIME_GRANT_TOKEN",
  "BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_GRANT_TOKEN",
  "BLUEY_JOBS_RUNNER_SERVER_COMMAND_KEYS",
  "BLUEY_JOBS_API_ORIGIN",
  "FUTURE_PRIVATE_APPLICATION_SECRET",
  "OPENAI_API_KEY",
  "NODE_OPTIONS",
  "NODE_PATH",
  "LD_PRELOAD",
  "LD_LIBRARY_PATH",
  "DYLD_INSERT_LIBRARIES",
  "DYLD_LIBRARY_PATH",
  "HTTP_PROXY",
  "HTTPS_PROXY",
  "ALL_PROXY",
  "http_proxy",
  "https_proxy",
  "all_proxy",
  "DBUS_SESSION_BUS_ADDRESS",
  "PLAYWRIGHT_BROWSERS_PATH",
  "ELECTRON_RUN_AS_NODE",
];

describe("browser child environment allowlist", () => {
  it.each(PLATFORMS)("keeps exactly the documented OS keys for %s", (platform) => {
    const allKeys = [...new Set([...COMMON_KEYS, ...Object.values(OS_KEYS).flat()])];
    const input = Object.fromEntries(allKeys.map((name) => [name, `synthetic-${name}`]));
    const expectedKeys = [...COMMON_KEYS, ...OS_KEYS[platform]];
    const output = browserLaunchEnvironment(input, platform);
    expect(Object.keys(output).sort()).toEqual([...expectedKeys].sort());
    for (const name of expectedKeys) expect(output[name]).toBe(input[name]);
  });

  it.each(PLATFORMS)("drops known, future and injection credentials for %s", (platform) => {
    const input = Object.fromEntries(
      EXCLUDED_KEYS.map((name) => [name, "synthetic-excluded-sentinel"]),
    );
    const output = browserLaunchEnvironment({ ...input, PATH: "/fixture/bin" }, platform);
    expect(output).toEqual({ PATH: "/fixture/bin" });
    expect(Object.values(output)).not.toContain("synthetic-excluded-sentinel");
  });

  it("returns fresh defined empty objects for empty or undefined input", () => {
    const empty = browserLaunchEnvironment({}, "linux");
    const missing = browserLaunchEnvironment(undefined, "linux");
    expect(empty).toEqual({});
    expect(missing).toEqual({});
    expect(empty).not.toBe(missing);
  });

  it("preserves allowed empty strings and excludes undefined entries", () => {
    expect(browserLaunchEnvironment({ PATH: "", HOME: undefined, LANG: "" }, "darwin"))
      .toEqual({ PATH: "", LANG: "" });
  });

  it("does not mutate or alias the supplied environment", () => {
    const input = Object.freeze({ PATH: "/fixture/bin", BLUEY_JOBS_RUNNER_TOKEN: "synthetic-secret" });
    const output = browserLaunchEnvironment(input, "linux");
    output.PATH = "/changed-fixture/bin";
    expect(input).toEqual({ PATH: "/fixture/bin", BLUEY_JOBS_RUNNER_TOKEN: "synthetic-secret" });
    expect(browserLaunchEnvironment(input, "linux")).toEqual({ PATH: "/fixture/bin" });
  });

  it("does not read the value of an unknown future secret", () => {
    const input = { PATH: "/fixture/bin" };
    Object.defineProperty(input, "FUTURE_PRIVATE_SECRET", {
      enumerable: true,
      get() { throw new Error("unknown secret value must not be accessed"); },
    });
    expect(browserLaunchEnvironment(input, "linux")).toEqual({ PATH: "/fixture/bin" });
  });

  it.each(["darwin", "linux"] as const)("uses case-sensitive names on %s", (platform) => {
    expect(browserLaunchEnvironment({ PATH: "/fixture/bin", Path: "blocked", home: "blocked" }, platform))
      .toEqual({ PATH: "/fixture/bin" });
  });

  it("canonicalizes Windows OS names without changing their values", () => {
    expect(browserLaunchEnvironment({
      Path: "synthetic-path", systemroot: "synthetic-root", windir: "synthetic-windows",
      userprofile: "synthetic-user", localappdata: "synthetic-local",
      bluey_jobs_runner_token: "synthetic-secret", node_options: "synthetic-injection",
    }, "win32")).toEqual({
      PATH: "synthetic-path", SystemRoot: "synthetic-root", WINDIR: "synthetic-windows",
      USERPROFILE: "synthetic-user", LOCALAPPDATA: "synthetic-local",
    });
  });

  it("refuses defined case-insensitive Windows aliases rather than picking a winner", () => {
    expect(() => browserLaunchEnvironment({ PATH: "first", Path: "second" }, "win32"))
      .toThrow("Duplicate Windows browser environment variable");
  });

  it("does not treat an undefined Windows alias as a child entry", () => {
    expect(browserLaunchEnvironment({ PATH: undefined, Path: "synthetic-path" }, "win32"))
      .toEqual({ PATH: "synthetic-path" });
  });

  it("propagates an allowed-input failure without an inherited-environment fallback", () => {
    const input = {};
    Object.defineProperty(input, "PATH", {
      enumerable: true,
      get() { throw new Error("synthetic OS setting failure"); },
    });
    expect(() => browserLaunchEnvironment(input, "linux")).toThrow("synthetic OS setting failure");
  });

  it("refuses an unsupported platform even without input", () => {
    expect(() => browserLaunchEnvironment(undefined, "freebsd"))
      .toThrow("Unsupported browser launch platform");
  });
});
