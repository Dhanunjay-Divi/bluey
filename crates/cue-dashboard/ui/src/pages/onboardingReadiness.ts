export const AUDIO_READINESS_SCHEMA_VERSION = 1;

export const AUDIO_READINESS_SOURCES = ["microphone", "system"] as const;
export type AudioReadinessSource = (typeof AUDIO_READINESS_SOURCES)[number];

export const AUDIO_READINESS_STATES = [
  "ready",
  "permission_denied",
  "unavailable",
  "silent",
] as const;
export type AudioReadinessState = (typeof AUDIO_READINESS_STATES)[number];

export interface AudioReadinessSourceResult {
  source: AudioReadinessSource;
  state: AudioReadinessState;
  captured_bytes: number;
  sample_count: number;
  nonzero_samples: number;
  rms: number;
  peak: number;
}

export interface AudioReadinessProbeResult {
  schema_version: number;
  checked_at_ms: number;
  retained_audio: false;
  transcribed: false;
  uploaded: false;
  sources: AudioReadinessSourceResult[];
}

export type AudioReadinessFlow =
  | { status: "idle"; result: null; error: null }
  | { status: "running"; result: AudioReadinessProbeResult | null; error: null }
  | { status: "complete"; result: AudioReadinessProbeResult; error: null }
  | { status: "error"; result: AudioReadinessProbeResult | null; error: string };

export type AudioReadinessAction =
  | { type: "start" }
  | { type: "succeed"; result: AudioReadinessProbeResult }
  | { type: "fail"; error: unknown }
  | { type: "reset" };

export const INITIAL_AUDIO_READINESS_FLOW: AudioReadinessFlow = {
  status: "idle",
  result: null,
  error: null,
};

export function reduceAudioReadiness(
  state: AudioReadinessFlow,
  action: AudioReadinessAction,
): AudioReadinessFlow {
  switch (action.type) {
    case "start":
      return { status: "running", result: state.result, error: null };
    case "succeed": {
      try {
        return {
          status: "complete",
          result: normalizeProbeResult(action.result),
          error: null,
        };
      } catch (error) {
        return {
          status: "error",
          result: state.result,
          error: publicProbeError(error),
        };
      }
    }
    case "fail":
      return {
        status: "error",
        result: state.result,
        error: publicProbeError(action.error),
      };
    case "reset":
      return INITIAL_AUDIO_READINESS_FLOW;
  }
}

export function normalizeProbeResult(
  result: AudioReadinessProbeResult,
): AudioReadinessProbeResult {
  if (!result || result.schema_version !== AUDIO_READINESS_SCHEMA_VERSION) {
    throw new Error("Bluey returned an unsupported audio-check result.");
  }
  if (result.retained_audio !== false || result.transcribed !== false || result.uploaded !== false) {
    throw new Error("Bluey returned an unsafe audio-check result.");
  }
  if (!Array.isArray(result.sources)) {
    throw new Error("Bluey returned an unreadable audio-check result.");
  }
  if (result.sources.length !== AUDIO_READINESS_SOURCES.length) {
    throw new Error("Bluey returned an incomplete audio-check result.");
  }

  const bySource = new Map<AudioReadinessSource, AudioReadinessSourceResult>();
  for (const candidate of result.sources) {
    if (!candidate || !AUDIO_READINESS_SOURCES.includes(candidate.source)) {
      throw new Error("Bluey returned an unknown audio source.");
    }
    if (!AUDIO_READINESS_STATES.includes(candidate.state)) {
      throw new Error("Bluey returned an unknown audio state.");
    }
    if (bySource.has(candidate.source)) {
      throw new Error("Bluey returned a duplicate audio source.");
    }
    bySource.set(candidate.source, {
      source: candidate.source,
      state: candidate.state,
      captured_bytes: boundedMetric(candidate.captured_bytes),
      sample_count: boundedMetric(candidate.sample_count),
      nonzero_samples: boundedMetric(candidate.nonzero_samples),
      rms: boundedLevel(candidate.rms),
      peak: boundedLevel(candidate.peak),
    });
  }

  return {
    schema_version: AUDIO_READINESS_SCHEMA_VERSION,
    checked_at_ms: boundedMetric(result.checked_at_ms),
    retained_audio: false,
    transcribed: false,
    uploaded: false,
    sources: AUDIO_READINESS_SOURCES.map((source) => {
      const value = bySource.get(source);
      if (!value) throw new Error("Bluey returned an incomplete audio-check result.");
      return value;
    }),
  };
}

export function sourceResult(
  result: AudioReadinessProbeResult | null,
  source: AudioReadinessSource,
): AudioReadinessSourceResult | null {
  return result?.sources.find((candidate) => candidate.source === source) ?? null;
}

export function readinessCanContinue(result: AudioReadinessProbeResult | null): boolean {
  return result !== null
    && result.sources.some((source) => source.state === "ready" || source.state === "silent");
}

export function readinessIsFullyReady(result: AudioReadinessProbeResult | null): boolean {
  return result !== null
    && AUDIO_READINESS_SOURCES.every(
      (source) => {
        const state = sourceResult(result, source)?.state;
        return state === "ready" || state === "silent";
      },
    );
}

export function sourceStatusCopy(state: AudioReadinessState): string {
  switch (state) {
    case "ready":
      return "Ready";
    case "permission_denied":
      return "Permission needed";
    case "unavailable":
      return "Not available";
    case "silent":
      return "Connected, but no signal detected";
  }
}

function boundedMetric(value: number): number {
  if (!Number.isFinite(value) || value <= 0) return 0;
  return Math.min(Number.MAX_SAFE_INTEGER, Math.floor(value));
}

function boundedLevel(value: number): number {
  if (!Number.isFinite(value) || value <= 0) return 0;
  return Math.min(1, value);
}

function publicProbeError(error: unknown): string {
  const message = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  if (/already running|audio is active|session is ending/i.test(message)) {
    return "Finish the active listening session, then run the audio check again.";
  }
  return "The audio check could not finish. No test audio was kept or uploaded.";
}
