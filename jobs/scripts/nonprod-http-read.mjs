import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { request as httpRequest } from "node:http";

const MAX_RESPONSE_BYTES = 1024 * 1024;
const OPERATIONS = new Set([
  "account_before", "workspace_before", "resume_before",
  "account_after", "workspace_after", "resume_after", "isolated_workspace",
]);
const COMPONENTS = new Map([
  ["bluey-jobs-api", "jobs_api"], ["bluey-server", "account_api"],
  ["packet-fixture", "packet_fixture"],
]);

export function redactedChildLiveness(children) {
  assert(Array.isArray(children), "http_read_children");
  return children.flatMap(({ name, processHandle }) => {
    const component = COMPONENTS.get(name);
    if (!component) return [];
    const running = processHandle.exitCode === null && processHandle.signalCode === null;
    const exitCode = Number.isInteger(processHandle.exitCode) ? processHandle.exitCode : null;
    const signal = typeof processHandle.signalCode === "string"
      && /^SIG[A-Z0-9]{1,12}$/.test(processHandle.signalCode)
      ? processHandle.signalCode : null;
    return [{ component, running, exit_code: exitCode, signal }];
  });
}

export function getNonprodJson(url, token, diagnosticOperation = null, timeoutMs = 10_000) {
  assert(diagnosticOperation === null || OPERATIONS.has(diagnosticOperation),
    "http_read_operation");
  assert(Number.isSafeInteger(timeoutMs) && timeoutMs > 0 && timeoutMs <= 60_000,
    "http_read_timeout");
  return new Promise((resolveRequest, reject) => {
    let settled = false; let bodyBytes = 0; let timer;
    const chunks = [];
    const finish = (callback, value) => {
      if (!settled) { settled = true; clearTimeout(timer); callback(value); }
    };
    const request = httpRequest(url, { headers: { authorization: `Bearer ${token}` } },
      (response) => {
        response.on("data", (chunk) => {
          if (settled) return;
          bodyBytes += chunk.length;
          if (bodyBytes > MAX_RESPONSE_BYTES) {
            request.destroy(); finish(reject, new Error("workspace_body_limit")); return;
          }
          chunks.push(chunk);
        });
        response.on("end", () => {
          if (settled) return;
          const body = Buffer.concat(chunks, bodyBytes);
          if (response.statusCode !== 200) {
            const error = new Error(diagnosticOperation
              ? "human_http_read_status" : "workspace_status");
            if (diagnosticOperation) error.httpReadDiagnostic = Object.freeze({
              operation: diagnosticOperation,
              status: Number.isInteger(response.statusCode) ? response.statusCode : 0,
              response_bytes: bodyBytes,
              response_sha256: createHash("sha256").update(body).digest("hex"),
            });
            finish(reject, error); return;
          }
          try { finish(resolveRequest, JSON.parse(body.toString("utf8"))); }
          catch { finish(reject, new Error("workspace_json")); }
        });
      });
    timer = setTimeout(() => {
      request.destroy(); finish(reject, new Error("http_timeout"));
    }, timeoutMs);
    request.on("error", (error) => finish(reject, error));
    request.end();
  });
}
