use crate::constants::HF_RESOLVE_URL;
use crate::hf::{HfFile, is_quantized_name};
use anyhow::{Result, anyhow};
use dialoguer::{Select, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};
use sanitize_filename::sanitize;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub fn pick_model_interactive(
    files: &[HfFile],
    prefer_quantized: bool,
    models_dir: &Path,
) -> Result<String> {
    if files.is_empty() {
        return Err(anyhow!("No models found in Hugging Face API response"));
    }

    let theme = ColorfulTheme::default();
    let items: Vec<String> = files
        .iter()
        .map(|f| {
            let full = if is_quantized_name(&f.rfilename.to_lowercase()) {
                "quant"
            } else {
                "full-precision"
            };
            let size = f.size.map(format_size).unwrap_or_else(|| "?".into());
            let local_flag = if models_dir.join(&f.rfilename).exists() {
                " (local)"
            } else {
                ""
            };
            format!("{}  [{} | {}]{}", f.rfilename, full, size, local_flag)
        })
        .collect();

    let prompt = if prefer_quantized {
        "Pick a Whisper model (quantized preferred)"
    } else {
        "Pick a Whisper model (full-precision preferred)"
    };

    let sel = Select::with_theme(&theme)
        .with_prompt(prompt)
        .default(0)
        .items(&items)
        .interact()?;

    Ok(files[sel].rfilename.clone())
}

pub fn resolve_or_download_model(
    user_input: &str,
    models_dir: &Path,
    files: &[HfFile],
    prefer_quantized: bool,
    verbose: bool,
) -> Result<PathBuf> {
    // Existing path?
    let path = PathBuf::from(user_input);
    if path.exists() {
        return Ok(path);
    }

    // Model file in models dir?
    let candidate = models_dir.join(user_input);
    if candidate.exists() {
        return Ok(candidate);
    }

    // Treat as alias like "large-v3" and find best match
    let needle = user_input.to_lowercase();

    // score: (0 better) exact contains + preference, else fallback
    let best = files
        .iter()
        .filter(|f| f.rfilename.to_lowercase().contains(&needle))
        .min_by_key(|f| {
            let name = f.rfilename.to_lowercase();
            let is_q = is_quantized_name(&name);
            let pref_penalty = if prefer_quantized ^ is_q { 1u8 } else { 0u8 };
            (pref_penalty, name)
        })
        .map(|f| f.rfilename.clone());

    let filename = best.ok_or_else(|| {
        anyhow!(
            "Could not find a model matching '{}' in HF repo",
            user_input
        )
    })?;

    download_model_if_missing(&filename, models_dir, verbose)
}

pub fn download_model_if_missing(
    filename: &str,
    models_dir: &Path,
    verbose: bool,
) -> Result<PathBuf> {
    let dest = models_dir.join(filename);
    if dest.exists() {
        return Ok(dest);
    }
    fs::create_dir_all(models_dir)?;
    let url = format!("{}{}?download=true", HF_RESOLVE_URL, filename);

    println!("⬇️  Downloading model: {}", filename);
    let resp = reqwest::blocking::get(&url)?.error_for_status()?;
    let total = resp.content_length();

    let pb = ProgressBar::new(total.unwrap_or(0));
    pb.set_style(
        ProgressStyle::with_template("{spinner} {msg} {bytes}/{total_bytes} ({bytes_per_sec})")
            .unwrap(),
    );
    pb.set_message("Downloading");

    let mut src = resp;
    let mut file = File::create(&dest)?;
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    loop {
        let n = src.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        if let Some(t) = total {
            pb.set_length(t);
        }
        pb.set_position(downloaded);
    }
    pb.finish_and_clear();

    if verbose {
        println!("Saved model to {}", dest.display());
    }
    Ok(dest)
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0usize;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.1} {}", size, UNITS[unit])
}

pub fn build_basename_from_wav(wav_path: &Path) -> String {
    sanitize(
        wav_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("audio"),
    )
}
