use crate::cli::Args;
use crate::cmd::{ensure_in_path, run_cmd};
use crate::fs_utils::{create_dir_all, find_first_with_ext, whisper_models_dir};
use crate::hf::fetch_hf_files_cached;
use crate::models::{build_basename_from_wav, pick_model_interactive, resolve_or_download_model};
use anyhow::{Context, Result, anyhow};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as PCommand;
use tempfile::tempdir;

/// Return true if `s` looks like a *remote* URL we should hand to yt-dlp.
/// Accepts schemes (http/https/ftp), protocol-relative //host, or bare domains like example.com/path.
/// Refuses obvious local paths: drive paths, UNC, relative .\ or ./, POSIX absolute, and file://.
fn is_probable_url(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }

    // Reject obvious local paths first
    let starts_with_dot = s.starts_with(".\\")
        || s.starts_with("./")
        || s.starts_with("..\\")
        || s.starts_with("../");
    let looks_like_posix_abs = s.starts_with('/') || s.starts_with("~/");
    if starts_with_dot || looks_like_posix_abs {
        return false;
    }
    // Windows drive, e.g., C:\ or D:/ ...
    if s.len() >= 3 {
        let bytes = s.as_bytes();
        if bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/')
            && bytes[0].is_ascii_alphabetic()
        {
            return false;
        }
    }
    // UNC path \\server\share
    if s.starts_with("\\\\") {
        return false;
    }
    // file:// is local
    if s.to_ascii_lowercase().starts_with("file://") {
        return false;
    }

    // Known schemes
    if Regex::new(r"(?i)^(?:https?|ftp)://").unwrap().is_match(s) {
        return true;
    }
    // Protocol-relative
    if s.starts_with("//") {
        return true;
    }

    // Bare domain: one or more labels, then TLD (letters only), then optional path/query/fragment.
    // Example matches: example.com, www.example.co.uk/path?x, youtu.be/xyz
    // Example non-matches: report.v1, D3.3 (numeric TLD), file names with dots
    let bare_domain = Regex::new(r"^(?:[A-Za-z0-9-]+\.)+[A-Za-z]{2,63}(?:[/:?#][^\s]*)?$").unwrap();
    bare_domain.is_match(s)
}

/// If `candidate` doesn’t exist and has no extension, try common media extensions in the same folder.
/// Returns the first existing path found.
fn try_infer_with_exts(candidate: PathBuf) -> Option<PathBuf> {
    if candidate.exists() {
        return Some(candidate);
    }

    let parent = candidate
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().ok().unwrap_or_default());

    let stem_os = candidate.file_name()?;
    let stem = stem_os.to_string_lossy();

    // Only try if the provided name lacks an extension
    if Path::new(&*stem).extension().is_some() || candidate.extension().is_some() {
        return None;
    }

    let exts = [
        "mp4", "mkv", "webm", "mov", "m4a", "mp3", "wav", "flac", "avi", "m4v", "aac", "opus",
    ];
    for ext in exts {
        let p = parent.join(format!("{stem}.{ext}"));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

pub fn run() -> Result<()> {
    let args = Args::parse();

    let out_dir = args
        .out
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    let verbose = args.verbose;

    // whisper-cli always needed; ffmpeg always needed; yt-dlp only for remote URLs.
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

    let input = args
        .url
        .clone()
        .ok_or_else(|| anyhow!("No input provided. Pass a URL or a local video path."))?;

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner} {msg}")
            .unwrap()
            .tick_chars("⠇⠋⠙⠸⠴⠦⠇"),
    );

    // We'll set these based on the branch (URL vs local)
    let final_wav: PathBuf;
    let base_name: String;

    if is_probable_url(&input) {
        // Remote URL → use yt-dlp
        ensure_in_path("yt-dlp")?;

        pb.set_message("Downloading & extracting audio (yt-dlp)…");

        // Temporary working directory for yt-dlp
        let temp = tempdir()?;
        let temp_path = temp.path();

        // yt-dlp → WAV (highest quality)
        let output_tpl = temp_path.join("%(title)s.%(ext)s");
        let status = run_cmd(
            PCommand::new("yt-dlp")
                .arg(&input)
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

        // Build a nice base name and move WAV to destination
        base_name = build_basename_from_wav(&wav_path);
        final_wav = out_dir.join(format!("{base_name}.wav"));

        fs::rename(&wav_path, &final_wav)
            .or_else(|_| fs::copy(&wav_path, &final_wav).and_then(|_| fs::remove_file(&wav_path)))
            .with_context(|| format!("Failed to move WAV to {}", final_wav.display()))?;
    } else {
        // Local file → use ffmpeg directly
        pb.set_message("Extracting audio from local file (ffmpeg)…");

        // Resolve relative/absolute (don’t require existence yet)
        let candidate = {
            let p = PathBuf::from(&input);
            if p.is_absolute() {
                p
            } else {
                std::env::current_dir()
                    .context("Failed to resolve current working directory")?
                    .join(p)
            }
        };

        // If missing extension / not found, try common media extensions
        let input_path = if candidate.exists() {
            candidate
        } else if let Some(found) = try_infer_with_exts(candidate.clone()) {
            found
        } else {
            // Last attempt: normalize just for a nicer error message
            let display_cand = candidate.canonicalize().unwrap_or(candidate.clone());
            pb.finish_and_clear();
            return Err(anyhow!(
                "Input file not found. Tried: {}\nHint: include the extension or use one of: .mp4 .mkv .webm .mov .m4a .mp3 .wav .flac .avi .m4v .aac .opus",
                display_cand.display()
            ));
        };

        // Canonicalize (best-effort) for cleaner messages
        let display_path = input_path
            .canonicalize()
            .unwrap_or_else(|_| input_path.clone());

        if !input_path.is_file() {
            pb.finish_and_clear();
            return Err(anyhow!("Input is not a file: {}", display_path.display()));
        }

        // Base name from the input file
        base_name = input_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "audio".to_string());

        final_wav = out_dir.join(format!("{base_name}.wav"));

        // ffmpeg: extract PCM WAV (mono, 16 kHz — great default for STT)
        let status = run_cmd(
            PCommand::new("ffmpeg")
                .arg("-y") // overwrite if exists
                .arg("-i")
                .arg(&input_path)
                .arg("-vn")
                .arg("-acodec")
                .arg("pcm_s16le")
                .arg("-ar")
                .arg("16000")
                .arg("-ac")
                .arg("1")
                .arg(&final_wav),
            verbose,
        )?;
        if !status.success() {
            pb.finish_and_clear();
            return Err(anyhow!(
                "ffmpeg failed to extract audio from {}",
                display_path.display()
            ));
        }
    }

    pb.set_message("Transcribing with whisper-cli…");

    // whisper-cli flags: -m <model> -f <wav> -otxt -of <output_base> -l <lang> [-t <threads>]
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
