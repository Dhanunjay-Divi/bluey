import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { MailboxConnection, MailboxProviderAvailability } from "../types";
import {
  CommunicationAuthorizationControl,
  mailboxCalendarActionState,
  mailboxCommunicationAuthorizationState,
  mailboxProvidersAfterLoad,
} from "./ApplicationInboxSettings";

function connection(
  overrides: Partial<MailboxConnection> = {},
): MailboxConnection {
  return {
    id: "connection-one",
    provider: "gmail",
    status: "connected",
    account_label: "candidate@gmail.com",
    aliases: [],
    capabilities: ["status_sync"],
    created_at_ms: 1_786_000_000_000,
    updated_at_ms: 1_786_000_000_000,
    ...overrides,
  };
}

function availability(
  capabilities: string[],
): MailboxProviderAvailability {
  return {
    provider: "gmail",
    configured: true,
    capabilities,
  };
}

describe("explicit mailbox communication authorization", () => {
  it("offers the account-bound control only when both write capabilities are advertised", () => {
    const writeAvailable = availability([
      "status_sync",
      "recruiter_reply",
      "interview_calendar",
    ]);
    const readOnly = availability(["status_sync"]);
    const html = renderToStaticMarkup(createElement(CommunicationAuthorizationControl, {
      connection: connection(),
      availability: writeAvailable,
      busy: false,
      authorizing: false,
      onAuthorize: () => undefined,
    }));

    expect(mailboxCommunicationAuthorizationState(connection(), writeAvailable)).toBe("available");
    expect(mailboxCommunicationAuthorizationState(connection(), readOnly)).toBe("unavailable");
    expect(html).toContain("Authorize replies &amp; calendar");
    expect(html).toContain("Authorize replies and calendar for candidate@gmail.com");
    expect(html).toContain("Adds send and calendar-write scopes");
  });

  it("shows an authorized state only when the connected account has both exact grants", () => {
    const authorized = connection({
      capabilities: ["status_sync", "recruiter_reply", "interview_calendar"],
    });
    const partial = connection({ capabilities: ["status_sync", "recruiter_reply"] });
    const html = renderToStaticMarkup(createElement(CommunicationAuthorizationControl, {
      connection: authorized,
      availability: availability(["recruiter_reply", "interview_calendar"]),
      busy: false,
      authorizing: false,
      onAuthorize: () => undefined,
    }));

    expect(mailboxCommunicationAuthorizationState(authorized)).toBe("authorized");
    expect(mailboxCommunicationAuthorizationState(
      partial,
      availability(["recruiter_reply", "interview_calendar"]),
    )).toBe("available");
    expect(html).toContain("Replies &amp; calendar authorized");
    expect(html).toContain("every exact draft still requires review");
    expect(html).not.toContain("<button");
  });

  it("does not offer write authorization while the connection needs reauthorization", () => {
    const needsReconnect = connection({ status: "reauthorization_required" });
    const writeAvailable = availability(["recruiter_reply", "interview_calendar"]);
    const html = renderToStaticMarkup(createElement(CommunicationAuthorizationControl, {
      connection: needsReconnect,
      availability: writeAvailable,
      busy: false,
      authorizing: false,
      onAuthorize: () => undefined,
    }));

    expect(mailboxCommunicationAuthorizationState(needsReconnect, writeAvailable)).toBe(
      "unavailable",
    );
    expect(html).toBe("");
  });

  it("reports calendar availability from real connection and grant state", () => {
    const writeAvailable = availability([
      "status_sync",
      "recruiter_reply",
      "interview_calendar",
    ]);
    const authorized = connection({
      capabilities: ["status_sync", "recruiter_reply", "interview_calendar"],
    });

    expect(mailboxCalendarActionState([], [writeAvailable])).toBe("unavailable");
    expect(mailboxCalendarActionState([connection()], [writeAvailable])).toBe("available");
    expect(mailboxCalendarActionState([authorized], [])).toBe("authorized");
  });

  it("announces the pending authorization redirect", () => {
    const html = renderToStaticMarkup(createElement(CommunicationAuthorizationControl, {
      connection: connection(),
      availability: availability(["status_sync", "recruiter_reply", "interview_calendar"]),
      busy: true,
      authorizing: true,
      onAuthorize: () => undefined,
    }));

    expect(html).toContain('aria-busy="true"');
    expect(html).toContain("Opening authorization…");
  });

  it("clears stale provider authorization offers while loading or after failure", () => {
    const stale = [availability(["recruiter_reply", "interview_calendar"])];

    expect(mailboxProvidersAfterLoad(stale)).toBe(stale);
    expect(mailboxProvidersAfterLoad(null)).toEqual([]);
  });
});
