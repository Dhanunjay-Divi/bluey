import { mkdir, readdir, rm } from "node:fs/promises";
import { join } from "node:path";

const HEADED_CHROMIUM = /^chromium-\d+$/;
const OPTIONAL_PLAYWRIGHT_PAYLOAD = /^(?:chromium_headless_shell|ffmpeg)-\d+$/;
const INSTALL_METADATA = new Set([".links", ".DS_Store"]);

export function classifyBrowserBundleEntries(names) {
  const entries = [...names].sort();
  return {
    headedChromium: entries.filter((name) => HEADED_CHROMIUM.test(name)),
    removable: entries.filter((name) => (
      OPTIONAL_PLAYWRIGHT_PAYLOAD.test(name) || INSTALL_METADATA.has(name)
    )),
    unexpected: entries.filter((name) => (
      !HEADED_CHROMIUM.test(name)
      && !OPTIONAL_PLAYWRIGHT_PAYLOAD.test(name)
      && !INSTALL_METADATA.has(name)
    )),
  };
}

export async function prepareBrowserBundleDirectory(bundleRoot) {
  await rm(bundleRoot, { recursive: true, force: true });
  await mkdir(bundleRoot, { recursive: true });
}

export async function retainHeadedChromiumOnly(bundleRoot) {
  const before = await readdir(bundleRoot, { withFileTypes: true });
  const classified = classifyBrowserBundleEntries(before.map((entry) => entry.name));
  const headedEntry = before.find((entry) => entry.name === classified.headedChromium[0]);
  if (classified.headedChromium.length !== 1
    || !headedEntry?.isDirectory()
    || classified.unexpected.length > 0) {
    throw new Error(
      `Unexpected Playwright browser bundle: ${JSON.stringify(classified)}`,
    );
  }
  await Promise.all(classified.removable.map((name) => (
    rm(join(bundleRoot, name), {
      recursive: true,
      force: true,
    })
  )));
  const after = await readdir(bundleRoot, { withFileTypes: true });
  const retained = classifyBrowserBundleEntries(after.map((entry) => entry.name));
  const retainedEntry = after.find((entry) => entry.name === retained.headedChromium[0]);
  if (retained.headedChromium.length !== 1
    || !retainedEntry?.isDirectory()
    || retained.removable.length > 0
    || retained.unexpected.length > 0) {
    throw new Error(`Playwright browser bundle pruning failed: ${JSON.stringify(retained)}`);
  }
  return retained.headedChromium[0];
}
