import { describe, expect, it } from "vitest";
import { hasNegativeSubmissionOutcome } from "../src/submission-confirmation.js";

const NEGATIVE_SUBMISSION_OUTCOMES = [
  "You already applied for this role.",
  "You already submitted an application for this role.",
  "Application already submitted.",
  "The application was already submitted.",
  "Your application has been already submitted.",
  "The application had been already submitted.",
  "Your application has already been submitted.",
  "The application had already been submitted.",
  "We were unable to submit your application.",
  "We failed to submit the application.",
  "We could not submit your application.",
  "We couldn't submit your application.",
  "We cannot submit the application right now.",
  "We can't submit your application.",
  "Your application could not be submitted.",
  "Your application couldn't be submitted.",
  "Your application cannot be submitted.",
  "Your application can't be submitted.",
  "Your application was unable to be submitted.",
  "Your application couldn’t be submitted.",
  "Your application was not submitted.",
  "Your application wasn't submitted.",
  "Your application has not been successfully submitted.",
  "Your application hasn't been successfully submitted.",
  "Your application has not yet been submitted.",
  "Your application hasn't yet been submitted.",
  "Your application hasn’t yet been submitted.",
  "The application had not yet been submitted.",
  "The application hadn't yet been submitted.",
  "Your application is not submitted.",
  "Your application isn't successfully submitted.",
  "Your application was not yet submitted.",
  "Your application wasn't yet submitted.",
  "Your application wasn’t yet submitted.",
  "You have not submitted your application.",
  "You have not yet submitted your application.",
  "You haven't submitted your application.",
  "You haven't yet submitted your application.",
  "You haven’t yet submitted your application.",
  "The system has not yet submitted your application.",
  "We had not yet submitted an application.",
  "You did not submit the application.",
  "You didn't submit your application.",
  "You didn't yet submit your application.",
  "You didn’t yet submit your application.",
  "Status: NOT\nSUBMITTED.",
  "Status: NOT YET SUBMITTED.",
  "Your application was not received.",
  "Your application wasn't received.",
  "Your application has not been received.",
  "Your application hasn't been received.",
  "Your application is not received.",
  "Your application isn't received.",
  "We did not receive your application.",
  "We didn't receive the application.",
  "We have not received your application.",
  "We haven't received the application.",
  "Submission failed.",
  "Application submission failed.",
  "Your application submission was unsuccessful.",
  "Submission was not successful.",
  "Your application was not successfully submitted.",
] as const;

const NON_NEGATIVE_CONFIRMATION_TEXT = [
  "Thank you for applying. Your application has been received.",
  "Thanks for applying. We have received your application.",
  "Your application was successfully submitted.",
  "We could not be more excited to review your application.",
  "If you have not received a reply, contact recruiting.",
  "Your application was submitted. A cover letter was not required.",
  "Thank you for applying. You haven't submitted a cover letter because it was optional.",
  "Thank you for applying. We haven't yet reviewed your application.",
] as const;

describe("negative submission outcome detection", () => {
  it.each(NEGATIVE_SUBMISSION_OUTCOMES)("rejects employer text: %s", (body) => {
    expect(hasNegativeSubmissionOutcome(body)).toBe(true);
  });

  it.each(NON_NEGATIVE_CONFIRMATION_TEXT)("does not reject control text: %s", (body) => {
    expect(hasNegativeSubmissionOutcome(body)).toBe(false);
  });
});
