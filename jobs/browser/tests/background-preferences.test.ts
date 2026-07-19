import { mkdtemp, readFile, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  BrowserPreferenceStore,
  closeDisposition,
  parseBrowserPreferences,
} from "../src/background-preferences.js";

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, { recursive: true, force: true })));
});

describe("background preference", () => {
  it("hides on close only after explicit persisted opt-in", async () => {
    const root = await temporaryDirectory();
    const first = await BrowserPreferenceStore.open(root);
    expect(first.snapshot().backgroundEnabled).toBe(false);
    expect(closeDisposition({ backgroundEnabled: false, quitting: false })).toBe("quit");

    await first.setBackgroundEnabled(true);
    const restarted = await BrowserPreferenceStore.open(root);
    expect(restarted.snapshot().backgroundEnabled).toBe(true);
    expect(closeDisposition({ backgroundEnabled: true, quitting: false })).toBe("hide");
    expect(closeDisposition({ backgroundEnabled: true, quitting: true })).toBe("close");

    const file = join(root, "preferences", "browser-controller-v1.json");
    expect(JSON.parse(await readFile(file, "utf8"))).toEqual({ version: 1, backgroundEnabled: true });
    if (process.platform !== "win32") expect((await stat(file)).mode & 0o777).toBe(0o600);
  });

  it("fails closed for malformed or future preference records", () => {
    expect(parseBrowserPreferences(null).backgroundEnabled).toBe(false);
    expect(parseBrowserPreferences({ version: 2, backgroundEnabled: true }).backgroundEnabled).toBe(false);
    expect(parseBrowserPreferences({ version: 1, backgroundEnabled: "yes" }).backgroundEnabled).toBe(false);
  });
});

async function temporaryDirectory(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "bluey-browser-preferences-"));
  temporaryDirectories.push(path);
  return path;
}
