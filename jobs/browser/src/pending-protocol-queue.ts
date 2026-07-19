const MAX_PENDING_PROTOCOL_URLS = 32;
const MAX_PROTOCOL_URL_LENGTH = 8_192;
const PROTOCOL_PREFIX = "bluey-jobs://";

export class PendingProtocolQueue {
  private readonly urls: string[] = [];

  push(url: string): boolean {
    if (typeof url !== "string"
      || url.length === 0
      || url.length > MAX_PROTOCOL_URL_LENGTH
      || !url.startsWith(PROTOCOL_PREFIX)) return false;
    if (this.urls.includes(url)) return true;
    if (this.urls.length === MAX_PENDING_PROTOCOL_URLS) this.urls.shift();
    this.urls.push(url);
    return true;
  }

  drain(): string[] {
    return this.urls.splice(0);
  }

  get size(): number {
    return this.urls.length;
  }
}

export function enqueueProtocolArguments(
  queue: PendingProtocolQueue,
  argv: readonly string[],
): void {
  for (const argument of argv) queue.push(argument);
}

export const PENDING_PROTOCOL_CAP = MAX_PENDING_PROTOCOL_URLS;
