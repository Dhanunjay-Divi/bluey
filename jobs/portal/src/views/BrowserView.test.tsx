import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type {
  LocalBrowserReleaseArtifact,
  LocalBrowserReleaseAvailability,
  RunnerChannelAvailability,
} from "../types";
import {
  detectLocalBrowserTarget,
  localBrowserReleaseAuthorized,
  localBrowserReleasePresentation,
  selectMacBrowserArchitecture,
  type LocalBrowserClientTarget,
} from "../lib/browser-release";
import { BrowserInstallContent } from "./BrowserView";

const RELEASE_URL =
  "https://artifacts.bluey.sh/jobs/browser/releases/browser-release-603-17";
const RELEASE_ID = "browser-release-603-17";
const ARTIFACT_ORIGIN = "https://artifacts.bluey.sh";
const INSTALLER_FILE_NAME = "Bluey-Browser-2.4.1-mac-arm64.dmg";
const ARTIFACT_URL = `${RELEASE_URL}/${INSTALLER_FILE_NAME}`;
const ARTIFACT_SHA256 = "24".repeat(32);
const DESCRIPTOR_SHA256 = "5a".repeat(32);
const MANIFEST_SHA256 = "91".repeat(32);

const macArmTarget: LocalBrowserClientTarget = {
  platform: "macos",
  architecture: "arm64",
};

function artifact(
  overrides: Partial<LocalBrowserReleaseArtifact> = {},
): LocalBrowserReleaseArtifact {
  return {
    platform: "macos",
    architecture: "arm64",
    package_kind: "dmg",
    role: "installer",
    file_name: INSTALLER_FILE_NAME,
    url: ARTIFACT_URL,
    size_bytes: 123_456_789,
    sha256: ARTIFACT_SHA256,
    descriptor_sha256: DESCRIPTOR_SHA256,
    ...overrides,
  };
}

function releaseArtifacts(): LocalBrowserReleaseArtifact[] {
  return [
    artifact(),
    artifact({
      package_kind: "zip",
      role: "updater",
      file_name: "Bluey-Browser-2.4.1-mac-arm64.zip",
      url: `${RELEASE_URL}/Bluey-Browser-2.4.1-mac-arm64.zip`,
      sha256: "25".repeat(32),
    }),
    artifact({
      architecture: "x64",
      file_name: "Bluey-Browser-2.4.1-mac-x64.dmg",
      url: `${RELEASE_URL}/Bluey-Browser-2.4.1-mac-x64.dmg`,
      sha256: "26".repeat(32),
      descriptor_sha256: "5b".repeat(32),
    }),
    artifact({
      architecture: "x64",
      package_kind: "zip",
      role: "updater",
      file_name: "Bluey-Browser-2.4.1-mac-x64.zip",
      url: `${RELEASE_URL}/Bluey-Browser-2.4.1-mac-x64.zip`,
      sha256: "27".repeat(32),
      descriptor_sha256: "5b".repeat(32),
    }),
    artifact({
      platform: "windows",
      architecture: "x64",
      package_kind: "exe",
      file_name: "Bluey-Browser-2.4.1-win-x64.exe",
      url: `${RELEASE_URL}/Bluey-Browser-2.4.1-win-x64.exe`,
      sha256: "28".repeat(32),
      descriptor_sha256: "5c".repeat(32),
    }),
  ];
}

function availableRelease(
  artifacts: LocalBrowserReleaseArtifact[] = releaseArtifacts(),
  overrides: Partial<Extract<LocalBrowserReleaseAvailability, { status: "available" }>> = {},
): Extract<LocalBrowserReleaseAvailability, { status: "available" }> {
  return {
    status: "available",
    reason: "The active beta release is available.",
    channel: "beta",
    release_id: RELEASE_ID,
    artifact_origin: ARTIFACT_ORIGIN,
    manifest_sha256: MANIFEST_SHA256,
    release_sequence: 17,
    build_id: "browser-603.17",
    app_version: "2.4.1",
    protocol_version: 7,
    released_at_ms: 1_786_000_000_000,
    artifacts,
    ...overrides,
  };
}

function releaseArtifactsWith(
  index: number,
  overrides: Partial<LocalBrowserReleaseArtifact>,
): LocalBrowserReleaseArtifact[] {
  const artifacts = releaseArtifacts();
  const selected = artifacts[index];
  if (!selected) throw new Error("Invalid release artifact fixture index");
  artifacts[index] = { ...selected, ...overrides };
  return artifacts;
}

function access(
  release?: LocalBrowserReleaseAvailability,
): RunnerChannelAvailability {
  return {
    status: "available",
    available: true,
    plan_included: true,
    distribution_enabled: true,
    reason: "Bluey Browser is available on this account.",
    next_action: "Run locally.",
    ...(release ? { release } : {}),
  };
}

function renderInstall(
  runnerAccess: RunnerChannelAvailability,
  target: LocalBrowserClientTarget = macArmTarget,
): string {
  return renderToStaticMarkup(
    <BrowserInstallContent access={runnerAccess} target={target} onClose={() => undefined} />,
  );
}

function expectNoInstaller(html: string): void {
  expect(html).not.toContain("<a ");
  expect(html).not.toContain("data-browser-download");
  expect(html).not.toContain("bluey-jobs://open");
  expect(html).not.toContain('href="/download"');
}

describe("Bluey Browser release presentation", () => {
  it.each([
    [
      "Safari",
      {
        platform: "MacIntel",
        userAgent:
          "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) " +
          "AppleWebKit/605.1.15 Version/18.5 Safari/605.1.15",
      },
    ],
    [
      "Chrome",
      {
        platform: "MacIntel",
        userAgent:
          "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) " +
          "AppleWebKit/537.36 Chrome/140.0.0.0 Safari/537.36",
      },
    ],
  ])("does not guess architecture from a realistic macOS %s identity", (_browser, identity) => {
    expect(detectLocalBrowserTarget(identity)).toEqual({
      platform: "macos",
      architecture: "unknown",
    });
  });

  it.each([
    ["arm64", INSTALLER_FILE_NAME, "mac-x64.dmg"],
    ["x64", "Bluey-Browser-2.4.1-mac-x64.dmg", "mac-arm64.dmg"],
  ] as const)(
    "maps an explicit %s Mac choice to exactly one signed installer",
    (architecture, expectedFileName, excludedFileName) => {
      const detected = detectLocalBrowserTarget({
        platform: "MacIntel",
        userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
      });
      const selected = selectMacBrowserArchitecture(detected, architecture);
      const html = renderInstall(access(availableRelease()), selected);

      expect(html).toContain(expectedFileName);
      expect(html).not.toContain(excludedFileName);
      expect(html.match(/data-browser-download=/g)).toHaveLength(1);
    },
  );

  it("keeps run authority independent from unobservable macOS architecture", () => {
    const runnerAccess = access(availableRelease());
    const unknownMacTarget: LocalBrowserClientTarget = {
      platform: "macos",
      architecture: "unknown",
    };

    expect(localBrowserReleaseAuthorized(runnerAccess, unknownMacTarget)).toBe(true);
    expect(localBrowserReleasePresentation(runnerAccess, unknownMacTarget)).toMatchObject({
      status: "hidden",
      reason: "unknown_target",
    });
  });

  it("does not authorize a local run on an unsupported operating system", () => {
    expect(localBrowserReleaseAuthorized(access(availableRelease()), {
      platform: "unknown",
      architecture: "unknown",
    })).toBe(false);
  });

  it("does not authorize a local run on the unsupported Windows arm64 target", () => {
    expect(localBrowserReleaseAuthorized(access(availableRelease()), {
      platform: "windows",
      architecture: "arm64",
    })).toBe(false);
  });

  it("renders the exact server-owned artifact URL and metadata for one supported target", () => {
    const html = renderInstall(access(availableRelease()));

    expect(html).toContain(`href="${ARTIFACT_URL}"`);
    expect(html).toContain("Version: 2.4.1");
    expect(html).toContain("Channel: beta");
    expect(html).toContain(INSTALLER_FILE_NAME);
    expect(html).toContain("123,456,789 bytes");
    expect(html).toContain(`SHA-256: ${ARTIFACT_SHA256}`);
    expect(html).toContain("Download for macOS arm64");
    expect(html).not.toContain('href="/download"');
  });

  it.each(["internal", "beta", "stable"] as const)(
    "honors the server-assigned %s channel without changing artifact selection",
    (channel) => {
      const html = renderInstall(access(availableRelease(releaseArtifacts(), { channel })));

      expect(html).toContain(`Channel: ${channel}`);
      expect(html).toContain(`href="${ARTIFACT_URL}"`);
    },
  );

  it.each([
    ["macOS x64", { platform: "macos", architecture: "x64" }, "mac-x64.dmg"],
    ["Windows x64", { platform: "windows", architecture: "x64" }, "win-x64.exe"],
  ] as const)("selects only the exact %s installer", (_label, target, fileName) => {
    const html = renderInstall(access(availableRelease()), target);

    expect(html).toContain(fileName);
    expect(html).not.toContain(INSTALLER_FILE_NAME);
    expect(html).not.toContain("mac-arm64.zip");
  });

  it("hides every URL when release distribution is disabled", () => {
    const html = renderInstall(access({
      status: "disabled",
      reason: "Bluey Browser distribution is disabled for this release.",
    }));

    expect(html).toContain("distribution is disabled");
    expectNoInstaller(html);
  });

  it("fails closed when an older API omits release authority", () => {
    const html = renderInstall(access());

    expect(html).toContain("could not confirm an active Browser release");
    expectNoInstaller(html);
  });

  it("hides every URL when the account has no assigned release channel", () => {
    const html = renderInstall(access({
      status: "unassigned",
      reason: "This account has not been assigned a Browser release channel.",
    }));

    expect(html).toContain("has not been assigned");
    expectNoInstaller(html);
  });

  it("hides every URL when the active release is unavailable or revoked", () => {
    const html = renderInstall(access({
      status: "unavailable",
      reason: "The assigned release is no longer available.",
    }));

    expect(html).toContain("no longer available");
    expectNoInstaller(html);
  });

  it("hides every URL when the server marks an expired activation unavailable", () => {
    const html = renderInstall(access({
      status: "unavailable",
      reason: "The assigned Browser release activation has expired.",
    }));

    expect(html).toContain("release activation has expired");
    expectNoInstaller(html);
  });

  it("keeps an available release hidden while the distribution flag is off", () => {
    const runnerAccess = access(availableRelease());
    runnerAccess.available = false;
    runnerAccess.distribution_enabled = false;
    runnerAccess.reason = "Local Browser distribution is not enabled.";
    const html = renderInstall(runnerAccess);

    expect(html).toContain("distribution is not enabled");
    expectNoInstaller(html);
  });

  it("does not guess an unsupported platform artifact", () => {
    const html = renderInstall(access(availableRelease()), {
      platform: "windows",
      architecture: "arm64",
    });

    expect(html).toContain("is not available for Windows arm64 on the beta channel");
    expectNoInstaller(html);
  });

  it("explains an unknown operating system without exposing an artifact", () => {
    const html = renderInstall(access(availableRelease()), {
      platform: "unknown",
      architecture: "unknown",
    });

    expect(html).toContain("could not identify this computer&#x27;s operating system");
    expectNoInstaller(html);
  });

  it("explains an unknown architecture without guessing between signed artifacts", () => {
    const html = renderInstall(access(availableRelease()), {
      platform: "macos",
      architecture: "unknown",
    });

    expect(html).toContain("could not identify whether this computer uses arm64 or x64");
    expect(html).toContain('aria-label="Choose your Mac processor"');
    expect(html).toContain('data-browser-architecture="arm64"');
    expect(html).toContain('data-browser-architecture="x64"');
    expect(html).toContain("Apple silicon");
    expect(html).toContain("Intel Mac");
    expectNoInstaller(html);
  });

  it("rejects a relative generic download path even if it arrives as artifact metadata", () => {
    const html = renderInstall(
      access(availableRelease(releaseArtifactsWith(0, { url: "/download" }))),
    );

    expect(html).toContain("could not verify the Browser release metadata");
    expectNoInstaller(html);
    expect(html).not.toContain("/download");
  });

  it.each([
    ["query", `${ARTIFACT_URL}?download=1`],
    ["credentials", ARTIFACT_URL.replace("https://", "https://user:secret@")],
    ["fragment", `${ARTIFACT_URL}#download`],
    ["insecure scheme", ARTIFACT_URL.replace("https://", "http://")],
    [
      "mutable alias",
      ARTIFACT_URL.replace("/browser-release-603-17/", "/latest/"),
    ],
    ["foreign origin", ARTIFACT_URL.replace(ARTIFACT_ORIGIN, "https://evil.example")],
    ["near-prefix path", ARTIFACT_URL.replace("/jobs/browser/", "/jobs/browser-evil/")],
    ["encoded release segment", ARTIFACT_URL.replace(RELEASE_ID, "%62rowser-release-603-17")],
  ])("rejects an artifact URL with a %s", (_label, url) => {
    const html = renderInstall(
      access(availableRelease(releaseArtifactsWith(0, { url }))),
    );

    expect(html).toContain("could not verify the Browser release metadata");
    expectNoInstaller(html);
  });

  it.each([
    ["foreign approved origin", { artifact_origin: "https://evil.example" }],
    ["non-default port", { artifact_origin: "https://artifacts.bluey.sh:8443" }],
    ["origin path", { artifact_origin: "https://artifacts.bluey.sh/releases" }],
    ["release mismatch", { release_id: "browser-release-603-18" }],
    ["reserved release", { release_id: "latest" }],
  ])("rejects %s metadata even when package fields are otherwise valid", (_label, overrides) => {
    const html = renderInstall(access(availableRelease(releaseArtifacts(), overrides)));

    expect(html).toContain("could not verify the Browser release metadata");
    expectNoInstaller(html);
  });

  it("rejects duplicate installer targets instead of choosing the first URL", () => {
    const duplicate = artifact({
      file_name: "Bluey-Browser-2.4.1-mac-arm64-copy.dmg",
      url: `${RELEASE_URL}/Bluey-Browser-2.4.1-mac-arm64-copy.dmg`,
      sha256: "29".repeat(32),
    });
    const html = renderInstall(access(availableRelease([...releaseArtifacts(), duplicate])));

    expect(html).toContain("could not verify the Browser release metadata");
    expectNoInstaller(html);
  });

  it.each([
    ["macOS ZIP installer", 1, { role: "installer" }],
    ["macOS DMG updater", 0, { role: "updater" }],
    ["Windows ZIP updater", 4, { package_kind: "zip", role: "updater" }],
    ["Windows arm64 installer", 4, { architecture: "arm64" }],
  ] as const)("rejects incompatible %s metadata", (_label, index, overrides) => {
    const html = renderInstall(
      access(availableRelease(releaseArtifactsWith(index, overrides))),
    );

    expect(html).toContain("could not verify the Browser release metadata");
    expectNoInstaller(html);
  });

  it.each([
    "../Bluey-Browser.dmg",
    "Bluey Browser.dmg",
    "Bluey..Browser.dmg",
    "Bluey-Browser.dmg.exe",
  ])("rejects the unsafe or package-mismatched filename %s", (fileName) => {
    const html = renderInstall(
      access(availableRelease(releaseArtifactsWith(0, { file_name: fileName }))),
    );

    expect(html).toContain("could not verify the Browser release metadata");
    expectNoInstaller(html);
  });

  it("rejects malformed camelCase release metadata instead of guessing its meaning", () => {
    const malformed: Record<string, unknown> = { ...availableRelease() };
    delete malformed.release_sequence;
    malformed.releaseSequence = 17;
    const html = renderInstall(
      access(malformed as unknown as LocalBrowserReleaseAvailability),
    );

    expect(html).toContain("could not verify the Browser release metadata");
    expectNoInstaller(html);
  });

  it("rejects unknown release fields and unsupported channels", () => {
    const malformed = {
      ...availableRelease(),
      channel: "canary",
      download_url: ARTIFACT_URL,
    } as unknown as LocalBrowserReleaseAvailability;
    const html = renderInstall(access(malformed));

    expect(html).toContain("could not verify the Browser release metadata");
    expectNoInstaller(html);
  });
});
