use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperError,
};

#[derive(Clone, Copy)]
enum LanguageMode {
    English,
    AutoDetect,
}

#[derive(Clone, Copy)]
struct DecodeAttempt {
    language: LanguageMode,
    best_of: i32,
    threads: i32,
}

pub fn transcribe(
    model_path: &Path,
    input: &[f32],
    sample_rate: u32,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> Result<String> {
    const MIN_AUDIO_SAMPLES_16K: usize = 3_200; // 200ms at 16kHz

    if input.is_empty() {
        return Ok(String::new());
    }

    if !model_path.exists() {
        return Err(anyhow!(
            "Model not found at {}. Place a ggml model in your models folder and select it.",
            model_path.display()
        ));
    }

    let audio_16k = preprocess_audio(&resample_to_16k(input, sample_rate));
    if audio_16k.is_empty() {
        return Ok(String::new());
    }
    if audio_16k.len() < MIN_AUDIO_SAMPLES_16K {
        // Very short captures often fail inside whisper with a generic error.
        return Ok(String::new());
    }

    let model_path_str = model_path.to_string_lossy();
    let ctx = WhisperContext::new_with_params(
        model_path_str.as_ref(),
        WhisperContextParameters::default(),
    )?;

    let threads = std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 6) as i32)
        .unwrap_or(4);

    // Retry with progressively simpler decode settings when whisper returns
    // known transient decode failures (notably -7 on some systems/models).
    let attempts = [
        DecodeAttempt {
            language: LanguageMode::English,
            best_of: 2,
            threads,
        },
        DecodeAttempt {
            language: LanguageMode::English,
            best_of: 1,
            threads: threads.clamp(1, 3),
        },
        DecodeAttempt {
            language: LanguageMode::AutoDetect,
            best_of: 1,
            threads: threads.clamp(1, 3),
        },
    ];

    let mut saw_recoverable_decode_error = false;
    for attempt in attempts {
        match decode_once(&ctx, &audio_16k, cancel_flag.clone(), attempt) {
            Ok(text) => {
                if !text.trim().is_empty() {
                    return Ok(text);
                }
            }
            Err(WhisperError::GenericError(-6)) | Err(WhisperError::GenericError(-7)) => {
                saw_recoverable_decode_error = true;
            }
            Err(err) => return Err(err.into()),
        }
    }

    if saw_recoverable_decode_error {
        eprintln!("whisper: decode produced recoverable errors (-6/-7) across all attempts");
    }

    Ok(String::new())
}

fn decode_once(
    ctx: &WhisperContext,
    audio_16k: &[f32],
    cancel_flag: Option<Arc<AtomicBool>>,
    attempt: DecodeAttempt,
) -> std::result::Result<String, WhisperError> {
    let mut state = ctx.create_state()?;
    let mut params = FullParams::new(SamplingStrategy::Greedy {
        best_of: attempt.best_of,
    });

    params.set_n_threads(attempt.threads);
    params.set_translate(false);
    match attempt.language {
        LanguageMode::English => {
            params.set_language(Some("en"));
        }
        LanguageMode::AutoDetect => {
            params.set_language(None);
            params.set_detect_language(true);
        }
    }
    params.set_no_context(true);
    params.set_no_timestamps(true);
    params.set_suppress_blank(true);
    params.set_temperature(0.0);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);

    if let Some(cancel_flag) = cancel_flag {
        params.set_abort_callback_safe(move || cancel_flag.load(Ordering::Relaxed));
    }

    state.full(params, audio_16k)?;

    let mut text = String::new();
    let n_segments = state.full_n_segments()?;
    for idx in 0..n_segments {
        let segment = state.full_get_segment_text(idx)?;
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }

        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(trimmed);
    }

    Ok(text)
}

fn preprocess_audio(samples: &[f32]) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(samples.len());
    let mut sum_sq = 0.0_f64;
    let mut finite_count = 0_usize;

    for &sample in samples {
        let cleaned = if sample.is_finite() {
            sample.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        out.push(cleaned);
        sum_sq += (cleaned as f64) * (cleaned as f64);
        finite_count += 1;
    }

    if finite_count == 0 {
        return out;
    }

    // Light automatic gain for very quiet captures to reduce false no-speech.
    let rms = (sum_sq / finite_count as f64).sqrt() as f32;
    if rms > 0.0005 && rms < 0.035 {
        let gain = (0.05 / rms).clamp(1.0, 12.0);
        if gain > 1.05 {
            for sample in &mut out {
                *sample = (*sample * gain).clamp(-1.0, 1.0);
            }
        }
    }

    out
}

fn resample_to_16k(input: &[f32], source_rate: u32) -> Vec<f32> {
    const TARGET_RATE: u32 = 16_000;

    if source_rate == TARGET_RATE {
        return input.to_vec();
    }

    if source_rate == 0 || input.is_empty() {
        return Vec::new();
    }

    let ratio = source_rate as f64 / TARGET_RATE as f64;
    let output_len = ((input.len() as f64) / ratio).floor() as usize;

    let mut output = Vec::with_capacity(output_len);
    for n in 0..output_len {
        let src_pos = n as f64 * ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;

        let a = *input.get(idx).unwrap_or(&0.0);
        let b = *input.get(idx + 1).unwrap_or(&a);
        output.push(a + (b - a) * frac);
    }

    output
}
