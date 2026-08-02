const CAPABILITY_PATTERN = /^[A-Za-z0-9_-]+\.[A-Fa-f0-9]{64}$/;
const MAX_CAPABILITY_LENGTH = 4_096;

export interface LocalRunCapabilities {
  result: string;
  resume: string;
  expiresAtMs: number;
}

export function localRunCapabilities(
  claim: unknown,
  expectedRunId: string,
  nowMs = Date.now(),
): LocalRunCapabilities {
  if (!claim || typeof claim !== "object" || Array.isArray(claim)) {
    throw new Error("invalid local run claim");
  }
  const payload = claim as Record<string, unknown>;
  if (payload.runId !== expectedRunId) throw new Error("local run mismatch");
  const raw = payload._blueyCapabilities;
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    throw new Error("missing local run capabilities");
  }
  const capabilities = raw as Record<string, unknown>;
  const result = validCapability(capabilities.result);
  const resume = validCapability(capabilities.resume);
  const expiresAtMs = capabilities.expiresAtMs;
  if (
    !Number.isSafeInteger(expiresAtMs)
    || (expiresAtMs as number) <= nowMs
    || result === resume
  ) {
    throw new Error("invalid local run capabilities");
  }
  return { result, resume, expiresAtMs: expiresAtMs as number };
}

export function localRunRequestPayload(claim: unknown): Record<string, unknown> {
  if (!claim || typeof claim !== "object" || Array.isArray(claim)) {
    throw new Error("invalid local run claim");
  }
  const { _blueyCapabilities: _discarded, ...request } = claim as Record<string, unknown>;
  return request;
}

function validCapability(value: unknown): string {
  if (
    typeof value !== "string"
    || value.length < 100
    || value.length > MAX_CAPABILITY_LENGTH
    || !CAPABILITY_PATTERN.test(value)
  ) {
    throw new Error("invalid local run capability");
  }
  return value;
}
