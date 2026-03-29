use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeStatus {
    Active,
    Processing,
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileArtifact {
    pub name: String,
    pub file_type: String,
    pub size: String,
    pub author: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worktree {
    pub id: String,
    pub branch: String,
    pub status: WorktreeStatus,
    pub files: Vec<FileArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwimlaneConfig {
    pub active: String,
    pub base: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub repos: Vec<String>,
    pub swimlane: SwimlaneConfig,
    pub worktrees: Vec<Worktree>,
}

impl Workspace {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            repos: Vec::new(),
            swimlane: SwimlaneConfig {
                active: String::new(),
                base: String::new(),
            },
            worktrees: Vec::new(),
        }
    }

    pub fn add_worktree(&mut self, worktree: Worktree) {
        self.worktrees.push(worktree);
    }

    pub fn find_worktree(&self, id: &str) -> Option<&Worktree> {
        self.worktrees.iter().find(|wt| wt.id == id)
    }
}
