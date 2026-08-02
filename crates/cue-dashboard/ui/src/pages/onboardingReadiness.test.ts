import { describe, expect, it } from "vitest";
import {
  AUDIO_READINESS_SCHEMA_VERSION,
  INITIAL_AUDIO_READINESS_FLOW,
  normalizeProbeResult,
  readinessCanContinue,
  readinessIsFullyReady,
  reduceAudioReadiness,
  sourceResult,
  type AudioReadinessProbeResult,
  type AudioReadinessSourceResult,
} from "./onboardingReadiness";

function source(
  name: AudioReadinessSourceResult["source"],
  state: AudioReadinessSourceResult["state"] = "ready",
): AudioReadinessSourceResult {
  return {
    source: name,
    state,
    captured_bytes: 96_000,
    sample_count: 48_000,
    nonzero_samples: 40_000,
    rms: 0.2,
    peak: 0.8,
  };
}

function result(sources: AudioReadinessSourceResult[]): AudioReadinessProbeResult {
  return {
    schema_version: AUDIO_READINESS_SCHEMA_VERSION,
    checked_at_ms: 1_725_000_000_000,
    retained_audio: false,
    transcribed: false,
    uploaded: false,
    sources,
  };
}

describe("audio readiness flow", () => {
  it("keeps a previous result visible while a retry runs", () => {
    const previous = result([source("microphone"), source("system", "silent")]);
    const complete = reduceAudioReadiness(INITIAL_AUDIO_READINESS_FLOW, {
      type: "succeed",
      result: previous,
    });
    const retrying = reduceAudioReadiness(complete, { type: "start" });

    expect(retrying.status).toBe("running");
    expect(retrying.result).toEqual(previous);
    expect(retrying.error).toBeNull();
  });

  it("does not expose daemon error details", () => {
    const failed = reduceAudioReadiness(INITIAL_AUDIO_READINESS_FLOW, {
      type: "fail",
      error: "helper failed at /Users/private/path with token=secret",
    });

    expect(failed.status).toBe("error");
    expect(failed.error).toBe(
      "The audio check could not finish. No test audio was kept or uploaded.",
    );
    expect(failed.error).not.toContain("/Users/private");
  });

  it("gives an actionable busy-session error without forwarding internals", () => {
    const failed = reduceAudioReadiness(INITIAL_AUDIO_READINESS_FLOW, {
      type: "fail",
      error: "audio is active: internal generation 41",
    });
    expect(failed.error).toMatch(/Finish the active listening session/);
    expect(failed.error).not.toContain("generation");
  });
});

describe("audio readiness result normalization", () => {
  it("orders exact sources and bounds metrics", () => {
    const normalized = normalizeProbeResult(result([{
      ...source("system"),
      captured_bytes: -10,
      rms: Number.NaN,
      peak: 9,
    }, source("microphone")]));

    expect(normalized.sources.map((item) => item.source)).toEqual(["microphone", "system"]);
    expect(sourceResult(normalized, "system")).toMatchObject({
      captured_bytes: 0,
      rms: 0,
      peak: 1,
    });
  });

  it("rejects missing, duplicate, and unknown sources", () => {
    expect(() => normalizeProbeResult(result([source("microphone")]))).toThrow(/incomplete/);
    expect(() => normalizeProbeResult(result([
      source("microphone"),
      source("microphone"),
    ]))).toThrow(/duplicate/);
    expect(() => normalizeProbeResult(result([
      source("microphone"),
      { ...source("system"), source: "loopback" },
    ] as unknown as AudioReadinessSourceResult[]))).toThrow(/unknown audio source/);
  });

  it("fails closed on unsupported schemas", () => {
    expect(() => normalizeProbeResult({
      ...result([]),
      schema_version: 99,
    })).toThrow(/unsupported audio-check result/);
  });

  it("fails closed rather than masking violated privacy invariants", () => {
    expect(() => normalizeProbeResult({
      ...result([source("microphone")]),
      retained_audio: true,
    } as unknown as AudioReadinessProbeResult)).toThrow(/unsafe audio-check result/);
  });

  it("requires both sources for full readiness but allows explicit degraded continuation", () => {
    const partial = normalizeProbeResult(result([
      source("microphone"),
      source("system", "permission_denied"),
    ]));
    expect(readinessIsFullyReady(partial)).toBe(false);
    expect(readinessCanContinue(partial)).toBe(true);

    const denied = normalizeProbeResult(result([
      source("microphone", "permission_denied"),
      source("system", "unavailable"),
    ]));
    expect(readinessCanContinue(denied)).toBe(false);
  });

  it("treats a successfully opened silent source as configured", () => {
    const configured = normalizeProbeResult(result([
      source("microphone", "ready"),
      source("system", "silent"),
    ]));
    expect(readinessIsFullyReady(configured)).toBe(true);
    expect(readinessCanContinue(configured)).toBe(true);
  });
});
