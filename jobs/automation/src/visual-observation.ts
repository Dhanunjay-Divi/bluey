import type { BrowserPage, FormControlKind } from "./contracts.js";

export type VisualObservationPhase = "discover_controls" | "prepare" | "fill" | "validate";

export interface VisualControlObservation {
  id: string;
  label: string;
  kind: FormControlKind;
  confidence: number;
  bounds: { x: number; y: number; width: number; height: number };
}

export interface VisualObservationRequest {
  phase: VisualObservationPhase;
  screenshot: Uint8Array;
  url: string;
}

export interface VisualObservationProvider {
  readonly name: string;
  observe(request: VisualObservationRequest): Promise<VisualControlObservation[]>;
}

export interface DomBoundVisualObservation {
  observation: VisualControlObservation;
  selector: string;
}

export interface VisualObservationOptions {
  enabled?: boolean;
  minimumConfidence?: number;
}

export interface PrivateVisualParserOptions {
  endpoint: string;
  authorization?: string;
  fetch?: typeof fetch;
  timeoutMs?: number;
  maxScreenshotBytes?: number;
  maxResponseBytes?: number;
  maxObservations?: number;
}

interface PrivateVisualParserResponse {
  schema_version: "bluey.visual-observation.v1";
  observations: unknown[];
}

const VISUAL_OBSERVATION_SCHEMA = "bluey.visual-observation.v1";
const FORM_CONTROL_KINDS = new Set<FormControlKind>([
  "text",
  "email",
  "tel",
  "url",
  "textarea",
  "select",
  "checkbox",
  "radio",
  "file",
  "hidden",
  "other",
]);

/**
 * Bluey's private service boundary for screenshot parsers such as OmniParser.
 * The service returns observations only; it never receives an action plan and
 * this client intentionally exposes no click or submit operation.
 */
export class PrivateVisualParser implements VisualObservationProvider {
  readonly name = "bluey-private-visual-parser";

  private readonly endpoint: URL;
  private readonly authorization?: string;
  private readonly fetcher: typeof fetch;
  private readonly timeoutMs: number;
  private readonly maxScreenshotBytes: number;
  private readonly maxResponseBytes: number;
  private readonly maxObservations: number;

  constructor(options: PrivateVisualParserOptions) {
    this.endpoint = validatePrivateParserEndpoint(options.endpoint);
    this.authorization = options.authorization?.trim() || undefined;
    if (!isLoopback(this.endpoint) && !this.authorization) {
      throw new Error("private visual parser authentication is required for non-loopback endpoints");
    }
    this.fetcher = options.fetch ?? fetch;
    this.timeoutMs = clampInteger(options.timeoutMs ?? 8_000, 250, 30_000);
    this.maxScreenshotBytes = clampInteger(options.maxScreenshotBytes ?? 8 * 1024 * 1024, 1, 16 * 1024 * 1024);
    this.maxResponseBytes = clampInteger(options.maxResponseBytes ?? 512 * 1024, 1, 2 * 1024 * 1024);
    this.maxObservations = clampInteger(options.maxObservations ?? 256, 1, 512);
  }

  async observe(request: VisualObservationRequest): Promise<VisualControlObservation[]> {
    if (request.screenshot.byteLength === 0 || request.screenshot.byteLength > this.maxScreenshotBytes) {
      throw new Error("visual observation screenshot is empty or exceeds the configured limit");
    }

    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), this.timeoutMs);
    try {
      const response = await this.fetcher(this.endpoint, {
        method: "POST",
        redirect: "error",
        signal: controller.signal,
        headers: {
          "content-type": "application/json",
          "accept": "application/json",
          ...(this.authorization ? { authorization: this.authorization } : {}),
        },
        body: JSON.stringify({
          schema_version: VISUAL_OBSERVATION_SCHEMA,
          phase: request.phase,
          page_url: redactPageUrl(request.url),
          screenshot: {
            media_type: "image/png",
            base64: uint8ToBase64(request.screenshot),
          },
        }),
      });
      if (!response.ok) {
        throw new Error(`visual observation service returned HTTP ${response.status}`);
      }
      const declaredLength = Number(response.headers.get("content-length") || "0");
      if (Number.isFinite(declaredLength) && declaredLength > this.maxResponseBytes) {
        throw new Error("visual observation response exceeds the configured limit");
      }
      const text = await response.text();
      if (new TextEncoder().encode(text).byteLength > this.maxResponseBytes) {
        throw new Error("visual observation response exceeds the configured limit");
      }
      return parsePrivateVisualParserResponse(text, this.maxObservations);
    } finally {
      clearTimeout(timeout);
    }
  }
}

/**
 * Optional screenshot grounding for forms whose DOM is incomplete. The result
 * is observation only: each suggestion must be bound back to a real DOM
 * control before normal fill/validation logic may use it. Final Submit is not
 * a supported phase and cannot be delegated to this class.
 */
export class VisualFormObserver {
  private readonly enabled: boolean;
  private readonly minimumConfidence: number;

  constructor(private readonly provider: VisualObservationProvider, options: VisualObservationOptions = {}) {
    this.enabled = options.enabled ?? visualObservationEnabled();
    this.minimumConfidence = clamp(options.minimumConfidence ?? 0.88, 0.5, 1);
  }

  async observe(page: BrowserPage, phase: VisualObservationPhase): Promise<VisualControlObservation[]> {
    if (!this.enabled) return [];
    const screenshot = await page.screenshot({ fullPage: true });
    const observations = await this.provider.observe({ phase, screenshot, url: page.url() });
    return observations.filter((item) => (
      Number.isFinite(item.confidence)
      && item.confidence >= this.minimumConfidence
      && validBounds(item.bounds)
      && item.label.trim().length > 0
    ));
  }

  bindToDom(
    observations: VisualControlObservation[],
    controls: Array<{ selector: string; label: string; kind: FormControlKind }>,
  ): DomBoundVisualObservation[] {
    return observations.flatMap((observation) => {
      const normalizedLabel = normalizeLabel(observation.label);
      const candidates = controls.filter((control) => (
        control.kind === observation.kind
        && normalizeLabel(control.label) === normalizedLabel
        && control.selector.trim().length > 0
      ));
      return candidates.length === 1 ? [{ observation, selector: candidates[0].selector }] : [];
    });
  }
}

export function visualObservationEnabled(env: Record<string, string | undefined> = runtimeEnvironment()): boolean {
  return env.BLUEY_JOBS_VISUAL_OBSERVATION_ENABLED === "1";
}

function runtimeEnvironment(): Record<string, string | undefined> {
  const candidate = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process;
  return candidate?.env ?? {};
}

function validBounds(bounds: VisualControlObservation["bounds"]): boolean {
  return [bounds.x, bounds.y, bounds.width, bounds.height].every(Number.isFinite)
    && bounds.x >= 0
    && bounds.y >= 0
    && bounds.width > 0
    && bounds.height > 0;
}

function normalizeLabel(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(maximum, value));
}

function clampInteger(value: number, minimum: number, maximum: number): number {
  if (!Number.isFinite(value)) return minimum;
  return Math.trunc(clamp(value, minimum, maximum));
}

function validatePrivateParserEndpoint(value: string): URL {
  let endpoint: URL;
  try {
    endpoint = new URL(value);
  } catch {
    throw new Error("private visual parser endpoint must be a valid URL");
  }
  if (endpoint.username || endpoint.password || endpoint.search || endpoint.hash) {
    throw new Error("private visual parser endpoint cannot contain credentials, query parameters, or a fragment");
  }
  if (endpoint.protocol !== "https:" && !(endpoint.protocol === "http:" && isLoopback(endpoint))) {
    throw new Error("private visual parser endpoint must use HTTPS unless it is loopback-only");
  }
  return endpoint;
}

function isLoopback(url: URL): boolean {
  return url.hostname === "localhost" || url.hostname === "127.0.0.1" || url.hostname === "[::1]";
}

function redactPageUrl(value: string): string {
  try {
    const url = new URL(value);
    url.username = "";
    url.password = "";
    url.search = "";
    url.hash = "";
    return url.toString();
  } catch {
    return "about:blank";
  }
}

function uint8ToBase64(value: Uint8Array): string {
  const buffer = (globalThis as { Buffer?: { from(bytes: Uint8Array): { toString(encoding: string): string } } }).Buffer;
  if (!buffer) throw new Error("visual observation requires a Node-compatible base64 encoder");
  return buffer.from(value).toString("base64");
}

function parsePrivateVisualParserResponse(text: string, maxObservations: number): VisualControlObservation[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new Error("visual observation service returned invalid JSON");
  }
  if (!isRecord(parsed)
    || parsed.schema_version !== VISUAL_OBSERVATION_SCHEMA
    || !Array.isArray(parsed.observations)) {
    throw new Error("visual observation service returned an unsupported schema");
  }
  const response = parsed as unknown as PrivateVisualParserResponse;
  if (response.observations.length > maxObservations) {
    throw new Error("visual observation response contains too many controls");
  }
  return response.observations.map(parseVisualControlObservation);
}

function parseVisualControlObservation(value: unknown): VisualControlObservation {
  if (!isRecord(value)
    || typeof value.id !== "string"
    || typeof value.label !== "string"
    || typeof value.kind !== "string"
    || !FORM_CONTROL_KINDS.has(value.kind as FormControlKind)
    || typeof value.confidence !== "number"
    || !isRecord(value.bounds)
    || typeof value.bounds.x !== "number"
    || typeof value.bounds.y !== "number"
    || typeof value.bounds.width !== "number"
    || typeof value.bounds.height !== "number") {
    throw new Error("visual observation service returned a malformed control");
  }
  return {
    id: value.id,
    label: value.label,
    kind: value.kind as FormControlKind,
    confidence: value.confidence,
    bounds: {
      x: value.bounds.x,
      y: value.bounds.y,
      width: value.bounds.width,
      height: value.bounds.height,
    },
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
