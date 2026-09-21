#include "answer_style_dialog.h"

#include <limits.h>
#include <stdlib.h>
#include <string.h>

#include "overlay_windows_shell.h"

#ifndef WM_DPICHANGED
#define WM_DPICHANGED 0x02E0
#endif

#ifndef WC_ERR_INVALID_CHARS
#define WC_ERR_INVALID_CHARS 0x00000080
#endif

#define BLUEY_STYLE_ID_DEFAULT 12101
#define BLUEY_STYLE_ID_CONCISE 12102
#define BLUEY_STYLE_ID_STAR 12103
#define BLUEY_STYLE_ID_CUSTOM 12104
#define BLUEY_STYLE_ID_CUSTOM_EDIT 12105
#define BLUEY_STYLE_ID_APPLY 12106
#define BLUEY_STYLE_ID_CANCEL 12107

typedef struct BlueyAnswerStyleDialogState {
    HWND hwnd;
    HWND owner;
    HWND title;
    HWND subtitle;
    HWND radios[4];
    HWND guidance;
    HWND custom_label;
    HWND custom_edit;
    HWND note;
    HWND error;
    HWND apply_button;
    HWND cancel_button;
    HFONT title_font;
    HFONT body_font;
    HFONT small_font;
    HBRUSH background_brush;
    HBRUSH edit_brush;
    unsigned dpi;
    bool light_theme;
    bool initialized;
    BlueyAnswerStyleMode selected_mode;
    BlueyAnswerStyleMode saved_mode;
    wchar_t saved_custom[BLUEY_ANSWER_STYLE_MAX_UTF8_BYTES + 1u];
    BlueyAnswerStyleApplyFn apply;
    void *apply_context;
} BlueyAnswerStyleDialogState;

static BlueyAnswerStyleDialogState g_answer_style_dialog;

static int bluey_style_scale(int logical_px) {
    return bluey_scale_logical_px(
        logical_px,
        g_answer_style_dialog.dpi == 0
            ? BLUEY_DEFAULT_DPI
            : g_answer_style_dialog.dpi);
}

static void bluey_style_delete_resources(void) {
    if (g_answer_style_dialog.title_font) {
        DeleteObject(g_answer_style_dialog.title_font);
        g_answer_style_dialog.title_font = NULL;
    }
    if (g_answer_style_dialog.body_font) {
        DeleteObject(g_answer_style_dialog.body_font);
        g_answer_style_dialog.body_font = NULL;
    }
    if (g_answer_style_dialog.small_font) {
        DeleteObject(g_answer_style_dialog.small_font);
        g_answer_style_dialog.small_font = NULL;
    }
    if (g_answer_style_dialog.background_brush) {
        DeleteObject(g_answer_style_dialog.background_brush);
        g_answer_style_dialog.background_brush = NULL;
    }
    if (g_answer_style_dialog.edit_brush) {
        DeleteObject(g_answer_style_dialog.edit_brush);
        g_answer_style_dialog.edit_brush = NULL;
    }
}

static HFONT bluey_style_font(int points, int weight) {
    int height = -MulDiv(
        points,
        (int)(g_answer_style_dialog.dpi == 0
            ? BLUEY_DEFAULT_DPI
            : g_answer_style_dialog.dpi),
        72);
    return CreateFontW(
        height,
        0,
        0,
        0,
        weight,
        FALSE,
        FALSE,
        FALSE,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        DEFAULT_PITCH | FF_DONTCARE,
        L"Segoe UI");
}

static void bluey_style_refresh_resources(void) {
    bluey_style_delete_resources();
    g_answer_style_dialog.title_font = bluey_style_font(17, FW_SEMIBOLD);
    g_answer_style_dialog.body_font = bluey_style_font(10, FW_NORMAL);
    g_answer_style_dialog.small_font = bluey_style_font(9, FW_NORMAL);
    COLORREF background = g_answer_style_dialog.light_theme
        ? RGB(247, 251, 255)
        : RGB(16, 23, 31);
    COLORREF edit = g_answer_style_dialog.light_theme
        ? RGB(255, 255, 255)
        : RGB(25, 35, 46);
    g_answer_style_dialog.background_brush = CreateSolidBrush(background);
    g_answer_style_dialog.edit_brush = CreateSolidBrush(edit);

    HWND title_controls[] = {g_answer_style_dialog.title};
    HWND body_controls[] = {
        g_answer_style_dialog.subtitle,
        g_answer_style_dialog.radios[0],
        g_answer_style_dialog.radios[1],
        g_answer_style_dialog.radios[2],
        g_answer_style_dialog.radios[3],
        g_answer_style_dialog.guidance,
        g_answer_style_dialog.custom_label,
        g_answer_style_dialog.custom_edit,
        g_answer_style_dialog.apply_button,
        g_answer_style_dialog.cancel_button,
    };
    HWND small_controls[] = {g_answer_style_dialog.note, g_answer_style_dialog.error};
    for (size_t index = 0; index < sizeof(title_controls) / sizeof(title_controls[0]); index++) {
        if (title_controls[index]) {
            SendMessageW(
                title_controls[index],
                WM_SETFONT,
                (WPARAM)g_answer_style_dialog.title_font,
                TRUE);
        }
    }
    for (size_t index = 0; index < sizeof(body_controls) / sizeof(body_controls[0]); index++) {
        if (body_controls[index]) {
            SendMessageW(
                body_controls[index],
                WM_SETFONT,
                (WPARAM)g_answer_style_dialog.body_font,
                TRUE);
        }
    }
    for (size_t index = 0; index < sizeof(small_controls) / sizeof(small_controls[0]); index++) {
        if (small_controls[index]) {
            SendMessageW(
                small_controls[index],
                WM_SETFONT,
                (WPARAM)g_answer_style_dialog.small_font,
                TRUE);
        }
    }
}

static void bluey_style_layout(void) {
    if (!g_answer_style_dialog.hwnd) return;
    RECT client;
    GetClientRect(g_answer_style_dialog.hwnd, &client);
    int width = client.right - client.left;
    int height = client.bottom - client.top;
    int margin = bluey_style_scale(24);
    int gap = bluey_style_scale(8);
    int radio_width = (width - margin * 2 - gap * 3) / 4;

    MoveWindow(
        g_answer_style_dialog.title,
        margin,
        bluey_style_scale(18),
        width - margin * 2,
        bluey_style_scale(30),
        TRUE);
    MoveWindow(
        g_answer_style_dialog.subtitle,
        margin,
        bluey_style_scale(50),
        width - margin * 2,
        bluey_style_scale(40),
        TRUE);
    for (int index = 0; index < 4; index++) {
        MoveWindow(
            g_answer_style_dialog.radios[index],
            margin + index * (radio_width + gap),
            bluey_style_scale(98),
            radio_width,
            bluey_style_scale(30),
            TRUE);
    }
    MoveWindow(
        g_answer_style_dialog.guidance,
        margin,
        bluey_style_scale(139),
        width - margin * 2,
        bluey_style_scale(42),
        TRUE);
    MoveWindow(
        g_answer_style_dialog.custom_label,
        margin,
        bluey_style_scale(190),
        width - margin * 2,
        bluey_style_scale(22),
        TRUE);
    MoveWindow(
        g_answer_style_dialog.custom_edit,
        margin,
        bluey_style_scale(214),
        width - margin * 2,
        bluey_style_scale(98),
        TRUE);
    MoveWindow(
        g_answer_style_dialog.note,
        margin,
        bluey_style_scale(320),
        width - margin * 2,
        bluey_style_scale(34),
        TRUE);
    MoveWindow(
        g_answer_style_dialog.error,
        margin,
        bluey_style_scale(355),
        width - margin * 2 - bluey_style_scale(220),
        bluey_style_scale(34),
        TRUE);
    int button_y = height - margin - bluey_style_scale(34);
    MoveWindow(
        g_answer_style_dialog.cancel_button,
        width - margin - bluey_style_scale(206),
        button_y,
        bluey_style_scale(96),
        bluey_style_scale(34),
        TRUE);
    MoveWindow(
        g_answer_style_dialog.apply_button,
        width - margin - bluey_style_scale(102),
        button_y,
        bluey_style_scale(102),
        bluey_style_scale(34),
        TRUE);
}

static const wchar_t *bluey_style_guidance(BlueyAnswerStyleMode mode) {
    switch (mode) {
    case BLUEY_ANSWER_STYLE_DEFAULT:
        return L"Bluey chooses the clearest structure for each question.";
    case BLUEY_ANSWER_STYLE_CONCISE:
        return L"A direct, speakable answer, usually in 2-4 sentences.";
    case BLUEY_ANSWER_STYLE_STAR:
        return L"Evidence-backed Situation, Task, Action, and Result.";
    case BLUEY_ANSWER_STYLE_CUSTOM:
        return L"Write session-specific answer instructions below.";
    default:
        return L"";
    }
}

static const wchar_t *bluey_style_apply_label(BlueyAnswerStyleMode mode) {
    switch (mode) {
    case BLUEY_ANSWER_STYLE_DEFAULT:
        return L"Use Default";
    case BLUEY_ANSWER_STYLE_CONCISE:
        return L"Use Concise";
    case BLUEY_ANSWER_STYLE_STAR:
        return L"Use STAR";
    case BLUEY_ANSWER_STYLE_CUSTOM:
        return L"Use Custom";
    default:
        return L"Apply";
    }
}

static void bluey_style_set_error(const wchar_t *message) {
    SetWindowTextW(g_answer_style_dialog.error, message ? message : L"");
}

static void bluey_style_select(BlueyAnswerStyleMode mode, bool focus_custom) {
    if (mode < BLUEY_ANSWER_STYLE_DEFAULT || mode > BLUEY_ANSWER_STYLE_CUSTOM) {
        mode = BLUEY_ANSWER_STYLE_DEFAULT;
    }
    g_answer_style_dialog.selected_mode = mode;
    for (int index = 0; index < 4; index++) {
        SendMessageW(
            g_answer_style_dialog.radios[index],
            BM_SETCHECK,
            index == (int)mode ? BST_CHECKED : BST_UNCHECKED,
            0);
    }
    bool custom = mode == BLUEY_ANSWER_STYLE_CUSTOM;
    EnableWindow(g_answer_style_dialog.custom_edit, custom);
    SetWindowTextW(g_answer_style_dialog.guidance, bluey_style_guidance(mode));
    SetWindowTextW(g_answer_style_dialog.apply_button, bluey_style_apply_label(mode));
    bluey_style_set_error(L"");
    if (custom && focus_custom) SetFocus(g_answer_style_dialog.custom_edit);
    InvalidateRect(g_answer_style_dialog.hwnd, NULL, TRUE);
}

static HWND bluey_style_create_child(
    DWORD ex_style,
    const wchar_t *class_name,
    const wchar_t *text,
    DWORD style,
    int id
) {
    return CreateWindowExW(
        ex_style,
        class_name,
        text,
        WS_CHILD | WS_VISIBLE | style,
        0,
        0,
        1,
        1,
        g_answer_style_dialog.hwnd,
        (HMENU)(INT_PTR)id,
        GetModuleHandleW(NULL),
        NULL);
}

static bool bluey_style_create_children(void) {
    g_answer_style_dialog.title = bluey_style_create_child(
        0, L"STATIC", L"How should Bluey answer?", SS_LEFT, 0);
    g_answer_style_dialog.subtitle = bluey_style_create_child(
        0,
        L"STATIC",
        L"Choose a style for this session. Default clears saved answer rules.",
        SS_LEFT,
        0);
    const wchar_t *labels[] = {L"Default", L"Concise", L"STAR", L"Custom"};
    const int ids[] = {
        BLUEY_STYLE_ID_DEFAULT,
        BLUEY_STYLE_ID_CONCISE,
        BLUEY_STYLE_ID_STAR,
        BLUEY_STYLE_ID_CUSTOM,
    };
    for (int index = 0; index < 4; index++) {
        DWORD style = BS_AUTORADIOBUTTON | WS_TABSTOP;
        if (index == 0) style |= WS_GROUP;
        g_answer_style_dialog.radios[index] = bluey_style_create_child(
            0, L"BUTTON", labels[index], style, ids[index]);
    }
    g_answer_style_dialog.guidance = bluey_style_create_child(
        0, L"STATIC", L"", SS_LEFT, 0);
    g_answer_style_dialog.custom_label = bluey_style_create_child(
        0, L"STATIC", L"Custom instructions", SS_LEFT, 0);
    g_answer_style_dialog.custom_edit = bluey_style_create_child(
        WS_EX_CLIENTEDGE,
        L"EDIT",
        L"",
        ES_LEFT | ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN
            | WS_VSCROLL | WS_TABSTOP,
        BLUEY_STYLE_ID_CUSTOM_EDIT);
    g_answer_style_dialog.note = bluey_style_create_child(
        0,
        L"STATIC",
        L"This changes answer wording only. Search and Analyse Screen remain separate actions.",
        SS_LEFT,
        0);
    g_answer_style_dialog.error = bluey_style_create_child(
        0, L"STATIC", L"", SS_LEFT, 0);
    g_answer_style_dialog.cancel_button = bluey_style_create_child(
        0,
        L"BUTTON",
        L"Cancel",
        BS_PUSHBUTTON | WS_TABSTOP,
        BLUEY_STYLE_ID_CANCEL);
    g_answer_style_dialog.apply_button = bluey_style_create_child(
        0,
        L"BUTTON",
        L"Use Default",
        BS_DEFPUSHBUTTON | WS_TABSTOP,
        BLUEY_STYLE_ID_APPLY);

    HWND required[] = {
        g_answer_style_dialog.title,
        g_answer_style_dialog.subtitle,
        g_answer_style_dialog.radios[0],
        g_answer_style_dialog.radios[1],
        g_answer_style_dialog.radios[2],
        g_answer_style_dialog.radios[3],
        g_answer_style_dialog.guidance,
        g_answer_style_dialog.custom_label,
        g_answer_style_dialog.custom_edit,
        g_answer_style_dialog.note,
        g_answer_style_dialog.error,
        g_answer_style_dialog.cancel_button,
        g_answer_style_dialog.apply_button,
    };
    for (size_t index = 0; index < sizeof(required) / sizeof(required[0]); index++) {
        if (!required[index]) return false;
    }
    SendMessageW(
        g_answer_style_dialog.custom_edit,
        EM_SETLIMITTEXT,
        BLUEY_ANSWER_STYLE_MAX_UTF8_BYTES,
        0);
    return true;
}

static char *bluey_style_custom_utf8(size_t *text_len) {
    if (text_len) *text_len = 0;
    int wide_len = GetWindowTextLengthW(g_answer_style_dialog.custom_edit);
    if (wide_len < 0 || wide_len > INT_MAX - 1) return NULL;
    wchar_t *wide = (wchar_t *)calloc((size_t)wide_len + 1u, sizeof(wchar_t));
    if (!wide) return NULL;
    if (GetWindowTextW(
            g_answer_style_dialog.custom_edit,
            wide,
            wide_len + 1) != wide_len) {
        free(wide);
        return NULL;
    }
    if (wide_len == 0) {
        free(wide);
        char *empty = (char *)calloc(1, 1);
        return empty;
    }
    int bytes = WideCharToMultiByte(
        CP_UTF8,
        WC_ERR_INVALID_CHARS,
        wide,
        wide_len,
        NULL,
        0,
        NULL,
        NULL);
    if (bytes <= 0) {
        free(wide);
        return NULL;
    }
    char *utf8 = (char *)malloc((size_t)bytes + 1u);
    if (!utf8) {
        free(wide);
        return NULL;
    }
    int written = WideCharToMultiByte(
        CP_UTF8,
        WC_ERR_INVALID_CHARS,
        wide,
        wide_len,
        utf8,
        bytes,
        NULL,
        NULL);
    free(wide);
    if (written != bytes) {
        free(utf8);
        return NULL;
    }
    utf8[bytes] = '\0';
    if (text_len) *text_len = (size_t)bytes;
    return utf8;
}

static bool bluey_style_utf8_to_saved_custom(
    const char *text,
    size_t text_len
) {
    if ((!text && text_len != 0) || text_len > INT_MAX) return false;
    if (text_len == 0) {
        g_answer_style_dialog.saved_custom[0] = L'\0';
        return true;
    }
    int wide_len = MultiByteToWideChar(
        CP_UTF8,
        MB_ERR_INVALID_CHARS,
        text,
        (int)text_len,
        NULL,
        0);
    if (wide_len <= 0
        || (size_t)wide_len >= sizeof(g_answer_style_dialog.saved_custom)
            / sizeof(g_answer_style_dialog.saved_custom[0])) {
        return false;
    }
    if (MultiByteToWideChar(
            CP_UTF8,
            MB_ERR_INVALID_CHARS,
            text,
            (int)text_len,
            g_answer_style_dialog.saved_custom,
            wide_len) != wide_len) {
        return false;
    }
    g_answer_style_dialog.saved_custom[wide_len] = L'\0';
    return true;
}

static void bluey_style_apply(void) {
    char *custom = NULL;
    size_t custom_len = 0;
    if (g_answer_style_dialog.selected_mode == BLUEY_ANSWER_STYLE_CUSTOM) {
        custom = bluey_style_custom_utf8(&custom_len);
        if (!custom) {
            bluey_style_set_error(L"Use valid text for custom instructions.");
            MessageBeep(MB_ICONWARNING);
            return;
        }
    }

    char *instructions = (char *)malloc(BLUEY_ANSWER_STYLE_MAX_UTF8_BYTES + 1u);
    if (!instructions) {
        free(custom);
        bluey_style_set_error(L"Bluey could not save this style. Try again.");
        return;
    }
    size_t instructions_len = 0;
    BlueyAnswerStyleResult result = bluey_answer_style_prepare(
        g_answer_style_dialog.selected_mode,
        custom,
        custom_len,
        instructions,
        BLUEY_ANSWER_STYLE_MAX_UTF8_BYTES + 1u,
        &instructions_len);
    if (result != BLUEY_ANSWER_STYLE_OK) {
        free(instructions);
        free(custom);
        if (result == BLUEY_ANSWER_STYLE_EMPTY_CUSTOM) {
            bluey_style_set_error(
                L"Write custom instructions, or choose Default to clear them.");
        } else if (result == BLUEY_ANSWER_STYLE_TOO_LONG) {
            bluey_style_set_error(L"Custom instructions are too long.");
        } else {
            bluey_style_set_error(L"Use valid text for custom instructions.");
        }
        MessageBeep(MB_ICONWARNING);
        return;
    }

    bool applied = g_answer_style_dialog.apply
        && g_answer_style_dialog.apply(
            instructions,
            instructions_len,
            g_answer_style_dialog.selected_mode,
            g_answer_style_dialog.apply_context);
    if (!applied) {
        free(instructions);
        free(custom);
        bluey_style_set_error(L"Bluey could not save this style. Try again.");
        MessageBeep(MB_ICONWARNING);
        return;
    }

    g_answer_style_dialog.saved_mode = g_answer_style_dialog.selected_mode;
    if (g_answer_style_dialog.selected_mode == BLUEY_ANSWER_STYLE_CUSTOM) {
        GetWindowTextW(
            g_answer_style_dialog.custom_edit,
            g_answer_style_dialog.saved_custom,
            (int)(sizeof(g_answer_style_dialog.saved_custom)
                / sizeof(g_answer_style_dialog.saved_custom[0])));
    }
    free(instructions);
    free(custom);
    bluey_answer_style_dialog_close();
}

static void bluey_style_release_owner(void) {
    if (g_answer_style_dialog.owner && IsWindow(g_answer_style_dialog.owner)) {
        EnableWindow(g_answer_style_dialog.owner, TRUE);
        SetActiveWindow(g_answer_style_dialog.owner);
    }
}

static LRESULT CALLBACK bluey_answer_style_dialog_proc(
    HWND hwnd,
    UINT message,
    WPARAM wparam,
    LPARAM lparam
) {
    switch (message) {
    case WM_NCCREATE:
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            (LONG_PTR)((CREATESTRUCTW *)lparam)->lpCreateParams);
        return TRUE;
    case WM_CREATE:
        g_answer_style_dialog.hwnd = hwnd;
        g_answer_style_dialog.dpi = bluey_dpi_for_window(hwnd);
        if (!bluey_style_create_children()) return -1;
        bluey_style_refresh_resources();
        SetWindowTextW(
            g_answer_style_dialog.custom_edit,
            g_answer_style_dialog.saved_custom);
        bluey_style_select(g_answer_style_dialog.saved_mode, false);
        bluey_style_layout();
        bluey_apply_capture_exclusion(hwnd);
        return 0;
    case WM_COMMAND: {
        int id = LOWORD(wparam);
        if (HIWORD(wparam) == BN_CLICKED) {
            if (id >= BLUEY_STYLE_ID_DEFAULT && id <= BLUEY_STYLE_ID_CUSTOM) {
                BlueyAnswerStyleMode mode = (BlueyAnswerStyleMode)(
                    id - BLUEY_STYLE_ID_DEFAULT);
                bluey_style_select(mode, mode == BLUEY_ANSWER_STYLE_CUSTOM);
                return 0;
            }
            if (id == BLUEY_STYLE_ID_APPLY) {
                bluey_style_apply();
                return 0;
            }
            if (id == BLUEY_STYLE_ID_CANCEL) {
                bluey_answer_style_dialog_close();
                return 0;
            }
        }
        break;
    }
    case WM_CTLCOLORSTATIC:
    case WM_CTLCOLORBTN: {
        HDC dc = (HDC)wparam;
        HWND control = (HWND)lparam;
        if (control == g_answer_style_dialog.apply_button
            || control == g_answer_style_dialog.cancel_button) {
            break;
        }
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(
            dc,
            control == g_answer_style_dialog.error
                ? RGB(222, 74, 86)
                : (g_answer_style_dialog.light_theme
                    ? RGB(18, 35, 48)
                    : RGB(232, 241, 247)));
        return (LRESULT)g_answer_style_dialog.background_brush;
    }
    case WM_CTLCOLOREDIT: {
        HDC dc = (HDC)wparam;
        SetBkColor(
            dc,
            g_answer_style_dialog.light_theme
                ? RGB(255, 255, 255)
                : RGB(25, 35, 46));
        SetTextColor(
            dc,
            g_answer_style_dialog.light_theme
                ? RGB(18, 35, 48)
                : RGB(238, 246, 250));
        return (LRESULT)g_answer_style_dialog.edit_brush;
    }
    case WM_ERASEBKGND: {
        RECT rect;
        GetClientRect(hwnd, &rect);
        FillRect((HDC)wparam, &rect, g_answer_style_dialog.background_brush);
        return 1;
    }
    case WM_SIZE:
        bluey_style_layout();
        return 0;
    case WM_DPICHANGED: {
        g_answer_style_dialog.dpi = HIWORD(wparam);
        RECT *suggested = (RECT *)lparam;
        SetWindowPos(
            hwnd,
            NULL,
            suggested->left,
            suggested->top,
            suggested->right - suggested->left,
            suggested->bottom - suggested->top,
            SWP_NOACTIVATE | SWP_NOZORDER);
        bluey_style_refresh_resources();
        bluey_style_layout();
        return 0;
    }
    case WM_GETMINMAXINFO: {
        MINMAXINFO *limits = (MINMAXINFO *)lparam;
        limits->ptMinTrackSize.x = bluey_style_scale(520);
        limits->ptMinTrackSize.y = bluey_style_scale(440);
        return 0;
    }
    case WM_CLOSE:
        bluey_answer_style_dialog_close();
        return 0;
    case WM_NCDESTROY:
        bluey_style_delete_resources();
        g_answer_style_dialog.hwnd = NULL;
        bluey_style_release_owner();
        return 0;
    default:
        break;
    }
    return DefWindowProcW(hwnd, message, wparam, lparam);
}

static bool bluey_style_register_class(HINSTANCE instance) {
    const wchar_t *class_name = L"BlueyAnswerStyleDialog";
    WNDCLASSW window_class = {0};
    window_class.lpfnWndProc = bluey_answer_style_dialog_proc;
    window_class.hInstance = instance;
    window_class.lpszClassName = class_name;
    window_class.hCursor = LoadCursorW(NULL, IDC_ARROW);
    if (RegisterClassW(&window_class)) return true;
    return GetLastError() == ERROR_CLASS_ALREADY_EXISTS;
}

bool bluey_answer_style_dialog_show(
    HINSTANCE instance,
    HWND owner,
    bool light_theme,
    BlueyOverlayAccountState account_state,
    BlueyAnswerStyleApplyFn apply,
    void *context
) {
    if (!instance || !owner || !apply
        || !bluey_answer_style_can_open(account_state)) {
        return false;
    }
    if (g_answer_style_dialog.hwnd) {
        ShowWindow(g_answer_style_dialog.hwnd, SW_SHOW);
        SetForegroundWindow(g_answer_style_dialog.hwnd);
        return true;
    }
    if (!g_answer_style_dialog.initialized) {
        g_answer_style_dialog.initialized = true;
        g_answer_style_dialog.saved_mode = BLUEY_ANSWER_STYLE_DEFAULT;
    }
    if (!bluey_style_register_class(instance)) return false;

    g_answer_style_dialog.owner = owner;
    g_answer_style_dialog.light_theme = light_theme;
    g_answer_style_dialog.apply = apply;
    g_answer_style_dialog.apply_context = context;
    g_answer_style_dialog.dpi = bluey_dpi_for_window(owner);
    int width = bluey_style_scale(560);
    int height = bluey_style_scale(460);
    RECT owner_rect;
    GetWindowRect(owner, &owner_rect);
    int x = owner_rect.left + ((owner_rect.right - owner_rect.left) - width) / 2;
    int y = owner_rect.top + ((owner_rect.bottom - owner_rect.top) - height) / 2;

    HWND hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_DLGMODALFRAME,
        L"BlueyAnswerStyleDialog",
        L"Bluey answer style",
        WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
        x,
        y,
        width,
        height,
        owner,
        NULL,
        instance,
        &g_answer_style_dialog);
    if (!hwnd) {
        g_answer_style_dialog.owner = NULL;
        return false;
    }
    EnableWindow(owner, FALSE);
    ShowWindow(hwnd, SW_SHOW);
    UpdateWindow(hwnd);
    SetForegroundWindow(hwnd);
    HWND initial_focus = g_answer_style_dialog.saved_mode == BLUEY_ANSWER_STYLE_CUSTOM
        ? g_answer_style_dialog.custom_edit
        : g_answer_style_dialog.radios[g_answer_style_dialog.saved_mode];
    SetFocus(initial_focus);
    return true;
}

bool bluey_answer_style_dialog_process_message(const MSG *message) {
    if (!message || !g_answer_style_dialog.hwnd) return false;
    if (message->hwnd != g_answer_style_dialog.hwnd
        && !IsChild(g_answer_style_dialog.hwnd, message->hwnd)) {
        return false;
    }
    if (message->message == WM_KEYDOWN) {
        BlueyAnswerStyleKey key = BLUEY_ANSWER_STYLE_KEY_OTHER;
        if (message->wParam == VK_ESCAPE) key = BLUEY_ANSWER_STYLE_KEY_ESCAPE;
        else if (message->wParam == VK_RETURN) key = BLUEY_ANSWER_STYLE_KEY_ENTER;
        else if (message->wParam == VK_TAB) key = BLUEY_ANSWER_STYLE_KEY_TAB;
        bool custom_focused = GetFocus() == g_answer_style_dialog.custom_edit;
        bool control_pressed = (GetKeyState(VK_CONTROL) & 0x8000) != 0;
        BlueyAnswerStyleKeyAction action = bluey_answer_style_key_action(
            key,
            custom_focused,
            control_pressed);
        if (action == BLUEY_ANSWER_STYLE_KEY_ACTION_CANCEL) {
            bluey_answer_style_dialog_close();
            return true;
        }
        if (action == BLUEY_ANSWER_STYLE_KEY_ACTION_APPLY) {
            bluey_style_apply();
            return true;
        }
        if (key == BLUEY_ANSWER_STYLE_KEY_ENTER && custom_focused) {
            return false;
        }
    }
    return IsDialogMessageW(g_answer_style_dialog.hwnd, (MSG *)message) != FALSE;
}

bool bluey_answer_style_dialog_is_open(void) {
    return g_answer_style_dialog.hwnd != NULL;
}

bool bluey_answer_style_dialog_hydrate(
    const char *instructions,
    size_t instructions_len
) {
    if ((!instructions && instructions_len != 0)
        || instructions_len > BLUEY_ANSWER_STYLE_MAX_UTF8_BYTES
        || !bluey_answer_style_is_valid_utf8(instructions, instructions_len)) {
        return false;
    }
    if (!g_answer_style_dialog.initialized) {
        g_answer_style_dialog.initialized = true;
    }
    BlueyAnswerStyleMode mode = bluey_answer_style_match(
        instructions,
        instructions_len);
    g_answer_style_dialog.saved_mode = mode;
    g_answer_style_dialog.saved_custom[0] = L'\0';
    if (mode == BLUEY_ANSWER_STYLE_CUSTOM) {
        char *trimmed = (char *)malloc(instructions_len + 1u);
        if (!trimmed) return false;
        size_t trimmed_len = 0;
        BlueyAnswerStyleResult result = bluey_answer_style_prepare(
            BLUEY_ANSWER_STYLE_CUSTOM,
            instructions,
            instructions_len,
            trimmed,
            instructions_len + 1u,
            &trimmed_len);
        bool converted = result == BLUEY_ANSWER_STYLE_OK
            && bluey_style_utf8_to_saved_custom(trimmed, trimmed_len);
        free(trimmed);
        if (!converted) return false;
    }
    if (g_answer_style_dialog.hwnd) {
        SetWindowTextW(
            g_answer_style_dialog.custom_edit,
            g_answer_style_dialog.saved_custom);
        bluey_style_select(mode, false);
    }
    return true;
}

void bluey_answer_style_dialog_close(void) {
    HWND hwnd = g_answer_style_dialog.hwnd;
    if (hwnd) DestroyWindow(hwnd);
}

void bluey_answer_style_dialog_dispose(void) {
    bluey_answer_style_dialog_close();
    bluey_style_delete_resources();
    bluey_style_release_owner();
}
