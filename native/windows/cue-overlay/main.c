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

#define COBJMACROS
#include <windows.h>
#include <windowsx.h>

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
static HWND g_help_button;
static HWND g_session_button;
static HWND g_page_button;
static HWND g_attach_button;
static HWND g_recap_button;
static HWND g_note_button;
static HWND g_theme_button;
static HWND g_close_button;
static wchar_t g_title[256] = L"Bluey";
static wchar_t g_body[2048] = L"Waiting for meeting intelligence...";
static wchar_t g_kind[64] = L"system";
static wchar_t g_source[256] = L"";
static wchar_t g_card_id[80] = L"";
static bool g_visible = true;
static bool g_collapsed = false;
static bool g_recording = false;
static bool g_light_theme = false;
static double g_opacity = 0.92;
static RECT g_expanded_rect = {0, 0, 0, 0};
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

#define ID_ASK_EDIT 1001
#define ID_SEND_BUTTON 1002
#define ID_RECORD_BUTTON 1003
#define ID_HELP_BUTTON 1004
#define ID_SESSION_BUTTON 1005
#define ID_PAGE_BUTTON 1007
#define ID_ATTACH_BUTTON 1008
#define ID_NOTE_BUTTON 1009
#define ID_CLOSE_BUTTON 1010
#define ID_RECAP_BUTTON 1011
#define ID_THEME_BUTTON 1012

static void apply_capture_exclusion(HWND hwnd) {
    if (!SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)) {
        SetWindowDisplayAffinity(hwnd, WDA_MONITOR);
    }
}

static void emit_ready(void) {
    printf("{\"type\":\"ready\",\"platform\":\"windows\",\"capture_excluded\":true}\n");
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

static void emit_simple_event(const char *type) {
    printf("{\"type\":\"%s\"}\n", type);
    fflush(stdout);
}

static void emit_ask_event(const wchar_t *question) {
    char utf8[4096];
    int written = WideCharToMultiByte(CP_UTF8, 0, question, -1, utf8, sizeof(utf8), NULL, NULL);
    if (written <= 0) return;

    fputs("{\"type\":\"ask_requested\",\"question\":\"", stdout);
    json_print_escaped(utf8);
    fputs("\",\"provider\":\"auto\",\"model\":\"\",\"mode\":\"General\"}\n", stdout);
    fflush(stdout);
}

static void update_record_button(void) {
    if (g_record_button) {
        SetWindowTextW(g_record_button, g_recording ? L"Stop" : L"Mic");
        InvalidateRect(g_record_button, NULL, TRUE);
    }
}

static void draw_dark_button(const DRAWITEMSTRUCT *item) {
    wchar_t text[96];
    GetWindowTextW(item->hwndItem, text, 96);

    bool pressed = (item->itemState & ODS_SELECTED) != 0;
    HBRUSH bg = CreateSolidBrush(
        g_light_theme
            ? (pressed ? RGB(205, 232, 246) : RGB(244, 249, 252))
            : (pressed ? RGB(34, 47, 58) : RGB(10, 15, 22))
    );
    HPEN border = CreatePen(PS_SOLID, 1, g_light_theme ? RGB(82, 172, 205) : RGB(45, 91, 112));
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
        g_ask_edit, g_send_button, g_record_button, g_help_button, g_session_button,
        g_page_button, g_attach_button, g_recap_button, g_note_button, g_theme_button, g_close_button
    };
    for (int i = 0; i < (int)(sizeof(controls) / sizeof(controls[0])); i++) {
        if (controls[i]) ShowWindow(controls[i], state);
    }
}

static int clamp_int(int value, int min_value, int max_value) {
    if (value < min_value) return min_value;
    if (value > max_value) return max_value;
    return value;
}

static void show_full_overlay(bool emit_event) {
    g_collapsed = false;
    set_controls_visible(true);
    apply_capture_exclusion(g_hwnd);

    if (g_expanded_rect.right > g_expanded_rect.left && g_expanded_rect.bottom > g_expanded_rect.top) {
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
    }

    RECT work = {0, 0, 0, 0};
    MONITORINFO monitor = {0};
    monitor.cbSize = sizeof(monitor);
    HMONITOR hmonitor = MonitorFromRect(&g_expanded_rect, MONITOR_DEFAULTTONEAREST);
    if (GetMonitorInfoW(hmonitor, &monitor)) {
        work = monitor.rcWork;
    } else {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &work, 0);
    }

    int width = 96;
    int height = 42;
    int margin = 14;
    int x = clamp_int(g_expanded_rect.right - width, work.left + margin, work.right - width - margin);
    int y = clamp_int(g_expanded_rect.top, work.top + margin, work.bottom - height - margin);

    g_collapsed = true;
    g_visible = false;
    set_controls_visible(false);
    SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW | SWP_NOACTIVATE);
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
    int row_h = 34;
    int input_y = bottom - 72;
    int chip_y = bottom - 30;
    int button_w = 72;
    int send_w = 64;
    int gap = 10;

    MoveWindow(g_record_button, composer_left + 12, input_y, button_w, row_h, TRUE);
    MoveWindow(g_send_button, composer_left + composer_w - 12 - send_w, input_y, send_w, row_h, TRUE);
    MoveWindow(g_ask_edit, composer_left + 12 + button_w + gap, input_y, composer_w - 24 - button_w - send_w - (gap * 2), row_h, TRUE);

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
}

static void create_controls(HWND hwnd) {
    HFONT font = (HFONT)GetStockObject(DEFAULT_GUI_FONT);
    if (!g_edit_brush) refresh_edit_brush();
    g_ask_edit = CreateWindowExW(
        0, L"EDIT", L"",
        WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL,
        0, 0, 100, 30, hwnd, (HMENU)ID_ASK_EDIT, GetModuleHandleW(NULL), NULL
    );
    g_send_button = CreateWindowW(L"BUTTON", L"Send", WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        0, 0, 60, 30, hwnd, (HMENU)ID_SEND_BUTTON, GetModuleHandleW(NULL), NULL);
    g_record_button = CreateWindowW(L"BUTTON", L"Mic", WS_CHILD | WS_VISIBLE | BS_OWNERDRAW,
        0, 0, 60, 30, hwnd, (HMENU)ID_RECORD_BUTTON, GetModuleHandleW(NULL), NULL);
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
        g_ask_edit, g_send_button, g_record_button, g_help_button, g_session_button,
        g_page_button, g_attach_button, g_recap_button, g_note_button, g_theme_button, g_close_button
    };
    for (int i = 0; i < (int)(sizeof(controls) / sizeof(controls[0])); i++) {
        SendMessageW(controls[i], WM_SETFONT, (WPARAM)font, TRUE);
    }
    SendMessageW(g_ask_edit, EM_SETCUEBANNER, FALSE, (LPARAM)L"Ask me anything...");
    layout_controls();
}

static void send_current_question(void) {
    wchar_t question[2048];
    GetWindowTextW(g_ask_edit, question, 2048);
    if (wcslen(question) == 0) {
        wcscpy_s(question, 2048, L"Answer the latest clear question from the current transcript, screen context, and attached files. If there is no clear question yet, summarize what Bluey needs next.");
    }
    emit_ask_event(question);
    SetWindowTextW(g_ask_edit, L"");
    SetFocus(g_ask_edit);
}

static DWORD WINAPI stdin_thread(LPVOID unused) {
    (void)unused;
    char line[8192];
    while (fgets(line, sizeof(line), stdin)) {
        if (strstr(line, "\"type\":\"show\"")) {
            show_full_overlay(true);
        } else if (strstr(line, "\"type\":\"hide\"")) {
            collapse_to_pill(g_hwnd, true);
        } else if (strstr(line, "\"type\":\"toggle\"")) {
            if (g_visible) collapse_to_pill(g_hwnd, true);
            else show_full_overlay(true);
        } else if (strstr(line, "\"type\":\"clear\"")) {
            wcscpy_s(g_title, 256, L"Bluey");
            wcscpy_s(g_body, 2048, L"");
            wcscpy_s(g_kind, 64, L"system");
            wcscpy_s(g_source, 256, L"");
            wcscpy_s(g_card_id, 80, L"");
            InvalidateRect(g_hwnd, NULL, TRUE);
        } else if (strstr(line, "\"type\":\"boot\"")) {
            wcscpy_s(g_title, 256, L"Bluey online");
            wcscpy_s(g_body, 2048, L"> overlay link established\n> session memory loaded\n> context controls armed\n> ready");
            wcscpy_s(g_kind, 64, L"system");
            wcscpy_s(g_source, 256, L"");
            wcscpy_s(g_card_id, 80, L"");
            naive_extract_json_string(line, "title", g_title, 256);
            show_full_overlay(false);
            InvalidateRect(g_hwnd, NULL, TRUE);
        } else if (strstr(line, "\"type\":\"set_position\"")) {
            if (strstr(line, "top_left")) position_window("top_left");
            else if (strstr(line, "bottom_left")) position_window("bottom_left");
            else if (strstr(line, "bottom_right")) position_window("bottom_right");
            else if (strstr(line, "center")) position_window("center");
            else position_window("top_right");
        } else if (strstr(line, "\"type\":\"set_opacity\"")) {
            double opacity = g_opacity;
            if (naive_extract_json_number(line, "opacity", &opacity)) {
                set_window_opacity(opacity);
            }
        } else if (strstr(line, "\"type\":\"push_card\"")) {
            naive_extract_json_string(line, "title", g_title, 256);
            naive_extract_json_string(line, "body", g_body, 2048);
            naive_extract_json_string(line, "kind", g_kind, 64);
            naive_extract_json_string(line, "source", g_source, 256);
            naive_extract_json_string(line, "id", g_card_id, 80);
            if (g_visible && !g_collapsed) {
                show_full_overlay(false);
            }
            InvalidateRect(g_hwnd, NULL, TRUE);
        } else if (strstr(line, "\"type\":\"update_card\"")) {
            wchar_t id[80] = L"";
            naive_extract_json_string(line, "id", id, 80);
            if (wcslen(g_card_id) == 0 || wcscmp(id, g_card_id) == 0) {
                naive_extract_json_string(line, "body", g_body, 2048);
                InvalidateRect(g_hwnd, NULL, TRUE);
            }
        } else if (strstr(line, "\"type\":\"shutdown\"")) {
            PostMessage(g_hwnd, WM_CLOSE, 0, 0);
            break;
        } else if (strstr(line, "\"type\":\"ping\"")) {
            printf("{\"type\":\"pong\"}\n");
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
        ID2D1SolidColorBrush_Release(g_d2d_brush);
        g_d2d_brush = NULL;
    }
    if (g_d2d_target) {
        ID2D1HwndRenderTarget_Release(g_d2d_target);
        g_d2d_target = NULL;
    }
}

static void release_d2d_resources(void) {
    release_d2d_target();
    if (g_fmt_pill) {
        IDWriteTextFormat_Release(g_fmt_pill);
        g_fmt_pill = NULL;
    }
    if (g_fmt_brand) {
        IDWriteTextFormat_Release(g_fmt_brand);
        g_fmt_brand = NULL;
    }
    if (g_fmt_label) {
        IDWriteTextFormat_Release(g_fmt_label);
        g_fmt_label = NULL;
    }
    if (g_fmt_title) {
        IDWriteTextFormat_Release(g_fmt_title);
        g_fmt_title = NULL;
    }
    if (g_fmt_body) {
        IDWriteTextFormat_Release(g_fmt_body);
        g_fmt_body = NULL;
    }
    if (g_dwrite_factory) {
        IDWriteFactory_Release(g_dwrite_factory);
        g_dwrite_factory = NULL;
    }
    if (g_d2d_factory) {
        ID2D1Factory_Release(g_d2d_factory);
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
    ID2D1SolidColorBrush_SetColor(g_d2d_brush, &color);
}

static void d2d_fill_round(float left, float top, float right, float bottom, float radius, int red, int green, int blue, float alpha) {
    D2D1_ROUNDED_RECT rounded;
    rounded.rect = d2d_rectf(left, top, right, bottom);
    rounded.radiusX = radius;
    rounded.radiusY = radius;
    d2d_set_brush_color(red, green, blue, alpha);
    ID2D1HwndRenderTarget_FillRoundedRectangle(g_d2d_target, &rounded, (ID2D1Brush *)g_d2d_brush);
}

static void d2d_stroke_round(float left, float top, float right, float bottom, float radius, int red, int green, int blue, float alpha, float width) {
    D2D1_ROUNDED_RECT rounded;
    rounded.rect = d2d_rectf(left, top, right, bottom);
    rounded.radiusX = radius;
    rounded.radiusY = radius;
    d2d_set_brush_color(red, green, blue, alpha);
    ID2D1HwndRenderTarget_DrawRoundedRectangle(g_d2d_target, &rounded, (ID2D1Brush *)g_d2d_brush, width, NULL);
}

static void d2d_text(const wchar_t *text, IDWriteTextFormat *format, D2D1_RECT_F rect, int red, int green, int blue, float alpha) {
    d2d_set_brush_color(red, green, blue, alpha);
    ID2D1HwndRenderTarget_DrawText(
        g_d2d_target,
        text,
        (UINT32)wcslen(text),
        format,
        &rect,
        (ID2D1Brush *)g_d2d_brush,
        D2D1_DRAW_TEXT_OPTIONS_NONE,
        DWRITE_MEASURING_MODE_NATURAL
    );
}

static HRESULT create_text_format(float size, DWRITE_FONT_WEIGHT weight, DWRITE_TEXT_ALIGNMENT alignment, DWRITE_PARAGRAPH_ALIGNMENT paragraph, IDWriteTextFormat **out) {
    HRESULT hr = IDWriteFactory_CreateTextFormat(
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
    IDWriteTextFormat_SetTextAlignment(*out, alignment);
    IDWriteTextFormat_SetParagraphAlignment(*out, paragraph);
    IDWriteTextFormat_SetWordWrapping(*out, DWRITE_WORD_WRAPPING_WRAP);
    return S_OK;
}

static bool init_d2d_resources(void) {
    if (g_d2d_available) return true;

    D2D1_FACTORY_OPTIONS options;
    ZeroMemory(&options, sizeof(options));
    HRESULT hr = D2D1CreateFactory(
        D2D1_FACTORY_TYPE_SINGLE_THREADED,
        &IID_ID2D1Factory,
        &options,
        (void **)&g_d2d_factory
    );
    if (FAILED(hr)) {
        release_d2d_resources();
        return false;
    }

    hr = DWriteCreateFactory(
        DWRITE_FACTORY_TYPE_SHARED,
        &IID_IDWriteFactory,
        (IUnknown **)&g_dwrite_factory
    );
    if (FAILED(hr)) {
        release_d2d_resources();
        return false;
    }

    if (FAILED(create_text_format(13.0f, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, &g_fmt_pill)) ||
        FAILED(create_text_format(20.0f, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, &g_fmt_brand)) ||
        FAILED(create_text_format(13.0f, DWRITE_FONT_WEIGHT_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, &g_fmt_label)) ||
        FAILED(create_text_format(21.0f, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, &g_fmt_title)) ||
        FAILED(create_text_format(16.0f, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, &g_fmt_body))) {
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

    HRESULT hr = ID2D1Factory_CreateHwndRenderTarget(
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
    hr = ID2D1HwndRenderTarget_CreateSolidColorBrush(
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
    if (FAILED(ID2D1HwndRenderTarget_Resize(g_d2d_target, &size))) {
        release_d2d_target();
    }
}

static void draw_bluey_logo_d2d(float x, float y, float size) {
    d2d_fill_round(x, y, x + size, y + size, size * 0.22f, 12, 52, 78, 1.0f);
    d2d_stroke_round(x + 0.5f, y + 0.5f, x + size - 0.5f, y + size - 0.5f, size * 0.22f, 120, 236, 246, 1.0f, 1.2f);
    d2d_fill_round(x + size * 0.20f, y + size * 0.33f, x + size * 0.80f, y + size * 0.76f, size * 0.14f, 6, 17, 31, 1.0f);
    d2d_stroke_round(x + size * 0.20f, y + size * 0.33f, x + size * 0.80f, y + size * 0.76f, size * 0.14f, 100, 233, 255, 1.0f, 1.6f);

    d2d_set_brush_color(245, 252, 255, 1.0f);
    ID2D1HwndRenderTarget_DrawLine(g_d2d_target, d2d_point(x + size * 0.34f, y + size * 0.45f), d2d_point(x + size * 0.44f, y + size * 0.50f), (ID2D1Brush *)g_d2d_brush, 2.0f, NULL);
    ID2D1HwndRenderTarget_DrawLine(g_d2d_target, d2d_point(x + size * 0.44f, y + size * 0.50f), d2d_point(x + size * 0.34f, y + size * 0.58f), (ID2D1Brush *)g_d2d_brush, 2.0f, NULL);
    d2d_set_brush_color(122, 248, 255, 1.0f);
    ID2D1HwndRenderTarget_DrawLine(g_d2d_target, d2d_point(x + size * 0.50f, y + size * 0.61f), d2d_point(x + size * 0.66f, y + size * 0.61f), (ID2D1Brush *)g_d2d_brush, 2.0f, NULL);

    d2d_set_brush_color(139, 255, 157, 1.0f);
    ID2D1HwndRenderTarget_DrawLine(g_d2d_target, d2d_point(x + size * 0.72f, y + size * 0.20f), d2d_point(x + size * 0.72f, y + size * 0.52f), (ID2D1Brush *)g_d2d_brush, 1.6f, NULL);
    ID2D1HwndRenderTarget_DrawLine(g_d2d_target, d2d_point(x + size * 0.56f, y + size * 0.36f), d2d_point(x + size * 0.88f, y + size * 0.36f), (ID2D1Brush *)g_d2d_brush, 1.6f, NULL);
}

static void draw_resize_affordance_d2d(RECT rect) {
    d2d_stroke_round((float)rect.left + 1.0f, (float)rect.top + 1.0f, (float)rect.right - 1.0f, (float)rect.bottom - 1.0f, 18.0f, 38, 98, 138, 0.8f, 1.0f);
    d2d_set_brush_color(120, 236, 246, 1.0f);
    float right = (float)rect.right - 11.0f;
    float bottom = (float)rect.bottom - 10.0f;
    for (int i = 0; i < 3; i++) {
        float offset = 7.0f + ((float)i * 6.0f);
        ID2D1HwndRenderTarget_DrawLine(g_d2d_target, d2d_point(right - offset, bottom), d2d_point(right, bottom - offset), (ID2D1Brush *)g_d2d_brush, 2.0f, NULL);
    }
}

static bool paint_with_d2d(HWND hwnd) {
    if (!ensure_d2d_target(hwnd)) return false;

    RECT rect;
    GetClientRect(hwnd, &rect);
    ID2D1HwndRenderTarget_BeginDraw(g_d2d_target);
    D2D1_COLOR_F bg_color = g_light_theme ? d2d_color_rgb(246, 250, 252, 1.0f) : d2d_color_rgb(2, 4, 6, 1.0f);
    ID2D1HwndRenderTarget_Clear(g_d2d_target, &bg_color);

    if (g_collapsed) {
        draw_bluey_logo_d2d(10.0f, 8.0f, 25.0f);
        d2d_set_brush_color(62, 220, 128, 1.0f);
        D2D1_ELLIPSE dot;
        dot.point = d2d_point((float)rect.right - 17.0f, 14.0f);
        dot.radiusX = 5.0f;
        dot.radiusY = 5.0f;
        ID2D1HwndRenderTarget_FillEllipse(g_d2d_target, &dot, (ID2D1Brush *)g_d2d_brush);
        d2d_text(L"Bluey", g_fmt_pill, d2d_rectf(40.0f, 0.0f, (float)rect.right - 16.0f, (float)rect.bottom), g_light_theme ? 8 : 235, g_light_theme ? 22 : 245, g_light_theme ? 32 : 255, 1.0f);
    } else {
        int header_w = clamp_int((rect.right * 84) / 100, 520, 780);
        if (header_w > rect.right - 28) header_w = rect.right - 28;
        int header_left = (rect.right - header_w) / 2;
        int composer_w = clamp_int((rect.right * 72) / 100, 560, 760);
        int composer_left = (rect.right - composer_w) / 2;

        d2d_fill_round((float)header_left, 8.0f, (float)(header_left + header_w), 54.0f, 22.0f, g_light_theme ? 248 : 5, g_light_theme ? 252 : 11, g_light_theme ? 255 : 18, 1.0f);
        d2d_stroke_round((float)header_left + 0.5f, 8.5f, (float)(header_left + header_w) - 0.5f, 53.5f, 22.0f, g_light_theme ? 82 : 36, g_light_theme ? 172 : 102, g_light_theme ? 205 : 124, 0.9f, 1.2f);

        d2d_fill_round((float)composer_left, (float)rect.bottom - 110.0f, (float)(composer_left + composer_w), (float)rect.bottom - 8.0f, 22.0f, g_light_theme ? 248 : 5, g_light_theme ? 252 : 11, g_light_theme ? 255 : 18, 1.0f);
        d2d_stroke_round((float)composer_left + 0.5f, (float)rect.bottom - 109.5f, (float)(composer_left + composer_w) - 0.5f, (float)rect.bottom - 8.5f, 22.0f, g_light_theme ? 82 : 38, g_light_theme ? 172 : 98, g_light_theme ? 205 : 138, 0.9f, 1.2f);

        draw_resize_affordance_d2d(rect);
        draw_bluey_logo_d2d((float)header_left + 14.0f, 14.0f, 28.0f);

        d2d_set_brush_color(g_recording ? 62 : 40, g_recording ? 220 : 92, g_recording ? 128 : 62, 1.0f);
        D2D1_ELLIPSE record_dot;
        record_dot.point = d2d_point((float)header_left + 114.5f, 29.5f);
        record_dot.radiusX = 4.5f;
        record_dot.radiusY = 4.5f;
        ID2D1HwndRenderTarget_FillEllipse(g_d2d_target, &record_dot, (ID2D1Brush *)g_d2d_brush);

        d2d_text(L"Bluey", g_fmt_brand, d2d_rectf((float)header_left + 50.0f, 10.0f, (float)header_left + 118.0f, 46.0f), g_light_theme ? 8 : 230, g_light_theme ? 22 : 240, g_light_theme ? 32 : 245, 1.0f);

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
        d2d_text(g_body, g_fmt_body, d2d_rectf(18.0f, (float)body_top, (float)rect.right - 18.0f, (float)rect.bottom - 124.0f), g_light_theme ? 22 : 230, g_light_theme ? 43 : 240, g_light_theme ? 56 : 245, 1.0f);
    }

    HRESULT hr = ID2D1HwndRenderTarget_EndDraw(g_d2d_target, NULL, NULL);
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
        if (id == ID_RECORD_BUTTON) {
            g_recording = !g_recording;
            update_record_button();
            InvalidateRect(hwnd, NULL, TRUE);
            emit_simple_event(g_recording ? "recording_start_requested" : "recording_stop_requested");
            return 0;
        }
        if (id == ID_HELP_BUTTON) {
            overlay_message_box(
                L"Green dot: Bluey is connected.\nHelp: show this guide.\nSession: continue or start clean.\nAttach: add files or show attached docs.\nTheme: switch black/white background while keeping Bluey borders.\nStyle: answer rules.\nAnalyse Screen: search/read the active browser page or available screen context and generate an answer.\nRecap: summarize the active session from the bottom bar.\nQuit: stop Bluey completely. Hide/collapse behavior becomes a small Bluey button.\nMic: start/stop audio capture.\nMic dot: dim off, bright green recording.\nSend: ask Bluey.\nMiddle cards: readable but click-through.",
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
            if (answer == IDYES) emit_simple_event("attach_requested");
            if (answer == IDNO) emit_simple_event("context_list_requested");
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
            if (abs(dx) > 2 || abs(dy) > 2) g_collapsed_drag_moved = true;

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
                ReleaseCapture();
                g_collapsed_dragging = false;
                if (g_collapsed_drag_moved) {
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
    case WM_NCHITTEST: {
        if (g_collapsed) return HTCLIENT;
        POINT point = { GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam) };
        RECT rect;
        GetWindowRect(hwnd, &rect);
        int border = 10;
        int header_h = 62;
        int composer_h = 118;
        bool left = point.x < rect.left + border;
        bool right = point.x >= rect.right - border;
        bool top = point.y < rect.top + border;
        bool bottom = point.y >= rect.bottom - border;

        if (top && left) return HTTOPLEFT;
        if (top && right) return HTTOPRIGHT;
        if (bottom && left) return HTBOTTOMLEFT;
        if (bottom && right) return HTBOTTOMRIGHT;
        if (left) return HTLEFT;
        if (right) return HTRIGHT;
        if (top) return HTTOP;
        if (bottom) return HTBOTTOM;
        if (point.y < rect.top + header_h) {
            if (point.x < rect.right - 526) return HTCAPTION;
            return HTCLIENT;
        }
        if (point.y >= rect.bottom - composer_h) return HTCLIENT;
        return HTTRANSPARENT;
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
            DrawTextW(hdc, L"Bluey", -1, &pill_text, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
            DeleteObject(font);
            EndPaint(hwnd, &ps);
            return 0;
        }

        HBRUSH bg = CreateSolidBrush(g_light_theme ? RGB(246, 250, 252) : RGB(2, 4, 6));
        FillRect(hdc, &rect, bg);
        DeleteObject(bg);

        SetBkMode(hdc, TRANSPARENT);
        HBRUSH header = CreateSolidBrush(g_light_theme ? RGB(248, 252, 255) : RGB(5, 11, 18));
        HPEN header_pen = CreatePen(PS_SOLID, 1, g_light_theme ? RGB(82, 172, 205) : RGB(36, 102, 124));
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
        HPEN composer_pen = CreatePen(PS_SOLID, 1, g_light_theme ? RGB(82, 172, 205) : RGB(38, 98, 138));
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
        DrawTextW(hdc, L"Bluey", -1, &brand_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

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

        RECT body_rect = {18, body_top, rect.right - 18, rect.bottom - 124};
        SelectObject(hdc, body_font);
        SetTextColor(hdc, g_light_theme ? RGB(22, 43, 56) : RGB(230, 240, 245));
        DrawTextW(hdc, g_body, -1, &body_rect, DT_LEFT | DT_TOP | DT_WORDBREAK);

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

    DWORD ex_style = WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW;
    g_hwnd = CreateWindowExW(
        ex_style,
        class_name,
        L"Bluey Overlay",
        WS_POPUP | WS_THICKFRAME,
        80,
        80,
        860,
        460,
        NULL,
        NULL,
        instance,
        NULL
    );

    create_controls(g_hwnd);
    set_window_opacity(g_opacity);
    apply_capture_exclusion(g_hwnd);
    ShowWindow(g_hwnd, SW_SHOWNOACTIVATE);
    emit_ready();
    CreateThread(NULL, 0, stdin_thread, NULL, 0, NULL);

    MSG msg;
    while (GetMessage(&msg, NULL, 0, 0)) {
        TranslateMessage(&msg);
        DispatchMessage(&msg);
    }
    return 0;
}
