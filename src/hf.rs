use crate::constants::{CACHE_TTL, HF_REPO_API};
use anyhow::{Result, anyhow};
use dirs::cache_dir;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HfFile {
    pub rfilename: String,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct HfModel {
    #[serde(default)]
    siblings: Vec<HfFile>,
}

pub fn cache_file_path() -> Result<PathBuf> {
    let base = cache_dir().ok_or_else(|| anyhow!("Cannot determine cache directory"))?;
    Ok(base.join("vid2txt").join("models.json"))
}

pub fn fetch_hf_files_cached(refresh: bool, prefer_quantized: bool) -> Result<Vec<HfFile>> {
    let path = cache_file_path()?;
    if !refresh {
        if let Ok(meta) = fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                if modified.elapsed().unwrap_or_else(|_| CACHE_TTL * 2) < CACHE_TTL {
                    if let Ok(bytes) = fs::read(&path) {
                        if let Ok(model) = serde_json::from_slice::<HfModel>(&bytes) {
                            return Ok(filter_and_sort_files(model.siblings, prefer_quantized));
                        }
                    }
                }
            }
        }
    }

    let resp = reqwest::blocking::get(HF_REPO_API)?.error_for_status()?;
    let model: HfModel = resp.json()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_vec(&model)?)?;

    Ok(filter_and_sort_files(model.siblings, prefer_quantized))
}

pub fn filter_and_sort_files(files: Vec<HfFile>, prefer_quantized: bool) -> Vec<HfFile> {
    // accept legacy ggml and new gguf names
    let re = Regex::new(r"^ggml-.*\.(bin|gguf)$").unwrap();

    let mut v: Vec<HfFile> = files
        .into_iter()
        .filter(|f| re.is_match(&f.rfilename))
        .collect();

    // stable sort: (quantized preference first), then lexical name
    v.sort_by(|a, b| {
        let (an, bn) = (a.rfilename.to_lowercase(), b.rfilename.to_lowercase());
        let aq = is_quantized_name(&an);
        let bq = is_quantized_name(&bn);

        // if prefer_quantized: quantized first; else full first
        let a_score = if prefer_quantized { !aq } else { aq } as u8; // false < true
        let b_score = if prefer_quantized { !bq } else { bq } as u8;

        a_score.cmp(&b_score).then_with(|| an.cmp(&bn))
    });

    v
}

pub fn is_quantized_name(name: &str) -> bool {
    // rough heuristics for whisper.cpp repo
    name.contains("-q") || name.contains(".q") || name.contains("-q5") || name.contains("-q8")
}
