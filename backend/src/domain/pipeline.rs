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

    fn advance_to(p: &mut Pipeline, target: Stage) {
        while p.current_stage != target {
            p.advance().expect("advance_to failed");
        }
    }
}
