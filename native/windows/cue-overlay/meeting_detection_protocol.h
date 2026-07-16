#ifndef BLUEY_MEETING_DETECTION_PROTOCOL_H
#define BLUEY_MEETING_DETECTION_PROTOCOL_H

#include <stdbool.h>
#include <stddef.h>
#include <string.h>

#include "json_type_extract.h"

#define BLUEY_MEETING_DETECTION_DEFAULT_ENABLED 0

static inline bool bluey_meeting_detection_default_enabled(void) {
    return BLUEY_MEETING_DETECTION_DEFAULT_ENABLED != 0;
}

static inline bool bluey_parse_meeting_detection_enabled(
    const char *json,
    size_t json_len,
    bool *enabled
) {
    if (!json || !enabled) return false;
    const char *value = NULL;
    size_t value_len = 0;
    if (!json_find_top_level_value(
            json,
            json_len,
            "enabled",
            &value,
            &value_len)) {
        return false;
    }
    if (value_len == 4 && memcmp(value, "true", 4) == 0) {
        *enabled = true;
        return true;
    }
    if (value_len == 5 && memcmp(value, "false", 5) == 0) {
        *enabled = false;
        return true;
    }
    return false;
}

#endif
