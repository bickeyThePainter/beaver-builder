use beaver_builder::domain::pipeline::Stage;
use beaver_builder::protocol::events::Event;
use beaver_builder::protocol::messages::WsMessage;
use beaver_builder::protocol::ops::Op;
use chrono::Utc;

// ── Op round-trip tests ──────────────────────────────────────────────

#[test]
fn round_trip_all_8_op_variants() {
    let ops = vec![
        Op::UserMessage {
            task_id: "task-001".into(),
            content: "Build a REST API for managing blog posts".into(),
        },
        Op::StartPipeline {
            task_id: "task-001".into(),
            workspace_id: "ws-001".into(),
        },
        Op::AdvanceStage {
            pipeline_id: "pipe-001".into(),
        },
        Op::RevertStage {
            pipeline_id: "pipe-001".into(),
            reason: "Missing error handling".into(),
        },
        Op::ApproveHumanReview {
            pipeline_id: "pipe-001".into(),
        },
        Op::RejectHumanReview {
            pipeline_id: "pipe-001".into(),
            reason: "Tests insufficient".into(),
        },
        Op::Deploy {
            pipeline_id: "pipe-001".into(),
            environment: "staging".into(),
        },
        Op::InterruptPipeline {
            pipeline_id: "pipe-001".into(),
        },
    ];

    assert_eq!(ops.len(), 8, "must test all 8 Op variants");

    for op in &ops {
        let json = serde_json::to_string(op).expect("serialize Op");
        let parsed: Op = serde_json::from_str(&json).expect("deserialize Op");
        let re_json = serde_json::to_string(&parsed).expect("re-serialize Op");
        assert_eq!(json, re_json, "round-trip mismatch for Op");
    }
}

// ── Event round-trip tests ───────────────────────────────────────────

#[test]
fn round_trip_all_10_event_variants() {
    let events = vec![
        Event::PipelineCreated {
            pipeline_id: "pipe-001".into(),
            task_id: "task-001".into(),
            stage: Stage::Planner,
        },
        Event::StageTransition {
            pipeline_id: "pipe-001".into(),
            from: Stage::Created,
            to: Stage::Planner,
            timestamp: Utc::now(),
        },
        Event::AgentOutput {
            pipeline_id: "pipe-001".into(),
            stage: Stage::Planner,
            delta: "Here is the plan...".into(),
            is_final: true,
        },
        Event::ToolExecution {
            pipeline_id: "pipe-001".into(),
            tool: "create_file".into(),
            params: serde_json::json!({"path": "src/main.rs", "content": "fn main() {}"}),
            result: serde_json::json!({"ok": true}),
            duration_ms: 150,
        },
        Event::ApprovalRequired {
            pipeline_id: "pipe-001".into(),
            task_id: "task-001".into(),
            summary: "Implementation complete. Ready for review.".into(),
        },
        Event::ReviewSubmitted {
            pipeline_id: "pipe-001".into(),
            verdict: "rejected".into(),
            iteration: 2,
        },
        Event::DeployStatus {
            pipeline_id: "pipe-001".into(),
            status: "success".into(),
            url: Some("https://staging.example.com".into()),
        },
        Event::PushComplete {
            pipeline_id: "pipe-001".into(),
            remote: "origin".into(),
            sha: "a1b2c3d4e5f6".into(),
        },
        Event::Error {
            pipeline_id: Some("pipe-001".into()),
            code: "invalid_transition".into(),
            message: "Cannot advance from completed stage".into(),
        },
        Event::Warning {
            pipeline_id: "pipe-001".into(),
            message: "Review loop exhausted after 3 iterations".into(),
        },
    ];

    assert_eq!(events.len(), 10, "must test all 10 Event variants");

    for event in &events {
        let json = serde_json::to_string(event).expect("serialize Event");
        let parsed: Event = serde_json::from_str(&json).expect("deserialize Event");
        let re_json = serde_json::to_string(&parsed).expect("re-serialize Event");
        assert_eq!(json, re_json, "round-trip mismatch for Event");
    }
}

// ── Fixture file round-trip tests ────────────────────────────────────

#[test]
fn fixture_ops_json_round_trip() {
    let fixture = std::fs::read_to_string("../tests/fixtures/ops.json")
        .expect("read ops.json fixture");
    let ops: Vec<Op> = serde_json::from_str(&fixture).expect("deserialize ops.json");

    assert_eq!(ops.len(), 8, "ops.json should contain all 8 Op variants");

    // Verify each can be serialized and deserialized
    for op in &ops {
        let json = serde_json::to_string(op).expect("serialize Op from fixture");
        let parsed: Op = serde_json::from_str(&json).expect("deserialize Op from fixture");
        let re_json = serde_json::to_string(&parsed).expect("re-serialize Op from fixture");
        assert_eq!(json, re_json);
    }

    // Verify the structure matches tagged enum format
    let raw: Vec<serde_json::Value> = serde_json::from_str(&fixture).expect("parse raw JSON");
    for entry in &raw {
        assert!(
            entry.get("type").is_some(),
            "each Op should have a 'type' field: {entry}"
        );
        assert!(
            entry.get("payload").is_some(),
            "each Op should have a 'payload' field: {entry}"
        );
    }
}

#[test]
fn fixture_events_json_round_trip() {
    let fixture = std::fs::read_to_string("../tests/fixtures/events.json")
        .expect("read events.json fixture");
    let events: Vec<Event> = serde_json::from_str(&fixture).expect("deserialize events.json");

    assert_eq!(
        events.len(),
        10,
        "events.json should contain all 10 Event variants"
    );

    // Verify each can be serialized and deserialized
    for event in &events {
        let json = serde_json::to_string(event).expect("serialize Event from fixture");
        let parsed: Event = serde_json::from_str(&json).expect("deserialize Event from fixture");
        let re_json = serde_json::to_string(&parsed).expect("re-serialize Event from fixture");
        assert_eq!(json, re_json);
    }

    // Verify the structure matches tagged enum format
    let raw: Vec<serde_json::Value> = serde_json::from_str(&fixture).expect("parse raw JSON");
    for entry in &raw {
        assert!(
            entry.get("type").is_some(),
            "each Event should have a 'type' field: {entry}"
        );
        assert!(
            entry.get("payload").is_some(),
            "each Event should have a 'payload' field: {entry}"
        );
    }
}

// ── Stage fields serialize as snake_case ─────────────────────────────

#[test]
fn stage_transition_event_uses_snake_case_stages() {
    let event = Event::StageTransition {
        pipeline_id: "p1".into(),
        from: Stage::HumanReview,
        to: Stage::Deploy,
        timestamp: Utc::now(),
    };
    let json = serde_json::to_string(&event).expect("serialize");
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse");

    assert_eq!(
        value["payload"]["from"], "human_review",
        "Stage::HumanReview should serialize as snake_case"
    );
    assert_eq!(
        value["payload"]["to"], "deploy",
        "Stage::Deploy should serialize as snake_case"
    );
}

#[test]
fn pipeline_created_event_uses_snake_case_stage() {
    let event = Event::PipelineCreated {
        pipeline_id: "p1".into(),
        task_id: "t1".into(),
        stage: Stage::InitAgent,
    };
    let json = serde_json::to_string(&event).expect("serialize");
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse");

    assert_eq!(
        value["payload"]["stage"], "init_agent",
        "Stage::InitAgent should serialize as snake_case"
    );
}

#[test]
fn agent_output_event_uses_snake_case_stage() {
    let event = Event::AgentOutput {
        pipeline_id: "p1".into(),
        stage: Stage::HumanReview,
        delta: "output".into(),
        is_final: true,
    };
    let json = serde_json::to_string(&event).expect("serialize");
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse");

    assert_eq!(
        value["payload"]["stage"], "human_review",
        "Stage::HumanReview should serialize as snake_case in AgentOutput"
    );
}

// ── WsMessage envelope tests ────────────────────────────────────────

#[test]
fn ws_message_op_envelope_format() {
    let msg = WsMessage::Op(Op::StartPipeline {
        task_id: "t1".into(),
        workspace_id: "w1".into(),
    });
    let json = serde_json::to_string(&msg).expect("serialize");
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse");

    assert_eq!(value["kind"], "op");
    assert_eq!(value["payload"]["type"], "StartPipeline");
    assert!(value["payload"]["payload"].is_object());
}

#[test]
fn ws_message_event_envelope_format() {
    let msg = WsMessage::Event(Event::Warning {
        pipeline_id: "p1".into(),
        message: "test".into(),
    });
    let json = serde_json::to_string(&msg).expect("serialize");
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse");

    assert_eq!(value["kind"], "event");
    assert_eq!(value["payload"]["type"], "Warning");
    assert!(value["payload"]["payload"].is_object());
}

#[test]
fn ws_message_round_trip_op() {
    let msg = WsMessage::Op(Op::AdvanceStage {
        pipeline_id: "p1".into(),
    });
    let json = serde_json::to_string(&msg).expect("serialize");
    let parsed: WsMessage = serde_json::from_str(&json).expect("deserialize");
    let re_json = serde_json::to_string(&parsed).expect("re-serialize");
    assert_eq!(json, re_json);
}

#[test]
fn ws_message_round_trip_event() {
    let msg = WsMessage::Event(Event::PipelineCreated {
        pipeline_id: "p1".into(),
        task_id: "t1".into(),
        stage: Stage::Planner,
    });
    let json = serde_json::to_string(&msg).expect("serialize");
    let parsed: WsMessage = serde_json::from_str(&json).expect("deserialize");
    let re_json = serde_json::to_string(&parsed).expect("re-serialize");
    assert_eq!(json, re_json);
}

// ── Error event with null pipeline_id ────────────────────────────────

#[test]
fn error_event_with_null_pipeline_id_round_trips() {
    let event = Event::Error {
        pipeline_id: None,
        code: "unknown".into(),
        message: "something went wrong".into(),
    };
    let json = serde_json::to_string(&event).expect("serialize");
    let parsed: Event = serde_json::from_str(&json).expect("deserialize");
    let re_json = serde_json::to_string(&parsed).expect("re-serialize");
    assert_eq!(json, re_json);
}

// ── DeployStatus with null url ──────────────────────────────────────

#[test]
fn deploy_status_with_null_url_round_trips() {
    let event = Event::DeployStatus {
        pipeline_id: "p1".into(),
        status: "in_progress".into(),
        url: None,
    };
    let json = serde_json::to_string(&event).expect("serialize");
    let parsed: Event = serde_json::from_str(&json).expect("deserialize");
    let re_json = serde_json::to_string(&parsed).expect("re-serialize");
    assert_eq!(json, re_json);
}
