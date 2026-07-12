#ifndef BLUEY_ASK_EVENT_PROTOCOL_H
#define BLUEY_ASK_EVENT_PROTOCOL_H

#include <stdbool.h>

static inline bool ask_event_answers_current_transcript(
    bool listen_triggered,
    bool has_transcript_context
) {
    return listen_triggered || has_transcript_context;
}

static inline const char *ask_event_current_transcript_json_field(bool enabled) {
    return enabled ? ",\"answer_current_transcript\":true" : "";
}

#endif
