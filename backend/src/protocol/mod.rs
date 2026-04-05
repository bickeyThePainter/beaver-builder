pub mod ops;
pub mod events;
pub mod messages;

#[cfg(test)]
mod tests {
    use super::events::Event;
    use super::messages::WsMessage;
    use super::ops::Op;
    use crate::domain::pipeline::Stage;
    use chrono::Utc;

    // ── Op round-trip tests ───────────────────────────────────────────

    #[test]
    fn op_user_message_round_trip() {
        let op = Op::UserMessage {
            task_id: "t1".into(),
            content: "hello".into(),
        };
        round_trip_op(&op);
    }

    #[test]
    fn op_start_pipeline_round_trip() {
        let op = Op::StartPipeline {
            task_id: "t1".into(),
            workspace_id: "w1".into(),
        };
        round_trip_op(&op);
    }

    #[test]
    fn op_advance_stage_round_trip() {
        let op = Op::AdvanceStage {
            pipeline_id: "p1".into(),
        };
        round_trip_op(&op);
    }

    #[test]
    fn op_revert_stage_round_trip() {
        let op = Op::RevertStage {
            pipeline_id: "p1".into(),
            reason: "needs fix".into(),
        };
        round_trip_op(&op);
    }

    #[test]
    fn op_approve_human_review_round_trip() {
        let op = Op::ApproveHumanReview {
            pipeline_id: "p1".into(),
        };
        round_trip_op(&op);
    }

    #[test]
    fn op_reject_human_review_round_trip() {
        let op = Op::RejectHumanReview {
            pipeline_id: "p1".into(),
            reason: "not good enough".into(),
        };
        round_trip_op(&op);
    }

    #[test]
    fn op_deploy_round_trip() {
        let op = Op::Deploy {
            pipeline_id: "p1".into(),
            environment: "staging".into(),
        };
        round_trip_op(&op);
    }

    #[test]
    fn op_interrupt_pipeline_round_trip() {
        let op = Op::InterruptPipeline {
            pipeline_id: "p1".into(),
        };
        round_trip_op(&op);
    }

    // ── Event round-trip tests ────────────────────────────────────────

    #[test]
    fn event_pipeline_created_round_trip() {
        let event = Event::PipelineCreated {
            pipeline_id: "p1".into(),
            task_id: "t1".into(),
            stage: Stage::Created,
        };
        round_trip_event(&event);
    }

    #[test]
    fn event_stage_transition_round_trip() {
        let event = Event::StageTransition {
            pipeline_id: "p1".into(),
            from: Stage::Planner,
            to: Stage::InitAgent,
            timestamp: Utc::now(),
        };
        round_trip_event(&event);
    }

    #[test]
    fn event_agent_output_round_trip() {
        let event = Event::AgentOutput {
            pipeline_id: "p1".into(),
            stage: Stage::Coder,
            delta: "some output".into(),
            is_final: false,
        };
        round_trip_event(&event);
    }

    #[test]
    fn event_tool_execution_round_trip() {
        let event = Event::ToolExecution {
            pipeline_id: "p1".into(),
            tool: "file_write".into(),
            params: serde_json::json!({"path": "main.rs"}),
            result: serde_json::json!({"ok": true}),
            duration_ms: 42,
        };
        round_trip_event(&event);
    }

    #[test]
    fn event_approval_required_round_trip() {
        let event = Event::ApprovalRequired {
            pipeline_id: "p1".into(),
            task_id: "t1".into(),
            summary: "ready for review".into(),
        };
        round_trip_event(&event);
    }

    #[test]
    fn event_review_submitted_round_trip() {
        let event = Event::ReviewSubmitted {
            pipeline_id: "p1".into(),
            verdict: "APPROVE".into(),
            iteration: 1,
        };
        round_trip_event(&event);
    }

    #[test]
    fn event_deploy_status_round_trip() {
        let event = Event::DeployStatus {
            pipeline_id: "p1".into(),
            status: "running".into(),
            url: Some("https://staging.example.com".into()),
        };
        round_trip_event(&event);
    }

    #[test]
    fn event_push_complete_round_trip() {
        let event = Event::PushComplete {
            pipeline_id: "p1".into(),
            remote: "origin".into(),
            sha: "abc123".into(),
        };
        round_trip_event(&event);
    }

    #[test]
    fn event_error_round_trip() {
        let event = Event::Error {
            pipeline_id: Some("p1".into()),
            code: "INTERNAL".into(),
            message: "something broke".into(),
        };
        round_trip_event(&event);
    }

    #[test]
    fn event_warning_round_trip() {
        let event = Event::Warning {
            pipeline_id: "p1".into(),
            message: "review loop exhausted".into(),
        };
        round_trip_event(&event);
    }

    // ── WsMessage envelope tests ──────────────────────────────────────

    #[test]
    fn ws_message_op_envelope_round_trip() {
        let msg = WsMessage::Op(Op::AdvanceStage {
            pipeline_id: "p1".into(),
        });
        let json = serde_json::to_string(&msg).expect("serialize WsMessage::Op");
        let parsed: WsMessage = serde_json::from_str(&json).expect("deserialize WsMessage::Op");
        let reparsed = serde_json::to_string(&parsed).expect("re-serialize");
        assert_eq!(json, reparsed);
    }

    #[test]
    fn ws_message_event_envelope_round_trip() {
        let msg = WsMessage::Event(Event::Warning {
            pipeline_id: "p1".into(),
            message: "test".into(),
        });
        let json = serde_json::to_string(&msg).expect("serialize WsMessage::Event");
        let parsed: WsMessage =
            serde_json::from_str(&json).expect("deserialize WsMessage::Event");
        let reparsed = serde_json::to_string(&parsed).expect("re-serialize");
        assert_eq!(json, reparsed);
    }

    #[test]
    fn ws_message_op_envelope_has_correct_kind() {
        let msg = WsMessage::Op(Op::StartPipeline {
            task_id: "t1".into(),
            workspace_id: "w1".into(),
        });
        let json = serde_json::to_string(&msg).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        assert_eq!(value["kind"], "op");
    }

    #[test]
    fn ws_message_event_envelope_has_correct_kind() {
        let msg = WsMessage::Event(Event::PipelineCreated {
            pipeline_id: "p1".into(),
            task_id: "t1".into(),
            stage: Stage::Created,
        });
        let json = serde_json::to_string(&msg).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        assert_eq!(value["kind"], "event");
    }

    #[test]
    fn stage_fields_serialize_as_snake_case_in_events() {
        let event = Event::StageTransition {
            pipeline_id: "p1".into(),
            from: Stage::HumanReview,
            to: Stage::Deploy,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"human_review\""), "expected snake_case Stage in JSON: {json}");
    }

    // ── Helpers ───────────────────────────────────────────────────────

    fn round_trip_op(op: &Op) {
        let json = serde_json::to_string(op).expect("serialize Op");
        let parsed: Op = serde_json::from_str(&json).expect("deserialize Op");
        let reparsed = serde_json::to_string(&parsed).expect("re-serialize Op");
        assert_eq!(json, reparsed);
    }

    fn round_trip_event(event: &Event) {
        let json = serde_json::to_string(event).expect("serialize Event");
        let parsed: Event = serde_json::from_str(&json).expect("deserialize Event");
        let reparsed = serde_json::to_string(&parsed).expect("re-serialize Event");
        assert_eq!(json, reparsed);
    }
}
