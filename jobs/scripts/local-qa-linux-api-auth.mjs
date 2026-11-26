// Transport-injected, route-closed synthetic auth for the Linux local-QA unit.
// The owner owns HTTP, service lifecycle, and all persistent state.
const REFUSAL = "local_qa_linux_api_auth_refused";
const OPERATOR_EMAIL = "operator@bluey-qa.test";
const CANDIDATE_EMAIL = "candidate@bluey-qa.test";
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const OTP = /^[0-9]{6}$/;
const SHA256 = /^[0-9a-f]{64}$/;
const TAXONOMY_HEADERS = ["x-bluey-jobs-taxonomy-sha256", "x-bluey-jobs-taxonomy-version"];

function refuse() { throw new Error(REFUSAL); }
function need(value) { if (!value) refuse(); }
function plain(value) { return value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype; }
function exactKeys(value, names) { return plain(value) && Object.keys(value).sort().join(",") === [...names].sort().join(","); }
function checkSignal(signal) {
  need(signal === undefined || (typeof signal === "object" && typeof signal.aborted === "boolean"));
  if (signal?.aborted) refuse();
}
function password(value) { need(typeof value === "string" && /^Qa-[0-9a-f]{48}$/.test(value)); return value; }
function account(value, email, admin) {
  need(exactKeys(value, ["balance_cents", "email", "id", "is_admin", "is_temporary", "temporary_expires_at", "trial_seconds_remaining"])
    && UUID.test(value.id) && value.email === email && value.is_admin === admin && value.is_temporary === false
    && value.temporary_expires_at === null && Number.isSafeInteger(value.balance_cents)
    && Number.isSafeInteger(value.trial_seconds_remaining));
  return Object.freeze({ id: value.id, email, admin });
}
function authReply(value, email, admin) {
  need(exactKeys(value, ["access_token", "account", "expires_in", "refresh_token"]) && typeof value.access_token === "string"
    && value.access_token.length > 0 && value.access_token.length <= 16_384 && !/[\0\r\n]/.test(value.access_token));
  need(typeof value.refresh_token === "string" && value.refresh_token.length > 0 && value.refresh_token.length <= 16_384
    && !/[\0\r\n]/.test(value.refresh_token) && Number.isSafeInteger(value.expires_in) && value.expires_in > 0);
  return Object.freeze({ account: account(value.account, email, admin), token: value.access_token });
}
function session(value, secrets) {
  need(plain(value) && value.operatorEmail === OPERATOR_EMAIL && value.candidateEmail === CANDIDATE_EMAIL
    && secrets && typeof secrets === "object");
  return Object.freeze({ operatorEmail: OPERATOR_EMAIL, candidateEmail: CANDIDATE_EMAIL,
    operatorPassword: password(secrets.operatorPassword), candidatePassword: password(secrets.candidatePassword) });
}
function expected(value) {
  need(exactKeys(value, ["candidateId", "operatorId"]) && UUID.test(value.operatorId) && UUID.test(value.candidateId)
    && value.operatorId !== value.candidateId);
  return Object.freeze({ operatorId: value.operatorId, candidateId: value.candidateId });
}
function taxonomyHeaders(headers) {
  need(plain(headers) && Object.keys(headers).sort().join(",") === TAXONOMY_HEADERS.join(","));
  need(typeof headers["x-bluey-jobs-taxonomy-version"] === "string" && headers["x-bluey-jobs-taxonomy-version"].length > 0
    && headers["x-bluey-jobs-taxonomy-version"].length <= 128 && SHA256.test(headers["x-bluey-jobs-taxonomy-sha256"]));
  return Object.freeze({
    "x-bluey-jobs-taxonomy-version": headers["x-bluey-jobs-taxonomy-version"],
    "x-bluey-jobs-taxonomy-sha256": headers["x-bluey-jobs-taxonomy-sha256"],
  });
}

// Used only inside the owned signup/browser bridge, after the real form has
// created its synthetic candidate. The browser bearer is never exported.
export async function bindLocalQaLinuxImportedCandidate({ call, candidatePassword, accountId, signal,
  readBoundResume }) {
  try {
    need(typeof call === "function" && typeof accountId === "string" && UUID.test(accountId)
      && (readBoundResume === undefined || typeof readBoundResume === "function"));
    const request = async (path, options) => {
      checkSignal(signal);
      const result = await call(path, Object.freeze(options));
      checkSignal(signal);
      return result;
    };
    const auth = authReply(await request("/auth/login", { method: "POST",
      body: Object.freeze({ email: CANDIDATE_EMAIL, password: password(candidatePassword) }) }), CANDIDATE_EMAIL, false);
    need(auth.account.id === accountId);
    const scoped = (path, options = {}) => request(path, { ...options, token: auth.token });
    const beta = await scoped("/api/jobs/beta-access");
    need(exactKeys(beta, ["access", "reason", "schemaVersion"])
      && beta.access === "admitted" && beta.reason === "admitted" && beta.schemaVersion === 1);
    let onboardingAttempted = false, prepareAttempted = false, preparedApplicationId, preparedResumeId, preparedJobId;
    return Object.freeze({
      readWorkspace: () => scoped("/api/jobs/workspace"),
      readTaxonomy: () => scoped("/api/jobs/taxonomy"),
      readResumeSource: () => scoped("/api/jobs/resume-source"),
      async saveOnboarding(body, headers) {
        need(!onboardingAttempted && exactKeys(body, ["profile", "preferences", "track"]));
        onboardingAttempted = true;
        return scoped("/api/jobs/onboarding/complete", { method: "POST", body, headers: taxonomyHeaders(headers) });
      },
      async prepareDraft(jobId) {
        need(onboardingAttempted && !prepareAttempted && typeof jobId === "string"
          && /^[A-Za-z0-9_-]{1,160}$/.exec(jobId)?.[0] === jobId);
        prepareAttempted = true;
        const result = await scoped("/api/jobs/applications", { method: "POST", body: Object.freeze({ job_id: jobId,
          mode: "factual", submission_mode: "review_first" }) });
        const id = result?.application?.id;
        need(typeof id === "string" && UUID.exec(id)?.[0] === id && result.application.job_id === jobId);
        if (readBoundResume !== undefined) {
          const resumeId = result?.resume_version?.id;
          need(typeof resumeId === "string" && UUID.exec(resumeId)?.[0] === resumeId
            && result.application.resume_version_id === resumeId
            && result.resume_version.mode === "factual" && result.resume_version.job_id === jobId);
          preparedResumeId = resumeId;
          preparedJobId = jobId;
        }
        preparedApplicationId = id;
        return result;
      },
      async readEmptyCommunicationActions(applicationId) {
        need(preparedApplicationId !== undefined && (applicationId === undefined || applicationId === preparedApplicationId));
        const result = await scoped(`/api/jobs/communication-actions?limit=100${applicationId ? `&application_id=${applicationId}` : ""}`);
        need(Array.isArray(result) && result.length === 0);
        return result;
      },
      ...(readBoundResume === undefined ? {} : {
        account: Object.freeze({ id: auth.account.id, email: auth.account.email,
          is_admin: false, is_temporary: false }),
        async readResume(resumeId) {
          need(typeof preparedResumeId === "string" && resumeId === preparedResumeId);
          checkSignal(signal);
          const result = await readBoundResume(resumeId, auth.token);
          checkSignal(signal);
          need(plain(result) && result.id === resumeId && result.mode === "factual"
            && result.job_id === preparedJobId);
          return result;
        },
      }),
    });
  } catch { refuse(); }
}

/**
 * Run the fixed, synthetic two-account authentication flow through an owner
 * supplied closed transport. `call` must itself reject anything other than an
 * exact HTTP-200 JSON reply; this module never receives an origin or URL.
 */
export async function authenticateLocalQaLinuxApi({ mode, sessionConfig, sessionSecrets, call, takeSignupCode,
  switchSignupRecipient, signupCandidateInBrowser, expectedIds, readBoundResume, readPreparedResume,
  commitBrowserBootstrap, signal }) {
  try {
    need((mode === "bootstrap" || mode === "restart") && typeof call === "function");
    need(signupCandidateInBrowser === undefined || (mode === "bootstrap" && typeof signupCandidateInBrowser === "function"));
    need(readPreparedResume === undefined || (mode === "bootstrap" && signupCandidateInBrowser !== undefined
      && typeof readPreparedResume === "function"));
    need(commitBrowserBootstrap === undefined || (readPreparedResume !== undefined
      && typeof commitBrowserBootstrap === "function"));
    const credentials = session(sessionConfig, sessionSecrets);
    if (mode === "bootstrap") need(typeof takeSignupCode === "function" && typeof switchSignupRecipient === "function" && expectedIds === undefined);
    else expectedIds = expected(expectedIds);
    const request = async (path, options) => {
      checkSignal(signal);
      const result = await call(path, Object.freeze(options));
      checkSignal(signal);
      need(plain(result));
      return result;
    };
    const signup = async (email, secret, fingerprint) => {
      need(email === OPERATOR_EMAIL || email === CANDIDATE_EMAIL);
      const start = Object.freeze({ email, password: secret, terms_accepted: true, device_fingerprint: fingerprint });
      await request("/auth/signup/start", { method: "POST", body: start });
      const code = await takeSignupCode("signup");
      checkSignal(signal); need(typeof code === "string" && OTP.test(code));
      return authReply(await request("/auth/signup/confirm", { method: "POST", body: Object.freeze({ email, otp: code, device_fingerprint: fingerprint }) }),
        email, email === OPERATOR_EMAIL);
    };
    const loginCandidate = async () => authReply(await request("/auth/login", { method: "POST", body: Object.freeze({
      email: CANDIDATE_EMAIL, password: credentials.candidatePassword,
    }) }), CANDIDATE_EMAIL, false);
    const requireBetaAccess = async candidate => {
      const beta = await request("/api/jobs/beta-access", { method: "GET", token: candidate.token });
      need(exactKeys(beta, ["access", "reason", "schemaVersion"])
        && beta.access === "admitted" && beta.reason === "admitted" && beta.schemaVersion === 1);
    };
    let operator, candidate;
    if (mode === "bootstrap") {
      operator = await signup(OPERATOR_EMAIL, credentials.operatorPassword, "local-qa-operator");
      const cohort = await request("/admin/jobs/public-beta", { method: "GET", token: operator.token });
      need(plain(cohort) && Number.isSafeInteger(cohort.revision) && cohort.revision >= 0);
      const now = Date.now();
      await request("/admin/jobs/public-beta", { method: "PUT", token: operator.token, body: Object.freeze({
        expectedRevision: cohort.revision, state: "open", opensAtMs: now - 60_000, closesAtMs: now + 7 * 86_400_000, hardCap: 2,
      }) });
      need(await switchSignupRecipient(CANDIDATE_EMAIL) === true);
      if (signupCandidateInBrowser === undefined) {
        candidate = await signup(CANDIDATE_EMAIL, credentials.candidatePassword, "local-qa-candidate");
      } else {
        checkSignal(signal);
        let active = true, bindAttempts = 0, verified;
        const bindBootstrap = async accountId => {
          need(active && ++bindAttempts === 1 && typeof accountId === "string" && UUID.exec(accountId)?.[0] === accountId);
          const authenticated = await loginCandidate();
          need(active && bindAttempts === 1 && authenticated.account.id === accountId
            && accountId !== operator.account.id);
          await requireBetaAccess(authenticated);
          checkSignal(signal);
          need(active && bindAttempts === 1);
          const ids = Object.freeze({ operatorId: operator.account.id, candidateId: accountId });
          need(await commitBrowserBootstrap(ids) === true);
          checkSignal(signal);
          need(active && bindAttempts === 1);
          verified = authenticated;
          return true;
        };
        let observed;
        try {
          observed = commitBrowserBootstrap === undefined
            ? await signupCandidateInBrowser(credentials.candidatePassword)
            : await signupCandidateInBrowser(credentials.candidatePassword, bindBootstrap);
        } finally { active = false; }
        checkSignal(signal);
        need(exactKeys(observed, ["accountId"]) && UUID.test(observed.accountId));
        // Keep the browser token inside the browser. Independently log in over
        // the owned API before a Prepare seeder can consume the custody receipt.
        if (commitBrowserBootstrap === undefined) candidate = await loginCandidate();
        else {
          need(bindAttempts === 1 && verified !== undefined);
          candidate = verified;
        }
        need(candidate.account.id === observed.accountId);
      }
      need(operator.account.id !== candidate.account.id);
    } else {
      operator = authReply(await request("/auth/login", { method: "POST", body: Object.freeze({ email: OPERATOR_EMAIL, password: credentials.operatorPassword }) }), OPERATOR_EMAIL, true);
      candidate = await loginCandidate();
      need(operator.account.id === expectedIds.operatorId && candidate.account.id === expectedIds.candidateId);
    }
    await requireBetaAccess(candidate);
    const initialWorkspace = await request("/api/jobs/workspace", { method: "GET", token: candidate.token });
    const readWorkspace = async () => request("/api/jobs/workspace", { method: "GET", token: candidate.token });
    const readResumeSource = async () => {
      need(mode === "bootstrap" && signupCandidateInBrowser !== undefined);
      return request("/api/jobs/resume-source", { method: "GET", token: candidate.token });
    };
    const readTaxonomy = async () => request("/api/jobs/taxonomy", { method: "GET", token: candidate.token });
    const saveOnboarding = async (body, headers) => {
      need(mode === "bootstrap");
      const safeHeaders = taxonomyHeaders(headers);
      // Onboarding is the only current mutation that takes taxonomy headers;
      // no caller-supplied header set can add credentials or idempotency data.
      return request("/api/jobs/onboarding/complete", { method: "POST", body, headers: safeHeaders, token: candidate.token });
    };
    const prepareDraft = async body => {
      if (mode !== "bootstrap") refuse();
      need(exactKeys(body, ["job_id", "mode", "submission_mode"]) && typeof body.job_id === "string"
        && /^[A-Za-z0-9_-]{1,160}$/.test(body.job_id) && body.mode === "factual" && body.submission_mode === "review_first");
      return request("/api/jobs/applications", { method: "POST", body: Object.freeze({ ...body }), token: candidate.token });
    };
    const readResume = async id => {
      const read = mode === "restart" ? readBoundResume : readPreparedResume;
      need(typeof read === "function" && typeof id === "string" && UUID.test(id)
        && Array.isArray(initialWorkspace.applications) && initialWorkspace.applications.length === 1
        && initialWorkspace.applications[0]?.resume_version_id === id);
      checkSignal(signal);
      const resume = await read(id, candidate.token);
      checkSignal(signal);
      need(plain(resume));
      return resume;
    };
    // Bind the admitted account before exposing read or bootstrap-only writes.
    need(plain(initialWorkspace));
    return Object.freeze({ operatorId: operator.account.id, candidateId: candidate.account.id,
      readWorkspace, readTaxonomy, saveOnboarding, prepareDraft, readResume, readResumeSource });
  } catch { refuse(); }
}
