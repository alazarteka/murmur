use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

#[cfg(target_os = "macos")]
use block2::RcBlock;
#[cfg(target_os = "macos")]
use objc2::runtime::Bool;
#[cfg(target_os = "macos")]
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

pub struct RecordingSession {
    stop_tx: mpsc::Sender<()>,
    worker: Option<JoinHandle<()>>,
    pub samples: Arc<Mutex<Vec<f32>>>,
    reached_capacity: Arc<AtomicBool>,
    pub sample_rate: u32,
    pub started_at: Instant,
}

pub struct CapturedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub duration_ms: i64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct CaptureSignalStats {
    pub rms: f32,
    pub peak: f32,
    pub active_ratio: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioInputStatus {
    pub available_inputs: usize,
    pub default_input: Option<String>,
    pub default_sample_rate: Option<u32>,
    pub ok: bool,
    pub message: Option<String>,
}

pub fn input_status() -> AudioInputStatus {
    let host = cpal::default_host();

    let (available_inputs, list_err) = match host.input_devices() {
        Ok(devices) => (devices.count(), None),
        Err(err) => (0, Some(err.to_string())),
    };

    let default_device = host.default_input_device();
    let default_input = default_device
        .as_ref()
        .and_then(|device| device.name().ok())
        .filter(|name| !name.trim().is_empty());

    let default_sample_rate = default_device
        .as_ref()
        .and_then(|device| device.default_input_config().ok())
        .map(|cfg| cfg.sample_rate().0);

    let message = if let Some(err) = list_err {
        Some(format!("Failed to enumerate input devices: {err}"))
    } else if default_input.is_none() {
        Some(
            "No default microphone detected. Check System Settings > Privacy & Security > Microphone."
                .to_string(),
        )
    } else {
        None
    };

    AudioInputStatus {
        available_inputs,
        default_input,
        default_sample_rate,
        ok: message.is_none(),
        message,
    }
}

pub fn start_capture(max_seconds: u32) -> Result<RecordingSession> {
    #[cfg(target_os = "macos")]
    ensure_microphone_permission()?;

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| {
            anyhow!(
                "No input microphone device found. Check System Settings > Privacy & Security > Microphone."
            )
        })?;

    let supported = device.default_input_config().map_err(|err| {
        anyhow!(
            "Failed to access microphone configuration: {err}. Verify microphone permissions and input device availability."
        )
    })?;
    let sample_rate = supported.sample_rate().0;
    let channels = usize::from(supported.channels());
    let config: StreamConfig = supported.clone().into();

    let max_samples = sample_rate as usize * max_seconds as usize;
    let samples = Arc::new(Mutex::new(Vec::<f32>::with_capacity(max_samples)));
    let samples_for_thread = Arc::clone(&samples);
    let reached_capacity = Arc::new(AtomicBool::new(false));
    let capacity_for_thread = Arc::clone(&reached_capacity);

    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();

    let worker = thread::spawn(move || {
        let err_fn = |err| eprintln!("audio stream error: {err}");

        let stream = match supported.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _| {
                    push_samples_f32(
                        data,
                        channels,
                        max_samples,
                        &samples_for_thread,
                        &capacity_for_thread,
                    )
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => {
                let samples_for_thread = Arc::clone(&samples_for_thread);
                let capacity_for_thread = Arc::clone(&capacity_for_thread);
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        push_samples_i16(
                            data,
                            channels,
                            max_samples,
                            &samples_for_thread,
                            &capacity_for_thread,
                        )
                    },
                    err_fn,
                    None,
                )
            }
            SampleFormat::U16 => {
                let samples_for_thread = Arc::clone(&samples_for_thread);
                let capacity_for_thread = Arc::clone(&capacity_for_thread);
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        push_samples_u16(
                            data,
                            channels,
                            max_samples,
                            &samples_for_thread,
                            &capacity_for_thread,
                        )
                    },
                    err_fn,
                    None,
                )
            }
            _ => Err(cpal::BuildStreamError::StreamConfigNotSupported),
        };

        match stream {
            Ok(stream) => {
                if let Err(err) = stream.play() {
                    let _ = ready_tx.send(Err(anyhow!("Failed to start audio stream: {err}")));
                    return;
                }
                let _ = ready_tx.send(Ok(()));
                let _ = stop_rx.recv();
                drop(stream);
            }
            Err(err) => {
                let _ = ready_tx.send(Err(anyhow!("Failed to build audio stream: {err}")));
            }
        }
    });

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(RecordingSession {
            stop_tx,
            worker: Some(worker),
            samples,
            reached_capacity,
            sample_rate,
            started_at: Instant::now(),
        }),
        Ok(Err(err)) => {
            let _ = worker.join();
            Err(err)
        }
        Err(err) => {
            let _ = worker.join();
            Err(anyhow!("Failed to initialize audio thread: {err}"))
        }
    }
}

#[cfg(target_os = "macos")]
fn ensure_microphone_permission() -> Result<()> {
    use std::sync::mpsc;
    use std::time::Duration;

    let media_type = unsafe { AVMediaTypeAudio }
        .ok_or_else(|| anyhow!("Failed to resolve AVMediaTypeAudio for microphone permission"))?;

    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) };
    match status {
        AVAuthorizationStatus::Authorized => Ok(()),
        AVAuthorizationStatus::Restricted => Err(anyhow!(
            "Microphone access is restricted by system policy."
        )),
        AVAuthorizationStatus::Denied => Err(anyhow!(
            "Microphone access denied. Enable Murmur in System Settings > Privacy & Security > Microphone."
        )),
        AVAuthorizationStatus::NotDetermined => {
            let (tx, rx) = mpsc::channel::<bool>();
            let handler = RcBlock::new(move |granted: Bool| {
                let _ = tx.send(granted.as_bool());
            });
            unsafe {
                AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &handler);
            }
            match rx.recv_timeout(Duration::from_secs(20)) {
                Ok(true) => Ok(()),
                Ok(false) => Err(anyhow!(
                    "Microphone access denied. Enable Murmur in System Settings > Privacy & Security > Microphone."
                )),
                Err(_) => Err(anyhow!(
                    "Microphone permission prompt timed out. Open System Settings > Privacy & Security > Microphone and enable Murmur."
                )),
            }
        }
        _ => Err(anyhow!("Unknown microphone authorization status.")),
    }
}

pub fn stop_capture(mut session: RecordingSession) -> CapturedAudio {
    let _ = session.stop_tx.send(());
    if let Some(worker) = session.worker.take() {
        let _ = worker.join();
    }

    let duration_ms = session.started_at.elapsed().as_millis() as i64;
    let samples = session
        .samples
        .lock()
        .map_or_else(|_| Vec::new(), |buf| buf.clone());
    let truncated = session.reached_capacity.load(Ordering::Relaxed);

    CapturedAudio {
        samples,
        sample_rate: session.sample_rate,
        duration_ms,
        truncated,
    }
}

pub fn analyze_signal(samples: &[f32]) -> CaptureSignalStats {
    if samples.is_empty() {
        return CaptureSignalStats {
            rms: 0.0,
            peak: 0.0,
            active_ratio: 0.0,
        };
    }

    let mut sum_sq = 0.0_f64;
    let mut peak = 0.0_f32;
    let mut active = 0_usize;

    for &sample in samples {
        let abs = sample.abs();
        peak = peak.max(abs);
        if abs > 0.01 {
            active += 1;
        }
        sum_sq += (sample as f64) * (sample as f64);
    }

    CaptureSignalStats {
        rms: (sum_sq / samples.len() as f64).sqrt() as f32,
        peak,
        active_ratio: active as f32 / samples.len() as f32,
    }
}

fn push_samples_f32(
    data: &[f32],
    channels: usize,
    max_samples: usize,
    out: &Arc<Mutex<Vec<f32>>>,
    truncated: &Arc<AtomicBool>,
) {
    let mut buffer = match out.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    append_mono(data, channels, max_samples, truncated, &mut buffer, |s| s);
}

fn push_samples_i16(
    data: &[i16],
    channels: usize,
    max_samples: usize,
    out: &Arc<Mutex<Vec<f32>>>,
    truncated: &Arc<AtomicBool>,
) {
    let mut buffer = match out.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    append_mono(data, channels, max_samples, truncated, &mut buffer, |s| {
        s as f32 / i16::MAX as f32
    });
}

fn push_samples_u16(
    data: &[u16],
    channels: usize,
    max_samples: usize,
    out: &Arc<Mutex<Vec<f32>>>,
    truncated: &Arc<AtomicBool>,
) {
    let mut buffer = match out.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    append_mono(data, channels, max_samples, truncated, &mut buffer, |s| {
        (s as f32 - 32768.0) / 32768.0
    });
}

fn append_mono<T, F>(
    data: &[T],
    channels: usize,
    max_samples: usize,
    truncated: &Arc<AtomicBool>,
    out: &mut Vec<f32>,
    convert: F,
) where
    T: Copy,
    F: Fn(T) -> f32,
{
    if channels == 0 {
        return;
    }

    for frame in data.chunks(channels) {
        if out.len() >= max_samples {
            truncated.store(true, Ordering::Relaxed);
            break;
        }
        let sum: f32 = frame.iter().map(|v| convert(*v)).sum();
        out.push(sum / channels as f32);
    }
}
