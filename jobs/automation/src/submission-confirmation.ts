const NEGATIVE_SUBMISSION_OUTCOME_PATTERNS = [
  /\balready (?:applied|submitted(?: (?:an|the|your))? application)\b/iu,
  /\b(?:the |your )?application (?:(?:was|has been|had been) )?already submitted\b/iu,
  /\b(?:the |your )?application (?:has|had) already been submitted\b/iu,
  /\b(?:unable|failed) to submit (?:the |your )?application\b/iu,
  /\b(?:could not|couldn['’]t|cannot|can['’]t) submit (?:the |your )?application\b/iu,
  /\b(?:could not|couldn['’]t|cannot|can['’]t|unable to) be submitted\b/iu,
  /\b(?:an |the |your )?application (?:was not|wasn['’]t)(?: yet)? (?:successfully )?(?:submitted|received)\b/iu,
  /\b(?:an |the |your )?application (?:is not|isn['’]t)(?: yet)? (?:successfully )?(?:submitted|received)\b/iu,
  /\b(?:an |the |your )?application (?:has not|hasn['’]t)(?: yet)? been (?:successfully )?(?:submitted|received)\b/iu,
  /\b(?:an |the |your )?application (?:had not|hadn['’]t)(?: yet)? been (?:successfully )?(?:submitted|received)\b/iu,
  /\b(?:have not|haven['’]t)(?: yet)? submitted (?:an |the |your )?application\b/iu,
  /\b(?:has not|hasn['’]t)(?: yet)? submitted (?:an |the |your )?application\b/iu,
  /\b(?:had not|hadn['’]t)(?: yet)? submitted (?:an |the |your )?application\b/iu,
  /\b(?:did not|didn['’]t)(?: yet)? submit (?:an |the |your )?application\b/iu,
  /\bnot (?:yet )?(?:successfully )?submitted\b/iu,
  /\b(?:(?:did not|didn['’]t) receive|(?:have not|haven['’]t) received) (?:the |your )?application\b/iu,
  /\b(?:application )?submission (?:failed|was unsuccessful|was not successful)\b/iu,
] as const;

/** Reject mixed positive/negative pages before extracting a confirmation excerpt. */
export function hasNegativeSubmissionOutcome(body: string): boolean {
  const normalized = body.replace(/\s+/gu, " ").trim();
  return NEGATIVE_SUBMISSION_OUTCOME_PATTERNS.some((pattern) => pattern.test(normalized));
}
