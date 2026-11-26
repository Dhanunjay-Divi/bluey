export const DOCUMENT_SYMBOL_CHUNK = "document-symbols";

const DINGBAT_PACKAGE_PATH = /(?:^|\/)node_modules\/dingbat-to-unicode(?=\/|[?#]|$)/;

/**
 * Keep Mammoth's large, immutable dingbat lookup table in its own deferred chunk.
 * Rollup also calls this function for virtual CommonJS proxy IDs, so normalize the
 * separators before applying the package-boundary match.
 */
export function portalManualChunk(id: string): string | undefined {
  const normalizedId = id.replace(/\\/g, "/");

  if (DINGBAT_PACKAGE_PATH.test(normalizedId)) {
    return DOCUMENT_SYMBOL_CHUNK;
  }

  return undefined;
}
