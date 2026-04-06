use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Creates a temporary workspace with a `.puffer` directory ready for tests.
pub fn temp_workspace() -> Result<(TempDir, PathBuf)> {
    let tempdir = tempfile::tempdir()?;
    let workspace = tempdir.path().join("workspace");
    fs::create_dir_all(workspace.join(".puffer"))?;
    Ok((tempdir, workspace))
}

/// Reads a UTF-8 file into a string for tests.
pub fn read_to_string(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?)
}
