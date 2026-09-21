#ifndef BLUEY_ANSWER_STYLE_DIALOG_H
#define BLUEY_ANSWER_STYLE_DIALOG_H

#include <stdbool.h>
#include <stddef.h>

#include <windows.h>

#include "answer_style_state.h"

typedef bool (*BlueyAnswerStyleApplyFn)(
    const char *instructions,
    size_t instructions_len,
    BlueyAnswerStyleMode mode,
    void *context);

#ifdef __cplusplus
extern "C" {
#endif

bool bluey_answer_style_dialog_show(
    HINSTANCE instance,
    HWND owner,
    bool light_theme,
    BlueyOverlayAccountState account_state,
    BlueyAnswerStyleApplyFn apply,
    void *context);
bool bluey_answer_style_dialog_process_message(const MSG *message);
bool bluey_answer_style_dialog_is_open(void);
bool bluey_answer_style_dialog_hydrate(
    const char *instructions,
    size_t instructions_len);
void bluey_answer_style_dialog_close(void);
void bluey_answer_style_dialog_dispose(void);

#ifdef __cplusplus
}
#endif

#endif
