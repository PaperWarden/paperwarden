use crate::JobId;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const PREFIX: &str = "pw-";

/// Owns an isolated on-disk job workspace and removes it when dropped.
#[derive(Debug)]
pub struct Workspace {
    path: PathBuf,
    cleaned: bool,
}

impl Workspace {
    pub fn create(root: &Path, job_id: &JobId) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let path = root.join(format!("{PREFIX}{}", job_id.as_str()));
        fs::create_dir(&path)?;
        Ok(Self {
            path,
            cleaned: false,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn cleanup(&mut self) -> io::Result<()> {
        match fs::remove_dir_all(&self.path) {
            Ok(()) => {
                self.cleaned = true;
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.cleaned = true;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// Removes directories created by PaperWarden in a dedicated workspace root.
    /// Symlinks and unrelated entries are deliberately left untouched.
    pub fn recover_stale(root: &Path) -> io::Result<usize> {
        if !root.exists() {
            return Ok(0);
        }
        let mut removed = 0;
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if file_type.is_dir() && !file_type.is_symlink() && name.starts_with(PREFIX) {
                fs::remove_dir_all(entry.path())?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        if !self.cleaned {
            let _ = self.cleanup();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "paperwarden-{test_name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn successful_job_can_clean_explicitly() {
        let root = root("success");
        let mut workspace = Workspace::create(&root, &JobId::new("success-1").expect("valid id"))
            .expect("workspace created");
        let path = workspace.path().to_path_buf();
        fs::write(path.join("output.bin"), b"synthetic").expect("write output");
        workspace.cleanup().expect("workspace cleaned");
        assert!(!path.exists());
        fs::remove_dir(root).expect("remove empty root");
    }

    #[test]
    fn failed_or_cancelled_scope_cleans_on_drop() {
        let root = root("drop");
        let path;
        {
            let workspace = Workspace::create(&root, &JobId::new("cancelled-1").expect("valid id"))
                .expect("workspace created");
            path = workspace.path().to_path_buf();
            fs::write(path.join("partial.bin"), b"partial").expect("write partial output");
        }
        assert!(!path.exists());
        fs::remove_dir(root).expect("remove empty root");
    }

    #[test]
    fn restart_recovery_removes_only_owned_directories() {
        let root = root("recovery");
        fs::create_dir_all(root.join("pw-stale-1")).expect("create stale workspace");
        fs::create_dir_all(root.join("unrelated")).expect("create unrelated directory");
        let removed = Workspace::recover_stale(&root).expect("recover stale workspaces");
        assert_eq!(removed, 1);
        assert!(root.join("unrelated").exists());
        fs::remove_dir_all(root).expect("remove test root");
    }
}
