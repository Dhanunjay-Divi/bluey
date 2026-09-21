#ifndef BLUEY_ANSWER_STYLE_STATE_H
#define BLUEY_ANSWER_STYLE_STATE_H

#include <stdbool.h>
#include <stddef.h>

#include "overlay_state.h"

#define BLUEY_ANSWER_STYLE_MAX_UTF8_BYTES (16u * 1024u)
#define BLUEY_ANSWER_STYLE_MAX_TOKEN_BYTES 128u
#define BLUEY_ANSWER_STYLE_EVENT_MAX_BYTES \
    ((BLUEY_ANSWER_STYLE_MAX_UTF8_BYTES * 6u) \
        + (BLUEY_ANSWER_STYLE_MAX_TOKEN_BYTES * 6u) + 96u)

typedef enum BlueyAnswerStyleMode {
    BLUEY_ANSWER_STYLE_DEFAULT = 0,
    BLUEY_ANSWER_STYLE_CONCISE = 1,
    BLUEY_ANSWER_STYLE_STAR = 2,
    BLUEY_ANSWER_STYLE_CUSTOM = 3,
} BlueyAnswerStyleMode;

typedef enum BlueyAnswerStyleResult {
    BLUEY_ANSWER_STYLE_OK = 0,
    BLUEY_ANSWER_STYLE_EMPTY_CUSTOM = 1,
    BLUEY_ANSWER_STYLE_TOO_LONG = 2,
    BLUEY_ANSWER_STYLE_INVALID_UTF8 = 3,
    BLUEY_ANSWER_STYLE_OUTPUT_TOO_SMALL = 4,
    BLUEY_ANSWER_STYLE_INVALID_MODE = 5,
} BlueyAnswerStyleResult;

typedef enum BlueyAnswerStyleKey {
    BLUEY_ANSWER_STYLE_KEY_OTHER = 0,
    BLUEY_ANSWER_STYLE_KEY_ESCAPE = 1,
    BLUEY_ANSWER_STYLE_KEY_ENTER = 2,
    BLUEY_ANSWER_STYLE_KEY_TAB = 3,
} BlueyAnswerStyleKey;

typedef enum BlueyAnswerStyleKeyAction {
    BLUEY_ANSWER_STYLE_KEY_ACTION_NONE = 0,
    BLUEY_ANSWER_STYLE_KEY_ACTION_CANCEL = 1,
    BLUEY_ANSWER_STYLE_KEY_ACTION_APPLY = 2,
    BLUEY_ANSWER_STYLE_KEY_ACTION_NAVIGATE = 3,
} BlueyAnswerStyleKeyAction;

typedef enum BlueyAnswerStyleSessionFieldStatus {
    BLUEY_ANSWER_STYLE_SESSION_FIELD_ABSENT = 0,
    BLUEY_ANSWER_STYLE_SESSION_FIELD_VALID = 1,
    BLUEY_ANSWER_STYLE_SESSION_FIELD_INVALID = 2,
} BlueyAnswerStyleSessionFieldStatus;

#ifdef __cplusplus
extern "C" {
#endif

bool bluey_answer_style_can_open(BlueyOverlayAccountState account_state);
const char *bluey_answer_style_mode_name(BlueyAnswerStyleMode mode);
const char *bluey_answer_style_preset_instructions(BlueyAnswerStyleMode mode);
BlueyAnswerStyleMode bluey_answer_style_match(
    const char *instructions,
    size_t instructions_len);

bool bluey_answer_style_is_valid_utf8(const char *text, size_t text_len);
BlueyAnswerStyleResult bluey_answer_style_prepare(
    BlueyAnswerStyleMode mode,
    const char *custom_text,
    size_t custom_text_len,
    char *output,
    size_t output_capacity,
    size_t *output_len);

BlueyAnswerStyleKeyAction bluey_answer_style_key_action(
    BlueyAnswerStyleKey key,
    bool custom_editor_focused,
    bool control_pressed);

bool bluey_answer_style_build_event_json(
    const char *instructions,
    size_t instructions_len,
    const char *token,
    size_t token_len,
    char *output,
    size_t output_capacity,
    size_t *output_len);

BlueyAnswerStyleSessionFieldStatus bluey_answer_style_parse_session_field(
    const char *json,
    size_t json_len,
    char *output,
    size_t output_capacity,
    size_t *output_len);

#ifdef __cplusplus
}
#endif

#endif
