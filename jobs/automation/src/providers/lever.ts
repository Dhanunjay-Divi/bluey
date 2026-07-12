import type {
  AdapterContext,
  ApplicationAdapter,
  ApplicationPacket,
  BrowserLocator,
  BrowserPage,
  FormControl,
  InterventionRequest,
  NormalizedJob,
  SubmissionReceipt,
  ValidationIssue,
} from "../contracts.js";

export const LEVER_ADAPTER_VERSION = "2026.07.0-beta.1";

export const LEVER_ADAPTER_PROFILE = Object.freeze({
  kind: "lever",
  version: LEVER_ADAPTER_VERSION,
  maturity: "beta",
  capability: "beta_review",
  submissionMode: "review_only",
  certified: false,
  requiresFinalReview: true,
} as const);

export const LEVER_CAPABILITY = Object.freeze({
  provider: "lever",
  release: "beta",
  mode: "review_only",
  certified: false,
  requiresFinalReview: true,
} as const);

export const LEVER_STATES = ["detect", "prepare", "fill", "validate", "submit", "receipt"] as const;

export type LeverState = typeof LEVER_STATES[number];
export type LeverPageKind = "posting" | "application" | "confirmation" | "closed" | "unsupported";
export type LeverStateOutcome =
  | "recognized"
  | "unsupported"
  | "ready"
  | "completed"
  | "paused"
  | "blocked"
  | "review_required"
  | "click_started"
  | "submitted"
  | "uncertain"
  | "failed";

export interface LeverStateSnapshot {
  state: LeverState;
  outcome: LeverStateOutcome;
  pageKind: LeverPageKind;
  issueCount: number;
}

export interface LeverSubmitOptions {
  /** Set only after the account owner has reviewed the preserved form. */
  finalReviewApproved?: boolean;
}

export interface LeverAdapterOptions {
  finalReviewApproval?: (context: AdapterContext) => boolean | Promise<boolean>;
  now?: () => Date;
}

interface LeverSession {
  pageKind: LeverPageKind;
  prepared: boolean;
  submitClicked: boolean;
  operationalIssues: ValidationIssue[];
  validationIssues: ValidationIssue[];
  challenge?: InterventionRequest;
  current?: LeverStateSnapshot;
  history: LeverStateSnapshot[];
}

interface LocatedControl {
  locator?: BrowserLocator;
  problem?: "missing" | "ambiguous";
}

const LEVER_HOSTS = new Set(["jobs.lever.co", "jobs.eu.lever.co"]);
const APPLICATION_FORM = "#application-form";

const APPLY_SELECTORS = [
  "[data-qa='btn-apply-bottom'] a[data-qa='show-page-apply']",
  ".last-section-apply a.postings-btn[href$='/apply']",
  ".postings-btn-wrapper a.postings-btn[href$='/apply']",
];

// Never include a generic submit selector here. Lever also renders a hidden
// hCaptcha submit button, so the irreversible control must be provider-scoped.
const SUBMIT_SELECTORS = [
  "#application-form #btn-submit[data-qa='btn-submit']",
  "#application-form button#btn-submit.template-btn-submit",
  "#application-form .last-section-apply button.template-btn-submit[type='submit']",
];

const CONFIRMATION_PATTERNS = [
  /thank you for (?:submitting )?your application[.!]?/i,
  /thank you for applying[.!]?/i,
  /thanks for applying[.!]?/i,
  /your application (?:was|has been) (?:successfully )?submitted[.!]?/i,
  /your application has been received[.!]?/i,
  /we (?:have|'ve) received your application[.!]?/i,
];

const NON_CONFIRMATION_PATTERNS = [
  /already (?:applied|submitted (?:an|your) application)/i,
  /application (?:was|has been) already submitted/i,
  /unable to submit (?:the |your )?application/i,
  /could not submit (?:the |your )?application/i,
];

const CLOSED_PATTERNS = [
  /this (?:job|position|posting) is no longer available/i,
  /this (?:job|position|posting) (?:has been|is) closed/i,
  /no longer accepting applications/i,
  /position has been filled/i,
];

const VISIBLE_FORM_ERROR_PATTERNS = [
  /please (?:complete|fill (?:out )?)all required fields/i,
  /please enter a valid email(?: address)?/i,
  /there (?:was|were) (?:an error|errors) (?:with|submitting) your application/i,
  /correct the highlighted fields/i,
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

const AUTHORIZATION_PATTERNS = [
  /work authori[sz]ation/i,
  /authori[sz]ed to work/i,
  /legally able to work/i,
  /right to work/i,
  /visa sponsorship|require sponsorship|need sponsorship|immigration support|visa status/i,
];

const FIELD_ALIASES: Record<string, string[]> = {
  full_name: ["full name", "candidate name", "name"],
  first_name: ["first name", "given name"],
  last_name: ["last name", "family name", "surname"],
  email: ["email", "email address"],
  phone: ["phone", "phone number", "mobile", "mobile number"],
  location: ["current location", "preferred location", "location", "city"],
  current_company: ["current company", "current employer", "company", "organization", "org"],
  linkedin_url: ["linkedin url", "linkedin profile", "linkedin"],
  twitter_url: ["twitter url", "twitter profile", "twitter"],
  github_url: ["github url", "github profile", "github"],
  portfolio_url: ["portfolio url", "portfolio", "personal website"],
  other_website: ["other website", "website"],
};

const CHALLENGE_DEFINITIONS: Array<{
  kind: "captcha" | "two_factor" | "assessment";
  bodyPatterns: RegExp[];
  visibleSelectors: string[];
  title: string;
  detail: string;
}> = [
  {
    kind: "captcha",
    bodyPatterns: [
      /verify you are human/i,
      /complete (?:the )?(?:captcha|security check)/i,
      /hcaptcha challenge/i,
    ],
    visibleSelectors: [
      "iframe[src*='hcaptcha.com'][title*='challenge']",
      ".h-captcha iframe[title*='challenge']",
      "[data-qa='captcha-challenge']",
    ],
    title: "Complete CAPTCHA",
    detail: "Take over the preserved Lever page, complete the check, then let Bluey resume.",
  },
  {
    kind: "two_factor",
    bodyPatterns: [
      /enter (?:the )?(?:verification|security|one-time) code/i,
      /enter (?:the )?one-time verification code/i,
      /two-factor authentication is required/i,
      /approve (?:this )?sign-in/i,
    ],
    visibleSelectors: ["[data-qa='two-factor-challenge']", "input[autocomplete='one-time-code']"],
    title: "Verify your sign-in",
    detail: "Complete this verification in the preserved browser, then return to the application.",
  },
  {
    kind: "assessment",
    bodyPatterns: [
      /\b(?:complete|start) (?:the |this |your )?(?:required )?(?:assessment|skills test)(?: to continue| now)?\b/i,
      /\b(?:an |the )?assessment is required to continue\b/i,
    ],
    visibleSelectors: ["[data-qa='assessment-challenge']"],
    title: "Assessment needs you",
    detail: "Take over for this assessment. Bluey will not answer or bypass it.",
  },
];

export class LeverApplicationStateMachine implements ApplicationAdapter {
  readonly kind = "lever" as const;
  readonly version = LEVER_ADAPTER_VERSION;
  readonly profile = LEVER_ADAPTER_PROFILE;
  readonly capability = LEVER_CAPABILITY;

  private readonly sessions = new WeakMap<AdapterContext, LeverSession>();

  constructor(private readonly options: LeverAdapterOptions = {}) {}

  detect(url: URL): boolean {
    return url.protocol === "https:" && LEVER_HOSTS.has(url.hostname.toLowerCase());
  }

  async normalize(page: BrowserPage): Promise<NormalizedJob> {
    const url = new URL(page.url());
    const parts = url.pathname.split("/").filter(Boolean);
    if (parts.at(-1)?.toLowerCase() === "apply") parts.pop();
    const titleText = await page.title();
    const titleParts = titleText.split(/\s+-\s+/).map((value) => value.trim()).filter(Boolean);
    const company = titleParts.length > 1 ? titleParts.shift()! : "Employer";
    const canonical = new URL(url.toString());
    canonical.pathname = `/${parts.join("/")}`;
    canonical.search = "";
    canonical.hash = "";

    return {
      externalId: parts.at(-1) || canonical.toString(),
      canonicalUrl: canonical.toString(),
      company,
      title: titleParts.join(" - ") || titleText || "Open role",
      location: "",
      workplace: "unknown",
      description: await page.bodyText(),
      source: "lever",
    };
  }

  currentState(context: AdapterContext): LeverStateSnapshot | undefined {
    const current = this.sessions.get(context)?.current;
    return current ? { ...current } : undefined;
  }

  stateHistory(context: AdapterContext): LeverStateSnapshot[] {
    return (this.sessions.get(context)?.history ?? []).map((state) => ({ ...state }));
  }

  async prepare(context: AdapterContext): Promise<void> {
    const session = this.startSession(context);
    session.pageKind = await recognizePage(context.page, this.detect.bind(this));
    await this.transition(
      context,
      session,
      "detect",
      session.pageKind === "unsupported" ? "unsupported" : "recognized",
    );

    if (session.pageKind === "unsupported") {
      addIssue(session.operationalIssues, {
        field: "application",
        message: "This page is not a recognized Lever-hosted posting or application.",
        severity: "blocking",
      });
      await this.transition(context, session, "prepare", "blocked");
      return;
    }

    const challenge = await detectChallenge(context.page);
    if (challenge) {
      session.challenge = challenge;
      await this.transition(context, session, "prepare", "paused");
      return;
    }

    if (session.pageKind === "closed") {
      addIssue(session.operationalIssues, {
        field: "application",
        message: "Lever reports that this posting is closed or no longer accepting applications.",
        severity: "blocking",
      });
      await this.transition(context, session, "prepare", "blocked");
      return;
    }

    if (session.pageKind === "confirmation") {
      session.prepared = true;
      await this.transition(context, session, "prepare", "completed");
      return;
    }

    if (session.pageKind === "posting") {
      const apply = await locateUniqueVisible(context.page, APPLY_SELECTORS);
      if (!apply.locator) {
        addIssue(session.operationalIssues, {
          field: "application",
          message: apply.problem === "ambiguous"
            ? "Lever exposed more than one provider-scoped Apply control."
            : "Bluey could not find a provider-scoped Lever Apply control.",
          severity: "blocking",
        });
        await this.transition(context, session, "prepare", "blocked");
        return;
      }

      try {
        await apply.locator.click();
        await context.page.waitForSettled();
      } catch {
        addIssue(session.operationalIssues, {
          field: "application",
          message: "The Lever application form did not open cleanly.",
          severity: "blocking",
        });
        await this.transition(context, session, "prepare", "failed");
        return;
      }

      if (!safeUrlMatches(context.page.url(), this.detect.bind(this))) {
        session.pageKind = "unsupported";
        addIssue(session.operationalIssues, {
          field: "application",
          message: "The Apply control left the supported Lever-hosted application surface.",
          severity: "blocking",
        });
        await this.transition(context, session, "prepare", "blocked");
        return;
      }

      session.pageKind = await recognizePage(context.page, this.detect.bind(this));
      const afterNavigationChallenge = await detectChallenge(context.page);
      if (afterNavigationChallenge) {
        session.challenge = afterNavigationChallenge;
        await this.transition(context, session, "prepare", "paused");
        return;
      }
    }

    if (session.pageKind === "confirmation") {
      session.prepared = true;
      await this.transition(context, session, "prepare", "completed");
      return;
    }

    if (session.pageKind !== "application" || !(await this.hasUniqueApplicationForm(context.page, session))) {
      await this.transition(context, session, "prepare", "blocked");
      return;
    }

    session.prepared = true;
    await this.transition(context, session, "prepare", "ready");
  }

  async fill(context: AdapterContext): Promise<void> {
    const session = await this.ensureSession(context);
    const challenge = await detectChallenge(context.page);
    if (challenge) {
      session.challenge = challenge;
      await this.transition(context, session, "fill", "paused");
      return;
    }

    if (session.pageKind === "confirmation") {
      await this.transition(context, session, "fill", "completed");
      return;
    }

    if (!session.prepared || session.pageKind !== "application") {
      addIssue(session.operationalIssues, {
        field: "application",
        message: "The Lever application form is not ready for filling.",
        severity: "blocking",
      });
      await this.transition(context, session, "fill", "blocked");
      return;
    }

    const controls = await leverControls(context.page);
    let filled = 0;
    for (const control of controls) {
      if (control.kind === "hidden" || control.kind === "other" || isGuardedField(control)) continue;
      const locator = context.page.locator(control.selector);

      try {
        if (control.kind === "file") {
          const path = attachmentPath(control, context.packet);
          if (!path) continue;
          await locator.setInputFiles([path]);
          filled += 1;
          continue;
        }

        const answer = answerFor(control, context.packet);
        if (answer === undefined || answer.trim() === "") continue;

        if (control.kind === "select") {
          const option = exactOption(control, answer);
          if (!option) continue;
          await locator.selectOption(option);
          filled += 1;
        } else if (control.kind === "radio") {
          if (!radioMatches(control, answer)) continue;
          await locator.setChecked(true);
          filled += 1;
        } else if (control.kind === "checkbox") {
          await locator.setChecked(isAffirmative(answer));
          filled += 1;
        } else {
          await locator.fill(answer);
          filled += 1;
        }
      } catch {
        addIssue(session.operationalIssues, {
          field: displayField(control),
          message: `Lever did not accept the prepared value for ${displayField(control)}.`,
          severity: "blocking",
        });
      }
    }

    session.validationIssues = [];
    await this.transition(context, session, "fill", "completed", filled);
  }

  async validate(context: AdapterContext): Promise<ValidationIssue[]> {
    const session = await this.ensureSession(context);
    const challenge = await detectChallenge(context.page);
    if (challenge) {
      session.challenge = challenge;
      session.validationIssues = [];
      await this.transition(context, session, "validate", "paused");
      return [];
    }

    session.validationIssues = await this.collectValidationIssues(context, session);
    const blocked = session.validationIssues.some((issue) => issue.severity === "blocking");
    await this.transition(context, session, "validate", blocked ? "blocked" : "completed");
    return session.validationIssues.map((issue) => ({ ...issue }));
  }

  async submit(context: AdapterContext, submitOptions: LeverSubmitOptions = {}): Promise<SubmissionReceipt> {
    const session = await this.ensureSession(context);

    if (session.pageKind === "confirmation") return this.receipt(context);

    const challenge = await detectChallenge(context.page);
    if (challenge) {
      session.challenge = challenge;
      await this.transition(context, session, "submit", "paused");
      return this.emitReceipt(context, session, challengeReceipt(challenge), "paused");
    }

    // Once the irreversible control has been activated, every later entry is
    // receipt reconciliation. Re-entering prepare must not make another click possible.
    if (session.submitClicked) return this.receipt(context);

    session.validationIssues = await this.collectValidationIssues(context, session);
    const blocking = session.validationIssues.filter((issue) => issue.severity === "blocking");
    if (blocking.length > 0) {
      await this.transition(context, session, "submit", "blocked");
      return this.emitReceipt(context, session, issueReceipt(blocking, context.page.url()), "blocked");
    }

    const finalReviewApproved = submitOptions.finalReviewApproved === true
      || await this.options.finalReviewApproval?.(context) === true;
    if (!finalReviewApproved) {
      await this.transition(context, session, "submit", "review_required");
      return this.emitReceipt(context, session, reviewReceipt(context.page.url()), "review_required");
    }

    const submit = await locateUniqueVisible(context.page, SUBMIT_SELECTORS);
    if (!submit.locator) {
      const message = submit.problem === "ambiguous"
        ? "Lever exposed more than one provider-scoped Submit control."
        : "Bluey could not find the provider-scoped Lever Submit control.";
      const result: SubmissionReceipt = {
        status: "needs_input",
        issues: [{ field: "submission", message, severity: "blocking" }],
        intervention: {
          kind: "browser_takeover",
          title: "Review the Lever submit control",
          detail: `${message} Continue only in the preserved browser.`,
          takeoverUrl: context.page.url(),
          resolution: { kind: "browser_takeover", resumeAfter: false },
        },
      };
      await this.transition(context, session, "submit", "blocked");
      return this.emitReceipt(context, session, result, "blocked");
    }

    // Fence the irreversible action before awaiting the browser. Any exception
    // from this point forward is side-effect uncertainty, never a retry signal.
    session.submitClicked = true;
    await this.transition(context, session, "submit", "click_started");
    await context.beforeFinalSubmit?.();
    try {
      await submit.locator.click();
    } catch {
      await context.afterFinalSubmit?.("activation_uncertain");
      return this.emitReceipt(context, session, uncertainReceipt(context.page.url()), "uncertain");
    }
    await context.afterFinalSubmit?.("activated");
    try {
      await context.page.waitForSettled();
    } catch {
      return this.emitReceipt(context, session, uncertainReceipt(context.page.url()), "uncertain");
    }

    return this.receipt(context);
  }

  async receipt(context: AdapterContext): Promise<SubmissionReceipt> {
    const session = await this.ensureSession(context);
    const challenge = await detectChallenge(context.page);
    if (challenge) {
      session.challenge = challenge;
      return this.emitReceipt(context, session, challengeReceipt(challenge), "paused");
    }

    const body = await context.page.bodyText();
    const confirmationText = confirmationExcerpt(body);
    if (confirmationText && session.submitClicked) {
      session.pageKind = "confirmation";
      return this.emitReceipt(context, session, {
        status: "submitted",
        confirmationText,
        confirmationUrl: context.page.url(),
        submittedAt: (this.options.now?.() ?? new Date()).toISOString(),
        issues: [],
      }, "submitted");
    }

    if (confirmationText) {
      const result: SubmissionReceipt = {
        status: "needs_input",
        confirmationText,
        confirmationUrl: context.page.url(),
        issues: [{
          field: "submission",
          message: "Lever confirmation is visible, but this run did not record the submit action.",
          severity: "blocking",
        }],
        intervention: {
          kind: "browser_takeover",
          title: "Reconcile the Lever confirmation",
          detail: "Confirmation evidence is present, but it is not yet grounded to this application run.",
          takeoverUrl: context.page.url(),
          resolution: { kind: "browser_takeover", resumeAfter: false },
        },
      };
      return this.emitReceipt(context, session, result, "uncertain");
    }

    if (session.submitClicked) {
      return this.emitReceipt(context, session, uncertainReceipt(context.page.url()), "uncertain");
    }

    const result: SubmissionReceipt = {
      status: "failed",
      issues: [{
        field: "submission",
        message: "No explicit Lever confirmation evidence is present on this page.",
        severity: "blocking",
      }],
    };
    return this.emitReceipt(context, session, result, "failed");
  }

  private startSession(context: AdapterContext): LeverSession {
    const previous = this.sessions.get(context);
    const session: LeverSession = {
      pageKind: "unsupported",
      prepared: false,
      submitClicked: previous?.submitClicked ?? false,
      operationalIssues: [],
      validationIssues: [],
      history: previous?.history ?? [],
    };
    this.sessions.set(context, session);
    return session;
  }

  private async ensureSession(context: AdapterContext): Promise<LeverSession> {
    let session = this.sessions.get(context);
    if (!session) {
      await this.prepare(context);
      session = this.sessions.get(context);
    }
    if (!session) throw new Error("Lever state machine could not initialize");
    return session;
  }

  private async hasUniqueApplicationForm(page: BrowserPage, session: LeverSession): Promise<boolean> {
    const form = page.locator(APPLICATION_FORM);
    const count = await form.count();
    const visible = count === 1 && await form.isVisible();
    if (visible) return true;

    addIssue(session.operationalIssues, {
      field: "application",
      message: count > 1
        ? "Lever exposed more than one application form."
        : "Bluey could not verify one visible Lever application form.",
      severity: "blocking",
    });
    return false;
  }

  private async collectValidationIssues(
    context: AdapterContext,
    session: LeverSession,
  ): Promise<ValidationIssue[]> {
    const issues = session.operationalIssues.map((issue) => ({ ...issue }));
    if (session.pageKind === "confirmation") return issues;
    if (!session.prepared || session.pageKind !== "application") {
      addIssue(issues, {
        field: "application",
        message: "The Lever application form is not in a valid prepared state.",
        severity: "blocking",
      });
      return issues;
    }

    if (!(await this.hasUniqueApplicationForm(context.page, { ...session, operationalIssues: issues }))) {
      return issues;
    }

    const controls = await leverControls(context.page);
    const handledGroups = new Set<string>();
    for (const control of controls) {
      if (control.kind === "hidden") continue;
      const groupKey = control.kind === "radio" ? `radio:${control.name || control.selector}` : control.selector;
      if (handledGroups.has(groupKey)) continue;
      handledGroups.add(groupKey);

      const field = displayField(control);
      if (isGuardedField(control)) {
        addIssue(issues, {
          field,
          message: isSensitiveField(control)
            ? `Review the sensitive Lever question ${field} yourself.`
            : `Confirm the work authorization or sponsorship answer for ${field} yourself.`,
          severity: "blocking",
        });
        continue;
      }

      const attachment = attachmentKind(control);
      const answer = answerFor(control, context.packet);
      const known = Boolean(attachment || matchCanonicalField(control) || answer !== undefined);

      if (control.required && (control.kind === "other" || (control.kind === "file" && !attachment))) {
        addIssue(issues, {
          field,
          message: `Lever requires an unknown question: ${field}.`,
          severity: "blocking",
        });
        continue;
      }

      if (control.required && !known) {
        addIssue(issues, {
          field,
          message: `Lever requires an unknown question: ${field}.`,
          severity: "blocking",
        });
        continue;
      }

      if (!control.required) continue;

      if (attachment && !attachmentPath(control, context.packet)) {
        addIssue(issues, {
          field,
          message: `The packet is missing the required ${attachment === "resume" ? "resume" : "cover letter"} attachment.`,
          severity: "blocking",
        });
        continue;
      }

      if (!attachment && answer === undefined) {
        addIssue(issues, {
          field,
          message: `Bluey needs a confirmed fact for ${field}.`,
          severity: "blocking",
        });
        continue;
      }

      if (!hasAcceptedValue(control, controls)) {
        addIssue(issues, {
          field,
          message: `Lever did not accept the prepared value for ${field}.`,
          severity: "blocking",
        });
      }
    }

    const body = await context.page.bodyText();
    if (VISIBLE_FORM_ERROR_PATTERNS.some((pattern) => pattern.test(body))) {
      addIssue(issues, {
        field: "application",
        message: "Lever is showing a form validation error that needs review.",
        severity: "blocking",
      });
    }
    return issues;
  }

  private async transition(
    context: AdapterContext,
    session: LeverSession,
    state: LeverState,
    outcome: LeverStateOutcome,
    count?: number,
  ): Promise<void> {
    const issueCount = new Set(
      [...session.operationalIssues, ...session.validationIssues]
        .map((issue) => `${issue.field}\u0000${issue.message}\u0000${issue.severity}`),
    ).size;
    const snapshot: LeverStateSnapshot = {
      state,
      outcome,
      pageKind: session.pageKind,
      issueCount,
    };
    session.current = snapshot;
    session.history.push(snapshot);
    await context.log("lever_state_changed", {
      state,
      outcome,
      page_kind: session.pageKind,
      issue_count: snapshot.issueCount,
      item_count: count ?? 0,
      release: LEVER_CAPABILITY.release,
      mode: LEVER_CAPABILITY.mode,
      certified: LEVER_CAPABILITY.certified,
    });
  }

  private async emitReceipt(
    context: AdapterContext,
    session: LeverSession,
    receipt: SubmissionReceipt,
    outcome: LeverStateOutcome,
  ): Promise<SubmissionReceipt> {
    await this.transition(context, session, "receipt", outcome);
    return receipt;
  }
}

export function createLeverAdapter(options: LeverAdapterOptions = {}): LeverApplicationStateMachine {
  return new LeverApplicationStateMachine(options);
}

async function recognizePage(
  page: BrowserPage,
  detect: (url: URL) => boolean,
): Promise<LeverPageKind> {
  let url: URL;
  try {
    url = new URL(page.url());
  } catch {
    return "unsupported";
  }
  if (!detect(url)) return "unsupported";

  const body = await page.bodyText();
  if (confirmationExcerpt(body)) return "confirmation";
  if (CLOSED_PATTERNS.some((pattern) => pattern.test(body))) return "closed";

  const parts = url.pathname.split("/").filter(Boolean);
  if (parts.at(-1)?.toLowerCase() === "apply") return "application";
  if (parts.length === 2) return "posting";
  return "unsupported";
}

async function locateUniqueVisible(page: BrowserPage, selectors: string[]): Promise<LocatedControl> {
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
    const locator = page.locator(selector);
    const count = await locator.count();
    if (count > 1) return { problem: "ambiguous" };
    if (count === 1 && await locator.isVisible()) matches.push(locator);
  }
  if (matches.length > 1) return { problem: "ambiguous" };
  return matches[0] ? { locator: matches[0] } : { problem: "missing" };
}

async function detectChallenge(page: BrowserPage): Promise<InterventionRequest | undefined> {
  const body = await page.bodyText();
  for (const definition of CHALLENGE_DEFINITIONS) {
    const bodyMatch = definition.bodyPatterns.some((pattern) => pattern.test(body));
    let visibleMatch = false;
    for (const selector of definition.visibleSelectors) {
      const locator = page.locator(selector);
      if ((await locator.count()) === 1 && await locator.isVisible()) {
        visibleMatch = true;
        break;
      }
    }
    if (!bodyMatch && !visibleMatch) continue;
    return {
      kind: definition.kind,
      title: definition.title,
      detail: definition.detail,
      takeoverUrl: page.url(),
      resolution: { kind: "browser_takeover", resumeAfter: true },
    };
  }
  return undefined;
}

function searchableParts(control: FormControl): string[] {
  return [control.label, control.name, control.placeholder]
    .map(normalize)
    .filter(Boolean);
}

async function leverControls(page: BrowserPage): Promise<FormControl[]> {
  const controls = await page.controls();
  return Promise.all(controls.map(async (control) => {
    if (!/^\[data-bluey-field-id=(?:"[^"]+"|'[^']+')\]$/.test(control.selector)) return control;
    const questionLabel = page.locator(`.application-question:has(${control.selector}) .application-label`);
    if ((await questionLabel.count()) !== 1) return control;
    const providerLabel = cleanLabel(await questionLabel.textContent());
    return providerLabel ? { ...control, label: providerLabel } : control;
  }));
}

function searchableField(control: FormControl): string {
  return searchableParts(control).join(" ");
}

function displayField(control: FormControl): string {
  const label = cleanLabel(control.label) || cleanLabel(control.placeholder);
  if (label) return label;
  if (control.name && !/^cards\[|^[a-f0-9-]{24,}$/i.test(control.name)) return control.name;
  return "Lever required question";
}

function cleanLabel(value: string | null): string {
  return (value ?? "").replace(/[\u2731*]+\s*$/, "").replace(/\s+/g, " ").trim();
}

function matchCanonicalField(control: FormControl): string | undefined {
  const parts = searchableParts(control);
  for (const [canonical, aliases] of Object.entries(FIELD_ALIASES)) {
    if (parts.some((part) => [canonical, ...aliases].some((alias) => containsPhrase(part, normalize(alias))))) {
      return canonical;
    }
  }
  return undefined;
}

function answerFor(control: FormControl, packet: ApplicationPacket): string | undefined {
  const parts = searchableParts(control);
  const entries = Object.entries(packet.answers).filter(([, value]) => value.trim() !== "");
  const exact = entries.find(([key]) => parts.includes(normalize(key)));
  if (exact) return exact[1];

  const canonical = matchCanonicalField(control);
  if (!canonical) return undefined;
  const aliases = FIELD_ALIASES[canonical] ?? [];
  const canonicalAnswer = entries.find(([key]) => {
    const normalizedKey = normalize(key);
    return normalizedKey === normalize(canonical) || aliases.some((alias) => normalizedKey === normalize(alias));
  });
  if (canonicalAnswer) return canonicalAnswer[1];
  if (canonical === "email") return packet.applicationEmail;
  if (canonical === "full_name") {
    const first = answerEntry(packet, "first_name", "first name", "given name");
    const last = answerEntry(packet, "last_name", "last name", "family name", "surname");
    const joined = [first, last].filter(Boolean).join(" ").trim();
    return joined || undefined;
  }
  return undefined;
}

function answerEntry(packet: ApplicationPacket, ...keys: string[]): string | undefined {
  const normalizedKeys = new Set(keys.map(normalize));
  return Object.entries(packet.answers).find(([key]) => normalizedKeys.has(normalize(key)))?.[1];
}

function attachmentKind(control: FormControl): "resume" | "cover_letter" | undefined {
  if (control.kind !== "file") return undefined;
  const field = searchableField(control);
  if (/cover letter/.test(field)) return "cover_letter";
  if (/\bresume\b|curriculum|\bcv\b/.test(field)) return "resume";
  return undefined;
}

function attachmentPath(control: FormControl, packet: ApplicationPacket): string | undefined {
  const kind = attachmentKind(control);
  if (kind === "resume") return packet.resumePath;
  if (kind === "cover_letter") return packet.coverLetterPath;
  return undefined;
}

function exactOption(control: FormControl, answer: string): string | undefined {
  const normalizedAnswer = normalize(answer);
  return control.options?.find((option) => normalize(option.value) === normalizedAnswer)?.value
    ?? control.options?.find((option) => normalize(option.label) === normalizedAnswer)?.value;
}

function radioMatches(control: FormControl, answer: string): boolean {
  const normalizedAnswer = normalize(answer);
  const value = normalize(control.value);
  const label = normalize(control.label);
  return value === normalizedAnswer
    || label === normalizedAnswer
    || label.endsWith(` ${normalizedAnswer}`);
}

function hasAcceptedValue(control: FormControl, controls: FormControl[]): boolean {
  if (control.kind === "radio") {
    return controls.some((candidate) => candidate.kind === "radio"
      && candidate.name === control.name
      && candidate.checked === true);
  }
  if (control.kind === "checkbox") return control.checked === true;
  return control.value.trim().length > 0;
}

function isSensitiveField(control: FormControl): boolean {
  const field = searchableField(control);
  return SENSITIVE_PATTERNS.some((pattern) => pattern.test(field));
}

function isGuardedField(control: FormControl): boolean {
  const field = searchableField(control);
  return isSensitiveField(control) || AUTHORIZATION_PATTERNS.some((pattern) => pattern.test(field));
}

function isAffirmative(value: string): boolean {
  return /^(?:1|true|yes|y|on|agree|accepted)$/i.test(value.trim());
}

function containsPhrase(value: string, phrase: string): boolean {
  return value === phrase || ` ${value} `.includes(` ${phrase} `);
}

function normalize(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}

function confirmationExcerpt(body: string): string | undefined {
  const normalized = body.replace(/\s+/g, " ").trim();
  if ([...NON_CONFIRMATION_PATTERNS, ...CLOSED_PATTERNS].some((pattern) => pattern.test(normalized))) return undefined;
  for (const pattern of CONFIRMATION_PATTERNS) {
    const match = pattern.exec(normalized);
    if (!match || match.index === undefined) continue;
    const start = Math.max(0, match.index - 120);
    const end = Math.min(normalized.length, match.index + match[0].length + 180);
    return normalized.slice(start, end);
  }
  return undefined;
}

function safeUrlMatches(rawUrl: string, detect: (url: URL) => boolean): boolean {
  try {
    return detect(new URL(rawUrl));
  } catch {
    return false;
  }
}

function addIssue(issues: ValidationIssue[], issue: ValidationIssue): void {
  if (!issues.some((candidate) => candidate.field === issue.field && candidate.message === issue.message)) {
    issues.push(issue);
  }
}

function reviewReceipt(url: string): SubmissionReceipt {
  return {
    status: "needs_input",
    issues: [],
    intervention: {
      kind: "browser_takeover",
      title: "Review this Lever application",
      detail: "Review every answer and attachment in the preserved browser. Bluey will not submit until you explicitly approve final review.",
      takeoverUrl: url,
      resolution: { kind: "browser_takeover", resumeAfter: true },
    },
  };
}

function challengeReceipt(intervention: InterventionRequest): SubmissionReceipt {
  return { status: "needs_input", issues: [], intervention };
}

function issueReceipt(issues: ValidationIssue[], url: string): SubmissionReceipt {
  const first = issues[0];
  const sensitive = first && /sensitive/i.test(first.message);
  const authorization = first && /authorization|sponsorship/i.test(first.message);
  const unknown = first && /unknown question/i.test(first.message);
  return {
    status: "needs_input",
    issues,
    intervention: {
      kind: sensitive ? "sensitive_question" : unknown ? "unknown_question" : "missing_fact",
      title: sensitive || authorization
        ? "Your choice is needed"
        : unknown
          ? "A new Lever question needs your answer"
          : "One Lever detail is missing",
      detail: first?.message ?? "Review the required Lever field.",
      field: first?.field,
      takeoverUrl: url,
      resolution: { kind: "answer", resumeAfter: true },
    },
  };
}

function uncertainReceipt(url: string): SubmissionReceipt {
  return {
    status: "needs_input",
    issues: [{
      field: "submission",
      message: "Lever did not show explicit confirmation after the submit control was activated. The side effect is uncertain; do not retry automatically.",
      severity: "blocking",
    }],
    intervention: {
      kind: "browser_takeover",
      title: "Confirm the Lever application result",
      detail: "The submit response is uncertain. Inspect the preserved browser or employer evidence before deciding what happened.",
      takeoverUrl: url,
      resolution: { kind: "browser_takeover", resumeAfter: false },
    },
  };
}
