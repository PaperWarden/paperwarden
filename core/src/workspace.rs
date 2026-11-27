//! Isolated on-disk job workspaces.
//!
//! This is the only module that creates or deletes job workspace directories.
//! Feature code must not manage temporary files itself (`ARCHITECTURE.md`,
//! ADR-0004).
//!
//! A workspace root must be an absolute path to a real directory, dedicated to
//! PaperWarden, with one [`crate::JobRunner`] per root. Each workspace is
//! created fresh (never reused), private to the app on Unix-like systems, and
//! holds an `input` and an `output` directory.

use crate::JobId;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const PREFIX: &str = "pw-";
const INPUT_DIR: &str = "input";
const OUTPUT_DIR: &str = "output";

/// Owns an isolated on-disk job workspace and removes it when dropped.
#[derive(Debug)]
pub struct Workspace {
    path: PathBuf,
    cleaned: bool,
}

/// What [`Workspace::recover_stale`] did.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecoveryReport {
    /// Abandoned workspaces removed.
    pub removed: usize,
    /// Abandoned workspaces that could not be removed; a later run retries.
    pub failed: usize,
}

impl Workspace {
    /// Creates a new, empty workspace for `job_id` inside `root`.
    ///
    /// # Errors
    ///
    /// Returns an error if `root` is relative, is a link or a file, or cannot
    /// be created, or if a workspace for this job already exists. An existing
    /// workspace is never touched.
    pub fn create(root: &Path, job_id: &JobId) -> io::Result<Self> {
        prepare_root(root)?;
        let path = root.join(format!("{PREFIX}{}", job_id.as_str()));
        create_private_dir(&path)?;
        // From here on, Drop removes the directory if setup fails.
        let workspace = Self {
            path,
            cleaned: false,
        };
        create_private_dir(&workspace.input_dir())?;
        create_private_dir(&workspace.output_dir())?;
        Ok(workspace)
    }

    /// Returns the workspace directory.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the directory that holds the staged input.
    pub fn input_dir(&self) -> PathBuf {
        self.path.join(INPUT_DIR)
    }

    /// Returns the directory that holds the job output.
    pub fn output_dir(&self) -> PathBuf {
        self.path.join(OUTPUT_DIR)
    }

    /// Whether [`cleanup`](Self::cleanup) has removed the workspace.
    pub fn is_cleaned(&self) -> bool {
        self.cleaned
    }

    /// Removes the workspace and everything in it. Links inside it are
    /// removed, never followed.
    ///
    /// Idempotent: a workspace that is already gone counts as cleaned.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory exists but cannot be removed. The
    /// workspace then stays dirty and removal is retried when it is dropped.
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

    /// Removes workspaces left in `root` by a crash or a failed cleanup. Run it
    /// at startup, before any job starts in `root`.
    ///
    /// Only real directories named like workspaces are removed. Links and
    /// unrelated entries are left untouched, and removal never follows links.
    /// A workspace that cannot be removed is counted and skipped, so one bad
    /// entry does not block the rest.
    ///
    /// # Errors
    ///
    /// Returns an error if `root` is a link or a file, or cannot be read.
    pub fn recover_stale(root: &Path) -> io::Result<RecoveryReport> {
        let mut report = RecoveryReport::default();
        match fs::symlink_metadata(root) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(report),
            Err(error) => return Err(error),
            Ok(metadata) => require_real_dir(&metadata)?,
        }
        for entry in fs::read_dir(root)? {
            let Ok(entry) = entry else {
                report.failed = report.failed.saturating_add(1);
                continue;
            };
            let is_workspace = entry.file_name().to_string_lossy().starts_with(PREFIX);
            // DirEntry::file_type does not follow links.
            let is_real_dir = entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink());
            if !(is_workspace && is_real_dir) {
                continue;
            }
            match fs::remove_dir_all(entry.path()) {
                Ok(()) => report.removed = report.removed.saturating_add(1),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => report.failed = report.failed.saturating_add(1),
            }
        }
        Ok(report)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        if !self.cleaned {
            let _ = self.cleanup();
        }
    }
}

fn prepare_root(root: &Path) -> io::Result<()> {
    if !root.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "workspace root must be an absolute path",
        ));
    }
    match fs::symlink_metadata(root) {
        Ok(metadata) => require_real_dir(&metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(root)?;
            require_real_dir(&fs::symlink_metadata(root)?)
        }
        Err(error) => Err(error),
    }
}

fn require_real_dir(metadata: &fs::Metadata) -> io::Result<()> {
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "workspace root must be a real directory, not a link or a file",
        ));
    }
    Ok(())
}

/// Creates one directory readable only by the app, failing if anything
/// already exists at `path`.
#[cfg(unix)]
fn create_private_dir(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(path)
}

/// Creates one directory, failing if anything already exists at `path`. App
/// storage on non-Unix platforms is already private to the app.
#[cfg(not(unix))]
fn create_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir(path)
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
    fn successful_job_can_clean_explicitly_and_idempotently() {
        let root = root("success");
        let mut workspace = Workspace::create(&root, &JobId::new("success-1").expect("valid id"))
            .expect("workspace created");
        let path = workspace.path().to_path_buf();
        assert!(workspace.input_dir().is_dir());
        assert!(workspace.output_dir().is_dir());
        fs::write(workspace.output_dir().join("output"), b"synthetic").expect("write output");
        workspace.cleanup().expect("workspace cleaned");
        workspace.cleanup().expect("second cleanup is a no-op");
        assert!(workspace.is_cleaned());
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
            fs::write(workspace.input_dir().join("input"), b"partial").expect("write partial");
        }
        assert!(!path.exists());
        fs::remove_dir(root).expect("remove empty root");
    }

    #[test]
    fn existing_workspace_is_never_reused_or_removed() {
        let root = root("existing");
        let id = JobId::new("same-1").expect("valid id");
        let first = Workspace::create(&root, &id).expect("first workspace");
        fs::write(first.input_dir().join("input"), b"first job").expect("write");
        let second = Workspace::create(&root, &id);
        assert_eq!(
            second.map(|_| ()).map_err(|error| error.kind()),
            Err(io::ErrorKind::AlreadyExists)
        );
        assert_eq!(
            fs::read(first.input_dir().join("input")).expect("first job intact"),
            b"first job".to_vec()
        );
        drop(first);
        fs::remove_dir(root).expect("remove empty root");
    }

    #[test]
    fn root_must_be_absolute_and_a_directory() {
        let id = JobId::new("root-1").expect("valid id");
        assert!(Workspace::create(Path::new("relative"), &id).is_err());
        let root = root("file-root");
        fs::write(&root, b"not a directory").expect("write file");
        assert!(Workspace::create(&root, &id).is_err());
        assert!(Workspace::recover_stale(&root).is_err());
        fs::remove_file(root).expect("remove file");
    }

    #[test]
    fn restart_recovery_removes_only_owned_directories() {
        let root = root("recovery");
        fs::create_dir_all(root.join("pw-stale-1")).expect("create stale workspace");
        fs::create_dir_all(root.join("unrelated")).expect("create unrelated directory");
        let report = Workspace::recover_stale(&root).expect("recover stale workspaces");
        assert_eq!(
            report,
            RecoveryReport {
                removed: 1,
                failed: 0
            }
        );
        assert!(root.join("unrelated").exists());
        fs::remove_dir_all(root).expect("remove test root");
    }
}
