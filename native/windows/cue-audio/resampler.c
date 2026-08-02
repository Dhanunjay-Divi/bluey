#include "resampler.h"

#include <math.h>
#include <string.h>

#define BLUEY_TARGET_SAMPLE_RATE 16000.0
#define BLUEY_RESAMPLER_HALF_TAPS (BLUEY_RESAMPLER_TAPS / 2)
#define BLUEY_PI 3.14159265358979323846

static double normalized_sinc(double value) {
    if (fabs(value) < 1.0e-12) {
        return 1.0;
    }
    double radians = BLUEY_PI * value;
    return sin(radians) / radians;
}

int bluey_resampler_init(BlueyResampler *resampler, double source_rate) {
    if (resampler == NULL || source_rate < 8000.0 || source_rate > 384000.0) {
        return 0;
    }
    memset(resampler, 0, sizeof(*resampler));
    resampler->source_rate = source_rate;
    resampler->output_step = source_rate / BLUEY_TARGET_SAMPLE_RATE;
    double rate_ratio = BLUEY_TARGET_SAMPLE_RATE / source_rate;
    if (rate_ratio > 1.0) {
        rate_ratio = 1.0;
    }
    /* Leave transition-band headroom below the lower Nyquist frequency. */
    resampler->cutoff = 0.5 * rate_ratio * 0.94;
    resampler->next_output_time = (double)(BLUEY_RESAMPLER_HALF_TAPS - 1);
    resampler->latest_input_index = -1;

    /* Precompute every fractional phase: live capture performs no trigonometry. */
    for (int phase_index = 0; phase_index < BLUEY_RESAMPLER_PHASES; phase_index++) {
        double fractional = (double)phase_index / (double)BLUEY_RESAMPLER_PHASES;
        double coefficient_sum = 0.0;
        for (int tap = 0; tap < BLUEY_RESAMPLER_TAPS; tap++) {
            double distance = (double)(tap - BLUEY_RESAMPLER_HALF_TAPS + 1) - fractional;
            double window_phase = (double)tap / (double)(BLUEY_RESAMPLER_TAPS - 1);
            double window = 0.42
                - 0.5 * cos(2.0 * BLUEY_PI * window_phase)
                + 0.08 * cos(4.0 * BLUEY_PI * window_phase);
            double coefficient = 2.0 * resampler->cutoff
                * normalized_sinc(2.0 * resampler->cutoff * distance)
                * window;
            resampler->coefficients[phase_index][tap] = (float)coefficient;
            coefficient_sum += coefficient;
        }
        if (fabs(coefficient_sum) < 1.0e-12) {
            return 0;
        }
        for (int tap = 0; tap < BLUEY_RESAMPLER_TAPS; tap++) {
            resampler->coefficients[phase_index][tap] =
                (float)((double)resampler->coefficients[phase_index][tap] / coefficient_sum);
        }
    }
    return 1;
}

static float sample_at(const BlueyResampler *resampler, int64_t index) {
    if (index < 0 || index > resampler->latest_input_index
        || resampler->latest_input_index - index >= BLUEY_RESAMPLER_RING_CAPACITY) {
        return 0.0f;
    }
    return resampler->ring[(size_t)index % BLUEY_RESAMPLER_RING_CAPACITY];
}

int bluey_resampler_push(
    BlueyResampler *resampler,
    float sample,
    BlueyResamplerSink sink,
    void *context
) {
    if (resampler == NULL || sink == NULL || resampler->output_step <= 0.0) {
        return 0;
    }
    resampler->latest_input_index += 1;
    resampler->ring[(size_t)resampler->latest_input_index % BLUEY_RESAMPLER_RING_CAPACITY] = sample;

    while (resampler->next_output_time + BLUEY_RESAMPLER_HALF_TAPS
           <= (double)resampler->latest_input_index) {
        int64_t center = (int64_t)floor(resampler->next_output_time);
        int64_t first = center - BLUEY_RESAMPLER_HALF_TAPS + 1;
        double fractional = resampler->next_output_time - (double)center;
        int phase_index = (int)(fractional * BLUEY_RESAMPLER_PHASES);
        if (phase_index < 0) {
            phase_index = 0;
        } else if (phase_index >= BLUEY_RESAMPLER_PHASES) {
            phase_index = BLUEY_RESAMPLER_PHASES - 1;
        }
        double weighted_sum = 0.0;
        for (int tap = 0; tap < BLUEY_RESAMPLER_TAPS; tap++) {
            int64_t input_index = first + tap;
            weighted_sum += (double)resampler->coefficients[phase_index][tap]
                * (double)sample_at(resampler, input_index);
        }
        double filtered = weighted_sum;
        if (filtered > 1.0) {
            filtered = 1.0;
        } else if (filtered < -1.0) {
            filtered = -1.0;
        }
        int32_t scaled = filtered >= 0.0
            ? (int32_t)(filtered * 32767.0 + 0.5)
            : (int32_t)(filtered * 32768.0 - 0.5);
        if (!sink((int16_t)scaled, context)) {
            return 0;
        }
        resampler->next_output_time += resampler->output_step;
    }
    return 1;
}
