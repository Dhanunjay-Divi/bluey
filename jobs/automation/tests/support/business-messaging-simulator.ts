import { createHash } from "node:crypto";

export const BUSINESS_MESSAGING_SIMULATOR_VERSION = 1 as const;
export const BUSINESS_MESSAGING_GRAMMAR_VERSION =
  "business_messaging_command_grammar_v1" as const;
export const WHATSAPP_STATUS_EVIDENCE_REVISION =
  "rev_test_whatsapp_status_schema_v1" as const;
export const WHATSAPP_GATEWAY_EVIDENCE_REVISION =
  "rev_test_whatsapp_gateway_v1" as const;
export const WHATSAPP_CLOSE_EVIDENCE_REVISION =
  "rev_test_whatsapp_close_schema_v1" as const;
export const APPLE_GATEWAY_EVIDENCE_REVISION =
  "rev_test_apple_gateway_v1" as const;
export const APPLE_CLOSE_EVIDENCE_REVISION =
  "rev_test_apple_close_schema_v1" as const;

export type BusinessMessagingProvider =
  | "whatsapp_business_platform"
  | "apple_messages_for_business";

export type PersonalMessagingChannel =
  | "whatsapp"
  | "whatsapp_personal"
  | "personal_whatsapp"
  | "whatsapp_web"
  | "whatsapp_qr_session"
  | "qr_device_session"
  | "imessage"
  | "personal_imessage"
  | "sms"
  | "background_sms";

export type BusinessMessagingCommandKind =
  | "help"
  | "status"
  | "matches"
  | "show"
  | "save"
  | "pass"
  | "prepare"
  | "review"
  | "approve"
  | "cancel"
  | "pause"
  | "stop";

export type BusinessMessagingPassReason =
  | "NOT_RELEVANT"
  | "LOCATION"
  | "COMPENSATION"
  | "SENIORITY"
  | "EMPLOYMENT_TYPE"
  | "OTHER_ROLE";

export type BusinessMessagingRejectionCode =
  | "invalid_input"
  | "unsupported"
  | "personal_whatsapp_unsupported"
  | "personal_imessage_background_unsupported"
  | "non_synthetic_identifier"
  | "provider_policy_mismatch"
  | "authority_denied"
  | "suppressed"
  | "plan_reference_mismatch"
  | "action_truth_ceiling"
  | "provider_truth_ceiling";

export interface BusinessMessagingReadSetV1 {
  connection_revision: string;
  consent_revision: string;
  opt_out_parser_revision: string;
  command_parser_revision: string;
  locale_table_revision: string;
  provider_policy_revision: string;
  provider_eligibility_revision: string;
  jobs_workspace_revision: string;
  career_track_revision: string;
  original_source_revision: string;
  job_integrity_revision: string;
  adapter_release_revision: string;
  kill_switch_revision: string;
}

export interface BusinessMessagingAuthorityV1 {
  connection_active: boolean;
  consent_active: boolean;
  provider_eligible: boolean;
  track_eligible: boolean;
  source_verified: boolean;
  integrity_verified: boolean;
  suppression_active: boolean;
  kill_switch_active: boolean;
}

export interface BusinessMessagingProviderPolicyV1 {
  mode: "whatsapp_customer_service_window" | "apple_active_conversation";
  status_evidence_revision: string;
}

export type BusinessMessagingScriptedOutcome =
  | "simulated_no_effect"
  | "provider_accepted"
  | "whatsapp_delivered"
  | "whatsapp_read"
  | "failed_pre_request"
  | "timeout_after_request_start"
  | "provider_closed";

export interface BusinessMessagingSimulatorInputV1 {
  version: 1;
  provider: BusinessMessagingProvider;
  command: string;
  now_ms: number;
  plan_id: string;
  plan_revision: number;
  account_id: string;
  connection_id: string;
  business_endpoint_id: string;
  provider_subject_id: string;
  read_set: BusinessMessagingReadSetV1;
  authority: BusinessMessagingAuthorityV1;
  provider_policy: BusinessMessagingProviderPolicyV1;
  scripted_outcome: BusinessMessagingScriptedOutcome;
}

export interface CanonicalCommandV1 {
  version: 1;
  grammar_version: typeof BUSINESS_MESSAGING_GRAMMAR_VERSION;
  kind: BusinessMessagingCommandKind;
  normalized: string;
  arguments: {
    job_ref: string | null;
    limit: number | null;
    pass_reason: BusinessMessagingPassReason | null;
    plan_ref: string | null;
  };
}

export interface CanonicalPlanV1 {
  version: 1;
  account_id: string;
  action: "save" | "pass" | "prepare";
  authority: BusinessMessagingAuthorityV1;
  business_endpoint_id: string;
  command_sha256: string;
  connection_id: string;
  expires_at_ms: number;
  plan_id: string;
  plan_revision: number;
  provider: BusinessMessagingProvider;
  provider_policy: BusinessMessagingProviderPolicyV1;
  provider_subject_id: string;
  read_set: BusinessMessagingReadSetV1;
  required_step_up: "none" | "authenticated_web";
  state: "simulated";
}

export interface CanonicalOperationV1 {
  version: 1;
  action: "save" | "pass" | "prepare";
  adapter_release_revision: string;
  operation_key_sha256: string;
  plan_id: string;
  plan_revision: number;
  plan_sha256: string;
  provider: BusinessMessagingProvider;
  provider_policy_revision: string;
  request_state: "request_not_started" | "request_started";
  scripted_outcome: BusinessMessagingScriptedOutcome;
}

export interface ZeroEffectAuditV1 {
  browser_attempts: 0;
  credential_reads: 0;
  external_writes: 0;
  jobs_mutations: 0;
  network_attempts: 0;
  process_attempts: 0;
  provider_attempts: 0;
}

export type BusinessMessagingReceiptAssertion =
  | "simulated_no_effect"
  | "step_up_required"
  | "stopped"
  | "paused"
  | "denied_pre_effect"
  | "provider_accepted"
  | "delivered"
  | "read"
  | "failed_pre_effect"
  | "side_effect_unknown"
  | "provider_closed";

export interface CanonicalReceiptV1 {
  version: 1;
  assertion: BusinessMessagingReceiptAssertion;
  command_sha256: string;
  operation_sha256: string | null;
  plan_sha256: string | null;
  provider: BusinessMessagingProvider;
  provider_evidence_revision: string | null;
  request_started: boolean;
  retry_allowed: boolean;
  simulated: true;
  terminal_at_ms: number;
  zero_effect_audit: ZeroEffectAuditV1;
}

export interface CanonicalArtifact<T> {
  value: T;
  canonical: string;
  sha256: string;
}

export interface BusinessMessagingSimulationSuccess {
  ok: true;
  command: CanonicalArtifact<CanonicalCommandV1>;
  plan: CanonicalArtifact<CanonicalPlanV1> | null;
  operation: CanonicalArtifact<CanonicalOperationV1> | null;
  receipt: CanonicalArtifact<CanonicalReceiptV1>;
}

export interface BusinessMessagingSimulationRejection {
  ok: false;
  code: BusinessMessagingRejectionCode;
}

export type BusinessMessagingSimulationResult =
  | BusinessMessagingSimulationSuccess
  | BusinessMessagingSimulationRejection;

export interface ParsedBusinessMessagingCommand {
  ok: true;
  command: CanonicalCommandV1;
}

export interface RejectedBusinessMessagingCommand {
  ok: false;
  code: "invalid_input" | "unsupported";
}

const INPUT_KEYS = [
  "version",
  "provider",
  "command",
  "now_ms",
  "plan_id",
  "plan_revision",
  "account_id",
  "connection_id",
  "business_endpoint_id",
  "provider_subject_id",
  "read_set",
  "authority",
  "provider_policy",
  "scripted_outcome",
] as const;

const READ_SET_KEYS = [
  "connection_revision",
  "consent_revision",
  "opt_out_parser_revision",
  "command_parser_revision",
  "locale_table_revision",
  "provider_policy_revision",
  "provider_eligibility_revision",
  "jobs_workspace_revision",
  "career_track_revision",
  "original_source_revision",
  "job_integrity_revision",
  "adapter_release_revision",
  "kill_switch_revision",
] as const;

const AUTHORITY_KEYS = [
  "connection_active",
  "consent_active",
  "provider_eligible",
  "track_eligible",
  "source_verified",
  "integrity_verified",
  "suppression_active",
  "kill_switch_active",
] as const;

const PROVIDER_POLICY_KEYS = ["mode", "status_evidence_revision"] as const;
const JOB_REF = /^J-[0-9A-HJ-NP-TV-Z]{10,26}$/;
const PLAN_REF = /^P-[0-9A-HJ-NP-TV-Z]{10,26}$/;
const SYNTHETIC_ACCOUNTS = new Set(["acct_test_owner", "acct_test_alternate"]);
const SYNTHETIC_CONNECTIONS = new Set(["conn_test_primary", "conn_test_secondary"]);
const SYNTHETIC_ENDPOINTS = new Set([
  "endpoint_test_whatsapp",
  "endpoint_test_apple",
  "endpoint_test_alternate",
]);
const SYNTHETIC_SUBJECTS = new Set(["subject_test_owner", "subject_test_alternate"]);
const SYNTHETIC_STATUS_EVIDENCE_REVISIONS = new Set([
  WHATSAPP_STATUS_EVIDENCE_REVISION,
  WHATSAPP_GATEWAY_EVIDENCE_REVISION,
  WHATSAPP_CLOSE_EVIDENCE_REVISION,
  APPLE_GATEWAY_EVIDENCE_REVISION,
  APPLE_CLOSE_EVIDENCE_REVISION,
  "rev_test_unpinned_status_v1",
]);
const READ_SET_REVISION_STEMS: Record<keyof BusinessMessagingReadSetV1, string> = {
  connection_revision: "connection",
  consent_revision: "consent",
  opt_out_parser_revision: "opt_out",
  command_parser_revision: "command",
  locale_table_revision: "locale",
  provider_policy_revision: "provider_policy",
  provider_eligibility_revision: "provider_eligibility",
  jobs_workspace_revision: "workspace",
  career_track_revision: "track",
  original_source_revision: "source",
  job_integrity_revision: "integrity",
  adapter_release_revision: "adapter",
  kill_switch_revision: "kill_switch",
};

const PASS_REASONS = new Set<BusinessMessagingPassReason>([
  "NOT_RELEVANT",
  "LOCATION",
  "COMPENSATION",
  "SENIORITY",
  "EMPLOYMENT_TYPE",
  "OTHER_ROLE",
]);

const BUSINESS_PROVIDERS = new Set<BusinessMessagingProvider>([
  "whatsapp_business_platform",
  "apple_messages_for_business",
]);

const SCRIPTED_OUTCOMES = new Set<BusinessMessagingScriptedOutcome>([
  "simulated_no_effect",
  "provider_accepted",
  "whatsapp_delivered",
  "whatsapp_read",
  "failed_pre_request",
  "timeout_after_request_start",
  "provider_closed",
]);

const ZERO_EFFECT_AUDIT: ZeroEffectAuditV1 = Object.freeze({
  browser_attempts: 0,
  credential_reads: 0,
  external_writes: 0,
  jobs_mutations: 0,
  network_attempts: 0,
  process_attempts: 0,
  provider_attempts: 0,
});

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (!isObject(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

function isSafePositiveInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}

function isBusinessProvider(value: unknown): value is BusinessMessagingProvider {
  return typeof value === "string" && BUSINESS_PROVIDERS.has(value as BusinessMessagingProvider);
}

function isScriptedOutcome(value: unknown): value is BusinessMessagingScriptedOutcome {
  return typeof value === "string" && SCRIPTED_OUTCOMES.has(value as BusinessMessagingScriptedOutcome);
}

function isReadSet(value: unknown): value is BusinessMessagingReadSetV1 {
  return (
    hasExactKeys(value, READ_SET_KEYS) &&
    READ_SET_KEYS.every(
      (key) =>
        typeof value[key] === "string" &&
        new RegExp(`^rev_test_${READ_SET_REVISION_STEMS[key]}_v[1-9][0-9]{0,2}$`).test(value[key]),
    )
  );
}

function isAuthority(value: unknown): value is BusinessMessagingAuthorityV1 {
  return hasExactKeys(value, AUTHORITY_KEYS) && AUTHORITY_KEYS.every((key) => typeof value[key] === "boolean");
}

function isProviderPolicy(value: unknown): value is BusinessMessagingProviderPolicyV1 {
  return (
    hasExactKeys(value, PROVIDER_POLICY_KEYS) &&
    (value.mode === "whatsapp_customer_service_window" || value.mode === "apple_active_conversation") &&
    typeof value.status_evidence_revision === "string" &&
    SYNTHETIC_STATUS_EVIDENCE_REVISIONS.has(value.status_evidence_revision)
  );
}

function personalChannelRejection(value: unknown): BusinessMessagingRejectionCode | undefined {
  switch (value) {
    case "whatsapp":
    case "whatsapp_personal":
    case "personal_whatsapp":
    case "whatsapp_web":
    case "whatsapp_qr_session":
    case "qr_device_session":
      return "personal_whatsapp_unsupported";
    case "imessage":
    case "personal_imessage":
    case "sms":
    case "background_sms":
      return "personal_imessage_background_unsupported";
    default:
      return undefined;
  }
}

function validateInput(value: unknown): BusinessMessagingSimulatorInputV1 | BusinessMessagingSimulationRejection {
  if (isObject(value)) {
    const personalRejection = personalChannelRejection(value.provider);
    if (personalRejection !== undefined) return { ok: false, code: personalRejection };
  }
  if (!hasExactKeys(value, INPUT_KEYS)) return { ok: false, code: "invalid_input" };
  if (
    value.version !== BUSINESS_MESSAGING_SIMULATOR_VERSION ||
    !isBusinessProvider(value.provider) ||
    typeof value.command !== "string" ||
    !isSafePositiveInteger(value.now_ms) ||
    typeof value.plan_id !== "string" ||
    !PLAN_REF.test(value.plan_id) ||
    !isSafePositiveInteger(value.plan_revision) ||
    typeof value.account_id !== "string" ||
    typeof value.connection_id !== "string" ||
    typeof value.business_endpoint_id !== "string" ||
    typeof value.provider_subject_id !== "string" ||
    !isReadSet(value.read_set) ||
    !isAuthority(value.authority) ||
    !isProviderPolicy(value.provider_policy) ||
    !isScriptedOutcome(value.scripted_outcome)
  ) {
    return { ok: false, code: "invalid_input" };
  }
  if (
    !SYNTHETIC_ACCOUNTS.has(value.account_id) ||
    !SYNTHETIC_CONNECTIONS.has(value.connection_id) ||
    !SYNTHETIC_ENDPOINTS.has(value.business_endpoint_id) ||
    !SYNTHETIC_SUBJECTS.has(value.provider_subject_id)
  ) {
    return { ok: false, code: "non_synthetic_identifier" };
  }
  return value as unknown as BusinessMessagingSimulatorInputV1;
}

function sortedJsonValue(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortedJsonValue);
  if (!isObject(value)) return value;
  const sorted: Record<string, unknown> = {};
  for (const key of Object.keys(value).sort()) sorted[key] = sortedJsonValue(value[key]);
  return sorted;
}

export function canonicalBusinessMessagingJson(value: unknown): string {
  return `${JSON.stringify(sortedJsonValue(value))}\n`;
}

export function businessMessagingSha256(canonical: string): string {
  return createHash("sha256").update(canonical, "utf8").digest("hex");
}

function artifact<T>(value: T): CanonicalArtifact<T> {
  const canonical = canonicalBusinessMessagingJson(value);
  return Object.freeze({ value: deepFreeze(value), canonical, sha256: businessMessagingSha256(canonical) });
}

function deepFreeze<T>(value: T): T {
  if (isObject(value) || Array.isArray(value)) {
    for (const nested of Object.values(value)) deepFreeze(nested);
    Object.freeze(value);
  }
  return value;
}

function commandValue(
  kind: BusinessMessagingCommandKind,
  normalized: string,
  values: Partial<CanonicalCommandV1["arguments"]> = {},
): CanonicalCommandV1 {
  return {
    version: BUSINESS_MESSAGING_SIMULATOR_VERSION,
    grammar_version: BUSINESS_MESSAGING_GRAMMAR_VERSION,
    kind,
    normalized,
    arguments: {
      job_ref: values.job_ref ?? null,
      limit: values.limit ?? null,
      pass_reason: values.pass_reason ?? null,
      plan_ref: values.plan_ref ?? null,
    },
  };
}

export function parseBusinessMessagingCommand(
  value: unknown,
): ParsedBusinessMessagingCommand | RejectedBusinessMessagingCommand {
  if (typeof value !== "string") return { ok: false, code: "invalid_input" };
  if (value.length === 0 || value.length > 256 || !/^[\x20-\x7e]+$/.test(value)) {
    return { ok: false, code: "unsupported" };
  }

  if (/^STOP$/i.test(value)) return { ok: true, command: commandValue("stop", "STOP") };

  const [verb, ...argumentsList] = value.split(" ");
  const upperVerb = verb.toUpperCase();
  if (verb.length === 0 || !/^[A-Za-z]+$/.test(verb)) return { ok: false, code: "unsupported" };

  if (argumentsList.length === 0) {
    switch (upperVerb) {
      case "HELP":
        return { ok: true, command: commandValue("help", "HELP") };
      case "STATUS":
        return { ok: true, command: commandValue("status", "STATUS") };
      case "MATCHES":
        return { ok: true, command: commandValue("matches", "MATCHES") };
      case "PAUSE":
        return { ok: true, command: commandValue("pause", "PAUSE") };
      default:
        return { ok: false, code: "unsupported" };
    }
  }

  if (upperVerb === "MATCHES" && argumentsList.length === 1 && /^[1-5]$/.test(argumentsList[0])) {
    const limit = Number(argumentsList[0]);
    return { ok: true, command: commandValue("matches", `MATCHES ${limit}`, { limit }) };
  }

  if (argumentsList.length === 1 && JOB_REF.test(argumentsList[0])) {
    const jobRef = argumentsList[0];
    switch (upperVerb) {
      case "SHOW":
        return { ok: true, command: commandValue("show", `SHOW ${jobRef}`, { job_ref: jobRef }) };
      case "SAVE":
        return { ok: true, command: commandValue("save", `SAVE ${jobRef}`, { job_ref: jobRef }) };
      case "PREPARE":
        return { ok: true, command: commandValue("prepare", `PREPARE ${jobRef}`, { job_ref: jobRef }) };
      default:
        break;
    }
  }

  if (argumentsList.length === 1 && PLAN_REF.test(argumentsList[0])) {
    const planRef = argumentsList[0];
    switch (upperVerb) {
      case "REVIEW":
        return { ok: true, command: commandValue("review", `REVIEW ${planRef}`, { plan_ref: planRef }) };
      case "APPROVE":
        return { ok: true, command: commandValue("approve", `APPROVE ${planRef}`, { plan_ref: planRef }) };
      case "CANCEL":
        return { ok: true, command: commandValue("cancel", `CANCEL ${planRef}`, { plan_ref: planRef }) };
      default:
        break;
    }
  }

  if (
    upperVerb === "PASS" &&
    argumentsList.length === 2 &&
    JOB_REF.test(argumentsList[0]) &&
    PASS_REASONS.has(argumentsList[1] as BusinessMessagingPassReason)
  ) {
    const jobRef = argumentsList[0];
    const passReason = argumentsList[1] as BusinessMessagingPassReason;
    return {
      ok: true,
      command: commandValue("pass", `PASS ${jobRef} ${passReason}`, {
        job_ref: jobRef,
        pass_reason: passReason,
      }),
    };
  }

  return { ok: false, code: "unsupported" };
}

function providerPolicyMatches(input: BusinessMessagingSimulatorInputV1): boolean {
  if (input.provider === "whatsapp_business_platform") {
    return (
      input.provider_policy.mode === "whatsapp_customer_service_window" &&
      (input.provider_policy.status_evidence_revision === WHATSAPP_STATUS_EVIDENCE_REVISION ||
        input.provider_policy.status_evidence_revision === WHATSAPP_GATEWAY_EVIDENCE_REVISION ||
        input.provider_policy.status_evidence_revision === WHATSAPP_CLOSE_EVIDENCE_REVISION ||
        input.provider_policy.status_evidence_revision === "rev_test_unpinned_status_v1")
    );
  }
  return (
    input.provider_policy.mode === "apple_active_conversation" &&
    (input.provider_policy.status_evidence_revision === APPLE_GATEWAY_EVIDENCE_REVISION ||
      input.provider_policy.status_evidence_revision === APPLE_CLOSE_EVIDENCE_REVISION)
  );
}

function allPositiveAuthority(input: BusinessMessagingSimulatorInputV1): boolean {
  const authority = input.authority;
  return (
    authority.connection_active &&
    authority.consent_active &&
    authority.provider_eligible &&
    authority.track_eligible &&
    authority.source_verified &&
    authority.integrity_verified &&
    !authority.suppression_active &&
    !authority.kill_switch_active
  );
}

function makeReceipt(
  input: BusinessMessagingSimulatorInputV1,
  commandSha256: string,
  assertion: BusinessMessagingReceiptAssertion,
  planSha256: string | null,
  operationSha256: string | null,
  requestStarted: boolean,
  retryAllowed: boolean,
  providerEvidenceRevision: string | null,
): CanonicalArtifact<CanonicalReceiptV1> {
  return artifact({
    version: BUSINESS_MESSAGING_SIMULATOR_VERSION,
    assertion,
    command_sha256: commandSha256,
    operation_sha256: operationSha256,
    plan_sha256: planSha256,
    provider: input.provider,
    provider_evidence_revision: providerEvidenceRevision,
    request_started: requestStarted,
    retry_allowed: retryAllowed,
    simulated: true,
    terminal_at_ms: input.now_ms,
    zero_effect_audit: ZERO_EFFECT_AUDIT,
  });
}

function noPlanSuccess(
  input: BusinessMessagingSimulatorInputV1,
  command: CanonicalArtifact<CanonicalCommandV1>,
  assertion: BusinessMessagingReceiptAssertion,
): BusinessMessagingSimulationSuccess {
  return {
    ok: true,
    command,
    plan: null,
    operation: null,
    receipt: makeReceipt(input, command.sha256, assertion, null, null, false, false, null),
  };
}

function planAction(
  command: CanonicalCommandV1,
): "save" | "pass" | "prepare" | undefined {
  switch (command.kind) {
    case "save":
    case "pass":
    case "prepare":
      return command.kind;
    default:
      return undefined;
  }
}

function outcomeProjection(input: BusinessMessagingSimulatorInputV1): {
  assertion: BusinessMessagingReceiptAssertion;
  requestStarted: boolean;
  retryAllowed: boolean;
  providerEvidenceRevision: string | null;
} | BusinessMessagingSimulationRejection {
  switch (input.scripted_outcome) {
    case "simulated_no_effect":
      return {
        assertion: "simulated_no_effect",
        requestStarted: false,
        retryAllowed: false,
        providerEvidenceRevision: null,
      };
    case "failed_pre_request":
      return {
        assertion: "failed_pre_effect",
        requestStarted: false,
        retryAllowed: true,
        providerEvidenceRevision: null,
      };
    case "timeout_after_request_start":
      return {
        assertion: "side_effect_unknown",
        requestStarted: true,
        retryAllowed: false,
        providerEvidenceRevision: null,
      };
    case "provider_closed":
      if (
        input.provider_policy.status_evidence_revision !==
        (input.provider === "whatsapp_business_platform"
          ? WHATSAPP_CLOSE_EVIDENCE_REVISION
          : APPLE_CLOSE_EVIDENCE_REVISION)
      ) {
        return { ok: false, code: "provider_truth_ceiling" };
      }
      return {
        assertion: "provider_closed",
        requestStarted: true,
        retryAllowed: false,
        providerEvidenceRevision: input.provider_policy.status_evidence_revision,
      };
    case "provider_accepted":
      if (
        input.provider_policy.status_evidence_revision !==
        (input.provider === "whatsapp_business_platform"
          ? WHATSAPP_GATEWAY_EVIDENCE_REVISION
          : APPLE_GATEWAY_EVIDENCE_REVISION)
      ) {
        return { ok: false, code: "provider_truth_ceiling" };
      }
      return {
        assertion: "provider_accepted",
        requestStarted: true,
        retryAllowed: false,
        providerEvidenceRevision: input.provider_policy.status_evidence_revision,
      };
    case "whatsapp_delivered":
      return input.provider === "whatsapp_business_platform" &&
        input.provider_policy.status_evidence_revision === WHATSAPP_STATUS_EVIDENCE_REVISION
        ? {
            assertion: "delivered",
            requestStarted: true,
            retryAllowed: false,
            providerEvidenceRevision: input.provider_policy.status_evidence_revision,
          }
        : { ok: false, code: "provider_truth_ceiling" };
    case "whatsapp_read":
      return input.provider === "whatsapp_business_platform" &&
        input.provider_policy.status_evidence_revision === WHATSAPP_STATUS_EVIDENCE_REVISION
        ? {
            assertion: "read",
            requestStarted: true,
            retryAllowed: false,
            providerEvidenceRevision: input.provider_policy.status_evidence_revision,
          }
        : { ok: false, code: "provider_truth_ceiling" };
  }
}

export function simulateBusinessMessaging(value: unknown): BusinessMessagingSimulationResult {
  const validated = validateInput(value);
  if ("ok" in validated) return validated;
  const input = validated;

  const parsed = parseBusinessMessagingCommand(input.command);
  if (!parsed.ok) return { ok: false, code: parsed.code };
  const command = artifact(parsed.command);

  if (parsed.command.kind === "stop") return noPlanSuccess(input, command, "stopped");
  if (!providerPolicyMatches(input)) return { ok: false, code: "provider_policy_mismatch" };
  if (input.authority.suppression_active) return { ok: false, code: "suppressed" };
  if (input.authority.kill_switch_active) return { ok: false, code: "authority_denied" };

  if (
    parsed.command.kind === "approve" ||
    parsed.command.kind === "review" ||
    parsed.command.kind === "cancel"
  ) {
    if (parsed.command.arguments.plan_ref !== input.plan_id) {
      return { ok: false, code: "plan_reference_mismatch" };
    }
    if (parsed.command.kind === "approve") return noPlanSuccess(input, command, "step_up_required");
    if (parsed.command.kind === "cancel") return noPlanSuccess(input, command, "denied_pre_effect");
    return noPlanSuccess(input, command, "simulated_no_effect");
  }

  if (parsed.command.kind === "pause") return noPlanSuccess(input, command, "paused");

  const action = planAction(parsed.command);
  if (action === undefined) return noPlanSuccess(input, command, "simulated_no_effect");
  if (!allPositiveAuthority(input)) return { ok: false, code: "authority_denied" };
  if (action === "prepare" && input.scripted_outcome !== "simulated_no_effect") {
    return { ok: false, code: "action_truth_ceiling" };
  }
  if (input.now_ms > Number.MAX_SAFE_INTEGER - 300_000) {
    return { ok: false, code: "invalid_input" };
  }

  const plan = artifact<CanonicalPlanV1>({
    version: BUSINESS_MESSAGING_SIMULATOR_VERSION,
    account_id: input.account_id,
    action,
    authority: { ...input.authority },
    business_endpoint_id: input.business_endpoint_id,
    command_sha256: command.sha256,
    connection_id: input.connection_id,
    expires_at_ms: input.now_ms + 300_000,
    plan_id: input.plan_id,
    plan_revision: input.plan_revision,
    provider: input.provider,
    provider_policy: { ...input.provider_policy },
    provider_subject_id: input.provider_subject_id,
    read_set: { ...input.read_set },
    required_step_up: action === "prepare" ? "authenticated_web" : "none",
    state: "simulated",
  });

  const outcome = outcomeProjection(input);
  if ("ok" in outcome) return outcome;

  const operationKeyProjection = {
    account_id: input.account_id,
    action,
    adapter_release_revision: input.read_set.adapter_release_revision,
    business_endpoint_id: input.business_endpoint_id,
    connection_revision: input.read_set.connection_revision,
    consent_revision: input.read_set.consent_revision,
    plan_id: input.plan_id,
    plan_revision: input.plan_revision,
    plan_sha256: plan.sha256,
    provider: input.provider,
    provider_policy_revision: input.read_set.provider_policy_revision,
    provider_subject_id: input.provider_subject_id,
  };
  const operationKeySha256 = businessMessagingSha256(canonicalBusinessMessagingJson(operationKeyProjection));
  const operation = artifact<CanonicalOperationV1>({
    version: BUSINESS_MESSAGING_SIMULATOR_VERSION,
    action,
    adapter_release_revision: input.read_set.adapter_release_revision,
    operation_key_sha256: operationKeySha256,
    plan_id: input.plan_id,
    plan_revision: input.plan_revision,
    plan_sha256: plan.sha256,
    provider: input.provider,
    provider_policy_revision: input.read_set.provider_policy_revision,
    request_state: outcome.requestStarted ? "request_started" : "request_not_started",
    scripted_outcome: input.scripted_outcome,
  });
  const receipt = makeReceipt(
    input,
    command.sha256,
    outcome.assertion,
    plan.sha256,
    operation.sha256,
    outcome.requestStarted,
    outcome.retryAllowed,
    outcome.providerEvidenceRevision,
  );
  return { ok: true, command, plan, operation, receipt };
}
