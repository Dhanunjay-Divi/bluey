#include "audio_args.h"

#include <stddef.h>
#include <string.h>

#define BLUEY_DEFAULT_DURATION_MS 3000U
#define BLUEY_MIN_DURATION_MS 250U
#define BLUEY_MAX_DURATION_MS 30000U

enum BlueySeenArgument {
    BLUEY_SEEN_SOURCE = 1,
    BLUEY_SEEN_DURATION = 2,
    BLUEY_SEEN_CONTINUOUS = 4
};

static int parse_duration_ms(const char *text, uint32_t *duration_ms) {
    if (text == NULL || text[0] == '\0' || duration_ms == NULL) {
        return 0;
    }

    uint32_t value = 0;
    for (const unsigned char *cursor = (const unsigned char *)text; *cursor != '\0'; cursor++) {
        if (*cursor < (unsigned char)'0' || *cursor > (unsigned char)'9') {
            return 0;
        }
        uint32_t digit = (uint32_t)(*cursor - (unsigned char)'0');
        if (value > (BLUEY_MAX_DURATION_MS - digit) / 10U) {
            return 0;
        }
        value = value * 10U + digit;
    }

    if (value < BLUEY_MIN_DURATION_MS || value > BLUEY_MAX_DURATION_MS) {
        return 0;
    }
    *duration_ms = value;
    return 1;
}

BlueyAudioArgsStatus bluey_audio_parse_args(
    int argc,
    const char *const *argv,
    BlueyAudioArgs *args
) {
    if (argc < 0 || args == NULL || (argc > 0 && argv == NULL)) {
        return BLUEY_AUDIO_ARGS_UNKNOWN_OPTION;
    }

    args->source = BLUEY_CAPTURE_SOURCE_SYSTEM;
    args->duration_ms = BLUEY_DEFAULT_DURATION_MS;
    args->continuous = 0;
    unsigned int seen = 0;

    for (int index = 1; index < argc; index++) {
        const char *argument = argv[index];
        if (argument == NULL) {
            return BLUEY_AUDIO_ARGS_UNKNOWN_OPTION;
        }

        if (strcmp(argument, "--source") == 0) {
            if ((seen & BLUEY_SEEN_SOURCE) != 0U) {
                return BLUEY_AUDIO_ARGS_DUPLICATE_OPTION;
            }
            if (index + 1 >= argc || argv[index + 1] == NULL) {
                return BLUEY_AUDIO_ARGS_MISSING_VALUE;
            }
            index++;
            if (strcmp(argv[index], "system") == 0) {
                args->source = BLUEY_CAPTURE_SOURCE_SYSTEM;
            } else if (strcmp(argv[index], "microphone") == 0) {
                args->source = BLUEY_CAPTURE_SOURCE_MICROPHONE;
            } else {
                return BLUEY_AUDIO_ARGS_INVALID_SOURCE;
            }
            seen |= BLUEY_SEEN_SOURCE;
            continue;
        }

        if (strcmp(argument, "--duration-ms") == 0) {
            if ((seen & BLUEY_SEEN_DURATION) != 0U) {
                return BLUEY_AUDIO_ARGS_DUPLICATE_OPTION;
            }
            if (index + 1 >= argc || argv[index + 1] == NULL) {
                return BLUEY_AUDIO_ARGS_MISSING_VALUE;
            }
            index++;
            if (!parse_duration_ms(argv[index], &args->duration_ms)) {
                return BLUEY_AUDIO_ARGS_INVALID_DURATION;
            }
            seen |= BLUEY_SEEN_DURATION;
            continue;
        }

        if (strcmp(argument, "--continuous") == 0) {
            if ((seen & BLUEY_SEEN_CONTINUOUS) != 0U) {
                return BLUEY_AUDIO_ARGS_DUPLICATE_OPTION;
            }
            args->continuous = 1;
            seen |= BLUEY_SEEN_CONTINUOUS;
            continue;
        }

        return BLUEY_AUDIO_ARGS_UNKNOWN_OPTION;
    }

    if ((seen & BLUEY_SEEN_CONTINUOUS) != 0U && (seen & BLUEY_SEEN_DURATION) != 0U) {
        return BLUEY_AUDIO_ARGS_CONFLICTING_OPTIONS;
    }
    return BLUEY_AUDIO_ARGS_OK;
}

const char *bluey_audio_args_status_code(BlueyAudioArgsStatus status) {
    switch (status) {
    case BLUEY_AUDIO_ARGS_OK:
        return "ok";
    case BLUEY_AUDIO_ARGS_UNKNOWN_OPTION:
        return "unknown_option";
    case BLUEY_AUDIO_ARGS_MISSING_VALUE:
        return "missing_value";
    case BLUEY_AUDIO_ARGS_INVALID_SOURCE:
        return "invalid_source";
    case BLUEY_AUDIO_ARGS_INVALID_DURATION:
        return "invalid_duration";
    case BLUEY_AUDIO_ARGS_DUPLICATE_OPTION:
        return "duplicate_option";
    case BLUEY_AUDIO_ARGS_CONFLICTING_OPTIONS:
        return "conflicting_options";
    default:
        return "unknown_option";
    }
}
