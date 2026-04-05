use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use beaver_builder::application::orchestrator::PipelineOrchestrator;
use beaver_builder::domain::pipeline::Stage;
use beaver_builder::llm::provider::{
    LlmError, LlmProvider, LlmRequest, LlmResponse, StreamChunk, Usage,
};
use beaver_builder::protocol::events::Event;
use beaver_builder::protocol::ops::Op;
use tokio::sync::{broadcast, mpsc};

// ── Mock LLM Provider ────────────────────────────────────────────────

struct MockLlmProvider;

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn chat(&self, _request: LlmRequest) -> Result<LlmResponse, LlmError> {
        Ok(LlmResponse {
            content: "Mock LLM response".into(),
            model: "mock-model".into(),
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            }),
        })
    }

    async fn chat_stream(
        &self,
        _request: LlmRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>, LlmError> {
        let (tx, rx) = mpsc::channel(1);
        tokio::spawn(async move {
            let _ = tx
                .send(StreamChunk {
                    delta: "mock stream".into(),
                    is_final: true,
                })
                .await;
        });
        Ok(rx)
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn setup() -> (
    mpsc::Sender<Op>,
    broadcast::Receiver<Event>,
    tokio::task::JoinHandle<()>,
) {
    let (sq_tx, sq_rx) = mpsc::channel::<Op>(64);
    let (eq_tx, eq_rx) = broadcast::channel::<Event>(256);
    let llm: Arc<dyn LlmProvider> = Arc::new(MockLlmProvider);
    let orchestrator = PipelineOrchestrator::new(sq_rx, eq_tx, llm);
    let handle = tokio::spawn(orchestrator.run());
    (sq_tx, eq_rx, handle)
}

async fn recv_event(rx: &mut broadcast::Receiver<Event>) -> Event {
    tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("timed out waiting for event")
        .expect("channel closed")
}

fn stage_of_transition(event: &Event) -> Option<Stage> {
    match event {
        Event::StageTransition { to, .. } => Some(*to),
        _ => None,
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn start_pipeline_emits_created_and_transition() {
    let (sq_tx, mut eq_rx, _handle) = setup();

    sq_tx
        .send(Op::StartPipeline {
            task_id: "t1".into(),
            workspace_id: "ws1".into(),
        })
        .await
        .expect("send");

    let evt1 = recv_event(&mut eq_rx).await;
    assert!(matches!(evt1, Event::PipelineCreated { .. }));

    let evt2 = recv_event(&mut eq_rx).await;
    assert_eq!(stage_of_transition(&evt2), Some(Stage::Planner));
}

#[tokio::test]
async fn full_pipeline_advancement() {
    let (sq_tx, mut eq_rx, _handle) = setup();

    // Start pipeline
    sq_tx
        .send(Op::StartPipeline {
            task_id: "t1".into(),
            workspace_id: "ws1".into(),
        })
        .await
        .expect("send");

    // Consume PipelineCreated + first transition to Planner
    let _ = recv_event(&mut eq_rx).await; // PipelineCreated
    let _ = recv_event(&mut eq_rx).await; // StageTransition -> Planner

    let pipeline_id = "p1".to_string();

    // Advance through: InitAgent, Coder, Reviewer, HumanReview, Deploy, Push, Completed
    let expected_stages = [
        Stage::InitAgent,
        Stage::Coder,
        Stage::Reviewer,
        Stage::HumanReview,
        Stage::Deploy,
        Stage::Push,
        Stage::Completed,
    ];

    for expected in expected_stages {
        sq_tx
            .send(Op::AdvanceStage {
                pipeline_id: pipeline_id.clone(),
            })
            .await
            .expect("send");

        // Some stages emit extra events (ApprovalRequired for HumanReview)
        loop {
            let evt = recv_event(&mut eq_rx).await;
            if let Some(stage) = stage_of_transition(&evt) {
                assert_eq!(stage, expected);
                break;
            }
            // Otherwise it's an extra event like ApprovalRequired — consume and continue
        }
    }
}

#[tokio::test]
async fn review_loop_and_auto_escalate() {
    let (sq_tx, mut eq_rx, _handle) = setup();

    // Start pipeline
    sq_tx
        .send(Op::StartPipeline {
            task_id: "t1".into(),
            workspace_id: "ws1".into(),
        })
        .await
        .expect("send");

    let _ = recv_event(&mut eq_rx).await; // PipelineCreated
    let _ = recv_event(&mut eq_rx).await; // -> Planner

    let pipeline_id = "p1".to_string();

    // Advance to Reviewer: Planner -> InitAgent -> Coder -> Reviewer
    for _ in 0..3 {
        sq_tx
            .send(Op::AdvanceStage {
                pipeline_id: pipeline_id.clone(),
            })
            .await
            .expect("send");
        loop {
            let evt = recv_event(&mut eq_rx).await;
            if matches!(evt, Event::StageTransition { .. }) {
                break;
            }
        }
    }

    // Now at Reviewer. Do 3 revert cycles.
    for i in 0..3 {
        sq_tx
            .send(Op::RevertStage {
                pipeline_id: pipeline_id.clone(),
                reason: format!("fix #{}", i + 1),
            })
            .await
            .expect("send");

        // Consume ReviewSubmitted + StageTransition(Coder)
        loop {
            let evt = recv_event(&mut eq_rx).await;
            if let Some(Stage::Coder) = stage_of_transition(&evt) {
                break;
            }
        }

        // Advance back to Reviewer
        sq_tx
            .send(Op::AdvanceStage {
                pipeline_id: pipeline_id.clone(),
            })
            .await
            .expect("send");
        loop {
            let evt = recv_event(&mut eq_rx).await;
            if let Some(Stage::Reviewer) = stage_of_transition(&evt) {
                break;
            }
        }
    }

    // 4th revert → auto-escalate
    sq_tx
        .send(Op::RevertStage {
            pipeline_id: pipeline_id.clone(),
            reason: "one more".into(),
        })
        .await
        .expect("send");

    // Expect Warning + ApprovalRequired + StageTransition(HumanReview)
    let mut saw_warning = false;
    let mut saw_approval = false;
    let mut saw_human_review = false;

    for _ in 0..5 {
        let evt = recv_event(&mut eq_rx).await;
        match &evt {
            Event::Warning { .. } => saw_warning = true,
            Event::ApprovalRequired { .. } => saw_approval = true,
            Event::StageTransition { to, .. } if *to == Stage::HumanReview => {
                saw_human_review = true;
            }
            _ => {}
        }
        if saw_warning && saw_approval && saw_human_review {
            break;
        }
    }

    assert!(saw_warning, "expected Warning event");
    assert!(saw_approval, "expected ApprovalRequired event");
    assert!(saw_human_review, "expected StageTransition to HumanReview");
}

#[tokio::test]
async fn approve_human_review() {
    let (sq_tx, mut eq_rx, _handle) = setup();

    sq_tx
        .send(Op::StartPipeline {
            task_id: "t1".into(),
            workspace_id: "ws1".into(),
        })
        .await
        .expect("send");

    let _ = recv_event(&mut eq_rx).await; // PipelineCreated
    let _ = recv_event(&mut eq_rx).await; // -> Planner

    let pipeline_id = "p1".to_string();

    // Advance to HumanReview (4 advances: Planner->Init->Coder->Reviewer->HumanReview)
    for _ in 0..4 {
        sq_tx
            .send(Op::AdvanceStage {
                pipeline_id: pipeline_id.clone(),
            })
            .await
            .expect("send");
        loop {
            let evt = recv_event(&mut eq_rx).await;
            if matches!(evt, Event::StageTransition { .. }) {
                break;
            }
        }
    }

    // Approve
    sq_tx
        .send(Op::ApproveHumanReview {
            pipeline_id: pipeline_id.clone(),
        })
        .await
        .expect("send");

    let evt = recv_event(&mut eq_rx).await;
    assert_eq!(stage_of_transition(&evt), Some(Stage::Deploy));
}

#[tokio::test]
async fn reject_human_review_reverts_to_coder() {
    let (sq_tx, mut eq_rx, _handle) = setup();

    sq_tx
        .send(Op::StartPipeline {
            task_id: "t1".into(),
            workspace_id: "ws1".into(),
        })
        .await
        .expect("send");

    let _ = recv_event(&mut eq_rx).await;
    let _ = recv_event(&mut eq_rx).await;

    let pipeline_id = "p1".to_string();

    // Advance to HumanReview
    for _ in 0..4 {
        sq_tx
            .send(Op::AdvanceStage {
                pipeline_id: pipeline_id.clone(),
            })
            .await
            .expect("send");
        loop {
            let evt = recv_event(&mut eq_rx).await;
            if matches!(evt, Event::StageTransition { .. }) {
                break;
            }
        }
    }

    // Reject
    sq_tx
        .send(Op::RejectHumanReview {
            pipeline_id: pipeline_id.clone(),
            reason: "needs more work".into(),
        })
        .await
        .expect("send");

    let evt = recv_event(&mut eq_rx).await;
    assert_eq!(stage_of_transition(&evt), Some(Stage::Coder));
}

#[tokio::test]
async fn interrupt_pipeline_transitions_to_failed() {
    let (sq_tx, mut eq_rx, _handle) = setup();

    sq_tx
        .send(Op::StartPipeline {
            task_id: "t1".into(),
            workspace_id: "ws1".into(),
        })
        .await
        .expect("send");

    let _ = recv_event(&mut eq_rx).await;
    let _ = recv_event(&mut eq_rx).await;

    let pipeline_id = "p1".to_string();

    sq_tx
        .send(Op::InterruptPipeline {
            pipeline_id: pipeline_id.clone(),
        })
        .await
        .expect("send");

    // Expect Warning + StageTransition(Failed)
    let mut saw_warning = false;
    let mut saw_failed = false;

    for _ in 0..3 {
        let evt = recv_event(&mut eq_rx).await;
        match &evt {
            Event::Warning { .. } => saw_warning = true,
            Event::StageTransition { to, .. } if *to == Stage::Failed => saw_failed = true,
            _ => {}
        }
        if saw_warning && saw_failed {
            break;
        }
    }

    assert!(saw_warning, "expected Warning event");
    assert!(saw_failed, "expected transition to Failed");
}
