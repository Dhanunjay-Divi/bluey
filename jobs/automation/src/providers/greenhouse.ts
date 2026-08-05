import type {
  AdapterContext,
  ApplicationAdapter,
  ApplicationPacket,
  BrowserLocator,
  BrowserPage,
  EffectiveSubmitTargetIdentity,
  ExactSubmitFormEvidence,
  FormControl,
  InterventionRequest,
  NormalizedJob,
  SubmissionReceipt,
  ValidationIssue,
} from "../contracts.js";
import {
  checkedExpectation,
  exactSubmitFileEvidence,
  exactSubmitTrustedFieldValues,
  fileExpectation,
  type FormFillExpectation,
  valueExpectation,
  verifyFillExpectations,
} from "../form-readback.js";
import {
  assertApprovedProviderJob,
  assertApprovedProviderJobOrConfirmation,
  captureEffectiveSubmitTarget,
  isApprovedProviderConfirmation,
  sameEffectiveSubmitTarget,
  sameExactSubmitFields,
  sameExactSubmitPartOrder,
} from "../effective-submit-target.js";
import { ExactSubmitEvidenceError } from "../trusted-submit.js";
import { hasNegativeSubmissionOutcome } from "../submission-confirmation.js";

export const GREENHOUSE_ADAPTER_PROFILE = Object.freeze({
  kind: "greenhouse",
  version: "2026.07.1-beta.1",
  maturity: "beta",
  capability: "beta_review",
  submissionMode: "review_only",
  certified: false,
  requiresFinalReview: true,
} as const);

export const GREENHOUSE_APPLICATION_STATES = [
  "detect",
  "prepare",
  "fill",
  "validate",
  "submit",
  "receipt",
] as const;

export type GreenhouseApplicationStateName = typeof GREENHOUSE_APPLICATION_STATES[number];
export type GreenhouseVariant = "public" | "embedded";

export interface GreenhouseDetection {
  variant: GreenhouseVariant;
  source: "official_url" | "embedded_form";
  tenant?: string;
}

export type GreenhouseApplicationState =
  | { name: "detect" }
  | { name: "prepare"; detection: GreenhouseDetection }
  | { name: "fill"; detection: GreenhouseDetection; formOpened: boolean }
  | { name: "validate"; detection: GreenhouseDetection; filledFieldCount: number }
  | {
      name: "submit";
      detection: GreenhouseDetection;
      reviewRequired: true;
      reviewApproved: boolean;
    }
  | { name: "receipt"; detection?: GreenhouseDetection; receipt: SubmissionReceipt };

export interface GreenhouseStateMachineOptions {
  finalReviewApproved?: boolean;
  now?: () => Date;
}

export interface GreenhouseAdapterOptions {
  finalReviewApproval?: (context: AdapterContext) => boolean | Promise<boolean>;
  now?: () => Date;
}

interface ValidationRecord {
  issue: ValidationIssue;
  interventionKind: "missing_fact" | "unknown_question" | "sensitive_question";
  choices?: string[];
}

interface LocatedControl {
  locator?: BrowserLocator;
  problem?: "missing" | "ambiguous";
}

interface FieldRule {
  key: string;
  aliases: string[];
}

const OFFICIAL_HOSTS = new Set(["boards.greenhouse.io", "job-boards.greenhouse.io"]);

const APPLY_SELECTORS = [
  "#apply_button",
  "a#apply_button",
  "a[href='#app']",
  "a[href$='/apply']",
  "[data-testid='apply-button']",
];

const SUBMIT_SELECTORS = [
  "#submit_app",
  "button#submit_app",
  "input#submit_app",
  "form#application_form button[type='submit']",
  "form[action*='greenhouse.io'] button[type='submit']",
  "button[data-testid='submit-application']",
  "button[type='submit']:has-text('Submit Application')",
  "input[type='submit'][value*='Submit']",
];

const SUBMIT_IDENTITY_ATTRIBUTES = [
  "id",
  "name",
  "type",
  "value",
  "data-testid",
  "data-qa",
  "formaction",
  "aria-label",
  "disabled",
  "aria-disabled",
] as const;

const EMBEDDED_MARKERS: Array<{ selector: string; weight: number; providerBound?: boolean }> = [
  { selector: "iframe[src*='greenhouse.io/embed/job_app']", weight: 3, providerBound: true },
  { selector: "form[action*='greenhouse.io']", weight: 3, providerBound: true },
  { selector: "#application_form", weight: 1 },
  { selector: "#submit_app", weight: 1 },
  { selector: "input[name^='job_application[']", weight: 1 },
  { selector: "[data-greenhouse-job-id]", weight: 2 },
];

const FIELD_RULES: FieldRule[] = [
  { key: "first_name", aliases: ["first name", "given name"] },
  { key: "last_name", aliases: ["last name", "family name", "surname"] },
  { key: "full_name", aliases: ["full name", "legal name", "candidate name"] },
  { key: "email", aliases: ["email", "email address"] },
  { key: "phone", aliases: ["phone", "phone number", "mobile", "mobile number"] },
  { key: "location", aliases: ["location", "current location", "city"] },
  { key: "address", aliases: ["address", "street address"] },
  { key: "linkedin_url", aliases: ["linkedin", "linkedin url", "linkedin profile"] },
  { key: "github_url", aliases: ["github", "github url", "github profile"] },
  { key: "portfolio_url", aliases: ["portfolio", "portfolio url", "personal website", "website"] },
  { key: "current_company", aliases: ["current company", "current employer"] },
  { key: "current_title", aliases: ["current title", "current role", "current job title"] },
];

const SENSITIVE_PATTERNS = [
  /\bgender\b/i,
  /\brace\b/i,
  /\bethnic/i,
  /\bdisabilit/i,
  /\bveteran\b/i,
  /sexual orientation/i,
  /\breligion\b/i,
  /date of birth|birth date|\bdob\b/i,
  /social security|\bssn\b/i,
];

const ELIGIBILITY_REVIEW_PATTERNS = [
  /work authori[sz]ation|legally authori[sz]ed|authori[sz]ed to work|legally able to work|right to work/i,
  /sponsorship|immigration|visa status|require a visa|need a visa/i,
];

const CONFIRMATION_PATTERNS = [
  /^(?:thank you|thanks) for applying\b/i,
  /^your application (?:has been|was) (?:successfully )?(?:submitted|received)\b/i,
  /^application (?:has been |was )?(?:successfully )?(?:submitted|received)\b/i,
  /^we(?:'ve| have) received your application\b/i,
];

const VISIBLE_FORM_ERROR_PATTERNS = [
  /please (?:complete|fill (?:out )?)\s+all required fields/i,
  /please enter a valid email(?: address)?/i,
  /there (?:was|were) (?:an error|errors) (?:with|submitting) your application/i,
  /correct the highlighted fields/i,
  /unable to submit (?:the |your )?application/i,
  /could not submit (?:the |your )?application/i,
];

const CLOSED_PATTERNS = [
  /this (?:job|position|posting) is no longer available/i,
  /this (?:job|position|posting) (?:has been|is) closed/i,
  /no longer accepting applications/i,
  /position has been filled/i,
];

const CHALLENGES: Array<{
  kind: "captcha" | "two_factor" | "assessment";
  title: string;
  detail: string;
  patterns: RegExp[];
  selectors: string[];
}> = [
  {
    kind: "captcha",
    title: "Complete the Greenhouse security check",
    detail: "Take over the preserved browser and complete the check. Bluey will not bypass it.",
    patterns: [/verify you are human/i, /complete (?:the )?captcha/i, /captcha (?:is )?required/i, /i(?:'| a)m not a robot/i],
    selectors: ["iframe[src*='recaptcha']", "iframe[src*='hcaptcha']", ".g-recaptcha", ".h-captcha"],
  },
  {
    kind: "two_factor",
    title: "Complete Greenhouse verification",
    detail: "Take over the preserved browser and complete the verification step.",
    patterns: [/verification code/i, /two[ -]?factor/i, /authenticator code/i, /one[ -]?time (?:code|passcode)/i],
    selectors: ["input[autocomplete='one-time-code']"],
  },
  {
    kind: "assessment",
    title: "Complete the employer assessment",
    detail: "This application requires an assessment. Take over the preserved browser to continue.",
    patterns: [
      /assessment (?:is )?required/i,
      /complete (?:this|the|a) assessment/i,
      /complete (?:the )?required assessment/i,
      /skills test (?:is )?required/i,
      /complete (?:the )?(?:required )?skills test/i,
    ],
    selectors: ["iframe[src*='assessment']", "[data-testid='assessment']"],
  },
];

export function detectGreenhouseUrl(rawUrl: URL | string): GreenhouseDetection | undefined {
  let url: URL;
  try {
    url = typeof rawUrl === "string" ? new URL(rawUrl) : rawUrl;
  } catch {
    return undefined;
  }
  if (url.protocol !== "https:" || !OFFICIAL_HOSTS.has(url.hostname.toLowerCase())) return undefined;

  const embedded = /^\/embed\/(?:job_app|job_board)\b/i.test(url.pathname);
  return {
    variant: embedded ? "embedded" : "public",
    source: "official_url",
    tenant: tenantFromUrl(url, embedded),
  };
}

export async function detectGreenhouseVariant(page: BrowserPage): Promise<GreenhouseDetection | undefined> {
  const official = detectGreenhouseUrl(page.url());
  if (official) return official;

  let markerScore = 0;
  let providerBound = false;
  for (const marker of EMBEDDED_MARKERS) {
    if (await selectorExists(page, marker.selector)) {
      markerScore += marker.weight;
      providerBound ||= marker.providerBound === true;
    }
  }
  const body = await page.bodyText().catch(() => "");
  if (/powered by greenhouse/i.test(body)) markerScore += 1;
  if (markerScore < 2 || !providerBound) return undefined;

  const url = safeUrl(page.url());
  return {
    variant: "embedded",
    source: "embedded_form",
    tenant: url?.searchParams.get("for") || undefined,
  };
}

export class GreenhouseApplicationStateMachine {
  private current: GreenhouseApplicationState = { name: "detect" };
  private readonly transitions: GreenhouseApplicationStateName[] = ["detect"];
  private readonly now: () => Date;
  private detection?: GreenhouseDetection;
  private reviewApproved: boolean;
  private pausedForFinalReview = false;
  private submitStarted = false;
  private fillExpectations: FormFillExpectation[] = [];

  constructor(
    private readonly context: AdapterContext,
    options: GreenhouseStateMachineOptions = {},
  ) {
    this.now = options.now ?? (() => new Date());
    this.reviewApproved = options.finalReviewApproved ?? false;
  }

  get state(): Readonly<GreenhouseApplicationState> {
    return this.current;
  }

  get history(): readonly GreenhouseApplicationStateName[] {
    return this.transitions;
  }

  getReceipt(): SubmissionReceipt | undefined {
    return this.current.name === "receipt" ? this.current.receipt : undefined;
  }

  async detect(): Promise<GreenhouseDetection | undefined> {
    if (this.current.name !== "detect") return this.detection;
    this.detection = await detectGreenhouseVariant(this.context.page);
    if (!this.detection) {
      await this.finish(failedReceipt("application", "This page is not a recognized Greenhouse application form."));
      return undefined;
    }
    await this.move({ name: "prepare", detection: this.detection });
    return this.detection;
  }

  async prepare(): Promise<void> {
    if (this.current.name === "receipt") return;
    this.expectState("prepare");
    await this.context.page.installExactSubmitGuard(
      "greenhouse",
      this.context.approvedCanonicalUrl,
    );
    assertApprovedProviderJobOrConfirmation(
      "greenhouse",
      this.context.approvedCanonicalUrl,
      this.context.page.url(),
    );
    const challenge = await detectChallenge(this.context.page);
    if (challenge) {
      await this.finish(interventionReceipt(challenge));
      return;
    }
    if (await isClosedPosting(this.context.page)) {
      await this.finish(closedPostingReceipt());
      return;
    }

    let controls = await this.readControls();
    if (!controls) return;
    let formOpened = false;
    if (!hasActionableControls(controls)) {
      const apply = await locateUniqueVisible(this.context.page, APPLY_SELECTORS);
      if (!apply.locator) {
        if (apply.problem === "ambiguous") {
          await this.finish(ambiguousControlReceipt("Apply", this.context.page.url()));
          return;
        }
        const detail = this.detection?.variant === "embedded"
          ? "The embedded Greenhouse application form is not available in the preserved page."
          : "Bluey could not find the Greenhouse application form or its Apply control.";
        await this.finish(failedReceipt("application", detail));
        return;
      }
      await apply.locator.click();
      formOpened = true;
      await this.context.page.waitForSettled();

      const afterOpenChallenge = await detectChallenge(this.context.page);
      if (afterOpenChallenge) {
        await this.finish(interventionReceipt(afterOpenChallenge));
        return;
      }
      if (await isClosedPosting(this.context.page)) {
        await this.finish(closedPostingReceipt());
        return;
      }
      controls = await this.readControls();
      if (!controls) return;
      if (!hasActionableControls(controls)) {
        await this.finish(failedReceipt("application", "The Greenhouse Apply control did not expose an application form."));
        return;
      }
    }

    assertApprovedProviderJob(
      "greenhouse",
      this.context.approvedCanonicalUrl,
      this.context.page.url(),
    );
    await this.context.page.beginExactSubmitGuard();
    await this.move({ name: "fill", detection: this.requireDetection(), formOpened });
  }

  async fill(): Promise<void> {
    if (this.current.name === "receipt") return;
    this.expectState("fill");
    await this.context.page.installExactSubmitGuard(
      "greenhouse",
      this.context.approvedCanonicalUrl,
    );
    const challenge = await detectChallenge(this.context.page);
    if (challenge) {
      await this.finish(interventionReceipt(challenge));
      return;
    }
    if (await isClosedPosting(this.context.page)) {
      await this.finish(closedPostingReceipt());
      return;
    }

    assertApprovedProviderJob(
      "greenhouse",
      this.context.approvedCanonicalUrl,
      this.context.page.url(),
    );
    await this.context.page.beginExactSubmitGuard();

    const controls = await this.readControls();
    if (!controls) return;
    this.fillExpectations = [];
    let filledFieldCount = 0;
    const handledRadioGroups = new Set<string>();
    for (const control of controls) {
      if (control.kind === "hidden" || control.kind === "other") continue;
      if (manualReviewKind(control)) continue;

      if (control.kind === "file") {
        const path = documentPath(control, this.context.packet);
        if (path) {
          let selectedFiles = control.files ?? [];
          if (!hasControlValue(control)) {
            selectedFiles = await this.context.page.locator(control.selector).setInputFiles([path]);
            filledFieldCount += 1;
          }
          this.fillExpectations.push(fileExpectation(
            control,
            displayField(control),
            path,
            selectedFiles,
          ));
        }
        continue;
      }

      const answer = answerFor(control, this.context.packet);
      if (answer === undefined || answer.trim() === "") continue;
      const locator = this.context.page.locator(control.selector);
      if (control.kind === "select") {
        const option = bestOption(control, answer);
        if (option !== undefined) {
          this.fillExpectations.push(valueExpectation(control, displayField(control), option));
          if (!hasControlValue(control)) {
            await locator.selectOption(option);
            filledFieldCount += 1;
          }
        }
      } else if (control.kind === "radio") {
        const group = control.name || control.selector;
        if (handledRadioGroups.has(group)) continue;
        handledRadioGroups.add(group);
        const groupControls = controls.filter((candidate) => candidate.kind === "radio"
          && (candidate.name || candidate.selector) === group);
        const matchingControl = groupControls.find((candidate) => radioMatches(candidate, answer));
        if (!matchingControl) continue;
        for (const candidate of groupControls) {
          this.fillExpectations.push(checkedExpectation(
            candidate,
            displayField(candidate),
            candidate.selector === matchingControl.selector,
          ));
        }
        if (!groupControls.some((candidate) => candidate.checked)) {
          await this.context.page.locator(matchingControl.selector).setChecked(true);
          filledFieldCount += 1;
        }
      } else if (control.kind === "checkbox") {
        const checked = booleanAnswer(answer);
        this.fillExpectations.push(checkedExpectation(control, displayField(control), checked));
        if (Boolean(control.checked) !== checked && !hasControlValue(control)) {
          await locator.setChecked(checked);
          filledFieldCount += 1;
        }
      } else {
        this.fillExpectations.push(valueExpectation(control, displayField(control), answer));
        if (!hasControlValue(control)) {
          await locator.fill(answer);
          filledFieldCount += 1;
        }
      }
    }
    Object.freeze(this.fillExpectations);

    await this.context.log("greenhouse_fields_filled", {
      variant: this.requireDetection().variant,
      count: filledFieldCount,
      capability: GREENHOUSE_ADAPTER_PROFILE.capability,
    });
    await this.move({
      name: "validate",
      detection: this.requireDetection(),
      filledFieldCount,
    });
  }

  async validate(): Promise<ValidationIssue[]> {
    if (this.current.name === "receipt") return this.current.receipt.issues;
    if (this.current.name === "submit") return [];
    this.expectState("validate");
    const challenge = await detectChallenge(this.context.page);
    if (challenge) {
      await this.finish(interventionReceipt(challenge));
      return [];
    }
    if (await isClosedPosting(this.context.page)) {
      await this.finish(closedPostingReceipt());
      return this.requireReceipt().issues;
    }

    const controls = await this.readControls();
    if (!controls) return this.getReceipt()?.issues ?? [];
    const records = combinedValidationRecords(
      this.fillExpectations,
      controls,
      this.context.packet,
    );
    if (records.length > 0) {
      await this.finish(validationReceipt(records));
      return records.map((record) => record.issue);
    }

    await this.move({
      name: "submit",
      detection: this.requireDetection(),
      reviewRequired: true,
      reviewApproved: this.reviewApproved,
    });
    return [];
  }

  approveFinalReview(): void {
    this.reviewApproved = true;
    if (this.current.name === "submit") {
      this.current = { ...this.current, reviewApproved: true };
      return;
    }
    if (this.current.name === "receipt" && this.pausedForFinalReview) {
      this.pausedForFinalReview = false;
      this.current = {
        name: "submit",
        detection: this.requireDetection(),
        reviewRequired: true,
        reviewApproved: true,
      };
      this.transitions.push("submit");
    }
  }

  async submit(): Promise<SubmissionReceipt> {
    if (this.current.name === "receipt") return this.current.receipt;
    this.expectState("submit");

    if (this.submitStarted) {
      return this.finish(uncertainSubmissionReceipt(this.context.page.url()));
    }

    const challenge = await detectChallenge(this.context.page);
    if (challenge) return this.finish(interventionReceipt(challenge));
    if (await isClosedPosting(this.context.page)) return this.finish(closedPostingReceipt());

    if (!this.reviewApproved) {
      this.pausedForFinalReview = true;
      return this.finish(interventionReceipt({
        kind: "browser_takeover",
        title: "Review the Greenhouse application",
        detail: "Review every employer-facing field and document in the preserved form, then approve submission.",
        takeoverUrl: this.context.page.url(),
        resolution: { kind: "browser_takeover", resumeAfter: true },
      }));
    }

    const controls = await this.readControls();
    if (!controls) return this.requireReceipt();
    const records = combinedValidationRecords(
      this.fillExpectations,
      controls,
      this.context.packet,
    );
    if (records.length > 0) return this.finish(validationReceipt(records));
    const authorizedFileEvidence = exactSubmitFileEvidence(this.fillExpectations, controls);
    const trustedFieldValues = exactSubmitTrustedFieldValues(this.fillExpectations);

    const submit = await locateUniqueVisible(this.context.page, SUBMIT_SELECTORS);
    if (!submit.locator) {
      if (submit.problem === "ambiguous") {
        return this.finish(ambiguousControlReceipt("Submit", this.context.page.url()));
      }
      return this.finish(failedReceipt("application", "Bluey could not find the Greenhouse Submit Application control."));
    }
    const authorizedUrl = this.context.page.url();
    const authorizedSubmit = submit.locator;
    const authorizedSubmitIdentity = await submitControlIdentity(authorizedSubmit);
    const authorizedSubmitTarget = await captureEffectiveSubmitTarget(
      authorizedSubmit,
      "greenhouse",
      this.context.approvedCanonicalUrl,
      authorizedUrl,
    );
    const authorizedSubmitEvidence = await authorizedSubmit.successfulSubmitEvidence(
      trustedFieldValues,
      authorizedSubmitTarget.providerJobKey,
    );
    await this.context.page.assertExactSubmitGuardClean();

    // Fence the irreversible action before awaiting any browser-side work.
    if (!this.context.beforeFinalSubmit) {
      throw new Error("Greenhouse final submit authority is unavailable");
    }
    this.submitStarted = true;
    await this.context.beforeFinalSubmit({
      adapter: "greenhouse",
      adapterVersion: GREENHOUSE_ADAPTER_PROFILE.version,
      control: "greenhouse_submit_application",
      target: authorizedSubmitTarget,
      files: authorizedFileEvidence,
      fields: authorizedSubmitEvidence.fields,
      partOrder: authorizedSubmitEvidence.partOrder,
    });
    const finalSubmit = await this.verifyAuthorizedSubmitState(
      authorizedUrl,
      authorizedSubmit,
      authorizedSubmitIdentity,
      authorizedSubmitTarget,
      authorizedSubmitEvidence,
      trustedFieldValues,
    );
    let clickFailed = false;
    let submitHttpStatus: number | undefined;
    try {
      submitHttpStatus = await finalSubmit.clickWithExactSubmit({
        target: authorizedSubmitTarget,
        files: authorizedFileEvidence,
        fields: authorizedSubmitEvidence.fields,
        partOrder: authorizedSubmitEvidence.partOrder,
      });
    } catch (error) {
      if (error instanceof ExactSubmitEvidenceError) {
        throw new Error("Greenhouse submit evidence changed during activation");
      }
      clickFailed = true;
    }
    await this.context.afterFinalSubmit?.(clickFailed ? "activation_uncertain" : "activated");
    try {
      await this.context.page.waitForSettled();
    } catch {
      clickFailed = true;
    }

    const afterSubmitChallenge = await detectChallenge(this.context.page);
    if (afterSubmitChallenge) return this.finish(interventionReceipt(afterSubmitChallenge));

    const body = await this.context.page.bodyText().catch(() => "");
    const postSubmitControls = await this.context.page.controls().catch(() => []);
    const postSubmitRecords = validationRecords(postSubmitControls, this.context.packet);
    if (postSubmitRecords.length > 0) return this.finish(validationReceipt(postSubmitRecords));
    const rerenderedSubmit = await locateUniqueVisible(this.context.page, SUBMIT_SELECTORS);
    if (rerenderedSubmit.locator
      || rerenderedSubmit.problem === "ambiguous"
      || VISIBLE_FORM_ERROR_PATTERNS.some((pattern) => pattern.test(body))) {
      return this.finish(postSubmitFormReceipt("Greenhouse", this.context.page.url()));
    }
    const confirmationText = confirmationEvidence(body);
    if (submitHttpStatus !== undefined
      && confirmationText
      && isApprovedProviderConfirmation(
      "greenhouse",
      this.context.approvedCanonicalUrl,
      this.context.page.url(),
      authorizedSubmitTarget.providerJobKey,
    )) {
      return this.finish({
        status: "submitted",
        submitHttpStatus,
        confirmationText,
        confirmationUrl: this.context.page.url(),
        submittedAt: this.now().toISOString(),
        issues: [],
      });
    }

    return this.finish(uncertainSubmissionReceipt(this.context.page.url(), clickFailed));
  }

  private async readControls(): Promise<FormControl[] | undefined> {
    try {
      return await this.context.page.controls();
    } catch {
      await this.finish(failedReceipt("application", "Bluey could not inspect the Greenhouse application controls."));
      return undefined;
    }
  }

  private async verifyAuthorizedSubmitState(
    authorizedUrl: string,
    authorizedSubmit: BrowserLocator,
    authorizedSubmitIdentity: string,
    authorizedSubmitTarget: EffectiveSubmitTargetIdentity,
    authorizedSubmitEvidence: Readonly<ExactSubmitFormEvidence>,
    trustedFieldValues: ReturnType<typeof exactSubmitTrustedFieldValues>,
  ): Promise<BrowserLocator> {
    if (this.context.page.url() !== authorizedUrl) {
      throw new Error("Greenhouse page changed after final submit authority");
    }
    if (await detectChallenge(this.context.page) || await isClosedPosting(this.context.page)) {
      throw new Error("Greenhouse application changed after final submit authority");
    }

    let controls: FormControl[];
    try {
      controls = await this.context.page.controls();
    } catch {
      throw new Error("Greenhouse controls changed after final submit authority");
    }
    if (combinedValidationRecords(this.fillExpectations, controls, this.context.packet).length > 0) {
      throw new Error("Greenhouse fields changed after final submit authority");
    }

    const originalCount = await authorizedSubmit.count().catch(() => 0);
    const originalVisible = originalCount === 1
      && await authorizedSubmit.isVisible().catch(() => false);
    if (!originalVisible) {
      throw new Error("Greenhouse submit control changed after final submit authority");
    }

    const currentSubmit = await locateUniqueVisible(this.context.page, SUBMIT_SELECTORS);
    if (!currentSubmit.locator) {
      throw new Error("Greenhouse submit control changed after final submit authority");
    }
    const currentSubmitIdentity = await submitControlIdentity(currentSubmit.locator);
    let currentSubmitTarget: EffectiveSubmitTargetIdentity;
    try {
      currentSubmitTarget = await captureEffectiveSubmitTarget(
        currentSubmit.locator,
        "greenhouse",
        this.context.approvedCanonicalUrl,
        this.context.page.url(),
      );
    } catch {
      throw new Error("Greenhouse submit target changed after final submit authority");
    }
    if (currentSubmitIdentity !== authorizedSubmitIdentity) {
      throw new Error("Greenhouse submit control changed after final submit authority");
    }
    if (!sameEffectiveSubmitTarget(currentSubmitTarget, authorizedSubmitTarget)) {
      throw new Error("Greenhouse submit target changed after final submit authority");
    }
    const currentSubmitEvidence = await currentSubmit.locator.successfulSubmitEvidence(
      trustedFieldValues,
      currentSubmitTarget.providerJobKey,
    );
    if (!sameExactSubmitFields(
      currentSubmitEvidence.fields,
      authorizedSubmitEvidence.fields,
    ) || !sameExactSubmitPartOrder(
      currentSubmitEvidence.partOrder,
      authorizedSubmitEvidence.partOrder,
    )) {
      throw new Error("Greenhouse submit fields changed after final submit authority");
    }
    await this.context.page.assertExactSubmitGuardClean();
    if (this.context.page.url() !== authorizedUrl) {
      throw new Error("Greenhouse page changed after final submit authority");
    }
    return currentSubmit.locator;
  }

  private expectState(expected: GreenhouseApplicationStateName): void {
    if (this.current.name !== expected) {
      throw new Error(`Greenhouse state machine expected ${expected}, received ${this.current.name}`);
    }
  }

  private requireDetection(): GreenhouseDetection {
    if (!this.detection) throw new Error("Greenhouse application has not been detected");
    return this.detection;
  }

  private requireReceipt(): SubmissionReceipt {
    const receipt = this.getReceipt();
    if (!receipt) throw new Error("Greenhouse receipt is not available");
    return receipt;
  }

  private async move(state: GreenhouseApplicationState): Promise<void> {
    this.current = state;
    this.transitions.push(state.name);
    await this.context.log("greenhouse_state_transition", {
      state: state.name,
      variant: this.detection?.variant,
      capability: GREENHOUSE_ADAPTER_PROFILE.capability,
    });
  }

  private async finish(receipt: SubmissionReceipt): Promise<SubmissionReceipt> {
    this.current = { name: "receipt", detection: this.detection, receipt };
    this.transitions.push("receipt");
    await this.context.log("greenhouse_state_transition", {
      state: "receipt",
      variant: this.detection?.variant,
      status: receipt.status,
      capability: GREENHOUSE_ADAPTER_PROFILE.capability,
    });
    return receipt;
  }
}

export class GreenhouseAdapter implements ApplicationAdapter {
  readonly kind = GREENHOUSE_ADAPTER_PROFILE.kind;
  readonly version = GREENHOUSE_ADAPTER_PROFILE.version;
  readonly profile = GREENHOUSE_ADAPTER_PROFILE;
  private readonly machines = new WeakMap<AdapterContext, GreenhouseApplicationStateMachine>();

  constructor(private readonly options: GreenhouseAdapterOptions = {}) {}

  detect(url: URL): boolean {
    return detectGreenhouseUrl(url) !== undefined;
  }

  async normalize(page: BrowserPage): Promise<NormalizedJob> {
    const url = safeUrl(page.url());
    const pageTitle = await page.title();
    const parsed = parseGreenhouseTitle(pageTitle);
    return {
      externalId: greenhouseJobId(url) ?? page.url(),
      canonicalUrl: page.url(),
      company: parsed.company,
      title: parsed.title,
      location: "",
      workplace: "unknown",
      description: await page.bodyText(),
      source: "greenhouse",
    };
  }

  async prepare(context: AdapterContext): Promise<void> {
    const machine = this.machineFor(context);
    await machine.detect();
    await machine.prepare();
  }

  async fill(context: AdapterContext): Promise<void> {
    await this.machineFor(context).fill();
  }

  async validate(context: AdapterContext): Promise<ValidationIssue[]> {
    return this.machineFor(context).validate();
  }

  async submit(context: AdapterContext): Promise<SubmissionReceipt> {
    const machine = this.machineFor(context);
    if (await this.options.finalReviewApproval?.(context)) machine.approveFinalReview();
    return machine.submit();
  }

  stateFor(context: AdapterContext): Readonly<GreenhouseApplicationState> {
    return this.machineFor(context).state;
  }

  private machineFor(context: AdapterContext): GreenhouseApplicationStateMachine {
    let machine = this.machines.get(context);
    if (!machine) {
      machine = new GreenhouseApplicationStateMachine(context, { now: this.options.now });
      this.machines.set(context, machine);
    }
    return machine;
  }
}

export function createGreenhouseAdapter(options: GreenhouseAdapterOptions = {}): GreenhouseAdapter {
  return new GreenhouseAdapter(options);
}

function tenantFromUrl(url: URL, embedded: boolean): string | undefined {
  const queryTenant = url.searchParams.get("for");
  if (queryTenant) return queryTenant;
  if (embedded) return undefined;
  const [first] = url.pathname.split("/").filter(Boolean);
  return first && first !== "jobs" ? first : undefined;
}

function safeUrl(value: string): URL | undefined {
  try {
    return new URL(value);
  } catch {
    return undefined;
  }
}

function greenhouseJobId(url: URL | undefined): string | undefined {
  if (!url) return undefined;
  const path = url.pathname.split("/").filter(Boolean);
  const jobsIndex = path.findIndex((part) => part.toLowerCase() === "jobs");
  return url.searchParams.get("gh_jid")
    ?? url.searchParams.get("token")
    ?? (jobsIndex >= 0 ? path[jobsIndex + 1] : undefined);
}

function parseGreenhouseTitle(value: string): { title: string; company: string } {
  const application = value.match(/^\s*Job Application for\s+(.+?)\s+at\s+(.+?)\s*$/i);
  if (application) return { title: application[1]!, company: application[2]! };
  const parts = value.split(/[|\-]/).map((part) => part.trim()).filter(Boolean);
  return { title: parts[0] || "Open role", company: parts.at(-1) || "Employer" };
}

async function selectorExists(page: BrowserPage, selector: string): Promise<boolean> {
  try {
    return (await page.locator(selector).count()) > 0;
  } catch {
    return false;
  }
}

async function locateUniqueVisible(page: BrowserPage, selectors: string[]): Promise<LocatedControl> {
  // A selector union lets Playwright de-duplicate one element that matches more
  // than one provider selector while still exposing two distinct controls.
  try {
    const locator = page.locator(selectors.map((selector) => `${selector}:visible`).join(", "));
    const count = await locator.count();
    if (count > 1) return { problem: "ambiguous" };
    if (count === 1 && await locator.isVisible()) return { locator };
  } catch {
    // Lightweight page implementations may not support selector unions.
  }

  const matches: BrowserLocator[] = [];
  for (const selector of selectors) {
    try {
      const locator = page.locator(selector);
      const count = await locator.count();
      if (count > 1) return { problem: "ambiguous" };
      if (count === 1 && await locator.isVisible()) matches.push(locator);
    } catch {
      // Try the next provider-specific selector.
    }
  }
  if (matches.length > 1) return { problem: "ambiguous" };
  return matches[0] ? { locator: matches[0] } : { problem: "missing" };
}

async function submitControlIdentity(locator: BrowserLocator): Promise<string> {
  const [text, ...attributes] = await Promise.all([
    locator.textContent(),
    ...SUBMIT_IDENTITY_ATTRIBUTES.map((attribute) => locator.getAttribute(attribute)),
  ]);
  return JSON.stringify([
    (text ?? "").replace(/\s+/g, " ").trim(),
    ...attributes,
  ]);
}

async function detectChallenge(page: BrowserPage): Promise<InterventionRequest | undefined> {
  const body = await page.bodyText().catch(() => "");
  for (const challenge of CHALLENGES) {
    const textMatch = challenge.patterns.some((pattern) => pattern.test(body));
    let selectorMatch = false;
    if (!textMatch) {
      for (const selector of challenge.selectors) {
        try {
          const locator = page.locator(selector);
          if ((await locator.count()) > 0 && await locator.isVisible()) {
            selectorMatch = true;
            break;
          }
        } catch {
          // A missing challenge marker is expected on normal forms.
        }
      }
    }
    if (textMatch || selectorMatch) {
      return {
        kind: challenge.kind,
        title: challenge.title,
        detail: challenge.detail,
        takeoverUrl: page.url(),
        resolution: { kind: "browser_takeover", resumeAfter: true },
      };
    }
  }
  return undefined;
}

async function isClosedPosting(page: BrowserPage): Promise<boolean> {
  const body = await page.bodyText().catch(() => "");
  return CLOSED_PATTERNS.some((pattern) => pattern.test(body));
}

function hasActionableControls(controls: FormControl[]): boolean {
  return controls.some((control) => control.kind !== "hidden");
}

function searchableField(control: FormControl): string {
  return [control.label, control.name, control.placeholder].filter(Boolean).join(" ").trim();
}

function displayField(control: FormControl): string {
  return control.label.trim() || control.name.trim() || control.selector;
}

function normalize(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}

function descriptors(control: FormControl): string[] {
  return [control.label, control.name, control.placeholder]
    .map(normalize)
    .filter(Boolean);
}

function knownFieldRule(control: FormControl): FieldRule | undefined {
  const values = descriptors(control);
  return FIELD_RULES.find((rule) => rule.aliases.some((alias) => {
    const normalizedAlias = normalize(alias);
    return values.some((value) => value === normalizedAlias
      || value.endsWith(` ${normalizedAlias}`)
      || value.startsWith(`${normalizedAlias} `));
  }));
}

function explicitAnswer(control: FormControl, packet: ApplicationPacket): string | undefined {
  const values = new Set(descriptors(control));
  return Object.entries(packet.answers).find(([key]) => values.has(normalize(key)))?.[1];
}

function answerFor(control: FormControl, packet: ApplicationPacket): string | undefined {
  const explicit = explicitAnswer(control, packet);
  if (explicit !== undefined) return explicit;
  const rule = knownFieldRule(control);
  if (!rule) return undefined;
  const aliases = new Set([normalize(rule.key), ...rule.aliases.map(normalize)]);
  const answer = Object.entries(packet.answers).find(([key]) => aliases.has(normalize(key)))?.[1];
  if (answer !== undefined) return answer;
  return rule.key === "email" ? packet.applicationEmail : undefined;
}

function documentKind(control: FormControl): "resume" | "cover_letter" | undefined {
  const field = searchableField(control);
  if (/resume|curriculum|\bcv\b/i.test(field)) return "resume";
  if (/cover letter/i.test(field)) return "cover_letter";
  return undefined;
}

function documentPath(control: FormControl, packet: ApplicationPacket): string | undefined {
  const kind = documentKind(control);
  return kind === "resume" ? packet.resumePath : kind === "cover_letter" ? packet.coverLetterPath : undefined;
}

function manualReviewKind(control: FormControl): "sensitive" | "eligibility" | undefined {
  const field = searchableField(control);
  if (SENSITIVE_PATTERNS.some((pattern) => pattern.test(field))) return "sensitive";
  if (ELIGIBILITY_REVIEW_PATTERNS.some((pattern) => pattern.test(field))) return "eligibility";
  return undefined;
}

function hasControlValue(control: FormControl): boolean {
  if (control.kind === "checkbox" || control.kind === "radio") return Boolean(control.checked);
  if (control.kind === "file" && control.files !== undefined) return control.files.length > 0;
  return control.value.trim().length > 0;
}

function booleanAnswer(value: string): boolean {
  return /^(?:1|true|yes|y|on|checked)$/i.test(value.trim());
}

function bestOption(control: FormControl, answer: string): string | undefined {
  const normalizedAnswer = normalize(answer);
  const options = (control.options ?? []).filter((option) => option.value.trim() !== "");
  return options.find((option) => normalize(option.value) === normalizedAnswer)?.value
    ?? options.find((option) => normalize(option.label) === normalizedAnswer)?.value
    ?? options.find((option) => normalize(option.label).includes(normalizedAnswer))?.value;
}

function radioMatches(control: FormControl, answer: string): boolean {
  const normalizedAnswer = normalize(answer);
  return [control.value, control.label]
    .map(normalize)
    .filter(Boolean)
    .some((candidate) => candidate === normalizedAnswer
      || candidate.endsWith(` ${normalizedAnswer}`));
}

function validationRecords(controls: FormControl[], packet: ApplicationPacket): ValidationRecord[] {
  const records: ValidationRecord[] = [];
  const visitedRadioGroups = new Set<string>();

  for (const control of controls) {
    if (control.kind === "hidden") continue;
    const reviewKind = manualReviewKind(control);

    if (control.kind === "file" && hasControlValue(control) && !documentPath(control, packet)) {
      const field = displayField(control);
      records.push({
        issue: {
          field,
          message: `Greenhouse found an attachment in ${field} that is not part of the approved packet.`,
          severity: "blocking",
        },
        interventionKind: "unknown_question",
      });
      continue;
    }

    const answer = answerFor(control, packet);
    if (control.kind === "select"
      && answer !== undefined
      && answer.trim() !== ""
      && !bestOption(control, answer)) {
      const field = displayField(control);
      records.push({
        issue: {
          field,
          message: `Greenhouse did not accept the packet value for ${field}.`,
          severity: "blocking",
        },
        interventionKind: "missing_fact",
        choices: choicesFor(control, controls),
      });
      continue;
    }

    if (control.kind === "radio") {
      const group = control.name || control.selector;
      if (visitedRadioGroups.has(group)) continue;
      visitedRadioGroups.add(group);
      const groupControls = controls.filter((candidate) => candidate.kind === "radio"
        && (candidate.name || candidate.selector) === group);
      if (answer !== undefined
        && answer.trim() !== ""
        && !groupControls.some((candidate) => radioMatches(candidate, answer))) {
        const field = displayField(control);
        records.push({
          issue: {
            field,
            message: `Greenhouse did not accept the packet value for ${field}.`,
            severity: "blocking",
          },
          interventionKind: "missing_fact",
          choices: choicesFor(control, controls),
        });
        continue;
      }
      if (!control.required && !reviewKind) continue;
      if (groupControls.some((candidate) => candidate.checked)) continue;
    } else {
      if (!control.required && !reviewKind) continue;
      if (hasControlValue(control)) continue;
    }

    const field = displayField(control);
    const known = Boolean(knownFieldRule(control) || documentKind(control));
    const interventionKind = reviewKind === "sensitive"
      ? "sensitive_question"
      : reviewKind === "eligibility"
        ? "unknown_question"
        : known
          ? "missing_fact"
          : "unknown_question";
    const message = reviewKind
      ? `${field} requires your review; Bluey will not answer it automatically.`
      : answer === undefined && !documentPath(control, packet)
        ? `Greenhouse requires ${field}, but the packet has no confirmed answer.`
        : `Greenhouse did not accept the packet value for ${field}.`;
    records.push({
      issue: { field, message, severity: "blocking" },
      interventionKind,
      choices: choicesFor(control, controls),
    });
  }
  return records;
}

function combinedValidationRecords(
  expectations: readonly FormFillExpectation[],
  controls: FormControl[],
  packet: ApplicationPacket,
): ValidationRecord[] {
  const readback = readbackValidationRecords(expectations, controls);
  const readbackFields = new Set(readback.map((record) => record.issue.field));
  return [
    ...readback,
    ...validationRecords(controls, packet)
      .filter((record) => !readbackFields.has(record.issue.field)),
  ];
}

function readbackValidationRecords(
  expectations: readonly FormFillExpectation[],
  controls: FormControl[],
): ValidationRecord[] {
  return verifyFillExpectations(expectations, controls, "Greenhouse")
    .map((issue) => ({
      issue,
      interventionKind: "missing_fact",
    }));
}

function choicesFor(control: FormControl, controls: FormControl[]): string[] | undefined {
  if (control.kind === "select") {
    const choices = (control.options ?? []).filter((option) => option.value.trim() !== "").map((option) => option.label);
    return choices.length ? choices : undefined;
  }
  if (control.kind === "radio") {
    const group = control.name || control.selector;
    const choices = controls
      .filter((candidate) => candidate.kind === "radio" && (candidate.name || candidate.selector) === group)
      .map((candidate) => candidate.value || candidate.label)
      .filter(Boolean);
    return choices.length ? choices : undefined;
  }
  return undefined;
}

function validationReceipt(records: ValidationRecord[]): SubmissionReceipt {
  const first = records[0]!;
  const titles = {
    missing_fact: "One Greenhouse field is missing",
    unknown_question: "A Greenhouse question needs your answer",
    sensitive_question: "A sensitive Greenhouse question needs you",
  } as const;
  return interventionReceipt({
    kind: first.interventionKind,
    title: titles[first.interventionKind],
    detail: first.issue.message,
    field: first.issue.field,
    choices: first.choices,
    resolution: { kind: "answer", resumeAfter: true },
  }, records.map((record) => record.issue));
}

function interventionReceipt(
  intervention: InterventionRequest,
  issues: ValidationIssue[] = [],
): SubmissionReceipt {
  return { status: "needs_input", issues, intervention };
}

function failedReceipt(field: string, message: string): SubmissionReceipt {
  return {
    status: "failed",
    issues: [{ field, message, severity: "blocking" }],
  };
}

function closedPostingReceipt(): SubmissionReceipt {
  return failedReceipt(
    "application",
    "Greenhouse reports that this posting is closed or no longer accepting applications.",
  );
}

function ambiguousControlReceipt(action: "Apply" | "Submit", url: string): SubmissionReceipt {
  const message = `Greenhouse exposed more than one provider-scoped ${action} control.`;
  return interventionReceipt({
    kind: "browser_takeover",
    title: `Review the Greenhouse ${action} controls`,
    detail: `${message} Bluey will not choose one automatically.`,
    takeoverUrl: url,
    resolution: { kind: "browser_takeover", resumeAfter: false },
  }, [{ field: "application", message, severity: "blocking" }]);
}

function uncertainSubmissionReceipt(url: string, clickFailed = false): SubmissionReceipt {
  return interventionReceipt({
    kind: "browser_takeover",
    title: "Confirm the Greenhouse application result",
    detail: clickFailed
      ? "The submit response was interrupted, so Bluey will not retry or claim success. Review the preserved browser."
      : "Greenhouse did not show explicit confirmation evidence. Review the preserved browser before any retry.",
    takeoverUrl: url,
    resolution: { kind: "browser_takeover", resumeAfter: false },
  }, [{
    field: "submission",
    message: "Greenhouse did not show explicit confirmation after the submit control was activated; do not retry automatically.",
    severity: "blocking",
  }]);
}

function postSubmitFormReceipt(provider: string, url: string): SubmissionReceipt {
  const message = `${provider} returned the application form or a validation error after Submit.`;
  return interventionReceipt({
    kind: "browser_takeover",
    title: `Review the ${provider} submission error`,
    detail: `${message} Bluey will not treat confirmation-looking text as success or retry.`,
    takeoverUrl: url,
    resolution: { kind: "browser_takeover", resumeAfter: false },
  }, [{ field: "submission", message, severity: "blocking" }]);
}

function confirmationEvidence(body: string): string | undefined {
  if (hasNegativeSubmissionOutcome(body)) return undefined;
  const lines = body.split(/\n+/).map((line) => line.replace(/\s+/g, " ").trim()).filter(Boolean);
  for (const line of lines) {
    const candidates = [line, ...(line.match(/[^.!?]+[.!?]?/g) ?? []).map((sentence) => sentence.trim())];
    const evidence = candidates.find((candidate) => CONFIRMATION_PATTERNS.some((pattern) => pattern.test(candidate)));
    if (evidence) return evidence.slice(0, 500);
  }
  return undefined;
}
