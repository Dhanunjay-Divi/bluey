const CANDIDATE_EMAIL = "candidate@bluey-qa.test";
const TIMEOUT_MS = 8_000;
const SIGNUP_HREF = "/login?mode=signup&next=%2Fjobs";
const PASSWORD_PATTERN = /^Qa-[a-f0-9]{48}$/;
const OTP_PATTERN = /^\d{6}$/;
const ACCOUNT_ID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const REFUSAL = Symbol("localQaBrowserSignupRefusal");
const REFUSAL_STAGES = new Set([
  "origin",
  "input",
  "fresh_page",
  "jobs_navigation",
  "signup_link",
  "signup_navigation",
  "signup_form",
  "email_boundary",
  "email_fill",
  "password_boundary",
  "password_fill",
  "password_confirm_boundary",
  "password_confirm_fill",
  "terms_boundary",
  "terms_check",
  "start_response",
  "start_submit",
  "start_status",
  "otp_boundary",
  "signup_code",
  "otp_form",
  "otp_fill",
  "confirm_response",
  "confirm_navigation",
  "confirm_boundary",
  "confirm_submit",
  "confirm_status",
  "confirm_body",
  "confirm_account",
  "jobs_onboarding",
]);

function refusal(stage) {
  const suffix = REFUSAL_STAGES.has(stage) ? stage : "internal";
  const error = new Error(`local_qa_browser_signup_refused:${suffix}`);
  Object.defineProperty(error, REFUSAL, { value: true });
  return error;
}

async function guarded(stage, operation) {
  try {
    return await operation();
  } catch (error) {
    if (error?.[REFUSAL] === true) {
      throw error;
    }
    throw refusal(stage);
  }
}

function localOrigin(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw refusal("origin");
  }
  const port = Number(url.port);
  if (
    url.protocol !== "http:" ||
    url.hostname !== "127.0.0.1" ||
    !Number.isInteger(port) ||
    port <= 1024 ||
    port > 65535 ||
    port === 5187 ||
    url.username ||
    url.password ||
    url.pathname !== "/" ||
    url.search ||
    url.hash ||
    url.origin !== value
  ) {
    throw refusal("origin");
  }
  return url.origin;
}

async function requireUrl(page, expected, stage) {
  const current = await guarded(stage, () => page.url());
  if (current !== expected) {
    throw refusal(stage);
  }
}

async function requireVisible(locator, stage) {
  await guarded(stage, () => locator.waitFor({ state: "visible", timeout: TIMEOUT_MS }));
}

function matchingPost(origin, path) {
  const expected = `${origin}${path}`;
  return (response) => {
    try {
      return response.url() === expected && response.request().method() === "POST";
    } catch {
      return false;
    }
  };
}

async function requireStatus(response, stage) {
  const status = await guarded(stage, () => response.status());
  if (status !== 200) {
    throw refusal(stage);
  }
}

async function responseAccountId(response) {
  const body = await guarded("confirm_body", () => response.json());
  const account = body?.account;
  if (
    !account ||
    account.email !== CANDIDATE_EMAIL ||
    account.is_admin !== false ||
    account.is_temporary !== false ||
    typeof account.id !== "string" ||
    !ACCOUNT_ID_PATTERN.test(account.id)
  ) {
    throw refusal("confirm_account");
  }
  return account.id;
}

/**
 * Completes the synthetic local-QA signup UI action without creating, importing,
 * or configuring a browser. The caller owns the already-isolated page and SMTP sink.
 */
export async function completeLocalQaBrowserSignup({ page, origin, password, takeSignupCode }) {
  if (!page || typeof takeSignupCode !== "function" || !PASSWORD_PATTERN.test(password)) {
    throw refusal("input");
  }

  const local = localOrigin(origin);
  const jobsUrl = `${local}/jobs/`;
  const signupUrl = `${local}${SIGNUP_HREF}`;

  await requireUrl(page, "about:blank", "fresh_page");
  await guarded("jobs_navigation", () => page.goto(jobsUrl, { timeout: TIMEOUT_MS }));
  await requireUrl(page, jobsUrl, "jobs_navigation");

  const startLink = page.getByRole("link", { name: "Start my job search", exact: true });
  await requireVisible(startLink, "signup_link");
  const href = await guarded("signup_link", () =>
    startLink.getAttribute("href", { timeout: TIMEOUT_MS }),
  );
  if (href !== SIGNUP_HREF) {
    throw refusal("signup_link");
  }
  const loginNavigation = guarded("signup_navigation", () =>
    page.waitForURL(signupUrl, { timeout: TIMEOUT_MS }),
  );
  await Promise.all([
    loginNavigation,
    guarded("signup_navigation", () => startLink.click({ timeout: TIMEOUT_MS })),
  ]);
  await requireUrl(page, signupUrl, "signup_navigation");

  await requireVisible(
    page.getByRole("heading", { name: "Create account", exact: true }),
    "signup_form",
  );
  const email = page.locator("#accountEmail");
  const passwordInput = page.locator("#accountPassword");
  const passwordConfirm = page.locator("#accountPasswordConfirm");
  const terms = page.locator("#signupTerms");
  const submit = page.locator("#accountPrimaryButton");
  const otp = page.locator("#signupOtp");
  const confirm = page.locator("#confirmSignupButton");
  for (const locator of [email, passwordInput, passwordConfirm, terms, submit]) {
    await requireVisible(locator, "signup_form");
  }

  await requireUrl(page, signupUrl, "email_boundary");
  await guarded("email_fill", () => email.fill(CANDIDATE_EMAIL, { timeout: TIMEOUT_MS }));
  await requireUrl(page, signupUrl, "password_boundary");
  await guarded("password_fill", () => passwordInput.fill(password, { timeout: TIMEOUT_MS }));
  await requireUrl(page, signupUrl, "password_confirm_boundary");
  await guarded("password_confirm_fill", () => passwordConfirm.fill(password, { timeout: TIMEOUT_MS }));
  await requireUrl(page, signupUrl, "terms_boundary");
  await guarded("terms_check", () => terms.check({ timeout: TIMEOUT_MS }));

  const startResponse = guarded("start_response", () =>
    page.waitForResponse(matchingPost(local, "/auth/signup/start"), { timeout: TIMEOUT_MS }),
  );
  const [started] = await Promise.all([
    startResponse,
    guarded("start_submit", () => submit.click({ timeout: TIMEOUT_MS })),
  ]);
  await requireStatus(started, "start_status");

  await requireUrl(page, signupUrl, "otp_boundary");
  const signupCode = await guarded("signup_code", () => takeSignupCode());
  if (typeof signupCode !== "string" || !OTP_PATTERN.test(signupCode)) {
    throw refusal("signup_code");
  }
  await requireVisible(otp, "otp_form");
  await requireVisible(confirm, "otp_form");
  await requireUrl(page, signupUrl, "otp_boundary");
  await guarded("otp_fill", () => otp.fill(signupCode, { timeout: TIMEOUT_MS }));
  await requireUrl(page, signupUrl, "confirm_boundary");

  const confirmResponse = guarded("confirm_response", () =>
    page.waitForResponse(matchingPost(local, "/auth/signup/confirm"), { timeout: TIMEOUT_MS }),
  );
  const jobsNavigation = guarded("confirm_navigation", () =>
    page.waitForURL(jobsUrl, { timeout: TIMEOUT_MS }),
  );
  const [confirmed] = await Promise.all([
    confirmResponse,
    jobsNavigation,
    guarded("confirm_submit", () => confirm.click({ timeout: TIMEOUT_MS })),
  ]);
  await requireStatus(confirmed, "confirm_status");
  await requireUrl(page, jobsUrl, "confirm_navigation");
  await requireVisible(
    page.getByRole("heading", { name: "Import once, then tailor every job", exact: true }),
    "jobs_onboarding",
  );

  return { accountId: await responseAccountId(confirmed) };
}

export const localQaBrowserSignup = Object.freeze({
  candidateEmail: CANDIDATE_EMAIL,
  completeLocalQaBrowserSignup,
});
