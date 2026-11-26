import type { ApplicationPacket, NormalizedJob } from "./contracts.js";

const CHECKSUM_PATTERN = /^[a-f0-9]{64}$/;

export class ApprovedExecutionIntegrityError extends Error {
  readonly code = "approved_execution_changed";

  constructor(message = "The approved application packet changed after review") {
    super(message);
    this.name = "ApprovedExecutionIntegrityError";
  }
}

export interface ApprovedExecutionSnapshot {
  approvedPacket: ApplicationPacket;
  approvedJob: NormalizedJob;
  checksum: string;
}

/**
 * Reproduce the server's canonical approved-execution checksum. Runtime
 * metadata selects the explicit server schema and admission proof, while the
 * checksum covers the reviewed packet before those transport fields and
 * approvedPacketChecksum are attached.
 */
export function approvedExecutionChecksum(
  packet: ApplicationPacket,
  job: NormalizedJob,
): string {
  const packetWithoutChecksum = { ...packet } as ApplicationPacket;
  delete (packetWithoutChecksum as Partial<ApplicationPacket>).approvedPacketChecksum;
  const schemaVersion = packet.approvedExecutionSchemaVersion ?? 1;
  const admission = packet.approvedExecutionAdmission;
  delete packetWithoutChecksum.approvedExecutionSchemaVersion;
  delete packetWithoutChecksum.approvedExecutionAdmission;
  if (schemaVersion === 1) {
    if (admission !== undefined) {
      throw new ApprovedExecutionIntegrityError("Legacy approval cannot carry admission authority");
    }
    return sha256Hex(canonicalJson({
      schema_version: 1,
      packet: packetWithoutChecksum,
      job,
    }));
  }
  if (schemaVersion !== 2 && schemaVersion !== 3) {
    throw new ApprovedExecutionIntegrityError("Unsupported approved execution checksum version");
  }
  assertApprovedExecutionAdmission(admission, schemaVersion);
  return sha256Hex(canonicalJson({
    schema_version: schemaVersion,
    admission,
    packet: packetWithoutChecksum,
    job,
  }));
}

export function assertApprovedExecutionChecksum(
  packet: ApplicationPacket,
  job: NormalizedJob,
): string {
  const expected = packet.approvedPacketChecksum;
  if (!CHECKSUM_PATTERN.test(expected || "")) {
    throw new ApprovedExecutionIntegrityError("The application packet has no valid approval checksum");
  }
  const actual = approvedExecutionChecksum(packet, job);
  if (!constantTimeTextEqual(actual, expected)) throw new ApprovedExecutionIntegrityError();
  return expected;
}

/** Clone and freeze the exact reviewed inputs before a runner sees them. */
export function createApprovedExecutionSnapshot(
  packet: ApplicationPacket,
  job: NormalizedJob,
): ApprovedExecutionSnapshot {
  const checksum = assertApprovedExecutionChecksum(packet, job);
  const approvedPacket = deepFreeze(cloneJson(packet) as ApplicationPacket);
  const approvedJob = deepFreeze(cloneJson(job) as NormalizedJob);
  assertApprovedExecutionChecksum(approvedPacket, approvedJob);
  return Object.freeze({ approvedPacket, approvedJob, checksum });
}

/** Runtime document paths belong in a disposable packet, never the approval. */
export function cloneApprovedPacketForRuntime(packet: ApplicationPacket): ApplicationPacket {
  return cloneJson(packet) as ApplicationPacket;
}

function canonicalJson(value: unknown): string {
  return JSON.stringify(canonicalValue(value));
}

function canonicalValue(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map((entry) => canonicalValue(entry));
  }
  if (value && typeof value === "object") {
    const prototype = Object.getPrototypeOf(value);
    if (prototype !== Object.prototype && prototype !== null) {
      throw new ApprovedExecutionIntegrityError(
        "Approved execution contains a non-JSON object",
      );
    }
    const record = value as Record<string, unknown>;
    const sorted: Record<string, unknown> = {};
    for (const key of Object.keys(record).sort(compareUnicodeScalars)) {
      const entry = record[key];
      if (hasUnpairedSurrogate(key)) {
        throw new ApprovedExecutionIntegrityError(
          "Approved execution contains a non-interoperable string",
        );
      }
      sorted[key] = canonicalValue(entry);
    }
    return sorted;
  }
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value) || Object.is(value, -0)) {
      throw new ApprovedExecutionIntegrityError(
        "Approved execution contains a non-interoperable number",
      );
    }
    return value;
  }
  if (typeof value === "string") {
    if (hasUnpairedSurrogate(value)) {
      throw new ApprovedExecutionIntegrityError(
        "Approved execution contains a non-interoperable string",
      );
    }
    return value;
  }
  if (value === null || typeof value === "boolean") return value;
  throw new ApprovedExecutionIntegrityError("Approved execution contains a non-JSON value");
}

function cloneJson(value: unknown): unknown {
  canonicalValue(value);
  return JSON.parse(JSON.stringify(value)) as unknown;
}

function hasUnpairedSurrogate(value: string): boolean {
  for (let index = 0; index < value.length; index += 1) {
    const codeUnit = value.charCodeAt(index);
    if (codeUnit >= 0xd800 && codeUnit <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (!Number.isInteger(next) || next < 0xdc00 || next > 0xdfff) return true;
      index += 1;
    } else if (codeUnit >= 0xdc00 && codeUnit <= 0xdfff) {
      return true;
    }
  }
  return false;
}

function assertApprovedExecutionAdmission(value: unknown, schemaVersion: 2 | 3): void {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new ApprovedExecutionIntegrityError("Approved execution admission is missing");
  }
  const admission = value as Record<string, unknown>;
  if (admission.kind === "review_approval") {
    if (!sameKeys(admission, ["kind"])) {
      throw new ApprovedExecutionIntegrityError("Review approval admission is invalid");
    }
    return;
  }
  const expectedKeys = [
      ...(schemaVersion === 3 ? ["ats_certification"] : []),
      "authority_fingerprint",
      "authorization_id",
      "career_track_id",
      "kind",
      "revision_no",
    ];
  if (admission.kind !== "track_auto_submit"
    || !sameKeys(admission, expectedKeys)
    || !validAdmissionId(admission.authorization_id)
    || !validAdmissionId(admission.career_track_id)
    || !Number.isSafeInteger(admission.revision_no)
    || (admission.revision_no as number) <= 0
    || typeof admission.authority_fingerprint !== "string"
    || !CHECKSUM_PATTERN.test(admission.authority_fingerprint)
    || (schemaVersion === 3
      && !validAtsCertificationAdmission(admission.ats_certification))) {
    throw new ApprovedExecutionIntegrityError("Auto-submit admission is invalid");
  }
}

function validAtsCertificationAdmission(value: unknown): boolean {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const certification = value as Record<string, unknown>;
  if (!sameKeys(certification, [
    "activation_generation",
    "activation_sha256",
    "adapter_bundle_sha256",
    "adapter_version",
    "expires_at_ms",
    "layout_contract_version",
    "layout_set_sha256",
    "manifest_sha256",
    "provider",
    "runner_target_sha256s",
    "schema_version",
    "surface_sha256",
    "target_key_sha256",
    "variant_key",
  ])
    || certification.schema_version !== 1
    || (certification.provider !== "greenhouse" && certification.provider !== "lever")
    || !validAdmissionId(certification.adapter_version)
    || !validAdmissionId(certification.variant_key)
    || !Number.isSafeInteger(certification.layout_contract_version)
    || (certification.layout_contract_version as number) <= 0
    || !Number.isSafeInteger(certification.activation_generation)
    || (certification.activation_generation as number) <= 0
    || !Number.isSafeInteger(certification.expires_at_ms)
    || (certification.expires_at_ms as number) <= 0
    || !Array.isArray(certification.runner_target_sha256s)
    || certification.runner_target_sha256s.length < 1
    || certification.runner_target_sha256s.length > 2) {
    return false;
  }
  const digests = [
    certification.manifest_sha256,
    certification.activation_sha256,
    certification.target_key_sha256,
    certification.layout_set_sha256,
    certification.surface_sha256,
    certification.adapter_bundle_sha256,
    ...certification.runner_target_sha256s,
  ];
  if (!digests.every((digest) => typeof digest === "string" && CHECKSUM_PATTERN.test(digest))) {
    return false;
  }
  return certification.runner_target_sha256s.every((digest, index, values) => (
    index === 0 || values[index - 1] < digest
  ));
}

function sameKeys(value: Record<string, unknown>, expected: string[]): boolean {
  const actual = Object.keys(value).sort();
  return actual.length === expected.length
    && actual.every((key, index) => key === expected[index]);
}

function validAdmissionId(value: unknown): value is string {
  return typeof value === "string"
    && value.length >= 1
    && value.length <= 240
    && value.trim() === value
    && !/[\u0000-\u001f\u007f]/u.test(value);
}

function deepFreeze<T>(value: T): T {
  if (!value || typeof value !== "object" || Object.isFrozen(value)) return value;
  for (const nested of Object.values(value as Record<string, unknown>)) deepFreeze(nested);
  return Object.freeze(value);
}

function compareUnicodeScalars(left: string, right: string): number {
  const leftScalars = [...left];
  const rightScalars = [...right];
  const length = Math.min(leftScalars.length, rightScalars.length);
  for (let index = 0; index < length; index += 1) {
    const leftCodePoint = leftScalars[index]!.codePointAt(0)!;
    const rightCodePoint = rightScalars[index]!.codePointAt(0)!;
    if (leftCodePoint !== rightCodePoint) return leftCodePoint - rightCodePoint;
  }
  return leftScalars.length - rightScalars.length;
}

function constantTimeTextEqual(left: string, right: string): boolean {
  if (left.length !== right.length) return false;
  let difference = 0;
  for (let index = 0; index < left.length; index += 1) {
    difference |= left.charCodeAt(index) ^ right.charCodeAt(index);
  }
  return difference === 0;
}

function sha256Hex(value: string): string {
  const bytes = new TextEncoder().encode(value);
  const bitLength = BigInt(bytes.length) * 8n;
  const paddedLength = Math.ceil((bytes.length + 9) / 64) * 64;
  const padded = new Uint8Array(paddedLength);
  padded.set(bytes);
  padded[bytes.length] = 0x80;
  for (let index = 0; index < 8; index += 1) {
    padded[padded.length - 1 - index] = Number((bitLength >> BigInt(index * 8)) & 0xffn);
  }

  const state = new Uint32Array([
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
  ]);
  const schedule = new Uint32Array(64);
  for (let offset = 0; offset < padded.length; offset += 64) {
    for (let index = 0; index < 16; index += 1) {
      const cursor = offset + index * 4;
      schedule[index] = (
        (padded[cursor]! << 24)
        | (padded[cursor + 1]! << 16)
        | (padded[cursor + 2]! << 8)
        | padded[cursor + 3]!
      ) >>> 0;
    }
    for (let index = 16; index < 64; index += 1) {
      const x = schedule[index - 15]!;
      const y = schedule[index - 2]!;
      const sigma0 = rotateRight(x, 7) ^ rotateRight(x, 18) ^ (x >>> 3);
      const sigma1 = rotateRight(y, 17) ^ rotateRight(y, 19) ^ (y >>> 10);
      schedule[index] = (schedule[index - 16]! + sigma0 + schedule[index - 7]! + sigma1) >>> 0;
    }

    let [a, b, c, d, e, f, g, h] = state;
    for (let index = 0; index < 64; index += 1) {
      const sum1 = rotateRight(e!, 6) ^ rotateRight(e!, 11) ^ rotateRight(e!, 25);
      const choose = (e! & f!) ^ (~e! & g!);
      const temporary1 = (h! + sum1 + choose + SHA256_CONSTANTS[index]! + schedule[index]!) >>> 0;
      const sum0 = rotateRight(a!, 2) ^ rotateRight(a!, 13) ^ rotateRight(a!, 22);
      const majority = (a! & b!) ^ (a! & c!) ^ (b! & c!);
      const temporary2 = (sum0 + majority) >>> 0;
      h = g;
      g = f;
      f = e;
      e = (d! + temporary1) >>> 0;
      d = c;
      c = b;
      b = a;
      a = (temporary1 + temporary2) >>> 0;
    }
    state[0] = (state[0]! + a!) >>> 0;
    state[1] = (state[1]! + b!) >>> 0;
    state[2] = (state[2]! + c!) >>> 0;
    state[3] = (state[3]! + d!) >>> 0;
    state[4] = (state[4]! + e!) >>> 0;
    state[5] = (state[5]! + f!) >>> 0;
    state[6] = (state[6]! + g!) >>> 0;
    state[7] = (state[7]! + h!) >>> 0;
  }
  return [...state].map((word) => word.toString(16).padStart(8, "0")).join("");
}

function rotateRight(value: number, amount: number): number {
  return (value >>> amount) | (value << (32 - amount));
}

const SHA256_CONSTANTS = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);
