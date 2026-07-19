export interface CloseableBrowserContext {
  close(): Promise<unknown>;
}

export async function closeBrowserContexts(
  contexts: Iterable<CloseableBrowserContext>,
): Promise<void> {
  await Promise.all([...contexts].map((context) => context.close().catch(() => undefined)));
}
