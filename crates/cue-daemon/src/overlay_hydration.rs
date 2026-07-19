//! Per-process barrier for native-overlay state hydration.
//!
//! A helper's transport-level `Ready` event is forwarded asynchronously. The
//! daemon must not deliver ordinary commands until the matching process
//! generation has received its complete persisted-state snapshot.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum OverlayHydrationPhase {
    #[default]
    Pending,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct OverlayHydrationState {
    generation: u64,
    phase: OverlayHydrationPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayHydrationDecision {
    Pending,
    Ready,
    Failed,
    Replaced,
}

/// Programmatic appearance feedback emitted while a generation is being
/// hydrated. These acknowledgements confirm helper state; they must not be
/// mistaken for a newer user-driven appearance change.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct OverlayHydrationFeedback {
    generation: u64,
    visibility: Option<bool>,
}

impl OverlayHydrationFeedback {
    pub(crate) fn begin(&mut self, generation: u64) {
        if generation <= self.generation {
            return;
        }
        self.generation = generation;
        self.visibility = None;
    }

    pub(crate) fn expect_visibility(&mut self, generation: u64, visible: bool) {
        if generation == self.generation {
            self.visibility = Some(visible);
        }
    }

    pub(crate) fn consume_visibility(&mut self, generation: u64, visible: bool) -> bool {
        if generation != self.generation {
            return false;
        }
        self.visibility.take() == Some(visible)
    }
}

impl OverlayHydrationState {
    /// Start a newer process generation. Generations are monotonic, so an old
    /// spawn completion can never roll the barrier backward.
    pub(crate) fn begin(&mut self, generation: u64) -> bool {
        if generation <= self.generation {
            return false;
        }
        self.generation = generation;
        self.phase = OverlayHydrationPhase::Pending;
        true
    }

    pub(crate) fn complete(&mut self, generation: u64) -> bool {
        if generation != self.generation || self.phase != OverlayHydrationPhase::Pending {
            return false;
        }
        self.phase = OverlayHydrationPhase::Ready;
        true
    }

    pub(crate) fn fail(&mut self, generation: u64) -> bool {
        if generation != self.generation || self.phase == OverlayHydrationPhase::Failed {
            return false;
        }
        self.phase = OverlayHydrationPhase::Failed;
        true
    }

    pub(crate) fn decision(&self, expected_generation: u64) -> OverlayHydrationDecision {
        if expected_generation != self.generation {
            return OverlayHydrationDecision::Replaced;
        }
        match self.phase {
            OverlayHydrationPhase::Pending => OverlayHydrationDecision::Pending,
            OverlayHydrationPhase::Ready => OverlayHydrationDecision::Ready,
            OverlayHydrationPhase::Failed => OverlayHydrationDecision::Failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_wait_until_the_exact_generation_is_hydrated() {
        let mut state = OverlayHydrationState::default();
        assert!(state.begin(7));
        assert_eq!(state.decision(7), OverlayHydrationDecision::Pending);
        assert!(state.complete(7));
        assert_eq!(state.decision(7), OverlayHydrationDecision::Ready);
    }

    #[test]
    fn a_new_generation_replaces_waiters_for_the_old_process() {
        let mut state = OverlayHydrationState::default();
        assert!(state.begin(7));
        assert!(state.begin(8));
        assert_eq!(state.decision(7), OverlayHydrationDecision::Replaced);
        assert_eq!(state.decision(8), OverlayHydrationDecision::Pending);
    }

    #[test]
    fn stale_ready_or_failure_cannot_mutate_the_new_generation() {
        let mut state = OverlayHydrationState::default();
        assert!(state.begin(7));
        assert!(state.begin(8));
        assert!(!state.complete(7));
        assert!(!state.fail(7));
        assert_eq!(state.decision(8), OverlayHydrationDecision::Pending);
    }

    #[test]
    fn hydration_failure_wakes_commands_to_retry_on_a_new_process() {
        let mut state = OverlayHydrationState::default();
        assert!(state.begin(7));
        assert!(state.fail(7));
        assert_eq!(state.decision(7), OverlayHydrationDecision::Failed);
        assert!(state.begin(8));
        assert_eq!(state.decision(7), OverlayHydrationDecision::Replaced);
        assert_eq!(state.decision(8), OverlayHydrationDecision::Pending);
    }

    #[test]
    fn duplicate_transitions_do_not_reopen_or_regress_a_generation() {
        let mut state = OverlayHydrationState::default();
        assert!(state.begin(7));
        assert!(!state.begin(7));
        assert!(state.complete(7));
        assert!(!state.complete(7));
        assert!(!state.begin(6));
        assert_eq!(state.decision(7), OverlayHydrationDecision::Ready);
    }

    #[test]
    fn matching_hydration_feedback_is_consumed_once() {
        let mut feedback = OverlayHydrationFeedback::default();
        feedback.begin(7);
        feedback.expect_visibility(7, false);

        assert!(feedback.consume_visibility(7, false));
        assert!(!feedback.consume_visibility(7, false));
    }

    #[test]
    fn mismatched_feedback_is_not_suppressed_and_clears_the_expectation() {
        let mut feedback = OverlayHydrationFeedback::default();
        feedback.begin(7);
        feedback.expect_visibility(7, false);

        assert!(!feedback.consume_visibility(7, true));
        assert!(!feedback.consume_visibility(7, false));
    }

    #[test]
    fn replacement_generation_clears_stale_feedback() {
        let mut feedback = OverlayHydrationFeedback::default();
        feedback.begin(7);
        feedback.expect_visibility(7, false);
        feedback.begin(8);

        assert!(!feedback.consume_visibility(7, false));
        assert!(!feedback.consume_visibility(8, false));
    }
}
