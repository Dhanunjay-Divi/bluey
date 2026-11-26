// Test-only browser execution inputs. The caller still uses the production
// executor, provider registry, document materializer, lease, proof and receipt paths.
// Any FilesystemNativeRootForTest used by an integration test is explicitly a
// non-physical-native custody facade; it is not evidence of native storage.
import { createHash } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { PDFDocument } from "pdf-lib";
import {
  approvedExecutionChecksum,
  finalSubmitSurfaceSha256,
  type ApplicationPacket,
  type NormalizedJob,
  type ProviderFinalSubmitProof,
} from "@bluey/jobs-automation";
import { AccountResidencyIndex } from "../src/account-residency.js";
import { openRunnerDataRoot } from "../src/safe-runner-storage.js";
import { SubjectStorageManager, type ManagedProfileStorage } from "../src/subject-storage-manager.js";
import { loadOrCreateRunnerVolumeIdentity } from "../src/volume-identity.js";
import { FilesystemNativeRootForTest } from "./filesystem-native-storage.fixture.js";

export const GREENHOUSE_JOB_ID = "1234567";
export const GREENHOUSE_JOB_URL =
  `https://boards.greenhouse.io/acme/jobs/${GREENHOUSE_JOB_ID}`;

export interface BrowserRunFixture {
  readonly root: string;
  readonly resumeBytes: Uint8Array;
  readonly resumeSha256: string;
  readonly resumePath: string;
  readonly packet: ApplicationPacket;
  readonly job: NormalizedJob;
  dispose(): Promise<void>;
}

export interface BrowserRunStorageFixture {
  readonly storage: ManagedProfileStorage;
  readonly root: string;
  dispose(): Promise<void>;
}

/**
 * Real temporary managed-layout files through the explicit test-only filesystem
 * facade. This is deliberately not physical native retained-handle custody proof.
 */
export async function createBrowserRunStorageFixture(
  purgeSubject: string,
  profileScope: string,
): Promise<BrowserRunStorageFixture> {
  const parent = await mkdtemp(join(tmpdir(), "bluey-browser-run-storage-"));
  try {
    const root = await openRunnerDataRoot(join(parent, "runner"));
    const identity = await loadOrCreateRunnerVolumeIdentity(root);
    const residency = new AccountResidencyIndex(root, identity);
    const manager = new SubjectStorageManager(
      new FilesystemNativeRootForTest(root.path),
      identity,
      residency,
    );
    const locator = await residency.registerProfile(
      purgeSubject,
      profileScope,
    );
    return {
      storage: await manager.ensureProfile(locator),
      root: parent,
      dispose: () => rm(parent, { recursive: true, force: true }),
    };
  } catch (error) {
    await rm(parent, { recursive: true, force: true });
    throw error;
  }
}

export async function createBrowserRunFixture(): Promise<BrowserRunFixture> {
  const root = await mkdtemp(join(tmpdir(), "bluey-browser-run-execution-"));
  try {
    const pdf = await PDFDocument.create();
    const page = pdf.addPage([612, 792]);
    page.drawText("Synthetic applicant resume", { x: 72, y: 720, size: 14 });
    const resumeBytes = await pdf.save();
    const resumeSha256 = createHash("sha256").update(resumeBytes).digest("hex");
    const resumePath = join(root, `resume-${resumeSha256}.pdf`);
    await writeFile(resumePath, resumeBytes, { mode: 0o400 });
    const job: NormalizedJob = {
      externalId: GREENHOUSE_JOB_ID,
      canonicalUrl: GREENHOUSE_JOB_URL,
      company: "Acme",
      title: "Synthetic Engineer",
      location: "New York, NY",
      workplace: "hybrid",
      description: "Synthetic fixture role.",
      source: "greenhouse",
      employmentType: "full_time",
    };
    const packet: ApplicationPacket = {
      applicationId: "application-browser-execution-fixture",
      jobId: "job-browser-execution-fixture",
      resumeVersionId: "resume-browser-execution-fixture",
      resumePath,
      answers: { email: "synthetic@example.test" },
      verifiedClaimIds: [],
      applicationIdentityId: "identity-browser-execution-fixture",
      applicationEmail: "synthetic@example.test",
      browserProfileId: "profile-browser-execution-fixture",
      approvedExecutionSchemaVersion: 3,
      approvedExecutionAdmission: {
        kind: "track_auto_submit",
        authorization_id: "authorization-browser-execution-fixture",
        career_track_id: "track-browser-execution-fixture",
        revision_no: 1,
        authority_fingerprint: "a".repeat(64),
        ats_certification: {
          schema_version: 1,
          provider: "greenhouse",
          adapter_version: "2026.07.1-beta.1",
          variant_key: "public",
          layout_contract_version: 1,
          surface_sha256: greenhouseSurfaceSha256(resumeSha256, resumeBytes.byteLength),
          manifest_sha256: "c".repeat(64),
          activation_sha256: "d".repeat(64),
          activation_generation: 1,
          target_key_sha256: "e".repeat(64),
          layout_set_sha256: "f".repeat(64),
          adapter_bundle_sha256: "1".repeat(64),
          runner_target_sha256s: ["2".repeat(64)],
          expires_at_ms: 1_900_000_000_000,
        },
      },
      approvedPacketChecksum: "",
    };
    packet.approvedPacketChecksum = approvedExecutionChecksum(packet, job);
    return {
      root,
      resumeBytes,
      resumeSha256,
      resumePath,
      packet,
      job,
      dispose: () => rm(root, { recursive: true, force: true }),
    };
  } catch (error) {
    await rm(root, { recursive: true, force: true });
    throw error;
  }
}

export function greenhouseApplicationHtml(): string {
  return `<!doctype html><html><body>
    <form id="application_form" action="${GREENHOUSE_JOB_URL}" method="post"
      enctype="multipart/form-data" data-greenhouse-job-id="${GREENHOUSE_JOB_ID}">
      <input name="job_id" type="hidden" value="${GREENHOUSE_JOB_ID}">
      <label for="email">Email</label><input id="email" name="email" type="email" required>
      <label for="resume">Resume</label><input id="resume" name="resume" type="file" required>
      <button id="submit" type="submit">Submit application</button>
    </form>
  </body></html>`;
}

export const greenhouseConfirmationHtml =
  // Synthetic same-document confirmation, not a real HTTP redirect fixture.
  `<!doctype html><script>
  history.replaceState(null, "", ${JSON.stringify(`${GREENHOUSE_JOB_URL}/confirmation`)});
  </script>
  <main id="confirmation">Application received</main>`;

function greenhouseSurfaceSha256(resumeSha256: string, resumeLength: number): string {
  const proof: ProviderFinalSubmitProof = {
    adapter: "greenhouse",
    adapterVersion: "2026.07.1-beta.1",
    control: "greenhouse_submit_application",
    target: {
      actionUrl: GREENHOUSE_JOB_URL,
      method: "post",
      enctype: "multipart/form-data",
      formTarget: "_self",
      providerJobKey: `greenhouse:acme:${GREENHOUSE_JOB_ID}`,
      formIdentity: JSON.stringify([0, "application_form", "", "", "", GREENHOUSE_JOB_ID, ""]),
    },
    fields: [
      { fieldName: "job_id", valueByteLength: 0, valueSha256: "0".repeat(64) },
      { fieldName: "email", valueByteLength: 0, valueSha256: "0".repeat(64) },
    ],
    files: [{
      fieldName: "resume", name: `resume-${resumeSha256}.pdf`,
      byteLength: resumeLength, sha256: resumeSha256,
    }],
    partOrder: [
      { kind: "field", index: 0 },
      { kind: "field", index: 1 },
      { kind: "file", index: 0 },
    ],
  };
  return finalSubmitSurfaceSha256(proof);
}
