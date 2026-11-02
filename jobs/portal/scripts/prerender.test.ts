import { mkdtemp, mkdir, readFile, realpath, rename, rm, stat, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, expect, it, vi } from "vitest";
import { build } from "vite";
import { renderPublicEntry } from "./prerender-entry";
import {
  HTML_LIMIT,
  RENDERER_BUILTINS,
  ROOT_TEMPLATE,
  insertPublicEntry,
  prerenderFile,
  readRegular,
  renderPublicEntry as renderBundledPublicEntry,
  validatePublicAssets,
  validateRendererBundle,
  writePrerenderedHtml,
} from "./prerender.mjs";

vi.mock("vite", () => ({ build: vi.fn() }));

const template = `<!doctype html><html><body>${ROOT_TEMPLATE}</body></html>`;
const assetTags = '<link rel="icon" href="/assets/favicon-512.png">'
  + '<link rel="stylesheet" href="/jobs/assets/entry-abc.css">'
  + '<script type="module" src="/jobs/assets/entry-abc.js"></script>';

async function withAssets(action: (web: string, output: string) => Promise<void>) {
  const root = await mkdtemp(path.join(tmpdir(), "bluey-public-prerender-test-"));
  try {
    const web = path.join(root, "web");
    const output = path.join(web, "jobs");
    await mkdir(path.join(output, "assets"), { recursive: true });
    await mkdir(path.join(web, "assets"));
    await writeFile(path.join(output, "assets/entry-abc.js"), "export {};");
    await writeFile(path.join(output, "assets/entry-abc.css"), "body {}");
    await writeFile(path.join(web, "assets/favicon-512.png"), "fixture-image");
    await action(await realpath(web), await realpath(output));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

describe("public AuthGate prerender", () => {
  it("renders no-JavaScript hero, plans, caveats and ordinary working anchors from AuthGate", () => {
    const html = renderPublicEntry();
    expect(html).toContain("Every application,");
    expect(html).toContain("already tailored.");
    expect(html).toContain('id="jobs-plans"');
    expect(html).toContain("Public-beta admission opens the Jobs workspace, not cloud automation.");
    expect(html).toContain("Cloud automation appears only after it is enabled for your account.");
    expect(html).toContain('href="/login?next=%2Fjobs"');
    expect(html).toContain('href="/login?mode=signup&amp;next=%2Fjobs"');
    expect(html).toContain('href="#how-it-works"');
    expect(html).toContain('href="#jobs-plans"');
    expect(html).not.toMatch(/<script|bluey_access_token|taylor@example|browser-final-review|\/api\//);
  });

  it("produces deterministic markup and preserves the template metadata and client tags", () => {
    const source = `<head><title>Bluey Jobs</title>${assetTags}</head>${template}`;
    const first = insertPublicEntry(source, renderPublicEntry());
    expect(first).toBe(insertPublicEntry(source, renderPublicEntry()));
    expect(first).toContain('<div id="root" data-bluey-jobs-prerendered="true">');
    expect(first).toContain('<main class="jobs-entry">');
    expect(first).toContain(assetTags);
    expect(first).toContain("<title>Bluey Jobs</title>");
    expect(first).not.toContain("<!--bluey-jobs-public-entry-->");
  });

  it("keeps exactly the marker-only root in the development HTML template", async () => {
    const source = await readFile(new URL("../index.html", import.meta.url), "utf8");
    expect(source.split(ROOT_TEMPLATE)).toHaveLength(2);
    expect(source).not.toContain('data-bluey-jobs-prerendered="true"');
  });

  it("rejects missing, duplicate, already-filled and additional root identities", () => {
    for (const source of ["<div></div>", ROOT_TEMPLATE + ROOT_TEMPLATE,
      '<div id="root">old content</div>', `${ROOT_TEMPLATE}<div id='root'></div>`]) {
      expect(() => insertPublicEntry(source, renderPublicEntry())).toThrow("missing or ambiguous root");
    }
  });

  it("rejects script-bearing or nonlanding markup and oversized HTML", () => {
    expect(() => insertPublicEntry(template, '<main class="jobs-entry"><script></script></main>'))
      .toThrow("invalid public markup");
    expect(() => insertPublicEntry(template, "private account markup")).toThrow("invalid public markup");
    expect(() => insertPublicEntry("x".repeat(HTML_LIMIT + 1), renderPublicEntry())).toThrow("HTML too large");
    expect(() => insertPublicEntry(template, `<main class="jobs-entry">${"x".repeat(128 * 1024)}</main>`))
      .toThrow("invalid public markup");
  });

  it("accepts exact built client resources, shared favicon and bounded inline brand SVGs", async () => {
    await withAssets(async (web, output) => {
      const image = "data:image/svg+xml;base64,PHN2Zy8+";
      await expect(validatePublicAssets(`${assetTags}<link rel="preload" as="image" href="${image}"/>`
        + `<img src="${image}">`,
        output, web)).resolves.toBeUndefined();
    });
  });

  it("rejects inline SVG URLs on links other than exact image preloads", async () => {
    await withAssets(async (web, output) => {
      const image = "data:image/svg+xml;base64,PHN2Zy8+";
      for (const attributes of ['rel="stylesheet" as="image"', 'rel="preload" as="script"',
        'rel="preload"', 'rel="icon" as="image"', 'rel="preload" data-as="image"',
        'rel="preload" rel="stylesheet" as="image"']) {
        await expect(validatePublicAssets(`${assetTags}<link ${attributes} href="${image}">`, output, web))
          .rejects.toThrow("invalid resource reference");
      }
    });
  });

  it("rejects missing local assets", async () => {
    await withAssets(async (web, output) => {
      await expect(validatePublicAssets(assetTags.replace("entry-abc.js", "missing.js"), output, web))
        .rejects.toThrow();
    });
  });

  it("rejects remote, escaped, traversal and source-tree resource references", async () => {
    await withAssets(async (web, output) => {
      for (const reference of ["https://example.invalid/a.js", "/jobs/assets/../entry.js",
        "/jobs/assets/../assets/entry-abc.js", "/jobs/assets/./entry-abc.js",
        "/jobs/assets//entry-abc.js", "/jobs/assets/%2e%2e/a.js", "/src/main.tsx", "//example.invalid/a.js"]) {
        await expect(validatePublicAssets(assetTags.replace("/jobs/assets/entry-abc.js", reference),
          output, web)).rejects.toThrow();
      }
    });
  });

  it("rejects symlinked resources", async () => {
    await withAssets(async (web, output) => {
      await symlink(path.join(output, "assets/entry-abc.js"), path.join(output, "assets/link.js"));
      await expect(validatePublicAssets(assetTags.replace("entry-abc.js", "link.js"), output, web))
        .rejects.toThrow("noncanonical resource");
    });
  });

  it("rejects absent entry or CSS, ambiguous resource lists and oversized inline images", async () => {
    await withAssets(async (web, output) => {
      for (const html of ["", '<script src="/jobs/assets/entry-abc.js"></script>',
        `${assetTags}<img srcset="anything">`, `${assetTags}<script></script>`,
        `${assetTags}<img src="data:image/svg+xml;base64,${"A".repeat(32769)}">`]) {
        await expect(validatePublicAssets(html, output, web)).rejects.toThrow();
      }
    });
  });

  it("writes only the original HTML inode and reads back exact fsynced output", async () => {
    await withAssets(async (_web, output) => {
      const filename = path.join(output, "index.html");
      await writeFile(filename, template);
      const snapshot = await readRegular(filename, HTML_LIMIT);
      const html = insertPublicEntry(template, renderPublicEntry());
      await writePrerenderedHtml(filename, snapshot, html);
      expect((await stat(filename)).ino).toBe(snapshot.identity.ino);
      expect(await readFile(filename, "utf8")).toBe(html);
    });
  });

  it("rejects an equivalent-byte replacement after the HTML snapshot without writing it", async () => {
    await withAssets(async (_web, output) => {
      const filename = path.join(output, "index.html");
      await writeFile(filename, template);
      const snapshot = await readRegular(filename, HTML_LIMIT);
      await rename(filename, path.join(output, "original.html"));
      await writeFile(filename, template);
      await expect(writePrerenderedHtml(filename, snapshot, insertPublicEntry(template, renderPublicEntry())))
        .rejects.toThrow("client HTML changed");
      expect(await readFile(filename, "utf8")).toBe(template);
    });
  });
});

function bundle(code: string, imports: string[] = [...RENDERER_BUILTINS]) {
  return { output: [{ type: "chunk", isEntry: true, dynamicImports: [], imports, code }] };
}

describe("bounded in-memory public renderer", () => {
  it("admits only one bounded entry with the pinned canonical Node SSR builtin set", () => {
    expect(RENDERER_BUILTINS).toEqual(["node:async_hooks", "node:crypto", "node:stream", "node:util"]);
    expect(validateRendererBundle(bundle("export {};", [...RENDERER_BUILTINS]))).toBe("export {};");
    expect(validateRendererBundle(bundle("export {};", [...RENDERER_BUILTINS].reverse()))).toBe("export {};");
    for (const imports of [[], RENDERER_BUILTINS.slice(1),
      [...RENDERER_BUILTINS, RENDERER_BUILTINS[0]]]) {
      expect(() => validateRendererBundle(bundle("export {};", imports))).toThrow("unapproved imports");
    }
  });

  it("rejects filesystem, network, process, module-loader and VM builtins or package externals", () => {
    for (const name of ["node:fs", "node:fs/promises", "node:net", "node:child_process", "node:module",
      "node:vm", "react", "stream", "/private/unknown-module.mjs"]) {
      expect(() => validateRendererBundle(bundle("export {};", [name]))).toThrow("unapproved imports");
    }
    expect(() => validateRendererBundle(bundle("export {};", ["/private/unknown-module.mjs"])))
      .toThrow("builtin names=");
  });

  it("rejects dynamic imports, additional chunks, nonentry assets and oversized renderer bytes", () => {
    const dynamic = bundle("export {};");
    Object.assign(dynamic.output[0], { dynamicImports: ["chunk.mjs"] });
    for (const output of [dynamic, { output: [...bundle("export {};").output, ...bundle("export {};").output] },
      { output: [{ type: "asset" }] }, bundle("x".repeat(4 * 1024 * 1024 + 1))]) {
      expect(() => validateRendererBundle(output)).toThrow();
    }
  });

  it("imports the bundled renderer in memory without writing any temporary module", async () => {
    vi.mocked(build).mockResolvedValueOnce(bundle(
      'export function renderPublicEntry() { return "synthetic-public-markup"; }',
    ) as Awaited<ReturnType<typeof build>>);
    await expect(renderBundledPublicEntry()).resolves.toBe("synthetic-public-markup");
    const configuration = vi.mocked(build).mock.calls.at(-1)?.[0];
    expect(configuration).toMatchObject({ configFile: false, envFile: false, ssr: { noExternal: true },
      build: { write: false, sourcemap: false } });
  });

  it("propagates a build failure before producing public markup or changing generated HTML", async () => {
    await withAssets(async (web, output) => {
      const filename = path.join(output, "index.html");
      const original = `${assetTags}${template}`;
      await writeFile(filename, original);
      vi.mocked(build).mockRejectedValueOnce(new Error("synthetic SSR build failure"));
      await expect(prerenderFile(filename, output, web)).rejects.toThrow("synthetic SSR build failure");
      expect(await readFile(filename, "utf8")).toBe(original);
    });
  });

  it("preserves generated HTML after renderer import, missing export or render failures", async () => {
    for (const code of ["not valid javascript !!!", "export const unrelated = true;",
      'export function renderPublicEntry() { throw new Error("synthetic render failure"); }']) {
      await withAssets(async (web, output) => {
        const filename = path.join(output, "index.html");
        await writeFile(filename, template);
        vi.mocked(build).mockResolvedValueOnce(bundle(code) as Awaited<ReturnType<typeof build>>);
        await expect(prerenderFile(filename, output, web)).rejects.toThrow();
        expect(await readFile(filename, "utf8")).toBe(template);
      });
    }
  });

  it("completes the synthetic in-memory renderer to generated HTML path with resolved assets", async () => {
    await withAssets(async (web, output) => {
      const filename = path.join(output, "index.html");
      const original = `${assetTags}${template}`;
      const markup = '<main class="jobs-entry">Synthetic public pipeline fixture</main>';
      await writeFile(filename, original);
      vi.mocked(build).mockResolvedValueOnce(bundle(
        `export function renderPublicEntry() { return ${JSON.stringify(markup)}; }`,
      ) as Awaited<ReturnType<typeof build>>);
      await prerenderFile(filename, output, web);
      expect(await readFile(filename, "utf8")).toBe(insertPublicEntry(original, markup));
    });
  });
});
