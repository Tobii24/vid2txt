use crate::cli::Args;
use crate::cmd::{ensure_in_path, run_cmd};
use crate::fs_utils::{create_dir_all, find_first_with_ext, whisper_models_dir};
use crate::hf::fetch_hf_files_cached;
use crate::models::{build_basename_from_wav, pick_model_interactive, resolve_or_download_model};
use anyhow::{Context, Result, anyhow};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::path::PathBuf;
use std::process::Command as PCommand;
use tempfile::tempdir;

pub fn run() -> Result<()> {
    let args = Args::parse();

    let out_dir = args
        .out
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    let verbose = args.verbose;

    // Ensure dependencies exist
    ensure_in_path("yt-dlp")?;
    ensure_in_path("ffmpeg")?;
    ensure_in_path("whisper-cli")?;

    // Determine models dir next to whisper-cli binary
    let models_dir = whisper_models_dir()?;
    create_dir_all(&models_dir)?;

    // Cache-aware fetch of HF file list (order already honors preference)
    let files = fetch_hf_files_cached(args.refresh_models, args.prefer_quantized)?;

    // --list-models mode
    if args.list_models {
        if files.is_empty() {
            return Err(anyhow!("No models found in Hugging Face API response"));
        }
        println!("Available models ({}):", files.len());
        for f in &files {
            let size = f
                .size
                .map(|s| crate::models::format_size(s))
                .unwrap_or_default();
            if size.is_empty() {
                println!("- {}", f.rfilename);
            } else {
                println!("- {} ({size})", f.rfilename);
            }
        }
        return Ok(());
    }

    // Decide model path: provided alias/path or interactive picker
    let model_path = if let Some(m) = args.model.clone() {
        resolve_or_download_model(&m, &models_dir, &files, args.prefer_quantized, verbose)?
    } else {
        let picked = pick_model_interactive(&files, args.prefer_quantized, &models_dir)?;
        resolve_or_download_model(&picked, &models_dir, &files, args.prefer_quantized, verbose)?
    };

    // Create output directory if missing
    create_dir_all(&out_dir)?;

    // Temporary working directory for yt-dlp
    let temp = tempdir()?;
    let temp_path = temp.path();

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner} {msg}")
            .unwrap()
            .tick_chars("⠇⠋⠙⠸⠴⠦⠇"),
    );
    pb.set_message("Downloading & extracting video audio");

    // yt-dlp → WAV (highest quality)
    let output_tpl = temp_path.join("%(title)s.%(ext)s");
    let status = run_cmd(
        PCommand::new("yt-dlp")
            .arg(&args.url.unwrap())
            .arg("-f")
            .arg("bestaudio/best")
            .arg("--extract-audio")
            .arg("--audio-format")
            .arg("wav")
            .arg("--audio-quality")
            .arg("0")
            .arg("--restrict-filenames")
            .arg("--windows-filenames")
            .arg("-o")
            .arg(output_tpl.display().to_string()),
        verbose,
    )?;
    if !status.success() {
        pb.finish_and_clear();
        return Err(anyhow!("yt-dlp failed"));
    }

    // Find the produced WAV file
    let wav_path = find_first_with_ext(temp_path, "wav")?
        .ok_or_else(|| anyhow!("No WAV file produced by yt-dlp"))?;

    // Build a nice base name and final paths
    let base_name = build_basename_from_wav(&wav_path);
    let final_wav = out_dir.join(format!("{base_name}.wav"));

    // Move/copy the WAV to destination
    fs::rename(&wav_path, &final_wav)
        .or_else(|_| fs::copy(&wav_path, &final_wav).and_then(|_| fs::remove_file(&wav_path)))
        .with_context(|| format!("Failed to move WAV to {}", final_wav.display()))?;

    pb.set_message("Transcribing with whisper-cli…");

    // whisper-cli flags: -m <model> -f <wav> -otxt -of <output_base> [optional: -l <lang> -t <threads>]
    let output_base = PathBuf::from(&out_dir).join(&base_name);

    let mut whisper = PCommand::new("whisper-cli");
    whisper.arg("-m").arg(&model_path);
    whisper.arg("-f").arg(&final_wav);
    whisper.arg("-otxt");
    whisper.arg("-of").arg(&output_base);
    whisper.arg("-l").arg(&args.language);

    if let Some(t) = args.threads {
        whisper.arg("-t").arg(t.to_string());
    }

    let status = run_cmd(&mut whisper, verbose)?;
    if !status.success() {
        pb.finish_and_clear();
        return Err(anyhow!("whisper-cli failed"));
    }

    pb.finish_and_clear();

    let transcript_txt = out_dir.join(format!("{base_name}.txt"));
    if transcript_txt.exists() {
        println!("✅ Done! Transcript: {}", transcript_txt.display());
        println!("Model used: {}", model_path.display());
        println!("WAV saved at: {}", final_wav.display());
    } else {
        println!(
            "⚠️ whisper-cli ran, but no .txt was found at {}",
            transcript_txt.display()
        );
    }

    Ok(())
}
