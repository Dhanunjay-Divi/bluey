import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { describe, expect, it } from "vitest";
import type {
  AdapterContext,
  BrowserLocator,
  BrowserPage,
  ExactSubmitExpectation,
  FormControl,
  ProviderFinalSubmitProof,
} from "../src/contracts.js";
import {
  LEVER_CAPABILITY,
  LEVER_STATES,
  LeverApplicationStateMachine,
} from "../src/providers/lever.js";
import { certifiedProviderJobKey } from "../src/provider-job-key.js";
import { ExactSubmitEvidenceError } from "../src/trusted-submit.js";

interface LeverFixture {
  name: string;
  postingUrl: string;
  applicationUrl: string;
  title: string;
  startAtPosting: boolean;
  applySelector: string;
  submitSelector: string;
  providerLabels?: Record<string, string>;
  controls: FormControl[];
  answers: Record<string, string>;
  expectedValues: Record<string, string>;
}

const FIXTURES = [
  loadFixture("us-current.json"),
  loadFixture("eu-custom.json"),
  loadFixture("legacy-hosted.json"),
];
const RESUME_SHA = "a".repeat(64);
const COVER_LETTER_SHA = "b".repeat(64);
const RESUME_NAME = `resume-${RESUME_SHA}.pdf`;
const COVER_LETTER_NAME = `cover-letter-${COVER_LETTER_SHA}.pdf`;
const RESUME_PATH = `/snapshots/${RESUME_NAME}`;
const COVER_LETTER_PATH = `/snapshots/${COVER_LETTER_NAME}`;

describe("LeverApplicationStateMachine", () => {
  it("is explicitly beta, Review-only, and not certified", () => {
    expect(LEVER_STATES).toEqual(["detect", "prepare", "fill", "validate", "submit", "receipt"]);
    expect(LEVER_CAPABILITY).toEqual({
      provider: "lever",
      release: "beta",
      mode: "review_only",
      certified: false,
      requiresFinalReview: true,
    });

    const adapter = new LeverApplicationStateMachine();
    expect(adapter.detect(new URL("https://jobs.lever.co/acme/posting-id"))).toBe(true);
    expect(adapter.detect(new URL("https://jobs.eu.lever.co/acme/posting-id/apply"))).toBe(true);
    expect(adapter.detect(new URL("https://jobs.lever.co.evil.example/acme/posting-id"))).toBe(false);
    expect(adapter.detect(new URL("http://jobs.lever.co/acme/posting-id"))).toBe(false);
  });

  for (const fixture of FIXTURES) {
    it(`fills ${fixture.name} and stops at final review`, async () => {
      const adapter = new LeverApplicationStateMachine();
      const page = new LeverFixturePage(fixture);
      const submitHooks: string[] = [];
      const context = makeContext(page, fixture.answers, { events: submitHooks });

      await adapter.prepare(context);
      await adapter.fill(context);
      expect(await adapter.validate(context)).toEqual([]);
      const receipt = await adapter.submit(context);

      expect(receipt.status).toBe("needs_input");
      expect(receipt.intervention?.title).toBe("Review this Lever application");
      expect(page.submitClicks).toBe(0);
      expect(submitHooks).toEqual([]);
      for (const [selector, expected] of Object.entries(fixture.expectedValues)) {
        const control = page.control(selector);
        const expectedValue = control.kind !== "file"
          ? expected
          : /cover/i.test(control.label) ? COVER_LETTER_NAME : RESUME_NAME;
        expect(control.value).toBe(expectedValue);
      }
      expect(adapter.stateHistory(context).map((state) => state.state)).toEqual(LEVER_STATES);
      expect(adapter.currentState(context)).toMatchObject({ state: "receipt", outcome: "review_required" });
    });
  }

  it("rejects an Apply redirect to another Lever job with zero field or file writes", async () => {
    const fixture = loadFixture("us-current.json");
    const page = new LeverFixturePage(fixture, {
      applicationUrlAfterApply: "https://jobs.lever.co/northstar/22222222-2222-4222-8222-222222222222/apply",
    });
    const adapter = new LeverApplicationStateMachine();
    const context = makeContext(page, fixture.answers);

    await expect(adapter.prepare(context)).rejects.toThrow("does not match the approved provider job");

    expect(page.fieldWrites).toBe(0);
    expect(page.uploads).toEqual([]);
    expect(page.submitClicks).toBe(0);
  });

  it("rejects an initially wrong posting before clicking its visible Apply control", async () => {
    const approved = loadFixture("us-current.json");
    const wrongPosting = {
      ...approved,
      postingUrl: "https://jobs.lever.co/northstar/22222222-2222-4222-8222-222222222222",
      applicationUrl: "https://jobs.lever.co/northstar/22222222-2222-4222-8222-222222222222/apply",
    };
    const page = new LeverFixturePage(wrongPosting);
    const adapter = new LeverApplicationStateMachine();
    const context = makeContext(page, approved.answers, {
      events: [],
      approvedCanonicalUrl: approved.postingUrl,
    });

    await expect(adapter.prepare(context)).rejects.toThrow("does not match the approved provider job");

    expect(page.applyClicks).toBe(0);
    expect(page.fieldWrites).toBe(0);
    expect(page.uploads).toEqual([]);
    expect(page.submitClicks).toBe(0);
  });

  it("rechecks the exact Lever job immediately before the first fill", async () => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const page = new LeverFixturePage(fixture);
    const adapter = new LeverApplicationStateMachine();
    const context = makeContext(page, fixture.answers);
    await adapter.prepare(context);
    page.navigateTo("https://jobs.lever.co/northstar/22222222-2222-4222-8222-222222222222/apply");

    await expect(adapter.fill(context)).rejects.toThrow("does not match the approved provider job");

    expect(page.fieldWrites).toBe(0);
    expect(page.uploads).toEqual([]);
    expect(page.submitClicks).toBe(0);
  });

  it("uploads the packet resume and cover letter to their distinct Lever fields", async () => {
    const fixture = loadFixture("eu-custom.json");
    const page = new LeverFixturePage(fixture);
    const adapter = new LeverApplicationStateMachine();
    const context = makeContext(page, fixture.answers);

    await adapter.prepare(context);
    await adapter.fill(context);

    expect(page.control("[data-bluey-field-id='resume']").value).toBe(RESUME_NAME);
    expect(page.control("[data-bluey-field-id='cover']").value).toBe(COVER_LETTER_NAME);
    expect(page.uploads).toEqual([
      ["[data-bluey-field-id='resume']", RESUME_PATH],
      ["[data-bluey-field-id='cover']", COVER_LETTER_PATH],
    ]);
  });

  it("distinguishes a missing known fact from an unknown required question", async () => {
    const missingFixture = asDirectApplication(loadFixture("us-current.json"));
    missingFixture.answers = {};
    const missingPage = new LeverFixturePage(missingFixture);
    const missingAdapter = new LeverApplicationStateMachine();
    const missingContext = makeContext(missingPage, missingFixture.answers);

    await missingAdapter.prepare(missingContext);
    await missingAdapter.fill(missingContext);
    const missingIssues = await missingAdapter.validate(missingContext);
    expect(missingIssues).toEqual(expect.arrayContaining([
      expect.objectContaining({ field: "Full name", message: expect.stringContaining("confirmed fact") }),
    ]));
    const missingReceipt = await missingAdapter.submit(missingContext);
    expect(missingReceipt.intervention?.kind).toBe("missing_fact");
    expect(missingPage.submitClicks).toBe(0);

    const unknownFixture = asDirectApplication(loadFixture("us-current.json"));
    unknownFixture.controls.push({
      selector: "[data-bluey-field-id='custom-question']",
      kind: "textarea",
      label: "Are you bound by a non-compete?",
      name: "cards[custom][field0]",
      placeholder: "",
      required: true,
      value: "",
    });
    const unknownPage = new LeverFixturePage(unknownFixture);
    const unknownAdapter = new LeverApplicationStateMachine();
    const unknownContext = makeContext(unknownPage, unknownFixture.answers);

    await unknownAdapter.prepare(unknownContext);
    await unknownAdapter.fill(unknownContext);
    const unknownIssues = await unknownAdapter.validate(unknownContext);
    expect(unknownIssues).toEqual(expect.arrayContaining([
      expect.objectContaining({
        field: "Are you bound by a non-compete?",
        message: expect.stringContaining("unknown question"),
      }),
    ]));
    const unknownReceipt = await unknownAdapter.submit(unknownContext);
    expect(unknownReceipt.intervention?.kind).toBe("unknown_question");
    expect(unknownPage.submitClicks).toBe(0);
  });

  it("pauses authorization questions even when a packet answer exists", async () => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    fixture.controls.push({
      selector: "[data-bluey-field-id='authorized-yes']",
      kind: "radio",
      label: "Legally authorized to work Yes",
      name: "work-authorization",
      placeholder: "",
      required: true,
      value: "yes",
      checked: false,
    });
    fixture.controls.push({
      selector: "[data-bluey-field-id='authorized-no']",
      kind: "radio",
      label: "Legally authorized to work No",
      name: "work-authorization",
      placeholder: "",
      required: true,
      value: "no",
      checked: false,
    });
    fixture.answers.work_authorization = "yes";
    const page = new LeverFixturePage(fixture);
    const adapter = new LeverApplicationStateMachine();
    const context = makeContext(page, fixture.answers);

    await adapter.prepare(context);
    await adapter.fill(context);
    const issues = await adapter.validate(context);

    expect(issues).toEqual(expect.arrayContaining([
      expect.objectContaining({ message: expect.stringContaining("authorization") }),
    ]));
    expect(page.control("[data-bluey-field-id='authorized-yes']").checked).toBe(false);
  });

  it("pauses on a visible challenge without filling or submitting", async () => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const page = new LeverFixturePage(fixture, { body: "Verify you are human to continue." });
    const adapter = new LeverApplicationStateMachine();
    const submitHooks: string[] = [];
    const proofs: ProviderFinalSubmitProof[] = [];
    const context = makeContext(page, fixture.answers, {
      events: submitHooks,
      proofs,
    });

    await adapter.prepare(context);
    await adapter.fill(context);
    expect(await adapter.validate(context)).toEqual([]);
    const receipt = await adapter.submit(context, { finalReviewApproved: true });

    expect(receipt.status).toBe("needs_input");
    expect(receipt.intervention?.kind).toBe("captcha");
    expect(receipt.confirmationText).toBeUndefined();
    expect(page.control("[data-bluey-field-id='name']").value).toBe("");
    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual([]);
  });

  it("blocks validation when Lever cannot accept an exact select option", async () => {
    const fixture = asDirectApplication(loadFixture("eu-custom.json"));
    fixture.answers.preferred_location = "Paris";
    const page = new LeverFixturePage(fixture);
    const adapter = new LeverApplicationStateMachine();
    const submitHooks: string[] = [];
    const context = makeContext(page, fixture.answers, { events: submitHooks });

    await adapter.prepare(context);
    await adapter.fill(context);
    const issues = await adapter.validate(context);

    expect(issues).toEqual(expect.arrayContaining([
      expect.objectContaining({
        field: "Preferred location",
        message: expect.stringContaining("did not accept"),
        severity: "blocking",
      }),
    ]));
    expect(page.control("[data-bluey-field-id='location']").value).toBe("");
    expect(submitHooks).toEqual([]);
  });

  it("blocks submission when Lever discards a prepared field value", async () => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const page = new LeverFixturePage(
      fixture,
      { ignoredWrites: ["[data-bluey-field-id='name']"] },
    );
    const adapter = new LeverApplicationStateMachine();
    const submitHooks: string[] = [];
    const context = makeContext(page, fixture.answers, { events: submitHooks });

    await adapter.prepare(context);
    await adapter.fill(context);
    const issues = await adapter.validate(context);

    expect(issues).toEqual(expect.arrayContaining([
      expect.objectContaining({
        field: "Full name",
        message: expect.stringContaining("did not register"),
        severity: "blocking",
      }),
    ]));
    const receipt = await adapter.submit(context, { finalReviewApproved: true });
    expect(receipt.status).toBe("needs_input");
    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual([]);
  });

  it.each([
    ["text", (page: LeverFixturePage) => {
      page.control("[data-bluey-field-id='name']").value = "Grace Hopper";
    }],
    ["textarea", (page: LeverFixturePage) => {
      page.control("[data-bluey-field-id='why']").value = "Changed after review";
    }],
    ["select", (page: LeverFixturePage) => {
      page.control("[data-bluey-field-id='location']").value = "toronto";
    }],
    ["checkbox", (page: LeverFixturePage) => {
      page.control("[data-bluey-field-id='accurate']").checked = false;
    }],
    ["radio", (page: LeverFixturePage) => {
      page.control("[data-bluey-field-id='remote']").checked = false;
      page.control("[data-bluey-field-id='office']").checked = true;
    }],
    ["file", (page: LeverFixturePage) => {
      page.replaceSelectedFileBytes("[data-bluey-field-id='resume']");
    }],
  ])("fails closed when a %s expectation changes after final authority", async (_kind, mutate) => {
    const fixture = leverAllControlsFixture();
    const page = new LeverFixturePage(fixture);
    const adapter = new LeverApplicationStateMachine();
    const submitHooks: string[] = [];
    const context = makeContext(page, fixture.answers, {
      events: submitHooks,
      before: async () => { mutate(page); },
    });
    await prepareValidApplication(adapter, context);

    await expect(adapter.submit(context, { finalReviewApproved: true }))
      .rejects.toThrow("changed after final submit authority");

    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual(["before"]);
  });

  it("rejects same-name different resume bytes after final authority", async () => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const page = new LeverFixturePage(fixture);
    const adapter = new LeverApplicationStateMachine();
    const submitHooks: string[] = [];
    const context = makeContext(page, fixture.answers, {
      events: submitHooks,
      before: async () => { page.replaceSelectedFileBytes("[data-bluey-field-id='resume']"); },
    });
    await prepareValidApplication(adapter, context);

    await expect(adapter.submit(context, { finalReviewApproved: true }))
      .rejects.toThrow("changed after final submit authority");

    expect(page.control("[data-bluey-field-id='resume']").value).toBe(RESUME_NAME);
    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual(["before"]);
  });

  it("rejects an outer form action change with unchanged page and submit button", async () => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const page = new LeverFixturePage(fixture);
    const adapter = new LeverApplicationStateMachine();
    const submitHooks: string[] = [];
    const context = makeContext(page, fixture.answers, {
      events: submitHooks,
      before: async () => { page.mutateOuterFormAction(`${fixture.applicationUrl}&target-drift=1`); },
    });
    await prepareValidApplication(adapter, context);

    await expect(adapter.submit(context, { finalReviewApproved: true }))
      .rejects.toThrow("submit target changed");

    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual(["before"]);
  });

  it.each([
    ["outer form enctype", (page: LeverFixturePage) => {
      page.mutateOuterFormEnctype("text/plain");
    }],
    ["submitter formenctype", (page: LeverFixturePage) => {
      page.mutateSubmitterFormEnctype("application/x-www-form-urlencoded");
    }],
    ["outer form target", (page: LeverFixturePage) => {
      page.mutateOuterFormTarget("_blank");
    }],
    ["submitter formtarget", (page: LeverFixturePage) => {
      page.mutateSubmitterFormTarget("_blank");
    }],
  ] as const)("rejects %s drift after final authority", async (_name, mutate) => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const page = new LeverFixturePage(fixture);
    const adapter = new LeverApplicationStateMachine();
    const submitHooks: string[] = [];
    const context = makeContext(page, fixture.answers, {
      events: submitHooks,
      before: async () => { mutate(page); },
    });
    await prepareValidApplication(adapter, context);

    await expect(adapter.submit(context, { finalReviewApproved: true }))
      .rejects.toThrow("submit target changed");

    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual(["before"]);
  });

  it.each([
    ["selected file bytes", (page: LeverFixturePage) => {
      page.replaceSelectedFileBytes("[data-bluey-field-id='resume']");
    }],
    ["outer form action", (page: LeverFixturePage) => {
      page.mutateOuterFormAction(`${page.url()}&click-handler-drift=1`);
    }],
    ["outer form enctype", (page: LeverFixturePage) => {
      page.mutateOuterFormEnctype("text/plain");
    }],
    ["submitter formenctype", (page: LeverFixturePage) => {
      page.mutateSubmitterFormEnctype("application/x-www-form-urlencoded");
    }],
    ["_blank/window.open form target", (page: LeverFixturePage) => {
      page.mutateSubmitterFormTarget("_blank");
    }],
  ] as const)("aborts a click-handler mutation of %s before outgoing submit", async (_name, mutate) => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const page = new LeverFixturePage(fixture, { duringSubmitActivation: mutate });
    const adapter = new LeverApplicationStateMachine();
    const submitHooks: string[] = [];
    const context = makeContext(page, fixture.answers, { events: submitHooks });
    await prepareValidApplication(adapter, context);

    await expect(adapter.submit(context, { finalReviewApproved: true }))
      .rejects.toThrow("submit evidence changed during activation");

    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual(["before"]);
  });

  it.each([
    ["cross-origin action", (page: LeverFixturePage) => {
      page.mutateOuterFormAction("https://example.com/northstar/job/apply");
    }],
    ["unsupported method", (page: LeverFixturePage) => {
      page.mutateOuterFormMethod("get");
    }],
    ["unsupported outer enctype", (page: LeverFixturePage) => {
      page.mutateOuterFormEnctype("text/plain");
    }],
    ["unsupported submitter formenctype", (page: LeverFixturePage) => {
      page.mutateSubmitterFormEnctype("application/x-www-form-urlencoded");
    }],
    ["unsupported outer form target", (page: LeverFixturePage) => {
      page.mutateOuterFormTarget("_blank");
    }],
    ["unsupported submitter formtarget", (page: LeverFixturePage) => {
      page.mutateSubmitterFormTarget("_blank");
    }],
    ["another provider job", (page: LeverFixturePage) => {
      page.mutateOuterFormAction("https://jobs.lever.co/northstar/22222222-2222-4222-8222-222222222222/apply");
    }],
  ] as const)("rejects an invalid effective submit target: %s", async (_name, mutate) => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const page = new LeverFixturePage(fixture);
    const adapter = new LeverApplicationStateMachine();
    const submitHooks: string[] = [];
    const context = makeContext(page, fixture.answers, { events: submitHooks });
    await prepareValidApplication(adapter, context);
    mutate(page);

    await expect(adapter.submit(context, { finalReviewApproved: true }))
      .rejects.toThrow("submit target is invalid");

    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual([]);
  });

  it.each([
    ["URL", (page: LeverFixturePage) => {
      page.navigateTo(`${page.url()}?changed-after-authority=1`);
    }],
    ["Submit control", (page: LeverFixturePage) => {
      page.replaceSubmitControl();
    }],
  ])("fails closed when the %s changes during final authority", async (_kind, mutate) => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const page = new LeverFixturePage(fixture);
    const adapter = new LeverApplicationStateMachine();
    const submitHooks: string[] = [];
    const context = makeContext(page, fixture.answers, {
      events: submitHooks,
      before: async () => { mutate(page); },
    });
    await prepareValidApplication(adapter, context);

    await expect(adapter.submit(context, { finalReviewApproved: true }))
      .rejects.toThrow("changed after final submit authority");

    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual(["before"]);
  });

  it("blocks ambiguous Lever form recognition before any field action", async () => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const page = new LeverFixturePage(fixture, { formCount: 2 });
    const adapter = new LeverApplicationStateMachine();
    const context = makeContext(page, fixture.answers);

    await adapter.prepare(context);
    await adapter.fill(context);
    const issues = await adapter.validate(context);

    expect(issues).toEqual(expect.arrayContaining([
      expect.objectContaining({ message: expect.stringContaining("more than one application form") }),
    ]));
    expect(page.control("[data-bluey-field-id='name']").value).toBe("");
  });

  it("treats a success-looking URL without confirmation text as uncertain and never retries", async () => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const page = new LeverFixturePage(fixture, {
      afterSubmitBody: "Your response is being processed.",
      afterSubmitUrl: `${fixture.applicationUrl}/success`,
    });
    const adapter = new LeverApplicationStateMachine();
    const context = makeContext(page, fixture.answers);

    await prepareValidApplication(adapter, context);
    const first = await adapter.submit(context, { finalReviewApproved: true });
    await adapter.prepare(context);
    await adapter.fill(context);
    await adapter.validate(context);
    const second = await adapter.submit(context, { finalReviewApproved: true });

    expect(first.status).toBe("needs_input");
    expect(first.intervention?.kind).toBe("browser_takeover");
    expect(first.issues[0]?.message).toContain("side effect is uncertain");
    expect(first.confirmationText).toBeUndefined();
    expect(first.confirmationUrl).toBeUndefined();
    expect(second.status).toBe("needs_input");
    expect(page.submitClicks).toBe(1);
  });

  it("treats a lost submit response as uncertain", async () => {
    const fixture = asDirectApplication(loadFixture("legacy-hosted.json"));
    const page = new LeverFixturePage(fixture, { throwAfterSubmit: true });
    const adapter = new LeverApplicationStateMachine();
    const submitHooks: string[] = [];
    const context = makeContext(page, fixture.answers, { events: submitHooks });

    await prepareValidApplication(adapter, context);
    const receipt = await adapter.submit(context, { finalReviewApproved: true });

    expect(receipt.status).toBe("needs_input");
    expect(receipt.confirmationText).toBeUndefined();
    expect(receipt.confirmationUrl).toBeUndefined();
    expect(page.submitClicks).toBe(1);
    expect(submitHooks).toEqual(["before", "after:activation_uncertain"]);
  });

  it("awaits a durable fence failure and never activates Submit", async () => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const page = new LeverFixturePage(fixture);
    const adapter = new LeverApplicationStateMachine();
    const submitHooks: string[] = [];
    const context = makeContext(page, fixture.answers, {
      events: submitHooks,
      before: async () => { throw new Error("durable fence response unavailable"); },
    });
    await prepareValidApplication(adapter, context);

    await expect(adapter.submit(context, { finalReviewApproved: true }))
      .rejects.toThrow("durable fence response unavailable");

    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual(["before"]);
  });

  it("does not treat a duplicate-application notice as confirmation", async () => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const page = new LeverFixturePage(fixture, {
      afterSubmitBody: "You already submitted an application. We have received your application before.",
    });
    const adapter = new LeverApplicationStateMachine();
    const context = makeContext(page, fixture.answers);

    await prepareValidApplication(adapter, context);
    const receipt = await adapter.submit(context, { finalReviewApproved: true });

    expect(receipt.status).toBe("needs_input");
    expect(receipt.confirmationText).toBeUndefined();
    expect(receipt.confirmationUrl).toBeUndefined();
  });

  it("does not claim a confirmation page that predates this run", async () => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const confirmation = "Thank you for applying. Your application has been received.";
    const page = new LeverFixturePage(fixture, { body: confirmation });
    const adapter = new LeverApplicationStateMachine();
    const context = makeContext(page, fixture.answers);

    await adapter.prepare(context);
    const receipt = await adapter.submit(context);

    expect(receipt.status).toBe("needs_input");
    expect(receipt.confirmationText).toBe(confirmation);
    expect(receipt.intervention?.title).toBe("Reconcile the Lever confirmation");
  });

  it("records only explicit confirmation evidence returned by Lever", async () => {
    const fixture = asDirectApplication(loadFixture("eu-custom.json"));
    const confirmation = "Thank you for your application. Reference candidate-123.";
    const page = new LeverFixturePage(fixture, {
      afterSubmitBody: confirmation,
      afterSubmitUrl: `${fixture.applicationUrl.replace(/\/apply$/u, "")}/confirmation`,
    });
    const adapter = new LeverApplicationStateMachine();
    const submitHooks: string[] = [];
    const proofs: ProviderFinalSubmitProof[] = [];
    const context = makeContext(page, fixture.answers, {
      events: submitHooks,
      proofs,
    });

    await prepareValidApplication(adapter, context);
    const receipt = await adapter.submit(context, { finalReviewApproved: true });

    expect(receipt.status).toBe("submitted");
    expect(receipt.submitHttpStatus).toBe(200);
    expect(receipt.confirmationText).toBe(confirmation);
    expect(receipt.confirmationUrl).toBe(`${fixture.applicationUrl.replace(/\/apply$/u, "")}/confirmation`);
    expect(receipt.submittedAt).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    expect(page.submitClicks).toBe(1);
    expect(submitHooks).toEqual(["before", "after:activated"]);
    expect(proofs).toEqual([expect.objectContaining({
      adapter: "lever",
      adapterVersion: "2026.07.0-beta.1",
      control: "lever_application_submit",
      target: expect.objectContaining({
        actionUrl: fixture.applicationUrl,
        method: "post",
        enctype: "multipart/form-data",
        formTarget: "_self",
      }),
    })]);
  });

  it("rejects confirmation text when Lever also returns validation state", async () => {
    const fixture = asDirectApplication(loadFixture("eu-custom.json"));
    const page = new LeverFixturePage(fixture, {
      afterSubmitBody: "Thank you for applying. Please complete all required fields.",
      afterSubmitUrl: `${fixture.applicationUrl.replace(/\/apply$/u, "")}/confirmation`,
    });
    const adapter = new LeverApplicationStateMachine();
    const context = makeContext(page, fixture.answers);
    await prepareValidApplication(adapter, context);

    const receipt = await adapter.submit(context, { finalReviewApproved: true });

    expect(receipt.status).toBe("needs_input");
    expect(receipt.issues[0]?.message).toContain("validation error");
    expect(receipt).not.toHaveProperty("submittedAt");
  });

  it("rejects confirmation text when Lever rerenders ambiguous submit controls", async () => {
    const fixture = asDirectApplication(loadFixture("eu-custom.json"));
    const page = new LeverFixturePage(fixture, {
      afterSubmitBody: "Thank you for your application. Reference candidate-123.",
      afterSubmitCount: 2,
      afterSubmitUrl: `${fixture.applicationUrl.replace(/\/apply$/u, "")}/confirmation`,
    });
    const adapter = new LeverApplicationStateMachine();
    const context = makeContext(page, fixture.answers);
    await prepareValidApplication(adapter, context);

    const receipt = await adapter.submit(context, { finalReviewApproved: true });

    expect(await page.bodyText()).toContain("Thank you for your application");
    expect(receipt.status).toBe("needs_input");
    expect(receipt.issues[0]?.message).toContain("returned the application form");
    expect(receipt).not.toHaveProperty("submittedAt");
    expect(page.submitClicks).toBe(1);
  });

  it.each([
    "Thank you for applying. Unfortunately, your application was not submitted.",
    "Thank you for applying. Your application could not be submitted.",
    "Thank you for applying. You have not submitted your application.",
    "Thank you for applying. You haven't yet submitted your application.",
    "Thank you for applying. Your application hasn’t yet been submitted.",
  ])("rejects mixed positive and negative Lever confirmation text: %s", async (afterSubmitBody) => {
    const fixture = asDirectApplication(loadFixture("eu-custom.json"));
    const page = new LeverFixturePage(fixture, {
      afterSubmitBody,
      afterSubmitUrl: `${fixture.applicationUrl.replace(/\/apply$/u, "")}/confirmation`,
    });
    const adapter = new LeverApplicationStateMachine();
    const context = makeContext(page, fixture.answers);
    await prepareValidApplication(adapter, context);

    const receipt = await adapter.submit(context, { finalReviewApproved: true });

    expect(receipt.status).toBe("needs_input");
    expect(receipt.confirmationText).toBeUndefined();
    expect(receipt).not.toHaveProperty("submittedAt");
    expect(page.submitClicks).toBe(1);
  });

  it("does not claim same-host confirmation text redirected to another job", async () => {
    const fixture = asDirectApplication(loadFixture("us-current.json"));
    const page = new LeverFixturePage(fixture, {
      afterSubmitBody: "Thank you for applying. Your application has been received.",
      afterSubmitUrl: "https://jobs.lever.co/northstar/22222222-2222-4222-8222-222222222222/confirmation",
    });
    const adapter = new LeverApplicationStateMachine();
    const context = makeContext(page, fixture.answers);
    await prepareValidApplication(adapter, context);

    const receipt = await adapter.submit(context, { finalReviewApproved: true });

    expect(receipt.status).toBe("needs_input");
    expect(receipt).not.toHaveProperty("submittedAt");
    expect(page.submitClicks).toBe(1);
  });
});

async function prepareValidApplication(
  adapter: LeverApplicationStateMachine,
  context: AdapterContext,
): Promise<void> {
  await adapter.prepare(context);
  await adapter.fill(context);
  expect(await adapter.validate(context)).toEqual([]);
}

function makeContext(
  page: BrowserPage,
  answers: Record<string, string>,
  hooks?: {
    events: string[];
    proofs?: ProviderFinalSubmitProof[];
    before?: () => Promise<void>;
    approvedCanonicalUrl?: string;
  },
): AdapterContext {
  return {
    runner: "local",
    runId: "run-lever-123",
    accountId: "account-123",
    approvedCanonicalUrl: hooks?.approvedCanonicalUrl ?? page.url(),
    page,
    packet: {
      applicationId: "application-123",
      jobId: "job-123",
      resumeVersionId: "resume-version-123",
      approvedPacketChecksum: "c".repeat(64),
      resumePath: RESUME_PATH,
      coverLetterPath: COVER_LETTER_PATH,
      answers: { ...answers },
      verifiedClaimIds: ["claim-1"],
      applicationIdentityId: "identity-123",
      applicationEmail: "ada@example.com",
      browserProfileId: "browser-profile-123",
    },
    async log() {},
    async beforeFinalSubmit(proof) {
      hooks?.events.push("before");
      hooks?.proofs?.push(proof);
      await hooks?.before?.();
    },
    async afterFinalSubmit(outcome: string) {
      hooks?.events.push(`after:${outcome}`);
    },
  };
}

function loadFixture(name: string): LeverFixture {
  return JSON.parse(readFileSync(new URL(`./fixtures/lever/${name}`, import.meta.url), "utf8")) as LeverFixture;
}

function asDirectApplication(fixture: LeverFixture): LeverFixture {
  return { ...fixture, startAtPosting: false, controls: fixture.controls.map((control) => ({ ...control })) };
}

function leverAllControlsFixture(): LeverFixture {
  const fixture = asDirectApplication(loadFixture("eu-custom.json"));
  fixture.controls.push(
    {
      selector: "[data-bluey-field-id='accurate']",
      kind: "checkbox",
      label: "Confirm accuracy",
      name: "confirm_accuracy",
      placeholder: "",
      required: true,
      value: "",
      checked: false,
    },
    {
      selector: "[data-bluey-field-id='remote']",
      kind: "radio",
      label: "Work mode Remote",
      name: "work_mode",
      placeholder: "",
      required: true,
      value: "remote",
      checked: false,
    },
    {
      selector: "[data-bluey-field-id='office']",
      kind: "radio",
      label: "Work mode Office",
      name: "work_mode",
      placeholder: "",
      required: true,
      value: "office",
      checked: false,
    },
  );
  fixture.answers = {
    ...fixture.answers,
    confirm_accuracy: "yes",
    work_mode: "remote",
  };
  return fixture;
}

interface FixturePageOptions {
  body?: string;
  afterSubmitBody?: string;
  afterSubmitCount?: number;
  afterSubmitUrl?: string;
  formCount?: number;
  submitCount?: number;
  throwAfterSubmit?: boolean;
  ignoredWrites?: string[];
  applicationUrlAfterApply?: string;
  duringSubmitActivation?: (page: LeverFixturePage) => void;
}

class LeverFixturePage implements BrowserPage {
  readonly uploads: Array<[string, string]> = [];
  submitClicks = 0;
  applyClicks = 0;

  private currentUrl: string;
  private body: string;
  private onApplication: boolean;
  private readonly controlsState: FormControl[];
  private submitSelector: string;
  private formAction: string;
  private formMethod = "post";
  private formEnctype = "multipart/form-data";
  private submitterEnctype?: string;
  private formTarget = "_self";
  private submitterTarget?: string;
  fieldWrites = 0;

  constructor(
    private readonly fixture: LeverFixture,
    private readonly options: FixturePageOptions = {},
  ) {
    this.onApplication = !fixture.startAtPosting;
    this.currentUrl = this.onApplication ? fixture.applicationUrl : fixture.postingUrl;
    this.body = options.body ?? (this.onApplication ? "Submit your application" : "Lever hosted job posting");
    this.submitSelector = fixture.submitSelector;
    this.formAction = fixture.applicationUrl;
    this.controlsState = fixture.controls.map((control) => ({
      ...control,
      options: control.options?.map((option) => ({ ...option })),
      files: control.files?.map((file) => ({ ...file })) ?? [],
    }));
  }

  url(): string {
    return this.currentUrl;
  }

  async installExactSubmitGuard(): Promise<void> {}

  async beginExactSubmitGuard(): Promise<void> {}

  async assertExactSubmitGuardClean(): Promise<void> {}

  title(): Promise<string> {
    return Promise.resolve(this.fixture.title);
  }

  controls(): Promise<FormControl[]> {
    if (!this.onApplication) return Promise.resolve([]);
    return Promise.resolve(this.controlsState.map((control) => ({
      ...control,
      options: control.options?.map((option) => ({ ...option })),
      files: control.files?.map((file) => ({ ...file })),
    })));
  }

  bodyText(): Promise<string> {
    return Promise.resolve(this.body);
  }

  waitForSettled(): Promise<void> {
    return Promise.resolve();
  }

  screenshot(): Promise<Uint8Array> {
    return Promise.resolve(new Uint8Array());
  }

  control(selector: string): FormControl {
    const control = this.controlsState.find((candidate) => candidate.selector === selector);
    if (!control) throw new Error(`Missing fixture control ${selector}`);
    return control;
  }

  navigateTo(url: string): void {
    this.currentUrl = url;
  }

  replaceSubmitControl(): void {
    this.submitSelector = this.submitSelector === "#application-form #btn-submit[data-qa='btn-submit']"
      ? "#application-form button#btn-submit.template-btn-submit"
      : "#application-form #btn-submit[data-qa='btn-submit']";
  }

  replaceSelectedFileBytes(selector: string): void {
    const control = this.control(selector);
    const selected = control.files?.[0];
    if (!selected) throw new Error(`Fixture file is not selected: ${selector}`);
    control.files = [{ ...selected, sha256: "f".repeat(64) }];
  }

  mutateOuterFormAction(actionUrl: string): void {
    this.formAction = actionUrl;
  }

  mutateOuterFormMethod(method: string): void {
    this.formMethod = method;
  }

  mutateOuterFormEnctype(enctype: string): void {
    this.formEnctype = enctype;
  }

  mutateSubmitterFormEnctype(enctype: string): void {
    this.submitterEnctype = enctype;
  }

  mutateOuterFormTarget(target: string): void {
    this.formTarget = target;
  }

  mutateSubmitterFormTarget(target: string): void {
    this.submitterTarget = target;
  }

  locator(selector: string): BrowserLocator {
    const control = this.controlsState.find((candidate) => candidate.selector === selector);
    const providerLabelSelector = Object.entries(this.fixture.providerLabels ?? {}).find(
      ([controlSelector]) => selector === `.application-question:has(${controlSelector}) .application-label`,
    );
    const isSubmit = (): boolean => selector === this.submitSelector;
    const count = (): number => {
      if (control) return this.onApplication ? 1 : 0;
      if (providerLabelSelector) return this.onApplication ? 1 : 0;
      if (selector === "#application-form") return this.onApplication ? this.options.formCount ?? 1 : 0;
      if (selector === this.fixture.applySelector) return this.onApplication ? 0 : 1;
      if (isSubmit() && this.submitClicks > 0 && this.options.afterSubmitCount !== undefined) {
        return this.options.afterSubmitCount;
      }
      if (isSubmit()) return this.onApplication ? this.options.submitCount ?? 1 : 0;
      return 0;
    };

    return {
      count: async () => count(),
      fill: async (value) => {
        if (control && !this.options.ignoredWrites?.includes(selector)) {
          control.value = value;
          this.fieldWrites += 1;
        }
      },
      click: async () => {
        if (selector === this.fixture.applySelector && !this.onApplication) {
          this.applyClicks += 1;
          this.onApplication = true;
          this.currentUrl = this.options.applicationUrlAfterApply ?? this.fixture.applicationUrl;
          this.body = this.options.body ?? "Submit your application";
          return;
        }
        if (!isSubmit() || !this.onApplication) return;
        this.submitClicks += 1;
        if (this.options.afterSubmitBody !== undefined) this.body = this.options.afterSubmitBody;
        if (this.options.afterSubmitUrl !== undefined) this.currentUrl = this.options.afterSubmitUrl;
        if (this.options.throwAfterSubmit) throw new Error("Synthetic response loss after click");
      },
      textContent: async () => isSubmit() ? "Submit Application" : providerLabelSelector?.[1] ?? "",
      getAttribute: async (name) => leverSubmitAttribute(this.submitSelector, selector, name),
      isVisible: async () => count() > 0,
      selectOption: async (value) => {
        if (control && !this.options.ignoredWrites?.includes(selector)) {
          control.value = value;
          this.fieldWrites += 1;
        }
      },
      setChecked: async (checked) => {
        if (control && !this.options.ignoredWrites?.includes(selector)) {
          control.checked = checked;
          this.fieldWrites += 1;
        }
      },
      setInputFiles: async (paths) => {
        if (!control || this.options.ignoredWrites?.includes(selector)) return [];
        const files = paths.map(fileEvidenceForSnapshotPath);
        control.value = files.map((file) => file.name).join(", ");
        control.files = files;
        for (const path of paths) this.uploads.push([selector, path]);
        return files;
      },
      effectiveSubmitTarget: async (adapter) => {
        if (!isSubmit()) throw new Error("Fixture locator is not the submit control");
        return {
          actionUrl: this.formAction,
          method: this.formMethod,
          enctype: this.submitterEnctype ?? this.formEnctype,
          formTarget: this.submitterTarget ?? this.formTarget,
          providerJobKey: certifiedProviderJobKey(adapter, this.formAction, "submit"),
          formIdentity: "[0,\"application-form\",\"\",\"\",\"\",\"\",\"\"]",
        };
      },
      successfulSubmitEvidence: async (trustedFields, providerJobKey) => (
        fixtureSubmitEvidence(this.controlsState, trustedFields, providerJobKey)
      ),
      clickWithExactSubmit: async (expectation) => {
        if (!isSubmit()) throw new Error("Fixture locator is not the submit control");
        this.options.duringSubmitActivation?.(this);
        this.assertExactSubmit(expectation);
        this.submitClicks += 1;
        if (this.options.afterSubmitBody !== undefined) this.body = this.options.afterSubmitBody;
        if (this.options.afterSubmitUrl !== undefined) {
          this.currentUrl = this.options.afterSubmitUrl;
          if (/\/confirmation(?:[?#]|$)/u.test(this.currentUrl)) this.onApplication = false;
        }
        if (this.options.throwAfterSubmit) throw new Error("Synthetic response loss after click");
        return 200;
      },
    };
  }

  private assertExactSubmit(expectation: ExactSubmitExpectation): void {
    let target;
    try {
      target = {
        actionUrl: this.formAction,
        method: this.formMethod,
        enctype: this.submitterEnctype ?? this.formEnctype,
        formTarget: this.submitterTarget ?? this.formTarget,
        providerJobKey: certifiedProviderJobKey("lever", this.formAction, "submit"),
        formIdentity: "[0,\"application-form\",\"\",\"\",\"\",\"\",\"\"]",
      };
    } catch {
      throw new ExactSubmitEvidenceError();
    }
    const files = this.controlsState.flatMap((control) => (
      control.kind === "file"
        ? (control.files ?? []).map((file) => ({ ...file, fieldName: control.name }))
        : []
    ));
    const evidence = fixtureSubmitEvidence(this.controlsState);
    const fields = evidence.fields;
    if (target.actionUrl !== expectation.target.actionUrl
      || target.method !== expectation.target.method
      || target.enctype !== expectation.target.enctype
      || target.formTarget !== expectation.target.formTarget
      || target.providerJobKey !== expectation.target.providerJobKey
      || target.formIdentity !== expectation.target.formIdentity
      || files.length !== expectation.files.length
      || files.some((file, index) => {
        const expected = expectation.files[index];
        return file.fieldName !== expected?.fieldName
          || file.name !== expected.name
          || file.byteLength !== expected.byteLength
          || file.sha256 !== expected.sha256;
      })
      || fields.length !== expectation.fields.length
      || fields.some((field, index) => {
        const expected = expectation.fields[index];
        return field.fieldName !== expected?.fieldName
          || field.valueByteLength !== expected.valueByteLength
          || field.valueSha256 !== expected.valueSha256;
      })
      || evidence.partOrder.length !== expectation.partOrder.length
      || evidence.partOrder.some((entry, index) => (
        entry.kind !== expectation.partOrder[index]?.kind
          || entry.index !== expectation.partOrder[index]?.index
      ))) {
      throw new ExactSubmitEvidenceError();
    }
  }
}

function fixtureFieldEvidence(
  controls: readonly FormControl[],
  trustedFields: ReadonlyArray<Readonly<{ fieldName: string; value: string }>> = [],
  providerJobKey?: string,
) {
  const trustedByName = new Map<string, string[]>();
  for (const field of trustedFields) {
    const values = trustedByName.get(field.fieldName) ?? [];
    values.push(field.value);
    trustedByName.set(field.fieldName, values);
  }
  const offsets = new Map<string, number>();
  const providerJobId = providerJobKey?.split(":").at(-1);
  return controls.flatMap((control) => {
    if (!control.name
      || control.kind === "file"
      || ((control.kind === "checkbox" || control.kind === "radio") && !control.checked)) {
      return [];
    }
    const offset = offsets.get(control.name) ?? 0;
    const trusted = trustedByName.get(control.name)?.[offset];
    if (trusted !== undefined) offsets.set(control.name, offset + 1);
    let value = trusted ?? control.value;
    if ((control.kind === "checkbox" || control.kind === "radio") && !value) value = "on";
    if (providerJobId && /^(?:job_id|jobid|gh_jid|posting_id|postingid)$/iu.test(control.name)) {
      value = providerJobId;
    }
    const bytes = Buffer.from(value.replace(/\r\n|\r|\n/gu, "\r\n"), "utf8");
    return [{
      fieldName: control.name,
      valueByteLength: bytes.byteLength,
      valueSha256: createHash("sha256").update(bytes).digest("hex"),
    }];
  });
}

function fixtureSubmitEvidence(
  controls: readonly FormControl[],
  trustedFields: ReadonlyArray<Readonly<{ fieldName: string; value: string }>> = [],
  providerJobKey?: string,
) {
  const fields = fixtureFieldEvidence(controls, trustedFields, providerJobKey);
  const partOrder: Array<{ kind: "field" | "file"; index: number }> = [];
  let fieldIndex = 0;
  let fileIndex = 0;
  for (const control of controls) {
    if (!control.name) continue;
    if (control.kind === "file") {
      for (const _file of control.files ?? []) {
        partOrder.push({ kind: "file", index: fileIndex });
        fileIndex += 1;
      }
      continue;
    }
    if ((control.kind === "checkbox" || control.kind === "radio") && !control.checked) continue;
    partOrder.push({ kind: "field", index: fieldIndex });
    fieldIndex += 1;
  }
  return { fields, partOrder };
}

function fileEvidenceForSnapshotPath(path: string) {
  const name = path.split(/[\\/]/u).at(-1) ?? "";
  const sha256 = /-([a-f0-9]{64})\.pdf$/u.exec(name)?.[1] ?? "f".repeat(64);
  return { name, byteLength: 1_024, sha256 };
}

function leverSubmitAttribute(currentSelector: string, selector: string, name: string): string | null {
  if (selector !== currentSelector) return null;
  if (name === "id") return "btn-submit";
  if (name === "type") return "submit";
  if (currentSelector.includes("data-qa") && name === "data-qa") return "btn-submit";
  return null;
}
