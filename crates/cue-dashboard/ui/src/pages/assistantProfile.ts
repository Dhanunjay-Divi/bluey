export const ASSISTANT_PROFILE_SCHEMA_VERSION = 1;
export const MAX_ASSISTANT_ROLE_CHARS = 200;
export const MAX_ASSISTANT_COMPANY_CHARS = 200;
export const MAX_ASSISTANT_INSTRUCTIONS_CHARS = 4_000;
export const MAX_PRIORITY_QUESTIONS = 12;
export const MAX_PRIORITY_QUESTION_CHARS = 500;

export const ASSISTANT_MODES = [
  "general",
  "interview",
  "behavioral_interview",
  "coding",
  "system_design",
  "meeting",
  "writing",
] as const;

export type AssistantMode = (typeof ASSISTANT_MODES)[number];

export interface AssistantSourceReference {
  application_id?: string;
  receipt_id?: string;
  resume_version_id?: string;
  receipt_fingerprint?: string;
}

export interface AssistantProfile {
  schema_version: number;
  mode: AssistantMode;
  target_role: string | null;
  company: string | null;
  custom_instructions: string | null;
  priority_questions: string[];
  source?: AssistantSourceReference | null;
}

export interface AssistantModeOption {
  value: AssistantMode;
  label: string;
  shortLabel: string;
  description: string;
  guidance: string;
}

export const ASSISTANT_MODE_OPTIONS: readonly AssistantModeOption[] = [
  {
    value: "general",
    label: "General",
    shortLabel: "General coach",
    description: "Flexible help for conversations, research, and everyday work.",
    guidance: "Bluey stays adaptable and follows the context and instructions you provide.",
  },
  {
    value: "interview",
    label: "Interview",
    shortLabel: "Interview coach",
    description: "Role-aware answers, likely follow-ups, and concise talking points.",
    guidance: "Add the role and company so answers can use the right seniority and business context.",
  },
  {
    value: "behavioral_interview",
    label: "Behavioral",
    shortLabel: "Behavioral coach",
    description: "Grounded story structure, outcomes, and thoughtful follow-ups.",
    guidance: "Use priority questions for the stories or competencies you want close at hand.",
  },
  {
    value: "coding",
    label: "Coding",
    shortLabel: "Coding coach",
    description: "Approach, implementation, trade-offs, walkthrough, and tests.",
    guidance: "Put language, constraints, and explanation style in custom instructions.",
  },
  {
    value: "system_design",
    label: "System design",
    shortLabel: "System design coach",
    description: "Requirements, estimates, architecture, trade-offs, and deep dives.",
    guidance: "Add scale assumptions or areas to emphasize, such as data modeling or reliability.",
  },
  {
    value: "meeting",
    label: "Meeting",
    shortLabel: "Meeting coach",
    description: "Discussion support, decisions, objections, and clear next steps.",
    guidance: "Describe your role and desired outcome so suggestions fit the room.",
  },
  {
    value: "writing",
    label: "Writing",
    shortLabel: "Writing coach",
    description: "Draft and revise with your voice, audience, and constraints.",
    guidance: "Describe the audience, tone, and format you want Bluey to preserve.",
  },
];

export const EMPTY_ASSISTANT_PROFILE: AssistantProfile = {
  schema_version: ASSISTANT_PROFILE_SCHEMA_VERSION,
  mode: "general",
  target_role: null,
  company: null,
  custom_instructions: null,
  priority_questions: [],
  source: null,
};

export type AssistantProfileErrors = Record<string, string>;

/** Mirrors the Rust boundary, including single-line role/question normalization. */
export function normalizeAssistantProfile(profile: AssistantProfile): AssistantProfile {
  return {
    schema_version: profile.schema_version,
    mode: profile.mode,
    target_role: normalizeSingleLineOptional(profile.target_role),
    company: normalizeSingleLineOptional(profile.company),
    custom_instructions: normalizeOptional(profile.custom_instructions),
    priority_questions: profile.priority_questions
      .map((question) => normalizeSingleLine(question))
      .filter((question) => question.length > 0),
    source: profile.source ?? null,
  };
}

/** Validates the editable draft without silently discarding blank question rows. */
export function validateAssistantProfile(profile: AssistantProfile): AssistantProfileErrors {
  const errors: AssistantProfileErrors = {};

  if (profile.schema_version !== ASSISTANT_PROFILE_SCHEMA_VERSION) {
    errors.schema_version = "This coach setup uses an unsupported schema version.";
  }
  if (!ASSISTANT_MODES.includes(profile.mode)) {
    errors.mode = "Choose a valid coach mode.";
  }
  if (charCount(normalizeSingleLine(profile.target_role ?? "")) > MAX_ASSISTANT_ROLE_CHARS) {
    errors.target_role = `Role must be ${MAX_ASSISTANT_ROLE_CHARS} characters or fewer.`;
  }
  if (charCount(normalizeSingleLine(profile.company ?? "")) > MAX_ASSISTANT_COMPANY_CHARS) {
    errors.company = `Company must be ${MAX_ASSISTANT_COMPANY_CHARS} characters or fewer.`;
  }
  if (charCount(normalizeText(profile.custom_instructions ?? "")) > MAX_ASSISTANT_INSTRUCTIONS_CHARS) {
    errors.custom_instructions = `Instructions must be ${MAX_ASSISTANT_INSTRUCTIONS_CHARS.toLocaleString()} characters or fewer.`;
  }
  if (profile.priority_questions.length > MAX_PRIORITY_QUESTIONS) {
    errors.priority_questions = `Add no more than ${MAX_PRIORITY_QUESTIONS} priority questions.`;
  }

  profile.priority_questions.forEach((question, index) => {
    const key = priorityQuestionErrorKey(index);
    if (!normalizeSingleLine(question)) {
      errors[key] = "Enter a question or remove this row.";
    } else if (charCount(normalizeSingleLine(question)) > MAX_PRIORITY_QUESTION_CHARS) {
      errors[key] = `Question must be ${MAX_PRIORITY_QUESTION_CHARS} characters or fewer.`;
    }
  });

  return errors;
}

export function priorityQuestionErrorKey(index: number): string {
  return `priority_questions.${index}`;
}

export function hasAssistantProfileErrors(errors: AssistantProfileErrors): boolean {
  return Object.keys(errors).length > 0;
}

export function assistantProfilesEqual(left: AssistantProfile, right: AssistantProfile): boolean {
  return JSON.stringify(normalizeAssistantProfile(left)) === JSON.stringify(normalizeAssistantProfile(right));
}

export function charCount(value: string): number {
  return Array.from(value).length;
}

function normalizeOptional(value: string | null | undefined): string | null {
  const normalized = normalizeText(value ?? "");
  return normalized || null;
}

function normalizeSingleLineOptional(value: string | null | undefined): string | null {
  const normalized = normalizeSingleLine(value ?? "");
  return normalized || null;
}

function normalizeText(value: string): string {
  return Array.from(value)
    .filter((character) => character === "\n" || character === "\t" || !isControlCharacter(character))
    .join("")
    .trim();
}

function normalizeSingleLine(value: string): string {
  return Array.from(value)
    .filter((character) => !isControlCharacter(character) || isWhitespace(character))
    .join("")
    .split(/\s+/u)
    .filter(Boolean)
    .join(" ");
}

function isControlCharacter(character: string): boolean {
  return /\p{Cc}/u.test(character);
}

function isWhitespace(character: string): boolean {
  return /\s/u.test(character);
}
