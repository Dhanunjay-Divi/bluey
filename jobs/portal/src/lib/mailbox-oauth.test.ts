import { describe, expect, it } from "vitest";
import type { MailboxConnection } from "../types";
import {
  decodeMailboxOAuthStart,
  decodeMailboxProviderAvailability,
  type MailboxAuthorizationPurpose,
  validateMailboxOAuthAuthorizationUrl,
} from "./mailbox-oauth";

const ORIGIN = "https://jobs.bluey.example";
const TOKEN = "A".repeat(43);

function authorizationUrl(
  provider: MailboxConnection["provider"],
  purpose: MailboxAuthorizationPurpose,
): string {
  const google = provider === "gmail";
  const url = new URL(google
    ? "https://accounts.google.com/o/oauth2/v2/auth"
    : "https://login.microsoftonline.com/common/oauth2/v2.0/authorize");
  const scopes = google
    ? [
        "openid",
        "email",
        "https://www.googleapis.com/auth/gmail.readonly",
        ...(purpose === "communication_write"
          ? [
              "https://www.googleapis.com/auth/gmail.send",
              "https://www.googleapis.com/auth/calendar.events",
            ]
          : []),
      ]
    : [
        "openid",
        "email",
        "offline_access",
        "User.Read",
        "Mail.Read",
        ...(purpose === "communication_write" ? ["Mail.Send", "Calendars.ReadWrite"] : []),
      ];
  url.searchParams.set("client_id", google ? "client.apps.googleusercontent.com" : "client-id");
  url.searchParams.set("redirect_uri", `${ORIGIN}/api/jobs/oauth/${provider}/callback`);
  url.searchParams.set("response_type", "code");
  url.searchParams.set("scope", scopes.join(" "));
  url.searchParams.set("state", TOKEN);
  url.searchParams.set("code_challenge", TOKEN);
  url.searchParams.set("code_challenge_method", "S256");
  if (google) {
    url.searchParams.set("access_type", "offline");
    url.searchParams.set("include_granted_scopes", "true");
    url.searchParams.set("prompt", "consent");
  } else {
    url.searchParams.set("response_mode", "query");
  }
  return url.href;
}

describe("mailbox OAuth runtime contracts", () => {
  it("decodes only the exact two-provider capability contract", () => {
    const value = [
      {
        provider: "gmail",
        configured: true,
        capabilities: [
          "status_sync",
          "application_correlation",
          "review_interventions",
          "recruiter_reply",
          "interview_calendar",
        ],
      },
      {
        provider: "outlook",
        configured: false,
        capabilities: [
          "status_sync",
          "application_correlation",
          "review_interventions",
        ],
      },
    ];

    expect(decodeMailboxProviderAvailability(value)).toEqual(value);
    expect(() => decodeMailboxProviderAvailability(value.slice(0, 1))).toThrow("could not verify");
    expect(() => decodeMailboxProviderAvailability([
      { ...value[0], capabilities: [...value[0].capabilities, "admin"] },
      value[1],
    ])).toThrow("could not verify");
    expect(() => decodeMailboxProviderAvailability([
      value[1],
      value[0],
    ])).toThrow("could not verify");
    expect(() => decodeMailboxProviderAvailability([
      { ...value[0], token: "secret" },
      value[1],
    ])).toThrow("could not verify");
  });

  it("decodes only one bounded authorization URL field", () => {
    const start = { authorization_url: authorizationUrl("gmail", "mailbox_read") };
    expect(decodeMailboxOAuthStart(start)).toEqual(start);
    expect(() => decodeMailboxOAuthStart({ ...start, access_token: "secret" })).toThrow(
      "could not verify",
    );
    expect(() => decodeMailboxOAuthStart({ authorization_url: " javascript:alert(1)" })).toThrow(
      "could not verify",
    );
  });

  it.each([
    ["gmail", "mailbox_read"],
    ["gmail", "communication_write"],
    ["outlook", "mailbox_read"],
    ["outlook", "communication_write"],
  ] as const)("accepts the exact %s %s authorization request", (provider, purpose) => {
    const value = authorizationUrl(provider, purpose);
    expect(validateMailboxOAuthAuthorizationUrl(
      { authorization_url: value },
      provider,
      purpose,
      ORIGIN,
    )).toBe(value);
  });

  it.each([
    ["wrong host", (url: URL) => { url.hostname = "accounts.evil.example"; }],
    ["HTTP", (url: URL) => { url.protocol = "http:"; }],
    ["userinfo", (url: URL) => { url.username = "attacker"; }],
    ["nonstandard port", (url: URL) => { url.port = "444"; }],
    ["fragment", (url: URL) => { url.hash = "token"; }],
    ["wrong callback", (url: URL) => { url.searchParams.set("redirect_uri", "https://evil.example/callback"); }],
    ["missing state", (url: URL) => { url.searchParams.delete("state"); }],
    ["duplicate state", (url: URL) => { url.searchParams.append("state", TOKEN); }],
    ["weak PKCE", (url: URL) => { url.searchParams.set("code_challenge_method", "plain"); }],
    ["wrong scopes", (url: URL) => { url.searchParams.set("scope", "openid email"); }],
    ["extra query", (url: URL) => { url.searchParams.set("next", "https://evil.example"); }],
  ])("rejects a %s before redirect", (_label, mutate) => {
    const url = new URL(authorizationUrl("gmail", "communication_write"));
    mutate(url);
    expect(() => validateMailboxOAuthAuthorizationUrl(
      { authorization_url: url.href },
      "gmail",
      "communication_write",
      ORIGIN,
    )).toThrow("could not verify");
  });

  it("rejects a Microsoft tenant path escape", () => {
    const url = new URL(authorizationUrl("outlook", "mailbox_read"));
    url.pathname = "/common/../oauth2/v2.0/authorize";
    expect(() => validateMailboxOAuthAuthorizationUrl(
      { authorization_url: url.href },
      "outlook",
      "mailbox_read",
      ORIGIN,
    )).toThrow("could not verify");
  });
});
