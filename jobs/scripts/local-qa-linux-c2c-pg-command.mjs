// Pure command composition for the root-verified C2C PGDG capsule.  The
// caller owns physical validation of the actor state and process lifecycle.
import { posix } from "node:path";
import { validateLocalQaLinuxC2cPgCapsulePlan } from "./local-qa-linux-c2c-pg-capsule.mjs";
import { isDurableQaFixtureRoot } from "./local-qa-durable-posting-seeder.mjs";

const REFUSED = "BLUEY_LOCAL_QA_LINUX_C2C_PG_COMMAND_REFUSED";
const { dirname, join } = posix;
const ROLE = "bluey_qa_local";
const DATABASES = Object.freeze(["bluey_jobs_qa", "postgres"]);
const STATE_KEYS = Object.freeze(["dataDirectory", "home", "passwordFile"]);
const INPUT_KEYS = Object.freeze(["capsulePlan", "port", "state"]);

const fail = () => { throw new Error(REFUSED); };
const need = value => { if (!value) fail(); };
const plain = value => value !== null && typeof value === "object"
  && Object.getPrototypeOf(value) === Object.prototype && !Array.isArray(value);
const exactKeys = (value, names) => plain(value) && Reflect.ownKeys(value).length === names.length
  && names.every(name => Object.hasOwn(value, name));
function validateState(state) {
  need(exactKeys(state, STATE_KEYS) && typeof state.home === "string");
  const root = dirname(state.home);
  need(isDurableQaFixtureRoot(root, "linux") && state.home === join(root, "bluey-jobs-qa-draft")
    && state.dataDirectory === join(state.home, "postgres")
    && state.passwordFile === join(state.home, ".local-qa-postgres-password"));
  return Object.freeze({ dataDirectory: state.dataDirectory, home: state.home, passwordFile: state.passwordFile });
}

function validatePort(value) {
  need(Number.isSafeInteger(value) && value > 1024 && value <= 65535 && value !== 5432);
  return value;
}

function frozenEnvironment(plan, state) {
  const environment = {
    HOME: state.home,
    LANG: plan.environment.LANG,
    LC_ALL: plan.environment.LC_ALL,
    LD_LIBRARY_PATH: plan.environment.LD_LIBRARY_PATH,
    PATH: plan.environment.PATH,
    TZ: plan.environment.TZ,
  };
  need(exactKeys(environment, ["HOME", "LANG", "LC_ALL", "LD_LIBRARY_PATH", "PATH", "TZ"]));
  return Object.freeze(environment);
}

function command(path, argv, environment, cwd) {
  return Object.freeze({ path, argv: Object.freeze([...argv]), env: environment, cwd });
}

/**
 * Compose the only permitted actor-side invocations for PGDG 17.11/vector
 * 0.8.7.  It never launches a process and never incorporates process.env.
 */
export function composeLocalQaLinuxC2cPgCommands(input) {
  try {
    need(exactKeys(input, INPUT_KEYS));
    validateLocalQaLinuxC2cPgCapsulePlan(input.capsulePlan);
    const plan = input.capsulePlan;
    const state = validateState(input.state);
    const port = validatePort(input.port);
    const env = frozenEnvironment(plan, state);
    const prefix = ["-X", "-w", "-q", "-A", "-t", "-h", "127.0.0.1", "-p", String(port), "-U", ROLE,
      "-v", "ON_ERROR_STOP=1"];
    const psql = Object.freeze(Object.fromEntries(DATABASES.map(database => [database, command(plan.tools.psql.path,
      [...prefix, "-d", database], env, state.home)])));
    return Object.freeze({
      initdb: command(plan.tools.initdb.path, ["-D", state.dataDirectory, "-L", plan.sharePath, "-U", ROLE,
        "--auth-local=scram-sha-256", "--auth-host=scram-sha-256", "--pwfile", state.passwordFile,
        "--encoding=UTF8", "--no-locale"], env, state.home),
      postgres: command(plan.tools.postgres.path, ["-D", state.dataDirectory, "-h", "127.0.0.1", "-p", String(port),
        "-k", "", "-c", "ssl=off", "-c", "max_connections=64", ...plan.postgresArguments], env, state.home),
      pgControldata: command(plan.tools.pg_controldata.path, [state.dataDirectory], env, state.home),
      psql,
      vectorVersion: plan.vectorVersion,
      runtimeQualified: false,
    });
  } catch { fail(); }
}
