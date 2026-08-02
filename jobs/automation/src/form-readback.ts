import type {
  FormControl,
  FormControlKind,
  ValidationIssue,
} from "./contracts.js";

export interface FormFillExpectation {
  selector: string;
  kind: FormControlKind;
  field: string;
  expectedValue?: string;
  expectedChecked?: boolean;
  expectedFileName?: string;
}

export function valueExpectation(
  control: FormControl,
  field: string,
  expectedValue: string,
): FormFillExpectation {
  return {
    selector: control.selector,
    kind: control.kind,
    field,
    expectedValue,
  };
}

export function checkedExpectation(
  control: FormControl,
  field: string,
  expectedChecked: boolean,
): FormFillExpectation {
  return {
    selector: control.selector,
    kind: control.kind,
    field,
    expectedChecked,
  };
}

export function fileExpectation(
  control: FormControl,
  field: string,
  expectedPath: string,
): FormFillExpectation {
  return {
    selector: control.selector,
    kind: control.kind,
    field,
    expectedFileName: fileName(expectedPath),
  };
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

function expectationMatches(
  expectation: FormFillExpectation,
  control: FormControl,
): boolean {
  if (expectation.expectedChecked !== undefined) {
    return Boolean(control.checked) === expectation.expectedChecked;
  }

  if (expectation.expectedFileName !== undefined) {
    return fileName(control.value) === expectation.expectedFileName;
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
