use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperError};

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
    params.set_language(Some("en"));
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

    match state.full(params, &audio_16k) {
        Ok(_) => {}
        Err(WhisperError::GenericError(-6)) => {
            // Treat known short/silent decode failures as no-speech.
            return Ok(String::new());
        }
        Err(err) => return Err(err.into()),
    }

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
