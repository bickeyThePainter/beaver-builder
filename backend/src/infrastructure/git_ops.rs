//! Git operations -- infrastructure adapter for git worktree management.

use std::path::Path;
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Git command failed: {0}")]
    CommandFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct GitOps;

impl GitOps {
    /// Initialize a new git repository at the given path.
    pub fn init(path: &Path) -> Result<(), GitError> {
        let output = Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()?;

        if !output.status.success() {
            return Err(GitError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }

    /// Create a new worktree from the given repository.
    pub fn create_worktree(
        repo_path: &Path,
        worktree_path: &Path,
        branch: &str,
    ) -> Result<(), GitError> {
        let output = Command::new("git")
            .args([
                "worktree",
                "add",
                &worktree_path.to_string_lossy(),
                "-b",
                branch,
            ])
            .current_dir(repo_path)
            .output()?;

        if !output.status.success() {
            return Err(GitError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }

    /// Stage all changes and commit with the given message.
    pub fn commit(worktree_path: &Path, message: &str) -> Result<String, GitError> {
        // Stage all changes
        let add_output = Command::new("git")
            .args(["add", "-A"])
            .current_dir(worktree_path)
            .output()?;

        if !add_output.status.success() {
            return Err(GitError::CommandFailed(
                String::from_utf8_lossy(&add_output.stderr).to_string(),
            ));
        }

        // Commit
        let commit_output = Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(worktree_path)
            .output()?;

        if !commit_output.status.success() {
            let stderr = String::from_utf8_lossy(&commit_output.stderr).to_string();
            // "nothing to commit" is not really an error
            if stderr.contains("nothing to commit") {
                return Ok("nothing to commit".into());
            }
            return Err(GitError::CommandFailed(stderr));
        }

        // Get the commit SHA
        let sha_output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(worktree_path)
            .output()?;

        Ok(String::from_utf8_lossy(&sha_output.stdout).trim().to_string())
    }

    /// Get the diff for the current worktree.
    pub fn diff(worktree_path: &Path) -> Result<String, GitError> {
        let output = Command::new("git")
            .args(["diff", "--cached"])
            .current_dir(worktree_path)
            .output()?;

        if !output.status.success() {
            return Err(GitError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Push to a remote.
    pub fn push(worktree_path: &Path, remote: &str, branch: &str) -> Result<(), GitError> {
        let output = Command::new("git")
            .args(["push", remote, branch])
            .current_dir(worktree_path)
            .output()?;

        if !output.status.success() {
            return Err(GitError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }

    /// Get the current branch name.
    pub fn current_branch(worktree_path: &Path) -> Result<String, GitError> {
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(worktree_path)
            .output()?;

        if !output.status.success() {
            return Err(GitError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}
