export type AnswerMemoryScope = "account" | "track" | "company";

export interface AnswerMemoryRecord {
  id: string;
  key: string;
  question: string;
  value: string;
  scope: AnswerMemoryScope;
  scope_id?: string;
  confirmed: boolean;
  source: "user" | "intervention" | "import";
  last_used_at_ms?: number;
  use_count?: number;
}

export interface AnswerMemoryContext {
  trackId?: string;
  company?: string;
}

export interface AnswerMemoryMatch {
  answer: AnswerMemoryRecord;
  reason: "key" | "question";
}

export function normalizeAnswerMemoryKey(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/['"]/g, "")
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
}

export function answerMemoryKeyForQuestion(question: string): string {
  const lower = question.toLowerCase();
  if (/authori[sz].*work|work.*authori[sz]|legal.*work/.test(lower)) return "work_authorization";
  if (/sponsor|visa/.test(lower)) return "sponsorship";
  if (/salary|compensation|pay range/.test(lower)) return "compensation";
  if (/phone|mobile/.test(lower)) return "phone";
  if (/linkedin/.test(lower)) return "linkedin";
  if (/portfolio|website|github/.test(lower)) return "portfolio";
  if (/location|city|state|relocat/.test(lower)) return "location";
  if (/start date|available|notice/.test(lower)) return "availability";
  return normalizeAnswerMemoryKey(question);
}

export function selectAnswerMemory(
  question: string,
  answers: AnswerMemoryRecord[],
  context: AnswerMemoryContext = {},
): AnswerMemoryMatch | undefined {
  const key = answerMemoryKeyForQuestion(question);
  const questionKey = normalizeAnswerMemoryKey(question);
  const confirmed = answers.filter((answer) => answer.confirmed && answer.value.trim());
  const scoped = confirmed
    .filter((answer) => scopeMatches(answer, context))
    .sort((left, right) => scopeRank(right, context) - scopeRank(left, context)
      || (right.last_used_at_ms ?? 0) - (left.last_used_at_ms ?? 0)
      || (right.use_count ?? 0) - (left.use_count ?? 0));

  const exactKey = scoped.find((answer) => normalizeAnswerMemoryKey(answer.key) === key);
  if (exactKey) return { answer: exactKey, reason: "key" };

  const exactQuestion = scoped.find((answer) => normalizeAnswerMemoryKey(answer.question) === questionKey);
  if (exactQuestion) return { answer: exactQuestion, reason: "question" };

  const contained = scoped.find((answer) => {
    const candidateKey = normalizeAnswerMemoryKey(answer.key);
    const candidateQuestion = normalizeAnswerMemoryKey(answer.question);
    return candidateKey && (questionKey.includes(candidateKey) || candidateQuestion.includes(questionKey) || questionKey.includes(candidateQuestion));
  });
  return contained ? { answer: contained, reason: "question" } : undefined;
}

export function buildRememberedAnswers(
  questions: string[],
  answers: AnswerMemoryRecord[],
  context: AnswerMemoryContext = {},
): Record<string, string> {
  const output: Record<string, string> = {};
  for (const question of questions) {
    const match = selectAnswerMemory(question, answers, context);
    if (match) output[answerMemoryKeyForQuestion(question)] = match.answer.value;
  }
  return output;
}

function scopeMatches(answer: AnswerMemoryRecord, context: AnswerMemoryContext): boolean {
  if (answer.scope === "account") return true;
  if (answer.scope === "track") return Boolean(context.trackId && answer.scope_id === context.trackId);
  if (answer.scope === "company") {
    return Boolean(context.company && normalizeAnswerMemoryKey(answer.scope_id ?? "") === normalizeAnswerMemoryKey(context.company));
  }
  return false;
}

function scopeRank(answer: AnswerMemoryRecord, context: AnswerMemoryContext): number {
  if (answer.scope === "company" && scopeMatches(answer, context)) return 3;
  if (answer.scope === "track" && scopeMatches(answer, context)) return 2;
  if (answer.scope === "account") return 1;
  return 0;
}
