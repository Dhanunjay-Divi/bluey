import { describe, expect, it } from "vitest";
import type { FormControl } from "../src/contracts.js";
import {
  checkedExpectation,
  fileExpectation,
  valueExpectation,
  verifyFillExpectations,
} from "../src/form-readback.js";

describe("form fill read-back", () => {
  const resumeSha = "a".repeat(64);
  const coverLetterSha = "b".repeat(64);
  const resumeName = `resume-${resumeSha}.pdf`;
  const coverLetterName = `cover-letter-${coverLetterSha}.pdf`;

  it("accepts registered text, email, phone, checkbox, and file values", () => {
    const controls = [
      control("name", "text", " Ada   Lovelace "),
      control("email", "email", "ADA@EXAMPLE.COM"),
      control("phone", "tel", "(415) 555-0199"),
      { ...control("authorized", "checkbox", ""), checked: true },
      {
        ...control("resume", "file", `C:\\fakepath\\${resumeName}`),
        files: [{ name: resumeName, byteLength: 1_024, sha256: resumeSha }],
      },
    ];

    const issues = verifyFillExpectations([
      valueExpectation(controls[0], "Full name", "Ada Lovelace"),
      valueExpectation(controls[1], "Email", "ada@example.com"),
      valueExpectation(controls[2], "Phone", "415-555-0199"),
      checkedExpectation(controls[3], "Work authorization", true),
      fileExpectation(controls[4], "Resume", `/tmp/${resumeName}`),
    ], controls, "Employer form");

    expect(issues).toEqual([]);
  });

  it("blocks missing or rejected values without echoing candidate data", () => {
    const email = control("email", "email", "");
    const location = control("location", "select", "remote");

    const issues = verifyFillExpectations([
      valueExpectation(email, "Email", "private@example.com"),
      valueExpectation(location, "Location", "onsite"),
    ], [email, location], "Lever");

    expect(issues).toHaveLength(2);
    expect(issues.every((issue) => issue.severity === "blocking")).toBe(true);
    expect(JSON.stringify(issues)).not.toContain("private@example.com");
    expect(JSON.stringify(issues)).not.toContain("onsite");
  });

  it("deduplicates repeated expectations for the same field control", () => {
    const name = control("name", "text", "");
    const expectation = valueExpectation(name, "Full name", "Ada Lovelace");

    const issues = verifyFillExpectations(
      [expectation, expectation],
      [name],
      "Greenhouse",
    );

    expect(issues).toHaveLength(1);
  });

  it("rejects a same-name file whose selected DOM bytes have a different hash", () => {
    const resume = {
      ...control("resume", "file", resumeName),
      files: [{ name: resumeName, byteLength: 1_024, sha256: "c".repeat(64) }],
    };

    const issues = verifyFillExpectations([
      fileExpectation(resume, "Resume", `/snapshots/${resumeName}`),
    ], [resume], "Greenhouse");

    expect(issues).toEqual([expect.objectContaining({ field: "Resume", severity: "blocking" })]);
  });

  it.each([
    ["missing evidence", undefined, [`/snapshots/${resumeName}`]],
    ["missing file", [], [`/snapshots/${resumeName}`]],
    ["extra file", [
      { name: resumeName, byteLength: 1_024, sha256: resumeSha },
      { name: coverLetterName, byteLength: 512, sha256: coverLetterSha },
    ], [`/snapshots/${resumeName}`]],
    ["reordered files", [
      { name: coverLetterName, byteLength: 512, sha256: coverLetterSha },
      { name: resumeName, byteLength: 1_024, sha256: resumeSha },
    ], [`/snapshots/${resumeName}`, `/snapshots/${coverLetterName}`]],
  ] as const)("rejects %s for an exact file expectation", (_name, files, expectedPaths) => {
    const upload = {
      ...control("documents", "file", "selected"),
      ...(files === undefined ? {} : { files: [...files] }),
    };
    const selected = files === undefined ? [] : [...files];
    const expectation = fileExpectation(
      upload,
      "Application documents",
      expectedPaths,
      selected,
    );

    expect(verifyFillExpectations(
      [expectation],
      [upload],
      "Employer form",
    )).toHaveLength(1);
  });
});

function control(
  selector: string,
  kind: FormControl["kind"],
  value: string,
): FormControl {
  return {
    selector,
    kind,
    name: selector,
    label: selector,
    placeholder: "",
    required: true,
    value,
  };
}
