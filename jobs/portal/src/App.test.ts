import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { ApiError } from "./api";
import { AppShell } from "./components/AppShell";
import { previewWorkspace } from "./data/preview";
import type { CareerProfile, MailboxConnection, UploadResumeSourceResponse } from "./types";

type ResumeUploadRequestId = ReturnType<Crypto["randomUUID"]>;

let ResumeUploadAttemptLineage: typeof import("./App").ResumeUploadAttemptLineage;
let uploadResumeSourceWithLineage: typeof import("./App").uploadResumeSourceWithLineage;
let openMailboxCommunicationAuthorization:
  typeof import("./App").openMailboxCommunicationAuthorization;
let jobsPortalHomeDestination: typeof import("./App").jobsPortalHomeDestination;

beforeAll(async () => {
  vi.stubGlobal("window", {
    location: { search: "", origin: "https://jobs.bluey.example" },
    matchMedia: () => ({ matches: false }),
  });
  vi.stubGlobal("localStorage", {
    getItem: () => null,
    setItem: () => undefined,
    removeItem: () => undefined,
  });
  const app = await import("./App");
  ResumeUploadAttemptLineage = app.ResumeUploadAttemptLineage;
  uploadResumeSourceWithLineage = app.uploadResumeSourceWithLineage;
  openMailboxCommunicationAuthorization = app.openMailboxCommunicationAuthorization;
  jobsPortalHomeDestination = app.jobsPortalHomeDestination;
});

afterAll(() => {
  vi.unstubAllGlobals();
});

describe("Jobs portal home route", () => {
  it("uses Overview as the canonical basename-relative home", () => {
    expect(jobsPortalHomeDestination()).toBe("/overview");
    expect(jobsPortalHomeDestination("?preview=1&scenario=many-matches"))
      .toBe("/overview?preview=1&scenario=many-matches");
  });

  it("exposes Overview in both navigation layouts without growing the mobile tab bar", () => {
    const markup = renderToStaticMarkup(createElement(
      MemoryRouter,
      { initialEntries: ["/overview?preview=1"] },
      createElement(
        AppShell,
        {
          account: { email: "taylor@example.com", balance_cents: 2450 },
          workspace: previewWorkspace,
          onRefresh: () => undefined,
          preview: true,
          previewSearch: "?preview=1",
          children: createElement("p", null, "Command center"),
        },
      ),
    ));
    const desktopNavigation = markup.match(/<nav class="desktop-nav"[\s\S]*?<\/nav>/)?.[0] || "";
    const mobileNavigation = markup.match(/<nav class="mobile-nav"[\s\S]*?<\/nav>/)?.[0] || "";

    expect(desktopNavigation).toContain('href="/overview?preview=1"');
    expect(desktopNavigation).toContain('href="/settings?preview=1"');
    expect(mobileNavigation).toContain('href="/overview?preview=1"');
    expect(mobileNavigation).not.toContain('href="/settings?preview=1"');
    expect(mobileNavigation.match(/ href=/g)).toHaveLength(5);
  });
});

describe("resume source upload request lineage", () => {
  it("reuses one request id after an ambiguous timeout and clears it after success", async () => {
    const requestIds: ResumeUploadRequestId[] = [
      "00000000-0000-4000-8000-000000000001",
      "00000000-0000-4000-8000-000000000002",
    ];
    const lineage = new ResumeUploadAttemptLineage(() => {
      const requestId = requestIds.shift();
      if (!requestId) throw new Error("test request ids exhausted");
      return requestId;
    });
    const file = new File(["exact resume"], "resume.txt", { type: "text/plain" });
    const profile = previewWorkspace.profile;
    const observed: string[] = [];
    let calls = 0;
    const upload = async (
      _file: File,
      savedProfile: CareerProfile,
      _pageCount: number | undefined,
      requestId: ResumeUploadRequestId,
    ): Promise<UploadResumeSourceResponse> => {
      observed.push(requestId);
      calls += 1;
      if (calls === 1) throw new ApiError(0, "Upload timed out after the server may have committed.");
      return responseFor(savedProfile, requestId);
    };

    await expect(
      uploadResumeSourceWithLineage(lineage, file, profile, undefined, upload),
    ).rejects.toMatchObject({ status: 0 });
    await expect(
      uploadResumeSourceWithLineage(lineage, file, profile, undefined, upload),
    ).resolves.toMatchObject({ asset: { id: observed[0] } });
    await uploadResumeSourceWithLineage(lineage, file, profile, undefined, upload);

    expect(observed).toEqual([
      "00000000-0000-4000-8000-000000000001",
      "00000000-0000-4000-8000-000000000001",
      "00000000-0000-4000-8000-000000000002",
    ]);
  });

  it("uses a new request id for a reselected file or changed profile", async () => {
    let nextId = 0;
    const lineage = new ResumeUploadAttemptLineage(
      () =>
        `00000000-0000-4000-8000-${String(++nextId).padStart(12, "0")}` as ResumeUploadRequestId,
    );
    const firstSelection = new File(["same bytes"], "resume.txt", { type: "text/plain" });
    const reselectedFile = new File(["same bytes"], "resume.txt", { type: "text/plain" });
    const profile = previewWorkspace.profile;
    const changedProfile = { ...profile, headline: `${profile.headline} II` };
    const observed: string[] = [];
    const ambiguousUpload = async (
      _file: File,
      _profile: CareerProfile,
      _pageCount: number | undefined,
      requestId: ResumeUploadRequestId,
    ): Promise<UploadResumeSourceResponse> => {
      observed.push(requestId);
      throw new ApiError(503, "The response was unavailable.");
    };

    await expect(
      uploadResumeSourceWithLineage(lineage, firstSelection, profile, undefined, ambiguousUpload),
    ).rejects.toMatchObject({ status: 503 });
    await expect(
      uploadResumeSourceWithLineage(lineage, reselectedFile, profile, undefined, ambiguousUpload),
    ).rejects.toMatchObject({ status: 503 });
    await expect(
      uploadResumeSourceWithLineage(
        lineage,
        reselectedFile,
        changedProfile,
        undefined,
        ambiguousUpload,
      ),
    ).rejects.toMatchObject({ status: 503 });
    await expect(
      uploadResumeSourceWithLineage(
        lineage,
        firstSelection,
        profile,
        undefined,
        ambiguousUpload,
      ),
    ).rejects.toMatchObject({ status: 503 });

    expect(observed).toEqual([
      "00000000-0000-4000-8000-000000000001",
      "00000000-0000-4000-8000-000000000002",
      "00000000-0000-4000-8000-000000000003",
      "00000000-0000-4000-8000-000000000004",
    ]);
  });

  it("clears a request id after a definitive conflict", async () => {
    let nextId = 0;
    const lineage = new ResumeUploadAttemptLineage(
      () =>
        `00000000-0000-4000-8000-${String(++nextId).padStart(12, "0")}` as ResumeUploadRequestId,
    );
    const file = new File(["resume"], "resume.txt", { type: "text/plain" });
    const observed: string[] = [];
    const conflict = async (
      _file: File,
      _profile: CareerProfile,
      _pageCount: number | undefined,
      requestId: ResumeUploadRequestId,
    ): Promise<UploadResumeSourceResponse> => {
      observed.push(requestId);
      throw new ApiError(409, "Choose the file again.");
    };

    await expect(
      uploadResumeSourceWithLineage(lineage, file, previewWorkspace.profile, undefined, conflict),
    ).rejects.toMatchObject({ status: 409 });
    await expect(
      uploadResumeSourceWithLineage(lineage, file, previewWorkspace.profile, undefined, conflict),
    ).rejects.toMatchObject({ status: 409 });

    expect(observed).toEqual([
      "00000000-0000-4000-8000-000000000001",
      "00000000-0000-4000-8000-000000000002",
    ]);
  });
});

describe("mailbox communication authorization", () => {
  const connection: MailboxConnection = {
    id: "connection-one",
    provider: "gmail",
    status: "connected",
    account_label: "candidate@gmail.com",
    aliases: [],
    capabilities: ["status_sync"],
    created_at_ms: 1,
    updated_at_ms: 1,
  };

  it("never starts or redirects from preview mode", async () => {
    const start = vi.fn(async () => ({ authorization_url: "https://accounts.example.test" }));
    const redirect = vi.fn();

    await openMailboxCommunicationAuthorization(
      true,
      connection,
      start,
      redirect,
      "https://jobs.bluey.example",
    );

    expect(start).not.toHaveBeenCalled();
    expect(redirect).not.toHaveBeenCalled();
  });

  it("starts the account-bound flow before redirecting outside preview", async () => {
    const authorizationUrl = googleCommunicationAuthorizationUrl();
    const start = vi.fn(async () => ({
      authorization_url: authorizationUrl,
    }));
    const redirect = vi.fn();

    await openMailboxCommunicationAuthorization(
      false,
      connection,
      start,
      redirect,
      "https://jobs.bluey.example",
    );

    expect(start).toHaveBeenCalledWith("connection-one");
    expect(redirect).toHaveBeenCalledWith(authorizationUrl);
  });

  it("does not redirect when the account-bound authorization URL fails verification", async () => {
    const start = vi.fn(async () => ({
      authorization_url: "https://attacker.example/communication-consent",
    }));
    const redirect = vi.fn();

    await expect(openMailboxCommunicationAuthorization(
      false,
      connection,
      start,
      redirect,
      "https://jobs.bluey.example",
    )).rejects.toThrow("could not verify");

    expect(start).toHaveBeenCalledWith("connection-one");
    expect(redirect).not.toHaveBeenCalled();
  });
});

function googleCommunicationAuthorizationUrl(): string {
  const token = "A".repeat(43);
  const url = new URL("https://accounts.google.com/o/oauth2/v2/auth");
  url.searchParams.set("client_id", "client.apps.googleusercontent.com");
  url.searchParams.set(
    "redirect_uri",
    "https://jobs.bluey.example/api/jobs/oauth/gmail/callback",
  );
  url.searchParams.set("response_type", "code");
  url.searchParams.set("scope", [
    "openid",
    "email",
    "https://www.googleapis.com/auth/gmail.readonly",
    "https://www.googleapis.com/auth/gmail.send",
    "https://www.googleapis.com/auth/calendar.events",
  ].join(" "));
  url.searchParams.set("state", token);
  url.searchParams.set("code_challenge", token);
  url.searchParams.set("code_challenge_method", "S256");
  url.searchParams.set("access_type", "offline");
  url.searchParams.set("include_granted_scopes", "true");
  url.searchParams.set("prompt", "consent");
  return url.href;
}

function responseFor(profile: CareerProfile, requestId: string): UploadResumeSourceResponse {
  return {
    asset: {
      id: requestId,
      file_name: "resume.txt",
      media_type: "text/plain",
      file_type: "txt",
      sha256: "a".repeat(64),
      size_bytes: 12,
      template_status: "text_only",
      created_at_ms: 1,
      updated_at_ms: 1,
    },
    profile,
  };
}
