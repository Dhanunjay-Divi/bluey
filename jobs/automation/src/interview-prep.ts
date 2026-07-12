import type { ApplicationEvidenceRecord, ApplicationReceiptBundle } from "./receipts.js";
import { assertSubmissionReceiptComplete } from "./packet-guards.js";

export interface VerifiedCareerClaim {
  id: string;
  category?: string;
  label: string;
  statement: string;
  status: "confirmed" | "unverified" | "rejected";
  source?: string;
  tags?: string[];
}

export interface SubmittedResumeSnapshot {
  versionId: string;
  content: Record<string, unknown>;
  claims: VerifiedCareerClaim[];
}

export interface InterviewPrepInput {
  receipt: ApplicationReceiptBundle;
  resume: SubmittedResumeSnapshot;
  evidence?: ApplicationEvidenceRecord[];
  generatedAt?: string;
}

export interface PrepSource {
  id: string;
  kind: "job" | "resume_claim" | "application_answer" | "submission" | "interview_event";
  label: string;
  content: string;
}

export interface PrepQuestion {
  id: string;
  category: "introduction" | "motivation" | "role_requirement" | "truth_gap";
  question: string;
  reason: string;
  sourceIds: string[];
  claimIds: string[];
  needsCandidateInput: boolean;
}

export interface PrepCommitment {
  key: string;
  answer: string;
  sourceId: string;
}

export interface InterviewPrepPacket {
  schemaVersion: 1;
  prepId: string;
  generatedAt: string;
  applicationId: string;
  receiptId: string;
  resumeVersionId: string;
  company: string;
  title: string;
  location: string;
  submittedAt: string;
  interviewAt?: string;
  interviewLabel?: string;
  launchPrompt: string;
  resumeContent: Record<string, unknown>;
  sources: PrepSource[];
  commitments: PrepCommitment[];
  questions: PrepQuestion[];
  warnings: string[];
}

export const INTERVIEW_PREP_SYSTEM_PROMPT = [
  "You are Bluey's interview coach.",
  "Use only the submitted-application context supplied by Bluey Jobs.",
  "Never invent employers, projects, tools, metrics, responsibilities, or outcomes.",
  "Label an inference, call out a missing fact, and ask for a truthful example when evidence is absent.",
  "Start with a concise preparation brief, then coach one practice question at a time.",
].join(" ");

const SENSITIVE_ANSWER_KEY = /(?:^|[^a-z])(name|email|phone|mobile|address|street|zip|postal|birth|dob|age|gender|sex|pronoun|race|ethnicity|religion|marital|disability|veteran|military|ssn|social security|national id|salary|compensation|pay|authorization|sponsorship|citizenship|immigration)(?:[^a-z]|$)/i;
const SENSITIVE_VALUE = /(?:\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b|\b(?:\+?\d[\s().-]*){9,}\d\b)/i;
const PRIVATE_RESUME_KEY = /^(contact|contact_info|email|phone|mobile|address|street|postal|zip|date_of_birth|dob)$/i;
const STOP_WORDS = new Set([
  "about", "after", "again", "also", "and", "are", "been", "being", "but", "can", "company", "could",
  "from", "have", "into", "more", "our", "role", "that", "the", "their", "this", "through", "using", "will",
  "with", "work", "you", "your", "years",
]);

export function buildInterviewPrepPacket(input: InterviewPrepInput): InterviewPrepPacket {
  const { receipt, resume } = input;
  assertSubmissionReceiptComplete(receipt);
  if (receipt.result.status !== "submitted") {
    throw new Error("Interview preparation requires a confirmed submitted application");
  }
  if (resume.versionId !== receipt.packet.resumeVersionId) {
    throw new Error("Interview preparation resume does not match the submitted resume version");
  }

  const verifiedClaims = resolveVerifiedClaims(receipt, resume.claims);
  const roleSignals = extractRoleSignals(receipt.job.description);
  const evidence = (input.evidence ?? [])
    .filter((item) => item.application_id === receipt.applicationId)
    .sort((left, right) => left.occurred_at_ms - right.occurred_at_ms || left.id.localeCompare(right.id));
  const interview = evidence.find((item) => item.kind === "interview_event");
  const safeAnswers = safeApplicationAnswers(receipt.packet.answers);
  const sources = buildSources(receipt, verifiedClaims, roleSignals, safeAnswers, interview);
  const commitments = safeAnswers.map(([key, answer], index): PrepCommitment => ({
    key,
    answer,
    sourceId: `application-answer-${index + 1}`,
  }));
  const questions = buildQuestions(receipt, roleSignals, verifiedClaims);
  const warnings = questions
    .filter((question) => question.needsCandidateInput)
    .map((question) => `No submitted, verified claim directly supports: ${question.reason}`);

  const submittedAt = receipt.result.submittedAt ?? receipt.generatedAt;
  const generatedAt = validIso(input.generatedAt ?? new Date().toISOString(), "prep generation time");
  return {
    schemaVersion: 1,
    prepId: `prep-${receipt.receiptId}`,
    generatedAt,
    applicationId: receipt.applicationId,
    receiptId: receipt.receiptId,
    resumeVersionId: resume.versionId,
    company: receipt.job.company,
    title: receipt.job.title,
    location: receipt.job.location,
    submittedAt: validIso(submittedAt, "submission time"),
    interviewAt: interview ? new Date(interview.occurred_at_ms).toISOString() : undefined,
    interviewLabel: interview?.label,
    launchPrompt: launchPrompt(receipt.job.company, receipt.job.title, Boolean(interview)),
    resumeContent: sanitizeResumeContent(resume.content),
    sources,
    commitments,
    questions,
    warnings,
  };
}

export function interviewPrepUserMessage(packet: InterviewPrepPacket): string {
  const sourceText = packet.sources.map((source) => [
    `[${source.id}] ${source.label}`,
    limitText(source.content, 4_000),
  ].join("\n")).join("\n\n");
  const questionText = packet.questions.map((question, index) => [
    `${index + 1}. ${question.question}`,
    `Evidence: ${question.sourceIds.join(", ") || "candidate input required"}`,
  ].join("\n")).join("\n\n");
  const schedule = packet.interviewAt
    ? `${packet.interviewLabel ?? "Interview"} at ${packet.interviewAt}`
    : "No interview event is attached yet.";
  return [
    packet.launchPrompt,
    "",
    "Document context:",
    `Application: ${packet.applicationId}`,
    `Receipt: ${packet.receiptId}`,
    `Submitted resume version: ${packet.resumeVersionId}`,
    `Role: ${packet.title} at ${packet.company}`,
    `Schedule: ${schedule}`,
    "",
    "Submitted-application sources:",
    sourceText,
    "",
    "Submitted resume content (contact fields removed):",
    limitText(JSON.stringify(packet.resumeContent), 30_000),
    "",
    "Evidence-linked practice plan:",
    questionText,
    "",
    "Coaching rule: cite the source IDs you used internally, but do not read IDs aloud. Never fill a truth gap by guessing.",
  ].join("\n");
}

function resolveVerifiedClaims(
  receipt: ApplicationReceiptBundle,
  claims: VerifiedCareerClaim[],
): VerifiedCareerClaim[] {
  const byId = new Map<string, VerifiedCareerClaim>();
  for (const claim of claims) {
    if (!claim.id.trim()) throw new Error("Interview preparation claim IDs cannot be empty");
    if (byId.has(claim.id)) throw new Error(`Interview preparation claim ${claim.id} is duplicated`);
    byId.set(claim.id, claim);
  }
  return receipt.packet.verifiedClaimIds.map((id) => {
    const claim = byId.get(id);
    if (!claim) throw new Error(`Submitted verified claim ${id} is missing from the resume snapshot`);
    if (claim.status !== "confirmed" && claim.source !== "resume_import") {
      throw new Error(`Submitted verified claim ${id} is not confirmed`);
    }
    if (!claim.statement.trim()) throw new Error(`Submitted verified claim ${id} has no evidence statement`);
    return claim;
  }).filter((claim) => {
    const descriptor = `${claim.category ?? ""} ${claim.label}`;
    return !SENSITIVE_ANSWER_KEY.test(descriptor) && !SENSITIVE_VALUE.test(claim.statement);
  });
}

function buildSources(
  receipt: ApplicationReceiptBundle,
  claims: VerifiedCareerClaim[],
  roleSignals: string[],
  safeAnswers: Array<[string, string]>,
  interview?: ApplicationEvidenceRecord,
): PrepSource[] {
  const sources: PrepSource[] = [
    {
      id: "submitted-job",
      kind: "job",
      label: `${receipt.job.title} at ${receipt.job.company}`,
      content: [receipt.job.location, receipt.job.workplace, receipt.job.compensation].filter(Boolean).join(" | "),
    },
    {
      id: "submission-receipt",
      kind: "submission",
      label: "Confirmed application receipt",
      content: `${receipt.result.confirmationText ?? receipt.result.confirmationUrl} (${receipt.receiptId})`,
    },
  ];
  roleSignals.forEach((signal, index) => sources.push({
    id: `job-requirement-${index + 1}`,
    kind: "job",
    label: "Role requirement",
    content: signal,
  }));
  claims.forEach((claim) => sources.push({
    id: `resume-claim-${claim.id}`,
    kind: "resume_claim",
    label: claim.label || "Verified resume claim",
    content: claim.statement,
  }));
  safeAnswers.forEach(([key, answer], index) => sources.push({
    id: `application-answer-${index + 1}`,
    kind: "application_answer",
    label: humanize(key),
    content: answer,
  }));
  if (interview) sources.push({
    id: `interview-event-${interview.id}`,
    kind: "interview_event",
    label: interview.label,
    content: new Date(interview.occurred_at_ms).toISOString(),
  });
  return sources;
}

function buildQuestions(
  receipt: ApplicationReceiptBundle,
  roleSignals: string[],
  claims: VerifiedCareerClaim[],
): PrepQuestion[] {
  const openingClaimIds = claims.slice(0, 3).map((claim) => claim.id);
  const questions: PrepQuestion[] = [
    {
      id: "introduction",
      category: "introduction",
      question: `Tell me about yourself in the context of this ${receipt.job.title} role.`,
      reason: "Connect the submitted resume to the role without introducing new claims.",
      sourceIds: ["submitted-job", ...openingClaimIds.map((id) => `resume-claim-${id}`)],
      claimIds: openingClaimIds,
      needsCandidateInput: openingClaimIds.length === 0,
    },
    {
      id: "motivation",
      category: "motivation",
      question: `Why are you interested in ${receipt.job.company} and this ${receipt.job.title} position?`,
      reason: "Ground motivation in the submitted job rather than generic company praise.",
      sourceIds: ["submitted-job", ...roleSignals.slice(0, 2).map((_, index) => `job-requirement-${index + 1}`)],
      claimIds: [],
      needsCandidateInput: false,
    },
  ];

  roleSignals.slice(0, 6).forEach((signal, index) => {
    const matchingClaims = rankClaims(signal, claims).slice(0, 2);
    const sourceId = `job-requirement-${index + 1}`;
    questions.push({
      id: `role-requirement-${index + 1}`,
      category: matchingClaims.length ? "role_requirement" : "truth_gap",
      question: matchingClaims.length
        ? `Walk me through your most relevant experience for: ${shorten(signal, 150)}`
        : `What is your closest truthful experience with this requirement: ${shorten(signal, 150)}`,
      reason: signal,
      sourceIds: [sourceId, ...matchingClaims.map((claim) => `resume-claim-${claim.id}`)],
      claimIds: matchingClaims.map((claim) => claim.id),
      needsCandidateInput: matchingClaims.length === 0,
    });
  });
  return questions;
}

export function extractRoleSignals(description: string): string[] {
  const pieces = description
    .replace(/\r/g, "\n")
    .split(/\n+|(?<=[.!?])\s+(?=[A-Z])/)
    .map((value) => value.replace(/^\s*[-*\u2022\d.)]+\s*/, "").replace(/\s+/g, " ").trim())
    .filter((value) => value.length >= 24 && value.length <= 360);
  const ranked = pieces
    .map((value, index) => ({ value, index, score: signalScore(value) }))
    .filter((item) => item.score >= 3)
    .sort((left, right) => right.score - left.score || left.index - right.index);
  const seen = new Set<string>();
  const selected: string[] = [];
  for (const item of ranked) {
    const normalized = item.value.toLowerCase();
    if (seen.has(normalized)) continue;
    seen.add(normalized);
    selected.push(item.value);
    if (selected.length === 8) break;
  }
  return selected;
}

function signalScore(value: string): number {
  const lower = value.toLowerCase();
  let score = 0;
  if (/\b(require|qualification|experience|proficien|expert|skill|knowledge|ability|responsib|build|design|lead|manage|develop|deliver|collaborat)\w*\b/.test(lower)) score += 3;
  if (/\b(must|minimum|preferred|you will|you have|we are looking)\b/.test(lower)) score += 2;
  if (tokenize(value).size >= 4) score += 1;
  return score;
}

function rankClaims(signal: string, claims: VerifiedCareerClaim[]): VerifiedCareerClaim[] {
  const signalTokens = tokenize(signal);
  return claims
    .map((claim, index) => ({ claim, index, overlap: overlapScore(signalTokens, tokenize(`${claim.label} ${claim.statement} ${(claim.tags ?? []).join(" ")}`)) }))
    .filter((item) => item.overlap > 0)
    .sort((left, right) => right.overlap - left.overlap || left.index - right.index)
    .map((item) => item.claim);
}

function tokenize(value: string): Set<string> {
  return new Set(
    value.toLowerCase().match(/[a-z0-9+#.]{3,}/g)?.filter((token) => !STOP_WORDS.has(token)) ?? [],
  );
}

function overlapScore(left: Set<string>, right: Set<string>): number {
  let shared = 0;
  for (const token of left) if (right.has(token)) shared += 1;
  return shared;
}

function safeApplicationAnswers(answers: Record<string, string>): Array<[string, string]> {
  return Object.entries(answers)
    .filter(([key, value]) => {
      const answer = value.trim();
      return answer.length > 0
        && answer.length <= 2_000
        && !SENSITIVE_ANSWER_KEY.test(humanize(key))
        && !SENSITIVE_VALUE.test(answer);
    })
    .sort(([left], [right]) => left.localeCompare(right));
}

function sanitizeResumeContent(value: Record<string, unknown>): Record<string, unknown> {
  return sanitizeObject(value) as Record<string, unknown>;
}

function sanitizeObject(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sanitizeObject);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.entries(value as Record<string, unknown>)
      .filter(([key]) => !PRIVATE_RESUME_KEY.test(key))
      .map(([key, nested]) => [key, sanitizeObject(nested)]),
  );
}

function launchPrompt(company: string, title: string, scheduled: boolean): string {
  const timing = scheduled ? "I have an interview scheduled." : "Prepare me before an interview is scheduled.";
  return `Help me prepare for the ${title} role at ${company}. ${timing} Use only the attached submitted-application sources, label any inference, ask one question at a time, and never invent experience.`;
}

function humanize(value: string): string {
  return value.replace(/[_\-.]+/g, " ").replace(/\s+/g, " ").trim();
}

function shorten(value: string, max: number): string {
  return value.length <= max ? value : `${value.slice(0, max - 3).trimEnd()}...`;
}

function limitText(value: string, max: number): string {
  return value.length <= max ? value : `${value.slice(0, max - 20).trimEnd()}\n[content truncated]`;
}

function validIso(value: string, label: string): string {
  const parsed = Date.parse(value);
  if (!Number.isFinite(parsed)) throw new Error(`Interview preparation needs a valid ${label}`);
  return new Date(parsed).toISOString();
}
