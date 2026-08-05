import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { isAbsolute, join, normalize, parse } from "node:path";

const ADDON_FILE_NAME = "bluey_jobs_runner_native_storage.node";
const ADDON_URL = new URL(`./native/${ADDON_FILE_NAME}`, import.meta.url);
const COMPONENT_PATTERN = /^[A-Za-z0-9_-][A-Za-z0-9_.-]{0,159}$/;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const DECIMAL_PATTERN = /^(?:0|[1-9][0-9]*)$/;
const DEVICE_ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;

export type NativeRunnerStorageErrorCode =
  | "configuration"
  | "inventory_limit"
  | "io_failure"
  | "path_escape"
  | "root_changed"
  | "root_locked"
  | "unsafe_entry"
  | "unsafe_permissions"
  | "unsupported_platform"
  | "native_addon_missing"
  | "native_contract_invalid";

export type NativeRunnerStorageOperation =
  | "load"
  | "open_root"
  | "assert_root"
  | "ensure_directory"
  | "open_directory"
  | "move_entry_noreplace"
  | "ensure_child_directory"
  | "open_child_directory"
  | "write_file_exclusive"
  | "replace_file"
  | "read_file_bounded"
  | "inventory"
  | "remove_entry";

const NATIVE_ERROR_CODES = new Set<NativeRunnerStorageErrorCode>([
  "configuration",
  "inventory_limit",
  "io_failure",
  "path_escape",
  "root_changed",
  "root_locked",
  "unsafe_entry",
  "unsafe_permissions",
  "unsupported_platform",
]);

export class NativeRunnerStorageError extends Error {
  constructor(
    readonly operation: NativeRunnerStorageOperation,
    readonly code: NativeRunnerStorageErrorCode,
    options?: ErrorOptions,
  ) {
    super(`Native runner storage ${operation} failed (${code}).`, options);
    this.name = "NativeRunnerStorageError";
  }
}

export interface NativeRunnerInventoryEntry {
  readonly relativePath: string;
  readonly kind: "directory" | "file";
  readonly deviceId: string;
  readonly linkCount: number;
  readonly sizeBytes: number;
  readonly sha256: string;
}

export interface NativeRunnerInventory {
  readonly entries: readonly NativeRunnerInventoryEntry[];
  readonly count: number;
  readonly bytes: number;
  readonly sha256: string;
}

export interface NativeRunnerStorageDirectory {
  readonly relativePath: string;
  readonly canonicalPath: string;
  readonly deviceId: string;
  readonly linkCount: number;
  ensureChildDirectory(name: string): Promise<NativeRunnerStorageDirectory>;
  openChildDirectory(name: string): Promise<NativeRunnerStorageDirectory>;
  writeFileExclusive(name: string, contents: Buffer): Promise<boolean>;
  replaceFile(name: string, contents: Buffer): Promise<void>;
  readFileBounded(name: string, maximumBytes: number): Promise<Buffer>;
  inventory(): Promise<NativeRunnerInventory>;
  removeEntry(name: string): Promise<void>;
}

export interface NativeRunnerStorageRoot {
  readonly configuredPath: string;
  readonly deviceId: string;
  readonly linkCount: number;
  assertUnchanged(): void;
  ensureDirectory(components: readonly string[]): Promise<NativeRunnerStorageDirectory>;
  openDirectory(components: readonly string[]): Promise<NativeRunnerStorageDirectory>;
  moveEntryNoReplace(
    sourceComponents: readonly string[],
    destinationComponents: readonly string[],
  ): Promise<NativeRunnerMoveOutcome>;
}

export type NativeRunnerMoveOutcome = "destination_exists" | "moved" | "source_missing";

export interface NativeRunnerStorageFactory {
  openRoot(configuredRoot: string): NativeRunnerStorageRoot;
}

interface RawNativeRoot {
  readonly configuredPath: unknown;
  readonly deviceId: unknown;
  readonly linkCount: unknown;
  assertUnchanged: unknown;
  ensureDirectory: unknown;
  openDirectory: unknown;
  moveEntryNoReplace: unknown;
}

interface RawNativeDirectory {
  readonly relativePath: unknown;
  readonly canonicalPath: unknown;
  readonly deviceId: unknown;
  readonly linkCount: unknown;
  ensureChildDirectory: unknown;
  openChildDirectory: unknown;
  writeFileExclusive: unknown;
  replaceFile: unknown;
  readFileBounded: unknown;
  inventory: unknown;
  removeEntry: unknown;
}

type RawRootConstructor = new (configuredRoot: string) => RawNativeRoot;
type RawDirectoryConstructor = abstract new (...arguments_: never[]) => RawNativeDirectory;

interface RawNativeModule {
  readonly RunnerStorageRoot: RawRootConstructor;
  readonly RunnerStorageDirectory: RawDirectoryConstructor;
}

class ValidatedNativeRunnerStorageFactory implements NativeRunnerStorageFactory {
  constructor(private readonly nativeModule: RawNativeModule) {}

  openRoot(configuredRoot: string): NativeRunnerStorageRoot {
    assertConfiguredRoot(configuredRoot);
    let rawRoot: RawNativeRoot;
    try {
      rawRoot = new this.nativeModule.RunnerStorageRoot(configuredRoot);
    } catch (error) {
      throw translateNativeError("open_root", error);
    }
    if (!(rawRoot instanceof this.nativeModule.RunnerStorageRoot)) {
      throw contractError("open_root");
    }
    return new ValidatedNativeRunnerStorageRoot(this.nativeModule, rawRoot, configuredRoot);
  }
}

class ValidatedNativeRunnerStorageRoot implements NativeRunnerStorageRoot {
  constructor(
    private readonly nativeModule: RawNativeModule,
    private readonly rawRoot: RawNativeRoot,
    private readonly expectedPath: string,
  ) {
    assertRawRootShape(rawRoot, "open_root");
    if (rawRoot.configuredPath !== expectedPath) throw contractError("open_root");
  }

  get configuredPath(): string {
    return this.expectedPath;
  }

  get deviceId(): string {
    return parseDeviceId(this.rawRoot.deviceId, "assert_root");
  }

  get linkCount(): number {
    return parsePositiveCount(this.rawRoot.linkCount, "assert_root");
  }

  assertUnchanged(): void {
    const result = invokeSync(this.rawRoot, "assertUnchanged", [], "assert_root");
    if (result !== undefined) throw contractError("assert_root");
  }

  ensureDirectory(components: readonly string[]): Promise<NativeRunnerStorageDirectory> {
    return this.openDirectoryInternal(components, true);
  }

  openDirectory(components: readonly string[]): Promise<NativeRunnerStorageDirectory> {
    return this.openDirectoryInternal(components, false);
  }

  async moveEntryNoReplace(
    sourceComponents: readonly string[],
    destinationComponents: readonly string[],
  ): Promise<NativeRunnerMoveOutcome> {
    assertMoveComponents(sourceComponents, "move_entry_noreplace");
    assertMoveComponents(destinationComponents, "move_entry_noreplace");
    const result = await invokePromise(
      this.rawRoot,
      "moveEntryNoReplace",
      [[...sourceComponents], [...destinationComponents]],
      "move_entry_noreplace",
    );
    if (
      result !== "destination_exists" &&
      result !== "moved" &&
      result !== "source_missing"
    ) {
      throw contractError("move_entry_noreplace");
    }
    return result;
  }

  private async openDirectoryInternal(
    components: readonly string[],
    create: boolean,
  ): Promise<NativeRunnerStorageDirectory> {
    const operation = create ? "ensure_directory" : "open_directory";
    assertComponents(components, operation);
    const method = create ? "ensureDirectory" : "openDirectory";
    const rawDirectory = await invokePromise(
      this.rawRoot,
      method,
      [[...components]],
      operation,
    );
    const expectedRelativePath = components.join("/");
    const expectedCanonicalPath = join(this.expectedPath, ...components);
    return validateDirectory(
      this.nativeModule,
      rawDirectory,
      expectedRelativePath,
      expectedCanonicalPath,
      operation,
    );
  }
}

class ValidatedNativeRunnerStorageDirectory implements NativeRunnerStorageDirectory {
  constructor(
    private readonly nativeModule: RawNativeModule,
    private readonly rawDirectory: RawNativeDirectory,
    readonly relativePath: string,
    readonly canonicalPath: string,
  ) {}

  get deviceId(): string {
    return parseDeviceId(this.rawDirectory.deviceId, "assert_root");
  }

  get linkCount(): number {
    return parsePositiveCount(this.rawDirectory.linkCount, "assert_root");
  }

  ensureChildDirectory(name: string): Promise<NativeRunnerStorageDirectory> {
    return this.openChildDirectoryInternal(name, true);
  }

  openChildDirectory(name: string): Promise<NativeRunnerStorageDirectory> {
    return this.openChildDirectoryInternal(name, false);
  }

  async writeFileExclusive(name: string, contents: Buffer): Promise<boolean> {
    assertEntryName(name, "write_file_exclusive");
    assertBuffer(contents, "write_file_exclusive");
    const result = await invokePromise(
      this.rawDirectory,
      "writeFileExclusive",
      [name, contents],
      "write_file_exclusive",
    );
    if (typeof result !== "boolean") throw contractError("write_file_exclusive");
    return result;
  }

  async replaceFile(name: string, contents: Buffer): Promise<void> {
    assertEntryName(name, "replace_file");
    assertBuffer(contents, "replace_file");
    const result = await invokePromise(
      this.rawDirectory,
      "replaceFile",
      [name, contents],
      "replace_file",
    );
    if (result !== undefined) throw contractError("replace_file");
  }

  async readFileBounded(name: string, maximumBytes: number): Promise<Buffer> {
    assertEntryName(name, "read_file_bounded");
    if (!Number.isSafeInteger(maximumBytes) || maximumBytes < 1 || maximumBytes > 0xffff_ffff) {
      throw new NativeRunnerStorageError("read_file_bounded", "configuration");
    }
    const result = await invokePromise(
      this.rawDirectory,
      "readFileBounded",
      [name, maximumBytes],
      "read_file_bounded",
    );
    if (!Buffer.isBuffer(result)) throw contractError("read_file_bounded");
    if (result.byteLength > maximumBytes) throw contractError("read_file_bounded");
    return result;
  }

  async inventory(): Promise<NativeRunnerInventory> {
    const result = await invokePromise(this.rawDirectory, "inventory", [], "inventory");
    return parseInventory(result);
  }

  async removeEntry(name: string): Promise<void> {
    assertEntryName(name, "remove_entry");
    const result = await invokePromise(
      this.rawDirectory,
      "removeEntry",
      [name],
      "remove_entry",
    );
    if (result !== undefined) throw contractError("remove_entry");
  }

  private async openChildDirectoryInternal(
    name: string,
    create: boolean,
  ): Promise<NativeRunnerStorageDirectory> {
    const operation = create ? "ensure_child_directory" : "open_child_directory";
    assertEntryName(name, operation);
    const method = create ? "ensureChildDirectory" : "openChildDirectory";
    const rawDirectory = await invokePromise(
      this.rawDirectory,
      method,
      [name],
      operation,
    );
    const expectedRelativePath = this.relativePath ? `${this.relativePath}/${name}` : name;
    return validateDirectory(
      this.nativeModule,
      rawDirectory,
      expectedRelativePath,
      join(this.canonicalPath, name),
      operation,
    );
  }
}

let defaultFactory: NativeRunnerStorageFactory | undefined;

export function openNativeRunnerStorageRoot(configuredRoot: string): NativeRunnerStorageRoot {
  defaultFactory ??= new ValidatedNativeRunnerStorageFactory(loadDefaultNativeModule());
  return defaultFactory.openRoot(configuredRoot);
}

/** Test-only dependency injection; this does not replace or mutate the production module cache. */
export function createInjectedNativeRunnerStorageForTest(
  nativeModule: unknown,
): NativeRunnerStorageFactory {
  return new ValidatedNativeRunnerStorageFactory(validateNativeModule(nativeModule));
}

export function nativeRunnerStorageAddonPath(): string {
  return fileURLToPath(ADDON_URL);
}

function loadDefaultNativeModule(): RawNativeModule {
  if (process.platform !== "darwin" && process.platform !== "linux") {
    throw new NativeRunnerStorageError("load", "unsupported_platform");
  }
  const addonPath = nativeRunnerStorageAddonPath();
  try {
    return validateNativeModule(createRequire(import.meta.url)(addonPath));
  } catch (error) {
    if (error instanceof NativeRunnerStorageError) throw error;
    const code = isMissingAddonError(error, addonPath)
      ? "native_addon_missing"
      : "native_contract_invalid";
    throw new NativeRunnerStorageError("load", code, { cause: error });
  }
}

function validateNativeModule(value: unknown): RawNativeModule {
  if (!isRecord(value) || !hasExactKeys(value, ["RunnerStorageDirectory", "RunnerStorageRoot"])) {
    throw contractError("load");
  }
  if (typeof value.RunnerStorageRoot !== "function"
    || typeof value.RunnerStorageDirectory !== "function") {
    throw contractError("load");
  }
  return value as unknown as RawNativeModule;
}

function validateDirectory(
  nativeModule: RawNativeModule,
  value: unknown,
  expectedRelativePath: string,
  expectedCanonicalPath: string,
  operation: NativeRunnerStorageOperation,
): NativeRunnerStorageDirectory {
  if (!isRecord(value) || !(value instanceof nativeModule.RunnerStorageDirectory)) {
    throw contractError(operation);
  }
  const directory = value as unknown as RawNativeDirectory;
  assertRawDirectoryShape(directory, operation);
  if (directory.relativePath !== expectedRelativePath
    || directory.canonicalPath !== expectedCanonicalPath) {
    throw contractError(operation);
  }
  return new ValidatedNativeRunnerStorageDirectory(
    nativeModule,
    directory,
    expectedRelativePath,
    expectedCanonicalPath,
  );
}

function assertRawRootShape(
  root: RawNativeRoot,
  operation: NativeRunnerStorageOperation,
): void {
  if (typeof root.configuredPath !== "string"
    || typeof root.deviceId !== "string"
    || typeof root.linkCount !== "string"
    || typeof root.assertUnchanged !== "function"
    || typeof root.ensureDirectory !== "function"
    || typeof root.openDirectory !== "function"
    || typeof root.moveEntryNoReplace !== "function") {
    throw contractError(operation);
  }
}

function assertRawDirectoryShape(
  directory: RawNativeDirectory,
  operation: NativeRunnerStorageOperation,
): void {
  if (typeof directory.relativePath !== "string"
    || typeof directory.canonicalPath !== "string"
    || typeof directory.deviceId !== "string"
    || typeof directory.linkCount !== "string"
    || typeof directory.ensureChildDirectory !== "function"
    || typeof directory.openChildDirectory !== "function"
    || typeof directory.writeFileExclusive !== "function"
    || typeof directory.replaceFile !== "function"
    || typeof directory.readFileBounded !== "function"
    || typeof directory.inventory !== "function"
    || typeof directory.removeEntry !== "function") {
    throw contractError(operation);
  }
}

function invokeSync(
  receiver: object,
  method: string,
  arguments_: readonly unknown[],
  operation: NativeRunnerStorageOperation,
): unknown {
  const callable = Reflect.get(receiver, method);
  if (typeof callable !== "function") throw contractError(operation);
  try {
    return Reflect.apply(callable, receiver, arguments_);
  } catch (error) {
    throw translateNativeError(operation, error);
  }
}

async function invokePromise(
  receiver: object,
  method: string,
  arguments_: readonly unknown[],
  operation: NativeRunnerStorageOperation,
): Promise<unknown> {
  let result: unknown;
  try {
    result = invokeSync(receiver, method, arguments_, operation);
  } catch (error) {
    throw translateNativeError(operation, error);
  }
  if (!(result instanceof Promise)) throw contractError(operation);
  try {
    return await result;
  } catch (error) {
    throw translateNativeError(operation, error);
  }
}

function parseInventory(value: unknown): NativeRunnerInventory {
  if (!isRecord(value) || !hasExactKeys(value, ["bytes", "count", "entries", "sha256"])) {
    throw contractError("inventory");
  }
  if (!Array.isArray(value.entries)
    || !Number.isSafeInteger(value.count)
    || value.count !== value.entries.length
    || typeof value.sha256 !== "string"
    || !SHA256_PATTERN.test(value.sha256)) {
    throw contractError("inventory");
  }
  const bytes = parseDecimalBytes(value.bytes, "inventory");
  const entries = value.entries.map((entry) => parseInventoryEntry(entry));
  let priorPath: Buffer | undefined;
  let calculatedBytes = 0;
  for (const entry of entries) {
    const encodedPath = Buffer.from(entry.relativePath, "utf8");
    if (priorPath && Buffer.compare(encodedPath, priorPath) <= 0) {
      throw contractError("inventory");
    }
    priorPath = encodedPath;
    calculatedBytes += entry.sizeBytes;
    if (!Number.isSafeInteger(calculatedBytes)) throw contractError("inventory");
  }
  if (calculatedBytes !== bytes) throw contractError("inventory");
  return Object.freeze({ entries: Object.freeze(entries), count: value.count, bytes, sha256: value.sha256 });
}

function parseInventoryEntry(value: unknown): NativeRunnerInventoryEntry {
  if (!isRecord(value)
    || !hasExactKeys(value, [
      "deviceId",
      "kind",
      "linkCount",
      "relativePath",
      "sha256",
      "sizeBytes",
    ])
    || typeof value.relativePath !== "string"
    || !isSafeRelativePath(value.relativePath)
    || (value.kind !== "directory" && value.kind !== "file")
    || typeof value.sha256 !== "string"
    || !SHA256_PATTERN.test(value.sha256)) {
    throw contractError("inventory");
  }
  const sizeBytes = parseDecimalBytes(value.sizeBytes, "inventory");
  const deviceId = parseDeviceId(value.deviceId, "inventory");
  const linkCount = parsePositiveCount(value.linkCount, "inventory");
  if (value.kind === "directory" && sizeBytes !== 0) throw contractError("inventory");
  return Object.freeze({
    relativePath: value.relativePath,
    kind: value.kind,
    deviceId,
    linkCount,
    sizeBytes,
    sha256: value.sha256,
  });
}

function parseDecimalBytes(value: unknown, operation: NativeRunnerStorageOperation): number {
  if (typeof value !== "string" || !DECIMAL_PATTERN.test(value)) throw contractError(operation);
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) throw contractError(operation);
  return parsed;
}

function parsePositiveCount(value: unknown, operation: NativeRunnerStorageOperation): number {
  const parsed = parseDecimalBytes(value, operation);
  if (parsed < 1) throw contractError(operation);
  return parsed;
}

function parseDeviceId(value: unknown, operation: NativeRunnerStorageOperation): string {
  if (typeof value !== "string" || !DEVICE_ID_PATTERN.test(value)) {
    throw contractError(operation);
  }
  return value;
}

function assertConfiguredRoot(configuredRoot: string): void {
  if (typeof configuredRoot !== "string"
    || configuredRoot.includes("\0")
    || !isAbsolute(configuredRoot)
    || normalize(configuredRoot) !== configuredRoot
    || parse(configuredRoot).root === configuredRoot) {
    throw new NativeRunnerStorageError("open_root", "configuration");
  }
}

function assertComponents(
  components: readonly string[],
  operation: NativeRunnerStorageOperation,
): void {
  if (!Array.isArray(components)) {
    throw new NativeRunnerStorageError(operation, "configuration");
  }
  for (const component of components) assertControlledComponent(component, operation);
}

function assertMoveComponents(
  components: readonly string[],
  operation: NativeRunnerStorageOperation,
): void {
  if (!Array.isArray(components) || components.length === 0) {
    throw new NativeRunnerStorageError(operation, "configuration");
  }
  assertComponents(components.slice(0, -1), operation);
  assertEntryName(components.at(-1)!, operation);
}

function assertControlledComponent(
  component: string,
  operation: NativeRunnerStorageOperation,
): void {
  if (typeof component !== "string" || !COMPONENT_PATTERN.test(component)) {
    throw new NativeRunnerStorageError(operation, "path_escape");
  }
}

function assertEntryName(name: string, operation: NativeRunnerStorageOperation): void {
  if (typeof name !== "string"
    || !isSafeInventoryComponent(name)
    || name.includes("/")
    || name === ".bluey-runner-storage.lock"
    || /^\.bluey-stage-[0-9]+-[0-9a-fA-F]{16}$/.test(name)) {
    throw new NativeRunnerStorageError(operation, "path_escape");
  }
}

function assertBuffer(value: unknown, operation: NativeRunnerStorageOperation): asserts value is Buffer {
  if (!Buffer.isBuffer(value)) throw new NativeRunnerStorageError(operation, "configuration");
}

function isSafeRelativePath(value: string): boolean {
  return value.length > 0 && value.split("/").every(isSafeInventoryComponent);
}

function isSafeInventoryComponent(component: string): boolean {
  return component.length > 0
    && component !== "."
    && component !== ".."
    && Buffer.byteLength(component, "utf8") <= 255
    && !/[\\\u0000-\u001f\u007f]/.test(component);
}

function translateNativeError(
  operation: NativeRunnerStorageOperation,
  error: unknown,
): NativeRunnerStorageError {
  if (error instanceof NativeRunnerStorageError) return error;
  const message = error instanceof Error ? error.message : "";
  const match = /^bluey_runner_storage:([a-z_]+)$/.exec(message);
  if (match && NATIVE_ERROR_CODES.has(match[1] as NativeRunnerStorageErrorCode)) {
    return new NativeRunnerStorageError(operation, match[1] as NativeRunnerStorageErrorCode, {
      cause: error,
    });
  }
  return contractError(operation, error);
}

function contractError(
  operation: NativeRunnerStorageOperation,
  cause?: unknown,
): NativeRunnerStorageError {
  return new NativeRunnerStorageError(operation, "native_contract_invalid", { cause });
}

function hasExactKeys(value: object, expected: readonly string[]): boolean {
  const actual = Object.getOwnPropertyNames(value).sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isMissingAddonError(error: unknown, addonPath: string): boolean {
  return isRecord(error)
    && error.code === "MODULE_NOT_FOUND"
    && typeof error.message === "string"
    && error.message.includes(addonPath);
}
