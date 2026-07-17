import { describe, expect, it } from "vitest";
import {
  CONTEXT_ROLE_OPTIONS,
  contextRoleNeedsConfirmation,
  contextRoleOption,
} from "./contextRoles";

describe("context answer roles", () => {
  it("labels the default role as unverified", () => {
    expect(contextRoleOption("other").label).toBe("General / unverified");
  });

  it("requires confirmation only for a personal story", () => {
    for (const option of CONTEXT_ROLE_OPTIONS) {
      expect(contextRoleNeedsConfirmation(option.value)).toBe(
        option.value === "user_confirmed_story",
      );
    }
  });
});
