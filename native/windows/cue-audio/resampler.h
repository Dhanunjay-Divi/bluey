#ifndef BLUEY_WINDOWS_AUDIO_RESAMPLER_H
#define BLUEY_WINDOWS_AUDIO_RESAMPLER_H

#include <stddef.h>
#include <stdint.h>

#define BLUEY_RESAMPLER_TAPS 64
#define BLUEY_RESAMPLER_PHASES 256
#define BLUEY_RESAMPLER_RING_CAPACITY 256

typedef int (*BlueyResamplerSink)(int16_t sample, void *context);

typedef struct BlueyResampler {
    double source_rate;
    double output_step;
    double cutoff;
    double next_output_time;
    int64_t latest_input_index;
    float ring[BLUEY_RESAMPLER_RING_CAPACITY];
    float coefficients[BLUEY_RESAMPLER_PHASES][BLUEY_RESAMPLER_TAPS];
} BlueyResampler;

int bluey_resampler_init(BlueyResampler *resampler, double source_rate);
int bluey_resampler_push(
    BlueyResampler *resampler,
    float sample,
    BlueyResamplerSink sink,
    void *context
);

#endif
