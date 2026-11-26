import { afterEach, describe, expect, it, vi } from "vitest";
import {
  handleLocalProtocol,
  localClaimNonce,
  type ProtocolActiveRun,
  type LocalProtocolDependencies,
} from "../src/local-protocol-handler.js";
import {
  localRunCapabilityFixture,
  localRunReleaseFixture,
} from "./fixtures/local-run-capability.js";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("local protocol controller", () => {
  it("opens the controller without network access for the non-sensitive open command", async () => {
    const dependencies = fixture();
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    await handleLocalProtocol("bluey-jobs://open", dependencies);
    expect(dependencies.showController).toHaveBeenCalledOnce();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("does not consume a claim ticket while admission is paused", async () => {
    const dependencies = fixture({
      admissionDecision: () => ({ allowed: false as const, reason: "paused" as const }),
    });
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    await handleLocalProtocol(
      `bluey-jobs://run/run-123?ticket=${"a".repeat(64)}`,
      dependencies,
    );
    expect(dependencies.showPaused).toHaveBeenCalledWith("manual");
    expect(fetchMock).not.toHaveBeenCalled();
    expect(dependencies.execute).not.toHaveBeenCalled();
  });

  it("does not consume a claim ticket without packaged release authority", async () => {
    const dependencies = fixture({ buildProof: undefined });
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    await handleLocalProtocol(
      `bluey-jobs://run/run-123?ticket=${"a".repeat(64)}`,
      dependencies,
    );
    expect(dependencies.showPaused).toHaveBeenCalledWith("unavailable");
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("joins concurrent identical run links around one claim and one execution", async () => {
    const dependencies = fixture();
    const gate = deferred<void>();
    const fetchMock = vi.fn(async () => {
      await gate.promise;
      return Response.json(claimResponse());
    });
    vi.stubGlobal("fetch", fetchMock);
    const url = `bluey-jobs://run/run-123?ticket=${"a".repeat(64)}`;
    const first = handleLocalProtocol(url, dependencies);
    const duplicate = handleLocalProtocol(url, dependencies);
    await Promise.resolve();

    expect(fetchMock).toHaveBeenCalledOnce();
    gate.resolve();
    await Promise.all([first, duplicate]);
    expect(dependencies.execute).toHaveBeenCalledOnce();

    await handleLocalProtocol(url, dependencies);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(dependencies.execute).toHaveBeenCalledTimes(2);
  });

  it("claims with the exact build proof and a deterministic replay nonce", async () => {
    const dependencies = fixture();
    const fetchMock = vi.fn(async () => Response.json(claimResponse()));
    vi.stubGlobal("fetch", fetchMock);
    const ticket = "a".repeat(64);
    await handleLocalProtocol(`bluey-jobs://run/run-123?ticket=${ticket}`, dependencies);

    expect(fetchMock).toHaveBeenCalledOnce();
    const init = fetchMock.mock.calls[0]?.[1] as RequestInit;
    expect(JSON.parse(String(init.body))).toEqual({
      ticket,
      claimNonce: localClaimNonce("run-123", ticket, buildProof()),
      buildProof: buildProof(),
    });
  });

  it("pauses safely when account, plan, or identity authority is unavailable", async () => {
    const dependencies = fixture();
    vi.stubGlobal("fetch", vi.fn(async () => new Response(null, { status: 403 })));
    await handleLocalProtocol(
      `bluey-jobs://run/run-123?ticket=${"a".repeat(64)}`,
      dependencies,
    );
    expect(dependencies.showPaused).toHaveBeenCalledWith("unavailable");
    expect(dependencies.execute).not.toHaveBeenCalled();
  });

  it("pauses without executing when the server requires another Browser release", async () => {
    const dependencies = fixture();
    vi.stubGlobal("fetch", vi.fn(async () => new Response(null, { status: 426 })));
    await handleLocalProtocol(
      `bluey-jobs://run/run-123?ticket=${"a".repeat(64)}`,
      dependencies,
    );
    expect(dependencies.showPaused).toHaveBeenCalledWith("unavailable");
    expect(dependencies.execute).not.toHaveBeenCalled();
  });

  it("fails closed for a resume command that is not bound to the active run", async () => {
    const dependencies = fixture();
    const capability = resumeCapability();
    await handleLocalProtocol(
      `bluey-jobs://resume/run-123?capability=${encodeURIComponent(capability)}`,
      dependencies,
    );
    expect(dependencies.handleUnexpected).toHaveBeenCalledOnce();
    expect(dependencies.execute).not.toHaveBeenCalled();
  });

  it("does not resume an active run while the device admission fence is closed", async () => {
    const capability = resumeCapability();
    const active: ProtocolActiveRun = {
      request: {} as ProtocolActiveRun["request"],
      delivery: {
        apiOrigin: "https://bluey.sh",
        capabilities: { resume: capability } as ProtocolActiveRun["delivery"]["capabilities"],
      },
    };
    const dependencies = fixture({
      admissionDecision: () => ({
        allowed: false as const,
        reason: "device_unavailable" as const,
      }),
      activeRun: vi.fn(() => active),
    });
    await handleLocalProtocol(
      `bluey-jobs://resume/run-123?capability=${encodeURIComponent(capability)}`,
      dependencies,
    );
    expect(dependencies.showPaused).toHaveBeenCalledWith("device_unavailable", active);
    expect(dependencies.showResuming).not.toHaveBeenCalled();
    expect(dependencies.execute).not.toHaveBeenCalled();
  });
});

function fixture(overrides: Partial<LocalProtocolDependencies> = {}): LocalProtocolDependencies {
  return {
    buildProof: buildProof(),
    showController: vi.fn(),
    isOnline: () => true,
    admissionDecision: () => ({ allowed: true }),
    showPaused: vi.fn(),
    showPreparing: vi.fn(),
    showResuming: vi.fn(),
    activeRun: vi.fn(),
    execute: vi.fn(async () => undefined),
    handleExecutionFailure: vi.fn(async () => undefined),
    handleUnexpected: vi.fn(),
    ...overrides,
  };
}

function buildProof() {
  return {
    descriptor: Buffer.from("signed descriptor bytes").toString("base64url"),
    signature: "a".repeat(86),
  };
}

function resumeCapability(): string {
  return localRunCapabilityFixture("resume", Date.now() + 60_000);
}

function claimResponse(): Record<string, unknown> {
  const expiresAtMs = Date.now() + 60_000;
  return {
    runId: "run-123",
    accountId: "account-123",
    applicationId: "application-123",
    browserProfileId: "profile-123",
    applicationIdentityId: "identity-123",
    url: "https://jobs.example.test/apply",
    packet: { applicationId: "application-123" },
    _blueyRelease: localRunReleaseFixture(),
    _blueyCapabilities: {
      result: capability("result", expiresAtMs),
      resume: capability("resume", expiresAtMs),
      submit: capability("submit", expiresAtMs),
      expiresAtMs,
    },
  };
}

function capability(operation: "result" | "resume" | "submit", expiresAtMs: number): string {
  return localRunCapabilityFixture(operation, expiresAtMs);
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve(value?: T | PromiseLike<T>): void;
} {
  let resolve!: (value?: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}
