use clap::{Parser, ValueHint};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "vid2txt", version, about)]
pub struct Args {
    /// Video URL
    #[arg(value_hint = ValueHint::Url, required_unless_present = "list_models")]
    pub url: Option<String>,

    /// Output directory for WAV + transcript (.txt). Defaults to current dir
    #[arg(short, long)]
    pub out: Option<PathBuf>,

    /// Whisper model alias (e.g., large-v3) OR an existing file name/path
    #[arg(short, long)]
    pub model: Option<String>,

    /// Force language code for transcription (e.g. en, pt, es)
    #[arg(long, default_value = "auto")]
    pub language: String,

    /// Number of threads for whisper-cli (-t)
    #[arg(long)]
    pub threads: Option<u32>,

    /// Show command output from yt-dlp/whisper-cli
    #[arg(short, long)]
    pub verbose: bool,

    /// List available remote Whisper models and exit
    #[arg(long)]
    pub list_models: bool,

    /// Prefer quantized models first when listing/picking
    #[arg(long)]
    pub prefer_quantized: bool,

    /// Force refreshing the model list from Hugging Face, ignoring cache
    #[arg(long)]
    pub refresh_models: bool,
}
