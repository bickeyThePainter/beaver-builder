use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use thiserror::Error;

/// Maximum number of Coder <-> Reviewer iterations before forcing human review.
pub const MAX_REVIEW_ITERATIONS: u8 = 3;

// ---------------------------------------------------------------------------
// Stage -- the pipeline's position in the state machine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Created,
    IntentClarifier,
    InitAgent,
    Planner,
    Coder,
    Reviewer,
    HumanReview,
    Deploy,
    Push,
    Completed,
    Failed,
}

impl Stage {
    /// The "happy path" next stage (no loops, no failures).
    fn happy_next(self) -> Option<Stage> {
        match self {
            Stage::Created => Some(Stage::IntentClarifier),
            Stage::IntentClarifier => Some(Stage::InitAgent),
            Stage::InitAgent => Some(Stage::Planner),
            Stage::Planner => Some(Stage::Coder),
            Stage::Coder => Some(Stage::Reviewer),
            Stage::Reviewer => Some(Stage::HumanReview),
            Stage::HumanReview => Some(Stage::Deploy),
            Stage::Deploy => Some(Stage::Push),
            Stage::Push => Some(Stage::Completed),
            Stage::Completed | Stage::Failed => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Stage::Created => "Created",
            Stage::IntentClarifier => "Intent Clarifier",
            Stage::InitAgent => "Init Agent",
            Stage::Planner => "Planner",
            Stage::Coder => "Coder",
            Stage::Reviewer => "Reviewer",
            Stage::HumanReview => "Human Review",
            Stage::Deploy => "Deploy",
            Stage::Push => "Push",
            Stage::Completed => "Completed",
            Stage::Failed => "Failed",
        }
    }
}

// ---------------------------------------------------------------------------
// Transition -- a record of a stage change
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub from: Stage,
    pub to: Stage,
    pub timestamp: DateTime<Utc>,
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Pipeline -- the aggregate root
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: String,
    pub task_id: String,
    pub current_stage: Stage,
    pub review_iterations: u8,
    pub transitions: Vec<Transition>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum TransitionError {
    #[error("Cannot advance from {from:?} to {to:?}: invalid transition")]
    InvalidTransition { from: Stage, to: Stage },

    #[error("Review loop exhausted ({iterations} iterations). Must go to human review.")]
    ReviewLoopExhausted { iterations: u8 },

    #[error("Pipeline is in terminal state {0:?}")]
    TerminalState(Stage),
}

impl Pipeline {
    pub fn new(id: String, task_id: String) -> Self {
        Self {
            id,
            task_id,
            current_stage: Stage::Created,
            review_iterations: 0,
            transitions: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Advance to the next stage on the happy path.
    pub fn advance(&mut self) -> Result<Transition, TransitionError> {
        let next = self.current_stage.happy_next()
            .ok_or(TransitionError::TerminalState(self.current_stage))?;

        // Special case: if Reviewer is done and we'd go to HumanReview, that's normal advance
        self.apply_transition(next, None)
    }

    /// Revert from Reviewer back to Coder (review loop).
    pub fn revert_to_coder(&mut self, reason: String) -> Result<Transition, TransitionError> {
        match self.current_stage {
            Stage::Reviewer => {
                if self.review_iterations >= MAX_REVIEW_ITERATIONS {
                    return Err(TransitionError::ReviewLoopExhausted {
                        iterations: self.review_iterations,
                    });
                }
                self.review_iterations += 1;
                self.apply_transition(Stage::Coder, Some(reason))
            }
            Stage::HumanReview => {
                // Human rejected -- reset review counter and go back to Coder
                self.review_iterations = 0;
                self.apply_transition(Stage::Coder, Some(reason))
            }
            other => Err(TransitionError::InvalidTransition {
                from: other,
                to: Stage::Coder,
            }),
        }
    }

    /// Force-advance from Reviewer to HumanReview when loop is exhausted.
    pub fn force_human_review(&mut self) -> Result<Transition, TransitionError> {
        if self.current_stage != Stage::Reviewer {
            return Err(TransitionError::InvalidTransition {
                from: self.current_stage,
                to: Stage::HumanReview,
            });
        }
        self.apply_transition(Stage::HumanReview, Some("Review loop exhausted".into()))
    }

    /// Transition to Failed from any non-terminal state.
    pub fn fail(&mut self, reason: String) -> Result<Transition, TransitionError> {
        match self.current_stage {
            Stage::Completed | Stage::Failed => {
                Err(TransitionError::TerminalState(self.current_stage))
            }
            _ => self.apply_transition(Stage::Failed, Some(reason)),
        }
    }

    /// Current stage of the pipeline.
    pub fn current_stage(&self) -> Stage {
        self.current_stage
    }

    /// Check if the pipeline is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self.current_stage, Stage::Completed | Stage::Failed)
    }

    // -- internal --

    fn apply_transition(
        &mut self,
        to: Stage,
        reason: Option<String>,
    ) -> Result<Transition, TransitionError> {
        let transition = Transition {
            from: self.current_stage,
            to,
            timestamp: Utc::now(),
            reason,
        };
        self.current_stage = to;
        self.transitions.push(transition.clone());
        Ok(transition)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helper: advance pipeline to a specific stage on the happy path
    // -----------------------------------------------------------------------
    fn advance_to(p: &mut Pipeline, target: Stage) {
        while p.current_stage != target {
            p.advance().expect("advance_to helper failed");
        }
    }

    // -----------------------------------------------------------------------
    // Happy path: all valid transitions
    // -----------------------------------------------------------------------

    #[test]
    fn happy_path_advances_through_all_stages() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        let stages = [
            Stage::IntentClarifier,
            Stage::InitAgent,
            Stage::Planner,
            Stage::Coder,
            Stage::Reviewer,
            Stage::HumanReview,
            Stage::Deploy,
            Stage::Push,
            Stage::Completed,
        ];
        for expected in stages {
            let t = p.advance().unwrap();
            assert_eq!(t.to, expected);
            assert_eq!(p.current_stage, expected);
        }
        assert_eq!(p.transitions.len(), 9);
    }

    #[test]
    fn each_happy_path_transition_records_from_and_to() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        let t = p.advance().unwrap();
        assert_eq!(t.from, Stage::Created);
        assert_eq!(t.to, Stage::IntentClarifier);
        assert!(t.reason.is_none());
    }

    // -----------------------------------------------------------------------
    // Invalid transitions
    // -----------------------------------------------------------------------

    #[test]
    fn cannot_advance_from_completed() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Completed);
        let err = p.advance().unwrap_err();
        assert!(matches!(err, TransitionError::TerminalState(Stage::Completed)));
    }

    #[test]
    fn cannot_advance_from_failed() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        p.advance().unwrap();
        p.fail("broke".into()).unwrap();
        let err = p.advance().unwrap_err();
        assert!(matches!(err, TransitionError::TerminalState(Stage::Failed)));
    }

    #[test]
    fn revert_to_coder_invalid_from_planner() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Planner);
        let err = p.revert_to_coder("nope".into()).unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    #[test]
    fn revert_to_coder_invalid_from_coder() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Coder);
        let err = p.revert_to_coder("nope".into()).unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    #[test]
    fn revert_to_coder_invalid_from_deploy() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Deploy);
        let err = p.revert_to_coder("nope".into()).unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    #[test]
    fn force_human_review_invalid_from_coder() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Coder);
        let err = p.force_human_review().unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    #[test]
    fn force_human_review_invalid_from_human_review() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::HumanReview);
        let err = p.force_human_review().unwrap_err();
        assert!(matches!(err, TransitionError::InvalidTransition { .. }));
    }

    #[test]
    fn fail_from_completed_returns_terminal() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Completed);
        let err = p.fail("x".into()).unwrap_err();
        assert!(matches!(err, TransitionError::TerminalState(Stage::Completed)));
    }

    #[test]
    fn fail_from_failed_returns_terminal() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        p.advance().unwrap();
        p.fail("x".into()).unwrap();
        let err = p.fail("again".into()).unwrap_err();
        assert!(matches!(err, TransitionError::TerminalState(Stage::Failed)));
    }

    // -----------------------------------------------------------------------
    // Review loop cap and auto-escalation
    // -----------------------------------------------------------------------

    #[test]
    fn review_loop_increments_and_caps() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Reviewer);

        for i in 0..MAX_REVIEW_ITERATIONS {
            p.revert_to_coder(format!("fix #{}", i + 1)).unwrap();
            assert_eq!(p.current_stage, Stage::Coder);
            assert_eq!(p.review_iterations, i + 1);
            p.advance().unwrap(); // back to Reviewer
        }

        // 4th revert should fail with ReviewLoopExhausted
        let err = p.revert_to_coder("one more".into()).unwrap_err();
        assert!(matches!(err, TransitionError::ReviewLoopExhausted { iterations: 3 }));

        // Force to human review instead
        p.force_human_review().unwrap();
        assert_eq!(p.current_stage, Stage::HumanReview);
    }

    #[test]
    fn review_loop_advance_revert_advance_revert_advance_escalates() {
        // Scenario: Coder -> Reviewer -> Coder -> Reviewer -> Coder -> Reviewer -> (exhausted)
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Reviewer);

        // First iteration
        p.revert_to_coder("fix 1".into()).unwrap();
        p.advance().unwrap();
        assert_eq!(p.review_iterations, 1);

        // Second iteration
        p.revert_to_coder("fix 2".into()).unwrap();
        p.advance().unwrap();
        assert_eq!(p.review_iterations, 2);

        // Third iteration
        p.revert_to_coder("fix 3".into()).unwrap();
        p.advance().unwrap();
        assert_eq!(p.review_iterations, 3);

        // Fourth attempt must fail
        let err = p.revert_to_coder("fix 4".into()).unwrap_err();
        assert!(matches!(err, TransitionError::ReviewLoopExhausted { .. }));

        // Escalate
        p.force_human_review().unwrap();
        assert_eq!(p.current_stage, Stage::HumanReview);
    }

    // -----------------------------------------------------------------------
    // Human rejection resets review counter
    // -----------------------------------------------------------------------

    #[test]
    fn human_rejection_resets_review_counter() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::HumanReview);
        p.revert_to_coder("not good enough".into()).unwrap();
        assert_eq!(p.current_stage, Stage::Coder);
        assert_eq!(p.review_iterations, 0);
    }

    #[test]
    fn human_rejection_after_review_loop_resets_counter() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Reviewer);

        // Exhaust review loop
        for i in 0..MAX_REVIEW_ITERATIONS {
            p.revert_to_coder(format!("fix #{}", i + 1)).unwrap();
            p.advance().unwrap();
        }
        assert_eq!(p.review_iterations, MAX_REVIEW_ITERATIONS);

        // Force to human review
        p.force_human_review().unwrap();
        assert_eq!(p.current_stage, Stage::HumanReview);

        // Human rejects => counter resets
        p.revert_to_coder("needs rethinking".into()).unwrap();
        assert_eq!(p.review_iterations, 0);
        assert_eq!(p.current_stage, Stage::Coder);

        // Can now loop again from fresh
        p.advance().unwrap(); // Coder -> Reviewer
        p.revert_to_coder("still needs work".into()).unwrap();
        assert_eq!(p.review_iterations, 1);
    }

    // -----------------------------------------------------------------------
    // Fail from any active stage
    // -----------------------------------------------------------------------

    #[test]
    fn fail_from_any_active_stage() {
        let active_stages = [
            Stage::Created,
            Stage::IntentClarifier,
            Stage::InitAgent,
            Stage::Planner,
            Stage::Coder,
            Stage::Reviewer,
            Stage::HumanReview,
            Stage::Deploy,
            Stage::Push,
        ];
        for stage in active_stages {
            let mut p = Pipeline::new("p1".into(), "t1".into());
            advance_to(&mut p, stage);
            p.fail("something broke".into()).unwrap();
            assert_eq!(p.current_stage, Stage::Failed);
        }
    }

    // -----------------------------------------------------------------------
    // Transition history
    // -----------------------------------------------------------------------

    #[test]
    fn transition_history_records_all_transitions() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Reviewer);
        p.revert_to_coder("fix it".into()).unwrap();
        p.advance().unwrap(); // back to Reviewer

        // Created->IC->IA->Planner->Coder->Reviewer->Coder->Reviewer = 7
        assert_eq!(p.transitions.len(), 7);

        let last = p.transitions.last().unwrap();
        assert_eq!(last.from, Stage::Coder);
        assert_eq!(last.to, Stage::Reviewer);

        // The revert transition should have a reason
        let revert = &p.transitions[5];
        assert_eq!(revert.from, Stage::Reviewer);
        assert_eq!(revert.to, Stage::Coder);
        assert_eq!(revert.reason.as_deref(), Some("fix it"));
    }

    #[test]
    fn transition_timestamps_are_sequential() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Completed);

        for window in p.transitions.windows(2) {
            assert!(window[0].timestamp <= window[1].timestamp);
        }
    }

    // -----------------------------------------------------------------------
    // Pipeline::new defaults
    // -----------------------------------------------------------------------

    #[test]
    fn new_pipeline_starts_at_created() {
        let p = Pipeline::new("p1".into(), "t1".into());
        assert_eq!(p.current_stage, Stage::Created);
        assert_eq!(p.review_iterations, 0);
        assert!(p.transitions.is_empty());
        assert!(!p.is_terminal());
    }

    #[test]
    fn is_terminal_correct() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        assert!(!p.is_terminal());
        advance_to(&mut p, Stage::Completed);
        assert!(p.is_terminal());

        let mut p2 = Pipeline::new("p2".into(), "t2".into());
        p2.advance().unwrap();
        p2.fail("x".into()).unwrap();
        assert!(p2.is_terminal());
    }

    // -----------------------------------------------------------------------
    // Stage serialization
    // -----------------------------------------------------------------------

    #[test]
    fn stage_serializes_to_snake_case() {
        assert_eq!(serde_json::to_string(&Stage::IntentClarifier).unwrap(), "\"intent_clarifier\"");
        assert_eq!(serde_json::to_string(&Stage::HumanReview).unwrap(), "\"human_review\"");
        assert_eq!(serde_json::to_string(&Stage::InitAgent).unwrap(), "\"init_agent\"");
        assert_eq!(serde_json::to_string(&Stage::Created).unwrap(), "\"created\"");
        assert_eq!(serde_json::to_string(&Stage::Completed).unwrap(), "\"completed\"");
    }

    #[test]
    fn stage_deserializes_from_snake_case() {
        let s: Stage = serde_json::from_str("\"intent_clarifier\"").unwrap();
        assert_eq!(s, Stage::IntentClarifier);
        let s: Stage = serde_json::from_str("\"human_review\"").unwrap();
        assert_eq!(s, Stage::HumanReview);
    }
}
