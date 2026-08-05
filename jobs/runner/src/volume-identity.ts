import {
  createHash,
  createPrivateKey,
  createPublicKey,
  generateKeyPairSync,
  randomBytes,
  sign,
  type KeyObject,
  verify,
} from "node:crypto";
import {
  ensurePrivateRunnerDirectory,
  listSafeRunnerDirectory,
  readBoundedRegularFile,
  runnerPath,
  type RunnerDataRoot,
  writeDurableFileExclusive,
} from "./safe-runner-storage.js";

const IDENTITY_DIRECTORY = "volume-identity";
const IDENTITY_FILE = "ed25519-private.pk8";
const MAXIMUM_IDENTITY_BYTES = 512;
const ED25519_PRIVATE_KEY_DER_BYTES = 48;
const ED25519_PUBLIC_KEY_BYTES = 32;
const ED25519_SIGNATURE_BYTES = 64;

export type VolumeIdentityErrorCode =
  | "corrupt_identity"
  | "dirty_root_without_identity"
  | "invalid_encoding";

export class VolumeIdentityError extends Error {
  constructor(readonly code: VolumeIdentityErrorCode) {
    super({
      corrupt_identity: "The runner volume identity is corrupt.",
      dirty_root_without_identity: "A non-empty runner data root has no volume identity.",
      invalid_encoding: "An Ed25519 value is not canonically encoded.",
    }[code]);
    this.name = "VolumeIdentityError";
  }
}

export interface RunnerVolumeIdentity {
  readonly volumeId: string;
  readonly publicKeyRaw: string;
  readonly publicKeyFingerprint: string;
  readonly privateKey: KeyObject;
}

/**
 * Load the stable Ed25519 key for this data root. A key may be generated only
 * while the canonical root is provably empty; missing or corrupt identity
 * material on a root containing any entry is a fail-closed condition.
 */
export async function loadOrCreateRunnerVolumeIdentity(
  root: RunnerDataRoot,
): Promise<RunnerVolumeIdentity> {
  const identityPath = runnerPath(root, IDENTITY_DIRECTORY, IDENTITY_FILE);
  let encoded = await readBoundedRegularFile(root, identityPath, MAXIMUM_IDENTITY_BYTES);
  if (!encoded) {
    const entries = await listSafeRunnerDirectory(root, root.path);
    if (entries.length !== 0) {
      throw new VolumeIdentityError("dirty_root_without_identity");
    }

    await ensurePrivateRunnerDirectory(root, IDENTITY_DIRECTORY);
    const generated = generateKeyPairSync("ed25519").privateKey.export({
      format: "der",
      type: "pkcs8",
    });
    if (!Buffer.isBuffer(generated) || generated.length !== ED25519_PRIVATE_KEY_DER_BYTES) {
      throw new VolumeIdentityError("corrupt_identity");
    }
    await writeDurableFileExclusive(root, identityPath, generated);
    encoded = await readBoundedRegularFile(root, identityPath, MAXIMUM_IDENTITY_BYTES);
    if (!encoded) throw new VolumeIdentityError("corrupt_identity");
  }
  return parseVolumeIdentity(encoded);
}

export function createRunnerProcessInstanceId(): string {
  return randomBytes(32).toString("base64url");
}

export function ed25519PublicKeyFingerprint(publicKeyRaw: string): string {
  const bytes = decodeCanonicalBase64Url(publicKeyRaw, ED25519_PUBLIC_KEY_BYTES);
  return createHash("sha256").update(bytes).digest("hex");
}

export function signEd25519(privateKey: KeyObject, message: Uint8Array): string {
  if (privateKey.type !== "private" || privateKey.asymmetricKeyType !== "ed25519") {
    throw new VolumeIdentityError("corrupt_identity");
  }
  return sign(null, message, privateKey).toString("base64url");
}

export function verifyEd25519(
  publicKeyRaw: string,
  message: Uint8Array,
  signature: string,
): boolean {
  const raw = decodeCanonicalBase64Url(publicKeyRaw, ED25519_PUBLIC_KEY_BYTES);
  const signatureBytes = decodeCanonicalBase64Url(signature, ED25519_SIGNATURE_BYTES);
  const publicKey = createPublicKey({
    format: "jwk",
    key: { crv: "Ed25519", kty: "OKP", x: raw.toString("base64url") },
  });
  return verify(null, message, publicKey, signatureBytes);
}

export function decodeCanonicalBase64Url(value: string, expectedBytes: number): Buffer {
  if (typeof value !== "string"
    || !Number.isSafeInteger(expectedBytes)
    || expectedBytes <= 0
    || !/^[A-Za-z0-9_-]+$/.test(value)) {
    throw new VolumeIdentityError("invalid_encoding");
  }
  const decoded = Buffer.from(value, "base64url");
  if (decoded.length !== expectedBytes || decoded.toString("base64url") !== value) {
    throw new VolumeIdentityError("invalid_encoding");
  }
  return decoded;
}

function parseVolumeIdentity(encoded: Buffer): RunnerVolumeIdentity {
  try {
    if (encoded.length !== ED25519_PRIVATE_KEY_DER_BYTES) {
      throw new VolumeIdentityError("corrupt_identity");
    }
    const privateKey = createPrivateKey({ key: encoded, format: "der", type: "pkcs8" });
    if (privateKey.type !== "private" || privateKey.asymmetricKeyType !== "ed25519") {
      throw new VolumeIdentityError("corrupt_identity");
    }
    const canonical = privateKey.export({ format: "der", type: "pkcs8" });
    if (!Buffer.isBuffer(canonical) || !canonical.equals(encoded)) {
      throw new VolumeIdentityError("corrupt_identity");
    }
    const publicKey = createPublicKey(privateKey).export({ format: "jwk" });
    if (publicKey.kty !== "OKP" || publicKey.crv !== "Ed25519" || !publicKey.x) {
      throw new VolumeIdentityError("corrupt_identity");
    }
    const publicKeyBytes = decodeCanonicalBase64Url(publicKey.x, ED25519_PUBLIC_KEY_BYTES);
    const publicKeyRaw = publicKeyBytes.toString("base64url");
    const publicKeyFingerprint = ed25519PublicKeyFingerprint(publicKeyRaw);
    const volumeId = createHash("sha256")
      .update("bluey-jobs-runner\0volume-id-v1\0", "utf8")
      .update(publicKeyBytes)
      .digest("base64url");
    return { volumeId, publicKeyRaw, publicKeyFingerprint, privateKey };
  } catch (error) {
    if (error instanceof VolumeIdentityError && error.code === "corrupt_identity") throw error;
    throw new VolumeIdentityError("corrupt_identity");
  }
}
