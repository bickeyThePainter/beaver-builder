//! Integration tests for protocol serialization round-trips.
//!
//! Tests every Op and Event variant, plus WsMessage wrapping,
//! using fixtures from tests/fixtures/{ops,events}.json.

use beaver_builder::protocol::ops::Op;
use beaver_builder::protocol::events::Event;
use beaver_builder::protocol::messages::WsMessage;

// ---------------------------------------------------------------------------
// Op round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn op_user_message_round_trip() {
    let op = Op::UserMessage {
        task_id: "t1".into(),
        content: "Build a REST API".into(),
    };
    let json = serde_json::to_string(&op).unwrap();
    let decoded: Op = serde_json::from_str(&json).unwrap();
    match decoded {
        Op::UserMessage { task_id, content } => {
            assert_eq!(task_id, "t1");
            assert_eq!(content, "Build a REST API");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn op_start_pipeline_round_trip() {
    let op = Op::StartPipeline {
        task_id: "t1".into(),
        workspace_id: "ws1".into(),
    };
    let json = serde_json::to_string(&op).unwrap();
    let decoded: Op = serde_json::from_str(&json).unwrap();
    match decoded {
        Op::StartPipeline { task_id, workspace_id } => {
            assert_eq!(task_id, "t1");
            assert_eq!(workspace_id, "ws1");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn op_advance_stage_round_trip() {
    let op = Op::AdvanceStage { pipeline_id: "pl1".into() };
    let json = serde_json::to_string(&op).unwrap();
    let decoded: Op = serde_json::from_str(&json).unwrap();
    match decoded {
        Op::AdvanceStage { pipeline_id } => assert_eq!(pipeline_id, "pl1"),
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn op_revert_stage_round_trip() {
    let op = Op::RevertStage {
        pipeline_id: "pl1".into(),
        reason: "Missing error handling".into(),
    };
    let json = serde_json::to_string(&op).unwrap();
    let decoded: Op = serde_json::from_str(&json).unwrap();
    match decoded {
        Op::RevertStage { pipeline_id, reason } => {
            assert_eq!(pipeline_id, "pl1");
            assert_eq!(reason, "Missing error handling");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn op_approve_human_review_round_trip() {
    let op = Op::ApproveHumanReview { pipeline_id: "pl1".into() };
    let json = serde_json::to_string(&op).unwrap();
    let decoded: Op = serde_json::from_str(&json).unwrap();
    match decoded {
        Op::ApproveHumanReview { pipeline_id } => assert_eq!(pipeline_id, "pl1"),
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn op_reject_human_review_round_trip() {
    let op = Op::RejectHumanReview {
        pipeline_id: "pl1".into(),
        reason: "Doesn't match spec".into(),
    };
    let json = serde_json::to_string(&op).unwrap();
    let decoded: Op = serde_json::from_str(&json).unwrap();
    match decoded {
        Op::RejectHumanReview { pipeline_id, reason } => {
            assert_eq!(pipeline_id, "pl1");
            assert_eq!(reason, "Doesn't match spec");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn op_deploy_round_trip() {
    let op = Op::Deploy {
        pipeline_id: "pl1".into(),
        environment: "staging".into(),
    };
    let json = serde_json::to_string(&op).unwrap();
    let decoded: Op = serde_json::from_str(&json).unwrap();
    match decoded {
        Op::Deploy { pipeline_id, environment } => {
            assert_eq!(pipeline_id, "pl1");
            assert_eq!(environment, "staging");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn op_push_round_trip() {
    let op = Op::Push {
        pipeline_id: "pl1".into(),
        remote: "origin".into(),
        branch: "main".into(),
    };
    let json = serde_json::to_string(&op).unwrap();
    let decoded: Op = serde_json::from_str(&json).unwrap();
    match decoded {
        Op::Push { pipeline_id, remote, branch } => {
            assert_eq!(pipeline_id, "pl1");
            assert_eq!(remote, "origin");
            assert_eq!(branch, "main");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn op_interrupt_pipeline_round_trip() {
    let op = Op::InterruptPipeline { pipeline_id: "pl1".into() };
    let json = serde_json::to_string(&op).unwrap();
    let decoded: Op = serde_json::from_str(&json).unwrap();
    match decoded {
        Op::InterruptPipeline { pipeline_id } => assert_eq!(pipeline_id, "pl1"),
        _ => panic!("Wrong variant"),
    }
}

// ---------------------------------------------------------------------------
// Event round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn event_pipeline_created_round_trip() {
    let evt = Event::PipelineCreated {
        pipeline_id: "pl1".into(),
        task_id: "t1".into(),
        stage: beaver_builder::domain::pipeline::Stage::Created,
    };
    let json = serde_json::to_string(&evt).unwrap();
    assert!(json.contains("\"created\""));
    let decoded: Event = serde_json::from_str(&json).unwrap();
    match decoded {
        Event::PipelineCreated { pipeline_id, task_id, stage } => {
            assert_eq!(pipeline_id, "pl1");
            assert_eq!(task_id, "t1");
            assert_eq!(stage, beaver_builder::domain::pipeline::Stage::Created);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn event_stage_transition_round_trip() {
    let evt = Event::StageTransition {
        pipeline_id: "pl1".into(),
        from: beaver_builder::domain::pipeline::Stage::Coder,
        to: beaver_builder::domain::pipeline::Stage::Reviewer,
        timestamp: "2026-03-29T10:00:00Z".into(),
    };
    let json = serde_json::to_string(&evt).unwrap();
    let decoded: Event = serde_json::from_str(&json).unwrap();
    match decoded {
        Event::StageTransition { pipeline_id, from, to, timestamp } => {
            assert_eq!(pipeline_id, "pl1");
            assert_eq!(from, beaver_builder::domain::pipeline::Stage::Coder);
            assert_eq!(to, beaver_builder::domain::pipeline::Stage::Reviewer);
            assert_eq!(timestamp, "2026-03-29T10:00:00Z");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn event_agent_output_round_trip() {
    let evt = Event::AgentOutput {
        pipeline_id: "pl1".into(),
        stage: beaver_builder::domain::pipeline::Stage::IntentClarifier,
        delta: "Hello, what would you like to build?".into(),
        is_final: false,
    };
    let json = serde_json::to_string(&evt).unwrap();
    let decoded: Event = serde_json::from_str(&json).unwrap();
    match decoded {
        Event::AgentOutput { pipeline_id, stage, delta, is_final } => {
            assert_eq!(pipeline_id, "pl1");
            assert_eq!(stage, beaver_builder::domain::pipeline::Stage::IntentClarifier);
            assert_eq!(delta, "Hello, what would you like to build?");
            assert!(!is_final);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn event_tool_execution_round_trip() {
    let params = serde_json::json!({"path": "src/main.rs", "content": "fn main() {}"});
    let result = serde_json::json!({"success": true, "bytes_written": 42});
    let evt = Event::ToolExecution {
        pipeline_id: "pl1".into(),
        tool: "write_file".into(),
        params: params.clone(),
        result: result.clone(),
        duration_ms: 12,
    };
    let json = serde_json::to_string(&evt).unwrap();
    let decoded: Event = serde_json::from_str(&json).unwrap();
    match decoded {
        Event::ToolExecution { pipeline_id, tool, params: p, result: r, duration_ms } => {
            assert_eq!(pipeline_id, "pl1");
            assert_eq!(tool, "write_file");
            assert_eq!(p, params);
            assert_eq!(r, result);
            assert_eq!(duration_ms, 12);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn event_approval_required_round_trip() {
    let evt = Event::ApprovalRequired {
        pipeline_id: "pl1".into(),
        task_id: "t1".into(),
        summary: "Review complete".into(),
    };
    let json = serde_json::to_string(&evt).unwrap();
    let decoded: Event = serde_json::from_str(&json).unwrap();
    match decoded {
        Event::ApprovalRequired { pipeline_id, task_id, summary } => {
            assert_eq!(pipeline_id, "pl1");
            assert_eq!(task_id, "t1");
            assert_eq!(summary, "Review complete");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn event_review_submitted_round_trip() {
    let evt = Event::ReviewSubmitted {
        pipeline_id: "pl1".into(),
        verdict: "approved".into(),
        iteration: 2,
    };
    let json = serde_json::to_string(&evt).unwrap();
    let decoded: Event = serde_json::from_str(&json).unwrap();
    match decoded {
        Event::ReviewSubmitted { pipeline_id, verdict, iteration } => {
            assert_eq!(pipeline_id, "pl1");
            assert_eq!(verdict, "approved");
            assert_eq!(iteration, 2);
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn event_deploy_status_round_trip() {
    let evt = Event::DeployStatus {
        pipeline_id: "pl1".into(),
        status: "success".into(),
        url: Some("https://staging.example.com".into()),
    };
    let json = serde_json::to_string(&evt).unwrap();
    let decoded: Event = serde_json::from_str(&json).unwrap();
    match decoded {
        Event::DeployStatus { pipeline_id, status, url } => {
            assert_eq!(pipeline_id, "pl1");
            assert_eq!(status, "success");
            assert_eq!(url.as_deref(), Some("https://staging.example.com"));
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn event_deploy_status_null_url_round_trip() {
    let evt = Event::DeployStatus {
        pipeline_id: "pl1".into(),
        status: "in_progress".into(),
        url: None,
    };
    let json = serde_json::to_string(&evt).unwrap();
    assert!(json.contains("null"));
    let decoded: Event = serde_json::from_str(&json).unwrap();
    match decoded {
        Event::DeployStatus { url, .. } => assert!(url.is_none()),
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn event_push_complete_round_trip() {
    let evt = Event::PushComplete {
        pipeline_id: "pl1".into(),
        remote: "origin".into(),
        sha: "abc123".into(),
    };
    let json = serde_json::to_string(&evt).unwrap();
    let decoded: Event = serde_json::from_str(&json).unwrap();
    match decoded {
        Event::PushComplete { pipeline_id, remote, sha } => {
            assert_eq!(pipeline_id, "pl1");
            assert_eq!(remote, "origin");
            assert_eq!(sha, "abc123");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn event_error_with_pipeline_id_round_trip() {
    let evt = Event::Error {
        pipeline_id: Some("pl1".into()),
        code: "INVALID_TRANSITION".into(),
        message: "Cannot advance".into(),
    };
    let json = serde_json::to_string(&evt).unwrap();
    let decoded: Event = serde_json::from_str(&json).unwrap();
    match decoded {
        Event::Error { pipeline_id, code, message } => {
            assert_eq!(pipeline_id.as_deref(), Some("pl1"));
            assert_eq!(code, "INVALID_TRANSITION");
            assert_eq!(message, "Cannot advance");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn event_error_without_pipeline_id_round_trip() {
    let evt = Event::Error {
        pipeline_id: None,
        code: "PARSE_ERROR".into(),
        message: "Bad JSON".into(),
    };
    let json = serde_json::to_string(&evt).unwrap();
    assert!(json.contains("null"));
    let decoded: Event = serde_json::from_str(&json).unwrap();
    match decoded {
        Event::Error { pipeline_id, .. } => assert!(pipeline_id.is_none()),
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn event_warning_with_pipeline_id_round_trip() {
    let evt = Event::Warning {
        pipeline_id: Some("pl1".into()),
        message: "Review loop exhausted".into(),
    };
    let json = serde_json::to_string(&evt).unwrap();
    let decoded: Event = serde_json::from_str(&json).unwrap();
    match decoded {
        Event::Warning { pipeline_id, message } => {
            assert_eq!(pipeline_id.as_deref(), Some("pl1"));
            assert_eq!(message, "Review loop exhausted");
        }
        _ => panic!("Wrong variant"),
    }
}

#[test]
fn event_warning_without_pipeline_id_round_trip() {
    let evt = Event::Warning {
        pipeline_id: None,
        message: "System warning".into(),
    };
    let json = serde_json::to_string(&evt).unwrap();
    let decoded: Event = serde_json::from_str(&json).unwrap();
    match decoded {
        Event::Warning { pipeline_id, .. } => assert!(pipeline_id.is_none()),
        _ => panic!("Wrong variant"),
    }
}

// ---------------------------------------------------------------------------
// WsMessage wrapping tests
// ---------------------------------------------------------------------------

#[test]
fn ws_message_op_round_trip() {
    let op = Op::StartPipeline {
        task_id: "t1".into(),
        workspace_id: "ws1".into(),
    };
    let msg = WsMessage::Op { payload: op };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"kind\":\"op\""));
    let decoded: WsMessage = serde_json::from_str(&json).unwrap();
    match decoded {
        WsMessage::Op { payload } => match payload {
            Op::StartPipeline { task_id, .. } => assert_eq!(task_id, "t1"),
            _ => panic!("Wrong Op variant"),
        },
        _ => panic!("Expected WsMessage::Op"),
    }
}

#[test]
fn ws_message_event_round_trip() {
    let evt = Event::PipelineCreated {
        pipeline_id: "pl1".into(),
        task_id: "t1".into(),
        stage: beaver_builder::domain::pipeline::Stage::IntentClarifier,
    };
    let msg = WsMessage::Event { payload: evt };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"kind\":\"event\""));
    let decoded: WsMessage = serde_json::from_str(&json).unwrap();
    match decoded {
        WsMessage::Event { payload } => match payload {
            Event::PipelineCreated { pipeline_id, .. } => assert_eq!(pipeline_id, "pl1"),
            _ => panic!("Wrong Event variant"),
        },
        _ => panic!("Expected WsMessage::Event"),
    }
}

// ---------------------------------------------------------------------------
// Fixture-based tests: deserialize from the JSON fixture files
// ---------------------------------------------------------------------------

#[test]
fn all_op_fixtures_deserialize() {
    let fixture = include_str!("../../tests/fixtures/ops.json");
    let parsed: serde_json::Value = serde_json::from_str(fixture).unwrap();
    let ops = parsed["ops"].as_array().unwrap();

    assert_eq!(ops.len(), 9, "Expected 9 Op fixtures");

    for (i, op_val) in ops.iter().enumerate() {
        let result = serde_json::from_value::<Op>(op_val.clone());
        assert!(result.is_ok(), "Op fixture #{i} failed to deserialize: {:?}", result.err());
    }
}

#[test]
fn all_event_fixtures_deserialize() {
    let fixture = include_str!("../../tests/fixtures/events.json");
    let parsed: serde_json::Value = serde_json::from_str(fixture).unwrap();
    let events = parsed["events"].as_array().unwrap();

    assert_eq!(events.len(), 15, "Expected 15 Event fixtures");

    for (i, evt_val) in events.iter().enumerate() {
        let result = serde_json::from_value::<Event>(evt_val.clone());
        assert!(result.is_ok(), "Event fixture #{i} failed to deserialize: {:?}", result.err());
    }
}

#[test]
fn op_fixtures_round_trip() {
    let fixture = include_str!("../../tests/fixtures/ops.json");
    let parsed: serde_json::Value = serde_json::from_str(fixture).unwrap();
    let ops = parsed["ops"].as_array().unwrap();

    for (i, op_val) in ops.iter().enumerate() {
        let op: Op = serde_json::from_value(op_val.clone()).unwrap();
        let re_serialized = serde_json::to_value(&op).unwrap();
        let re_deserialized: Op = serde_json::from_value(re_serialized).unwrap();
        let final_json = serde_json::to_string(&re_deserialized).unwrap();
        let original_json = serde_json::to_string(&op).unwrap();
        assert_eq!(final_json, original_json, "Op fixture #{i} round-trip mismatch");
    }
}

#[test]
fn event_fixtures_round_trip() {
    let fixture = include_str!("../../tests/fixtures/events.json");
    let parsed: serde_json::Value = serde_json::from_str(fixture).unwrap();
    let events = parsed["events"].as_array().unwrap();

    for (i, evt_val) in events.iter().enumerate() {
        let evt: Event = serde_json::from_value(evt_val.clone()).unwrap();
        let re_serialized = serde_json::to_value(&evt).unwrap();
        let re_deserialized: Event = serde_json::from_value(re_serialized).unwrap();
        let final_json = serde_json::to_string(&re_deserialized).unwrap();
        let original_json = serde_json::to_string(&evt).unwrap();
        assert_eq!(final_json, original_json, "Event fixture #{i} round-trip mismatch");
    }
}

// ---------------------------------------------------------------------------
// Op serialization format: ensure tagged correctly
// ---------------------------------------------------------------------------

#[test]
fn op_serializes_with_type_and_payload_tags() {
    let op = Op::StartPipeline {
        task_id: "t1".into(),
        workspace_id: "ws1".into(),
    };
    let val: serde_json::Value = serde_json::to_value(&op).unwrap();
    assert_eq!(val["type"], "StartPipeline");
    assert!(val["payload"].is_object());
    assert_eq!(val["payload"]["task_id"], "t1");
}

#[test]
fn event_serializes_with_type_and_payload_tags() {
    let evt = Event::StageTransition {
        pipeline_id: "pl1".into(),
        from: beaver_builder::domain::pipeline::Stage::Coder,
        to: beaver_builder::domain::pipeline::Stage::Reviewer,
        timestamp: "2026-03-29T10:00:00Z".into(),
    };
    let val: serde_json::Value = serde_json::to_value(&evt).unwrap();
    assert_eq!(val["type"], "StageTransition");
    assert!(val["payload"].is_object());
    assert_eq!(val["payload"]["from"], "coder");
    assert_eq!(val["payload"]["to"], "reviewer");
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn tool_execution_with_arbitrary_json_params() {
    let params = serde_json::json!({
        "deeply": {"nested": [1, 2, {"key": true}]},
        "number": 42,
        "string": "hello"
    });
    let result = serde_json::json!(null);
    let evt = Event::ToolExecution {
        pipeline_id: "pl1".into(),
        tool: "complex_tool".into(),
        params: params.clone(),
        result: result.clone(),
        duration_ms: 100,
    };
    let json = serde_json::to_string(&evt).unwrap();
    let decoded: Event = serde_json::from_str(&json).unwrap();
    match decoded {
        Event::ToolExecution { params: p, result: r, .. } => {
            assert_eq!(p, params);
            assert_eq!(r, result);
        }
        _ => panic!("Wrong variant"),
    }
}
