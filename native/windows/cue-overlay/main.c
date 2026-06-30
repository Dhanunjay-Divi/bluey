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

#ifdef DrawText
#undef DrawText
#endif

#include <initguid.h>
#include <d2d1.h>
#include <dwrite.h>
#include <stdio.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>
#include <wctype.h>
#include "json_type_extract.h"

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
static HWND g_transcript_clear_button;
static HWND g_paste_answer_button;
static HWND g_help_button;
static HWND g_session_button;
static HWND g_page_button;
static HWND g_attach_button;
static HWND g_recap_button;
static HWND g_note_button;
static HWND g_theme_button;
static HWND g_close_button;
static HWND g_tooltip;
static wchar_t g_title[256] = L"bluey";
static wchar_t g_body[2048] = L"Waiting for meeting intelligence...";
static wchar_t g_kind[64] = L"system";
static wchar_t g_source[256] = L"";
static wchar_t g_card_id[80] = L"";
static bool g_visible = true;
static bool g_collapsed = false;
static bool g_recording = false;
static DWORD g_last_record_toggle_ms = 0;
static DWORD g_record_restart_after_ms = 0;
static int g_auto_send_mode = 2;
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
static WNDPROC g_ask_edit_proc = NULL;

static void send_current_question(void);
static void show_full_overlay(bool emit_event);
static void update_paste_answer_button(void);

#define MAX_CONTEXT_CHIPS 16
typedef struct OverlayContextChip {
    wchar_t title[260];
    wchar_t kind[64];
    wchar_t path[520];
} OverlayContextChip;

static OverlayContextChip g_context_chips[MAX_CONTEXT_CHIPS];
static int g_context_chip_count = 0;
static bool g_show_context_chips = false;
static OverlayContextChip g_sent_chips[MAX_CONTEXT_CHIPS];
static int g_sent_chip_count = 0;
static void consume_sent_context_chips(void);

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
#define COLLAPSED_DRAG_THRESHOLD 4

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
    L"Supported: PDF, DOC/DOCX, Excel/ODS, CSV/TSV, text, Markdown, code/data files, and PNG/JPEG/WebP/GIF/HEIC/BMP/TIFF images.";

static void load_session_token(void) {
    DWORD n = GetEnvironmentVariableA(
        "BLUEY_OVERLAY_SESSION_TOKEN",
        g_session_token,
        (DWORD)sizeof(g_session_token));
    if (n == 0 || n >= sizeof(g_session_token)) {
        g_session_token[0] = '\0';
    }
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
    add_control_tooltip(g_auto_send_combo, L"Choose which audio source should auto-send when listening stops.");
    add_control_tooltip(g_transcript_clear_button, L"Clear current captions from the next answer");
    add_control_tooltip(g_help_button, L"Show Bluey help");
    add_control_tooltip(g_session_button, L"Open conversation history");
    add_control_tooltip(g_page_button, L"Capture the screen as context");
    add_control_tooltip(g_attach_button, L"Attach documents or images");
    add_control_tooltip(g_recap_button, L"Create a recap for this recording");
    add_control_tooltip(g_note_button, L"Set how Bluey should answer");
    add_control_tooltip(g_theme_button, L"Toggle light or dark theme");
    add_control_tooltip(g_close_button, L"Turn Bluey off");
}

// Print `,"token":"..."` if a token is set, else nothing.
// Caller must have already opened the JSON object and emitted >= 1 field.
static void emit_token_field(void) {
    if (g_session_token[0] != '\0') {
        printf(",\"token\":\"%s\"", g_session_token);
    }
}

static void emit_ready(void) {
    printf("{\"type\":\"ready\",\"platform\":\"windows\",\"capture_excluded\":true");
    emit_token_field();
    printf("}\n");
    fflush(stdout);
}

static void json_print_escaped(const char *text) {
    for (const unsigned char *p = (const unsigned char *)text; *p; ++p) {
        switch (*p) {
        case '\\': fputs("\\\\", stdout); break;
        case '"': fputs("\\\"", stdout); break;
        case '\n': fputs("\\n", stdout); break;
        case '\r': fputs("\\r", stdout); break;
        case '\t': fputs("\\t", stdout); break;
        default:
            if (*p < 0x20) {
                printf("\\u%04x", *p);
            } else {
                fputc(*p, stdout);
            }
        }
    }
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

static void normalize_answer_display_text(wchar_t *text, size_t capacity) {
    if (!text || capacity == 0) return;

    wchar_t out[2048];
    size_t write = 0;
    bool at_line_start = true;
    bool in_fence = false;
    size_t limit = capacity < 2048 ? capacity : 2048;

    for (size_t read = 0; text[read] != L'\0' && write + 1 < limit; read++) {
        wchar_t ch = text[read];

        if (at_line_start && ch == L'`' && text[read + 1] == L'`' && text[read + 2] == L'`') {
            in_fence = !in_fence;
            while (text[read] != L'\0' && text[read] != L'\n') {
                read++;
            }
            if (text[read] == L'\n' && write + 1 < limit) {
                out[write++] = L'\n';
            }
            at_line_start = true;
            continue;
        }

        if (in_fence) {
            out[write++] = ch;
            if (ch == L'\n') {
                at_line_start = true;
            } else if (ch != L'\r') {
                at_line_start = false;
            }
            continue;
        }

        if (at_line_start) {
            size_t cursor = read;
            while (text[cursor] == L' ' || text[cursor] == L'\t') {
                cursor++;
            }
            if (text[cursor] == L'#') {
                while (text[cursor] == L'#') {
                    cursor++;
                }
                if (text[cursor] == L' ') {
                    cursor++;
                }
                read = cursor;
                ch = text[read];
                if (ch == L'\0') break;
            }
        }

        if (ch == L'`') {
            continue;
        }
        if ((ch == L'*' && text[read + 1] == L'*')
            || (ch == L'_' && text[read + 1] == L'_')) {
            read++;
            continue;
        }

        out[write++] = ch;
        if (ch == L'\n') {
            at_line_start = true;
        } else if (ch != L'\r') {
            at_line_start = false;
        }
    }

    out[write] = L'\0';
    wcscpy_s(text, capacity, out);
}

static void emit_simple_event(const char *type) {
    printf("{\"type\":\"%s\"", type);
    emit_token_field();
    printf("}\n");
    fflush(stdout);
}

static void emit_lifecycle_event(const char *stage, const char *status, const char *detail) {
    printf("{\"type\":\"lifecycle\",\"stage\":\"");
    json_print_escaped(stage ? stage : "");
    printf("\",\"status\":\"");
    json_print_escaped(status ? status : "ok");
    printf("\"");
    if (detail && detail[0] != '\0') {
        printf(",\"detail\":\"");
        json_print_escaped(detail);
        printf("\"");
    }
    emit_token_field();
    printf("}\n");
    fflush(stdout);
}

static void emit_ask_event(const wchar_t *question) {
    char *utf8 = wide_to_utf8_alloc(question);
    if (!utf8) return;

    fputs("{\"type\":\"ask_requested\",\"question\":\"", stdout);
    json_print_escaped(utf8);
    free(utf8);
    // Close `question`, emit fixed fields, append `,"token":"..."` if set,
    // then close the JSON object.
    fputs("\",\"provider\":\"auto\",\"model\":\"\",\"mode\":\"General\"", stdout);
    emit_token_field();
    fputs("}\n", stdout);
    fflush(stdout);
}

static void emit_paste_text_event(const wchar_t *text) {
    char *utf8 = wide_to_utf8_alloc(text);
    if (!utf8) return;

    fputs("{\"type\":\"paste_text_requested\",\"text\":\"", stdout);
    json_print_escaped(utf8);
    free(utf8);
    fputc('"', stdout);
    emit_token_field();
    fputs("}\n", stdout);
    fflush(stdout);
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
        L"pdf", L"doc", L"docx", L"rtf", L"xls", L"xlsx", L"xlsm", L"xlsb", L"ods",
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
    if (skipped == total) {
        swprintf_s(g_body, 2048, L"Bluey cannot use that file type as context yet. %ls", g_supported_drop_formats);
    } else {
        swprintf_s(
            g_body,
            2048,
            L"Bluey skipped %u file%ls that are not readable context. %ls",
            skipped,
            skipped == 1 ? L"" : L"s",
            g_supported_drop_formats);
    }
    wcscpy_s(g_kind, 64, L"warning");
    wcscpy_s(g_source, 256, L"");
    wcscpy_s(g_card_id, 80, L"");
    if (g_visible && !g_collapsed) show_full_overlay(false);
    InvalidateRect(g_hwnd, NULL, TRUE);
}

static void show_supported_drop_loading(UINT count) {
    wcscpy_s(g_title, 256, L"Docs loading");
    wcscpy_s(
        g_body,
        2048,
        count == 1 ? L"Indexing dropped document..." : L"Indexing dropped documents...");
    wcscpy_s(g_kind, 64, L"context");
    wcscpy_s(g_source, 256, L"");
    wcscpy_s(g_card_id, 80, L"");
    if (g_visible && !g_collapsed) show_full_overlay(false);
    InvalidateRect(g_hwnd, NULL, TRUE);
}

static void emit_attach_files_event_from_drop(HDROP drop) {
    UINT count = DragQueryFileW(drop, 0xFFFFFFFFu, NULL, 0);
    if (count == 0) return;
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
    if (supported_count == 0) return;
    if (skipped_count == 0) show_supported_drop_loading(supported_count);
    g_show_context_chips = false;

    fputs("{\"type\":\"attach_files_requested\",\"paths\":[", stdout);
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

        if (emitted) fputc(',', stdout);
        fputc('"', stdout);
        json_print_escaped(utf8);
        fputc('"', stdout);
        free(utf8);
        emitted = true;
    }
    fputc(']', stdout);
    emit_token_field();
    fputs("}\n", stdout);
    fflush(stdout);
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
        g_ask_edit, g_send_button, g_record_button, g_auto_send_combo, g_paste_answer_button, g_help_button, g_session_button,
        g_page_button, g_attach_button, g_recap_button, g_note_button, g_theme_button, g_close_button
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

static void set_utf8_text(wchar_t *dest, size_t dest_len, const char *text) {
    MultiByteToWideChar(CP_UTF8, 0, text, -1, dest, (int)dest_len);
}

static void naive_extract_json_string(const char *line, const char *key, wchar_t *dest, size_t dest_len) {
    char pattern[64];
    snprintf(pattern, sizeof(pattern), "\"%s\":\"", key);
    const char *start = strstr(line, pattern);
    if (!start) return;
    start += strlen(pattern);
    const char *end = strchr(start, '"');
    if (!end) return;

    char buffer[2048];
    size_t len = (size_t)(end - start);
    if (len >= sizeof(buffer)) len = sizeof(buffer) - 1;
    memcpy(buffer, start, len);
    buffer[len] = '\0';
    set_utf8_text(dest, dest_len, buffer);
}

static bool naive_extract_json_number(const char *line, const char *key, double *dest) {
    char pattern[64];
    snprintf(pattern, sizeof(pattern), "\"%s\":", key);
    const char *start = strstr(line, pattern);
    if (!start) return false;
    start += strlen(pattern);
    while (*start == ' ') start++;
    char *end = NULL;
    double value = strtod(start, &end);
    if (end == start) return false;
    *dest = value;
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

    int chip_w = (composer_w - 34) / 2;
    MoveWindow(g_recap_button, composer_left + 12, chip_y, chip_w, 30, TRUE);
    MoveWindow(g_page_button, composer_left + 22 + chip_w, chip_y, chip_w, 30, TRUE);

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
    MoveWindow(g_attach_button, header_right - margin - 62 - (small_w + 8) * 3, header_y + 4, small_w, header_h, TRUE);
    MoveWindow(g_session_button, header_right - margin - 62 - (small_w + 8) * 4, header_y + 4, small_w, header_h, TRUE);
    MoveWindow(g_help_button, header_right - margin - 62 - (small_w + 8) * 5, header_y + 4, small_w, header_h, TRUE);

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
        g_transcript_clear_button,
        g_paste_answer_button,
        g_help_button,
        g_session_button,
        g_page_button,
        g_attach_button,
        g_recap_button,
        g_note_button,
        g_theme_button,
        g_close_button,
    };
    for (size_t i = 0; i < sizeof(controls) / sizeof(controls[0]); i++) {
        if (point_hits_visible_child(controls[i], point, 8)) return true;
    }
    return false;
}

static bool point_hits_brand_move_handle(POINT point) {
    if (!g_hwnd || g_collapsed) return false;
    RECT rect;
    if (!GetClientRect(g_hwnd, &rect)) return false;
    POINT local = point;
    if (!ScreenToClient(g_hwnd, &local)) return false;

    int header_w = clamp_int((rect.right * 84) / 100, 520, 780);
    if (header_w > rect.right - 28) header_w = rect.right - 28;
    int header_left = (rect.right - header_w) / 2;
    RECT handle = {header_left + 8, 8, header_left + 132, 54};
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
    SendMessageW(g_auto_send_combo, CB_ADDSTRING, 0, (LPARAM)L"Auto-send when mic stops");
    SendMessageW(g_auto_send_combo, CB_ADDSTRING, 0, (LPARAM)L"Auto-send when system stops");
    SendMessageW(g_auto_send_combo, CB_ADDSTRING, 0, (LPARAM)L"Auto-send when mic or system stops");
    SendMessageW(g_auto_send_combo, CB_SETDROPPEDWIDTH, 280, 0);
    SendMessageW(g_auto_send_combo, CB_SETCURSEL, g_auto_send_mode, 0);
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
    g_close_button = CreateWindowW(L"BUTTON", L"Quit", WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        0, 0, 60, 30, hwnd, (HMENU)ID_CLOSE_BUTTON, GetModuleHandleW(NULL), NULL);

    HWND controls[] = {
        g_ask_edit, g_send_button, g_record_button, g_auto_send_combo, g_transcript_clear_button, g_paste_answer_button, g_help_button, g_session_button,
        g_page_button, g_attach_button, g_recap_button, g_note_button, g_theme_button, g_close_button
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
    g_show_context_chips = false;
    InvalidateRect(g_hwnd, NULL, TRUE);
}

static void clear_local_transcript_context(void) {
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
    if (!is_meaningful_transcript_question(out)) return false;
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
    if (!is_meaningful_transcript_question(out)) return false;
    return true;
}

static bool is_live_transcript_answer_prompt(const wchar_t *question) {
    if (!question) return false;
    return wcsncmp(question, L"Answer the latest ", 18) == 0
        && wcsstr(question, L"live captions from the current session transcript") != NULL;
}

static void send_current_question(void) {
    static const wchar_t *fallback = L"Answer the latest clear question from the current transcript, screen context, and attached files. If there is no clear question yet, summarize what Bluey needs next.";
    int length = GetWindowTextLengthW(g_ask_edit);
    bool used_fallback = length <= 0;
    wchar_t transcript_question[1024] = L"";
    bool used_short_transcript = used_fallback && live_transcript_visible_question(transcript_question, 260);
    bool used_transcript_text = used_short_transcript
        || (used_fallback && live_transcript_question_text(transcript_question, 1024));
    bool transcript_context = has_transcript_context();
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
    const wchar_t *fallback_question = used_transcript_text ? transcript_question : fallback;
    char detail[192];
    snprintf(
        detail,
        sizeof(detail),
        "typed_chars=%d fallback=%s transcript_context=%s generic_live_prompt=%s context_ids=%d",
        length > 0 ? length : 0,
        used_fallback ? "true" : "false",
        transcript_context ? "true" : "false",
        is_live_transcript_answer_prompt(fallback_question) ? "true" : "false",
        g_context_chip_count
    );
    emit_lifecycle_event("ask_answer_sent", "ok", detail);
    if (length <= 0) {
        emit_ask_event(fallback_question);
    } else {
        wchar_t *question = (wchar_t *)calloc((size_t)length + 1, sizeof(wchar_t));
        if (!question) return;
        GetWindowTextW(g_ask_edit, question, length + 1);
        emit_ask_event(question);
        free(question);
    }
    SetWindowTextW(g_ask_edit, L"");
    clear_local_transcript_context();
    consume_sent_context_chips();
    InvalidateRect(g_hwnd, NULL, TRUE);
    SetFocus(g_ask_edit);
}

static void safe_extract_json_to_wide(const char *line, size_t line_len, const char *key, wchar_t *dest, size_t dest_wchars) {
    char buf[4096];
    if (json_extract_string(line, line_len, key, buf, sizeof(buf))) {
        set_utf8_text(dest, dest_wchars, buf);
    }
}

static bool safe_extract_json_number(const char *line, size_t line_len, const char *key, double *dest) {
    char buf[64];
    if (json_extract_string(line, line_len, key, buf, sizeof(buf))) {
        char *end = NULL;
        double value = strtod(buf, &end);
        if (end != buf) {
            *dest = value;
            return true;
        }
    }
    /* Fallback: try the naive number extractor for non-string numbers */
    return naive_extract_json_number(line, key, dest);
}

static void set_chips_from_json_key(
    const char *line,
    size_t line_len,
    const char *array_key,
    OverlayContextChip *chips,
    int *chip_count
) {
    *chip_count = 0;
    const char *end = line + line_len;
    char needle[96];
    snprintf(needle, sizeof(needle), "\"%s\"", array_key);
    const char *items = strstr(line, needle);
    if (!items) return;

    const char *p = strchr(items, '[');
    if (!p || p >= end) return;
    p++;

    while (p < end && *chip_count < MAX_CONTEXT_CHIPS) {
        p = json_skip_ws(p, end);
        if (p >= end || *p == ']') break;
        if (*p == ',') {
            p++;
            continue;
        }
        if (*p != '{') break;

        const char *object_start = p;
        const char *object_end = json_skip_value(p, end);
        if (!object_end || object_end <= object_start) break;
        size_t object_len = (size_t)(object_end - object_start);

        char title[JSON_MAX_FIELD_LEN];
        char kind[128];
        char path[JSON_MAX_FIELD_LEN];
        bool has_title = json_extract_string(object_start, object_len, "title", title, sizeof(title));
        bool has_kind = json_extract_string(object_start, object_len, "kind", kind, sizeof(kind));
        bool has_path = json_extract_string(object_start, object_len, "path", path, sizeof(path));

        OverlayContextChip *chip = &chips[*chip_count];
        set_utf8_text(chip->title, sizeof(chip->title) / sizeof(chip->title[0]), has_title ? title : "Attached file");
        set_utf8_text(chip->kind, sizeof(chip->kind) / sizeof(chip->kind[0]), has_kind ? kind : "document");
        set_utf8_text(chip->path, sizeof(chip->path) / sizeof(chip->path[0]), has_path ? path : "");
        (*chip_count)++;
        p = object_end;
    }
}

static void set_context_chips_from_json(const char *line, size_t line_len) {
    set_chips_from_json_key(line, line_len, "items", g_context_chips, &g_context_chip_count);
    if (g_context_chip_count <= 0) {
        g_show_context_chips = false;
    }
}

static void set_sent_chips_from_json(const char *line, size_t line_len) {
    set_chips_from_json_key(line, line_len, "attachments", g_sent_chips, &g_sent_chip_count);
}

static DWORD WINAPI stdin_thread(LPVOID unused) {
    (void)unused;
    char line[8192];
    while (fgets(line, sizeof(line), stdin)) {
        size_t line_len = strlen(line);

        /* Item 6: reject overlong lines */
        if (json_line_too_long(line_len)) {
            continue;
        }

        /* Item 5: safely extract the top-level "type" field */
        char msg_type[128];
        if (!json_extract_type(line, line_len, msg_type, sizeof(msg_type))) {
            continue; /* no valid type field - drop */
        }

        if (strcmp(msg_type, "show") == 0) {
            show_full_overlay(true);
        } else if (strcmp(msg_type, "hide") == 0) {
            collapse_to_pill(g_hwnd, true);
        } else if (strcmp(msg_type, "toggle") == 0) {
            if (g_visible) collapse_to_pill(g_hwnd, true);
            else show_full_overlay(true);
        } else if (strcmp(msg_type, "clear") == 0) {
            wcscpy_s(g_title, 256, L"bluey");
            wcscpy_s(g_body, 2048, L"");
            wcscpy_s(g_kind, 64, L"system");
            wcscpy_s(g_source, 256, L"");
            wcscpy_s(g_card_id, 80, L"");
            g_sent_chip_count = 0;
            update_paste_answer_button();
            InvalidateRect(g_hwnd, NULL, TRUE);
        } else if (strcmp(msg_type, "boot") == 0) {
            wcscpy_s(g_title, 256, L"bluey online");
            wcscpy_s(g_body, 2048, L"> overlay link established\n> session memory loaded\n> context controls armed\n> ready");
            wcscpy_s(g_kind, 64, L"system");
            wcscpy_s(g_source, 256, L"");
            wcscpy_s(g_card_id, 80, L"");
            safe_extract_json_to_wide(line, line_len, "title", g_title, 256);
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
            safe_extract_json_to_wide(line, line_len, "title", g_title, 256);
            safe_extract_json_to_wide(line, line_len, "body", g_body, 2048);
            safe_extract_json_to_wide(line, line_len, "kind", g_kind, 64);
            if (_wcsicmp(g_kind, L"answer") == 0) {
                normalize_answer_display_text(g_body, 2048);
            }
            safe_extract_json_to_wide(line, line_len, "source", g_source, 256);
            safe_extract_json_to_wide(line, line_len, "id", g_card_id, 80);
            set_sent_chips_from_json(line, line_len);
            if (g_visible && !g_collapsed) {
                show_full_overlay(false);
            }
            update_paste_answer_button();
            InvalidateRect(g_hwnd, NULL, TRUE);
        } else if (strcmp(msg_type, "update_card") == 0) {
            wchar_t id[80] = L"";
            safe_extract_json_to_wide(line, line_len, "id", id, 80);
            if (wcslen(g_card_id) == 0 || wcscmp(id, g_card_id) == 0) {
                safe_extract_json_to_wide(line, line_len, "body", g_body, 2048);
                if (_wcsicmp(g_kind, L"answer") == 0) {
                    normalize_answer_display_text(g_body, 2048);
                }
                update_paste_answer_button();
                InvalidateRect(g_hwnd, NULL, TRUE);
            }
        } else if (strcmp(msg_type, "shutdown") == 0) {
            PostMessage(g_hwnd, WM_CLOSE, 0, 0);
            break;
        } else if (strcmp(msg_type, "transcript_partial") == 0) {
            safe_extract_json_to_wide(line, line_len, "text", g_transcript_partial, 1024);
            safe_extract_json_to_wide(line, line_len, "source", g_transcript_source, 64);
            update_transcript_clear_button();
            update_paste_answer_button();
            InvalidateRect(g_hwnd, NULL, TRUE);
        } else if (strcmp(msg_type, "transcript_final") == 0) {
            safe_extract_json_to_wide(line, line_len, "text", g_transcript_final, 1024);
            safe_extract_json_to_wide(line, line_len, "source", g_transcript_source, 64);
            g_transcript_partial[0] = L'\0';
            update_transcript_clear_button();
            update_paste_answer_button();
            InvalidateRect(g_hwnd, NULL, TRUE);
        } else if (strcmp(msg_type, "session_switched") == 0) {
            safe_extract_json_to_wide(line, line_len, "title", g_session_banner, 256);
            if (wcslen(g_session_banner) == 0) wcscpy_s(g_session_banner, 256, L"New session");
            g_session_banner_tick = GetTickCount64();
            g_transcript_partial[0] = L'\0';
            g_transcript_final[0] = L'\0';
            g_transcript_source[0] = L'\0';
            update_transcript_clear_button();
            update_paste_answer_button();
            InvalidateRect(g_hwnd, NULL, TRUE);
        } else if (strcmp(msg_type, "ping") == 0) {
            emit_simple_event("pong");
            fflush(stdout);
        }
    }
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
    const wchar_t *prefix = context_kind_is_image(chip->kind) ? L"IMG" : L"DOC";
    swprintf(dest, dest_len, L"%ls %ls", prefix, title);
    dest[dest_len - 1] = L'\0';
}

static float context_chip_width(const wchar_t *label) {
    size_t len = wcslen(label);
    float width = 42.0f + (float)len * 6.3f;
    if (width < 96.0f) width = 96.0f;
    if (width > 164.0f) width = 164.0f;
    return width;
}

static void draw_context_chips_d2d(RECT rect) {
    if (!g_show_context_chips || g_context_chip_count <= 0) return;

    int composer_w = clamp_int((rect.right * 72) / 100, 560, 760);
    int composer_left = (rect.right - composer_w) / 2;
    float x = (float)composer_left + 8.0f;
    float y = (float)rect.bottom - 145.0f;
    float max_right = (float)(composer_left + composer_w - 8);

    for (int i = 0; i < g_context_chip_count && x < max_right - 50.0f; i++) {
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
    if (g_sent_chip_count <= 0 || _wcsicmp(g_kind, L"question") != 0) return;

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
    if (!g_show_context_chips || g_context_chip_count <= 0) return;

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
    if (g_sent_chip_count <= 0 || _wcsicmp(g_kind, L"question") != 0) return;

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

static bool paint_with_d2d(HWND hwnd) {
    if (!ensure_d2d_target(hwnd)) return false;

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
        float context_reserved = (g_show_context_chips && g_context_chip_count > 0) ? 34.0f : 0.0f;
        float sent_reserved = (_wcsicmp(g_kind, L"question") == 0 && g_sent_chip_count > 0) ? 34.0f : 0.0f;
        d2d_text(g_body, g_fmt_body, d2d_rectf(18.0f, (float)body_top, (float)rect.right - 18.0f, (float)rect.bottom - 124.0f - context_reserved - sent_reserved), g_light_theme ? 22 : 230, g_light_theme ? 43 : 240, g_light_theme ? 56 : 245, 1.0f);
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
        if (id == ID_RECORD_BUTTON) {
            DWORD now_ms = GetTickCount();
            if ((DWORD)(now_ms - g_last_record_toggle_ms) < 300) {
                return 0;
            }
            if (!g_recording && g_record_restart_after_ms != 0 && (LONG)(now_ms - g_record_restart_after_ms) < 0) {
                return 0;
            }
            g_last_record_toggle_ms = now_ms;
            bool was_recording = g_recording;
            g_recording = !g_recording;
            g_record_restart_after_ms = was_recording ? now_ms + 1200 : 0;
            update_record_button();
            InvalidateRect(hwnd, NULL, TRUE);
            emit_simple_event(g_recording ? "recording_start_requested" : "recording_stop_requested");
            if (was_recording && has_auto_send_context()) {
                send_current_question();
            }
            return 0;
        }
        if (id == ID_TRANSCRIPT_CLEAR_BUTTON) {
            clear_local_transcript_context();
            InvalidateRect(hwnd, NULL, TRUE);
            emit_simple_event("transcript_clear_requested");
            return 0;
        }
        if (id == ID_PASTE_ANSWER_BUTTON) {
            return 0;
        }
        if (id == ID_HELP_BUTTON) {
            overlay_message_box(
                L"Green dot: Bluey is connected.\nHelp: show this guide.\nSession: continue or start clean.\nAttach: add files or show attached docs.\nTheme: switch black/white background while keeping Bluey borders.\nStyle: answer rules.\nAnalyse Screen: search/read the active browser page or available screen context and generate an answer.\nRecap: summarize the active session from the bottom bar.\nQuit: stop Bluey completely. Hide/collapse behavior becomes a small Bluey button.\nMic: start/stop audio capture.\nMic dot: dim off, bright green recording.\nAnswer: ask Bluey.\nHold any blank Bluey space to move it. Controls stay clickable.",
                L"Bluey controls",
                MB_OK | MB_ICONINFORMATION
            );
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
                emit_simple_event("attach_requested");
            }
            if (answer == IDNO) {
                g_show_context_chips = true;
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
            if (answer == IDOK) emit_simple_event("analyze_screen_requested");
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
    case WM_KEYDOWN:
        if (wparam == VK_RETURN && GetFocus() == g_ask_edit) {
            send_current_question();
            return 0;
        }
        break;
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
        LRESULT resize_hit = hit_test_expanded_resize(point);
        if (resize_hit != HTNOWHERE) return resize_hit;
        if (point_hits_brand_move_handle(point)) return HTCAPTION;
        return HTTRANSPARENT;
    }
    case WM_SETCURSOR: {
        if ((HWND)wparam == hwnd) {
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
        if (paint_with_d2d(hwnd)) {
            ValidateRect(hwnd, NULL);
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

        int context_reserved = (g_show_context_chips && g_context_chip_count > 0) ? 34 : 0;
        int sent_reserved = (_wcsicmp(g_kind, L"question") == 0 && g_sent_chip_count > 0) ? 34 : 0;
        RECT body_rect = {18, body_top, rect.right - 18, rect.bottom - 124 - context_reserved - sent_reserved};
        SelectObject(hdc, body_font);
        SetTextColor(hdc, g_light_theme ? RGB(22, 43, 56) : RGB(230, 240, 245));
        DrawTextW(hdc, g_body, -1, &body_rect, DT_LEFT | DT_TOP | DT_WORDBREAK);
        draw_sent_chips_gdi(hdc, rect, context_reserved);
        draw_context_chips_gdi(hdc, rect);

        DeleteObject(label_font);
        DeleteObject(title_font);
        DeleteObject(body_font);
        EndPaint(hwnd, &ps);
        return 0;
    }
    case WM_CLOSE:
        collapse_to_pill(hwnd, true);
        return 0;
    case WM_DESTROY:
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

    create_controls(g_hwnd);
    DragAcceptFiles(g_hwnd, TRUE);
    set_window_opacity(g_opacity);
    apply_capture_exclusion(g_hwnd);
    ShowWindow(g_hwnd, SW_SHOWNOACTIVATE);
    load_session_token();
    emit_ready();
    CreateThread(NULL, 0, stdin_thread, NULL, 0, NULL);

    MSG msg;
    while (GetMessage(&msg, NULL, 0, 0)) {
        TranslateMessage(&msg);
        DispatchMessage(&msg);
    }
    return 0;
}
