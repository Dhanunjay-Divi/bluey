import { describe, expect, it } from "vitest";
import {
  createDefaultAdapterRegistry,
  executeApplication,
  type AdapterContext,
  type BrowserLocator,
  type BrowserPage,
  type FormControl,
} from "../src/index.js";

const PROVIDERS = [
  ["greenhouse", "https://boards.greenhouse.io/acme/jobs/123"],
  ["lever", "https://jobs.lever.co/acme/123"],
  ["ashby", "https://jobs.ashbyhq.com/acme/123"],
  ["smartrecruiters", "https://jobs.smartrecruiters.com/Acme/123"],
  ["workday", "https://acme.wd5.myworkdayjobs.com/en-US/jobs/job/123"],
] as const;

describe("deterministic ATS application adapters", () => {
  for (const [provider, url] of PROVIDERS.slice(2)) {
    it(`fills and submits a ${provider} fixture`, async () => {
      const page = new FixturePage(url, [
        field("first_name", "First name", true),
        field("email", "Email address", true, "email"),
        field("resume", "Resume", true, "file"),
      ]);
      const events: string[] = [];
      const submitHooks: string[] = [];
      const result = await executeApplication(context(page, events, submitHooks));
      expect(result.adapter).toBe(provider);
      expect(result.receipt.status).toBe("submitted");
      expect(page.control("first_name").value).toBe("Ada");
      expect(page.control("email").value).toBe("ada@example.com");
      expect(page.control("resume").value).toBe("resume.pdf");
      expect(events).toContain("application_execution_finished");
      expect(submitHooks).toEqual(["before", "after:activated"]);
    });
  }

  it("uses the constrained semantic adapter for a compatible direct employer form", async () => {
    const page = new FixturePage("https://careers.acme.com/jobs/123/apply", [
      field("first_name", "First name", true),
      field("email", "Email address", true, "email"),
      field("resume", "Resume", true, "file"),
    ]);
    const submitHooks: string[] = [];
    const result = await executeApplication(context(page, [], submitHooks));
    expect(result.adapter).toBe("semantic");
    expect(result.receipt.status).toBe("submitted");
    expect(submitHooks).toEqual(["before", "after:activated"]);
  });

  it("pauses on an unanswered required question", async () => {
    const page = new FixturePage(PROVIDERS[0][1], [field("legal", "Are you bound by a non-compete?", true)]);
    const result = await executeApplication(context(page, []));
    expect(result.receipt.status).toBe("needs_input");
    expect(result.receipt.intervention?.kind).toBe("unknown_question");
  });

  it("selects the matching option in a required radio group", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [
      { ...field("authorized_yes", "Authorized to work Yes", true, "radio"), name: "authorized", value: "yes" },
      { ...field("authorized_no", "Authorized to work No", true, "radio"), name: "authorized", value: "no" },
    ]);
    const result = await executeApplication({
      ...context(page, []),
      packet: {
        ...context(page, []).packet,
        answers: { work_authorization: "yes" },
      },
    });

    expect(result.receipt.status).toBe("submitted");
    expect(page.control("authorized_yes").checked).toBe(true);
    expect(page.control("authorized_no").checked).not.toBe(true);
  });

  it("does not claim success without confirmation evidence", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [field("first_name", "First name", true)], "Application form", false);
    const result = await executeApplication(context(page, []));

    expect(result.receipt.status).toBe("needs_input");
    expect(result.receipt.intervention?.kind).toBe("browser_takeover");
  });

  it("pauses before a CAPTCHA", async () => {
    const page = new FixturePage(PROVIDERS[1][1], [], "Verify you are human. CAPTCHA");
    const result = await executeApplication(context(page, []));
    expect(result.receipt.status).toBe("needs_input");
    expect(result.receipt.intervention?.kind).toBe("captcha");
  });

  it("does not fence validation or challenge-only paths", async () => {
    const validationHooks: string[] = [];
    const validationPage = new FixturePage(PROVIDERS[4][1], [
      field("non_compete", "Are you bound by a non-compete?", true),
    ]);
    const validation = await executeApplication(context(validationPage, [], validationHooks));
    expect(validation.receipt.status).toBe("needs_input");
    expect(validationHooks).toEqual([]);

    const challengeHooks: string[] = [];
    const challengePage = new FixturePage(PROVIDERS[4][1], [], "Verify you are human. CAPTCHA");
    const challenge = await executeApplication(context(challengePage, [], challengeHooks));
    expect(challenge.receipt.intervention?.kind).toBe("captcha");
    expect(challengeHooks).toEqual([]);
  });

  it("does not fence Next and fences the eventual final submit exactly once", async () => {
    const page = new FixturePage(
      PROVIDERS[4][1],
      [field("first_name", "First name", true)],
      "Application form",
      true,
      true,
    );
    const submitHooks: string[] = [];

    const result = await executeApplication(context(page, [], submitHooks));

    expect(result.receipt.status).toBe("submitted");
    expect(page.nextClicks).toBe(1);
    expect(page.submitClicks).toBe(1);
    expect(submitHooks).toEqual(["before", "after:activated"]);
  });

  it("does not click when the durable before-hook is uncertain", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [field("first_name", "First name", true)]);
    const adapterContext = context(page, [], []);
    let hookCalls = 0;
    adapterContext.beforeFinalSubmit = async () => {
      hookCalls += 1;
      throw new Error("durable fence response unavailable");
    };

    await expect(executeApplication(adapterContext)).rejects.toThrow("durable fence response unavailable");
    expect(hookCalls).toBe(1);
    expect(page.submitClicks).toBe(0);
  });
});

function context(page: BrowserPage, events: string[], submitHooks?: string[]): AdapterContext {
  return {
    runner: "local",
    runId: "run-123",
    accountId: "account-123",
    page,
    packet: {
      applicationId: "application-123",
      jobId: "job-123",
      resumeVersionId: "resume-version-123",
      resumePath: "/tmp/resume.pdf",
      answers: { first_name: "Ada", email: "ada@example.com" },
      verifiedClaimIds: ["claim-1"],
      applicationIdentityId: "identity-123",
      applicationEmail: "ada@example.com",
      browserProfileId: "profile-123",
    },
    async log(event) { events.push(event); },
    ...(submitHooks ? {
      async beforeFinalSubmit() { submitHooks.push("before"); },
      async afterFinalSubmit(outcome: string) { submitHooks.push(`after:${outcome}`); },
    } : {}),
  };
}

function field(selector: string, label: string, required: boolean, kind: FormControl["kind"] = "text"): FormControl {
  return { selector, label, required, kind, name: selector, placeholder: "", value: "" };
}

class FixturePage implements BrowserPage {
  private submitted = false;
  private finalStep: boolean;
  submitClicks = 0;
  nextClicks = 0;

  constructor(
    private readonly currentUrl: string,
    private readonly fields: FormControl[],
    private body = "Application form",
    private readonly confirmsSubmission = true,
    startBeforeFinalStep = false,
  ) {
    this.finalStep = !startBeforeFinalStep;
  }

  url(): string { return this.currentUrl; }
  async title(): Promise<string> { return "Software Engineer | Acme"; }
  controls(): Promise<FormControl[]> { return Promise.resolve(this.submitted ? [] : this.fields.map((item) => ({ ...item }))); }
  bodyText(): Promise<string> { return Promise.resolve(this.submitted && this.confirmsSubmission ? "Thank you for applying. Application submitted." : this.body); }
  waitForSettled(): Promise<void> { return Promise.resolve(); }
  screenshot(): Promise<Uint8Array> { return Promise.resolve(new Uint8Array()); }
  control(selector: string): FormControl { return this.fields.find((item) => item.selector === selector)!; }

  locator(selector: string): BrowserLocator {
    const control = this.fields.find((item) => item.selector === selector);
    const submit = this.finalStep && (selector.includes("submit") || selector.includes("Submit"));
    const next = !this.finalStep && !/submit/i.test(selector) && /next|continue/i.test(selector);
    return {
      count: async () => control || submit || next ? 1 : 0,
      fill: async (value) => { if (control) control.value = value; },
      click: async () => {
        if (next) {
          this.nextClicks += 1;
          this.finalStep = true;
        }
        if (submit) {
          this.submitClicks += 1;
          this.submitted = true;
        }
      },
      textContent: async () => "",
      getAttribute: async () => null,
      isVisible: async () => Boolean(control || submit || next),
      selectOption: async (value) => { if (control) control.value = value; },
      setChecked: async (checked) => { if (control) control.checked = checked; },
      setInputFiles: async (paths) => { if (control) control.value = paths[0]?.split("/").at(-1) || ""; },
    };
  }
}
