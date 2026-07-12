import { execFileSync, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const MAX_TEXT_BYTES = 5 * 1024 * 1024;

const TEXT_EXTENSIONS = new Set([
  ".bash",
  ".bat",
  ".c",
  ".cfg",
  ".cjs",
  ".cmd",
  ".conf",
  ".cpp",
  ".cs",
  ".css",
  ".env",
  ".ex",
  ".exs",
  ".go",
  ".gql",
  ".graphql",
  ".h",
  ".hpp",
  ".html",
  ".ini",
  ".java",
  ".js",
  ".json",
  ".jsonc",
  ".jsx",
  ".kt",
  ".lock",
  ".md",
  ".mjs",
  ".php",
  ".properties",
  ".ps1",
  ".py",
  ".rb",
  ".rs",
  ".sh",
  ".sql",
  ".swift",
  ".tf",
  ".tfvars",
  ".toml",
  ".ts",
  ".tsx",
  ".txt",
  ".xml",
  ".yaml",
  ".yml",
  ".zsh",
]);

const ARCHIVE_PATTERN = /\.(?:7z|bz2|dmg|gz|iso|jar|rar|tar|tbz2|tgz|txz|war|xz|zip|zst|tar\.(?:bz2|gz|xz|zst))(?:\.enc)?$/i;
const EVIDENCE_EXTENSION_PATTERN = /\.(?:bmp|csv|docx?|gif|html?|jpe?g|json|pdf|png|sqlite3?|tiff?|txt|webp|xlsx?|ya?ml)$/i;
const DATA_EXTENSION_PATTERN = /\.(?:csv|db|docx?|json|odt|pdf|rtf|sqlite3?|txt|xlsx?|ya?ml)$/i;

const KNOWN_SECRET_PATTERNS = [
  [
    "private key material",
    /-{5}BEGIN (?:(?:EC |ENCRYPTED |OPENSSH |RSA )?PRIVATE KEY|PGP PRIVATE KEY BLOCK)-{5}/g,
  ],
  ["AWS access key", /\b(?:AKIA|ASIA)[0-9A-Z]{16}\b/g],
  ["GitHub access token", /\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{50,})\b/g],
  ["GitLab access token", /\bglpat-[A-Za-z0-9_-]{20,}\b/g],
  ["Google API key", /\bAIza[0-9A-Za-z_-]{35}\b/g],
  ["npm access token", /\bnpm_[A-Za-z0-9]{30,}\b/g],
  ["Anthropic API key", /\bsk-ant-[A-Za-z0-9_-]{20,}\b/g],
  ["OpenAI API key", /\bsk-(?:(?:proj|svcacct)-[A-Za-z0-9_-]{20,}|[A-Za-z0-9]{32,})\b/g],
  ["PyPI access token", /\bpypi-AgEIcH[A-Za-z0-9_-]{40,}\b/g],
  ["SendGrid API key", /\bSG\.[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{20,}\b/g],
  ["Slack access token", /\bxox[baprs]-[A-Za-z0-9-]{20,}\b/g],
  ["Stripe live key", /\b[rs]k_live_[A-Za-z0-9]{16,}\b/g],
  ["JSON Web Token", /\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b/g],
];

const GENERIC_CREDENTIAL_PATTERN =
  /\b(api[_-]?key|auth[_-]?token|client[_-]?secret|access[_-]?token|refresh[_-]?token|password|passwd)\b\s*[:=]\s*(["'`])([^"'`\r\n]{8,})\2/gi;
const UNQUOTED_ENV_CREDENTIAL_PATTERN =
  /^\s*(?:export\s+)?((?:[A-Z][A-Z0-9_]*_)?(?:API_KEY|AUTH_TOKEN|ACCESS_TOKEN|REFRESH_TOKEN|CLIENT_SECRET|ENCRYPTION_KEY|PRIVATE_KEY|PASSWORD|PASSWD|SECRET|TOKEN))\s*=\s*([A-Za-z0-9_+/=-]{8,})/i;
const CREDENTIAL_URL_PATTERN = /\bhttps?:\/\/([^\s:/@]+):([^\s/@]{8,})@[^\s]+/gi;

function normalizedPath(filePath) {
  return filePath.replaceAll("\\", "/").replace(/^\.\//, "");
}

function compareText(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

function isDocumentationPath(filePath) {
  const normalized = normalizedPath(filePath).toLowerCase();
  return normalized.startsWith("docs/") || normalized.startsWith("assets/") || /(^|\/)readme(?:\.[^/]*)?$/.test(normalized);
}

function isTestOrExamplePath(filePath) {
  const normalized = normalizedPath(filePath).toLowerCase();
  const segments = normalized.split("/");
  return (
    isDocumentationPath(normalized) ||
    segments.some((segment) =>
      ["__fixtures__", "__tests__", "example", "examples", "fixture", "fixtures", "sample", "samples", "test", "tests"].includes(
        segment,
      ),
    ) ||
    /(?:^|[._-])(?:spec|test)\.[^/]+$/.test(path.posix.basename(normalized)) ||
    /\.(?:example|sample|template)(?:\.[^/]*)?$/.test(normalized)
  );
}

function hasSyntheticName(filePath) {
  return /(?:^|[._-])(?:dummy|example|fake|fixture|placeholder|sample|synthetic|test)(?:[._-]|$)/i.test(
    path.posix.basename(normalizedPath(filePath)),
  );
}

function isPlaceholderValue(value, filePath, line) {
  const candidate = value.trim();
  const lower = candidate.toLowerCase();
  const context = `${candidate} ${line}`.toLowerCase();
  if (/\$\{[^}]+\}|\$\{\{[^}]+\}\}|<[^>]+>|process\.env|std::env|env::var/.test(candidate)) return true;
  if (/^(?:x+|0+|\*+|redacted)$/i.test(candidate)) return true;
  if (/^[a-z]+(?:\s+[a-z]+){2,}[.!]?$/i.test(candidate) && (isTestOrExamplePath(filePath) || normalizedPath(filePath).startsWith("web/"))) {
    return true;
  }
  if (
    /(?:abcdefghijklmnopqrstuvwxyz|abcd1234|change[-_ ]?me|dummy|example|fake|not[-_ ]?(?:a[-_ ]?)?real|not needed|placeholder|redacted|replace[-_ ]?me|your[-_ ]|(?:^|[_-])(?:bad|ok|test|xxx)[0-9]*(?:[_-]|$))/.test(
      lower,
    )
  ) {
    return true;
  }
  if (isTestOrExamplePath(filePath) && /(?:dummy|example|fake|local|password|placeholder|sample|secret|synthetic|test)/.test(context)) {
    return true;
  }
  return false;
}

function isCanonicalExampleJwt(value) {
  const parts = value.split(".");
  if (parts.length !== 3) return false;
  try {
    const payload = JSON.parse(Buffer.from(parts[1], "base64url").toString("utf8"));
    const serialized = JSON.stringify(payload).toLowerCase();
    return (
      parts[2] === "SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c" &&
      /"(?:sub|name)":"(?:dummy|example|test)"/.test(serialized)
    );
  } catch {
    return false;
  }
}

function safeKnownSecret(value, filePath, line, label) {
  if (label === "JSON Web Token" && isCanonicalExampleJwt(value)) return true;
  return isPlaceholderValue(value, filePath, line);
}

function looksOpaqueCredential(value) {
  const classes = [/[a-z]/.test(value), /[A-Z]/.test(value), /[0-9]/.test(value), /[_+/=-]/.test(value)];
  return classes.filter(Boolean).length >= 2;
}

export function classifyTrackedPath(filePath) {
  const normalized = normalizedPath(filePath);
  const lower = normalized.toLowerCase();
  const segments = lower.split("/");
  const basename = segments.at(-1) ?? "";
  const findings = [];

  if (ARCHIVE_PATTERN.test(lower)) findings.push("archive artifact");

  const fixturePath = segments.some((segment) => ["__fixtures__", "fixture", "fixtures", "testdata"].includes(segment));
  const credentialStem =
    /(?:^|[._-])(?:api[-_]?key|auth|cookie|credential|identity|login|oauth|password|private[-_]?key|secret|session|token)(?:[._-]|$)/.test(
      basename,
    );
  const credentialFixtureExtension = /\.(?:db|env|ini|json|sqlite3?|txt|ya?ml)$/.test(basename);
  if (fixturePath && credentialStem && credentialFixtureExtension) findings.push("credential-shaped fixture file");

  const placeholderCredentialFile = /\.(?:example|sample|template)(?:\.[^/]*)?$/.test(basename);
  if (
    !placeholderCredentialFile &&
    (/^(?:\.env(?:\.[^/]+)?|\.netrc|\.npmrc|\.pypirc)$/.test(basename) ||
      /^(?:credentials?|service[-_]?account|client[-_]?secret|oauth[-_]?client)(?:\.[^/]*)?$/.test(basename) ||
      /^(?:id_rsa|id_dsa|id_ecdsa|id_ed25519)$/.test(basename) ||
      /\.(?:jks|key|keystore|p12|p8|pem|pfx)$/.test(basename))
  ) {
    findings.push("credential-bearing file path");
  }

  const sensitiveDataDirectory = segments.some((segment) =>
    [
      "applicant-data",
      "applicant_data",
      "applicant",
      "applicants",
      "application-data",
      "application_data",
      "candidate",
      "candidate-data",
      "candidate_data",
      "candidate-records",
      "candidates",
      "cover-letters",
      "customer-data",
      "customer_data",
      "job-applications",
      "personal-data",
      "personal_data",
      "resumes",
      "user-data",
      "userdata",
      "user_data",
    ].includes(segment),
  );
  const sensitiveJobsRoot =
    /^jobs\/(?:applicants?|applications?|candidates?|exports?|profiles?|resumes?|uploads?|users?)\//.test(lower);
  if (sensitiveDataDirectory || sensitiveJobsRoot) findings.push("candidate or user data directory");

  const candidateDataFile =
    (/^(?:applicants?|candidates?|customers?|users?)(?:[-_](?:data|profile|record))?\./.test(basename) ||
      /^(?:application|cover[-_ ]?letter|resume)(?:[._-]|$)/.test(basename)) &&
    DATA_EXTENSION_PATTERN.test(basename);
  if (candidateDataFile && !(isTestOrExamplePath(lower) && hasSyntheticName(lower))) {
    findings.push("candidate or user data file");
  }

  const jobsRuntimeProfile =
    /^jobs\/(?:browser|runner)\/(?:active|profiles|snapshots)\//.test(lower) ||
    segments.some((segment) =>
      ["browser-profile", "browser-profiles", "chrome-user-data", "chromium-profile", "playwright-profile", "user-data-dir"].includes(
        segment,
      ),
    );
  const chromiumProfileSignature =
    basename === "local state" ||
    (segments.includes("default") && /^(?:cookies|history|login data|preferences|web data)(?:-journal)?$/.test(basename)) ||
    segments.some((segment) => ["gpucache", "indexeddb", "session storage"].includes(segment));
  if (jobsRuntimeProfile || chromiumProfileSignature) findings.push("generated browser profile");

  const evidenceDirectory = segments.some((segment) => ["receipt", "receipts", "screenshot", "screenshots"].includes(segment));
  const evidenceFile = /(?:^|[._-])(?:receipt|screenshot)(?:[._-]|$)/.test(basename) && EVIDENCE_EXTENSION_PATTERN.test(basename);
  const permittedSyntheticEvidence = isTestOrExamplePath(lower) && hasSyntheticName(lower);
  if ((evidenceDirectory || evidenceFile) && !isDocumentationPath(lower) && !permittedSyntheticEvidence) {
    findings.push("receipt or screenshot artifact");
  }

  return [...new Set(findings)].sort();
}

export function scanTextForSecrets(filePath, contents) {
  const findings = [];
  const lines = contents.split(/\r?\n/);

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    for (const [label, pattern] of KNOWN_SECRET_PATTERNS) {
      pattern.lastIndex = 0;
      for (const match of line.matchAll(pattern)) {
        if (!safeKnownSecret(match[0], filePath, line, label)) {
          findings.push({ line: index + 1, kind: label });
        }
      }
    }

    GENERIC_CREDENTIAL_PATTERN.lastIndex = 0;
    for (const match of line.matchAll(GENERIC_CREDENTIAL_PATTERN)) {
      if (!isPlaceholderValue(match[3], filePath, line)) {
        findings.push({ line: index + 1, kind: `raw ${match[1].toLowerCase()} assignment` });
      }
    }

    if (/\.(?:cfg|conf|env|ini|properties|sh|toml|ya?ml)$/i.test(filePath)) {
      const match = UNQUOTED_ENV_CREDENTIAL_PATTERN.exec(line);
      if (match && looksOpaqueCredential(match[2]) && !isPlaceholderValue(match[2], filePath, line)) {
        findings.push({ line: index + 1, kind: `raw ${match[1].toLowerCase()} assignment` });
      }
    }

    CREDENTIAL_URL_PATTERN.lastIndex = 0;
    for (const match of line.matchAll(CREDENTIAL_URL_PATTERN)) {
      if (!isPlaceholderValue(match[2], filePath, line)) {
        findings.push({ line: index + 1, kind: "credential embedded in URL" });
      }
    }
  }

  return [...new Map(findings.map((finding) => [`${finding.line}:${finding.kind}`, finding])).values()].sort(
    (left, right) => left.line - right.line || compareText(left.kind, right.kind),
  );
}

function isTextPath(filePath) {
  const basename = path.posix.basename(filePath).toLowerCase();
  const extension = path.posix.extname(basename);
  return (
    extension === "" ||
    TEXT_EXTENSIONS.has(extension) ||
    ["dockerfile", "gemfile", "makefile", "procfile"].includes(basename)
  );
}

function trackedEntries(repoRoot) {
  const output = execFileSync("git", ["ls-files", "--cached", "--stage", "-z"], {
    cwd: repoRoot,
    encoding: null,
  });
  return output
    .toString("utf8")
    .split("\0")
    .filter(Boolean)
    .map((entry) => {
      const match = /^(\d+) ([0-9a-f]+) (\d+)\t([\s\S]+)$/.exec(entry);
      if (!match) throw new Error(`Unable to parse tracked entry: ${JSON.stringify(entry)}`);
      return { mode: match[1], hash: match[2], stage: Number(match[3]), filePath: match[4] };
    })
    .filter((entry) => entry.stage === 0);
}

function readTrackedText(entries, repoRoot) {
  const textEntries = entries.filter((entry) => entry.mode !== "160000" && isTextPath(entry.filePath));
  const hashes = [...new Set(textEntries.map((entry) => entry.hash))];
  if (hashes.length === 0) return { blobs: new Map(), oversizedPaths: [] };

  const checked = spawnSync("git", ["cat-file", "--batch-check=%(objectname) %(objecttype) %(objectsize)"], {
    cwd: repoRoot,
    input: `${hashes.join("\n")}\n`,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  if (checked.status !== 0) throw new Error(checked.stderr || "git cat-file --batch-check failed");

  const sizes = new Map();
  for (const line of checked.stdout.trim().split("\n")) {
    const [hash, type, rawSize] = line.split(" ");
    if (type === "blob") sizes.set(hash, Number(rawSize));
  }
  const oversizedPaths = textEntries
    .filter((entry) => (sizes.get(entry.hash) ?? Number.POSITIVE_INFINITY) > MAX_TEXT_BYTES)
    .map((entry) => entry.filePath)
    .sort();
  const selected = hashes.filter((hash) => (sizes.get(hash) ?? Number.POSITIVE_INFINITY) <= MAX_TEXT_BYTES);
  if (selected.length === 0) return { blobs: new Map(), oversizedPaths };

  const loaded = spawnSync("git", ["cat-file", "--batch"], {
    cwd: repoRoot,
    input: `${selected.join("\n")}\n`,
    encoding: null,
    maxBuffer: 256 * 1024 * 1024,
  });
  if (loaded.status !== 0) throw new Error(loaded.stderr.toString("utf8") || "git cat-file --batch failed");

  const blobs = new Map();
  let offset = 0;
  for (const expectedHash of selected) {
    const newline = loaded.stdout.indexOf(10, offset);
    if (newline < 0) throw new Error(`Missing git cat-file header for ${expectedHash}`);
    const header = loaded.stdout.subarray(offset, newline).toString("utf8");
    const [actualHash, type, rawSize] = header.split(" ");
    const size = Number(rawSize);
    if (actualHash !== expectedHash || type !== "blob" || !Number.isSafeInteger(size)) {
      throw new Error(`Unexpected git cat-file header: ${header}`);
    }
    const start = newline + 1;
    const end = start + size;
    const blob = loaded.stdout.subarray(start, end);
    if (!blob.includes(0)) blobs.set(actualHash, blob.toString("utf8"));
    offset = end + 1;
  }
  return { blobs, oversizedPaths };
}

export function runPrivacyGate(repoRoot) {
  const entries = trackedEntries(repoRoot);
  const { blobs, oversizedPaths } = readTrackedText(entries, repoRoot);
  const issues = [];

  for (const filePath of oversizedPaths) issues.push({ filePath, kind: "tracked text exceeds the privacy scan limit" });

  for (const entry of entries) {
    for (const kind of classifyTrackedPath(entry.filePath)) {
      issues.push({ filePath: entry.filePath, kind });
    }
    const contents = blobs.get(entry.hash);
    if (contents !== undefined) {
      for (const finding of scanTextForSecrets(entry.filePath, contents)) {
        issues.push({ filePath: entry.filePath, ...finding });
      }
    }
  }

  issues.sort(
    (left, right) =>
      compareText(left.filePath, right.filePath) ||
      (left.line ?? 0) - (right.line ?? 0) ||
      compareText(left.kind, right.kind),
  );
  return { trackedPaths: entries.length, scannedTextFiles: entries.filter((entry) => blobs.has(entry.hash)).length, issues };
}

function main() {
  const repoRoot = execFileSync("git", ["rev-parse", "--show-toplevel"], { encoding: "utf8" }).trim();
  const result = runPrivacyGate(repoRoot);
  if (result.issues.length > 0) {
    console.error("Jobs privacy gate rejected tracked content:");
    for (const issue of result.issues) {
      const location = issue.line ? `${JSON.stringify(issue.filePath)}:${issue.line}` : JSON.stringify(issue.filePath);
      console.error(`- ${location}: ${issue.kind}`);
    }
    console.error("Use synthetic fixtures with explicit dummy/example naming; never allowlist real candidate data or credentials.");
    process.exitCode = 1;
    return;
  }
  console.log(`Jobs privacy gate passed (${result.trackedPaths} tracked paths, ${result.scannedTextFiles} text files scanned).`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
