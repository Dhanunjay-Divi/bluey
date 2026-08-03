import { detectAts } from "./adapters.js";
import {
  atsCapabilityProfile,
  type SubmissionCapability,
  type SubmissionPolicy,
} from "./adapter-capabilities.js";

export type {
  SubmissionCapability,
  SubmissionPolicy,
} from "./adapter-capabilities.js";

const HANDOFF_HOSTS = [
  "linkedin.com",
  "www.linkedin.com",
  "indeed.com",
  "www.indeed.com",
];
const BLOCKED_PROTOCOLS = new Set(["file:", "ftp:", "data:", "javascript:"]);

export interface PolicyDecision {
  policy: SubmissionPolicy;
  capability: SubmissionCapability;
  reason: string;
}

export function submissionPolicy(rawUrl: string): PolicyDecision {
  let url: URL;
  try {
    url = new URL(rawUrl);
  } catch {
    return {
      policy: "blocked",
      capability: "blocked",
      reason: "The job link is not a valid URL.",
    };
  }

  if (
    BLOCKED_PROTOCOLS.has(url.protocol) ||
    !["http:", "https:"].includes(url.protocol)
  ) {
    return {
      policy: "blocked",
      capability: "blocked",
      reason: "Only public HTTP job links are supported.",
    };
  }

  const host = url.hostname.toLowerCase();
  if (isPrivateHost(host)) {
    return {
      policy: "blocked",
      capability: "blocked",
      reason: "Private network addresses cannot be opened by Jobs.",
    };
  }

  if (
    HANDOFF_HOSTS.some(
      (candidate) => host === candidate || host.endsWith(`.${candidate}`),
    )
  ) {
    return {
      policy: "handoff",
      capability: "handoff",
      reason:
        "Bluey can prepare the complete packet, then hands this site to you for submission.",
    };
  }

  const ats = detectAts(rawUrl);
  const profile = atsCapabilityProfile(ats);
  return {
    policy: profile.policy,
    capability: profile.capability,
    reason: profile.reason,
  };
}

function isPrivateHost(host: string): boolean {
  if (host === "localhost" || host.endsWith(".local")) return true;
  if (/^127\./.test(host) || /^10\./.test(host) || /^192\.168\./.test(host))
    return true;
  const match = host.match(/^172\.(\d+)\./);
  if (match && Number(match[1]) >= 16 && Number(match[1]) <= 31) return true;
  return host === "0.0.0.0" || host === "::1" || host.startsWith("[::1]");
}
