use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::pipeline::Stage;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Event {
    PipelineCreated {
        pipeline_id: String,
        task_id: String,
        stage: Stage,
    },
    StageTransition {
        pipeline_id: String,
        from: Stage,
        to: Stage,
        timestamp: DateTime<Utc>,
    },
    AgentOutput {
        pipeline_id: String,
        stage: Stage,
        delta: String,
        is_final: bool,
    },
    ToolExecution {
        pipeline_id: String,
        tool: String,
        params: serde_json::Value,
        result: serde_json::Value,
        duration_ms: u64,
    },
    ApprovalRequired {
        pipeline_id: String,
        task_id: String,
        summary: String,
    },
    ReviewSubmitted {
        pipeline_id: String,
        verdict: String,
        iteration: u8,
    },
    DeployStatus {
        pipeline_id: String,
        status: String,
        url: Option<String>,
    },
    PushComplete {
        pipeline_id: String,
        remote: String,
        sha: String,
    },
    Error {
        pipeline_id: Option<String>,
        code: String,
        message: String,
    },
    Warning {
        pipeline_id: String,
        message: String,
    },
}
