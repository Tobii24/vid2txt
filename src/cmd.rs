use anyhow::{Context, Result};
use std::process::{Command, ExitStatus};

pub fn ensure_in_path(bin: &str) -> Result<()> {
    which::which(bin)
        .with_context(|| format!("Required tool '{}' not found in PATH", bin))
        .map(|_| ())
}

pub fn run_cmd(cmd: &mut Command, verbose: bool) -> Result<ExitStatus> {
    if verbose {
        Ok(cmd.status()?)
    } else {
        let output = cmd.output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("{stderr}");
        }
        Ok(output.status)
    }
}
