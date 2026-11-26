// Private current Jobs API composition for a disposable Linux QA unit.
import { lstat } from "node:fs/promises";
import { request } from "node:http";
import { join } from "node:path";
import { authenticateLocalQaLinuxApi, bindLocalQaLinuxImportedCandidate } from "./local-qa-linux-api-auth.mjs";
import { prepareLocalQaLinuxApiProcess } from "./local-qa-linux-api-process.mjs";
import { LOCAL_QA_LINUX_EFFECT_FLAGS, prepareLocalQaLinuxC2cDraft, requireLocalQaLinuxC2cEnvironment,
  requireLocalQaLinuxC2cSession } from "./local-qa-linux-c2c-session.mjs";
import { createLocalQaLinuxApiPidfdBinder } from "./local-qa-linux-pidfd-client.mjs";
import { createLocalQaLinuxPgRuntime, validateLocalQaLinuxPgRuntimeBinding } from "./local-qa-linux-pg-runtime.mjs";
import { assertLocalQaLinuxPostgresRunRoot } from "./local-qa-linux-postgres.mjs";
import { createLocalQaLinuxC2cUnitAssertion } from "./local-qa-linux-c2c-unit.mjs";
import { runLocalQaLinuxC2cRestartGateway, validateLocalQaLinuxC2cBrowserBinding } from "./local-qa-linux-c2c-restart-gateway.mjs";
import { parseLocalQaBuildBinding } from "./local-qa-build-binding.mjs";
import { startLocalQaObjectStore } from "./local-qa-object-store.mjs";
import { startLocalQaSmtp } from "./local-qa-smtp.mjs";
import {
  acquireLocalQaSessionLease, assertLocalQaSessionLease, initializeLocalQaSessionState, inspectLocalQaSessionState,
  loadLocalQaSessionSecrets, releaseLocalQaSessionLease, requireLocalQaBootstrapReceipt, writeLocalQaBootstrapIntent,
  writeLocalQaBootstrapReceipt,
} from "./local-qa-session-state.mjs";
import { initializeLocalQaState, inspectLocalQaState, loadLocalQaSecrets } from "./local-qa-state.mjs";

const REFUSED = "local_qa_linux_owned_api_refused";
const UNCLOSED = "local_qa_linux_owned_api_unclosed";
const MAX_BODY = 1024 * 1024;
const REQUEST_MS = 15_000;
const RESUME = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const TAXONOMY = ["x-bluey-jobs-taxonomy-sha256", "x-bluey-jobs-taxonomy-version"];
const ROUTES = Object.freeze({
  "/auth/signup/start": ["POST"], "/auth/signup/confirm": ["POST"], "/auth/login": ["POST"],
  "/admin/jobs/public-beta": ["GET", "PUT"], "/api/jobs/beta-access": ["GET"], "/api/jobs/workspace": ["GET"],
  "/api/jobs/taxonomy": ["GET"], "/api/jobs/onboarding/complete": ["POST"], "/api/jobs/applications": ["POST"],
  "/api/jobs/resume-source": ["GET"],
});
const own = value => value !== null && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype;
const exact = (value, keys) => own(value) && Object.keys(value).sort().join(",") === keys;
const port = value => Number.isSafeInteger(value) && value > 1024 && value <= 65535 && ![5187, 5432].includes(value);
function need(value) { if (!value) throw new Error(REFUSED); }

export function validateLocalQaLinuxApiRequest(path, options = {}, resumeId, allowPrepareCommunications = false) {
  const scopedResume = typeof resumeId === "string" && RESUME.exec(resumeId)?.[0] === resumeId
    && path === `/api/jobs/resume-versions/${resumeId}`;
  const communications = "/api/jobs/communication-actions?limit=100";
  const communicationPrefix = `${communications}&application_id=`;
  const appId = typeof path === "string" && path.startsWith(communicationPrefix) ? path.slice(communicationPrefix.length) : "";
  const scopedCommunication = allowPrepareCommunications === true
    && (path === communications || RESUME.exec(appId)?.[0] === appId);
  need(typeof path === "string" && (Object.hasOwn(ROUTES, path) || scopedResume || scopedCommunication) && own(options)
    && Object.keys(options).every(key => ["method", "body", "headers", "token"].includes(key)));
  const { method = "GET", body, token, headers } = options;
  need((scopedResume || scopedCommunication ? method === "GET" : ROUTES[path].includes(method))
    && (method === "GET" ? body === undefined : own(body)));
  need(path.startsWith("/auth/") ? token === undefined : typeof token === "string"
    && token.length <= 8192 && /^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/.exec(token)?.[0] === token);
  const onboarding = path === "/api/jobs/onboarding/complete";
  if (onboarding) {
    need(exact(headers, TAXONOMY.join(",")));
    const sha = headers[TAXONOMY[0]], version = headers[TAXONOMY[1]];
    need(typeof sha === "string" && /^[a-f0-9]{64}$/.exec(sha)?.[0] === sha
      && typeof version === "string" && /^[A-Za-z0-9._:-]{1,128}$/.exec(version)?.[0] === version);
  } else need(headers === undefined || exact(headers, ""));
  const bytes = body === undefined ? null : Buffer.from(JSON.stringify(body));
  need(bytes === null || bytes.length <= MAX_BODY);
  return Object.freeze({ method, bytes, headers: Object.freeze({
    ...(onboarding ? { ...headers } : {}),
    ...(token ? { authorization: `Bearer ${token}` } : {}),
    ...(bytes === null ? {} : { "content-type": "application/json", "content-length": String(bytes.length) }),
  }) });
}

// Fixed-address HTTP; no redirects, ambient proxy, pooled credentials, or
// retry. Ownership is checked on both sides of even a failed request.
export function createLocalQaLinuxApiTransport({ apiPort, assertCurrent, signal, resumeId, allowPrepareCommunications }) {
  need(port(apiPort) && typeof assertCurrent === "function" && (signal === undefined || signal instanceof AbortSignal));
  return async (path, options) => {
    try {
      const { method, bytes, headers } = validateLocalQaLinuxApiRequest(path, options, resumeId, allowPrepareCommunications);
      signal?.throwIfAborted();
      need(await assertCurrent() === true);
      let result;
      try {
        result = await new Promise((resolve, reject) => {
          const fail = () => reject(new Error(REFUSED));
          const req = request({ host: "127.0.0.1", port: apiPort, path, method, headers, agent: false, signal }, res => {
            let size = 0;
            const chunks = [];
            if (res.statusCode !== 200 || !/^application\/json(?:\s*;|$)/i.test(res.headers["content-type"] ?? "")) {
              res.destroy(); fail(); return;
            }
            res.on("data", chunk => {
              size += chunk.length;
              if (size > MAX_BODY) { res.destroy(); fail(); } else chunks.push(chunk);
            });
            res.once("aborted", fail);
            res.once("error", fail);
            res.once("end", () => {
              try { resolve(JSON.parse(Buffer.concat(chunks).toString("utf8"))); } catch { fail(); }
            });
          });
          const deadline = setTimeout(() => req.destroy(new Error(REFUSED)), REQUEST_MS);
          req.once("close", () => clearTimeout(deadline));
          req.once("error", fail);
          req.end(bytes ?? undefined);
        });
      } finally {
        need(await assertCurrent() === true);
      }
      signal?.throwIfAborted();
      return result;
    } catch { throw new Error(REFUSED); }
  };
}

export function validateLocalQaLinuxOwnedApiBinding(input) {
  need(exact(input, "apiPort,assertOwnedUnit,buildBinding,networkNamespace,objectPort,pgCapsule,pidfdToolchain,postgresPort,root,smtpPort,unitCgroup"));
  const { root, assertOwnedUnit, unitCgroup, networkNamespace, pgCapsule, pidfdToolchain,
    apiPort, postgresPort, objectPort, smtpPort } = input;
  need(typeof root === "string" && [apiPort, postgresPort, objectPort, smtpPort].every(port)
    && new Set([apiPort, postgresPort, objectPort, smtpPort]).size === 4);
  const stateDirectory = join(root, "bluey-jobs-qa-draft");
  const pgBinding = validateLocalQaLinuxPgRuntimeBinding({ root, stateDirectory, port: postgresPort, pgCapsule,
    pidfdToolchain, unitCgroup, networkNamespace, assertOwnedUnit });
  return Object.freeze({ pgBinding, buildBinding: parseLocalQaBuildBinding(input.buildBinding), apiPort, objectPort, smtpPort });
}

export function buildLocalQaLinuxApiEnvironment({ stateDirectory, root, apiPort, config, sessionSecrets, secrets, pg, store, smtp }) {
  return Object.freeze({
    PATH: "/usr/bin:/bin", LANG: "C", LC_ALL: "C", TZ: "UTC", HOME: stateDirectory, TMPDIR: join(root, "tmp"),
    RUST_LOG: "warn", BLUEY_API_HOST: "127.0.0.1", BLUEY_PORT: String(apiPort),
    BLUEY_PUBLIC_URL: `http://127.0.0.1:${apiPort}`, BLUEY_SERVER_DB_BACKEND: "postgres",
    BLUEY_DATABASE_URL: pg.databaseUrl, BLUEY_JWT_SECRET: secrets.jwtSecret, BLUEY_JOBS_DATA_KEY: secrets.jobsDataKey,
    BLUEY_JOBS_BETA_ENABLED: "1", BLUEY_ADMIN_EMAILS: config.operatorEmail,
    BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY_ID: config.keyId, BLUEY_JOBS_RUNNER_PURGE_SIGNING_KEY: sessionSecrets.purgeSeed,
    BLUEY_JOBS_RUNNER_MINIMUM_BUILD_ID: config.minimumBuild,
    BLUEY_JOBS_RUNNER_PURGE_VERIFYING_KEYS_JSON: JSON.stringify([{ keyId: config.keyId, publicKeyBase64url: config.publicKey }]),
    ...Object.fromEntries(LOCAL_QA_LINUX_EFFECT_FLAGS.map(name => [name, "0"])), ...store.env, ...smtp.env,
  });
}

// Handles are local-only. A failed upstream close retains the lease and
// defers any uncertain allocation to the admitted unit's cleanup.
export function createLocalQaLinuxOwnedApiSession(input) {
  const { pgBinding, buildBinding, apiPort, objectPort, smtpPort } = validateLocalQaLinuxOwnedApiBinding(input);
  const { root, stateDirectory, unitCgroup, networkNamespace, pidfdToolchain } = pgBinding;
  const open = async (mode, args, { savedBrowser, signupBrowser, includeSampleResume = false, prepareFixture,
    reviewFirstMockFixture } = {}) => {
    need(exact(args, "root,signal,stateDirectory") && args.root === root && args.stateDirectory === stateDirectory
      && (args.signal === undefined || args.signal instanceof AbortSignal));
    need((savedBrowser === undefined || mode === "restart") && (signupBrowser === undefined || mode === "bootstrap"));
    need(typeof includeSampleResume === "boolean" && (!includeSampleResume || signupBrowser !== undefined));
    const includeMockReceipt = reviewFirstMockFixture !== undefined;
    need(!includeMockReceipt || (includeSampleResume && prepareFixture === undefined
      && exact(reviewFirstMockFixture, "artifactPath,artifactSha256")));
    const includePrepare = prepareFixture !== undefined || includeMockReceipt;
    need(prepareFixture === undefined || (includeSampleResume && exact(prepareFixture, "artifactPath,artifactSha256")));
    const { signal } = args;
    const rootStamp = await assertLocalQaLinuxPostgresRunRoot(root);
    let lease, pg, store, smtp, api, bindApiStop, postingSeeder, mockWorker, closePromise;
    let closed = false, uncertain = false;
    const assertOwnedUnit = async observed => {
      requireLocalQaLinuxC2cEnvironment(process.env);
      need(await assertLocalQaLinuxPostgresRunRoot(root) === rootStamp);
      need(await pgBinding.assertOwnedUnit(Object.freeze({ ...observed, root, stateDirectory, unitCgroup,
        networkNamespace, buildBinding })) === true);
      need(await assertLocalQaLinuxPostgresRunRoot(root) === rootStamp);
      return true;
    };
    const current = async () => {
      need(!closed);
      signal?.throwIfAborted();
      await assertOwnedUnit();
      if (lease) await assertLocalQaSessionLease(stateDirectory, lease);
      signal?.throwIfAborted();
      return true;
    };
    const close = () => {
      if (closePromise) return closePromise;
      closed = true;
      return closePromise = (async () => {
        try {
          need(!uncertain);
          await assertOwnedUnit();
          await mockWorker?.close();
          await postingSeeder?.close();
          await api?.close();
          await smtp?.close();
          await store?.close();
          await pg?.close();
          if (lease) await releaseLocalQaSessionLease(stateDirectory, lease);
        } catch { throw new Error(UNCLOSED); }
      })();
    };
    try {
      await current();
      const executable = await prepareLocalQaLinuxApiProcess({ root, stateDirectory, port: apiPort, buildBinding,
        unitCgroup, networkNamespace, assertOwnedUnit, bindApiStop: input => {
          need(typeof bindApiStop === "function"); return bindApiStop(input);
        } });
      if (includePrepare) {
        const { createLocalQaDurablePostingSeeder, createLocalQaDurableMockReceiptActor } =
          await import("./local-qa-durable-posting-seeder.mjs");
        const fixture = includeMockReceipt ? reviewFirstMockFixture : prepareFixture;
        postingSeeder = await createLocalQaDurablePostingSeeder({ root, stateDirectory, ...fixture,
          fixtureMode: includeMockReceipt ? "review_first_mock_receipt" : "imported_resume" });
        if (includeMockReceipt) {
          mockWorker = await createLocalQaDurableMockReceiptActor({ root, stateDirectory, ...fixture });
        }
      }
      if (mode === "bootstrap") {
        let absent = false;
        try { await lstat(stateDirectory); } catch (error) { if (error?.code === "ENOENT") absent = true; else throw error; }
        need(absent);
        await current();
        await initializeLocalQaState(stateDirectory);
      }
      const state = await inspectLocalQaState(stateDirectory);
      lease = await acquireLocalQaSessionLease(stateDirectory);
      await current();
      const config = mode === "bootstrap" ? await initializeLocalQaSessionState(stateDirectory, lease)
        : await inspectLocalQaSessionState(stateDirectory);
      need(mode === "bootstrap" ? !config.bootstrapIntentPresent && !config.bootstrapReceiptPresent
        : config.bootstrapIntentPresent && config.bootstrapReceiptPresent);
      const sessionSecrets = await loadLocalQaSessionSecrets(stateDirectory);
      const secrets = await loadLocalQaSecrets(stateDirectory);
      const postgres = await createLocalQaLinuxPgRuntime({ ...pgBinding, assertOwnedUnit });
      if (mode === "bootstrap") await postgres.initialize({ signal });
      pg = await postgres.start({ signal });
      let receiptAuthority, expectedIds, browserBootstrapCommitted = false;
      await current();
      if (mode === "bootstrap") {
        ({ receiptAuthority } = await writeLocalQaBootstrapIntent(stateDirectory, lease, { systemIdentifier: pg.systemIdentifier }));
        need(receiptAuthority);
      } else expectedIds = await requireLocalQaBootstrapReceipt(stateDirectory, pg.systemIdentifier);
      bindApiStop = await createLocalQaLinuxApiPidfdBinder({ root, stateDirectory, unitCgroup, networkNamespace,
        pidfdToolchain, assertOwnedUnit });
      store = await startLocalQaObjectStore({ directory: state.objectDirectory, qaId: state.qaId, bucket: state.bucket,
        prefix: state.prefix, accessKey: secrets.objectAccessKey, secret: secrets.objectSecret, port: objectPort });
      await current();
      smtp = await startLocalQaSmtp({ recipient: mode === "bootstrap" ? config.operatorEmail : config.candidateEmail, port: smtpPort });
      await current();
      api = await executable.start({ env: buildLocalQaLinuxApiEnvironment({ stateDirectory, root, apiPort, config, sessionSecrets,
        secrets, pg, store, smtp }), signal });
      const verifyApi = async () => {
        await current();
        need(await pg.assertCurrent() === true);
        need(await api.assertCurrent() === true);
        await current();
        return true;
      };
      const call = createLocalQaLinuxApiTransport({ apiPort, signal, assertCurrent: verifyApi,
        allowPrepareCommunications: includePrepare });
      let switched = false, browserResumeAssetId, verifyResumeReadback, browserPrepared, browserMockReceipt;
      const readScopedResume = (resumeId, token) =>
        createLocalQaLinuxApiTransport({ apiPort, signal, resumeId, assertCurrent: verifyApi })(
          `/api/jobs/resume-versions/${resumeId}`, { token },
        );
      const authorized = await authenticateLocalQaLinuxApi({ mode, sessionConfig: config, sessionSecrets, call, signal, expectedIds,
        takeSignupCode: () => smtp.takeCode("signup"),
        commitBrowserBootstrap: includePrepare ? async ids => {
          need(!browserBootstrapCommitted && receiptAuthority && exact(ids, "candidateId,operatorId"));
          await verifyApi();
          await writeLocalQaBootstrapReceipt(stateDirectory, lease, { ...ids, systemIdentifier: pg.systemIdentifier,
            receiptAuthority });
          receiptAuthority = undefined;
          const durable = await requireLocalQaBootstrapReceipt(stateDirectory, pg.systemIdentifier);
          need(durable.candidateId === ids.candidateId && durable.operatorId === ids.operatorId);
          await verifyApi();
          browserBootstrapCommitted = true;
          return true;
        } : undefined,
        signupCandidateInBrowser: signupBrowser === undefined ? undefined : async (password, bindBootstrap) => {
          // The saved-account actor does not load or package the writable signup driver.
          const gateway = await import("./local-qa-linux-signup-gateway.mjs");
          let mockReceiptSetup;
          const result = await gateway.runLocalQaLinuxBrowserSignup({ browser: signupBrowser, buildBinding,
            backendPort: apiPort, password, includeSampleResume, assertOwned: verifyApi, signal,
            takeSignupCode: () => smtp.takeCode("signup"),
            setupMockReceipt: includeMockReceipt ? async draft => {
              need(typeof mockReceiptSetup === "function");
              return mockReceiptSetup(draft);
            } : undefined,
            setupPrepare: includePrepare ? async ({ accountId, assetId }) => {
              const { createLocalQaImportedResumeMatch } = await import("./local-qa-imported-resume-prepare.mjs");
              const scoped = await bindLocalQaLinuxImportedCandidate({ call, candidatePassword: password, accountId, signal,
                readBoundResume: includeMockReceipt ? readScopedResume : undefined });
              const asset = await scoped.readResumeSource();
              const { profile } = await scoped.readWorkspace();
              gateway.requireLocalQaSampleResumeReadback({ asset, profile, assetId });
              const makeMatch = createLocalQaImportedResumeMatch({ ...scoped, mockReceipt: includeMockReceipt, async seedPosting(input) {
                need(input.accountId === accountId && typeof bindBootstrap === "function");
                await verifyApi();
                need(await bindBootstrap(accountId) === true && browserBootstrapCommitted);
                await verifyApi();
                const id = await postingSeeder.operation({ ...input, systemIdentifier: pg.systemIdentifier,
                  databaseUrl: pg.databaseUrl, dataKey: secrets.jobsDataKey }, { signal });
                await verifyApi();
                need(typeof id === "string" && /^[A-Za-z0-9_-]{1,160}$/.exec(id)?.[0] === id);
                return id;
              } });
              const { jobId } = await makeMatch({ accountId, assetId });
              mockReceiptSetup = includeMockReceipt ? async draft => {
                need(exact(draft, "accountId,applicationId,jobId,resumeId,sourceAssetId,sourceSha256")
                  && draft.accountId === accountId && draft.jobId === jobId && draft.sourceAssetId === assetId
                  && draft.sourceSha256 === asset.sha256 && [draft.applicationId, draft.resumeId]
                    .every(value => RESUME.exec(value)?.[0] === value)
                  && scoped.account?.id === accountId && scoped.account.email === config.candidateEmail
                  && scoped.account.is_admin === false && scoped.account.is_temporary === false
                  && typeof scoped.readResume === "function");
                const frozen = Object.freeze({ ...draft });
                const sameDraft = value => need(exact(value, "accountId,applicationId,jobId,resumeId,sourceAssetId,sourceSha256")
                  && Object.entries(frozen).every(([key, expected]) => value[key] === expected));
                return Object.freeze({
                  async readOwnedState(value) {
                    sameDraft(value);
                    await verifyApi();
                    const workspace = await scoped.readWorkspace();
                    const resume = await scoped.readResume(frozen.resumeId);
                    const application = workspace?.applications?.[0];
                    const phase = application?.state === "awaiting_review" ? "prepared"
                      : application?.state === "submitted" ? "submitted" : undefined;
                    need(Array.isArray(workspace?.applications) && workspace.applications.length === 1
                      && application.id === frozen.applicationId && phase !== undefined);
                    const census = await pg.readReviewFirstReceiptCensus(
                      frozen.accountId, frozen.applicationId, phase, application.run_id,
                    );
                    await verifyApi();
                    return Object.freeze({ account: scoped.account, census, workspace, resume });
                  },
                  async runMockWorker(value) {
                    sameDraft(value);
                    await verifyApi();
                    const admitted = store.admitMockReceipt({ accountId: frozen.accountId, applicationId: frozen.applicationId });
                    need(admitted.accountId === frozen.accountId && admitted.applicationId === frozen.applicationId);
                    const before = store.counters();
                    const result = await mockWorker.operation(frozen, { systemIdentifier: pg.systemIdentifier,
                      databaseUrl: pg.databaseUrl, dataKey: secrets.jobsDataKey, objectEnv: store.env, signal });
                    await verifyApi();
                    const after = store.counters();
                    need(after.put === before.put + 3 && after.get === before.get + 3
                      && after.delete === before.delete && after.denied === before.denied);
                    return result;
                  },
                  async readBoundResume(value) {
                    need(exact(value, "resumeId,token") && value.resumeId === frozen.resumeId
                      && typeof value.token === "string");
                    await verifyApi();
                    const resume = await readScopedResume(value.resumeId, value.token);
                    await verifyApi();
                    return resume;
                  },
                });
              } : undefined;
              return Object.freeze({ jobId, prepareDraft: () => scoped.prepareDraft(jobId),
                readEmptyCommunicationActions: scoped.readEmptyCommunicationActions });
            } : undefined });
          if (includeSampleResume) {
            browserResumeAssetId = result.resumeAssetId;
            verifyResumeReadback = gateway.requireLocalQaSampleResumeReadback;
            browserPrepared = result.prepared;
            browserMockReceipt = result.mockReceipt;
          }
          return Object.freeze({ accountId: result.accountId });
        },
        readBoundResume: savedBrowser === undefined ? undefined : readScopedResume,
        readPreparedResume: includePrepare ? (resumeId, token) => {
          need(resumeId === browserPrepared?.resumeId);
          return readScopedResume(resumeId, token);
        } : undefined,
        async switchSignupRecipient(recipient) {
          need(mode === "bootstrap" && !switched && recipient === config.candidateEmail);
          switched = true;
          await current();
          await smtp.close();
          smtp = undefined;
          smtp = await startLocalQaSmtp({ recipient: config.candidateEmail, port: smtpPort });
          await current();
          return true;
        } });
      await current();
      if (mode === "bootstrap" && !browserBootstrapCommitted) {
        await writeLocalQaBootstrapReceipt(stateDirectory, lease, { operatorId: authorized.operatorId,
          candidateId: authorized.candidateId, systemIdentifier: pg.systemIdentifier, receiptAuthority });
      }
      const ids = await requireLocalQaBootstrapReceipt(stateDirectory, pg.systemIdentifier);
      need(ids.operatorId === authorized.operatorId && ids.candidateId === authorized.candidateId);
      if (signupBrowser !== undefined) {
        await verifyApi();
        if (includeSampleResume) {
          need(typeof browserResumeAssetId === "string" && typeof verifyResumeReadback === "function");
          const asset = await authorized.readResumeSource();
          const workspace = await authorized.readWorkspace();
          verifyResumeReadback({ asset, profile: workspace.profile, assetId: browserResumeAssetId,
            onboardingComplete: includePrepare });
          if (includePrepare && !includeMockReceipt) {
            const { requireImportedQaPrepared } = await import("./local-qa-imported-resume-prepare.mjs");
            need(exact(browserPrepared, "applicationId,jobId,resumeId"));
            requireImportedQaPrepared({ workspace, resume: await authorized.readResume(browserPrepared.resumeId),
              assetId: browserResumeAssetId, ...browserPrepared });
          }
        }
        let census;
        if (includeMockReceipt) {
          need(exact(browserPrepared, "applicationId,jobId,resumeId")
            && exact(browserMockReceipt, "applicationCount,mockReceiptObserved,submittedCount")
            && browserMockReceipt.applicationCount === 1 && browserMockReceipt.submittedCount === 1
            && browserMockReceipt.mockReceiptObserved === true);
          const workspace = await authorized.readWorkspace();
          const resume = await authorized.readResume(browserPrepared.resumeId);
          const application = workspace?.applications?.[0];
          need(Array.isArray(workspace?.applications) && workspace.applications.length === 1
            && application?.id === browserPrepared.applicationId && application.job_id === browserPrepared.jobId
            && application.resume_version_id === browserPrepared.resumeId && application.state === "submitted"
            && application.submission_mode === "review_first" && application.submitted_at_ms > 0
            && resume?.id === browserPrepared.resumeId && resume.job_id === browserPrepared.jobId
            && resume.mode === "factual");
          census = await pg.readReviewFirstReceiptCensus(
            ids.candidateId, browserPrepared.applicationId, "submitted", application.run_id,
          );
        } else {
          census = await pg.readC2cCensus(ids.candidateId);
          need(census.accounts === 1 && (includeSampleResume ? census.profiles === 1 : census.profiles <= 1));
          const preparedRows = ["applications", "prepared", "resumes", "factual_draft_links"];
          for (const [key, count] of Object.entries(census)) {
            if (!["accounts", "profiles", "entitlement"].includes(key)) {
              const expected = includePrepare && preparedRows.includes(key) ? 1 : 0;
              need(count === expected);
            }
          }
        }
        const { captured, denied, duplicate } = smtp.counters();
        need(captured === 1 && denied === 0 && duplicate === 0 && smtp.takeCode("signup") === null);
        await verifyApi();
        await close();
        return Object.freeze({ browserSignup: true, candidateWorkspace: true, signupMailCount: captured,
          applicationCount: census.applications, submittedCount: census.submitted,
          ...(includeSampleResume ? { sampleResumeImported: true } : {}),
          ...(includePrepare && !includeMockReceipt ? { reviewFirstPrepared: true } : {}),
          ...(includeMockReceipt ? { mockReceiptObserved: true } : {}) });
      }
      if (savedBrowser !== undefined) {
        need(mode === "restart");
        const browser = validateLocalQaLinuxC2cBrowserBinding(savedBrowser.browser, [apiPort, pg.port, objectPort, smtpPort]);
        const evidence = await runLocalQaLinuxC2cRestartGateway({ browser, buildBinding, backendPort: apiPort,
          preparedDraft: savedBrowser.preparedDraft, accountId: ids.candidateId, signal,
          credentials: Object.freeze({ email: config.candidateEmail, password: sessionSecrets.candidatePassword }),
          assertOwned: verifyApi, readWorkspace: authorized.readWorkspace, readResume: authorized.readResume,
          readCensus: async () => {
            await verifyApi();
            const result = await pg.readC2cCensus(ids.candidateId);
            await verifyApi();
            return result;
          },
          readMailAttempts: async () => {
            await current();
            const { captured, denied, duplicate } = smtp.counters();
            return captured + denied + duplicate;
          },
        });
        await close();
        return evidence;
      }
      return requireLocalQaLinuxC2cSession({
        accountId: authorized.candidateId, candidateEmail: config.candidateEmail, databaseUrl: pg.databaseUrl,
        dataKey: secrets.jobsDataKey, systemIdentifier: pg.systemIdentifier, postgresRuntime: pg.postgresRuntime,
        queryIdentity: pg.queryIdentity, readWorkspace: authorized.readWorkspace, readTaxonomy: authorized.readTaxonomy,
        saveOnboarding: authorized.saveOnboarding, prepareDraft: authorized.prepareDraft, close,
      });
    } catch (error) {
      uncertain = /(?:unclosed|cleanup_unproven)$/i.test(error?.message ?? "");
      await close();
      throw new Error(REFUSED);
    }
  };
  return Object.freeze({ startOwnedSession: args => open("bootstrap", args), openSavedSession: args => open("restart", args),
    runSavedC2cBrowser: (args, options) => open("restart", args, { savedBrowser: options }),
    runBrowserSignup: (args, browser, includeSampleResume, prepareFixture,
      reviewFirstMockFixture) => open("bootstrap", args, { signupBrowser: browser, includeSampleResume,
      prepareFixture, reviewFirstMockFixture }) });
}

// Separate from the saved-draft journey: this fresh run uses its sole browser
// context for account creation, then closes all services without retaining access.
export async function runLocalQaLinuxOwnedBrowserSignup({ binding, browser, signal, includeSampleResume = false,
  prepareFixture, reviewFirstMockFixture }) {
  const { pgBinding, apiPort, objectPort, smtpPort } = validateLocalQaLinuxOwnedApiBinding(binding);
  const scopedBrowser = validateLocalQaLinuxC2cBrowserBinding(browser, [apiPort, pgBinding.port, objectPort, smtpPort]);
  const { runBrowserSignup } = createLocalQaLinuxOwnedApiSession(binding);
  return runBrowserSignup({ root: pgBinding.root, stateDirectory: pgBinding.stateDirectory, signal }, scopedBrowser,
    includeSampleResume, prepareFixture, reviewFirstMockFixture);
}

export async function runLocalQaLinuxOwnedC2cJourney({ binding, fixture, browser, signal }) {
  const { pgBinding, apiPort, objectPort, smtpPort } = validateLocalQaLinuxOwnedApiBinding(binding);
  const scopedBrowser = validateLocalQaLinuxC2cBrowserBinding(browser, [apiPort, pgBinding.port, objectPort, smtpPort]);
  const { startOwnedSession, runSavedC2cBrowser } = createLocalQaLinuxOwnedApiSession(binding);
  const preparedDraft = await prepareLocalQaLinuxC2cDraft({ root: pgBinding.root, fixture, startOwnedSession, signal });
  const evidence = await runSavedC2cBrowser({ root: pgBinding.root, stateDirectory: pgBinding.stateDirectory, signal },
    Object.freeze({ browser: scopedBrowser, preparedDraft }));
  return Object.freeze({ preparedDraft, evidence });
}

export async function runLocalQaLinuxSealedC2cJourney({ binding, unit, fixture, browser, signal }) {
  need(exact(unit, "bootId,invocationId,mainPid,systemctl,unitName")
    && exact(binding, "apiPort,buildBinding,networkNamespace,objectPort,pgCapsule,pidfdToolchain,postgresPort,root,smtpPort,unitCgroup"));
  const { root, unitCgroup, networkNamespace, buildBinding } = binding;
  const assertOwnedUnit = await createLocalQaLinuxC2cUnitAssertion({ ...unit, root, unitCgroup, networkNamespace, buildBinding });
  return runLocalQaLinuxOwnedC2cJourney({ binding: Object.freeze({ ...binding, assertOwnedUnit }), fixture, browser, signal });
}

export async function prepareLocalQaLinuxOwnedC2cDraft({ binding, fixture, signal }) {
  const { startOwnedSession } = createLocalQaLinuxOwnedApiSession(binding);
  return prepareLocalQaLinuxC2cDraft({ root: binding.root, fixture, startOwnedSession, signal });
}
