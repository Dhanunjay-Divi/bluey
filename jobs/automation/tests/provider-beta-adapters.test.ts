import { describe, expect, it } from "vitest";
import greenhouseNegativeJson from "./fixtures/greenhouse/negative-cases.json";
import greenhousePublicJson from "./fixtures/greenhouse/public-modern.json";
import leverNegativeJson from "./fixtures/lever/negative-cases.json";
import leverCurrentJson from "./fixtures/lever/us-current.json";
import {
  GREENHOUSE_ADAPTER_PROFILE,
  LEVER_ADAPTER_PROFILE,
  LEVER_CAPABILITY,
  GreenhouseAdapter,
  LeverApplicationStateMachine,
  createDefaultAdapterRegistry,
  executeApplication,
  submissionPolicy,
  type AdapterContext,
  type ApplicationAdapter,
  type BrowserLocator,
  type BrowserPage,
  type FormControl,
  type InterventionRequest,
} from "../src/index.js";

interface GreenhouseFixture {
  url: string;
  title: string;
  body: string;
  confirmationBody: string;
  markers: string[];
  controls: FormControl[];
}

interface LeverFixture {
  postingUrl: string;
  applicationUrl: string;
  title: string;
  applySelector: string;
  submitSelector: string;
  controls: FormControl[];
  answers: Record<string, string>;
}

interface NegativeCases {
  ambiguousApply: { count: number };
  ambiguousSubmit: { count: number };
  unknownRequired: FormControl;
  guardedRequired: FormControl[];
  challenges: Array<{ kind: InterventionRequest["kind"]; body: string }>;
  wrongHost: string;
  unclearConfirmation: { body: string; url: string };
  closedBody: string;
  unsupportedUrl: string;
}

const GREENHOUSE = greenhousePublicJson as GreenhouseFixture;
const GREENHOUSE_NEGATIVE = greenhouseNegativeJson as NegativeCases;
const LEVER = leverCurrentJson as LeverFixture;
const LEVER_NEGATIVE = leverNegativeJson as NegativeCases;

describe("registered provider-specific beta adapters", () => {
  it("exports review-only metadata and replaces only the generic Greenhouse and Lever adapters", () => {
    const registry = createDefaultAdapterRegistry();
    const greenhouse = registry.resolve(GREENHOUSE.url);
    const lever = registry.resolve(LEVER.applicationUrl);
    const adapters: ApplicationAdapter[] = [greenhouse, lever];

    expect(greenhouse).toBeInstanceOf(GreenhouseAdapter);
    expect(lever).toBeInstanceOf(LeverApplicationStateMachine);
    expect(registry.resolve("https://jobs.eu.lever.co/acme/posting-id/apply"))
      .toBeInstanceOf(LeverApplicationStateMachine);
    expect(adapters.map((adapter) => adapter.version)).toEqual([
      GREENHOUSE_ADAPTER_PROFILE.version,
      LEVER_ADAPTER_PROFILE.version,
    ]);
    expect(GREENHOUSE_ADAPTER_PROFILE).toMatchObject({
      maturity: "beta",
      capability: "beta_review",
      submissionMode: "review_only",
      certified: false,
      requiresFinalReview: true,
    });
    expect(LEVER_ADAPTER_PROFILE).toMatchObject({
      maturity: "beta",
      capability: "beta_review",
      submissionMode: "review_only",
      certified: false,
      requiresFinalReview: true,
    });
    expect(LEVER_CAPABILITY).toMatchObject({ release: "beta", mode: "review_only", certified: false });

    expect(registry.resolve("https://jobs.ashbyhq.com/acme/role").kind).toBe("ashby");
    expect(registry.resolve("https://jobs.smartrecruiters.com/Acme/role").kind).toBe("smartrecruiters");
    expect(registry.resolve("https://acme.wd5.myworkdayjobs.com/jobs/role").kind).toBe("workday");
    expect(registry.resolve("https://careers.acme.example/role").kind).toBe("semantic");
    expect(submissionPolicy(GREENHOUSE.url).capability).toBe("beta_review");
    expect(submissionPolicy("https://careers.acme.example/role")).toMatchObject({
      policy: "handoff",
      capability: "unknown_review",
    });
  });

  for (const provider of ["greenhouse", "lever"] as const) {
    it(`runs packet guards before any ${provider} page action`, async () => {
      const page = provider === "greenhouse"
        ? new GreenhousePage()
        : new LeverPage();
      const context = makeContext(page, provider === "lever" ? LEVER.answers : {});
      context.packet.browserProfileId = undefined;

      await expect(executeApplication(context)).rejects.toThrow("browserProfileId");
      expect(page.interactions).toBe(0);
      expect(page.submitClicks).toBe(0);
    });
  }
});

describe("Greenhouse review-only safety", () => {
  it("clicks zero times before final approval, once after approval, and never again on re-entry", async () => {
    let approved = false;
    const page = new GreenhousePage();
    const context = makeContext(page);
    const registry = createDefaultAdapterRegistry(undefined, {
      greenhouse: {
        finalReviewApproval: () => approved,
        now: () => new Date("2026-07-11T12:00:00.000Z"),
      },
    });

    const beforeApproval = await executeApplication(context, registry);
    expect(beforeApproval.receipt).toMatchObject({
      status: "needs_input",
      intervention: { title: "Review the Greenhouse application" },
    });
    expect(page.submitClicks).toBe(0);

    approved = true;
    const afterApproval = await executeApplication(context, registry);
    const reentered = await executeApplication(context, registry);
    expect(afterApproval.receipt).toMatchObject({
      status: "submitted",
      submittedAt: "2026-07-11T12:00:00.000Z",
    });
    expect(reentered.receipt.status).toBe("submitted");
    expect(page.submitClicks).toBe(1);
  });

  it("blocks ambiguous provider-scoped Apply and Submit controls", async () => {
    const applyPage = new GreenhousePage({
      controls: [],
      markers: [],
      applyCount: GREENHOUSE_NEGATIVE.ambiguousApply.count,
      submitCount: 0,
    });
    const applyResult = await executeApplication(makeContext(applyPage));
    expect(applyResult.receipt.issues).toEqual(expect.arrayContaining([
      expect.objectContaining({ message: expect.stringContaining("provider-scoped Apply") }),
    ]));
    expect(applyPage.applyClicks).toBe(0);

    const submitPage = new GreenhousePage({
      submitCount: GREENHOUSE_NEGATIVE.ambiguousSubmit.count,
    });
    const registry = createDefaultAdapterRegistry(undefined, {
      greenhouse: { finalReviewApproval: () => true },
    });
    const submitResult = await executeApplication(makeContext(submitPage), registry);
    expect(submitResult.receipt).toMatchObject({ status: "needs_input" });
    expect(submitResult.receipt.issues[0]?.message).toContain("provider-scoped Submit");
    expect(submitPage.submitClicks).toBe(0);
  });

  it("fails closed for unknown, sensitive, and sponsorship questions", async () => {
    const unknownPage = new GreenhousePage({
      controls: [...GREENHOUSE.controls, GREENHOUSE_NEGATIVE.unknownRequired],
    });
    const unknownResult = await executeApplication(makeContext(unknownPage));
    expect(unknownResult.receipt.issues).toEqual(expect.arrayContaining([
      expect.objectContaining({ field: GREENHOUSE_NEGATIVE.unknownRequired.label, severity: "blocking" }),
    ]));
    expect(unknownPage.submitClicks).toBe(0);

    const guardedPage = new GreenhousePage({
      controls: [...GREENHOUSE.controls, ...GREENHOUSE_NEGATIVE.guardedRequired],
    });
    const guardedResult = await executeApplication(makeContext(guardedPage, {
      "Gender identity": "Prefer not to say",
      "Will you require visa sponsorship? Yes": "yes",
    }));
    expect(guardedResult.receipt.issues.map((issue) => issue.field)).toEqual(expect.arrayContaining([
      "Gender identity",
      "Will you require visa sponsorship? Yes",
    ]));
    expect(guardedPage.control(GREENHOUSE_NEGATIVE.guardedRequired[0]!.selector).value).toBe("");
    expect(guardedPage.submitClicks).toBe(0);
  });

  for (const challenge of GREENHOUSE_NEGATIVE.challenges) {
    it(`pauses for a Greenhouse ${challenge.kind} challenge`, async () => {
      const page = new GreenhousePage({ body: challenge.body });
      const result = await executeApplication(makeContext(page));
      expect(result.receipt).toMatchObject({
        status: "needs_input",
        intervention: { kind: challenge.kind },
      });
      expect(page.submitClicks).toBe(0);
    });
  }

  it("rejects wrong hosts, closed postings, and unsupported variants", async () => {
    const adapter = new GreenhouseAdapter();
    expect(adapter.detect(new URL(GREENHOUSE_NEGATIVE.wrongHost))).toBe(false);
    expect(adapter.detect(new URL(GREENHOUSE.url.replace("https:", "http:")))).toBe(false);

    const wrongHostPage = new GreenhousePage({
      url: GREENHOUSE_NEGATIVE.wrongHost,
      controls: [],
      markers: ["#application_form", "#submit_app"],
      submitCount: 0,
    });
    const wrongHostContext = makeContext(wrongHostPage);
    await adapter.prepare(wrongHostContext);
    await adapter.fill(wrongHostContext);
    expect(await adapter.validate(wrongHostContext)).toEqual(expect.arrayContaining([
      expect.objectContaining({ severity: "blocking" }),
    ]));
    expect((await adapter.submit(wrongHostContext)).status).toBe("failed");

    for (const options of [
      { body: GREENHOUSE_NEGATIVE.closedBody },
      {
        url: GREENHOUSE_NEGATIVE.unsupportedUrl,
        controls: [],
        markers: [],
        submitCount: 0,
      },
    ]) {
      const page = new GreenhousePage(options);
      const result = await executeApplication(makeContext(page));
      expect(result.receipt.status).not.toBe("submitted");
      expect(page.submitClicks).toBe(0);
    }
  });

  it("treats unclear confirmation as uncertain and never retries", async () => {
    const page = new GreenhousePage({
      afterSubmitBody: GREENHOUSE_NEGATIVE.unclearConfirmation.body,
      afterSubmitUrl: GREENHOUSE_NEGATIVE.unclearConfirmation.url,
    });
    const context = makeContext(page);
    const registry = createDefaultAdapterRegistry(undefined, {
      greenhouse: { finalReviewApproval: () => true },
    });

    const first = await executeApplication(context, registry);
    const second = await executeApplication(context, registry);
    expect(first.receipt).toMatchObject({ status: "needs_input" });
    expect(first.receipt.confirmationText).toBeUndefined();
    expect(second.receipt.status).not.toBe("submitted");
    expect(page.submitClicks).toBe(1);
  });
});

describe("Lever review-only safety", () => {
  it("clicks zero times before final approval, once after approval, and never again on re-entry", async () => {
    let approved = false;
    const page = new LeverPage({
      afterSubmitBody: "Thank you for applying. Your application has been received.",
    });
    const context = makeContext(page, LEVER.answers);
    const registry = createDefaultAdapterRegistry(undefined, {
      lever: {
        finalReviewApproval: () => approved,
        now: () => new Date("2026-07-11T12:00:00.000Z"),
      },
    });

    const beforeApproval = await executeApplication(context, registry);
    expect(beforeApproval.receipt).toMatchObject({
      status: "needs_input",
      intervention: { title: "Review this Lever application" },
    });
    expect(page.submitClicks).toBe(0);

    approved = true;
    const afterApproval = await executeApplication(context, registry);
    const reentered = await executeApplication(context, registry);
    expect(afterApproval.receipt).toMatchObject({
      status: "submitted",
      submittedAt: "2026-07-11T12:00:00.000Z",
    });
    expect(reentered.receipt.status).toBe("submitted");
    expect(page.submitClicks).toBe(1);
  });

  it("blocks ambiguous provider-scoped Apply and Submit controls", async () => {
    const applyPage = new LeverPage({
      startAtPosting: true,
      applyCount: LEVER_NEGATIVE.ambiguousApply.count,
    });
    const applyResult = await executeApplication(makeContext(applyPage, LEVER.answers));
    expect(applyResult.receipt.issues).toEqual(expect.arrayContaining([
      expect.objectContaining({ message: expect.stringContaining("provider-scoped Apply") }),
    ]));
    expect(applyPage.applyClicks).toBe(0);

    const submitPage = new LeverPage({
      submitCount: LEVER_NEGATIVE.ambiguousSubmit.count,
    });
    const registry = createDefaultAdapterRegistry(undefined, {
      lever: { finalReviewApproval: () => true },
    });
    const submitResult = await executeApplication(makeContext(submitPage, LEVER.answers), registry);
    expect(submitResult.receipt).toMatchObject({ status: "needs_input" });
    expect(submitResult.receipt.issues[0]?.message).toContain("provider-scoped Submit");
    expect(submitPage.submitClicks).toBe(0);
  });

  it("fails closed for unknown, sensitive, and authorization questions", async () => {
    const unknownPage = new LeverPage({
      controls: [...LEVER.controls, LEVER_NEGATIVE.unknownRequired],
    });
    const unknownResult = await executeApplication(makeContext(unknownPage, LEVER.answers));
    expect(unknownResult.receipt.issues).toEqual(expect.arrayContaining([
      expect.objectContaining({ field: LEVER_NEGATIVE.unknownRequired.label, severity: "blocking" }),
    ]));
    expect(unknownPage.submitClicks).toBe(0);

    const guardedPage = new LeverPage({
      controls: [...LEVER.controls, ...LEVER_NEGATIVE.guardedRequired],
    });
    const guardedResult = await executeApplication(makeContext(guardedPage, {
      ...LEVER.answers,
      "Gender identity": "Prefer not to say",
      work_authorization: "yes",
    }));
    expect(guardedResult.receipt.issues.map((issue) => issue.field)).toEqual(expect.arrayContaining([
      "Gender identity",
      "Are you legally authorized to work? Yes",
    ]));
    expect(guardedPage.control(LEVER_NEGATIVE.guardedRequired[0]!.selector).value).toBe("");
    expect(guardedPage.submitClicks).toBe(0);
  });

  for (const challenge of LEVER_NEGATIVE.challenges) {
    it(`pauses for a Lever ${challenge.kind} challenge`, async () => {
      const page = new LeverPage({ body: challenge.body });
      const result = await executeApplication(makeContext(page, LEVER.answers));
      expect(result.receipt).toMatchObject({
        status: "needs_input",
        intervention: { kind: challenge.kind },
      });
      expect(page.submitClicks).toBe(0);
    });
  }

  it("rejects wrong hosts, closed postings, and unsupported variants", async () => {
    const adapter = new LeverApplicationStateMachine();
    expect(adapter.detect(new URL(LEVER_NEGATIVE.wrongHost))).toBe(false);
    expect(adapter.detect(new URL(LEVER.applicationUrl.replace("https:", "http:")))).toBe(false);

    const wrongHostPage = new LeverPage({ url: LEVER_NEGATIVE.wrongHost });
    const wrongHostContext = makeContext(wrongHostPage, LEVER.answers);
    await adapter.prepare(wrongHostContext);
    await adapter.fill(wrongHostContext);
    expect(await adapter.validate(wrongHostContext)).toEqual(expect.arrayContaining([
      expect.objectContaining({ severity: "blocking" }),
    ]));
    expect((await adapter.submit(wrongHostContext, { finalReviewApproved: true })).status).not.toBe("submitted");

    for (const options of [
      { body: LEVER_NEGATIVE.closedBody },
      { url: LEVER_NEGATIVE.unsupportedUrl },
    ]) {
      const page = new LeverPage(options);
      const result = await executeApplication(makeContext(page, LEVER.answers));
      expect(result.receipt.status).not.toBe("submitted");
      expect(page.submitClicks).toBe(0);
    }
  });

  it("treats unclear confirmation as uncertain and never retries", async () => {
    const page = new LeverPage({
      afterSubmitBody: LEVER_NEGATIVE.unclearConfirmation.body,
      afterSubmitUrl: LEVER_NEGATIVE.unclearConfirmation.url,
    });
    const context = makeContext(page, LEVER.answers);
    const registry = createDefaultAdapterRegistry(undefined, {
      lever: { finalReviewApproval: () => true },
    });

    const first = await executeApplication(context, registry);
    const second = await executeApplication(context, registry);
    expect(first.receipt).toMatchObject({ status: "needs_input" });
    expect(first.receipt.confirmationText).toBeUndefined();
    expect(second.receipt.status).not.toBe("submitted");
    expect(page.submitClicks).toBe(1);
  });
});

function makeContext(page: BrowserPage, answers: Record<string, string> = {}): AdapterContext {
  return {
    runner: "local",
    runId: "run-provider-beta",
    accountId: "account-provider-beta",
    page,
    packet: {
      applicationId: "application-provider-beta",
      jobId: "job-provider-beta",
      resumeVersionId: "resume-provider-beta",
      approvedPacketChecksum: "c".repeat(64),
      resumePath: "/packets/resume.pdf",
      coverLetterPath: "/packets/cover-letter.pdf",
      answers: {
        first_name: "Ada",
        last_name: "Lovelace",
        full_name: "Ada Lovelace",
        email: "ada@example.com",
        phone: "+1 212 555 0100",
        ...answers,
      },
      verifiedClaimIds: ["claim-1"],
      applicationIdentityId: "identity-provider-beta",
      applicationEmail: "ada@example.com",
      browserProfileId: "profile-provider-beta",
    },
    async log() {},
  };
}

interface GreenhousePageOptions {
  url?: string;
  body?: string;
  controls?: FormControl[];
  markers?: string[];
  applyCount?: number;
  submitCount?: number;
  afterSubmitBody?: string;
  afterSubmitUrl?: string;
}

class GreenhousePage implements BrowserPage {
  interactions = 0;
  applyClicks = 0;
  submitClicks = 0;

  private currentUrl: string;
  private body: string;
  private submitted = false;
  private readonly controlsState: FormControl[];
  private readonly markers: string[];

  constructor(private readonly options: GreenhousePageOptions = {}) {
    this.currentUrl = options.url ?? GREENHOUSE.url;
    this.body = options.body ?? GREENHOUSE.body;
    this.controlsState = copyControls(options.controls ?? GREENHOUSE.controls);
    this.markers = options.markers ?? GREENHOUSE.markers;
  }

  url(): string {
    this.interactions += 1;
    return this.currentUrl;
  }

  async title(): Promise<string> {
    this.interactions += 1;
    return GREENHOUSE.title;
  }

  async controls(): Promise<FormControl[]> {
    this.interactions += 1;
    return this.submitted ? [] : copyControls(this.controlsState);
  }

  async bodyText(): Promise<string> {
    this.interactions += 1;
    return this.body;
  }

  async waitForSettled(): Promise<void> {
    this.interactions += 1;
  }

  async screenshot(): Promise<Uint8Array> {
    this.interactions += 1;
    return new Uint8Array();
  }

  control(selector: string): FormControl {
    const control = this.controlsState.find((candidate) => candidate.selector === selector);
    if (!control) throw new Error(`Missing Greenhouse control ${selector}`);
    return control;
  }

  locator(selector: string): BrowserLocator {
    this.interactions += 1;
    const control = this.controlsState.find((candidate) => candidate.selector === selector);
    const isSubmit = greenhouseSubmitQuery(selector);
    const isApply = !isSubmit && greenhouseApplyQuery(selector);
    const count = (): number => {
      if (control && !this.submitted) return 1;
      if (isSubmit && !this.submitted) return this.options.submitCount ?? 1;
      if (isApply && !this.submitted) return this.options.applyCount ?? 0;
      if (this.markers.includes(selector) && !this.submitted) return 1;
      return 0;
    };
    return {
      count: async () => count(),
      fill: async (value) => { if (control) control.value = value; },
      click: async () => {
        if (isApply) {
          this.applyClicks += 1;
          return;
        }
        if (!isSubmit) return;
        this.submitClicks += 1;
        this.submitted = true;
        this.body = this.options.afterSubmitBody ?? GREENHOUSE.confirmationBody;
        this.currentUrl = this.options.afterSubmitUrl ?? this.currentUrl;
      },
      textContent: async () => "",
      getAttribute: async () => null,
      isVisible: async () => count() > 0,
      selectOption: async (value) => { if (control) control.value = value; },
      setChecked: async (checked) => { if (control) control.checked = checked; },
      setInputFiles: async (paths) => {
        if (control) control.value = paths[0]?.split("/").at(-1) ?? "";
      },
    };
  }
}

interface LeverPageOptions {
  url?: string;
  body?: string;
  controls?: FormControl[];
  startAtPosting?: boolean;
  applyCount?: number;
  submitCount?: number;
  afterSubmitBody?: string;
  afterSubmitUrl?: string;
}

class LeverPage implements BrowserPage {
  interactions = 0;
  applyClicks = 0;
  submitClicks = 0;

  private currentUrl: string;
  private body: string;
  private onApplication: boolean;
  private submitted = false;
  private readonly controlsState: FormControl[];

  constructor(private readonly options: LeverPageOptions = {}) {
    this.onApplication = !options.startAtPosting;
    this.currentUrl = options.url
      ?? (this.onApplication ? LEVER.applicationUrl : LEVER.postingUrl);
    this.body = options.body ?? (this.onApplication ? "Submit your application" : "Lever hosted posting");
    this.controlsState = copyControls(options.controls ?? LEVER.controls);
  }

  url(): string {
    this.interactions += 1;
    return this.currentUrl;
  }

  async title(): Promise<string> {
    this.interactions += 1;
    return LEVER.title;
  }

  async controls(): Promise<FormControl[]> {
    this.interactions += 1;
    return this.onApplication && !this.submitted ? copyControls(this.controlsState) : [];
  }

  async bodyText(): Promise<string> {
    this.interactions += 1;
    return this.body;
  }

  async waitForSettled(): Promise<void> {
    this.interactions += 1;
  }

  async screenshot(): Promise<Uint8Array> {
    this.interactions += 1;
    return new Uint8Array();
  }

  control(selector: string): FormControl {
    const control = this.controlsState.find((candidate) => candidate.selector === selector);
    if (!control) throw new Error(`Missing Lever control ${selector}`);
    return control;
  }

  locator(selector: string): BrowserLocator {
    this.interactions += 1;
    const control = this.controlsState.find((candidate) => candidate.selector === selector);
    const isSubmit = leverSubmitQuery(selector);
    const isApply = !isSubmit && leverApplyQuery(selector);
    const providerLabel = selector.startsWith(".application-question:has(");
    const count = (): number => {
      if (control && this.onApplication && !this.submitted) return 1;
      if (selector === "#application-form") return this.onApplication && !this.submitted ? 1 : 0;
      if (isSubmit) return this.onApplication && !this.submitted ? this.options.submitCount ?? 1 : 0;
      if (isApply) return !this.onApplication ? this.options.applyCount ?? 1 : 0;
      if (providerLabel) return 0;
      return 0;
    };
    return {
      count: async () => count(),
      fill: async (value) => { if (control) control.value = value; },
      click: async () => {
        if (isApply) {
          this.applyClicks += 1;
          this.onApplication = true;
          this.currentUrl = LEVER.applicationUrl;
          this.body = this.options.body ?? "Submit your application";
          return;
        }
        if (!isSubmit) return;
        this.submitClicks += 1;
        this.submitted = true;
        this.body = this.options.afterSubmitBody ?? "Thank you for applying.";
        this.currentUrl = this.options.afterSubmitUrl ?? this.currentUrl;
      },
      textContent: async () => "",
      getAttribute: async () => null,
      isVisible: async () => count() > 0,
      selectOption: async (value) => { if (control) control.value = value; },
      setChecked: async (checked) => { if (control) control.checked = checked; },
      setInputFiles: async (paths) => {
        if (control) control.value = paths[0]?.split("/").at(-1) ?? "";
      },
    };
  }
}

function copyControls(controls: FormControl[]): FormControl[] {
  return controls.map((control) => ({
    ...control,
    options: control.options?.map((option) => ({ ...option })),
  }));
}

function greenhouseSubmitQuery(selector: string): boolean {
  return selector.includes("#submit_app") || selector.includes("submit-application");
}

function greenhouseApplyQuery(selector: string): boolean {
  return selector.includes("#apply_button")
    || selector.includes("href='#app'")
    || selector.includes("href$='/apply'")
    || selector.includes("apply-button");
}

function leverSubmitQuery(selector: string): boolean {
  return selector.includes("btn-submit") || selector.includes("template-btn-submit");
}

function leverApplyQuery(selector: string): boolean {
  return selector.includes("show-page-apply") || selector.includes("postings-btn");
}
