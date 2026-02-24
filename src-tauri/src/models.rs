use anyhow::{anyhow, Result};
use reqwest::blocking::Client;
use serde::Serialize;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub file_name: String,
    pub label: String,
    pub quality: String,
    pub installed: bool,
    pub active: bool,
    pub download_url: Option<String>,
}

struct KnownModel {
    file_name: &'static str,
    label: &'static str,
    quality: &'static str,
    download_url: &'static str,
}

const PREFERRED_ORDER: &[&str] = &[
    "ggml-large-v3-turbo-q5_0.bin",
    "ggml-large-v3-turbo.bin",
    "ggml-large-v3.bin",
    "ggml-medium.en.bin",
    "ggml-small.en.bin",
    "ggml-base.en.bin",
    "ggml-tiny.en.bin",
];

const KNOWN_MODELS: &[KnownModel] = &[
    KnownModel {
        file_name: "ggml-large-v3-turbo-q5_0.bin",
        label: "large-v3-turbo-q5_0",
        quality: "best balance",
        download_url:
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
    },
    KnownModel {
        file_name: "ggml-large-v3-turbo.bin",
        label: "large-v3-turbo",
        quality: "highest quality (fast)",
        download_url:
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
    },
    KnownModel {
        file_name: "ggml-large-v3.bin",
        label: "large-v3",
        quality: "highest quality",
        download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
    },
    KnownModel {
        file_name: "ggml-medium.en.bin",
        label: "medium.en",
        quality: "high quality",
        download_url:
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin",
    },
    KnownModel {
        file_name: "ggml-small.en.bin",
        label: "small.en",
        quality: "better than base",
        download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
    },
    KnownModel {
        file_name: "ggml-base.en.bin",
        label: "base.en",
        quality: "balanced",
        download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
    },
    KnownModel {
        file_name: "ggml-tiny.en.bin",
        label: "tiny.en",
        quality: "fastest",
        download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
    },
];

pub fn pick_default_model(models_dir: &Path) -> String {
    if let Some(best) = PREFERRED_ORDER
        .iter()
        .find(|name| models_dir.join(name).exists())
        .map(|name| (*name).to_string())
    {
        return best;
    }

    read_installed_model_files(models_dir)
        .ok()
        .and_then(|files| files.into_iter().next())
        .unwrap_or_else(|| "ggml-base.en.bin".to_string())
}

pub fn list_models(models_dir: &Path, active_model: &str) -> Result<Vec<ModelInfo>> {
    let installed_files = read_installed_model_files(models_dir)?;
    let installed_set: HashSet<String> = installed_files.iter().cloned().collect();

    let mut models = Vec::new();
    let mut seen = HashSet::new();

    for known in KNOWN_MODELS {
        let installed = installed_set.contains(known.file_name);
        models.push(ModelInfo {
            file_name: known.file_name.to_string(),
            label: known.label.to_string(),
            quality: known.quality.to_string(),
            installed,
            active: active_model == known.file_name,
            download_url: Some(known.download_url.to_string()),
        });
        seen.insert(known.file_name.to_string());
    }

    for file_name in installed_files {
        if seen.contains(&file_name) {
            continue;
        }

        models.push(ModelInfo {
            label: file_name.clone(),
            file_name: file_name.clone(),
            quality: "custom".to_string(),
            installed: true,
            active: active_model == file_name,
            download_url: None,
        });
    }

    Ok(models)
}

pub fn download_model<F>(models_dir: &Path, file_name: &str, mut on_progress: F) -> Result<()>
where
    F: FnMut(u8),
{
    let known = find_known_model(file_name).ok_or_else(|| {
        anyhow!(
            "No auto-download URL known for model '{}'. Add the file manually to the models folder.",
            file_name
        )
    })?;

    fs::create_dir_all(models_dir)?;

    let destination = models_dir.join(file_name);
    if destination.exists() {
        on_progress(100);
        return Ok(());
    }

    let partial = models_dir.join(format!("{file_name}.part"));
    let _ = fs::remove_file(&partial);

    let result = (|| -> Result<()> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(60 * 30))
            .build()?;

        let mut response = client
            .get(known.download_url)
            .header("User-Agent", "murmur/0.1")
            .send()?
            .error_for_status()?;

        let total_bytes = response.content_length();
        let mut file = File::create(&partial)?;

        let mut downloaded: u64 = 0;
        let mut last_percent: u8 = 0;
        let mut buffer = [0_u8; 64 * 1024];

        on_progress(0);

        loop {
            let read = response.read(&mut buffer)?;
            if read == 0 {
                break;
            }

            file.write_all(&buffer[..read])?;
            downloaded += read as u64;

            if let Some(total) = total_bytes {
                if total > 0 {
                    let percent = ((downloaded.saturating_mul(100)) / total).min(100) as u8;
                    if percent != last_percent {
                        last_percent = percent;
                        on_progress(percent);
                    }
                }
            }
        }

        file.flush()?;
        file.sync_all()?;

        fs::rename(&partial, &destination)?;
        on_progress(100);

        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }

    result
}

fn read_installed_model_files(models_dir: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    if !models_dir.exists() {
        return Ok(files);
    }

    for entry in fs::read_dir(models_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if path.extension().and_then(|e| e.to_str()) != Some("bin") {
            continue;
        }

        if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
            files.push(file_name.to_string());
        }
    }

    files.sort();
    Ok(files)
}

fn find_known_model(file_name: &str) -> Option<&'static KnownModel> {
    KNOWN_MODELS
        .iter()
        .find(|model| model.file_name == file_name)
}
