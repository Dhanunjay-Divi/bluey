use std::sync::Arc;

use cue_core::overlay_ipc::OverlayUiState;

pub(crate) type SharedOverlayUiState = Arc<parking_lot::Mutex<OverlayUiState>>;

pub(crate) fn new_shared_overlay_ui_state() -> SharedOverlayUiState {
    Arc::new(parking_lot::Mutex::new(OverlayUiState::Idle))
}

pub(crate) struct OverlayUiStateScope {
    state: SharedOverlayUiState,
}

impl Drop for OverlayUiStateScope {
    fn drop(&mut self) {
        *self.state.lock() = OverlayUiState::Idle;
    }
}

pub(crate) fn enter_overlay_ui_state(
    state: &SharedOverlayUiState,
    next: OverlayUiState,
) -> OverlayUiStateScope {
    *state.lock() = next;
    reset_overlay_ui_state_on_scope_exit(state)
}

pub(crate) fn reset_overlay_ui_state_on_scope_exit(
    state: &SharedOverlayUiState,
) -> OverlayUiStateScope {
    OverlayUiStateScope {
        state: Arc::clone(state),
    }
}

#[cfg(test)]
mod tests {
    use cue_core::overlay_ipc::OverlayUiState;

    use super::*;

    #[test]
    fn overlay_ui_state_scope_enters_then_resets_to_idle() {
        let state = new_shared_overlay_ui_state();
        {
            let _scope = enter_overlay_ui_state(&state, OverlayUiState::AttachOpen);
            assert_eq!(*state.lock(), OverlayUiState::AttachOpen);
        }

        assert_eq!(*state.lock(), OverlayUiState::Idle);
    }

    #[test]
    fn overlay_ui_state_submit_scope_resets_existing_open_state() {
        let state = new_shared_overlay_ui_state();
        *state.lock() = OverlayUiState::InstructionsOpen;
        {
            let _scope = reset_overlay_ui_state_on_scope_exit(&state);
            assert_eq!(*state.lock(), OverlayUiState::InstructionsOpen);
        }

        assert_eq!(*state.lock(), OverlayUiState::Idle);
    }
}
