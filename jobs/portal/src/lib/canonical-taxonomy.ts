import canonicalTaxonomySource from "../../../taxonomy/canonical-v1.json?raw";

export interface CanonicalRoleFamily {
  readonly id: string;
  readonly label: string;
}

export interface CanonicalRole {
  readonly id: string;
  readonly label: string;
  readonly family_id: string;
  readonly aliases: readonly string[];
}

export interface AmbiguousRoleAlias {
  readonly alias: string;
  readonly candidate_role_ids: readonly string[];
  readonly reason: string;
}

export interface CanonicalSkill {
  readonly id: string;
  readonly label: string;
  readonly aliases: readonly string[];
}

export interface CanonicalCountry {
  readonly code: string;
  readonly label: string;
  readonly aliases: readonly string[];
}

export interface CanonicalSubdivision {
  readonly country_code: string;
  readonly code: string;
  readonly label: string;
  readonly aliases: readonly string[];
}

export interface CanonicalMetroCity {
  readonly id: string;
  readonly label: string;
  readonly subdivision_id: string | null;
  readonly aliases: readonly string[];
}

export interface CanonicalMetro {
  readonly id: string;
  readonly label: string;
  readonly country_code: string;
  readonly subdivision_ids: readonly string[];
  readonly aliases: readonly string[];
  readonly cities: readonly CanonicalMetroCity[];
}

export interface CanonicalTaxonomy {
  readonly schema_version: number;
  readonly taxonomy_version: string;
  readonly role_families: readonly CanonicalRoleFamily[];
  readonly roles: readonly CanonicalRole[];
  readonly ambiguous_role_aliases: readonly AmbiguousRoleAlias[];
  readonly skills: readonly CanonicalSkill[];
  readonly countries: readonly CanonicalCountry[];
  readonly subdivisions: readonly CanonicalSubdivision[];
  readonly metros: readonly CanonicalMetro[];
}

export interface CanonicalTaxonomyDescriptor {
  readonly schema_version: number;
  readonly taxonomy_version: string;
  readonly digest_algorithm: "sha256";
  readonly digest_sha256: string | null;
}

export type CanonicalTaxonomyDescriptorCheck =
  | {
      readonly status: "checking";
      readonly refresh_required: false;
      readonly reason: "local_digest_pending";
    }
  | {
      readonly status: "current";
      readonly refresh_required: false;
      readonly reason: null;
    }
  | {
      readonly status: "refresh_required";
      readonly refresh_required: true;
      readonly reason:
        | "missing_server_descriptor"
        | "schema_version_mismatch"
        | "taxonomy_version_mismatch"
        | "unsupported_digest"
        | "invalid_server_digest"
        | "digest_mismatch"
        | "invalid_server_response"
        | "invalid_server_registry"
        | "server_registry_mismatch";
    };

export type TargetRoleResolution =
  | {
      readonly status: "resolved";
      readonly review_required: false;
      readonly input: string;
      readonly matched_value: string;
      readonly matched_by: "id" | "label" | "alias";
      readonly role: CanonicalRole;
    }
  | {
      readonly status: "ambiguous";
      readonly review_required: true;
      readonly input: string;
      readonly matched_value: string;
      readonly alias: string;
      readonly candidates: readonly CanonicalRole[];
    }
  | {
      readonly status: "custom";
      readonly review_required: true;
      readonly input: string;
      readonly custom_label: string;
      readonly reason: "empty_role" | "unknown_role";
    };

export type PostingRoleClassification =
  | {
      readonly status: "known";
      readonly raw: string;
      readonly normalized: string;
      readonly family_id: string;
      readonly matched_role_ids: readonly string[];
    }
  | {
      readonly status: "ambiguous";
      readonly raw: string;
      readonly normalized: string;
      readonly candidate_family_ids: readonly string[];
      readonly matched_role_ids: readonly string[];
    }
  | {
      readonly status: "unknown";
      readonly raw: string;
      readonly normalized: string;
      readonly reason: "empty" | "no_known_family";
    };

const EXPECTED_SCHEMA_VERSION = 1;
const SHA256_HEX = /^[a-f0-9]{64}$/;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function jsonValuesEqual(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true;
  if (Array.isArray(left) || Array.isArray(right)) {
    return Array.isArray(left)
      && Array.isArray(right)
      && left.length === right.length
      && left.every((entry, index) => jsonValuesEqual(entry, right[index]));
  }
  if (!isRecord(left) || !isRecord(right)) return false;
  const leftKeys = Object.keys(left).sort();
  const rightKeys = Object.keys(right).sort();
  return leftKeys.length === rightKeys.length
    && leftKeys.every((key, index) => key === rightKeys[index] && jsonValuesEqual(left[key], right[key]));
}

function record(value: unknown, context: string): Record<string, unknown> {
  if (!isRecord(value)) throw new Error(`${context} must be an object`);
  return value;
}

function exactKeys(value: Record<string, unknown>, expected: readonly string[], context: string): void {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (actual.length !== wanted.length || actual.some((key, index) => key !== wanted[index])) {
    throw new Error(`${context} has an unsupported field set`);
  }
}

function nonEmptyString(value: unknown, context: string): string {
  if (typeof value !== "string" || !value.trim()) throw new Error(`${context} must be a non-empty string`);
  return value.trim();
}

function stringList(value: unknown, context: string): string[] {
  if (!Array.isArray(value)) throw new Error(`${context} must be an array`);
  const strings = value.map((entry, index) => nonEmptyString(entry, `${context}[${index}]`));
  const normalized = strings.map((entry) => entry.toLowerCase());
  if (new Set(normalized).size !== normalized.length) throw new Error(`${context} contains duplicate values`);
  return strings;
}

function objectList(value: unknown, context: string): Record<string, unknown>[] {
  if (!Array.isArray(value)) throw new Error(`${context} must be an array`);
  return value.map((entry, index) => record(entry, `${context}[${index}]`));
}

function ensureUnique(values: readonly string[], context: string): void {
  const normalized = values.map(normalizeLookupValue);
  if (new Set(normalized).size !== normalized.length) throw new Error(`${context} contains duplicate values`);
}

function validateRoleFamily(value: Record<string, unknown>, index: number): CanonicalRoleFamily {
  exactKeys(value, ["id", "label"], `role_families[${index}]`);
  return {
    id: nonEmptyString(value.id, `role_families[${index}].id`),
    label: nonEmptyString(value.label, `role_families[${index}].label`),
  };
}

function validateRole(value: Record<string, unknown>, index: number): CanonicalRole {
  exactKeys(value, ["id", "label", "family_id", "aliases"], `roles[${index}]`);
  return {
    id: nonEmptyString(value.id, `roles[${index}].id`),
    label: nonEmptyString(value.label, `roles[${index}].label`),
    family_id: nonEmptyString(value.family_id, `roles[${index}].family_id`),
    aliases: stringList(value.aliases, `roles[${index}].aliases`),
  };
}

function validateAmbiguousRoleAlias(value: Record<string, unknown>, index: number): AmbiguousRoleAlias {
  exactKeys(value, ["alias", "candidate_role_ids", "reason"], `ambiguous_role_aliases[${index}]`);
  const candidateRoleIds = stringList(
    value.candidate_role_ids,
    `ambiguous_role_aliases[${index}].candidate_role_ids`,
  );
  if (candidateRoleIds.length < 2) {
    throw new Error(`ambiguous_role_aliases[${index}] must identify at least two roles`);
  }
  return {
    alias: nonEmptyString(value.alias, `ambiguous_role_aliases[${index}].alias`),
    candidate_role_ids: candidateRoleIds,
    reason: nonEmptyString(value.reason, `ambiguous_role_aliases[${index}].reason`),
  };
}

function validateSkill(value: Record<string, unknown>, index: number): CanonicalSkill {
  exactKeys(value, ["id", "label", "aliases"], `skills[${index}]`);
  return {
    id: nonEmptyString(value.id, `skills[${index}].id`),
    label: nonEmptyString(value.label, `skills[${index}].label`),
    aliases: stringList(value.aliases, `skills[${index}].aliases`),
  };
}

function validateCountry(value: Record<string, unknown>, index: number): CanonicalCountry {
  exactKeys(value, ["code", "label", "aliases"], `countries[${index}]`);
  const code = nonEmptyString(value.code, `countries[${index}].code`);
  if (!/^[A-Z]{2}$/.test(code)) throw new Error(`countries[${index}].code must be ISO alpha-2`);
  return {
    code,
    label: nonEmptyString(value.label, `countries[${index}].label`),
    aliases: stringList(value.aliases, `countries[${index}].aliases`),
  };
}

function validateSubdivision(value: Record<string, unknown>, index: number): CanonicalSubdivision {
  exactKeys(value, ["country_code", "code", "label", "aliases"], `subdivisions[${index}]`);
  return {
    country_code: nonEmptyString(value.country_code, `subdivisions[${index}].country_code`),
    code: nonEmptyString(value.code, `subdivisions[${index}].code`),
    label: nonEmptyString(value.label, `subdivisions[${index}].label`),
    aliases: stringList(value.aliases, `subdivisions[${index}].aliases`),
  };
}

function validateMetroCity(
  value: Record<string, unknown>,
  metroIndex: number,
  cityIndex: number,
): CanonicalMetroCity {
  const context = `metros[${metroIndex}].cities[${cityIndex}]`;
  exactKeys(value, ["id", "label", "subdivision_id", "aliases"], context);
  const subdivisionId = value.subdivision_id === null
    ? null
    : nonEmptyString(value.subdivision_id, `${context}.subdivision_id`);
  return {
    id: nonEmptyString(value.id, `${context}.id`),
    label: nonEmptyString(value.label, `${context}.label`),
    subdivision_id: subdivisionId,
    aliases: stringList(value.aliases, `${context}.aliases`),
  };
}

function validateMetro(value: Record<string, unknown>, index: number): CanonicalMetro {
  const context = `metros[${index}]`;
  exactKeys(value, ["id", "label", "country_code", "subdivision_ids", "aliases", "cities"], context);
  const cities = objectList(value.cities, `${context}.cities`).map((city, cityIndex) => (
    validateMetroCity(city, index, cityIndex)
  ));
  if (cities.length === 0) throw new Error(`${context} must contain at least one bounded city`);
  return {
    id: nonEmptyString(value.id, `${context}.id`),
    label: nonEmptyString(value.label, `${context}.label`),
    country_code: nonEmptyString(value.country_code, `${context}.country_code`),
    subdivision_ids: stringList(value.subdivision_ids, `${context}.subdivision_ids`),
    aliases: stringList(value.aliases, `${context}.aliases`),
    cities,
  };
}

export function validateCanonicalTaxonomy(value: unknown): CanonicalTaxonomy {
  const taxonomy = record(value, "canonical taxonomy");
  exactKeys(taxonomy, [
    "schema_version",
    "taxonomy_version",
    "role_families",
    "roles",
    "ambiguous_role_aliases",
    "skills",
    "countries",
    "subdivisions",
    "metros",
  ], "canonical taxonomy");
  if (taxonomy.schema_version !== EXPECTED_SCHEMA_VERSION) {
    throw new Error(`canonical taxonomy schema must be ${EXPECTED_SCHEMA_VERSION}`);
  }

  const roleFamilies = objectList(taxonomy.role_families, "role_families").map(validateRoleFamily);
  const roles = objectList(taxonomy.roles, "roles").map(validateRole);
  const ambiguousRoleAliases = objectList(
    taxonomy.ambiguous_role_aliases,
    "ambiguous_role_aliases",
  ).map(validateAmbiguousRoleAlias);
  const skills = objectList(taxonomy.skills, "skills").map(validateSkill);
  const countries = objectList(taxonomy.countries, "countries").map(validateCountry);
  const subdivisions = objectList(taxonomy.subdivisions, "subdivisions").map(validateSubdivision);
  const metros = objectList(taxonomy.metros, "metros").map(validateMetro);

  ensureUnique(roleFamilies.map((family) => family.id), "role family IDs");
  ensureUnique(roleFamilies.map((family) => family.label), "role family labels");
  ensureUnique(roles.map((role) => role.id), "role IDs");
  ensureUnique(roles.map((role) => role.label), "role labels");
  ensureUnique(ambiguousRoleAliases.map((entry) => entry.alias), "ambiguous role aliases");
  ensureUnique(skills.map((skill) => skill.id), "skill IDs");
  ensureUnique(skills.map((skill) => skill.label), "skill labels");
  ensureUnique(countries.map((country) => country.code), "country codes");
  ensureUnique(countries.map((country) => country.label), "country labels");
  ensureUnique(
    subdivisions.map((subdivision) => `${subdivision.country_code}:${subdivision.code}`),
    "subdivision codes",
  );
  ensureUnique(
    subdivisions.map((subdivision) => `${subdivision.country_code}:${subdivision.label}`),
    "subdivision labels",
  );

  const familyIds = new Set(roleFamilies.map((family) => family.id));
  const roleIds = new Set(roles.map((role) => role.id));
  const countryCodes = new Set(countries.map((country) => country.code));
  const subdivisionIds = new Set(
    subdivisions.map((subdivision) => `${subdivision.country_code}-${subdivision.code}`),
  );
  for (const role of roles) {
    if (!familyIds.has(role.family_id)) throw new Error(`role ${role.id} references an unknown family`);
  }
  for (const entry of ambiguousRoleAliases) {
    for (const roleId of entry.candidate_role_ids) {
      if (!roleIds.has(roleId)) throw new Error(`ambiguous alias ${entry.alias} references an unknown role`);
    }
  }

  const skillAliases = new Map<string, string>();
  for (const skill of skills) {
    for (const alias of [skill.id, skill.label, ...skill.aliases]) {
      if (!/^[\x00-\x7F]+$/.test(alias)) throw new Error(`skill alias ${alias} must be ASCII`);
      const key = alias.trim().toLowerCase();
      const existing = skillAliases.get(key);
      if (existing && existing !== skill.id) throw new Error(`skill alias ${alias} resolves to multiple skills`);
      skillAliases.set(key, skill.id);
    }
  }

  const countryAliases = new Map<string, string>();
  for (const country of countries) {
    for (const alias of [country.code, country.label, ...country.aliases]) {
      const key = normalizeWords(alias);
      if (!key) throw new Error(`country alias ${alias} has no canonical tokens`);
      const existing = countryAliases.get(key);
      if (existing && existing !== country.code) {
        throw new Error(`country alias ${alias} resolves to multiple countries`);
      }
      countryAliases.set(key, country.code);
    }
  }
  for (const subdivision of subdivisions) {
    if (!countryCodes.has(subdivision.country_code)) {
      throw new Error(`subdivision ${subdivision.code} references an unknown country`);
    }
    for (const alias of [subdivision.code, subdivision.label, ...subdivision.aliases]) {
      if (!normalizeWords(alias)) throw new Error(`subdivision alias ${alias} has no canonical tokens`);
    }
  }

  const metroIds = new Set<string>();
  const metroAliases = new Map<string, string>();
  const cityIds = new Set<string>();
  const cityAliases = new Map<string, string>();
  for (const metro of metros) {
    if (metroIds.has(metro.id)) throw new Error(`duplicate metro id ${metro.id}`);
    metroIds.add(metro.id);
    if (!countryCodes.has(metro.country_code)) {
      throw new Error(`metro ${metro.id} references an unknown country`);
    }
    for (const subdivisionId of metro.subdivision_ids) {
      if (!subdivisionIds.has(subdivisionId) || !subdivisionId.startsWith(`${metro.country_code}-`)) {
        throw new Error(`metro ${metro.id} references an invalid subdivision`);
      }
    }
    for (const alias of [metro.id, metro.label, ...metro.aliases]) {
      const key = normalizeWords(alias);
      if (!key) throw new Error(`metro alias ${alias} has no canonical tokens`);
      const existing = metroAliases.get(key);
      if (existing && existing !== metro.id) throw new Error(`metro alias ${alias} resolves to multiple metros`);
      metroAliases.set(key, metro.id);
    }
    for (const city of metro.cities) {
      if (cityIds.has(city.id)) throw new Error(`duplicate city id ${city.id}`);
      cityIds.add(city.id);
      if (city.subdivision_id !== null && !metro.subdivision_ids.includes(city.subdivision_id)) {
        throw new Error(`city ${city.id} references a subdivision outside its metro`);
      }
      for (const alias of [city.id, city.label, ...city.aliases]) {
        const normalized = normalizeWords(alias);
        if (!normalized) throw new Error(`city alias ${alias} has no canonical tokens`);
        const key = `${metro.country_code}:${city.subdivision_id ?? "*"}:${normalized}`;
        const existing = cityAliases.get(key);
        if (existing && existing !== city.id) throw new Error(`city alias ${alias} resolves to multiple cities`);
        cityAliases.set(key, city.id);
      }
    }
  }

  const deterministicAliases = new Map<string, string>();
  for (const role of roles) {
    for (const alias of [role.id, role.label, ...role.aliases]) {
      const key = normalizeTargetRolePhrase(alias);
      if (!key) throw new Error(`role alias ${alias} has no canonical tokens`);
      const existing = deterministicAliases.get(key);
      if (existing && existing !== role.id) throw new Error(`role alias ${alias} resolves to multiple roles`);
      deterministicAliases.set(key, role.id);
    }
  }
  for (const entry of ambiguousRoleAliases) {
    const key = normalizeTargetRolePhrase(entry.alias);
    if (!key) throw new Error(`ambiguous role alias ${entry.alias} has no canonical tokens`);
    if (deterministicAliases.has(key)) {
      throw new Error(`ambiguous role alias ${entry.alias} also has a deterministic resolution`);
    }
  }

  return {
    schema_version: EXPECTED_SCHEMA_VERSION,
    taxonomy_version: nonEmptyString(taxonomy.taxonomy_version, "taxonomy_version"),
    role_families: roleFamilies,
    roles,
    ambiguous_role_aliases: ambiguousRoleAliases,
    skills,
    countries,
    subdivisions,
    metros,
  };
}

function deepFreeze<T>(value: T): T {
  if (typeof value !== "object" || value === null || Object.isFrozen(value)) return value;
  for (const child of Object.values(value)) deepFreeze(child);
  return Object.freeze(value);
}

export const CANONICAL_TAXONOMY = deepFreeze(validateCanonicalTaxonomy(JSON.parse(canonicalTaxonomySource)));

export const CANONICAL_TAXONOMY_DESCRIPTOR: CanonicalTaxonomyDescriptor = Object.freeze({
  schema_version: CANONICAL_TAXONOMY.schema_version,
  taxonomy_version: CANONICAL_TAXONOMY.taxonomy_version,
  digest_algorithm: "sha256",
  digest_sha256: null,
});

let descriptorPromise: Promise<CanonicalTaxonomyDescriptor> | null = null;

export function canonicalTaxonomyDescriptor(): Promise<CanonicalTaxonomyDescriptor> {
  descriptorPromise ??= sha256Hex(canonicalTaxonomySource).then((digest) => Object.freeze({
    ...CANONICAL_TAXONOMY_DESCRIPTOR,
    digest_sha256: digest,
  }));
  return descriptorPromise;
}

async function sha256Hex(value: string): Promise<string> {
  if (!globalThis.crypto?.subtle) throw new Error("canonical taxonomy SHA-256 is unavailable");
  const digest = await globalThis.crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function checkCanonicalTaxonomyDescriptor(
  serverDescriptor: CanonicalTaxonomyDescriptor | null | undefined,
  localDescriptor: CanonicalTaxonomyDescriptor = CANONICAL_TAXONOMY_DESCRIPTOR,
): CanonicalTaxonomyDescriptorCheck {
  if (!serverDescriptor) return refreshRequired("missing_server_descriptor");
  if (serverDescriptor.schema_version !== localDescriptor.schema_version) {
    return refreshRequired("schema_version_mismatch");
  }
  if (serverDescriptor.taxonomy_version !== localDescriptor.taxonomy_version) {
    return refreshRequired("taxonomy_version_mismatch");
  }
  if (serverDescriptor.digest_algorithm !== "sha256") return refreshRequired("unsupported_digest");
  if (typeof serverDescriptor.digest_sha256 !== "string" || !SHA256_HEX.test(serverDescriptor.digest_sha256)) {
    return refreshRequired("invalid_server_digest");
  }
  if (localDescriptor.digest_sha256 === null) {
    return { status: "checking", refresh_required: false, reason: "local_digest_pending" };
  }
  if (serverDescriptor.digest_sha256 !== localDescriptor.digest_sha256) return refreshRequired("digest_mismatch");
  return { status: "current", refresh_required: false, reason: null };
}

export function checkJobsTaxonomyResponse(
  response: unknown,
  localDescriptor: CanonicalTaxonomyDescriptor = CANONICAL_TAXONOMY_DESCRIPTOR,
): CanonicalTaxonomyDescriptorCheck {
  if (!isRecord(response)) return refreshRequired("invalid_server_response");
  if (typeof response.taxonomyVersion !== "string" || typeof response.taxonomySha256 !== "string") {
    return refreshRequired("invalid_server_response");
  }

  let registry: CanonicalTaxonomy;
  try {
    registry = validateCanonicalTaxonomy(response.registry);
  } catch {
    return refreshRequired("invalid_server_registry");
  }
  if (
    registry.taxonomy_version !== response.taxonomyVersion
    || !jsonValuesEqual(response.registry, CANONICAL_TAXONOMY)
  ) {
    return refreshRequired("server_registry_mismatch");
  }

  return checkCanonicalTaxonomyDescriptor({
    schema_version: registry.schema_version,
    taxonomy_version: response.taxonomyVersion,
    digest_algorithm: "sha256",
    digest_sha256: response.taxonomySha256,
  }, localDescriptor);
}

function refreshRequired(
  reason: Extract<CanonicalTaxonomyDescriptorCheck, { status: "refresh_required" }>["reason"],
): CanonicalTaxonomyDescriptorCheck {
  return { status: "refresh_required", refresh_required: true, reason };
}

function normalizeLookupValue(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/&/g, " and ")
    .replace(/[^a-z0-9+#.]+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function cleanDisplayValue(value: string): string {
  return value.trim().replace(/\s+/g, " ");
}

function normalizeWords(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function normalizeTargetRolePhrase(value: string): string {
  const words = normalizeWords(value).split(/\s+/).filter(Boolean);
  while (words.length > 0) {
    if ((words[0] === "entry" || words[0] === "mid") && words[1] === "level") {
      words.splice(0, 2);
    } else if (["junior", "jr", "senior", "sr", "staff", "lead", "principal"].includes(words[0] ?? "")) {
      words.shift();
    } else {
      break;
    }
  }
  if (["i", "ii", "iii", "iv", "1", "2", "3", "4"].includes(words.at(-1) ?? "")) words.pop();
  return words.join(" ");
}

const ROLES_BY_ID = new Map(CANONICAL_TAXONOMY.roles.map((role) => [role.id, role]));
const ROLE_LOOKUP = new Map<string, CanonicalRole>();
for (const role of CANONICAL_TAXONOMY.roles) {
  ROLE_LOOKUP.set(normalizeTargetRolePhrase(role.id), role);
  ROLE_LOOKUP.set(normalizeTargetRolePhrase(role.label), role);
  for (const alias of role.aliases) {
    ROLE_LOOKUP.set(normalizeTargetRolePhrase(alias), role);
  }
}
const AMBIGUOUS_ROLE_LOOKUP = new Map(
  CANONICAL_TAXONOMY.ambiguous_role_aliases.map((entry) => [normalizeTargetRolePhrase(entry.alias), entry]),
);

export function resolveTargetRole(value: string): TargetRoleResolution {
  const input = cleanDisplayValue(value);
  const matchedValue = normalizeTargetRolePhrase(input);
  const ambiguous = AMBIGUOUS_ROLE_LOOKUP.get(matchedValue);
  if (ambiguous) {
    return {
      status: "ambiguous",
      review_required: true,
      input,
      matched_value: matchedValue,
      alias: ambiguous.alias,
      candidates: ambiguous.candidate_role_ids.map((roleId) => {
        const role = ROLES_BY_ID.get(roleId);
        if (!role) throw new Error(`canonical taxonomy role ${roleId} is unavailable`);
        return role;
      }),
    };
  }
  const resolved = ROLE_LOOKUP.get(matchedValue);
  if (resolved) {
    const matchedBy = input.toLowerCase() === resolved.id.toLowerCase()
      ? "id"
      : normalizeTargetRolePhrase(resolved.label) === matchedValue
        ? "label"
        : "alias";
    return {
      status: "resolved",
      review_required: false,
      input,
      matched_value: matchedValue,
      matched_by: matchedBy,
      role: resolved,
    };
  }
  return {
    status: "custom",
    review_required: true,
    input,
    custom_label: input,
    reason: matchedValue ? "unknown_role" : "empty_role",
  };
}

export function canonicalRoleFamilyId(value: string): string | null {
  const resolution = resolveTargetRole(value);
  return resolution.status === "resolved" ? resolution.role.family_id : null;
}

interface PostingRoleSpanMatch {
  readonly start: number;
  readonly end: number;
  readonly role_ids: readonly string[];
}

/**
 * Classify a posting or employment title without selecting a target role.
 * This mirrors the server's longest-span presentation semantics; only one
 * proven family is returned as known, while ambiguous evidence stays closed.
 */
export function classifyPostingRole(value: string): PostingRoleClassification {
  const raw = value;
  const normalized = normalizeWords(raw);
  if (!normalized) return { status: "unknown", raw, normalized, reason: "empty" };

  const spanMatches: PostingRoleSpanMatch[] = [];
  for (const role of CANONICAL_TAXONOMY.roles) {
    for (const term of [role.id, role.label, ...role.aliases]) {
      const normalizedTerm = normalizeWords(term);
      const singleWord = !normalizedTerm.includes(" ");
      if (singleWord && Array.from(normalizedTerm).length <= 2 && normalized !== normalizedTerm) {
        continue;
      }
      for (const [start, end] of normalizedPhraseSpans(normalized, normalizedTerm)) {
        spanMatches.push({ start, end, role_ids: [role.id] });
      }
    }
  }
  for (const ambiguous of CANONICAL_TAXONOMY.ambiguous_role_aliases) {
    const normalizedTerm = normalizeWords(ambiguous.alias);
    for (const [start, end] of normalizedPhraseSpans(normalized, normalizedTerm)) {
      spanMatches.push({ start, end, role_ids: ambiguous.candidate_role_ids });
    }
  }

  const longestMatches = spanMatches.filter((candidate, candidateIndex) =>
    !spanMatches.some((other, otherIndex) =>
      candidateIndex !== otherIndex
      && other.start <= candidate.start
      && other.end >= candidate.end
      && (other.start < candidate.start || other.end > candidate.end)));
  const matchedRoleIds = Array.from(new Set(longestMatches.flatMap((match) => match.role_ids))).sort();
  const familyIds = Array.from(new Set(matchedRoleIds.flatMap((roleId) => {
    const role = ROLES_BY_ID.get(roleId);
    return role ? [role.family_id] : [];
  }))).sort();

  if (familyIds.length === 1) {
    return {
      status: "known",
      raw,
      normalized,
      family_id: familyIds[0] ?? "",
      matched_role_ids: matchedRoleIds,
    };
  }
  if (familyIds.length > 1) {
    return {
      status: "ambiguous",
      raw,
      normalized,
      candidate_family_ids: familyIds,
      matched_role_ids: matchedRoleIds,
    };
  }
  return { status: "unknown", raw, normalized, reason: "no_known_family" };
}

function normalizedPhraseSpans(haystack: string, needle: string): Array<readonly [number, number]> {
  if (!needle) return [];
  const spans: Array<readonly [number, number]> = [];
  let fromIndex = 0;
  while (fromIndex <= haystack.length - needle.length) {
    const start = haystack.indexOf(needle, fromIndex);
    if (start < 0) break;
    const end = start + needle.length;
    const before = haystack.slice(0, start).match(/.$/u)?.[0];
    const after = haystack.slice(end).match(/^./u)?.[0];
    if (!isPostingRoleWordCharacter(before) && !isPostingRoleWordCharacter(after)) {
      spans.push([start, end]);
    }
    fromIndex = start + 1;
  }
  return spans;
}

function isPostingRoleWordCharacter(value: string | undefined): boolean {
  return value !== undefined && /^[\p{L}\p{N}]$/u.test(value);
}

interface SkillTextCandidate {
  readonly skill_id: string;
  readonly start: number;
  readonly end: number;
}

const SKILLS_BY_TERM = new Map<string, CanonicalSkill>();
const SKILL_MATCH_TERMS: Array<{ readonly skill: CanonicalSkill; readonly term: string }> = [];
const TERSE_FREE_PROSE_SKILL_IDS = new Set(["go", "r", "c"]);
for (const skill of CANONICAL_TAXONOMY.skills) {
  const seenTerms = new Set<string>();
  for (const value of [skill.id, skill.label, ...skill.aliases]) {
    const term = value.trim().toLowerCase();
    SKILLS_BY_TERM.set(term, skill);
    if (seenTerms.has(term)) continue;
    seenTerms.add(term);
    if (
      TERSE_FREE_PROSE_SKILL_IDS.has(skill.id)
      && (term === skill.id || term === skill.label.toLowerCase())
    ) continue;
    SKILL_MATCH_TERMS.push({ skill, term });
  }
}

export function canonicalSkillMatchesText(text: string, skillValue: string): boolean {
  const skill = SKILLS_BY_TERM.get(skillValue.trim().toLowerCase());
  if (!skill || !text) return false;
  const candidates = classifyCanonicalSkillCandidates(text.toLowerCase());
  return candidates.some((candidate) => candidate.skill_id === skill.id);
}

export function isCanonicalSkillValue(skillValue: string): boolean {
  return SKILLS_BY_TERM.has(skillValue.trim().toLowerCase());
}

function classifyCanonicalSkillCandidates(text: string): SkillTextCandidate[] {
  const candidates: SkillTextCandidate[] = [];
  for (const { skill, term } of SKILL_MATCH_TERMS) {
    let fromIndex = 0;
    while (fromIndex <= text.length - term.length) {
      const start = text.indexOf(term, fromIndex);
      if (start < 0) break;
      const end = start + term.length;
      if (hasSkillBoundaries(text, start, end)) candidates.push({ skill_id: skill.id, start, end });
      fromIndex = start + 1;
    }
  }
  candidates.sort((left, right) => (
    (right.end - right.start) - (left.end - left.start)
    || left.start - right.start
    || compareSkillIds(left.skill_id, right.skill_id)
  ));

  const accepted: SkillTextCandidate[] = [];
  for (const candidate of candidates) {
    if (accepted.every((existing) => candidate.end <= existing.start || candidate.start >= existing.end)) {
      accepted.push(candidate);
    }
  }
  return accepted.sort((left, right) => left.start - right.start || compareSkillIds(left.skill_id, right.skill_id));
}

function compareSkillIds(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function hasSkillBoundaries(value: string, start: number, end: number): boolean {
  const before = value.slice(0, start).match(/.$/u)?.[0];
  const after = value.slice(end).match(/^./u)?.[0];
  return !isSkillWordCharacter(before) && !isSkillWordCharacter(after);
}

function isSkillWordCharacter(value: string | undefined): boolean {
  return value !== undefined && /^[\p{L}\p{N}_]$/u.test(value);
}

export const CANONICAL_ROLE_SUGGESTIONS: readonly string[] = Object.freeze(
  CANONICAL_TAXONOMY.roles.map((role) => role.label),
);

export function canonicalRoleSuggestions(
  query: string,
  selected: readonly string[] = [],
  limit = 8,
): string[] {
  const needle = normalizeTargetRolePhrase(query);
  const selectedLabels = new Set(selected.map(normalizeTargetRolePhrase));
  const ambiguous = AMBIGUOUS_ROLE_LOOKUP.get(needle);
  const deterministic = ROLE_LOOKUP.get(needle);
  const preferredIds = ambiguous?.candidate_role_ids ?? (deterministic ? [deterministic.id] : []);
  const preferredRank = new Map(preferredIds.map((roleId, index) => [roleId, index]));
  const boundedLimit = Math.max(0, Math.floor(limit));

  return CANONICAL_TAXONOMY.roles
    .filter((role) => !selectedLabels.has(normalizeTargetRolePhrase(role.label)))
    .map((role, index) => {
      const values = [role.id, role.label, ...role.aliases].map(normalizeTargetRolePhrase);
      const rank = !needle
        ? 3
        : values.some((value) => value.startsWith(needle))
          ? 0
          : values.some((value) => value.split(" ").some((word) => word.startsWith(needle)))
            ? 1
            : values.some((value) => value.includes(needle))
              ? 2
              : 4;
      return { role, index, rank, preferred: preferredRank.get(role.id) };
    })
    .filter((entry) => entry.preferred !== undefined || entry.rank < 4)
    .sort((left, right) => {
      const leftPreferred = left.preferred ?? Number.MAX_SAFE_INTEGER;
      const rightPreferred = right.preferred ?? Number.MAX_SAFE_INTEGER;
      return leftPreferred - rightPreferred || left.rank - right.rank || left.index - right.index;
    })
    .slice(0, boundedLimit)
    .map((entry) => entry.role.label);
}

function assertRoleResolutionContract(alias: string, expectedRoleIds: readonly string[]): void {
  const resolution = resolveTargetRole(alias);
  const actualRoleIds = resolution.status === "resolved"
    ? [resolution.role.id]
    : resolution.status === "ambiguous"
      ? resolution.candidates.map((role) => role.id)
      : [];
  if (
    actualRoleIds.length !== expectedRoleIds.length
    || actualRoleIds.some((roleId, index) => roleId !== expectedRoleIds[index])
  ) {
    throw new Error(`canonical taxonomy alias ${alias} violates the portal resolution contract`);
  }
}

assertRoleResolutionContract("swe", ["software-engineer"]);
assertRoleResolutionContract("sde", ["software-engineer"]);
assertRoleResolutionContract("cra", ["clinical-research-associate"]);
assertRoleResolutionContract("crc", ["clinical-research-coordinator"]);
assertRoleResolutionContract("pm", ["product-manager", "project-manager", "program-manager"]);
assertRoleResolutionContract("tpm", ["technical-product-manager", "technical-program-manager"]);
