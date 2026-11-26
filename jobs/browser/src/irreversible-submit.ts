import { constants } from "node:fs";
import { lstat, open } from "node:fs/promises";
import { dirname, isAbsolute, join, resolve } from "node:path";
import type {
  FinalSubmitActivationOutcome,
  ProviderFinalSubmitProof,
} from "@bluey/jobs-automation";

export const FINAL_SUBMIT_MARKER_FILE = "irreversible-submit.jsonl";

const MAX_MARKER_BYTES = 512;
const DIRECTORY_SYNC_UNSUPPORTED = new Set(["EBADF", "EINVAL", "EISDIR", "ENOTSUP", "EPERM"]);

export type FinalSubmitMarkerErrorCode =
  | "invalid_run_directory"
  | "submit_authority_exists"
  | "submit_marker_write_failed"
  | "submit_marker_read_failed"
  | "submit_activation_write_failed";

export class FinalSubmitMarkerError extends Error {
  readonly code: FinalSubmitMarkerErrorCode;

  constructor(code: FinalSubmitMarkerErrorCode) {
    super(code);
    this.name = "FinalSubmitMarkerError";
    this.code = code;
  }
}

export interface FinalSubmitAuthority {
  readonly markerPath: string;
  recordActivation(outcome: FinalSubmitActivationOutcome): Promise<void>;
}

export interface FinalSubmitHooks {
  beforeFinalSubmit(proof: ProviderFinalSubmitProof): Promise<void>;
  afterFinalSubmit(outcome: FinalSubmitActivationOutcome): Promise<void>;
}

type FinalSubmitMarkerPhase =
  | "authority_acquired"
  | "activation_observed"
  | "activation_uncertain";

export function finalSubmitMarkerPath(runDirectory: string): string {
  if (!isAbsolute(runDirectory) || runDirectory.length > 4_096 || runDirectory.includes("\0")) {
    throw new FinalSubmitMarkerError("invalid_run_directory");
  }
  const normalizedDirectory = resolve(runDirectory);
  const markerPath = join(normalizedDirectory, FINAL_SUBMIT_MARKER_FILE);
  if (dirname(markerPath) !== normalizedDirectory) {
    throw new FinalSubmitMarkerError("invalid_run_directory");
  }
  return markerPath;
}

export async function finalSubmitMarkerExists(runDirectory: string): Promise<boolean> {
  const markerPath = finalSubmitMarkerPath(runDirectory);
  try {
    await lstat(markerPath);
    return true;
  } catch (error) {
    if (nodeErrorCode(error) === "ENOENT") return false;
    throw new FinalSubmitMarkerError("submit_marker_read_failed");
  }
}

export async function acquireFinalSubmitAuthority(
  runDirectory: string,
  now: () => Date = () => new Date(),
): Promise<FinalSubmitAuthority> {
  const markerPath = await createExclusiveMarker(runDirectory, "authority_acquired", now);
  return new FileFinalSubmitAuthority(markerPath, now);
}

async function createExclusiveMarker(
  runDirectory: string,
  phase: "authority_acquired",
  now: () => Date,
): Promise<string> {
  const markerPath = finalSubmitMarkerPath(runDirectory);
  const record = markerRecord(phase, now);
  let handle;
  try {
    handle = await open(
      markerPath,
      constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY | noFollowFlag(),
      0o600,
    );
  } catch (error) {
    if (nodeErrorCode(error) === "EEXIST") {
      throw new FinalSubmitMarkerError("submit_authority_exists");
    }
    throw new FinalSubmitMarkerError("submit_marker_write_failed");
  }

  try {
    await handle.chmod(0o600);
    await handle.writeFile(record, { encoding: "utf8" });
    await handle.sync();
  } catch {
    // The exclusive marker remains authoritative even when its first write is incomplete.
    throw new FinalSubmitMarkerError("submit_marker_write_failed");
  } finally {
    await handle.close().catch(() => undefined);
  }

  await syncDirectory(runDirectory);
  return markerPath;
}

export function durableFinalSubmitHooks(
  runDirectory: string,
  now: () => Date = () => new Date(),
): FinalSubmitHooks {
  let authority: FinalSubmitAuthority | undefined;
  return {
    async beforeFinalSubmit(_proof) {
      if (authority) throw new FinalSubmitMarkerError("submit_authority_exists");
      authority = await acquireFinalSubmitAuthority(runDirectory, now);
    },
    async afterFinalSubmit(outcome) {
      if (!authority) throw new FinalSubmitMarkerError("submit_activation_write_failed");
      await authority.recordActivation(outcome);
    },
  };
}

class FileFinalSubmitAuthority implements FinalSubmitAuthority {
  private activationRecorded = false;

  constructor(
    readonly markerPath: string,
    private readonly now: () => Date,
  ) {}

  async recordActivation(outcome: FinalSubmitActivationOutcome): Promise<void> {
    if (this.activationRecorded || !["activated", "activation_uncertain"].includes(outcome)) {
      throw new FinalSubmitMarkerError("submit_activation_write_failed");
    }
    this.activationRecorded = true;
    const phase = outcome === "activated" ? "activation_observed" : "activation_uncertain";
    const record = markerRecord(phase, this.now);
    let handle;
    try {
      handle = await open(
        this.markerPath,
        constants.O_APPEND | constants.O_WRONLY | noFollowFlag(),
        0o600,
      );
      await handle.chmod(0o600);
      const metadata = await handle.stat();
      if (metadata.size + Buffer.byteLength(record) > MAX_MARKER_BYTES) {
        throw new FinalSubmitMarkerError("submit_activation_write_failed");
      }
      await handle.writeFile(record, { encoding: "utf8" });
      await handle.sync();
    } catch (error) {
      if (error instanceof FinalSubmitMarkerError) throw error;
      throw new FinalSubmitMarkerError("submit_activation_write_failed");
    } finally {
      await handle?.close().catch(() => undefined);
    }
  }
}

function markerRecord(
  phase: FinalSubmitMarkerPhase,
  now: () => Date,
): string {
  const at = now();
  if (!Number.isFinite(at.getTime())) throw new FinalSubmitMarkerError("submit_marker_write_failed");
  return `${JSON.stringify({ phase, at: at.toISOString() })}\n`;
}

async function syncDirectory(runDirectory: string): Promise<void> {
  let handle;
  try {
    handle = await open(runDirectory, constants.O_RDONLY);
    await handle.sync();
  } catch (error) {
    if (!DIRECTORY_SYNC_UNSUPPORTED.has(nodeErrorCode(error) || "")) {
      throw new FinalSubmitMarkerError("submit_marker_write_failed");
    }
  } finally {
    await handle?.close().catch(() => undefined);
  }
}

function nodeErrorCode(error: unknown): string | undefined {
  return error && typeof error === "object" && "code" in error
    ? String((error as { code?: unknown }).code)
    : undefined;
}

function noFollowFlag(): number {
  return process.platform === "win32" ? 0 : constants.O_NOFOLLOW;
}
