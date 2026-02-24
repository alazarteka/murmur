use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

pub struct RecordingSession {
    stop_tx: mpsc::Sender<()>,
    worker: Option<JoinHandle<()>>,
    pub samples: Arc<Mutex<Vec<f32>>>,
    pub sample_rate: u32,
    pub started_at: Instant,
}

pub struct CapturedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub duration_ms: i64,
}

pub fn start_capture(max_seconds: u32) -> Result<RecordingSession> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("No input microphone device found"))?;

    let supported = device.default_input_config()?;
    let sample_rate = supported.sample_rate().0;
    let channels = usize::from(supported.channels());
    let config: StreamConfig = supported.clone().into();

    let max_samples = sample_rate as usize * max_seconds as usize;
    let samples = Arc::new(Mutex::new(Vec::<f32>::with_capacity(max_samples)));
    let samples_for_thread = Arc::clone(&samples);

    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();

    let worker = thread::spawn(move || {
        let err_fn = |err| eprintln!("audio stream error: {err}");

        let stream = match supported.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _| {
                    push_samples_f32(data, channels, max_samples, &samples_for_thread)
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => {
                let samples_for_thread = Arc::clone(&samples_for_thread);
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        push_samples_i16(data, channels, max_samples, &samples_for_thread)
                    },
                    err_fn,
                    None,
                )
            }
            SampleFormat::U16 => {
                let samples_for_thread = Arc::clone(&samples_for_thread);
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        push_samples_u16(data, channels, max_samples, &samples_for_thread)
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

    CapturedAudio {
        samples,
        sample_rate: session.sample_rate,
        duration_ms,
    }
}

fn push_samples_f32(data: &[f32], channels: usize, max_samples: usize, out: &Arc<Mutex<Vec<f32>>>) {
    let mut buffer = match out.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    append_mono(data, channels, max_samples, &mut buffer, |s| s);
}

fn push_samples_i16(data: &[i16], channels: usize, max_samples: usize, out: &Arc<Mutex<Vec<f32>>>) {
    let mut buffer = match out.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    append_mono(data, channels, max_samples, &mut buffer, |s| {
        s as f32 / i16::MAX as f32
    });
}

fn push_samples_u16(data: &[u16], channels: usize, max_samples: usize, out: &Arc<Mutex<Vec<f32>>>) {
    let mut buffer = match out.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    append_mono(data, channels, max_samples, &mut buffer, |s| {
        (s as f32 - 32768.0) / 32768.0
    });
}

fn append_mono<T, F>(
    data: &[T],
    channels: usize,
    max_samples: usize,
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
            break;
        }
        let sum: f32 = frame.iter().map(|v| convert(*v)).sum();
        out.push(sum / channels as f32);
    }
}
