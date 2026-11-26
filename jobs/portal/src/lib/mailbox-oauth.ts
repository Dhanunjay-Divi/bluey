import type {
  MailboxConnection,
  MailboxOAuthStart,
  MailboxProviderAvailability,
} from "../types";

export type MailboxAuthorizationPurpose = "mailbox_read" | "communication_write";

const BASE_CAPABILITIES = [
  "status_sync",
  "application_correlation",
  "review_interventions",
] as const;
const WRITE_CAPABILITIES = ["recruiter_reply", "interview_calendar"] as const;
const GOOGLE_READ_SCOPES = [
  "openid",
  "email",
  "https://www.googleapis.com/auth/gmail.readonly",
] as const;
const GOOGLE_WRITE_SCOPES = [
  ...GOOGLE_READ_SCOPES,
  "https://www.googleapis.com/auth/gmail.send",
  "https://www.googleapis.com/auth/calendar.events",
] as const;
const MICROSOFT_READ_SCOPES = [
  "openid",
  "email",
  "offline_access",
  "User.Read",
  "Mail.Read",
] as const;
const MICROSOFT_WRITE_SCOPES = [
  ...MICROSOFT_READ_SCOPES,
  "Mail.Send",
  "Calendars.ReadWrite",
] as const;

export function decodeMailboxProviderAvailability(
  value: unknown,
): MailboxProviderAvailability[] {
  if (!Array.isArray(value) || value.length !== 2) throw invalidMailboxAuthorization();
  const expectedProviders: MailboxConnection["provider"][] = ["gmail", "outlook"];
  return value.map((item, index) => {
    const record = exactRecord(item, ["provider", "configured", "capabilities"]);
    const provider = record.provider;
    if ((provider !== "gmail" && provider !== "outlook")
      || provider !== expectedProviders[index]
      || typeof record.configured !== "boolean") {
      throw invalidMailboxAuthorization();
    }
    if (!Array.isArray(record.capabilities)
      || record.capabilities.some((capability) => typeof capability !== "string")) {
      throw invalidMailboxAuthorization();
    }
    const capabilities = record.capabilities as string[];
    const baseOnly = stringArraysMatch(capabilities, BASE_CAPABILITIES);
    const withWrite = stringArraysMatch(capabilities, [
      ...BASE_CAPABILITIES,
      ...WRITE_CAPABILITIES,
    ]);
    if ((!baseOnly && !withWrite) || (withWrite && !record.configured)) {
      throw invalidMailboxAuthorization();
    }
    return { provider, configured: record.configured, capabilities };
  });
}

export function decodeMailboxOAuthStart(value: unknown): MailboxOAuthStart {
  const record = exactRecord(value, ["authorization_url"]);
  const authorizationUrl = exactBoundedText(record.authorization_url, 16_384);
  return { authorization_url: authorizationUrl };
}

export function validateMailboxOAuthAuthorizationUrl(
  start: unknown,
  provider: MailboxConnection["provider"],
  purpose: MailboxAuthorizationPurpose,
  portalOrigin: string,
): string {
  const authorizationUrl = decodeMailboxOAuthStart(start).authorization_url;
  let url: URL;
  let origin: URL;
  try {
    url = new URL(authorizationUrl);
    origin = new URL(portalOrigin);
  } catch {
    throw invalidMailboxAuthorization();
  }
  if (origin.origin !== portalOrigin
    || !["http:", "https:"].includes(origin.protocol)
    || url.protocol !== "https:"
    || url.username
    || url.password
    || url.port
    || url.hash) {
    throw invalidMailboxAuthorization();
  }

  if (provider === "gmail") {
    if (url.hostname !== "accounts.google.com" || url.pathname !== "/o/oauth2/v2/auth") {
      throw invalidMailboxAuthorization();
    }
  } else {
    const path = /^\/([A-Za-z0-9._~-]{1,128})\/oauth2\/v2\.0\/authorize$/.exec(
      url.pathname,
    );
    if (url.hostname !== "login.microsoftonline.com"
      || !path
      || path[1] === "."
      || path[1] === "..") {
      throw invalidMailboxAuthorization();
    }
  }

  const google = provider === "gmail";
  const requiredKeys = google
    ? [
        "client_id",
        "redirect_uri",
        "response_type",
        "scope",
        "state",
        "code_challenge",
        "code_challenge_method",
        "access_type",
        "include_granted_scopes",
        "prompt",
      ]
    : [
        "client_id",
        "redirect_uri",
        "response_type",
        "scope",
        "state",
        "code_challenge",
        "code_challenge_method",
        "response_mode",
      ];
  const queryEntries = [...url.searchParams.entries()];
  if (queryEntries.length !== requiredKeys.length
    || requiredKeys.some((key) => url.searchParams.getAll(key).length !== 1)
    || queryEntries.some(([key]) => !requiredKeys.includes(key))) {
    throw invalidMailboxAuthorization();
  }

  const callback = new URL(`/api/jobs/oauth/${provider}/callback`, origin.origin).href;
  const clientId = url.searchParams.get("client_id") ?? "";
  const state = url.searchParams.get("state") ?? "";
  const challenge = url.searchParams.get("code_challenge") ?? "";
  if (!boundedOpaque(clientId, 2_048)
    || url.searchParams.get("redirect_uri") !== callback
    || url.searchParams.get("response_type") !== "code"
    || !/^[A-Za-z0-9_-]{43}$/.test(state)
    || !/^[A-Za-z0-9_-]{43}$/.test(challenge)
    || url.searchParams.get("code_challenge_method") !== "S256") {
    throw invalidMailboxAuthorization();
  }

  const expectedScopes = provider === "gmail"
    ? purpose === "communication_write" ? GOOGLE_WRITE_SCOPES : GOOGLE_READ_SCOPES
    : purpose === "communication_write" ? MICROSOFT_WRITE_SCOPES : MICROSOFT_READ_SCOPES;
  const scope = url.searchParams.get("scope") ?? "";
  if (scope !== expectedScopes.join(" ")) throw invalidMailboxAuthorization();

  if (google) {
    if (url.searchParams.get("access_type") !== "offline"
      || url.searchParams.get("include_granted_scopes") !== "true"
      || url.searchParams.get("prompt") !== "consent") {
      throw invalidMailboxAuthorization();
    }
  } else if (url.searchParams.get("response_mode") !== "query") {
    throw invalidMailboxAuthorization();
  }
  return url.href;
}

function exactRecord(
  value: unknown,
  keys: readonly string[],
): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw invalidMailboxAuthorization();
  }
  const record = value as Record<string, unknown>;
  const actual = Object.keys(record);
  if (actual.length !== keys.length
    || actual.some((key) => !keys.includes(key))
    || keys.some((key) => !Object.prototype.hasOwnProperty.call(record, key))) {
    throw invalidMailboxAuthorization();
  }
  return record;
}

function exactBoundedText(value: unknown, maxBytes: number): string {
  if (typeof value !== "string"
    || !value
    || value !== value.trim()
    || new TextEncoder().encode(value).length > maxBytes
    || /\p{Cc}/u.test(value)) {
    throw invalidMailboxAuthorization();
  }
  return value;
}

function boundedOpaque(value: string, maxBytes: number): boolean {
  return Boolean(value)
    && value === value.trim()
    && new TextEncoder().encode(value).length <= maxBytes
    && !/[\p{Cc}\s]/u.test(value);
}

function stringArraysMatch(
  actual: string[],
  expected: readonly string[],
): boolean {
  return actual.length === expected.length
    && actual.every((value, index) => value === expected[index]);
}

function invalidMailboxAuthorization(): Error {
  return new Error("Bluey could not verify the mailbox authorization response.");
}
