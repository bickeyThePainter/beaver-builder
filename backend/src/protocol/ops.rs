use serde::{Deserialize, Serialize};

/// Ops are commands submitted by the client via WebSocket.
/// They enter the Submission Queue and are processed sequentially by the orchestrator.
///
/// Design principle: Ops represent *intent* -- they may be rejected if the current
/// state doesn't permit the operation. The orchestrator validates before executing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Op {
    /// Send a chat message to the Intent Clarifier agent.
    UserMessage {
        task_id: String,
        content: String,
    },

    /// Initialize a new pipeline for a task within a workspace.
    StartPipeline {
        task_id: String,
        workspace_id: String,
    },

    /// Manually advance the pipeline to the next stage.
    /// Used for admin overrides or stages that complete externally.
    AdvanceStage {
        pipeline_id: String,
    },

    /// Revert the pipeline to a previous stage.
    /// Primary use: Reviewer sends back to Coder with feedback.
    RevertStage {
        pipeline_id: String,
        reason: String,
    },

    /// Human approves at the HumanReview gate. Pipeline proceeds to Deploy.
    ApproveHumanReview {
        pipeline_id: String,
    },

    /// Human rejects at the HumanReview gate. Pipeline reverts to Coder.
    RejectHumanReview {
        pipeline_id: String,
        reason: String,
    },

    /// Trigger deployment to the specified environment.
    Deploy {
        pipeline_id: String,
        environment: String,
    },

    /// Push committed changes to the remote repository.
    Push {
        pipeline_id: String,
        remote: String,
        branch: String,
    },

    /// Halt the currently executing agent. The pipeline enters a paused state
    /// and can be resumed or advanced manually.
    InterruptPipeline {
        pipeline_id: String,
    },
}
