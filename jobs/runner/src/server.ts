import { createHash, timingSafeEqual } from "node:crypto";
import {
  createServer,
  type IncomingMessage,
  type ServerResponse,
} from "node:http";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { chromium, type BrowserContext, type Page } from "playwright";
import {
  ApprovedExecutionIntegrityError,
  CERTIFIED_BROWSER_TRANSPORT_HARDENING_ARGS,
  PlaywrightBrowserPage,
  assertApprovedExecutionChecksum,
  assertCertifiedProviderNavigationJob,
  assertMaterializedDocumentSnapshot,
  assertPublicApplicationUrl,
  createApplicationReceipt,
  createApprovedExecutionSnapshot,
  createFinalSubmitProof,
  executeApplication,
  materializeApplicationDocuments,
  restartDisposition,
  submissionPolicy,
  type ApplicationPacket,
  type EvidenceObjectUpload,
  type MaterializedDocument,
  type NormalizedJob,
  type ProviderFinalSubmitProof,
  type ReceiptDocument,
  type SubmissionReceipt,
} from "@bluey/jobs-automation";
import { installBrowserNetworkGuard } from "./browser-network-guard.js";
import {
  createBrowserProfileSnapshotClientFromEnv,
  type BrowserProfileSnapshotClient,
  type BrowserProfileSnapshotLeaseContext,
} from "./profile-snapshot-client.js";
import {
  createExecutionLeaseClientFromEnv,
  ExecutionLeaseError,
  type ActiveExecutionLease,
  type ExecutionLeaseClient,
  type ExecutionLeaseCheckpointMetadata,
  type ReconciledCheckpointPhase,
} from "./execution-lease.js";
import {
  AccountResidencyIndex,
  accountPurgeSubjectHash,
} from "./account-residency.js";
import {
  abortLeasedRun,
  beginLeasedRun,
  finalizeLeasedRun,
  LeasedRunError,
  terminalLeaseOutcome,
} from "./leased-run.js";
import { RunnerEncryptionError } from "./crypto-envelope.js";
import {
  installEncryptedProfileSnapshot,
  managedProfilePaths,
  parseProfileKey,
  profilePaths,
  profilePathsFromScope,
  readEncryptedProfileSnapshot,
  restoreProfile,
  sealProfile,
  writeProfileSnapshotGeneration,
  type ManagedProfilePaths,
  type ProfilePaths,
} from "./profile-store.js";
import {
  CURRENT_CHECKPOINT_VERSION,
  removeManagedRunCheckpoint,
  scanManagedRunCheckpoints,
  writeManagedRunCheckpoint,
  type CloudRunCheckpoint,
  type RunCheckpointScan,
} from "./run-checkpoint-store.js";
import {
  ProfileRecoveryBlockedError,
  ProfileRecoveryIsolation,
} from "./recovery-isolation.js";
import { providerRegistryForResumeAction } from "./resume-policy.js";
import {
  durableResultScope,
  readManagedResult,
  readResult,
  ResultStoreError,
  stageManagedResult,
  writeManagedResult,
} from "./result-store.js";
import {
  assertDurableResultBinding,
  isRecoverableSubmittedCheckpointPhase,
  recoverManagedSubmittedResult,
  type DurableResultBinding,
} from "./submitted-result-recovery.js";
import {
  decideRunnerInterventionResolution,
  parseRunnerInterventionResolution,
  RunnerInterventionPolicyError,
  type RunResolution,
} from "./intervention-policy.js";
import { scanLegacyRunnerStorage } from "./legacy-runner-storage.js";
import {
  openNativeRunnerStorageRoot,
  type NativeRunnerStorageRoot,
} from "./native-runner-storage.js";
import {
  bindRunnerDataRoot,
  type RunnerDataRoot,
} from "./safe-runner-storage.js";
import {
  SubjectStorageManager,
  type ManagedProfileStorage,
  type ManagedResultStorage,
} from "./subject-storage-manager.js";
import {
  createRunnerProcessInstanceId,
  loadOrCreateRunnerVolumeIdentity,
} from "./volume-identity.js";
import {
  parseRunnerVolumeServerCommandKeys,
  RunnerVolumeClient,
  RunnerVolumeClientError,
} from "./runner-volume-client.js";

interface CloudRunRequest {
  accountId: string;
  applicationIdentityId: string;
  browserProfileId: string;
  browserSessionId: string;
  runId: string;
  applicationId: string;
  url: string;
  packet: ApplicationPacket;
  job: NormalizedJob;
}

interface DurableRunResultRequest {
  accountId: string;
  applicationId: string;
  applicationIdentityId: string;
  browserSessionId: string;
  runId: string;
  requestId: string;
}

type RunEvent = {
  id: string;
  occurredAt: string;
  type: string;
  detail?: Record<string, unknown>;
};
type ExecutedRunResult = Awaited<ReturnType<typeof executeRun>>;
export interface SubmissionReceiptAuthority {
  leaseToken: string;
  fence: number;
}

type CloudRunResult = {
  accountId: string;
  applicationId: string;
  applicationIdentityId: string;
  browserSessionId: string;
  runId: string;
  receipt: SubmissionReceipt;
  receiptBundle?: ExecutedRunResult["receiptBundle"];
  evidenceObjects?: EvidenceObjectUpload[];
  receiptAuthority?: SubmissionReceiptAuthority;
  receiptPath?: string;
};

interface BrowserRunExecution {
  result: CloudRunResult;
  events: RunEvent[];
  paths: ManagedProfilePaths;
  context?: BrowserContext;
  keepActive: boolean;
}

const port = Number(process.env.PORT || 8091);
let root = "";
const serviceToken = process.env.BLUEY_JOBS_RUNNER_TOKEN || "";
const profileKey = process.env.BLUEY_JOBS_PROFILE_ENCRYPTION_KEY
  ? parseProfileKey(process.env.BLUEY_JOBS_PROFILE_ENCRYPTION_KEY)
  : undefined;
let leaseClient: ExecutionLeaseClient;
let profileSnapshotClient: BrowserProfileSnapshotClient;
let runnerVolumeClient: RunnerVolumeClient;
let accountResidency: AccountResidencyIndex;
let subjectStorage: SubjectStorageManager;
let nativeStorageRoot: NativeRunnerStorageRoot;
let runnerVolumeReady = false;
let storageAttestationQuiescing = false;
let startupStorageRecoveryComplete = false;
const locks = new Map<string, Promise<void>>();
const activeRuns = new Map<
  string,
  {
    context: BrowserContext;
    paths: ManagedProfilePaths;
    input: CloudRunRequest;
    events: RunEvent[];
    lease: ActiveExecutionLease;
    checkpointCreatedAtMs: number;
  }
>();
const activeScopes = new Set<string>();
const pendingBrowserSessions = new Map<
  string,
  { profileScope: string; references: number }
>();
const recoveryIsolation = new ProfileRecoveryIsolation();
const MAXIMUM_MANAGED_RECEIPT_ARTIFACT_BYTES = 64 * 1024 * 1024;

interface TrackedAccountWork {
  paths: ProfilePaths | ManagedProfilePaths;
  readonly lease: ActiveExecutionLease;
  readonly done: Promise<void>;
  finish(): void;
  context?: BrowserContext;
  stopping: boolean;
  pendingWrite?: {
    phase: "registering" | "writing";
    readonly done: Promise<void>;
    finish(): void;
  };
}

const trackedAccountWork = new Map<string, TrackedAccountWork>();

const runnerServer = createServer(async (request, response) => {
  try {
    if (request.url === "/healthz" && request.method === "GET") {
      return json(response, runnerVolumeReady ? 200 : 503, {
        ok: runnerVolumeReady,
      });
    }
    if (!authorized(request))
      return json(response, 401, { error: "Unauthorized" });
    runnerVolumeClient.requireReady();
    if (request.url === "/results" && request.method === "POST") {
      const completed = await recoverDurableRunResultWithResidency(
        root,
        profileKey!,
        await body<unknown>(request),
      );
      if (!completed)
        return json(response, 404, { error: "Durable result not found" });
      return json(response, 200, completed);
    }
    if (request.url === "/runs" && request.method === "POST") {
      const input = await validate(await body<CloudRunRequest>(request));
      const requestId = `${input.runId}:initial`;
      const paths = profilePaths(
        root,
        input.accountId,
        input.applicationIdentityId,
      );
      const resultContext = { requestId, profileScope: paths.scope };
      const checkpointCreatedAtMs = Date.now();
      const completed = await readMutationResult(resultContext, input);
      if (completed) return json(response, 200, completed);
      const result = await serializedProfileMutation(paths.scope, async () => {
        const existing = await readMutationResult(resultContext, input);
        if (existing) return existing;
        await requireProfileMutation(paths.scope);
        const recoveredDuringRetry = await readMutationResult(
          resultContext,
          input,
        );
        if (recoveredDuringRetry) return recoveredDuringRetry;
        const recovered = await recoverSubmittedCheckpointForRequest(
          paths.scope,
          input.browserSessionId,
          requestId,
          input,
        );
        if (recovered) return recovered;
        const { lease, execution } = await beginLeasedRun(
          leaseClient,
          {
            accountId: input.accountId,
            applicationId: input.applicationId,
            runId: input.runId,
            browserProfileId: input.browserProfileId,
          },
          (activeLease) =>
            run(input, paths, activeLease, requestId, checkpointCreatedAtMs),
          async (activeLease) => {
            const managedPaths = trackedManagedProfilePaths(activeLease);
            if (activeLease.finalSubmitAttempted) {
              if (!managedPaths) throw new ProfileRecoveryBlockedError();
              await markCloudCheckpointUnknown(
                input,
                managedPaths,
                [],
                activeLease,
                requestId,
                checkpointCreatedAtMs,
                input.url,
              ).catch(() => undefined);
            } else if (managedPaths) {
              await removeCloudRunCheckpoint(
                managedPaths,
                input.browserSessionId,
                activeLease,
              ).catch(() => undefined);
            }
            finishTrackedAccountWork(input.browserSessionId, activeLease);
            return abortLeasedRun(activeLease, async () => {});
          },
        );
        if (execution.keepActive) {
          try {
            await withRegisteredResultWrite(lease, resultContext, (storage) =>
              writeManagedResult(
                storage,
                resultContext,
                execution.result,
                profileKey!,
              ),
            );
          } catch {
            return abortLeasedRun(lease, async () => {
              await closeBrowserExecution(
                execution.context,
                execution.paths,
                input,
                lease,
              );
              await removeCloudRunCheckpoint(
                execution.paths,
                input.browserSessionId,
                lease,
              );
            });
          }
          if (!execution.context) return abortLeasedRun(lease, async () => {});
          if (trackedAccountWorkIsStopping(input.browserSessionId, lease)) {
            return abortLeasedRun(lease, async () => {
              await closeBrowserExecution(
                execution.context,
                execution.paths,
                input,
                lease,
              );
              await removeCloudRunCheckpoint(
                execution.paths,
                input.browserSessionId,
                lease,
              );
            });
          }
          activeScopes.add(execution.paths.scope);
          activeRuns.set(input.browserSessionId, {
            context: execution.context,
            paths: execution.paths,
            input,
            events: execution.events,
            lease,
            checkpointCreatedAtMs,
          });
          return execution.result;
        }

        const outcome = terminalLeaseOutcome(
          execution.result.receipt.status,
          lease,
        );
        if (outcome === "side_effect_unknown") {
          await markCloudCheckpointUnknown(
            input,
            execution.paths,
            execution.events,
            lease,
            requestId,
            checkpointCreatedAtMs,
            input.url,
          ).catch(() => undefined);
          return abortLeasedRun(lease, () =>
            closeBrowserExecution(
              execution.context,
              execution.paths,
              input,
              lease,
            ),
          );
        }
        return finalizeLeasedRun({
          lease,
          intendedOutcome: outcome,
          cleanup: () =>
            closeBrowserExecution(
              execution.context,
              execution.paths,
              input,
              lease,
              false,
            ),
          async stage() {
            await withRegisteredResultWrite(lease, resultContext, (storage) =>
              stageManagedResult(
                storage,
                resultContext,
                execution.result,
                profileKey!,
              ),
            );
          },
          async commit() {
            await withRegisteredResultWrite(lease, resultContext, (storage) =>
              writeManagedResult(
                storage,
                resultContext,
                execution.result,
                profileKey!,
              ),
            );
            await removeCloudRunCheckpoint(
              execution.paths,
              input.browserSessionId,
              lease,
            );
            return execution.result;
          },
        }).finally(() =>
          finishTrackedAccountWork(input.browserSessionId, lease),
        );
      });
      return json(response, 200, result);
    }
    const resume = request.url?.match(
      /^\/runs\/([A-Za-z0-9_-]{3,160})\/resume$/,
    );
    if (resume && request.method === "POST") {
      const resolution = parseRunnerInterventionResolution(
        await body<unknown>(request),
      );
      if (!/^[A-Za-z0-9:_-]{3,240}$/.test(resolution.requestId || "")) {
        return json(response, 400, { error: "A valid request ID is required" });
      }
      if (!/^[a-f0-9]{40}$/.test(resolution.profileScope || "")) {
        return json(response, 400, {
          error: "A valid profile scope is required",
        });
      }
      const interventionDecision =
        decideRunnerInterventionResolution(resolution);
      if (interventionDecision.kind === "requires_reapproval") {
        return json(response, 409, {
          error: interventionDecision.reason,
          state: "needs_confirmation",
          requiresReapproval: true,
        });
      }
      const resultContext = {
        requestId: resolution.requestId,
        profileScope: resolution.profileScope,
      };
      const binding: DurableResultBinding = {
        accountId: resolution.accountId,
        applicationId: resolution.applicationId,
        applicationIdentityId: resolution.applicationIdentityId,
        browserSessionId: resume[1]!,
        runId: resolution.runId,
      };
      const expectedScope = profilePaths(
        root,
        binding.accountId,
        binding.applicationIdentityId,
      ).scope;
      if (
        expectedScope !== resolution.profileScope ||
        !requestIdMatchesRun(binding.runId, resolution.requestId, true)
      ) {
        return json(response, 400, {
          error: "The run resolution binding is invalid",
        });
      }
      const result = await serializedBrowserSessionMutation(
        resume[1]!,
        resolution.profileScope,
        async () => {
          const existing = await readMutationResult(resultContext, binding);
          if (existing) return existing;
          await requireProfileMutation(resolution.profileScope);
          const recoveredDuringRetry = await readMutationResult(
            resultContext,
            binding,
          );
          if (recoveredDuringRetry) return recoveredDuringRetry;
          const recovered = await recoverSubmittedCheckpointForRequest(
            resolution.profileScope,
            resume[1]!,
            resolution.requestId,
            binding,
          );
          if (recovered) return recovered;
          const active =
            activeRuns.get(resume[1]!) ??
            (await restoreCloudRunCheckpoint(
              resume[1]!,
              resolution.profileScope,
              binding,
            ));
          if (!active) return undefined;
          if (!activeRunMatchesBinding(active, binding)) {
            throw new ResultStoreError("result_promotion_conflict");
          }
          assertApprovedExecutionChecksum(
            active.input.packet,
            active.input.job,
          );
          try {
            const executed = await executeRun(
              active.input,
              active.paths,
              active.context,
              active.events,
              active.lease,
              false,
              resolution.action,
              resolution.requestId,
              active.checkpointCreatedAtMs,
            );
            if (
              executed.receipt.status === "needs_input" &&
              !active.lease.finalSubmitAttempted
            ) {
              await withRegisteredResultWrite(
                active.lease,
                resultContext,
                (storage) =>
                  writeManagedResult(
                    storage,
                    resultContext,
                    executed,
                    profileKey!,
                  ),
              );
              return executed;
            }

            const outcome = terminalLeaseOutcome(
              executed.receipt.status,
              active.lease,
            );
            if (outcome === "side_effect_unknown") {
              await markCloudCheckpointUnknown(
                active.input,
                active.paths,
                active.events,
                active.lease,
                resolution.requestId,
                active.checkpointCreatedAtMs,
                active.context.pages()[0]?.url() || active.input.url,
              ).catch(() => undefined);
              return abortLeasedRun(active.lease, () =>
                closeActiveRun(resume[1]!, active),
              );
            }
            return finalizeLeasedRun({
              lease: active.lease,
              intendedOutcome: outcome,
              cleanup: () => closeActiveRun(resume[1]!, active, false),
              async stage() {
                await withRegisteredResultWrite(
                  active.lease,
                  resultContext,
                  (storage) =>
                    stageManagedResult(
                      storage,
                      resultContext,
                      executed,
                      profileKey!,
                    ),
                );
              },
              async commit() {
                await withRegisteredResultWrite(
                  active.lease,
                  resultContext,
                  (storage) =>
                    writeManagedResult(
                      storage,
                      resultContext,
                      executed,
                      profileKey!,
                    ),
                );
                await removeCloudRunCheckpoint(
                  active.paths,
                  active.input.browserSessionId,
                  active.lease,
                );
                return executed;
              },
            }).finally(() =>
              finishTrackedAccountWork(resume[1]!, active.lease),
            );
          } catch (error) {
            if (error instanceof LeasedRunError) throw error;
            if (active.lease.finalSubmitAttempted) {
              await markCloudCheckpointUnknown(
                active.input,
                active.paths,
                active.events,
                active.lease,
                resolution.requestId,
                active.checkpointCreatedAtMs,
                active.context.pages()[0]?.url() || active.input.url,
              ).catch(() => undefined);
            } else {
              await removeCloudRunCheckpoint(
                active.paths,
                active.input.browserSessionId,
                active.lease,
              ).catch(() => undefined);
            }
            return abortLeasedRun(active.lease, () =>
              closeActiveRun(resume[1]!, active),
            );
          }
        },
      );
      if (!result)
        return json(response, 404, { error: "Browser run not found" });
      return json(response, 200, result);
    }
    const release = request.url?.match(/^\/runs\/([A-Za-z0-9_-]{3,160})$/);
    if (release && request.method === "DELETE") {
      const browserSessionId = release[1]!;
      const coordinated = await serializedKnownBrowserSessionMutation(
        browserSessionId,
        activeRuns.get(browserSessionId)?.paths.scope,
        async (profileScope) => {
          await requireProfileMutation(profileScope);
          const active = activeRuns.get(browserSessionId);
          if (!active) return;
          if (active.paths.scope !== profileScope) {
            throw new ResultStoreError("result_promotion_conflict");
          }
          try {
            await finalizeLeasedRun({
              lease: active.lease,
              intendedOutcome: "released",
              cleanup: () => closeActiveRun(browserSessionId, active, false),
              async stage() {},
              async commit() {
                await removeCloudRunCheckpoint(
                  active.paths,
                  active.input.browserSessionId,
                  active.lease,
                );
              },
            });
          } finally {
            finishTrackedAccountWork(browserSessionId, active.lease);
          }
        },
      );
      if (!coordinated) return json(response, 204, {});
      return json(response, 204, {});
    }
    return json(response, 404, { error: "Not found" });
  } catch (error) {
    const failure = publicRunnerFailure(error);
    console.error("Bluey Jobs runner request failed", { code: failure.code });
    return json(response, failure.status, { error: failure.message });
  }
});

if (isMainModule()) {
  void startRunner().catch(() => {
    console.error("Bluey Jobs runner startup failed", {
      code: "startup_reconciliation_failed",
    });
    process.exitCode = 1;
  });
}

async function startRunner(): Promise<void> {
  if (!serviceToken) throw new Error("BLUEY_JOBS_RUNNER_TOKEN is required");
  if (!profileKey)
    throw new Error("BLUEY_JOBS_PROFILE_ENCRYPTION_KEY is required");
  process.umask(0o077);
  const acquired = await acquireRunnerStorageRoot(runnerDataRootFromEnv());
  const dataRoot = acquired.dataRoot;
  nativeStorageRoot = acquired.nativeRoot;
  root = dataRoot.path;
  const identity = await loadOrCreateRunnerVolumeIdentity(dataRoot);
  accountResidency = new AccountResidencyIndex(dataRoot, identity);
  subjectStorage = new SubjectStorageManager(
    nativeStorageRoot,
    identity,
    accountResidency,
  );
  const legacyStorage = await scanLegacyRunnerStorage(nativeStorageRoot);
  const processInstanceId = createRunnerProcessInstanceId();
  runnerVolumeClient = new RunnerVolumeClient({
    origin: requiredRunnerEnv("BLUEY_JOBS_API_ORIGIN"),
    workerSigningKey: requiredRunnerEnv("BLUEY_JOBS_WORKER_SIGNING_KEY"),
    workerId: requiredRunnerEnv("BLUEY_JOBS_RUNNER_ID"),
    admissionGrantId: requiredRunnerEnv("BLUEY_JOBS_RUNNER_ADMISSION_GRANT_ID"),
    admissionGrantToken: requiredRunnerEnv(
      "BLUEY_JOBS_RUNNER_ADMISSION_GRANT_TOKEN",
    ),
    provider: requiredRunnerEnv("BLUEY_JOBS_RUNNER_PROVIDER"),
    providerResourceId: requiredRunnerEnv(
      "BLUEY_JOBS_RUNNER_PROVIDER_RESOURCE_ID",
    ),
    resourceFingerprint: requiredRunnerEnv(
      "BLUEY_JOBS_RUNNER_RESOURCE_FINGERPRINT",
    ),
    legacyArtifactCount: legacyStorage.legacyArtifactCount,
    runnerBuildId: requiredRunnerEnv("BLUEY_JOBS_RUNNER_BUILD_ID"),
    processInstanceId,
    identity,
    residency: accountResidency,
    subjectStorage,
    serverCommandKeys: parseRunnerVolumeServerCommandKeys(
      requiredRunnerEnv("BLUEY_JOBS_RUNNER_SERVER_COMMAND_KEYS"),
    ),
    stopAccountWork: stopAccountWorkBySubjectHash,
    quiesceForStorageAttestation:
      prepareRunnerForStorageAttestation,
    onReadinessChanged: (ready) => {
      runnerVolumeReady = ready;
      if (ready) storageAttestationQuiescing = false;
    },
    onFatalControlFailure: () => {
      runnerVolumeReady = false;
      storageAttestationQuiescing = true;
      console.error("Bluey Jobs runner volume control failed", {
        code: "runner_volume_control_failed",
      });
      runnerServer.close();
    },
  });
  await runnerVolumeClient.start({ deferControlLoop: true });
  await subjectStorage.reconcileLocators(await accountResidency.allLocators());
  const reconciledLegacyStorage =
    await scanLegacyRunnerStorage(nativeStorageRoot);
  if (reconciledLegacyStorage.unclassifiedRootPaths.length !== 0) {
    throw new Error("Runner legacy storage cutover is incomplete");
  }
  leaseClient = createExecutionLeaseClientFromEnv({
    volumeId: identity.volumeId,
    enrollmentEpoch: runnerVolumeClient.enrollmentEpoch,
    processInstanceId,
    keyFingerprint: identity.publicKeyFingerprint,
    createExecutionLeaseClaimProof: (input) =>
      runnerVolumeClient.createExecutionLeaseClaimProof(input),
  });
  profileSnapshotClient = createBrowserProfileSnapshotClientFromEnv();
  if (reconciledLegacyStorage.legacyArtifactCount === 0) {
    await completeRunnerStartupStorageRecovery();
  }
  await runnerVolumeClient.activateControlLoop();
  runnerServer.listen(port, "0.0.0.0", () => {
    console.log(`Bluey Jobs runner control endpoint listening on ${port}`);
  });
}

export async function acquireRunnerStorageRoot(
  configuredRoot: string,
  openNativeRoot: (
    configuredPath: string,
  ) => NativeRunnerStorageRoot = openNativeRunnerStorageRoot,
  bindDataRoot: (
    retainedRoot: NativeRunnerStorageRoot,
  ) => Promise<RunnerDataRoot> = bindRunnerDataRoot,
): Promise<{
  readonly nativeRoot: NativeRunnerStorageRoot;
  readonly dataRoot: RunnerDataRoot;
}> {
  const nativeRoot = openNativeRoot(configuredRoot);
  const dataRoot = await bindDataRoot(nativeRoot);
  nativeRoot.assertUnchanged();
  return Object.freeze({ nativeRoot, dataRoot });
}

async function completeRunnerStartupStorageRecovery(): Promise<void> {
  if (startupStorageRecoveryComplete) return;
  await assertManagedStorageClosedWorld();
  await reconcileManagedOrphanActiveProfiles();
  await assertManagedStorageClosedWorld();
  await restoreCloudRunCheckpoints();
  startupStorageRecoveryComplete = true;
}

async function managedProfileScopes(): Promise<readonly string[]> {
  const byScope = new Map<string, string>();
  for (const locator of await accountResidency.allLocators()) {
    if (locator.kind !== "profile") continue;
    if (await accountResidency.hasPurgeBarrier(locator.subjectSha256)) {
      continue;
    }
    const existing = byScope.get(locator.scope);
    if (existing && existing !== locator.subjectSha256) {
      throw new ProfileRecoveryBlockedError();
    }
    byScope.set(locator.scope, locator.subjectSha256);
  }
  return Object.freeze([...byScope.keys()].sort());
}

async function assertManagedStorageClosedWorld(): Promise<void> {
  const subjects = new Set<string>();
  for (const locator of await accountResidency.allLocators()) {
    subjects.add(locator.subjectSha256);
  }
  if (subjects.size === 0) subjects.add("0".repeat(64));
  for (const subjectSha256 of [...subjects].sort()) {
    const audit = await subjectStorage.auditSubject(subjectSha256);
    if (!audit.ok) throw new ProfileRecoveryBlockedError();
  }
}

async function reconcileManagedOrphanActiveProfiles(): Promise<void> {
  for (const profileScope of await managedProfileScopes()) {
    await runnerVolumeClient.withExistingProfileResidency(
      profileScope,
      async (storage) => {
        if (!storage) throw new ProfileRecoveryBlockedError();
        const active = await storage.active.inventory();
        if (active.entries.length === 0) return;
        await sealProfile(managedProfilePaths(storage), profileKey!);
      },
    );
  }
}

async function scanManagedCloudRunCheckpoints(
  profileScope: string,
): Promise<RunCheckpointScan<CloudRunRequest, RunEvent>> {
  return runnerVolumeClient.withExistingProfileResidency(
    profileScope,
    async (storage) => {
      if (!storage) {
        return {
          checkpoints: [],
          failures: [{ profileScope, code: "profile_unreadable" }],
        };
      }
      return scanManagedRunCheckpoints<CloudRunRequest, RunEvent>(
        storage,
        profileKey!,
      );
    },
  );
}

async function removeExistingCloudRunCheckpoint(
  profileScope: string,
  browserSessionId: string,
): Promise<void> {
  await runnerVolumeClient.withExistingProfileResidency(
    profileScope,
    async (storage) => {
      if (!storage) return;
      await removeManagedRunCheckpoint(storage, browserSessionId);
    },
  );
}

async function removeCloudRunCheckpoint(
  paths: ManagedProfilePaths,
  browserSessionId: string,
  lease: ActiveExecutionLease,
): Promise<void> {
  await withTrackedProfileResidency(lease, paths.scope, async (storage) => {
    assertManagedProfileBinding(paths, storage);
    await removeManagedRunCheckpoint(storage, browserSessionId);
  });
}

async function restoreCloudRunCheckpoints(): Promise<void> {
  const profileScopes = await managedProfileScopes();
  for (const profileScope of profileScopes) {
    const scan = await scanManagedCloudRunCheckpoints(profileScope);
    if (scan.failures.length > 0) {
      recoveryIsolation.block(profileScope);
      console.error("Bluey Jobs runner profile recovery deferred", {
        code: "profile_recovery_blocked",
      });
      continue;
    }
    if (scan.checkpoints.length === 0) continue;
    if (recoveryIsolation.isBlocked(profileScope)) continue;
    const recovered = await recoveryIsolation.attemptStartup(profileScope, () =>
      recoverProfileScope(profileScope),
    );
    if (!recovered) {
      console.error("Bluey Jobs runner profile recovery deferred", {
        code: "profile_recovery_blocked",
      });
    }
  }
}

async function restoreCloudRunCheckpoint(
  browserSessionId: string,
  profileScope: string,
  binding: DurableResultBinding,
): Promise<NonNullable<ReturnType<typeof activeRuns.get>> | undefined> {
  const existing = activeRuns.get(browserSessionId);
  if (existing) {
    if (!activeRunMatchesBinding(existing, binding)) {
      recoveryIsolation.rejectMutation(profileScope);
    }
    return existing;
  }
  const scan = await scanManagedCloudRunCheckpoints(profileScope);
  if (scan.failures.length > 0) recoveryIsolation.rejectMutation(profileScope);
  const found = scan.checkpoints.find(
    ({ checkpoint }) => checkpoint.browserSessionId === browserSessionId,
  )?.checkpoint;
  if (!found) return undefined;
  try {
    if (!checkpointMatchesBinding(found, binding)) {
      throw new ResultStoreError("result_promotion_conflict");
    }
    if (restartDisposition(found.phase, found.expiresAtMs) !== "restore") {
      await reconcileStoredCheckpoint(found);
      return undefined;
    }
    await restoreCheckpoint(found);
    return activeRuns.get(browserSessionId);
  } catch {
    recoveryIsolation.rejectMutation(profileScope);
  }
}

async function recoverCheckpoint(
  checkpoint: CloudRunCheckpoint<CloudRunRequest, RunEvent>,
): Promise<void> {
  const disposition = restartDisposition(
    checkpoint.phase,
    checkpoint.expiresAtMs,
  );
  if (disposition === "restore") await restoreCheckpoint(checkpoint);
  else await reconcileStoredCheckpoint(checkpoint);
}

async function recoverProfileScope(profileScope: string): Promise<void> {
  const scan = await scanManagedCloudRunCheckpoints(profileScope);
  if (scan.failures.length > 0) throw new ProfileRecoveryBlockedError();
  for (const { checkpoint } of scan.checkpoints)
    await recoverCheckpoint(checkpoint);

  const verified = await scanManagedCloudRunCheckpoints(profileScope);
  if (verified.failures.length > 0) throw new ProfileRecoveryBlockedError();
  for (const { checkpoint } of verified.checkpoints) {
    if (
      restartDisposition(checkpoint.phase, checkpoint.expiresAtMs) !==
        "restore" ||
      !activeRunMatchesCheckpoint(checkpoint)
    ) {
      throw new ProfileRecoveryBlockedError();
    }
  }
}

async function requireProfileMutation(profileScope: string): Promise<void> {
  await recoveryIsolation.requireMutation(profileScope, () =>
    recoverProfileScope(profileScope),
  );
}

async function restoreCheckpoint(
  checkpoint: CloudRunCheckpoint<CloudRunRequest, RunEvent>,
): Promise<void> {
  const approvedRequest = await validate(checkpoint.request);
  const unresolvedPaths = profilePathsFromScope(root, checkpoint.profileScope);
  if (
    profilePaths(
      root,
      approvedRequest.accountId,
      approvedRequest.applicationIdentityId,
    ).scope !== unresolvedPaths.scope
  ) {
    throw new Error("Cloud run checkpoint profile mismatch");
  }
  const existing = activeRuns.get(checkpoint.browserSessionId);
  if (existing) {
    if (!activeRunMatchesCheckpoint(checkpoint))
      throw new ProfileRecoveryBlockedError();
    return;
  }
  if (activeScopes.has(unresolvedPaths.scope))
    throw new ProfileRecoveryBlockedError();

  let lease: ActiveExecutionLease | undefined;
  let context: BrowserContext | undefined;
  let paths: ManagedProfilePaths | undefined;
  try {
    lease = await leaseClient.claim({
      accountId: approvedRequest.accountId,
      applicationId: approvedRequest.applicationId,
      runId: approvedRequest.runId,
      browserProfileId: approvedRequest.browserProfileId,
    });
    trackAccountWork(approvedRequest, unresolvedPaths, lease);
    paths = await registerTrackedProfileResidency(lease, unresolvedPaths.scope);
    await writeCloudCheckpoint({
      input: approvedRequest,
      paths,
      events: checkpoint.events,
      lease,
      requestId: checkpoint.workflow.requestId,
      checkpointCreatedAtMs: checkpoint.createdAtMs,
      phase: checkpoint.phase,
      status: checkpoint.workflow.status,
      browserUrl: checkpoint.browser.url,
      providerReview: checkpoint.workflow.providerReview,
    });
    await restoreDurableBrowserProfile(approvedRequest, paths, lease);
    context = await chromium.launchPersistentContext(paths.directory, {
      headless: true,
      acceptDownloads: true,
      offline: true,
      serviceWorkers: "block",
      args: [...CERTIFIED_BROWSER_TRANSPORT_HARDENING_ARGS],
      viewport: { width: 1440, height: 1000 },
    });
    await attachTrackedAccountWorkContext(
      approvedRequest.browserSessionId,
      lease,
      context,
    );
    await installBrowserNetworkGuard(context);
    const recoveryUrl = /^https?:\/\//i.test(checkpoint.browser.url)
      ? checkpoint.browser.url
      : approvedRequest.url;
    await assertPublicApplicationUrl(recoveryUrl);
    const page = await prepareRecoveredCloudPage(
      context,
      recoveryUrl,
      approvedRequest.job,
    );
    const active = {
      context,
      paths,
      input: approvedRequest,
      events: checkpoint.events,
      lease,
      checkpointCreatedAtMs: checkpoint.createdAtMs,
    };
    assertTrackedAccountWorkMayContinue(
      approvedRequest.browserSessionId,
      lease,
    );
    activeScopes.add(paths.scope);
    activeRuns.set(checkpoint.browserSessionId, active);
    await writeCloudCheckpoint({
      input: approvedRequest,
      paths,
      events: checkpoint.events,
      lease,
      requestId: checkpoint.workflow.requestId,
      checkpointCreatedAtMs: checkpoint.createdAtMs,
      phase: checkpoint.phase,
      status: checkpoint.workflow.status,
      browserUrl: page.url() || recoveryUrl,
      providerReview: checkpoint.workflow.providerReview,
    });
  } catch (error) {
    if (context && lease && paths) {
      await closeBrowserExecution(context, paths, approvedRequest, lease).catch(
        () => undefined,
      );
    }
    if (activeRuns.get(checkpoint.browserSessionId)?.lease === lease) {
      activeRuns.delete(checkpoint.browserSessionId);
      activeScopes.delete(checkpoint.profileScope);
    }
    await lease?.stopHeartbeat().catch(() => undefined);
    if (lease)
      finishTrackedAccountWork(approvedRequest.browserSessionId, lease);
    throw error;
  }
}

function activeRunMatchesCheckpoint(
  checkpoint: CloudRunCheckpoint<CloudRunRequest, RunEvent>,
): boolean {
  const active = activeRuns.get(checkpoint.browserSessionId);
  return (
    active !== undefined &&
    active.paths.scope === checkpoint.profileScope &&
    active.input.accountId === checkpoint.request.accountId &&
    active.input.applicationId === checkpoint.request.applicationId &&
    active.input.applicationIdentityId ===
      checkpoint.request.applicationIdentityId &&
    active.input.browserProfileId === checkpoint.request.browserProfileId &&
    active.input.browserSessionId === checkpoint.request.browserSessionId &&
    active.input.runId === checkpoint.request.runId
  );
}

function activeRunMatchesBinding(
  active: NonNullable<ReturnType<typeof activeRuns.get>>,
  binding: DurableResultBinding,
): boolean {
  return (
    active.input.accountId === binding.accountId &&
    active.input.applicationId === binding.applicationId &&
    active.input.applicationIdentityId === binding.applicationIdentityId &&
    active.input.browserSessionId === binding.browserSessionId &&
    active.input.runId === binding.runId
  );
}

function checkpointMatchesBinding(
  checkpoint: CloudRunCheckpoint<CloudRunRequest, RunEvent>,
  binding: DurableResultBinding,
): boolean {
  return (
    checkpoint.request.accountId === binding.accountId &&
    checkpoint.request.applicationId === binding.applicationId &&
    checkpoint.request.applicationIdentityId ===
      binding.applicationIdentityId &&
    checkpoint.request.browserSessionId === binding.browserSessionId &&
    checkpoint.request.runId === binding.runId
  );
}

async function reconcileStoredCheckpoint(
  checkpoint: CloudRunCheckpoint<CloudRunRequest, RunEvent>,
): Promise<void> {
  if (await recoverSubmittedCheckpointResult(checkpoint)) return;
  await reconcileCheckpointMetadata(
    checkpoint.request,
    {
      fence: checkpoint.lease.fence,
      expiresAtMs: checkpoint.lease.expiresAtMs,
      ownerId: checkpoint.lease.ownerId,
      leaseToken: checkpoint.lease.leaseToken ?? "",
    },
    checkpoint.version,
    checkpoint.phase,
  );
  await removeExistingCloudRunCheckpoint(
    checkpoint.profileScope,
    checkpoint.browserSessionId,
  );
}

async function recoverSubmittedCheckpointForRequest(
  profileScope: string,
  browserSessionId: string,
  requestId: string,
  binding: DurableResultBinding,
): Promise<CloudRunResult | undefined> {
  const scan = await scanManagedCloudRunCheckpoints(profileScope);
  if (scan.failures.length > 0) recoveryIsolation.rejectMutation(profileScope);
  const checkpoint = scan.checkpoints.find(
    ({ checkpoint: candidate }) =>
      candidate.profileScope === profileScope &&
      candidate.browserSessionId === browserSessionId &&
      candidate.workflow.requestId === requestId,
  )?.checkpoint;
  if (!checkpoint) return undefined;
  try {
    if (!checkpointMatchesBinding(checkpoint, binding)) {
      throw new ResultStoreError("result_promotion_conflict");
    }
    const recovered = await recoverSubmittedCheckpointResult(checkpoint);
    if (recovered) assertDurableResultBinding(recovered, binding);
    return recovered;
  } catch {
    recoveryIsolation.rejectMutation(profileScope);
  }
}

async function readMutationResult(
  resultContext: { requestId: string; profileScope: string },
  binding: DurableResultBinding,
): Promise<CloudRunResult | undefined> {
  try {
    const result = await runnerVolumeClient.withExistingResultResidency(
      localResultScope(resultContext),
      (storage) =>
        storage
          ? readManagedResult<CloudRunResult>(
              storage,
              resultContext,
              profileKey!,
            )
          : Promise.resolve(undefined),
    );
    if (result) assertMutationResultBinding(result, resultContext, binding);
    return result;
  } catch {
    recoveryIsolation.rejectMutation(resultContext.profileScope);
  }
}

function assertMutationResultBinding(
  result: CloudRunResult,
  resultContext: { requestId: string; profileScope: string },
  binding: DurableResultBinding,
): void {
  assertDurableResultBinding(result, binding);
  const validRequestId =
    resultContext.requestId === `${binding.runId}:initial` ||
    (resultContext.requestId.startsWith(`${binding.runId}:resume:`) &&
      /^:resume:[1-6]$/.test(
        resultContext.requestId.slice(binding.runId.length),
      ));
  if (
    !validRequestId ||
    profilePaths(root, binding.accountId, binding.applicationIdentityId)
      .scope !== resultContext.profileScope
  ) {
    throw new ResultStoreError("result_promotion_conflict");
  }
}

async function recoverSubmittedCheckpointResult(
  checkpoint: CloudRunCheckpoint<CloudRunRequest, RunEvent>,
): Promise<CloudRunResult | undefined> {
  const leaseToken = checkpoint.lease.leaseToken;
  if (
    checkpoint.version !== CURRENT_CHECKPOINT_VERSION ||
    !isRecoverableSubmittedCheckpointPhase(checkpoint.phase) ||
    !leaseToken
  ) {
    return undefined;
  }
  const expectedProfileScope = profilePaths(
    root,
    checkpoint.request.accountId,
    checkpoint.request.applicationIdentityId,
  ).scope;
  if (expectedProfileScope !== checkpoint.profileScope) {
    throw new ResultStoreError("result_promotion_conflict");
  }
  const resultContext = {
    requestId: checkpoint.workflow.requestId,
    profileScope: checkpoint.profileScope,
  };
  const recovered = await runnerVolumeClient.withExistingResultResidency(
    localResultScope(resultContext),
    (storage) => {
      if (!storage) return Promise.resolve(undefined);
      return recoverManagedSubmittedResult<CloudRunResult>(
        storage,
        profileKey!,
        {
          accountId: checkpoint.request.accountId,
          applicationId: checkpoint.request.applicationId,
          applicationIdentityId: checkpoint.request.applicationIdentityId,
          browserSessionId: checkpoint.request.browserSessionId,
          runId: checkpoint.request.runId,
          leaseToken,
          fence: checkpoint.lease.fence,
          resultContext,
        },
        leaseClient,
      );
    },
  );
  if (!recovered) return undefined;
  await removeExistingCloudRunCheckpoint(
    checkpoint.profileScope,
    checkpoint.browserSessionId,
  );
  return recovered;
}

async function reconcileCheckpointMetadata(
  request: CloudRunRequest,
  lease: ExecutionLeaseCheckpointMetadata,
  checkpointVersion: 1 | 2,
  checkpointPhase: ReconciledCheckpointPhase,
): Promise<void> {
  await leaseClient.reconcileCheckpoint({
    accountId: request.accountId,
    applicationId: request.applicationId,
    runId: request.runId,
    ownerId: lease.ownerId,
    fence: lease.fence,
    ...(checkpointVersion === 2 ? { leaseToken: lease.leaseToken } : {}),
    checkpointVersion,
    checkpointPhase,
  });
}

async function writeCloudCheckpoint(input: {
  input: CloudRunRequest;
  paths: ManagedProfilePaths;
  events: RunEvent[];
  lease: ActiveExecutionLease;
  requestId: string;
  checkpointCreatedAtMs: number;
  phase: CloudRunCheckpoint["phase"];
  status: CloudRunCheckpoint["workflow"]["status"];
  browserUrl: string;
  providerReview?: { adapter: string; adapterVersion?: string };
}): Promise<void> {
  const now = Date.now();
  const checkpoint: CloudRunCheckpoint<CloudRunRequest, RunEvent> = {
    version: CURRENT_CHECKPOINT_VERSION,
    phase: input.phase,
    createdAtMs: input.checkpointCreatedAtMs,
    updatedAtMs: now,
    expiresAtMs: now + 24 * 60 * 60 * 1_000,
    profileScope: input.paths.scope,
    browserSessionId: input.input.browserSessionId,
    request: input.input,
    browser: { url: input.browserUrl },
    workflow: {
      status: input.status,
      requestId: input.requestId,
      ...(input.providerReview ? { providerReview: input.providerReview } : {}),
    },
    events: input.events,
    lease: input.lease.checkpointMetadata(),
  };
  await withTrackedProfileResidency(
    input.lease,
    input.paths.scope,
    async (storage) => {
      assertManagedProfileBinding(input.paths, storage);
      await writeManagedRunCheckpoint(storage, checkpoint, profileKey!);
    },
  );
}

async function markCloudCheckpointUnknown(
  input: CloudRunRequest,
  paths: ManagedProfilePaths,
  events: RunEvent[],
  lease: ActiveExecutionLease,
  requestId: string,
  checkpointCreatedAtMs: number,
  browserUrl: string,
): Promise<void> {
  await writeCloudCheckpoint({
    input,
    paths,
    events,
    lease,
    requestId,
    checkpointCreatedAtMs,
    phase: "side_effect_unknown",
    status: "side_effect_unknown",
    browserUrl,
  });
}

async function run(
  input: CloudRunRequest,
  unresolvedPaths: ProfilePaths,
  lease: ActiveExecutionLease,
  requestId: string,
  checkpointCreatedAtMs: number,
): Promise<BrowserRunExecution> {
  const events: RunEvent[] = [];
  trackAccountWork(input, unresolvedPaths, lease);
  const paths = await registerTrackedProfileResidency(
    lease,
    unresolvedPaths.scope,
  );
  const decision = submissionPolicy(input.url);
  if (decision.policy !== "automate") {
    const receipt: SubmissionReceipt = {
      status: "needs_input",
      issues: [
        { field: "submission", message: decision.reason, severity: "blocking" },
      ],
      intervention: {
        kind: "browser_takeover",
        title: "Finish this application",
        detail: decision.reason,
        resolution: { kind: "browser_takeover", resumeAfter: false },
      },
    };
    return {
      result: { ...durableResultIdentity(input), receipt },
      events,
      paths,
      keepActive: false,
    };
  }

  if (activeScopes.has(paths.scope))
    throw new Error("This application email already has an active browser run");
  if (activeRuns.has(input.browserSessionId))
    throw new Error("This browser session is already active");
  let context: BrowserContext | undefined;
  let profileRestored = false;
  try {
    await writeCloudCheckpoint({
      input,
      paths,
      events,
      lease,
      requestId,
      checkpointCreatedAtMs,
      phase: "prepared",
      status: "prepared",
      browserUrl: input.url,
    });
    await restoreDurableBrowserProfile(input, paths, lease);
    profileRestored = true;
    context = await chromium.launchPersistentContext(paths.directory, {
      headless: true,
      acceptDownloads: true,
      offline: true,
      serviceWorkers: "block",
      args: [...CERTIFIED_BROWSER_TRANSPORT_HARDENING_ARGS],
      viewport: { width: 1440, height: 1000 },
    });
    await attachTrackedAccountWorkContext(
      input.browserSessionId,
      lease,
      context,
    );
    await installBrowserNetworkGuard(context);
    const result = await executeRun(
      input,
      paths,
      context,
      events,
      lease,
      true,
      undefined,
      requestId,
      checkpointCreatedAtMs,
    );
    return {
      result,
      events,
      paths,
      context,
      keepActive:
        result.receipt.status === "needs_input" && !lease.finalSubmitAttempted,
    };
  } catch {
    if (context) {
      await closeBrowserExecution(context, paths, input, lease, false).catch(
        () => undefined,
      );
    } else if (
      profileRestored &&
      !trackedAccountWorkIsStopping(input.browserSessionId, lease)
    ) {
      await withTrackedProfileResidency(lease, paths.scope, async (storage) => {
        assertManagedProfileBinding(paths, storage);
        await sealProfile(managedProfilePaths(storage), profileKey!);
      }).catch(() => undefined);
    }
    // beginLeasedRun's failure callback retains this tracking entry while it
    // durably records an uncertain irreversible outcome, then releases it.
    throw new Error("Browser execution failed");
  }
}

async function executeRun(
  input: CloudRunRequest,
  paths: ManagedProfilePaths,
  context: BrowserContext,
  events: RunEvent[],
  lease: ActiveExecutionLease,
  navigate: boolean,
  resumeAction?: string,
  requestId = `${input.runId}:initial`,
  checkpointCreatedAtMs = Date.now(),
) {
  assertApprovedExecutionChecksum(input.packet, input.job);
  const { documents, runDirectoryPath } = await withTrackedProfileResidency(
    lease,
    paths.scope,
    async (storage) => {
      assertManagedProfileBinding(paths, storage);
      const runDirectory = await storage.receipts.ensureChildDirectory(
        input.runId,
      );
      const documentDirectory =
        await runDirectory.ensureChildDirectory("documents");
      const documents = await materializeApplicationDocuments(
        input.packet,
        documentDirectory.canonicalPath,
      );
      return { documents, runDirectoryPath: runDirectory.canonicalPath };
    },
  );
  const runtimePacket: ApplicationPacket = {
    ...documents.packet,
    applicationIdentityId: input.applicationIdentityId,
    browserProfileId: input.browserProfileId,
  };
  const preparedPage = navigate
    ? await prepareFreshCloudPage(context, input.job)
    : await wrapActiveCloudPage(context, input.job);
  const { page, browserPage } = preparedPage;
  if (navigate)
    await page.goto(input.url, {
      waitUntil: "domcontentloaded",
      timeout: 45_000,
    });
  const adapterContext = {
    runner: "cloud",
    runId: input.runId,
    accountId: input.accountId,
    approvedCanonicalUrl: input.job.canonicalUrl,
    page: browserPage,
    packet: runtimePacket,
    async log(type: string, detail: Record<string, unknown> = {}) {
      events.push({
        id: `${input.runId}:${events.length + 1}`,
        occurredAt: new Date().toISOString(),
        type,
        detail,
      });
    },
    beforeFinalSubmit: async (providerProof: ProviderFinalSubmitProof) => {
      assertTrackedAccountWorkMayContinue(input.browserSessionId, lease);
      await withTrackedProfileResidency(lease, paths.scope, async (storage) => {
        assertManagedProfileBinding(paths, storage);
        const retainedRunDirectory = await storage.receipts.openChildDirectory(
          input.runId,
        );
        if (retainedRunDirectory.canonicalPath !== runDirectoryPath) {
          throw new ProfileRecoveryBlockedError();
        }
        await assertMaterializedDocumentSnapshot(documents.resume);
        if (documents.coverLetter) {
          await assertMaterializedDocumentSnapshot(documents.coverLetter);
        }
      });
      const finalSubmitProof = createFinalSubmitProof(
        providerProof,
        {
          resume: {
            versionId: input.packet.resumeVersionId,
            sha256: documents.resume.sha256,
          },
          ...(documents.coverLetter
            ? {
                coverLetter: { sha256: documents.coverLetter.sha256 },
              }
            : {}),
        },
        {
          approvedCanonicalUrl: input.job.canonicalUrl,
          pageUrl: browserPage.url(),
        },
      );
      await writeCloudCheckpoint({
        input,
        paths,
        events,
        lease,
        requestId,
        checkpointCreatedAtMs,
        phase: "final_submit_started",
        status: "side_effect_unknown",
        browserUrl: browserPage.url(),
      });
      await lease.beforeFinalSubmit(finalSubmitProof);
    },
    afterFinalSubmit: async (outcome: "activated" | "activation_uncertain") => {
      await lease.afterFinalSubmit(outcome);
      await writeCloudCheckpoint({
        input,
        paths,
        events,
        lease,
        requestId,
        checkpointCreatedAtMs,
        phase:
          outcome === "activated"
            ? "final_submit_activated"
            : "side_effect_unknown",
        status: "side_effect_unknown",
        browserUrl: browserPage.url(),
      });
    },
  } as const;
  const providerRegistry = providerRegistryForResumeAction(resumeAction);
  const execution = providerRegistry
    ? await executeApplication(adapterContext, providerRegistry)
    : await executeApplication(adapterContext);
  if (
    execution.receipt.status === "needs_input" &&
    !lease.finalSubmitAttempted
  ) {
    const providerReview =
      ["greenhouse", "lever"].includes(execution.adapter) &&
      execution.receipt.issues.length === 0
        ? {
            adapter: execution.adapter,
            adapterVersion: execution.adapterVersion,
          }
        : undefined;
    await writeCloudCheckpoint({
      input,
      paths,
      events,
      lease,
      requestId,
      checkpointCreatedAtMs,
      phase: providerReview ? "provider_review" : "needs_input",
      status: providerReview ? "provider_review" : "needs_input",
      browserUrl: browserPage.url(),
      providerReview,
    });
  }
  const screenshotBytes = Buffer.from(
    await browserPage.screenshot({ fullPage: true }),
  );
  const screenshotPath = await writeManagedReceiptArtifact(
    paths,
    lease,
    input.runId,
    "final.png",
    screenshotBytes,
  );
  if (execution.receipt.status === "needs_input") {
    const takeoverOrigin = process.env.BLUEY_JOBS_TAKEOVER_ORIGIN?.replace(
      /\/$/,
      "",
    );
    if (execution.receipt.intervention && takeoverOrigin) {
      execution.receipt.intervention.takeoverUrl = `${takeoverOrigin}/sessions/${encodeURIComponent(input.browserSessionId)}`;
    }
  }
  const receiptDocuments: ReceiptDocument[] = [
    {
      kind: "resume",
      versionId: input.packet.resumeVersionId,
      storageKey: documents.resume.path,
      sha256: documents.resume.sha256,
    },
  ];
  if (documents.coverLetter)
    receiptDocuments.push({
      kind: "cover_letter",
      storageKey: documents.coverLetter.path,
      sha256: documents.coverLetter.sha256,
    });
  const receipt = createApplicationReceipt({
    receiptId: `receipt-${input.runId}`,
    accountId: input.accountId,
    runId: input.runId,
    runner: "cloud",
    applicationIdentityId: input.applicationIdentityId,
    browserProfileId: input.browserProfileId,
    adapter: execution.adapter,
    adapterVersion: execution.adapterVersion,
    job: input.job,
    packet: input.packet,
    documents: receiptDocuments,
    events,
    result: execution.receipt,
    finalUrl: page.url(),
    screenshotKeys: [screenshotPath],
  });
  const receiptPath = await writeManagedReceiptArtifact(
    paths,
    lease,
    input.runId,
    "receipt.json",
    Buffer.from(`${JSON.stringify(receipt, null, 2)}\n`, "utf8"),
  );
  const evidenceObjects: EvidenceObjectUpload[] = [
    await evidenceObject(documents.resume, "resume", "application/pdf"),
    ...(documents.coverLetter
      ? [
          await evidenceObject(
            documents.coverLetter,
            "cover_letter",
            "application/pdf",
          ),
        ]
      : []),
    {
      original_key: screenshotPath,
      kind: "screenshot",
      media_type: "image/png",
      sha256: createHash("sha256").update(screenshotBytes).digest("hex"),
      bytes_base64: screenshotBytes.toString("base64"),
    },
  ];
  const receiptAuthority = submissionReceiptAuthority(
    execution.receipt.status,
    lease,
  );
  return {
    ...durableResultIdentity(input),
    receipt: execution.receipt,
    receiptBundle: receipt,
    evidenceObjects,
    ...(receiptAuthority ? { receiptAuthority } : {}),
    receiptPath,
  };
}

function durableResultIdentity(
  input: CloudRunRequest,
): Pick<
  CloudRunResult,
  | "accountId"
  | "applicationId"
  | "applicationIdentityId"
  | "browserSessionId"
  | "runId"
> {
  return {
    accountId: input.accountId,
    applicationId: input.applicationId,
    applicationIdentityId: input.applicationIdentityId,
    browserSessionId: input.browserSessionId,
    runId: input.runId,
  };
}

export function submissionReceiptAuthority(
  status: SubmissionReceipt["status"],
  lease: Pick<
    ActiveExecutionLease,
    | "checkpointMetadata"
    | "finalSubmitActivationOutcome"
    | "finalSubmitAttempted"
    | "finalSubmitAuthorized"
  >,
): SubmissionReceiptAuthority | undefined {
  if (status !== "submitted") return undefined;
  const authority = lease.checkpointMetadata();
  if (
    !lease.finalSubmitAttempted ||
    !lease.finalSubmitAuthorized ||
    !lease.finalSubmitActivationOutcome ||
    !/^[A-Za-z0-9_-]{43}$/.test(authority.leaseToken) ||
    !Number.isSafeInteger(authority.fence) ||
    authority.fence <= 0
  ) {
    throw new ExecutionLeaseError("irreversible", "invalid_state");
  }
  return { leaseToken: authority.leaseToken, fence: authority.fence };
}

async function evidenceObject(
  document: MaterializedDocument,
  kind: "resume" | "cover_letter" | "attachment",
  mediaType: string,
): Promise<EvidenceObjectUpload> {
  const bytes = Buffer.from(document.bytesBase64, "base64");
  return {
    original_key: document.path,
    kind,
    media_type: mediaType,
    sha256: createHash("sha256").update(bytes).digest("hex"),
    bytes_base64: bytes.toString("base64"),
  };
}

async function validate(input: CloudRunRequest): Promise<CloudRunRequest> {
  for (const [name, value] of Object.entries({
    accountId: input.accountId,
    applicationIdentityId: input.applicationIdentityId,
    browserSessionId: input.browserSessionId,
    runId: input.runId,
    applicationId: input.applicationId,
  })) {
    if (!/^[A-Za-z0-9_-]{3,160}$/.test(value))
      throw new Error(`Invalid ${name}`);
  }
  if (!/^[A-Za-z0-9:_-]{3,160}$/.test(input.browserProfileId))
    throw new Error("Invalid browserProfileId");
  if (input.packet.applicationId !== input.applicationId)
    throw new Error("Application bundle mismatch");
  if (
    input.packet.applicationIdentityId &&
    input.packet.applicationIdentityId !== input.applicationIdentityId
  ) {
    throw new Error("Application email mismatch");
  }
  if (
    input.packet.browserProfileId &&
    input.packet.browserProfileId !== input.browserProfileId
  )
    throw new Error("Browser profile mismatch");
  const approved = createApprovedExecutionSnapshot(input.packet, input.job);
  assertCloudRunCertifiedNavigation({
    url: input.url,
    job: approved.approvedJob,
  });
  await assertPublicApplicationUrl(input.url);
  return Object.freeze({
    ...input,
    packet: approved.approvedPacket,
    job: approved.approvedJob,
  });
}

export function assertCloudRunCertifiedNavigation(input: {
  url: string;
  job: NormalizedJob;
}): void {
  assertCertifiedProviderNavigationJob(input.url, input.job);
}

async function installCertifiedExactSubmitGuard(
  page: PlaywrightBrowserPage,
  job: NormalizedJob,
): Promise<void> {
  if (job.source === "greenhouse" || job.source === "lever") {
    await page.installExactSubmitGuard(job.source, job.canonicalUrl);
  }
}

async function prepareFreshCloudPage(
  context: BrowserContext,
  job: NormalizedJob,
): Promise<{ page: Page; browserPage: PlaywrightBrowserPage }> {
  await context.setOffline(true);
  const existingPages = context.pages();
  const page = existingPages[0] ?? (await context.newPage());
  await page.goto("about:blank", {
    waitUntil: "domcontentloaded",
    timeout: 10_000,
  });
  for (const candidate of existingPages) {
    if (candidate !== page) await candidate.close();
  }
  const browserPage = new PlaywrightBrowserPage(page);
  await installCertifiedExactSubmitGuard(browserPage, job);
  assertQuiescedCloudContext(context, page);
  await context.setOffline(false);
  return { page, browserPage };
}

async function wrapActiveCloudPage(
  context: BrowserContext,
  job: NormalizedJob,
): Promise<{ page: Page; browserPage: PlaywrightBrowserPage }> {
  const pages = context.pages();
  if (pages.length !== 1 || context.serviceWorkers().length > 0) {
    throw new ProfileRecoveryBlockedError();
  }
  const page = pages[0]!;
  const browserPage = new PlaywrightBrowserPage(page);
  await installCertifiedExactSubmitGuard(browserPage, job);
  return { page, browserPage };
}

type CloudPageGuardInstaller = (
  page: Page,
  job: NormalizedJob,
) => Promise<void>;

export async function prepareRecoveredCloudPage(
  context: BrowserContext,
  recoveryUrl: string,
  job: NormalizedJob,
  installPageGuard: CloudPageGuardInstaller = installCertifiedRawPageGuard,
): Promise<Page> {
  await context.setOffline(true);
  const pages = context.pages();
  const existingHttpPages = pages.filter((candidate) =>
    /^https?:\/\//i.test(candidate.url()),
  );
  assertCloudRecoveryNavigation({
    recoveryUrl,
    existingPageUrls: existingHttpPages.map((candidate) => candidate.url()),
    job,
  });
  if (
    job.source !== "greenhouse" &&
    job.source !== "lever" &&
    existingHttpPages.some((candidate) => candidate.url() !== recoveryUrl)
  ) {
    throw new ProfileRecoveryBlockedError();
  }
  if (existingHttpPages.length > 1) throw new ProfileRecoveryBlockedError();
  const page =
    existingHttpPages[0] ??
    pages.find((candidate) => candidate.url() === "about:blank") ??
    (await context.newPage());
  if (!/^https?:\/\//i.test(page.url()) && page.url() !== "about:blank") {
    await page.goto("about:blank", {
      waitUntil: "domcontentloaded",
      timeout: 10_000,
    });
  }
  await installPageGuard(page, job);
  for (const candidate of context.pages()) {
    if (candidate !== page) await candidate.close();
  }
  assertQuiescedCloudContext(context, page);
  await context.setOffline(false);
  if (!/^https?:\/\//i.test(page.url())) {
    await page.goto(recoveryUrl, {
      waitUntil: "domcontentloaded",
      timeout: 45_000,
    });
  }
  return page;
}

async function installCertifiedRawPageGuard(
  page: Page,
  job: NormalizedJob,
): Promise<void> {
  await installCertifiedExactSubmitGuard(new PlaywrightBrowserPage(page), job);
}

function assertQuiescedCloudContext(context: BrowserContext, page: Page): void {
  if (
    context.serviceWorkers().length > 0 ||
    context.pages().length !== 1 ||
    context.pages()[0] !== page
  ) {
    throw new ProfileRecoveryBlockedError();
  }
}

export function assertCloudRecoveryNavigation(input: {
  recoveryUrl: string;
  existingPageUrls: readonly string[];
  job: NormalizedJob;
}): void {
  assertCloudRunCertifiedNavigation({ url: input.recoveryUrl, job: input.job });
  for (const url of input.existingPageUrls) {
    assertCloudRunCertifiedNavigation({ url, job: input.job });
  }
}

function validateDurableRunResultRequest(
  value: unknown,
): DurableRunResultRequest {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Invalid durable result request");
  }
  const input = value as Record<string, unknown>;
  if (
    Object.keys(input).length !== 6 ||
    typeof input.accountId !== "string" ||
    !/^[A-Za-z0-9_-]{3,160}$/.test(input.accountId) ||
    typeof input.applicationId !== "string" ||
    !/^[A-Za-z0-9_-]{3,160}$/.test(input.applicationId) ||
    typeof input.applicationIdentityId !== "string" ||
    !/^[A-Za-z0-9_-]{3,160}$/.test(input.applicationIdentityId) ||
    typeof input.browserSessionId !== "string" ||
    !/^[A-Za-z0-9_-]{3,160}$/.test(input.browserSessionId) ||
    typeof input.runId !== "string" ||
    !/^[A-Za-z0-9_-]{3,160}$/.test(input.runId) ||
    typeof input.requestId !== "string" ||
    !/^[A-Za-z0-9:_-]{3,240}$/.test(input.requestId) ||
    !requestIdMatchesRun(input.runId, input.requestId)
  ) {
    throw new Error("Invalid durable result request");
  }
  return {
    accountId: input.accountId,
    applicationId: input.applicationId,
    applicationIdentityId: input.applicationIdentityId,
    browserSessionId: input.browserSessionId,
    runId: input.runId,
    requestId: input.requestId,
  };
}

export async function recoverDurableRunResult(
  dataRoot: string,
  encryptionKey: Buffer,
  value: unknown,
): Promise<unknown | undefined> {
  const lookup = validateDurableRunResultRequest(value);
  const paths = profilePaths(
    dataRoot,
    lookup.accountId,
    lookup.applicationIdentityId,
  );
  const result = await readResult<unknown>(
    dataRoot,
    {
      requestId: lookup.requestId,
      profileScope: paths.scope,
    },
    encryptionKey,
  );
  if (result !== undefined) {
    assertDurableResultBinding(result, {
      accountId: lookup.accountId,
      applicationId: lookup.applicationId,
      applicationIdentityId: lookup.applicationIdentityId,
      browserSessionId: lookup.browserSessionId,
      runId: lookup.runId,
    });
  }
  return result;
}

async function recoverDurableRunResultWithResidency(
  dataRoot: string,
  encryptionKey: Buffer,
  value: unknown,
): Promise<unknown | undefined> {
  const lookup = validateDurableRunResultRequest(value);
  const paths = profilePaths(
    dataRoot,
    lookup.accountId,
    lookup.applicationIdentityId,
  );
  const context = { requestId: lookup.requestId, profileScope: paths.scope };
  return runnerVolumeClient.withExistingResultResidency(
    localResultScope(context),
    async (storage) => {
      if (!storage) return undefined;
      const result = await readManagedResult<unknown>(
        storage,
        context,
        encryptionKey,
      );
      if (result !== undefined) {
        assertDurableResultBinding(result, {
          accountId: lookup.accountId,
          applicationId: lookup.applicationId,
          applicationIdentityId: lookup.applicationIdentityId,
          browserSessionId: lookup.browserSessionId,
          runId: lookup.runId,
        });
      }
      return result;
    },
  );
}

async function closeBrowserExecution(
  context: BrowserContext | undefined,
  paths: ManagedProfilePaths,
  input: CloudRunRequest,
  lease: ActiveExecutionLease,
  finishWork = true,
): Promise<void> {
  if (!context) {
    if (finishWork) finishTrackedAccountWork(input.browserSessionId, lease);
    return;
  }
  let failed = false;
  try {
    await context.close();
  } catch {
    failed = true;
  }
  if (trackedAccountWorkIsStopping(input.browserSessionId, lease)) {
    if (finishWork) finishTrackedAccountWork(input.browserSessionId, lease);
    if (failed) throw new Error("Browser cleanup failed");
    return;
  }
  try {
    await withTrackedProfileResidency(lease, paths.scope, async (storage) => {
      assertManagedProfileBinding(paths, storage);
      await persistBrowserProfileSnapshot(
        managedProfilePaths(storage),
        profileKey!,
        profileSnapshotLeaseContext(input, lease),
        profileSnapshotClient,
      );
    });
  } catch {
    failed = true;
  } finally {
    if (finishWork) finishTrackedAccountWork(input.browserSessionId, lease);
  }
  if (failed) throw new Error("Browser cleanup failed");
}

export async function persistBrowserProfileSnapshot(
  paths: ProfilePaths | ManagedProfilePaths,
  encryptionKey: Buffer,
  context: BrowserProfileSnapshotLeaseContext,
  client: Pick<BrowserProfileSnapshotClient, "store">,
): Promise<void> {
  await sealProfile(paths, encryptionKey);
  const snapshot = await readEncryptedProfileSnapshot(paths);
  if (!snapshot)
    throw new Error("Encrypted browser profile snapshot was not created");
  const stored = await client.store(context, snapshot);
  await writeProfileSnapshotGeneration(paths, stored.generation);
}

async function closeActiveRun(
  browserSessionId: string,
  active: NonNullable<ReturnType<typeof activeRuns.get>>,
  finishWork = true,
): Promise<void> {
  try {
    await closeBrowserExecution(
      active.context,
      active.paths,
      active.input,
      active.lease,
      finishWork,
    );
  } finally {
    if (activeRuns.get(browserSessionId) === active)
      activeRuns.delete(browserSessionId);
    activeScopes.delete(active.paths.scope);
  }
}

async function restoreDurableBrowserProfile(
  input: CloudRunRequest,
  paths: ManagedProfilePaths,
  lease: ActiveExecutionLease,
): Promise<void> {
  const remoteSnapshot = await profileSnapshotClient.restore(
    profileSnapshotLeaseContext(input, lease),
  );
  await withTrackedProfileResidency(lease, paths.scope, async (storage) => {
    assertManagedProfileBinding(paths, storage);
    const operationPaths = managedProfilePaths(storage);
    if (remoteSnapshot) {
      await installEncryptedProfileSnapshot(operationPaths, remoteSnapshot);
    }
    await restoreProfile(operationPaths, profileKey!);
  });
}

function profileSnapshotLeaseContext(
  input: CloudRunRequest,
  lease: ActiveExecutionLease,
): BrowserProfileSnapshotLeaseContext {
  const metadata = lease.checkpointMetadata();
  return {
    accountId: input.accountId,
    applicationId: input.applicationId,
    runId: input.runId,
    browserProfileId: input.browserProfileId,
    leaseToken: metadata.leaseToken,
    fence: metadata.fence,
  };
}

async function registerTrackedProfileResidency(
  lease: ActiveExecutionLease,
  profileScope: string,
): Promise<ManagedProfilePaths> {
  const tracked = [...trackedAccountWork.values()].find(
    (candidate) => candidate.lease === lease,
  );
  if (!tracked || tracked.stopping || tracked.pendingWrite) {
    throw new ProfileRecoveryBlockedError();
  }
  let resolveDone = (): void => undefined;
  const done = new Promise<void>((resolve) => {
    resolveDone = resolve;
  });
  let finished = false;
  const pendingWrite = {
    phase: "registering" as "registering" | "writing",
    done,
    finish() {
      if (finished) return;
      finished = true;
      resolveDone();
    },
  };
  tracked.pendingWrite = pendingWrite;
  try {
    const storage = await runnerVolumeClient.registerProfile(
      lease.purgeSubject,
      profileScope,
    );
    if (tracked.stopping) throw new ProfileRecoveryBlockedError();
    pendingWrite.phase = "writing";
    const paths = managedProfilePaths(storage);
    if (tracked.paths.scope !== paths.scope) {
      throw new ProfileRecoveryBlockedError();
    }
    if ("storageVersion" in tracked.paths) {
      assertManagedProfileBinding(tracked.paths, storage);
      return tracked.paths;
    }
    tracked.paths = paths;
    return paths;
  } finally {
    if (tracked.pendingWrite === pendingWrite) tracked.pendingWrite = undefined;
    pendingWrite.finish();
  }
}

function assertManagedProfileBinding(
  paths: ManagedProfilePaths,
  storage: ManagedProfileStorage,
): void {
  const original = paths.storage;
  if (
    paths.storageVersion !== "managed-v2" ||
    paths.subjectSha256 !== storage.subjectSha256 ||
    paths.scope !== storage.scope ||
    paths.directory !== storage.active.canonicalPath ||
    original.subjectSha256 !== storage.subjectSha256 ||
    original.scope !== storage.scope ||
    original.root.deviceId !== storage.root.deviceId ||
    original.root.relativePath !== storage.root.relativePath ||
    original.active.relativePath !== storage.active.relativePath ||
    original.snapshots.relativePath !== storage.snapshots.relativePath ||
    original.checkpoints.relativePath !== storage.checkpoints.relativePath ||
    original.receipts.relativePath !== storage.receipts.relativePath ||
    original.temporary.relativePath !== storage.temporary.relativePath
  ) {
    throw new ProfileRecoveryBlockedError();
  }
}

async function writeManagedReceiptArtifact(
  paths: ManagedProfilePaths,
  lease: ActiveExecutionLease,
  runId: string,
  name: "final.png" | "receipt.json",
  contents: Buffer,
): Promise<string> {
  if (
    contents.length < 1 ||
    contents.length > MAXIMUM_MANAGED_RECEIPT_ARTIFACT_BYTES
  ) {
    throw new ProfileRecoveryBlockedError();
  }
  return withTrackedProfileResidency(lease, paths.scope, async (storage) => {
    assertManagedProfileBinding(paths, storage);
    const runDirectory = await storage.receipts.openChildDirectory(runId);
    if (
      runDirectory.deviceId !== storage.receipts.deviceId ||
      runDirectory.canonicalPath !== join(storage.receipts.canonicalPath, runId)
    ) {
      throw new ProfileRecoveryBlockedError();
    }
    await runDirectory.replaceFile(name, contents);
    const inventory = await runDirectory.inventory();
    const entry = inventory.entries.find(
      (candidate) => candidate.relativePath === name,
    );
    const expectedSha256 = createHash("sha256").update(contents).digest("hex");
    if (
      !entry ||
      entry.kind !== "file" ||
      entry.deviceId !== runDirectory.deviceId ||
      entry.linkCount !== 1 ||
      entry.sizeBytes !== contents.length ||
      entry.sha256 !== expectedSha256
    ) {
      throw new ProfileRecoveryBlockedError();
    }
    const persisted = await runDirectory.readFileBounded(
      name,
      MAXIMUM_MANAGED_RECEIPT_ARTIFACT_BYTES,
    );
    try {
      if (!persisted.equals(contents)) {
        throw new ProfileRecoveryBlockedError();
      }
    } finally {
      persisted.fill(0);
    }
    return join(runDirectory.canonicalPath, name);
  });
}

async function withTrackedProfileResidency<T>(
  lease: ActiveExecutionLease,
  profileScope: string,
  operation: (storage: ManagedProfileStorage) => Promise<T>,
): Promise<T> {
  const tracked = [...trackedAccountWork.values()].find(
    (candidate) => candidate.lease === lease,
  );
  if (!tracked || tracked.stopping || tracked.pendingWrite) {
    throw new ProfileRecoveryBlockedError();
  }
  let resolveDone = (): void => undefined;
  const done = new Promise<void>((resolve) => {
    resolveDone = resolve;
  });
  let finished = false;
  const pendingWrite = {
    phase: "registering" as "registering" | "writing",
    done,
    finish() {
      if (finished) return;
      finished = true;
      resolveDone();
    },
  };
  tracked.pendingWrite = pendingWrite;
  try {
    return await runnerVolumeClient.withProfileWriteResidency(
      lease.purgeSubject,
      profileScope,
      async (storage) => {
        if (tracked.stopping) throw new ProfileRecoveryBlockedError();
        pendingWrite.phase = "writing";
        if (
          tracked.paths.scope !== profileScope ||
          !("storageVersion" in tracked.paths)
        ) {
          throw new ProfileRecoveryBlockedError();
        }
        assertManagedProfileBinding(tracked.paths, storage);
        return operation(storage);
      },
    );
  } finally {
    if (tracked.pendingWrite === pendingWrite) tracked.pendingWrite = undefined;
    pendingWrite.finish();
  }
}

function localResultScope(context: {
  requestId: string;
  profileScope: string;
}): string {
  const scope = durableResultScope(context);
  if (!/^[0-9a-f]{64}$/.test(scope)) throw new ProfileRecoveryBlockedError();
  return scope;
}

async function withRegisteredResultWrite<T>(
  lease: ActiveExecutionLease,
  context: { requestId: string; profileScope: string },
  operation: (storage: ManagedResultStorage) => Promise<T>,
): Promise<T> {
  const tracked = [...trackedAccountWork.values()].find(
    (candidate) => candidate.lease === lease,
  );
  if (!tracked || tracked.stopping || tracked.pendingWrite) {
    throw new ProfileRecoveryBlockedError();
  }
  let resolveDone = (): void => undefined;
  const done = new Promise<void>((resolve) => {
    resolveDone = resolve;
  });
  let finished = false;
  const pendingWrite = {
    phase: "registering" as "registering" | "writing",
    done,
    finish() {
      if (finished) return;
      finished = true;
      resolveDone();
    },
  };
  tracked.pendingWrite = pendingWrite;
  try {
    return await runnerVolumeClient.withResultWriteResidency(
      lease.purgeSubject,
      localResultScope(context),
      async (storage) => {
        if (tracked.stopping) throw new ProfileRecoveryBlockedError();
        pendingWrite.phase = "writing";
        return operation(storage);
      },
    );
  } finally {
    if (tracked.pendingWrite === pendingWrite) tracked.pendingWrite = undefined;
    pendingWrite.finish();
  }
}

function trackAccountWork(
  input: CloudRunRequest,
  paths: ProfilePaths | ManagedProfilePaths,
  lease: ActiveExecutionLease,
): void {
  if (storageAttestationQuiescing) {
    throw new ProfileRecoveryBlockedError();
  }
  const existing = trackedAccountWork.get(input.browserSessionId);
  if (existing) {
    if (existing.lease !== lease || existing.paths.scope !== paths.scope) {
      throw new ProfileRecoveryBlockedError();
    }
    return;
  }
  let resolveDone = (): void => undefined;
  const done = new Promise<void>((resolve) => {
    resolveDone = resolve;
  });
  let finished = false;
  trackedAccountWork.set(input.browserSessionId, {
    paths,
    lease,
    done,
    stopping: false,
    finish() {
      if (finished) return;
      finished = true;
      resolveDone();
    },
  });
}

function trackedManagedProfilePaths(
  lease: ActiveExecutionLease,
): ManagedProfilePaths | undefined {
  const tracked = [...trackedAccountWork.values()].find(
    (candidate) => candidate.lease === lease,
  );
  return tracked && "storageVersion" in tracked.paths
    ? tracked.paths
    : undefined;
}

async function attachTrackedAccountWorkContext(
  browserSessionId: string,
  lease: ActiveExecutionLease,
  context: BrowserContext,
): Promise<void> {
  const tracked = trackedAccountWork.get(browserSessionId);
  if (!tracked || tracked.lease !== lease)
    throw new ProfileRecoveryBlockedError();
  tracked.context = context;
  if (tracked.stopping) {
    await context.close().catch(() => undefined);
    throw new ProfileRecoveryBlockedError();
  }
}

function finishTrackedAccountWork(
  browserSessionId: string,
  lease: ActiveExecutionLease,
): void {
  const tracked = trackedAccountWork.get(browserSessionId);
  if (!tracked || tracked.lease !== lease) return;
  trackedAccountWork.delete(browserSessionId);
  tracked.finish();
}

function assertTrackedAccountWorkMayContinue(
  browserSessionId: string,
  lease: ActiveExecutionLease,
): void {
  const tracked = trackedAccountWork.get(browserSessionId);
  if (!tracked || tracked.lease !== lease || tracked.stopping) {
    throw new ProfileRecoveryBlockedError();
  }
}

function trackedAccountWorkIsStopping(
  browserSessionId: string,
  lease: ActiveExecutionLease,
): boolean {
  const tracked = trackedAccountWork.get(browserSessionId);
  return tracked?.lease === lease && tracked.stopping;
}

export async function stopAccountWorkBySubjectHash(
  purgeSubjectSha256: string,
): Promise<
  { readonly status: "stopped" } | { readonly status: "irreversible" }
> {
  const matches = [...trackedAccountWork.entries()].filter(
    ([, tracked]) =>
      accountPurgeSubjectHash(tracked.lease.purgeSubject) ===
      purgeSubjectSha256,
  );
  if (matches.some(([, tracked]) => tracked.lease.finalSubmitAttempted)) {
    return { status: "irreversible" };
  }
  for (const [, tracked] of matches) tracked.stopping = true;
  if (matches.some(([, tracked]) => tracked.lease.finalSubmitAttempted)) {
    for (const [, tracked] of matches) tracked.stopping = false;
    return { status: "irreversible" };
  }

  let irreversible = false;
  for (const [browserSessionId, tracked] of matches) {
    await tracked.context?.close().catch(() => undefined);
    // A write that already crossed residency registration may still be
    // materializing bytes when purge arrives. Wait for both registration and
    // writing phases before closing the tracked run so the purge inventory is
    // the final writer and no artifact can reappear after its zero rescan.
    await tracked.pendingWrite?.done;
    const active = activeRuns.get(browserSessionId);
    if (active?.lease === tracked.lease) {
      await serializedBrowserSessionMutation(
        browserSessionId,
        tracked.paths.scope,
        async () => {
          const current = activeRuns.get(browserSessionId);
          if (current?.lease !== tracked.lease) return;
          if (current.lease.finalSubmitAttempted) {
            irreversible = true;
            return;
          }
          await closeActiveRun(browserSessionId, current).catch(
            () => undefined,
          );
          await current.lease.finish("failed").catch(() => undefined);
        },
      ).catch(() => undefined);
      if (!irreversible)
        finishTrackedAccountWork(browserSessionId, tracked.lease);
      continue;
    }
    await tracked.done;
  }
  if (
    irreversible ||
    matches.some(([, tracked]) => tracked.lease.finalSubmitAttempted)
  ) {
    for (const [, tracked] of matches) tracked.stopping = false;
    return { status: "irreversible" };
  }
  return { status: "stopped" };
}

export async function quiesceRunnerWorkForStorageAttestation(): Promise<
  { readonly status: "quiesced" } | { readonly status: "irreversible" }
> {
  runnerVolumeReady = false;
  storageAttestationQuiescing = true;
  const current = [...trackedAccountWork.values()];
  if (current.some((tracked) => tracked.lease.finalSubmitAttempted)) {
    return { status: "irreversible" };
  }
  const subjects = [
    ...new Set(
      current.map((tracked) =>
        accountPurgeSubjectHash(tracked.lease.purgeSubject),
      ),
    ),
  ].sort();
  for (const subjectSha256 of subjects) {
    const result = await stopAccountWorkBySubjectHash(subjectSha256);
    if (result.status === "irreversible") {
      return { status: "irreversible" };
    }
  }
  if (
    [...trackedAccountWork.values()].some(
      (tracked) => tracked.lease.finalSubmitAttempted,
    )
  ) {
    return { status: "irreversible" };
  }
  return { status: "quiesced" };
}

async function prepareRunnerForStorageAttestation(): Promise<
  | { readonly status: "quiesced" }
  | { readonly status: "irreversible" }
  | { readonly status: "storage_not_ready" }
> {
  const quiescence = await quiesceRunnerWorkForStorageAttestation();
  if (quiescence.status !== "quiesced") return quiescence;
  const legacy = await scanLegacyRunnerStorage(nativeStorageRoot);
  if (legacy.unclassifiedRootPaths.length !== 0) {
    throw new RunnerVolumeClientError(
      "storage_attestation",
      "invalid_response",
    );
  }
  if (legacy.legacyArtifactCount !== 0) {
    return { status: "storage_not_ready" };
  }
  await completeRunnerStartupStorageRecovery();
  return { status: "quiesced" };
}

export function publicRunnerFailure(error: unknown): {
  status: number;
  code: string;
  message: string;
} {
  if (error instanceof ProfileRecoveryBlockedError) {
    return {
      status: 503,
      code: error.code,
      message:
        "This browser profile is temporarily unavailable while durable recovery completes.",
    };
  }
  if (error instanceof RunnerInterventionPolicyError) {
    return {
      status: 400,
      code: "invalid_intervention_resolution",
      message: error.message,
    };
  }
  if (error instanceof ApprovedExecutionIntegrityError) {
    return {
      status: 409,
      code: error.code,
      message:
        "This application packet changed after approval and must be reviewed again.",
    };
  }
  if (error instanceof RunnerEncryptionError) {
    return {
      status: 500,
      code: `encryption_${error.code}`,
      message: "The application runner could not read encrypted state.",
    };
  }
  if (error instanceof ResultStoreError) {
    return {
      status: 500,
      code: `result_store_${error.code}`,
      message: "The application runner could not read durable result state.",
    };
  }
  if (
    error instanceof ExecutionLeaseError &&
    error.code === "lease_unavailable"
  ) {
    return {
      status: 409,
      code: "lease_unavailable",
      message: "This run is already active.",
    };
  }
  if (error instanceof LeasedRunError) {
    return {
      status: error.outcome === "submitted_result_pending" ? 503 : 500,
      code: error.outcome,
      message:
        error.outcome === "submitted_result_pending"
          ? "The submitted result is durably pending recovery."
          : "The application runner could not safely finish this run.",
    };
  }
  if (error instanceof ExecutionLeaseError) {
    return {
      status: 503,
      code: `lease_${error.code}`,
      message: "The application runner is temporarily unavailable.",
    };
  }
  return {
    status: 500,
    code: "runner_failed",
    message: "The application runner could not finish this run.",
  };
}

function authorized(request: IncomingMessage): boolean {
  const supplied = Buffer.from(
    (request.headers.authorization || "").replace(/^Bearer\s+/i, ""),
  );
  const expected = Buffer.from(serviceToken);
  return (
    supplied.length === expected.length &&
    supplied.length > 0 &&
    timingSafeEqual(supplied, expected)
  );
}

async function body<T>(request: IncomingMessage): Promise<T> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of request) {
    const value = Buffer.from(chunk);
    size += value.length;
    if (size > 5 * 1024 * 1024) throw new Error("Request is too large");
    chunks.push(value);
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8")) as T;
}

function json(response: ServerResponse, status: number, value: unknown): void {
  response.writeHead(status, {
    "Content-Type": "application/json",
    "Cache-Control": "no-store",
  });
  response.end(JSON.stringify(value));
}

async function serialized<T>(
  scope: string,
  operation: () => Promise<T>,
): Promise<T> {
  const previous = locks.get(scope) || Promise.resolve();
  let release!: () => void;
  const current = new Promise<void>((resolve) => {
    release = resolve;
  });
  const queued = previous.then(() => current);
  locks.set(scope, queued);
  await previous;
  try {
    return await operation();
  } finally {
    release();
    if (locks.get(scope) === queued) locks.delete(scope);
  }
}

function reservePendingBrowserSession(
  browserSessionId: string,
  profileScope: string,
): void {
  const existing = pendingBrowserSessions.get(browserSessionId);
  if (existing) {
    if (existing.profileScope !== profileScope) {
      throw new ProfileRecoveryBlockedError();
    }
    existing.references += 1;
    return;
  }
  pendingBrowserSessions.set(browserSessionId, { profileScope, references: 1 });
}

function releasePendingBrowserSession(
  browserSessionId: string,
  profileScope: string,
): void {
  const existing = pendingBrowserSessions.get(browserSessionId);
  if (!existing || existing.profileScope !== profileScope) return;
  existing.references -= 1;
  if (existing.references === 0)
    pendingBrowserSessions.delete(browserSessionId);
}

export async function serializedBrowserSessionMutation<T>(
  browserSessionId: string,
  profileScope: string,
  operation: () => Promise<T>,
): Promise<T> {
  const activeProfileScope = activeRuns.get(browserSessionId)?.paths.scope;
  if (activeProfileScope && activeProfileScope !== profileScope) {
    throw new ProfileRecoveryBlockedError();
  }
  reservePendingBrowserSession(browserSessionId, profileScope);
  try {
    return await serialized(`browser-session:${browserSessionId}`, () =>
      serializedProfileMutation(profileScope, operation),
    );
  } finally {
    releasePendingBrowserSession(browserSessionId, profileScope);
  }
}

export async function serializedKnownBrowserSessionMutation(
  browserSessionId: string,
  knownProfileScope: string | undefined,
  operation: (profileScope: string) => Promise<void>,
): Promise<boolean> {
  const profileScope =
    knownProfileScope ??
    pendingBrowserSessions.get(browserSessionId)?.profileScope;
  if (!profileScope) return false;
  await serializedBrowserSessionMutation(browserSessionId, profileScope, () =>
    operation(profileScope),
  );
  return true;
}

function requestIdMatchesRun(
  runId: string,
  requestId: string,
  resumeOnly = false,
): boolean {
  return (
    (!resumeOnly && requestId === `${runId}:initial`) ||
    (requestId.startsWith(`${runId}:resume:`) &&
      /^:resume:[1-6]$/.test(requestId.slice(runId.length)))
  );
}

async function serializedProfileMutation<T>(
  profileScope: string,
  operation: () => Promise<T>,
): Promise<T> {
  return withProfileFailureIsolation(profileScope, () =>
    serialized(profileScope, operation),
  );
}

async function withProfileFailureIsolation<T>(
  profileScope: string,
  operation: () => Promise<T>,
): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    if (error instanceof LeasedRunError) recoveryIsolation.block(profileScope);
    throw error;
  }
}

function isMainModule(): boolean {
  return (
    process.argv[1] !== undefined &&
    pathToFileURL(resolve(process.argv[1])).href === import.meta.url
  );
}

export function runnerDataRootFromEnv(
  env: NodeJS.ProcessEnv = process.env,
): string {
  const value = env.BLUEY_JOBS_RUNNER_DATA?.trim();
  if (!value) throw new Error("BLUEY_JOBS_RUNNER_DATA is required");
  return value;
}

function requiredRunnerEnv(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
}
