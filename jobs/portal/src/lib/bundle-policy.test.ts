import { describe, expect, it } from "vitest";

import { DOCUMENT_SYMBOL_CHUNK, portalManualChunk } from "../../bundle-policy";

describe("portalManualChunk", () => {
  it.each([
    "/workspace/node_modules/dingbat-to-unicode/dist/index.js",
    "/workspace/node_modules/dingbat-to-unicode/dist/dingbats.js",
    "\0/workspace/node_modules/dingbat-to-unicode/dist/index.js?commonjs-proxy",
    "\0/workspace/node_modules/dingbat-to-unicode/dist/dingbats.js?commonjs-module"
  ])("assigns dingbat package module %s to the document-symbol chunk", (id) => {
    expect(portalManualChunk(id)).toBe(DOCUMENT_SYMBOL_CHUNK);
  });

  it("normalizes Windows module separators before matching", () => {
    const id =
      "\0C:\\workspace\\node_modules\\dingbat-to-unicode\\dist\\dingbats.js?commonjs-proxy";

    expect(portalManualChunk(id)).toBe(DOCUMENT_SYMBOL_CHUNK);
  });

  it.each([
    "/workspace/src/dingbat-to-unicode/dist/dingbats.js",
    "/workspace/node_modules/dingbat-to-unicode-extra/dist/dingbats.js",
    "\0commonjsHelpers.js",
    "/workspace/node_modules/mammoth/lib/index.js",
    "/workspace/node_modules/jszip/lib/index.js"
  ])("leaves unrelated module %s under Rollup's default chunk ownership", (id) => {
    expect(portalManualChunk(id)).toBeUndefined();
  });
});
