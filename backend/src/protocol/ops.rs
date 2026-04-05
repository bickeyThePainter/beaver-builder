use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Op {
    UserMessage {
        task_id: String,
        content: String,
    },
    StartPipeline {
        task_id: String,
        workspace_id: String,
    },
    AdvanceStage {
        pipeline_id: String,
    },
    RevertStage {
        pipeline_id: String,
        reason: String,
    },
    ApproveHumanReview {
        pipeline_id: String,
    },
    RejectHumanReview {
        pipeline_id: String,
        reason: String,
    },
    Deploy {
        pipeline_id: String,
        environment: String,
    },
    InterruptPipeline {
        pipeline_id: String,
    },
}
