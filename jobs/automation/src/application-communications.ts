import {
  selectAnswerMemory,
  type AnswerMemoryRecord,
} from "./answer-memory.js";

export type CommunicationProvider = "gmail" | "outlook_email";

export type CommunicationKind =
  | "acknowledgement"
  | "interview"
  | "information_request"
  | "assessment"
  | "rejection"
  | "offer"
  | "unknown";

export interface NormalizedApplicationMessage {
  provider: CommunicationProvider;
  connectionId: string;
  externalId: string;
  threadId?: string;
  fromAddress: string;
  fromName?: string;
  toAddresses: string[];
  subject: string;
  text: string;
  receivedAt: string;
}

export interface CommunicationApplication {
  id: string;
  jobId: string;
  company: string;
  title: string;
  applicationEmail: string;
  trackId?: string;
  employerDomains?: string[];
  submittedAt?: string;
}

export interface CommunicationCorrelation {
  applicationId?: string;
  confidence: "strong" | "possible" | "none";
  score: number;
  reasons: string[];
  ambiguousApplicationIds: string[];
}

export interface CommunicationReplyPlan {
  action: "no_reply" | "draft" | "auto_send" | "needs_input";
  reason: string;
  subject?: string;
  body?: string;
  answerMemoryIds: string[];
  missingQuestions: string[];
  blockedQuestions: string[];
}

export interface ApplicationCommunicationPlan {
  kind: CommunicationKind;
  correlation: CommunicationCorrelation;
  requestedQuestions: string[];
  reply: CommunicationReplyPlan;
  outcome?: "interview" | "rejected" | "offer";
  calendarCandidate: boolean;
}

export interface CommunicationPlanningOptions {
  answerMemory?: AnswerMemoryRecord[];
  autoReplyEnabled?: boolean;
}

const SENSITIVE_QUESTION = /\b(?:work authori[sz]ation|visa|sponsor(?:ship)?|citizen(?:ship)?|immigration|salary|compensation|pay|race|ethnicity|gender|sex|disab(?:ility|led)|veteran|criminal|background check|drug test|legal|attest|certif(?:y|ication)|signature|social security|ssn)\b/i;
const ASSESSMENT = /\b(?:assessment|coding challenge|take[- ]home|hackerrank|codesignal|technical test|online test)\b/i;
const INTERVIEW = /\b(?:interview|phone screen|screening call|meet with|schedule|availability|calendar|time slots?)\b/i;
const REJECTION = /\b(?:not moving forward|will not be moving forward|decided not to proceed|other candidates|not selected|regret to inform|position has been filled)\b/i;
const OFFER = /\b(?:pleased to offer|offer letter|employment offer|extend an offer|contingent offer)\b/i;
const ACKNOWLEDGEMENT = /\b(?:application (?:was )?received|thank you for applying|thanks for applying|we received your application|application confirmation)\b/i;
const INFORMATION_REQUEST = /\b(?:please (?:confirm|provide|send|share|answer)|could you|can you|would you|what is|when can|are you|do you|will you|have you)\b/i;

export function planApplicationCommunication(
  message: NormalizedApplicationMessage,
  applications: CommunicationApplication[],
  options: CommunicationPlanningOptions = {},
): ApplicationCommunicationPlan {
  assertNormalizedMessage(message);
  const kind = classifyApplicationMessage(message);
  const correlation = correlateApplicationMessage(message, applications);
  const requestedQuestions = extractRequestedQuestions(message);
  const outcome = kind === "interview" || kind === "rejection" || kind === "offer"
    ? kind === "rejection" ? "rejected" : kind
    : undefined;
  const calendarCandidate = kind === "interview";
  const reply = buildReplyPlan(
    message,
    applications.find((application) => application.id === correlation.applicationId),
    kind,
    correlation,
    requestedQuestions,
    options,
  );
  return {
    kind,
    correlation,
    requestedQuestions,
    reply,
    outcome,
    calendarCandidate,
  };
}

export function classifyApplicationMessage(
  message: Pick<NormalizedApplicationMessage, "subject" | "text">,
): CommunicationKind {
  const content = `${message.subject}\n${message.text}`;
  if (REJECTION.test(content)) return "rejection";
  if (OFFER.test(content)) return "offer";
  if (ASSESSMENT.test(content)) return "assessment";
  if (INTERVIEW.test(content)) return "interview";
  if (ACKNOWLEDGEMENT.test(content)) return "acknowledgement";
  if (INFORMATION_REQUEST.test(content) || content.includes("?")) return "information_request";
  return "unknown";
}

export function correlateApplicationMessage(
  message: NormalizedApplicationMessage,
  applications: CommunicationApplication[],
): CommunicationCorrelation {
  const content = normalize(`${message.subject} ${message.text}`);
  const fromDomain = emailDomain(message.fromAddress);
  const recipients = new Set(message.toAddresses.map(normalizeEmail));
  const scored = applications
    .map((application) => {
      let score = 0;
      const reasons: string[] = [];
      if (recipients.has(normalizeEmail(application.applicationEmail))) {
        score += 45;
        reasons.push("application email matches");
      }
      const company = normalize(application.company);
      if (company && content.includes(company)) {
        score += 30;
        reasons.push("company matches");
      }
      const title = normalize(application.title);
      if (title && content.includes(title)) {
        score += 25;
        reasons.push("job title matches");
      }
      if ((application.employerDomains ?? []).some((domain) => normalizeDomain(domain) === fromDomain)) {
        score += 35;
        reasons.push("employer domain matches");
      }
      return { application, score, reasons };
    })
    .filter((candidate) => candidate.score > 0)
    .sort((left, right) => right.score - left.score || left.application.id.localeCompare(right.application.id));

  const top = scored[0];
  if (!top) {
    return { confidence: "none", score: 0, reasons: [], ambiguousApplicationIds: [] };
  }
  const tied = scored.filter((candidate) => candidate.score === top.score);
  const runnerUp = scored[1];
  const uniquelyStrong = top.score >= 60
    && tied.length === 1
    && (!runnerUp || top.score - runnerUp.score >= 20);
  const possible = top.score >= 30 && tied.length === 1;
  return {
    applicationId: uniquelyStrong || possible ? top.application.id : undefined,
    confidence: uniquelyStrong ? "strong" : possible ? "possible" : "none",
    score: top.score,
    reasons: top.reasons,
    ambiguousApplicationIds: tied.length > 1 ? tied.map((candidate) => candidate.application.id) : [],
  };
}

export function extractRequestedQuestions(
  message: Pick<NormalizedApplicationMessage, "text">,
): string[] {
  const lines = message.text
    .split(/\r?\n/)
    .map((line) => line.replace(/^[\s>*\-•\d.)]+/, "").trim())
    .filter(Boolean);
  const questions: string[] = [];
  for (const line of lines) {
    const segments = line.match(/[^?]+\?/g) ?? [];
    for (const segment of segments) addUnique(questions, segment.trim());
    if (!line.includes("?") && /^(?:please (?:confirm|provide|send|share|answer)|kindly (?:confirm|provide|send|share))/i.test(line)) {
      addUnique(questions, line);
    }
  }
  return questions.slice(0, 20);
}

function buildReplyPlan(
  message: NormalizedApplicationMessage,
  application: CommunicationApplication | undefined,
  kind: CommunicationKind,
  correlation: CommunicationCorrelation,
  questions: string[],
  options: CommunicationPlanningOptions,
): CommunicationReplyPlan {
  if (!application || correlation.confidence !== "strong") {
    return needsInput("Bluey could not tie this message to exactly one application.", questions);
  }
  if (kind === "rejection" || kind === "acknowledgement") {
    return noReply("This status message does not need a reply.");
  }
  if (kind === "offer") {
    return needsInput("Offers always require candidate review.", questions);
  }
  if (kind === "assessment") {
    return needsInput("Assessments require candidate review and completion.", questions);
  }
  if (kind === "interview") {
    return needsInput("Interview scheduling requires confirmed availability before Bluey replies.", questions);
  }
  if (kind === "unknown" || questions.length === 0) {
    return needsInput("Bluey needs the candidate to review this message.", questions);
  }

  const memory = options.answerMemory ?? [];
  const blockedQuestions = questions.filter((question) => SENSITIVE_QUESTION.test(question));
  const safeQuestions = questions.filter((question) => !SENSITIVE_QUESTION.test(question));
  const answers = safeQuestions.map((question) => ({
    question,
    match: selectAnswerMemory(question, memory, {
      trackId: application.trackId,
      company: application.company,
    }),
  }));
  const missingQuestions = answers
    .filter((answer) => !answer.match)
    .map((answer) => answer.question);
  if (blockedQuestions.length > 0 || missingQuestions.length > 0) {
    return {
      action: "needs_input",
      reason: blockedQuestions.length > 0
        ? "The recruiter asked for sensitive or candidate-controlled information."
        : "A requested answer is not in verified Answer Memory.",
      answerMemoryIds: answers.flatMap((answer) => answer.match ? [answer.match.answer.id] : []),
      missingQuestions,
      blockedQuestions,
    };
  }

  const greeting = message.fromName?.trim()
    ? `Hello ${firstName(message.fromName)},`
    : "Hello,";
  const body = [
    greeting,
    "",
    "Thank you for reaching out. Here are the requested details:",
    "",
    ...answers.map((answer) => `${answer.question}\n${answer.match!.answer.value}`),
    "",
    "Best,",
  ].join("\n");
  return {
    action: options.autoReplyEnabled ? "auto_send" : "draft",
    reason: options.autoReplyEnabled
      ? "Every requested answer is verified and the message is tied to one application."
      : "A complete reply is ready for review.",
    subject: replySubject(message.subject),
    body,
    answerMemoryIds: answers.map((answer) => answer.match!.answer.id),
    missingQuestions: [],
    blockedQuestions: [],
  };
}

function assertNormalizedMessage(message: NormalizedApplicationMessage): void {
  if (!message.connectionId.trim() || !message.externalId.trim()) {
    throw new Error("Application messages need a connection and provider message ID");
  }
  if (!message.fromAddress.trim() || !message.receivedAt.trim()) {
    throw new Error("Application messages need a sender and received time");
  }
  if (!Number.isFinite(Date.parse(message.receivedAt))) {
    throw new Error("Application messages need a valid received time");
  }
}

function needsInput(reason: string, missingQuestions: string[]): CommunicationReplyPlan {
  return {
    action: "needs_input",
    reason,
    answerMemoryIds: [],
    missingQuestions,
    blockedQuestions: [],
  };
}

function noReply(reason: string): CommunicationReplyPlan {
  return {
    action: "no_reply",
    reason,
    answerMemoryIds: [],
    missingQuestions: [],
    blockedQuestions: [],
  };
}

function normalize(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}

function normalizeEmail(value: string): string {
  return value.trim().toLowerCase();
}

function normalizeDomain(value: string): string {
  return value.trim().toLowerCase().replace(/^@/, "").replace(/^www\./, "");
}

function emailDomain(value: string): string {
  return normalizeDomain(value.split("@").at(-1) ?? "");
}

function replySubject(subject: string): string {
  return /^\s*re:/i.test(subject) ? subject.trim() : `Re: ${subject.trim()}`;
}

function firstName(value: string): string {
  return value.trim().split(/\s+/)[0] || value.trim();
}

function addUnique(values: string[], value: string): void {
  if (!values.some((existing) => normalize(existing) === normalize(value))) values.push(value);
}
