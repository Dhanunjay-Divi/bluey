#include "answer_style_state.h"

#include <stdint.h>
#include <string.h>

#include "json_type_extract.h"

static const char *const BLUEY_CONCISE_INSTRUCTIONS =
    "Give the direct, speakable answer first. Keep it concise, usually 2-4 "
    "sentences unless the task needs more detail. Use only verified session "
    "context and user-provided facts; never invent details.";

static const char *const BLUEY_STAR_INSTRUCTIONS =
    "For behavioral questions, answer in concise labeled Situation, Task, "
    "Action, and Result sections. Use only facts supported by the session, "
    "approved context, or the user. If a required fact is missing, say what "
    "is missing or leave a clear placeholder; never invent employers, "
    "projects, dates, people, metrics, or outcomes. For non-behavioral "
    "questions, answer directly and concisely.";

bool bluey_answer_style_can_open(BlueyOverlayAccountState account_state) {
    return account_state == BLUEY_OVERLAY_ACCOUNT_SIGNED_IN;
}

const char *bluey_answer_style_mode_name(BlueyAnswerStyleMode mode) {
    switch (mode) {
    case BLUEY_ANSWER_STYLE_DEFAULT:
        return "default";
    case BLUEY_ANSWER_STYLE_CONCISE:
        return "concise";
    case BLUEY_ANSWER_STYLE_STAR:
        return "star";
    case BLUEY_ANSWER_STYLE_CUSTOM:
        return "custom";
    default:
        return "invalid";
    }
}

const char *bluey_answer_style_preset_instructions(BlueyAnswerStyleMode mode) {
    switch (mode) {
    case BLUEY_ANSWER_STYLE_DEFAULT:
        return "";
    case BLUEY_ANSWER_STYLE_CONCISE:
        return BLUEY_CONCISE_INSTRUCTIONS;
    case BLUEY_ANSWER_STYLE_STAR:
        return BLUEY_STAR_INSTRUCTIONS;
    case BLUEY_ANSWER_STYLE_CUSTOM:
    default:
        return NULL;
    }
}

static bool bluey_utf8_continuation(unsigned char value) {
    return (value & 0xc0u) == 0x80u;
}

bool bluey_answer_style_is_valid_utf8(const char *text, size_t text_len) {
    if (!text && text_len != 0) return false;

    size_t index = 0;
    while (index < text_len) {
        unsigned char first = (unsigned char)text[index];
        if (first == 0) return false;
        if (first <= 0x7fu) {
            index++;
            continue;
        }
        if (first >= 0xc2u && first <= 0xdfu) {
            if (index + 1 >= text_len
                || !bluey_utf8_continuation((unsigned char)text[index + 1])) {
                return false;
            }
            index += 2;
            continue;
        }
        if (first >= 0xe0u && first <= 0xefu) {
            if (index + 2 >= text_len) return false;
            unsigned char second = (unsigned char)text[index + 1];
            unsigned char third = (unsigned char)text[index + 2];
            if (!bluey_utf8_continuation(second)
                || !bluey_utf8_continuation(third)
                || (first == 0xe0u && second < 0xa0u)
                || (first == 0xedu && second >= 0xa0u)) {
                return false;
            }
            index += 3;
            continue;
        }
        if (first >= 0xf0u && first <= 0xf4u) {
            if (index + 3 >= text_len) return false;
            unsigned char second = (unsigned char)text[index + 1];
            unsigned char third = (unsigned char)text[index + 2];
            unsigned char fourth = (unsigned char)text[index + 3];
            if (!bluey_utf8_continuation(second)
                || !bluey_utf8_continuation(third)
                || !bluey_utf8_continuation(fourth)
                || (first == 0xf0u && second < 0x90u)
                || (first == 0xf4u && second >= 0x90u)) {
                return false;
            }
            index += 4;
            continue;
        }
        return false;
    }
    return true;
}

static bool bluey_ascii_space(unsigned char value) {
    return value == ' ' || value == '\t' || value == '\n'
        || value == '\r' || value == '\f' || value == '\v';
}

BlueyAnswerStyleMode bluey_answer_style_match(
    const char *instructions,
    size_t instructions_len
) {
    if (!instructions && instructions_len != 0) return BLUEY_ANSWER_STYLE_CUSTOM;
    size_t start = 0;
    size_t end = instructions_len;
    while (start < end && bluey_ascii_space((unsigned char)instructions[start])) start++;
    while (end > start && bluey_ascii_space((unsigned char)instructions[end - 1])) end--;
    size_t trimmed_len = end - start;
    if (trimmed_len == 0) return BLUEY_ANSWER_STYLE_DEFAULT;

    const char *concise = bluey_answer_style_preset_instructions(
        BLUEY_ANSWER_STYLE_CONCISE);
    size_t concise_len = strlen(concise);
    if (trimmed_len == concise_len
        && memcmp(instructions + start, concise, concise_len) == 0) {
        return BLUEY_ANSWER_STYLE_CONCISE;
    }
    const char *star = bluey_answer_style_preset_instructions(
        BLUEY_ANSWER_STYLE_STAR);
    size_t star_len = strlen(star);
    if (trimmed_len == star_len
        && memcmp(instructions + start, star, star_len) == 0) {
        return BLUEY_ANSWER_STYLE_STAR;
    }
    return BLUEY_ANSWER_STYLE_CUSTOM;
}

BlueyAnswerStyleResult bluey_answer_style_prepare(
    BlueyAnswerStyleMode mode,
    const char *custom_text,
    size_t custom_text_len,
    char *output,
    size_t output_capacity,
    size_t *output_len
) {
    if (output_len) *output_len = 0;
    if (!output || output_capacity == 0) {
        return BLUEY_ANSWER_STYLE_OUTPUT_TOO_SMALL;
    }

    const char *source = bluey_answer_style_preset_instructions(mode);
    size_t source_len = source ? strlen(source) : 0;
    if (mode == BLUEY_ANSWER_STYLE_CUSTOM) {
        if (!custom_text && custom_text_len != 0) {
            return BLUEY_ANSWER_STYLE_INVALID_UTF8;
        }
        size_t start = 0;
        size_t end = custom_text_len;
        while (start < end && bluey_ascii_space((unsigned char)custom_text[start])) start++;
        while (end > start && bluey_ascii_space((unsigned char)custom_text[end - 1])) end--;
        if (start == end) return BLUEY_ANSWER_STYLE_EMPTY_CUSTOM;
        source = custom_text + start;
        source_len = end - start;
    } else if (!source) {
        return BLUEY_ANSWER_STYLE_INVALID_MODE;
    }

    if (source_len > BLUEY_ANSWER_STYLE_MAX_UTF8_BYTES) {
        return BLUEY_ANSWER_STYLE_TOO_LONG;
    }
    if (!bluey_answer_style_is_valid_utf8(source, source_len)) {
        return BLUEY_ANSWER_STYLE_INVALID_UTF8;
    }
    if (source_len + 1u > output_capacity) {
        return BLUEY_ANSWER_STYLE_OUTPUT_TOO_SMALL;
    }
    if (source_len != 0) memcpy(output, source, source_len);
    output[source_len] = '\0';
    if (output_len) *output_len = source_len;
    return BLUEY_ANSWER_STYLE_OK;
}

BlueyAnswerStyleKeyAction bluey_answer_style_key_action(
    BlueyAnswerStyleKey key,
    bool custom_editor_focused,
    bool control_pressed
) {
    switch (key) {
    case BLUEY_ANSWER_STYLE_KEY_ESCAPE:
        return BLUEY_ANSWER_STYLE_KEY_ACTION_CANCEL;
    case BLUEY_ANSWER_STYLE_KEY_TAB:
        return BLUEY_ANSWER_STYLE_KEY_ACTION_NAVIGATE;
    case BLUEY_ANSWER_STYLE_KEY_ENTER:
        return custom_editor_focused && !control_pressed
            ? BLUEY_ANSWER_STYLE_KEY_ACTION_NONE
            : BLUEY_ANSWER_STYLE_KEY_ACTION_APPLY;
    case BLUEY_ANSWER_STYLE_KEY_OTHER:
    default:
        return BLUEY_ANSWER_STYLE_KEY_ACTION_NONE;
    }
}

typedef struct BlueyAnswerStyleJsonWriter {
    char *output;
    size_t capacity;
    size_t length;
    bool failed;
} BlueyAnswerStyleJsonWriter;

static bool bluey_json_write(
    BlueyAnswerStyleJsonWriter *writer,
    const char *text,
    size_t text_len
) {
    if (!writer || writer->failed || !text
        || text_len >= writer->capacity
        || writer->length > writer->capacity - text_len - 1u) {
        if (writer) writer->failed = true;
        return false;
    }
    memcpy(writer->output + writer->length, text, text_len);
    writer->length += text_len;
    writer->output[writer->length] = '\0';
    return true;
}

static bool bluey_json_write_literal(
    BlueyAnswerStyleJsonWriter *writer,
    const char *text
) {
    return bluey_json_write(writer, text, strlen(text));
}

static bool bluey_json_write_escaped(
    BlueyAnswerStyleJsonWriter *writer,
    const char *text,
    size_t text_len
) {
    static const char hex[] = "0123456789abcdef";
    for (size_t index = 0; index < text_len; index++) {
        unsigned char value = (unsigned char)text[index];
        if (value == '"' || value == '\\') {
            char escaped[2] = {'\\', (char)value};
            if (!bluey_json_write(writer, escaped, sizeof(escaped))) return false;
        } else if (value == '\n') {
            if (!bluey_json_write_literal(writer, "\\n")) return false;
        } else if (value == '\r') {
            if (!bluey_json_write_literal(writer, "\\r")) return false;
        } else if (value == '\t') {
            if (!bluey_json_write_literal(writer, "\\t")) return false;
        } else if (value < 0x20u) {
            char escaped[6] = {
                '\\', 'u', '0', '0', hex[value >> 4u], hex[value & 0x0fu],
            };
            if (!bluey_json_write(writer, escaped, sizeof(escaped))) return false;
        } else {
            char byte = (char)value;
            if (!bluey_json_write(writer, &byte, 1u)) return false;
        }
    }
    return true;
}

bool bluey_answer_style_build_event_json(
    const char *instructions,
    size_t instructions_len,
    const char *token,
    size_t token_len,
    char *output,
    size_t output_capacity,
    size_t *output_len
) {
    if (output_len) *output_len = 0;
    if (!output || output_capacity == 0
        || (!instructions && instructions_len != 0)
        || (!token && token_len != 0)
        || instructions_len > BLUEY_ANSWER_STYLE_MAX_UTF8_BYTES
        || token_len > BLUEY_ANSWER_STYLE_MAX_TOKEN_BYTES
        || !bluey_answer_style_is_valid_utf8(instructions, instructions_len)
        || !bluey_answer_style_is_valid_utf8(token, token_len)) {
        return false;
    }

    BlueyAnswerStyleJsonWriter writer = {
        output,
        output_capacity,
        0,
        false,
    };
    output[0] = '\0';
    bluey_json_write_literal(
        &writer,
        "{\"type\":\"instructions_updated\",\"text\":\"");
    bluey_json_write_escaped(&writer, instructions, instructions_len);
    bluey_json_write_literal(&writer, "\"");
    if (token_len != 0) {
        bluey_json_write_literal(&writer, ",\"token\":\"");
        bluey_json_write_escaped(&writer, token, token_len);
        bluey_json_write_literal(&writer, "\"");
    }
    bluey_json_write_literal(&writer, "}\n");
    if (writer.failed) return false;
    if (output_len) *output_len = writer.length;
    return true;
}

BlueyAnswerStyleSessionFieldStatus bluey_answer_style_parse_session_field(
    const char *json,
    size_t json_len,
    char *output,
    size_t output_capacity,
    size_t *output_len
) {
    if (output_len) *output_len = 0;
    if (!json || !output || output_capacity == 0) {
        return BLUEY_ANSWER_STYLE_SESSION_FIELD_INVALID;
    }
    output[0] = '\0';

    const char *raw = NULL;
    size_t raw_len = 0;
    if (!json_find_top_level_value(
            json,
            json_len,
            "answer_instructions",
            &raw,
            &raw_len)) {
        return BLUEY_ANSWER_STYLE_SESSION_FIELD_ABSENT;
    }
    if (raw_len == 4 && memcmp(raw, "null", 4) == 0) {
        return BLUEY_ANSWER_STYLE_SESSION_FIELD_VALID;
    }

    char *decoded = NULL;
    size_t decoded_len = 0;
    if (!json_extract_string_alloc_limited(
            json,
            json_len,
            "answer_instructions",
            BLUEY_ANSWER_STYLE_MAX_UTF8_BYTES,
            &decoded,
            &decoded_len)) {
        return BLUEY_ANSWER_STYLE_SESSION_FIELD_INVALID;
    }
    bool valid = decoded_len <= BLUEY_ANSWER_STYLE_MAX_UTF8_BYTES
        && decoded_len + 1u <= output_capacity
        && bluey_answer_style_is_valid_utf8(decoded, decoded_len);
    if (valid) {
        if (decoded_len != 0) memcpy(output, decoded, decoded_len);
        output[decoded_len] = '\0';
        if (output_len) *output_len = decoded_len;
    }
    free(decoded);
    return valid
        ? BLUEY_ANSWER_STYLE_SESSION_FIELD_VALID
        : BLUEY_ANSWER_STYLE_SESSION_FIELD_INVALID;
}
