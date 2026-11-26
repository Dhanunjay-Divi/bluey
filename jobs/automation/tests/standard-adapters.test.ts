import { describe, expect, it } from "vitest";
import {
  createDefaultAdapterRegistry,
  executeApplication,
  type AdapterContext,
  type ApplicationAdapter,
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
const RESUME_SHA256 = "a".repeat(64);

describe("deterministic ATS application adapters", () => {
  for (const [provider, url] of PROVIDERS.slice(2)) {
    it(`fills a ${provider} fixture without activating final submit`, async () => {
      const page = new FixturePage(url, [
        field("first_name", "First name", true),
        field("email", "Email address", true, "email"),
        field("resume", "Resume", true, "file"),
      ]);
      const events: string[] = [];
      const submitHooks: string[] = [];
      const result = await executeApplication(
        context(page, events, submitHooks),
      );
      expect(result.adapter).toBe(provider);
      expect(result.receipt.status).toBe("needs_input");
      expect(result.receipt.intervention?.kind).toBe("browser_takeover");
      expect(page.control("first_name").value).toBe("Ada");
      expect(page.control("email").value).toBe("ada@example.com");
      expect(page.control("resume").value).toBe(`resume-${RESUME_SHA256}.pdf`);
      expect(page.submitClicks).toBe(0);
      expect(events).toContain("application_execution_finished");
      expect(submitHooks).toEqual([]);
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
    expect(result.receipt.status).toBe("needs_input");
    expect(result.receipt.intervention?.kind).toBe("browser_takeover");
    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual([]);
  });

  it("pauses on an unanswered required question", async () => {
    const page = new FixturePage(PROVIDERS[0][1], [
      field("legal", "Are you bound by a non-compete?", true),
    ]);
    const result = await executeApplication(context(page, []));
    expect(result.receipt.status).toBe("needs_input");
    expect(result.receipt.intervention?.kind).toBe("unknown_question");
  });

  it("selects the matching option in a required radio group", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [
      {
        ...field("authorized_yes", "Authorized to work Yes", true, "radio"),
        name: "authorized",
        value: "yes",
      },
      {
        ...field("authorized_no", "Authorized to work No", true, "radio"),
        name: "authorized",
        value: "no",
      },
    ]);
    const result = await executeApplication({
      ...context(page, []),
      packet: {
        ...context(page, []).packet,
        answers: { work_authorization: "yes" },
      },
    });

    expect(result.receipt.status).toBe("needs_input");
    expect(result.receipt.intervention?.kind).toBe("browser_takeover");
    expect(page.control("authorized_yes").checked).toBe(true);
    expect(page.control("authorized_no").checked).not.toBe(true);
    expect(page.submitClicks).toBe(0);
  });

  it("does not accept confirmation-like page text as proof of submission", async () => {
    const page = new FixturePage(
      PROVIDERS[4][1],
      [],
      "Thank you for applying. Application submitted.",
    );

    const result = await executeApplication(context(page, []));

    expect(result.receipt.status).toBe("needs_input");
    expect(result.receipt.intervention?.kind).toBe("browser_takeover");
    expect(page.submitClicks).toBe(0);
  });

  it("does not claim success without confirmation evidence", async () => {
    const page = new FixturePage(
      PROVIDERS[4][1],
      [field("first_name", "First name", true)],
      "Application form",
      false,
    );
    const result = await executeApplication(context(page, []));

    expect(result.receipt.status).toBe("needs_input");
    expect(result.receipt.intervention?.kind).toBe("browser_takeover");
  });

  it("pauses before a CAPTCHA", async () => {
    const page = new FixturePage(
      PROVIDERS[1][1],
      [],
      "Verify you are human. CAPTCHA",
    );
    const result = await executeApplication(context(page, []));
    expect(result.receipt.status).toBe("needs_input");
    expect(result.receipt.intervention?.kind).toBe("captcha");
  });

  it("does not fence validation or challenge-only paths", async () => {
    const validationHooks: string[] = [];
    const validationPage = new FixturePage(PROVIDERS[4][1], [
      field("non_compete", "Are you bound by a non-compete?", true),
    ]);
    const validation = await executeApplication(
      context(validationPage, [], validationHooks),
    );
    expect(validation.receipt.status).toBe("needs_input");
    expect(validationHooks).toEqual([]);

    const challengeHooks: string[] = [];
    const challengePage = new FixturePage(
      PROVIDERS[4][1],
      [],
      "Verify you are human. CAPTCHA",
    );
    const challenge = await executeApplication(
      context(challengePage, [], challengeHooks),
    );
    expect(challenge.receipt.intervention?.kind).toBe("captcha");
    expect(challengeHooks).toEqual([]);
  });

  it("advances safe Next steps but stops before generic final submit", async () => {
    const page = new FixturePage(
      PROVIDERS[4][1],
      [field("first_name", "First name", true)],
      "Application form",
      true,
      true,
    );
    const submitHooks: string[] = [];

    const result = await executeApplication(context(page, [], submitHooks));

    expect(result.receipt.status).toBe("needs_input");
    expect(result.receipt.intervention?.kind).toBe("browser_takeover");
    expect(page.nextClicks).toBe(1);
    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual([]);
  });

  it("does not invoke the durable final-submit hook for review-only adapters", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [
      field("first_name", "First name", true),
    ]);
    const adapterContext = context(page, [], []);
    let hookCalls = 0;
    adapterContext.beforeFinalSubmit = async () => {
      hookCalls += 1;
      throw new Error("durable fence response unavailable");
    };

    const result = await executeApplication(adapterContext);

    expect(result.receipt.status).toBe("needs_input");
    expect(result.receipt.intervention?.kind).toBe("browser_takeover");
    expect(hookCalls).toBe(0);
    expect(page.submitClicks).toBe(0);
  });

  it("blocks submission when a reactive form silently discards a filled value", async () => {
    const page = new FixturePage(
      PROVIDERS[4][1],
      [field("first_name", "First name", true)],
      "Application form",
      true,
      false,
      ["first_name"],
    );
    const submitHooks: string[] = [];

    const result = await executeApplication(context(page, [], submitHooks));

    expect(result.receipt.status).toBe("needs_input");
    expect(result.receipt.issues[0]?.message).toContain("did not register");
    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual([]);
  });

  it("downgrades an unauthorized adapter that claims a submitted result", async () => {
    const page = new FixturePage("https://careers.acme.com/jobs/123/apply", []);
    const events: string[] = [];
    const submitHooks: string[] = [];
    let finalSubmitHookExposed = false;
    const unsafeAdapter: ApplicationAdapter = {
      kind: "semantic",
      version: "unsafe-test-adapter",
      detect: () => true,
      normalize: async (currentPage) => ({
        externalId: "job-123",
        canonicalUrl: currentPage.url(),
        company: "Acme",
        title: "Software Engineer",
        location: "",
        workplace: "unknown",
        description: "Test role",
        source: "semantic",
      }),
      prepare: async () => undefined,
      fill: async () => undefined,
      validate: async () => [],
      submit: async (adapterContext) => {
        finalSubmitHookExposed = adapterContext.beforeFinalSubmit !== undefined;
        return {
          status: "submitted",
          confirmationText: "Synthetic confirmation",
          submittedAt: new Date().toISOString(),
          issues: [],
        };
      },
    };

    const result = await executeApplication(
      context(page, events, submitHooks),
      createDefaultAdapterRegistry(unsafeAdapter),
    );

    expect(result.receipt.status).toBe("needs_input");
    expect(result.receipt.intervention?.kind).toBe("browser_takeover");
    expect(events).toContain("adapter_submission_authority_violation");
    expect(finalSubmitHookExposed).toBe(false);
    expect(submitHooks).toEqual([]);
  });

  it.each([
    ["Resume", "resume_file", "resume"],
    ["CV", "cv", "resume"],
    ["Curriculum Vitae", "cv_upload", "resume"],
    ["Cover Letter", "cover_letter", "cover-letter"],
  ])("uploads the approved document for %s", async (label, name, expected) => {
    const item = { ...field("document", label, true, "file"), name };
    const page = new FixturePage(PROVIDERS[4][1], [item]);
    const base = context(page, []);
    const result = await executeApplication({
      ...base,
      packet: { ...base.packet, coverLetterPath: `/tmp/cover-letter-${RESUME_SHA256}.pdf` },
    });
    expect(page.control("document").value).toBe(`${expected}-${RESUME_SHA256}.pdf`);
    expect(page.writes.get("document")).toBeGreaterThan(0);
    expect(result.receipt.intervention?.kind).toBe("browser_takeover");
    expect(page.submitClicks).toBe(0);
  });

  it.each([
    ["Photo", "photo"],
    ["Supporting document", "supporting_document"],
    ["Resume and cover letter", "resume_cover_letter"],
    ["Resume transcript", "resume_transcript"],
    ["Resume or supporting document", "resume_supporting_document"],
  ])("blocks unknown or conflicting required file %s even if prefilled", async (label, name) => {
    const item = { ...field("document", label, true, "file"), name, value: "existing.pdf" };
    const page = new FixturePage(PROVIDERS[4][1], [item], "Application form", true, true);
    const hooks: string[] = [];
    const result = await executeApplication(context(page, [], hooks));
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.control("document").value).toBe("existing.pdf");
    expect(page.writes.get("document") ?? 0).toBe(0);
    expect(page.nextClicks).toBe(0);
    expect(page.submitClicks).toBe(0);
    expect(hooks).toEqual([]);
  });

  it("leaves an optional unknown file untouched", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [field("photo", "Photo", false, "file")]);
    await executeApplication(context(page, []));
    expect(page.writes.get("photo") ?? 0).toBe(0);
    expect(page.submitClicks).toBe(0);
  });

  it.each([
    ["", "", ""],
    ["!!!", "---", "..."],
  ])("blocks an unidentifiable required field even if prefilled", async (label, name, placeholder) => {
    const page = new FixturePage(PROVIDERS[4][1], [{
      ...field("unknown", label, true), name, placeholder, value: "existing",
    }], "Application form", true, true);
    const base = context(page, []);
    const hooks: string[] = [];
    const result = await executeApplication({
      ...base,
      packet: { ...base.packet, answers: { "!!!": "wrong", first_name: "Ada" } },
      beforeFinalSubmit: async () => { hooks.push("before"); },
    });
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.control("unknown").value).toBe("existing");
    expect(page.writes.get("unknown") ?? 0).toBe(0);
    expect(page.nextClicks).toBe(0);
    expect(page.submitClicks).toBe(0);
    expect(hooks).toEqual([]);
  });

  it("rejects different approved answers at the same normalized matching tier", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [field("answer", "First name", true)]);
    const base = context(page, []);
    const result = await executeApplication({
      ...base,
      packet: { ...base.packet, answers: { "First name": "Ada", "first-name": "Grace" } },
    });
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.writes.get("answer") ?? 0).toBe(0);
    expect(page.submitClicks).toBe(0);
  });

  it.each([
    ["Phone", { mobile: "111", "phone number": "222" }],
  ])("blocks conflicting alias candidates for %s", async (label, answers) => {
    const page = new FixturePage(PROVIDERS[4][1], [field("answer", label, true)]);
    const base = context(page, []);
    const result = await executeApplication({ ...base, packet: { ...base.packet, answers } });
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.writes.get("answer") ?? 0).toBe(0);
    expect(page.submitClicks).toBe(0);
  });

  it("does not map a unique fuzzy containment answer", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [{
      ...field("answer", "Candidate favorite city", true), value: "existing",
    }], "Application form", true, true);
    const base = context(page, []);
    const result = await executeApplication({
      ...base, packet: { ...base.packet, answers: { "favorite city": "Austin" } },
    });
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.control("answer").value).toBe("existing");
    expect(page.writes.get("answer") ?? 0).toBe(0);
    expect(page.nextClicks).toBe(0);
    expect(page.submitClicks).toBe(0);
  });

  it.each([
    ["Manager email", "email"],
    ["Reference email", "email"],
    ["Previous salary", "salary_expectation"],
  ])("does not map applicant facts to %s", async (label, name) => {
    const page = new FixturePage(PROVIDERS[4][1], [{
      ...field("answer", label, true), name, value: "existing",
    }], "Application form", true, true);
    const base = context(page, []);
    const result = await executeApplication({
      ...base,
      packet: { ...base.packet, answers: { email: "ada@example.com", salary_expectation: "100000" } },
    });
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.control("answer").value).toBe("existing");
    expect(page.writes.get("answer") ?? 0).toBe(0);
    expect(page.nextClicks).toBe(0);
    expect(page.submitClicks).toBe(0);
  });

  it.each(["number", "date"])("blocks a prefilled unsupported required %s control", async (name) => {
    const page = new FixturePage(PROVIDERS[4][1], [{
      ...field("unsupported", name, true, "other"), value: "existing",
    }], "Application form", true, true);
    const result = await executeApplication(context(page, []));
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.writes.get("unsupported") ?? 0).toBe(0);
    expect(page.nextClicks).toBe(0);
    expect(page.submitClicks).toBe(0);
  });

  it("ignores a blank normalized answer key and fills a unique approved answer", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [field("first_name", "First name", true)]);
    const base = context(page, []);
    await executeApplication({
      ...base, packet: { ...base.packet, answers: { "!!!": "wrong", first_name: "Ada" } },
    });
    expect(page.control("first_name").value).toBe("Ada");
    expect(page.writes.get("first_name")).toBeGreaterThan(0);
    expect(page.submitClicks).toBe(0);
  });

  it.each([["yes", true], ["y", true], ["true", true], ["1", true], ["on", true],
    ["no", false], ["n", false], ["false", false], ["0", false], ["off", false]])(
    "maps explicit checkbox answer %s", async (answer, expected) => {
      const page = new FixturePage(PROVIDERS[4][1], [field("choice", "Choice", false, "checkbox")]);
      const base = context(page, []);
      await executeApplication({ ...base, packet: { ...base.packet, answers: { choice: answer } } });
      expect(page.control("choice").checked).toBe(expected);
      expect(page.writes.get("choice")).toBeGreaterThan(0);
      expect(page.submitClicks).toBe(0);
    },
  );

  it("blocks an explicitly false required consent checkbox", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [field("consent", "Consent", true, "checkbox")]);
    const base = context(page, []);
    const result = await executeApplication({
      ...base, packet: { ...base.packet, answers: { consent: "no" } },
    });
    expect(page.control("consent").checked).toBe(false);
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.submitClicks).toBe(0);
  });

  it("blocks an ambiguous mapped optional checkbox without changing it", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [{
      ...field("choice", "Choice", false, "checkbox"), checked: true,
    }], "Application form", true, true);
    const base = context(page, []);
    const result = await executeApplication({
      ...base, packet: { ...base.packet, answers: { choice: "maybe" } },
    });
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.writes.get("choice") ?? 0).toBe(0);
    expect(page.control("choice").checked).toBe(true);
    expect(page.nextClicks).toBe(0);
    expect(page.submitClicks).toBe(0);
  });

  it.each(["maybe", "decline", "", "!!!"])(
    "blocks an ambiguous checkbox answer %s without changing a prechecked control", async (answer) => {
      const page = new FixturePage(PROVIDERS[4][1], [{
        ...field("choice", "Choice", true, "checkbox"), checked: true,
      }], "Application form", true, true);
      const base = context(page, []);
      const hooks: string[] = [];
      const result = await executeApplication({
        ...base, packet: { ...base.packet, answers: { choice: answer } },
        beforeFinalSubmit: async () => { hooks.push("before"); },
      });
      expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
      expect(page.control("choice").checked).toBe(true);
      expect(page.writes.get("choice") ?? 0).toBe(0);
      expect(page.nextClicks).toBe(0);
      expect(page.submitClicks).toBe(0);
      expect(hooks).toEqual([]);
    },
  );

  it.each([
    ["ny", [{ label: "New York", value: "ny" }, { label: "Texas", value: "tx" }], "ny"],
    ["New York", [{ label: "New York", value: "ny" }, { label: "Texas", value: "tx" }], "ny"],
  ])("selects a uniquely exact option %s", async (answer, options, expected) => {
    const page = new FixturePage(PROVIDERS[4][1], [{
      ...field("choice", "Choice", true, "select"), options,
    }]);
    const base = context(page, []);
    await executeApplication({ ...base, packet: { ...base.packet, answers: { choice: answer } } });
    expect(page.control("choice").value).toBe(expected);
    expect(page.writes.get("choice")).toBeGreaterThan(0);
    expect(page.submitClicks).toBe(0);
  });

  it.each([
    ["New York", [{ label: "New York City", value: "city" }, { label: "New York State", value: "state" }]],
    ["Yes", [{ label: "Yes", value: "a" }, { label: "YES", value: "b" }]],
    ["Yes", [{ label: "Affirmative", value: "yes" }, { label: "Yes", value: "other" }]],
    ["!!!", [{ label: "---", value: "x" }]],
    ["", [{ label: "Yes", value: "yes" }]],
  ])("blocks a non-unique or unmatched select %s even if preselected", async (answer, options) => {
    const page = new FixturePage(PROVIDERS[4][1], [{
      ...field("choice", "Choice", true, "select"), options, value: "existing",
    }], "Application form", true, true);
    const base = context(page, []);
    const hooks: string[] = [];
    const result = await executeApplication({
      ...base, packet: { ...base.packet, answers: { choice: answer } },
      beforeFinalSubmit: async () => { hooks.push("before"); },
    });
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.control("choice").value).toBe("existing");
    expect(page.writes.get("choice") ?? 0).toBe(0);
    expect(page.nextClicks).toBe(0);
    expect(page.submitClicks).toBe(0);
    expect(hooks).toEqual([]);
  });

  it("blocks a mapped ambiguous optional select without changing its selection", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [{
      ...field("choice", "Choice", false, "select"), value: "existing",
      options: [{ label: "Yes", value: "a" }, { label: "YES", value: "b" }],
    }], "Application form", true, true);
    const base = context(page, []);
    const result = await executeApplication({
      ...base, packet: { ...base.packet, answers: { choice: "yes" } },
    });
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.control("choice").value).toBe("existing");
    expect(page.writes.get("choice") ?? 0).toBe(0);
    expect(page.nextClicks).toBe(0);
    expect(page.submitClicks).toBe(0);
  });

  it("selects the unique exact radio rather than a suffix match", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [
      { ...field("yes", "Choice Yes", true, "radio"), name: "choice", value: "yes", checked: true },
      { ...field("also", "Choice Definitely yes", true, "radio"), name: "choice", value: "definitely yes" },
    ], "Application form", true, true);
    const base = context(page, []);
    const hooks: string[] = [];
    const result = await executeApplication({
      ...base, packet: { ...base.packet, answers: { choice: "yes" } },
      beforeFinalSubmit: async () => { hooks.push("before"); },
    });
    expect(result.receipt.intervention?.kind).toBe("browser_takeover");
    expect(page.writes.get("yes")).toBeGreaterThan(0);
    expect(page.writes.get("also") ?? 0).toBe(0);
    expect(page.control("yes").checked).toBe(true);
    expect(page.nextClicks).toBe(1);
    expect(page.submitClicks).toBe(0);
    expect(hooks).toEqual([]);
  });

  it("does not select not authorized for the approved answer authorized", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [
      { ...field("not", "Authorization Not authorized", true, "radio"), name: "authorization", value: "not authorized", checked: true },
      { ...field("other", "Authorization Other", true, "radio"), name: "authorization", value: "other" },
    ], "Application form", true, true);
    const base = context(page, []);
    const hooks: string[] = [];
    const result = await executeApplication({
      ...base, packet: { ...base.packet, answers: { authorization: "authorized" } },
      beforeFinalSubmit: async () => { hooks.push("before"); },
    });
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.writes.get("not") ?? 0).toBe(0);
    expect(page.writes.get("other") ?? 0).toBe(0);
    expect(page.nextClicks).toBe(0);
    expect(page.submitClicks).toBe(0);
    expect(hooks).toEqual([]);
  });

  it("blocks duplicate exact radio targets", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [
      { ...field("first", "Choice Yes", true, "radio"), name: "choice", value: "yes", checked: true },
      { ...field("second", "Choice Yes", true, "radio"), name: "choice", value: "yes" },
    ], "Application form", true, true);
    const base = context(page, []);
    const result = await executeApplication({
      ...base, packet: { ...base.packet, answers: { choice: "yes" } },
    });
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.writes.get("first") ?? 0).toBe(0);
    expect(page.writes.get("second") ?? 0).toBe(0);
    expect(page.nextClicks).toBe(0);
    expect(page.submitClicks).toBe(0);
  });

  it("blocks a radio group whose members resolve to different approved answers", async () => {
    const page = new FixturePage(PROVIDERS[4][1], [
      { ...field("yes", "Work authorization Yes", true, "radio"), name: "choice", value: "yes", checked: true },
      { ...field("no", "Sponsorship No", true, "radio"), name: "choice", value: "no" },
    ], "Application form", true, true);
    const base = context(page, []);
    const result = await executeApplication({
      ...base, packet: { ...base.packet, answers: { work_authorization: "yes", sponsorship_required: "no" } },
    });
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.writes.get("yes") ?? 0).toBe(0);
    expect(page.writes.get("no") ?? 0).toBe(0);
    expect(page.nextClicks).toBe(0);
    expect(page.submitClicks).toBe(0);
  });

  it.each([
    ["unnamed", "", "yes"],
    ["unmatched", "choice", "maybe"],
  ])("blocks a required %s radio even if preselected", async (_case, name, answer) => {
    const page = new FixturePage(PROVIDERS[4][1], [{
      ...field("yes", "Choice Yes", true, "radio"), name, value: "yes", checked: true,
    }], "Application form", true, true);
    const base = context(page, []);
    const result = await executeApplication({
      ...base, packet: { ...base.packet, answers: { choice: answer } },
    });
    expect(result.receipt.issues.some((issue) => issue.severity === "blocking")).toBe(true);
    expect(page.control("yes").checked).toBe(true);
    expect(page.writes.get("yes") ?? 0).toBe(0);
    expect(page.nextClicks).toBe(0);
    expect(page.submitClicks).toBe(0);
  });
});

function context(
  page: BrowserPage,
  events: string[],
  submitHooks?: string[],
): AdapterContext {
  return {
    runner: "local",
    runId: "run-123",
    accountId: "account-123",
    approvedCanonicalUrl: page.url(),
    page,
    packet: {
      applicationId: "application-123",
      jobId: "job-123",
      resumeVersionId: "resume-version-123",
      approvedPacketChecksum: "c".repeat(64),
      resumePath: `/tmp/resume-${RESUME_SHA256}.pdf`,
      answers: { first_name: "Ada", email: "ada@example.com" },
      verifiedClaimIds: ["claim-1"],
      applicationIdentityId: "identity-123",
      applicationEmail: "ada@example.com",
      browserProfileId: "profile-123",
    },
    async log(event) {
      events.push(event);
    },
    ...(submitHooks
      ? {
          async beforeFinalSubmit() {
            submitHooks.push("before");
          },
          async afterFinalSubmit(outcome: string) {
            submitHooks.push(`after:${outcome}`);
          },
        }
      : {}),
  };
}

function field(
  selector: string,
  label: string,
  required: boolean,
  kind: FormControl["kind"] = "text",
): FormControl {
  return {
    selector,
    label,
    required,
    kind,
    name: selector,
    placeholder: "",
    value: "",
  };
}

class FixturePage implements BrowserPage {
  private submitted = false;
  private finalStep: boolean;
  submitClicks = 0;
  nextClicks = 0;
  readonly writes = new Map<string, number>();

  constructor(
    private readonly currentUrl: string,
    private readonly fields: FormControl[],
    private body = "Application form",
    private readonly confirmsSubmission = true,
    startBeforeFinalStep = false,
    ignoredWrites: string[] = [],
  ) {
    this.finalStep = !startBeforeFinalStep;
    this.ignoredWrites = new Set(ignoredWrites);
  }

  private readonly ignoredWrites: Set<string>;

  url(): string {
    return this.currentUrl;
  }
  installExactSubmitGuard(): Promise<void> {
    return Promise.resolve();
  }
  beginExactSubmitGuard(): Promise<void> {
    return Promise.resolve();
  }
  assertExactSubmitGuardClean(): Promise<void> {
    return Promise.resolve();
  }
  async title(): Promise<string> {
    return "Software Engineer | Acme";
  }
  controls(): Promise<FormControl[]> {
    return Promise.resolve(
      this.submitted ? [] : this.fields.map((item) => ({ ...item })),
    );
  }
  bodyText(): Promise<string> {
    return Promise.resolve(
      this.submitted && this.confirmsSubmission
        ? "Thank you for applying. Application submitted."
        : this.body,
    );
  }
  waitForSettled(): Promise<void> {
    return Promise.resolve();
  }
  screenshot(): Promise<Uint8Array> {
    return Promise.resolve(new Uint8Array());
  }
  control(selector: string): FormControl {
    return this.fields.find((item) => item.selector === selector)!;
  }

  locator(selector: string): BrowserLocator {
    const control = this.fields.find((item) => item.selector === selector);
    const submit =
      this.finalStep &&
      (selector.includes("submit") || selector.includes("Submit"));
    const next =
      !this.finalStep &&
      !/submit/i.test(selector) &&
      /next|continue/i.test(selector);
    return {
      count: async () => (control || submit || next ? 1 : 0),
      fill: async (value) => {
        if (control) this.writes.set(selector, (this.writes.get(selector) ?? 0) + 1);
        if (control && !this.ignoredWrites.has(selector)) control.value = value;
      },
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
      selectOption: async (value) => {
        if (control) this.writes.set(selector, (this.writes.get(selector) ?? 0) + 1);
        if (control && !this.ignoredWrites.has(selector)) control.value = value;
      },
      setChecked: async (checked) => {
        if (control) this.writes.set(selector, (this.writes.get(selector) ?? 0) + 1);
        if (control && !this.ignoredWrites.has(selector))
          control.checked = checked;
      },
      setInputFiles: async (paths) => {
        if (control) this.writes.set(selector, (this.writes.get(selector) ?? 0) + 1);
        if (control && !this.ignoredWrites.has(selector)) {
          const files = paths.map(fileEvidenceForSnapshotPath);
          control.value = files.map((file) => file.name).join(", ");
          control.files = files;
          return files;
        }
        return [];
      },
      effectiveSubmitTarget: async () => {
        throw new Error("Review-only fixture has no certified submit target");
      },
      successfulSubmitEvidence: async () => ({ fields: [], partOrder: [] }),
      clickWithExactSubmit: async () => {
        throw new Error("Review-only fixture cannot activate final submit");
      },
    };
  }
}

function fileEvidenceForSnapshotPath(path: string) {
  const name = path.split(/[\\/]/u).at(-1) ?? "";
  const sha256 = /-([a-f0-9]{64})\.pdf$/u.exec(name)?.[1] ?? "";
  return { name, byteLength: 1_024, sha256 };
}
