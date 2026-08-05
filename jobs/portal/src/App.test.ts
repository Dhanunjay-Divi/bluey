import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { ApiError } from "./api";
import { previewWorkspace } from "./data/preview";
import type { CareerProfile, UploadResumeSourceResponse } from "./types";

type ResumeUploadRequestId = ReturnType<Crypto["randomUUID"]>;

let ResumeUploadAttemptLineage: typeof import("./App").ResumeUploadAttemptLineage;
let uploadResumeSourceWithLineage: typeof import("./App").uploadResumeSourceWithLineage;

beforeAll(async () => {
  vi.stubGlobal("window", { location: { search: "" } });
  const app = await import("./App");
  ResumeUploadAttemptLineage = app.ResumeUploadAttemptLineage;
  uploadResumeSourceWithLineage = app.uploadResumeSourceWithLineage;
});

afterAll(() => {
  vi.unstubAllGlobals();
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
