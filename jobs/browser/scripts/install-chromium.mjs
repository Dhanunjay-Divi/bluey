import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  prepareBrowserBundleDirectory,
  retainHeadedChromiumOnly,
} from "./browser-bundle-content.mjs";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const require = createRequire(import.meta.url);
const playwrightRoot = dirname(require.resolve("playwright/package.json"));
const cli = join(playwrightRoot, "cli.js");
const browsers = join(packageRoot, "browser-bundle");
await prepareBrowserBundleDirectory(browsers);
const result = spawnSync(process.execPath, [cli, "install", "--no-shell", "chromium"], {
  cwd: packageRoot,
  env: { ...process.env, PLAYWRIGHT_BROWSERS_PATH: browsers },
  stdio: "inherit",
});

if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
const retained = await retainHeadedChromiumOnly(browsers);
console.log(`Bluey Browser retained headed Playwright runtime: ${retained}`);
