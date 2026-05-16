/*
 * json_type_extract.h - Safe top-level JSON "type" field extractor.
 *
 * Replaces unsafe strstr-based JSON parsing. Properly handles escaped
 * strings so that a "type" key nested inside a string value cannot be
 * confused with the top-level "type" field.
 *
 * Usage:
 *   char type_buf[128];
 *   if (json_extract_type(line, strlen(line), type_buf, sizeof(type_buf))) {
 *       // type_buf contains the value of the top-level "type" field
 *   }
 *
 * Also provides json_extract_string for extracting other top-level string fields.
 *
 * Length limit: rejects lines longer than JSON_MAX_LINE_LEN.
 */
#ifndef JSON_TYPE_EXTRACT_H
#define JSON_TYPE_EXTRACT_H

#include <string.h>
#include <stdbool.h>
#include <stddef.h>

#define JSON_MAX_LINE_LEN 131072  /* 128 KB max line */
#define JSON_MAX_TEXT_LEN 65536   /* 64 KB max text field */
#define JSON_MAX_FIELD_LEN 16384  /* 16 KB max general field */

/* Skip whitespace, return pointer to next non-whitespace or end. */
static inline const char *json_skip_ws(const char *p, const char *end) {
    while (p < end && (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n'))
        p++;
    return p;
}

/* Skip a JSON string (starting AFTER the opening quote).
 * Returns pointer past the closing quote, or NULL on error. */
static inline const char *json_skip_string(const char *p, const char *end) {
    while (p < end) {
        if (*p == '\\') {
            p += 2; /* skip escaped char */
        } else if (*p == '"') {
            return p + 1;
        } else {
            p++;
        }
    }
    return NULL; /* unterminated string */
}

/* Skip a JSON value (string, number, object, array, bool, null).
 * Returns pointer past the value, or NULL on error. */
static inline const char *json_skip_value(const char *p, const char *end) {
    p = json_skip_ws(p, end);
    if (p >= end) return NULL;

    switch (*p) {
    case '"':
        return json_skip_string(p + 1, end);
    case '{': {
        int depth = 1;
        p++;
        while (p < end && depth > 0) {
            if (*p == '"') {
                p = json_skip_string(p + 1, end);
                if (!p) return NULL;
            } else {
                if (*p == '{') depth++;
                else if (*p == '}') depth--;
                p++;
            }
        }
        return (depth == 0) ? p : NULL;
    }
    case '[': {
        int depth = 1;
        p++;
        while (p < end && depth > 0) {
            if (*p == '"') {
                p = json_skip_string(p + 1, end);
                if (!p) return NULL;
            } else {
                if (*p == '[') depth++;
                else if (*p == ']') depth--;
                p++;
            }
        }
        return (depth == 0) ? p : NULL;
    }
    default:
        /* number, true, false, null - skip until delimiter */
        while (p < end && *p != ',' && *p != '}' && *p != ']'
               && *p != ' ' && *p != '\t' && *p != '\r' && *p != '\n')
            p++;
        return p;
    }
}

/* Extract a top-level string field value from a JSON object.
 * Only looks at the FIRST level of keys (not nested objects).
 * Returns true if found, with value copied to dest (unescaped basic sequences).
 * dest_len includes null terminator space. */
static inline bool json_extract_string(const char *json, size_t json_len,
                                        const char *key, char *dest, size_t dest_len) {
    if (!json || json_len == 0 || !key || !dest || dest_len == 0) return false;
    if (json_len > JSON_MAX_LINE_LEN) return false;

    const char *end = json + json_len;
    const char *p = json_skip_ws(json, end);
    if (p >= end || *p != '{') return false;
    p++;

    size_t key_len = strlen(key);

    while (p < end) {
        p = json_skip_ws(p, end);
        if (p >= end || *p == '}') break;
        if (*p == ',') { p++; continue; }

        /* Expect a key string */
        if (*p != '"') return false;
        p++;
        const char *key_start = p;
        const char *key_end_ptr = json_skip_string(p, end);
        if (!key_end_ptr) return false;
        size_t this_key_len = (size_t)(key_end_ptr - 1 - key_start);

        p = json_skip_ws(key_end_ptr, end);
        if (p >= end || *p != ':') return false;
        p++;
        p = json_skip_ws(p, end);

        /* Check if this is our target key */
        bool is_match = (this_key_len == key_len && memcmp(key_start, key, key_len) == 0);

        if (is_match && p < end && *p == '"') {
            /* Extract the string value */
            p++; /* skip opening quote */
            size_t di = 0;
            while (p < end && *p != '"') {
                if (*p == '\\' && (p + 1) < end) {
                    p++;
                    char c = *p;
                    switch (c) {
                    case '"': case '\\': case '/': break;
                    case 'n': c = '\n'; break;
                    case 'r': c = '\r'; break;
                    case 't': c = '\t'; break;
                    default: break; /* skip \uXXXX etc */
                    }
                    if (di < dest_len - 1) dest[di++] = c;
                    p++;
                } else {
                    if (di < dest_len - 1) dest[di++] = *p;
                    p++;
                }
            }
            dest[di] = '\0';
            return true;
        } else {
            /* Skip this value */
            const char *next = json_skip_value(p, end);
            if (!next) return false;
            p = next;
        }
    }
    return false;
}

/* Extract the top-level "type" field from a JSON line. */
static inline bool json_extract_type(const char *json, size_t json_len,
                                      char *dest, size_t dest_len) {
    return json_extract_string(json, json_len, "type", dest, dest_len);
}

/* Check if a line exceeds the maximum allowed length. */
static inline bool json_line_too_long(size_t len) {
    return len > JSON_MAX_LINE_LEN;
}

#endif /* JSON_TYPE_EXTRACT_H */
