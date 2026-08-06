import type {
  LocalBrowserReleaseArchitecture,
  LocalBrowserReleaseArtifact,
  LocalBrowserReleaseAvailability,
  LocalBrowserReleasePlatform,
  RunnerChannelAvailability,
} from "../types";

export type LocalBrowserClientPlatform = LocalBrowserReleasePlatform | "unknown";
export type LocalBrowserClientArchitecture = LocalBrowserReleaseArchitecture | "unknown";

export interface LocalBrowserClientTarget {
  platform: LocalBrowserClientPlatform;
  architecture: LocalBrowserClientArchitecture;
}

export type LocalBrowserReleaseHiddenReason =
  | "disabled"
  | "invalid"
  | "missing"
  | "unassigned"
  | "unavailable"
  | "unknown_target"
  | "unsupported_target";

export type LocalBrowserReleasePresentation =
  | {
      status: "available";
      release: Extract<LocalBrowserReleaseAvailability, { status: "available" }>;
      artifact: LocalBrowserReleaseArtifact;
    }
  | {
      status: "hidden";
      reason: LocalBrowserReleaseHiddenReason;
      message: string;
    };

interface NavigatorIdentity {
  platform?: string;
  userAgent?: string;
}

const SHA256_PATTERN = /^[a-f0-9]{64}$/;
const BUILD_ID_PATTERN = /^browser-(0|[1-9][0-9]{0,8})\.(0|[1-9][0-9]{0,8})$/;
const SEMVER_PATTERN =
  /^(0|[1-9][0-9]{0,8})\.(0|[1-9][0-9]{0,8})\.(0|[1-9][0-9]{0,8})$/;
const SAFE_FILE_NAME_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._+-]{0,199}$/;
const SAFE_RELEASE_PATH_ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/;
const MUTABLE_RELEASE_PATH_IDS = new Set([
  "beta",
  "current",
  "download",
  "internal",
  "latest",
  "stable",
]);
const AVAILABLE_RELEASE_KEYS = [
  "app_version",
  "artifact_origin",
  "artifacts",
  "build_id",
  "channel",
  "manifest_sha256",
  "protocol_version",
  "reason",
  "release_id",
  "release_sequence",
  "released_at_ms",
  "status",
] as const;
const UNAVAILABLE_RELEASE_KEYS = ["reason", "status"] as const;
const ARTIFACT_KEYS = [
  "architecture",
  "descriptor_sha256",
  "file_name",
  "package_kind",
  "platform",
  "role",
  "sha256",
  "size_bytes",
  "url",
] as const;
const EXPECTED_ARTIFACT_IDENTITIES = new Set([
  "macos:arm64:dmg:installer",
  "macos:arm64:zip:updater",
  "macos:x64:dmg:installer",
  "macos:x64:zip:updater",
  "windows:x64:exe:installer",
]);

export function detectLocalBrowserTarget(identity?: NavigatorIdentity): LocalBrowserClientTarget {
  const browserIdentity = identity ?? (typeof navigator === "undefined" ? undefined : navigator);
  const platform = browserIdentity?.platform ?? "";
  const userAgent = browserIdentity?.userAgent ?? "";
  const source = `${platform} ${userAgent}`.toLowerCase();

  const detectedPlatform: LocalBrowserClientPlatform = /windows|win32|win64/.test(source)
    ? "windows"
    : /macintosh|mac os|macos/.test(source)
      ? "macos"
      : "unknown";
  const architecture: LocalBrowserClientArchitecture = /arm64|aarch64/.test(source)
    ? "arm64"
    : /x86_64|x64|amd64|win64/.test(source)
      ? "x64"
      : "unknown";

  return { platform: detectedPlatform, architecture };
}

export function selectMacBrowserArchitecture(
  target: LocalBrowserClientTarget,
  architecture: LocalBrowserReleaseArchitecture,
): LocalBrowserClientTarget {
  if (target.platform !== "macos" || target.architecture !== "unknown") {
    throw new Error("A manual Browser architecture choice is valid only for an unknown Mac.");
  }
  return { platform: "macos", architecture };
}

export function localBrowserReleasePresentation(
  access: RunnerChannelAvailability,
  target: LocalBrowserClientTarget,
): LocalBrowserReleasePresentation {
  const release: unknown = access.release;

  if (release !== undefined && isUnavailableRelease(release)) {
    return {
      status: "hidden",
      reason: release.status,
      message: release.reason,
    };
  }
  if (release !== undefined && !isRecord(release)) {
    return invalidReleasePresentation();
  }
  if (
    access.status !== "available" ||
    access.available !== true ||
    access.distribution_enabled !== true ||
    access.plan_included !== true
  ) {
    return {
      status: "hidden",
      reason: "disabled",
      message: validReason(access.reason)
        ? access.reason
        : "Bluey Browser distribution is not available for this account.",
    };
  }
  if (release === undefined) {
    return {
      status: "hidden",
      reason: "missing",
      message:
        "Bluey could not confirm an active Browser release for this account. No installer is available.",
    };
  }
  if (!isValidAvailableRelease(release)) {
    return invalidReleasePresentation();
  }
  if (target.platform === "unknown") {
    return {
      status: "hidden",
      reason: "unknown_target",
      message:
        "Bluey could not identify this computer's operating system. " +
        "Open Jobs on a supported macOS or Windows computer.",
    };
  }
  if (target.architecture === "unknown") {
    return {
      status: "hidden",
      reason: "unknown_target",
      message:
        "Bluey could not identify whether this computer uses arm64 or x64. No installer is shown.",
    };
  }

  const matchingArtifacts = release.artifacts.filter(
    (artifact) =>
      artifact.role === "installer" &&
      artifact.platform === target.platform &&
      artifact.architecture === target.architecture,
  );
  if (matchingArtifacts.length !== 1) {
    return {
      status: "hidden",
      reason: "unsupported_target",
      message:
        `Bluey Browser ${release.app_version} is not available for ` +
        `${targetLabel(target)} on the ${release.channel} channel.`,
    };
  }
  return {
    status: "available",
    release,
    artifact: matchingArtifacts[0],
  };
}

export function localBrowserReleaseAuthorized(
  access: RunnerChannelAvailability,
  target: LocalBrowserClientTarget,
): boolean {
  const targetCanRun = target.platform === "macos"
    || (target.platform === "windows" && target.architecture !== "arm64");
  return (
    targetCanRun &&
    access.status === "available" &&
    access.available === true &&
    access.distribution_enabled === true &&
    access.plan_included === true &&
    isValidAvailableRelease(access.release)
  );
}

export function exactByteSize(sizeBytes: number): string {
  return `${new Intl.NumberFormat("en-US").format(sizeBytes)} bytes`;
}

export function targetLabel(target: LocalBrowserClientTarget): string {
  const platform = target.platform === "macos"
    ? "macOS"
    : target.platform === "windows"
      ? "Windows"
      : "an unknown platform";
  return target.architecture === "unknown" ? platform : `${platform} ${target.architecture}`;
}

function isValidAvailableRelease(
  release: unknown,
): release is Extract<LocalBrowserReleaseAvailability, { status: "available" }> {
  if (
    !isRecord(release) ||
    !hasExactKeys(release, AVAILABLE_RELEASE_KEYS) ||
    release.status !== "available" ||
    (release.channel !== "internal" &&
      release.channel !== "beta" &&
      release.channel !== "stable") ||
    !validReason(release.reason) ||
    typeof release.manifest_sha256 !== "string"
  ) {
    return false;
  }
  if (
    !SHA256_PATTERN.test(release.manifest_sha256) ||
    typeof release.release_id !== "string" ||
    !isImmutableReleaseId(release.release_id) ||
    typeof release.artifact_origin !== "string" ||
    !isApprovedArtifactOrigin(release.artifact_origin) ||
    !isPositiveInteger(release.release_sequence) ||
    typeof release.build_id !== "string" ||
    !BUILD_ID_PATTERN.test(release.build_id) ||
    typeof release.app_version !== "string" ||
    !SEMVER_PATTERN.test(release.app_version) ||
    !isPositiveInteger(release.protocol_version) ||
    !isNonnegativeInteger(release.released_at_ms) ||
    !Array.isArray(release.artifacts) ||
    !isExactArtifactSet(
      release.artifacts,
      release.release_id,
      release.artifact_origin,
    )
  ) {
    return false;
  }
  return true;
}

function isUnavailableRelease(
  release: unknown,
): release is Exclude<LocalBrowserReleaseAvailability, { status: "available" }> {
  return (
    isRecord(release) &&
    hasExactKeys(release, UNAVAILABLE_RELEASE_KEYS) &&
    (release.status === "disabled" ||
      release.status === "unassigned" ||
      release.status === "unavailable") &&
    validReason(release.reason)
  );
}

function isExactArtifactSet(
  artifacts: unknown[],
  releaseId: string,
  artifactOrigin: string,
): artifacts is LocalBrowserReleaseArtifact[] {
  if (
    artifacts.length !== EXPECTED_ARTIFACT_IDENTITIES.size ||
    !artifacts.every((artifact) => isValidArtifact(artifact, releaseId, artifactOrigin))
  ) {
    return false;
  }
  const identities = artifacts.map(artifactIdentity);
  if (
    new Set(identities).size !== identities.length ||
    identities.some((identity) => !EXPECTED_ARTIFACT_IDENTITIES.has(identity)) ||
    new Set(artifacts.map((artifact) => artifact.url)).size !== artifacts.length ||
    new Set(artifacts.map((artifact) => artifact.file_name)).size !== artifacts.length ||
    new Set(artifacts.map((artifact) => artifact.sha256)).size !== artifacts.length
  ) {
    return false;
  }
  for (const platform of ["macos", "windows"] as const) {
    for (const architecture of ["arm64", "x64"] as const) {
      const descriptors = new Set(
        artifacts
          .filter(
            (artifact) =>
              artifact.platform === platform && artifact.architecture === architecture,
          )
          .map((artifact) => artifact.descriptor_sha256),
      );
      if (descriptors.size > 1) return false;
    }
  }
  return true;
}

function isValidArtifact(
  artifact: unknown,
  releaseId: string,
  artifactOrigin: string,
): artifact is LocalBrowserReleaseArtifact {
  if (!isRecord(artifact) || !hasExactKeys(artifact, ARTIFACT_KEYS)) return false;
  if (
    (artifact.platform !== "macos" && artifact.platform !== "windows") ||
    (artifact.architecture !== "arm64" && artifact.architecture !== "x64") ||
    (artifact.package_kind !== "dmg" &&
      artifact.package_kind !== "zip" &&
      artifact.package_kind !== "exe") ||
    (artifact.role !== "installer" && artifact.role !== "updater") ||
    typeof artifact.size_bytes !== "number" ||
    !Number.isSafeInteger(artifact.size_bytes) ||
    artifact.size_bytes <= 0 ||
    typeof artifact.sha256 !== "string" ||
    !SHA256_PATTERN.test(artifact.sha256) ||
    typeof artifact.descriptor_sha256 !== "string" ||
    !SHA256_PATTERN.test(artifact.descriptor_sha256) ||
    typeof artifact.file_name !== "string" ||
    !isSafeFileName(artifact.file_name) ||
    typeof artifact.url !== "string" ||
    !isCompatiblePackageAndRole(
      artifact.platform,
      artifact.architecture,
      artifact.package_kind,
      artifact.role,
      artifact.file_name,
    )
  ) {
    return false;
  }

  try {
    const url = new URL(artifact.url);
    const path = url.pathname.split("/");
    return (
      url.protocol === "https:" &&
      url.username === "" &&
      url.password === "" &&
      url.search === "" &&
      url.hash === "" &&
      url.toString() === artifact.url &&
      url.origin === artifactOrigin &&
      path.length === 6 &&
      path[0] === "" &&
      path[1] === "jobs" &&
      path[2] === "browser" &&
      path[3] === "releases" &&
      path[4] === releaseId &&
      path[5] === artifact.file_name
    );
  } catch {
    return false;
  }
}

function isImmutableReleaseId(value: string): boolean {
  return SAFE_RELEASE_PATH_ID_PATTERN.test(value)
    && !MUTABLE_RELEASE_PATH_IDS.has(value.toLowerCase());
}

function isApprovedArtifactOrigin(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "https:"
      && url.username === ""
      && url.password === ""
      && url.port === ""
      && url.pathname === "/"
      && url.search === ""
      && url.hash === ""
      && url.origin === value;
  } catch {
    return false;
  }
}

function isSafeFileName(fileName: string): boolean {
  return SAFE_FILE_NAME_PATTERN.test(fileName) && !fileName.includes("..");
}

function isCompatiblePackageAndRole(
  platform: LocalBrowserReleasePlatform,
  architecture: LocalBrowserReleaseArchitecture,
  packageKind: LocalBrowserReleaseArtifact["package_kind"],
  role: LocalBrowserReleaseArtifact["role"],
  fileName: string,
): boolean {
  const extensionMatches = fileName.endsWith(`.${packageKind}`);
  if (!extensionMatches) return false;
  if (platform === "macos") {
    return (
      (packageKind === "dmg" && role === "installer") ||
      (packageKind === "zip" && role === "updater")
    );
  }
  if (platform === "windows") {
    return (
      architecture === "x64" &&
      packageKind === "exe" &&
      role === "installer"
    );
  }
  return false;
}

function artifactIdentity(artifact: LocalBrowserReleaseArtifact): string {
  return [
    artifact.platform,
    artifact.architecture,
    artifact.package_kind,
    artifact.role,
  ].join(":");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(
  value: Record<string, unknown>,
  expectedKeys: readonly string[],
): boolean {
  const actualKeys = Object.keys(value).sort();
  const expected = [...expectedKeys].sort();
  return (
    actualKeys.length === expected.length &&
    actualKeys.every((key, index) => key === expected[index])
  );
}

function validReason(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= 500;
}

function isNonnegativeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function isPositiveInteger(value: unknown): value is number {
  return isNonnegativeInteger(value) && value > 0;
}

function invalidReleasePresentation(): LocalBrowserReleasePresentation {
  return {
    status: "hidden",
    reason: "invalid",
    message: "Bluey could not verify the Browser release metadata. No installer is available.",
  };
}
