import { invoke } from "./tauri";
import {
  ASSISTANT_MODES,
  hasAssistantProfileErrors,
  validateAssistantProfile,
  type AssistantProfile,
} from "../pages/assistantProfile";

export const WORKSPACE_SCHEMA_VERSION = 1;
export const WORKSPACE_STALE_REVISION_ERROR = "workspace changed in another view; reload before saving";

export interface WorkspaceActivityReference {
  meeting_id: string;
  title: string;
  started_at: string;
  ended_at?: string;
}

export interface WorkspaceContextReference {
  meeting_id: string;
  context_id: string;
  title: string;
  kind: "image" | "diagram" | "code" | "document" | "text" | "other";
  processing_status: "pending" | "ready" | "unsupported" | "failed";
  created_at: string;
}

export interface WorkspaceArtifactReference {
  meeting_id: string;
  conversation_turn_id: string;
  artifact_type: "code" | "system_design" | "screen" | "document" | "structured";
  title: string;
  created_at: string;
}

export interface WorkspaceLinkedJobMetadata {
  import_id: string;
  context_sha256: string;
  source: {
    application_id?: string;
    receipt_id?: string;
    resume_version_id?: string;
    receipt_fingerprint?: string;
  };
  linked_at: string;
}

export type WorkspaceDeletionState =
  | { state: "active" }
  | { state: "deleted"; deleted_at: string };

export interface WorkspaceRecord {
  schema_version: number;
  id: string;
  owner_account_id?: string;
  title: string;
  revision: number;
  profile: AssistantProfile;
  instructions?: string;
  activity?: WorkspaceActivityReference[];
  context?: WorkspaceContextReference[];
  artifacts?: WorkspaceArtifactReference[];
  linked_job?: WorkspaceLinkedJobMetadata;
  deletion_state: WorkspaceDeletionState;
  created_at: string;
  updated_at: string;
}

export interface WorkspaceCreateRequest {
  title: string;
  profile?: AssistantProfile;
  instructions?: string;
}

export type WorkspaceInstructionsPatch =
  | { action: "unchanged" }
  | { action: "clear" }
  | { action: "set"; text: string };

export interface WorkspaceUpdateRequest {
  workspace_id: string;
  expected_revision: number;
  title?: string;
  profile?: AssistantProfile;
  instructions: WorkspaceInstructionsPatch;
}

export interface WorkspaceListResponse {
  type: "workspace_list";
  workspaces: WorkspaceRecord[];
  active_workspace_id: string | null;
}

export interface WorkspaceResponse {
  type: "workspace";
  workspace: WorkspaceRecord;
  active_workspace_id: string | null;
}

export interface WorkspaceDeletedResponse {
  type: "workspace_deleted";
  workspace_id: string;
  deleted: boolean;
  active_workspace_id: string | null;
}

export async function workspaceList(): Promise<WorkspaceListResponse> {
  return assertWorkspaceListResponse(await invoke<unknown>("workspace_list"));
}

export async function workspaceGet(workspaceId: string): Promise<WorkspaceResponse> {
  return assertWorkspaceResponse(
    await invoke<unknown>("workspace_get", { workspaceId }),
    "workspace_get",
  );
}

export async function workspaceCreate(request: WorkspaceCreateRequest): Promise<WorkspaceResponse> {
  return assertWorkspaceResponse(
    await invoke<unknown>("workspace_create", { request }),
    "workspace_create",
  );
}

export async function workspaceUpdate(request: WorkspaceUpdateRequest): Promise<WorkspaceResponse> {
  return assertWorkspaceResponse(
    await invoke<unknown>("workspace_update", { request }),
    "workspace_update",
  );
}

export async function workspaceActivate(workspaceId: string): Promise<WorkspaceResponse> {
  return assertWorkspaceResponse(
    await invoke<unknown>("workspace_activate", { workspaceId }),
    "workspace_activate",
  );
}

export async function workspaceDelete(
  workspaceId: string,
  expectedRevision: number,
): Promise<WorkspaceDeletedResponse> {
  const value = await invoke<unknown>("workspace_delete", { workspaceId, expectedRevision });
  if (
    !isObject(value)
    || value.type !== "workspace_deleted"
    || typeof value.workspace_id !== "string"
    || typeof value.deleted !== "boolean"
    || !isOptionalId(value.active_workspace_id)
  ) {
    throw new Error("Bluey returned an unreadable workspace_delete response.");
  }
  return value as unknown as WorkspaceDeletedResponse;
}

export function workspaceInstructionsPatch(
  previous: string | null | undefined,
  next: string | null | undefined,
): WorkspaceInstructionsPatch {
  const previousText = normalizeInstructions(previous);
  const nextText = normalizeInstructions(next);
  if (previousText === nextText) return { action: "unchanged" };
  if (!nextText) return { action: "clear" };
  return { action: "set", text: nextText };
}

function assertWorkspaceListResponse(value: unknown): WorkspaceListResponse {
  if (
    !isObject(value)
    || value.type !== "workspace_list"
    || !Array.isArray(value.workspaces)
    || !value.workspaces.every(isWorkspaceRecord)
    || !isOptionalId(value.active_workspace_id)
  ) {
    throw new Error("Bluey returned an unreadable workspace_list response.");
  }
  return value as unknown as WorkspaceListResponse;
}

function assertWorkspaceResponse(value: unknown, command: string): WorkspaceResponse {
  if (
    !isObject(value)
    || value.type !== "workspace"
    || !isWorkspaceRecord(value.workspace)
    || !isOptionalId(value.active_workspace_id)
  ) {
    throw new Error(`Bluey returned an unreadable ${command} response.`);
  }
  return value as unknown as WorkspaceResponse;
}

function isWorkspaceRecord(value: unknown): boolean {
  if (
    !isObject(value)
    || value.schema_version !== WORKSPACE_SCHEMA_VERSION
    || !isUuid(value.id)
    || !isBoundedText(value.title, 120)
    || !Number.isSafeInteger(value.revision)
    || (value.revision as number) <= 0
    || !isAssistantProfile(value.profile)
    || !isOptionalBoundedText(value.owner_account_id, 200)
    || !isOptionalBoundedText(value.instructions, 4_000)
    || !isTimestamp(value.created_at)
    || !isTimestamp(value.updated_at)
    || !isDeletionState(value.deletion_state)
  ) {
    return false;
  }

  const activity = value.activity ?? [];
  const context = value.context ?? [];
  const artifacts = value.artifacts ?? [];
  if (
    !Array.isArray(activity)
    || activity.length > 128
    || !activity.every(isActivityReference)
    || !Array.isArray(context)
    || context.length > 256
    || !context.every(isContextReference)
    || !Array.isArray(artifacts)
    || artifacts.length > 128
    || !artifacts.every(isArtifactReference)
  ) {
    return false;
  }

  if (value.linked_job !== undefined) {
    if (!isLinkedJob(value.linked_job)) return false;
    const profile = value.profile as unknown as AssistantProfile;
    if (!sourcesEqual(profile.source, value.linked_job.source)) return false;
  }
  return true;
}

function isAssistantProfile(value: unknown): value is AssistantProfile {
  if (
    !isObject(value)
    || value.schema_version !== 1
    || typeof value.mode !== "string"
    || !ASSISTANT_MODES.includes(value.mode as AssistantProfile["mode"])
    || !isNullableString(value.target_role)
    || !isNullableString(value.company)
    || !isNullableString(value.custom_instructions)
    || !Array.isArray(value.priority_questions)
    || !value.priority_questions.every((question) => typeof question === "string")
    || (value.source !== undefined && value.source !== null && !isAssistantSource(value.source))
  ) {
    return false;
  }
  return !hasAssistantProfileErrors(validateAssistantProfile(value as unknown as AssistantProfile));
}

function isActivityReference(value: unknown): boolean {
  return isObject(value)
    && isUuid(value.meeting_id)
    && isBoundedText(value.title, 200)
    && isTimestamp(value.started_at)
    && (value.ended_at === undefined || isTimestamp(value.ended_at));
}

function isContextReference(value: unknown): boolean {
  return isObject(value)
    && isUuid(value.meeting_id)
    && isUuid(value.context_id)
    && isBoundedText(value.title, 200)
    && ["image", "diagram", "code", "document", "text", "other"].includes(String(value.kind))
    && ["pending", "ready", "unsupported", "failed"].includes(String(value.processing_status))
    && isTimestamp(value.created_at);
}

function isArtifactReference(value: unknown): boolean {
  return isObject(value)
    && isUuid(value.meeting_id)
    && isUuid(value.conversation_turn_id)
    && ["code", "system_design", "screen", "document", "structured"].includes(String(value.artifact_type))
    && isBoundedText(value.title, 200)
    && isTimestamp(value.created_at);
}

function isLinkedJob(value: unknown): value is WorkspaceLinkedJobMetadata {
  return isObject(value)
    && isBoundedText(value.import_id, 200)
    && typeof value.context_sha256 === "string"
    && /^[a-fA-F0-9]{64}$/.test(value.context_sha256)
    && isAssistantSource(value.source)
    && isTimestamp(value.linked_at);
}

function isAssistantSource(value: unknown): value is NonNullable<AssistantProfile["source"]> {
  if (!isObject(value)) return false;
  for (const field of ["application_id", "receipt_id", "resume_version_id"] as const) {
    const candidate = value[field];
    if (candidate !== undefined && !isBoundedText(candidate, 200)) return false;
  }
  return value.receipt_fingerprint === undefined
    || (typeof value.receipt_fingerprint === "string"
      && /^[a-fA-F0-9]{64}$/.test(value.receipt_fingerprint));
}

function sourcesEqual(
  left: AssistantProfile["source"] | null | undefined,
  right: AssistantProfile["source"] | null | undefined,
): boolean {
  if (!left || !right) return !left && !right;
  return left.application_id === right.application_id
    && left.receipt_id === right.receipt_id
    && left.resume_version_id === right.resume_version_id
    && left.receipt_fingerprint === right.receipt_fingerprint;
}

function isDeletionState(value: unknown): boolean {
  return isObject(value)
    && (value.state === "active"
      || (value.state === "deleted" && isTimestamp(value.deleted_at)));
}

function isUuid(value: unknown): value is string {
  return typeof value === "string"
    && value !== "00000000-0000-0000-0000-000000000000"
    && /^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[1-8a-fA-F0-9][a-fA-F0-9]{3}-[89abABa-fA-F0-9][a-fA-F0-9]{3}-[a-fA-F0-9]{12}$/.test(value);
}

function isTimestamp(value: unknown): value is string {
  return typeof value === "string" && /^\d{1,32}$/.test(value);
}

function isBoundedText(value: unknown, maxCharacters: number): value is string {
  return typeof value === "string"
    && Array.from(value).length > 0
    && Array.from(value).length <= maxCharacters;
}

function isOptionalBoundedText(value: unknown, maxCharacters: number): boolean {
  return value === undefined || isBoundedText(value, maxCharacters);
}

function isNullableString(value: unknown): boolean {
  return value === undefined || value === null || typeof value === "string";
}

function isOptionalId(value: unknown): boolean {
  return value === null || typeof value === "string";
}

function normalizeInstructions(value: string | null | undefined): string {
  return Array.from(value ?? "")
    .filter((character) => character === "\n" || character === "\t" || !/\p{Cc}/u.test(character))
    .join("")
    .trim();
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
