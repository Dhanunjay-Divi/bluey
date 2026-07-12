#ifndef JSON_TYPE_EXTRACT_H
#define JSON_TYPE_EXTRACT_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define JSON_MAX_LINE_LEN (8u * 1024u * 1024u)
#define JSON_MAX_TEXT_LEN JSON_MAX_LINE_LEN
#define JSON_MAX_FIELD_LEN (16u * 1024u)
#define JSON_MAX_DEPTH 128u

static inline const char *json_skip_ws(const char *p, const char *end) {
    while (p < end && (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n')) {
        p++;
    }
    return p;
}

static inline bool json_is_hex(char c) {
    return (c >= '0' && c <= '9')
        || (c >= 'a' && c <= 'f')
        || (c >= 'A' && c <= 'F');
}

static inline unsigned json_hex_value(char c) {
    if (c >= '0' && c <= '9') return (unsigned)(c - '0');
    if (c >= 'a' && c <= 'f') return (unsigned)(c - 'a' + 10);
    return (unsigned)(c - 'A' + 10);
}

static inline const char *json_skip_string(const char *p, const char *end) {
    while (p < end) {
        unsigned char c = (unsigned char)*p++;
        if (c == '"') return p;
        if (c < 0x20) return NULL;
        if (c != '\\') continue;
        if (p >= end) return NULL;

        char escaped = *p++;
        if (escaped == 'u') {
            if ((size_t)(end - p) < 4) return NULL;
            for (size_t i = 0; i < 4; i++) {
                if (!json_is_hex(p[i])) return NULL;
            }
            p += 4;
        } else if (escaped != '"' && escaped != '\\' && escaped != '/'
                   && escaped != 'b' && escaped != 'f' && escaped != 'n'
                   && escaped != 'r' && escaped != 't') {
            return NULL;
        }
    }
    return NULL;
}

static inline const char *json_skip_value_depth(
    const char *p,
    const char *end,
    unsigned depth
);

static inline const char *json_skip_object(const char *p, const char *end, unsigned depth) {
    p = json_skip_ws(p, end);
    if (p < end && *p == '}') return p + 1;

    while (p < end) {
        if (*p != '"') return NULL;
        p = json_skip_string(p + 1, end);
        if (!p) return NULL;
        p = json_skip_ws(p, end);
        if (p >= end || *p != ':') return NULL;
        p = json_skip_value_depth(p + 1, end, depth + 1);
        if (!p) return NULL;
        p = json_skip_ws(p, end);
        if (p < end && *p == '}') return p + 1;
        if (p >= end || *p != ',') return NULL;
        p = json_skip_ws(p + 1, end);
    }
    return NULL;
}

static inline const char *json_skip_array(const char *p, const char *end, unsigned depth) {
    p = json_skip_ws(p, end);
    if (p < end && *p == ']') return p + 1;

    while (p < end) {
        p = json_skip_value_depth(p, end, depth + 1);
        if (!p) return NULL;
        p = json_skip_ws(p, end);
        if (p < end && *p == ']') return p + 1;
        if (p >= end || *p != ',') return NULL;
        p = json_skip_ws(p + 1, end);
    }
    return NULL;
}

static inline const char *json_skip_value_depth(
    const char *p,
    const char *end,
    unsigned depth
) {
    if (depth > JSON_MAX_DEPTH) return NULL;
    p = json_skip_ws(p, end);
    if (p >= end) return NULL;

    if (*p == '"') return json_skip_string(p + 1, end);
    if (*p == '{') return json_skip_object(p + 1, end, depth);
    if (*p == '[') return json_skip_array(p + 1, end, depth);

    const char *start = p;
    while (p < end && *p != ',' && *p != '}' && *p != ']'
           && *p != ' ' && *p != '\t' && *p != '\r' && *p != '\n') {
        if ((unsigned char)*p < 0x20) return NULL;
        p++;
    }
    return p > start ? p : NULL;
}

static inline const char *json_skip_value(const char *p, const char *end) {
    return json_skip_value_depth(p, end, 0);
}

static inline bool json_find_top_level_value(
    const char *json,
    size_t json_len,
    const char *key,
    const char **value,
    size_t *value_len
) {
    if (value) *value = NULL;
    if (value_len) *value_len = 0;
    if (!json || json_len == 0 || json_len > JSON_MAX_LINE_LEN || !key || !value || !value_len) {
        return false;
    }

    const char *end = json + json_len;
    const char *p = json_skip_ws(json, end);
    if (p >= end || *p != '{') return false;
    p = json_skip_ws(p + 1, end);
    size_t key_len = strlen(key);

    while (p < end && *p != '}') {
        if (*p != '"') return false;
        const char *key_start = p + 1;
        const char *key_after = json_skip_string(key_start, end);
        if (!key_after) return false;
        const char *key_end = key_after - 1;

        p = json_skip_ws(key_after, end);
        if (p >= end || *p != ':') return false;
        p = json_skip_ws(p + 1, end);
        const char *after_value = json_skip_value(p, end);
        if (!after_value) return false;

        if ((size_t)(key_end - key_start) == key_len
            && memcmp(key_start, key, key_len) == 0) {
            *value = p;
            *value_len = (size_t)(after_value - p);
            return true;
        }

        p = json_skip_ws(after_value, end);
        if (p < end && *p == '}') break;
        if (p >= end || *p != ',') return false;
        p = json_skip_ws(p + 1, end);
    }
    return false;
}

static inline bool json_extract_object(
    const char *json,
    size_t json_len,
    const char *key,
    const char **object,
    size_t *object_len
) {
    const char *value = NULL;
    size_t length = 0;
    if (!json_find_top_level_value(json, json_len, key, &value, &length)
        || length < 2 || value[0] != '{') {
        return false;
    }
    *object = value;
    *object_len = length;
    return true;
}

static inline bool json_extract_array(
    const char *json,
    size_t json_len,
    const char *key,
    const char **array,
    size_t *array_len
) {
    const char *value = NULL;
    size_t length = 0;
    if (!json_find_top_level_value(json, json_len, key, &value, &length)
        || length < 2 || value[0] != '[') {
        return false;
    }
    *array = value;
    *array_len = length;
    return true;
}

static inline bool json_append_codepoint(
    char *dest,
    size_t capacity,
    size_t *length,
    uint32_t codepoint
) {
    unsigned char encoded[4];
    size_t count = 0;
    if (codepoint <= 0x7f) {
        encoded[count++] = (unsigned char)codepoint;
    } else if (codepoint <= 0x7ff) {
        encoded[count++] = (unsigned char)(0xc0 | (codepoint >> 6));
        encoded[count++] = (unsigned char)(0x80 | (codepoint & 0x3f));
    } else if (codepoint <= 0xffff) {
        encoded[count++] = (unsigned char)(0xe0 | (codepoint >> 12));
        encoded[count++] = (unsigned char)(0x80 | ((codepoint >> 6) & 0x3f));
        encoded[count++] = (unsigned char)(0x80 | (codepoint & 0x3f));
    } else if (codepoint <= 0x10ffff) {
        encoded[count++] = (unsigned char)(0xf0 | (codepoint >> 18));
        encoded[count++] = (unsigned char)(0x80 | ((codepoint >> 12) & 0x3f));
        encoded[count++] = (unsigned char)(0x80 | ((codepoint >> 6) & 0x3f));
        encoded[count++] = (unsigned char)(0x80 | (codepoint & 0x3f));
    } else {
        return false;
    }
    if (count > capacity - *length) return false;
    memcpy(dest + *length, encoded, count);
    *length += count;
    return true;
}

static inline bool json_read_hex_quad(const char *p, const char *end, uint32_t *value) {
    if ((size_t)(end - p) < 4) return false;
    uint32_t decoded = 0;
    for (size_t i = 0; i < 4; i++) {
        if (!json_is_hex(p[i])) return false;
        decoded = (decoded << 4) | json_hex_value(p[i]);
    }
    *value = decoded;
    return true;
}

static inline bool json_extract_string_alloc_limited(
    const char *json,
    size_t json_len,
    const char *key,
    size_t max_decoded_len,
    char **dest,
    size_t *dest_len
) {
    if (dest) *dest = NULL;
    if (dest_len) *dest_len = 0;
    if (!dest || !dest_len) return false;

    const char *value = NULL;
    size_t value_len = 0;
    if (!json_find_top_level_value(json, json_len, key, &value, &value_len)
        || value_len < 2 || value[0] != '"' || value[value_len - 1] != '"') {
        return false;
    }

    const char *p = value + 1;
    const char *end = value + value_len - 1;
    size_t raw_len = (size_t)(end - p);
    size_t allocation_len = raw_len < max_decoded_len ? raw_len : max_decoded_len;
    char *decoded = (char *)malloc(allocation_len + 1);
    if (!decoded) return false;

    size_t written = 0;
    while (p < end) {
        unsigned char c = (unsigned char)*p++;
        if (c != '\\') {
            if (c < 0x20 || written >= max_decoded_len) {
                free(decoded);
                return false;
            }
            decoded[written++] = (char)c;
            continue;
        }

        if (p >= end) {
            free(decoded);
            return false;
        }
        char escaped = *p++;
        if (escaped != 'u') {
            char unescaped;
            switch (escaped) {
            case '"': unescaped = '"'; break;
            case '\\': unescaped = '\\'; break;
            case '/': unescaped = '/'; break;
            case 'b': unescaped = '\b'; break;
            case 'f': unescaped = '\f'; break;
            case 'n': unescaped = '\n'; break;
            case 'r': unescaped = '\r'; break;
            case 't': unescaped = '\t'; break;
            default:
                free(decoded);
                return false;
            }
            if (written >= max_decoded_len) {
                free(decoded);
                return false;
            }
            decoded[written++] = unescaped;
            continue;
        }

        uint32_t codepoint = 0;
        if (!json_read_hex_quad(p, end, &codepoint)) {
            free(decoded);
            return false;
        }
        p += 4;
        if (codepoint >= 0xd800 && codepoint <= 0xdbff) {
            uint32_t low = 0;
            if ((size_t)(end - p) < 6 || p[0] != '\\' || p[1] != 'u'
                || !json_read_hex_quad(p + 2, end, &low)
                || low < 0xdc00 || low > 0xdfff) {
                free(decoded);
                return false;
            }
            p += 6;
            codepoint = 0x10000 + ((codepoint - 0xd800) << 10) + (low - 0xdc00);
        } else if (codepoint >= 0xdc00 && codepoint <= 0xdfff) {
            free(decoded);
            return false;
        }

        if (!json_append_codepoint(decoded, max_decoded_len, &written, codepoint)) {
            free(decoded);
            return false;
        }
    }

    decoded[written] = '\0';
    *dest = decoded;
    *dest_len = written;
    return true;
}

static inline bool json_extract_string_alloc(
    const char *json,
    size_t json_len,
    const char *key,
    char **dest,
    size_t *dest_len
) {
    return json_extract_string_alloc_limited(
        json,
        json_len,
        key,
        JSON_MAX_TEXT_LEN,
        dest,
        dest_len);
}

static inline bool json_extract_string(
    const char *json,
    size_t json_len,
    const char *key,
    char *dest,
    size_t dest_len
) {
    if (!dest || dest_len == 0) return false;
    dest[0] = '\0';
    char *value = NULL;
    size_t value_len = 0;
    if (!json_extract_string_alloc_limited(json, json_len, key, dest_len - 1, &value, &value_len)) {
        return false;
    }
    memcpy(dest, value, value_len + 1);
    free(value);
    return true;
}

static inline bool json_extract_number(
    const char *json,
    size_t json_len,
    const char *key,
    double *dest
) {
    if (!dest) return false;
    const char *value = NULL;
    size_t value_len = 0;
    if (!json_find_top_level_value(json, json_len, key, &value, &value_len)) return false;

    char buffer[128];
    if (value_len >= 2 && value[0] == '"' && value[value_len - 1] == '"') {
        char *decoded = NULL;
        size_t decoded_len = 0;
        if (!json_extract_string_alloc_limited(
                json, json_len, key, sizeof(buffer) - 1, &decoded, &decoded_len)) {
            return false;
        }
        memcpy(buffer, decoded, decoded_len + 1);
        free(decoded);
    } else {
        if (value_len == 0 || value_len >= sizeof(buffer)) return false;
        memcpy(buffer, value, value_len);
        buffer[value_len] = '\0';
    }

    char *number_end = NULL;
    double parsed = strtod(buffer, &number_end);
    if (number_end == buffer || *number_end != '\0') return false;
    *dest = parsed;
    return true;
}

static inline bool json_extract_type(
    const char *json,
    size_t json_len,
    char *dest,
    size_t dest_len
) {
    return json_extract_string(json, json_len, "type", dest, dest_len);
}

static inline bool json_line_too_long(size_t len) {
    return len > JSON_MAX_LINE_LEN;
}

#endif
