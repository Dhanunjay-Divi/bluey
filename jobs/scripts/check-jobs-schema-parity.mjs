import fs from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const JOBS_PARITY_TABLES = [
  "jobs_discovery_memberships",
  "jobs_discovery_runs",
  "jobs_discovery_sources",
  "jobs_execution_leases",
  "jobs_local_run_resume_actions",
];

const REQUIRED_INDEX_SIGNATURES = new Map([
  [
    "jobs_discovery_memberships",
    ["idx_jobs_discovery_memberships_job on jobs_discovery_memberships (account_id, job_id)"],
  ],
  [
    "jobs_discovery_runs",
    ["idx_jobs_discovery_runs_source on jobs_discovery_runs (source_id, started_at_ms desc)"],
  ],
  [
    "jobs_discovery_sources",
    [
      "idx_jobs_discovery_sources_due on jobs_discovery_sources (status, health, next_run_at_ms, lease_expires_at_ms)",
      "unique idx_jobs_discovery_sources_board_owner on jobs_discovery_sources (account_id, provider, source_key)",
    ].sort(),
  ],
  [
    "jobs_execution_leases",
    [
      "idx_jobs_execution_leases_binding on jobs_execution_leases (account_id, application_id, run_id)",
      "unique idx_jobs_execution_leases_active_application on jobs_execution_leases (application_id) where phase in ('prepared', 'click_started')",
      "unique idx_jobs_execution_leases_active_profile on jobs_execution_leases (browser_profile_id) where phase in ('prepared', 'click_started')",
    ].sort(),
  ],
  [
    "jobs_local_run_resume_actions",
    [
      "idx_jobs_local_resume_actions_application on jobs_local_run_resume_actions (account_id, application_id, created_at_ms desc)",
      "unique idx_jobs_local_resume_actions_active_run on jobs_local_run_resume_actions (run_id) where status = 'approved'",
    ].sort(),
  ],
]);

function balancedBody(sql, openingParen) {
  let depth = 0;
  let quote = null;
  for (let index = openingParen; index < sql.length; index += 1) {
    const character = sql[index];
    if (quote) {
      if (character === quote && sql[index + 1] === quote) {
        index += 1;
      } else if (character === quote) {
        quote = null;
      }
      continue;
    }
    if (character === "'" || character === '"') {
      quote = character;
    } else if (character === "(") {
      depth += 1;
    } else if (character === ")") {
      depth -= 1;
      if (depth === 0) return sql.slice(openingParen + 1, index);
    }
  }
  throw new Error("Unbalanced CREATE TABLE statement");
}

function splitTopLevel(value) {
  const parts = [];
  let start = 0;
  let depth = 0;
  let quote = null;
  for (let index = 0; index < value.length; index += 1) {
    const character = value[index];
    if (quote) {
      if (character === quote && value[index + 1] === quote) {
        index += 1;
      } else if (character === quote) {
        quote = null;
      }
      continue;
    }
    if (character === "'" || character === '"') quote = character;
    else if (character === "(") depth += 1;
    else if (character === ")") depth -= 1;
    else if (character === "," && depth === 0) {
      parts.push(value.slice(start, index));
      start = index + 1;
    }
  }
  parts.push(value.slice(start));
  return parts.map((part) => part.trim()).filter(Boolean);
}

function normalizeSql(value) {
  return value
    .replace(/--[^\n]*/g, " ")
    .replace(/\bbigint\b/gi, "INTEGER")
    .replace(/\s+/g, " ")
    .trim()
    .toLowerCase();
}

function extractTable(sql, tableName) {
  const expression = new RegExp(`CREATE\\s+TABLE\\s+IF\\s+NOT\\s+EXISTS\\s+${tableName}\\s*\\(`, "i");
  const match = expression.exec(sql);
  if (!match) return null;
  const openingParen = sql.indexOf("(", match.index);
  return splitTopLevel(balancedBody(sql, openingParen)).map(normalizeSql);
}

function extractIndexes(sql, tableName) {
  const indexes = [];
  const expression =
    /CREATE\s+(UNIQUE\s+)?INDEX\s+IF\s+NOT\s+EXISTS\s+([A-Za-z_][A-Za-z0-9_]*)\s+ON\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^;]+?)\)\s*(WHERE\s+[^;]+)?;/gi;
  for (const match of sql.matchAll(expression)) {
    if (match[3].toLowerCase() !== tableName.toLowerCase()) continue;
    indexes.push(
      normalizeSql(`${match[1] ? "unique " : ""}${match[2]} on ${match[3]} (${match[4]}) ${match[5] ?? ""}`),
    );
  }
  return [...new Set(indexes)].sort();
}

function parityTableNames(sql) {
  return [...sql.matchAll(/CREATE\s+TABLE\s+IF\s+NOT\s+EXISTS\s+(jobs_[A-Za-z0-9_]+)/gi)]
    .map((match) => match[1].toLowerCase())
    .filter(
      (tableName) =>
        tableName.startsWith("jobs_discovery_") ||
        tableName === "jobs_execution_leases" ||
        tableName === "jobs_local_run_resume_actions",
    )
    .sort();
}

function firstDifference(left, right) {
  const length = Math.max(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    if (left[index] !== right[index]) return { index, left: left[index], right: right[index] };
  }
  return null;
}

export function compareJobsSchemas(sqliteSource, postgresSource) {
  const issues = [];
  const expectedNames = [...JOBS_PARITY_TABLES].sort();
  const sqliteNames = parityTableNames(sqliteSource);
  const postgresNames = parityTableNames(postgresSource);

  if (JSON.stringify(sqliteNames) !== JSON.stringify(expectedNames)) {
    issues.push(`SQLite parity tables: expected ${expectedNames.join(", ")}; found ${sqliteNames.join(", ") || "none"}`);
  }
  if (JSON.stringify(postgresNames) !== JSON.stringify(expectedNames)) {
    issues.push(`Postgres parity tables: expected ${expectedNames.join(", ")}; found ${postgresNames.join(", ") || "none"}`);
  }

  for (const tableName of JOBS_PARITY_TABLES) {
    const sqliteTable = extractTable(sqliteSource, tableName);
    const postgresTable = extractTable(postgresSource, tableName);
    if (!sqliteTable || !postgresTable) continue;

    const tableDifference = firstDifference(sqliteTable, postgresTable);
    if (tableDifference) {
      issues.push(
        `${tableName} definition ${tableDifference.index + 1} differs: SQLite=${JSON.stringify(tableDifference.left ?? "<missing>")} Postgres=${JSON.stringify(tableDifference.right ?? "<missing>")}`,
      );
    }

    const sqliteIndexes = extractIndexes(sqliteSource, tableName);
    const postgresIndexes = extractIndexes(postgresSource, tableName);
    const requiredIndexes = REQUIRED_INDEX_SIGNATURES.get(tableName) ?? [];
    for (const [dialect, indexes] of [
      ["SQLite", sqliteIndexes],
      ["Postgres", postgresIndexes],
    ]) {
      const requiredDifference = firstDifference(requiredIndexes, indexes);
      if (requiredDifference) {
        issues.push(
          `${dialect} ${tableName} required index ${requiredDifference.index + 1} differs: expected=${JSON.stringify(requiredDifference.left ?? "<missing>")} found=${JSON.stringify(requiredDifference.right ?? "<missing>")}`,
        );
      }
    }
    const indexDifference = firstDifference(sqliteIndexes, postgresIndexes);
    if (indexDifference) {
      issues.push(
        `${tableName} index ${indexDifference.index + 1} differs: SQLite=${JSON.stringify(indexDifference.left ?? "<missing>")} Postgres=${JSON.stringify(indexDifference.right ?? "<missing>")}`,
      );
    }
  }

  return issues;
}

function main() {
  const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
  const sqlitePath = path.join(repoRoot, "server/src/db/mod.rs");
  const postgresPath = path.join(repoRoot, "infra/postgres/server-runtime/002_jobs.sql");
  const sqliteSource = fs.readFileSync(sqlitePath, "utf8");
  const postgresSource = fs.readFileSync(postgresPath, "utf8");
  const issues = compareJobsSchemas(sqliteSource, postgresSource);

  const includePath = 'include_str!("../../../infra/postgres/server-runtime/002_jobs.sql")';
  if (!sqliteSource.includes(includePath)) issues.push(`server migration runner does not include 002_jobs.sql via ${includePath}`);
  if (!sqliteSource.includes('&[&"002_jobs.sql"]')) issues.push("server migration runner does not record 002_jobs.sql");

  if (issues.length > 0) {
    console.error("Jobs SQLite/Postgres schema parity failed:");
    for (const issue of issues.sort()) console.error(`- ${issue}`);
    process.exitCode = 1;
    return;
  }
  const indexCount = JOBS_PARITY_TABLES.reduce(
    (count, tableName) => count + extractIndexes(sqliteSource, tableName).length,
    0,
  );
  console.log(`Jobs SQLite/Postgres schema parity passed (${JOBS_PARITY_TABLES.length} tables, ${indexCount} indexes).`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
