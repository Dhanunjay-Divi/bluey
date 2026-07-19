import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const browserRoot = join(import.meta.dirname, "..");

describe("sandboxed controller renderer", () => {
  it("uses a strict local-only CSP and no inline or remote executable content", async () => {
    const html = await readFile(join(browserRoot, "src", "renderer", "controller.html"), "utf8");
    expect(html).toContain("default-src 'none'");
    expect(html).toContain("script-src 'self'");
    expect(html).toContain("connect-src 'none'");
    expect(html).not.toMatch(/unsafe-inline|unsafe-eval|https?:\/\//i);
    expect(html).not.toMatch(/<script(?![^>]*\bsrc=)/i);
  });

  it("renders untrusted labels as text and keeps IPC action-only", async () => {
    const [renderer, preload, windowSource, shellSource, tsconfig] = await Promise.all([
      readFile(join(browserRoot, "src", "renderer", "controller.ts"), "utf8"),
      readFile(join(browserRoot, "src", "controller-preload.cts"), "utf8"),
      readFile(join(browserRoot, "src", "controller-window.ts"), "utf8"),
      readFile(join(browserRoot, "src", "browser-shell.ts"), "utf8"),
      readFile(join(browserRoot, "tsconfig.json"), "utf8"),
    ]);
    expect(renderer).toContain("textContent");
    expect(renderer).not.toContain("innerHTML");
    expect(preload).not.toMatch(/shell|openExternal|executeJavaScript|applicationEmail|resumePath|answers/);
    expect(windowSource).toContain("contextIsolation: true");
    expect(windowSource).toContain("nodeIntegration: false");
    expect(windowSource).toContain("sandbox: true");
    expect(windowSource).toContain("if (this.showOnReady) this.show()");
    expect(shellSource).toContain("shouldShowControllerOnReady({");
    expect(JSON.parse(tsconfig).include).toContain("src/**/*.cts");
  });
});
