use std::time::Duration;

// Hugging Face repo (Whisper models for whisper.cpp)
pub const HF_REPO_API: &str =
    "https://huggingface.co/api/models/ggerganov/whisper.cpp?expand=siblings";
pub const HF_RESOLVE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/"; // + rfilename
pub const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60); // 24h
