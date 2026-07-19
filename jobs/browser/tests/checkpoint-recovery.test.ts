import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { reconcileLocalRunCheckpoints } from "../src/checkpoint-recovery.js";
import { acquireFinalSubmitAuthority } from "../src/irreversible-submit.js";
import { LocalCheckpointStore, type LocalRunCheckpoint } from "../src/local-checkpoint-store.js";
import { localRunDirectory, type StartRunRequest } from "../src/local-run-contracts.js";

const temporaryDirectories: string[] = [];

afterEach(async () => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, { recursive: true, force: true })));
});

describe("local checkpoint reconciliation", () => {
  it("reports an interrupted final submit as unknown and never restores or retries it", async () => {
    const root = await temporaryDirectory();
    const store = await LocalCheckpointStore.open(root);
    const checkpoint = fixture();
    const runDirectory = localRunDirectory(root, checkpoint.request);
    await mkdir(runDirectory, { recursive: true });
    await acquireFinalSubmitAuthority(runDirectory);
    await store.write(checkpoint);
    const unknown = vi.fn();
    const recovered = vi.fn();
    const contextFor = vi.fn();
    const fetchMock = vi.fn(async () => new Response(null, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    await reconcileLocalRunCheckpoints({
      store,
      userDataDirectory: root,
      activeRuns: new Map(),
      contextFor,
      onRecovered: recovered,
      onUnknown: unknown,
    });

    expect(contextFor).not.toHaveBeenCalled();
    expect(recovered).not.toHaveBeenCalled();
    expect(unknown).toHaveBeenCalledOnce();
    expect(fetchMock).toHaveBeenCalledOnce();
    const persisted = await store.read<StartRunRequest>(store.scopeFor(checkpoint.request));
    expect(persisted?.phase).toBe("side_effect_unknown");
    expect(persisted?.workflow.status).toBe("side_effect_unknown");
  });
});

function fixture(): LocalRunCheckpoint<StartRunRequest> {
  const expiresAtMs = Date.now() + 60_000;
  const request: StartRunRequest = {
    accountId: "account-123",
    applicationIdentityId: "identity-123",
    runId: "run-123",
    applicationId: "application-123",
    browserProfileId: "profile-123",
    url: "https://jobs.example.test/apply",
    packet: {
      applicationId: "application-123",
      jobId: "job-123",
      resumeVersionId: "resume-123",
      approvedPacketChecksum: "checksum-123",
      answers: {},
      verifiedClaimIds: [],
      applicationIdentityId: "identity-123",
    },
  };
  return {
    version: 1,
    phase: "final_submit_started",
    createdAtMs: Date.now() - 1_000,
    updatedAtMs: Date.now(),
    expiresAtMs,
    request,
    delivery: {
      apiOrigin: "https://bluey.example.test",
      capabilities: {
        result: capability("result", expiresAtMs),
        resume: capability("resume", expiresAtMs),
        expiresAtMs,
        runId: "run-123",
      },
    },
    browser: { url: request.url },
    workflow: { status: "side_effect_unknown" },
    events: [],
  };
}

function capability(operation: "result" | "resume", expiresAtMs: number): string {
  const claims = {
    version: 1,
    audience: "bluey-jobs-local-run",
    account_id: "account-123",
    application_id: "application-123",
    run_id: "run-123",
    browser_profile_id: "profile-123",
    operation,
    expires_at_ms: expiresAtMs,
    nonce: `${operation}-`.padEnd(32, "n"),
  };
  return `${Buffer.from(JSON.stringify(claims)).toString("base64url")}.${"a".repeat(64)}`;
}

async function temporaryDirectory(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), "bluey-browser-recovery-"));
  temporaryDirectories.push(path);
  return path;
}
