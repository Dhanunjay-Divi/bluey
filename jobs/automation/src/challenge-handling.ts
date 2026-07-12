import type { InterventionRequest } from "./contracts.js";

const EMAIL_CODE_TTL_MS = 10 * 60 * 1_000;
const CLOCK_SKEW_MS = 30 * 1_000;

export type AuthenticationChallengeKind =
  | "captcha"
  | "email_otp"
  | "sms_otp"
  | "authenticator"
  | "push"
  | "assessment";

export interface AuthenticationChallenge {
  kind: AuthenticationChallengeKind;
  applicationId: string;
  company: string;
  targetUrl: string;
  startedAt: string;
  expectedSenderDomains?: string[];
}

export interface ConnectedInbox {
  provider: "gmail" | "outlook_email";
  addresses: string[];
  connected: boolean;
}

export interface InboxMessageEnvelope {
  id: string;
  provider: "gmail" | "outlook_email";
  sender: string;
  recipients: string[];
  subject: string;
  text: string;
  receivedAt: string;
}

/** Ephemeral only. Never serialize this value into an intervention or log. */
export interface EmailOtpCandidate {
  code: string;
  provider: "gmail" | "outlook_email";
  messageId: string;
  sender: string;
  destination: string;
  receivedAt: string;
  expiresAt: string;
}

export interface ChallengePlan {
  intervention: InterventionRequest;
  ephemeralOtp?: EmailOtpCandidate;
}

export function planAuthenticationChallenge(
  challenge: AuthenticationChallenge,
  inboxes: ConnectedInbox[],
  messages: InboxMessageEnvelope[],
  nowMs = Date.now(),
): ChallengePlan {
  if (challenge.kind === "captcha") {
    return takeoverPlan("captcha", "Complete CAPTCHA", "Take over the preserved browser page. Bluey resumes this application after you finish.");
  }
  if (challenge.kind === "assessment") {
    return takeoverPlan("assessment", "Assessment needs you", "Take over for this assessment. Bluey keeps the application ready and resumes afterward.");
  }
  if (challenge.kind !== "email_otp") {
    return takeoverPlan("two_factor", "Verify your sign-in", "Approve this check on your phone or authenticator, then return to the preserved application.");
  }

  const candidate = findEmailOtpCandidate(challenge, inboxes, messages, nowMs);
  if (!candidate) {
    return takeoverPlan("two_factor", "Check your email", "No matching email code is available yet. Take over now or wait for the connected inbox to receive it.");
  }
  return {
    ephemeralOtp: candidate,
    intervention: {
      kind: "two_factor",
      title: "Email code ready",
      detail: `A new verification email for ${challenge.company} arrived at ${maskEmail(candidate.destination)}. Approve it to continue this application.`,
      resolution: {
        kind: "email_otp_approval",
        resumeAfter: true,
        expiresAt: candidate.expiresAt,
        provider: candidate.provider,
        messageId: candidate.messageId,
      },
    },
  };
}

export function findEmailOtpCandidate(
  challenge: AuthenticationChallenge,
  inboxes: ConnectedInbox[],
  messages: InboxMessageEnvelope[],
  nowMs = Date.now(),
): EmailOtpCandidate | undefined {
  const startedAtMs = Date.parse(challenge.startedAt);
  if (!Number.isFinite(startedAtMs)) throw new Error("Challenge needs a valid start time");
  const activeInboxes = inboxes.filter((inbox) => inbox.connected);
  const allowedAddresses = new Set(activeInboxes.flatMap((inbox) => inbox.addresses.map(normalizeEmail)));
  const allowedProviders = new Set(activeInboxes.map((inbox) => inbox.provider));
  const expectedDomains = expectedDomainSet(challenge);

  return messages
    .flatMap((message) => {
      const receivedAtMs = Date.parse(message.receivedAt);
      const destination = message.recipients.map(normalizeEmail).find((address) => allowedAddresses.has(address));
      if (!destination || !allowedProviders.has(message.provider)) return [];
      if (!Number.isFinite(receivedAtMs) || receivedAtMs < startedAtMs - CLOCK_SKEW_MS || receivedAtMs > nowMs) return [];
      if (nowMs - receivedAtMs > EMAIL_CODE_TTL_MS) return [];
      if (!messageLooksRelated(message, challenge, expectedDomains)) return [];
      const code = extractVerificationCode(`${message.subject}\n${message.text}`);
      if (!code) return [];
      return [{
        code,
        provider: message.provider,
        messageId: message.id,
        sender: message.sender,
        destination,
        receivedAt: message.receivedAt,
        expiresAt: new Date(Math.min(receivedAtMs + EMAIL_CODE_TTL_MS, startedAtMs + EMAIL_CODE_TTL_MS)).toISOString(),
      } satisfies EmailOtpCandidate];
    })
    .sort((left, right) => Date.parse(right.receivedAt) - Date.parse(left.receivedAt))[0];
}

export function approveEmailOtp(candidate: EmailOtpCandidate, nowMs = Date.now()): string {
  if (nowMs >= Date.parse(candidate.expiresAt)) throw new Error("Email verification code has expired");
  return candidate.code;
}

function takeoverPlan(kind: "captcha" | "two_factor" | "assessment", title: string, detail: string): ChallengePlan {
  return {
    intervention: {
      kind,
      title,
      detail,
      resolution: { kind: "browser_takeover", resumeAfter: true },
    },
  };
}

function expectedDomainSet(challenge: AuthenticationChallenge): Set<string> {
  const domains = new Set((challenge.expectedSenderDomains ?? []).map(normalizeDomain));
  try {
    domains.add(normalizeDomain(new URL(challenge.targetUrl).hostname));
  } catch {
    // The application URL is validated elsewhere. A malformed URL simply contributes no sender hint here.
  }
  return domains;
}

function messageLooksRelated(
  message: InboxMessageEnvelope,
  challenge: AuthenticationChallenge,
  expectedDomains: Set<string>,
): boolean {
  const senderDomain = normalizeDomain(message.sender.split("@").at(-1) ?? "");
  const content = `${message.subject} ${message.text}`.toLowerCase();
  const hasCodeLanguage = /verification|security code|one[ -]?time|passcode|login code|sign[ -]?in code/.test(content);
  if (!hasCodeLanguage) return false;
  const companyTokens = challenge.company.toLowerCase().split(/\W+/).filter((token) => token.length >= 3);
  const companyMatch = companyTokens.some((token) => content.includes(token));
  const domainMatch = [...expectedDomains].some((domain) => senderDomain === domain || senderDomain.endsWith(`.${domain}`));
  return companyMatch || domainMatch;
}

function extractVerificationCode(content: string): string | undefined {
  const patterns = [
    /(?:verification|security|one[ -]?time|passcode|login|sign[ -]?in)\s+(?:code\s+)?(?:is\s+|:\s*)?([A-Z0-9]{4,8})\b/i,
    /\bcode\s*(?:is|:)\s*([A-Z0-9]{4,8})\b/i,
  ];
  for (const pattern of patterns) {
    const match = content.match(pattern)?.[1];
    if (match && /\d/.test(match)) return match.toUpperCase();
  }
  return undefined;
}

function normalizeEmail(value: string): string {
  return value.trim().toLowerCase();
}

function normalizeDomain(value: string): string {
  return value.trim().toLowerCase().replace(/^www\./, "");
}

function maskEmail(value: string): string {
  const [local, domain] = value.split("@");
  if (!local || !domain) return "your connected inbox";
  return `${local.slice(0, 1)}${"•".repeat(Math.max(2, Math.min(5, local.length - 1)))}@${domain}`;
}
