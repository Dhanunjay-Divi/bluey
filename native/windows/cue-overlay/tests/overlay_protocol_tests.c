#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>

#include "../answer_snapshot_recovery.h"
#include "../ask_event_protocol.h"
#include "../json_type_extract.h"
#include "../meeting_banner_protocol.h"
#include "../meeting_detection_protocol.h"
#include "../ndjson_stream.h"

#define MAX_CAPTURED_RECORDS 8

typedef struct RecordCapture {
    char *records[MAX_CAPTURED_RECORDS];
    size_t lengths[MAX_CAPTURED_RECORDS];
    size_t count;
} RecordCapture;

static void fail_check(const char *expression, int line) {
    fprintf(stderr, "check failed at line %d: %s\n", line, expression);
    exit(1);
}

#define CHECK(expression) do { if (!(expression)) fail_check(#expression, __LINE__); } while (0)

static bool capture_record(const char *record, size_t record_len, void *context) {
    RecordCapture *capture = (RecordCapture *)context;
    if (capture->count >= MAX_CAPTURED_RECORDS) return false;
    char *copy = (char *)malloc(record_len + 1);
    if (!copy) return false;
    memcpy(copy, record, record_len);
    copy[record_len] = '\0';
    capture->records[capture->count] = copy;
    capture->lengths[capture->count] = record_len;
    capture->count++;
    return true;
}

static void dispose_capture(RecordCapture *capture) {
    for (size_t i = 0; i < capture->count; i++) free(capture->records[i]);
    memset(capture, 0, sizeof(*capture));
}

static char *make_body(size_t body_len, unsigned seed) {
    static const unsigned char rocket_utf8[] = {0xf0u, 0x9fu, 0x9au, 0x80u};
    char *body = (char *)malloc(body_len + 1);
    CHECK(body != NULL);
    for (size_t i = 0; i < body_len; i++) {
        body[i] = (char)('a' + (int)((i + seed) % 26));
    }
    if (body_len >= sizeof(rocket_utf8)) {
        memcpy(body + body_len - sizeof(rocket_utf8), rocket_utf8, sizeof(rocket_utf8));
    }
    body[body_len] = '\0';
    return body;
}

static char *make_push_card_record(
    const char *body,
    size_t body_len,
    bool crlf,
    size_t *record_len
) {
    static const char prefix[] =
        "{\"type\":\"push_card\",\"card\":{\"id\":\"card-1\",\"kind\":\"answer\","
        "\"title\":\"Large answer\",\"body\":\"";
    static const char suffix[] = "\"}}";
    size_t newline_len = crlf ? 2 : 1;
    size_t total = sizeof(prefix) - 1 + body_len + sizeof(suffix) - 1 + newline_len;
    char *record = (char *)malloc(total + 1);
    CHECK(record != NULL);

    size_t offset = 0;
    memcpy(record + offset, prefix, sizeof(prefix) - 1);
    offset += sizeof(prefix) - 1;
    memcpy(record + offset, body, body_len);
    offset += body_len;
    memcpy(record + offset, suffix, sizeof(suffix) - 1);
    offset += sizeof(suffix) - 1;
    if (crlf) record[offset++] = '\r';
    record[offset++] = '\n';
    record[offset] = '\0';
    *record_len = offset;
    return record;
}

static bool extract_card_body(
    const char *record,
    size_t record_len,
    char **body,
    size_t *body_len
) {
    const char *card = NULL;
    size_t card_len = 0;
    return json_extract_object(record, record_len, "card", &card, &card_len)
        && json_extract_string_alloc(card, card_len, "body", body, body_len);
}

static void check_captured_body(
    const RecordCapture *capture,
    size_t index,
    const char *expected,
    size_t expected_len
) {
    CHECK(index < capture->count);
    char type[32];
    CHECK(json_extract_type(
        capture->records[index], capture->lengths[index], type, sizeof(type)));
    CHECK(strcmp(type, "push_card") == 0);

    char *body = NULL;
    size_t body_len = 0;
    CHECK(extract_card_body(
        capture->records[index], capture->lengths[index], &body, &body_len));
    CHECK(body_len == expected_len);
    CHECK(memcmp(body, expected, expected_len) == 0);
    free(body);
}

static void test_answer_over_2k(void) {
    char *body = make_body(3072, 3);
    size_t record_len = 0;
    char *record = make_push_card_record(body, 3072, false, &record_len);
    NdjsonStream stream;
    RecordCapture capture = {0};
    ndjson_stream_init(&stream, JSON_MAX_LINE_LEN);

    CHECK(ndjson_stream_feed(
        &stream, record, record_len, capture_record, &capture) == NDJSON_FEED_OK);
    CHECK(capture.count == 1);
    check_captured_body(&capture, 0, body, 3072);

    ndjson_stream_dispose(&stream);
    dispose_capture(&capture);
    free(record);
    free(body);
}

static void test_answer_over_8k_fragmented(void) {
    static const size_t fragments[] = {1, 7, 2047, 3, 8191, 11, 4096, 2, 29};
    char *body = make_body(20000, 9);
    size_t record_len = 0;
    char *record = make_push_card_record(body, 20000, true, &record_len);
    NdjsonStream stream;
    RecordCapture capture = {0};
    ndjson_stream_init(&stream, JSON_MAX_LINE_LEN);

    size_t offset = 0;
    size_t fragment_index = 0;
    while (offset < record_len) {
        size_t fragment_len = fragments[fragment_index % (sizeof(fragments) / sizeof(fragments[0]))];
        if (fragment_len > record_len - offset) fragment_len = record_len - offset;
        CHECK(ndjson_stream_feed(
            &stream,
            record + offset,
            fragment_len,
            capture_record,
            &capture) == NDJSON_FEED_OK);
        offset += fragment_len;
        fragment_index++;
    }
    CHECK(capture.count == 1);
    check_captured_body(&capture, 0, body, 20000);

    ndjson_stream_dispose(&stream);
    dispose_capture(&capture);
    free(record);
    free(body);
}

static void test_coalesced_large_records(void) {
    char *first_body = make_body(2500, 2);
    char *second_body = make_body(12000, 5);
    size_t first_len = 0;
    size_t second_len = 0;
    char *first = make_push_card_record(first_body, 2500, false, &first_len);
    char *second = make_push_card_record(second_body, 12000, false, &second_len);
    char *coalesced = (char *)malloc(first_len + second_len);
    CHECK(coalesced != NULL);
    memcpy(coalesced, first, first_len);
    memcpy(coalesced + first_len, second, second_len);

    NdjsonStream stream;
    RecordCapture capture = {0};
    ndjson_stream_init(&stream, JSON_MAX_LINE_LEN);
    CHECK(ndjson_stream_feed(
        &stream,
        coalesced,
        first_len + second_len,
        capture_record,
        &capture) == NDJSON_FEED_OK);
    CHECK(capture.count == 2);
    check_captured_body(&capture, 0, first_body, 2500);
    check_captured_body(&capture, 1, second_body, 12000);

    ndjson_stream_dispose(&stream);
    dispose_capture(&capture);
    free(coalesced);
    free(second);
    free(first);
    free(second_body);
    free(first_body);
}

static void test_unicode_escapes_and_final_unterminated_record(void) {
    static const char record[] =
        "{\"type\":\"push_card\",\"card\":{\"kind\":\"answer\","
        "\"body\":\"Hi \\u263a \\ud83d\\ude80\"}}";
    static const unsigned char expected[] = {
        'H', 'i', ' ', 0xe2, 0x98, 0xba, ' ', 0xf0, 0x9f, 0x9a, 0x80
    };
    NdjsonStream stream;
    RecordCapture capture = {0};
    ndjson_stream_init(&stream, JSON_MAX_LINE_LEN);

    CHECK(ndjson_stream_feed(
        &stream, record, sizeof(record) - 1, capture_record, &capture) == NDJSON_FEED_OK);
    CHECK(capture.count == 0);
    CHECK(ndjson_stream_finish(&stream, capture_record, &capture) == NDJSON_FEED_OK);
    CHECK(capture.count == 1);
    check_captured_body(
        &capture, 0, (const char *)expected, sizeof(expected));

    ndjson_stream_dispose(&stream);
    dispose_capture(&capture);
}

static void test_oversized_record_recovery(void) {
    char oversized[1501];
    memset(oversized, 'x', sizeof(oversized));
    oversized[sizeof(oversized) - 1] = '\n';
    static const char ping[] = "{\"type\":\"ping\"}\n";
    char *input = (char *)malloc(sizeof(oversized) + sizeof(ping) - 1);
    CHECK(input != NULL);
    memcpy(input, oversized, sizeof(oversized));
    memcpy(input + sizeof(oversized), ping, sizeof(ping) - 1);

    NdjsonStream stream;
    RecordCapture capture = {0};
    ndjson_stream_init(&stream, 1024);
    CHECK(ndjson_stream_feed(
        &stream, input, 600, capture_record, &capture) == NDJSON_FEED_OK);
    CHECK(ndjson_stream_feed(
        &stream,
        input + 600,
        sizeof(oversized) + sizeof(ping) - 1 - 600,
        capture_record,
        &capture) == NDJSON_FEED_OK);
    CHECK(stream.rejected_records == 1);
    CHECK(capture.count == 1);
    char type[16];
    CHECK(json_extract_type(capture.records[0], capture.lengths[0], type, sizeof(type)));
    CHECK(strcmp(type, "ping") == 0);

    ndjson_stream_dispose(&stream);
    dispose_capture(&capture);
    free(input);
}

static void test_top_level_context_array_and_nested_decoys(void) {
    static const char context_record[] =
        "{\"note\":\"\\\"items\\\":[{\\\"id\\\":\\\"wrong\\\"}]\","
        "\"type\":\"set_context_items\",\"items\":[{"
        "\"id\":\"01234567-89ab-cdef-0123-456789abcdef\","
        "\"title\":\"Resume \\u263a.pdf\",\"kind\":\"document\"}]}";
    const char *array = NULL;
    size_t array_len = 0;
    CHECK(json_extract_array(
        context_record, sizeof(context_record) - 1, "items", &array, &array_len));
    const char *object = json_skip_ws(array + 1, array + array_len - 1);
    const char *object_end = json_skip_value(object, array + array_len - 1);
    CHECK(object_end != NULL);

    char id[80];
    CHECK(json_extract_string(
        object, (size_t)(object_end - object), "id", id, sizeof(id)));
    CHECK(strcmp(id, "01234567-89ab-cdef-0123-456789abcdef") == 0);
    char *title = NULL;
    size_t title_len = 0;
    CHECK(json_extract_string_alloc(
        object, (size_t)(object_end - object), "title", &title, &title_len));
    static const unsigned char expected_title[] = {
        'R', 'e', 's', 'u', 'm', 'e', ' ', 0xe2, 0x98, 0xba, '.', 'p', 'd', 'f'
    };
    CHECK(title_len == sizeof(expected_title));
    CHECK(memcmp(title, expected_title, sizeof(expected_title)) == 0);
    free(title);
}

static void test_nested_artifact_and_optional_session_fields(void) {
    static const char card_record[] =
        "{\"type\":\"push_card\",\"card\":{\"kind\":\"answer\","
        "\"artifact\":{\"artifact_type\":\"code\",\"title\":\"Code canvas\","
        "\"body\":\"first line\\nsecond line\",\"confidence\":0.91}}}";
    const char *card = NULL;
    size_t card_len = 0;
    const char *artifact = NULL;
    size_t artifact_len = 0;
    CHECK(json_extract_object(
        card_record, sizeof(card_record) - 1, "card", &card, &card_len));
    CHECK(json_extract_object(card, card_len, "artifact", &artifact, &artifact_len));
    char *artifact_body = NULL;
    size_t artifact_body_len = 0;
    CHECK(json_extract_string_alloc(
        artifact, artifact_len, "body", &artifact_body, &artifact_body_len));
    CHECK(artifact_body_len == strlen("first line\nsecond line"));
    CHECK(memcmp(
        artifact_body,
        "first line\nsecond line",
        artifact_body_len) == 0);
    free(artifact_body);

    static const char inactive_session[] =
        "{\"type\":\"set_active_session\",\"code\":\"\",\"title\":\"No active session\"}";
    char id[80] = "stale-session-id";
    CHECK(!json_extract_string(
        inactive_session, sizeof(inactive_session) - 1, "id", id, sizeof(id)));
    CHECK(id[0] == '\0');
}

static void test_answer_current_transcript_ask_field(void) {
    CHECK(!ask_event_answers_current_transcript(false, false));
    CHECK(ask_event_answers_current_transcript(false, true));
    CHECK(ask_event_answers_current_transcript(true, false));
    CHECK(ask_event_answers_current_transcript(true, true));
    CHECK(strcmp(ask_event_current_transcript_json_field(false), "") == 0);
    CHECK(strcmp(
        ask_event_current_transcript_json_field(true),
        ",\"answer_current_transcript\":true") == 0);

    char payload[160];
    int payload_len = snprintf(
        payload,
        sizeof(payload),
        "{\"type\":\"ask_requested\",\"question\":\"listen\"%s}",
        ask_event_current_transcript_json_field(true));
    CHECK(payload_len > 0 && (size_t)payload_len < sizeof(payload));
    const char *value = NULL;
    size_t value_len = 0;
    CHECK(json_find_top_level_value(
        payload,
        (size_t)payload_len,
        "answer_current_transcript",
        &value,
        &value_len));
    CHECK(value_len == 4 && memcmp(value, "true", 4) == 0);

    payload_len = snprintf(
        payload,
        sizeof(payload),
        "{\"type\":\"ask_requested\",\"question\":\"typed\"%s}",
        ask_event_current_transcript_json_field(false));
    CHECK(payload_len > 0 && (size_t)payload_len < sizeof(payload));
    CHECK(!json_find_top_level_value(
        payload,
        (size_t)payload_len,
        "answer_current_transcript",
        &value,
        &value_len));

    static const char correlated_ask[] =
        "{\"type\":\"ask_requested\",\"question\":\"typed\","
        "\"interaction_id\":\"550e8400-e29b-41d4-a716-446655440000\","
        "\"initiated_at_unix_ms\":1750000000123}";
    char interaction_id[40];
    CHECK(json_extract_string(
        correlated_ask,
        sizeof(correlated_ask) - 1,
        "interaction_id",
        interaction_id,
        sizeof(interaction_id)));
    CHECK(strcmp(interaction_id, "550e8400-e29b-41d4-a716-446655440000") == 0);
    CHECK(json_find_top_level_value(
        correlated_ask,
        sizeof(correlated_ask) - 1,
        "initiated_at_unix_ms",
        &value,
        &value_len));
    CHECK(value_len == strlen("1750000000123"));
    CHECK(memcmp(value, "1750000000123", value_len) == 0);
}

static void test_fresh_state_answer_snapshot_contract(void) {
    static const char snapshot[] =
        "{\"type\":\"update_card\",\"id\":\"answer-42\","
        "\"interaction_id\":\"550e8400-e29b-41d4-a716-446655440000\","
        "\"body\":\"Recovered answer\",\"done\":false,"
        "\"sequence\":7,\"snapshot\":true,\"render_ack\":\"final\"}";
    char type[32];
    char id[80];
    char interaction_id[40];
    char render_ack[24];
    char *body = NULL;
    size_t body_len = 0;
    const char *snapshot_value = NULL;
    size_t snapshot_len = 0;
    double sequence = 0.0;

    CHECK(json_extract_type(snapshot, sizeof(snapshot) - 1, type, sizeof(type)));
    CHECK(strcmp(type, "update_card") == 0);
    CHECK(json_extract_string(
        snapshot, sizeof(snapshot) - 1, "id", id, sizeof(id)));
    CHECK(strcmp(id, "answer-42") == 0);
    CHECK(json_extract_string(
        snapshot,
        sizeof(snapshot) - 1,
        "interaction_id",
        interaction_id,
        sizeof(interaction_id)));
    CHECK(strcmp(interaction_id, "550e8400-e29b-41d4-a716-446655440000") == 0);
    CHECK(json_extract_string(
        snapshot,
        sizeof(snapshot) - 1,
        "render_ack",
        render_ack,
        sizeof(render_ack)));
    CHECK(strcmp(render_ack, "final") == 0);
    CHECK(json_extract_string_alloc(
        snapshot, sizeof(snapshot) - 1, "body", &body, &body_len));
    CHECK(body_len == strlen("Recovered answer"));
    CHECK(memcmp(body, "Recovered answer", body_len) == 0);
    CHECK(json_extract_number(
        snapshot, sizeof(snapshot) - 1, "sequence", &sequence));
    CHECK(sequence == 7.0);
    CHECK(json_find_top_level_value(
        snapshot,
        sizeof(snapshot) - 1,
        "snapshot",
        &snapshot_value,
        &snapshot_len));
    CHECK(snapshot_len == 4 && memcmp(snapshot_value, "true", 4) == 0);
    free(body);

    wchar_t recovered_id[80] = L"";
    wchar_t recovered_kind[64] = L"system";
    wchar_t recovered_title[256] = L"Stale title";
    wchar_t recovered_source[256] = L"stale-provider";
    int sent_chip_count = 3;
    int recovery_mode = 2;
    CHECK(recover_answer_snapshot_state(
        L"answer-42",
        recovered_id,
        80,
        recovered_kind,
        64,
        recovered_title,
        256,
        recovered_source,
        256,
        &sent_chip_count,
        &recovery_mode));
    CHECK(wcscmp(recovered_id, L"answer-42") == 0);
    CHECK(wcscmp(recovered_kind, L"answer") == 0);
    CHECK(wcscmp(recovered_title, L"Bluey") == 0);
    CHECK(wcscmp(recovered_source, L"Bluey answer stream") == 0);
    CHECK(sent_chip_count == 0);
    CHECK(recovery_mode == 0);
}

static void test_answer_render_acknowledgement_contract(void) {
    static const char acknowledgement[] =
        "{\"type\":\"answer_render_acknowledged\","
        "\"id\":\"00000000-0000-0000-0000-000000000001\","
        "\"interaction_id\":\"550e8400-e29b-41d4-a716-446655440000\","
        "\"phase\":\"first_text\",\"sequence\":9}";
    char type[48];
    char id[40];
    char interaction_id[40];
    char phase[24];
    double sequence = 0.0;

    CHECK(json_extract_type(
        acknowledgement,
        sizeof(acknowledgement) - 1,
        type,
        sizeof(type)));
    CHECK(strcmp(type, "answer_render_acknowledged") == 0);
    CHECK(json_extract_string(
        acknowledgement,
        sizeof(acknowledgement) - 1,
        "id",
        id,
        sizeof(id)));
    CHECK(strcmp(id, "00000000-0000-0000-0000-000000000001") == 0);
    CHECK(json_extract_string(
        acknowledgement,
        sizeof(acknowledgement) - 1,
        "interaction_id",
        interaction_id,
        sizeof(interaction_id)));
    CHECK(strcmp(interaction_id, "550e8400-e29b-41d4-a716-446655440000") == 0);
    CHECK(json_extract_string(
        acknowledgement,
        sizeof(acknowledgement) - 1,
        "phase",
        phase,
        sizeof(phase)));
    CHECK(strcmp(phase, "first_text") == 0);
    CHECK(json_extract_number(
        acknowledgement,
        sizeof(acknowledgement) - 1,
        "sequence",
        &sequence));
    CHECK(sequence == 9.0);
}

typedef struct RenderAckPolicySlot {
    bool active;
    unsigned requested_sequence;
    unsigned paint_sequence;
} RenderAckPolicySlot;

typedef struct RenderAckPolicyState {
    char card_id[40];
    char interaction_id[40];
    unsigned current_sequence;
    RenderAckPolicySlot first_text;
    RenderAckPolicySlot final;
} RenderAckPolicyState;

static void apply_render_ack_policy_update(
    RenderAckPolicyState *state,
    const char *card_id,
    const char *interaction_id,
    const char *phase,
    unsigned sequence
) {
    CHECK(state != NULL);
    CHECK(card_id != NULL);
    CHECK(interaction_id != NULL);
    bool same_generation = state->card_id[0] != '\0'
        && strcmp(state->card_id, card_id) == 0
        && strcmp(state->interaction_id, interaction_id) == 0;
    if (!same_generation) {
        memset(state, 0, sizeof(*state));
        CHECK(strlen(card_id) < sizeof(state->card_id));
        CHECK(strlen(interaction_id) < sizeof(state->interaction_id));
        strcpy(state->card_id, card_id);
        strcpy(state->interaction_id, interaction_id);
    } else if (sequence <= state->current_sequence) {
        return;
    }
    state->current_sequence = sequence;

    RenderAckPolicySlot *slots[] = {&state->first_text, &state->final};
    for (size_t index = 0; index < sizeof(slots) / sizeof(slots[0]); index++) {
        if (slots[index]->active) slots[index]->paint_sequence = sequence;
    }

    RenderAckPolicySlot *requested = NULL;
    if (phase && strcmp(phase, "first_text") == 0) requested = &state->first_text;
    else if (phase && strcmp(phase, "final") == 0) requested = &state->final;
    if (requested) {
        requested->active = true;
        requested->requested_sequence = sequence;
        requested->paint_sequence = sequence;
    }
}

static void test_render_ack_survives_superseding_non_ack_updates(void) {
    static const char card_id[] = "00000000-0000-0000-0000-000000000001";
    static const char interaction_id[] = "550e8400-e29b-41d4-a716-446655440000";
    RenderAckPolicyState state = {0};

    apply_render_ack_policy_update(
        &state, card_id, interaction_id, "first_text", 3);
    apply_render_ack_policy_update(&state, card_id, interaction_id, NULL, 4);
    CHECK(state.first_text.active);
    CHECK(state.first_text.requested_sequence == 3);
    CHECK(state.first_text.paint_sequence == 4);
    CHECK(!state.final.active);

    apply_render_ack_policy_update(&state, card_id, interaction_id, "final", 5);
    CHECK(state.first_text.active);
    CHECK(state.first_text.requested_sequence == 3);
    CHECK(state.first_text.paint_sequence == 5);
    CHECK(state.final.active);
    CHECK(state.final.requested_sequence == 5);
    CHECK(state.final.paint_sequence == 5);

    apply_render_ack_policy_update(&state, card_id, interaction_id, NULL, 4);
    CHECK(state.current_sequence == 5);
    CHECK(state.first_text.paint_sequence == 5);
    CHECK(state.final.paint_sequence == 5);

    apply_render_ack_policy_update(
        &state,
        "00000000-0000-0000-0000-000000000002",
        "550e8400-e29b-41d4-a716-446655440001",
        NULL,
        1);
    CHECK(!state.first_text.active);
    CHECK(!state.final.active);
}

static void test_meeting_banner_timeout_is_expired_not_dismissed(void) {
    CHECK(strcmp(bluey_meeting_banner_timeout_action(), "expired") == 0);
    CHECK(strcmp(bluey_meeting_banner_timeout_action(), "dismiss") != 0);
}

static void test_meeting_detection_enabled_command_requires_boolean(void) {
    static const char enabled[] =
        "{\"type\":\"set_meeting_detection_enabled\",\"enabled\":true}";
    static const char disabled[] =
        "{\"type\":\"set_meeting_detection_enabled\",\"enabled\":false}";
    static const char missing[] =
        "{\"type\":\"set_meeting_detection_enabled\"}";
    static const char invalid[] =
        "{\"type\":\"set_meeting_detection_enabled\",\"enabled\":\"false\"}";
    bool value = false;

    CHECK(!bluey_meeting_detection_default_enabled());
    CHECK(bluey_parse_meeting_detection_enabled(
        enabled, sizeof(enabled) - 1, &value));
    CHECK(value);
    CHECK(bluey_parse_meeting_detection_enabled(
        disabled, sizeof(disabled) - 1, &value));
    CHECK(!value);
    CHECK(!bluey_parse_meeting_detection_enabled(
        missing, sizeof(missing) - 1, &value));
    CHECK(!bluey_parse_meeting_detection_enabled(
        invalid, sizeof(invalid) - 1, &value));
}

int main(void) {
    test_answer_over_2k();
    test_answer_over_8k_fragmented();
    test_coalesced_large_records();
    test_unicode_escapes_and_final_unterminated_record();
    test_oversized_record_recovery();
    test_top_level_context_array_and_nested_decoys();
    test_nested_artifact_and_optional_session_fields();
    test_answer_current_transcript_ask_field();
    test_fresh_state_answer_snapshot_contract();
    test_answer_render_acknowledgement_contract();
    test_render_ack_survives_superseding_non_ack_updates();
    test_meeting_banner_timeout_is_expired_not_dismissed();
    test_meeting_detection_enabled_command_requires_boolean();
    puts("overlay protocol tests passed");
    return 0;
}
