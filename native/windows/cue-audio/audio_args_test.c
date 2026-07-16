#include "audio_args.h"

#include <stdio.h>
#include <string.h>

static int expect_ok(
    const char *name,
    int argc,
    const char *const *argv,
    BlueyCaptureSource source,
    uint32_t duration_ms,
    int continuous
) {
    BlueyAudioArgs args;
    BlueyAudioArgsStatus status = bluey_audio_parse_args(argc, argv, &args);
    if (status != BLUEY_AUDIO_ARGS_OK
        || args.source != source
        || args.duration_ms != duration_ms
        || args.continuous != continuous) {
        fprintf(
            stderr,
            "%s failed: status=%s source=%d duration=%u continuous=%d\n",
            name,
            bluey_audio_args_status_code(status),
            (int)args.source,
            (unsigned int)args.duration_ms,
            args.continuous
        );
        return 0;
    }
    return 1;
}

static int expect_error(
    const char *name,
    int argc,
    const char *const *argv,
    BlueyAudioArgsStatus expected
) {
    BlueyAudioArgs args;
    BlueyAudioArgsStatus actual = bluey_audio_parse_args(argc, argv, &args);
    if (actual != expected) {
        fprintf(
            stderr,
            "%s failed: expected=%s actual=%s\n",
            name,
            bluey_audio_args_status_code(expected),
            bluey_audio_args_status_code(actual)
        );
        return 0;
    }
    return 1;
}

int main(void) {
    const char *defaults[] = {"bluey-audio"};
    const char *system_duration[] = {
        "bluey-audio", "--source", "system", "--duration-ms", "250"
    };
    const char *microphone_duration[] = {
        "bluey-audio", "--duration-ms", "30000", "--source", "microphone"
    };
    const char *continuous[] = {
        "bluey-audio", "--source", "system", "--continuous"
    };
    const char *unknown[] = {"bluey-audio", "--bogus"};
    const char *missing_source[] = {"bluey-audio", "--source"};
    const char *missing_duration[] = {"bluey-audio", "--duration-ms"};
    const char *invalid_source[] = {"bluey-audio", "--source", "speaker"};
    const char *duration_text[] = {"bluey-audio", "--duration-ms", "1000ms"};
    const char *duration_sign[] = {"bluey-audio", "--duration-ms", "+1000"};
    const char *duration_space[] = {"bluey-audio", "--duration-ms", " 1000"};
    const char *duration_low[] = {"bluey-audio", "--duration-ms", "249"};
    const char *duration_high[] = {"bluey-audio", "--duration-ms", "30001"};
    const char *duration_overflow[] = {
        "bluey-audio", "--duration-ms", "999999999999999999999999"
    };
    const char *duplicate_source[] = {
        "bluey-audio", "--source", "system", "--source", "microphone"
    };
    const char *duplicate_duration[] = {
        "bluey-audio", "--duration-ms", "1000", "--duration-ms", "2000"
    };
    const char *duplicate_continuous[] = {
        "bluey-audio", "--continuous", "--continuous"
    };
    const char *conflicting_mode[] = {
        "bluey-audio", "--continuous", "--duration-ms", "1000"
    };

    int passed = 1;
    passed &= expect_ok(
        "defaults",
        1,
        defaults,
        BLUEY_CAPTURE_SOURCE_SYSTEM,
        3000U,
        0
    );
    passed &= expect_ok(
        "system duration",
        5,
        system_duration,
        BLUEY_CAPTURE_SOURCE_SYSTEM,
        250U,
        0
    );
    passed &= expect_ok(
        "microphone duration",
        5,
        microphone_duration,
        BLUEY_CAPTURE_SOURCE_MICROPHONE,
        30000U,
        0
    );
    passed &= expect_ok(
        "continuous",
        4,
        continuous,
        BLUEY_CAPTURE_SOURCE_SYSTEM,
        3000U,
        1
    );
    passed &= expect_error(
        "unknown option",
        2,
        unknown,
        BLUEY_AUDIO_ARGS_UNKNOWN_OPTION
    );
    passed &= expect_error(
        "missing source",
        2,
        missing_source,
        BLUEY_AUDIO_ARGS_MISSING_VALUE
    );
    passed &= expect_error(
        "missing duration",
        2,
        missing_duration,
        BLUEY_AUDIO_ARGS_MISSING_VALUE
    );
    passed &= expect_error(
        "invalid source",
        3,
        invalid_source,
        BLUEY_AUDIO_ARGS_INVALID_SOURCE
    );
    passed &= expect_error(
        "duration text",
        3,
        duration_text,
        BLUEY_AUDIO_ARGS_INVALID_DURATION
    );
    passed &= expect_error(
        "duration sign",
        3,
        duration_sign,
        BLUEY_AUDIO_ARGS_INVALID_DURATION
    );
    passed &= expect_error(
        "duration whitespace",
        3,
        duration_space,
        BLUEY_AUDIO_ARGS_INVALID_DURATION
    );
    passed &= expect_error(
        "duration below minimum",
        3,
        duration_low,
        BLUEY_AUDIO_ARGS_INVALID_DURATION
    );
    passed &= expect_error(
        "duration above maximum",
        3,
        duration_high,
        BLUEY_AUDIO_ARGS_INVALID_DURATION
    );
    passed &= expect_error(
        "duration overflow",
        3,
        duration_overflow,
        BLUEY_AUDIO_ARGS_INVALID_DURATION
    );
    passed &= expect_error(
        "duplicate source",
        5,
        duplicate_source,
        BLUEY_AUDIO_ARGS_DUPLICATE_OPTION
    );
    passed &= expect_error(
        "duplicate duration",
        5,
        duplicate_duration,
        BLUEY_AUDIO_ARGS_DUPLICATE_OPTION
    );
    passed &= expect_error(
        "duplicate continuous",
        3,
        duplicate_continuous,
        BLUEY_AUDIO_ARGS_DUPLICATE_OPTION
    );
    passed &= expect_error(
        "conflicting modes",
        4,
        conflicting_mode,
        BLUEY_AUDIO_ARGS_CONFLICTING_OPTIONS
    );
    passed &= strcmp(
        bluey_audio_args_status_code(BLUEY_AUDIO_ARGS_INVALID_DURATION),
        "invalid_duration"
    ) == 0;

    if (!passed) {
        return 1;
    }
    printf("audio argument tests passed\n");
    return 0;
}
