import type { JobsBlueyHandoffIssueResponse } from "../types";

const HANDOFF_AUDIENCE = "bluey-desktop-interview-prep-v1";
const NONCE_PATTERN = /^[A-Za-z0-9_-]{43}$/;

/**
 * Fail closed before invoking a custom protocol handler. The server response is
 * authenticated, but this prevents future API/proxy drift from turning the
 * portal action into an arbitrary external-URL launcher.
 */
export function validatedBlueyHandoffUrl(response: JobsBlueyHandoffIssueResponse): string {
  if (response.schema_version !== 1 || response.audience !== HANDOFF_AUDIENCE) {
    throw new Error("Bluey returned an unsupported desktop handoff.");
  }
  if (
    !NONCE_PATTERN.test(response.nonce)
    || !Number.isSafeInteger(response.expires_at_ms)
    || response.expires_at_ms <= 0
    || !Number.isSafeInteger(response.expires_in_seconds)
    || response.expires_in_seconds < 1
    || response.expires_in_seconds > 90
  ) {
    throw new Error("Bluey returned an invalid desktop handoff.");
  }

  let url: URL;
  try {
    url = new URL(response.deep_link_url);
  } catch {
    throw new Error("Bluey returned an invalid desktop handoff.");
  }
  const keys = [...url.searchParams.keys()];
  if (
    url.protocol !== "bluey:"
    || url.hostname !== "jobs"
    || url.pathname !== "/interview-prep"
    || url.username
    || url.password
    || url.port
    || url.hash
    || keys.length !== 1
    || keys[0] !== "nonce"
    || url.searchParams.getAll("nonce").length !== 1
    || url.searchParams.get("nonce") !== response.nonce
  ) {
    throw new Error("Bluey returned an invalid desktop handoff.");
  }
  return response.deep_link_url;
}
