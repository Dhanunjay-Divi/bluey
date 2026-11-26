import type {
  ExactSubmitFileEvidence,
  FormControl,
  FormFileEvidence,
  FormControlKind,
  TrustedSubmitFieldValue,
  ValidationIssue,
} from "./contracts.js";

export const FORM_FILE_READBACK_LIMITS = Object.freeze({
  maxFileCount: 2,
  maxFileBytes: 12 * 1024 * 1024,
  maxAggregateBytes: 24 * 1024 * 1024,
  maxFileNameChars: 255,
} as const);

export interface ExpectedFormFileEvidence {
  name: string;
  byteLength: number | null;
  sha256: string;
}

export interface FormFillExpectation {
  selector: string;
  kind: FormControlKind;
  field: string;
  controlName?: string;
  expectedValue?: string;
  expectedChecked?: boolean;
  expectedSubmitValue?: string;
  expectedFiles?: ExpectedFormFileEvidence[];
}

export function valueExpectation(
  control: FormControl,
  field: string,
  expectedValue: string,
): FormFillExpectation {
  return Object.freeze({
    selector: control.selector,
    kind: control.kind,
    field,
    controlName: control.name,
    expectedValue,
    expectedSubmitValue: expectedValue,
  });
}

export function checkedExpectation(
  control: FormControl,
  field: string,
  expectedChecked: boolean,
): FormFillExpectation {
  return Object.freeze({
    selector: control.selector,
    kind: control.kind,
    field,
    controlName: control.name,
    expectedChecked,
    expectedSubmitValue: expectedChecked ? (control.value || "on") : undefined,
  });
}

export function fileExpectation(
  control: FormControl,
  field: string,
  expectedPaths: string | readonly string[],
  selectedFiles: readonly FormFileEvidence[] = control.files ?? [],
): FormFillExpectation {
  const paths = typeof expectedPaths === "string" ? [expectedPaths] : [...expectedPaths];
  const expectedFiles = paths.map((path, index) => {
    const identity = snapshotFileIdentity(path);
    const selected = selectedFiles[index];
    const exactIdentity = selected?.name === identity.name
      && selected.sha256 === identity.sha256
      && validSelectedFileByteLength(selected.byteLength);
    return {
      ...identity,
      // The exact DOM File size becomes expected evidence only when its bytes
      // match the immutable content address. Size can never stand in for hash.
      byteLength: exactIdentity ? selected.byteLength : null,
    };
  });
  return Object.freeze({
    selector: control.selector,
    kind: control.kind,
    field,
    controlName: control.name,
    expectedFiles,
  });
}

export function exactSubmitTrustedFieldValues(
  expectations: readonly FormFillExpectation[],
): ReadonlyArray<Readonly<TrustedSubmitFieldValue>> {
  const values: TrustedSubmitFieldValue[] = [];
  for (const expectation of expectations) {
    if (expectation.expectedFiles !== undefined || expectation.expectedChecked === false) continue;
    const value = expectation.expectedSubmitValue;
    if (value === undefined) continue;
    const fieldName = expectation.controlName;
    if (!fieldName
      || fieldName.length > 240
      || /[\u0000-\u001f\u007f]/u.test(fieldName)) {
      throw new Error("Application answer submit field is invalid");
    }
    values.push(Object.freeze({ fieldName, value }));
  }
  return Object.freeze(values);
}

export function snapshotFileIdentity(path: string): Pick<FormFileEvidence, "name" | "sha256"> {
  const name = fileName(path);
  const match = /^(?:resume|cover-letter)-([a-f0-9]{64})\.pdf$/u.exec(name);
  if (!match || name.length > FORM_FILE_READBACK_LIMITS.maxFileNameChars) {
    throw new Error("Application document path is not an immutable content-addressed snapshot");
  }
  return { name, sha256: match[1]! };
}

export function verifyFillExpectations(
  expectations: readonly FormFillExpectation[],
  controls: readonly FormControl[],
  provider: string,
): ValidationIssue[] {
  const currentBySelector = new Map(controls.map((control) => [control.selector, control]));
  const issues: ValidationIssue[] = [];
  const seen = new Set<string>();

  for (const expectation of expectations) {
    const control = currentBySelector.get(expectation.selector);
    if (control && expectationMatches(expectation, control)) continue;

    const key = `${expectation.selector}\u0000${expectation.field}`;
    if (seen.has(key)) continue;
    seen.add(key);
    issues.push({
      field: expectation.field,
      message: `${provider} did not register Bluey's prepared value for ${expectation.field}.`,
      severity: "blocking",
    });
  }

  return issues;
}

export function exactSubmitFileEvidence(
  expectations: readonly FormFillExpectation[],
  controls: readonly FormControl[],
): ReadonlyArray<Readonly<ExactSubmitFileEvidence>> {
  const controlsBySelector = new Map(controls.map((control) => [control.selector, control]));
  const expectedSelectors = new Set(
    expectations.filter((expectation) => expectation.expectedFiles !== undefined)
      .map((expectation) => expectation.selector),
  );
  if (controls.some((control) => control.kind === "file"
    && (control.files?.length ?? 0) > 0
    && !expectedSelectors.has(control.selector))) {
    throw new Error("Application form contains an unexpected selected document");
  }

  const evidence: ExactSubmitFileEvidence[] = [];
  for (const expectation of expectations) {
    if (expectation.expectedFiles === undefined) continue;
    const control = controlsBySelector.get(expectation.selector);
    if (!control
      || !control.name
      || control.name.length > 240
      || /[\u0000-\u001f\u007f]/u.test(control.name)) {
      throw new Error("Application document submit field is invalid");
    }
    for (const file of expectation.expectedFiles) {
      if (file.byteLength === null) {
        throw new Error("Application document evidence is incomplete");
      }
      evidence.push(Object.freeze({
        fieldName: control.name,
        name: file.name,
        byteLength: file.byteLength,
        sha256: file.sha256,
      }));
    }
  }
  const aggregateBytes = evidence.reduce((total, file) => total + file.byteLength, 0);
  if (evidence.length > FORM_FILE_READBACK_LIMITS.maxFileCount
    || aggregateBytes > FORM_FILE_READBACK_LIMITS.maxAggregateBytes) {
    throw new Error("Application document submit evidence exceeds its bounds");
  }
  return Object.freeze(evidence);
}

function expectationMatches(
  expectation: FormFillExpectation,
  control: FormControl,
): boolean {
  if (expectation.expectedChecked !== undefined) {
    return Boolean(control.checked) === expectation.expectedChecked;
  }

  if (expectation.expectedFiles !== undefined) {
    const actualFiles = control.files;
    return actualFiles !== undefined
      && actualFiles.length === expectation.expectedFiles.length
      && expectation.expectedFiles.every((expected, index) => {
        const actual = actualFiles[index];
        return expected.byteLength !== null
          && actual?.name === expected.name
          && actual.byteLength === expected.byteLength
          && actual.sha256 === expected.sha256;
      });
  }

  if (expectation.expectedValue === undefined) return false;
  return normalizeValue(expectation.kind, control.value)
    === normalizeValue(expectation.kind, expectation.expectedValue);
}

function normalizeValue(kind: FormControlKind, value: string): string {
  const trimmed = value.replace(/\r\n/g, "\n").trim();
  if (kind === "email") return trimmed.toLowerCase();
  if (kind === "tel") return trimmed.replace(/\D/g, "");
  if (kind === "url") return trimmed.replace(/\/+$/, "");
  if (kind === "text") return trimmed.replace(/\s+/g, " ");
  return trimmed;
}

function fileName(value: string): string {
  return value.split(/[\\/]/).filter(Boolean).at(-1) ?? "";
}

function validSelectedFileByteLength(value: number): boolean {
  return Number.isSafeInteger(value)
    && value > 0
    && value <= FORM_FILE_READBACK_LIMITS.maxFileBytes;
}
