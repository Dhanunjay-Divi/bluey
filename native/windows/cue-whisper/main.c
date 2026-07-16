/**
 * Development-only capability sentinel for Windows local transcription.
 *
 * Bluey's production Windows archives intentionally omit this binary until a
 * pinned whisper.cpp implementation has passed the same acoustic, dependency,
 * and clean-machine gates as the macOS helper.  In particular, this program
 * must never emit synthetic transcript events: callers either use managed live
 * captions or receive an explicit unsupported-capability failure.
 */

#include <stdio.h>

int main(void) {
    fputs(
        "cue-whisper: local Whisper inference is not available in this Windows build; "
        "use Bluey managed live captions\n",
        stderr
    );
    return 2;
}
