import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import type {
  ExactSubmitExpectation,
  ExactSubmitFieldEvidence,
  ExactSubmitFileEvidence,
  ExactSubmitPartOrderEntry,
} from "./contracts.js";
import { FORM_FILE_READBACK_LIMITS } from "./form-readback.js";

const MAX_MULTIPART_OVERHEAD_BYTES = 4 * 1024 * 1024;
const MAX_MULTIPART_HEADER_BYTES = 16 * 1024;

export const EXACT_SUBMIT_FIELD_LIMITS = Object.freeze({
  maxFieldCount: 256,
  maxFieldNameChars: 240,
  maxValueBytes: 64 * 1024,
  maxAggregateValueBytes: 512 * 1024,
});

export function isSuccessfulExactSubmitHttpStatus(value: unknown): value is number {
  return typeof value === "number"
    && Number.isSafeInteger(value)
    && ((value >= 200 && value <= 299)
      || value === 301
      || value === 302
      || value === 303
      || value === 307
      || value === 308);
}

export class ExactSubmitEvidenceError extends Error {
  constructor() {
    super("Outgoing application submit evidence did not match the approved form");
    this.name = "ExactSubmitEvidenceError";
  }
}

export class AdditionalSubmitRequestError extends Error {
  constructor() {
    super("Application submit activation emitted more than one mutating request");
    this.name = "AdditionalSubmitRequestError";
  }
}

export interface OutgoingSubmitRequestEvidence {
  url: string;
  method: string;
  contentType: string;
  body: Uint8Array | null;
  headers?: Readonly<Record<string, string>>;
}

export type OwnedExactSubmitPart =
  | { kind: "field"; fieldName: string; value: string }
  | {
      kind: "file";
      fieldName: string;
      name: string;
      mediaType: "application/pdf";
      bytes: Uint8Array;
    };

export interface OwnedExactSubmitRequest {
  parts: ReadonlyArray<Readonly<OwnedExactSubmitPart>>;
}

export function assertExactOutgoingSubmit(
  expectation: ExactSubmitExpectation,
  request: OutgoingSubmitRequestEvidence,
): void {
  try {
    assertExpectedFiles(expectation.files);
    assertExpectedFields(expectation.fields);
    assertNoCrossTypeFieldOverlap(expectation.files, expectation.fields);
    assertExpectedPartOrder(expectation.partOrder, expectation.fields, expectation.files);
    assertRequestIdentity(expectation, request);
    const boundary = multipartBoundary(request.contentType);
    const actual = multipartEvidence(request.body, boundary);
    if (actual.parts.length !== expectation.files.length + expectation.fields.length) {
      throw new Error("part evidence");
    }
    const actualFiles = actual.files;
    if (actualFiles.length !== expectation.files.length
      || actualFiles.some((actual, index) => {
        const expected = expectation.files[index];
        return actual.fieldName !== expected?.fieldName
          || actual.name !== expected.name
          || actual.byteLength !== expected.byteLength
          || actual.sha256 !== expected.sha256;
      })) {
      throw new Error("file evidence");
    }
    if (actual.fields.length !== expectation.fields.length
      || actual.fields.some((field, index) => {
        const expected = expectation.fields[index];
        return field.fieldName !== expected?.fieldName
          || field.valueByteLength !== expected.valueByteLength
          || field.valueSha256 !== expected.valueSha256;
      })) {
      throw new Error("field evidence");
    }
    if (!samePartOrder(actual.partOrder, expectation.partOrder)) {
      throw new Error("global part order");
    }
  } catch {
    throw new ExactSubmitEvidenceError();
  }
}

/**
 * Freeze the values and Node-owned document bytes that may hydrate Chromium's
 * intercepted multipart request. Delivery keeps the browser's original
 * boundary and header encoding.
 */
export function createOwnedExactSubmitRequest(
  expectation: ExactSubmitExpectation,
  parts: ReadonlyArray<Readonly<OwnedExactSubmitPart>>,
): OwnedExactSubmitRequest {
  try {
    assertExpectedFiles(expectation.files);
    assertExpectedFields(expectation.fields);
    assertNoCrossTypeFieldOverlap(expectation.files, expectation.fields);
    assertExpectedPartOrder(expectation.partOrder, expectation.fields, expectation.files);
    if (parts.length !== expectation.files.length + expectation.fields.length) {
      throw new Error("owned part count");
    }
    const fields: ExactSubmitFieldEvidence[] = [];
    const files: ExactSubmitFileEvidence[] = [];
    const ownedParts = parts.map((part) => {
      if (part.kind === "field") {
        assertFieldName(part.fieldName);
        const value = normalizeMultipartTextValue(part.value);
        const bytes = Buffer.from(value, "utf8");
        fields.push({
          fieldName: part.fieldName,
          valueByteLength: bytes.byteLength,
          valueSha256: createHash("sha256").update(bytes).digest("hex"),
        });
        return Object.freeze({ kind: "field" as const, fieldName: part.fieldName, value });
      }
      assertFieldName(part.fieldName);
      if (part.mediaType !== "application/pdf") throw new Error("owned file media type");
      const bytes = Buffer.from(part.bytes.buffer, part.bytes.byteOffset, part.bytes.byteLength);
      const file = {
        fieldName: part.fieldName,
        name: part.name,
        byteLength: bytes.byteLength,
        sha256: createHash("sha256").update(bytes).digest("hex"),
      };
      assertExpectedFiles([file]);
      files.push(file);
      return Object.freeze({
        kind: "file" as const,
        fieldName: part.fieldName,
        name: part.name,
        mediaType: "application/pdf" as const,
        bytes: Buffer.from(bytes),
      });
    });
    if (!sameFiles(files, expectation.files) || !sameFields(fields, expectation.fields)) {
      throw new Error("owned evidence");
    }
    const partOrder = ownedPartOrder(ownedParts);
    if (!samePartOrder(partOrder, expectation.partOrder)) throw new Error("owned part order");
    return Object.freeze({
      parts: Object.freeze(ownedParts),
    });
  } catch {
    throw new ExactSubmitEvidenceError();
  }
}

/**
 * Validate the page-generated request's exact ordered shape and hydrate only
 * upload bodies omitted by Chromium's interception API. The browser-generated
 * boundary and all multipart headers are retained byte-for-byte.
 */
export function hydrateExactOutgoingSubmit(
  expectation: ExactSubmitExpectation,
  owned: OwnedExactSubmitRequest,
  request: OutgoingSubmitRequestEvidence,
): Buffer {
  try {
    assertExpectedFiles(expectation.files);
    assertExpectedFields(expectation.fields);
    assertNoCrossTypeFieldOverlap(expectation.files, expectation.fields);
    assertExpectedPartOrder(expectation.partOrder, expectation.fields, expectation.files);
    assertRequestIdentity(expectation, request);
    const boundary = multipartBoundary(request.contentType);
    const actual = multipartEvidence(request.body, boundary);
    if (actual.parts.length !== owned.parts.length) throw new Error("ordered part count");
    for (let index = 0; index < owned.parts.length; index += 1) {
      const expected = owned.parts[index]!;
      const observed = actual.parts[index]!;
      if (expected.kind !== observed.kind
        || expected.fieldName !== observed.fieldName) {
        throw new Error("ordered part identity");
      }
      if (expected.kind === "field" && observed.kind === "field") {
        if (!observed.bytes.equals(Buffer.from(expected.value, "utf8"))) {
          throw new Error("ordered field bytes");
        }
        continue;
      }
      if (expected.kind === "file" && observed.kind === "file") {
        const expectedBytes = Buffer.from(
          expected.bytes.buffer,
          expected.bytes.byteOffset,
          expected.bytes.byteLength,
        );
        if (observed.name !== expected.name
          || observed.mediaType !== expected.mediaType
          || observed.bytes.byteLength !== 0 && !observed.bytes.equals(expectedBytes)) {
          throw new Error("ordered file evidence");
        }
        continue;
      }
      throw new Error("ordered part kind");
    }
    const sourceBody = Buffer.from(
      request.body.buffer,
      request.body.byteOffset,
      request.body.byteLength,
    );
    const chunks: Buffer[] = [];
    let cursor = 0;
    let omittedFileBytes = 0;
    for (let index = 0; index < owned.parts.length; index += 1) {
      const expected = owned.parts[index]!;
      const observed = actual.parts[index]!;
      if (expected.kind !== "file" || observed.kind !== "file"
        || observed.bytes.byteLength !== 0) {
        continue;
      }
      chunks.push(sourceBody.subarray(cursor, observed.bodyStart));
      chunks.push(Buffer.from(
        expected.bytes.buffer,
        expected.bytes.byteOffset,
        expected.bytes.byteLength,
      ));
      omittedFileBytes += expected.bytes.byteLength;
      cursor = observed.bodyEnd;
    }
    chunks.push(sourceBody.subarray(cursor));
    const hydrated = Buffer.concat(chunks);
    const browserMultipartByteLength = sourceBody.byteLength + omittedFileBytes;
    if (!Number.isSafeInteger(browserMultipartByteLength)
      || hydrated.byteLength !== browserMultipartByteLength) {
      throw new Error("browser multipart length");
    }
    if (hydrated.byteLength > FORM_FILE_READBACK_LIMITS.maxAggregateBytes
      + MAX_MULTIPART_OVERHEAD_BYTES) {
      throw new Error("hydrated body bytes");
    }
    assertBrowserContentLength(request.headers ?? {}, browserMultipartByteLength);
    assertExactOutgoingSubmit(expectation, { ...request, body: hydrated });
    return hydrated;
  } catch {
    throw new ExactSubmitEvidenceError();
  }
}

/** @deprecated Use hydrateExactOutgoingSubmit when releasing a request. */
export function assertExactOutgoingSubmitShape(
  expectation: ExactSubmitExpectation,
  owned: OwnedExactSubmitRequest,
  request: OutgoingSubmitRequestEvidence,
): void {
  hydrateExactOutgoingSubmit(expectation, owned, request);
}

function assertExpectedFields(fields: readonly ExactSubmitFieldEvidence[]): void {
  let aggregateBytes = 0;
  if (fields.length < 1 || fields.length > EXACT_SUBMIT_FIELD_LIMITS.maxFieldCount) {
    throw new Error("field count");
  }
  for (const field of fields) {
    assertFieldName(field.fieldName);
    if (!Number.isSafeInteger(field.valueByteLength)
      || field.valueByteLength < 0
      || field.valueByteLength > EXACT_SUBMIT_FIELD_LIMITS.maxValueBytes
      || !/^[a-f0-9]{64}$/u.test(field.valueSha256)) {
      throw new Error("field identity");
    }
    aggregateBytes += field.valueByteLength;
  }
  if (!Number.isSafeInteger(aggregateBytes)
    || aggregateBytes > EXACT_SUBMIT_FIELD_LIMITS.maxAggregateValueBytes) {
    throw new Error("aggregate field bytes");
  }
}

function assertNoMethodOverrideSemantics(
  rawUrl: string,
  headers: Readonly<Record<string, string>>,
): void {
  const forbidden = new Set(["x-http-method-override", "x-http-method", "x-method-override"]);
  if (Object.keys(headers).some((name) => forbidden.has(name.toLowerCase())
    || name.toLowerCase() === "content-encoding")) {
    throw new Error("method override");
  }
  const url = new URL(rawUrl);
  if (Array.from(url.searchParams.keys()).some(isMethodOverrideName)) {
    throw new Error("method override");
  }
}

function assertExpectedFiles(files: readonly ExactSubmitFileEvidence[]): void {
  let aggregateBytes = 0;
  if (files.length < 1 || files.length > FORM_FILE_READBACK_LIMITS.maxFileCount) {
    throw new Error("file count");
  }
  for (const file of files) {
    assertFieldName(file.fieldName);
    if (!/^(?:resume|cover-letter)-[a-f0-9]{64}\.pdf$/u.test(file.name)
      || file.name !== `resume-${file.sha256}.pdf`
        && file.name !== `cover-letter-${file.sha256}.pdf`
      || !/^[a-f0-9]{64}$/u.test(file.sha256)
      || !Number.isSafeInteger(file.byteLength)
      || file.byteLength < 1
      || file.byteLength > FORM_FILE_READBACK_LIMITS.maxFileBytes) {
      throw new Error("file identity");
    }
    aggregateBytes += file.byteLength;
  }
  if (!Number.isSafeInteger(aggregateBytes)
    || aggregateBytes > FORM_FILE_READBACK_LIMITS.maxAggregateBytes) {
    throw new Error("aggregate bytes");
  }
}

function multipartBoundary(contentType: string): string {
  if (contentType.length > 512 || contentType.trim() !== contentType) {
    throw new Error("content type");
  }
  const prefix = /^multipart\/form-data/iu.exec(contentType);
  if (!prefix) throw new Error("content type");
  let remainder = contentType.slice(prefix[0].length);
  let boundary: string | undefined;
  while (remainder.trim()) {
    const parameter = /^\s*;\s*([A-Za-z0-9!#$%&'*+.^_`|~-]+)\s*=\s*(?:"([^"\\\r\n]{1,70})"|([0-9A-Za-z'()+_,\-./:=?]{1,70}))/u
      .exec(remainder);
    if (!parameter
      || parameter[1]!.toLowerCase() !== "boundary"
      || boundary !== undefined) {
      throw new Error("boundary parameter");
    }
    boundary = parameter[2] ?? parameter[3];
    remainder = remainder.slice(parameter[0].length);
  }
  if (!boundary || !/^[0-9A-Za-z'()+_,\-./:=?]{1,70}$/u.test(boundary)) {
    throw new Error("boundary");
  }
  return boundary;
}

function multipartEvidence(
  bodyBytes: Uint8Array,
  boundary: string,
): {
  files: ExactSubmitFileEvidence[];
  fields: ExactSubmitFieldEvidence[];
  parts: ParsedMultipartPart[];
  partOrder: ExactSubmitPartOrderEntry[];
} {
  const body = Buffer.from(bodyBytes.buffer, bodyBytes.byteOffset, bodyBytes.byteLength);
  const delimiter = Buffer.from(`--${boundary}`, "ascii");
  const nextDelimiter = Buffer.from(`\r\n--${boundary}`, "ascii");
  const headerTerminator = Buffer.from("\r\n\r\n", "ascii");
  const lineBreak = Buffer.from("\r\n", "ascii");
  const files: ExactSubmitFileEvidence[] = [];
  const fields: ExactSubmitFieldEvidence[] = [];
  const parts: ParsedMultipartPart[] = [];
  const partOrder: ExactSubmitPartOrderEntry[] = [];
  let aggregateFieldBytes = 0;
  let cursor = 0;
  while (cursor < body.length) {
    if (!body.subarray(cursor, cursor + delimiter.length).equals(delimiter)) {
      throw new Error("delimiter");
    }
    cursor += delimiter.length;
    if (body.subarray(cursor, cursor + 2).equals(Buffer.from("--", "ascii"))) {
      cursor += 2;
      if (cursor === body.length) return { files, fields, parts, partOrder };
      if (body.subarray(cursor, cursor + 2).equals(lineBreak) && cursor + 2 === body.length) {
        return { files, fields, parts, partOrder };
      }
      throw new Error("closing delimiter");
    }
    if (!body.subarray(cursor, cursor + 2).equals(lineBreak)) throw new Error("part line break");
    cursor += 2;
    const headerEnd = body.indexOf(headerTerminator, cursor);
    if (headerEnd < 0 || headerEnd - cursor > MAX_MULTIPART_HEADER_BYTES) {
      throw new Error("part headers");
    }
    const headers = body.subarray(cursor, headerEnd).toString("latin1");
    const partStart = headerEnd + headerTerminator.length;
    const partEnd = body.indexOf(nextDelimiter, partStart);
    if (partEnd < 0) throw new Error("part body");
    const headerLines = headers.split("\r\n");
    const parsedHeaders = headerLines.map((line) => {
      const parsed = /^([A-Za-z0-9!#$%&'*+.^_`|~-]+):[ \t]*([^\r\n]*)$/u.exec(line);
      if (!parsed) throw new Error("part header");
      return { name: parsed[1]!.toLowerCase(), value: parsed[2]! };
    });
    const dispositionLines = parsedHeaders
      .filter((header) => header.name === "content-disposition");
    if (dispositionLines.length !== 1
      || !/^form-data(?:;|$)/iu.test(dispositionLines[0]!.value)) {
      throw new Error("content disposition");
    }
    const parameters = dispositionParameters(
      `content-disposition:${dispositionLines[0]!.value}`,
    );
    const fieldName = parameters.name;
    const fileName = parameters.filename;
    const bytes = body.subarray(partStart, partEnd);
    if (fileName !== undefined) {
      if (parsedHeaders.length !== 2
        || parsedHeaders[0]?.name !== "content-disposition"
        || parsedHeaders[1]?.name !== "content-type"
        || parsedHeaders[1]?.value.toLowerCase() !== "application/pdf") {
        throw new Error("file content type");
      }
      parts.push({
        kind: "file",
        fieldName,
        name: fileName,
        mediaType: parsedHeaders[1]!.value.toLowerCase(),
        bytes,
        bodyStart: partStart,
        bodyEnd: partEnd,
      });
      partOrder.push({ kind: "file", index: files.length });
      files.push({
        fieldName,
        name: fileName,
        byteLength: bytes.byteLength,
        sha256: createHash("sha256").update(bytes).digest("hex"),
      });
    } else {
      if (parsedHeaders.length !== 1
        || parsedHeaders[0]?.name !== "content-disposition"
        || isMethodOverrideName(fieldName)) {
        throw new Error("text part headers");
      }
      if (bytes.byteLength > EXACT_SUBMIT_FIELD_LIMITS.maxValueBytes) {
        throw new Error("field bytes");
      }
      fields.push({
        fieldName,
        valueByteLength: bytes.byteLength,
        valueSha256: createHash("sha256").update(bytes).digest("hex"),
      });
      partOrder.push({ kind: "field", index: fields.length - 1 });
      parts.push({ kind: "field", fieldName, bytes, bodyStart: partStart, bodyEnd: partEnd });
      aggregateFieldBytes += bytes.byteLength;
      if (fields.length > EXACT_SUBMIT_FIELD_LIMITS.maxFieldCount) {
        throw new Error("field count");
      }
      if (!Number.isSafeInteger(aggregateFieldBytes)
        || aggregateFieldBytes > EXACT_SUBMIT_FIELD_LIMITS.maxAggregateValueBytes) {
        throw new Error("aggregate field bytes");
      }
    }
    cursor = partEnd + 2;
  }
  throw new Error("missing closing delimiter");
}

type ParsedMultipartPart =
  | {
      kind: "field";
      fieldName: string;
      bytes: Buffer;
      bodyStart: number;
      bodyEnd: number;
    }
  | {
      kind: "file";
      fieldName: string;
      name: string;
      mediaType: string;
      bytes: Buffer;
      bodyStart: number;
      bodyEnd: number;
    };

function assertRequestIdentity(
  expectation: ExactSubmitExpectation,
  request: OutgoingSubmitRequestEvidence,
): asserts request is OutgoingSubmitRequestEvidence & { body: Uint8Array } {
  assertNoMethodOverrideSemantics(request.url, request.headers ?? {});
  if (request.url !== expectation.target.actionUrl
    || request.method.toLowerCase() !== expectation.target.method
    || expectation.target.method !== "post"
    || expectation.target.enctype !== "multipart/form-data"
    || expectation.target.formTarget !== "_self"
    || !request.body
    || request.body.byteLength > FORM_FILE_READBACK_LIMITS.maxAggregateBytes
      + MAX_MULTIPART_OVERHEAD_BYTES) {
    throw new Error("request identity");
  }
}

function assertFieldName(fieldName: string): void {
  if (!/^[A-Za-z0-9_.:[\]-]{1,240}$/u.test(fieldName)) {
    throw new Error("field identity");
  }
}

function assertNoCrossTypeFieldOverlap(
  files: readonly ExactSubmitFileEvidence[],
  fields: readonly ExactSubmitFieldEvidence[],
): void {
  const fileNames = new Set(files.map((file) => file.fieldName));
  if (fields.some((field) => fileNames.has(field.fieldName))) {
    throw new Error("cross-type field overlap");
  }
}

function assertExpectedPartOrder(
  partOrder: readonly ExactSubmitPartOrderEntry[],
  fields: readonly ExactSubmitFieldEvidence[],
  files: readonly ExactSubmitFileEvidence[],
): void {
  if (!Array.isArray(partOrder) || partOrder.length !== fields.length + files.length) {
    throw new Error("part order count");
  }
  const fieldIndexes = new Set<number>();
  const fileIndexes = new Set<number>();
  for (const entry of partOrder) {
    if (!entry || (entry.kind !== "field" && entry.kind !== "file")
      || !Number.isSafeInteger(entry.index)
      || entry.index < 0) {
      throw new Error("part order entry");
    }
    const indexes = entry.kind === "field" ? fieldIndexes : fileIndexes;
    const limit = entry.kind === "field" ? fields.length : files.length;
    if (entry.index >= limit || indexes.has(entry.index)) throw new Error("part order index");
    indexes.add(entry.index);
  }
  if (fieldIndexes.size !== fields.length || fileIndexes.size !== files.length) {
    throw new Error("part order coverage");
  }
}

function ownedPartOrder(
  parts: readonly Readonly<OwnedExactSubmitPart>[],
): ExactSubmitPartOrderEntry[] {
  let fieldIndex = 0;
  let fileIndex = 0;
  return parts.map((part) => part.kind === "field"
    ? { kind: "field", index: fieldIndex++ }
    : { kind: "file", index: fileIndex++ });
}

function samePartOrder(
  actual: readonly ExactSubmitPartOrderEntry[],
  expected: readonly ExactSubmitPartOrderEntry[],
): boolean {
  return actual.length === expected.length && actual.every((entry, index) => {
    const item = expected[index];
    return entry.kind === item?.kind && entry.index === item.index;
  });
}

function assertBrowserContentLength(
  headers: Readonly<Record<string, string>>,
  byteLength: number,
): void {
  const entries = Object.entries(headers)
    .filter(([name]) => name.toLowerCase() === "content-length");
  if (entries.length > 1) throw new Error("content length");
  if (entries.length === 0) return;
  const value = entries[0]![1];
  if (!/^(?:0|[1-9][0-9]*)$/u.test(value)) throw new Error("content length");
  const expected = Number(value);
  if (!Number.isSafeInteger(expected) || expected !== byteLength) {
    throw new Error("content length");
  }
}

function sameFiles(
  actual: readonly ExactSubmitFileEvidence[],
  expected: readonly ExactSubmitFileEvidence[],
): boolean {
  return actual.length === expected.length && actual.every((file, index) => {
    const item = expected[index];
    return file.fieldName === item?.fieldName
      && file.name === item.name
      && file.byteLength === item.byteLength
      && file.sha256 === item.sha256;
  });
}

function sameFields(
  actual: readonly ExactSubmitFieldEvidence[],
  expected: readonly ExactSubmitFieldEvidence[],
): boolean {
  return actual.length === expected.length && actual.every((field, index) => {
    const item = expected[index];
    return field.fieldName === item?.fieldName
      && field.valueByteLength === item.valueByteLength
      && field.valueSha256 === item.valueSha256;
  });
}

function normalizeMultipartTextValue(value: string): string {
  return value.replace(/\r\n|\r|\n/gu, "\r\n");
}

function isMethodOverrideName(name: string): boolean {
  return /^(?:_method|method|x-http-method-override|x-method-override)$/iu.test(name);
}

function dispositionParameters(disposition: string): { name: string; filename?: string } {
  const prefix = /^content-disposition:\s*form-data/iu.exec(disposition);
  if (!prefix) throw new Error("content disposition");
  const parameters = new Map<string, string>();
  let remainder = disposition.slice(prefix[0].length);
  while (remainder.trim()) {
    const parameter = /^\s*;\s*([A-Za-z0-9!#$%&'*+.^_`|~-]+)\s*=\s*"([^"\\\r\n]{0,255})"/u
      .exec(remainder);
    if (!parameter) throw new Error("invalid disposition parameter");
    const key = parameter[1]!.toLowerCase();
    if ((key !== "name" && key !== "filename") || parameters.has(key)) {
      throw new Error("unsupported disposition parameter");
    }
    parameters.set(key, parameter[2]!);
    remainder = remainder.slice(parameter[0].length);
  }
  const name = parameters.get("name");
  if (!name
    || name.length > 240
    || /[\u0000-\u001f\u007f]/u.test(name)) {
    throw new Error("invalid field name");
  }
  const filename = parameters.get("filename");
  if (filename !== undefined
    && (filename.length > FORM_FILE_READBACK_LIMITS.maxFileNameChars
      || /[\u0000-\u001f\u007f]/u.test(filename))) {
    throw new Error("invalid file name");
  }
  return filename === undefined ? { name } : { name, filename };
}
