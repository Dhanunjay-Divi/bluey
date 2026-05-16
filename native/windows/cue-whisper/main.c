/**
 * cue-whisper: Local Whisper STT helper for Windows.
 *
 * Reads PCM16 LE 16kHz mono from stdin in ~3-second chunks,
 * runs whisper.cpp transcription, and emits NDJSON events to stdout.
 *
 * NOTE: STUB implementation. Real whisper.cpp integration requires
 * CMake + MSVC build of whisper.cpp and is deferred to a follow-up.
 * This stub validates the IPC protocol and model path configuration.
 *
 * TODO: Integrate whisper.cpp via CMake (link libwhisper.a statically).
 * TODO: Call whisper_full() with the same parameters as the macOS helper.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

#ifdef _WIN32
#include <io.h>
#include <fcntl.h>
#include <windows.h>
#define PATH_SEP '\\'
#else
#include <unistd.h>
#define PATH_SEP '/'
#endif

#define SAMPLE_RATE 16000
#define CHUNK_DURATION_SEC 3
#define CHUNK_SAMPLES (SAMPLE_RATE * CHUNK_DURATION_SEC)
#define CHUNK_BYTES (CHUNK_SAMPLES * 2)
#define RMS_THRESHOLD 500.0

static int check_model_exists(const char *path) {
    FILE *f = fopen(path, "rb");
    if (f) { fclose(f); return 1; }
    return 0;
}

int main(void) {
    short *buf;
    size_t rd;
    const char *model_path;
    char default_path[512];

#ifdef _WIN32
    _setmode(_fileno(stdin), _O_BINARY);
    _setmode(_fileno(stdout), _O_BINARY);
#endif

    /* Resolve model path (parity with macOS helper) */
    model_path = getenv("BLUEY_WHISPER_MODEL");
    if (!model_path || model_path[0] == '\0') {
        const char *home = getenv("USERPROFILE");
        if (!home) home = getenv("HOME");
        if (home) {
            snprintf(default_path, sizeof(default_path),
                     "%s%c.cache%cbluey%cwhisper%ctiny.en-q5_1.bin",
                     home, PATH_SEP, PATH_SEP, PATH_SEP, PATH_SEP);
            model_path = default_path;
        }
    }

    if (!model_path || !check_model_exists(model_path)) {
        fprintf(stderr, "cue-whisper: ERROR: model not found at %s\n",
                model_path ? model_path : "(unset)");
        fprintf(stderr, "cue-whisper: Run: infra/scripts/download-whisper-model.sh\n");
        return 1;
    }

    fprintf(stderr, "cue-whisper: [STUB] model found: %s\n", model_path);
    fprintf(stderr, "cue-whisper: [STUB] Real whisper.cpp not yet integrated on Windows\n");

    buf = (short *)malloc(CHUNK_BYTES);
    if (!buf) return 1;

    while (1) {
        size_t n_samples;
        double sum_sq;
        double rms;
        size_t i;

        rd = fread(buf, 1, CHUNK_BYTES, stdin);
        if (rd == 0) break;

        n_samples = rd / 2;
        sum_sq = 0.0;
        for (i = 0; i < n_samples; i++) {
            sum_sq += (double)buf[i] * (double)buf[i];
        }
        rms = sqrt(sum_sq / (double)(n_samples > 0 ? n_samples : 1));

        if (rms > RMS_THRESHOLD) {
            printf("{\"type\":\"partial\",\"text\":\"[speech detected]\"}\n");
            printf("{\"type\":\"final\",\"text\":\"[stub transcription]\",\"confidence\":0.0}\n");
            fflush(stdout);
        }
    }

    free(buf);
    return 0;
}
