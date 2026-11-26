// Owned nonproduction orchestration only. The Linux runner supplies the
// independently fenced startup/authentication and unit/SQL assertions.
import { attachLocalQaLinuxPostgres } from "./local-qa-linux-postgres.mjs";
import { createLocalQaLinuxPostmasterObserver } from "./local-qa-linux-postmaster-observer.mjs";
import { createLocalQaDurableDraft } from "./local-qa-durable-draft.mjs";
import {
  createLocalQaDurablePostingSeeder,
  isDurableQaFixtureRoot,
  parseDurableQaPostgresUrl,
} from "./local-qa-durable-posting-seeder.mjs";
import { join, resolve } from "node:path";

const CANDIDATE = "candidate@bluey-qa.test";
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const SHA256 = /^[0-9a-f]{64}$/;
const OPERATIONS = ["readWorkspace", "readTaxonomy", "saveOnboarding", "prepareDraft"];
const SESSION_KEYS = ["accountId", "candidateEmail", "databaseUrl", "dataKey", "systemIdentifier",
  "postgresRuntime", "queryIdentity", "close", ...OPERATIONS].sort().join(",");
const RUNTIME_KEYS = "assertOwnedUnit,networkNamespace,postgresPath,postgresSha256,unitCgroup";
const FORBIDDEN_ENV = ["BLUEY_DATABASE_URL", "BLUEY_TEST_POSTGRES_URL", "PGHOST", "PGPORT", "PGUSER",
  "PGDATABASE", "PGPASSWORD", "PGPASSFILE", "PGSERVICE", "PGSERVICEFILE", "PGOPTIONS",
  "BLUEY_POSTGRES_CA_CERT_PATH", "BLUEY_POSTGRES_ACCEPT_INVALID_CERTS", "BLUEY_POSTGRES_ACCEPT_INVALID_HOSTNAMES",
  "OPENAI_API_KEY", "OPENAI_API_KEYS", "ANTHROPIC_API_KEY", "ANTHROPIC_API_KEYS", "GEMINI_API_KEY",
  "GOOGLE_API_KEY", "DEEPSEEK_API_KEY", "BLUEY_JOBS_WORKFLOW_TOKEN", "BLUEY_JOBS_WORKFLOW_ORIGIN",
  "BLUEY_JOBS_GOOGLE_CLIENT_SECRET", "BLUEY_JOBS_MICROSOFT_CLIENT_SECRET"];
export const LOCAL_QA_LINUX_EFFECT_FLAGS = Object.freeze([
  "BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED", "BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED",
  "BLUEY_JOBS_MANAGED_CLOUD_RUNTIME_ENABLED", "BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED",
  "BLUEY_JOBS_WORKFLOW_COMMAND_RECONCILIATION_ENABLED", "BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED",
  "BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED", "BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED",
  "BLUEY_JOBS_MAILBOX_SYNC_ENABLED", "BLUEY_JOBS_MODEL_GENERATION_ENABLED", "BLUEY_JOBS_GLOBAL_ARCHIVE_ENABLED",
  "BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED"]);

function need(value, code) {
  if (!value) throw new Error(`local_qa_linux_c2c_${code}`);
}

const exactMatch = (regex, value) => typeof value === "string" && regex.exec(value)?.[0] === value;

export function requireLocalQaLinuxC2cEnvironment(env) {
  need(env && FORBIDDEN_ENV.every(name => env[name] === undefined), "ambient_authority");
  need(LOCAL_QA_LINUX_EFFECT_FLAGS.every(name => env[name] === undefined || env[name] === "0"), "effect_flags");
}

export function requireLocalQaLinuxC2cSession(session) {
  need(session && Object.getPrototypeOf(session) === Object.prototype
    && Object.keys(session).sort().join(",") === SESSION_KEYS, "session_shape");
  need(exactMatch(UUID, session.accountId) && session.candidateEmail === CANDIDATE,
    "candidate_scope");
  need(typeof session.dataKey === "string" && /^[A-Za-z0-9+/]{43}=$/.test(session.dataKey)
    && Buffer.from(session.dataKey, "base64").toString("base64") === session.dataKey,
  "data_key_shape");
  need(exactMatch(/^[1-9][0-9]{0,19}$/, session.systemIdentifier)
    && BigInt(session.systemIdentifier) <= 18_446_744_073_709_551_615n, "system_identity_shape");
  parseDurableQaPostgresUrl(session.databaseUrl);
  need(["queryIdentity", "close", ...OPERATIONS].every(name => typeof session[name] === "function"),
    "session_operations");
  const runtime = session.postgresRuntime;
  need(runtime && Object.getPrototypeOf(runtime) === Object.prototype
    && Object.keys(runtime).sort().join(",") === RUNTIME_KEYS, "runtime_binding");
  const { assertOwnedUnit, networkNamespace, postgresPath, postgresSha256, unitCgroup } = runtime;
  need(typeof assertOwnedUnit === "function" && exactMatch(/^net:\[[1-9][0-9]{0,19}\]$/, networkNamespace)
    && typeof postgresPath === "string" && postgresPath !== "/" && resolve(postgresPath) === postgresPath
    && !/[\0\r\n]/.test(postgresPath) && exactMatch(SHA256, postgresSha256)
    && exactMatch(/^\/[^\0\r\n]+$/, unitCgroup) && resolve(unitCgroup) === unitCgroup, "runtime_binding");
  return Object.freeze({ ...session, postgresRuntime: Object.freeze({
    assertOwnedUnit, networkNamespace, postgresPath, postgresSha256, unitCgroup,
  }) });
}

/**
 * Bootstrap one synthetic C2C draft, then close the backend before returning.
 * `startOwnedSession` must close partial allocations if startup rejects, and
 * must not return until its independent OS/invocation fence is active. Its API
 * operations and physical PostgreSQL query are bound to this exact session.
 */
export async function prepareLocalQaLinuxC2cDraft({ root, fixture, startOwnedSession, signal }) {
  need(process.platform === "linux" && root === process.env.BLUEY_TEST_RUN_ROOT && isDurableQaFixtureRoot(root), "run_root");
  requireLocalQaLinuxC2cEnvironment(process.env);
  need(typeof startOwnedSession === "function" && (signal === undefined || signal instanceof AbortSignal), "owner");
  need(fixture && Object.getPrototypeOf(fixture) === Object.prototype
    && Object.keys(fixture).sort().join(",") === "artifactPath,artifactSha256"
    && typeof fixture.artifactPath === "string" && exactMatch(SHA256, fixture.artifactSha256), "fixture_binding");
  signal?.throwIfAborted();
  const stateDirectory = join(root, "bluey-jobs-qa-draft");
  // Bind the artifact while durable state is still absent; the outer runner is
  // not allowed to hand a pre-existing state directory to this entrypoint.
  const seeder = await createLocalQaDurablePostingSeeder({ root, stateDirectory, ...fixture, fixtureMode: "c2c" });
  let acquired, phase = "startup";
  try {
    signal?.throwIfAborted();
    acquired = await startOwnedSession(Object.freeze({ root, stateDirectory, signal }));
    const session = requireLocalQaLinuxC2cSession(acquired);
    phase = "database_attachment";
    signal?.throwIfAborted();
    const assertOwnedPostmaster = await createLocalQaLinuxPostmasterObserver({ root, ...session.postgresRuntime });
    signal?.throwIfAborted();
    const postgres = await attachLocalQaLinuxPostgres({ root, databaseUrl: session.databaseUrl,
      dataKey: session.dataKey, expectedSystemIdentifier: session.systemIdentifier, expectedCandidateId: session.accountId,
      assertOwnedPostmaster, queryIdentity: session.queryIdentity });
    const current = async () => {
      signal?.throwIfAborted();
      requireLocalQaLinuxC2cEnvironment(process.env);
      await postgres.assertCurrent();
      signal?.throwIfAborted();
    };
    const guarded = run => async (...args) => {
      await current();
      const result = await run(...args);
      await current();
      return result;
    };
    const prepare = createLocalQaDurableDraft({ fixtureMode: "c2c",
      ...Object.fromEntries(OPERATIONS.map(name => [name, guarded((...args) => session[name](...args))])),
      seedPosting: guarded(({ accountId, trackId }) => {
        need(accountId === session.accountId, "candidate_scope");
        return seeder.operation({ accountId, trackId, systemIdentifier: postgres.systemIdentifier,
          databaseUrl: session.databaseUrl, dataKey: session.dataKey }, { signal });
      }) });
    phase = "prepare";
    return await prepare({ accountId: session.accountId, candidateEmail: CANDIDATE });
  } catch {
    // A partially attempted fixture must never be retried or expose secrets
    // contained in an exception from the caller's network/session helpers.
    throw new Error(`local_qa_linux_c2c_${phase}_failed`);
  } finally {
    let cleanupFailed = false;
    try { await seeder.close(); } catch { cleanupFailed = true; }
    if (acquired !== undefined) {
      try {
        need(typeof acquired?.close === "function", "session_close_missing");
        await acquired.close();
      } catch { cleanupFailed = true; }
    }
    need(!cleanupFailed, "cleanup_failed");
  }
}
