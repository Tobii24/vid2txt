# vid2txt

A fast and convenient CLI tool that:

1. Downloads audio from any supported site via [`yt-dlp`](https://github.com/yt-dlp/yt-dlp)
2. Converts it to highest-quality WAV using `ffmpeg`
3. Transcribes it with [`whisper-cli`](https://github.com/ggerganov/whisper.cpp)
4. Outputs a `.txt` transcript (with optional custom output directory)
5. Lets you interactively choose and install Whisper models from Hugging Face
6. Caches the model list for faster runs, with options to refresh and prefer quantized models
7. Works on Windows, macOS, and Linux

---

## Features

- **Site-agnostic** — works with YouTube, Vimeo, SoundCloud, and any source `yt-dlp` supports.
- **Interactive model selection** — pick from all available Whisper models (full-precision or quantized) right from the CLI.
- **Model installer** — missing models are downloaded automatically into `whisper-cli`’s `models/` folder.
- **Cache system** — model list is cached for 24h; use `--refresh-models` to fetch fresh data.
- **Windows-safe filenames** — avoids invalid path characters.
- **Verbose mode** — debug problems by showing `yt-dlp` and `whisper-cli` output.

---

## Requirements

Install and ensure the following are available in your system `PATH`:

- [yt-dlp](https://github.com/yt-dlp/yt-dlp)
- [ffmpeg](https://ffmpeg.org/)
- [whisper-cli](https://github.com/ggerganov/whisper.cpp) (built from whisper.cpp)

---

## Installation

```bash
# Clone the repo
git clone https://github.com/Tobii24/vid2txt

cd vid2txt

# Build in release mode
cargo build --release
```
