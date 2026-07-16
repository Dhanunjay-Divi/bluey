#ifndef BLUEY_WINDOWS_AUDIO_ARGS_H
#define BLUEY_WINDOWS_AUDIO_ARGS_H

#include <stdint.h>

typedef enum BlueyCaptureSource {
    BLUEY_CAPTURE_SOURCE_SYSTEM,
    BLUEY_CAPTURE_SOURCE_MICROPHONE
} BlueyCaptureSource;

typedef struct BlueyAudioArgs {
    BlueyCaptureSource source;
    uint32_t duration_ms;
    int continuous;
} BlueyAudioArgs;

typedef enum BlueyAudioArgsStatus {
    BLUEY_AUDIO_ARGS_OK,
    BLUEY_AUDIO_ARGS_UNKNOWN_OPTION,
    BLUEY_AUDIO_ARGS_MISSING_VALUE,
    BLUEY_AUDIO_ARGS_INVALID_SOURCE,
    BLUEY_AUDIO_ARGS_INVALID_DURATION,
    BLUEY_AUDIO_ARGS_DUPLICATE_OPTION,
    BLUEY_AUDIO_ARGS_CONFLICTING_OPTIONS
} BlueyAudioArgsStatus;

BlueyAudioArgsStatus bluey_audio_parse_args(
    int argc,
    const char *const *argv,
    BlueyAudioArgs *args
);

const char *bluey_audio_args_status_code(BlueyAudioArgsStatus status);

#endif
