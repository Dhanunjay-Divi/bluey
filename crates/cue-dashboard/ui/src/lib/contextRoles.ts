export type AnswerContextRole =
  | "other"
  | "candidate_resume"
  | "job_description"
  | "interview_preparation"
  | "user_confirmed_story";

export interface ContextRoleOption {
  value: AnswerContextRole;
  label: string;
  description: string;
}

export const CONTEXT_ROLE_OPTIONS: readonly ContextRoleOption[] = [
  {
    value: "other",
    label: "General / unverified",
    description: "Useful background, but never treated as evidence that an experience happened to you.",
  },
  {
    value: "candidate_resume",
    label: "Resume",
    description: "Your resume or candidate profile; answers stay bounded to its stated facts.",
  },
  {
    value: "job_description",
    label: "Job description",
    description: "The target role, requirements, or employer context.",
  },
  {
    value: "interview_preparation",
    label: "Interview prep / unverified",
    description: "Practice material and example stories that must not be presented as your history.",
  },
  {
    value: "user_confirmed_story",
    label: "My confirmed story",
    description: "Your own lived experience, explicitly confirmed by you for grounded interview answers.",
  },
] as const;

export function contextRoleOption(role: AnswerContextRole): ContextRoleOption {
  return CONTEXT_ROLE_OPTIONS.find((option) => option.value === role) ?? CONTEXT_ROLE_OPTIONS[0];
}

export function contextRoleNeedsConfirmation(role: AnswerContextRole): boolean {
  return role === "user_confirmed_story";
}
