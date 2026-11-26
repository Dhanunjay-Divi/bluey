// Pure v4 wire syntax and byte contracts only. Parsing is not signature,
// resource-isolation, dependency-health, or business-admission authority.
import { Buffer } from "node:buffer";
import { createHash, createHmac } from "node:crypto";

const MAX_WIRE_BYTES = 64 * 1024;
const MAX_DEPTH = 16;
const ROUTE_ID = /^[A-Za-z0-9_-]{20,128}$/;
const SHA256 = /^[0-9a-f]{64}$/;
const ROLES = [
  "discovery_worker", "global_discovery_worker", "jobs_api", "managed_runner",
  "original_source_verifier", "workflow_command_dispatcher",
  "workflow_cleanup_dispatcher", "workflow_gateway", "workflow_worker",
] as const;

export type ProvisioningRuntimeRole = typeof ROLES[number];
export type ManagedCloudAuthorityBinding = Readonly<{
  authorityKind: "provisioning_v4";
  queueGenerationId: string;
} & (
  | { purpose: "production"; contextSha256: null }
  | { purpose: "nonproduction_synthetic"; contextSha256: string }
)>;

export function parseManagedCloudAuthorityBinding(value: unknown): ManagedCloudAuthorityBinding {
  const row = wireRecord(value, ["authorityKind", "contextSha256", "purpose", "queueGenerationId"], 4096);
  if (row.authorityKind !== "provisioning_v4") throw invalid();
  const queueGenerationId = routeId(row.queueGenerationId);
  if (row.purpose === "production" && row.contextSha256 === null) {
    return Object.freeze({ authorityKind: "provisioning_v4", purpose: "production", contextSha256: null,
      queueGenerationId });
  }
  if (row.purpose === "nonproduction_synthetic") {
    return Object.freeze({ authorityKind: "provisioning_v4", purpose: "nonproduction_synthetic",
      contextSha256: digest(row.contextSha256), queueGenerationId });
  }
  throw invalid();
}

export function parseManagedCloudAuthorityBindingBytes(bytes: Uint8Array): ManagedCloudAuthorityBinding {
  if (!(bytes instanceof Uint8Array) || bytes.byteLength > 4096) throw invalid();
  return parseManagedCloudAuthorityBinding(parseProvisioningCanonicalJsonBytes(bytes));
}

/** Sorted UTF-8 keys, safe JSON integers, and exactly one final LF. */
export function provisioningCanonicalJsonBytes(value: unknown): Uint8Array {
  const chunks: string[] = [];
  let size = 0;
  const emit = (part: string): void => {
    size += Buffer.byteLength(part, "utf8");
    if (size > MAX_WIRE_BYTES) throw invalid();
    chunks.push(part);
  };
  const visit = (item: unknown, depth: number): void => {
    if (depth > MAX_DEPTH) throw invalid();
    if (item === null || typeof item === "boolean") {
      emit(JSON.stringify(item));
    } else if (typeof item === "string") {
      validUnicode(item);
      if (Buffer.byteLength(item, "utf8") > MAX_WIRE_BYTES) throw invalid();
      emit(JSON.stringify(item));
    } else if (typeof item === "number") {
      if (!Number.isSafeInteger(item) || Object.is(item, -0)) throw invalid();
      emit(JSON.stringify(item));
    } else if (Array.isArray(item)) {
      if (item.length > 4096 || Object.getPrototypeOf(item) !== Array.prototype
        || Object.getOwnPropertySymbols(item).length !== 0
        || Object.getOwnPropertyNames(item).length !== item.length + 1) throw invalid();
      emit("[");
      for (let index = 0; index < item.length; index += 1) {
        const entry = Object.getOwnPropertyDescriptor(item, String(index));
        if (!entry || !entry.enumerable || !("value" in entry)) throw invalid();
        if (index !== 0) emit(",");
        visit(entry.value, depth + 1);
      }
      emit("]");
    } else {
      const row = plainRecord(item);
      const keys = Object.keys(row);
      if (keys.length > 128) throw invalid();
      keys.forEach(validUnicode);
      keys.sort((left, right) => Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8")));
      emit("{");
      keys.forEach((key, index) => {
        if (index !== 0) emit(",");
        emit(JSON.stringify(key));
        emit(":");
        visit(row[key], depth + 1);
      });
      emit("}");
    }
  };
  visit(value, 0);
  emit("\n");
  return Buffer.from(chunks.join(""), "utf8");
}

/** Exact re-encoding also rejects duplicate keys, extra LF, escapes and spacing. */
export function parseProvisioningCanonicalJsonBytes(bytes: Uint8Array): unknown {
  if (!(bytes instanceof Uint8Array) || bytes.byteLength === 0 || bytes.byteLength > MAX_WIRE_BYTES) {
    throw invalid();
  }
  let value: unknown;
  try {
    value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)) as unknown;
  } catch {
    throw invalid();
  }
  if (!Buffer.from(provisioningCanonicalJsonBytes(value)).equals(Buffer.from(bytes))) throw invalid();
  return value;
}

export function provisioningCanonicalEqual(left: unknown, right: unknown): boolean {
  return Buffer.from(provisioningCanonicalJsonBytes(left)).equals(Buffer.from(provisioningCanonicalJsonBytes(right)));
}

export function deriveManagedCloudProvisioningSessionToken(
  grantToken: string,
  grantId: string,
  runtimeInstanceId: string,
  authorityBinding: ManagedCloudAuthorityBinding,
): string {
  const key = token32(grantToken);
  const binding = parseManagedCloudAuthorityBinding(authorityBinding);
  return createHmac("sha256", key)
    .update("bluey-jobs-managed-cloud-runtime-session-v2\0", "utf8")
    .update(routeId(grantId), "utf8").update("\0", "utf8")
    .update(routeId(runtimeInstanceId), "utf8").update("\0", "utf8")
    .update(provisioningCanonicalJsonBytes(binding))
    .digest("base64url");
}

export function deriveManagedCloudProvisioningDependencyEvidenceSha256(input: Readonly<{
  role: ProvisioningRuntimeRole;
  activationSha256: string;
  manifestSha256: string;
  componentId: string;
  artifactSha256: string;
  taskQueueSha256: string;
  failureConverterSha256: string;
  authorityBinding: ManagedCloudAuthorityBinding;
}>): string {
  const row = exactRecord(input, ["activationSha256", "artifactSha256", "authorityBinding", "componentId",
    "failureConverterSha256", "manifestSha256", "role", "taskQueueSha256"]);
  const role = runtimeRole(row.role);
  const componentId = runtimeComponent(row.componentId, role);
  const hash = createHash("sha256").update("bluey-jobs-managed-cloud-dependency-evidence-v2\0", "utf8");
  for (const value of [role, digest(row.activationSha256), digest(row.manifestSha256), componentId,
    digest(row.artifactSha256), digest(row.taskQueueSha256), digest(row.failureConverterSha256)]) {
    hash.update(value, "utf8").update("\0", "utf8");
  }
  return hash.update(provisioningCanonicalJsonBytes(parseManagedCloudAuthorityBinding(row.authorityBinding)))
    .update("\0", "utf8").digest("hex");
}

export type ProvisioningLifecycleState = "parked" | "latch_acknowledged" | "operational";
type ListenerState = "listening" | "closed";
type LoopState = "parked" | "running" | "stopped";
type ControlProgress = "fresh" | "stale";
type LoopFields = Readonly<{ loopState: LoopState; controlProgress: ControlProgress }>;

export type ProvisioningTemporalNamespaceProbe = Readonly<{
  endpointId: string;
  transport: "tls";
  authority: string;
  namespace: string;
  namespaceId: string;
  state: "registered" | "not_registered";
}>;
export type ProvisioningGatewayControlProbe = Readonly<{
  endpointId: string;
  transport: "https";
  authority: string;
  runtimeInstanceId: string;
  instanceEpoch: number;
  headRevision: number;
  transitionSha256: string;
  lifecycleState: ProvisioningLifecycleState;
  observationSha256: string;
}>;
export type ProvisioningBootStartupProbe = Readonly<{
  bootId: string;
  bootGeneration: number;
  startupRequestId: string;
  parkedHeartbeatSequence: number;
}>;
export type ProvisioningVolumeBindingProbe = Readonly<{
  volumeId: string;
  enrollmentEpoch: number;
  processInstanceId: string;
  runtimeGrantId: string;
  runtimeSha256: string;
  storageAttestationGeneration: number;
  storageAttestationSha256: string;
}>;
export type ManagedCloudDependencyProbe = Readonly<{ version: 1 }> & (
  | Readonly<{ kind: "jobs_api"; listenerState: ListenerState; serverState: "running" | "stopped" }>
  | Readonly<{ kind: "workflow_command_dispatcher" | "workflow_cleanup_dispatcher";
      gatewayControl: ProvisioningGatewayControlProbe | null }> & LoopFields
  | Readonly<{ kind: "workflow_gateway"; listenerState: ListenerState;
      temporalNamespace: ProvisioningTemporalNamespaceProbe | null }>
  | Readonly<{ kind: "workflow_worker"; workerState: "running" | "not_running";
      temporalNamespace: ProvisioningTemporalNamespaceProbe | null }>
  | Readonly<{ kind: "discovery_worker" | "global_discovery_worker" }> & LoopFields
  | Readonly<{ kind: "original_source_verifier"; bootStartup: ProvisioningBootStartupProbe | null }> & LoopFields
  | Readonly<{ kind: "managed_runner"; listenerState: ListenerState; storageRecoveryComplete: boolean;
      volumeBinding: ProvisioningVolumeBindingProbe | null }>
);

export function parseManagedCloudDependencyProbe(value: unknown): ManagedCloudDependencyProbe {
  const row = plainRecord(wireValue(value));
  if (row.version !== 1) throw invalid();
  const kind = runtimeRole(row.kind);
  const common = ["version", "kind"];
  switch (kind) {
    case "jobs_api":
      exactRecord(row, [...common, "listenerState", "serverState"]);
      return Object.freeze({ version: 1, kind, listenerState: listenerState(row.listenerState),
        serverState: choice(row.serverState, ["running", "stopped"]) });
    case "workflow_command_dispatcher":
    case "workflow_cleanup_dispatcher":
      exactRecord(row, [...common, "loopState", "controlProgress", "gatewayControl"]);
      return Object.freeze({ version: 1, kind, ...loopFields(row), gatewayControl: gatewayProbe(row.gatewayControl) });
    case "workflow_gateway":
      exactRecord(row, [...common, "listenerState", "temporalNamespace"]);
      return Object.freeze({ version: 1, kind, listenerState: listenerState(row.listenerState),
        temporalNamespace: temporalProbe(row.temporalNamespace) });
    case "workflow_worker":
      exactRecord(row, [...common, "workerState", "temporalNamespace"]);
      return Object.freeze({ version: 1, kind, workerState: choice(row.workerState, ["running", "not_running"]),
        temporalNamespace: temporalProbe(row.temporalNamespace) });
    case "discovery_worker":
    case "global_discovery_worker":
      exactRecord(row, [...common, "loopState", "controlProgress"]);
      return Object.freeze({ version: 1, kind, ...loopFields(row) });
    case "original_source_verifier":
      exactRecord(row, [...common, "loopState", "controlProgress", "bootStartup"]);
      return Object.freeze({ version: 1, kind, ...loopFields(row), bootStartup: bootProbe(row.bootStartup) });
    case "managed_runner":
      exactRecord(row, [...common, "listenerState", "storageRecoveryComplete", "volumeBinding"]);
      return Object.freeze({ version: 1, kind, listenerState: listenerState(row.listenerState),
        storageRecoveryComplete: boolean(row.storageRecoveryComplete), volumeBinding: volumeProbe(row.volumeBinding) });
  }
}

type RuntimeFields = Readonly<{
  runtimeInstanceId: string; workerId: string; activationSha256: string; manifestSha256: string;
  componentId: string; role: ProvisioningRuntimeRole; headRevision: number; transitionSha256: string;
  artifactSha256: string; migrationSetSha256: string; configSchemaSha256: string; protocolSetSha256: string;
  taskQueueSha256: string; failureConverterSha256: string; dependencyEvidenceSha256: string;
}>;
type Scope = Readonly<{ environment: "staging" | "production"; region: string; channel: "shadow" | "canary" | "general" }>;
type HealthFields = Readonly<{
  healthState: "ready" | "degraded" | "draining";
  reasonCode: null | "artifact_mismatch" | "config_mismatch" | "dependency_unavailable" | "draining"
    | "head_mismatch" | "migration_mismatch" | "probe_failed" | "protocol_mismatch" | "startup";
}>;
type LifecycleFields = Readonly<{
  version: 2; authorityBinding: ManagedCloudAuthorityBinding; lifecycleState: ProvisioningLifecycleState;
  acknowledgedLatchSha256: string | null; dependencyProbe: ManagedCloudDependencyProbe;
  queueObservation: Readonly<{ pollerIdentitySha256: string; backlogCount: number; deliveredTaskCount: number }> | null;
}>;
export type ManagedCloudProvisioningClaimRequest = Readonly<{
  version: 2; authorityBinding: ManagedCloudAuthorityBinding; grantId: string; grantToken: string;
  workerId: string; sessionToken: string; runtimeInstanceId: string; runtimeIdentitySha256: string;
}>;
export type ManagedCloudProvisioningClaimResponse = RuntimeFields & Readonly<{
  version: 2; authorityBinding: ManagedCloudAuthorityBinding; servingLatchSha256: string | null;
  grantId: string; runtimeIdentitySha256: string; scope: Scope; activationExpiresAtMs: number;
  instanceEpoch: number; nextHeartbeatSequence: number; claimedAtMs: number; replayed: boolean;
}>;
type ObservedRuntimeFields = Omit<RuntimeFields, "headRevision" | "transitionSha256"> & Readonly<{
  observedHeadRevision: number; observedTransitionSha256: string;
}>;
export type ManagedCloudProvisioningHeartbeatRequest = ObservedRuntimeFields & HealthFields & LifecycleFields & Readonly<{
  sessionToken: string; heartbeatSequence: number; bootId?: string; bootGeneration?: number;
}>;
export type ManagedCloudProvisioningHeartbeatResponse = ObservedRuntimeFields & HealthFields & LifecycleFields & Readonly<{
  instanceEpoch: number; heartbeatSequence: number; heartbeatAtMs: number; replayed: boolean;
  observationSha256: string; servingLatchSha256: string | null;
}>;

// These are new, closed shapes, not extensions dynamically inferred from v1.
const CLAIM_REQUEST_KEYS = [
  "authorityBinding", "grantId", "grantToken", "runtimeIdentitySha256", "runtimeInstanceId",
  "sessionToken", "version", "workerId",
] as const;
const CLAIM_RESPONSE_KEYS = [
  "activationExpiresAtMs", "activationSha256", "artifactSha256", "authorityBinding", "claimedAtMs",
  "componentId", "configSchemaSha256", "dependencyEvidenceSha256", "failureConverterSha256", "grantId",
  "headRevision", "instanceEpoch", "manifestSha256", "migrationSetSha256", "nextHeartbeatSequence",
  "protocolSetSha256", "replayed", "role", "runtimeIdentitySha256", "runtimeInstanceId", "scope",
  "servingLatchSha256", "taskQueueSha256", "transitionSha256", "version", "workerId",
] as const;
const HEARTBEAT_REQUEST_KEYS = [
  "acknowledgedLatchSha256", "activationSha256", "artifactSha256", "authorityBinding", "componentId",
  "configSchemaSha256", "dependencyEvidenceSha256", "dependencyProbe", "failureConverterSha256", "healthState",
  "heartbeatSequence", "lifecycleState", "manifestSha256", "migrationSetSha256", "observedHeadRevision",
  "observedTransitionSha256", "protocolSetSha256", "queueObservation", "reasonCode", "role", "runtimeInstanceId",
  "sessionToken", "taskQueueSha256", "version", "workerId",
] as const;
const HEARTBEAT_RESPONSE_KEYS = [
  "acknowledgedLatchSha256", "activationSha256", "artifactSha256", "authorityBinding", "componentId",
  "configSchemaSha256", "dependencyEvidenceSha256", "dependencyProbe", "failureConverterSha256", "healthState",
  "heartbeatAtMs", "heartbeatSequence", "instanceEpoch", "lifecycleState", "manifestSha256", "migrationSetSha256",
  "observationSha256", "observedHeadRevision", "observedTransitionSha256", "protocolSetSha256", "queueObservation",
  "reasonCode", "replayed", "role", "runtimeInstanceId", "servingLatchSha256", "taskQueueSha256", "version", "workerId",
] as const;

export function parseManagedCloudProvisioningClaimRequest(value: unknown): ManagedCloudProvisioningClaimRequest {
  const row = wireRecord(value, CLAIM_REQUEST_KEYS);
  if (row.version !== 2) throw invalid();
  return Object.freeze({ version: 2, authorityBinding: parseManagedCloudAuthorityBinding(row.authorityBinding),
    grantId: routeId(row.grantId), grantToken: token32(row.grantToken).toString("base64url"),
    workerId: routeId(row.workerId), sessionToken: token32(row.sessionToken).toString("base64url"),
    runtimeInstanceId: routeId(row.runtimeInstanceId), runtimeIdentitySha256: digest(row.runtimeIdentitySha256) });
}

export function parseManagedCloudProvisioningClaimResponse(value: unknown): ManagedCloudProvisioningClaimResponse {
  const row = wireRecord(value, CLAIM_RESPONSE_KEYS);
  if (row.version !== 2) throw invalid();
  const scopeRow = exactRecord(row.scope, ["channel", "environment", "region"]);
  if (typeof scopeRow.region !== "string" || !/^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/.test(scopeRow.region)) {
    throw invalid();
  }
  const scope = Object.freeze({ environment: choice(scopeRow.environment, ["staging", "production"]),
    channel: choice(scopeRow.channel, ["shadow", "canary", "general"]), region: scopeRow.region });
  return Object.freeze({ ...runtimeFields(row, "headRevision", "transitionSha256"), version: 2,
    authorityBinding: parseManagedCloudAuthorityBinding(row.authorityBinding),
    servingLatchSha256: nullableDigest(row.servingLatchSha256), grantId: routeId(row.grantId),
    runtimeIdentitySha256: digest(row.runtimeIdentitySha256), scope,
    activationExpiresAtMs: integer(row.activationExpiresAtMs, 1), instanceEpoch: integer(row.instanceEpoch, 1),
    nextHeartbeatSequence: integer(row.nextHeartbeatSequence, 1), claimedAtMs: integer(row.claimedAtMs, 1),
    replayed: boolean(row.replayed) });
}

export function parseManagedCloudProvisioningHeartbeatRequest(value: unknown): ManagedCloudProvisioningHeartbeatRequest {
  const row = plainRecord(wireValue(value));
  const hasBootId = Object.hasOwn(row, "bootId");
  const hasBootGeneration = Object.hasOwn(row, "bootGeneration");
  if (hasBootId !== hasBootGeneration) throw invalid();
  exactRecord(row, hasBootId ? [...HEARTBEAT_REQUEST_KEYS, "bootId", "bootGeneration"] : HEARTBEAT_REQUEST_KEYS);
  const fields = observedRuntimeFields(row);
  if (hasBootId && fields.role !== "original_source_verifier") throw invalid();
  return Object.freeze({ ...fields, ...healthFields(row), ...lifecycleFields(row, fields.role),
    sessionToken: token32(row.sessionToken).toString("base64url"), heartbeatSequence: integer(row.heartbeatSequence, 1),
    ...(hasBootId ? { bootId: bootToken(row.bootId), bootGeneration: integer(row.bootGeneration, 1) } : {}) });
}

export function parseManagedCloudProvisioningHeartbeatResponse(value: unknown): ManagedCloudProvisioningHeartbeatResponse {
  const row = wireRecord(value, HEARTBEAT_RESPONSE_KEYS);
  const fields = observedRuntimeFields(row);
  return Object.freeze({ ...fields, ...healthFields(row), ...lifecycleFields(row, fields.role),
    instanceEpoch: integer(row.instanceEpoch, 1), heartbeatSequence: integer(row.heartbeatSequence, 1),
    heartbeatAtMs: integer(row.heartbeatAtMs, 1), replayed: boolean(row.replayed),
    observationSha256: digest(row.observationSha256), servingLatchSha256: nullableDigest(row.servingLatchSha256) });
}

/** Byte coherence only: this does not prove atomic storage, freshness or readiness. */
export function deriveManagedCloudProvisioningObservationSha256(input: Readonly<{
  grantId: string;
  heartbeat: ManagedCloudProvisioningHeartbeatRequest;
  heartbeatAtMs: number;
  instanceEpoch: number;
  servingLatchSha256: string | null;
}>): string {
  const row = exactRecord(input, ["grantId", "heartbeat", "heartbeatAtMs", "instanceEpoch", "servingLatchSha256"]);
  const { sessionToken: _sessionToken, ...heartbeat } = parseManagedCloudProvisioningHeartbeatRequest(row.heartbeat);
  const envelope = { grantId: routeId(row.grantId), heartbeat,
    heartbeatAtMs: integer(row.heartbeatAtMs, 1), instanceEpoch: integer(row.instanceEpoch, 1),
    servingLatchSha256: nullableDigest(row.servingLatchSha256) };
  return createHash("sha256").update("bluey-jobs-managed-cloud-runtime-observation-v2\0", "utf8")
    .update(provisioningCanonicalJsonBytes(envelope)).digest("hex");
}

function runtimeFields(row: Record<string, unknown>, head: string, transition: string): RuntimeFields {
  const role = runtimeRole(row.role);
  return { runtimeInstanceId: routeId(row.runtimeInstanceId), workerId: routeId(row.workerId),
    activationSha256: digest(row.activationSha256), manifestSha256: digest(row.manifestSha256),
    componentId: runtimeComponent(row.componentId, role), role, headRevision: integer(row[head], 1),
    transitionSha256: digest(row[transition]), artifactSha256: digest(row.artifactSha256),
    migrationSetSha256: digest(row.migrationSetSha256), configSchemaSha256: digest(row.configSchemaSha256),
    protocolSetSha256: digest(row.protocolSetSha256), taskQueueSha256: digest(row.taskQueueSha256),
    failureConverterSha256: digest(row.failureConverterSha256), dependencyEvidenceSha256: digest(row.dependencyEvidenceSha256) };
}

function observedRuntimeFields(row: Record<string, unknown>): ObservedRuntimeFields {
  const { headRevision, transitionSha256, ...fields } = runtimeFields(row, "observedHeadRevision", "observedTransitionSha256");
  return { ...fields, observedHeadRevision: headRevision, observedTransitionSha256: transitionSha256 };
}

function healthFields(row: Record<string, unknown>): HealthFields {
  return { healthState: choice(row.healthState, ["ready", "degraded", "draining"]),
    reasonCode: row.reasonCode === null ? null : choice(row.reasonCode, ["artifact_mismatch", "config_mismatch",
      "dependency_unavailable", "draining", "head_mismatch", "migration_mismatch", "probe_failed", "protocol_mismatch", "startup"]) };
}

function lifecycleFields(row: Record<string, unknown>, role: ProvisioningRuntimeRole): LifecycleFields {
  if (row.version !== 2) throw invalid();
  const dependencyProbe = parseManagedCloudDependencyProbe(row.dependencyProbe);
  if (dependencyProbe.kind !== role) throw invalid();
  let queueObservation: LifecycleFields["queueObservation"] = null;
  if (row.queueObservation !== null) {
    if (role !== "workflow_worker") throw invalid();
    const queue = exactRecord(row.queueObservation, ["pollerIdentitySha256", "backlogCount", "deliveredTaskCount"]);
    queueObservation = Object.freeze({ pollerIdentitySha256: digest(queue.pollerIdentitySha256),
      backlogCount: integer(queue.backlogCount, 0), deliveredTaskCount: integer(queue.deliveredTaskCount, 0) });
  }
  // Phase/health/latch eligibility and real probe facts require the future DB
  // validator. This structural parser deliberately supplies no admission result.
  return { version: 2, authorityBinding: parseManagedCloudAuthorityBinding(row.authorityBinding),
    lifecycleState: lifecycleState(row.lifecycleState), acknowledgedLatchSha256: nullableDigest(row.acknowledgedLatchSha256),
    dependencyProbe, queueObservation };
}

function nullableDigest(value: unknown): string | null {
  return value === null ? null : digest(value);
}

function temporalProbe(value: unknown): ProvisioningTemporalNamespaceProbe | null {
  if (value === null) return null;
  const row = exactRecord(value, ["endpointId", "transport", "authority", "namespace", "namespaceId", "state"]);
  if (row.transport !== "tls") throw invalid();
  return Object.freeze({ endpointId: routeId(row.endpointId), transport: "tls", authority: authority(row.authority),
    namespace: boundedText(row.namespace, 240), namespaceId: boundedText(row.namespaceId, 128),
    state: choice(row.state, ["registered", "not_registered"]) });
}

function gatewayProbe(value: unknown): ProvisioningGatewayControlProbe | null {
  if (value === null) return null;
  const row = exactRecord(value, ["endpointId", "transport", "authority", "runtimeInstanceId", "instanceEpoch",
    "headRevision", "transitionSha256", "lifecycleState", "observationSha256"]);
  if (row.transport !== "https") throw invalid();
  return Object.freeze({ endpointId: routeId(row.endpointId), transport: "https", authority: authority(row.authority),
    runtimeInstanceId: routeId(row.runtimeInstanceId), instanceEpoch: integer(row.instanceEpoch, 1),
    headRevision: integer(row.headRevision, 1), transitionSha256: digest(row.transitionSha256),
    lifecycleState: lifecycleState(row.lifecycleState), observationSha256: digest(row.observationSha256) });
}

function bootProbe(value: unknown): ProvisioningBootStartupProbe | null {
  if (value === null) return null;
  const row = exactRecord(value, ["bootId", "bootGeneration", "startupRequestId", "parkedHeartbeatSequence"]);
  return Object.freeze({ bootId: bootToken(row.bootId), bootGeneration: integer(row.bootGeneration, 1),
    startupRequestId: bootToken(row.startupRequestId), parkedHeartbeatSequence: integer(row.parkedHeartbeatSequence, 1) });
}

function volumeProbe(value: unknown): ProvisioningVolumeBindingProbe | null {
  if (value === null) return null;
  const row = exactRecord(value, ["volumeId", "enrollmentEpoch", "processInstanceId", "runtimeGrantId", "runtimeSha256",
    "storageAttestationGeneration", "storageAttestationSha256"]);
  const runtimeGrantId = row.runtimeGrantId;
  if (typeof runtimeGrantId !== "string" || !/^[A-Za-z0-9][A-Za-z0-9._:+-]{0,127}$/.test(runtimeGrantId)) throw invalid();
  return Object.freeze({ volumeId: token32(row.volumeId).toString("base64url"), enrollmentEpoch: integer(row.enrollmentEpoch, 1),
    processInstanceId: token32(row.processInstanceId).toString("base64url"), runtimeGrantId,
    runtimeSha256: digest(row.runtimeSha256), storageAttestationGeneration: integer(row.storageAttestationGeneration, 1),
    storageAttestationSha256: digest(row.storageAttestationSha256) });
}

function loopFields(row: Record<string, unknown>): LoopFields {
  const loopState = choice(row.loopState, ["parked", "running", "stopped"]);
  const controlProgress = choice(row.controlProgress, ["fresh", "stale"]);
  if (loopState === "stopped" && controlProgress !== "stale") throw invalid();
  return Object.freeze({ loopState, controlProgress });
}

function listenerState(value: unknown): ListenerState {
  return choice(value, ["listening", "closed"]);
}

function lifecycleState(value: unknown): ProvisioningLifecycleState {
  return choice(value, ["parked", "latch_acknowledged", "operational"]);
}

function authority(value: unknown): string {
  if (typeof value !== "string" || value.length > 253 || !/^[\x21-\x7e]+$/.test(value)) throw invalid();
  const parts = value.split(":");
  if (parts.length !== 2) throw invalid(); // No IPv6 until a shared canonical grammar is reviewed.
  const [host, port] = parts;
  if (!/^[1-9][0-9]{0,4}$/.test(port!) || Number(port) > 65535 || !host) throw invalid();
  const labels = host.split(".");
  if (labels.some((label) => !/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(label))) throw invalid();
  const final = labels[labels.length - 1]!;
  if (/^[0-9]+$/.test(final) || /^0x[0-9a-f]*$/.test(final)) {
    if (labels.length !== 4 || labels.some((label) => !/^(?:0|[1-9][0-9]{0,2})$/.test(label)
      || Number(label) > 255)) throw invalid();
  }
  return value;
}

function boundedText(value: unknown, maximumBytes: number): string {
  if (typeof value !== "string" || value.length === 0 || value.trim() !== value
    || value.normalize("NFC") !== value || /\p{Cc}/u.test(value)
    || Buffer.byteLength(value, "utf8") > maximumBytes) throw invalid();
  validUnicode(value);
  return value;
}

function bootToken(value: unknown): string {
  if (typeof value !== "string" || !/^[A-Za-z0-9_.:/-]{20,128}$/.test(value)) throw invalid();
  return value;
}

function integer(value: unknown, minimum: number): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || Object.is(value, -0) || value < minimum) throw invalid();
  return value;
}

function boolean(value: unknown): boolean {
  if (typeof value !== "boolean") throw invalid();
  return value;
}

function choice<const T extends string>(value: unknown, allowed: readonly T[]): T {
  if (typeof value !== "string" || !allowed.includes(value as T)) throw invalid();
  return value as T;
}

function wireValue(value: unknown, maximumBytes = MAX_WIRE_BYTES): unknown {
  // Value-shape parsing does not prove anything about original JSON bytes.
  // Only the separate raw canonical-document decoder can reject duplicate keys.
  if (provisioningCanonicalJsonBytes(value).byteLength > maximumBytes) throw invalid();
  return value;
}

function wireRecord(value: unknown, keys: readonly string[], maximumBytes = MAX_WIRE_BYTES): Record<string, unknown> {
  return exactRecord(wireValue(value, maximumBytes), keys);
}

function plainRecord(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw invalid();
  const prototype: unknown = Object.getPrototypeOf(value);
  if (prototype !== Object.prototype && prototype !== null) throw invalid();
  if (Object.getOwnPropertySymbols(value).length !== 0) throw invalid();
  const descriptors = Object.getOwnPropertyDescriptors(value);
  if (Object.values(descriptors).some((entry) => !entry.enumerable || !("value" in entry))) throw invalid();
  return value as Record<string, unknown>;
}

function exactRecord(value: unknown, keys: readonly string[]): Record<string, unknown> {
  const row = plainRecord(value);
  const actual = Object.keys(row).sort();
  const expected = [...keys].sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) throw invalid();
  return row;
}

function validUnicode(value: string): void {
  if (value.includes("\ufffd") || Buffer.from(value, "utf8").toString("utf8") !== value) throw invalid();
}

function routeId(value: unknown): string {
  if (typeof value !== "string" || !ROUTE_ID.test(value)) throw invalid();
  return value;
}

function digest(value: unknown): string {
  if (typeof value !== "string" || !SHA256.test(value)) throw invalid();
  return value;
}

function token32(value: unknown): Buffer {
  if (typeof value !== "string" || !/^[A-Za-z0-9_-]{43}$/.test(value)) throw invalid();
  const bytes = Buffer.from(value, "base64url");
  if (bytes.length !== 32 || bytes.toString("base64url") !== value) throw invalid();
  return bytes;
}

function runtimeRole(value: unknown): ProvisioningRuntimeRole {
  if (typeof value !== "string" || !ROLES.includes(value as ProvisioningRuntimeRole)) throw invalid();
  return value as ProvisioningRuntimeRole;
}

function runtimeComponent(value: unknown, role: ProvisioningRuntimeRole): string {
  const expected = role === "managed_runner" ? "jobs-runner"
    : ["jobs_api", "workflow_command_dispatcher", "workflow_cleanup_dispatcher"].includes(role)
      ? "jobs-api" : "jobs-workflows";
  if (value !== expected) throw invalid();
  return expected;
}

function invalid(): Error {
  return new Error("Managed-cloud provisioning protocol is invalid");
}
