import { describe, expect, it } from "vitest";
import {
  createJobsWorkerAuthHeaders,
  jobsWorkerScope,
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
    expect(jobsWorkerScope("GET", "/api/jobs/internal/discovery/lease")).toBeUndefined();
    expect(jobsWorkerScope("POST", "/api/jobs/internal/unknown")).toBeUndefined();
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
  });
});
