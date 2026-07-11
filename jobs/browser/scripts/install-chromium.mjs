import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const require = createRequire(import.meta.url);
const playwrightRoot = dirname(require.resolve("playwright/package.json"));
const cli = join(playwrightRoot, "cli.js");
const browsers = join(packageRoot, "browser-bundle");
const result = spawnSync(process.execPath, [cli, "install", "chromium"], {
  cwd: packageRoot,
  env: { ...process.env, PLAYWRIGHT_BROWSERS_PATH: browsers },
  stdio: "inherit",
});

if (result.error) throw result.error;
process.exit(result.status ?? 1);
