import { copyFile, mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const browserRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const source = join(browserRoot, "src", "renderer");
const destination = join(browserRoot, "dist", "renderer");

await mkdir(destination, { recursive: true });
await Promise.all([
  copyFile(join(source, "controller.html"), join(destination, "controller.html")),
  copyFile(join(source, "controller.css"), join(destination, "controller.css")),
]);
