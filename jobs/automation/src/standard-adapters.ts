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
import {
  checkedExpectation,
  fileExpectation,
  type FormFillExpectation,
  valueExpectation,
  verifyFillExpectations,
} from "./form-readback.js";

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

const STANDARD_FILL_VERSION = "2026.11.1";

const DEFINITIONS: AdapterDefinition[] = [
  {
    kind: "greenhouse",
    version: STANDARD_FILL_VERSION,
    applySelectors: ["#apply_button", ...COMMON_APPLY],
    nextSelectors: COMMON_NEXT,
    submitSelectors: ["#submit_app", ...COMMON_SUBMIT],
    confirmationPatterns: COMMON_CONFIRMATION,
  },
  {
    kind: "lever",
    version: STANDARD_FILL_VERSION,
    applySelectors: ["a.postings-btn", ...COMMON_APPLY],
    nextSelectors: COMMON_NEXT,
    submitSelectors: ["button.template-btn-submit", ...COMMON_SUBMIT],
    confirmationPatterns: COMMON_CONFIRMATION,
  },
  {
    kind: "ashby",
    version: STANDARD_FILL_VERSION,
    applySelectors: ["a[href*='/application']", ...COMMON_APPLY],
    nextSelectors: COMMON_NEXT,
    submitSelectors: COMMON_SUBMIT,
    confirmationPatterns: COMMON_CONFIRMATION,
  },
  {
    kind: "smartrecruiters",
    version: STANDARD_FILL_VERSION,
    applySelectors: ["a[data-test='apply-button']", ...COMMON_APPLY],
    nextSelectors: COMMON_NEXT,
    submitSelectors: [
      "button[data-test='submit-application']",
      ...COMMON_SUBMIT,
    ],
    confirmationPatterns: [
      /application (?:was )?sent/i,
      ...COMMON_CONFIRMATION,
    ],
  },
  {
    kind: "workday",
    version: STANDARD_FILL_VERSION,
    applySelectors: COMMON_APPLY,
    nextSelectors: COMMON_NEXT,
    submitSelectors: COMMON_SUBMIT,
    confirmationPatterns: COMMON_CONFIRMATION,
  },
  {
    kind: "semantic",
    version: STANDARD_FILL_VERSION,
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

const CHALLENGES: Array<{
  pattern: RegExp;
  intervention: InterventionRequest;
}> = [
  {
    pattern: /captcha|verify you are human|security check/i,
    intervention: {
      kind: "captcha",
      title: "Complete the security check",
      detail:
        "Take over the preserved browser, complete the check, then let Bluey continue.",
      resolution: { kind: "browser_takeover", resumeAfter: true },
    },
  },
  {
    pattern:
      /verification code|two-factor|two factor|authenticator code|one-time code/i,
    intervention: {
      kind: "two_factor",
      title: "Verification needed",
      detail:
        "Approve this sign-in or enter the code in the preserved browser.",
      resolution: { kind: "browser_takeover", resumeAfter: true },
    },
  },
  {
    pattern: /assessment|coding challenge|skills test/i,
    intervention: {
      kind: "assessment",
      title: "Assessment ready",
      detail:
        "This employer requires an assessment before the application can continue.",
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
  private readonly fillExpectations = new WeakMap<
    AdapterContext,
    FormFillExpectation[]
  >();
  private readonly fillIssues = new WeakMap<AdapterContext, ValidationIssue[]>();

  constructor(private readonly definition: AdapterDefinition) {
    this.kind = definition.kind;
    this.version = definition.version;
  }

  detect(url: URL): boolean {
    return detectDefinition(url) === this.definition.kind;
  }

  async normalize(page: BrowserPage): Promise<NormalizedJob> {
    const title = await page.title();
    const parts = title
      .split(/[|\-–—]/)
      .map((value) => value.trim())
      .filter(Boolean);
    return {
      externalId:
        new URL(page.url()).pathname.split("/").filter(Boolean).at(-1) ||
        page.url(),
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
    const apply = await firstVisible(
      context.page,
      this.definition.applySelectors,
    );
    if (apply) {
      await apply.click();
      await context.page.waitForSettled();
      await context.log("application_form_opened", {
        adapter: this.kind,
        version: this.version,
      });
    }
  }

  async fill(context: AdapterContext): Promise<void> {
    const controls = await context.page.controls();
    const expectations: FormFillExpectation[] = [];
    const issues: ValidationIssue[] = [];
    const radioGroups = new Set<string>();
    const block = (control: FormControl): void => {
      const field = displayField(control);
      issues.push({
        field,
        message: `Bluey needs an unambiguous approved answer for ${field}.`,
        severity: "blocking",
      });
    };
    let filled = 0;
    for (const control of controls) {
      if (control.kind === "hidden") continue;
      if (control.kind === "other") {
        if (control.required) block(control);
        continue;
      }
      if (control.kind === "file") {
        const purpose = documentPurpose(control);
        const file = purpose === "resume"
          ? context.packet.resumePath
          : purpose === "cover"
            ? context.packet.coverLetterPath
            : undefined;
        if (file) {
          const selectedFiles = await context.page.locator(control.selector).setInputFiles([file]);
          expectations.push(
            fileExpectation(control, displayField(control), file, selectedFiles),
          );
          filled += 1;
        } else if (control.required) block(control);
        continue;
      }
      if (control.kind === "radio") {
        if (!control.name.trim()) {
          if (control.required) block(control);
          continue;
        }
        if (radioGroups.has(control.name)) continue;
        radioGroups.add(control.name);
        const group = controls.filter(
          (candidate) => candidate.kind === "radio" && candidate.name === control.name,
        );
        const answers = group.map((candidate) => resolveAnswer(candidate, context.packet));
        const values = new Set(answers.flatMap((answer) =>
          answer.kind === "resolved" ? [answer.value] : [],
        ));
        const answer = values.size === 1 ? [...values][0] : undefined;
        const matches = answer?.trim()
          ? group.filter((candidate) => radioMatches(candidate, answer))
          : [];
        if (answers.some((candidate) => candidate.kind === "ambiguous")
          || (values.size > 0 && answers.some((candidate) => candidate.kind === "missing"))
          || values.size > 1 || (answer !== undefined && matches.length !== 1)) {
          block(control);
        } else if (matches.length === 1) {
          const selected = matches[0]!;
          await context.page.locator(selected.selector).setChecked(true);
          expectations.push(checkedExpectation(selected, displayField(selected), true));
          filled += 1;
        } else if (group.some((candidate) => candidate.required)) block(control);
        continue;
      }
      const resolution = resolveAnswer(control, context.packet);
      if (resolution.kind !== "resolved") {
        if (resolution.kind === "ambiguous" || control.required) block(control);
        continue;
      }
      const answer = resolution.value;
      if (!answer.trim()) {
        block(control);
        continue;
      }
      const locator = context.page.locator(control.selector);
      if (control.kind === "select") {
        const option = bestOption(control, answer);
        if (option) {
          await locator.selectOption(option);
          expectations.push(
            valueExpectation(control, displayField(control), option),
          );
          filled += 1;
        } else block(control);
      } else if (control.kind === "checkbox") {
        const checked = booleanAnswer(answer);
        if (checked === undefined) {
          block(control);
          continue;
        }
        await locator.setChecked(checked);
        expectations.push(
          checkedExpectation(control, displayField(control), checked),
        );
        filled += 1;
      } else {
        await locator.fill(answer);
        expectations.push(
          valueExpectation(control, displayField(control), answer),
        );
        filled += 1;
      }
    }
    this.fillExpectations.set(context, expectations);
    this.fillIssues.set(context, issues);
    await context.log("application_fields_filled", {
      adapter: this.kind,
      count: filled,
    });
  }

  async validate(context: AdapterContext): Promise<ValidationIssue[]> {
    const controls = await context.page.controls();
    const issues = verifyFillExpectations(
      this.fillExpectations.get(context) ?? [],
      controls,
      providerLabel(this.kind),
    );
    issues.push(...(this.fillIssues.get(context) ?? []));
    const readbackFields = new Set(issues.map((issue) => issue.field));
    for (const control of controls) {
      if (!control.required || hasValue(control)) continue;
      if (
        control.kind === "radio" &&
        controls.some(
          (candidate) =>
            candidate.kind === "radio" &&
            candidate.name === control.name &&
            candidate.checked,
        )
      )
        continue;
      const field = searchableField(control) || "required field";
      if (readbackFields.has(field)) continue;
      issues.push({
        field,
        message:
          resolveAnswer(control, context.packet).kind !== "resolved"
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
      if (
        this.definition.confirmationPatterns.some((pattern) =>
          pattern.test(currentBody),
        )
      ) {
        return reviewOnlyReceipt(
          "Review the application result",
          "This generic form reader found confirmation-like text, but it cannot verify who submitted the application. Review the preserved browser.",
        );
      }

      await this.fill(context);
      const issues = await this.validate(context);
      if (issues.some((issue) => issue.severity === "blocking")) {
        const sensitive = issues.find((issue) =>
          SENSITIVE_PATTERNS.some((pattern) => pattern.test(issue.field)),
        );
        const unknown = issues.find(
          (issue) => !isKnownProfileField(issue.field),
        );
        return interventionReceipt(
          {
            kind: sensitive
              ? "sensitive_question"
              : unknown
                ? "unknown_question"
                : "missing_fact",
            title: sensitive
              ? "Your choice is needed"
              : unknown
                ? "A new question needs your answer"
                : "One detail is missing",
            detail:
              issues[0]?.message || "Complete the required application field.",
            field: issues[0]?.field,
            resolution: { kind: "answer", resumeAfter: true },
          },
          issues,
        );
      }

      const submit = await firstVisible(
        context.page,
        this.definition.submitSelectors,
      );
      if (submit) {
        return reviewOnlyReceipt(
          "Application ready for review",
          "Bluey filled the available fields. Review them in the preserved browser and submit the application yourself.",
        );
      }

      const next = await firstVisible(
        context.page,
        this.definition.nextSelectors,
      );
      if (!next) {
        return reviewOnlyReceipt(
          "Continue in the browser",
          "Bluey filled the fields it could, but this form needs your review before continuing.",
        );
      }
      await next.click();
      await context.page.waitForSettled();
      await context.log("application_step_advanced", {
        adapter: this.kind,
        step: step + 1,
      });
    }
    return reviewOnlyReceipt(
      "Continue in the browser",
      "This application has more steps than Bluey's generic form reader can safely review.",
    );
  }
}

export function createStandardAdapters(): ApplicationAdapter[] {
  return DEFINITIONS.map((definition) => new StandardAtsAdapter(definition));
}

function detectDefinition(url: URL): AtsKind {
  const host = url.hostname.toLowerCase();
  if (host === "myworkdayjobs.com" || host.endsWith(".myworkdayjobs.com"))
    return "workday";
  if (host === "boards.greenhouse.io" || host === "job-boards.greenhouse.io")
    return "greenhouse";
  if (host === "jobs.lever.co") return "lever";
  if (host === "jobs.ashbyhq.com") return "ashby";
  if (
    host === "jobs.smartrecruiters.com" ||
    host.endsWith(".smartrecruiters.com")
  )
    return "smartrecruiters";
  return "semantic";
}

async function firstVisible(
  page: BrowserPage,
  selectors: string[],
): Promise<BrowserLocator | undefined> {
  for (const selector of selectors) {
    const locator = page.locator(selector);
    if ((await locator.count()) > 0 && (await locator.isVisible()))
      return locator;
  }
  return undefined;
}

async function detectChallenge(
  page: BrowserPage,
): Promise<InterventionRequest | undefined> {
  const body = await page.bodyText();
  return CHALLENGES.find((candidate) => candidate.pattern.test(body))
    ?.intervention;
}

function interventionReceipt(
  intervention: InterventionRequest,
  issues: ValidationIssue[] = [],
): SubmissionReceipt {
  return { status: "needs_input", issues, intervention };
}

function reviewOnlyReceipt(title: string, detail: string): SubmissionReceipt {
  return interventionReceipt({
    kind: "browser_takeover",
    title,
    detail,
    resolution: { kind: "browser_takeover", resumeAfter: false },
  });
}

function searchableField(control: FormControl): string {
  return [control.label, control.name, control.placeholder]
    .filter(Boolean)
    .join(" ")
    .trim();
}

function displayField(control: FormControl): string {
  return (
    control.label.trim() ||
    control.placeholder.trim() ||
    control.name.trim() ||
    "application field"
  );
}

function normalize(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

type AnswerResolution =
  | { kind: "missing" }
  | { kind: "ambiguous" }
  | { kind: "resolved"; value: string };

function uniqueAnswer(values: string[]): AnswerResolution {
  const unique = new Set(values);
  if (unique.size === 0) return { kind: "missing" };
  if (unique.size !== 1) return { kind: "ambiguous" };
  return { kind: "resolved", value: [...unique][0]! };
}

function resolveAnswer(
  control: FormControl,
  packet: ApplicationPacket,
): AnswerResolution {
  const field = answerQuestion(control);
  if (!field) return { kind: "missing" };
  const entries = Object.entries(packet.answers)
    .map(([key, value]) => [normalize(key), value] as const)
    .filter(([key]) => Boolean(key));
  const exact = uniqueAnswer(entries
    .filter(([key]) => key === field)
    .map(([, value]) => value));
  if (exact.kind !== "missing") return exact;
  const aliasValues: string[] = [];
  for (const [canonical, aliases] of Object.entries(ANSWER_ALIASES)) {
    if (!aliases.some((alias) => field === normalize(alias))
      && field !== normalize(canonical))
      continue;
    const candidates = entries.filter(
      ([key]) =>
        key === normalize(canonical) || aliases.some((alias) => key === normalize(alias)),
    );
    aliasValues.push(...candidates.map(([, value]) => value));
    if (!candidates.length && canonical === "email" && packet.applicationEmail)
      aliasValues.push(packet.applicationEmail);
  }
  return uniqueAnswer(aliasValues);
}

function answerQuestion(control: FormControl): string {
  // A canonical-looking name cannot override a different visible question
  // (e.g. Manager email with name=email). Fuzzy token containment is not fact
  // authority. A later agent must propose approved fact references explicitly.
  const field = normalize(control.label.trim() || control.placeholder.trim() || control.name);
  if (control.kind !== "radio") return field;
  const option = normalize(control.value);
  if (!option) return "";
  if (field === option) return normalize(control.name);
  const suffix = ` ${option}`;
  return field.endsWith(suffix) ? field.slice(0, -suffix.length).trim() : field;
}

function documentPurpose(control: FormControl): "resume" | "cover" | undefined {
  const field = normalize(control.label.trim() || control.placeholder.trim() || control.name)
    .replace(/^(?:upload|attach|your) /u, "")
    .replace(/ (?:file|upload|required|optional)$/u, "");
  if (/^(?:resume|cv|curriculum vitae)(?: (?:or )?(?:resume|cv|curriculum vitae))?$/u.test(field))
    return "resume";
  return field === "cover letter" ? "cover" : undefined;
}

function booleanAnswer(answer: string): boolean | undefined {
  if (/^(?:1|true|yes|y|on)$/iu.test(answer.trim())) return true;
  if (/^(?:0|false|no|n|off)$/iu.test(answer.trim())) return false;
  return undefined;
}

function bestOption(control: FormControl, answer: string): string | undefined {
  const normalizedAnswer = normalize(answer);
  const options = control.options || [];
  if (!normalizedAnswer) return undefined;
  const matches = options.filter((option) => Boolean(option.value.trim())
    && (normalize(option.value) === normalizedAnswer
      || normalize(option.label) === normalizedAnswer));
  return matches.length === 1 ? matches[0]!.value : undefined;
}

function radioMatches(control: FormControl, answer: string): boolean {
  const normalizedAnswer = normalize(answer);
  if (!normalizedAnswer) return false;
  const candidates = [control.value, control.label]
    .map(normalize)
    .filter(Boolean);
  return candidates.some((candidate) => candidate === normalizedAnswer);
}

function isKnownProfileField(field: string): boolean {
  const normalizedField = normalize(field);
  return Object.entries(ANSWER_ALIASES).some(
    ([canonical, aliases]) =>
      normalizedField.includes(normalize(canonical)) ||
      aliases.some((alias) => normalizedField.includes(normalize(alias))),
  );
}

function hasValue(control: FormControl): boolean {
  if (control.kind === "checkbox" || control.kind === "radio")
    return Boolean(control.checked);
  if (control.kind === "file") return Boolean(control.value);
  return control.value.trim().length > 0;
}

function providerLabel(kind: AtsKind): string {
  if (kind === "semantic") return "Employer form";
  return `${kind[0]?.toUpperCase() ?? ""}${kind.slice(1)}`;
}
