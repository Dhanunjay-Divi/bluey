#include "resampler.h"

#include <math.h>
#include <stdint.h>
#include <stdio.h>

#define TEST_PI 3.14159265358979323846
#define FNV_OFFSET_BASIS UINT64_C(1469598103934665603)
#define FNV_PRIME UINT64_C(1099511628211)

typedef struct SinkStats {
    size_t count;
    double sum;
    double sum_squares;
    int16_t peak;
    uint64_t hash;
} SinkStats;

typedef struct FailingSink {
    size_t accepted;
    size_t limit;
} FailingSink;

static void reset_stats(SinkStats *stats) {
    stats->count = 0;
    stats->sum = 0.0;
    stats->sum_squares = 0.0;
    stats->peak = 0;
    stats->hash = FNV_OFFSET_BASIS;
}

static int collect_sample(int16_t sample, void *context) {
    SinkStats *stats = (SinkStats *)context;
    double normalized = (double)sample / 32768.0;
    int32_t magnitude = sample < 0 ? -(int32_t)sample : (int32_t)sample;
    int32_t current_peak = stats->peak < 0 ? -(int32_t)stats->peak : (int32_t)stats->peak;
    if (magnitude > current_peak) {
        stats->peak = sample;
    }
    stats->count += 1;
    stats->sum += normalized;
    stats->sum_squares += normalized * normalized;
    stats->hash ^= (uint8_t)((uint16_t)sample & 0xffU);
    stats->hash *= FNV_PRIME;
    stats->hash ^= (uint8_t)(((uint16_t)sample >> 8) & 0xffU);
    stats->hash *= FNV_PRIME;
    return 1;
}

static int fail_after_limit(int16_t sample, void *context) {
    FailingSink *sink = (FailingSink *)context;
    (void)sample;
    if (sink->accepted >= sink->limit) {
        return 0;
    }
    sink->accepted += 1;
    return 1;
}

static int run_tone(double source_rate, double frequency, SinkStats *stats) {
    BlueyResampler resampler;
    reset_stats(stats);
    if (!bluey_resampler_init(&resampler, source_rate)) {
        return 0;
    }
    size_t input_count = (size_t)source_rate;
    for (size_t index = 0; index < input_count; index++) {
        float sample = (float)(0.5 * sin(2.0 * TEST_PI * frequency * (double)index / source_rate));
        if (!bluey_resampler_push(&resampler, sample, collect_sample, stats)) {
            return 0;
        }
    }
    return 1;
}

static int run_constant(double source_rate, float value, SinkStats *stats) {
    BlueyResampler resampler;
    reset_stats(stats);
    if (!bluey_resampler_init(&resampler, source_rate)) {
        return 0;
    }
    size_t input_count = (size_t)(source_rate / 4.0);
    for (size_t index = 0; index < input_count; index++) {
        if (!bluey_resampler_push(&resampler, value, collect_sample, stats)) {
            return 0;
        }
    }
    return 1;
}

static double rms(const SinkStats *stats) {
    return stats->count == 0 ? 0.0 : sqrt(stats->sum_squares / (double)stats->count);
}

static double mean(const SinkStats *stats) {
    return stats->count == 0 ? 0.0 : stats->sum / (double)stats->count;
}

int main(void) {
    BlueyResampler invalid;
    if (bluey_resampler_init(NULL, 48000.0)
        || bluey_resampler_init(&invalid, 0.0)
        || bluey_resampler_init(&invalid, 7999.0)
        || bluey_resampler_init(&invalid, 384001.0)) {
        fprintf(stderr, "invalid source rate was accepted\n");
        return 1;
    }

    SinkStats low_48k;
    SinkStats low_48k_repeat;
    SinkStats high_48k;
    SinkStats low_44k;
    SinkStats dc_48k;
    SinkStats nan_48k;
    if (!run_tone(48000.0, 1000.0, &low_48k)
        || !run_tone(48000.0, 1000.0, &low_48k_repeat)
        || !run_tone(48000.0, 12000.0, &high_48k)
        || !run_tone(44100.0, 1000.0, &low_44k)
        || !run_constant(48000.0, 0.25f, &dc_48k)
        || !run_constant(48000.0, NAN, &nan_48k)) {
        fprintf(stderr, "resampler rejected a valid stream\n");
        return 1;
    }
    if (low_48k.count < 15900 || low_48k.count > 16000
        || low_44k.count < 15900 || low_44k.count > 16000) {
        fprintf(stderr, "unexpected output counts: %zu, %zu\n", low_48k.count, low_44k.count);
        return 1;
    }
    if (low_48k.count != low_48k_repeat.count || low_48k.hash != low_48k_repeat.hash) {
        fprintf(stderr, "identical input did not produce deterministic output\n");
        return 1;
    }
    if (rms(&low_48k) < 0.32 || rms(&low_48k) > 0.38
        || rms(&low_44k) < 0.32 || rms(&low_44k) > 0.38) {
        fprintf(stderr, "passband RMS outside tolerance: %.6f, %.6f\n", rms(&low_48k), rms(&low_44k));
        return 1;
    }
    if (rms(&high_48k) >= rms(&low_48k) * 0.05) {
        fprintf(stderr, "stopband attenuation insufficient: %.6f vs %.6f\n", rms(&high_48k), rms(&low_48k));
        return 1;
    }
    if (mean(&dc_48k) < 0.245 || mean(&dc_48k) > 0.255) {
        fprintf(stderr, "DC level was not preserved: %.6f\n", mean(&dc_48k));
        return 1;
    }
    if (nan_48k.peak != 0 || rms(&nan_48k) != 0.0) {
        fprintf(stderr, "non-finite input was not sanitized\n");
        return 1;
    }

    BlueyResampler stopped;
    FailingSink failing = {0, 10};
    if (!bluey_resampler_init(&stopped, 48000.0)) {
        fprintf(stderr, "sink failure test could not initialize\n");
        return 1;
    }
    int propagated = 0;
    for (int index = 0; index < 1000; index++) {
        if (!bluey_resampler_push(&stopped, 0.0f, fail_after_limit, &failing)) {
            propagated = 1;
            break;
        }
    }
    if (!propagated || failing.accepted != failing.limit) {
        fprintf(stderr, "sink closure was not propagated deterministically\n");
        return 1;
    }

    printf(
        "resampler tests passed: 48k=%zu 44.1k=%zu passband=%.6f stopband=%.6f hash=%llu\n",
        low_48k.count,
        low_44k.count,
        rms(&low_48k),
        rms(&high_48k),
        (unsigned long long)low_48k.hash
    );
    return 0;
}
