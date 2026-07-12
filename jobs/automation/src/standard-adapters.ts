import type {
  AdapterContext,
  ApplicationAdapter,
  ApplicationPacket,
  AtsKind,
  BrowserLocator,
  BrowserPage,
  FormControl,
  InterventionRequest,
  NormalizedJob,
  SubmissionReceipt,
  ValidationIssue,
} from "./contracts.js";

interface AdapterDefinition {
  kind: AtsKind;
  version: string;
  applySelectors: string[];
  nextSelectors: string[];
  submitSelectors: string[];
  confirmationPatterns: RegExp[];
}

const COMMON_APPLY = [
  "a[data-automation-id='applyButton']",
  "button[data-automation-id='applyButton']",
  "a[href*='apply']",
  "button:has-text('Apply')",
];

const COMMON_NEXT = [
  "button[data-automation-id='bottom-navigation-next-button']",
  "button:has-text('Save and Continue')",
  "button:has-text('Next')",
  "button:has-text('Continue')",
];

const COMMON_SUBMIT = [
  "button[data-automation-id='bottom-navigation-next-button']:has-text('Submit')",
  "button[type='submit']:has-text('Submit')",
  "button:has-text('Submit application')",
  "button:has-text('Submit Application')",
  "input[type='submit']",
];

const COMMON_CONFIRMATION = [
  /application (?:has been |was )?submitted/i,
  /application (?:has been )?received/i,
  /thank(?:s| you) for applying/i,
];

const DEFINITIONS: AdapterDefinition[] = [
  {
    kind: "greenhouse",
    version: "2026.07.1",
    applySelectors: ["#apply_button", ...COMMON_APPLY],
    nextSelectors: COMMON_NEXT,
    submitSelectors: ["#submit_app", ...COMMON_SUBMIT],
    confirmationPatterns: COMMON_CONFIRMATION,
  },
  {
    kind: "lever",
    version: "2026.07.1",
    applySelectors: ["a.postings-btn", ...COMMON_APPLY],
    nextSelectors: COMMON_NEXT,
    submitSelectors: ["button.template-btn-submit", ...COMMON_SUBMIT],
    confirmationPatterns: COMMON_CONFIRMATION,
  },
  {
    kind: "ashby",
    version: "2026.07.1",
    applySelectors: ["a[href*='/application']", ...COMMON_APPLY],
    nextSelectors: COMMON_NEXT,
    submitSelectors: COMMON_SUBMIT,
    confirmationPatterns: COMMON_CONFIRMATION,
  },
  {
    kind: "smartrecruiters",
    version: "2026.07.1",
    applySelectors: ["a[data-test='apply-button']", ...COMMON_APPLY],
    nextSelectors: COMMON_NEXT,
    submitSelectors: ["button[data-test='submit-application']", ...COMMON_SUBMIT],
    confirmationPatterns: [/application (?:was )?sent/i, ...COMMON_CONFIRMATION],
  },
  {
    kind: "workday",
    version: "2026.07.1",
    applySelectors: COMMON_APPLY,
    nextSelectors: COMMON_NEXT,
    submitSelectors: COMMON_SUBMIT,
    confirmationPatterns: COMMON_CONFIRMATION,
  },
  {
    kind: "semantic",
    version: "2026.07.1",
    applySelectors: COMMON_APPLY,
    nextSelectors: COMMON_NEXT,
    submitSelectors: COMMON_SUBMIT,
    confirmationPatterns: COMMON_CONFIRMATION,
  },
];

const SENSITIVE_PATTERNS = [
  /gender/i,
  /race|ethnic/i,
  /disab/i,
  /veteran/i,
  /sexual orientation/i,
  /religion/i,
];

const CHALLENGES: Array<{ pattern: RegExp; intervention: InterventionRequest }> = [
  {
    pattern: /captcha|verify you are human|security check/i,
    intervention: {
      kind: "captcha",
      title: "Complete the security check",
      detail: "Take over the preserved browser, complete the check, then let Bluey continue.",
      resolution: { kind: "browser_takeover", resumeAfter: true },
    },
  },
  {
    pattern: /verification code|two-factor|two factor|authenticator code|one-time code/i,
    intervention: {
      kind: "two_factor",
      title: "Verification needed",
      detail: "Approve this sign-in or enter the code in the preserved browser.",
      resolution: { kind: "browser_takeover", resumeAfter: true },
    },
  },
  {
    pattern: /assessment|coding challenge|skills test/i,
    intervention: {
      kind: "assessment",
      title: "Assessment ready",
      detail: "This employer requires an assessment before the application can continue.",
      resolution: { kind: "browser_takeover", resumeAfter: true },
    },
  },
];

const ANSWER_ALIASES: Record<string, string[]> = {
  first_name: ["first name", "given name"],
  last_name: ["last name", "family name", "surname"],
  full_name: ["full name", "name"],
  email: ["email", "email address"],
  phone: ["phone", "phone number", "mobile"],
  location: ["location", "city", "current location"],
  address: ["address", "street address"],
  linkedin_url: ["linkedin", "linkedin profile"],
  portfolio_url: ["portfolio", "website", "personal website"],
  salary_expectation: ["salary", "compensation", "desired pay"],
  sponsorship_required: ["sponsorship", "visa sponsorship"],
  work_authorization: ["work authorization", "authorized to work"],
};

export class StandardAtsAdapter implements ApplicationAdapter {
  readonly kind: AtsKind;
  readonly version: string;

  constructor(private readonly definition: AdapterDefinition) {
    this.kind = definition.kind;
    this.version = definition.version;
  }

  detect(url: URL): boolean {
    return detectDefinition(url) === this.definition.kind;
  }

  async normalize(page: BrowserPage): Promise<NormalizedJob> {
    const title = await page.title();
    const parts = title.split(/[|\-–—]/).map((value) => value.trim()).filter(Boolean);
    return {
      externalId: new URL(page.url()).pathname.split("/").filter(Boolean).at(-1) || page.url(),
      canonicalUrl: page.url(),
      company: parts.at(-1) || "Employer",
      title: parts[0] || "Open role",
      location: "",
      workplace: "unknown",
      description: await page.bodyText(),
      source: this.kind,
    };
  }

  async prepare(context: AdapterContext): Promise<void> {
    const challenge = await detectChallenge(context.page);
    if (challenge) return;
    if ((await context.page.controls()).length > 0) return;
    const apply = await firstVisible(context.page, this.definition.applySelectors);
    if (apply) {
      await apply.click();
      await context.page.waitForSettled();
      await context.log("application_form_opened", { adapter: this.kind, version: this.version });
    }
  }

  async fill(context: AdapterContext): Promise<void> {
    const controls = await context.page.controls();
    let filled = 0;
    for (const control of controls) {
      if (control.kind === "hidden" || control.kind === "other") continue;
      const field = searchableField(control);
      if (control.kind === "file") {
        const file = /cover/i.test(field) ? context.packet.coverLetterPath : context.packet.resumePath;
        if (file) {
          await context.page.locator(control.selector).setInputFiles([file]);
          filled += 1;
        }
        continue;
      }
      const answer = answerFor(control, context.packet);
      if (answer === undefined || answer === "") continue;
      const locator = context.page.locator(control.selector);
      if (control.kind === "select") {
        const option = bestOption(control, answer);
        if (option) {
          await locator.selectOption(option);
          filled += 1;
        }
      } else if (control.kind === "radio") {
        if (radioMatches(control, answer)) {
          await locator.setChecked(true);
          filled += 1;
        }
      } else if (control.kind === "checkbox") {
        const checked = /^(1|true|yes|y|on)$/i.test(answer.trim());
        await locator.setChecked(checked);
        filled += 1;
      } else {
        await locator.fill(answer);
        filled += 1;
      }
    }
    await context.log("application_fields_filled", { adapter: this.kind, count: filled });
  }

  async validate(context: AdapterContext): Promise<ValidationIssue[]> {
    const issues: ValidationIssue[] = [];
    const controls = await context.page.controls();
    for (const control of controls) {
      if (!control.required || hasValue(control)) continue;
      if (control.kind === "radio" && controls.some((candidate) => (
        candidate.kind === "radio"
        && candidate.name === control.name
        && candidate.checked
      ))) continue;
      const field = searchableField(control) || "required field";
      issues.push({
        field,
        message: answerFor(control, context.packet) === undefined
          ? `Bluey needs an answer for ${field}.`
          : `The employer did not accept the value for ${field}.`,
        severity: "blocking",
      });
    }
    return issues;
  }

  async submit(context: AdapterContext): Promise<SubmissionReceipt> {
    for (let step = 0; step < 12; step += 1) {
      const challenge = await detectChallenge(context.page);
      if (challenge) return interventionReceipt(challenge);
      const currentBody = await context.page.bodyText();
      if (this.definition.confirmationPatterns.some((pattern) => pattern.test(currentBody))) {
        return {
          status: "submitted",
          confirmationText: confirmationExcerpt(currentBody),
          confirmationUrl: context.page.url(),
          submittedAt: new Date().toISOString(),
          issues: [],
        };
      }

      await this.fill(context);
      const issues = await this.validate(context);
      if (issues.some((issue) => issue.severity === "blocking")) {
        const sensitive = issues.find((issue) => SENSITIVE_PATTERNS.some((pattern) => pattern.test(issue.field)));
        const unknown = issues.find((issue) => !isKnownProfileField(issue.field));
        return interventionReceipt({
          kind: sensitive ? "sensitive_question" : unknown ? "unknown_question" : "missing_fact",
          title: sensitive ? "Your choice is needed" : unknown ? "A new question needs your answer" : "One detail is missing",
          detail: issues[0]?.message || "Complete the required application field.",
          field: issues[0]?.field,
          resolution: { kind: "answer", resumeAfter: true },
        }, issues);
      }

      const submit = await firstVisible(context.page, this.definition.submitSelectors);
      if (submit) {
        await context.beforeFinalSubmit?.();
        try {
          await submit.click();
        } catch (error) {
          await context.afterFinalSubmit?.("activation_uncertain");
          throw error;
        }
        await context.afterFinalSubmit?.("activated");
        await context.page.waitForSettled();
        const afterChallenge = await detectChallenge(context.page);
        if (afterChallenge) return interventionReceipt(afterChallenge);
        const body = await context.page.bodyText();
        const confirmed = this.definition.confirmationPatterns.some((pattern) => pattern.test(body));
        const stillHasForm = (await context.page.controls()).some((control) => control.required);
        if (confirmed || (!stillHasForm && confirmationUrl(context.page.url()))) {
          return {
            status: "submitted",
            confirmationText: confirmationExcerpt(body),
            confirmationUrl: context.page.url(),
            submittedAt: new Date().toISOString(),
            issues: [],
          };
        }
        return interventionReceipt({
          kind: "browser_takeover",
          title: "Confirm the application result",
          detail: "Bluey sent the form but the employer did not show a clear confirmation. Review the preserved browser before continuing.",
          resolution: { kind: "browser_takeover", resumeAfter: true },
        });
      }

      const next = await firstVisible(context.page, this.definition.nextSelectors);
      if (!next) {
        return {
          status: "failed",
          issues: [{
            field: "application",
            message: "Bluey could not find the next application step or submission control.",
            severity: "blocking",
          }],
        };
      }
      await next.click();
      await context.page.waitForSettled();
      await context.log("application_step_advanced", { adapter: this.kind, step: step + 1 });
    }
    return {
      status: "failed",
      issues: [{ field: "application", message: "The application exceeded 12 steps.", severity: "blocking" }],
    };
  }
}

export function createStandardAdapters(): ApplicationAdapter[] {
  return DEFINITIONS.map((definition) => new StandardAtsAdapter(definition));
}

function detectDefinition(url: URL): AtsKind {
  const host = url.hostname.toLowerCase();
  if (host.includes("myworkdayjobs.com")) return "workday";
  if (host === "boards.greenhouse.io" || host === "job-boards.greenhouse.io") return "greenhouse";
  if (host === "jobs.lever.co") return "lever";
  if (host === "jobs.ashbyhq.com") return "ashby";
  if (host === "jobs.smartrecruiters.com" || host.endsWith(".smartrecruiters.com")) return "smartrecruiters";
  return "semantic";
}

async function firstVisible(page: BrowserPage, selectors: string[]): Promise<BrowserLocator | undefined> {
  for (const selector of selectors) {
    const locator = page.locator(selector);
    if ((await locator.count()) > 0 && await locator.isVisible()) return locator;
  }
  return undefined;
}

async function detectChallenge(page: BrowserPage): Promise<InterventionRequest | undefined> {
  const body = await page.bodyText();
  return CHALLENGES.find((candidate) => candidate.pattern.test(body))?.intervention;
}

function interventionReceipt(intervention: InterventionRequest, issues: ValidationIssue[] = []): SubmissionReceipt {
  return { status: "needs_input", issues, intervention };
}

function searchableField(control: FormControl): string {
  return [control.label, control.name, control.placeholder].filter(Boolean).join(" ").trim();
}

function normalize(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}

function answerFor(control: FormControl, packet: ApplicationPacket): string | undefined {
  const field = normalize(searchableField(control));
  const entries = Object.entries(packet.answers);
  const exact = entries.find(([key]) => normalize(key) === field);
  if (exact) return exact[1];
  for (const [canonical, aliases] of Object.entries(ANSWER_ALIASES)) {
    if (!aliases.some((alias) => field === alias || field.includes(alias))) continue;
    const candidate = entries.find(([key]) => normalize(key) === normalize(canonical)
      || aliases.some((alias) => normalize(key) === normalize(alias)));
    if (candidate) return candidate[1];
    if (canonical === "email" && packet.applicationEmail) return packet.applicationEmail;
  }
  const contains = entries.find(([key]) => {
    const normalizedKey = normalize(key);
    return normalizedKey.length >= 4 && (field.includes(normalizedKey) || normalizedKey.includes(field));
  });
  return contains?.[1];
}

function bestOption(control: FormControl, answer: string): string | undefined {
  const normalizedAnswer = normalize(answer);
  const options = control.options || [];
  return options.find((option) => normalize(option.value) === normalizedAnswer)?.value
    ?? options.find((option) => normalize(option.label) === normalizedAnswer)?.value
    ?? options.find((option) => normalize(option.label).includes(normalizedAnswer))?.value;
}

function radioMatches(control: FormControl, answer: string): boolean {
  const normalizedAnswer = normalize(answer);
  const candidates = [control.value, control.label]
    .map(normalize)
    .filter(Boolean);
  return candidates.some((candidate) => candidate === normalizedAnswer
    || candidate.endsWith(` ${normalizedAnswer}`)
    || normalizedAnswer.endsWith(` ${candidate}`));
}

function isKnownProfileField(field: string): boolean {
  const normalizedField = normalize(field);
  return Object.entries(ANSWER_ALIASES).some(([canonical, aliases]) => (
    normalizedField.includes(normalize(canonical))
    || aliases.some((alias) => normalizedField.includes(normalize(alias)))
  ));
}

function confirmationUrl(value: string): boolean {
  try {
    return /(?:thank|confirmation|submitted|success|complete)/i.test(new URL(value).pathname);
  } catch {
    return false;
  }
}

function hasValue(control: FormControl): boolean {
  if (control.kind === "checkbox" || control.kind === "radio") return Boolean(control.checked);
  if (control.kind === "file") return Boolean(control.value);
  return control.value.trim().length > 0;
}

function confirmationExcerpt(body: string): string {
  const normalized = body.replace(/\s+/g, " ").trim();
  return normalized.slice(0, 500);
}
