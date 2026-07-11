import { describe, expect, it } from "vitest";
import { assertPublicApplicationUrl, isPrivateAddress } from "../src/network.js";

describe("application navigation policy", () => {
  it.each([
    "127.0.0.1",
    "10.2.3.4",
    "172.20.1.1",
    "192.168.1.2",
    "169.254.169.254",
    "100.64.0.1",
    "::1",
    "fc00::1",
    "fe80::1",
    "::ffff:127.0.0.1",
  ])("blocks private address %s", (address) => {
    expect(isPrivateAddress(address)).toBe(true);
  });

  it.each(["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"])("allows public address %s", (address) => {
    expect(isPrivateAddress(address)).toBe(false);
  });

  it("rejects private and credential-bearing application URLs", async () => {
    await expect(assertPublicApplicationUrl("https://127.0.0.1/jobs/1")).rejects.toThrow("Private network");
    await expect(assertPublicApplicationUrl("https://user:pass@8.8.8.8/jobs/1")).rejects.toThrow("credentials");
  });
});
