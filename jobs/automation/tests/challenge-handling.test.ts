import { describe, expect, it } from "vitest";
import {
  approveEmailOtp,
  findEmailOtpCandidate,
  planAuthenticationChallenge,
  type AuthenticationChallenge,
} from "../src/challenge-handling.js";

const now = Date.parse("2026-07-10T12:05:00.000Z");
const challenge: AuthenticationChallenge = {
  kind: "email_otp",
  applicationId: "app-1",
  company: "Acme",
  targetUrl: "https://acme.wd5.myworkdayjobs.com/job/123",
  startedAt: "2026-07-10T12:00:00.000Z",
  expectedSenderDomains: ["myworkday.com"],
};
const inboxes = [{ provider: "gmail" as const, addresses: ["jobs@example.com"], connected: true }];

describe("authentication challenge planning", () => {
  it("always hands CAPTCHA to the account owner and resumes afterward", () => {
    const plan = planAuthenticationChallenge({ ...challenge, kind: "captcha" }, inboxes, [], now);
    expect(plan.intervention.resolution).toEqual({ kind: "browser_takeover", resumeAfter: true });
    expect(plan.ephemeralOtp).toBeUndefined();
  });

  it("matches a fresh application email and keeps its code out of the intervention", () => {
    const plan = planAuthenticationChallenge(challenge, inboxes, [{
      id: "gmail-message-1",
      provider: "gmail",
      sender: "no-reply@myworkday.com",
      recipients: ["jobs@example.com"],
      subject: "Acme verification code",
      text: "Your verification code is 824193",
      receivedAt: "2026-07-10T12:04:00.000Z",
    }], now);
    expect(plan.ephemeralOtp?.code).toBe("824193");
    expect(plan.intervention.resolution).toMatchObject({
      kind: "email_otp_approval",
      provider: "gmail",
      messageId: "gmail-message-1",
      resumeAfter: true,
    });
    expect(JSON.stringify(plan.intervention)).not.toContain("824193");
  });

  it("ignores old, unrelated, and unconnected inbox messages", () => {
    const messages = [{
      id: "old",
      provider: "gmail" as const,
      sender: "no-reply@myworkday.com",
      recipients: ["jobs@example.com"],
      subject: "Acme verification code",
      text: "Your verification code is 111111",
      receivedAt: "2026-07-10T11:40:00.000Z",
    }];
    expect(findEmailOtpCandidate(challenge, inboxes, messages, now)).toBeUndefined();
    expect(findEmailOtpCandidate(challenge, [{ ...inboxes[0], connected: false }], [{ ...messages[0], receivedAt: "2026-07-10T12:04:00.000Z" }], now)).toBeUndefined();
  });

  it("will not use an expired code", () => {
    expect(() => approveEmailOtp({
      code: "824193",
      provider: "gmail",
      messageId: "gmail-message-1",
      sender: "no-reply@myworkday.com",
      destination: "jobs@example.com",
      receivedAt: "2026-07-10T12:04:00.000Z",
      expiresAt: "2026-07-10T12:05:00.000Z",
    }, now)).toThrow("expired");
  });
});
