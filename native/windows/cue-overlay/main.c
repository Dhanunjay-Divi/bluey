#ifndef UNICODE
#define UNICODE
#endif

#ifndef _UNICODE
#define _UNICODE
#endif

#include <windows.h>
#include <windowsx.h>
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

static LRESULT CALLBACK wnd_proc(HWND hwnd, UINT msg, WPARAM wparam, LPARAM lparam) {
    switch (msg) {
    case WM_SIZE:
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
