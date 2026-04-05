use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FsError {
    #[error("path traversal denied: {0}")]
    PathTraversal(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Sandboxed file operations rooted at a given directory.
/// All relative paths are resolved and checked to stay within the sandbox.
pub struct SandboxedFs {
    root: PathBuf,
}

impl SandboxedFs {
    pub fn new(root: PathBuf) -> Result<Self, FsError> {
        fs::create_dir_all(&root)?;
        // Canonicalize root after creation (resolves /tmp -> /private/tmp on macOS)
        let root = fs::canonicalize(&root)?;
        Ok(Self { root })
    }

    pub fn read_file(&self, relative: &str) -> Result<String, FsError> {
        let path = self.resolve(relative)?;
        Ok(fs::read_to_string(path)?)
    }

    pub fn write_file(&self, relative: &str, content: &str) -> Result<(), FsError> {
        let path = self.resolve(relative)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(fs::write(path, content)?)
    }

    pub fn create_dir(&self, relative: &str) -> Result<(), FsError> {
        let path = self.resolve(relative)?;
        Ok(fs::create_dir_all(path)?)
    }

    pub fn list_dir(&self, relative: &str) -> Result<Vec<String>, FsError> {
        let path = self.resolve(relative)?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str() {
                entries.push(name.to_string());
            }
        }
        entries.sort();
        Ok(entries)
    }

    /// Create a basic project scaffold.
    pub fn scaffold_project(&self, title: &str, spec: &str) -> Result<(), FsError> {
        self.create_dir("specs")?;
        self.write_file("README.md", &format!("# {title}\n\n{spec}\n"))?;
        self.write_file("CHANGELOG.md", "# Changelog\n")?;
        self.write_file("specs/design.md", &format!("# Design: {title}\n\n{spec}\n"))?;
        Ok(())
    }

    /// Resolve a relative path within the sandbox, preventing traversal.
    fn resolve(&self, relative: &str) -> Result<PathBuf, FsError> {
        // Build the joined path under root, then strip any .. / . components
        // to prevent traversal BEFORE checking the filesystem.
        let joined = self.root.join(relative);
        let normalized = Self::normalize_components(&joined);

        // Check that the normalized path stays inside the root
        if !normalized.starts_with(&self.root) {
            return Err(FsError::PathTraversal(relative.to_string()));
        }

        Ok(normalized)
    }

    /// Normalize path components in-memory (resolve `.` and `..` without hitting FS).
    /// This avoids macOS /tmp -> /private/tmp mismatches.
    fn normalize_components(path: &Path) -> PathBuf {
        let mut parts = Vec::new();
        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    parts.pop();
                }
                std::path::Component::CurDir => {}
                other => parts.push(other.as_os_str().to_owned()),
            }
        }
        let mut result = PathBuf::new();
        for part in parts {
            result.push(part);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn scaffold_creates_expected_files() {
        let dir = env::temp_dir().join(format!("bb_test_scaffold_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let fs_ops = SandboxedFs::new(dir.clone()).expect("create sandbox");
        fs_ops
            .scaffold_project("Test Project", "A test spec")
            .expect("scaffold");

        assert!(fs_ops.read_file("README.md").is_ok());
        assert!(fs_ops.read_file("CHANGELOG.md").is_ok());
        assert!(fs_ops.read_file("specs/design.md").is_ok());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_traversal_blocked() {
        let dir = env::temp_dir().join(format!("bb_test_traverse_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let fs_ops = SandboxedFs::new(dir.clone()).expect("create sandbox");
        // Write a legitimate file first so the parent exists
        fs_ops.write_file("legit.txt", "ok").expect("write");

        let result = fs_ops.read_file("../../etc/passwd");
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&dir);
    }
}
