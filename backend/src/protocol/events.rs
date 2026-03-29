use serde::{Deserialize, Serialize};
use crate::domain::pipeline::Stage;

/// Events are facts produced by the orchestrator and broadcast to all connected clients.
/// They are immutable records of what happened -- never commands.
///
/// Design principle: Events carry enough context for the frontend to update its state
/// without additional queries. Each event is self-describing via the `type` tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Event {
    /// A new pipeline has been created and registered.
    PipelineCreated {
        pipeline_id: String,
        task_id: String,
        stage: Stage,
    },

    /// The pipeline transitioned from one stage to another.
    StageTransition {
        pipeline_id: String,
        from: Stage,
        to: Stage,
        timestamp: String,
    },

    /// Streaming output from an agent. `is_final` marks the last chunk.
    AgentOutput {
        pipeline_id: String,
        stage: Stage,
        delta: String,
        is_final: bool,
    },

    /// A tool was invoked by an agent during execution.
    ToolExecution {
        pipeline_id: String,
        tool: String,
        params: serde_json::Value,
        result: serde_json::Value,
        duration_ms: u64,
    },

    /// The pipeline has reached the HumanReview gate and needs approval.
    ApprovalRequired {
        pipeline_id: String,
        task_id: String,
        summary: String,
    },

    /// The Reviewer agent submitted its verdict.
    ReviewSubmitted {
        pipeline_id: String,
        verdict: String,
        iteration: u8,
    },

    /// Deployment status update.
    DeployStatus {
        pipeline_id: String,
        status: String,
        url: Option<String>,
    },

    /// Push to remote completed successfully.
    PushComplete {
        pipeline_id: String,
        remote: String,
        sha: String,
    },

    /// An error occurred. `pipeline_id` is None for system-level errors.
    Error {
        pipeline_id: Option<String>,
        code: String,
        message: String,
    },

    /// A non-fatal warning.
    Warning {
        pipeline_id: Option<String>,
        message: String,
    },
}
