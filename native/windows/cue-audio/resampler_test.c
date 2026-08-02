#include "resampler.h"

#include <math.h>
#include <stdint.h>
#include <stdio.h>

#define TEST_PI 3.14159265358979323846

typedef struct SinkStats {
    size_t count;
    double sum_squares;
} SinkStats;

static int collect_sample(int16_t sample, void *context) {
    SinkStats *stats = (SinkStats *)context;
    double normalized = (double)sample / 32768.0;
    stats->count += 1;
    stats->sum_squares += normalized * normalized;
    return 1;
}

static int run_tone(double source_rate, double frequency, SinkStats *stats) {
    BlueyResampler resampler;
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

static double rms(const SinkStats *stats) {
    return stats->count == 0 ? 0.0 : sqrt(stats->sum_squares / (double)stats->count);
}

int main(void) {
    BlueyResampler invalid;
    if (bluey_resampler_init(&invalid, 0.0)) {
        fprintf(stderr, "invalid source rate was accepted\n");
        return 1;
    }

    SinkStats low_48k = {0};
    SinkStats high_48k = {0};
    SinkStats low_44k = {0};
    if (!run_tone(48000.0, 1000.0, &low_48k)
        || !run_tone(48000.0, 12000.0, &high_48k)
        || !run_tone(44100.0, 1000.0, &low_44k)) {
        fprintf(stderr, "resampler rejected a valid stream\n");
        return 1;
    }
    if (low_48k.count < 15900 || low_48k.count > 16000
        || low_44k.count < 15900 || low_44k.count > 16000) {
        fprintf(stderr, "unexpected output counts: %zu, %zu\n", low_48k.count, low_44k.count);
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
    printf(
        "resampler tests passed: 48k=%zu 44.1k=%zu passband=%.6f stopband=%.6f\n",
        low_48k.count,
        low_44k.count,
        rms(&low_48k),
        rms(&high_48k)
    );
    return 0;
}
