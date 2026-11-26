import { describe, expect, it } from "vitest";
import { previewProfileSkillMatchesText } from "./preview-application";

describe("preview profile skill matching", () => {
  it("preserves free-form custom skills with literal token boundaries", () => {
    expect(previewProfileSkillMatchesText("Maintained COBOL services.", "COBOL")).toBe(true);
    expect(previewProfileSkillMatchesText("Maintained cobol-based services.", "COBOL")).toBe(true);
    expect(previewProfileSkillMatchesText("Maintained MyCOBOL services.", "COBOL")).toBe(false);
    expect(previewProfileSkillMatchesText("Maintained COBOLish services.", "COBOL")).toBe(false);
  });

  it("keeps custom and canonical symbol-bearing skills distinct", () => {
    expect(previewProfileSkillMatchesText("Developed Q# services.", "Q#")).toBe(true);
    expect(previewProfileSkillMatchesText("Developed Q++ services.", "Q++")).toBe(true);
    expect(previewProfileSkillMatchesText("Developed Q# services.", "Q")).toBe(false);
    expect(previewProfileSkillMatchesText("Developed Q++ services.", "Q")).toBe(false);
    expect(previewProfileSkillMatchesText("Developed Q## services.", "Q#")).toBe(false);
    expect(previewProfileSkillMatchesText("Developed ++Q services.", "Q")).toBe(false);
    expect(previewProfileSkillMatchesText("Developed C# services.", "C")).toBe(false);
    expect(previewProfileSkillMatchesText("Developed C++ services.", "C#")).toBe(false);
    expect(previewProfileSkillMatchesText("Developed C++ services.", "C++")).toBe(true);
    expect(previewProfileSkillMatchesText("Developed Objective-C services.", "C")).toBe(false);
  });
});
