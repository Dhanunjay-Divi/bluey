import { describe, expect, it } from "vitest";
import {
  createJobsWorkerAuthHeaders,
  jobsWorkerScope,
  normalizeManagedCloudWorkerOrigin,
} from "../src/worker-auth.js";

const KEY = "0123456789abcdef0123456789abcdef";

describe("Bluey Jobs worker authentication", () => {
  it("matches the server bluey-jobs-worker-v1 signing vector", () => {
    const body = JSON.stringify({ account_id: "account-123", state: "running" });
    const headers = createJobsWorkerAuthHeaders({
      signingKey: KEY,
      workerId: "workflow-test",
      method: "POST",
      path: "/api/jobs/internal/applications/app-123/state",
      body,
      timestamp: 1_750_000_000,
      nonce: "abcdef0123456789abcdef0123456789",
    });

    expect(headers).toEqual({
      "x-bluey-jobs-worker-id": "workflow-test",
      "x-bluey-jobs-worker-timestamp": "1750000000",
      "x-bluey-jobs-worker-nonce": "abcdef0123456789abcdef0123456789",
      "x-bluey-jobs-worker-audience": "bluey-jobs-api",
      "x-bluey-jobs-worker-scope": "application-state",
      "x-bluey-jobs-worker-content-sha256": "d2bf9fe5a8a5253a3c0f969fdac700d8936d5b728770133ee502efea230979d6",
      "x-bluey-jobs-worker-signature": "60d14a1656e9b560ad3bdb871d92be635b68060079b566c1a8ec461a41187bfa",
    });
  });

  it("mirrors the server scopes for every internal worker operation", () => {
    expect(jobsWorkerScope("POST", "/api/jobs/internal/execution-leases/claim")).toBe("execution");
    expect(jobsWorkerScope("POST", "/api/jobs/internal/runner-volumes/enroll")).toBe("runner-volume");
    expect(jobsWorkerScope("POST", "/api/jobs/internal/runner-volumes/rv_test/poll")).toBe("runner-volume");
    expect(jobsWorkerScope("POST", "/api/jobs/internal/runner-volumes/rv_test/ack")).toBe("runner-volume");
    expect(jobsWorkerScope("POST", "/api/jobs/internal/discovery/lease")).toBe("discovery");
    expect(jobsWorkerScope("POST", "/api/jobs/internal/global-discovery/lease")).toBe("discovery");
    expect(jobsWorkerScope("POST", "/api/jobs/internal/global-discovery/source/batches")).toBe("discovery");
    expect(jobsWorkerScope("POST", "/api/jobs/internal/applications/app/receipt")).toBe("receipt");
    expect(jobsWorkerScope("POST", "/api/jobs/internal/applications/app/interventions")).toBe("intervention");
    expect(jobsWorkerScope("POST", "/api/jobs/internal/applications/app/state")).toBe("application-state");
    expect(jobsWorkerScope("POST", "/api/jobs/internal/runs/run/events")).toBe("run-events");
    expect(jobsWorkerScope(
      "POST",
      "/api/jobs/internal/workflow-commands/wfreq-v2-123456789012/materialize",
    )).toBe("workflow-command-materialize");
    expect(jobsWorkerScope(
      "POST",
      "/api/jobs/internal/workflow-commands/wfreq-v2-123456789012/intervention/prepare",
    )).toBe("workflow-command-execution");
    expect(jobsWorkerScope(
      "POST",
      "/api/jobs/internal/workflow-commands/wfreq-v2-123456789012/intervention/wfint-v2-123456789012/publish",
    )).toBe("workflow-command-execution");
    expect(jobsWorkerScope(
      "POST",
      "/api/jobs/internal/workflow-commands/wfreq-v2-123456789012/finalize",
    )).toBe("workflow-command-execution");
    expect(jobsWorkerScope(
      "POST",
      "/api/jobs/internal/workflow-commands/short/materialize",
    )).toBeUndefined();
    expect(jobsWorkerScope(
      "POST",
      "/api/jobs/internal/workflow-commands/wfreq-v2-1234567890/intervention/short/publish",
    )).toBeUndefined();
    expect(jobsWorkerScope(
      "POST",
      "/api/jobs/internal/workflow-commands/wfreq:v2:1234567890/materialize",
    )).toBeUndefined();
    expect(jobsWorkerScope(
      "POST",
      "/api/jobs/internal/managed-cloud/runtime-grants/cloud-grant-1234567890/claim",
    )).toBe("managed-cloud-runtime");
    expect(jobsWorkerScope(
      "POST",
      "/api/jobs/internal/managed-cloud/runtime-instances/cloud-instance-1234567890/heartbeats",
    )).toBe("managed-cloud-runtime");
    expect(jobsWorkerScope(
      "POST",
      "/api/jobs/internal/managed-cloud/runtime-grants/short/claim",
    )).toBeUndefined();
    expect(jobsWorkerScope("GET", "/api/jobs/internal/discovery/lease")).toBeUndefined();
    expect(jobsWorkerScope("POST", "/api/jobs/internal/unknown")).toBeUndefined();
  });

  it("binds managed-cloud grants to the exact normalized API origin", () => {
    const headers = createJobsWorkerAuthHeaders({
      signingKey: KEY,
      workerId: "workflow-gateway-runtime",
      method: "POST",
      path: "/api/jobs/internal/managed-cloud/runtime-grants/cloud-grant-1234567890/claim",
      body: "{}",
      origin: "https://JOBS-API.internal:443/",
      timestamp: 1_750_000_000,
      nonce: "abcdef0123456789abcdef0123456789",
    });

    expect(headers["x-bluey-jobs-worker-origin"]).toBe("https://jobs-api.internal");
    expect(headers["x-bluey-jobs-worker-signature"])
      .toBe("d2e4fb9caa257ad5c3af5aebc3f590c8d8e9a897abb7f40ae590c02d8019150f");
  });

  it("rejects credentials and paths the server cannot authenticate", () => {
    expect(() => createJobsWorkerAuthHeaders({
      signingKey: "short",
      workerId: "worker-test",
      method: "POST",
      path: "/api/jobs/internal/discovery/lease",
    })).toThrow("BLUEY_JOBS_WORKER_SIGNING_KEY");
    expect(() => createJobsWorkerAuthHeaders({
      signingKey: KEY,
      workerId: "worker-test",
      method: "POST",
      path: "/api/jobs/internal/discovery/lease?limit=1",
    })).toThrow("not signable");
    expect(() => createJobsWorkerAuthHeaders({
      signingKey: KEY,
      workerId: "workflow-gateway-runtime",
      method: "POST",
      path: "/api/jobs/internal/managed-cloud/runtime-grants/cloud-grant-1234567890/claim",
      body: "{}",
    })).toThrow("origin is required");
    expect(() => createJobsWorkerAuthHeaders({
      signingKey: KEY,
      workerId: "worker-test",
      method: "POST",
      path: "/api/jobs/internal/discovery/lease",
      origin: "https://jobs-api.internal",
    })).toThrow("only valid");
    expect(() => normalizeManagedCloudWorkerOrigin("https://jobs-api.internal/path"))
      .toThrow("origin is invalid");
  });
});
