import { describe, expect, it } from "vitest";
import type { FormControl } from "../src/contracts.js";
import {
  checkedExpectation,
  fileExpectation,
  valueExpectation,
  verifyFillExpectations,
} from "../src/form-readback.js";

describe("form fill read-back", () => {
  it("accepts registered text, email, phone, checkbox, and file values", () => {
    const controls = [
      control("name", "text", " Ada   Lovelace "),
      control("email", "email", "ADA@EXAMPLE.COM"),
      control("phone", "tel", "(415) 555-0199"),
      { ...control("authorized", "checkbox", ""), checked: true },
      control("resume", "file", "C:\\fakepath\\resume.pdf"),
    ];

    const issues = verifyFillExpectations([
      valueExpectation(controls[0], "Full name", "Ada Lovelace"),
      valueExpectation(controls[1], "Email", "ada@example.com"),
      valueExpectation(controls[2], "Phone", "415-555-0199"),
      checkedExpectation(controls[3], "Work authorization", true),
      fileExpectation(controls[4], "Resume", "/tmp/resume.pdf"),
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
