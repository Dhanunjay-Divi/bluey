import { mkdtemp, mkdir, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  classifyBrowserBundleEntries,
  prepareBrowserBundleDirectory,
  retainHeadedChromiumOnly,
} from "../scripts/browser-bundle-content.mjs";

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((path) => (
    rm(path, { recursive: true, force: true })
  )));
});

describe("packaged browser bundle", () => {
  it("classifies only full headed Chromium as runtime content", () => {
    expect(classifyBrowserBundleEntries([
      "ffmpeg-1011",
      "chromium-1228",
      ".links",
      "chromium_headless_shell-1228",
      "winldd-1007",
    ])).toEqual({
      headedChromium: ["chromium-1228"],
      removable: [".links", "chromium_headless_shell-1228", "ffmpeg-1011", "winldd-1007"],
      unexpected: [],
    });
  });

  it("removes optional Playwright payloads and rejects unexpected content", async () => {
    const root = await temporaryDirectory();
    for (const name of [
      "chromium-1228",
      "chromium_headless_shell-1228",
      "ffmpeg-1011",
      "winldd-1007",
      ".links",
    ]) await mkdir(join(root, name));

    expect(await retainHeadedChromiumOnly(root)).toBe("chromium-1228");
    expect(await readdir(root)).toEqual(["chromium-1228"]);

    await mkdir(join(root, "webkit-999"));
    await expect(retainHeadedChromiumOnly(root)).rejects.toThrow(/unexpected playwright browser bundle/i);
  });

  it("starts every install from an empty bundle so old Chromium revisions cannot accumulate", async () => {
    const root = await temporaryDirectory();
    await mkdir(join(root, "chromium-older"));
    await mkdir(join(root, "chromium-1227"));
    await prepareBrowserBundleDirectory(root);
    expect(await readdir(root)).toEqual([]);
  });

  it("rejects a file masquerading as the headed Chromium directory", async () => {
    const root = await temporaryDirectory();
    await writeFile(join(root, "chromium-1228"), "not a browser");
    await expect(retainHeadedChromiumOnly(root)).rejects.toThrow(/unexpected playwright browser bundle/i);
  });
});

async function temporaryDirectory(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "bluey-browser-bundle-"));
  temporaryDirectories.push(path);
  return path;
}
