import type { CDPSession, Page } from "playwright";

// This fence intentionally rejects every submit redirect. A response pause is
// required in addition to the existing request/body and response-status checks.
export class ChromiumSubmitResponseFenceError extends Error {
  constructor() {
    super("The application submit response could not be safely verified.");
    this.name = "ChromiumSubmitResponseFenceError";
  }
}

export class ChromiumSubmitResponseCleanupError extends Error {
  constructor() {
    super("Browser submit cleanup could not be confirmed.");
    this.name = "ChromiumSubmitResponseCleanupError";
  }
}

type Decision = { status: number } | { error: true };
type PausedResponse = {
  requestId: string;
  request: { url: string; method: string };
  frameId: string;
  resourceType: string;
  responseStatusCode?: number;
};

export class ChromiumSubmitResponseFence {
  private decision?: Decision;
  private resolveDecision!: (decision: Decision) => void;
  private readonly decided = new Promise<Decision>((resolve) => { this.resolveDecision = resolve; });
  private seen = false;
  private closed = false;
  private closePromise?: Promise<void>;

  private constructor(
    private readonly page: Page,
    private readonly expectedUrl: string,
    private readonly onFailure: () => void,
  ) {}

  static async install(
    page: Page,
    expectedUrl: string,
    onFailure: () => void,
  ): Promise<ChromiumSubmitResponseFence> {
    const fence = new ChromiumSubmitResponseFence(page, expectedUrl, onFailure);
    // Never disable/detach an ambiguously paused session: closing the page is
    // the conservative teardown if setup fails or any later decision fails.
    try {
      const session = await boundedSetup(page.context().newCDPSession(page));
      const { frameTree } = await boundedSetup(session.send("Page.getFrameTree"));
      const frameId = frameTree.frame.id;
      if (!frameId) throw new ChromiumSubmitResponseFenceError();
      session.on("Fetch.requestPaused", (event) => {
        void fence.handlePaused(session, frameId, event).catch(() => fence.fail());
      });
      session.on("close", () => {
        fence.fail();
        if (!fence.closed) void fence.closePage().catch(() => fence.fail());
      });
      page.once("close", () => { fence.closed = true; fence.fail(); });
      await boundedSetup(session.send("Fetch.enable", {
        patterns: [{ urlPattern: "*", resourceType: "Document", requestStage: "Response" }],
      }));
      if (fence.closed || fence.decision) throw new ChromiumSubmitResponseFenceError();
      return fence;
    } catch {
      fence.fail();
      await fence.closePage();
      throw new ChromiumSubmitResponseFenceError();
    }
  }

  async waitForDecision(): Promise<number> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeout = new Promise<Decision>((resolve) => {
      timer = setTimeout(() => resolve({ error: true }), 20_000);
    });
    const decision = await Promise.race([this.decided, timeout]);
    if (timer) clearTimeout(timer);
    if ("error" in decision || this.closed || this.decision !== decision) {
      this.fail();
      await this.closePage();
      throw new ChromiumSubmitResponseFenceError();
    }
    return decision.status;
  }

  async abort(): Promise<void> {
    this.fail();
    await this.closePage();
  }

  private async closePage(): Promise<void> {
    if (this.closePromise) return this.closePromise;
    if (this.closed) return;
    this.closePromise = boundedSetup(this.page.close())
      .then(() => {
        if (!this.page.isClosed()) throw new ChromiumSubmitResponseCleanupError();
      })
      .catch(() => { throw new ChromiumSubmitResponseCleanupError(); });
    return this.closePromise;
  }

  private fail(): void {
    try { this.onFailure(); } catch { /* Failure remains latched. */ }
    const wasPending = !this.decision;
    this.decision = { error: true };
    if (wasPending) this.resolveDecision(this.decision);
  }

  private async handlePaused(
    session: CDPSession,
    frameId: string,
    event: PausedResponse,
  ): Promise<void> {
    try {
      if (this.seen || this.closed || this.decision
        || event.request.url !== this.expectedUrl || event.request.method !== "POST"
        || event.resourceType !== "Document" || event.frameId !== frameId
        || !Number.isInteger(event.responseStatusCode)
        || event.responseStatusCode! < 100 || event.responseStatusCode! > 599) {
        this.fail();
        await boundedSetup(session.send("Fetch.failRequest", {
          requestId: event.requestId, errorReason: "BlockedByClient",
        }));
        await this.closePage();
        return;
      }
      this.seen = true;
      const status = event.responseStatusCode!;
      if (status >= 300 && status < 400) {
        this.fail();
        await boundedSetup(session.send("Fetch.failRequest", {
          requestId: event.requestId, errorReason: "BlockedByClient",
        }));
        return;
      }
      await boundedSetup(session.send("Fetch.continueResponse", { requestId: event.requestId }));
      if (this.closed || this.decision) {
        this.fail();
        await this.closePage();
        return;
      }
      this.decision = { status };
      this.resolveDecision(this.decision);
    } catch {
      this.fail();
      await this.closePage();
    }
  }
}

async function boundedSetup<T>(operation: Promise<T>): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      operation,
      new Promise<never>((_resolve, reject) => {
        timer = setTimeout(() => reject(new ChromiumSubmitResponseFenceError()), 5_000);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}
