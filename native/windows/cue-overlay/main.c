#ifndef UNICODE
#define UNICODE
#endif

#ifndef _UNICODE
#define _UNICODE
#endif

#ifndef WINVER
#define WINVER 0x0601
#endif

#ifndef _WIN32_WINNT
#define _WIN32_WINNT 0x0601
#endif

#ifndef _WIN32_IE
#define _WIN32_IE 0x0600
#endif

#ifndef __cplusplus
#define COBJMACROS
#endif
#include <windows.h>
#include <windowsx.h>
#include <shellapi.h>
#include <commctrl.h>
#include <tlhelp32.h>
#include <mmdeviceapi.h>
#include <audiopolicy.h>

#ifdef DrawText
#undef DrawText
#endif

#include <initguid.h>
#include <d2d1.h>
#include <dwrite.h>
#include <limits.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>
#include <wctype.h>
#include "answer_snapshot_recovery.h"
#include "meeting_banner_protocol.h"
#include "meeting_detection_protocol.h"
#include "ask_event_protocol.h"
#include "json_type_extract.h"
#include "ndjson_stream.h"

#ifndef WDA_EXCLUDEFROMCAPTURE
#define WDA_EXCLUDEFROMCAPTURE 0x00000011
#endif

#ifndef EM_SETCUEBANNER
#define EM_SETCUEBANNER 0x1501
#endif

static HWND g_hwnd;
static HWND g_ask_edit;
static HWND g_send_button;
static HWND g_record_button;
static HWND g_auto_send_combo;
static HWND g_answer_detail_combo;
static HWND g_transcript_clear_button;
static HWND g_paste_answer_button;
static HWND g_help_button;
static HWND g_session_button;
static HWND g_page_button;
static HWND g_attach_button;
static HWND g_recap_button;
static HWND g_note_button;
static HWND g_theme_button;
static HWND g_shortcuts_button;
static HWND g_close_button;
static HWND g_tooltip;
static HWND g_meeting_banner;
static HWND g_meeting_banner_title;
static HWND g_meeting_banner_app;
static HWND g_meeting_banner_reason;
static HWND g_meeting_banner_countdown;
static HWND g_meeting_banner_start;
static HWND g_meeting_banner_snooze;
static HWND g_meeting_banner_dismiss;
static HWND g_meeting_banner_ignore;
static HWND g_meeting_banner_settings;
static wchar_t g_meeting_candidate_id[260] = L"";
static wchar_t g_meeting_candidate_app_id[260] = L"";
static wchar_t g_meeting_candidate_provider[96] = L"";
static int g_meeting_candidate_confidence = 0;
static ULONGLONG g_meeting_banner_started_ms = 0;
static ULONGLONG g_meeting_banner_deadline_ms = 0;
static HANDLE g_meeting_detector_stop = NULL;
static HANDLE g_meeting_detector_thread = NULL;
static SRWLOCK g_meeting_detector_lock = SRWLOCK_INIT;
static volatile LONG g_meeting_detection_enabled =
    BLUEY_MEETING_DETECTION_DEFAULT_ENABLED;
static wchar_t g_title[256] = L"bluey";
static wchar_t g_initial_body[] = L"Waiting for meeting intelligence...";
static wchar_t *g_body = g_initial_body;
static SRWLOCK g_body_lock = SRWLOCK_INIT;
static wchar_t g_kind[64] = L"system";
static wchar_t g_source[256] = L"";
static wchar_t g_card_id[80] = L"";
static ULONGLONG g_card_update_sequence = 0;
typedef enum BlueyRenderAckPhase {
    BLUEY_RENDER_ACK_NONE = 0,
    BLUEY_RENDER_ACK_FIRST_TEXT = 1,
    BLUEY_RENDER_ACK_FINAL = 2,
} BlueyRenderAckPhase;

typedef struct BlueyPendingRenderAck {
    bool active;
    wchar_t card_id[80];
    char interaction_id[37];
    BlueyRenderAckPhase phase;
    ULONGLONG requested_sequence;
    ULONGLONG paint_sequence;
    ULONGLONG presentation_revision;
    unsigned retry_count;
} BlueyPendingRenderAck;

static SRWLOCK g_render_ack_lock = SRWLOCK_INIT;
static BlueyPendingRenderAck g_pending_first_text_ack;
static BlueyPendingRenderAck g_pending_final_ack;
static ULONGLONG g_presentation_revision = 0;
static wchar_t g_last_question[2048] = L"";
static int g_recovery_mode = 0; /* 0 none, 1 continue partial, 2 retry */
static bool g_visible = true;
static bool g_collapsed = false;
static bool g_interactive_mode = true;
static bool g_recording = false;
static DWORD g_last_record_toggle_ms = 0;
static DWORD g_record_restart_after_ms = 0;
static int g_auto_send_mode = 0;
static int g_answer_detail_mode = 0; /* 0 Auto, 1 Quick, 2 Thorough */
static bool g_auto_send_timer_armed = false;
static bool g_manual_send_timer_armed = false;
static ULONGLONG g_manual_send_started_ms = 0;
static int g_audio_auto_stop_remaining_secs = -1;
static int g_audio_auto_stop_idle_secs = 0;
static bool g_light_theme = false;
static double g_opacity = 0.92;
static const int BLUEY_LIGHT_ACCENT_R = 0;
static const int BLUEY_LIGHT_ACCENT_G = 98;
static const int BLUEY_LIGHT_ACCENT_B = 154;
static const int BLUEY_LIGHT_ACCENT_SOFT_R = 176;
static const int BLUEY_LIGHT_ACCENT_SOFT_G = 226;
static const int BLUEY_LIGHT_ACCENT_SOFT_B = 246;
static RECT g_expanded_rect = {0, 0, 0, 0};
static RECT g_collapsed_rect = {0, 0, 0, 0};
static HHOOK g_popup_hook = NULL;
static RECT g_popup_avoid_rect = {0, 0, 0, 0};
static HBRUSH g_edit_brush = NULL;
static bool g_collapsed_dragging = false;
static bool g_collapsed_drag_moved = false;
static POINT g_collapsed_drag_start = {0, 0};
static RECT g_collapsed_drag_rect = {0, 0, 0, 0};
static bool g_d2d_available = false;
static ID2D1Factory *g_d2d_factory = NULL;
static IDWriteFactory *g_dwrite_factory = NULL;
static ID2D1HwndRenderTarget *g_d2d_target = NULL;
static ID2D1SolidColorBrush *g_d2d_brush = NULL;
static IDWriteTextFormat *g_fmt_pill = NULL;
static IDWriteTextFormat *g_fmt_brand = NULL;
static IDWriteTextFormat *g_fmt_label = NULL;
static IDWriteTextFormat *g_fmt_title = NULL;
static IDWriteTextFormat *g_fmt_body = NULL;
static IDWriteTextFormat *g_fmt_partial = NULL;

#define EXPANDED_MIN_WIDTH 520
#define EXPANDED_MIN_HEIGHT 360
#define EXPANDED_RESIZE_HIT_SIZE 14
#define EXPANDED_SCREEN_MARGIN 12
static wchar_t g_transcript_partial[1024] = L"";
static wchar_t g_transcript_final[1024] = L"";
static wchar_t g_transcript_source[64] = L"";
static wchar_t g_session_banner[256] = L"";
static ULONGLONG g_session_banner_tick = 0;
static wchar_t g_active_session_id[80] = L"";
static wchar_t g_active_session_code[32] = L"";
static wchar_t g_active_session_title[160] = L"";
static WNDPROC g_ask_edit_proc = NULL;

static void send_current_question(void);
static void send_current_question_now(bool listen_triggered);
static void cancel_auto_send_timer(const char *origin);
static void schedule_auto_send_after_caption_settled(void);
static void show_full_overlay(bool emit_event);
static void hide_overlay_completely(bool emit_event);
static void update_paste_answer_button(void);
static void invoke_button_command(int id, HWND control);
static void focus_ask_input(void);
static void toggle_interactive_mode(void);
static bool handle_overlay_shortcut_key(WPARAM key, bool local_key);
static bool focus_next_keyboard_control(bool backward);
static bool activate_focused_keyboard_control(void);
static void hide_meeting_banner(const wchar_t *candidate_id);
static bool safe_extract_json_to_wide(
    const char *json,
    size_t json_len,
    const char *key,
    wchar_t *dest,
    size_t dest_wchars);
static bool safe_extract_json_number(
    const char *json,
    size_t json_len,
    const char *key,
    double *dest);

typedef struct MeetingEvidencePayload {
    wchar_t app_name[160];
    wchar_t app_id[260];
    wchar_t provider[96];
    wchar_t window_title[520];
    DWORD process_id;
    bool audio_input_active;
    bool audio_output_active;
    bool app_foreground;
    bool browser;
    bool dedicated_meeting_app;
    LONGLONG observed_at_unix_ms;
} MeetingEvidencePayload;

static void clear_pending_render_ack(void) {
    AcquireSRWLockExclusive(&g_render_ack_lock);
    ZeroMemory(&g_pending_first_text_ack, sizeof(g_pending_first_text_ack));
    ZeroMemory(&g_pending_final_ack, sizeof(g_pending_final_ack));
    ReleaseSRWLockExclusive(&g_render_ack_lock);
}

static void advance_presentation_revision(void) {
    AcquireSRWLockExclusive(&g_render_ack_lock);
    g_presentation_revision = g_presentation_revision == ULLONG_MAX
        ? 1
        : g_presentation_revision + 1;
    ReleaseSRWLockExclusive(&g_render_ack_lock);
}

static void reset_card_update_sequence(void) {
    g_card_update_sequence = 0;
    clear_pending_render_ack();
}

static bool json_read_optional_bool(
    const char *json,
    size_t json_len,
    const char *key,
    bool *dest
) {
    if (!dest) return false;
    *dest = false;

    const char *value = NULL;
    size_t value_len = 0;
    if (!json_find_top_level_value(json, json_len, key, &value, &value_len)) {
        return true;
    }
    if (value_len == 4 && memcmp(value, "true", 4) == 0) {
        *dest = true;
        return true;
    }
    return value_len == 5 && memcmp(value, "false", 5) == 0;
}

static bool json_read_optional_u64(
    const char *json,
    size_t json_len,
    const char *key,
    ULONGLONG *dest,
    bool *present
) {
    if (!dest || !present) return false;
    *dest = 0;
    *present = false;

    const char *value = NULL;
    size_t value_len = 0;
    if (!json_find_top_level_value(json, json_len, key, &value, &value_len)) {
        return true;
    }
    *present = true;
    if (value_len == 0) return false;

    ULONGLONG parsed = 0;
    for (size_t index = 0; index < value_len; index++) {
        unsigned char digit = (unsigned char)value[index];
        if (digit < '0' || digit > '9') return false;
        ULONGLONG numeric_digit = (ULONGLONG)(digit - '0');
        if (parsed > (ULLONG_MAX - numeric_digit) / 10ULL) return false;
        parsed = parsed * 10ULL + numeric_digit;
    }
    *dest = parsed;
    return true;
}

static bool should_apply_card_update(
    const char *json,
    size_t json_len,
    bool snapshot,
    bool snapshot_recovery,
    ULONGLONG *applied_sequence,
    bool *sequence_present_out
) {
    ULONGLONG sequence = 0;
    bool sequence_present = false;
    if (!json_read_optional_u64(
            json,
            json_len,
            "sequence",
            &sequence,
            &sequence_present)) {
        return false;
    }
    if (applied_sequence) *applied_sequence = sequence;
    if (sequence_present_out) *sequence_present_out = sequence_present;

    if (snapshot) {
        if (!sequence_present) {
            return snapshot_recovery && g_card_update_sequence == 0;
        }
        if (!snapshot_recovery && sequence <= g_card_update_sequence) return false;
        g_card_update_sequence = sequence;
        return true;
    }
    if (!sequence_present) {
        /* Keep compatibility with daemons predating sequenced card frames. */
        return true;
    }
    if (sequence <= g_card_update_sequence) return false;
    g_card_update_sequence = sequence;
    return true;
}

#define MAX_CONTEXT_CHIPS 16
typedef struct OverlayContextChip {
    wchar_t id[80];
    wchar_t title[260];
    wchar_t kind[64];
    wchar_t path[520];
    wchar_t processing_status[32];
    bool pending;
} OverlayContextChip;

static OverlayContextChip g_context_chips[MAX_CONTEXT_CHIPS];
static int g_context_chip_count = 0;
static bool g_show_context_chips = false;
static bool g_pending_context_chips = false;
static ULONGLONG g_pending_context_expected_until_ms = 0;
static OverlayContextChip g_sent_chips[MAX_CONTEXT_CHIPS];
static int g_sent_chip_count = 0;
static void consume_sent_context_chips(void);

static wchar_t *wide_duplicate(const wchar_t *text) {
    if (!text) return NULL;
    size_t length = wcslen(text);
    if (length > (SIZE_MAX / sizeof(wchar_t)) - 1) return NULL;
    wchar_t *copy = (wchar_t *)malloc((length + 1) * sizeof(wchar_t));
    if (!copy) return NULL;
    memcpy(copy, text, (length + 1) * sizeof(wchar_t));
    return copy;
}

static bool replace_body_owned(wchar_t *body) {
    if (!body) return false;
    AcquireSRWLockExclusive(&g_body_lock);
    wchar_t *previous = g_body;
    g_body = body;
    ReleaseSRWLockExclusive(&g_body_lock);
    if (previous != g_initial_body) free(previous);
    advance_presentation_revision();
    return true;
}

static bool set_body_text(const wchar_t *text) {
    wchar_t *copy = wide_duplicate(text ? text : L"");
    return replace_body_owned(copy);
}

static void copy_body_text(wchar_t *dest, size_t dest_len) {
    if (!dest || dest_len == 0) return;
    AcquireSRWLockShared(&g_body_lock);
    wcsncpy_s(dest, dest_len, g_body, _TRUNCATE);
    ReleaseSRWLockShared(&g_body_lock);
}

static bool wide_contains_ci(const wchar_t *text, const wchar_t *needle) {
    if (!text || !needle || !*needle) return false;
    size_t needle_len = wcslen(needle);
    for (const wchar_t *cursor = text; *cursor; cursor++) {
        if (_wcsnicmp(cursor, needle, needle_len) == 0) return true;
    }
    return false;
}

static void set_recovery_mode(int mode) {
    g_recovery_mode = mode;
    if (!g_send_button) return;
    SetWindowTextW(g_send_button, mode == 1 ? L"Continue" : (mode == 2 ? L"Retry" : L"Answer"));
}

static void update_recovery_action_from_current_card(void) {
    if (_wcsicmp(g_kind, L"answer") != 0) {
        set_recovery_mode(0);
        return;
    }
    AcquireSRWLockShared(&g_body_lock);
    bool should_continue = wide_contains_ci(g_body, L"kept the partial answer")
        || wide_contains_ci(g_body, L"select continue");
    bool should_retry = wide_contains_ci(g_body, L"select retry")
        || wide_contains_ci(g_body, L"could not finish")
        || wide_contains_ci(g_body, L"could not complete")
        || wide_contains_ci(g_body, L"busy for a moment");
    ReleaseSRWLockShared(&g_body_lock);

    if (should_continue) {
        set_recovery_mode(1);
    } else if (should_retry) {
        set_recovery_mode(2);
    } else {
        set_recovery_mode(0);
    }
}

#ifdef __cplusplus
#define BLUEY_COM_RELEASE(ptr) (ptr)->Release()
#define BLUEY_SET_COLOR(brush, color) (brush)->SetColor((color))
#define BLUEY_FILL_ROUNDED_RECTANGLE(target, rect, brush) (target)->FillRoundedRectangle((rect), (brush))
#define BLUEY_DRAW_ROUNDED_RECTANGLE(target, rect, brush, width, style) (target)->DrawRoundedRectangle((rect), (brush), (width), (style))
#define BLUEY_DRAW_TEXT(target, text, len, format, rect, brush, options, measuring_mode) (target)->DrawText((text), (len), (format), (rect), (brush), (options), (measuring_mode))
#define BLUEY_CREATE_TEXT_FORMAT(factory, family, collection, weight, style, stretch, size, locale, out) (factory)->CreateTextFormat((family), (collection), (weight), (style), (stretch), (size), (locale), (out))
#define BLUEY_SET_TEXT_ALIGNMENT(format, alignment) (format)->SetTextAlignment((alignment))
#define BLUEY_SET_PARAGRAPH_ALIGNMENT(format, alignment) (format)->SetParagraphAlignment((alignment))
#define BLUEY_SET_WORD_WRAPPING(format, wrapping) (format)->SetWordWrapping((wrapping))
#define BLUEY_CREATE_HWND_RENDER_TARGET(factory, props, hwnd_props, out) (factory)->CreateHwndRenderTarget((props), (hwnd_props), (out))
#define BLUEY_CREATE_SOLID_COLOR_BRUSH(target, color, props, out) (target)->CreateSolidColorBrush((color), (props), (out))
#define BLUEY_D2D_CREATE_FACTORY(options, out) D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, __uuidof(ID2D1Factory), (options), (void **)(out))
#define BLUEY_DWRITE_CREATE_FACTORY(out) DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, __uuidof(IDWriteFactory), (IUnknown **)(out))
#define BLUEY_TARGET_RESIZE(target, size) (target)->Resize((size))
#define BLUEY_BEGIN_DRAW(target) (target)->BeginDraw()
#define BLUEY_CLEAR(target, color) (target)->Clear((color))
#define BLUEY_DRAW_LINE(target, p0, p1, brush, width, style) (target)->DrawLine((p0), (p1), (brush), (width), (style))
#define BLUEY_FILL_ELLIPSE(target, ellipse, brush) (target)->FillEllipse((ellipse), (brush))
#define BLUEY_END_DRAW(target, tag1, tag2) (target)->EndDraw((tag1), (tag2))
#else
#define BLUEY_COM_RELEASE(ptr) IUnknown_Release((IUnknown *)(ptr))
#define BLUEY_SET_COLOR(brush, color) ID2D1SolidColorBrush_SetColor((brush), (color))
#define BLUEY_FILL_ROUNDED_RECTANGLE(target, rect, brush) ID2D1HwndRenderTarget_FillRoundedRectangle((target), (rect), (ID2D1Brush *)(brush))
#define BLUEY_DRAW_ROUNDED_RECTANGLE(target, rect, brush, width, style) ID2D1HwndRenderTarget_DrawRoundedRectangle((target), (rect), (ID2D1Brush *)(brush), (width), (style))
#define BLUEY_DRAW_TEXT(target, text, len, format, rect, brush, options, measuring_mode) ID2D1HwndRenderTarget_DrawText((target), (text), (len), (format), (rect), (ID2D1Brush *)(brush), (options), (measuring_mode))
#define BLUEY_CREATE_TEXT_FORMAT(factory, family, collection, weight, style, stretch, size, locale, out) IDWriteFactory_CreateTextFormat((factory), (family), (collection), (weight), (style), (stretch), (size), (locale), (out))
#define BLUEY_SET_TEXT_ALIGNMENT(format, alignment) IDWriteTextFormat_SetTextAlignment((format), (alignment))
#define BLUEY_SET_PARAGRAPH_ALIGNMENT(format, alignment) IDWriteTextFormat_SetParagraphAlignment((format), (alignment))
#define BLUEY_SET_WORD_WRAPPING(format, wrapping) IDWriteTextFormat_SetWordWrapping((format), (wrapping))
#define BLUEY_CREATE_HWND_RENDER_TARGET(factory, props, hwnd_props, out) ID2D1Factory_CreateHwndRenderTarget((factory), (props), (hwnd_props), (out))
#define BLUEY_CREATE_SOLID_COLOR_BRUSH(target, color, props, out) ID2D1HwndRenderTarget_CreateSolidColorBrush((target), (color), (props), (out))
#define BLUEY_D2D_CREATE_FACTORY(options, out) D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, &IID_ID2D1Factory, (options), (void **)(out))
#define BLUEY_DWRITE_CREATE_FACTORY(out) DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, &IID_IDWriteFactory, (IUnknown **)(out))
#define BLUEY_TARGET_RESIZE(target, size) ID2D1HwndRenderTarget_Resize((target), (size))
#define BLUEY_BEGIN_DRAW(target) ID2D1HwndRenderTarget_BeginDraw((target))
#define BLUEY_CLEAR(target, color) ID2D1HwndRenderTarget_Clear((target), (color))
#define BLUEY_DRAW_LINE(target, p0, p1, brush, width, style) ID2D1HwndRenderTarget_DrawLine((target), (p0), (p1), (ID2D1Brush *)(brush), (width), (style))
#define BLUEY_FILL_ELLIPSE(target, ellipse, brush) ID2D1HwndRenderTarget_FillEllipse((target), (ellipse), (ID2D1Brush *)(brush))
#define BLUEY_END_DRAW(target, tag1, tag2) ID2D1HwndRenderTarget_EndDraw((target), (tag1), (tag2))
#endif

#define ID_ASK_EDIT 1001
#define ID_SEND_BUTTON 1002
#define ID_RECORD_BUTTON 1003
#define ID_PASTE_ANSWER_BUTTON 1015
#define ID_AUTO_SEND_BUTTON 1014
#define ID_TRANSCRIPT_CLEAR_BUTTON 1013
#define ID_HELP_BUTTON 1004
#define ID_SESSION_BUTTON 1005
#define ID_PAGE_BUTTON 1007
#define ID_ATTACH_BUTTON 1008
#define ID_NOTE_BUTTON 1009
#define ID_CLOSE_BUTTON 1010
#define ID_RECAP_BUTTON 1011
#define ID_THEME_BUTTON 1012
#define ID_SHORTCUTS_BUTTON 1016
#define ID_ANSWER_DETAIL_BUTTON 1017
#define ID_MEETING_START_BUTTON 1101
#define ID_MEETING_SNOOZE_BUTTON 1102
#define ID_MEETING_DISMISS_BUTTON 1103
#define ID_MEETING_IGNORE_BUTTON 1104
#define ID_MEETING_SETTINGS_BUTTON 1105
#define COLLAPSED_DRAG_THRESHOLD 4
#define ID_HOTKEY_TOGGLE_OVERLAY 2001
#define ID_HOTKEY_FOCUS_ASK 2002
#define ID_HOTKEY_LISTEN 2003
#define ID_HOTKEY_SCREEN 2004
#define ID_HOTKEY_INTERACTIVE 2005
#define ID_HOTKEY_ANSWER 2006
#define ID_HOTKEY_HISTORY 2007
#define ID_HOTKEY_FILES 2008
#define ID_AUTOSEND_TIMER 3001
#define ID_MANUAL_SEND_TIMER 3002
#define ID_MEETING_BANNER_TIMER 3003
#define ID_RENDER_ACK_RETRY_TIMER 3004
#define WM_BLUEY_MEETING_EVIDENCE (WM_APP + 41)
#define WM_BLUEY_MEETING_DETECTION_DISABLED (WM_APP + 42)
#define AUTOSEND_CAPTION_SETTLE_DELAY_MS 300
#define MANUAL_CAPTION_SETTLE_DELAY_MS 600
#define MANUAL_FINAL_CAPTION_SETTLE_DELAY_MS 250
#define MANUAL_CAPTION_SETTLE_MAX_MS 2000
#define BLUEY_GLOBAL_HOTKEY_MODS (MOD_CONTROL | MOD_ALT | MOD_NOREPEAT)

/* Stealth: hide overlay from screen recording, screenshots, and screen-share.
 * WDA_EXCLUDEFROMCAPTURE (Windows 10 2004+ / build 19041) makes the window
 * invisible to all capture APIs (OBS, Teams screen-share, Win+Shift+S, etc.).
 * Fallback: WDA_MONITOR renders the window as black in captures on older builds
 * (still hidden from casual observation but not fully invisible). */
static void apply_capture_exclusion(HWND hwnd) {
    if (!SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)) {
        SetWindowDisplayAffinity(hwnd, WDA_MONITOR);
    }
}


// Per-session token from the daemon (BLUEY_OVERLAY_SESSION_TOKEN env var).
// Embedded in every emitted JSON event. Daemon validates + drops events
// whose token does not match its own per-session value.
static char g_session_token[129] = {0}; // 128-char max + NUL
static const wchar_t *g_supported_drop_formats =
    L"Supported: PDF, Word, PowerPoint, Excel/ODS, CSV/TSV, text, Markdown, code/data files, and PNG/JPEG/WebP/GIF/HEIC/BMP/TIFF images. Video files are not readable context yet.";

static void load_session_token(void) {
    DWORD n = GetEnvironmentVariableA(
        "BLUEY_OVERLAY_SESSION_TOKEN",
        g_session_token,
        (DWORD)sizeof(g_session_token));
    if (n == 0 || n >= sizeof(g_session_token)) {
        g_session_token[0] = '\0';
    }
}

#define BLUEY_OUTPUT_QUEUE_CAPACITY 256u
#define BLUEY_OUTPUT_QUEUE_MAX_BYTES (12u * 1024u * 1024u)
#define BLUEY_OUTPUT_RECORD_MAX_BYTES JSON_MAX_LINE_LEN
#define BLUEY_UI_METADATA_EVENT_MAX_BYTES (16u * 1024u)
#define BLUEY_UI_ASK_EVENT_MAX_BYTES (32u * 1024u)
#define BLUEY_UI_BULK_EVENT_MAX_BYTES (256u * 1024u)

typedef struct BlueyOutputRecord {
    char *data;
    size_t length;
    bool droppable;
} BlueyOutputRecord;

typedef struct BlueyJsonBuffer {
    char *data;
    size_t length;
    size_t capacity;
    size_t max_length;
    bool failed;
} BlueyJsonBuffer;

static CRITICAL_SECTION g_output_lock;
static bool g_output_lock_initialized = false;
static HANDLE g_output_wake = NULL;
static HANDLE g_output_thread = NULL;
static BlueyOutputRecord g_output_queue[BLUEY_OUTPUT_QUEUE_CAPACITY];
static size_t g_output_head = 0;
static size_t g_output_count = 0;
static size_t g_output_bytes = 0;
static bool g_output_stopping = false;
static volatile LONG g_output_failed = 0;
static volatile LONG g_output_dropped_records = 0;

static bool output_write_loss_summary(HANDLE output, LONG dropped_records);

static bool output_write_all(HANDLE output, const char *data, size_t length) {
    if (!data || length == 0 || output == INVALID_HANDLE_VALUE || output == NULL) {
        return false;
    }

    size_t offset = 0;
    while (offset < length) {
        size_t remaining = length - offset;
        DWORD chunk = remaining > (size_t)MAXDWORD ? MAXDWORD : (DWORD)remaining;
        DWORD written = 0;
        if (!WriteFile(output, data + offset, chunk, &written, NULL) || written == 0) {
            return false;
        }
        offset += (size_t)written;
    }
    return true;
}

static bool output_pop_locked(BlueyOutputRecord *record) {
    if (!record || g_output_count == 0) return false;
    *record = g_output_queue[g_output_head];
    ZeroMemory(&g_output_queue[g_output_head], sizeof(g_output_queue[g_output_head]));
    g_output_head = (g_output_head + 1u) % BLUEY_OUTPUT_QUEUE_CAPACITY;
    g_output_count--;
    if (record->length <= g_output_bytes) g_output_bytes -= record->length;
    else g_output_bytes = 0;
    return true;
}

static void output_remove_relative_locked(size_t relative_index) {
    if (relative_index >= g_output_count) return;
    size_t index = (g_output_head + relative_index) % BLUEY_OUTPUT_QUEUE_CAPACITY;
    BlueyOutputRecord removed = g_output_queue[index];

    for (size_t offset = relative_index; offset + 1u < g_output_count; offset++) {
        size_t current = (g_output_head + offset) % BLUEY_OUTPUT_QUEUE_CAPACITY;
        size_t next = (g_output_head + offset + 1u) % BLUEY_OUTPUT_QUEUE_CAPACITY;
        g_output_queue[current] = g_output_queue[next];
    }

    size_t tail = (g_output_head + g_output_count - 1u) % BLUEY_OUTPUT_QUEUE_CAPACITY;
    ZeroMemory(&g_output_queue[tail], sizeof(g_output_queue[tail]));
    g_output_count--;
    if (removed.length <= g_output_bytes) g_output_bytes -= removed.length;
    else g_output_bytes = 0;
    free(removed.data);
}

static DWORD WINAPI output_writer_thread(LPVOID unused) {
    (void)unused;
    HANDLE output = GetStdHandle(STD_OUTPUT_HANDLE);
    if (output == INVALID_HANDLE_VALUE || output == NULL) {
        InterlockedExchange(&g_output_failed, 1);
    }

    for (;;) {
        BlueyOutputRecord record;
        ZeroMemory(&record, sizeof(record));
        bool stopping = false;

        EnterCriticalSection(&g_output_lock);
        bool has_record = output_pop_locked(&record);
        stopping = g_output_stopping;
        LeaveCriticalSection(&g_output_lock);

        if (has_record) {
            if (InterlockedCompareExchange(&g_output_failed, 0, 0) == 0
                && !output_write_all(output, record.data, record.length)) {
                InterlockedExchange(&g_output_failed, 1);
            }
            free(record.data);
            LONG dropped_records = InterlockedExchange(&g_output_dropped_records, 0);
            if (dropped_records > 0
                && InterlockedCompareExchange(&g_output_failed, 0, 0) == 0
                && !output_write_loss_summary(output, dropped_records)) {
                InterlockedExchange(&g_output_failed, 1);
            }
            continue;
        }
        if (stopping || InterlockedCompareExchange(&g_output_failed, 0, 0) != 0) {
            break;
        }
        WaitForSingleObject(g_output_wake, INFINITE);
    }

    return 0;
}

static bool start_output_writer(void) {
    if (g_output_lock_initialized) return true;
    HANDLE output = GetStdHandle(STD_OUTPUT_HANDLE);
    if (output == INVALID_HANDLE_VALUE || output == NULL) return false;
    if (!InitializeCriticalSectionAndSpinCount(&g_output_lock, 256)) return false;
    g_output_lock_initialized = true;
    g_output_wake = CreateEventW(NULL, FALSE, FALSE, NULL);
    if (!g_output_wake) {
        DeleteCriticalSection(&g_output_lock);
        g_output_lock_initialized = false;
        return false;
    }
    g_output_thread = CreateThread(NULL, 0, output_writer_thread, NULL, 0, NULL);
    if (!g_output_thread) {
        CloseHandle(g_output_wake);
        g_output_wake = NULL;
        DeleteCriticalSection(&g_output_lock);
        g_output_lock_initialized = false;
        return false;
    }
    return true;
}

static void stop_output_writer(void) {
    if (!g_output_lock_initialized) return;

    EnterCriticalSection(&g_output_lock);
    g_output_stopping = true;
    LeaveCriticalSection(&g_output_lock);
    SetEvent(g_output_wake);

    DWORD wait_result = g_output_thread
        ? WaitForSingleObject(g_output_thread, 2000)
        : WAIT_OBJECT_0;
    if (wait_result == WAIT_TIMEOUT && g_output_thread) {
        CancelSynchronousIo(g_output_thread);
        SetEvent(g_output_wake);
        wait_result = WaitForSingleObject(g_output_thread, 500);
    }

    if (wait_result != WAIT_OBJECT_0) {
        /* The process is exiting. Keep shared state alive rather than racing a
         * writer still unwinding from a blocked pipe operation. */
        return;
    }

    if (g_output_thread) {
        CloseHandle(g_output_thread);
        g_output_thread = NULL;
    }
    if (g_output_wake) {
        CloseHandle(g_output_wake);
        g_output_wake = NULL;
    }

    EnterCriticalSection(&g_output_lock);
    BlueyOutputRecord record;
    while (output_pop_locked(&record)) free(record.data);
    LeaveCriticalSection(&g_output_lock);
    /* Input and detector workers are process-lifetime threads. Leave the
     * stopped lock available so late producers fail closed without racing a
     * deleted synchronization primitive during shutdown. */
}

static bool enqueue_output_record(char *data, size_t length, bool droppable) {
    if (!data || length == 0 || length > BLUEY_OUTPUT_RECORD_MAX_BYTES
        || !g_output_lock_initialized) {
        free(data);
        return false;
    }

    EnterCriticalSection(&g_output_lock);
    if (g_output_stopping
        || InterlockedCompareExchange(&g_output_failed, 0, 0) != 0) {
        LeaveCriticalSection(&g_output_lock);
        free(data);
        return false;
    }

    while (g_output_count >= BLUEY_OUTPUT_QUEUE_CAPACITY
           || length > BLUEY_OUTPUT_QUEUE_MAX_BYTES - g_output_bytes) {
        if (droppable) {
            InterlockedIncrement(&g_output_dropped_records);
            LeaveCriticalSection(&g_output_lock);
            free(data);
            return false;
        }

        size_t droppable_index = g_output_count;
        for (size_t index = 0; index < g_output_count; index++) {
            size_t slot = (g_output_head + index) % BLUEY_OUTPUT_QUEUE_CAPACITY;
            if (g_output_queue[slot].droppable) {
                droppable_index = index;
                break;
            }
        }
        if (droppable_index == g_output_count) {
            LeaveCriticalSection(&g_output_lock);
            free(data);
            return false;
        }
        output_remove_relative_locked(droppable_index);
        InterlockedIncrement(&g_output_dropped_records);
    }

    size_t tail = (g_output_head + g_output_count) % BLUEY_OUTPUT_QUEUE_CAPACITY;
    g_output_queue[tail].data = data;
    g_output_queue[tail].length = length;
    g_output_queue[tail].droppable = droppable;
    g_output_count++;
    g_output_bytes += length;
    LeaveCriticalSection(&g_output_lock);
    SetEvent(g_output_wake);
    return true;
}

static void json_buffer_init_limited(
    BlueyJsonBuffer *buffer,
    size_t initial_capacity,
    size_t max_length
) {
    if (!buffer) return;
    ZeroMemory(buffer, sizeof(*buffer));
    if (max_length == 0 || max_length > BLUEY_OUTPUT_RECORD_MAX_BYTES) {
        max_length = BLUEY_OUTPUT_RECORD_MAX_BYTES;
    }
    if (initial_capacity < 256u) initial_capacity = 256u;
    if (initial_capacity > max_length) initial_capacity = max_length;
    buffer->data = (char *)malloc(initial_capacity);
    if (!buffer->data) {
        buffer->failed = true;
        return;
    }
    buffer->capacity = initial_capacity;
    buffer->max_length = max_length;
    buffer->data[0] = '\0';
}

static bool json_buffer_reserve(BlueyJsonBuffer *buffer, size_t additional) {
    if (!buffer || buffer->failed || !buffer->data) return false;
    if (buffer->length >= buffer->max_length
        || additional > buffer->max_length - buffer->length - 1u) {
        buffer->failed = true;
        return false;
    }
    size_t required = buffer->length + additional + 1u;
    if (required <= buffer->capacity) return true;

    size_t capacity = buffer->capacity;
    while (capacity < required) {
        size_t next = capacity > buffer->max_length / 2u
            ? buffer->max_length
            : capacity * 2u;
        if (next <= capacity) {
            buffer->failed = true;
            return false;
        }
        capacity = next;
    }
    char *grown = (char *)realloc(buffer->data, capacity);
    if (!grown) {
        buffer->failed = true;
        return false;
    }
    buffer->data = grown;
    buffer->capacity = capacity;
    return true;
}

static bool json_buffer_append_bytes(
    BlueyJsonBuffer *buffer,
    const char *text,
    size_t text_len
) {
    if (!text || !json_buffer_reserve(buffer, text_len)) return false;
    memcpy(buffer->data + buffer->length, text, text_len);
    buffer->length += text_len;
    buffer->data[buffer->length] = '\0';
    return true;
}

static bool json_buffer_append(BlueyJsonBuffer *buffer, const char *text) {
    return text && json_buffer_append_bytes(buffer, text, strlen(text));
}

static bool json_buffer_append_char(BlueyJsonBuffer *buffer, char value) {
    return json_buffer_append_bytes(buffer, &value, 1u);
}

static bool json_buffer_append_format(BlueyJsonBuffer *buffer, const char *format, ...) {
    if (!buffer || !format || buffer->failed) return false;
    va_list args;
    va_start(args, format);
    va_list measure_args;
    va_copy(measure_args, args);
    int needed = vsnprintf(NULL, 0, format, measure_args);
    va_end(measure_args);
    if (needed < 0 || !json_buffer_reserve(buffer, (size_t)needed)) {
        va_end(args);
        if (buffer) buffer->failed = true;
        return false;
    }
    int written = vsnprintf(
        buffer->data + buffer->length,
        buffer->capacity - buffer->length,
        format,
        args);
    va_end(args);
    if (written != needed) {
        buffer->failed = true;
        return false;
    }
    buffer->length += (size_t)written;
    return true;
}

static bool json_buffer_append_escaped(BlueyJsonBuffer *buffer, const char *text) {
    if (!buffer || !text) return false;
    for (const unsigned char *cursor = (const unsigned char *)text; *cursor; cursor++) {
        switch (*cursor) {
        case '\\':
            if (!json_buffer_append(buffer, "\\\\")) return false;
            break;
        case '"':
            if (!json_buffer_append(buffer, "\\\"")) return false;
            break;
        case '\n':
            if (!json_buffer_append(buffer, "\\n")) return false;
            break;
        case '\r':
            if (!json_buffer_append(buffer, "\\r")) return false;
            break;
        case '\t':
            if (!json_buffer_append(buffer, "\\t")) return false;
            break;
        default:
            if (*cursor < 0x20) {
                if (!json_buffer_append_format(buffer, "\\u%04x", *cursor)) return false;
            } else if (!json_buffer_append_char(buffer, (char)*cursor)) {
                return false;
            }
            break;
        }
    }
    return true;
}

static void json_buffer_dispose(BlueyJsonBuffer *buffer) {
    if (!buffer) return;
    free(buffer->data);
    ZeroMemory(buffer, sizeof(*buffer));
}

static bool json_buffer_enqueue(BlueyJsonBuffer *buffer, bool droppable) {
    if (!buffer || buffer->failed || !buffer->data || buffer->length == 0) {
        json_buffer_dispose(buffer);
        return false;
    }
    char *data = buffer->data;
    size_t length = buffer->length;
    ZeroMemory(buffer, sizeof(*buffer));
    return enqueue_output_record(data, length, droppable);
}

static void ensure_tooltip_window(void) {
    if (g_tooltip) return;

    INITCOMMONCONTROLSEX icc;
    ZeroMemory(&icc, sizeof(icc));
    icc.dwSize = sizeof(icc);
    icc.dwICC = ICC_WIN95_CLASSES;
    InitCommonControlsEx(&icc);

    g_tooltip = CreateWindowExW(
        WS_EX_TOPMOST,
        TOOLTIPS_CLASSW,
        NULL,
        WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        g_hwnd,
        NULL,
        GetModuleHandleW(NULL),
        NULL);
    if (!g_tooltip) return;

    SetWindowPos(
        g_tooltip,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    SendMessageW(g_tooltip, TTM_SETDELAYTIME, TTDT_INITIAL, 450);
    SendMessageW(g_tooltip, TTM_SETDELAYTIME, TTDT_AUTOPOP, 9000);
}

static void add_control_tooltip(HWND control, LPCWSTR text) {
    if (!control || !text || !text[0]) return;
    ensure_tooltip_window();
    if (!g_tooltip) return;

    TOOLINFOW tool;
    ZeroMemory(&tool, sizeof(tool));
    tool.cbSize = sizeof(tool);
    tool.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
    tool.hwnd = g_hwnd;
    tool.uId = (UINT_PTR)control;
    tool.lpszText = (LPWSTR)text;
    SendMessageW(g_tooltip, TTM_ADDTOOLW, 0, (LPARAM)&tool);
}

static void configure_tooltips(void) {
    add_control_tooltip(g_ask_edit, L"Type or paste a question for Bluey");
    add_control_tooltip(g_send_button, L"Send the question");
    add_control_tooltip(g_record_button, L"Start or stop listening");
    add_control_tooltip(g_auto_send_combo, L"Choose which audio source should auto-send after captions pause briefly. Stop cancels pending auto-send.");
    add_control_tooltip(g_answer_detail_combo, L"Auto chooses for you. Quick is compact; Thorough gives a deeper answer.");
    add_control_tooltip(g_transcript_clear_button, L"Clear current captions from the next answer");
    add_control_tooltip(g_help_button, L"Show Bluey help");
    add_control_tooltip(g_session_button, L"Open conversation history");
    add_control_tooltip(g_page_button, L"Capture the screen as context");
    add_control_tooltip(g_attach_button, L"Attach documents or images");
    add_control_tooltip(g_recap_button, L"Create a recap for this recording");
    add_control_tooltip(g_note_button, L"Set how Bluey should answer");
    add_control_tooltip(g_theme_button, L"Toggle light or dark theme");
    add_control_tooltip(g_shortcuts_button, L"Show controls and shortcuts");
    add_control_tooltip(g_close_button, L"Turn Bluey off");
}

// Append `,"token":"..."` if a token is set, else nothing.
// Caller must have already opened the JSON object and emitted >= 1 field.
static bool append_token_field(BlueyJsonBuffer *buffer) {
    if (g_session_token[0] != '\0') {
        return json_buffer_append(buffer, ",\"token\":\"")
            && json_buffer_append_escaped(buffer, g_session_token)
            && json_buffer_append_char(buffer, '"');
    }
    return true;
}

static bool output_write_loss_summary(HANDLE output, LONG dropped_records) {
    if (dropped_records <= 0) return true;
    BlueyJsonBuffer buffer;
    json_buffer_init_limited(
        &buffer, 384u, BLUEY_UI_METADATA_EVENT_MAX_BYTES);
    json_buffer_append(
        &buffer,
        "{\"type\":\"lifecycle\","
        "\"stage\":\"native_output_records_dropped\","
        "\"status\":\"degraded\",\"detail\":\"");
    json_buffer_append_format(&buffer, "count=%ld", (long)dropped_records);
    json_buffer_append_char(&buffer, '"');
    append_token_field(&buffer);
    json_buffer_append(&buffer, "}\n");
    bool written = !buffer.failed
        && output_write_all(output, buffer.data, buffer.length);
    json_buffer_dispose(&buffer);
    return written;
}

static bool emit_ready(void) {
    BlueyJsonBuffer buffer;
    json_buffer_init_limited(
        &buffer, 256u, BLUEY_UI_METADATA_EVENT_MAX_BYTES);
    json_buffer_append(
        &buffer,
        "{\"type\":\"ready\",\"platform\":\"windows\",\"capture_excluded\":true");
    append_token_field(&buffer);
    json_buffer_append(&buffer, "}\n");
    return json_buffer_enqueue(&buffer, false);
}

static char *wide_to_utf8_alloc(const wchar_t *text) {
    int bytes = WideCharToMultiByte(CP_UTF8, 0, text, -1, NULL, 0, NULL, NULL);
    if (bytes <= 0) return NULL;
    char *utf8 = (char *)malloc((size_t)bytes);
    if (!utf8) return NULL;
    if (WideCharToMultiByte(CP_UTF8, 0, text, -1, utf8, bytes, NULL, NULL) <= 0) {
        free(utf8);
        return NULL;
    }
    return utf8;
}

static wchar_t *utf8_to_wide_alloc(const char *text, size_t text_len) {
    if (!text || text_len > INT_MAX) return NULL;
    if (text_len == 0) return wide_duplicate(L"");

    int wchars = MultiByteToWideChar(
        CP_UTF8,
        MB_ERR_INVALID_CHARS,
        text,
        (int)text_len,
        NULL,
        0);
    if (wchars <= 0 || (size_t)wchars > (SIZE_MAX / sizeof(wchar_t)) - 1) return NULL;

    wchar_t *wide = (wchar_t *)malloc(((size_t)wchars + 1) * sizeof(wchar_t));
    if (!wide) return NULL;
    if (MultiByteToWideChar(
            CP_UTF8,
            MB_ERR_INVALID_CHARS,
            text,
            (int)text_len,
            wide,
            wchars) != wchars) {
        free(wide);
        return NULL;
    }
    wide[wchars] = L'\0';
    return wide;
}

static bool json_buffer_append_wide_escaped(
    BlueyJsonBuffer *buffer,
    const wchar_t *text
) {
    char *utf8 = wide_to_utf8_alloc(text ? text : L"");
    if (!utf8) {
        if (buffer) buffer->failed = true;
        return false;
    }
    bool appended = json_buffer_append_escaped(buffer, utf8);
    free(utf8);
    return appended;
}

static void normalize_answer_display_text(wchar_t *text) {
    if (!text) return;

    size_t length = wcslen(text);
    size_t write = 0;
    bool at_line_start = true;
    bool in_fence = false;

    for (size_t read = 0; read < length; read++) {
        wchar_t ch = text[read];

        if (at_line_start && read + 2 < length
            && ch == L'`' && text[read + 1] == L'`' && text[read + 2] == L'`') {
            in_fence = !in_fence;
            while (read < length && text[read] != L'\n') {
                read++;
            }
            if (read < length && text[read] == L'\n') {
                text[write++] = L'\n';
            }
            at_line_start = true;
            continue;
        }

        if (in_fence) {
            text[write++] = ch;
            if (ch == L'\n') {
                at_line_start = true;
            } else if (ch != L'\r') {
                at_line_start = false;
            }
            continue;
        }

        if (at_line_start) {
            size_t cursor = read;
            while (cursor < length && (text[cursor] == L' ' || text[cursor] == L'\t')) {
                cursor++;
            }
            if (cursor < length && text[cursor] == L'#') {
                while (cursor < length && text[cursor] == L'#') {
                    cursor++;
                }
                if (cursor < length && text[cursor] == L' ') {
                    cursor++;
                }
                read = cursor;
                if (read >= length) break;
                ch = text[read];
            }
        }

        if (ch == L'`') {
            continue;
        }
        if (read + 1 < length
            && ((ch == L'*' && text[read + 1] == L'*')
                || (ch == L'_' && text[read + 1] == L'_'))) {
            read++;
            continue;
        }

        text[write++] = ch;
        if (ch == L'\n') {
            at_line_start = true;
        } else if (ch != L'\r') {
            at_line_start = false;
        }
    }

    text[write] = L'\0';
}

static bool emit_simple_event(const char *type) {
    BlueyJsonBuffer buffer;
    json_buffer_init_limited(
        &buffer, 256u, BLUEY_UI_METADATA_EVENT_MAX_BYTES);
    json_buffer_append(&buffer, "{\"type\":\"");
    json_buffer_append_escaped(&buffer, type ? type : "");
    json_buffer_append_char(&buffer, '"');
    append_token_field(&buffer);
    json_buffer_append(&buffer, "}\n");
    return json_buffer_enqueue(&buffer, false);
}

static bool emit_lifecycle_event(
    const char *stage,
    const char *status,
    const char *detail
) {
    BlueyJsonBuffer buffer;
    json_buffer_init_limited(
        &buffer, 512u, BLUEY_UI_METADATA_EVENT_MAX_BYTES);
    json_buffer_append(&buffer, "{\"type\":\"lifecycle\",\"stage\":\"");
    json_buffer_append_escaped(&buffer, stage ? stage : "");
    json_buffer_append(&buffer, "\",\"status\":\"");
    json_buffer_append_escaped(&buffer, status ? status : "ok");
    json_buffer_append_char(&buffer, '"');
    if (detail && detail[0] != '\0') {
        json_buffer_append(&buffer, ",\"detail\":\"");
        json_buffer_append_escaped(&buffer, detail);
        json_buffer_append_char(&buffer, '"');
    }
    append_token_field(&buffer);
    json_buffer_append(&buffer, "}\n");
    return json_buffer_enqueue(&buffer, true);
}

static bool register_bluey_hotkey(int id, UINT key, const char *name, char *failures, size_t failures_len) {
    if (RegisterHotKey(g_hwnd, id, BLUEY_GLOBAL_HOTKEY_MODS, key)) {
        return true;
    }
    if (failures && failures_len > 0) {
        char item[48];
        snprintf(item, sizeof(item), "%s:%lu", name ? name : "unknown", (unsigned long)GetLastError());
        size_t used = strlen(failures);
        if (used + 1 < failures_len && failures[0] != '\0') {
            strncat(failures, ",", failures_len - used - 1);
            used = strlen(failures);
        }
        if (used + 1 < failures_len) {
            strncat(failures, item, failures_len - used - 1);
        }
    }
    return false;
}

static bool generate_interaction_id(char destination[37]) {
    if (!destination) return false;
    GUID id;
    if (FAILED(CoCreateGuid(&id))) {
        destination[0] = '\0';
        return false;
    }
    int written = snprintf(
        destination,
        37,
        "%08lx-%04x-%04x-%02x%02x-%02x%02x%02x%02x%02x%02x",
        (unsigned long)id.Data1,
        (unsigned)id.Data2,
        (unsigned)id.Data3,
        (unsigned)id.Data4[0],
        (unsigned)id.Data4[1],
        (unsigned)id.Data4[2],
        (unsigned)id.Data4[3],
        (unsigned)id.Data4[4],
        (unsigned)id.Data4[5],
        (unsigned)id.Data4[6],
        (unsigned)id.Data4[7]);
    return written == 36;
}

static ULONGLONG current_unix_time_ms(void) {
    static const ULONGLONG WINDOWS_TO_UNIX_EPOCH_TICKS = 116444736000000000ULL;
    FILETIME file_time;
    ULARGE_INTEGER ticks;
    GetSystemTimeAsFileTime(&file_time);
    ticks.LowPart = file_time.dwLowDateTime;
    ticks.HighPart = file_time.dwHighDateTime;
    if (ticks.QuadPart <= WINDOWS_TO_UNIX_EPOCH_TICKS) return 0;
    return (ticks.QuadPart - WINDOWS_TO_UNIX_EPOCH_TICKS) / 10000ULL;
}

static bool canonical_uuid(const char *value) {
    if (!value || strlen(value) != 36u) return false;
    bool has_nonzero_digit = false;
    for (size_t index = 0; index < 36u; index++) {
        if (index == 8u || index == 13u || index == 18u || index == 23u) {
            if (value[index] != '-') return false;
            continue;
        }
        if (!json_is_hex(value[index])) return false;
        if (value[index] != '0') has_nonzero_digit = true;
    }
    return has_nonzero_digit;
}

static bool canonical_wide_uuid(const wchar_t *value) {
    if (!value || wcslen(value) != 36u) return false;
    bool has_nonzero_digit = false;
    for (size_t index = 0; index < 36u; index++) {
        if (index == 8u || index == 13u || index == 18u || index == 23u) {
            if (value[index] != L'-') return false;
            continue;
        }
        wchar_t digit = value[index];
        bool valid = (digit >= L'0' && digit <= L'9')
            || (digit >= L'a' && digit <= L'f')
            || (digit >= L'A' && digit <= L'F');
        if (!valid) return false;
        if (digit != L'0') has_nonzero_digit = true;
    }
    return has_nonzero_digit;
}

static BlueyRenderAckPhase parse_render_ack_phase(
    const char *json,
    size_t json_len
) {
    char phase[24];
    if (!json_extract_string(json, json_len, "render_ack", phase, sizeof(phase))) {
        return BLUEY_RENDER_ACK_NONE;
    }
    if (strcmp(phase, "first_text") == 0) return BLUEY_RENDER_ACK_FIRST_TEXT;
    if (strcmp(phase, "final") == 0) return BLUEY_RENDER_ACK_FINAL;
    return BLUEY_RENDER_ACK_NONE;
}

static void update_pending_render_acks(
    const wchar_t *card_id,
    const char *interaction_id,
    BlueyRenderAckPhase phase,
    ULONGLONG sequence,
    bool sequence_present,
    bool body_updated
) {
    AcquireSRWLockExclusive(&g_render_ack_lock);
    bool valid_update = body_updated
        && card_id
        && canonical_wide_uuid(card_id)
        && sequence_present
        && sequence == g_card_update_sequence;
    bool has_interaction = interaction_id && canonical_uuid(interaction_id);
    BlueyPendingRenderAck *slots[] = {
        &g_pending_first_text_ack,
        &g_pending_final_ack,
    };

    if (!valid_update) {
        ReleaseSRWLockExclusive(&g_render_ack_lock);
        return;
    }

    for (size_t index = 0; index < sizeof(slots) / sizeof(slots[0]); index++) {
        BlueyPendingRenderAck *pending = slots[index];
        if (!pending->active) continue;
        if (wcscmp(pending->card_id, card_id) != 0
            || (has_interaction
                && strcmp(pending->interaction_id, interaction_id) != 0)
            || sequence < pending->requested_sequence) {
            ZeroMemory(pending, sizeof(*pending));
            continue;
        }
        pending->paint_sequence = sequence;
        pending->presentation_revision = g_presentation_revision;
    }

    if (phase != BLUEY_RENDER_ACK_NONE && has_interaction) {
        BlueyPendingRenderAck *pending = phase == BLUEY_RENDER_ACK_FIRST_TEXT
            ? &g_pending_first_text_ack
            : &g_pending_final_ack;
        ZeroMemory(pending, sizeof(*pending));
        pending->active = true;
        wcsncpy_s(pending->card_id, 80, card_id, _TRUNCATE);
        strncpy_s(
            pending->interaction_id,
            sizeof(pending->interaction_id),
            interaction_id,
            _TRUNCATE);
        pending->phase = phase;
        pending->requested_sequence = sequence;
        pending->paint_sequence = sequence;
        pending->presentation_revision = g_presentation_revision;
    }
    ReleaseSRWLockExclusive(&g_render_ack_lock);
}

static ULONGLONG current_presentation_revision(void) {
    AcquireSRWLockShared(&g_render_ack_lock);
    ULONGLONG revision = g_presentation_revision;
    ReleaseSRWLockShared(&g_render_ack_lock);
    return revision;
}

static bool emit_pending_render_ack_locked(
    BlueyPendingRenderAck *pending,
    ULONGLONG painted_revision
) {
    if (!pending || !pending->active) return false;
    if (wcscmp(pending->card_id, g_card_id) != 0
        || !canonical_wide_uuid(pending->card_id)
        || pending->paint_sequence != g_card_update_sequence
        || pending->paint_sequence < pending->requested_sequence
        || !canonical_uuid(pending->interaction_id)
        || pending->presentation_revision != painted_revision) {
        ZeroMemory(pending, sizeof(*pending));
        return false;
    }

    const char *phase = pending->phase == BLUEY_RENDER_ACK_FIRST_TEXT
        ? "first_text"
        : (pending->phase == BLUEY_RENDER_ACK_FINAL ? "final" : NULL);
    if (!phase) {
        ZeroMemory(pending, sizeof(*pending));
        return false;
    }

    BlueyJsonBuffer buffer;
    json_buffer_init_limited(
        &buffer, 512u, BLUEY_UI_METADATA_EVENT_MAX_BYTES);
    json_buffer_append(
        &buffer,
        "{\"type\":\"answer_render_acknowledged\",\"id\":\"");
    json_buffer_append_wide_escaped(&buffer, pending->card_id);
    json_buffer_append(&buffer, "\",\"interaction_id\":\"");
    json_buffer_append(&buffer, pending->interaction_id);
    json_buffer_append(&buffer, "\",\"phase\":\"");
    json_buffer_append(&buffer, phase);
    json_buffer_append_format(
        &buffer,
        "\",\"sequence\":%llu",
        (unsigned long long)pending->paint_sequence);
    append_token_field(&buffer);
    json_buffer_append(&buffer, "}\n");
    bool queued = json_buffer_enqueue(&buffer, false);
    bool retry = false;
    if (queued) {
        ZeroMemory(pending, sizeof(*pending));
    } else if (pending->retry_count < 3u) {
        pending->retry_count++;
        retry = true;
    } else {
        ZeroMemory(pending, sizeof(*pending));
    }
    return retry;
}

static void emit_pending_render_ack_after_paint(ULONGLONG painted_revision) {
    if (!g_visible || g_collapsed) return;

    AcquireSRWLockExclusive(&g_render_ack_lock);
    if (g_presentation_revision != painted_revision) {
        ReleaseSRWLockExclusive(&g_render_ack_lock);
        InvalidateRect(g_hwnd, NULL, FALSE);
        return;
    }
    bool retry = emit_pending_render_ack_locked(
        &g_pending_first_text_ack, painted_revision);
    retry = emit_pending_render_ack_locked(
        &g_pending_final_ack, painted_revision) || retry;
    bool pending = g_pending_first_text_ack.active || g_pending_final_ack.active;
    ReleaseSRWLockExclusive(&g_render_ack_lock);
    if (retry && g_hwnd) SetTimer(g_hwnd, ID_RENDER_ACK_RETRY_TIMER, 25, NULL);
    else if (!pending && g_hwnd) KillTimer(g_hwnd, ID_RENDER_ACK_RETRY_TIMER);
}

static bool emit_ask_event(const wchar_t *question, bool answer_current_transcript) {
    ULONGLONG initiated_at_unix_ms = current_unix_time_ms();
    char *utf8 = wide_to_utf8_alloc(question);
    if (!utf8) return false;
    char interaction_id[37];
    if (!generate_interaction_id(interaction_id)) {
        free(utf8);
        return false;
    }

    BlueyJsonBuffer buffer;
    json_buffer_init_limited(
        &buffer, 4096u, BLUEY_UI_ASK_EVENT_MAX_BYTES);
    json_buffer_append(&buffer, "{\"type\":\"ask_requested\",\"question\":\"");
    json_buffer_append_escaped(&buffer, utf8);
    free(utf8);
    json_buffer_append(&buffer, "\",\"interaction_id\":\"");
    json_buffer_append(&buffer, interaction_id);
    json_buffer_append_format(
        &buffer,
        "\",\"initiated_at_unix_ms\":%llu",
        (unsigned long long)initiated_at_unix_ms);
    if (g_answer_detail_mode == 1) {
        json_buffer_append(
            &buffer,
            ",\"provider\":\"managed\",\"model\":\"instant\",\"mode\":\"instant\"");
    } else if (g_answer_detail_mode == 2) {
        json_buffer_append(
            &buffer,
            ",\"provider\":\"managed\",\"model\":\"deep\",\"mode\":\"deep\"");
    } else {
        json_buffer_append(
            &buffer,
            ",\"provider\":\"auto\",\"model\":\"\",\"mode\":\"general\"");
    }
    bool wrote_context_id = false;
    for (int i = 0; i < g_context_chip_count; i++) {
        if (!g_context_chips[i].pending || g_context_chips[i].id[0] == L'\0') continue;
        char *context_id = wide_to_utf8_alloc(g_context_chips[i].id);
        if (!context_id) continue;
        json_buffer_append(
            &buffer,
            wrote_context_id ? ",\"" : ",\"visible_context_ids\":[\"");
        json_buffer_append_escaped(&buffer, context_id);
        json_buffer_append_char(&buffer, '"');
        free(context_id);
        wrote_context_id = true;
    }
    if (wrote_context_id) json_buffer_append_char(&buffer, ']');
    json_buffer_append(
        &buffer,
        ask_event_current_transcript_json_field(answer_current_transcript));
    append_token_field(&buffer);
    json_buffer_append(&buffer, "}\n");
    return json_buffer_enqueue(&buffer, false);
}

static bool emit_paste_text_event(const wchar_t *text) {
    BlueyJsonBuffer buffer;
    json_buffer_init_limited(
        &buffer, 4096u, BLUEY_UI_BULK_EVENT_MAX_BYTES);
    json_buffer_append(&buffer, "{\"type\":\"paste_text_requested\",\"text\":\"");
    json_buffer_append_wide_escaped(&buffer, text);
    json_buffer_append_char(&buffer, '"');
    append_token_field(&buffer);
    json_buffer_append(&buffer, "}\n");
    return json_buffer_enqueue(&buffer, false);
}

static bool emit_meeting_banner_action(const char *action) {
    BlueyJsonBuffer buffer;
    json_buffer_init_limited(
        &buffer, 1024u, BLUEY_UI_METADATA_EVENT_MAX_BYTES);
    json_buffer_append(
        &buffer,
        "{\"type\":\"meeting_banner_action\",\"candidate_id\":\"");
    json_buffer_append_wide_escaped(&buffer, g_meeting_candidate_id);
    json_buffer_append(&buffer, "\",\"action\":\"");
    json_buffer_append_escaped(&buffer, action ? action : "dismiss");
    json_buffer_append(&buffer, "\",\"app_id\":\"");
    json_buffer_append_wide_escaped(&buffer, g_meeting_candidate_app_id);
    json_buffer_append_char(&buffer, '"');
    if (g_meeting_candidate_provider[0] != L'\0') {
        json_buffer_append(&buffer, ",\"provider\":\"");
        json_buffer_append_wide_escaped(&buffer, g_meeting_candidate_provider);
        json_buffer_append_char(&buffer, '"');
    }
    append_token_field(&buffer);
    json_buffer_append(&buffer, "}\n");
    return json_buffer_enqueue(&buffer, false);
}

static bool emit_meeting_evidence_event(const MeetingEvidencePayload *evidence) {
    if (!evidence || evidence->app_id[0] == L'\0') return false;
    BlueyJsonBuffer buffer;
    json_buffer_init_limited(
        &buffer, 2048u, BLUEY_UI_METADATA_EVENT_MAX_BYTES);
    json_buffer_append(
        &buffer,
        "{\"type\":\"meeting_evidence_observed\",\"evidence\":{"
        "\"source\":\"wasapi_session\",\"app_name\":\"");
    json_buffer_append_wide_escaped(&buffer, evidence->app_name);
    json_buffer_append(&buffer, "\",\"app_id\":\"");
    json_buffer_append_wide_escaped(&buffer, evidence->app_id);
    json_buffer_append_format(
        &buffer,
        "\",\"process_id\":%lu,\"audio_input_active\":%s,"
        "\"audio_output_active\":%s,\"app_foreground\":%s,"
        "\"browser\":%s,\"dedicated_meeting_app\":%s,"
        "\"observed_at_unix_ms\":%lld",
        (unsigned long)evidence->process_id,
        evidence->audio_input_active ? "true" : "false",
        evidence->audio_output_active ? "true" : "false",
        evidence->app_foreground ? "true" : "false",
        evidence->browser ? "true" : "false",
        evidence->dedicated_meeting_app ? "true" : "false",
        evidence->observed_at_unix_ms);
    if (evidence->provider[0] != L'\0') {
        json_buffer_append(&buffer, ",\"provider\":\"");
        json_buffer_append_wide_escaped(&buffer, evidence->provider);
        json_buffer_append_char(&buffer, '"');
    }
    if (evidence->window_title[0] != L'\0') {
        json_buffer_append(&buffer, ",\"window_title\":\"");
        json_buffer_append_wide_escaped(&buffer, evidence->window_title);
        json_buffer_append_char(&buffer, '"');
    }
    json_buffer_append_char(&buffer, '}');
    append_token_field(&buffer);
    json_buffer_append(&buffer, "}\n");
    return json_buffer_enqueue(&buffer, true);
}

typedef struct MeetingAppIdentity {
    wchar_t app_name[160];
    wchar_t app_id[260];
    bool browser;
    bool dedicated_meeting_app;
} MeetingAppIdentity;

static bool process_image_basename(DWORD process_id, wchar_t *dest, size_t dest_len) {
    if (!dest || dest_len == 0 || process_id == 0) return false;
    dest[0] = L'\0';
    HANDLE process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, process_id);
    if (!process) return false;
    DWORD length = (DWORD)dest_len;
    bool ok = QueryFullProcessImageNameW(process, 0, dest, &length) != FALSE;
    CloseHandle(process);
    if (!ok || dest[0] == L'\0') return false;
    const wchar_t *backslash = wcsrchr(dest, L'\\');
    const wchar_t *slash = wcsrchr(dest, L'/');
    const wchar_t *base = backslash;
    if (!base || (slash && slash > base)) base = slash;
    if (base) {
        memmove(dest, base + 1, (wcslen(base + 1) + 1) * sizeof(wchar_t));
    }
    return true;
}

static bool classify_meeting_process(DWORD process_id, MeetingAppIdentity *identity) {
    if (!identity) return false;
    ZeroMemory(identity, sizeof(*identity));
    wchar_t executable[260];
    if (!process_image_basename(process_id, executable, 260)) return false;

    const wchar_t *app_name = NULL;
    const wchar_t *app_id = NULL;
    bool browser = false;
    bool dedicated = false;
    if (_wcsicmp(executable, L"chrome.exe") == 0) {
        app_name = L"Google Chrome";
        app_id = L"chrome.exe";
        browser = true;
    } else if (_wcsicmp(executable, L"msedge.exe") == 0) {
        app_name = L"Microsoft Edge";
        app_id = L"msedge.exe";
        browser = true;
    } else if (_wcsicmp(executable, L"firefox.exe") == 0) {
        app_name = L"Firefox";
        app_id = L"firefox.exe";
        browser = true;
    } else if (_wcsicmp(executable, L"brave.exe") == 0) {
        app_name = L"Brave";
        app_id = L"brave.exe";
        browser = true;
    } else if (_wcsicmp(executable, L"arc.exe") == 0) {
        app_name = L"Arc";
        app_id = L"arc.exe";
        browser = true;
    } else if (_wcsicmp(executable, L"zoom.exe") == 0) {
        app_name = L"Zoom";
        app_id = L"zoom.exe";
        dedicated = true;
    } else if (_wcsicmp(executable, L"teams.exe") == 0
               || _wcsicmp(executable, L"ms-teams.exe") == 0
               || _wcsicmp(executable, L"msteams.exe") == 0) {
        app_name = L"Microsoft Teams";
        app_id = L"ms-teams.exe";
        dedicated = true;
    } else if (_wcsicmp(executable, L"slack.exe") == 0) {
        app_name = L"Slack";
        app_id = L"slack.exe";
        dedicated = true;
    } else if (wide_contains_ci(executable, L"webex")
               || _wcsicmp(executable, L"CiscoCollabHost.exe") == 0) {
        app_name = L"Webex";
        app_id = L"webex.exe";
        dedicated = true;
    } else if (wide_contains_ci(executable, L"gotomeeting")) {
        app_name = L"GoTo Meeting";
        app_id = L"gotomeeting.exe";
        dedicated = true;
    } else if (wide_contains_ci(executable, L"whereby")) {
        app_name = L"Whereby";
        app_id = L"whereby.exe";
        dedicated = true;
    } else {
        return false;
    }

    wcscpy_s(identity->app_name, 160, app_name);
    wcscpy_s(identity->app_id, 260, app_id);
    identity->browser = browser;
    identity->dedicated_meeting_app = dedicated;
    return true;
}

static bool meeting_provider_from_text(
    const wchar_t *text,
    wchar_t *provider,
    size_t provider_len
) {
    if (!text || !provider || provider_len == 0) return false;
    provider[0] = L'\0';
    const wchar_t *value = NULL;
    if (wide_contains_ci(text, L"meet.google.com")
        || wide_contains_ci(text, L"google meet")) {
        value = L"google_meet";
    } else if (wide_contains_ci(text, L"zoom.us")
               || wide_contains_ci(text, L"zoom meeting")) {
        value = L"zoom";
    } else if (wide_contains_ci(text, L"teams.microsoft")
               || wide_contains_ci(text, L"microsoft teams")
               || wide_contains_ci(text, L"teams meeting")) {
        value = L"microsoft_teams";
    } else if (wide_contains_ci(text, L"webex")) {
        value = L"webex";
    } else if (wide_contains_ci(text, L"slack huddle")
               || wide_contains_ci(text, L"huddle | slack")) {
        value = L"slack_huddle";
    } else if (wide_contains_ci(text, L"whereby")) {
        value = L"whereby";
    } else if (wide_contains_ci(text, L"gotomeeting")
               || wide_contains_ci(text, L"go to meeting")) {
        value = L"gotomeeting";
    }
    if (!value) return false;
    wcscpy_s(provider, provider_len, value);
    return true;
}

static void dedicated_provider(const wchar_t *app_id, wchar_t *provider, size_t provider_len) {
    if (!provider || provider_len == 0) return;
    provider[0] = L'\0';
    if (wide_contains_ci(app_id, L"zoom")) {
        wcscpy_s(provider, provider_len, L"zoom");
    } else if (wide_contains_ci(app_id, L"teams")) {
        wcscpy_s(provider, provider_len, L"microsoft_teams");
    } else if (wide_contains_ci(app_id, L"slack")) {
        wcscpy_s(provider, provider_len, L"slack_huddle");
    } else if (wide_contains_ci(app_id, L"webex")) {
        wcscpy_s(provider, provider_len, L"webex");
    } else if (wide_contains_ci(app_id, L"whereby")) {
        wcscpy_s(provider, provider_len, L"whereby");
    } else if (wide_contains_ci(app_id, L"gotomeeting")) {
        wcscpy_s(provider, provider_len, L"gotomeeting");
    }
}

typedef struct MeetingWindowSearch {
    const wchar_t *app_id;
    wchar_t provider[96];
    wchar_t title[520];
} MeetingWindowSearch;

static BOOL CALLBACK find_meeting_window_callback(HWND hwnd, LPARAM context_ptr) {
    MeetingWindowSearch *search = (MeetingWindowSearch *)context_ptr;
    if (!search || search->provider[0] != L'\0' || !IsWindowVisible(hwnd)) return TRUE;
    DWORD process_id = 0;
    GetWindowThreadProcessId(hwnd, &process_id);
    MeetingAppIdentity identity;
    if (!classify_meeting_process(process_id, &identity)
        || _wcsicmp(identity.app_id, search->app_id) != 0) {
        return TRUE;
    }
    wchar_t title[520];
    if (GetWindowTextW(hwnd, title, 520) <= 0) return TRUE;
    if (!meeting_provider_from_text(title, search->provider, 96)) return TRUE;
    wcscpy_s(search->title, 520, title);
    return FALSE;
}

static void find_meeting_window(
    const wchar_t *app_id,
    wchar_t *provider,
    size_t provider_len,
    wchar_t *title,
    size_t title_len
) {
    MeetingWindowSearch search;
    ZeroMemory(&search, sizeof(search));
    search.app_id = app_id;
    EnumWindows(find_meeting_window_callback, (LPARAM)&search);
    if (provider && provider_len > 0) wcscpy_s(provider, provider_len, search.provider);
    if (title && title_len > 0) wcscpy_s(title, title_len, search.title);
}

typedef struct AudioPidActivity {
    DWORD process_id;
    bool input_active;
    bool output_active;
} AudioPidActivity;

static void merge_audio_pid(
    AudioPidActivity *activities,
    size_t *count,
    size_t capacity,
    DWORD process_id,
    bool input_active,
    bool output_active
) {
    if (!activities || !count || process_id == 0) return;
    for (size_t i = 0; i < *count; i++) {
        if (activities[i].process_id == process_id) {
            activities[i].input_active = activities[i].input_active || input_active;
            activities[i].output_active = activities[i].output_active || output_active;
            return;
        }
    }
    if (*count >= capacity) return;
    activities[*count].process_id = process_id;
    activities[*count].input_active = input_active;
    activities[*count].output_active = output_active;
    (*count)++;
}

static void collect_device_audio_sessions(
    IMMDevice *device,
    bool input,
    AudioPidActivity *activities,
    size_t *activity_count,
    size_t activity_capacity
) {
    if (!device) return;
    IAudioSessionManager2 *manager = NULL;
    HRESULT hr = device->Activate(
        __uuidof(IAudioSessionManager2),
        CLSCTX_ALL,
        NULL,
        (void **)&manager);
    if (FAILED(hr) || !manager) return;

    IAudioSessionEnumerator *session_enumerator = NULL;
    hr = manager->GetSessionEnumerator(&session_enumerator);
    manager->Release();
    if (FAILED(hr) || !session_enumerator) return;

    int session_count = 0;
    if (FAILED(session_enumerator->GetCount(&session_count))) {
        session_enumerator->Release();
        return;
    }
    for (int index = 0; index < session_count; index++) {
        IAudioSessionControl *control = NULL;
        if (FAILED(session_enumerator->GetSession(index, &control)) || !control) continue;
        AudioSessionState state = AudioSessionStateInactive;
        bool active = SUCCEEDED(control->GetState(&state)) && state == AudioSessionStateActive;
        if (active) {
            IAudioSessionControl2 *control2 = NULL;
            if (SUCCEEDED(control->QueryInterface(
                    __uuidof(IAudioSessionControl2),
                    (void **)&control2))
                && control2) {
                DWORD process_id = 0;
                if (SUCCEEDED(control2->GetProcessId(&process_id))) {
                    merge_audio_pid(
                        activities,
                        activity_count,
                        activity_capacity,
                        process_id,
                        input,
                        !input);
                }
                control2->Release();
            }
        }
        control->Release();
    }
    session_enumerator->Release();
}

static void collect_endpoint_audio_sessions(
    IMMDeviceEnumerator *device_enumerator,
    EDataFlow flow,
    bool input,
    AudioPidActivity *activities,
    size_t *activity_count,
    size_t activity_capacity
) {
    if (!device_enumerator) return;
    const ERole roles[] = { eCommunications, eMultimedia };
    wchar_t previous_device_id[520] = L"";
    for (size_t role_index = 0; role_index < sizeof(roles) / sizeof(roles[0]); role_index++) {
        IMMDevice *device = NULL;
        HRESULT hr = device_enumerator->GetDefaultAudioEndpoint(
            flow,
            roles[role_index],
            &device);
        if (FAILED(hr) || !device) continue;

        LPWSTR device_id = NULL;
        bool duplicate = false;
        if (SUCCEEDED(device->GetId(&device_id)) && device_id) {
            duplicate = previous_device_id[0] != L'\0'
                && _wcsicmp(previous_device_id, device_id) == 0;
            if (!duplicate) {
                wcsncpy_s(previous_device_id, 520, device_id, _TRUNCATE);
            }
            CoTaskMemFree(device_id);
        }
        if (!duplicate) {
            collect_device_audio_sessions(
                device,
                input,
                activities,
                activity_count,
                activity_capacity);
        }
        device->Release();
    }
}

static LONGLONG unix_time_millis(void) {
    FILETIME file_time;
    GetSystemTimeAsFileTime(&file_time);
    ULARGE_INTEGER ticks;
    ticks.LowPart = file_time.dwLowDateTime;
    ticks.HighPart = file_time.dwHighDateTime;
    const ULONGLONG windows_to_unix_epoch_100ns = 116444736000000000ULL;
    if (ticks.QuadPart <= windows_to_unix_epoch_100ns) return 0;
    return (LONGLONG)((ticks.QuadPart - windows_to_unix_epoch_100ns) / 10000ULL);
}

static bool meeting_detection_enabled(void) {
    return InterlockedCompareExchange(&g_meeting_detection_enabled, 0, 0) != 0;
}

static void sample_windows_meeting_evidence(IMMDeviceEnumerator *device_enumerator) {
    if (!meeting_detection_enabled()) return;
    AudioPidActivity activities[64];
    size_t activity_count = 0;
    ZeroMemory(activities, sizeof(activities));
    collect_endpoint_audio_sessions(
        device_enumerator,
        eCapture,
        true,
        activities,
        &activity_count,
        64);
    collect_endpoint_audio_sessions(
        device_enumerator,
        eRender,
        false,
        activities,
        &activity_count,
        64);

    HWND foreground = GetForegroundWindow();
    DWORD foreground_pid = 0;
    if (foreground) GetWindowThreadProcessId(foreground, &foreground_pid);
    MeetingAppIdentity foreground_identity;
    bool has_foreground_identity = classify_meeting_process(
        foreground_pid,
        &foreground_identity);
    LONGLONG observed_at = unix_time_millis();

    MeetingEvidencePayload candidates[16];
    size_t candidate_count = 0;
    ZeroMemory(candidates, sizeof(candidates));
    for (size_t index = 0; index < activity_count; index++) {
        MeetingAppIdentity identity;
        if (!classify_meeting_process(activities[index].process_id, &identity)) continue;

        size_t candidate_index = candidate_count;
        for (size_t existing = 0; existing < candidate_count; existing++) {
            if (_wcsicmp(candidates[existing].app_id, identity.app_id) == 0) {
                candidate_index = existing;
                break;
            }
        }
        if (candidate_index == candidate_count) {
            if (candidate_count >= 16) continue;
            wcscpy_s(candidates[candidate_index].app_name, 160, identity.app_name);
            wcscpy_s(candidates[candidate_index].app_id, 260, identity.app_id);
            candidates[candidate_index].process_id = activities[index].process_id;
            candidates[candidate_index].browser = identity.browser;
            candidates[candidate_index].dedicated_meeting_app =
                identity.dedicated_meeting_app;
            candidates[candidate_index].observed_at_unix_ms = observed_at;
            candidate_count++;
        }
        candidates[candidate_index].audio_input_active =
            candidates[candidate_index].audio_input_active
            || activities[index].input_active;
        candidates[candidate_index].audio_output_active =
            candidates[candidate_index].audio_output_active
            || activities[index].output_active;
        candidates[candidate_index].app_foreground =
            has_foreground_identity
            && _wcsicmp(foreground_identity.app_id, identity.app_id) == 0;
    }

    for (size_t index = 0; index < candidate_count; index++) {
        if (!meeting_detection_enabled()) return;
        if (candidates[index].browser) {
            find_meeting_window(
                candidates[index].app_id,
                candidates[index].provider,
                96,
                candidates[index].window_title,
                520);
        } else {
            dedicated_provider(
                candidates[index].app_id,
                candidates[index].provider,
                96);
        }
        MeetingEvidencePayload *copy =
            (MeetingEvidencePayload *)malloc(sizeof(MeetingEvidencePayload));
        if (!copy) continue;
        *copy = candidates[index];
        if (!meeting_detection_enabled()
            || !PostMessageW(g_hwnd, WM_BLUEY_MEETING_EVIDENCE, 0, (LPARAM)copy)) {
            free(copy);
        }
    }
}

static DWORD WINAPI meeting_detector_thread(LPVOID unused) {
    HANDLE stop_event = (HANDLE)unused;
    HRESULT com_status = CoInitializeEx(NULL, COINIT_MULTITHREADED);
    IMMDeviceEnumerator *device_enumerator = NULL;
    HRESULT create_status = CoCreateInstance(
        __uuidof(MMDeviceEnumerator),
        NULL,
        CLSCTX_INPROC_SERVER,
        __uuidof(IMMDeviceEnumerator),
        (void **)&device_enumerator);
    if (SUCCEEDED(create_status) && device_enumerator) {
        sample_windows_meeting_evidence(device_enumerator);
        while (meeting_detection_enabled()
            && WaitForSingleObject(stop_event, 750) == WAIT_TIMEOUT) {
            sample_windows_meeting_evidence(device_enumerator);
        }
        device_enumerator->Release();
    }
    if (SUCCEEDED(com_status)) CoUninitialize();
    return 0;
}

static bool start_meeting_detector(void) {
    InterlockedExchange(&g_meeting_detection_enabled, 1);
    AcquireSRWLockExclusive(&g_meeting_detector_lock);
    if (g_meeting_detector_thread) {
        ReleaseSRWLockExclusive(&g_meeting_detector_lock);
        return true;
    }
    HANDLE stop_event = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (!stop_event) {
        InterlockedExchange(&g_meeting_detection_enabled, 0);
        ReleaseSRWLockExclusive(&g_meeting_detector_lock);
        return false;
    }
    HANDLE thread = CreateThread(
        NULL,
        0,
        meeting_detector_thread,
        stop_event,
        0,
        NULL);
    if (!thread) {
        CloseHandle(stop_event);
        InterlockedExchange(&g_meeting_detection_enabled, 0);
        ReleaseSRWLockExclusive(&g_meeting_detector_lock);
        return false;
    }
    g_meeting_detector_stop = stop_event;
    g_meeting_detector_thread = thread;
    ReleaseSRWLockExclusive(&g_meeting_detector_lock);
    return true;
}

static void stop_meeting_detector(void) {
    InterlockedExchange(&g_meeting_detection_enabled, 0);
    AcquireSRWLockExclusive(&g_meeting_detector_lock);
    HANDLE stop_event = g_meeting_detector_stop;
    HANDLE thread = g_meeting_detector_thread;
    g_meeting_detector_stop = NULL;
    g_meeting_detector_thread = NULL;
    if (stop_event) SetEvent(stop_event);
    ReleaseSRWLockExclusive(&g_meeting_detector_lock);

    if (thread) {
        WaitForSingleObject(thread, INFINITE);
        CloseHandle(thread);
    }
    if (stop_event) CloseHandle(stop_event);
}

static void set_meeting_detection_enabled(bool enabled) {
    if (enabled) {
        start_meeting_detector();
    } else {
        InterlockedExchange(&g_meeting_detection_enabled, 0);
        PostMessageW(g_hwnd, WM_BLUEY_MEETING_DETECTION_DISABLED, 0, 0);
        stop_meeting_detector();
    }
}

static wchar_t *drag_query_path_alloc(HDROP drop, UINT index) {
    UINT len = DragQueryFileW(drop, index, NULL, 0);
    if (len == 0) return NULL;
    wchar_t *path = (wchar_t *)calloc((size_t)len + 1, sizeof(wchar_t));
    if (!path) return NULL;
    if (DragQueryFileW(drop, index, path, len + 1) == 0) {
        free(path);
        return NULL;
    }
    return path;
}

static bool supported_drop_extension(const wchar_t *ext) {
    static const wchar_t *supported[] = {
        L"md", L"markdown", L"txt", L"log", L"csv", L"tsv", L"rst", L"adoc",
        L"rs", L"swift", L"c", L"h", L"cpp", L"hpp", L"js", L"jsx", L"ts", L"tsx",
        L"py", L"go", L"java", L"kt", L"kts", L"cs", L"rb", L"php", L"sql", L"sh",
        L"ps1", L"toml", L"yaml", L"yml", L"json", L"html", L"css", L"scss",
        L"pdf", L"doc", L"docx", L"rtf", L"ppt", L"pptx", L"xls", L"xlsx", L"xlsm", L"xlsb", L"ods",
        L"png", L"jpg", L"jpeg", L"gif", L"webp", L"heic", L"heif", L"bmp", L"tiff", L"tif",
    };
    for (size_t i = 0; i < sizeof(supported) / sizeof(supported[0]); i++) {
        if (_wcsicmp(ext, supported[i]) == 0) return true;
    }
    return false;
}

static bool is_supported_drop_path(const wchar_t *path) {
    DWORD attrs = GetFileAttributesW(path);
    if (attrs == INVALID_FILE_ATTRIBUTES || (attrs & FILE_ATTRIBUTE_DIRECTORY)) return false;

    const wchar_t *name = wcsrchr(path, L'\\');
    const wchar_t *slash = wcsrchr(path, L'/');
    if (!name || (slash && slash > name)) name = slash;
    name = name ? name + 1 : path;

    const wchar_t *dot = wcsrchr(name, L'.');
    if (!dot || dot[1] == L'\0') return false;
    return supported_drop_extension(dot + 1);
}

static void show_unsupported_drop_message(UINT skipped, UINT total) {
    wcscpy_s(g_title, 256, skipped == total ? L"File type not supported" : L"Some files were skipped");
    wchar_t message[2048];
    if (skipped == total) {
        swprintf_s(message, 2048, L"Bluey cannot use that file type as context yet. %ls", g_supported_drop_formats);
    } else {
        swprintf_s(
            message,
            2048,
            L"Bluey skipped %u file%ls that are not readable context. %ls",
            skipped,
            skipped == 1 ? L"" : L"s",
            g_supported_drop_formats);
    }
    set_body_text(message);
    wcscpy_s(g_kind, 64, L"warning");
    wcscpy_s(g_source, 256, L"");
    wcscpy_s(g_card_id, 80, L"");
    reset_card_update_sequence();
    if (g_visible && !g_collapsed) show_full_overlay(false);
    InvalidateRect(g_hwnd, NULL, TRUE);
}

static void show_supported_drop_loading(UINT count) {
    wcscpy_s(g_title, 256, L"Docs loading");
    set_body_text(
        count == 1 ? L"Indexing dropped document..." : L"Indexing dropped documents...");
    wcscpy_s(g_kind, 64, L"context");
    wcscpy_s(g_source, 256, L"");
    wcscpy_s(g_card_id, 80, L"");
    reset_card_update_sequence();
    if (g_visible && !g_collapsed) show_full_overlay(false);
    InvalidateRect(g_hwnd, NULL, TRUE);
}

static void expect_pending_context_chips(void) {
    g_pending_context_expected_until_ms = GetTickCount64() + 45000ULL;
}

static bool pending_context_expected(void) {
    if (g_pending_context_expected_until_ms == 0) return false;
    if (GetTickCount64() <= g_pending_context_expected_until_ms) return true;
    g_pending_context_expected_until_ms = 0;
    return false;
}

static bool context_chips_visible(void) {
    return (g_show_context_chips || g_pending_context_chips) && g_context_chip_count > 0;
}

static int pending_context_chip_count(void) {
    int count = 0;
    for (int i = 0; i < g_context_chip_count; i++) {
        if (g_context_chips[i].pending && g_context_chips[i].id[0] != L'\0') count++;
    }
    return count;
}

static bool emit_attach_files_event_from_drop(HDROP drop) {
    UINT count = DragQueryFileW(drop, 0xFFFFFFFFu, NULL, 0);
    if (count == 0) return false;
    if (count > 64) count = 64;

    UINT supported_count = 0;
    UINT skipped_count = 0;
    for (UINT i = 0; i < count; i++) {
        wchar_t *path = drag_query_path_alloc(drop, i);
        if (!path) continue;
        if (is_supported_drop_path(path)) supported_count++;
        else skipped_count++;
        free(path);
    }

    if (skipped_count > 0) show_unsupported_drop_message(skipped_count, supported_count + skipped_count);
    if (supported_count == 0) return false;

    BlueyJsonBuffer buffer;
    json_buffer_init_limited(
        &buffer, 4096u, BLUEY_UI_BULK_EVENT_MAX_BYTES);
    json_buffer_append(&buffer, "{\"type\":\"attach_files_requested\",\"paths\":[");
    bool emitted = false;
    for (UINT i = 0; i < count; i++) {
        wchar_t *path = drag_query_path_alloc(drop, i);
        if (!path) continue;
        if (!is_supported_drop_path(path)) {
            free(path);
            continue;
        }

        char *utf8 = wide_to_utf8_alloc(path);
        free(path);
        if (!utf8) continue;

        if (emitted) json_buffer_append_char(&buffer, ',');
        json_buffer_append_char(&buffer, '"');
        json_buffer_append_escaped(&buffer, utf8);
        json_buffer_append_char(&buffer, '"');
        free(utf8);
        emitted = true;
    }
    json_buffer_append_char(&buffer, ']');
    append_token_field(&buffer);
    json_buffer_append(&buffer, "}\n");
    if (!emitted) {
        json_buffer_dispose(&buffer);
        return false;
    }
    if (!json_buffer_enqueue(&buffer, false)) return false;

    if (skipped_count == 0) show_supported_drop_loading(supported_count);
    g_show_context_chips = false;
    g_pending_context_chips = false;
    expect_pending_context_chips();
    return true;
}

static void update_record_button(void) {
    if (g_record_button) {
        SetWindowTextW(g_record_button, g_recording ? L"Stop" : L"Mic");
        InvalidateRect(g_record_button, NULL, TRUE);
    }
}

static void update_auto_send_control(void) {
    if (g_auto_send_combo) {
        SendMessageW(g_auto_send_combo, CB_SETCURSEL, (WPARAM)g_auto_send_mode, 0);
        InvalidateRect(g_auto_send_combo, NULL, TRUE);
    }
}

static bool has_transcript_context(void) {
    return g_transcript_final[0] != L'\0' || g_transcript_partial[0] != L'\0';
}

static bool transcript_text_has_prefix(const wchar_t *prefix) {
    size_t prefix_len = wcslen(prefix);
    if (wcsncmp(g_transcript_final, prefix, prefix_len) == 0) return true;
    if (wcsncmp(g_transcript_partial, prefix, prefix_len) == 0) return true;
    return false;
}

static const wchar_t *transcript_source_label_for_answer(void) {
    if (wcsstr(g_transcript_source, L"microphone") != NULL
        || wcsstr(g_transcript_source, L"user") != NULL
        || wcsstr(g_transcript_source, L"Mic") != NULL
        || transcript_text_has_prefix(L"Mic:")) {
        return L"Mic";
    }
    if (wcsstr(g_transcript_source, L"system") != NULL
        || wcsstr(g_transcript_source, L"System") != NULL
        || transcript_text_has_prefix(L"System:")) {
        return L"System";
    }
    return L"Audio";
}

static bool transcript_question_has_source_label(const wchar_t *text) {
    if (!text) return false;
    return _wcsnicmp(text, L"Mic:", 4) == 0
        || _wcsnicmp(text, L"Microphone:", 11) == 0
        || _wcsnicmp(text, L"System:", 7) == 0
        || _wcsnicmp(text, L"Speaker:", 8) == 0
        || _wcsnicmp(text, L"Audio:", 6) == 0;
}

static void prefix_transcript_source_label(wchar_t *text, size_t capacity) {
    if (!text || text[0] == L'\0' || transcript_question_has_source_label(text)) return;
    wchar_t original[1024];
    wcscpy_s(original, 1024, text);
    wchar_t prefix[24];
    swprintf_s(prefix, 24, L"%ls: ", transcript_source_label_for_answer());
    size_t prefix_len = wcslen(prefix);
    if (capacity <= prefix_len + 1) return;
    size_t max_body = capacity - prefix_len - 1;
    size_t body_len = wcslen(original);
    const wchar_t *body = original;
    if (body_len > max_body) {
        body = original + (body_len - max_body);
    }
    swprintf_s(text, capacity, L"%ls%ls", prefix, body);
}

static bool transcript_source_matches_auto_send_mode(void) {
    if (!has_transcript_context()) return false;
    if (g_auto_send_mode == 3) return true;
    if (g_auto_send_mode == 1) {
        return wcsstr(g_transcript_source, L"microphone") != NULL
            || wcsstr(g_transcript_source, L"user") != NULL
            || wcsstr(g_transcript_source, L"Mic") != NULL
            || transcript_text_has_prefix(L"Mic:");
    }
    if (g_auto_send_mode == 2) {
        return wcsstr(g_transcript_source, L"system") != NULL
            || wcsstr(g_transcript_source, L"System") != NULL
            || transcript_text_has_prefix(L"System:");
    }
    return false;
}

static bool has_auto_send_context(void) {
    if (g_auto_send_mode == 0) return false;
    return transcript_source_matches_auto_send_mode();
}

static void cancel_auto_send_timer(const char *origin) {
    bool had_pending = g_auto_send_timer_armed;
    if (g_hwnd) KillTimer(g_hwnd, ID_AUTOSEND_TIMER);
    g_auto_send_timer_armed = false;
    if (had_pending || has_auto_send_context()) {
        char detail[192];
        snprintf(
            detail,
            sizeof(detail),
            "origin=%s mode=%d pending=%s transcript_context=%s",
            origin ? origin : "unknown",
            g_auto_send_mode,
            had_pending ? "true" : "false",
            has_transcript_context() ? "true" : "false"
        );
        emit_lifecycle_event("autosend_cancelled", "ok", detail);
    }
}

static void schedule_auto_send_after_caption_settled(void) {
    if (!g_hwnd || !g_recording || !has_auto_send_context()) return;
    KillTimer(g_hwnd, ID_AUTOSEND_TIMER);
    SetTimer(g_hwnd, ID_AUTOSEND_TIMER, AUTOSEND_CAPTION_SETTLE_DELAY_MS, NULL);
    g_auto_send_timer_armed = true;
    char detail[160];
    snprintf(
        detail,
        sizeof(detail),
        "mode=%d delay_ms=%u transcript_context=%s",
        g_auto_send_mode,
        AUTOSEND_CAPTION_SETTLE_DELAY_MS,
        has_transcript_context() ? "true" : "false"
    );
    emit_lifecycle_event("autosend_answer_scheduled", "ok", detail);
}

static void update_transcript_clear_button(void) {
    if (!g_transcript_clear_button) return;
    ShowWindow(
        g_transcript_clear_button,
        (!g_collapsed && has_transcript_context()) ? SW_SHOW : SW_HIDE
    );
}

static bool has_pasteable_answer(void) {
    return false;
}

static void update_paste_answer_button(void) {
    if (!g_paste_answer_button) return;
    ShowWindow(g_paste_answer_button, SW_HIDE);
    EnableWindow(g_paste_answer_button, FALSE);
    InvalidateRect(g_paste_answer_button, NULL, TRUE);
}

static void draw_dark_button(const DRAWITEMSTRUCT *item) {
    wchar_t text[96];
    GetWindowTextW(item->hwndItem, text, 96);

    bool pressed = (item->itemState & ODS_SELECTED) != 0;
    HBRUSH bg = CreateSolidBrush(
        g_light_theme
            ? (pressed ? RGB(BLUEY_LIGHT_ACCENT_SOFT_R, BLUEY_LIGHT_ACCENT_SOFT_G, BLUEY_LIGHT_ACCENT_SOFT_B) : RGB(244, 249, 252))
            : (pressed ? RGB(34, 47, 58) : RGB(10, 15, 22))
    );
    HPEN border = CreatePen(PS_SOLID, 1, g_light_theme ? RGB(BLUEY_LIGHT_ACCENT_R, BLUEY_LIGHT_ACCENT_G, BLUEY_LIGHT_ACCENT_B) : RGB(45, 91, 112));
    HGDIOBJ old_brush = SelectObject(item->hDC, bg);
    HGDIOBJ old_pen = SelectObject(item->hDC, border);
    RoundRect(item->hDC, item->rcItem.left, item->rcItem.top, item->rcItem.right, item->rcItem.bottom, 18, 18);
    SelectObject(item->hDC, old_brush);
    SelectObject(item->hDC, old_pen);
    DeleteObject(bg);
    DeleteObject(border);

    bool focused = ((item->itemState & ODS_FOCUS) != 0) || GetFocus() == item->hwndItem;
    if (focused) {
        HPEN focus_pen = CreatePen(PS_SOLID, 2, g_light_theme ? RGB(0, 118, 184) : RGB(86, 218, 255));
        HGDIOBJ old_focus_pen = SelectObject(item->hDC, focus_pen);
        HGDIOBJ old_focus_brush = SelectObject(item->hDC, GetStockObject(NULL_BRUSH));
        RECT focus_rect = item->rcItem;
        InflateRect(&focus_rect, -2, -2);
        RoundRect(item->hDC, focus_rect.left, focus_rect.top, focus_rect.right, focus_rect.bottom, 16, 16);
        SelectObject(item->hDC, old_focus_brush);
        SelectObject(item->hDC, old_focus_pen);
        DeleteObject(focus_pen);
    }

    SetBkMode(item->hDC, TRANSPARENT);
    SetTextColor(item->hDC, g_light_theme ? RGB(8, 22, 32) : RGB(234, 240, 246));
    HFONT font = CreateFontW(15, 0, 0, 0, FW_SEMIBOLD, FALSE, FALSE, FALSE,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
        DEFAULT_PITCH | FF_SWISS, L"Segoe UI");
    HGDIOBJ old_font = SelectObject(item->hDC, font);
    RECT text_rect = item->rcItem;
    DrawTextW(item->hDC, text, -1, &text_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
    SelectObject(item->hDC, old_font);
    DeleteObject(font);
}

static void refresh_edit_brush(void) {
    if (g_edit_brush) {
        DeleteObject(g_edit_brush);
        g_edit_brush = NULL;
    }
    g_edit_brush = CreateSolidBrush(g_light_theme ? RGB(248, 252, 255) : RGB(4, 8, 13));
}

static void draw_bluey_logo(HDC hdc, int x, int y, int size) {
    HBRUSH bg = CreateSolidBrush(RGB(12, 52, 78));
    HPEN border = CreatePen(PS_SOLID, 1, RGB(120, 236, 246));
    HGDIOBJ old_brush = SelectObject(hdc, bg);
    HGDIOBJ old_pen = SelectObject(hdc, border);
    RoundRect(hdc, x, y, x + size, y + size, 9, 9);
    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(bg);
    DeleteObject(border);

    HBRUSH panel = CreateSolidBrush(RGB(6, 17, 31));
    HPEN panel_border = CreatePen(PS_SOLID, 2, RGB(100, 233, 255));
    old_brush = SelectObject(hdc, panel);
    old_pen = SelectObject(hdc, panel_border);
    RoundRect(hdc, x + size / 5, y + size / 3, x + size - size / 5, y + size - size / 4, 8, 8);
    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(panel);
    DeleteObject(panel_border);

    HPEN white = CreatePen(PS_SOLID, 2, RGB(245, 252, 255));
    old_pen = SelectObject(hdc, white);
    MoveToEx(hdc, x + size * 34 / 100, y + size * 45 / 100, NULL);
    LineTo(hdc, x + size * 44 / 100, y + size * 50 / 100);
    LineTo(hdc, x + size * 34 / 100, y + size * 58 / 100);
    SelectObject(hdc, old_pen);
    DeleteObject(white);

    HPEN blue = CreatePen(PS_SOLID, 2, RGB(122, 248, 255));
    old_pen = SelectObject(hdc, blue);
    MoveToEx(hdc, x + size * 50 / 100, y + size * 61 / 100, NULL);
    LineTo(hdc, x + size * 66 / 100, y + size * 61 / 100);
    SelectObject(hdc, old_pen);
    DeleteObject(blue);

    HBRUSH lime = CreateSolidBrush(RGB(139, 255, 157));
    old_brush = SelectObject(hdc, lime);
    POINT spark[] = {
        {x + size * 72 / 100, y + size * 19 / 100},
        {x + size * 76 / 100, y + size * 32 / 100},
        {x + size * 89 / 100, y + size * 36 / 100},
        {x + size * 76 / 100, y + size * 40 / 100},
        {x + size * 72 / 100, y + size * 53 / 100},
        {x + size * 68 / 100, y + size * 40 / 100},
        {x + size * 55 / 100, y + size * 36 / 100},
        {x + size * 68 / 100, y + size * 32 / 100}
    };
    Polygon(hdc, spark, 8);
    SelectObject(hdc, old_brush);
    DeleteObject(lime);
}

static void draw_resize_affordance(HDC hdc, RECT rect) {
    HPEN frame = CreatePen(PS_SOLID, 1, RGB(38, 98, 138));
    HGDIOBJ old_pen = SelectObject(hdc, frame);
    HGDIOBJ old_brush = SelectObject(hdc, GetStockObject(HOLLOW_BRUSH));
    RoundRect(hdc, rect.left + 1, rect.top + 1, rect.right - 1, rect.bottom - 1, 18, 18);
    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(frame);

    HPEN grip = CreatePen(PS_SOLID, 2, RGB(120, 236, 246));
    old_pen = SelectObject(hdc, grip);
    int right = rect.right - 11;
    int bottom = rect.bottom - 10;
    for (int i = 0; i < 3; i++) {
        int offset = 7 + (i * 6);
        MoveToEx(hdc, right - offset, bottom, NULL);
        LineTo(hdc, right, bottom - offset);
    }
    SelectObject(hdc, old_pen);
    DeleteObject(grip);
}

static void set_controls_visible(bool visible) {
    int state = visible ? SW_SHOW : SW_HIDE;
    HWND controls[] = {
        g_ask_edit, g_send_button, g_record_button, g_auto_send_combo, g_answer_detail_combo, g_paste_answer_button, g_help_button, g_session_button,
        g_page_button, g_attach_button, g_recap_button, g_note_button, g_theme_button, g_shortcuts_button, g_close_button
    };
    for (int i = 0; i < (int)(sizeof(controls) / sizeof(controls[0])); i++) {
        if (controls[i]) ShowWindow(controls[i], state);
    }
    if (visible) {
        update_transcript_clear_button();
        update_paste_answer_button();
    } else if (g_transcript_clear_button) {
        ShowWindow(g_transcript_clear_button, SW_HIDE);
        if (g_paste_answer_button) ShowWindow(g_paste_answer_button, SW_HIDE);
    }
}

static int clamp_int(int value, int min_value, int max_value) {
    if (value < min_value) return min_value;
    if (value > max_value) return max_value;
    return value;
}

static bool rect_is_valid(RECT rect) {
    return rect.right > rect.left && rect.bottom > rect.top;
}

static RECT work_area_for_rect(RECT rect) {
    RECT work = {0, 0, 0, 0};
    MONITORINFO monitor = {0};
    monitor.cbSize = sizeof(monitor);
    HMONITOR hmonitor = MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST);
    if (GetMonitorInfoW(hmonitor, &monitor)) {
        work = monitor.rcWork;
    } else {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &work, 0);
    }
    return work;
}

static RECT clamp_rect_to_work_area(RECT rect, int margin) {
    RECT work = work_area_for_rect(rect);
    int width = rect.right - rect.left;
    int height = rect.bottom - rect.top;
    RECT clamped = rect;
    clamped.left = clamp_int(rect.left, work.left + margin, work.right - width - margin);
    clamped.top = clamp_int(rect.top, work.top + margin, work.bottom - height - margin);
    clamped.right = clamped.left + width;
    clamped.bottom = clamped.top + height;
    return clamped;
}

static RECT clamp_expanded_rect_to_focus_area(RECT rect, int margin) {
    RECT work = work_area_for_rect(rect);
    int work_w = work.right - work.left;
    int work_h = work.bottom - work.top;
    int max_w = work_w - margin * 2;
    int max_h = work_h - margin * 2;
    if (max_w < EXPANDED_MIN_WIDTH) max_w = work_w;
    if (max_h < EXPANDED_MIN_HEIGHT) max_h = work_h;
    int min_w = EXPANDED_MIN_WIDTH < max_w ? EXPANDED_MIN_WIDTH : max_w;
    int min_h = EXPANDED_MIN_HEIGHT < max_h ? EXPANDED_MIN_HEIGHT : max_h;
    int width = rect.right - rect.left;
    int height = rect.bottom - rect.top;
    width = clamp_int(width, min_w, max_w);
    height = clamp_int(height, min_h, max_h);

    RECT clamped = rect;
    clamped.right = clamped.left + width;
    clamped.bottom = clamped.top + height;
    return clamp_rect_to_work_area(clamped, margin);
}

static void save_overlay_rect(const wchar_t *name, RECT rect) {
    if (!rect_is_valid(rect)) return;
    HKEY key;
    if (RegCreateKeyExW(
            HKEY_CURRENT_USER,
            L"Software\\Bluey\\Overlay",
            0,
            NULL,
            0,
            KEY_SET_VALUE,
            NULL,
            &key,
            NULL) != ERROR_SUCCESS) {
        return;
    }
    RegSetValueExW(key, name, 0, REG_BINARY, (const BYTE *)&rect, sizeof(rect));
    RegCloseKey(key);
}

static bool load_overlay_rect(const wchar_t *name, RECT *rect) {
    HKEY key;
    if (RegOpenKeyExW(
            HKEY_CURRENT_USER,
            L"Software\\Bluey\\Overlay",
            0,
            KEY_QUERY_VALUE,
            &key) != ERROR_SUCCESS) {
        return false;
    }
    DWORD type = 0;
    DWORD size = sizeof(*rect);
    LONG status = RegQueryValueExW(key, name, NULL, &type, (BYTE *)rect, &size);
    RegCloseKey(key);
    return status == ERROR_SUCCESS
        && type == REG_BINARY
        && size == sizeof(*rect)
        && rect_is_valid(*rect);
}

static void load_overlay_placement(void) {
    RECT loaded;
    if (load_overlay_rect(L"expanded_rect", &loaded)) {
        g_expanded_rect = clamp_expanded_rect_to_focus_area(loaded, EXPANDED_SCREEN_MARGIN);
    }
    if (load_overlay_rect(L"collapsed_rect", &loaded)) {
        g_collapsed_rect = clamp_rect_to_work_area(loaded, 8);
    }
}

static void show_full_overlay(bool emit_event) {
    g_collapsed = false;
    set_controls_visible(true);
    apply_capture_exclusion(g_hwnd);

    if (rect_is_valid(g_expanded_rect)) {
        g_expanded_rect = clamp_expanded_rect_to_focus_area(g_expanded_rect, EXPANDED_SCREEN_MARGIN);
        SetWindowPos(
            g_hwnd,
            HWND_TOPMOST,
            g_expanded_rect.left,
            g_expanded_rect.top,
            g_expanded_rect.right - g_expanded_rect.left,
            g_expanded_rect.bottom - g_expanded_rect.top,
            SWP_SHOWWINDOW | SWP_NOACTIVATE
        );
    } else {
        ShowWindow(g_hwnd, SW_SHOWNOACTIVATE);
    }

    g_visible = true;
    InvalidateRect(g_hwnd, NULL, TRUE);
    if (emit_event) emit_simple_event("shown");
}

static void hide_overlay_completely(bool emit_event) {
    if (!g_hwnd) return;
    if (!g_collapsed && g_visible) {
        GetWindowRect(g_hwnd, &g_expanded_rect);
        g_expanded_rect = clamp_expanded_rect_to_focus_area(g_expanded_rect, EXPANDED_SCREEN_MARGIN);
        save_overlay_rect(L"expanded_rect", g_expanded_rect);
    }

    g_collapsed = false;
    g_visible = false;
    set_controls_visible(false);
    ShowWindow(g_hwnd, SW_HIDE);
    if (emit_event) emit_simple_event("hidden");
}

static void update_meeting_banner_countdown(void) {
    if (!g_meeting_banner || !IsWindowVisible(g_meeting_banner)) return;
    if (!meeting_detection_enabled()) {
        hide_meeting_banner(NULL);
        return;
    }
    ULONGLONG now = GetTickCount64();
    ULONGLONG remaining_ms =
        now >= g_meeting_banner_deadline_ms ? 0 : g_meeting_banner_deadline_ms - now;
    wchar_t label[128];
    swprintf_s(
        label,
        128,
        L"%d%% confidence  \x2022  closes in %llus",
        g_meeting_candidate_confidence,
        (unsigned long long)((remaining_ms + 999) / 1000));
    SetWindowTextW(g_meeting_banner_countdown, label);
    InvalidateRect(g_meeting_banner, NULL, FALSE);
    if (remaining_ms == 0) {
        emit_meeting_banner_action(bluey_meeting_banner_timeout_action());
        hide_meeting_banner(NULL);
    }
}

static void hide_meeting_banner(const wchar_t *candidate_id) {
    if (!g_meeting_banner) return;
    if (candidate_id && candidate_id[0] != L'\0'
        && _wcsicmp(candidate_id, g_meeting_candidate_id) != 0) {
        return;
    }
    KillTimer(g_meeting_banner, ID_MEETING_BANNER_TIMER);
    ShowWindow(g_meeting_banner, SW_HIDE);
    g_meeting_candidate_id[0] = L'\0';
    g_meeting_candidate_app_id[0] = L'\0';
    g_meeting_candidate_provider[0] = L'\0';
    g_meeting_banner_started_ms = 0;
    g_meeting_banner_deadline_ms = 0;
}

static const wchar_t *meeting_provider_display_name(const wchar_t *provider) {
    if (!provider) return NULL;
    if (_wcsicmp(provider, L"google_meet") == 0) return L"Google Meet detected";
    if (_wcsicmp(provider, L"zoom") == 0) return L"Zoom detected";
    if (_wcsicmp(provider, L"microsoft_teams") == 0) return L"Microsoft Teams detected";
    if (_wcsicmp(provider, L"webex") == 0) return L"Webex detected";
    if (_wcsicmp(provider, L"slack_huddle") == 0) return L"Slack Huddle detected";
    if (_wcsicmp(provider, L"whereby") == 0) return L"Whereby detected";
    if (_wcsicmp(provider, L"gotomeeting") == 0) return L"GoTo Meeting detected";
    return NULL;
}

static void show_meeting_banner_from_json(const char *line, size_t line_len) {
    if (!g_meeting_banner) return;
    const char *candidate = NULL;
    size_t candidate_len = 0;
    if (!json_extract_object(line, line_len, "candidate", &candidate, &candidate_len)) return;

    wchar_t candidate_id[260] = L"";
    wchar_t app_name[160] = L"Meeting app";
    wchar_t app_id[260] = L"";
    wchar_t provider[96] = L"";
    wchar_t reason[620] = L"Bluey detected corroborated meeting activity.";
    safe_extract_json_to_wide(candidate, candidate_len, "candidate_id", candidate_id, 260);
    safe_extract_json_to_wide(candidate, candidate_len, "app_name", app_name, 160);
    safe_extract_json_to_wide(candidate, candidate_len, "app_id", app_id, 260);
    safe_extract_json_to_wide(candidate, candidate_len, "provider", provider, 96);
    safe_extract_json_to_wide(candidate, candidate_len, "reason", reason, 620);
    if (candidate_id[0] == L'\0' || app_id[0] == L'\0') return;

    double confidence = 0;
    double timeout_secs = 12;
    safe_extract_json_number(candidate, candidate_len, "confidence", &confidence);
    safe_extract_json_number(line, line_len, "timeout_secs", &timeout_secs);
    int timeout = clamp_int((int)timeout_secs, 3, 60);
    g_meeting_candidate_confidence = clamp_int((int)confidence, 0, 100);
    wcscpy_s(g_meeting_candidate_id, 260, candidate_id);
    wcscpy_s(g_meeting_candidate_app_id, 260, app_id);
    wcscpy_s(g_meeting_candidate_provider, 96, provider);

    const wchar_t *provider_title = meeting_provider_display_name(provider);
    SetWindowTextW(
        g_meeting_banner_title,
        provider_title ? provider_title : L"Meeting detected");
    SetWindowTextW(g_meeting_banner_app, app_name);
    SetWindowTextW(g_meeting_banner_reason, reason);
    g_meeting_banner_started_ms = GetTickCount64();
    g_meeting_banner_deadline_ms =
        g_meeting_banner_started_ms + (ULONGLONG)timeout * 1000ULL;

    POINT cursor;
    GetCursorPos(&cursor);
    MONITORINFO monitor;
    ZeroMemory(&monitor, sizeof(monitor));
    monitor.cbSize = sizeof(monitor);
    HMONITOR active_monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
    RECT work = {0, 0, 1920, 1080};
    if (GetMonitorInfoW(active_monitor, &monitor)) work = monitor.rcWork;
    int width = 570;
    int height = 164;
    int margin = 18;
    SetWindowPos(
        g_meeting_banner,
        HWND_TOPMOST,
        work.right - width - margin,
        work.top + margin,
        width,
        height,
        SWP_SHOWWINDOW | SWP_NOACTIVATE);
    apply_capture_exclusion(g_meeting_banner);
    SetTimer(g_meeting_banner, ID_MEETING_BANNER_TIMER, 100, NULL);
    update_meeting_banner_countdown();
}

static LRESULT CALLBACK meeting_banner_wnd_proc(
    HWND hwnd,
    UINT message,
    WPARAM wparam,
    LPARAM lparam
) {
    switch (message) {
    case WM_MOUSEACTIVATE:
        return MA_NOACTIVATE;
    case WM_ERASEBKGND:
        return 1;
    case WM_CTLCOLORSTATIC: {
        HDC hdc = (HDC)wparam;
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(
            hdc,
            (HWND)lparam == g_meeting_banner_reason
                || (HWND)lparam == g_meeting_banner_countdown
                || (HWND)lparam == g_meeting_banner_app
                ? RGB(164, 177, 188)
                : RGB(236, 249, 255));
        return (LRESULT)GetStockObject(HOLLOW_BRUSH);
    }
    case WM_PAINT: {
        PAINTSTRUCT paint;
        HDC hdc = BeginPaint(hwnd, &paint);
        RECT rect;
        GetClientRect(hwnd, &rect);
        HBRUSH background = CreateSolidBrush(RGB(18, 23, 29));
        HBRUSH accent = CreateSolidBrush(RGB(102, 242, 140));
        HPEN border = CreatePen(PS_SOLID, 1, RGB(45, 91, 104));
        HGDIOBJ old_pen = SelectObject(hdc, border);
        HGDIOBJ old_brush = SelectObject(hdc, background);
        RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, 24, 24);
        SelectObject(hdc, accent);
        RoundRect(hdc, 12, 15, 18, rect.bottom - 14, 6, 6);

        if (g_meeting_banner_deadline_ms > g_meeting_banner_started_ms) {
            ULONGLONG now = GetTickCount64();
            ULONGLONG total =
                g_meeting_banner_deadline_ms - g_meeting_banner_started_ms;
            ULONGLONG remaining =
                now >= g_meeting_banner_deadline_ms
                    ? 0
                    : g_meeting_banner_deadline_ms - now;
            int progress_width =
                (int)(((ULONGLONG)(rect.right - rect.left - 24) * remaining) / total);
            RECT progress_rect = {
                rect.left + 12,
                rect.bottom - 5,
                rect.left + 12 + progress_width,
                rect.bottom - 2,
            };
            FillRect(hdc, &progress_rect, accent);
        }

        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(border);
        DeleteObject(accent);
        DeleteObject(background);
        EndPaint(hwnd, &paint);
        return 0;
    }
    case WM_TIMER:
        if (wparam == ID_MEETING_BANNER_TIMER) {
            update_meeting_banner_countdown();
            return 0;
        }
        break;
    case WM_COMMAND:
        switch (LOWORD(wparam)) {
        case ID_MEETING_START_BUTTON:
            emit_meeting_banner_action("start");
            emit_simple_event("recording_start_requested");
            hide_meeting_banner(NULL);
            return 0;
        case ID_MEETING_SNOOZE_BUTTON:
            emit_meeting_banner_action("snooze");
            hide_meeting_banner(NULL);
            return 0;
        case ID_MEETING_DISMISS_BUTTON:
            emit_meeting_banner_action("dismiss");
            hide_meeting_banner(NULL);
            return 0;
        case ID_MEETING_IGNORE_BUTTON:
            emit_meeting_banner_action("ignore");
            hide_meeting_banner(NULL);
            return 0;
        case ID_MEETING_SETTINGS_BUTTON:
            emit_meeting_banner_action("settings");
            hide_meeting_banner(NULL);
            return 0;
        default:
            break;
        }
        break;
    case WM_CLOSE:
        emit_meeting_banner_action("dismiss");
        hide_meeting_banner(NULL);
        return 0;
    }
    return DefWindowProcW(hwnd, message, wparam, lparam);
}

static void create_meeting_banner(HINSTANCE instance) {
    const wchar_t *class_name = L"BlueyMeetingBannerWindow";
    WNDCLASSW window_class;
    ZeroMemory(&window_class, sizeof(window_class));
    window_class.lpfnWndProc = meeting_banner_wnd_proc;
    window_class.hInstance = instance;
    window_class.lpszClassName = class_name;
    window_class.hCursor = LoadCursor(NULL, IDC_ARROW);
    RegisterClassW(&window_class);

    g_meeting_banner = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        class_name,
        L"Bluey meeting detection",
        WS_POPUP,
        0,
        0,
        570,
        164,
        NULL,
        NULL,
        instance,
        NULL);
    if (!g_meeting_banner) return;

    g_meeting_banner_title = CreateWindowExW(
        0, L"STATIC", L"Meeting detected", WS_CHILD | WS_VISIBLE,
        34, 16, 250, 22, g_meeting_banner, NULL, instance, NULL);
    g_meeting_banner_app = CreateWindowExW(
        0, L"STATIC", L"", WS_CHILD | WS_VISIBLE | SS_RIGHT,
        330, 18, 220, 20, g_meeting_banner, NULL, instance, NULL);
    g_meeting_banner_reason = CreateWindowExW(
        0, L"STATIC", L"", WS_CHILD | WS_VISIBLE,
        34, 44, 516, 34, g_meeting_banner, NULL, instance, NULL);
    g_meeting_banner_countdown = CreateWindowExW(
        0, L"STATIC", L"", WS_CHILD | WS_VISIBLE,
        34, 84, 215, 20, g_meeting_banner, NULL, instance, NULL);
    g_meeting_banner_start = CreateWindowExW(
        0, L"BUTTON", L"Start recording", WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON,
        260, 82, 126, 30, g_meeting_banner,
        (HMENU)(INT_PTR)ID_MEETING_START_BUTTON, instance, NULL);
    g_meeting_banner_snooze = CreateWindowExW(
        0, L"BUTTON", L"Snooze", WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON,
        394, 82, 74, 30, g_meeting_banner,
        (HMENU)(INT_PTR)ID_MEETING_SNOOZE_BUTTON, instance, NULL);
    g_meeting_banner_dismiss = CreateWindowExW(
        0, L"BUTTON", L"Dismiss", WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON,
        476, 82, 74, 30, g_meeting_banner,
        (HMENU)(INT_PTR)ID_MEETING_DISMISS_BUTTON, instance, NULL);
    g_meeting_banner_ignore = CreateWindowExW(
        0, L"BUTTON", L"Ignore app", WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON,
        394, 118, 76, 26, g_meeting_banner,
        (HMENU)(INT_PTR)ID_MEETING_IGNORE_BUTTON, instance, NULL);
    g_meeting_banner_settings = CreateWindowExW(
        0, L"BUTTON", L"Settings", WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON,
        478, 118, 72, 26, g_meeting_banner,
        (HMENU)(INT_PTR)ID_MEETING_SETTINGS_BUTTON, instance, NULL);
    apply_capture_exclusion(g_meeting_banner);
}

static void collapse_to_pill(HWND hwnd, bool emit_event) {
    if (!g_collapsed) {
        GetWindowRect(hwnd, &g_expanded_rect);
        g_expanded_rect = clamp_expanded_rect_to_focus_area(g_expanded_rect, EXPANDED_SCREEN_MARGIN);
        save_overlay_rect(L"expanded_rect", g_expanded_rect);
    }

    int width = 96;
    int height = 42;
    RECT target = {0, 0, 0, 0};
    if (rect_is_valid(g_collapsed_rect)) {
        target = g_collapsed_rect;
        target.right = target.left + width;
        target.bottom = target.top + height;
    } else {
        RECT work = work_area_for_rect(g_expanded_rect);
        int margin = 14;
        target.left = clamp_int(g_expanded_rect.right - width, work.left + margin, work.right - width - margin);
        target.top = clamp_int(g_expanded_rect.top, work.top + margin, work.bottom - height - margin);
        target.right = target.left + width;
        target.bottom = target.top + height;
    }
    target = clamp_rect_to_work_area(target, 8);
    g_collapsed_rect = target;
    save_overlay_rect(L"collapsed_rect", g_collapsed_rect);

    g_collapsed = true;
    g_visible = false;
    set_controls_visible(false);
    SetWindowPos(hwnd, HWND_TOPMOST, target.left, target.top, width, height, SWP_SHOWWINDOW | SWP_NOACTIVATE);
    InvalidateRect(hwnd, NULL, TRUE);
    if (emit_event) emit_simple_event("hidden");
}

static bool rects_intersect(RECT a, RECT b) {
    return a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top;
}

static void move_popup_away_from_overlay(HWND popup) {
    RECT popup_rect;
    if (!GetWindowRect(popup, &popup_rect)) return;

    int width = popup_rect.right - popup_rect.left;
    int height = popup_rect.bottom - popup_rect.top;
    int margin = 28;

    RECT work = {0, 0, 0, 0};
    MONITORINFO monitor = {0};
    monitor.cbSize = sizeof(monitor);
    HMONITOR hmonitor = MonitorFromRect(&g_popup_avoid_rect, MONITOR_DEFAULTTONEAREST);
    if (GetMonitorInfoW(hmonitor, &monitor)) {
        work = monitor.rcWork;
    } else {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &work, 0);
    }

    int centered_y = g_popup_avoid_rect.top + ((g_popup_avoid_rect.bottom - g_popup_avoid_rect.top) - height) / 2;
    int centered_x = g_popup_avoid_rect.left + ((g_popup_avoid_rect.right - g_popup_avoid_rect.left) - width) / 2;
    POINT candidates[] = {
        { g_popup_avoid_rect.right + margin, centered_y },
        { g_popup_avoid_rect.left - width - margin, centered_y },
        { centered_x, g_popup_avoid_rect.bottom + margin },
        { centered_x, g_popup_avoid_rect.top - height - margin },
        { work.left + margin, work.top + margin },
        { work.right - width - margin, work.top + margin },
        { work.left + margin, work.bottom - height - margin },
        { work.right - width - margin, work.bottom - height - margin }
    };

    int max_x = work.right - width;
    int max_y = work.bottom - height;
    if (max_x < work.left) max_x = work.left;
    if (max_y < work.top) max_y = work.top;

    POINT chosen = candidates[0];
    for (int i = 0; i < (int)(sizeof(candidates) / sizeof(candidates[0])); i++) {
        int x = clamp_int(candidates[i].x, work.left, max_x);
        int y = clamp_int(candidates[i].y, work.top, max_y);
        RECT candidate_rect = { x, y, x + width, y + height };
        chosen.x = x;
        chosen.y = y;
        if (!rects_intersect(candidate_rect, g_popup_avoid_rect)) {
            break;
        }
    }

    SetWindowPos(popup, HWND_TOPMOST, chosen.x, chosen.y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
}

static LRESULT CALLBACK popup_hook_proc(int code, WPARAM wparam, LPARAM lparam) {
    if (code == HCBT_ACTIVATE) {
        move_popup_away_from_overlay((HWND)wparam);
        if (g_popup_hook) {
            UnhookWindowsHookEx(g_popup_hook);
            g_popup_hook = NULL;
        }
    }
    return CallNextHookEx(g_popup_hook, code, wparam, lparam);
}

static int overlay_message_box(LPCWSTR text, LPCWSTR title, UINT flags) {
    GetWindowRect(g_hwnd, &g_popup_avoid_rect);
    g_popup_hook = SetWindowsHookExW(WH_CBT, popup_hook_proc, NULL, GetCurrentThreadId());
    int result = MessageBoxW(g_hwnd, text, title, flags | MB_TOPMOST | MB_SETFOREGROUND);
    if (g_popup_hook) {
        UnhookWindowsHookEx(g_popup_hook);
        g_popup_hook = NULL;
    }
    return result;
}

static void set_window_opacity(double opacity) {
    if (opacity < 0.18) opacity = 0.18;
    if (opacity > 1.0) opacity = 1.0;
    g_opacity = opacity;
    BYTE alpha = (BYTE)(opacity * 255.0);
    SetLayeredWindowAttributes(g_hwnd, 0, alpha, LWA_ALPHA);
}

static void position_window(const char *position) {
    RECT work;
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &work, 0);

    RECT rect;
    GetWindowRect(g_hwnd, &rect);
    int width = rect.right - rect.left;
    int height = rect.bottom - rect.top;
    int margin = 24;
    int x = work.right - width - margin;
    int y = work.top + margin;

    if (strstr(position, "top_left")) {
        x = work.left + margin;
        y = work.top + margin;
    } else if (strstr(position, "bottom_left")) {
        x = work.left + margin;
        y = work.bottom - height - margin;
    } else if (strstr(position, "bottom_right")) {
        x = work.right - width - margin;
        y = work.bottom - height - margin;
    } else if (strstr(position, "center")) {
        x = work.left + ((work.right - work.left) - width) / 2;
        y = work.top + ((work.bottom - work.top) - height) / 2;
    }

    SetWindowPos(g_hwnd, HWND_TOPMOST, x, y, width, height, SWP_NOACTIVATE);
}

static bool set_utf8_text(wchar_t *dest, size_t dest_len, const char *text, size_t text_len) {
    if (!dest || dest_len == 0) return false;
    dest[0] = L'\0';
    wchar_t *wide = utf8_to_wide_alloc(text, text_len);
    if (!wide) return false;
    wcsncpy_s(dest, dest_len, wide, _TRUNCATE);
    free(wide);
    return true;
}

static void layout_controls(void) {
    if (!g_ask_edit) return;
    if (g_collapsed) return;
    RECT rect;
    GetClientRect(g_hwnd, &rect);
    int composer_w = clamp_int((rect.right * 72) / 100, 560, 760);
    int composer_left = (rect.right - composer_w) / 2;
    int bottom = rect.bottom - 16;
    int button_h = 34;
    int input_h = 50;
    int input_y = bottom - 86;
    int button_y = input_y + ((input_h - button_h) / 2);
    int chip_y = bottom - 30;
    int button_w = 72;
    int send_w = 78;
    int auto_w = 220;
    int gap = 10;

    MoveWindow(g_record_button, composer_left + 12, button_y, button_w, button_h, TRUE);
    MoveWindow(g_send_button, composer_left + composer_w - 12 - send_w, button_y, send_w, button_h, TRUE);
    MoveWindow(g_auto_send_combo, composer_left + composer_w - 12 - send_w - gap - auto_w, button_y, auto_w, button_h + 100, TRUE);
    MoveWindow(g_ask_edit, composer_left + 12 + button_w + gap, input_y, composer_w - 24 - button_w - send_w - auto_w - (gap * 3), input_h, TRUE);

    int chip_gap = 8;
    int chip_w = (composer_w - 24 - (chip_gap * 2)) / 3;
    MoveWindow(g_recap_button, composer_left + 12, chip_y, chip_w, 30, TRUE);
    MoveWindow(g_answer_detail_combo, composer_left + 12 + chip_w + chip_gap, chip_y, chip_w, 120, TRUE);
    MoveWindow(g_page_button, composer_left + 12 + (chip_w + chip_gap) * 2, chip_y, chip_w, 30, TRUE);

    int header_w = clamp_int((rect.right * 84) / 100, 520, 780);
    if (header_w > rect.right - 28) header_w = rect.right - 28;
    int header_left = (rect.right - header_w) / 2;
    int header_right = header_left + header_w;
    int header_y = 8;
    int margin = 14;
    int small_w = 76;
    int header_h = 30;
    MoveWindow(g_close_button, header_right - margin - 62, header_y + 4, 62, header_h, TRUE);
    MoveWindow(g_note_button, header_right - margin - 62 - small_w - 8, header_y + 4, small_w, header_h, TRUE);
    MoveWindow(g_theme_button, header_right - margin - 62 - (small_w + 8) * 2, header_y + 4, small_w, header_h, TRUE);
    MoveWindow(g_shortcuts_button, header_right - margin - 62 - (small_w + 8) * 3, header_y + 4, small_w, header_h, TRUE);
    MoveWindow(g_attach_button, header_right - margin - 62 - (small_w + 8) * 4, header_y + 4, small_w, header_h, TRUE);
    MoveWindow(g_session_button, header_right - margin - 62 - (small_w + 8) * 5, header_y + 4, small_w, header_h, TRUE);
    MoveWindow(g_help_button, header_right - margin - 62 - (small_w + 8) * 6, header_y + 4, small_w, header_h, TRUE);

    int transcript_right = rect.right - 18;
    int transcript_bottom = rect.bottom - 132;
    MoveWindow(g_transcript_clear_button, transcript_right - 64, transcript_bottom - 30, 58, 24, TRUE);
    update_transcript_clear_button();

    MoveWindow(g_paste_answer_button, rect.right - 124, 58, 106, 28, TRUE);
    update_paste_answer_button();
}

static bool point_hits_visible_child(HWND child, POINT point, int padding) {
    if (!child || !IsWindowVisible(child)) return false;
    RECT child_rect;
    if (!GetWindowRect(child, &child_rect)) return false;
    InflateRect(&child_rect, padding, padding);
    return PtInRect(&child_rect, point) != 0;
}

static bool point_hits_overlay_control(POINT point) {
    HWND controls[] = {
        g_ask_edit,
        g_send_button,
        g_record_button,
        g_auto_send_combo,
        g_answer_detail_combo,
        g_transcript_clear_button,
        g_paste_answer_button,
        g_help_button,
        g_session_button,
        g_page_button,
        g_attach_button,
        g_recap_button,
        g_note_button,
        g_shortcuts_button,
        g_theme_button,
        g_close_button,
    };
    for (size_t i = 0; i < sizeof(controls) / sizeof(controls[0]); i++) {
        if (point_hits_visible_child(controls[i], point, 8)) return true;
    }
    return false;
}

static RECT clickthrough_move_handle_rect(RECT client) {
    int header_w = clamp_int((client.right * 84) / 100, 520, 780);
    if (header_w > client.right - 28) header_w = client.right - 28;
    int header_left = (client.right - header_w) / 2;
    RECT handle = {header_left + 124, 10, header_left + 166, 52};
    return handle;
}

static bool point_hits_clickthrough_move_handle(POINT point) {
    if (!g_hwnd || g_collapsed) return false;
    RECT rect;
    if (!GetClientRect(g_hwnd, &rect)) return false;
    POINT local = point;
    if (!ScreenToClient(g_hwnd, &local)) return false;

    RECT handle = clickthrough_move_handle_rect(rect);
    InflateRect(&handle, 18, 18);
    return PtInRect(&handle, local) != 0;
}

static LRESULT hit_test_expanded_resize(POINT point) {
    if (!g_hwnd || g_collapsed) return HTNOWHERE;
    RECT rect;
    if (!GetClientRect(g_hwnd, &rect)) return HTNOWHERE;
    POINT local = point;
    if (!ScreenToClient(g_hwnd, &local)) return HTNOWHERE;
    if (local.x < 0 || local.y < 0 || local.x >= rect.right || local.y >= rect.bottom) {
        return HTNOWHERE;
    }

    bool left = local.x < EXPANDED_RESIZE_HIT_SIZE;
    bool right = local.x >= rect.right - EXPANDED_RESIZE_HIT_SIZE;
    bool top = local.y < EXPANDED_RESIZE_HIT_SIZE;
    bool bottom = local.y >= rect.bottom - EXPANDED_RESIZE_HIT_SIZE;

    if (top && left) return HTTOPLEFT;
    if (top && right) return HTTOPRIGHT;
    if (bottom && left) return HTBOTTOMLEFT;
    if (bottom && right) return HTBOTTOMRIGHT;
    if (left) return HTLEFT;
    if (right) return HTRIGHT;
    if (top) return HTTOP;
    if (bottom) return HTBOTTOM;
    return HTNOWHERE;
}

static LRESULT CALLBACK ask_edit_proc(HWND hwnd, UINT msg, WPARAM wparam, LPARAM lparam) {
    if (msg == WM_KEYDOWN && wparam == VK_RETURN && (GetKeyState(VK_CONTROL) & 0x8000) != 0) {
        send_current_question();
        return 0;
    }
    if (msg == WM_KEYDOWN && wparam == VK_RETURN && (GetKeyState(VK_SHIFT) & 0x8000) == 0) {
        send_current_question();
        return 0;
    }
    if (msg == WM_SETCURSOR) {
        SetCursor(LoadCursorW(NULL, IDC_ARROW));
        return TRUE;
    }
    if (msg == WM_GETDLGCODE) {
        LRESULT code = g_ask_edit_proc ? CallWindowProcW(g_ask_edit_proc, hwnd, msg, wparam, lparam) : 0;
        return code | DLGC_WANTCHARS | DLGC_WANTARROWS;
    }
    return g_ask_edit_proc
        ? CallWindowProcW(g_ask_edit_proc, hwnd, msg, wparam, lparam)
        : DefWindowProcW(hwnd, msg, wparam, lparam);
}

static void create_controls(HWND hwnd) {
    HFONT font = (HFONT)GetStockObject(DEFAULT_GUI_FONT);
    if (!g_edit_brush) refresh_edit_brush();
    g_ask_edit = CreateWindowExW(
        0, L"EDIT", L"",
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN | ES_NOHIDESEL,
        0, 0, 100, 50, hwnd, (HMENU)ID_ASK_EDIT, GetModuleHandleW(NULL), NULL
    );
    g_send_button = CreateWindowW(L"BUTTON", L"Answer", WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        0, 0, 60, 30, hwnd, (HMENU)ID_SEND_BUTTON, GetModuleHandleW(NULL), NULL);
    g_record_button = CreateWindowW(L"BUTTON", L"Mic", WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        0, 0, 60, 30, hwnd, (HMENU)ID_RECORD_BUTTON, GetModuleHandleW(NULL), NULL);
    g_auto_send_combo = CreateWindowW(
        L"COMBOBOX", L"",
        WS_CHILD | WS_VISIBLE | CBS_DROPDOWNLIST | CBS_HASSTRINGS,
        0, 0, 220, 150, hwnd, (HMENU)ID_AUTO_SEND_BUTTON, GetModuleHandleW(NULL), NULL);
    SendMessageW(g_auto_send_combo, CB_ADDSTRING, 0, (LPARAM)L"Don't auto-send");
    SendMessageW(g_auto_send_combo, CB_ADDSTRING, 0, (LPARAM)L"Auto-send mic captions");
    SendMessageW(g_auto_send_combo, CB_ADDSTRING, 0, (LPARAM)L"Auto-send system captions");
    SendMessageW(g_auto_send_combo, CB_ADDSTRING, 0, (LPARAM)L"Auto-send mic or system captions");
    SendMessageW(g_auto_send_combo, CB_SETDROPPEDWIDTH, 280, 0);
    SendMessageW(g_auto_send_combo, CB_SETCURSEL, g_auto_send_mode, 0);
    g_answer_detail_combo = CreateWindowW(
        L"COMBOBOX", L"",
        WS_CHILD | WS_VISIBLE | CBS_DROPDOWNLIST | CBS_HASSTRINGS,
        0, 0, 150, 120, hwnd, (HMENU)ID_ANSWER_DETAIL_BUTTON, GetModuleHandleW(NULL), NULL);
    SendMessageW(g_answer_detail_combo, CB_ADDSTRING, 0, (LPARAM)L"Auto");
    SendMessageW(g_answer_detail_combo, CB_ADDSTRING, 0, (LPARAM)L"Quick");
    SendMessageW(g_answer_detail_combo, CB_ADDSTRING, 0, (LPARAM)L"Thorough");
    SendMessageW(g_answer_detail_combo, CB_SETCURSEL, g_answer_detail_mode, 0);
    g_transcript_clear_button = CreateWindowW(L"BUTTON", L"Clear", WS_CHILD | BS_OWNERDRAW,
        0, 0, 60, 24, hwnd, (HMENU)ID_TRANSCRIPT_CLEAR_BUTTON, GetModuleHandleW(NULL), NULL);
    g_paste_answer_button = CreateWindowW(L"BUTTON", L"", WS_CHILD | BS_OWNERDRAW,
        0, 0, 106, 28, hwnd, (HMENU)ID_PASTE_ANSWER_BUTTON, GetModuleHandleW(NULL), NULL);
    g_help_button = CreateWindowW(L"BUTTON", L"Help", WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        0, 0, 60, 30, hwnd, (HMENU)ID_HELP_BUTTON, GetModuleHandleW(NULL), NULL);
    g_session_button = CreateWindowW(L"BUTTON", L"Session", WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        0, 0, 60, 30, hwnd, (HMENU)ID_SESSION_BUTTON, GetModuleHandleW(NULL), NULL);
    g_page_button = CreateWindowW(L"BUTTON", L"Analyse Screen", WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        0, 0, 60, 30, hwnd, (HMENU)ID_PAGE_BUTTON, GetModuleHandleW(NULL), NULL);
    g_attach_button = CreateWindowW(L"BUTTON", L"Attach", WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        0, 0, 60, 30, hwnd, (HMENU)ID_ATTACH_BUTTON, GetModuleHandleW(NULL), NULL);
    g_recap_button = CreateWindowW(L"BUTTON", L"Recap", WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        0, 0, 60, 30, hwnd, (HMENU)ID_RECAP_BUTTON, GetModuleHandleW(NULL), NULL);
    g_note_button = CreateWindowW(L"BUTTON", L"Style", WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        0, 0, 60, 30, hwnd, (HMENU)ID_NOTE_BUTTON, GetModuleHandleW(NULL), NULL);
    g_theme_button = CreateWindowW(L"BUTTON", L"Theme", WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        0, 0, 60, 30, hwnd, (HMENU)ID_THEME_BUTTON, GetModuleHandleW(NULL), NULL);
    g_shortcuts_button = CreateWindowW(L"BUTTON", L"Shortcuts", WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        0, 0, 60, 30, hwnd, (HMENU)ID_SHORTCUTS_BUTTON, GetModuleHandleW(NULL), NULL);
    g_close_button = CreateWindowW(L"BUTTON", L"Quit", WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        0, 0, 60, 30, hwnd, (HMENU)ID_CLOSE_BUTTON, GetModuleHandleW(NULL), NULL);

    HWND controls[] = {
        g_ask_edit, g_send_button, g_record_button, g_auto_send_combo, g_answer_detail_combo, g_transcript_clear_button, g_paste_answer_button, g_help_button, g_session_button,
        g_page_button, g_attach_button, g_recap_button, g_note_button, g_theme_button, g_shortcuts_button, g_close_button
    };
    for (int i = 0; i < (int)(sizeof(controls) / sizeof(controls[0])); i++) {
        SendMessageW(controls[i], WM_SETFONT, (WPARAM)font, TRUE);
    }
    SendMessageW(g_ask_edit, EM_SETLIMITTEXT, 32768, 0);
    SendMessageW(g_ask_edit, EM_SETCUEBANNER, FALSE, (LPARAM)L"Ask me anything...");
    g_ask_edit_proc = (WNDPROC)SetWindowLongPtrW(g_ask_edit, GWLP_WNDPROC, (LONG_PTR)ask_edit_proc);
    update_auto_send_control();
    configure_tooltips();
    layout_controls();
}

static void consume_sent_context_chips(void) {
    for (int i = 0; i < g_context_chip_count; i++) {
        g_context_chips[i].pending = false;
    }
    g_show_context_chips = false;
    g_pending_context_chips = false;
    g_pending_context_expected_until_ms = 0;
    InvalidateRect(g_hwnd, NULL, TRUE);
}

static void clear_local_transcript_context(void) {
    if (g_auto_send_timer_armed) {
        cancel_auto_send_timer("transcript_clear");
    }
    if (g_manual_send_timer_armed && g_hwnd) {
        KillTimer(g_hwnd, ID_MANUAL_SEND_TIMER);
        g_manual_send_timer_armed = false;
        g_manual_send_started_ms = 0;
    }
    char detail[160];
    snprintf(
        detail,
        sizeof(detail),
        "final_chars=%zu partial_chars=%zu source_chars=%zu",
        wcslen(g_transcript_final),
        wcslen(g_transcript_partial),
        wcslen(g_transcript_source)
    );
    emit_lifecycle_event("transcript_context_cleared", "ok", detail);
    g_transcript_partial[0] = L'\0';
    g_transcript_final[0] = L'\0';
    g_transcript_source[0] = L'\0';
    update_transcript_clear_button();
}

static void trim_transcript_question(wchar_t *text) {
    if (!text) return;
    size_t len = wcslen(text);
    size_t start = 0;
    while (start < len && iswspace(text[start])) start++;
    size_t end = len;
    while (end > start && iswspace(text[end - 1])) end--;
    if (start > 0 || end < len) {
        size_t out = 0;
        for (size_t i = start; i < end; i++) {
            text[out++] = text[i];
        }
        text[out] = L'\0';
    }
}

static bool is_placeholder_transcript_question(const wchar_t *text) {
    if (!text || text[0] == L'\0') return true;
    wchar_t lower[260];
    size_t i = 0;
    for (; i < 259 && text[i] != L'\0'; i++) {
        lower[i] = (wchar_t)towlower(text[i]);
    }
    lower[i] = L'\0';
    return wcsstr(lower, L"captions appear here") != NULL
        || wcsstr(lower, L"live captions preview") != NULL
        || wcsstr(lower, L"starting audio") != NULL
        || wcsstr(lower, L"audio is live") != NULL
        || wcsstr(lower, L"listening for follow-up") != NULL;
}

static bool is_usable_transcript_question(const wchar_t *text) {
    if (!text || text[0] == L'\0') return false;
    if (is_placeholder_transcript_question(text)) return false;
    wchar_t body[1024];
    wcsncpy_s(body, 1024, text, _TRUNCATE);
    trim_transcript_question(body);
    if (transcript_question_has_source_label(body)) {
        wchar_t *colon = wcschr(body, L':');
        if (colon) {
            wchar_t remainder[1024];
            wcsncpy_s(remainder, 1024, colon + 1, _TRUNCATE);
            trim_transcript_question(remainder);
            wcsncpy_s(body, 1024, remainder, _TRUNCATE);
        }
    }
    size_t len = wcslen(body);
    if (len < 3) return false;
    for (size_t i = 0; i < len; i++) {
        if (iswalnum(body[i])) return true;
    }
    return false;
}

static bool is_filler_transcript_word(const wchar_t *word) {
    if (!word || word[0] == L'\0') return true;
    static const wchar_t *fillers[] = {
        L"a", L"an", L"and", L"audio", L"but", L"i", L"it", L"like",
        L"mic", L"microphone", L"okay", L"ok", L"so", L"speaker",
        L"sure", L"system", L"the", L"then", L"uh", L"um", L"yeah",
        L"yes", L"you", L"know"
    };
    for (int i = 0; i < (int)(sizeof(fillers) / sizeof(fillers[0])); i++) {
        if (wcscmp(word, fillers[i]) == 0) return true;
    }
    return false;
}

static bool is_meaningful_transcript_question(const wchar_t *text) {
    if (!text || text[0] == L'\0') return false;
    if (is_placeholder_transcript_question(text)) return false;
    size_t len = wcslen(text);
    if (len < 8) return false;

    int meaningful_words = 0;
    int first_meaningful_len = 0;
    bool has_question_mark = false;
    wchar_t word[64];
    int word_len = 0;
    for (size_t i = 0; i <= len; i++) {
        wchar_t ch = text[i];
        if (ch == L'?') has_question_mark = true;
        if (iswalnum(ch)) {
            if (word_len < (int)(sizeof(word) / sizeof(word[0])) - 1) {
                word[word_len++] = (wchar_t)towlower(ch);
            }
            continue;
        }
        if (word_len > 0) {
            word[word_len] = L'\0';
            if (!is_filler_transcript_word(word)) {
                meaningful_words++;
                if (first_meaningful_len == 0) first_meaningful_len = word_len;
            }
            word_len = 0;
        }
    }

    if (meaningful_words >= 2) return true;
    return has_question_mark && first_meaningful_len >= 5;
}

static bool live_transcript_visible_question(wchar_t *out, size_t capacity) {
    if (!out || capacity == 0) return false;
    out[0] = L'\0';
    const wchar_t *source = g_transcript_final[0] ? g_transcript_final : g_transcript_partial;
    if (!source || source[0] == L'\0') return false;
    size_t len = wcslen(source);
    if (len < 3 || len > 220 || len >= capacity) return false;
    wcscpy_s(out, capacity, source);
    trim_transcript_question(out);
    if (wcslen(out) < 3 || wcslen(out) > 220) return false;
    int line_count = 1;
    for (wchar_t *p = out; *p; p++) {
        if (*p == L'\r' || *p == L'\n') {
            line_count++;
            *p = L' ';
        }
    }
    if (line_count > 2) return false;
    prefix_transcript_source_label(out, capacity);
    if (!is_usable_transcript_question(out)) return false;
    return true;
}

static bool live_transcript_question_text(wchar_t *out, size_t capacity) {
    if (!out || capacity == 0) return false;
    out[0] = L'\0';
    const wchar_t *source = g_transcript_final[0] ? g_transcript_final : g_transcript_partial;
    if (!source || source[0] == L'\0') return false;
    size_t len = wcslen(source);
    if (len < 3) return false;
    if (len >= capacity) {
        source += len - (capacity - 1);
    }
    wcscpy_s(out, capacity, source);
    trim_transcript_question(out);
    if (wcslen(out) < 3) return false;
    for (wchar_t *p = out; *p; p++) {
        if (*p == L'\r' || *p == L'\n') {
            *p = L' ';
        }
    }
    prefix_transcript_source_label(out, capacity);
    if (!is_usable_transcript_question(out)) return false;
    return true;
}

static bool is_live_transcript_answer_prompt(const wchar_t *question) {
    if (!question) return false;
    return wcsncmp(question, L"Answer the latest ", 18) == 0
        && wcsstr(question, L"live captions from the current session transcript") != NULL;
}

static bool wide_contains_case_insensitive(const wchar_t *haystack, const wchar_t *needle) {
    if (!haystack || !needle || needle[0] == L'\0') return false;
    size_t needle_len = wcslen(needle);
    for (const wchar_t *p = haystack; *p; p++) {
        size_t i = 0;
        while (i < needle_len && p[i] && towlower(p[i]) == towlower(needle[i])) {
            i++;
        }
        if (i == needle_len) return true;
    }
    return false;
}

static bool context_chip_is_screen_context(const OverlayContextChip *chip) {
    if (!chip) return false;
    if (_wcsicmp(chip->kind, L"screen") == 0 || _wcsicmp(chip->kind, L"screenshot") == 0) {
        return true;
    }
    return _wcsicmp(chip->kind, L"image") == 0
        && wide_contains_case_insensitive(chip->title, L"screen");
}

static const wchar_t *fallback_question_for_context(void) {
    bool has_screen_context = false;
    bool has_file_context = false;
    for (int i = 0; i < g_context_chip_count; i++) {
        if (context_chip_is_screen_context(&g_context_chips[i])) {
            has_screen_context = true;
        } else {
            has_file_context = true;
        }
    }
    if (has_screen_context && has_file_context) {
        return L"Answer using the attached screen context and files.";
    }
    if (has_screen_context) {
        return L"Answer using the attached screen context.";
    }
    if (has_file_context) {
        return L"Answer using the attached files.";
    }
    return L"Answer using the attached context.";
}

static void arm_manual_send_timer(UINT delay_ms, const char *reason) {
    if (!g_hwnd) return;
    KillTimer(g_hwnd, ID_MANUAL_SEND_TIMER);
    SetTimer(g_hwnd, ID_MANUAL_SEND_TIMER, delay_ms > 0 ? delay_ms : 1, NULL);
    g_manual_send_timer_armed = true;
    char detail[192];
    snprintf(
        detail,
        sizeof(detail),
        "reason=%s delay_ms=%u elapsed_ms=%llu transcript_context=%s",
        reason ? reason : "unknown",
        delay_ms,
        (unsigned long long)(GetTickCount64() - g_manual_send_started_ms),
        has_transcript_context() ? "true" : "false"
    );
    emit_lifecycle_event("manual_answer_waiting_for_transcript_settle", "ok", detail);
}

static void reschedule_manual_send_after_transcript_update(bool final) {
    if (!g_manual_send_timer_armed || g_manual_send_started_ms == 0) return;
    ULONGLONG elapsed = GetTickCount64() - g_manual_send_started_ms;
    if (elapsed >= MANUAL_CAPTION_SETTLE_MAX_MS) {
        arm_manual_send_timer(1, "settle_cap");
        return;
    }
    UINT quiet_ms = final
        ? MANUAL_FINAL_CAPTION_SETTLE_DELAY_MS
        : MANUAL_CAPTION_SETTLE_DELAY_MS;
    ULONGLONG remaining = MANUAL_CAPTION_SETTLE_MAX_MS - elapsed;
    if ((ULONGLONG)quiet_ms > remaining) quiet_ms = (UINT)remaining;
    arm_manual_send_timer(quiet_ms, final ? "final_caption" : "partial_caption");
}

static void send_current_question(void) {
    if (g_recovery_mode != 0 && GetWindowTextLengthW(g_ask_edit) <= 0) {
        send_current_question_now(false);
        return;
    }
    int length = GetWindowTextLengthW(g_ask_edit);
    if (length <= 0 && g_recording) {
        if (!g_manual_send_timer_armed) {
            g_manual_send_started_ms = GetTickCount64();
        }
        arm_manual_send_timer(MANUAL_CAPTION_SETTLE_DELAY_MS, "enter");
        return;
    }
    if (g_manual_send_timer_armed && g_hwnd) {
        KillTimer(g_hwnd, ID_MANUAL_SEND_TIMER);
        g_manual_send_timer_armed = false;
        g_manual_send_started_ms = 0;
    }
    send_current_question_now(false);
}

static void send_current_question_now(bool listen_triggered) {
    int length = GetWindowTextLengthW(g_ask_edit);
    if (length <= 0 && g_recovery_mode != 0) {
        int recovery_mode = g_recovery_mode;
        const wchar_t *question = g_recovery_mode == 1
            ? L"Continue the previous answer from where it stopped. Do not repeat completed content. Finish any missing code, explanation, complexity, or conclusion."
            : (g_last_question[0]
                ? g_last_question
                : L"Retry the previous question and return a complete answer.");
        if (!emit_ask_event(question, false)) {
            MessageBeep(MB_ICONWARNING);
            SetFocus(g_ask_edit);
            return;
        }
        emit_lifecycle_event(
            "answer_recovery_requested",
            "ok",
            recovery_mode == 1 ? "action=continue platform=windows" : "action=retry platform=windows");
        set_recovery_mode(0);
        InvalidateRect(g_hwnd, NULL, TRUE);
        SetFocus(g_ask_edit);
        return;
    }
    bool used_fallback = length <= 0;
    wchar_t transcript_question[1024] = L"";
    bool used_short_transcript = used_fallback && live_transcript_visible_question(transcript_question, 260);
    bool used_transcript_text = used_short_transcript
        || (used_fallback && live_transcript_question_text(transcript_question, 1024));
    bool transcript_context = has_transcript_context();
    bool answer_current_transcript = ask_event_answers_current_transcript(
        listen_triggered,
        transcript_context);
    if (used_fallback && transcript_context && !used_transcript_text) {
        emit_lifecycle_event("ask_answer_blocked", "unusable_transcript", "typed_chars=0 transcript_context=true");
        SetFocus(g_ask_edit);
        return;
    }
    if (used_fallback && !transcript_context && g_context_chip_count <= 0) {
        emit_lifecycle_event("ask_answer_blocked", "empty", "typed_chars=0 transcript_context=false context_ids=0");
        SetFocus(g_ask_edit);
        return;
    }
    const wchar_t *fallback_question = used_transcript_text ? transcript_question : fallback_question_for_context();
    char detail[192];
    snprintf(
        detail,
        sizeof(detail),
        "typed_chars=%d fallback=%s transcript_context=%s generic_live_prompt=%s context_ids=%d",
        length > 0 ? length : 0,
        used_fallback ? "true" : "false",
        transcript_context ? "true" : "false",
        is_live_transcript_answer_prompt(fallback_question) ? "true" : "false",
        pending_context_chip_count()
    );
    bool emitted = false;
    if (length <= 0) {
        emitted = emit_ask_event(fallback_question, answer_current_transcript);
    } else {
        wchar_t *question = (wchar_t *)calloc((size_t)length + 1, sizeof(wchar_t));
        if (!question) return;
        GetWindowTextW(g_ask_edit, question, length + 1);
        emitted = emit_ask_event(question, answer_current_transcript);
        if (emitted) wcsncpy_s(g_last_question, 2048, question, _TRUNCATE);
        free(question);
    }
    if (!emitted) {
        MessageBeep(MB_ICONWARNING);
        SetFocus(g_ask_edit);
        return;
    }
    emit_lifecycle_event("ask_answer_sent", "ok", detail);
    set_recovery_mode(0);
    SetWindowTextW(g_ask_edit, L"");
    clear_local_transcript_context();
    consume_sent_context_chips();
    InvalidateRect(g_hwnd, NULL, TRUE);
    SetFocus(g_ask_edit);
}

static void invoke_button_command(int id, HWND control) {
    if (!g_hwnd) return;
    SendMessageW(g_hwnd, WM_COMMAND, MAKEWPARAM(id, BN_CLICKED), (LPARAM)control);
}

static void focus_ask_input(void) {
    if (!g_hwnd || !g_ask_edit) return;
    if (g_collapsed || !g_visible) show_full_overlay(true);
    SetFocus(g_ask_edit);
    int length = GetWindowTextLengthW(g_ask_edit);
    SendMessageW(g_ask_edit, EM_SETSEL, (WPARAM)length, (LPARAM)length);
    emit_lifecycle_event("shortcut_invoked", "ok", "source=keyboard action=text_input");
}

static void toggle_interactive_mode(void) {
    g_interactive_mode = !g_interactive_mode;
    InvalidateRect(g_hwnd, NULL, TRUE);
    emit_lifecycle_event(
        "shortcut_invoked",
        "ok",
        g_interactive_mode
            ? "source=keyboard action=interactive_on"
            : "source=keyboard action=clickthrough_on"
    );
}

static bool is_keyboard_focusable_control(HWND control) {
    if (!control || !IsWindow(control) || !IsWindowVisible(control) || !IsWindowEnabled(control)) {
        return false;
    }
    RECT rect = {0};
    GetWindowRect(control, &rect);
    return (rect.right - rect.left) > 4 && (rect.bottom - rect.top) > 4;
}

static int keyboard_focus_controls(HWND *controls, int max_controls) {
    HWND ordered[] = {
        g_help_button,
        g_session_button,
        g_page_button,
        g_attach_button,
        g_recap_button,
        g_note_button,
        g_shortcuts_button,
        g_theme_button,
        g_close_button,
        g_transcript_clear_button,
        g_ask_edit,
        g_auto_send_combo,
        g_answer_detail_combo,
        g_record_button,
        g_send_button,
    };
    int count = 0;
    for (int i = 0; i < (int)(sizeof(ordered) / sizeof(ordered[0])) && count < max_controls; i++) {
        if (is_keyboard_focusable_control(ordered[i])) {
            controls[count++] = ordered[i];
        }
    }
    return count;
}

static bool focus_next_keyboard_control(bool backward) {
    HWND controls[16];
    int count = keyboard_focus_controls(controls, (int)(sizeof(controls) / sizeof(controls[0])));
    if (count <= 0) return false;

    HWND focused = GetFocus();
    int current = -1;
    for (int i = 0; i < count; i++) {
        if (controls[i] == focused) {
            current = i;
            break;
        }
    }

    int next = 0;
    if (current >= 0) {
        next = backward ? (current - 1 + count) % count : (current + 1) % count;
    } else if (backward) {
        next = count - 1;
    }

    SetFocus(controls[next]);
    InvalidateRect(controls[next], NULL, TRUE);
    if (focused && focused != controls[next]) {
        InvalidateRect(focused, NULL, TRUE);
    }
    emit_lifecycle_event("keyboard_focus_moved", "ok", "platform=windows");
    return true;
}

static bool activate_focused_keyboard_control(void) {
    HWND focused = GetFocus();
    if (!focused) return false;
    if (focused == g_ask_edit) {
        return false;
    }
    if (!is_keyboard_focusable_control(focused)) return false;

    int id = GetDlgCtrlID(focused);
    switch (id) {
    case ID_AUTO_SEND_BUTTON:
        SendMessageW(g_auto_send_combo, CB_SHOWDROPDOWN, TRUE, 0);
        emit_lifecycle_event("keyboard_focus_activated", "ok", "control=auto_send platform=windows");
        return true;
    case ID_ANSWER_DETAIL_BUTTON:
        SendMessageW(g_answer_detail_combo, CB_SHOWDROPDOWN, TRUE, 0);
        emit_lifecycle_event("keyboard_focus_activated", "ok", "control=answer_detail platform=windows");
        return true;
    case ID_SEND_BUTTON:
    case ID_RECORD_BUTTON:
    case ID_TRANSCRIPT_CLEAR_BUTTON:
    case ID_HELP_BUTTON:
    case ID_SESSION_BUTTON:
    case ID_PAGE_BUTTON:
    case ID_ATTACH_BUTTON:
    case ID_RECAP_BUTTON:
    case ID_NOTE_BUTTON:
    case ID_THEME_BUTTON:
    case ID_SHORTCUTS_BUTTON:
    case ID_CLOSE_BUTTON:
        invoke_button_command(id, focused);
        emit_lifecycle_event("keyboard_focus_activated", "ok", "platform=windows");
        return true;
    default:
        return false;
    }
}

static bool handle_overlay_shortcut_key(WPARAM key, bool local_key) {
    if (!g_hwnd) return false;
    bool expanded = !g_collapsed && g_visible;

    if (local_key && key != VK_RETURN && key != VK_ESCAPE) {
        return false;
    }

    if (key == 'B') {
        if (expanded) {
            collapse_to_pill(g_hwnd, true);
        } else {
            show_full_overlay(true);
        }
        emit_lifecycle_event("shortcut_invoked", "ok", "source=keyboard action=hide_restore");
        return true;
    }

    if (key == 'T') {
        focus_ask_input();
        return true;
    }

    if (!expanded) return false;

    switch (key) {
    case VK_RETURN:
        send_current_question();
        emit_lifecycle_event("shortcut_invoked", "ok", "source=keyboard action=answer");
        return true;
    case 'L':
        invoke_button_command(ID_RECORD_BUTTON, g_record_button);
        emit_lifecycle_event("shortcut_invoked", "ok", "source=keyboard action=listen");
        return true;
    case 'S':
        invoke_button_command(ID_PAGE_BUTTON, g_page_button);
        emit_lifecycle_event("shortcut_invoked", "ok", "source=keyboard action=screen");
        return true;
    case 'I':
        toggle_interactive_mode();
        return true;
    case 'H':
        invoke_button_command(ID_SESSION_BUTTON, g_session_button);
        emit_lifecycle_event("shortcut_invoked", "ok", local_key ? "source=keyboard action=history" : "source=global action=history");
        return true;
    case 'F':
        invoke_button_command(ID_ATTACH_BUTTON, g_attach_button);
        emit_lifecycle_event("shortcut_invoked", "ok", local_key ? "source=keyboard action=files" : "source=global action=files");
        return true;
    case VK_ESCAPE:
        if (local_key) {
            collapse_to_pill(g_hwnd, true);
            emit_lifecycle_event("shortcut_invoked", "ok", "source=keyboard action=collapse");
            return true;
        }
        return false;
    default:
        return false;
    }
}

static bool safe_extract_json_to_wide(
    const char *json,
    size_t json_len,
    const char *key,
    wchar_t *dest,
    size_t dest_wchars
) {
    if (!dest || dest_wchars == 0) return false;
    dest[0] = L'\0';

    char *utf8 = NULL;
    size_t utf8_len = 0;
    if (!json_extract_string_alloc_limited(
            json, json_len, key, JSON_MAX_FIELD_LEN, &utf8, &utf8_len)) {
        return false;
    }
    bool converted = set_utf8_text(dest, dest_wchars, utf8, utf8_len);
    free(utf8);
    return converted;
}

static bool set_body_from_json(
    const char *json,
    size_t json_len,
    const char *key,
    bool normalize_answer
) {
    char *utf8 = NULL;
    size_t utf8_len = 0;
    if (!json_extract_string_alloc(json, json_len, key, &utf8, &utf8_len)) return false;
    wchar_t *wide = utf8_to_wide_alloc(utf8, utf8_len);
    free(utf8);
    if (!wide) return false;
    if (normalize_answer) normalize_answer_display_text(wide);
    return replace_body_owned(wide);
}

static bool safe_extract_json_number(
    const char *json,
    size_t json_len,
    const char *key,
    double *dest
) {
    return json_extract_number(json, json_len, key, dest);
}

static void set_chips_from_json_key(
    const char *json,
    size_t json_len,
    const char *array_key,
    OverlayContextChip *chips,
    int *chip_count
) {
    *chip_count = 0;
    const char *array = NULL;
    size_t array_len = 0;
    if (!json_extract_array(json, json_len, array_key, &array, &array_len)) return;

    const char *p = array + 1;
    const char *end = array + array_len - 1;
    while (p < end && *chip_count < MAX_CONTEXT_CHIPS) {
        p = json_skip_ws(p, end);
        if (p >= end) break;
        if (*p == ',') {
            p++;
            continue;
        }
        if (*p != '{') break;

        const char *object_end = json_skip_value(p, end);
        if (!object_end || object_end <= p) break;
        size_t object_len = (size_t)(object_end - p);

        OverlayContextChip *chip = &chips[*chip_count];
        memset(chip, 0, sizeof(*chip));
        safe_extract_json_to_wide(p, object_len, "id", chip->id, sizeof(chip->id) / sizeof(chip->id[0]));
        if (!safe_extract_json_to_wide(
                p, object_len, "title", chip->title, sizeof(chip->title) / sizeof(chip->title[0]))) {
            wcscpy_s(chip->title, sizeof(chip->title) / sizeof(chip->title[0]), L"Attached file");
        }
        if (!safe_extract_json_to_wide(
                p, object_len, "kind", chip->kind, sizeof(chip->kind) / sizeof(chip->kind[0]))) {
            wcscpy_s(chip->kind, sizeof(chip->kind) / sizeof(chip->kind[0]), L"document");
        }
        safe_extract_json_to_wide(p, object_len, "path", chip->path, sizeof(chip->path) / sizeof(chip->path[0]));
        safe_extract_json_to_wide(
            p,
            object_len,
            "processing_status",
            chip->processing_status,
            sizeof(chip->processing_status) / sizeof(chip->processing_status[0]));
        (*chip_count)++;
        p = object_end;
    }
}

static int find_context_chip_by_id(
    const OverlayContextChip *chips,
    int chip_count,
    const wchar_t *id
) {
    if (!id || id[0] == L'\0') return -1;
    for (int i = 0; i < chip_count; i++) {
        if (wcscmp(chips[i].id, id) == 0) return i;
    }
    return -1;
}

static void set_context_chips_from_json(const char *line, size_t line_len) {
    OverlayContextChip parsed[MAX_CONTEXT_CHIPS];
    int parsed_count = 0;
    set_chips_from_json_key(line, line_len, "items", parsed, &parsed_count);
    bool mutation_expected = pending_context_expected();
    bool has_pending = false;

    for (int i = 0; i < parsed_count; i++) {
        int previous = find_context_chip_by_id(g_context_chips, g_context_chip_count, parsed[i].id);
        parsed[i].pending = (previous >= 0 && g_context_chips[previous].pending)
            || (mutation_expected && previous < 0);
        has_pending = has_pending || parsed[i].pending;
    }
    memcpy(g_context_chips, parsed, (size_t)parsed_count * sizeof(parsed[0]));
    g_context_chip_count = parsed_count;

    if (g_context_chip_count <= 0) {
        g_show_context_chips = false;
        g_pending_context_chips = false;
        g_pending_context_expected_until_ms = 0;
        return;
    }
    g_pending_context_chips = has_pending;
    if (has_pending) g_show_context_chips = false;
    if (mutation_expected) g_pending_context_expected_until_ms = 0;
}

static void set_sent_chips_from_json(const char *card, size_t card_len) {
    set_chips_from_json_key(card, card_len, "attachments", g_sent_chips, &g_sent_chip_count);
}

static bool process_stdin_record(const char *line, size_t line_len, void *context) {
    (void)context;
    if (json_line_too_long(line_len)) return true;

    char msg_type[128];
    if (!json_extract_type(line, line_len, msg_type, sizeof(msg_type))) return true;

    if (strcmp(msg_type, "show") == 0) {
        show_full_overlay(true);
    } else if (strcmp(msg_type, "hide") == 0) {
        collapse_to_pill(g_hwnd, true);
    } else if (strcmp(msg_type, "toggle") == 0) {
        if (g_visible && !g_collapsed) collapse_to_pill(g_hwnd, true);
        else show_full_overlay(true);
    } else if (strcmp(msg_type, "clear") == 0) {
        wcscpy_s(g_title, 256, L"bluey");
        set_body_text(L"");
        wcscpy_s(g_kind, 64, L"system");
        wcscpy_s(g_source, 256, L"");
        wcscpy_s(g_card_id, 80, L"");
        reset_card_update_sequence();
        g_sent_chip_count = 0;
        set_recovery_mode(0);
        update_paste_answer_button();
        InvalidateRect(g_hwnd, NULL, TRUE);
    } else if (strcmp(msg_type, "boot") == 0) {
        wcscpy_s(g_title, 256, L"bluey online");
        set_body_text(L"> overlay link established\n> session memory loaded\n> context controls armed\n> ready");
        wcscpy_s(g_kind, 64, L"system");
        wcscpy_s(g_source, 256, L"");
        wcscpy_s(g_card_id, 80, L"");
        reset_card_update_sequence();
        safe_extract_json_to_wide(line, line_len, "title", g_title, 256);
        set_recovery_mode(0);
        show_full_overlay(false);
        update_paste_answer_button();
        InvalidateRect(g_hwnd, NULL, TRUE);
    } else if (strcmp(msg_type, "set_position") == 0) {
        char pos[32];
        if (json_extract_string(line, line_len, "position", pos, sizeof(pos))) {
            position_window(pos);
        } else {
            position_window("top_right");
        }
    } else if (strcmp(msg_type, "set_opacity") == 0) {
        double opacity = g_opacity;
        if (safe_extract_json_number(line, line_len, "opacity", &opacity)) {
            set_window_opacity(opacity);
        }
    } else if (strcmp(msg_type, "set_context_items") == 0) {
        set_context_chips_from_json(line, line_len);
        InvalidateRect(g_hwnd, NULL, TRUE);
    } else if (strcmp(msg_type, "push_card") == 0) {
        reset_card_update_sequence();
        const char *card = line;
        size_t card_len = line_len;
        json_extract_object(line, line_len, "card", &card, &card_len);

        const char *artifact = NULL;
        size_t artifact_len = 0;
        bool has_artifact = json_extract_object(card, card_len, "artifact", &artifact, &artifact_len);

        bool has_title = safe_extract_json_to_wide(card, card_len, "title", g_title, 256);
        safe_extract_json_to_wide(card, card_len, "kind", g_kind, 64);
        bool is_answer = _wcsicmp(g_kind, L"answer") == 0;
        bool has_body = set_body_from_json(card, card_len, "body", is_answer);
        if (!has_body && has_artifact) {
            has_body = set_body_from_json(artifact, artifact_len, "body", is_answer);
        }
        if (!has_body) set_body_text(L"");
        if (!has_title && has_artifact) {
            safe_extract_json_to_wide(artifact, artifact_len, "title", g_title, 256);
        }
        if (_wcsicmp(g_kind, L"question") == 0) copy_body_text(g_last_question, 2048);
        safe_extract_json_to_wide(card, card_len, "source", g_source, 256);
        safe_extract_json_to_wide(card, card_len, "id", g_card_id, 80);
        set_sent_chips_from_json(card, card_len);
        update_recovery_action_from_current_card();
        if (g_visible && !g_collapsed) show_full_overlay(false);
        update_paste_answer_button();
        InvalidateRect(g_hwnd, NULL, TRUE);
    } else if (strcmp(msg_type, "update_card") == 0) {
        wchar_t id[80] = L"";
        safe_extract_json_to_wide(line, line_len, "id", id, 80);
        char interaction_id[40] = "";
        json_extract_string(
            line,
            line_len,
            "interaction_id",
            interaction_id,
            sizeof(interaction_id));
        BlueyRenderAckPhase render_ack = parse_render_ack_phase(line, line_len);
        bool snapshot = false;
        bool valid_snapshot = json_read_optional_bool(
            line,
            line_len,
            "snapshot",
            &snapshot);
        bool card_matches = wcscmp(id, g_card_id) == 0 && id[0] != L'\0';
        bool snapshot_recovery =
            snapshot && g_card_id[0] == L'\0' && id[0] != L'\0';
        ULONGLONG applied_sequence = 0;
        bool sequence_present = false;
        if (valid_snapshot
            && (card_matches || snapshot_recovery)
            && should_apply_card_update(
                line,
                line_len,
                snapshot,
                snapshot_recovery,
                &applied_sequence,
                &sequence_present)) {
            if (snapshot_recovery) {
                /*
                 * A restarted overlay has no preceding push_card frame. Rebuild
                 * the answer presentation state from the authoritative daemon
                 * snapshot so it renders and behaves like a normal answer card.
                 */
                if (!recover_answer_snapshot_state(
                        id,
                        g_card_id,
                        80,
                        g_kind,
                        64,
                        g_title,
                        256,
                        g_source,
                        256,
                        &g_sent_chip_count,
                        &g_recovery_mode)) {
                    return true;
                }
                set_recovery_mode(g_recovery_mode);
            }
            bool is_answer = _wcsicmp(g_kind, L"answer") == 0;
            bool updated = set_body_from_json(line, line_len, "body", is_answer);
            const char *artifact = NULL;
            size_t artifact_len = 0;
            if (!updated && json_extract_object(
                    line, line_len, "artifact", &artifact, &artifact_len)) {
                updated = set_body_from_json(artifact, artifact_len, "body", is_answer);
            }
            update_pending_render_acks(
                id,
                interaction_id,
                render_ack,
                applied_sequence,
                sequence_present,
                updated);
            if (updated) {
                update_recovery_action_from_current_card();
                update_paste_answer_button();
                InvalidateRect(g_hwnd, NULL, TRUE);
            }
        }
    } else if (strcmp(msg_type, "shutdown") == 0) {
        PostMessage(g_hwnd, WM_CLOSE, 0, 0);
        return false;
    } else if (strcmp(msg_type, "transcript_partial") == 0) {
        safe_extract_json_to_wide(line, line_len, "text", g_transcript_partial, 1024);
        safe_extract_json_to_wide(line, line_len, "source", g_transcript_source, 64);
        g_audio_auto_stop_remaining_secs = -1;
        reschedule_manual_send_after_transcript_update(false);
        update_transcript_clear_button();
        update_paste_answer_button();
        InvalidateRect(g_hwnd, NULL, TRUE);
    } else if (strcmp(msg_type, "transcript_final") == 0) {
        safe_extract_json_to_wide(line, line_len, "text", g_transcript_final, 1024);
        safe_extract_json_to_wide(line, line_len, "source", g_transcript_source, 64);
        g_transcript_partial[0] = L'\0';
        g_audio_auto_stop_remaining_secs = -1;
        reschedule_manual_send_after_transcript_update(true);
        update_transcript_clear_button();
        update_paste_answer_button();
        schedule_auto_send_after_caption_settled();
        InvalidateRect(g_hwnd, NULL, TRUE);
    } else if (strcmp(msg_type, "listening_state_changed") == 0) {
        char state[32] = "idle";
        json_extract_string(line, line_len, "state", state, sizeof(state));
        g_recording = strcmp(state, "listening") == 0;
        if (g_recording || strcmp(state, "connecting") == 0) {
            hide_meeting_banner(NULL);
        }
        g_audio_auto_stop_remaining_secs = -1;
        update_record_button();
        InvalidateRect(g_hwnd, NULL, TRUE);
    } else if (strcmp(msg_type, "audio_auto_stop_countdown") == 0) {
        double remaining = 0.0;
        double idle = 0.0;
        safe_extract_json_number(line, line_len, "remaining_secs", &remaining);
        safe_extract_json_number(line, line_len, "idle_secs", &idle);
        g_audio_auto_stop_remaining_secs = remaining < 0.0 ? 0 : (int)remaining;
        g_audio_auto_stop_idle_secs = idle < 0.0 ? 0 : (int)idle;
        InvalidateRect(g_hwnd, NULL, TRUE);
    } else if (strcmp(msg_type, "audio_auto_stop_countdown_cleared") == 0) {
        g_audio_auto_stop_remaining_secs = -1;
        InvalidateRect(g_hwnd, NULL, TRUE);
    } else if (strcmp(msg_type, "set_meeting_detection_enabled") == 0) {
        bool enabled = false;
        if (bluey_parse_meeting_detection_enabled(line, line_len, &enabled)) {
            set_meeting_detection_enabled(enabled);
        }
    } else if (strcmp(msg_type, "show_meeting_banner") == 0) {
        if (meeting_detection_enabled() && !g_recording) {
            show_meeting_banner_from_json(line, line_len);
        }
    } else if (strcmp(msg_type, "hide_meeting_banner") == 0) {
        wchar_t candidate_id[260] = L"";
        safe_extract_json_to_wide(line, line_len, "candidate_id", candidate_id, 260);
        hide_meeting_banner(candidate_id[0] == L'\0' ? NULL : candidate_id);
    } else if (strcmp(msg_type, "session_switched") == 0) {
        reset_card_update_sequence();
        if (g_auto_send_timer_armed) cancel_auto_send_timer("session_switched");
        safe_extract_json_to_wide(line, line_len, "title", g_session_banner, 256);
        if (wcslen(g_session_banner) == 0) wcscpy_s(g_session_banner, 256, L"New session");
        g_session_banner_tick = GetTickCount64();
        g_transcript_partial[0] = L'\0';
        g_transcript_final[0] = L'\0';
        g_transcript_source[0] = L'\0';
        update_transcript_clear_button();
        update_paste_answer_button();
        InvalidateRect(g_hwnd, NULL, TRUE);
    } else if (strcmp(msg_type, "set_active_session") == 0) {
        wchar_t previous_session_id[80] = L"";
        wcscpy_s(previous_session_id, 80, g_active_session_id);
        safe_extract_json_to_wide(line, line_len, "id", g_active_session_id, 80);
        safe_extract_json_to_wide(line, line_len, "code", g_active_session_code, 32);
        safe_extract_json_to_wide(line, line_len, "title", g_active_session_title, 160);
        if (wcscmp(previous_session_id, g_active_session_id) != 0) {
            reset_card_update_sequence();
        }
        if (wcslen(g_active_session_code) == 0 && wcslen(g_active_session_id) >= 8) {
            wcsncpy_s(g_active_session_code, 32, g_active_session_id, 8);
        }
        InvalidateRect(g_hwnd, NULL, TRUE);
    } else if (strcmp(msg_type, "ping") == 0) {
        emit_simple_event("pong");
    }
    return true;
}

static DWORD WINAPI stdin_thread(LPVOID unused) {
    (void)unused;
    NdjsonStream stream;
    ndjson_stream_init(&stream, JSON_MAX_LINE_LEN);
    char chunk[16384];
    bool stopped = false;
    HANDLE input = GetStdHandle(STD_INPUT_HANDLE);

    while (!stopped) {
        DWORD read_count = 0;
        if (!ReadFile(input, chunk, (DWORD)sizeof(chunk), &read_count, NULL) || read_count == 0) break;
        size_t rejected_before = stream.rejected_records;
        NdjsonFeedResult result = ndjson_stream_feed(
            &stream, chunk, (size_t)read_count, process_stdin_record, NULL);
        if (stream.rejected_records != rejected_before) {
            fprintf(stderr, "bluey-overlay: rejected oversized NDJSON record\n");
        }
        if (result == NDJSON_FEED_STOPPED) stopped = true;
        if (result == NDJSON_FEED_OUT_OF_MEMORY) {
            fprintf(stderr, "bluey-overlay: out of memory buffering NDJSON input\n");
            stopped = true;
        }
    }
    if (!stopped) {
        ndjson_stream_finish(&stream, process_stdin_record, NULL);
    }
    ndjson_stream_dispose(&stream);
    return 0;
}


static void current_card_label(wchar_t *dest, size_t dest_len) {
    if (wcscmp(g_kind, L"question") == 0) {
        wcscpy_s(dest, dest_len, L"YOU");
    } else if (wcscmp(g_kind, L"answer") == 0) {
        wcscpy_s(dest, dest_len, L"BLUEY");
    } else if (wcscmp(g_kind, L"transcript") == 0) {
        if (wcsstr(g_title, L"Mic") || wcsstr(g_source, L"microphone") || wcsstr(g_source, L"user")) {
            wcscpy_s(dest, dest_len, L"MIC");
        } else if (wcsstr(g_title, L"System") || wcsstr(g_source, L"system")) {
            wcscpy_s(dest, dest_len, L"SYSTEM");
        } else {
            wcscpy_s(dest, dest_len, L"TRANSCRIPT");
        }
    } else if (wcscmp(g_kind, L"context") == 0) {
        wcscpy_s(dest, dest_len, L"CONTEXT");
    } else if (wcscmp(g_kind, L"warning") == 0) {
        wcscpy_s(dest, dest_len, L"WARNING");
    } else {
        wcscpy_s(dest, dest_len, L"BLUEY");
    }
}

static void release_d2d_target(void) {
    if (g_d2d_brush) {
        BLUEY_COM_RELEASE(g_d2d_brush);
        g_d2d_brush = NULL;
    }
    if (g_d2d_target) {
        BLUEY_COM_RELEASE(g_d2d_target);
        g_d2d_target = NULL;
    }
}

static void release_d2d_resources(void) {
    release_d2d_target();
    if (g_fmt_pill) {
        BLUEY_COM_RELEASE(g_fmt_pill);
        g_fmt_pill = NULL;
    }
    if (g_fmt_brand) {
        BLUEY_COM_RELEASE(g_fmt_brand);
        g_fmt_brand = NULL;
    }
    if (g_fmt_label) {
        BLUEY_COM_RELEASE(g_fmt_label);
        g_fmt_label = NULL;
    }
    if (g_fmt_title) {
        BLUEY_COM_RELEASE(g_fmt_title);
        g_fmt_title = NULL;
    }
    if (g_fmt_body) {
        BLUEY_COM_RELEASE(g_fmt_body);
        g_fmt_body = NULL;
    }
    if (g_fmt_partial) {
        BLUEY_COM_RELEASE(g_fmt_partial);
        g_fmt_partial = NULL;
    }
    if (g_dwrite_factory) {
        BLUEY_COM_RELEASE(g_dwrite_factory);
        g_dwrite_factory = NULL;
    }
    if (g_d2d_factory) {
        BLUEY_COM_RELEASE(g_d2d_factory);
        g_d2d_factory = NULL;
    }
    g_d2d_available = false;
}

static D2D1_COLOR_F d2d_color_rgb(int red, int green, int blue, float alpha) {
    D2D1_COLOR_F color;
    color.r = (FLOAT)red / 255.0f;
    color.g = (FLOAT)green / 255.0f;
    color.b = (FLOAT)blue / 255.0f;
    color.a = alpha;
    return color;
}

static D2D1_RECT_F d2d_rectf(float left, float top, float right, float bottom) {
    D2D1_RECT_F rect;
    rect.left = left;
    rect.top = top;
    rect.right = right;
    rect.bottom = bottom;
    return rect;
}

static D2D1_POINT_2F d2d_point(float x, float y) {
    D2D1_POINT_2F point;
    point.x = x;
    point.y = y;
    return point;
}

static void d2d_set_brush_color(int red, int green, int blue, float alpha) {
    D2D1_COLOR_F color = d2d_color_rgb(red, green, blue, alpha);
    BLUEY_SET_COLOR(g_d2d_brush, &color);
}

static void d2d_fill_round(float left, float top, float right, float bottom, float radius, int red, int green, int blue, float alpha) {
    D2D1_ROUNDED_RECT rounded;
    rounded.rect = d2d_rectf(left, top, right, bottom);
    rounded.radiusX = radius;
    rounded.radiusY = radius;
    d2d_set_brush_color(red, green, blue, alpha);
    BLUEY_FILL_ROUNDED_RECTANGLE(g_d2d_target, &rounded, g_d2d_brush);
}

static void d2d_stroke_round(float left, float top, float right, float bottom, float radius, int red, int green, int blue, float alpha, float width) {
    D2D1_ROUNDED_RECT rounded;
    rounded.rect = d2d_rectf(left, top, right, bottom);
    rounded.radiusX = radius;
    rounded.radiusY = radius;
    d2d_set_brush_color(red, green, blue, alpha);
    BLUEY_DRAW_ROUNDED_RECTANGLE(g_d2d_target, &rounded, g_d2d_brush, width, NULL);
}

static void d2d_text(const wchar_t *text, IDWriteTextFormat *format, D2D1_RECT_F rect, int red, int green, int blue, float alpha) {
    d2d_set_brush_color(red, green, blue, alpha);
    BLUEY_DRAW_TEXT(
        g_d2d_target,
        text,
        (UINT32)wcslen(text),
        format,
        &rect,
        g_d2d_brush,
        D2D1_DRAW_TEXT_OPTIONS_NONE,
        DWRITE_MEASURING_MODE_NATURAL
    );
}

static HRESULT create_text_format(float size, DWRITE_FONT_WEIGHT weight, DWRITE_TEXT_ALIGNMENT alignment, DWRITE_PARAGRAPH_ALIGNMENT paragraph, IDWriteTextFormat **out) {
    HRESULT hr = BLUEY_CREATE_TEXT_FORMAT(
        g_dwrite_factory,
        L"Segoe UI",
        NULL,
        weight,
        DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL,
        size,
        L"en-us",
        out
    );
    if (FAILED(hr)) return hr;
    BLUEY_SET_TEXT_ALIGNMENT(*out, alignment);
    BLUEY_SET_PARAGRAPH_ALIGNMENT(*out, paragraph);
    BLUEY_SET_WORD_WRAPPING(*out, DWRITE_WORD_WRAPPING_WRAP);
    return S_OK;
}

static bool context_kind_is_image(const wchar_t *kind) {
    return _wcsicmp(kind, L"image") == 0
        || _wcsicmp(kind, L"diagram") == 0
        || _wcsicmp(kind, L"screen") == 0
        || _wcsicmp(kind, L"screenshot") == 0;
}

static void context_chip_label(const OverlayContextChip *chip, wchar_t *dest, size_t dest_len) {
    const wchar_t *title = chip->title[0] ? chip->title : L"Attached file";
    const wchar_t *prefix = _wcsicmp(chip->kind, L"web") == 0
        ? L"WEB"
        : (context_kind_is_image(chip->kind) ? L"IMG" : L"DOC");
    if (_wcsicmp(chip->processing_status, L"pending") == 0) {
        swprintf(dest, dest_len, L"%ls Reading %ls...", prefix, title);
    } else if (_wcsicmp(chip->processing_status, L"ready") == 0) {
        swprintf(dest, dest_len, L"%ls %ls - Ready", prefix, title);
    } else if (_wcsicmp(chip->processing_status, L"failed") == 0
        || _wcsicmp(chip->processing_status, L"unsupported") == 0) {
        swprintf(dest, dest_len, L"%ls %ls - Needs attention", prefix, title);
    } else {
        swprintf(dest, dest_len, L"%ls %ls", prefix, title);
    }
    dest[dest_len - 1] = L'\0';
}

static bool sent_chips_visible_for_current_card(void) {
    if (g_sent_chip_count <= 0) return false;
    if (_wcsicmp(g_kind, L"question") == 0) return true;
    return _wcsicmp(g_kind, L"context") == 0 && wide_contains_ci(g_title, L"source");
}

static bool context_chip_is_web_source(const OverlayContextChip *chip) {
    if (!chip || _wcsicmp(chip->kind, L"web") != 0) return false;
    return _wcsnicmp(chip->path, L"https://", 8) == 0
        || _wcsnicmp(chip->path, L"http://", 7) == 0;
}

static float context_chip_width(const wchar_t *label) {
    size_t len = wcslen(label);
    float width = 42.0f + (float)len * 6.3f;
    if (width < 96.0f) width = 96.0f;
    if (width > 164.0f) width = 164.0f;
    return width;
}

static int sent_source_chip_index_at_client_point(POINT point) {
    if (!g_hwnd || !sent_chips_visible_for_current_card()) return -1;

    RECT rect;
    if (!GetClientRect(g_hwnd, &rect)) return -1;
    int context_reserved = context_chips_visible() ? 34 : 0;
    int x = 18;
    int y = rect.bottom - 158 - context_reserved;
    int max_right = rect.right - 18;

    for (int i = 0; i < g_sent_chip_count && x < max_right - 50; i++) {
        wchar_t label[360];
        context_chip_label(&g_sent_chips[i], label, sizeof(label) / sizeof(label[0]));
        int width = (int)context_chip_width(label);
        if (x + width > max_right) width = max_right - x;
        RECT chip_rect = {x, y, x + width, y + 26};
        if (PtInRect(&chip_rect, point) && context_chip_is_web_source(&g_sent_chips[i])) {
            return i;
        }
        x += width + 6;
    }
    return -1;
}

static int sent_source_chip_index_at_screen_point(POINT point) {
    if (!g_hwnd || !ScreenToClient(g_hwnd, &point)) return -1;
    return sent_source_chip_index_at_client_point(point);
}

static bool open_sent_source_chip_at_client_point(POINT point) {
    int index = sent_source_chip_index_at_client_point(point);
    if (index < 0) return false;
    ShellExecuteW(NULL, L"open", g_sent_chips[index].path, NULL, NULL, SW_SHOWNORMAL);
    emit_lifecycle_event("source_opened", "ok", "platform=windows kind=web");
    return true;
}

static bool should_draw_context_chip(const OverlayContextChip *chip) {
    return g_show_context_chips || (g_pending_context_chips && chip->pending);
}

static void draw_context_chips_d2d(RECT rect) {
    if (!context_chips_visible()) return;

    int composer_w = clamp_int((rect.right * 72) / 100, 560, 760);
    int composer_left = (rect.right - composer_w) / 2;
    float x = (float)composer_left + 8.0f;
    float y = (float)rect.bottom - 145.0f;
    float max_right = (float)(composer_left + composer_w - 8);

    for (int i = 0; i < g_context_chip_count && x < max_right - 50.0f; i++) {
        if (!should_draw_context_chip(&g_context_chips[i])) continue;
        wchar_t label[360];
        context_chip_label(&g_context_chips[i], label, sizeof(label) / sizeof(label[0]));
        float width = context_chip_width(label);
        if (x + width > max_right) width = max_right - x;
        bool is_image = context_kind_is_image(g_context_chips[i].kind);
        d2d_fill_round(x, y, x + width, y + 26.0f, 13.0f, is_image ? 18 : 20, is_image ? 35 : 24, is_image ? 58 : 34, 0.92f);
        d2d_stroke_round(x + 0.5f, y + 0.5f, x + width - 0.5f, y + 25.5f, 13.0f, is_image ? 76 : 48, is_image ? 180 : 126, is_image ? 220 : 150, 0.42f, 1.0f);
        d2d_text(label, g_fmt_partial, d2d_rectf(x + 10.0f, y + 3.0f, x + width - 8.0f, y + 24.0f), is_image ? 150 : 230, is_image ? 220 : 240, is_image ? 255 : 245, 1.0f);
        x += width + 6.0f;
    }
}

static void draw_sent_chips_d2d(RECT rect, float context_reserved) {
    if (!sent_chips_visible_for_current_card()) return;

    float x = 18.0f;
    float y = (float)rect.bottom - 158.0f - context_reserved;
    float max_right = (float)rect.right - 18.0f;

    for (int i = 0; i < g_sent_chip_count && x < max_right - 50.0f; i++) {
        wchar_t label[360];
        context_chip_label(&g_sent_chips[i], label, sizeof(label) / sizeof(label[0]));
        float width = context_chip_width(label);
        if (x + width > max_right) width = max_right - x;
        bool is_image = context_kind_is_image(g_sent_chips[i].kind);
        d2d_fill_round(x, y, x + width, y + 26.0f, 13.0f, is_image ? 205 : 226, is_image ? 232 : 237, is_image ? 248 : 242, 0.96f);
        d2d_stroke_round(x + 0.5f, y + 0.5f, x + width - 0.5f, y + 25.5f, 13.0f, is_image ? 68 : 24, is_image ? 180 : 70, is_image ? 220 : 82, 0.36f, 1.0f);
        d2d_text(label, g_fmt_partial, d2d_rectf(x + 10.0f, y + 3.0f, x + width - 8.0f, y + 24.0f), 20, 42, 54, 1.0f);
        x += width + 6.0f;
    }
}

static void draw_context_chips_gdi(HDC hdc, RECT rect) {
    if (!context_chips_visible()) return;

    int composer_w = clamp_int((rect.right * 72) / 100, 560, 760);
    int composer_left = (rect.right - composer_w) / 2;
    int x = composer_left + 8;
    int y = rect.bottom - 145;
    int max_right = composer_left + composer_w - 8;

    HFONT font = CreateFontW(12, 0, 0, 0, FW_SEMIBOLD, FALSE, FALSE, FALSE,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
        DEFAULT_PITCH | FF_SWISS, L"Segoe UI");
    HGDIOBJ old_font = SelectObject(hdc, font);
    SetBkMode(hdc, TRANSPARENT);

    for (int i = 0; i < g_context_chip_count && x < max_right - 50; i++) {
        if (!should_draw_context_chip(&g_context_chips[i])) continue;
        wchar_t label[360];
        context_chip_label(&g_context_chips[i], label, sizeof(label) / sizeof(label[0]));
        int width = (int)context_chip_width(label);
        if (x + width > max_right) width = max_right - x;
        bool is_image = context_kind_is_image(g_context_chips[i].kind);
        HBRUSH bg = CreateSolidBrush(is_image ? RGB(18, 35, 58) : RGB(20, 24, 34));
        HPEN border = CreatePen(PS_SOLID, 1, is_image ? RGB(76, 180, 220) : RGB(48, 126, 150));
        HGDIOBJ old_brush = SelectObject(hdc, bg);
        HGDIOBJ old_pen = SelectObject(hdc, border);
        RoundRect(hdc, x, y, x + width, y + 26, 18, 18);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(bg);
        DeleteObject(border);

        SetTextColor(hdc, is_image ? RGB(150, 220, 255) : RGB(230, 240, 245));
        RECT label_rect = {x + 10, y + 4, x + width - 8, y + 24};
        DrawTextW(hdc, label, -1, &label_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
        x += width + 6;
    }

    SelectObject(hdc, old_font);
    DeleteObject(font);
}

static void draw_sent_chips_gdi(HDC hdc, RECT rect, int context_reserved) {
    if (!sent_chips_visible_for_current_card()) return;

    int x = 18;
    int y = rect.bottom - 158 - context_reserved;
    int max_right = rect.right - 18;

    HFONT font = CreateFontW(12, 0, 0, 0, FW_SEMIBOLD, FALSE, FALSE, FALSE,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
        DEFAULT_PITCH | FF_SWISS, L"Segoe UI");
    HGDIOBJ old_font = SelectObject(hdc, font);
    SetBkMode(hdc, TRANSPARENT);

    for (int i = 0; i < g_sent_chip_count && x < max_right - 50; i++) {
        wchar_t label[360];
        context_chip_label(&g_sent_chips[i], label, sizeof(label) / sizeof(label[0]));
        int width = (int)context_chip_width(label);
        if (x + width > max_right) width = max_right - x;
        bool is_image = context_kind_is_image(g_sent_chips[i].kind);
        HBRUSH bg = CreateSolidBrush(is_image ? RGB(205, 232, 248) : RGB(226, 237, 242));
        HPEN border = CreatePen(PS_SOLID, 1, is_image ? RGB(68, 180, 220) : RGB(24, 70, 82));
        HGDIOBJ old_brush = SelectObject(hdc, bg);
        HGDIOBJ old_pen = SelectObject(hdc, border);
        RoundRect(hdc, x, y, x + width, y + 26, 18, 18);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(bg);
        DeleteObject(border);

        SetTextColor(hdc, RGB(20, 42, 54));
        RECT label_rect = {x + 10, y + 4, x + width - 8, y + 24};
        DrawTextW(hdc, label, -1, &label_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
        x += width + 6;
    }

    SelectObject(hdc, old_font);
    DeleteObject(font);
}

static bool init_d2d_resources(void) {
    if (g_d2d_available) return true;

    D2D1_FACTORY_OPTIONS options;
    ZeroMemory(&options, sizeof(options));
    HRESULT hr = BLUEY_D2D_CREATE_FACTORY(&options, &g_d2d_factory);
    if (FAILED(hr)) {
        release_d2d_resources();
        return false;
    }

    hr = BLUEY_DWRITE_CREATE_FACTORY(&g_dwrite_factory);
    if (FAILED(hr)) {
        release_d2d_resources();
        return false;
    }

    if (FAILED(create_text_format(13.0f, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, &g_fmt_pill)) ||
        FAILED(create_text_format(20.0f, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, &g_fmt_brand)) ||
        FAILED(create_text_format(13.0f, DWRITE_FONT_WEIGHT_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, &g_fmt_label)) ||
        FAILED(create_text_format(21.0f, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, &g_fmt_title)) ||
        FAILED(create_text_format(16.0f, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, &g_fmt_body)) ||
        FAILED(create_text_format(13.0f, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, &g_fmt_partial))) {
        release_d2d_resources();
        return false;
    }

    g_d2d_available = true;
    return true;
}

static bool ensure_d2d_target(HWND hwnd) {
    if (!init_d2d_resources()) return false;
    if (g_d2d_target) return true;

    RECT rect;
    GetClientRect(hwnd, &rect);
    D2D1_RENDER_TARGET_PROPERTIES properties;
    ZeroMemory(&properties, sizeof(properties));
    properties.type = D2D1_RENDER_TARGET_TYPE_DEFAULT;
    properties.pixelFormat.format = DXGI_FORMAT_UNKNOWN;
    properties.pixelFormat.alphaMode = D2D1_ALPHA_MODE_UNKNOWN;
    properties.dpiX = 0.0f;
    properties.dpiY = 0.0f;
    properties.usage = D2D1_RENDER_TARGET_USAGE_NONE;
    properties.minLevel = D2D1_FEATURE_LEVEL_DEFAULT;

    D2D1_HWND_RENDER_TARGET_PROPERTIES hwnd_properties;
    ZeroMemory(&hwnd_properties, sizeof(hwnd_properties));
    hwnd_properties.hwnd = hwnd;
    hwnd_properties.pixelSize.width = (UINT32)(rect.right - rect.left);
    hwnd_properties.pixelSize.height = (UINT32)(rect.bottom - rect.top);
    hwnd_properties.presentOptions = D2D1_PRESENT_OPTIONS_NONE;

    HRESULT hr = BLUEY_CREATE_HWND_RENDER_TARGET(
        g_d2d_factory,
        &properties,
        &hwnd_properties,
        &g_d2d_target
    );
    if (FAILED(hr)) {
        release_d2d_target();
        return false;
    }

    D2D1_COLOR_F brush_color = d2d_color_rgb(255, 255, 255, 1.0f);
    hr = BLUEY_CREATE_SOLID_COLOR_BRUSH(
        g_d2d_target,
        &brush_color,
        NULL,
        &g_d2d_brush
    );
    if (FAILED(hr)) {
        release_d2d_target();
        return false;
    }

    return true;
}

static void resize_d2d_target(HWND hwnd) {
    if (!g_d2d_target) return;
    RECT rect;
    GetClientRect(hwnd, &rect);
    D2D1_SIZE_U size;
    size.width = (UINT32)(rect.right - rect.left);
    size.height = (UINT32)(rect.bottom - rect.top);
    if (FAILED(BLUEY_TARGET_RESIZE(g_d2d_target, &size))) {
        release_d2d_target();
    }
}

static void draw_bluey_logo_d2d(float x, float y, float size) {
    d2d_fill_round(x, y, x + size, y + size, size * 0.22f, 12, 52, 78, 1.0f);
    d2d_stroke_round(x + 0.5f, y + 0.5f, x + size - 0.5f, y + size - 0.5f, size * 0.22f, 120, 236, 246, 1.0f, 1.2f);
    d2d_fill_round(x + size * 0.20f, y + size * 0.33f, x + size * 0.80f, y + size * 0.76f, size * 0.14f, 6, 17, 31, 1.0f);
    d2d_stroke_round(x + size * 0.20f, y + size * 0.33f, x + size * 0.80f, y + size * 0.76f, size * 0.14f, 100, 233, 255, 1.0f, 1.6f);

    d2d_set_brush_color(245, 252, 255, 1.0f);
    BLUEY_DRAW_LINE(g_d2d_target, d2d_point(x + size * 0.34f, y + size * 0.45f), d2d_point(x + size * 0.44f, y + size * 0.50f), g_d2d_brush, 2.0f, NULL);
    BLUEY_DRAW_LINE(g_d2d_target, d2d_point(x + size * 0.44f, y + size * 0.50f), d2d_point(x + size * 0.34f, y + size * 0.58f), g_d2d_brush, 2.0f, NULL);
    d2d_set_brush_color(122, 248, 255, 1.0f);
    BLUEY_DRAW_LINE(g_d2d_target, d2d_point(x + size * 0.50f, y + size * 0.61f), d2d_point(x + size * 0.66f, y + size * 0.61f), g_d2d_brush, 2.0f, NULL);

    d2d_set_brush_color(139, 255, 157, 1.0f);
    BLUEY_DRAW_LINE(g_d2d_target, d2d_point(x + size * 0.72f, y + size * 0.20f), d2d_point(x + size * 0.72f, y + size * 0.52f), g_d2d_brush, 1.6f, NULL);
    BLUEY_DRAW_LINE(g_d2d_target, d2d_point(x + size * 0.56f, y + size * 0.36f), d2d_point(x + size * 0.88f, y + size * 0.36f), g_d2d_brush, 1.6f, NULL);
}

static void draw_resize_affordance_d2d(RECT rect) {
    d2d_stroke_round((float)rect.left + 1.0f, (float)rect.top + 1.0f, (float)rect.right - 1.0f, (float)rect.bottom - 1.0f, 18.0f, 38, 98, 138, 0.8f, 1.0f);
    d2d_set_brush_color(120, 236, 246, 1.0f);
    float right = (float)rect.right - 11.0f;
    float bottom = (float)rect.bottom - 10.0f;
    for (int i = 0; i < 3; i++) {
        float offset = 7.0f + ((float)i * 6.0f);
        BLUEY_DRAW_LINE(g_d2d_target, d2d_point(right - offset, bottom), d2d_point(right, bottom - offset), g_d2d_brush, 2.0f, NULL);
    }
}

static bool paint_with_d2d(HWND hwnd, ULONGLONG *painted_revision) {
    if (!ensure_d2d_target(hwnd)) return false;
    if (painted_revision) *painted_revision = 0;

    RECT rect;
    GetClientRect(hwnd, &rect);
    BLUEY_BEGIN_DRAW(g_d2d_target);
    D2D1_COLOR_F bg_color = g_light_theme ? d2d_color_rgb(246, 250, 252, 1.0f) : d2d_color_rgb(2, 4, 6, 1.0f);
    BLUEY_CLEAR(g_d2d_target, &bg_color);

    if (g_collapsed) {
        draw_bluey_logo_d2d(10.0f, 8.0f, 25.0f);
        d2d_set_brush_color(62, 220, 128, 1.0f);
        D2D1_ELLIPSE dot;
        dot.point = d2d_point((float)rect.right - 17.0f, 14.0f);
        dot.radiusX = 5.0f;
        dot.radiusY = 5.0f;
        BLUEY_FILL_ELLIPSE(g_d2d_target, &dot, g_d2d_brush);
        d2d_text(L"bluey", g_fmt_pill, d2d_rectf(40.0f, 0.0f, (float)rect.right - 16.0f, (float)rect.bottom), g_light_theme ? 8 : 235, g_light_theme ? 22 : 245, g_light_theme ? 32 : 255, 1.0f);
    } else {
        int header_w = clamp_int((rect.right * 84) / 100, 520, 780);
        if (header_w > rect.right - 28) header_w = rect.right - 28;
        int header_left = (rect.right - header_w) / 2;
        int composer_w = clamp_int((rect.right * 72) / 100, 560, 760);
        int composer_left = (rect.right - composer_w) / 2;

        d2d_fill_round((float)header_left, 8.0f, (float)(header_left + header_w), 54.0f, 22.0f, g_light_theme ? 248 : 5, g_light_theme ? 252 : 11, g_light_theme ? 255 : 18, 1.0f);
        d2d_stroke_round((float)header_left + 0.5f, 8.5f, (float)(header_left + header_w) - 0.5f, 53.5f, 22.0f, g_light_theme ? BLUEY_LIGHT_ACCENT_R : 36, g_light_theme ? BLUEY_LIGHT_ACCENT_G : 102, g_light_theme ? BLUEY_LIGHT_ACCENT_B : 124, g_light_theme ? 0.82f : 0.9f, g_light_theme ? 1.5f : 1.2f);

        d2d_fill_round((float)composer_left, (float)rect.bottom - 110.0f, (float)(composer_left + composer_w), (float)rect.bottom - 8.0f, 22.0f, g_light_theme ? 248 : 5, g_light_theme ? 252 : 11, g_light_theme ? 255 : 18, 1.0f);
        d2d_stroke_round((float)composer_left + 0.5f, (float)rect.bottom - 109.5f, (float)(composer_left + composer_w) - 0.5f, (float)rect.bottom - 8.5f, 22.0f, g_light_theme ? BLUEY_LIGHT_ACCENT_R : 38, g_light_theme ? BLUEY_LIGHT_ACCENT_G : 98, g_light_theme ? BLUEY_LIGHT_ACCENT_B : 138, g_light_theme ? 0.86f : 0.9f, g_light_theme ? 1.5f : 1.2f);

        draw_resize_affordance_d2d(rect);
        draw_bluey_logo_d2d((float)header_left + 14.0f, 14.0f, 28.0f);

        d2d_set_brush_color(g_recording ? 62 : 40, g_recording ? 220 : 92, g_recording ? 128 : 62, 1.0f);
        D2D1_ELLIPSE record_dot;
        record_dot.point = d2d_point((float)header_left + 114.5f, 29.5f);
        record_dot.radiusX = 4.5f;
        record_dot.radiusY = 4.5f;
        BLUEY_FILL_ELLIPSE(g_d2d_target, &record_dot, g_d2d_brush);

        d2d_text(L"bluey", g_fmt_brand, d2d_rectf((float)header_left + 50.0f, 10.0f, (float)header_left + 118.0f, 46.0f), g_light_theme ? 8 : 230, g_light_theme ? 22 : 240, g_light_theme ? 32 : 245, 1.0f);
        if (wcslen(g_active_session_code) > 0) {
            wchar_t session_label[48];
            swprintf_s(session_label, 48, L"ID %s", g_active_session_code);
            d2d_text(session_label, g_fmt_label, d2d_rectf((float)header_left + 126.0f, 18.0f, (float)header_left + 220.0f, 42.0f), g_light_theme ? 22 : 168, g_light_theme ? 74 : 210, g_light_theme ? 96 : 226, 1.0f);
        }

        if (!g_interactive_mode) {
            RECT handle = clickthrough_move_handle_rect(rect);
            d2d_fill_round(
                (float)handle.left,
                (float)handle.top,
                (float)handle.right,
                (float)handle.bottom,
                9.0f,
                g_light_theme ? BLUEY_LIGHT_ACCENT_R : 0,
                g_light_theme ? BLUEY_LIGHT_ACCENT_G : 174,
                g_light_theme ? BLUEY_LIGHT_ACCENT_B : 236,
                g_light_theme ? 0.16f : 0.18f);
            d2d_stroke_round(
                (float)handle.left + 0.5f,
                (float)handle.top + 0.5f,
                (float)handle.right - 0.5f,
                (float)handle.bottom - 0.5f,
                9.0f,
                g_light_theme ? BLUEY_LIGHT_ACCENT_R : 66,
                g_light_theme ? BLUEY_LIGHT_ACCENT_G : 190,
                g_light_theme ? BLUEY_LIGHT_ACCENT_B : 255,
                0.95f,
                1.5f);
            d2d_set_brush_color(g_light_theme ? 0 : 102, g_light_theme ? 126 : 213, g_light_theme ? 178 : 255, 1.0f);
            float cx = ((float)handle.left + (float)handle.right) * 0.5f;
            float cy = ((float)handle.top + (float)handle.bottom) * 0.5f;
            BLUEY_DRAW_LINE(g_d2d_target, d2d_point(cx, (float)handle.top + 6.0f), d2d_point(cx, (float)handle.bottom - 6.0f), g_d2d_brush, 1.8f, NULL);
            BLUEY_DRAW_LINE(g_d2d_target, d2d_point((float)handle.left + 6.0f, cy), d2d_point((float)handle.right - 6.0f, cy), g_d2d_brush, 1.8f, NULL);
        }

        wchar_t card_label[32];
        current_card_label(card_label, 32);
        if (wcscmp(card_label, L"MIC") == 0) {
            d2d_text(card_label, g_fmt_label, d2d_rectf(18.0f, 60.0f, (float)rect.right - 18.0f, 82.0f), 118, 242, 153, 1.0f);
        } else {
            d2d_text(card_label, g_fmt_label, d2d_rectf(18.0f, 60.0f, (float)rect.right - 18.0f, 82.0f), 112, 238, 248, 1.0f);
        }

        bool show_title = wcscmp(g_kind, L"transcript") != 0
            && wcscmp(g_kind, L"question") != 0
            && wcscmp(g_kind, L"answer") != 0
            && wcslen(g_title) > 0;
        int body_top = show_title ? 108 : 86;
        if (show_title) {
            d2d_text(g_title, g_fmt_title, d2d_rectf(18.0f, 82.0f, (float)rect.right - 18.0f, 108.0f), g_light_theme ? 8 : 230, g_light_theme ? 22 : 240, g_light_theme ? 32 : 245, 1.0f);
        }
        float context_reserved = context_chips_visible() ? 34.0f : 0.0f;
        float sent_reserved = sent_chips_visible_for_current_card() ? 34.0f : 0.0f;
        AcquireSRWLockShared(&g_body_lock);
        if (painted_revision) {
            *painted_revision = current_presentation_revision();
        }
        d2d_text(g_body, g_fmt_body, d2d_rectf(18.0f, (float)body_top, (float)rect.right - 18.0f, (float)rect.bottom - 124.0f - context_reserved - sent_reserved), g_light_theme ? 22 : 230, g_light_theme ? 43 : 240, g_light_theme ? 56 : 245, 1.0f);
        ReleaseSRWLockShared(&g_body_lock);
        draw_sent_chips_d2d(rect, context_reserved);

        /* Transcript overlay: bottom-right floating banner (~400x80) */
        {
            float tx_right = (float)rect.right - 12.0f;
            float tx_bottom = (float)rect.bottom - 126.0f - context_reserved;
            float tx_left = tx_right - 400.0f;
            if (tx_left < 12.0f) tx_left = 12.0f;
            float tx_top = tx_bottom - 80.0f;

            /* Session banner (3 seconds) */
            if (g_session_banner[0] && (GetTickCount64() - g_session_banner_tick) < 3000) {
                d2d_fill_round(tx_left, tx_top, tx_right, tx_bottom, 8.0f, 10, 30, 50, 0.85f);
                d2d_text(g_session_banner, g_fmt_label, d2d_rectf(tx_left + 10.0f, tx_top + 4.0f, tx_right - 10.0f, tx_bottom - 4.0f), 100, 230, 255, 1.0f);
            } else if (g_audio_auto_stop_remaining_secs >= 0) {
                wchar_t countdown[160];
                if (g_audio_auto_stop_remaining_secs > 0) {
                    swprintf_s(
                        countdown,
                        160,
                        L"No speech detected. Listen stops in %d s to limit billing.",
                        g_audio_auto_stop_remaining_secs
                    );
                } else {
                    swprintf_s(
                        countdown,
                        160,
                        L"Listen auto-stopped after %d s without speech.",
                        g_audio_auto_stop_idle_secs
                    );
                }
                d2d_fill_round(tx_left, tx_top, tx_right, tx_bottom, 8.0f, 48, 32, 6, 0.88f);
                d2d_text(countdown, g_fmt_body, d2d_rectf(tx_left + 10.0f, tx_top + 8.0f, tx_right - 10.0f, tx_bottom - 6.0f), 255, 188, 84, 1.0f);
            } else if (g_transcript_final[0] || g_transcript_partial[0]) {
                d2d_fill_round(tx_left, tx_top, tx_right, tx_bottom, 8.0f, 5, 10, 18, 0.82f);
                /* Final text (normal, white) */
                if (g_transcript_final[0]) {
                    d2d_text(g_transcript_final, g_fmt_body, d2d_rectf(tx_left + 10.0f, tx_top + 4.0f, tx_right - 10.0f, tx_top + 44.0f), 235, 245, 255, 1.0f);
                }
                /* Partial text (dim, italic via g_fmt_partial) */
                if (g_transcript_partial[0]) {
                    d2d_text(g_transcript_partial, g_fmt_partial, d2d_rectf(tx_left + 10.0f, tx_top + 44.0f, tx_right - 10.0f, tx_bottom - 4.0f), 180, 200, 220, 0.7f);
                }
            }
        }

        draw_context_chips_d2d(rect);
    }

    HRESULT hr = BLUEY_END_DRAW(g_d2d_target, NULL, NULL);
    if (hr == D2DERR_RECREATE_TARGET) {
        release_d2d_target();
        return false;
    }
    return SUCCEEDED(hr);
}

static LRESULT CALLBACK wnd_proc(HWND hwnd, UINT msg, WPARAM wparam, LPARAM lparam) {
    switch (msg) {
    case WM_SIZE:
        resize_d2d_target(hwnd);
        layout_controls();
        return 0;
    case WM_DRAWITEM: {
        DRAWITEMSTRUCT *item = (DRAWITEMSTRUCT *)lparam;
        if (item && item->CtlType == ODT_BUTTON) {
            draw_dark_button(item);
            return TRUE;
        }
        break;
    }
    case WM_BLUEY_MEETING_EVIDENCE: {
        MeetingEvidencePayload *evidence = (MeetingEvidencePayload *)lparam;
        if (evidence) {
            if (meeting_detection_enabled()) {
                emit_meeting_evidence_event(evidence);
            }
            free(evidence);
        }
        return 0;
    }
    case WM_BLUEY_MEETING_DETECTION_DISABLED:
        hide_meeting_banner(NULL);
        return 0;
    case WM_TIMER:
        if (wparam == ID_RENDER_ACK_RETRY_TIMER) {
            KillTimer(hwnd, ID_RENDER_ACK_RETRY_TIMER);
            InvalidateRect(hwnd, NULL, FALSE);
            return 0;
        }
        if (wparam == ID_AUTOSEND_TIMER) {
            KillTimer(hwnd, ID_AUTOSEND_TIMER);
            g_auto_send_timer_armed = false;
            if (g_recording && has_auto_send_context()) {
                emit_lifecycle_event("autosend_answer_sent", "ok", "trigger=caption_settle platform=windows");
                send_current_question_now(true);
            } else {
                emit_lifecycle_event(
                    "autosend_answer_skipped",
                    g_recording ? "empty" : "manual_stop",
                    "trigger=caption_settle platform=windows"
                );
            }
            return 0;
        }
        if (wparam == ID_MANUAL_SEND_TIMER) {
            KillTimer(hwnd, ID_MANUAL_SEND_TIMER);
            ULONGLONG elapsed = g_manual_send_started_ms == 0
                ? MANUAL_CAPTION_SETTLE_MAX_MS
                : GetTickCount64() - g_manual_send_started_ms;
            if (g_recording && !has_transcript_context() && elapsed < MANUAL_CAPTION_SETTLE_MAX_MS) {
                UINT remaining = (UINT)(MANUAL_CAPTION_SETTLE_MAX_MS - elapsed);
                arm_manual_send_timer(remaining < 250 ? remaining : 250, "awaiting_first_caption");
                return 0;
            }
            g_manual_send_timer_armed = false;
            g_manual_send_started_ms = 0;
            send_current_question_now(true);
            return 0;
        }
        break;
    case WM_CTLCOLOREDIT:
    case WM_CTLCOLORSTATIC: {
        HDC hdc = (HDC)wparam;
        HWND control = (HWND)lparam;
        if (control == g_ask_edit) {
            SetTextColor(hdc, g_light_theme ? RGB(8, 22, 32) : RGB(238, 244, 250));
            SetBkColor(hdc, g_light_theme ? RGB(248, 252, 255) : RGB(4, 8, 13));
            return (LRESULT)g_edit_brush;
        }
        break;
    }
    case WM_COMMAND: {
        int id = LOWORD(wparam);
        int notify = HIWORD(wparam);
        if (id == ID_ASK_EDIT && notify == EN_CHANGE) {
            return 0;
        }
        if (id == ID_ASK_EDIT && notify == EN_MAXTEXT) {
            MessageBeep(MB_ICONINFORMATION);
            return 0;
        }
        if (id == ID_SEND_BUTTON) {
            send_current_question();
            return 0;
        }
        if (id == ID_AUTO_SEND_BUTTON) {
            if (HIWORD(wparam) == CBN_SELCHANGE) {
                int selected = (int)SendMessageW(g_auto_send_combo, CB_GETCURSEL, 0, 0);
                g_auto_send_mode = selected < 0 ? 0 : selected;
                update_auto_send_control();
            }
            return 0;
        }
        if (id == ID_ANSWER_DETAIL_BUTTON) {
            if (HIWORD(wparam) == CBN_SELCHANGE) {
                int selected = (int)SendMessageW(g_answer_detail_combo, CB_GETCURSEL, 0, 0);
                g_answer_detail_mode = selected < 0 ? 0 : selected;
            }
            return 0;
        }
        if (id == ID_RECORD_BUTTON) {
            DWORD now_ms = GetTickCount();
            if ((DWORD)(now_ms - g_last_record_toggle_ms) < 300) {
                return 0;
            }
            if (!g_recording && g_record_restart_after_ms != 0 && (LONG)(now_ms - g_record_restart_after_ms) < 0) {
                return 0;
            }
            bool was_recording = g_recording;
            bool next_recording = !g_recording;
            if (!emit_simple_event(
                    next_recording
                        ? "recording_start_requested"
                        : "recording_stop_requested")) {
                MessageBeep(MB_ICONWARNING);
                return 0;
            }
            g_last_record_toggle_ms = now_ms;
            g_recording = next_recording;
            g_record_restart_after_ms = was_recording ? now_ms + 1200 : 0;
            update_record_button();
            InvalidateRect(hwnd, NULL, TRUE);
            if (was_recording) {
                cancel_auto_send_timer("record_button");
            }
            return 0;
        }
        if (id == ID_TRANSCRIPT_CLEAR_BUTTON) {
            if (!emit_simple_event("transcript_clear_requested")) {
                MessageBeep(MB_ICONWARNING);
                return 0;
            }
            clear_local_transcript_context();
            InvalidateRect(hwnd, NULL, TRUE);
            return 0;
        }
        if (id == ID_PASTE_ANSWER_BUTTON) {
            return 0;
        }
        if (id == ID_HELP_BUTTON) {
            overlay_message_box(
                L"Green dot: Bluey is connected.\nHelp: show this guide.\nShortcuts: show controls and shortcuts.\nSession: continue or start clean.\nAttach: add files or show attached docs.\nTheme: switch black/white background while keeping Bluey borders.\nStyle: answer rules.\nAnalyse Screen: search/read the active browser page or available screen context and generate an answer.\nRecap: summarize the active session from the bottom bar.\nQuit: stop Bluey completely. Hide minimizes to the small button.\nMic: start/stop audio capture.\nMic dot: dim off, bright green recording.\nAnswer: ask Bluey.\nWhen click-through is off, blank Bluey space drags the window. When click-through is on, blank space clicks behind Bluey; drag the cyan move handle to reposition.",
                L"Bluey controls",
                MB_OK | MB_ICONINFORMATION
            );
            return 0;
        }
        if (id == ID_SHORTCUTS_BUTTON) {
            wchar_t shortcut_body[1800];
            swprintf(
                shortcut_body,
                sizeof(shortcut_body) / sizeof(shortcut_body[0]),
                L"%ls\n"
                L"%ls\n\n"
                L"Inside Bluey when click-through is off:\n\n"
                L"Enter Answer        Esc Close panel\n"
                L"Tab Next control    Shift+Tab Previous control\n"
                L"Letters always type normally when Ask is focused.\n\n"
                L"Global shortcuts work in both modes:\n"
                L"Ctrl+Alt+B         Minimize to pill / restore\n"
                L"Ctrl+Alt+T         Text input\n"
                L"Ctrl+Alt+L         Start or stop Listen\n"
                L"Ctrl+Alt+S         Capture screen context\n"
                L"Ctrl+Alt+I         Toggle click-through\n"
                L"Ctrl+Alt+H         History\n"
                L"Ctrl+Alt+F         Files\n"
                L"Ctrl+Alt+Enter     Answer\n\n"
                L"Ask focused: type normally. Enter answers. Shift+Enter adds a new line.",
                g_interactive_mode ? L"Click-through is off" : L"Click-through is on",
                g_interactive_mode
                    ? L"Blank Bluey space drags the window. Tab selects Bluey controls; Enter opens the selected control."
                    : L"Blank Bluey space clicks behind it. Drag the cyan move handle to move."
            );
            overlay_message_box(
                shortcut_body,
                L"Controls and shortcuts",
                MB_OK | MB_ICONINFORMATION
            );
            emit_lifecycle_event("shortcuts_overlay_opened", "ok", "platform=windows");
            return 0;
        }
        if (id == ID_RECAP_BUTTON) {
            emit_simple_event("recap_requested");
            return 0;
        }
        if (id == ID_SESSION_BUTTON) {
            int answer = overlay_message_box(
                L"Choose Yes to continue the active session with its transcript and attached context. Choose No to archive it and start a clean session.",
                L"Session",
                MB_YESNOCANCEL | MB_ICONINFORMATION
            );
            if (answer == IDYES) emit_simple_event("session_continue_requested");
            if (answer == IDNO) emit_simple_event("session_new_requested");
            return 0;
        }
        if (id == ID_ATTACH_BUTTON) {
            int answer = overlay_message_box(
                L"Choose Yes to attach new files. Choose No to show the documents, screenshots, page captures, and notes already attached to this session.",
                L"Session attachments",
                MB_YESNOCANCEL | MB_ICONINFORMATION
            );
            if (answer == IDYES) {
                g_show_context_chips = false;
                g_pending_context_chips = false;
                expect_pending_context_chips();
                emit_simple_event("attach_requested");
            }
            if (answer == IDNO) {
                g_show_context_chips = true;
                g_pending_context_chips = false;
                g_pending_context_expected_until_ms = 0;
                InvalidateRect(hwnd, NULL, TRUE);
                emit_simple_event("context_list_requested");
            }
            return 0;
        }
        if (id == ID_PAGE_BUTTON) {
            int answer = overlay_message_box(
                L"Bluey will read the active browser page or available screen context, attach that context, and generate an answer. The overlay is excluded from normal screen capture.",
                L"Analyse screen context?",
                MB_OKCANCEL | MB_ICONINFORMATION
            );
            if (answer == IDOK) {
                g_show_context_chips = false;
                g_pending_context_chips = false;
                expect_pending_context_chips();
                emit_simple_event("analyze_screen_requested");
            }
            return 0;
        }
        if (id == ID_NOTE_BUTTON) {
            emit_simple_event("instructions_requested");
            return 0;
        }
        if (id == ID_THEME_BUTTON) {
            g_light_theme = !g_light_theme;
            refresh_edit_brush();
            SetWindowTextW(g_theme_button, g_light_theme ? L"Black" : L"White");
            InvalidateRect(hwnd, NULL, TRUE);
            return 0;
        }
        if (id == ID_CLOSE_BUTTON) {
            int answer = overlay_message_box(
                L"This stops Bluey completely, the same as running bluey off. Sessions are saved for dashboard/history. Use hide/collapse if you only want the small Bluey button.",
                L"Quit Bluey?",
                MB_OKCANCEL | MB_ICONINFORMATION
            );
            if (answer == IDOK) emit_simple_event("close_requested");
            return 0;
        }
        break;
    }
    case WM_LBUTTONDOWN:
        if (g_collapsed) {
            g_collapsed_dragging = true;
            g_collapsed_drag_moved = false;
            GetCursorPos(&g_collapsed_drag_start);
            GetWindowRect(hwnd, &g_collapsed_drag_rect);
            SetCapture(hwnd);
            return 0;
        }
        {
            POINT local_point = {GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
            if (open_sent_source_chip_at_client_point(local_point)) return 0;
        }
        if (GetFocus() == g_ask_edit) {
            POINT point;
            GetCursorPos(&point);
            if (!point_hits_visible_child(g_ask_edit, point, 0)) {
                SetFocus(hwnd);
            }
        }
        break;
    case WM_MOUSEMOVE:
        if (g_collapsed && g_collapsed_dragging) {
            POINT current;
            GetCursorPos(&current);
            int dx = current.x - g_collapsed_drag_start.x;
            int dy = current.y - g_collapsed_drag_start.y;
            if (abs(dx) > COLLAPSED_DRAG_THRESHOLD || abs(dy) > COLLAPSED_DRAG_THRESHOLD) {
                g_collapsed_drag_moved = true;
            }

            RECT work = {0, 0, 0, 0};
            MONITORINFO monitor = {0};
            monitor.cbSize = sizeof(monitor);
            HMONITOR hmonitor = MonitorFromRect(&g_collapsed_drag_rect, MONITOR_DEFAULTTONEAREST);
            if (GetMonitorInfoW(hmonitor, &monitor)) {
                work = monitor.rcWork;
            } else {
                SystemParametersInfoW(SPI_GETWORKAREA, 0, &work, 0);
            }

            int width = g_collapsed_drag_rect.right - g_collapsed_drag_rect.left;
            int height = g_collapsed_drag_rect.bottom - g_collapsed_drag_rect.top;
            int x = clamp_int(g_collapsed_drag_rect.left + dx, work.left + 8, work.right - width - 8);
            int y = clamp_int(g_collapsed_drag_rect.top + dy, work.top + 8, work.bottom - height - 8);
            SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_NOACTIVATE);
            return 0;
        }
        break;
    case WM_LBUTTONUP:
        if (g_collapsed) {
            if (g_collapsed_dragging) {
                POINT current;
                GetCursorPos(&current);
                int dx = current.x - g_collapsed_drag_start.x;
                int dy = current.y - g_collapsed_drag_start.y;
                if (abs(dx) > COLLAPSED_DRAG_THRESHOLD || abs(dy) > COLLAPSED_DRAG_THRESHOLD) {
                    g_collapsed_drag_moved = true;
                }
                ReleaseCapture();
                g_collapsed_dragging = false;
                if (g_collapsed_drag_moved) {
                    GetWindowRect(hwnd, &g_collapsed_rect);
                    g_collapsed_rect = clamp_rect_to_work_area(g_collapsed_rect, 8);
                    save_overlay_rect(L"collapsed_rect", g_collapsed_rect);
                    return 0;
                }
            }
            show_full_overlay(true);
            return 0;
        }
        break;
    case WM_HOTKEY:
        switch ((int)wparam) {
        case ID_HOTKEY_TOGGLE_OVERLAY:
            handle_overlay_shortcut_key('B', false);
            return 0;
        case ID_HOTKEY_FOCUS_ASK:
            handle_overlay_shortcut_key('T', false);
            return 0;
        case ID_HOTKEY_LISTEN:
            handle_overlay_shortcut_key('L', false);
            return 0;
        case ID_HOTKEY_SCREEN:
            handle_overlay_shortcut_key('S', false);
            return 0;
        case ID_HOTKEY_INTERACTIVE:
            handle_overlay_shortcut_key('I', false);
            return 0;
        case ID_HOTKEY_ANSWER:
            handle_overlay_shortcut_key(VK_RETURN, false);
            return 0;
        case ID_HOTKEY_HISTORY:
            handle_overlay_shortcut_key('H', false);
            return 0;
        case ID_HOTKEY_FILES:
            handle_overlay_shortcut_key('F', false);
            return 0;
        default:
            break;
        }
        break;
    case WM_KEYDOWN: {
        bool ctrl = (GetKeyState(VK_CONTROL) & 0x8000) != 0;
        bool alt = (GetKeyState(VK_MENU) & 0x8000) != 0;
        bool shift = (GetKeyState(VK_SHIFT) & 0x8000) != 0;
        bool edit_focused = GetFocus() == g_ask_edit;
        if (g_interactive_mode && !ctrl && !alt && wparam == VK_TAB) {
            if (focus_next_keyboard_control(shift)) {
                return 0;
            }
        }
        if (g_interactive_mode && !ctrl && !alt && (wparam == VK_RETURN || wparam == VK_SPACE)) {
            if (activate_focused_keyboard_control()) {
                return 0;
            }
        }
        if (g_interactive_mode && !edit_focused && !ctrl && !alt && !shift &&
            (wparam == VK_RETURN || wparam == VK_ESCAPE)) {
            if (handle_overlay_shortcut_key(wparam, true)) {
                return 0;
            }
        }
        if (wparam == VK_RETURN && GetFocus() == g_ask_edit) {
            send_current_question();
            return 0;
        }
        break;
    }
    case WM_DROPFILES: {
        HDROP drop = (HDROP)wparam;
        emit_attach_files_event_from_drop(drop);
        DragFinish(drop);
        return 0;
    }
    case WM_NCHITTEST: {
        if (g_collapsed) return HTCLIENT;
        POINT point = { GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam) };
        if (point_hits_overlay_control(point)) return HTCLIENT;
        if (sent_source_chip_index_at_screen_point(point) >= 0) return HTCLIENT;
        if (!g_interactive_mode && point_hits_clickthrough_move_handle(point)) return HTCAPTION;
        LRESULT resize_hit = hit_test_expanded_resize(point);
        if (resize_hit != HTNOWHERE) return resize_hit;
        if (g_interactive_mode) return HTCAPTION;
        return HTTRANSPARENT;
    }
    case WM_SETCURSOR: {
        if ((HWND)wparam == hwnd) {
            POINT point;
            if (GetCursorPos(&point) && sent_source_chip_index_at_screen_point(point) >= 0) {
                SetCursor(LoadCursorW(NULL, IDC_HAND));
                return TRUE;
            }
            switch (LOWORD(lparam)) {
            case HTLEFT:
            case HTRIGHT:
                SetCursor(LoadCursorW(NULL, IDC_SIZEWE));
                return TRUE;
            case HTTOP:
            case HTBOTTOM:
                SetCursor(LoadCursorW(NULL, IDC_SIZENS));
                return TRUE;
            case HTTOPLEFT:
            case HTBOTTOMRIGHT:
                SetCursor(LoadCursorW(NULL, IDC_SIZENWSE));
                return TRUE;
            case HTTOPRIGHT:
            case HTBOTTOMLEFT:
                SetCursor(LoadCursorW(NULL, IDC_SIZENESW));
                return TRUE;
            case HTCLIENT:
                SetCursor(LoadCursorW(NULL, IDC_ARROW));
                return TRUE;
            default:
                break;
            }
        }
        break;
    }
    case WM_EXITSIZEMOVE:
        if (!g_collapsed) {
            GetWindowRect(hwnd, &g_expanded_rect);
            g_expanded_rect = clamp_expanded_rect_to_focus_area(g_expanded_rect, EXPANDED_SCREEN_MARGIN);
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                g_expanded_rect.left,
                g_expanded_rect.top,
                g_expanded_rect.right - g_expanded_rect.left,
                g_expanded_rect.bottom - g_expanded_rect.top,
                SWP_NOACTIVATE
            );
            save_overlay_rect(L"expanded_rect", g_expanded_rect);
        }
        return 0;
    case WM_GETMINMAXINFO: {
        if (!g_collapsed) {
            MINMAXINFO *mmi = (MINMAXINFO *)lparam;
            RECT current;
            if (!GetWindowRect(hwnd, &current)) current = g_expanded_rect;
            RECT work = work_area_for_rect(current);
            int work_w = work.right - work.left;
            int work_h = work.bottom - work.top;
            mmi->ptMinTrackSize.x = EXPANDED_MIN_WIDTH;
            mmi->ptMinTrackSize.y = EXPANDED_MIN_HEIGHT;
            mmi->ptMaxTrackSize.x = work_w;
            mmi->ptMaxTrackSize.y = work_h;
        }
        return 0;
    }
    case WM_PAINT: {
        ULONGLONG painted_revision = 0;
        if (paint_with_d2d(hwnd, &painted_revision)) {
            ValidateRect(hwnd, NULL);
            emit_pending_render_ack_after_paint(painted_revision);
            return 0;
        }

        PAINTSTRUCT ps;
        HDC hdc = BeginPaint(hwnd, &ps);
        RECT rect;
        GetClientRect(hwnd, &rect);

        if (g_collapsed) {
            HBRUSH bg = CreateSolidBrush(g_light_theme ? RGB(248, 252, 255) : RGB(9, 15, 24));
            FillRect(hdc, &rect, bg);
            DeleteObject(bg);

            draw_bluey_logo(hdc, 10, 8, 25);

            HBRUSH dot = CreateSolidBrush(RGB(62, 220, 128));
            HBRUSH previous_brush = (HBRUSH)SelectObject(hdc, dot);
            Ellipse(hdc, rect.right - 22, 9, rect.right - 12, 19);
            SelectObject(hdc, previous_brush);
            DeleteObject(dot);

            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, g_light_theme ? RGB(8, 22, 32) : RGB(235, 245, 255));
            HFONT font = CreateFontW(13, 0, 0, 0, FW_SEMIBOLD, FALSE, FALSE, FALSE,
                DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
                DEFAULT_PITCH | FF_SWISS, L"Segoe UI");
            SelectObject(hdc, font);
            RECT pill_text = {40, 0, rect.right - 16, rect.bottom};
            DrawTextW(hdc, L"bluey", -1, &pill_text, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
            DeleteObject(font);
            EndPaint(hwnd, &ps);
            return 0;
        }

        HBRUSH bg = CreateSolidBrush(g_light_theme ? RGB(246, 250, 252) : RGB(2, 4, 6));
        FillRect(hdc, &rect, bg);
        DeleteObject(bg);

        SetBkMode(hdc, TRANSPARENT);
        HBRUSH header = CreateSolidBrush(g_light_theme ? RGB(248, 252, 255) : RGB(5, 11, 18));
        HPEN header_pen = CreatePen(PS_SOLID, g_light_theme ? 2 : 1, g_light_theme ? RGB(BLUEY_LIGHT_ACCENT_R, BLUEY_LIGHT_ACCENT_G, BLUEY_LIGHT_ACCENT_B) : RGB(36, 102, 124));
        HGDIOBJ old_brush = SelectObject(hdc, header);
        HGDIOBJ old_pen = SelectObject(hdc, header_pen);
        int header_w = clamp_int((rect.right * 84) / 100, 520, 780);
        if (header_w > rect.right - 28) header_w = rect.right - 28;
        int header_left = (rect.right - header_w) / 2;
        RECT header_rect = {header_left, 8, header_left + header_w, 54};
        RoundRect(hdc, header_rect.left, header_rect.top, header_rect.right, header_rect.bottom, 30, 30);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(header);
        DeleteObject(header_pen);

        int composer_w = clamp_int((rect.right * 72) / 100, 560, 760);
        int composer_left = (rect.right - composer_w) / 2;
        HBRUSH composer = CreateSolidBrush(g_light_theme ? RGB(248, 252, 255) : RGB(5, 11, 18));
        HPEN composer_pen = CreatePen(PS_SOLID, g_light_theme ? 2 : 1, g_light_theme ? RGB(BLUEY_LIGHT_ACCENT_R, BLUEY_LIGHT_ACCENT_G, BLUEY_LIGHT_ACCENT_B) : RGB(38, 98, 138));
        old_brush = SelectObject(hdc, composer);
        old_pen = SelectObject(hdc, composer_pen);
        RECT composer_rect = {composer_left, rect.bottom - 110, composer_left + composer_w, rect.bottom - 8};
        RoundRect(hdc, composer_rect.left, composer_rect.top, composer_rect.right, composer_rect.bottom, 30, 30);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(composer);
        DeleteObject(composer_pen);

        draw_resize_affordance(hdc, rect);
        draw_bluey_logo(hdc, header_left + 14, 14, 28);

        HBRUSH record_dot = CreateSolidBrush(g_recording ? RGB(62, 220, 128) : RGB(40, 92, 62));
        HBRUSH previous_brush = (HBRUSH)SelectObject(hdc, record_dot);
        Ellipse(hdc, header_left + 110, 25, header_left + 119, 34);
        SelectObject(hdc, previous_brush);
        DeleteObject(record_dot);

        SetTextColor(hdc, g_light_theme ? RGB(8, 22, 32) : RGB(230, 240, 245));
        HFONT label_font = CreateFontW(13, 0, 0, 0, FW_BOLD, FALSE, FALSE, FALSE,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
            DEFAULT_PITCH | FF_SWISS, L"Segoe UI");
        HFONT title_font = CreateFontW(20, 0, 0, 0, FW_SEMIBOLD, FALSE, FALSE, FALSE,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
            DEFAULT_PITCH | FF_SWISS, L"Segoe UI");
        HFONT body_font = CreateFontW(16, 0, 0, 0, FW_NORMAL, FALSE, FALSE, FALSE,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
            DEFAULT_PITCH | FF_SWISS, L"Segoe UI");

        SelectObject(hdc, title_font);
        RECT brand_rect = {header_left + 50, 10, header_left + 118, 46};
        DrawTextW(hdc, L"bluey", -1, &brand_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

        if (!g_interactive_mode) {
            RECT handle = clickthrough_move_handle_rect(rect);
            HBRUSH handle_brush = CreateSolidBrush(g_light_theme ? RGB(224, 245, 255) : RGB(5, 36, 52));
            HPEN handle_pen = CreatePen(PS_SOLID, 2, g_light_theme ? RGB(BLUEY_LIGHT_ACCENT_R, BLUEY_LIGHT_ACCENT_G, BLUEY_LIGHT_ACCENT_B) : RGB(66, 190, 255));
            HGDIOBJ previous_handle_brush = SelectObject(hdc, handle_brush);
            HGDIOBJ previous_handle_pen = SelectObject(hdc, handle_pen);
            RoundRect(hdc, handle.left, handle.top, handle.right, handle.bottom, 14, 14);
            SelectObject(hdc, previous_handle_brush);
            SelectObject(hdc, previous_handle_pen);
            DeleteObject(handle_brush);
            DeleteObject(handle_pen);

            HPEN arrow_pen = CreatePen(PS_SOLID, 2, g_light_theme ? RGB(0, 126, 178) : RGB(102, 213, 255));
            HGDIOBJ previous_arrow_pen = SelectObject(hdc, arrow_pen);
            int cx = (handle.left + handle.right) / 2;
            int cy = (handle.top + handle.bottom) / 2;
            MoveToEx(hdc, cx, handle.top + 6, NULL);
            LineTo(hdc, cx, handle.bottom - 6);
            MoveToEx(hdc, handle.left + 6, cy, NULL);
            LineTo(hdc, handle.right - 6, cy);
            SelectObject(hdc, previous_arrow_pen);
            DeleteObject(arrow_pen);
        }

        wchar_t card_label[32];
        current_card_label(card_label, 32);
        SelectObject(hdc, label_font);
        SetTextColor(hdc, wcscmp(card_label, L"MIC") == 0 ? RGB(118, 242, 153) : RGB(112, 238, 248));
        RECT label_rect = {18, 60, rect.right - 18, 80};
        DrawTextW(hdc, card_label, -1, &label_rect, DT_LEFT | DT_TOP | DT_SINGLELINE);

        bool show_title = wcscmp(g_kind, L"transcript") != 0
            && wcscmp(g_kind, L"question") != 0
            && wcscmp(g_kind, L"answer") != 0
            && wcslen(g_title) > 0;
        int body_top = show_title ? 108 : 86;
        if (show_title) {
            SelectObject(hdc, title_font);
            SetTextColor(hdc, g_light_theme ? RGB(8, 22, 32) : RGB(230, 240, 245));
            RECT title_rect = {18, 82, rect.right - 18, 106};
            DrawTextW(hdc, g_title, -1, &title_rect, DT_LEFT | DT_TOP | DT_WORDBREAK);
        }

        int context_reserved = context_chips_visible() ? 34 : 0;
        int sent_reserved = sent_chips_visible_for_current_card() ? 34 : 0;
        RECT body_rect = {18, body_top, rect.right - 18, rect.bottom - 124 - context_reserved - sent_reserved};
        SelectObject(hdc, body_font);
        SetTextColor(hdc, g_light_theme ? RGB(22, 43, 56) : RGB(230, 240, 245));
        AcquireSRWLockShared(&g_body_lock);
        painted_revision = current_presentation_revision();
        DrawTextW(hdc, g_body, -1, &body_rect, DT_LEFT | DT_TOP | DT_WORDBREAK);
        ReleaseSRWLockShared(&g_body_lock);
        draw_sent_chips_gdi(hdc, rect, context_reserved);
        draw_context_chips_gdi(hdc, rect);

        DeleteObject(label_font);
        DeleteObject(title_font);
        DeleteObject(body_font);
        EndPaint(hwnd, &ps);
        emit_pending_render_ack_after_paint(painted_revision);
        return 0;
    }
    case WM_CLOSE:
        collapse_to_pill(hwnd, true);
        return 0;
    case WM_DESTROY:
        stop_meeting_detector();
        hide_meeting_banner(NULL);
        if (g_meeting_banner) {
            DestroyWindow(g_meeting_banner);
            g_meeting_banner = NULL;
        }
        UnregisterHotKey(hwnd, ID_HOTKEY_TOGGLE_OVERLAY);
        UnregisterHotKey(hwnd, ID_HOTKEY_FOCUS_ASK);
        UnregisterHotKey(hwnd, ID_HOTKEY_LISTEN);
        UnregisterHotKey(hwnd, ID_HOTKEY_SCREEN);
        UnregisterHotKey(hwnd, ID_HOTKEY_INTERACTIVE);
        UnregisterHotKey(hwnd, ID_HOTKEY_ANSWER);
        UnregisterHotKey(hwnd, ID_HOTKEY_HISTORY);
        UnregisterHotKey(hwnd, ID_HOTKEY_FILES);
        if (g_edit_brush) {
            DeleteObject(g_edit_brush);
            g_edit_brush = NULL;
        }
        release_d2d_resources();
        PostQuitMessage(0);
        return 0;
    }
    return DefWindowProc(hwnd, msg, wparam, lparam);
}

int WINAPI wWinMain(HINSTANCE instance, HINSTANCE prev, PWSTR cmd, int show) {
    (void)prev;
    (void)cmd;
    (void)show;

    const wchar_t *class_name = L"BlueyOverlayWindow";
    WNDCLASSW wc = {0};
    wc.lpfnWndProc = wnd_proc;
    wc.hInstance = instance;
    wc.lpszClassName = class_name;
    wc.hCursor = LoadCursor(NULL, IDC_ARROW);
    RegisterClassW(&wc);

    load_session_token();
    if (!start_output_writer()) return 1;

    load_overlay_placement();

    /* Stealth: WS_EX_TOOLWINDOW removes the window from Alt+Tab and the
     * taskbar, making it invisible in the task switcher. Combined with
     * WDA_EXCLUDEFROMCAPTURE this ensures the overlay leaves no trace in
     * screen recordings or window lists. */
    DWORD ex_style = WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW;
    int initial_x = 80;
    int initial_y = 80;
    int initial_w = 860;
    int initial_h = 460;
    if (rect_is_valid(g_expanded_rect)) {
        g_expanded_rect = clamp_expanded_rect_to_focus_area(g_expanded_rect, EXPANDED_SCREEN_MARGIN);
        initial_x = g_expanded_rect.left;
        initial_y = g_expanded_rect.top;
        initial_w = g_expanded_rect.right - g_expanded_rect.left;
        initial_h = g_expanded_rect.bottom - g_expanded_rect.top;
    }
    g_hwnd = CreateWindowExW(
        ex_style,
        class_name,
        L"Bluey Overlay",
        WS_POPUP | WS_THICKFRAME,
        initial_x,
        initial_y,
        initial_w,
        initial_h,
        NULL,
        NULL,
        instance,
        NULL
    );
    if (!g_hwnd) {
        stop_output_writer();
        return 1;
    }

    create_controls(g_hwnd);
    create_meeting_banner(instance);
    DragAcceptFiles(g_hwnd, TRUE);
    set_window_opacity(g_opacity);
    apply_capture_exclusion(g_hwnd);
    ShowWindow(g_hwnd, SW_SHOWNOACTIVATE);
    if (!emit_ready()) {
        DestroyWindow(g_hwnd);
        stop_output_writer();
        return 1;
    }
    int hotkeys_registered = 0;
    char hotkey_failures[192] = "";
    hotkeys_registered += register_bluey_hotkey(ID_HOTKEY_TOGGLE_OVERLAY, 'B', "B", hotkey_failures, sizeof(hotkey_failures)) ? 1 : 0;
    hotkeys_registered += register_bluey_hotkey(ID_HOTKEY_FOCUS_ASK, 'T', "T", hotkey_failures, sizeof(hotkey_failures)) ? 1 : 0;
    hotkeys_registered += register_bluey_hotkey(ID_HOTKEY_LISTEN, 'L', "L", hotkey_failures, sizeof(hotkey_failures)) ? 1 : 0;
    hotkeys_registered += register_bluey_hotkey(ID_HOTKEY_SCREEN, 'S', "S", hotkey_failures, sizeof(hotkey_failures)) ? 1 : 0;
    hotkeys_registered += register_bluey_hotkey(ID_HOTKEY_INTERACTIVE, 'I', "I", hotkey_failures, sizeof(hotkey_failures)) ? 1 : 0;
    hotkeys_registered += register_bluey_hotkey(ID_HOTKEY_ANSWER, VK_RETURN, "Enter", hotkey_failures, sizeof(hotkey_failures)) ? 1 : 0;
    hotkeys_registered += register_bluey_hotkey(ID_HOTKEY_HISTORY, 'H', "H", hotkey_failures, sizeof(hotkey_failures)) ? 1 : 0;
    hotkeys_registered += register_bluey_hotkey(ID_HOTKEY_FILES, 'F', "F", hotkey_failures, sizeof(hotkey_failures)) ? 1 : 0;
    char hotkey_detail[256];
    if (hotkey_failures[0] != '\0') {
        snprintf(hotkey_detail, sizeof(hotkey_detail), "modifier=ctrl_alt registered=%d failures=%s", hotkeys_registered, hotkey_failures);
        emit_lifecycle_event("global_shortcuts", "partial", hotkey_detail);
    } else {
        snprintf(hotkey_detail, sizeof(hotkey_detail), "modifier=ctrl_alt registered=%d", hotkeys_registered);
        emit_lifecycle_event("global_shortcuts", "ready", hotkey_detail);
    }
    HANDLE stdin_worker = CreateThread(NULL, 0, stdin_thread, NULL, 0, NULL);
    if (stdin_worker) CloseHandle(stdin_worker);
    MSG msg;
    while (GetMessage(&msg, NULL, 0, 0)) {
        TranslateMessage(&msg);
        DispatchMessage(&msg);
    }
    stop_meeting_detector();
    stop_output_writer();
    return 0;
}
