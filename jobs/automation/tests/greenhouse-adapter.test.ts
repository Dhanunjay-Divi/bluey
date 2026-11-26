import { createHash } from "node:crypto";
import { describe, expect, it } from "vitest";
import publicModernJson from "./fixtures/greenhouse/public-modern.json";
import embeddedLegacyJson from "./fixtures/greenhouse/embedded-legacy.json";
import type {
  AdapterContext,
  BrowserLocator,
  BrowserPage,
  ExactSubmitExpectation,
  FormControl,
  ProviderFinalSubmitProof,
} from "../src/contracts.js";
import {
  GREENHOUSE_ADAPTER_PROFILE,
  GreenhouseAdapter,
  GreenhouseApplicationStateMachine,
  detectGreenhouseUrl,
  type GreenhouseVariant,
} from "../src/providers/greenhouse.js";
import { certifiedProviderJobKey } from "../src/provider-job-key.js";
import { ExactSubmitEvidenceError } from "../src/trusted-submit.js";

interface FixtureDefinition {
  tenant: string;
  url: string;
  title: string;
  body: string;
  confirmationBody: string;
  markers: string[];
  controls: FormControl[];
}

const PUBLIC_MODERN = publicModernJson as FixtureDefinition;
const EMBEDDED_LEGACY = embeddedLegacyJson as FixtureDefinition;
const CERTIFIED_EMBEDDED_LEGACY: FixtureDefinition = {
  ...EMBEDDED_LEGACY,
  url: "https://boards.greenhouse.io/embed/job_app?for=northstar&token=67890",
};
const RESUME_SHA = "a".repeat(64);
const RESUME_NAME = `resume-${RESUME_SHA}.pdf`;
const RESUME_PATH = `/snapshots/${RESUME_NAME}`;

describe("Greenhouse provider state machine", () => {
  it("is explicitly beta, Review-only, and uncertified", () => {
    expect(GREENHOUSE_ADAPTER_PROFILE).toMatchObject({
      kind: "greenhouse",
      maturity: "beta",
      capability: "beta_review",
      submissionMode: "review_only",
      certified: false,
      requiresFinalReview: true,
    });
    expect(new GreenhouseAdapter().profile).toBe(GREENHOUSE_ADAPTER_PROFILE);
  });

  it.each([
    ["Acme modern public", PUBLIC_MODERN, "public"],
    ["Northstar legacy embedded", CERTIFIED_EMBEDDED_LEGACY, "embedded"],
  ] as const)("detects and completes the %s form variant after review", async (_name, definition, variant) => {
    const confirmationUrl = variant === "public"
      ? `${definition.url}/confirmation`
      : "https://boards.greenhouse.io/northstar/jobs/67890/confirmation";
    const page = new GreenhouseFixturePage(definition, {
      removeControlsAfterSubmit: true,
      postSubmitUrl: confirmationUrl,
    });
    const submitHooks: string[] = [];
    const proofs: ProviderFinalSubmitProof[] = [];
    const machine = new GreenhouseApplicationStateMachine(context(page, {}, {
      events: submitHooks,
      proofs,
    }), {
      now: () => new Date("2026-07-11T12:00:00.000Z"),
    });

    await expect(machine.detect()).resolves.toMatchObject({ variant: variant as GreenhouseVariant });
    expect(machine.state.name).toBe("prepare");
    await machine.prepare();
    expect(machine.state.name).toBe("fill");
    await machine.fill();
    expect(machine.state.name).toBe("validate");
    await expect(machine.validate()).resolves.toEqual([]);
    expect(machine.state.name).toBe("submit");

    machine.approveFinalReview();
    const receipt = await machine.submit();

    expect(receipt).toMatchObject({
      status: "submitted",
      confirmationUrl,
      submitHttpStatus: 200,
      submittedAt: "2026-07-11T12:00:00.000Z",
      issues: [],
    });
    expect(receipt.confirmationText).toBe(definition.confirmationBody);
    expect(page.submitClicks).toBe(1);
    expect(submitHooks).toEqual(["before", "after:activated"]);
    expect(proofs).toEqual([expect.objectContaining({
      adapter: "greenhouse",
      adapterVersion: "2026.07.1-beta.1",
      control: "greenhouse_submit_application",
      target: expect.objectContaining({
        actionUrl: definition.url,
        method: "post",
        enctype: "multipart/form-data",
        formTarget: "_self",
      }),
    })]);
    expect(machine.history).toEqual(["detect", "prepare", "fill", "validate", "submit", "receipt"]);
    expect(page.controlByLabel("First name").value).toBe("Ada");
    expect(page.controlByLabel("Last name").value).toBe("Lovelace");
    expect(page.controlByLabel("Email").value).toBe("ada@example.com");
    expect(page.controlByLabel("Resume").value).toBe(RESUME_NAME);
    if (variant === "embedded") expect(page.controlByLabel("How did you hear").value).toBe("referral");
  });

  it("requires final review by default and does not click Submit", async () => {
    const page = new GreenhouseFixturePage(PUBLIC_MODERN);
    const submitHooks: string[] = [];
    const machine = await readyMachine(page, context(page, {}, { events: submitHooks }));

    const receipt = await machine.submit();

    expect(receipt.status).toBe("needs_input");
    expect(receipt.intervention).toMatchObject({
      kind: "browser_takeover",
      title: "Review the Greenhouse application",
    });
    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual([]);
  });

  it.each(["after detection", "immediately before fill"] as const)(
    "rejects a redirect to another Greenhouse job %s with zero writes",
    async (stage) => {
      const page = new GreenhouseFixturePage(PUBLIC_MODERN);
      const machine = new GreenhouseApplicationStateMachine(context(page));
      await machine.detect();
      if (stage === "immediately before fill") await machine.prepare();
      page.navigateTo("https://job-boards.greenhouse.io/acme/jobs/99999");

      const operation = stage === "after detection" ? machine.prepare() : machine.fill();
      await expect(operation).rejects.toThrow("does not match the approved provider job");

      expect(page.fieldWrites).toBe(0);
      expect(page.fileSelections).toBe(0);
      expect(page.submitClicks).toBe(0);
    },
  );

  it("rejects an initially wrong job with a visible Apply control before any click", async () => {
    const page = new GreenhouseFixturePage({
      ...PUBLIC_MODERN,
      url: "https://job-boards.greenhouse.io/acme/jobs/99999",
      controls: [],
      markers: ["#apply_button"],
    });
    const machine = new GreenhouseApplicationStateMachine(context(page, {}, {
      events: [],
      approvedCanonicalUrl: PUBLIC_MODERN.url,
    }));
    await machine.detect();

    await expect(machine.prepare()).rejects.toThrow("does not match the approved provider job");

    expect(page.applyClicks).toBe(0);
    expect(page.fieldWrites).toBe(0);
    expect(page.fileSelections).toBe(0);
    expect(page.submitClicks).toBe(0);
  });

  it("stops on an unknown required Greenhouse question", async () => {
    const definition = withControl(PUBLIC_MODERN, field(
      "non-compete",
      "Are you currently bound by a non-compete agreement?",
      true,
    ));
    const page = new GreenhouseFixturePage(definition);
    const submitHooks: string[] = [];
    const machine = new GreenhouseApplicationStateMachine(context(page, {}, { events: submitHooks }));
    await machine.detect();
    await machine.prepare();
    await machine.fill();

    const issues = await machine.validate();

    expect(issues).toEqual([
      expect.objectContaining({
        field: "Are you currently bound by a non-compete agreement?",
        severity: "blocking",
      }),
    ]);
    expect(machine.getReceipt()).toMatchObject({
      status: "needs_input",
      intervention: { kind: "unknown_question" },
    });
    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual([]);
  });

  it("reports a missing packet document as a blocking validation issue", async () => {
    const page = new GreenhouseFixturePage(PUBLIC_MODERN);
    const machine = new GreenhouseApplicationStateMachine(context(page, { resumePath: undefined }));
    await machine.detect();
    await machine.prepare();
    await machine.fill();

    const issues = await machine.validate();

    expect(issues[0]).toMatchObject({ field: "Resume/CV", severity: "blocking" });
    expect(machine.getReceipt()?.intervention?.kind).toBe("missing_fact");
  });

  it("does not force a packet value into an unmatched required option", async () => {
    const page = new GreenhouseFixturePage(CERTIFIED_EMBEDDED_LEGACY);
    const machine = new GreenhouseApplicationStateMachine(context(page, {
      answers: { "How did you hear about us?": "Campus event" },
    }));
    await machine.detect();
    await machine.prepare();
    await machine.fill();

    const issues = await machine.validate();

    expect(issues[0]?.field).toBe("How did you hear about us?");
    expect(issues[0]?.message).toContain("did not accept");
    expect(page.controlByLabel("How did you hear").value).toBe("");
  });

  it("blocks submission when Greenhouse discards a prepared field value", async () => {
    const page = new GreenhouseFixturePage(
      PUBLIC_MODERN,
      { ignoredWrites: ["[data-bluey-field-id='first-name']"] },
    );
    const submitHooks: string[] = [];
    const machine = new GreenhouseApplicationStateMachine(
      context(page, {}, { events: submitHooks }),
    );
    await machine.detect();
    await machine.prepare();
    await machine.fill();

    const issues = await machine.validate();

    expect(issues).toEqual([
      expect.objectContaining({
        field: "First name",
        message: expect.stringContaining("did not register"),
        severity: "blocking",
      }),
    ]);
    expect(machine.getReceipt()?.intervention?.kind).toBe("missing_fact");
    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual([]);
  });

  it("reconstructs reviewed field and file expectations in a fresh adapter", async () => {
    const page = new GreenhouseFixturePage(PUBLIC_MODERN);
    const firstMachine = await readyMachine(page);
    expect(firstMachine.state.name).toBe("submit");

    page.controlByLabel("First name").value = "Grace";
    page.replaceSelectedFileBytes("Resume");
    const submitHooks: string[] = [];
    const resumedMachine = new GreenhouseApplicationStateMachine(
      context(page, {}, { events: submitHooks }),
    );
    await resumedMachine.detect();
    await resumedMachine.prepare();
    await resumedMachine.fill();

    const issues = await resumedMachine.validate();

    expect(issues).toEqual(expect.arrayContaining([
      expect.objectContaining({ field: "First name", severity: "blocking" }),
      expect.objectContaining({ field: "Resume/CV", severity: "blocking" }),
    ]));
    expect(page.controlByLabel("First name").value).toBe("Grace");
    expect(page.controlByLabel("Resume").value).toBe(RESUME_NAME);
    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual([]);
  });

  it("rejects an unexpected optional attachment before final authority", async () => {
    const definition = withControl(PUBLIC_MODERN, {
      ...field("[data-bluey-field-id='portfolio']", "Portfolio attachment", false, "file"),
      value: "unreviewed-portfolio.zip",
    });
    const page = new GreenhouseFixturePage(definition);
    const submitHooks: string[] = [];
    const machine = new GreenhouseApplicationStateMachine(
      context(page, {}, { events: submitHooks }),
    );
    await machine.detect();
    await machine.prepare();
    await machine.fill();

    const issues = await machine.validate();

    expect(issues).toEqual(expect.arrayContaining([
      expect.objectContaining({
        field: "Portfolio attachment",
        message: expect.stringContaining("not part of the approved packet"),
        severity: "blocking",
      }),
    ]));
    expect(page.controlByLabel("Portfolio attachment").value).toBe("unreviewed-portfolio.zip");
    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual([]);
  });

  it.each([
    ["text", (page: GreenhouseFixturePage) => {
      page.controlByLabel("First name").value = "Grace";
    }],
    ["textarea", (page: GreenhouseFixturePage) => {
      page.controlByLabel("Why Bluey").value = "Changed after review";
    }],
    ["select", (page: GreenhouseFixturePage) => {
      page.controlByLabel("Experience level").value = "entry";
    }],
    ["checkbox", (page: GreenhouseFixturePage) => {
      page.controlByLabel("Confirm accuracy").checked = false;
    }],
    ["radio", (page: GreenhouseFixturePage) => {
      page.controlByLabel("Work mode Remote").checked = false;
      page.controlByLabel("Work mode Office").checked = true;
    }],
    ["file", (page: GreenhouseFixturePage) => {
      page.replaceSelectedFileBytes("Resume");
    }],
  ])("fails closed when a %s expectation changes after final authority", async (_kind, mutate) => {
    const definition = greenhouseAllControlsFixture();
    const page = new GreenhouseFixturePage(definition);
    const submitHooks: string[] = [];
    const machine = await readyMachine(page, context(page, {
      answers: {
        "Why Bluey?": "Reliable automation",
        "Experience level": "Senior",
        "Confirm accuracy": "yes",
        work_mode: "Remote",
      },
    }, {
      events: submitHooks,
      before: async () => { mutate(page); },
    }));
    machine.approveFinalReview();

    await expect(machine.submit()).rejects.toThrow("changed after final submit authority");

    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual(["before"]);
  });

  it("rejects same-name different resume bytes after final authority", async () => {
    const page = new GreenhouseFixturePage(PUBLIC_MODERN);
    const submitHooks: string[] = [];
    const machine = await readyMachine(page, context(page, {}, {
      events: submitHooks,
      before: async () => { page.replaceSelectedFileBytes("Resume"); },
    }));
    machine.approveFinalReview();

    await expect(machine.submit()).rejects.toThrow("changed after final submit authority");

    expect(page.controlByLabel("Resume").value).toBe(RESUME_NAME);
    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual(["before"]);
  });

  it("rejects an outer form action change with unchanged page and submit button", async () => {
    const page = new GreenhouseFixturePage(PUBLIC_MODERN);
    const submitHooks: string[] = [];
    const machine = await readyMachine(page, context(page, {}, {
      events: submitHooks,
      before: async () => {
        page.mutateOuterFormAction(`${PUBLIC_MODERN.url}?target-drift=1`);
      },
    }));
    machine.approveFinalReview();

    await expect(machine.submit()).rejects.toThrow("submit target changed");

    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual(["before"]);
  });

  it.each([
    ["outer form enctype", (page: GreenhouseFixturePage) => {
      page.mutateOuterFormEnctype("text/plain");
    }],
    ["submitter formenctype", (page: GreenhouseFixturePage) => {
      page.mutateSubmitterFormEnctype("application/x-www-form-urlencoded");
    }],
    ["outer form target", (page: GreenhouseFixturePage) => {
      page.mutateOuterFormTarget("_blank");
    }],
    ["submitter formtarget", (page: GreenhouseFixturePage) => {
      page.mutateSubmitterFormTarget("_blank");
    }],
  ] as const)("rejects %s drift after final authority", async (_name, mutate) => {
    const page = new GreenhouseFixturePage(PUBLIC_MODERN);
    const submitHooks: string[] = [];
    const machine = await readyMachine(page, context(page, {}, {
      events: submitHooks,
      before: async () => { mutate(page); },
    }));
    machine.approveFinalReview();

    await expect(machine.submit()).rejects.toThrow("submit target changed");

    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual(["before"]);
  });

  it.each([
    ["selected file bytes", (page: GreenhouseFixturePage) => {
      page.replaceSelectedFileBytes("Resume");
    }],
    ["outer form action", (page: GreenhouseFixturePage) => {
      page.mutateOuterFormAction(`${PUBLIC_MODERN.url}?click-handler-drift=1`);
    }],
    ["outer form enctype", (page: GreenhouseFixturePage) => {
      page.mutateOuterFormEnctype("text/plain");
    }],
    ["submitter formenctype", (page: GreenhouseFixturePage) => {
      page.mutateSubmitterFormEnctype("application/x-www-form-urlencoded");
    }],
    ["_blank/window.open form target", (page: GreenhouseFixturePage) => {
      page.mutateSubmitterFormTarget("_blank");
    }],
  ] as const)("aborts a click-handler mutation of %s before outgoing submit", async (_name, mutate) => {
    const page = new GreenhouseFixturePage(PUBLIC_MODERN, {
      duringSubmitActivation: mutate,
    });
    const submitHooks: string[] = [];
    const machine = await readyMachine(page, context(page, {}, { events: submitHooks }));
    machine.approveFinalReview();

    await expect(machine.submit()).rejects.toThrow("submit evidence changed during activation");

    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual(["before"]);
  });

  it.each([
    ["cross-origin action", (page: GreenhouseFixturePage) => {
      page.mutateOuterFormAction("https://example.com/acme/jobs/12345");
    }],
    ["unsupported method", (page: GreenhouseFixturePage) => {
      page.mutateOuterFormMethod("get");
    }],
    ["unsupported outer enctype", (page: GreenhouseFixturePage) => {
      page.mutateOuterFormEnctype("text/plain");
    }],
    ["unsupported submitter formenctype", (page: GreenhouseFixturePage) => {
      page.mutateSubmitterFormEnctype("application/x-www-form-urlencoded");
    }],
    ["unsupported outer form target", (page: GreenhouseFixturePage) => {
      page.mutateOuterFormTarget("_blank");
    }],
    ["unsupported submitter formtarget", (page: GreenhouseFixturePage) => {
      page.mutateSubmitterFormTarget("_blank");
    }],
    ["another provider job", (page: GreenhouseFixturePage) => {
      page.mutateOuterFormAction("https://job-boards.greenhouse.io/acme/jobs/99999");
    }],
  ] as const)("rejects an invalid effective submit target: %s", async (_name, mutate) => {
    const page = new GreenhouseFixturePage(PUBLIC_MODERN);
    const submitHooks: string[] = [];
    const machine = await readyMachine(page, context(page, {}, { events: submitHooks }));
    machine.approveFinalReview();
    mutate(page);

    await expect(machine.submit()).rejects.toThrow("submit target is invalid");

    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual([]);
  });

  it.each([
    ["URL", (page: GreenhouseFixturePage) => {
      page.navigateTo(`${PUBLIC_MODERN.url}?changed-after-authority=1`);
    }],
    ["Submit control", (page: GreenhouseFixturePage) => {
      page.replaceSubmitControl();
    }],
  ])("fails closed when the %s changes during final authority", async (_kind, mutate) => {
    const page = new GreenhouseFixturePage(PUBLIC_MODERN);
    const submitHooks: string[] = [];
    const machine = await readyMachine(page, context(page, {}, {
      events: submitHooks,
      before: async () => { mutate(page); },
    }));
    machine.approveFinalReview();

    await expect(machine.submit()).rejects.toThrow("changed after final submit authority");

    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual(["before"]);
  });

  it("pauses immediately when Greenhouse presents a challenge", async () => {
    const page = new GreenhouseFixturePage({
      ...PUBLIC_MODERN,
      body: "Verify you are human. Complete the CAPTCHA to continue.",
    });
    const submitHooks: string[] = [];
    const machine = new GreenhouseApplicationStateMachine(context(page, {}, { events: submitHooks }));
    await machine.detect();

    await machine.prepare();

    expect(machine.state.name).toBe("receipt");
    expect(machine.getReceipt()).toMatchObject({
      status: "needs_input",
      intervention: { kind: "captcha" },
    });
    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual([]);
  });

  it("treats a lost submit response as uncertain and never retries the click", async () => {
    const page = new GreenhouseFixturePage({
      ...PUBLIC_MODERN,
      confirmationBody: "",
    }, { removeControlsAfterSubmit: true, postSubmitUrl: `${PUBLIC_MODERN.url}/confirmation` });
    const submitHooks: string[] = [];
    const machine = await readyMachine(page, context(page, {}, { events: submitHooks }));
    machine.approveFinalReview();

    const receipt = await machine.submit();
    const repeated = await machine.submit();

    expect(receipt).toMatchObject({
      status: "needs_input",
      intervention: { kind: "browser_takeover" },
    });
    expect(receipt).not.toHaveProperty("confirmationText");
    expect(receipt).not.toHaveProperty("confirmationUrl");
    expect(receipt).not.toHaveProperty("submittedAt");
    expect(repeated).toBe(receipt);
    expect(page.submitClicks).toBe(1);
    expect(submitHooks).toEqual(["before", "after:activated"]);
  });

  it("rejects confirmation text when Greenhouse rerenders the submit form", async () => {
    const page = new GreenhouseFixturePage(PUBLIC_MODERN, {
      postSubmitUrl: `${PUBLIC_MODERN.url}/confirmation`,
    });
    const machine = await readyMachine(page);
    machine.approveFinalReview();

    const receipt = await machine.submit();

    expect(receipt.status).toBe("needs_input");
    expect(receipt.issues[0]?.message).toContain("returned the application form");
    expect(receipt).not.toHaveProperty("submittedAt");
  });

  it("rejects confirmation text when Greenhouse rerenders ambiguous submit controls", async () => {
    const page = new GreenhouseFixturePage(PUBLIC_MODERN, {
      postSubmitCount: 2,
      postSubmitUrl: `${PUBLIC_MODERN.url}/confirmation`,
    });
    const machine = await readyMachine(page);
    machine.approveFinalReview();

    const receipt = await machine.submit();

    expect(await page.bodyText()).toBe(PUBLIC_MODERN.confirmationBody);
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
  ])("rejects mixed positive and negative Greenhouse confirmation text: %s", async (confirmationBody) => {
    const page = new GreenhouseFixturePage({
      ...PUBLIC_MODERN,
      confirmationBody,
    }, {
      removeControlsAfterSubmit: true,
      postSubmitUrl: `${PUBLIC_MODERN.url}/confirmation`,
    });
    const machine = await readyMachine(page);
    machine.approveFinalReview();

    const receipt = await machine.submit();

    expect(receipt.status).toBe("needs_input");
    expect(receipt.confirmationText).toBeUndefined();
    expect(receipt).not.toHaveProperty("submittedAt");
    expect(page.submitClicks).toBe(1);
  });

  it("does not claim same-host confirmation text redirected to another job", async () => {
    const page = new GreenhouseFixturePage(PUBLIC_MODERN, {
      removeControlsAfterSubmit: true,
      postSubmitUrl: "https://job-boards.greenhouse.io/acme/jobs/99999/confirmation",
    });
    const machine = await readyMachine(page);
    machine.approveFinalReview();

    const receipt = await machine.submit();

    expect(receipt.status).toBe("needs_input");
    expect(receipt).not.toHaveProperty("submittedAt");
    expect(page.submitClicks).toBe(1);
  });

  it("awaits a durable fence failure and never activates Submit", async () => {
    const page = new GreenhouseFixturePage(PUBLIC_MODERN);
    const submitHooks: string[] = [];
    const machine = await readyMachine(page, context(page, {}, {
      events: submitHooks,
      before: async () => { throw new Error("durable fence response unavailable"); },
    }));
    machine.approveFinalReview();

    await expect(machine.submit()).rejects.toThrow("durable fence response unavailable");

    expect(page.submitClicks).toBe(0);
    expect(submitHooks).toEqual(["before"]);
  });

  it("keeps URL-only detection limited to official Greenhouse hosts", () => {
    expect(detectGreenhouseUrl(PUBLIC_MODERN.url)).toMatchObject({ variant: "public", tenant: "acme" });
    expect(detectGreenhouseUrl("https://boards.greenhouse.io/embed/job_app?for=acme&token=123"))
      .toMatchObject({ variant: "embedded", tenant: "acme" });
    expect(detectGreenhouseUrl(EMBEDDED_LEGACY.url)).toBeUndefined();
  });
});

async function readyMachine(
  page: GreenhouseFixturePage,
  adapterContext: AdapterContext = context(page),
): Promise<GreenhouseApplicationStateMachine> {
  const machine = new GreenhouseApplicationStateMachine(adapterContext);
  await machine.detect();
  await machine.prepare();
  await machine.fill();
  await machine.validate();
  expect(machine.state.name).toBe("submit");
  return machine;
}

function context(
  page: BrowserPage,
  packetOverrides: Partial<AdapterContext["packet"]> = {},
  hooks?: {
    events: string[];
    proofs?: ProviderFinalSubmitProof[];
    before?: () => Promise<void>;
    approvedCanonicalUrl?: string;
  },
): AdapterContext {
  const answers = {
    first_name: "Ada",
    last_name: "Lovelace",
    email: "ada@example.com",
    "How did you hear about us?": "Referral",
    ...packetOverrides.answers,
  };
  return {
    runner: "local",
    runId: "run-greenhouse-123",
    accountId: "account-123",
    approvedCanonicalUrl: hooks?.approvedCanonicalUrl ?? page.url(),
    page,
    packet: {
      applicationId: "application-123",
      jobId: "job-123",
      resumeVersionId: "resume-version-123",
      approvedPacketChecksum: "c".repeat(64),
      resumePath: RESUME_PATH,
      verifiedClaimIds: ["claim-1"],
      applicationIdentityId: "identity-123",
      applicationEmail: "ada@example.com",
      browserProfileId: "browser-profile-123",
      ...packetOverrides,
      answers,
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

function field(selector: string, label: string, required: boolean, kind: FormControl["kind"] = "text"): FormControl {
  return { selector, label, required, kind, name: selector, placeholder: "", value: "" };
}

function withControl(definition: FixtureDefinition, control: FormControl): FixtureDefinition {
  return { ...definition, controls: [...definition.controls, control] };
}

function greenhouseAllControlsFixture(): FixtureDefinition {
  return {
    ...PUBLIC_MODERN,
    controls: [
      ...PUBLIC_MODERN.controls,
      field("[data-bluey-field-id='why-bluey']", "Why Bluey?", true, "textarea"),
      {
        ...field("[data-bluey-field-id='experience']", "Experience level", true, "select"),
        options: [
          { label: "Choose", value: "" },
          { label: "Entry", value: "entry" },
          { label: "Senior", value: "senior" },
        ],
      },
      {
        ...field("[data-bluey-field-id='accurate']", "Confirm accuracy", true, "checkbox"),
        checked: false,
      },
      {
        ...field("[data-bluey-field-id='remote']", "Work mode Remote", true, "radio"),
        name: "work_mode",
        value: "remote",
        checked: false,
      },
      {
        ...field("[data-bluey-field-id='office']", "Work mode Office", true, "radio"),
        name: "work_mode",
        value: "office",
        checked: false,
      },
    ],
  };
}

class GreenhouseFixturePage implements BrowserPage {
  private readonly controlsState: FormControl[];
  private currentUrl: string;
  private submitSelector = "#submit_app";
  private formAction: string;
  private formMethod = "post";
  private formEnctype = "multipart/form-data";
  private submitterEnctype?: string;
  private formTarget = "_self";
  private submitterTarget?: string;
  private submitted = false;
  submitClicks = 0;
  applyClicks = 0;
  fieldWrites = 0;
  fileSelections = 0;

  constructor(
    private readonly definition: FixtureDefinition,
    private readonly behavior: {
      removeControlsAfterSubmit?: boolean;
      postSubmitCount?: number;
      postSubmitUrl?: string;
      ignoredWrites?: string[];
      duringSubmitActivation?: (page: GreenhouseFixturePage) => void;
    } = {},
  ) {
    this.currentUrl = definition.url;
    this.formAction = definition.url;
    this.controlsState = definition.controls.map((control) => ({
      ...control,
      options: control.options?.map((option) => ({ ...option })),
      files: initialFileEvidence(control),
    }));
  }

  url(): string {
    return this.currentUrl;
  }

  async installExactSubmitGuard(): Promise<void> {}

  async beginExactSubmitGuard(): Promise<void> {}

  async assertExactSubmitGuardClean(): Promise<void> {}

  async title(): Promise<string> {
    return this.definition.title;
  }

  async controls(): Promise<FormControl[]> {
    if (this.submitted && this.behavior.removeControlsAfterSubmit) return [];
    return this.controlsState.map((control) => ({
      ...control,
      options: control.options?.map((option) => ({ ...option })),
      files: control.files?.map((file) => ({ ...file })),
    }));
  }

  async bodyText(): Promise<string> {
    return this.submitted ? this.definition.confirmationBody : this.definition.body;
  }

  async waitForSettled(): Promise<void> {}

  async screenshot(): Promise<Uint8Array> {
    return new Uint8Array();
  }

  controlByLabel(label: string): FormControl {
    const control = this.controlsState.find((candidate) => candidate.label.toLowerCase().includes(label.toLowerCase()));
    if (!control) throw new Error(`Fixture control not found: ${label}`);
    return control;
  }

  navigateTo(url: string): void {
    this.currentUrl = url;
  }

  replaceSubmitControl(): void {
    this.submitSelector = "button[data-testid='submit-application']";
  }

  replaceSelectedFileBytes(label: string): void {
    const control = this.controlByLabel(label);
    const selected = control.files?.[0];
    if (!selected) throw new Error(`Fixture file is not selected: ${label}`);
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
    const marker = this.definition.markers.includes(selector) && selector !== "#submit_app";
    const isSubmit = (): boolean => selector === this.submitSelector;
    const exists = (): boolean => Boolean(control || marker
      || isSubmit() && !(this.submitted && this.behavior.removeControlsAfterSubmit));
    const count = (): number => {
      if (isSubmit() && this.submitted && this.behavior.postSubmitCount !== undefined) {
        return this.behavior.postSubmitCount;
      }
      return exists() ? 1 : 0;
    };
    return {
      count: async () => count(),
      fill: async (value) => {
        if (control && !this.behavior.ignoredWrites?.includes(selector)) {
          control.value = value;
          this.fieldWrites += 1;
        }
      },
      click: async () => {
        if (selector === "#apply_button") {
          this.applyClicks += 1;
          return;
        }
        if (isSubmit()) {
          this.submitClicks += 1;
          this.submitted = true;
          if (this.behavior.postSubmitUrl) this.currentUrl = this.behavior.postSubmitUrl;
        }
      },
      textContent: async () => isSubmit() ? "Submit Application" : "",
      getAttribute: async (name) => submitAttribute(this.submitSelector, selector, name),
      isVisible: async () => count() > 0,
      selectOption: async (value) => {
        if (control && !this.behavior.ignoredWrites?.includes(selector)) {
          control.value = value;
          this.fieldWrites += 1;
        }
      },
      setChecked: async (checked) => {
        if (control && !this.behavior.ignoredWrites?.includes(selector)) {
          control.checked = checked;
          this.fieldWrites += 1;
        }
      },
      setInputFiles: async (paths) => {
        if (control && !this.behavior.ignoredWrites?.includes(selector)) {
          const files = paths.map(fileEvidenceForSnapshotPath);
          control.value = files.map((file) => file.name).join(", ");
          control.files = files;
          this.fileSelections += 1;
          return files;
        }
        return [];
      },
      effectiveSubmitTarget: async (adapter) => {
        if (!isSubmit()) throw new Error("Fixture locator is not the submit control");
        return {
          actionUrl: this.formAction,
          method: this.formMethod,
          enctype: this.submitterEnctype ?? this.formEnctype,
          formTarget: this.submitterTarget ?? this.formTarget,
          providerJobKey: certifiedProviderJobKey(adapter, this.formAction, "submit"),
          formIdentity: "[0,\"application_form\",\"\",\"\",\"\",\"\",\"\"]",
        };
      },
      successfulSubmitEvidence: async (trustedFields, providerJobKey) => (
        fixtureSubmitEvidence(this.controlsState, trustedFields, providerJobKey)
      ),
      clickWithExactSubmit: async (expectation) => {
        if (!isSubmit()) throw new Error("Fixture locator is not the submit control");
        this.behavior.duringSubmitActivation?.(this);
        this.assertExactSubmit(expectation);
        this.submitClicks += 1;
        this.submitted = true;
        if (this.behavior.postSubmitUrl) this.currentUrl = this.behavior.postSubmitUrl;
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
        providerJobKey: certifiedProviderJobKey("greenhouse", this.formAction, "submit"),
        formIdentity: "[0,\"application_form\",\"\",\"\",\"\",\"\",\"\"]",
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

function initialFileEvidence(control: FormControl) {
  if (control.kind !== "file" || !control.value) return [];
  return [{
    name: control.value.split(/[\\/]/u).at(-1) ?? control.value,
    byteLength: 256,
    sha256: "e".repeat(64),
  }];
}

function submitAttribute(currentSelector: string, selector: string, name: string): string | null {
  if (selector !== currentSelector) return null;
  if (name === "type") return "submit";
  if (currentSelector === "#submit_app" && name === "id") return "submit_app";
  if (currentSelector.includes("data-testid") && name === "data-testid") {
    return "submit-application";
  }
  return null;
}
