use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperError,
};

#[derive(Debug, Clone, Copy)]
struct SignalStats {
    rms: f32,
    active_ratio: f32,
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

    let audio_16k = resample_to_16k(input, sample_rate);
    if audio_16k.is_empty() {
        return Ok(String::new());
    }
    if audio_16k.len() < MIN_AUDIO_SAMPLES_16K {
        // Very short captures often fail inside whisper with a generic error.
        return Ok(String::new());
    }

    let signal = analyze_signal(&audio_16k);

    let model_path_str = model_path.to_string_lossy();
    let ctx = WhisperContext::new_with_params(
        model_path_str.as_ref(),
        WhisperContextParameters::default(),
    )?;

    let mut state = ctx.create_state()?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 2 });

    let threads = std::thread::available_parallelism()
        .map(|n| n.get().clamp(2, 8) as i32)
        .unwrap_or(4);

    params.set_n_threads(threads);
    params.set_translate(false);

    let model_name = model_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if model_name.ends_with(".en.bin") {
        params.set_language(Some("en"));
    } else {
        params.set_language(None);
        params.set_detect_language(true);
    }

    params.set_no_context(true);
    params.set_no_timestamps(true);
    params.set_suppress_blank(true);
    params.set_suppress_non_speech_tokens(true);
    params.set_temperature(0.0);
    params.set_temperature_inc(0.0);
    params.set_entropy_thold(2.4);
    params.set_logprob_thold(-1.0);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);

    if let Some(cancel_flag) = cancel_flag {
        params.set_abort_callback_safe(move || cancel_flag.load(Ordering::Relaxed));
    }

    match state.full(params, &audio_16k) {
        Ok(_) => {}
        Err(WhisperError::GenericError(-6)) => {
            // Treat known short/silent decode failures as no-speech.
            return Ok(String::new());
        }
        Err(err) => return Err(err.into()),
    }

    let mut text = String::new();
    let mut token_prob_sum = 0.0_f32;
    let mut token_prob_count = 0_usize;

    let n_segments = state.full_n_segments()?;
    for idx in 0..n_segments {
        let segment = state.full_get_segment_text(idx)?;
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }

        let n_tokens = state.full_n_tokens(idx)?;
        for token_idx in 0..n_tokens {
            if let Ok(prob) = state.full_get_token_prob(idx, token_idx) {
                if prob.is_finite() {
                    token_prob_sum += prob;
                    token_prob_count += 1;
                }
            }
        }

        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(trimmed);
    }

    let avg_token_prob = if token_prob_count > 0 {
        token_prob_sum / token_prob_count as f32
    } else {
        1.0
    };

    if is_likely_hallucination(text.as_str(), signal, avg_token_prob) {
        return Ok(String::new());
    }

    Ok(text)
}

fn analyze_signal(samples: &[f32]) -> SignalStats {
    if samples.is_empty() {
        return SignalStats {
            rms: 0.0,
            active_ratio: 0.0,
        };
    }

    let mut sum_sq = 0.0_f64;
    let mut active_count = 0_usize;

    for &s in samples {
        let abs = s.abs();
        if abs > 0.01 {
            active_count += 1;
        }
        sum_sq += (s as f64) * (s as f64);
    }

    SignalStats {
        rms: (sum_sq / samples.len() as f64).sqrt() as f32,
        active_ratio: active_count as f32 / samples.len() as f32,
    }
}

fn is_likely_hallucination(text: &str, signal: SignalStats, avg_token_prob: f32) -> bool {
    let key = text
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphabetic() || c.is_ascii_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let short_hallucination = matches!(
        key.as_str(),
        "you"
            | "thank you"
            | "thanks"
            | "thanks for watching"
            | "bye"
            | "okay"
            | "so"
            | "hmm"
            | "um"
    );

    if !short_hallucination {
        return false;
    }

    // Only suppress short canned outputs when the captured signal itself is very weak
    // or confidence is especially poor.
    signal.rms < 0.0012
        || signal.active_ratio < 0.004
        || (signal.rms < 0.003 && avg_token_prob < 0.30)
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
