use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub spec: String,
    pub workspace_id: String,
    pub pipeline_id: Option<String>,
    pub priority: Priority,
    pub created_at: DateTime<Utc>,
}

impl Task {
    pub fn new(id: String, title: String, workspace_id: String) -> Self {
        Self {
            id,
            title,
            spec: String::new(),
            workspace_id,
            pipeline_id: None,
            priority: Priority::Medium,
            created_at: Utc::now(),
        }
    }

    pub fn set_spec(&mut self, spec: String) {
        self.spec = spec;
    }

    pub fn attach_pipeline(&mut self, pipeline_id: String) {
        self.pipeline_id = Some(pipeline_id);
    }
}
