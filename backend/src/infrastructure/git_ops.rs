use std::path::Path;
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("git command failed: {0}")]
    CommandFailed(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Thin wrapper around git CLI operations.
pub struct GitOps;

impl GitOps {
    /// Initialize a new git repository at the given path.
    pub fn init(path: &Path) -> Result<String, GitError> {
        Self::run_git(path, &["init"])
    }

    /// Create a new git worktree linked to a branch.
    pub fn create_worktree(repo: &Path, branch: &str) -> Result<String, GitError> {
        let worktree_path = repo.join(format!("worktrees/{branch}"));
        Self::run_git(
            repo,
            &[
                "worktree",
                "add",
                &worktree_path.to_string_lossy(),
                "-b",
                branch,
            ],
        )
    }

    /// Commit all staged changes with the given message.
    pub fn commit(path: &Path, message: &str) -> Result<String, GitError> {
        Self::run_git(path, &["add", "-A"])?;
        Self::run_git(path, &["commit", "-m", message])
    }

    /// Push to a remote branch.
    pub fn push(path: &Path, remote: &str, branch: &str) -> Result<String, GitError> {
        Self::run_git(path, &["push", remote, branch])
    }

    /// Get the current branch name.
    pub fn current_branch(path: &Path) -> Result<String, GitError> {
        Self::run_git(path, &["rev-parse", "--abbrev-ref", "HEAD"])
            .map(|s| s.trim().to_string())
    }

    /// Get the diff of the working directory.
    pub fn diff(path: &Path) -> Result<String, GitError> {
        Self::run_git(path, &["diff"])
    }

    fn run_git(cwd: &Path, args: &[&str]) -> Result<String, GitError> {
        let output = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(GitError::CommandFailed(stderr))
        }
    }
}
