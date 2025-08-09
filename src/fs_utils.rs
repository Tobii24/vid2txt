use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn find_first_with_ext(dir: &Path, ext: &str) -> Result<Option<PathBuf>> {
    let ext_lc = ext.to_ascii_lowercase();
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_file() {
            if let Some(e) = p.extension().and_then(|s| s.to_str()) {
                if e.eq_ignore_ascii_case(&ext_lc) {
                    return Ok(Some(p.to_path_buf()));
                }
            }
        }
    }
    Ok(None)
}

pub fn whisper_models_dir() -> Result<PathBuf> {
    let cli = which::which("whisper-cli").context("Cannot locate whisper-cli in PATH")?;
    let parent = cli
        .parent()
        .ok_or_else(|| anyhow!("Unexpected whisper-cli path"))?;
    Ok(parent.join("models"))
}

pub fn create_dir_all(p: &Path) -> Result<()> {
    fs::create_dir_all(p).with_context(|| format!("Failed to create dir: {}", p.display()))
}
