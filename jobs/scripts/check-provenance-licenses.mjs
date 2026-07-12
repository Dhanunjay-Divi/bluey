import fs from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const LICENSE_OVERRIDES = new Map([
  // unionfs 4.6.0 ships an Unlicense LICENSE file but omits package.json license metadata.
  ["unionfs@4.6.0", "Unlicense"],
]);

function compareText(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

function packageNameFromLockPath(lockPath) {
  const marker = "node_modules/";
  const start = lockPath.lastIndexOf(marker);
  if (start < 0) return null;
  const remainder = lockPath.slice(start + marker.length);
  const segments = remainder.split("/");
  return remainder.startsWith("@") ? segments.slice(0, 2).join("/") : segments[0];
}

function sameDependencyMap(left = {}, right = {}) {
  const leftEntries = Object.entries(left).sort(([a], [b]) => compareText(a, b));
  const rightEntries = Object.entries(right).sort(([a], [b]) => compareText(a, b));
  return JSON.stringify(leftEntries) === JSON.stringify(rightEntries);
}

export function inventoryPackageLock(lockfile) {
  const issues = [];
  const identities = new Map();
  let lockEntries = 0;
  let overrideCount = 0;

  if (lockfile.lockfileVersion !== 3) issues.push(`jobs/package-lock.json must use lockfileVersion 3, found ${lockfile.lockfileVersion}`);
  if (!lockfile.packages || typeof lockfile.packages !== "object") {
    return { issues: ["jobs/package-lock.json has no packages inventory"], lockEntries: 0, packages: 0, licenses: new Map(), overrideCount: 0 };
  }

  for (const [lockPath, metadata] of Object.entries(lockfile.packages)) {
    const name = packageNameFromLockPath(lockPath);
    if (!name || metadata.link) continue;
    lockEntries += 1;
    const identity = `${name}@${metadata.version ?? "<missing>"}`;
    const override = LICENSE_OVERRIDES.get(identity);
    const license = typeof metadata.license === "string" && metadata.license.trim() ? metadata.license.trim() : override;
    if (override && !metadata.license) overrideCount += 1;

    if (!metadata.version) issues.push(`${lockPath}: missing version`);
    if (typeof metadata.resolved !== "string" || !metadata.resolved.startsWith("https://registry.npmjs.org/")) {
      issues.push(`${identity}: dependency is not pinned to the npm registry`);
    }
    if (typeof metadata.integrity !== "string" || !metadata.integrity.startsWith("sha512-")) {
      issues.push(`${identity}: missing sha512 integrity`);
    }
    if (!license) issues.push(`${identity}: missing audited license metadata`);
    if (license && /^(?:unknown|unlicensed)$/i.test(license)) issues.push(`${identity}: non-inventory license value ${license}`);

    const prior = identities.get(identity);
    if (prior && prior !== license) issues.push(`${identity}: inconsistent license metadata (${prior} vs ${license})`);
    identities.set(identity, license ?? "<missing>");
  }

  const licenses = new Map();
  for (const license of identities.values()) licenses.set(license, (licenses.get(license) ?? 0) + 1);
  return {
    issues: [...new Set(issues)].sort(),
    lockEntries,
    packages: identities.size,
    licenses: new Map([...licenses.entries()].sort(([left], [right]) => compareText(left, right))),
    overrideCount,
  };
}

export function validateWorkspaceLock(rootManifest, workspaceManifests, lockfile) {
  const issues = [];
  const expectedWorkspaces = [...(rootManifest.workspaces ?? [])].sort();
  const providedWorkspaces = [...workspaceManifests.keys()].sort();
  if (JSON.stringify(expectedWorkspaces) !== JSON.stringify(providedWorkspaces)) {
    issues.push(`workspace manifests differ: expected ${expectedWorkspaces.join(", ")}; found ${providedWorkspaces.join(", ")}`);
  }

  for (const workspace of expectedWorkspaces) {
    const manifest = workspaceManifests.get(workspace);
    const locked = lockfile.packages?.[workspace];
    if (!manifest) continue;
    if (!locked) {
      issues.push(`${workspace}: missing workspace entry in jobs/package-lock.json`);
      continue;
    }
    if (manifest.name !== locked.name || manifest.version !== locked.version) {
      issues.push(`${workspace}: name/version does not match jobs/package-lock.json`);
    }
    for (const field of ["dependencies", "devDependencies", "optionalDependencies", "peerDependencies"]) {
      if (!sameDependencyMap(manifest[field], locked[field])) issues.push(`${workspace}: ${field} does not match jobs/package-lock.json`);
    }
  }
  return issues.sort();
}

export function parseProvenanceRows(markdown) {
  const rows = [];
  const expression =
    /^\|\s*\[([^\]]+)\]\((https:\/\/github\.com\/[^)]+)\)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|$/gm;
  for (const match of markdown.matchAll(expression)) {
    rows.push({
      label: match[1].trim(),
      url: match[2].replace(/\/$/, ""),
      commit: match[3].replaceAll("`", "").trim(),
      license: match[4].trim(),
      decision: match[5].trim(),
    });
  }
  return rows;
}

function adaptationPaths(markdown) {
  const section = markdown.split("## Shipped adaptation map")[1]?.split(/^## /m)[0] ?? "";
  return [...section.matchAll(/^\|\s*`([^`]+)`\s*\|/gm)].map((match) => match[1]);
}

export function validateProvenance(provenance, notices, pathExists = () => true) {
  const issues = [];
  const rows = parseProvenanceRows(provenance);
  if (rows.length === 0) issues.push("THIRD_PARTY_PROVENANCE.md has no parseable source rows");

  const urls = new Set();
  for (const row of rows) {
    if (!/^[0-9a-f]{12}$/.test(row.commit)) issues.push(`${row.label}: reviewed commit must be exactly 12 lowercase hex characters`);
    if (!row.license || /^(?:n\/a|tbd|unknown)$/i.test(row.license)) issues.push(`${row.label}: missing observed license decision`);
    if (!row.decision || /^(?:n\/a|tbd)$/i.test(row.decision)) issues.push(`${row.label}: missing Bluey reuse decision`);
    if (urls.has(row.url)) issues.push(`${row.label}: duplicate provenance URL ${row.url}`);
    urls.add(row.url);

    if (/\bMIT\b/i.test(row.license) && /\badapted\b/i.test(row.decision)) {
      const repositoryName = row.url.split("/").at(-1);
      if (!new RegExp(`(^|\\n)- ${repositoryName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")},`, "i").test(notices)) {
        issues.push(`${row.label}: adapted MIT source is missing from THIRD_PARTY_NOTICES.md`);
      }
    }
  }

  if (!notices.includes("## MIT License")) issues.push("THIRD_PARTY_NOTICES.md is missing the retained MIT license text");
  for (const relativePath of adaptationPaths(provenance)) {
    const checkPath = relativePath.endsWith("/*") ? relativePath.slice(0, -2) : relativePath;
    if (!pathExists(checkPath)) issues.push(`adaptation map target does not exist: ${relativePath}`);
  }
  return { rows, issues: [...new Set(issues)].sort() };
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function main() {
  const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
  const jobsRoot = path.join(repoRoot, "jobs");
  const rootManifest = readJson(path.join(jobsRoot, "package.json"));
  const lockfile = readJson(path.join(jobsRoot, "package-lock.json"));
  const workspaceManifests = new Map(
    (rootManifest.workspaces ?? []).map((workspace) => [workspace, readJson(path.join(jobsRoot, workspace, "package.json"))]),
  );
  const inventory = inventoryPackageLock(lockfile);
  const workspaceIssues = validateWorkspaceLock(rootManifest, workspaceManifests, lockfile);
  const provenance = fs.readFileSync(path.join(jobsRoot, "THIRD_PARTY_PROVENANCE.md"), "utf8");
  const notices = fs.readFileSync(path.join(jobsRoot, "THIRD_PARTY_NOTICES.md"), "utf8");
  const provenanceResult = validateProvenance(provenance, notices, (relativePath) => fs.existsSync(path.join(jobsRoot, relativePath)));
  const issues = [...inventory.issues, ...workspaceIssues, ...provenanceResult.issues].sort();

  if (issues.length > 0) {
    console.error("Jobs provenance/license inventory failed:");
    for (const issue of issues) console.error(`- ${issue}`);
    process.exitCode = 1;
    return;
  }

  const licenseSummary = [...inventory.licenses].map(([license, count]) => `${license}=${count}`).join(", ");
  console.log(
    `Jobs dependency inventory passed (${inventory.lockEntries} lock entries, ${inventory.packages} unique package versions, ${inventory.overrideCount} audited override).`,
  );
  console.log(`License inventory: ${licenseSummary}`);
  console.log(`Source provenance passed (${provenanceResult.rows.length} commit-pinned repositories).`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
