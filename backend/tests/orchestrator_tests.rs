//! Integration tests for the PipelineOrchestrator.
//!
//! Creates an orchestrator with real channels, sends Ops, and verifies
//! the Events that come back.

use tokio::sync::{broadcast, mpsc};
use beaver_builder::protocol::ops::Op;
use beaver_builder::protocol::events::Event;
use beaver_builder::application::orchestrator::PipelineOrchestrator;

/// Helper: create an orchestrator and return (op_sender, event_receiver).
/// The orchestrator runs on a background task.
fn setup() -> (mpsc::Sender<Op>, broadcast::Receiver<Event>) {
    let (sq_tx, sq_rx) = mpsc::channel::<Op>(64);
    let (eq_tx, eq_rx) = broadcast::channel::<Event>(256);
    let orchestrator = PipelineOrchestrator::new(sq_rx, eq_tx);
    tokio::spawn(orchestrator.run());
    (sq_tx, eq_rx)
}

/// Helper: collect events until the channel is empty (with a short timeout).
async fn collect_events(rx: &mut broadcast::Receiver<Event>, count: usize) -> Vec<Event> {
    let mut events = Vec::new();
    for _ in 0..count {
        match tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await {
            Ok(Ok(evt)) => events.push(evt),
            _ => break,
        }
    }
    events
}

// ---------------------------------------------------------------------------
// StartPipeline
// ---------------------------------------------------------------------------

#[tokio::test]
async fn start_pipeline_emits_pipeline_created_and_stage_transition() {
    let (tx, mut rx) = setup();

    tx.send(Op::StartPipeline {
        task_id: "t1".into(),
        workspace_id: "ws1".into(),
    }).await.unwrap();

    let events = collect_events(&mut rx, 2).await;
    assert_eq!(events.len(), 2);

    // First event: PipelineCreated
    match &events[0] {
        Event::PipelineCreated { pipeline_id, task_id, stage } => {
            assert!(!pipeline_id.is_empty());
            assert_eq!(task_id, "t1");
            assert_eq!(*stage, beaver_builder::domain::pipeline::Stage::IntentClarifier);
        }
        other => panic!("Expected PipelineCreated, got: {:?}", other),
    }

    // Second event: StageTransition (Created -> IntentClarifier)
    match &events[1] {
        Event::StageTransition { from, to, .. } => {
            assert_eq!(*from, beaver_builder::domain::pipeline::Stage::Created);
            assert_eq!(*to, beaver_builder::domain::pipeline::Stage::IntentClarifier);
        }
        other => panic!("Expected StageTransition, got: {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// AdvanceStage through full pipeline
// ---------------------------------------------------------------------------

#[tokio::test]
async fn advance_through_full_pipeline() {
    let (tx, mut rx) = setup();

    // Start pipeline
    tx.send(Op::StartPipeline {
        task_id: "t1".into(),
        workspace_id: "ws1".into(),
    }).await.unwrap();

    let events = collect_events(&mut rx, 2).await;
    let pipeline_id = match &events[0] {
        Event::PipelineCreated { pipeline_id, .. } => pipeline_id.clone(),
        _ => panic!("Expected PipelineCreated"),
    };

    // Advance through remaining stages:
    // IntentClarifier -> InitAgent -> Planner -> Coder -> Reviewer -> HumanReview -> Deploy -> Push -> Completed
    let expected_stages = [
        (beaver_builder::domain::pipeline::Stage::IntentClarifier, beaver_builder::domain::pipeline::Stage::InitAgent),
        (beaver_builder::domain::pipeline::Stage::InitAgent, beaver_builder::domain::pipeline::Stage::Planner),
        (beaver_builder::domain::pipeline::Stage::Planner, beaver_builder::domain::pipeline::Stage::Coder),
        (beaver_builder::domain::pipeline::Stage::Coder, beaver_builder::domain::pipeline::Stage::Reviewer),
        (beaver_builder::domain::pipeline::Stage::Reviewer, beaver_builder::domain::pipeline::Stage::HumanReview),
        (beaver_builder::domain::pipeline::Stage::HumanReview, beaver_builder::domain::pipeline::Stage::Deploy),
        (beaver_builder::domain::pipeline::Stage::Deploy, beaver_builder::domain::pipeline::Stage::Push),
        (beaver_builder::domain::pipeline::Stage::Push, beaver_builder::domain::pipeline::Stage::Completed),
    ];

    for (expected_from, expected_to) in &expected_stages {
        tx.send(Op::AdvanceStage { pipeline_id: pipeline_id.clone() }).await.unwrap();
        let events = collect_events(&mut rx, 1).await;
        assert_eq!(events.len(), 1, "Expected 1 event for advance");
        match &events[0] {
            Event::StageTransition { from, to, .. } => {
                assert_eq!(from, expected_from);
                assert_eq!(to, expected_to);
            }
            other => panic!("Expected StageTransition, got: {:?}", other),
        }
    }
}

// ---------------------------------------------------------------------------
// Coder <-> Reviewer loop via RevertStage
// ---------------------------------------------------------------------------

#[tokio::test]
async fn coder_reviewer_loop_via_revert_stage() {
    let (tx, mut rx) = setup();

    // Start and advance to Reviewer
    tx.send(Op::StartPipeline { task_id: "t1".into(), workspace_id: "ws1".into() }).await.unwrap();
    let events = collect_events(&mut rx, 2).await;
    let pipeline_id = match &events[0] {
        Event::PipelineCreated { pipeline_id, .. } => pipeline_id.clone(),
        _ => panic!("Expected PipelineCreated"),
    };

    // Advance: IC -> IA -> Planner -> Coder -> Reviewer (4 advances)
    for _ in 0..4 {
        tx.send(Op::AdvanceStage { pipeline_id: pipeline_id.clone() }).await.unwrap();
        collect_events(&mut rx, 1).await;
    }

    // Now at Reviewer. Do 3 revert+advance cycles.
    for i in 0..3 {
        tx.send(Op::RevertStage {
            pipeline_id: pipeline_id.clone(),
            reason: format!("Fix #{}", i + 1),
        }).await.unwrap();
        let events = collect_events(&mut rx, 1).await;
        match &events[0] {
            Event::StageTransition { from, to, .. } => {
                assert_eq!(*from, beaver_builder::domain::pipeline::Stage::Reviewer);
                assert_eq!(*to, beaver_builder::domain::pipeline::Stage::Coder);
            }
            other => panic!("Expected StageTransition Reviewer->Coder, got: {:?}", other),
        }

        // Advance back to Reviewer
        tx.send(Op::AdvanceStage { pipeline_id: pipeline_id.clone() }).await.unwrap();
        let events = collect_events(&mut rx, 1).await;
        match &events[0] {
            Event::StageTransition { from, to, .. } => {
                assert_eq!(*from, beaver_builder::domain::pipeline::Stage::Coder);
                assert_eq!(*to, beaver_builder::domain::pipeline::Stage::Reviewer);
            }
            other => panic!("Expected StageTransition Coder->Reviewer, got: {:?}", other),
        }
    }

    // 4th revert should trigger auto-escalation: Warning + StageTransition(Reviewer->HumanReview) + ApprovalRequired
    tx.send(Op::RevertStage {
        pipeline_id: pipeline_id.clone(),
        reason: "One more".into(),
    }).await.unwrap();

    let events = collect_events(&mut rx, 3).await;
    assert!(events.len() >= 2, "Expected at least Warning + StageTransition, got {}", events.len());

    // Should contain a Warning about exhausted loop
    let has_warning = events.iter().any(|e| matches!(e, Event::Warning { message, .. } if message.contains("exhausted")));
    assert!(has_warning, "Expected warning about exhausted review loop");

    // Should contain StageTransition to HumanReview
    let has_transition = events.iter().any(|e| matches!(
        e,
        Event::StageTransition { to, .. } if *to == beaver_builder::domain::pipeline::Stage::HumanReview
    ));
    assert!(has_transition, "Expected transition to HumanReview");
}

// ---------------------------------------------------------------------------
// InterruptPipeline
// ---------------------------------------------------------------------------

#[tokio::test]
async fn interrupt_pipeline_emits_warning() {
    let (tx, mut rx) = setup();

    tx.send(Op::StartPipeline { task_id: "t1".into(), workspace_id: "ws1".into() }).await.unwrap();
    let events = collect_events(&mut rx, 2).await;
    let pipeline_id = match &events[0] {
        Event::PipelineCreated { pipeline_id, .. } => pipeline_id.clone(),
        _ => panic!("Expected PipelineCreated"),
    };

    tx.send(Op::InterruptPipeline { pipeline_id: pipeline_id.clone() }).await.unwrap();
    let events = collect_events(&mut rx, 1).await;

    match &events[0] {
        Event::Warning { pipeline_id: pid, message } => {
            assert_eq!(pid.as_deref(), Some(pipeline_id.as_str()));
            assert!(message.contains("interrupted") || message.contains("Interrupted"));
        }
        other => panic!("Expected Warning, got: {:?}", other),
    }

    // Further advance should fail (pipeline is Failed)
    tx.send(Op::AdvanceStage { pipeline_id: pipeline_id.clone() }).await.unwrap();
    let events = collect_events(&mut rx, 1).await;
    match &events[0] {
        Event::Error { .. } => {} // expected
        other => panic!("Expected Error after interrupt, got: {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Non-existent pipeline
// ---------------------------------------------------------------------------

#[tokio::test]
async fn advance_nonexistent_pipeline_emits_error() {
    let (tx, mut rx) = setup();

    tx.send(Op::AdvanceStage { pipeline_id: "nonexistent".into() }).await.unwrap();
    let events = collect_events(&mut rx, 1).await;

    match &events[0] {
        Event::Error { code, message, .. } => {
            assert_eq!(code, "op_failed");
            assert!(message.contains("not found"));
        }
        other => panic!("Expected Error, got: {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// ApproveHumanReview
// ---------------------------------------------------------------------------

#[tokio::test]
async fn approve_human_review_advances_to_deploy() {
    let (tx, mut rx) = setup();

    // Start and advance to HumanReview
    tx.send(Op::StartPipeline { task_id: "t1".into(), workspace_id: "ws1".into() }).await.unwrap();
    let events = collect_events(&mut rx, 2).await;
    let pipeline_id = match &events[0] {
        Event::PipelineCreated { pipeline_id, .. } => pipeline_id.clone(),
        _ => panic!("Expected PipelineCreated"),
    };

    // Advance: IC -> IA -> Planner -> Coder -> Reviewer -> HumanReview (5 advances)
    for _ in 0..5 {
        tx.send(Op::AdvanceStage { pipeline_id: pipeline_id.clone() }).await.unwrap();
        collect_events(&mut rx, 1).await;
    }

    // Now at HumanReview. Approve.
    tx.send(Op::ApproveHumanReview { pipeline_id: pipeline_id.clone() }).await.unwrap();
    let events = collect_events(&mut rx, 1).await;

    match &events[0] {
        Event::StageTransition { from, to, .. } => {
            assert_eq!(*from, beaver_builder::domain::pipeline::Stage::HumanReview);
            assert_eq!(*to, beaver_builder::domain::pipeline::Stage::Deploy);
        }
        other => panic!("Expected StageTransition to Deploy, got: {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// RejectHumanReview
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reject_human_review_reverts_to_coder() {
    let (tx, mut rx) = setup();

    tx.send(Op::StartPipeline { task_id: "t1".into(), workspace_id: "ws1".into() }).await.unwrap();
    let events = collect_events(&mut rx, 2).await;
    let pipeline_id = match &events[0] {
        Event::PipelineCreated { pipeline_id, .. } => pipeline_id.clone(),
        _ => panic!("Expected PipelineCreated"),
    };

    // Advance to HumanReview
    for _ in 0..5 {
        tx.send(Op::AdvanceStage { pipeline_id: pipeline_id.clone() }).await.unwrap();
        collect_events(&mut rx, 1).await;
    }

    // Reject
    tx.send(Op::RejectHumanReview {
        pipeline_id: pipeline_id.clone(),
        reason: "Needs rework".into(),
    }).await.unwrap();
    let events = collect_events(&mut rx, 1).await;

    match &events[0] {
        Event::StageTransition { from, to, .. } => {
            assert_eq!(*from, beaver_builder::domain::pipeline::Stage::HumanReview);
            assert_eq!(*to, beaver_builder::domain::pipeline::Stage::Coder);
        }
        other => panic!("Expected StageTransition to Coder, got: {:?}", other),
    }
}
