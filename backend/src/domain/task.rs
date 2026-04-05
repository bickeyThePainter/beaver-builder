use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub spec: String,
    pub workspace_id: String,
    pub pipeline_id: Option<String>,
}

impl Task {
    pub fn new(id: String, title: String, spec: String, workspace_id: String) -> Self {
        Self {
            id,
            title,
            spec,
            workspace_id,
            pipeline_id: None,
        }
    }

    pub fn attach_pipeline(&mut self, pipeline_id: String) {
        self.pipeline_id = Some(pipeline_id);
    }
}
