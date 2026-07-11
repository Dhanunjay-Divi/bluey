import { describe, expect, it } from "vitest";
import {
  accountProfileDirectory,
  identityContextKey,
  identityProfileDirectory,
} from "../src/profile.js";

describe("Bluey Browser identity isolation", () => {
  it("keeps application emails in different Chromium profiles", () => {
    const first = identityProfileDirectory("/bluey", "account-1", "identity-a");
    const second = identityProfileDirectory("/bluey", "account-1", "identity-b");
    expect(first).not.toBe(second);
    expect(first).toContain(accountProfileDirectory("/bluey", "account-1"));
  });

  it("reuses the same profile for the same Bluey account and application email", () => {
    expect(identityContextKey("account-1", "identity-a"))
      .toBe(identityContextKey("account-1", "identity-a"));
  });

  it("does not expose account or email identifiers in paths", () => {
    const path = identityProfileDirectory("/bluey", "secret-account", "person@example.com");
    expect(path).not.toContain("secret-account");
    expect(path).not.toContain("person@example.com");
  });
});
