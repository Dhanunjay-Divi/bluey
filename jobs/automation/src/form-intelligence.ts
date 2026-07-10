import type { InterventionRequest } from "./contracts.js";

export type FormFieldType = "text" | "textarea" | "email" | "tel" | "url" | "select" | "radio" | "checkbox" | "file";
export type AnswerScope = "account" | "track" | "company";

export interface ApplicationFormField {
  id: string;
  label: string;
  name?: string;
  type: FormFieldType;
  required: boolean;
  options?: string[];
  accepts?: string[];
}

export interface ProfileFact {
  value: string;
  verified: boolean;
  source: "resume" | "user" | "bluey" | "integration";
}

export interface ApplicationAnswerProfile {
  facts: Record<string, ProfileFact | undefined>;
  resumePath: string;
  coverLetterPath?: string;
}

export interface AnswerMemoryEntry {
  key: string;
  value: string;
  scope: AnswerScope;
  scopeId?: string;
  confirmed: boolean;
}

export interface AnswerPlanningContext {
  trackId: string;
  companyId: string;
  autoSubmit: boolean;
}

export interface FormAction {
  fieldId: string;
  action: "fill" | "upload" | "skip" | "intervene";
  value?: string;
  source?: string;
  reason: string;
}

export interface FormPlan {
  actions: FormAction[];
  interventions: InterventionRequest[];
  canSubmit: boolean;
}

const FIELD_ALIASES: Record<string, string[]> = {
  full_name: ["full name", "legal name", "candidate name", "name"],
  first_name: ["first name", "given name"],
  last_name: ["last name", "family name", "surname"],
  email: ["email", "email address"],
  phone: ["phone", "phone number", "mobile", "mobile number"],
  current_location: ["current location", "location", "city", "city state"],
  country: ["country", "country of residence"],
  postal_code: ["postal code", "zip", "zip code"],
  linkedin_url: ["linkedin", "linkedin url", "linkedin profile"],
  github_url: ["github", "github url", "github profile"],
  portfolio_url: ["portfolio", "portfolio url", "personal website", "website"],
  current_employer: ["current employer", "most recent employer", "company"],
  current_title: ["current title", "job title", "current role"],
  work_authorization: ["work authorization", "authorized to work", "legally authorized"],
  sponsorship: ["require sponsorship", "need sponsorship", "visa sponsorship", "sponsorship"],
  salary_expectation: ["salary expectation", "desired salary", "compensation expectation", "expected compensation"],
  notice_period: ["notice period", "start date", "earliest start date", "available to start"],
};

const SENSITIVE_PATTERNS = [
  /\bgender\b/i,
  /\brace\b/i,
  /\bethnic/i,
  /\bveteran\b/i,
  /\bdisabilit/i,
  /date of birth|birth date|\bdob\b/i,
  /social security|\bssn\b/i,
  /sexual orientation/i,
  /religion/i,
];

export function planApplicationForm(
  fields: ApplicationFormField[],
  profile: ApplicationAnswerProfile,
  memory: AnswerMemoryEntry[],
  context: AnswerPlanningContext,
): FormPlan {
  const actions: FormAction[] = [];
  const interventions: InterventionRequest[] = [];

  for (const field of fields) {
    const combinedLabel = `${field.label} ${field.name ?? ""}`.trim();
    const normalizedLabel = normalize(combinedLabel);

    if (isResumeField(field)) {
      actions.push(uploadAction(field, profile.resumePath, "job-specific resume"));
      continue;
    }
    if (isCoverLetterField(field)) {
      if (profile.coverLetterPath) actions.push(uploadAction(field, profile.coverLetterPath, "job-specific cover letter"));
      else handleUnanswered(field, actions, interventions, "A cover letter is required but this packet does not have one.");
      continue;
    }

    if (isSensitive(normalizedLabel)) {
      const detail = "Bluey needs your answer to this sensitive question.";
      addIntervention(field, interventions, "sensitive_question", detail);
      actions.push(interventionAction(field, detail));
      continue;
    }

    const memoryAnswer = resolveMemory(normalizedLabel, memory, context);
    if (memoryAnswer) {
      actions.push(fillAction(field, matchOption(memoryAnswer.value, field.options), `answer memory:${memoryAnswer.scope}`));
      continue;
    }

    const factKey = matchFactKey(normalizedLabel);
    const fact = factKey ? profile.facts[factKey] : undefined;
    if (fact?.value) {
      if (context.autoSubmit && !fact.verified) {
        addIntervention(field, interventions, "missing_fact", "This answer has not been confirmed for Auto-submit.");
        actions.push(interventionAction(field, "Unconfirmed profile fact"));
      } else {
        actions.push(fillAction(field, matchOption(fact.value, field.options), `profile:${factKey}`));
      }
      continue;
    }

    handleUnanswered(field, actions, interventions, "Bluey does not have a reliable answer for this field.");
  }

  return { actions, interventions, canSubmit: interventions.length === 0 };
}

export function resolveMemory(
  fieldLabel: string,
  entries: AnswerMemoryEntry[],
  context: AnswerPlanningContext,
): AnswerMemoryEntry | undefined {
  const candidates = entries.filter((entry) => entry.confirmed && normalize(entry.key) === normalize(fieldLabel));
  return candidates.find((entry) => entry.scope === "company" && entry.scopeId === context.companyId)
    ?? candidates.find((entry) => entry.scope === "track" && entry.scopeId === context.trackId)
    ?? candidates.find((entry) => entry.scope === "account");
}

function matchFactKey(label: string): string | undefined {
  let best: { key: string; score: number } | undefined;
  for (const [key, aliases] of Object.entries(FIELD_ALIASES)) {
    for (const alias of aliases) {
      const normalizedAlias = normalize(alias);
      const score = label === normalizedAlias
        ? 100
        : normalizedAlias.length >= 6 && label.includes(normalizedAlias)
          ? normalizedAlias.length
          : 0;
      if (score && (!best || score > best.score)) best = { key, score };
    }
  }
  return best?.key;
}

function handleUnanswered(
  field: ApplicationFormField,
  actions: FormAction[],
  interventions: InterventionRequest[],
  detail: string,
  kind: InterventionRequest["kind"] = "unknown_question",
): void {
  if (!field.required) {
    actions.push({ fieldId: field.id, action: "skip", reason: "Optional field has no confirmed answer" });
    return;
  }
  addIntervention(field, interventions, kind, detail);
  actions.push(interventionAction(field, detail));
}

function addIntervention(
  field: ApplicationFormField,
  interventions: InterventionRequest[],
  kind: InterventionRequest["kind"],
  detail: string,
): void {
  interventions.push({
    kind,
    title: field.label || "Application question",
    detail,
    field: field.id,
    choices: field.options,
  });
}

function fillAction(field: ApplicationFormField, value: string, source: string): FormAction {
  return { fieldId: field.id, action: "fill", value, source, reason: "Confirmed answer available" };
}

function uploadAction(field: ApplicationFormField, value: string, source: string): FormAction {
  return { fieldId: field.id, action: "upload", value, source, reason: "Packet document available" };
}

function interventionAction(field: ApplicationFormField, reason: string): FormAction {
  return { fieldId: field.id, action: "intervene", reason };
}

function isResumeField(field: ApplicationFormField): boolean {
  return field.type === "file" && /resume|curriculum|\bcv\b/i.test(`${field.label} ${field.name ?? ""}`);
}

function isCoverLetterField(field: ApplicationFormField): boolean {
  return field.type === "file" && /cover letter/i.test(`${field.label} ${field.name ?? ""}`);
}

function isSensitive(label: string): boolean {
  return SENSITIVE_PATTERNS.some((pattern) => pattern.test(label));
}

function matchOption(value: string, options?: string[]): string {
  if (!options?.length) return value;
  const normalizedValue = normalize(value);
  const exact = options.find((option) => normalize(option) === normalizedValue);
  if (exact) return exact;
  const contained = options.find((option) => normalize(option).includes(normalizedValue) || normalizedValue.includes(normalize(option)));
  return contained ?? value;
}

function normalize(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}
