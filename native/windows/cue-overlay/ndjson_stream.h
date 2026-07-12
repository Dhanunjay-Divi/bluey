#ifndef BLUEY_NDJSON_STREAM_H
#define BLUEY_NDJSON_STREAM_H

#include <stdbool.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>

typedef bool (*NdjsonRecordCallback)(const char *record, size_t record_len, void *context);

typedef enum NdjsonFeedResult {
    NDJSON_FEED_OK = 0,
    NDJSON_FEED_STOPPED,
    NDJSON_FEED_OUT_OF_MEMORY,
} NdjsonFeedResult;

typedef struct NdjsonStream {
    char *buffer;
    size_t length;
    size_t capacity;
    size_t max_record_bytes;
    size_t rejected_records;
    bool discarding_record;
} NdjsonStream;

static inline void ndjson_stream_init(NdjsonStream *stream, size_t max_record_bytes) {
    if (!stream) return;
    memset(stream, 0, sizeof(*stream));
    stream->max_record_bytes = max_record_bytes;
}

static inline void ndjson_stream_dispose(NdjsonStream *stream) {
    if (!stream) return;
    free(stream->buffer);
    memset(stream, 0, sizeof(*stream));
}

static inline bool ndjson_stream_reserve(NdjsonStream *stream, size_t needed) {
    if (needed <= stream->capacity) return true;
    if (needed == 0 || needed - 1 > stream->max_record_bytes) return false;

    size_t capacity = stream->capacity > 0 ? stream->capacity : 4096;
    size_t max_capacity = stream->max_record_bytes + 1;
    while (capacity < needed) {
        if (capacity >= max_capacity / 2) {
            capacity = max_capacity;
            break;
        }
        capacity *= 2;
    }
    if (capacity < needed) return false;

    char *grown = (char *)realloc(stream->buffer, capacity);
    if (!grown) return false;
    stream->buffer = grown;
    stream->capacity = capacity;
    return true;
}

static inline NdjsonFeedResult ndjson_stream_emit(
    NdjsonStream *stream,
    NdjsonRecordCallback callback,
    void *context
) {
    size_t record_len = stream->length;
    if (record_len > 0 && stream->buffer[record_len - 1] == '\r') {
        record_len--;
    }
    stream->length = 0;
    if (record_len == 0) return NDJSON_FEED_OK;
    stream->buffer[record_len] = '\0';
    return callback(stream->buffer, record_len, context)
        ? NDJSON_FEED_OK
        : NDJSON_FEED_STOPPED;
}

static inline NdjsonFeedResult ndjson_stream_feed(
    NdjsonStream *stream,
    const char *bytes,
    size_t byte_count,
    NdjsonRecordCallback callback,
    void *context
) {
    if (!stream || (!bytes && byte_count > 0) || !callback) return NDJSON_FEED_STOPPED;
    if (byte_count == 0) return NDJSON_FEED_OK;

    const char *cursor = bytes;
    const char *end = bytes + byte_count;
    while (cursor < end) {
        const char *newline = (const char *)memchr(cursor, '\n', (size_t)(end - cursor));
        const char *segment_end = newline ? newline : end;
        size_t segment_len = (size_t)(segment_end - cursor);

        if (stream->discarding_record) {
            if (!newline) return NDJSON_FEED_OK;
            stream->discarding_record = false;
            cursor = newline + 1;
            continue;
        }

        if (segment_len > stream->max_record_bytes - stream->length) {
            stream->length = 0;
            stream->rejected_records++;
            if (!newline) {
                stream->discarding_record = true;
                return NDJSON_FEED_OK;
            }
            cursor = newline + 1;
            continue;
        }

        size_t new_length = stream->length + segment_len;
        if (!ndjson_stream_reserve(stream, new_length + 1)) {
            return NDJSON_FEED_OUT_OF_MEMORY;
        }
        if (segment_len > 0) {
            memcpy(stream->buffer + stream->length, cursor, segment_len);
        }
        stream->length = new_length;
        stream->buffer[stream->length] = '\0';

        if (!newline) return NDJSON_FEED_OK;
        NdjsonFeedResult result = ndjson_stream_emit(stream, callback, context);
        if (result != NDJSON_FEED_OK) return result;
        cursor = newline + 1;
    }
    return NDJSON_FEED_OK;
}

static inline NdjsonFeedResult ndjson_stream_finish(
    NdjsonStream *stream,
    NdjsonRecordCallback callback,
    void *context
) {
    if (!stream || !callback) return NDJSON_FEED_STOPPED;
    if (stream->discarding_record) {
        stream->discarding_record = false;
        stream->length = 0;
        return NDJSON_FEED_OK;
    }
    return ndjson_stream_emit(stream, callback, context);
}

#endif
