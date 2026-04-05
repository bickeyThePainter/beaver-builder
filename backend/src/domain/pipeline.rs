use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_REVIEW_ITERATIONS: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Created,
    Planner,
    InitAgent,
    Coder,
    Reviewer,
    HumanReview,
    Deploy,
    Push,
    Completed,
    Failed,
}

impl Stage {
    /// Returns the next stage on the happy path, or None if terminal.
    pub fn happy_next(self) -> Option<Stage> {
        match self {
            Stage::Created => Some(Stage::Planner),
            Stage::Planner => Some(Stage::InitAgent),
            Stage::InitAgent => Some(Stage::Coder),
            Stage::Coder => Some(Stage::Reviewer),
            Stage::Reviewer => Some(Stage::HumanReview),
            Stage::HumanReview => Some(Stage::Deploy),
            Stage::Deploy => Some(Stage::Push),
            Stage::Push => Some(Stage::Completed),
            Stage::Completed | Stage::Failed => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub from: Stage,
    pub to: Stage,
    pub timestamp: DateTime<Utc>,
    pub reason: Option<String>,
}

#[derive(Debug, Error)]
pub enum TransitionError {
    #[error("invalid transition from {from:?} to {to:?}")]
    InvalidTransition { from: Stage, to: Stage },

    #[error("review loop exhausted after {iterations} iterations")]
    ReviewLoopExhausted { iterations: u8 },

    #[error("cannot transition from terminal state {0:?}")]
    TerminalState(Stage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: String,
    pub task_id: String,
    pub current_stage: Stage,
    pub review_iterations: u8,
    pub transitions: Vec<Transition>,
    pub created_at: DateTime<Utc>,
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
        let next = self
            .current_stage
            .happy_next()
            .ok_or(TransitionError::TerminalState(self.current_stage))?;

        let transition = Transition {
            from: self.current_stage,
            to: next,
            timestamp: Utc::now(),
            reason: None,
        };

        self.current_stage = next;
        self.transitions.push(transition.clone());
        Ok(transition)
    }

    /// Revert from Reviewer to Coder (review rejection).
    /// From HumanReview, this resets the review counter.
    pub fn revert_to_coder(&mut self, reason: String) -> Result<Transition, TransitionError> {
        match self.current_stage {
            Stage::Reviewer => {
                if self.review_iterations >= MAX_REVIEW_ITERATIONS {
                    return Err(TransitionError::ReviewLoopExhausted {
                        iterations: self.review_iterations,
                    });
                }
                self.review_iterations += 1;
                let transition = Transition {
                    from: Stage::Reviewer,
                    to: Stage::Coder,
                    timestamp: Utc::now(),
                    reason: Some(reason),
                };
                self.current_stage = Stage::Coder;
                self.transitions.push(transition.clone());
                Ok(transition)
            }
            Stage::HumanReview => {
                self.review_iterations = 0;
                let transition = Transition {
                    from: Stage::HumanReview,
                    to: Stage::Coder,
                    timestamp: Utc::now(),
                    reason: Some(reason),
                };
                self.current_stage = Stage::Coder;
                self.transitions.push(transition.clone());
                Ok(transition)
            }
            other => Err(TransitionError::InvalidTransition {
                from: other,
                to: Stage::Coder,
            }),
        }
    }

    /// Force transition from Reviewer to HumanReview (auto-escalation).
    pub fn force_human_review(&mut self) -> Result<Transition, TransitionError> {
        if self.current_stage != Stage::Reviewer {
            return Err(TransitionError::InvalidTransition {
                from: self.current_stage,
                to: Stage::HumanReview,
            });
        }
        let transition = Transition {
            from: Stage::Reviewer,
            to: Stage::HumanReview,
            timestamp: Utc::now(),
            reason: Some("auto-escalated after max review iterations".into()),
        };
        self.current_stage = Stage::HumanReview;
        self.transitions.push(transition.clone());
        Ok(transition)
    }

    /// Fail the pipeline from any non-terminal stage.
    pub fn fail(&mut self, reason: String) -> Result<Transition, TransitionError> {
        match self.current_stage {
            Stage::Completed | Stage::Failed => {
                Err(TransitionError::TerminalState(self.current_stage))
            }
            _ => {
                let transition = Transition {
                    from: self.current_stage,
                    to: Stage::Failed,
                    timestamp: Utc::now(),
                    reason: Some(reason),
                };
                self.current_stage = Stage::Failed;
                self.transitions.push(transition.clone());
                Ok(transition)
            }
        }
    }

    pub fn current_stage(&self) -> Stage {
        self.current_stage
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Existing tests ──────────────────────────────────────────────────

    #[test]
    fn happy_path_advances_through_all_stages() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        let stages = [
            Stage::Planner,
            Stage::InitAgent,
            Stage::Coder,
            Stage::Reviewer,
            Stage::HumanReview,
            Stage::Deploy,
            Stage::Push,
            Stage::Completed,
        ];
        for expected in stages {
            let t = p.advance().expect("advance should succeed");
            assert_eq!(t.to, expected);
        }
    }

    #[test]
    fn review_loop_caps_at_3() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Reviewer);
        for i in 0..3 {
            p.revert_to_coder(format!("fix #{}", i + 1))
                .expect("revert should succeed");
            p.advance().expect("advance back to Reviewer"); // back to Reviewer
        }
        let err = p.revert_to_coder("one more".into()).unwrap_err();
        assert!(matches!(err, TransitionError::ReviewLoopExhausted { .. }));
    }

    #[test]
    fn human_rejection_resets_counter() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::HumanReview);
        p.revert_to_coder("rejected".into())
            .expect("revert from human review should succeed");
        assert_eq!(p.review_iterations, 0);
        assert_eq!(p.current_stage, Stage::Coder);
    }

    #[test]
    fn cannot_advance_from_completed() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Completed);
        assert!(matches!(
            p.advance().unwrap_err(),
            TransitionError::TerminalState(_)
        ));
    }

    #[test]
    fn fail_from_any_active_stage() {
        for stage in [
            Stage::Planner,
            Stage::Coder,
            Stage::Reviewer,
            Stage::Deploy,
        ] {
            let mut p = Pipeline::new("p1".into(), "t1".into());
            advance_to(&mut p, stage);
            p.fail("broke".into()).expect("fail should succeed");
            assert_eq!(p.current_stage, Stage::Failed);
        }
    }

    #[test]
    fn stage_serializes_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&Stage::HumanReview).expect("serialize"),
            "\"human_review\""
        );
        assert_eq!(
            serde_json::to_string(&Stage::InitAgent).expect("serialize"),
            "\"init_agent\""
        );
    }

    // ── NEW: Invalid transition tests ───────────────────────────────────

    #[test]
    fn cannot_advance_from_failed() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Planner);
        p.fail("broke".into()).expect("fail should succeed");
        assert_eq!(p.current_stage, Stage::Failed);
        assert!(matches!(
            p.advance().unwrap_err(),
            TransitionError::TerminalState(Stage::Failed)
        ));
    }

    #[test]
    fn cannot_revert_from_planner() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Planner);
        let err = p.revert_to_coder("bad".into()).unwrap_err();
        assert!(matches!(
            err,
            TransitionError::InvalidTransition {
                from: Stage::Planner,
                to: Stage::Coder,
            }
        ));
    }

    #[test]
    fn cannot_revert_from_init_agent() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::InitAgent);
        let err = p.revert_to_coder("bad".into()).unwrap_err();
        assert!(matches!(
            err,
            TransitionError::InvalidTransition {
                from: Stage::InitAgent,
                to: Stage::Coder,
            }
        ));
    }

    #[test]
    fn cannot_revert_from_coder() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Coder);
        let err = p.revert_to_coder("bad".into()).unwrap_err();
        assert!(matches!(
            err,
            TransitionError::InvalidTransition {
                from: Stage::Coder,
                to: Stage::Coder,
            }
        ));
    }

    #[test]
    fn cannot_revert_from_deploy() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Deploy);
        let err = p.revert_to_coder("bad".into()).unwrap_err();
        assert!(matches!(
            err,
            TransitionError::InvalidTransition {
                from: Stage::Deploy,
                to: Stage::Coder,
            }
        ));
    }

    #[test]
    fn cannot_revert_from_push() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Push);
        let err = p.revert_to_coder("bad".into()).unwrap_err();
        assert!(matches!(
            err,
            TransitionError::InvalidTransition {
                from: Stage::Push,
                to: Stage::Coder,
            }
        ));
    }

    #[test]
    fn cannot_revert_from_completed() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Completed);
        let err = p.revert_to_coder("bad".into()).unwrap_err();
        assert!(matches!(
            err,
            TransitionError::InvalidTransition {
                from: Stage::Completed,
                to: Stage::Coder,
            }
        ));
    }

    #[test]
    fn cannot_revert_from_failed() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Planner);
        p.fail("broke".into()).expect("fail");
        let err = p.revert_to_coder("bad".into()).unwrap_err();
        assert!(matches!(
            err,
            TransitionError::InvalidTransition {
                from: Stage::Failed,
                to: Stage::Coder,
            }
        ));
    }

    #[test]
    fn cannot_fail_from_completed() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Completed);
        let err = p.fail("reason".into()).unwrap_err();
        assert!(matches!(err, TransitionError::TerminalState(Stage::Completed)));
    }

    #[test]
    fn cannot_fail_from_failed() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Planner);
        p.fail("first".into()).expect("fail");
        let err = p.fail("second".into()).unwrap_err();
        assert!(matches!(err, TransitionError::TerminalState(Stage::Failed)));
    }

    #[test]
    fn cannot_force_human_review_from_non_reviewer() {
        for stage in [
            Stage::Planner,
            Stage::InitAgent,
            Stage::Coder,
            Stage::HumanReview,
            Stage::Deploy,
            Stage::Push,
        ] {
            let mut p = Pipeline::new("p1".into(), "t1".into());
            advance_to(&mut p, stage);
            let err = p.force_human_review().unwrap_err();
            assert!(
                matches!(err, TransitionError::InvalidTransition { .. }),
                "expected InvalidTransition for force_human_review from {:?}",
                stage
            );
        }
    }

    // ── NEW: Transition history recording ───────────────────────────────

    #[test]
    fn transition_history_recorded_correctly() {
        let mut p = Pipeline::new("p1".into(), "t1".into());

        // Advance 3 times: Created->Planner, Planner->InitAgent, InitAgent->Coder
        p.advance().expect("advance 1");
        p.advance().expect("advance 2");
        p.advance().expect("advance 3");

        assert_eq!(p.transitions.len(), 3);

        assert_eq!(p.transitions[0].from, Stage::Created);
        assert_eq!(p.transitions[0].to, Stage::Planner);
        assert!(p.transitions[0].reason.is_none());

        assert_eq!(p.transitions[1].from, Stage::Planner);
        assert_eq!(p.transitions[1].to, Stage::InitAgent);

        assert_eq!(p.transitions[2].from, Stage::InitAgent);
        assert_eq!(p.transitions[2].to, Stage::Coder);
    }

    #[test]
    fn revert_transition_records_reason() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Reviewer);
        let initial_count = p.transitions.len();

        p.revert_to_coder("missing tests".into()).expect("revert");

        assert_eq!(p.transitions.len(), initial_count + 1);
        let last = p.transitions.last().unwrap();
        assert_eq!(last.from, Stage::Reviewer);
        assert_eq!(last.to, Stage::Coder);
        assert_eq!(last.reason.as_deref(), Some("missing tests"));
    }

    #[test]
    fn fail_transition_records_reason() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Coder);
        let initial_count = p.transitions.len();

        p.fail("build error".into()).expect("fail");

        assert_eq!(p.transitions.len(), initial_count + 1);
        let last = p.transitions.last().unwrap();
        assert_eq!(last.from, Stage::Coder);
        assert_eq!(last.to, Stage::Failed);
        assert_eq!(last.reason.as_deref(), Some("build error"));
    }

    #[test]
    fn force_human_review_records_reason() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Reviewer);
        let initial_count = p.transitions.len();

        p.force_human_review().expect("force_human_review");

        assert_eq!(p.transitions.len(), initial_count + 1);
        let last = p.transitions.last().unwrap();
        assert_eq!(last.from, Stage::Reviewer);
        assert_eq!(last.to, Stage::HumanReview);
        assert!(last.reason.is_some());
    }

    // ── NEW: Timestamp ordering ─────────────────────────────────────────

    #[test]
    fn timestamps_are_monotonically_increasing() {
        let mut p = Pipeline::new("p1".into(), "t1".into());

        // Advance through 5 transitions
        for _ in 0..5 {
            p.advance().expect("advance");
        }

        assert_eq!(p.transitions.len(), 5);

        for i in 1..p.transitions.len() {
            assert!(
                p.transitions[i].timestamp >= p.transitions[i - 1].timestamp,
                "timestamp at index {} ({}) should be >= index {} ({})",
                i,
                p.transitions[i].timestamp,
                i - 1,
                p.transitions[i - 1].timestamp,
            );
        }
    }

    #[test]
    fn timestamps_across_reverts_are_ordered() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Reviewer);
        p.revert_to_coder("fix".into()).expect("revert");
        p.advance().expect("advance");

        for i in 1..p.transitions.len() {
            assert!(
                p.transitions[i].timestamp >= p.transitions[i - 1].timestamp,
                "revert timestamps must be ordered"
            );
        }
    }

    // ── NEW: Auto-escalation (force_human_review from Reviewer) ─────────

    #[test]
    fn force_human_review_from_reviewer_succeeds() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Reviewer);

        let t = p.force_human_review().expect("force_human_review should succeed");
        assert_eq!(t.from, Stage::Reviewer);
        assert_eq!(t.to, Stage::HumanReview);
        assert_eq!(p.current_stage, Stage::HumanReview);
    }

    #[test]
    fn auto_escalation_after_exhausted_review_loop() {
        let mut p = Pipeline::new("p1".into(), "t1".into());
        advance_to(&mut p, Stage::Reviewer);

        // Exhaust the 3 review iterations
        for i in 0..3 {
            p.revert_to_coder(format!("fix #{}", i + 1)).expect("revert");
            p.advance().expect("advance back to Reviewer");
        }

        // 4th revert should fail
        let err = p.revert_to_coder("one more".into()).unwrap_err();
        assert!(matches!(err, TransitionError::ReviewLoopExhausted { iterations: 3 }));

        // Now force_human_review should succeed (the orchestrator does this)
        let t = p.force_human_review().expect("force_human_review after exhaustion");
        assert_eq!(t.to, Stage::HumanReview);
        assert_eq!(p.current_stage, Stage::HumanReview);
    }

    // ── NEW: Fail from ALL active stages (expanded) ─────────────────────

    #[test]
    fn fail_from_all_non_terminal_stages() {
        let active_stages = [
            Stage::Planner,
            Stage::InitAgent,
            Stage::Coder,
            Stage::Reviewer,
            Stage::HumanReview,
            Stage::Deploy,
            Stage::Push,
        ];
        for stage in active_stages {
            let mut p = Pipeline::new("p1".into(), "t1".into());
            advance_to(&mut p, stage);
            p.fail(format!("fail from {:?}", stage)).expect("fail should succeed");
            assert_eq!(
                p.current_stage,
                Stage::Failed,
                "expected Failed after failing from {:?}",
                stage
            );
        }
    }

    // ── NEW: Stage serialization (all variants) ─────────────────────────

    #[test]
    fn all_stages_serialize_to_snake_case() {
        let cases = [
            (Stage::Created, "\"created\""),
            (Stage::Planner, "\"planner\""),
            (Stage::InitAgent, "\"init_agent\""),
            (Stage::Coder, "\"coder\""),
            (Stage::Reviewer, "\"reviewer\""),
            (Stage::HumanReview, "\"human_review\""),
            (Stage::Deploy, "\"deploy\""),
            (Stage::Push, "\"push\""),
            (Stage::Completed, "\"completed\""),
            (Stage::Failed, "\"failed\""),
        ];
        for (stage, expected) in cases {
            let json = serde_json::to_string(&stage).expect("serialize");
            assert_eq!(json, expected, "Stage::{:?} serialization mismatch", stage);
        }
    }

    #[test]
    fn all_stages_deserialize_from_snake_case() {
        let cases = [
            ("\"created\"", Stage::Created),
            ("\"planner\"", Stage::Planner),
            ("\"init_agent\"", Stage::InitAgent),
            ("\"coder\"", Stage::Coder),
            ("\"reviewer\"", Stage::Reviewer),
            ("\"human_review\"", Stage::HumanReview),
            ("\"deploy\"", Stage::Deploy),
            ("\"push\"", Stage::Push),
            ("\"completed\"", Stage::Completed),
            ("\"failed\"", Stage::Failed),
        ];
        for (json, expected) in cases {
            let stage: Stage = serde_json::from_str(json).expect("deserialize");
            assert_eq!(stage, expected, "deserialization mismatch for {json}");
        }
    }

    #[test]
    fn invalid_stage_string_rejected() {
        let result = serde_json::from_str::<Stage>("\"not_a_stage\"");
        assert!(result.is_err(), "expected deserialization error for invalid stage");
    }

    // ── NEW: Pipeline::new initial state ────────────────────────────────

    #[test]
    fn new_pipeline_starts_at_created() {
        let p = Pipeline::new("p1".into(), "t1".into());
        assert_eq!(p.current_stage, Stage::Created);
        assert_eq!(p.review_iterations, 0);
        assert!(p.transitions.is_empty());
        assert_eq!(p.id, "p1");
        assert_eq!(p.task_id, "t1");
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    fn advance_to(p: &mut Pipeline, target: Stage) {
        while p.current_stage != target {
            p.advance().expect("advance_to failed");
        }
    }
}
