import { describe, expect, it } from "vitest";
import publicModernJson from "./fixtures/greenhouse/public-modern.json";
import embeddedLegacyJson from "./fixtures/greenhouse/embedded-legacy.json";
import type {
  AdapterContext,
  BrowserLocator,
  BrowserPage,
  FormControl,
} from "../src/contracts.js";
import {
  GREENHOUSE_ADAPTER_PROFILE,
  GreenhouseAdapter,
  GreenhouseApplicationStateMachine,
  detectGreenhouseUrl,
  type GreenhouseVariant,
} from "../src/providers/greenhouse.js";

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
    ["Northstar legacy embedded", EMBEDDED_LEGACY, "embedded"],
  ] as const)("detects and completes the %s form variant after review", async (_name, definition, variant) => {
    const page = new GreenhouseFixturePage(definition);
    const submitHooks: string[] = [];
    const machine = new GreenhouseApplicationStateMachine(context(page, {}, { events: submitHooks }), {
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
      confirmationUrl: definition.url,
      submittedAt: "2026-07-11T12:00:00.000Z",
      issues: [],
    });
    expect(receipt.confirmationText).toBe(definition.confirmationBody);
    expect(page.submitClicks).toBe(1);
    expect(submitHooks).toEqual(["before", "after:activated"]);
    expect(machine.history).toEqual(["detect", "prepare", "fill", "validate", "submit", "receipt"]);
    expect(page.controlByLabel("First name").value).toBe("Ada");
    expect(page.controlByLabel("Last name").value).toBe("Lovelace");
    expect(page.controlByLabel("Email").value).toBe("ada@example.com");
    expect(page.controlByLabel("Resume").value).toBe("resume.pdf");
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
    const page = new GreenhouseFixturePage(EMBEDDED_LEGACY);
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
  hooks?: { events: string[]; before?: () => Promise<void> },
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
    page,
    packet: {
      applicationId: "application-123",
      jobId: "job-123",
      resumeVersionId: "resume-version-123",
      approvedPacketChecksum: "c".repeat(64),
      resumePath: "/tmp/resume.pdf",
      verifiedClaimIds: ["claim-1"],
      applicationIdentityId: "identity-123",
      applicationEmail: "ada@example.com",
      browserProfileId: "browser-profile-123",
      ...packetOverrides,
      answers,
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

function field(selector: string, label: string, required: boolean, kind: FormControl["kind"] = "text"): FormControl {
  return { selector, label, required, kind, name: selector, placeholder: "", value: "" };
}

function withControl(definition: FixtureDefinition, control: FormControl): FixtureDefinition {
  return { ...definition, controls: [...definition.controls, control] };
}

class GreenhouseFixturePage implements BrowserPage {
  private readonly controlsState: FormControl[];
  private currentUrl: string;
  private submitted = false;
  submitClicks = 0;

  constructor(
    private readonly definition: FixtureDefinition,
    private readonly behavior: { removeControlsAfterSubmit?: boolean; postSubmitUrl?: string } = {},
  ) {
    this.currentUrl = definition.url;
    this.controlsState = definition.controls.map((control) => ({
      ...control,
      options: control.options?.map((option) => ({ ...option })),
    }));
  }

  url(): string {
    return this.currentUrl;
  }

  async title(): Promise<string> {
    return this.definition.title;
  }

  async controls(): Promise<FormControl[]> {
    if (this.submitted && this.behavior.removeControlsAfterSubmit) return [];
    return this.controlsState.map((control) => ({
      ...control,
      options: control.options?.map((option) => ({ ...option })),
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

  locator(selector: string): BrowserLocator {
    const control = this.controlsState.find((candidate) => candidate.selector === selector);
    const marker = this.definition.markers.includes(selector);
    const submit = selector === "#submit_app";
    return {
      count: async () => control || marker ? 1 : 0,
      fill: async (value) => { if (control) control.value = value; },
      click: async () => {
        if (submit) {
          this.submitClicks += 1;
          this.submitted = true;
          if (this.behavior.postSubmitUrl) this.currentUrl = this.behavior.postSubmitUrl;
        }
      },
      textContent: async () => "",
      getAttribute: async () => null,
      isVisible: async () => Boolean(control || marker),
      selectOption: async (value) => { if (control) control.value = value; },
      setChecked: async (checked) => { if (control) control.checked = checked; },
      setInputFiles: async (paths) => { if (control) control.value = paths[0]?.split("/").at(-1) || ""; },
    };
  }
}
