import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import type {
  AdapterContext,
  BrowserLocator,
  BrowserPage,
  FormControl,
} from "../src/contracts.js";
import {
  LEVER_CAPABILITY,
  LEVER_STATES,
  LeverApplicationStateMachine,
} from "../src/providers/lever.js";

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
        expect(page.control(selector).value).toBe(expected);
      }
      expect(adapter.stateHistory(context).map((state) => state.state)).toEqual(LEVER_STATES);
      expect(adapter.currentState(context)).toMatchObject({ state: "receipt", outcome: "review_required" });
    });
  }

  it("uploads the packet resume and cover letter to their distinct Lever fields", async () => {
    const fixture = loadFixture("eu-custom.json");
    const page = new LeverFixturePage(fixture);
    const adapter = new LeverApplicationStateMachine();
    const context = makeContext(page, fixture.answers);

    await adapter.prepare(context);
    await adapter.fill(context);

    expect(page.control("[data-bluey-field-id='resume']").value).toBe("resume.pdf");
    expect(page.control("[data-bluey-field-id='cover']").value).toBe("cover-letter.pdf");
    expect(page.uploads).toEqual([
      ["[data-bluey-field-id='resume']", "/packets/resume.pdf"],
      ["[data-bluey-field-id='cover']", "/packets/cover-letter.pdf"],
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
    const context = makeContext(page, fixture.answers, { events: submitHooks });

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
      afterSubmitUrl: "https://jobs.eu.lever.co/atlas/confirmation/candidate-123",
    });
    const adapter = new LeverApplicationStateMachine();
    const submitHooks: string[] = [];
    const context = makeContext(page, fixture.answers, { events: submitHooks });

    await prepareValidApplication(adapter, context);
    const receipt = await adapter.submit(context, { finalReviewApproved: true });

    expect(receipt.status).toBe("submitted");
    expect(receipt.confirmationText).toBe(confirmation);
    expect(receipt.confirmationUrl).toBe("https://jobs.eu.lever.co/atlas/confirmation/candidate-123");
    expect(receipt.submittedAt).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    expect(page.submitClicks).toBe(1);
    expect(submitHooks).toEqual(["before", "after:activated"]);
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
  hooks?: { events: string[]; before?: () => Promise<void> },
): AdapterContext {
  return {
    runner: "local",
    runId: "run-lever-123",
    accountId: "account-123",
    page,
    packet: {
      applicationId: "application-123",
      jobId: "job-123",
      resumeVersionId: "resume-version-123",
      resumePath: "/packets/resume.pdf",
      coverLetterPath: "/packets/cover-letter.pdf",
      answers: { ...answers },
      verifiedClaimIds: ["claim-1"],
      applicationIdentityId: "identity-123",
      applicationEmail: "ada@example.com",
      browserProfileId: "browser-profile-123",
    },
    async log() {},
    ...(hooks ? {
      async beforeFinalSubmit() {
        hooks.events.push("before");
        await hooks.before?.();
      },
      async afterFinalSubmit(outcome: string) { hooks.events.push(`after:${outcome}`); },
    } : {}),
  };
}

function loadFixture(name: string): LeverFixture {
  return JSON.parse(readFileSync(new URL(`./fixtures/lever/${name}`, import.meta.url), "utf8")) as LeverFixture;
}

function asDirectApplication(fixture: LeverFixture): LeverFixture {
  return { ...fixture, startAtPosting: false, controls: fixture.controls.map((control) => ({ ...control })) };
}

interface FixturePageOptions {
  body?: string;
  afterSubmitBody?: string;
  afterSubmitUrl?: string;
  formCount?: number;
  submitCount?: number;
  throwAfterSubmit?: boolean;
}

class LeverFixturePage implements BrowserPage {
  readonly uploads: Array<[string, string]> = [];
  submitClicks = 0;

  private currentUrl: string;
  private body: string;
  private onApplication: boolean;
  private readonly controlsState: FormControl[];

  constructor(
    private readonly fixture: LeverFixture,
    private readonly options: FixturePageOptions = {},
  ) {
    this.onApplication = !fixture.startAtPosting;
    this.currentUrl = this.onApplication ? fixture.applicationUrl : fixture.postingUrl;
    this.body = options.body ?? (this.onApplication ? "Submit your application" : "Lever hosted job posting");
    this.controlsState = fixture.controls.map((control) => ({
      ...control,
      options: control.options?.map((option) => ({ ...option })),
    }));
  }

  url(): string {
    return this.currentUrl;
  }

  title(): Promise<string> {
    return Promise.resolve(this.fixture.title);
  }

  controls(): Promise<FormControl[]> {
    if (!this.onApplication) return Promise.resolve([]);
    return Promise.resolve(this.controlsState.map((control) => ({
      ...control,
      options: control.options?.map((option) => ({ ...option })),
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

  locator(selector: string): BrowserLocator {
    const control = this.controlsState.find((candidate) => candidate.selector === selector);
    const providerLabelSelector = Object.entries(this.fixture.providerLabels ?? {}).find(
      ([controlSelector]) => selector === `.application-question:has(${controlSelector}) .application-label`,
    );
    const count = (): number => {
      if (control) return this.onApplication ? 1 : 0;
      if (providerLabelSelector) return this.onApplication ? 1 : 0;
      if (selector === "#application-form") return this.onApplication ? this.options.formCount ?? 1 : 0;
      if (selector === this.fixture.applySelector) return this.onApplication ? 0 : 1;
      if (selector === this.fixture.submitSelector) return this.onApplication ? this.options.submitCount ?? 1 : 0;
      return 0;
    };

    return {
      count: async () => count(),
      fill: async (value) => {
        if (control) control.value = value;
      },
      click: async () => {
        if (selector === this.fixture.applySelector && !this.onApplication) {
          this.onApplication = true;
          this.currentUrl = this.fixture.applicationUrl;
          this.body = this.options.body ?? "Submit your application";
          return;
        }
        if (selector !== this.fixture.submitSelector || !this.onApplication) return;
        this.submitClicks += 1;
        if (this.options.afterSubmitBody !== undefined) this.body = this.options.afterSubmitBody;
        if (this.options.afterSubmitUrl !== undefined) this.currentUrl = this.options.afterSubmitUrl;
        if (this.options.throwAfterSubmit) throw new Error("Synthetic response loss after click");
      },
      textContent: async () => providerLabelSelector?.[1] ?? "",
      getAttribute: async () => null,
      isVisible: async () => count() > 0,
      selectOption: async (value) => {
        if (control) control.value = value;
      },
      setChecked: async (checked) => {
        if (control) control.checked = checked;
      },
      setInputFiles: async (paths) => {
        if (!control) return;
        const path = paths[0] ?? "";
        control.value = path.split("/").at(-1) ?? "";
        this.uploads.push([selector, path]);
      },
    };
  }
}
