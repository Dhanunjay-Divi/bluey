import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, decodeInterviewPrepResponse, InterviewPrepBalanceRequired, jobsApi, matchesPageFromWire } from "./api";
import type { InterviewPrepCompletionResponse } from "./api";
import { previewWorkspace } from "./data/preview";
import {
  CANONICAL_TAXONOMY,
  canonicalTaxonomyDescriptor,
} from "./lib/canonical-taxonomy";
import type { CareerTrackPolicyAuthority, JobPosting, JobsWorkspace } from "./types";

class MemoryStorage implements Storage {
  private readonly values = new Map<string, string>();

  get length(): number {
    return this.values.size;
  }

  clear(): void {
    this.values.clear();
  }

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  key(index: number): string | null {
    return [...this.values.keys()][index] ?? null;
  }

  removeItem(key: string): void {
    this.values.delete(key);
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

function communicationActionResponse(status = "awaiting_approval") {
  const executionAvailable = status === "awaiting_approval" || status === "approved";
  const createdAtMs = 1_786_000_000_000;
  const updatedAtMs = status === "awaiting_approval" ? createdAtMs : createdAtMs + 1_000;
  return {
    id: "action-one",
    application_id: "application/one",
    connection_id: "connection-one",
    source_message_id: "message-one",
    kind: "reply",
    provider: "gmail",
    payload_sha256: "a".repeat(64),
    action_revision: status === "awaiting_approval" ? 1 : 2,
    status,
    execution_available: executionAvailable,
    execution_unavailable_reason: executionAvailable
      ? ""
      : "This communication is not awaiting executable approval.",
    approved_at_ms: status === "approved" ? updatedAtMs : null,
    dispatched_at_ms: null,
    created_at_ms: createdAtMs,
    updated_at_ms: updatedAtMs,
    connection_account_label: "candidate@gmail.com",
    source_context: {
      sender: "recruiter@example.org",
      reply_target: "recruiter@example.org",
      subject: "Interview availability",
      received_at_ms: 1_785_999_000_000,
    },
    payload: {
      to: "recruiter@example.org",
      subject: "Interview availability",
      body_text: "Tuesday afternoon works for me.",
    },
  };
}

async function currentTaxonomyHttpResponse() {
  const descriptor = await canonicalTaxonomyDescriptor();
  if (!descriptor.digest_sha256) throw new Error("canonical taxonomy digest is unavailable");
  return {
    taxonomyVersion: descriptor.taxonomy_version,
    taxonomySha256: descriptor.digest_sha256,
    registry: CANONICAL_TAXONOMY,
  };
}

function policyAuthority(applicationIdentityId: string): CareerTrackPolicyAuthority {
  return {
    taxonomy_version: "bluey-jobs-taxonomy-v1-2026-08-25",
    taxonomy_sha256: "1".repeat(64),
    taxonomy_activation_epoch: 1,
    canonicalizer_schema_version: 1,
    canonicalizer_sha256: "2".repeat(64),
    account_input_generation: 1,
    account_input_transition_sha256: "3".repeat(64),
    account_input_semantic_sha256: "4".repeat(64),
    track_input_generation: 1,
    track_input_transition_sha256: "5".repeat(64),
    track_semantic_sha256: "6".repeat(64),
    canonical_role_id: "software-engineer",
    canonical_role_family_id: "software-engineering",
    canonical_location_ids: ["country:US"],
    source_resume_asset_id: "resume-one",
    source_resume_sha256: "7".repeat(64),
    applicationIdentityId,
    application_identity_sha256: "8".repeat(64),
    job_preferences_sha256: "9".repeat(64),
    policy_revision_id: "revision-one",
    policy_revision_no: 1,
    canonical_policy_sha256: "a".repeat(64),
    policy_head_generation: 1,
    policy_head_transition_sha256: "b".repeat(64),
    policy_review_receipt_id: "receipt-one",
    policy_review_receipt_sha256: "c".repeat(64),
    review_state: "approved",
    review_reason_codes: [],
  };
}

function interviewPrepResponse(applicationId = "application/one"): InterviewPrepCompletionResponse {
  return {
    schema_version: 1,
    id: `prep-${"a".repeat(24)}`,
    application_id: applicationId,
    content: "Prepare a concise example about reliable delivery and ask a role-specific question.",
    generated_at_ms: 1_786_000_000_000,
    grounding: {
      receipt_id: "receipt-one",
      receipt_fingerprint: "b".repeat(64),
      resume_version_id: "resume-one",
      resume_checksum: "resume-checksum-v3",
      resume_document_sha256: "c".repeat(64),
      answer_keys_used: ["availability"],
      answer_keys_omitted: ["phone"],
    },
    provider: "openai",
    model: "gpt-5",
    cost_cents: 3,
    balance_cents_after: -2,
    trial_seconds_remaining: 0,
    cost_label: null,
    confidence: 0.9,
  };
}

function verifiedCuratedLeadResponse(id = "lead/one?") {
  return {
    ...previewWorkspace.matches[0],
    id,
    source: "greenhouse_import",
    availability_status: "active",
  };
}

function requirementsLoadResponse(id = "summary-one"): JobPosting {
  const prior = previewWorkspace.matches[0];
  return {
    ...prior,
    id,
    source: "smartrecruiters",
    canonical_url: "https://jobs.smartrecruiters.com/ExactTenant/stable-record-42",
    description: "Build reliable systems.",
    eligibility: {
      ...prior.eligibility!,
      capability: "blocked",
      can_prepare: false,
      can_queue_local: false,
      can_queue_cloud: false,
      can_auto_submit: false,
      review_reasons: [{ code: "original_source_unverified", message: "Verify this posting." }],
    },
  };
}

describe("Jobs API authentication", () => {
  const receiptFingerprint = "b".repeat(64);
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.stubGlobal("localStorage", new MemoryStorage());
    vi.stubGlobal("sessionStorage", new MemoryStorage());
  });

  it("sends interview preparation only with explicit per-request opt-in", async () => {
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) =>
      Response.json(interviewPrepResponse()));
    vi.stubGlobal("fetch", fetchMock);

    await expect(jobsApi.prepareInterview("application/one", false as unknown as true, receiptFingerprint))
      .rejects.toThrow("Interview preparation requires opt-in.");
    expect(fetchMock).not.toHaveBeenCalled();

    const result = await jobsApi.prepareInterview("application/one", true, receiptFingerprint);
    expect(result.application_id).toBe("application/one");
    expect(result.grounding.receipt_fingerprint).toBe("b".repeat(64));
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [path, init] = fetchMock.mock.calls[0];
    expect(path).toBe("/api/jobs/applications/application%2Fone/interview-prep");
    expect(init?.method).toBe("POST");
    expect(init?.body).toBe(JSON.stringify({ prepare: true, receipt_fingerprint: receiptFingerprint }));
  });

  it("does not dispatch without a valid workspace receipt fingerprint", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    await expect(jobsApi.prepareInterview("application/one", true, "INVALID")).rejects.toThrow("confirm the submitted receipt");
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("does not display a draft for a changed same-ID receipt", async () => {
    const fetchMock = vi.fn(async () => Response.json(interviewPrepResponse()));
    vi.stubGlobal("fetch", fetchMock);
    await expect(jobsApi.prepareInterview("application/one", true, "a".repeat(64))).rejects.toThrow("submitted receipt changed");
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("identifies the prep balance refusal without retrying by itself", async () => {
    const fetchMock = vi.fn(async () => new Response("insufficient balance", { status: 402 }));
    vi.stubGlobal("fetch", fetchMock);
    await expect(jobsApi.prepareInterview("application/one", true, receiptFingerprint)).rejects.toBeInstanceOf(InterviewPrepBalanceRequired);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it.each([
    [402, "payment pending"],
    [402, '{"reason":"insufficient_balance"}'],
    [409, "insufficient balance"],
    [429, "Try again later"],
    [502, "insufficient balance"],
    [503, "provider accounting pending"],
  ])("does not enable prep retry for an unrecognized %s refusal", async (status, body) => {
    const fetchMock = vi.fn(async () => new Response(body, { status }));
    vi.stubGlobal("fetch", fetchMock);
    const result = await jobsApi.prepareInterview("application/one", true, receiptFingerprint).catch((error: unknown) => error);
    expect(result).toBeInstanceOf(ApiError);
    expect(result).not.toBeInstanceOf(InterviewPrepBalanceRequired);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("keeps an interrupted prep request distinct from a balance refusal", async () => {
    const fetchMock = vi.fn(async () => { throw new DOMException("request interrupted", "AbortError"); });
    vi.stubGlobal("fetch", fetchMock);
    const result = await jobsApi.prepareInterview("application/one", true, receiptFingerprint).catch((error: unknown) => error);
    expect(result).not.toBeInstanceOf(InterviewPrepBalanceRequired);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("rejects unknown, incomplete, malformed, and cross-application interview prep payloads", () => {
    const payloads: unknown[] = [
      { ...interviewPrepResponse(), untrusted: true },
      (() => {
        const payload = interviewPrepResponse();
        delete (payload as Partial<typeof payload>).grounding;
        return payload;
      })(),
      interviewPrepResponse("application/two"),
      {
        ...interviewPrepResponse(),
        grounding: {
          ...interviewPrepResponse().grounding,
          receipt_fingerprint: "not-a-fingerprint",
        },
      },
      {
        ...interviewPrepResponse(),
        grounding: {
          ...interviewPrepResponse().grounding,
          answer_keys_omitted: ["availability"],
        },
      },
      { ...interviewPrepResponse(), id: [`prep-${"a".repeat(24)}`] },
      { ...interviewPrepResponse(), confidence: 1.1 },
    ];

    for (const payload of payloads) {
      expect(() => decodeInterviewPrepResponse(payload, "application/one")).toThrow(
        "Bluey Jobs returned an invalid interview preparation response.",
      );
    }
  });

  it("accepts only the closed v1 interview prep response with nullable server fields", () => {
    const payload = interviewPrepResponse();
    payload.content = "Prepare a STAR story.\n\n\tAsk a concise role-specific question.";
    payload.cost_label = "Included with Jobs beta";
    payload.confidence = null;

    expect(decodeInterviewPrepResponse(payload, "application/one")).toEqual(payload);
  });

  it("verifies one selected curated lead with an encoded bodyless POST", async () => {
    const id = "lead/one?";
    const fetchMock = vi.fn(async () => Response.json(verifiedCuratedLeadResponse(id)));
    vi.stubGlobal("fetch", fetchMock);

    await expect(jobsApi.verifySelectedCuratedLead(id)).resolves.toEqual(
      verifiedCuratedLeadResponse(id),
    );

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [path, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe("/api/jobs/matches/lead%2Fone%3F/verify-original");
    expect(init.method).toBe("POST");
    expect(init.body).toBeUndefined();
    expect(init.cache).toBe("no-store");
  });

  it("does not retry selected-lead verification after a non-authentication refusal", async () => {
    const fetchMock = vi.fn(async () => Response.json({ message: "not available" }, { status: 409 }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(jobsApi.verifySelectedCuratedLead("lead-one")).rejects.toMatchObject({
      status: 409,
      message: "not available",
    } satisfies Partial<ApiError>);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("refuses selected-lead verification responses that do not prove the same imported lead", async () => {
    const invalidResponses = [
      verifiedCuratedLeadResponse("another-lead"),
      { ...verifiedCuratedLeadResponse(), source: "curated_feed:feed-one" },
    ];

    for (const response of invalidResponses) {
      vi.stubGlobal("fetch", vi.fn(async () => Response.json(response)));
      await expect(jobsApi.verifySelectedCuratedLead("lead/one?")).rejects.toMatchObject({
        status: 0,
        message: "Bluey Jobs could not confirm this match. Refresh Matches before trying again.",
      } satisfies Partial<ApiError>);
    }
  });

  it("loads requirements for one encoded saved match with a bodyless POST", async () => {
    const id = "summary/one?";
    const posting = { ...requirementsLoadResponse(id), track_id: "track-one" };
    const fetchMock = vi.fn(async () => Response.json(posting));
    vi.stubGlobal("fetch", fetchMock);

    await expect(jobsApi.loadSmartRecruitersRequirements(
      id,
      "track-one",
      posting.source,
      posting.canonical_url,
    )).resolves.toEqual(posting);

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [path, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe("/api/jobs/matches/summary%2Fone%3F/load-requirements");
    expect(init.method).toBe("POST");
    expect(init.body).toBeUndefined();
    expect(init.cache).toBe("no-store");
  });

  it("refuses a requirements response for a different match, Track, or unrefreshed eligibility", async () => {
    const expected = { ...requirementsLoadResponse(), track_id: "track-one" };
    const unsafeActionResponses = ([
      "can_prepare",
      "can_queue_local",
      "can_queue_cloud",
      "can_auto_submit",
    ] as const).flatMap((field) => {
      const missing = { ...expected.eligibility } as Partial<NonNullable<JobPosting["eligibility"]>>;
      delete missing[field];
      return [
        { ...expected, eligibility: { ...expected.eligibility!, [field]: true } },
        { ...expected, eligibility: missing },
      ];
    });
    for (const posting of [
      { ...expected, id: "another-match" },
      { ...expected, track_id: "another-track" },
      { ...expected, source: "smartrecruiters_import" },
      { ...expected, canonical_url: "https://jobs.smartrecruiters.com/OtherTenant/stable-record-42" },
      { ...expected, description: "  " },
      { ...expected, eligibility: undefined },
      {
        ...expected,
        eligibility: {
          ...expected.eligibility!,
          review_reasons: [{ code: "posting_requirements_not_loaded", message: "Load requirements." }],
        },
      },
      ...unsafeActionResponses,
    ]) {
      vi.stubGlobal("fetch", vi.fn(async () => Response.json(posting)));
      await expect(
        jobsApi.loadSmartRecruitersRequirements(
          "summary-one",
          "track-one",
          expected.source,
          expected.canonical_url,
        ),
      ).rejects.toMatchObject({
        status: 0,
        message: "Bluey Jobs could not confirm this match. Refresh Matches before trying again.",
      } satisfies Partial<ApiError>);
    }
  });

  it("deduplicates concurrent refreshes for the initial workspace load", async () => {
    localStorage.setItem("bluey_access_token", "dummy-expired-access-token");
    localStorage.setItem("bluey_refresh_token", "dummy-refresh-token-one");
    let refreshCalls = 0;
    const authorizationHeaders: string[] = [];

    vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      if (path === "/auth/refresh") {
        refreshCalls += 1;
        await Promise.resolve();
        return Response.json({
          access_token: "dummy-fresh-access-token",
          refresh_token: "dummy-refresh-token-two",
        });
      }

      const authorization = new Headers(init?.headers).get("Authorization") ?? "";
      authorizationHeaders.push(`${path}:${authorization}`);
      if (authorization !== "Bearer dummy-fresh-access-token") {
        return Response.json({ message: "expired" }, { status: 401 });
      }
      if (path === "/api/jobs/workspace") return Response.json({ profile: {} });
      if (path === "/account/me") return Response.json({ email: "user@example.com", balance_cents: 0 });
      return Response.json({ message: "not found" }, { status: 404 });
    }));

    const [workspace, account] = await Promise.all([jobsApi.workspace(), jobsApi.account()]);

    expect(refreshCalls).toBe(1);
    expect(workspace.profile).toEqual({});
    expect(workspace.matches_page).toEqual({
      schema_version: 1,
      page_size: 1,
      has_more: false,
      next_cursor: null,
    });
    expect(account.email).toBe("user@example.com");
    expect(localStorage.getItem("bluey_access_token")).toBe("dummy-fresh-access-token");
    expect(localStorage.getItem("bluey_refresh_token")).toBe("dummy-refresh-token-two");
    expect(authorizationHeaders).toEqual(expect.arrayContaining([
      "/api/jobs/workspace:Bearer dummy-expired-access-token",
      "/account/me:Bearer dummy-expired-access-token",
      "/api/jobs/workspace:Bearer dummy-fresh-access-token",
      "/account/me:Bearer dummy-fresh-access-token",
    ]));
  });

  it.each(["replacement", "sign-out"] as const)("discards an in-flight refresh after %s", async (change) => {
    localStorage.setItem("bluey_access_token", "dummy-old-access");
    localStorage.setItem("bluey_refresh_token", "dummy-old-refresh");
    const refresh = deferred<Response>();
    const started = deferred<void>();
    const fetchMock = vi.fn(async (input: RequestInfo | URL, _init?: RequestInit) => {
      if (String(input) === "/auth/refresh") {
        started.resolve();
        return refresh.promise;
      }
      return Response.json({ message: "expired" }, { status: 401 });
    });
    vi.stubGlobal("fetch", fetchMock);
    const request = jobsApi.verifySelectedCuratedLead("previous-account-lead");
    const outcome = expect(request).rejects.toMatchObject({ status: 401 });
    await started.promise;
    localStorage.clear();
    if (change === "replacement") {
      localStorage.setItem("bluey_access_token", "dummy-replacement-access");
      localStorage.setItem("bluey_refresh_token", "dummy-replacement-refresh");
    }
    refresh.resolve(Response.json({ access_token: "dummy-old-rotated", refresh_token: "dummy-old-rotated-refresh" }));
    await outcome;
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(localStorage.getItem("bluey_access_token")).toBe(change === "replacement" ? "dummy-replacement-access" : null);
    expect(localStorage.getItem("bluey_refresh_token")).toBe(change === "replacement" ? "dummy-replacement-refresh" : null);
  });

  it.each(["superseded", "mixed stores"] as const)("does not refresh a download with %s credentials", async (kind) => {
    localStorage.setItem("bluey_access_token", "dummy-old-access");
    (kind === "superseded" ? localStorage : sessionStorage).setItem("bluey_refresh_token", "dummy-old-refresh");
    const unauthorized = deferred<Response>();
    const fetchMock = vi.fn((_input: RequestInfo | URL, _init?: RequestInit) => unauthorized.promise);
    vi.stubGlobal("fetch", fetchMock);
    const download = jobsApi.downloadTemplateResume("old-resume");
    const outcome = expect(download).rejects.toMatchObject({ status: 401 });
    if (kind === "superseded") {
      localStorage.setItem("bluey_access_token", "dummy-replacement-access");
      localStorage.setItem("bluey_refresh_token", "dummy-replacement-refresh");
    }
    unauthorized.resolve(new Response("expired", { status: 401 }));
    await outcome;
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(localStorage.getItem("bluey_access_token")).toBe(
      kind === "superseded" ? "dummy-replacement-access" : "dummy-old-access",
    );
  });

  it("reuses the same in-tab rotation when an original 401 arrives late", async () => {
    localStorage.setItem("bluey_access_token", "dummy-old-access");
    localStorage.setItem("bluey_refresh_token", "dummy-old-refresh");
    const late = deferred<Response>();
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      if (String(input) === "/auth/refresh") {
        return Response.json({ access_token: "dummy-rotated-access", refresh_token: "dummy-rotated-refresh" });
      }
      if (new Headers(init?.headers).get("Authorization") === "Bearer dummy-rotated-access") {
        return Response.json({ email: "candidate@example.test", profile: {} });
      }
      if (String(input) === "/account/me") return late.promise;
      return Response.json({ message: "expired" }, { status: 401 });
    });
    vi.stubGlobal("fetch", fetchMock);
    const account = jobsApi.account();
    await jobsApi.workspace();
    late.resolve(Response.json({ message: "expired" }, { status: 401 }));
    await expect(account).resolves.toMatchObject({ email: "candidate@example.test" });
    expect(fetchMock.mock.calls.filter(([url]) => url === "/auth/refresh")).toHaveLength(1);
  });

  it.each(["workspace", "download"] as const)("discards a successful %s response from a replaced session", async (kind) => {
    localStorage.setItem("bluey_access_token", "dummy-old-access");
    const pending = deferred<Response>();
    vi.stubGlobal("fetch", vi.fn(() => pending.promise));
    const request = kind === "workspace" ? jobsApi.workspace() : jobsApi.downloadTemplateResume("old-resume");
    const outcome = expect(request).rejects.toMatchObject({ status: 401, message: "Bluey Jobs session changed." });
    localStorage.setItem("bluey_access_token", "dummy-new-access");
    pending.resolve(Response.json({ profile: { headline: "old account" } }));
    await outcome;
  });

  it("discards a successful download if sign-out happens while reading its body", async () => {
    localStorage.setItem("bluey_access_token", "dummy-old-access");
    const blob = deferred<Blob>();
    const reading = deferred<void>();
    const response = new Response("synthetic");
    vi.spyOn(response, "blob").mockImplementation(() => {
      reading.resolve();
      return blob.promise;
    });
    vi.stubGlobal("fetch", vi.fn(async () => response));
    const download = jobsApi.downloadTemplateResume("old-resume");
    const outcome = expect(download).rejects.toMatchObject({ status: 401 });
    await reading.promise;
    localStorage.clear();
    blob.resolve(new Blob(["old document"]));
    await outcome;
  });

  it("clears a partial rotation after a storage error and does not retry", async () => {
    localStorage.setItem("bluey_access_token", "dummy-old-access");
    localStorage.setItem("bluey_refresh_token", "dummy-old-refresh");
    const setItem = localStorage.setItem.bind(localStorage);
    vi.spyOn(localStorage, "setItem").mockImplementation((key, value) => {
      if (key === "bluey_refresh_token" && value === "dummy-new-refresh") throw new Error("storage write rejected");
      setItem(key, value);
    });
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => String(input) === "/auth/refresh"
      ? Response.json({ access_token: "dummy-new-access", refresh_token: "dummy-new-refresh" })
      : Response.json({ message: "expired" }, { status: 401 }));
    vi.stubGlobal("fetch", fetchMock);
    await expect(jobsApi.workspace()).rejects.toMatchObject({ status: 401 });
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(localStorage.getItem("bluey_access_token")).toBeNull();
    expect(localStorage.getItem("bluey_refresh_token")).toBeNull();
  });

  it("keeps verified-company lookup and explicit saved-answer binding in the authenticated API", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const scope = { job_id: "job/one", scope_id: `company-v1-${"a".repeat(64)}`,
      company: "Example", domain: "example.test" };
    const answer = { ...previewWorkspace.answer_memory[0], id: "", scope: "company" as const,
      scope_id: scope.scope_id, company_job_id: scope.job_id, expected_company_scope: scope.scope_id };
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => String(input).includes("answer-company-scopes")
      ? Response.json(scope) : Response.json({ ...answer, id: "saved-answer" }));
    vi.stubGlobal("fetch", fetchMock);
    await expect(jobsApi.answerCompanyScope("job/one")).resolves.toEqual(scope);
    await jobsApi.saveAnswerMemory(answer);
    const calls = fetchMock.mock.calls as unknown as Array<[string, RequestInit]>;
    expect(calls[0][0]).toBe("/api/jobs/answer-company-scopes/job%2Fone");
    expect(calls[0][1].cache).toBe("no-store");
    expect(new Headers(calls[0][1].headers).get("Authorization")).toBe("Bearer dummy-access-token");
    expect(calls[1][0]).toBe("/api/jobs/answers");
    expect(calls[1][1].method).toBe("POST");
    expect(JSON.parse(String(calls[1][1].body))).toMatchObject({
      company_job_id: "job/one", expected_company_scope: scope.scope_id,
    });
  });

  it("maps and defaults the server policy identity binding at the API boundary", async () => {
    const firstAuthority = policyAuthority("identity-primary");
    const secondAuthority = policyAuthority("");
    const { applicationIdentityId: firstIdentityId, ...firstWireAuthority } = firstAuthority;
    const { applicationIdentityId: _missingIdentityId, ...secondWireAuthority } = secondAuthority;
    const wireWorkspace = {
      ...previewWorkspace,
      tracks: previewWorkspace.tracks.map((track, index) => ({
        ...track,
        policy: {
          ...track.policy,
          authority: index === 0
            ? { ...firstWireAuthority, application_identity_id: firstIdentityId }
            : secondWireAuthority,
        },
      })),
    };
    vi.stubGlobal("fetch", vi.fn(async () => Response.json(wireWorkspace)));

    const workspace = await jobsApi.workspace();

    expect(workspace.tracks[0].policy.authority?.applicationIdentityId)
      .toBe("identity-primary");
    expect(workspace.tracks[0].policy.authority).not.toHaveProperty(
      "application_identity_id",
    );
    expect(workspace.tracks[1].policy.authority?.applicationIdentityId).toBe("");
  });

  it("loads the authenticated workspace without using a browser cache", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const fetchMock = vi.fn(async () => Response.json(previewWorkspace));
    vi.stubGlobal("fetch", fetchMock);

    await jobsApi.workspace();

    const [path, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe("/api/jobs/workspace");
    expect(init.cache).toBe("no-store");
    expect(new Headers(init.headers).get("Authorization")).toBe(
      "Bearer dummy-access-token",
    );
  });

  it("validates the authenticated taxonomy before a Track write and sends its exact binding", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const taxonomy = await currentTaxonomyHttpResponse();
    const track = {
      ...previewWorkspace.tracks[0],
      id: "track-client-created",
      created_at_ms: 0,
      policy: {
        ...previewWorkspace.tracks[0].policy,
        authority: policyAuthority("identity-primary"),
      },
    };
    const savedTrack = { ...track };
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => (
      String(input) === "/api/jobs/taxonomy"
        ? Response.json(taxonomy)
        : Response.json(savedTrack)
    ));
    vi.stubGlobal("fetch", fetchMock);

    await expect(jobsApi.saveTrack(track)).resolves.toEqual(savedTrack);

    expect(fetchMock).toHaveBeenCalledTimes(2);
    const calls = fetchMock.mock.calls as unknown as Array<[string, RequestInit]>;
    expect(calls.map(([path]) => path)).toEqual([
      "/api/jobs/taxonomy",
      "/api/jobs/tracks",
    ]);
    expect(calls[0][1].method).toBeUndefined();
    expect(calls[1][1].method).toBe("POST");
    expect(new Headers(calls[0][1].headers).get("Authorization")).toBe(
      "Bearer dummy-access-token",
    );
    const writeHeaders = new Headers(calls[1][1].headers);
    expect(writeHeaders.get("Authorization")).toBe("Bearer dummy-access-token");
    expect(writeHeaders.get("X-Bluey-Jobs-Taxonomy-Version")).toBe(
      taxonomy.taxonomyVersion,
    );
    expect(writeHeaders.get("X-Bluey-Jobs-Taxonomy-SHA256")).toBe(
      taxonomy.taxonomySha256,
    );
    const writtenTrack = JSON.parse(String(calls[1][1].body)) as {
      policy: { authority: Record<string, unknown> };
    };
    expect(writtenTrack.policy.authority.application_identity_id).toBe("identity-primary");
    expect(writtenTrack.policy.authority).not.toHaveProperty("applicationIdentityId");
  });

  it("validates the authenticated taxonomy before the onboarding write", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const taxonomy = await currentTaxonomyHttpResponse();
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => (
      String(input) === "/api/jobs/taxonomy"
        ? Response.json(taxonomy)
        : Response.json(previewWorkspace)
    ));
    vi.stubGlobal("fetch", fetchMock);

    await expect(jobsApi.completeOnboarding(
      previewWorkspace.profile,
      previewWorkspace.preferences,
      previewWorkspace.tracks[0],
    )).resolves.toEqual(previewWorkspace);

    expect(fetchMock).toHaveBeenCalledTimes(2);
    const calls = fetchMock.mock.calls as unknown as Array<[string, RequestInit]>;
    expect(calls.map(([path]) => path)).toEqual([
      "/api/jobs/taxonomy",
      "/api/jobs/onboarding/complete",
    ]);
    expect(calls[1][1].method).toBe("POST");
    const writeHeaders = new Headers(calls[1][1].headers);
    expect(writeHeaders.get("X-Bluey-Jobs-Taxonomy-Version")).toBe(
      taxonomy.taxonomyVersion,
    );
    expect(writeHeaders.get("X-Bluey-Jobs-Taxonomy-SHA256")).toBe(
      taxonomy.taxonomySha256,
    );
  });

  it.each([
    {
      label: "Track",
      write: () => jobsApi.saveTrack({
        ...previewWorkspace.tracks[0],
        id: "track-client-created",
        created_at_ms: 0,
      }),
    },
    {
      label: "onboarding",
      write: () => jobsApi.completeOnboarding(
        previewWorkspace.profile,
        previewWorkspace.preferences,
        previewWorkspace.tracks[0],
      ),
    },
  ])("refuses stale or malformed taxonomy before the $label write", async ({ write }) => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const taxonomy = await currentTaxonomyHttpResponse();
    for (const invalidTaxonomy of [
      { ...taxonomy, taxonomySha256: "0".repeat(64) },
      { ...taxonomy, registry: {} },
    ]) {
      const fetchMock = vi.fn(async (_input: RequestInfo | URL) => (
        Response.json(invalidTaxonomy)
      ));
      vi.stubGlobal("fetch", fetchMock);

      await expect(write()).rejects.toMatchObject({ status: 409 });
      expect(fetchMock).toHaveBeenCalledTimes(1);
      expect(String(fetchMock.mock.calls[0][0])).toBe("/api/jobs/taxonomy");
    }
  });

  it("refreshes authentication on the taxonomy read before issuing the bound Track write", async () => {
    localStorage.setItem("bluey_access_token", "dummy-expired-access-token");
    localStorage.setItem("bluey_refresh_token", "dummy-refresh-token-one");
    const taxonomy = await currentTaxonomyHttpResponse();
    const track = {
      ...previewWorkspace.tracks[0],
      id: "track-client-created",
      created_at_ms: 0,
    };
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      if (path === "/auth/refresh") {
        return Response.json({
          access_token: "dummy-fresh-access-token",
          refresh_token: "dummy-refresh-token-two",
        });
      }
      const authorization = new Headers(init?.headers).get("Authorization");
      if (authorization !== "Bearer dummy-fresh-access-token") {
        return Response.json({ message: "expired" }, { status: 401 });
      }
      return path === "/api/jobs/taxonomy"
        ? Response.json(taxonomy)
        : Response.json(track);
    });
    vi.stubGlobal("fetch", fetchMock);

    await jobsApi.saveTrack(track);

    const calls = fetchMock.mock.calls as unknown as Array<[string, RequestInit]>;
    expect(calls.map(([path]) => path)).toEqual([
      "/api/jobs/taxonomy",
      "/auth/refresh",
      "/api/jobs/taxonomy",
      "/api/jobs/tracks",
    ]);
    expect(new Headers(calls[3][1].headers).get("Authorization")).toBe(
      "Bearer dummy-fresh-access-token",
    );
    expect(new Headers(calls[3][1].headers).get("X-Bluey-Jobs-Taxonomy-SHA256")).toBe(
      taxonomy.taxonomySha256,
    );
  });

  it("sends the narrow owner confirmation for an uncertain submission", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const fetchMock = vi.fn(async () => Response.json({
      id: "application-one",
      job_id: "job-one",
      state: "failed",
    }));
    vi.stubGlobal("fetch", fetchMock);

    await jobsApi.reconcileSubmissionNotSubmitted("application-one");

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [path, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe("/api/jobs/applications/application-one/reconcile-submission");
    expect(init.method).toBe("POST");
    expect(JSON.parse(String(init.body))).toEqual({ outcome: "not_submitted", confirmed: true });
    expect(new Headers(init.headers).get("Authorization")).toBe("Bearer dummy-access-token");
  });

  it("downloads account-scoped application evidence with owner authentication", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const fetchMock = vi.fn(async () => new Response("immutable evidence", {
      headers: {
        "Content-Disposition": "attachment; filename=\"application-receipt.json\"",
        "Content-Type": "application/json",
      },
    }));
    vi.stubGlobal("fetch", fetchMock);

    const downloaded = await jobsApi.downloadApplicationEvidence(
      "application/one",
      "evidence two",
      "fallback.json",
    );

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [path, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe(
      "/api/jobs/applications/application%2Fone/evidence/evidence%20two/download",
    );
    expect(new Headers(init.headers).get("Authorization")).toBe("Bearer dummy-access-token");
    expect(downloaded.fileName).toBe("application-receipt.json");
    expect(await downloaded.blob.text()).toBe("immutable evidence");
  });

  it("binds a stable client request id to a resume source upload", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const fetchMock = vi.fn(async () => Response.json({
      asset: { id: "00000000-0000-4000-8000-000000000001" },
      profile: { full_name: "Ada Lovelace" },
    }));
    vi.stubGlobal("fetch", fetchMock);
    const file = new File(["exact resume"], "resume.txt", { type: "text/plain" });

    await jobsApi.uploadResumeSource(
      file,
      previewWorkspace.profile,
      undefined,
      "00000000-0000-4000-8000-000000000001",
    );

    const [path, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe("/api/jobs/resume-source");
    expect(init.method).toBe("POST");
    expect(JSON.parse(String(init.body))).toMatchObject({
      request_id: "00000000-0000-4000-8000-000000000001",
      file_name: "resume.txt",
      media_type: "text/plain",
    });
  });

  it("lists privacy-safe communication summaries for the exact application", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const {
      payload: _payload,
      connection_account_label: _connectionAccountLabel,
      source_context: _sourceContext,
      ...summary
    } = communicationActionResponse();
    expect(_payload).toBeDefined();
    expect(_connectionAccountLabel).toBeDefined();
    expect(_sourceContext).toBeDefined();
    const fetchMock = vi.fn(async () => Response.json([summary]));
    vi.stubGlobal("fetch", fetchMock);

    const actions = await jobsApi.communicationActions("application/one", 500);

    expect(actions).toEqual([summary]);
    const [path, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe(
      "/api/jobs/communication-actions?limit=100&application_id=application%2Fone",
    );
    expect(new Headers(init.headers).get("Authorization")).toBe("Bearer dummy-access-token");
    expect(init.cache).toBe("no-store");
  });

  it("loads, approves, and cancels the exact encoded communication action", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const path = String(input);
      if (path.endsWith("/approve")) return Response.json(communicationActionResponse("approved"));
      if (path.endsWith("/cancel")) return Response.json(communicationActionResponse("cancelled"));
      return Response.json(communicationActionResponse());
    });
    vi.stubGlobal("fetch", fetchMock);

    const action = { ...communicationActionResponse(), id: "action/one" };
    await jobsApi.communicationAction("action/one");
    await jobsApi.approveCommunicationAction(action);
    await jobsApi.cancelCommunicationAction(action);

    expect(fetchMock).toHaveBeenCalledTimes(3);
    const calls = fetchMock.mock.calls as unknown as Array<[string, RequestInit]>;
    expect(calls.map(([path]) => path)).toEqual([
      "/api/jobs/communication-actions/action%2Fone",
      "/api/jobs/communication-actions/action%2Fone/approve",
      "/api/jobs/communication-actions/action%2Fone/cancel",
    ]);
    expect(calls[0][1].method).toBeUndefined();
    expect(calls[0][1].cache).toBe("no-store");
    expect(calls[1][1].method).toBe("POST");
    expect(calls[1][1].cache).toBe("no-store");
    expect(calls[2][1].method).toBe("POST");
    expect(calls[2][1].cache).toBe("no-store");
    expect(JSON.parse(String(calls[1][1].body))).toEqual({
      action_revision: action.action_revision,
      payload_sha256: action.payload_sha256,
    });
    expect(JSON.parse(String(calls[2][1].body))).toEqual({
      action_revision: action.action_revision,
      payload_sha256: action.payload_sha256,
    });
    expect(calls.every(([, init]) => (
      new Headers(init.headers).get("Authorization") === "Bearer dummy-access-token"
    ))).toBe(true);
  });

  it("starts explicit communication authorization for the exact connected account", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const fetchMock = vi.fn(async () => Response.json({
      authorization_url: "https://accounts.example.test/communication-consent",
    }));
    vi.stubGlobal("fetch", fetchMock);

    const result = await jobsApi.startMailboxCommunicationAuthorization("connection/one");

    expect(result.authorization_url).toContain("communication-consent");
    const [path, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe(
      "/api/jobs/mailbox-connections/connection%2Fone/communication-authorization/start",
    );
    expect(init.method).toBe("POST");
    expect(new Headers(init.headers).get("Authorization")).toBe("Bearer dummy-access-token");
  });

  it("runtime-decodes mailbox provider availability and OAuth start envelopes", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const providers = [
      {
        provider: "gmail",
        configured: true,
        capabilities: [
          "status_sync",
          "application_correlation",
          "review_interventions",
        ],
      },
      {
        provider: "outlook",
        configured: true,
        capabilities: [
          "status_sync",
          "application_correlation",
          "review_interventions",
          "recruiter_reply",
          "interview_calendar",
        ],
      },
    ];
    vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => (
      String(input).endsWith("/config")
        ? Response.json(providers)
        : Response.json({ authorization_url: "https://accounts.google.com/o/oauth2/v2/auth" })
    )));

    await expect(jobsApi.mailboxOAuthProviders()).resolves.toEqual(providers);
    await expect(jobsApi.startMailboxOAuth("gmail")).resolves.toEqual({
      authorization_url: "https://accounts.google.com/o/oauth2/v2/auth",
    });
  });

  it("fails closed on malformed mailbox OAuth response envelopes", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    vi.stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => (
      String(input).endsWith("/config")
        ? Response.json([{ provider: "gmail", configured: true, capabilities: [] }])
        : Response.json({
            authorization_url: "https://accounts.google.com/o/oauth2/v2/auth",
            access_token: "secret",
          })
    )));

    await expect(jobsApi.mailboxOAuthProviders()).rejects.toThrow("could not verify");
    await expect(jobsApi.startMailboxOAuth("gmail")).rejects.toThrow("could not verify");
  });

  it("fails closed when a communication response omits execution readiness", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const malformed = communicationActionResponse();
    delete (malformed as Partial<typeof malformed>).execution_available;
    vi.stubGlobal("fetch", vi.fn(async () => Response.json(malformed)));

    await expect(jobsApi.communicationAction("action-one")).rejects.toThrow(
      "could not verify this reviewed communication action",
    );
  });

  it("sends an authenticated opaque match cursor only in a no-store POST body", async () => {
    localStorage.setItem("bluey_access_token", "dummy-access-token");
    const fetchMock = vi.fn(async (
      _input: RequestInfo | URL,
      _init?: RequestInit,
    ) => Response.json({
      matches: [previewWorkspace.matches[0]],
      page: {
        schema_version: 1,
        page_size: 50,
        has_more: false,
        next_cursor: null,
      },
    }));
    vi.stubGlobal("fetch", fetchMock);

    const page = await jobsApi.matchesPage("opaque+/cursor=", 50);

    expect(page.matches).toHaveLength(1);
    expect(page.page.has_more).toBe(false);
    expect(String(fetchMock.mock.calls[0]?.[0])).toBe("/api/jobs/matches/page");
    const init = fetchMock.mock.calls[0]?.[1];
    expect(init?.method).toBe("POST");
    expect(init?.cache).toBe("no-store");
    expect(JSON.parse(String(init?.body))).toEqual({
      cursor: "opaque+/cursor=",
      limit: 50,
    });
    expect(localStorage.getItem("bluey_jobs_match_cursor")).toBeNull();
  });

  it("accepts an older bounded workspace without pagination metadata", async () => {
    const legacyWorkspace = { ...previewWorkspace } as Partial<JobsWorkspace>;
    delete legacyWorkspace.matches_page;
    vi.stubGlobal("fetch", vi.fn(async () => Response.json(legacyWorkspace)));

    const workspace = await jobsApi.workspace();

    expect(workspace.matches_page).toEqual({
      schema_version: 1,
      page_size: previewWorkspace.matches.length,
      has_more: false,
      next_cursor: null,
    });
  });

  it("fails closed on inconsistent match-page continuation metadata", () => {
    expect(matchesPageFromWire({
      matches: [],
      page: {
        schema_version: 1,
        page_size: 50,
        has_more: true,
        next_cursor: "opaque-next-cursor",
      },
    }).page.next_cursor).toBe("opaque-next-cursor");
    expect(() => matchesPageFromWire({
      matches: [],
      page: {
        schema_version: 1,
        page_size: 50,
        has_more: true,
        next_cursor: null,
      },
    })).toThrow("invalid match page");
    expect(() => matchesPageFromWire({
      matches: [],
      page: {
        schema_version: 1,
        page_size: 101,
        has_more: false,
        next_cursor: null,
      },
    })).toThrow("invalid match page");
    expect(() => matchesPageFromWire({
      matches: [previewWorkspace.matches[0], previewWorkspace.matches[0]],
      page: {
        schema_version: 1,
        page_size: 1,
        has_more: false,
        next_cursor: null,
      },
    })).toThrow("invalid match page");
  });
});
