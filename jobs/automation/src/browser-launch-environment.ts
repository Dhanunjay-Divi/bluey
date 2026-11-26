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
] as const;

const PLATFORM_KEYS = {
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
} as const;

/**
 * Browser processes receive OS startup settings, never the parent's complete
 * credential environment. Undefined input still produces a defined empty env,
 * so Playwright cannot fall back to inheritance. Path custody remains external.
 */
export function browserLaunchEnvironment(
  source: Readonly<Record<string, string | undefined>> | undefined,
  platform: NodeJS.Platform,
): Record<string, string> {
  if (platform !== "darwin" && platform !== "win32" && platform !== "linux") {
    throw new Error("Unsupported browser launch platform");
  }
  const windows = platform === "win32";
  const allowed = new Map<string, string>();
  for (const name of [...COMMON_KEYS, ...PLATFORM_KEYS[platform]]) {
    allowed.set(windows ? name.toUpperCase() : name, name);
  }
  const input = source ?? {};
  const output: Record<string, string> = {};
  for (const name of Object.keys(input)) {
    const canonical = allowed.get(windows ? name.toUpperCase() : name);
    if (canonical === undefined) continue;
    // Filter names before reading values, including unknown future secrets.
    const value = input[name];
    if (value === undefined) continue;
    if (Object.hasOwn(output, canonical)) {
      throw new Error("Duplicate Windows browser environment variable");
    }
    output[canonical] = value;
  }
  return output;
}
