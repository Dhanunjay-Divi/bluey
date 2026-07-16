#ifndef BLUEY_ANSWER_SNAPSHOT_RECOVERY_H
#define BLUEY_ANSWER_SNAPSHOT_RECOVERY_H

#include <stdbool.h>
#include <stddef.h>
#include <wchar.h>

static inline bool bluey_wide_value_fits(const wchar_t *value, size_t capacity) {
    return value && capacity > 0 && wcslen(value) < capacity;
}

static inline void bluey_copy_wide_value(
    wchar_t *destination,
    const wchar_t *value
) {
    size_t length = wcslen(value);
    wmemcpy(destination, value, length + 1);
}

/// Reconstruct the presentation fields that a normal answer `push_card`
/// establishes when the first frame seen by a restarted overlay is an
/// authoritative `update_card` snapshot.
static inline bool recover_answer_snapshot_state(
    const wchar_t *incoming_card_id,
    wchar_t *card_id,
    size_t card_id_capacity,
    wchar_t *kind,
    size_t kind_capacity,
    wchar_t *title,
    size_t title_capacity,
    wchar_t *source,
    size_t source_capacity,
    int *sent_chip_count,
    int *recovery_mode
) {
    static const wchar_t answer_kind[] = L"answer";
    static const wchar_t answer_title[] = L"Bluey";
    static const wchar_t answer_source[] = L"Bluey answer stream";

    if (!card_id || !kind || !title || !source || !sent_chip_count || !recovery_mode
        || !incoming_card_id || incoming_card_id[0] == L'\0'
        || !bluey_wide_value_fits(incoming_card_id, card_id_capacity)
        || !bluey_wide_value_fits(answer_kind, kind_capacity)
        || !bluey_wide_value_fits(answer_title, title_capacity)
        || !bluey_wide_value_fits(answer_source, source_capacity)) {
        return false;
    }

    bluey_copy_wide_value(card_id, incoming_card_id);
    bluey_copy_wide_value(kind, answer_kind);
    bluey_copy_wide_value(title, answer_title);
    bluey_copy_wide_value(source, answer_source);
    *sent_chip_count = 0;
    *recovery_mode = 0;
    return true;
}

#endif
