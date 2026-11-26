import { describe, expect, it } from "vitest";
import canonicalTaxonomySource from "../../../taxonomy/canonical-v1.json?raw";
import {
  CANONICAL_ROLE_SUGGESTIONS,
  CANONICAL_TAXONOMY,
  CANONICAL_TAXONOMY_DESCRIPTOR,
  canonicalRoleSuggestions,
  canonicalSkillMatchesText,
  canonicalTaxonomyDescriptor,
  classifyPostingRole,
  checkCanonicalTaxonomyDescriptor,
  checkJobsTaxonomyResponse,
  isCanonicalSkillValue,
  resolveTargetRole,
  validateCanonicalTaxonomy,
} from "./canonical-taxonomy";

async function sha256Hex(value: string): Promise<string> {
  const digest = await globalThis.crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

describe("canonical taxonomy projection", () => {
  it("validates and freezes the committed schema-one authority", () => {
    expect(CANONICAL_TAXONOMY.schema_version).toBe(1);
    expect(CANONICAL_TAXONOMY.taxonomy_version).toBe("bluey-jobs-taxonomy-v1-2026-08-25");
    expect(CANONICAL_TAXONOMY.roles).toHaveLength(49);
    expect(CANONICAL_TAXONOMY.skills).toHaveLength(44);
    expect(CANONICAL_TAXONOMY.countries).toHaveLength(4);
    expect(CANONICAL_TAXONOMY.subdivisions).toHaveLength(64);
    expect(CANONICAL_TAXONOMY.metros).toHaveLength(38);
    expect(CANONICAL_TAXONOMY.metros.flatMap((metro) => metro.cities)).toHaveLength(44);
    expect(Object.isFrozen(CANONICAL_TAXONOMY)).toBe(true);
    expect(Object.isFrozen(CANONICAL_TAXONOMY.roles[0]?.aliases)).toBe(true);
    expect(Object.isFrozen(CANONICAL_TAXONOMY.metros[0]?.cities[0]?.aliases)).toBe(true);
  });

  it("rejects unsupported schemas and broken role-family references", () => {
    expect(() => validateCanonicalTaxonomy({
      ...structuredClone(CANONICAL_TAXONOMY),
      schema_version: 2,
    })).toThrow("canonical taxonomy schema must be 1");
    expect(() => validateCanonicalTaxonomy({
      ...structuredClone(CANONICAL_TAXONOMY),
      roles: [{
        ...structuredClone(CANONICAL_TAXONOMY.roles[0]),
        family_id: "missing-family",
      }],
    })).toThrow("references an unknown family");
  });

  it("resolves deterministic aliases with exact token-safe matches", () => {
    expect(resolveTargetRole("SWE")).toMatchObject({
      status: "resolved",
      review_required: false,
      role: { id: "software-engineer", label: "Software Engineer" },
    });
    expect(resolveTargetRole("SDE")).toMatchObject({ status: "resolved", role: { id: "software-engineer" } });
    expect(resolveTargetRole("CRA")).toMatchObject({
      status: "resolved",
      role: { id: "clinical-research-associate" },
    });
    expect(resolveTargetRole("CRC")).toMatchObject({
      status: "resolved",
      role: { id: "clinical-research-coordinator" },
    });
    expect(resolveTargetRole("Senior Software Engineer II")).toMatchObject({
      status: "resolved",
      role: { id: "software-engineer" },
    });
    for (const layeredSeniority of [
      "Entry Level Senior Software Engineer",
      "Senior Entry Level Software Engineer",
    ]) {
      expect(resolveTargetRole(layeredSeniority)).toMatchObject({
        status: "resolved",
        role: { id: "software-engineer" },
      });
    }
    expect(resolveTargetRole("SWEPT")).toMatchObject({ status: "custom", review_required: true });
    expect(resolveTargetRole("software-engineer")).toMatchObject({
      status: "resolved",
      matched_by: "id",
      role: { id: "software-engineer" },
    });
    expect(resolveTargetRole("Capital One, Senior Software Engineer II")).toMatchObject({
      status: "custom",
      review_required: true,
    });
  });

  it("keeps PM and TPM ambiguous instead of guessing a target role", () => {
    const pm = resolveTargetRole("PM");
    const tpm = resolveTargetRole("TPM");
    expect(pm.status).toBe("ambiguous");
    expect(pm.status === "ambiguous" ? pm.candidates.map((role) => role.id) : []).toEqual([
      "product-manager",
      "project-manager",
      "program-manager",
    ]);
    expect(tpm.status).toBe("ambiguous");
    expect(tpm.status === "ambiguous" ? tpm.candidates.map((role) => role.id) : []).toEqual([
      "technical-product-manager",
      "technical-program-manager",
    ]);
  });

  it("classifies posting-title spans without selecting ambiguous or terse aliases", () => {
    expect(classifyPostingRole("Software Engineer, Backend")).toMatchObject({
      status: "known",
      family_id: "software-engineering",
      matched_role_ids: ["backend-engineer", "software-engineer"],
    });
    expect(classifyPostingRole("Technical Product Manager")).toMatchObject({
      status: "known",
      family_id: "product-management",
      matched_role_ids: ["technical-product-manager"],
    });
    expect(classifyPostingRole("Product Manager / Project Manager")).toMatchObject({
      status: "ambiguous",
      candidate_family_ids: ["product-management", "project-management"],
    });
    expect(classifyPostingRole("PM / Software Engineer")).toMatchObject({
      status: "ambiguous",
      candidate_family_ids: [
        "product-management",
        "program-management",
        "project-management",
        "software-engineering",
      ],
    });
    expect(classifyPostingRole("QA Coordinator")).toMatchObject({
      status: "unknown",
      reason: "no_known_family",
    });
    expect(classifyPostingRole("DE&I Specialist")).toMatchObject({
      status: "unknown",
      reason: "no_known_family",
    });
    expect(classifyPostingRole("QA")).toMatchObject({
      status: "known",
      family_id: "software-engineering",
    });
  });

  it("preserves custom roles as review-required input", () => {
    expect(resolveTargetRole("Clinical AI Workflow Specialist")).toEqual({
      status: "custom",
      review_required: true,
      input: "Clinical AI Workflow Specialist",
      custom_label: "Clinical AI Workflow Specialist",
      reason: "unknown_role",
    });
    expect(resolveTargetRole("---")).toMatchObject({
      status: "custom",
      review_required: true,
      reason: "empty_role",
    });
  });

  it("projects bounded metro and city authority with nullable subdivision support", () => {
    expect(CANONICAL_TAXONOMY.metros.find((metro) => metro.id === "US-DC-washington-metro")).toMatchObject({
      country_code: "US",
      subdivision_ids: ["US-DC", "US-VA"],
      cities: [
        { id: "US-DC-washington", subdivision_id: "US-DC" },
        { id: "US-VA-arlington", subdivision_id: "US-VA" },
        { id: "US-VA-falls-church", subdivision_id: "US-VA" },
      ],
    });
    expect(CANONICAL_TAXONOMY.metros.find((metro) => metro.id === "IN-bengaluru-metro")).toMatchObject({
      country_code: "IN",
      subdivision_ids: [],
      cities: [{ id: "IN-bengaluru", subdivision_id: null, aliases: ["Bangalore"] }],
    });
  });

  it("rejects malformed, duplicate, and cross-boundary metro data", () => {
    const firstMetro = CANONICAL_TAXONOMY.metros[0];
    const secondMetro = CANONICAL_TAXONOMY.metros[1];
    expect(firstMetro).toBeDefined();
    expect(secondMetro).toBeDefined();
    if (!firstMetro || !secondMetro) throw new Error("metro fixtures are unavailable");

    expect(() => validateCanonicalTaxonomy({
      ...structuredClone(CANONICAL_TAXONOMY),
      metros: CANONICAL_TAXONOMY.metros.map((metro, index) => (
        index === 0 ? { ...metro, unsupported: true } : metro
      )),
    })).toThrow("unsupported field set");
    expect(() => validateCanonicalTaxonomy({
      ...structuredClone(CANONICAL_TAXONOMY),
      metros: CANONICAL_TAXONOMY.metros.map((metro, index) => (
        index === 0
          ? {
              ...metro,
              cities: metro.cities.map((city, cityIndex) => (
                cityIndex === 0 ? { ...city, unsupported: true } : city
              )),
            }
          : metro
      )),
    })).toThrow("unsupported field set");
    expect(() => validateCanonicalTaxonomy({
      ...structuredClone(CANONICAL_TAXONOMY),
      metros: CANONICAL_TAXONOMY.metros.map((metro, index) => (
        index === 1 ? { ...metro, id: firstMetro.id } : metro
      )),
    })).toThrow("duplicate metro id");
    expect(() => validateCanonicalTaxonomy({
      ...structuredClone(CANONICAL_TAXONOMY),
      metros: CANONICAL_TAXONOMY.metros.map((metro, index) => (
        index === 1 ? { ...metro, aliases: [firstMetro.label] } : metro
      )),
    })).toThrow("resolves to multiple metros");
    expect(() => validateCanonicalTaxonomy({
      ...structuredClone(CANONICAL_TAXONOMY),
      metros: CANONICAL_TAXONOMY.metros.map((metro, index) => (
        index === 0 ? { ...metro, subdivision_ids: ["US-ZZ"] } : metro
      )),
    })).toThrow("references an invalid subdivision");
    expect(() => validateCanonicalTaxonomy({
      ...structuredClone(CANONICAL_TAXONOMY),
      metros: CANONICAL_TAXONOMY.metros.map((metro, index) => (
        index === 1
          ? {
              ...metro,
              cities: metro.cities.map((city, cityIndex) => (
                cityIndex === 0 ? { ...city, id: firstMetro.cities[0]?.id } : city
              )),
            }
          : metro
      )),
    })).toThrow("duplicate city id");
    expect(() => validateCanonicalTaxonomy({
      ...structuredClone(CANONICAL_TAXONOMY),
      metros: CANONICAL_TAXONOMY.metros.map((metro, index) => (
        index === 1
          ? {
              ...metro,
              cities: metro.cities.map((city, cityIndex) => (
                cityIndex === 0 ? { ...city, subdivision_id: "US-NY" } : city
              )),
            }
          : metro
      )),
    })).toThrow("outside its metro");
    expect(() => validateCanonicalTaxonomy({
      ...structuredClone(CANONICAL_TAXONOMY),
      metros: CANONICAL_TAXONOMY.metros.map((metro, index) => (
        index === 1
          ? {
              ...metro,
              cities: metro.cities.map((city, cityIndex) => (
                cityIndex === 1 ? { ...city, aliases: [metro.cities[0]?.label ?? ""] } : city
              )),
            }
          : metro
      )),
    })).toThrow("resolves to multiple cities");
  });

  it("matches canonical skills with token and symbol boundaries", () => {
    expect(canonicalSkillMatchesText("Built production services in Golang.", "Go")).toBe(true);
    expect(canonicalSkillMatchesText("Maintained Django services.", "Go")).toBe(false);
    expect(canonicalSkillMatchesText("Used R programming for statistical analysis.", "R")).toBe(true);
    expect(canonicalSkillMatchesText("Built Rust services.", "R")).toBe(false);
    expect(canonicalSkillMatchesText("Built Rust services.", "Rust")).toBe(true);
    expect(canonicalSkillMatchesText("Developed C# services.", "C#")).toBe(true);
    expect(canonicalSkillMatchesText("Developed C# services.", "C++")).toBe(false);
    expect(canonicalSkillMatchesText("Developed C# services.", "C")).toBe(false);
    expect(canonicalSkillMatchesText("Developed C++ services.", "C++")).toBe(true);
    expect(canonicalSkillMatchesText("Developed C++ services.", "C#")).toBe(false);
    expect(canonicalSkillMatchesText("Developed C++ services.", "C")).toBe(false);
  });

  it("requires contextual aliases for terse language skills in free prose", () => {
    for (const prose of ["Go to market", "Ready to go", "Owned go-to-market planning"]) {
      expect(canonicalSkillMatchesText(prose, "Go")).toBe(false);
    }
    expect(canonicalSkillMatchesText("Partnered with R&D", "R")).toBe(false);
    expect(canonicalSkillMatchesText("Presented to the C-suite", "C")).toBe(false);

    expect(canonicalSkillMatchesText("Built APIs in Golang", "Go")).toBe(true);
    expect(canonicalSkillMatchesText("Used the R language for forecasting", "R")).toBe(true);
    expect(canonicalSkillMatchesText("R programming and tidyverse", "R")).toBe(true);
    expect(canonicalSkillMatchesText("Maintained C language libraries", "C")).toBe(true);
    expect(canonicalSkillMatchesText("C programming on embedded systems", "C")).toBe(true);
    expect(isCanonicalSkillValue("Go")).toBe(true);
    expect(isCanonicalSkillValue("R")).toBe(true);
    expect(isCanonicalSkillValue("C")).toBe(true);
  });

  it("produces stable suggestions from taxonomy order and ambiguous candidates", () => {
    expect(canonicalRoleSuggestions("", [], 100)).toEqual(CANONICAL_ROLE_SUGGESTIONS);
    expect(canonicalRoleSuggestions("pm", [], 2)).toEqual(["Product Manager", "Project Manager"]);
    expect(canonicalRoleSuggestions("tpm", [], 2)).toEqual([
      "Technical Product Manager",
      "Technical Program Manager",
    ]);
    expect(canonicalRoleSuggestions("pm", ["Product Manager"], 2)).toEqual([
      "Project Manager",
      "Program Manager",
    ]);
  });

  it("computes the exact raw-file digest without a hardcoded checksum", async () => {
    const descriptor = await canonicalTaxonomyDescriptor();
    expect(descriptor.digest_sha256).toMatch(/^[a-f0-9]{64}$/);
    expect(descriptor.digest_sha256).toBe(await sha256Hex(canonicalTaxonomySource));
    expect(Object.isFrozen(descriptor)).toBe(true);
  });

  it("fails stale, missing, malformed, and digest-mismatched server descriptors closed", async () => {
    const local = await canonicalTaxonomyDescriptor();
    expect(checkCanonicalTaxonomyDescriptor(local, local)).toEqual({
      status: "current",
      refresh_required: false,
      reason: null,
    });
    expect(checkCanonicalTaxonomyDescriptor(local)).toEqual({
      status: "checking",
      refresh_required: false,
      reason: "local_digest_pending",
    });
    expect(checkCanonicalTaxonomyDescriptor(null, local)).toMatchObject({
      status: "refresh_required",
      reason: "missing_server_descriptor",
    });
    expect(checkCanonicalTaxonomyDescriptor({ ...local, schema_version: 2 }, local)).toMatchObject({
      status: "refresh_required",
      reason: "schema_version_mismatch",
    });
    expect(checkCanonicalTaxonomyDescriptor({ ...local, taxonomy_version: "stale" }, local)).toMatchObject({
      status: "refresh_required",
      reason: "taxonomy_version_mismatch",
    });
    expect(checkCanonicalTaxonomyDescriptor({ ...local, digest_sha256: "invalid" }, local)).toMatchObject({
      status: "refresh_required",
      reason: "invalid_server_digest",
    });
    expect(checkCanonicalTaxonomyDescriptor({ ...local, digest_sha256: "0".repeat(64) }, local)).toMatchObject({
      status: "refresh_required",
      reason: "digest_mismatch",
    });
    expect(CANONICAL_TAXONOMY_DESCRIPTOR.digest_sha256).toBeNull();
  });

  it("validates the authenticated camel-case taxonomy response before descriptor equality", async () => {
    const local = await canonicalTaxonomyDescriptor();
    const response = {
      taxonomyVersion: CANONICAL_TAXONOMY.taxonomy_version,
      taxonomySha256: local.digest_sha256,
      registry: structuredClone(CANONICAL_TAXONOMY),
    };
    expect(checkJobsTaxonomyResponse(response, local)).toEqual({
      status: "current",
      refresh_required: false,
      reason: null,
    });
    expect(checkJobsTaxonomyResponse({ ...response, taxonomyVersion: "stale" }, local)).toMatchObject({
      status: "refresh_required",
      reason: "server_registry_mismatch",
    });
    expect(checkJobsTaxonomyResponse({
      ...response,
      registry: {
        ...structuredClone(CANONICAL_TAXONOMY),
        metros: CANONICAL_TAXONOMY.metros.map((metro, index) => (
          index === 0
            ? {
                ...metro,
                cities: metro.cities.map((city, cityIndex) => (
                  cityIndex === 0 ? { ...city, label: "Altered Washington" } : city
                )),
              }
            : metro
        )),
      },
    }, local)).toMatchObject({
      status: "refresh_required",
      reason: "server_registry_mismatch",
    });
    expect(checkJobsTaxonomyResponse({
      ...response,
      registry: {
        ...structuredClone(CANONICAL_TAXONOMY),
        taxonomy_version: ` ${CANONICAL_TAXONOMY.taxonomy_version}`,
      },
    }, local)).toMatchObject({
      status: "refresh_required",
      reason: "server_registry_mismatch",
    });
    expect(checkJobsTaxonomyResponse({
      ...response,
      registry: {
        ...structuredClone(CANONICAL_TAXONOMY),
        roles: CANONICAL_TAXONOMY.roles.map((role, index) => (
          index === 0 ? { ...role, label: "Altered Software Engineer" } : role
        )),
      },
    }, local)).toMatchObject({
      status: "refresh_required",
      reason: "server_registry_mismatch",
    });
    expect(checkJobsTaxonomyResponse({ ...response, registry: { schema_version: 1 } }, local)).toMatchObject({
      status: "refresh_required",
      reason: "invalid_server_registry",
    });
    expect(checkJobsTaxonomyResponse({ taxonomyVersion: response.taxonomyVersion }, local)).toMatchObject({
      status: "refresh_required",
      reason: "invalid_server_response",
    });
  });
});
