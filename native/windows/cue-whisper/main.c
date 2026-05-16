/**
 * cue-whisper: Local Whisper STT helper for Windows.
 *
 * Reads PCM16 LE 16kHz mono from stdin in ~3-second chunks,
 * runs whisper.cpp transcription, and emits NDJSON events to stdout.
 *
 * NOTE: STUB implementation. Real whisper.cpp integration deferred.
 */

#include <stdio.h>
#include <stdlib.h>
#include <math.h>

#ifdef _WIN32
#include <io.h>
#include <fcntl.h>
#endif

#define SAMPLE_RATE 16000
#define CHUNK_DURATION_SEC 3
#define CHUNK_SAMPLES (SAMPLE_RATE * CHUNK_DURATION_SEC)
#define CHUNK_BYTES (CHUNK_SAMPLES * 2)

int main(void) {
    short *buf;
    size_t rd;

#ifdef _WIN32
    _setmode(_fileno(stdin), _O_BINARY);
    _setmode(_fileno(stdout), _O_BINARY);
#endif

    buf = (short *)malloc(CHUNK_BYTES);
    if (!buf) return 1;

    while (1) {
        size_t n_samples;
        double sum_sq;
        double rms;
        size_t i;

        rd = fread(buf, 1, CHUNK_BYTES, stdin);
        if (rd == 0) break;

        /* Compute RMS to detect speech vs silence */
        n_samples = rd / 2;
        sum_sq = 0.0;
        for (i = 0; i < n_samples; i++) {
            sum_sq += (double)buf[i] * (double)buf[i];
        }
        rms = sqrt(sum_sq / (double)(n_samples > 0 ? n_samples : 1));

        if (rms > 500.0) {
            printf("{\"type\":\"partial\",\"text\":\"[speech detected]\"}\n");
            printf("{\"type\":\"final\",\"text\":\"[stub transcription]\",\"confidence\":0.0}\n");
            fflush(stdout);
        }
    }

    free(buf);
    return 0;
}
