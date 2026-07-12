import {
  parsePostedAt,
  PublicAtsDiscoveryProvider,
  type JobsFetch,
  type NormalizedJob,
  type PublicAtsSource,
} from "@bluey/jobs-automation";
import {
  DiscoveryProviderError,
  type DiscoveryJob,
  type DiscoveryProviderFailure,
  type DiscoveryProviderRequest,
  type DiscoverySource,
  type JsonObject,
  type ScheduledDiscoveryProvider,
} from "./discovery.js";

const PROVIDER_VERSION = "1.0.0";
const MINIMUM_REQUEST_INTERVAL_MS = 1_000;
const MAX_COMPANY_LENGTH = 300;
const SAFE_IDENTIFIER = /^[A-Za-z0-9_-]+$/;

export type DiscoveryConfigurationErrorCode = "invalid_config" | "unsupported_provider";

export class DiscoveryConfigurationError extends Error {
  readonly code: DiscoveryConfigurationErrorCode;

  constructor(code: DiscoveryConfigurationErrorCode, message: string) {
    super(message);
    this.name = "DiscoveryConfigurationError";
    this.code = code;
  }
}

export interface ServerConfiguredDiscoverySource {
  id: string;
  provider: string;
  source_key: string;
  config: unknown;
}

export type PublicAtsDiscoveryPayload = JsonObject & {
  company: string;
  title: string;
  location: string;
  workplace: string;
  description: string;
  compensation: string;
  department: string;
  source: string;
};

export interface PreparedPublicAtsDiscovery {
  source: DiscoverySource;
  provider: ScheduledDiscoveryProvider<PublicAtsDiscoveryPayload>;
}

export interface PublicAtsDiscoveryAdapterOptions {
  fetch?: JobsFetch;
  timeoutMs?: number;
}

export function preparePublicAtsDiscovery(
  source: ServerConfiguredDiscoverySource,
  options: PublicAtsDiscoveryAdapterOptions = {},
): PreparedPublicAtsDiscovery {
  const configured = configuredSource(source);
  const publicAts = new PublicAtsDiscoveryProvider({
    fetch: options.fetch,
    timeoutMs: options.timeoutMs,
    maxAttempts: 1,
  });

  return {
    source: {
      id: source.id,
      url: configured.proofUrl,
    },
    provider: {
      name: "public-ats",
      version: PROVIDER_VERSION,
      minimumRequestIntervalMs: MINIMUM_REQUEST_INTERVAL_MS,
      discover: async (request) => discoverSnapshot(publicAts, configured.atsSource, request),
      classifyError: classifyPublicAtsError,
    },
  };
}

interface ConfiguredSource {
  atsSource: PublicAtsSource;
  proofUrl: string;
}

function configuredSource(source: ServerConfiguredDiscoverySource): ConfiguredSource {
  const config = requireConfigObject(source.config);
  switch (source.provider) {
    case "greenhouse": {
      assertOnlyKeys(config, ["boardToken", "board_token", "company", "kind"]);
      assertMatchingKind(config.kind, "greenhouse");
      const identifier = configuredIdentifier(
        source.source_key,
        config.board_token,
        config.boardToken,
        "Greenhouse board token",
      );
      const company = optionalCompany(config.company);
      return {
        atsSource: { kind: "greenhouse", boardToken: identifier, ...(company ? { company } : {}) },
        proofUrl: `https://boards-api.greenhouse.io/v1/boards/${encodeURIComponent(identifier)}/jobs?content=true`,
      };
    }
    case "lever": {
      assertOnlyKeys(config, ["company", "kind", "site"]);
      assertMatchingKind(config.kind, "lever");
      const identifier = configuredIdentifier(source.source_key, config.site, undefined, "Lever site");
      const company = optionalCompany(config.company);
      return {
        atsSource: { kind: "lever", site: identifier, ...(company ? { company } : {}) },
        proofUrl: `https://api.lever.co/v0/postings/${encodeURIComponent(identifier)}?mode=json`,
      };
    }
    default:
      throw new DiscoveryConfigurationError(
        "unsupported_provider",
        "The discovery source uses an unsupported provider",
      );
  }
}

async function discoverSnapshot(
  publicAts: PublicAtsDiscoveryProvider,
  source: PublicAtsSource,
  request: DiscoveryProviderRequest,
): Promise<{ jobs: DiscoveryJob<PublicAtsDiscoveryPayload>[] }> {
  const jobs = await publicAts.snapshot(source);

  const scheduledFor = new Date(request.scheduledFor);
  return {
    jobs: jobs.map((job) => scheduledJob(job, scheduledFor)),
  };
}

function scheduledJob(
  job: NormalizedJob,
  scheduledFor: Date,
): DiscoveryJob<PublicAtsDiscoveryPayload> {
  const postedAt = parsePostedAt(job.postedAt, scheduledFor)?.toISOString();
  return {
    externalId: job.externalId,
    canonicalUrl: job.canonicalUrl,
    ...(postedAt ? { postedAt } : {}),
    availability: "open",
    payload: {
      company: job.company,
      title: job.title,
      location: job.location,
      workplace: job.workplace,
      description: job.description,
      compensation: job.compensation ?? "",
      department: job.department ?? "",
      source: job.source,
    },
  };
}

function classifyPublicAtsError(error: unknown): DiscoveryProviderFailure {
  if (error instanceof DiscoveryProviderError) {
    return {
      code: error.code,
      retryable: error.retryable,
      retryAfterMs: error.retryAfterMs,
    };
  }
  if (error instanceof AggregateError) {
    const first = error.errors[0] as unknown;
    return first === undefined
      ? { code: "provider_error", retryable: true }
      : classifyPublicAtsError(first);
  }
  if (error instanceof Error && error.name === "AbortError") {
    return { code: "timeout", retryable: true };
  }

  const message = error instanceof Error ? error.message : "";
  const status = Number(message.match(/status\s+(\d{3})/i)?.[1]);
  if (status === 401 || status === 403) return { code: "unauthorized", retryable: false };
  if (status === 408) return { code: "timeout", retryable: true };
  if (status === 429) return { code: "throttled", retryable: true };
  if (status >= 500 && status <= 599) return { code: "unavailable", retryable: true };
  if (status >= 400 && status <= 499) return { code: "invalid_response", retryable: false };
  if (error instanceof TypeError) return { code: "unavailable", retryable: true };
  return { code: "provider_error", retryable: true };
}

function requireConfigObject(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new DiscoveryConfigurationError("invalid_config", "Discovery provider config must be an object");
  }
  return value as Record<string, unknown>;
}

function assertOnlyKeys(config: Record<string, unknown>, allowed: readonly string[]): void {
  const allowedKeys = new Set(allowed);
  if (Object.keys(config).some((key) => !allowedKeys.has(key))) {
    throw new DiscoveryConfigurationError("invalid_config", "Discovery provider config contains unsupported fields");
  }
}

function assertMatchingKind(value: unknown, expected: "greenhouse" | "lever"): void {
  if (value !== undefined && value !== expected) {
    throw new DiscoveryConfigurationError("invalid_config", "Discovery provider config kind does not match");
  }
}

function configuredIdentifier(
  sourceKey: string,
  primary: unknown,
  alternate: unknown,
  label: string,
): string {
  const configured = optionalIdentifier(primary, label);
  const configuredAlternate = optionalIdentifier(alternate, label);
  if (configured && configuredAlternate && configured !== configuredAlternate) {
    throw new DiscoveryConfigurationError("invalid_config", `${label} is ambiguous`);
  }
  const identifier = configured ?? configuredAlternate ?? sourceKey;
  if (typeof identifier !== "string" || !SAFE_IDENTIFIER.test(identifier)) {
    throw new DiscoveryConfigurationError("invalid_config", `${label} is invalid`);
  }
  if ((configured || configuredAlternate) && identifier !== sourceKey) {
    throw new DiscoveryConfigurationError("invalid_config", `${label} does not match the source key`);
  }
  return identifier;
}

function optionalIdentifier(value: unknown, label: string): string | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== "string" || !SAFE_IDENTIFIER.test(value)) {
    throw new DiscoveryConfigurationError("invalid_config", `${label} is invalid`);
  }
  return value;
}

function optionalCompany(value: unknown): string | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== "string") {
    throw new DiscoveryConfigurationError("invalid_config", "Discovery company must be a string");
  }
  const company = value.trim();
  if (company.length > MAX_COMPANY_LENGTH || /[\u0000-\u001f\u007f]/.test(company)) {
    throw new DiscoveryConfigurationError("invalid_config", "Discovery company is invalid");
  }
  return company || undefined;
}
