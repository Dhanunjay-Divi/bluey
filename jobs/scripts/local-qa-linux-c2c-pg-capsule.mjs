// Pure binding for the sealed Noble PGDG PostgreSQL 17.11 / pgvector 0.8.7
// capsule.  This module neither reads the filesystem nor starts a process.
import { createHash } from "node:crypto";
import { posix } from "node:path";

const REFUSED = "BLUEY_LOCAL_QA_LINUX_C2C_PG_CAPSULE_REFUSED";
const ROOT = "/run/bluey-jobs-c2c-postgres";
const ARCHIVE_SHA256 = "9a09356e9eb7ffa8ddf4e0dde66fbbb3c963ec1ce3e4b0c1a79a321cba94bdfa";
const MANIFEST_SHA256 = "e0f68a27c049e69aff5d789051ad731ddce5f41d120e46bdd21ba994bb46dc92";
const RUN = /^[0-9a-f]{32}$/;
const { join } = posix;
const PLAN_KEYS = Object.freeze([
  "version", "run", "capsuleRoot", "archiveSha256", "manifestSha256", "tools", "sharePath",
  "extensionLibraryPath", "libpqSearchPath", "libraries", "environment", "postgresArguments",
  "vectorVersion", "runtimeQualified",
]);
const own = value => value !== null && typeof value === "object"
  && Object.getPrototypeOf(value) === Object.prototype && !Array.isArray(value);
const keys = (value, expected) => own(value) && Reflect.ownKeys(value).length === expected.length
  && expected.every(key => Object.hasOwn(value, key));
const fail = () => { throw new Error(REFUSED); };
const need = value => { if (!value) fail(); };
const whole = (pattern, value) => typeof value === "string" && pattern.exec(value)?.[0] === value;
const sha = value => createHash("sha256").update(value).digest("hex");

const ROWS = Object.freeze({
  "usr/lib/postgresql/17/bin/initdb": ["file", 365, 122528, "713c98780fb3a85f8e9105b5be6e1bbe1de77f8d1e3dbd0ee11d99627796d0f2"],
  "usr/lib/postgresql/17/bin/pg_controldata": ["file", 365, 51624, "7c115e70c6fd45b0ab40c0456e92215a0ccceba0995058dd8608769175105b6d"],
  "usr/lib/postgresql/17/bin/postgres": ["file", 365, 11049336, "a8de97eb871cee1fdb84cad48ec4e2c481ac826c3801d9e8bb35adec971f7e43"],
  "usr/lib/postgresql/17/bin/psql": ["file", 365, 794200, "3dfa07948610868b5b97e292eaa294651a599b338e792a96ab0e4191f06a2c4a"],
  "usr/lib/postgresql/17/lib/vector.so": ["file", 292, 194632, "8ab9e382348afb69f4d521ec7cda7ca779362486508b8db5572a82ddab690a1a"],
  "usr/lib/x86_64-linux-gnu/libpq.so.5": ["link", 511, "libpq.so.5.18"],
  "usr/lib/x86_64-linux-gnu/libpq.so.5.18": ["file", 292, 387248, "e6f752c5ad7005afab0d9ea2932b93d6ad2d863b4333a55aafa1cdd734e74bde"],
  "usr/share/postgresql/17/extension/vector--0.8.7.sql": ["file", 292, 42850, "a4ceca5931cb614b40bec09ce6994a57058513d9af612c15b1458a338134609d"],
  "usr/share/postgresql/17/extension/vector.control": ["file", 292, 145, "9f7d0fcada5e68aa41adf9d5b3ab3fd43c83f2edd462c391a0f715d72a9941ec"],
});

const LIBRARIES = Object.freeze({
  vector: "usr/lib/postgresql/17/lib/vector.so",
  vectorControl: "usr/share/postgresql/17/extension/vector.control",
  vectorSql: "usr/share/postgresql/17/extension/vector--0.8.7.sql",
  libpq: "usr/lib/x86_64-linux-gnu/libpq.so.5.18",
  libpqLink: "usr/lib/x86_64-linux-gnu/libpq.so.5",
});

function libraryPin(capsuleRoot, relative) {
  const [type, mode, sizeOrTarget, sha256] = ROWS[relative];
  const base = { path: join(capsuleRoot, relative), type, mode };
  return type === "link"
    ? { ...base, target: sizeOrTarget }
    : { ...base, bytes: sizeOrTarget, sha256 };
}

function parseRows(raw) {
  try {
    need(Buffer.isBuffer(raw) && raw.length > 0 && raw.length <= 512 * 1024 && sha(raw) === MANIFEST_SHA256);
    const values = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(raw));
    need(Array.isArray(values));
    const selected = new Set();
    for (const row of values) {
      need(Array.isArray(row) && typeof row[0] === "string" && typeof row[1] === "string"
        && ((row[1] === "file" && row.length === 5 && Number.isSafeInteger(row[2])
          && Number.isSafeInteger(row[3]) && typeof row[4] === "string")
          || (row[1] === "link" && row.length === 4 && Number.isSafeInteger(row[2]) && typeof row[3] === "string")));
      if (Object.hasOwn(ROWS, row[0])) {
        need(!selected.has(row[0]) && JSON.stringify(row.slice(1)) === JSON.stringify(ROWS[row[0]]));
        selected.add(row[0]);
      }
    }
    need(selected.size === Object.keys(ROWS).length);
  } catch { fail(); }
}

/** Derive the fixed expected inputs for a run. This asserts no measurement. */
export function expectedLocalQaLinuxC2cPgCapsule(run) {
  need(whole(RUN, run) && run !== "0".repeat(32));
  const capsuleRoot = join(ROOT, run, "capsule");
  const bin = join(capsuleRoot, "usr/lib/postgresql/17/bin");
  const extensionLibraryPath = join(capsuleRoot, "usr/lib/postgresql/17/lib");
  const sharePath = join(capsuleRoot, "usr/share/postgresql/17");
  const libpqSearchPath = join(capsuleRoot, "usr/lib/x86_64-linux-gnu");
  const tools = Object.freeze(Object.fromEntries(["initdb", "pg_controldata", "postgres", "psql"].map(name => {
    const path = join(bin, name), row = ROWS[`usr/lib/postgresql/17/bin/${name}`];
    return [name, Object.freeze({ path, bytes: row[2], sha256: row[3], mode: 0o555 })];
  })));
  const libraries = Object.freeze(Object.fromEntries(Object.entries(LIBRARIES).map(([name, relative]) =>
    [name, Object.freeze(libraryPin(capsuleRoot, relative))])));
  // HOME is supplied only by the separate actor-owned state binder.
  const environment = Object.freeze({ LANG: "C", LC_ALL: "C", PATH: "/usr/bin:/bin", TZ: "UTC",
    LD_LIBRARY_PATH: libpqSearchPath });
  const postgresArguments = Object.freeze(["-c", `dynamic_library_path=${extensionLibraryPath}`]);
  return Object.freeze({ version: 1, run, capsuleRoot, archiveSha256: ARCHIVE_SHA256,
    manifestSha256: MANIFEST_SHA256, tools, sharePath, extensionLibraryPath, libpqSearchPath, libraries,
    environment, postgresArguments, vectorVersion: "0.8.7", runtimeQualified: false });
}

/**
 * Bind an independently measured archive and retained manifest to the expected
 * plan. The caller must still verify physical staged inputs before use.
 */
export function deriveLocalQaLinuxC2cPgCapsule({ run, manifestBytes, archiveSha256 }) {
  need(archiveSha256 === ARCHIVE_SHA256);
  parseRows(manifestBytes);
  return expectedLocalQaLinuxC2cPgCapsule(run);
}

/** Validate a root-owned staged plan without executing any capsule member. */
export function validateLocalQaLinuxC2cPgCapsulePlan(plan) {
  try {
    need(keys(plan, PLAN_KEYS) && plan.version === 1 && whole(RUN, plan.run)
      && plan.run !== "0".repeat(32)
      && plan.capsuleRoot === join(ROOT, plan.run, "capsule") && plan.archiveSha256 === ARCHIVE_SHA256
      && plan.manifestSha256 === MANIFEST_SHA256 && plan.vectorVersion === "0.8.7"
      && plan.extensionLibraryPath === join(plan.capsuleRoot, "usr/lib/postgresql/17/lib")
      && plan.sharePath === join(plan.capsuleRoot, "usr/share/postgresql/17")
      && plan.libpqSearchPath === join(plan.capsuleRoot, "usr/lib/x86_64-linux-gnu")
      && keys(plan.tools, ["initdb", "pg_controldata", "postgres", "psql"])
      && ["initdb", "pg_controldata", "postgres", "psql"].every(name => {
        const row = ROWS[`usr/lib/postgresql/17/bin/${name}`], tool = plan.tools[name];
        return keys(tool, ["bytes", "mode", "path", "sha256"])
          && tool.path === join(plan.capsuleRoot, "usr/lib/postgresql/17/bin", name)
          && tool.bytes === row[2] && tool.sha256 === row[3] && tool.mode === 0o555;
      })
      && keys(plan.libraries, Object.keys(LIBRARIES))
      && Object.entries(LIBRARIES).every(([name, relative]) => {
        const expected = libraryPin(plan.capsuleRoot, relative), supplied = plan.libraries[name];
        return keys(supplied, Object.keys(expected))
          && Object.entries(expected).every(([field, value]) => supplied[field] === value);
      })
      && keys(plan.environment, ["LANG", "LC_ALL", "LD_LIBRARY_PATH", "PATH", "TZ"])
      && plan.environment.LANG === "C" && plan.environment.LC_ALL === "C" && plan.environment.PATH === "/usr/bin:/bin"
      && plan.environment.TZ === "UTC" && plan.environment.LD_LIBRARY_PATH === plan.libpqSearchPath
      && Array.isArray(plan.postgresArguments) && plan.postgresArguments.length === 2
      && Reflect.ownKeys(plan.postgresArguments).length === 3
      && Object.hasOwn(plan.postgresArguments, "0") && Object.hasOwn(plan.postgresArguments, "1")
      && plan.postgresArguments[0] === "-c" && plan.postgresArguments[1] === `dynamic_library_path=${plan.extensionLibraryPath}`
      && plan.runtimeQualified === false);
    return true;
  } catch { fail(); }
}

export const LOCAL_QA_LINUX_C2C_PG_CAPSULE_PINS = Object.freeze({ archiveSha256: ARCHIVE_SHA256, manifestSha256: MANIFEST_SHA256 });
