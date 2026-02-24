use crate::audio::RecordingSession;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AppStatus {
    Idle,
    Recording,
    Processing,
}

struct Inner {
    status: AppStatus,
    recording: Option<RecordingSession>,
}

#[derive(Clone)]
pub struct SharedState {
    inner: Arc<Mutex<Inner>>,
    db_path: Arc<PathBuf>,
    model_path: Arc<PathBuf>,
}

impl SharedState {
    pub fn new(db_path: PathBuf, model_path: PathBuf) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                status: AppStatus::Idle,
                recording: None,
            })),
            db_path: Arc::new(db_path),
            model_path: Arc::new(model_path),
        }
    }

    pub fn status(&self) -> AppStatus {
        self.inner
            .lock()
            .map(|inner| inner.status)
            .unwrap_or(AppStatus::Idle)
    }

    pub fn set_recording(&self, session: RecordingSession) -> Result<(), &'static str> {
        let mut guard = self.inner.lock().map_err(|_| "State lock poisoned")?;
        if guard.status != AppStatus::Idle {
            return Err("App is not idle");
        }
        guard.recording = Some(session);
        guard.status = AppStatus::Recording;
        Ok(())
    }

    pub fn take_recording(&self) -> Result<RecordingSession, &'static str> {
        let mut guard = self.inner.lock().map_err(|_| "State lock poisoned")?;
        if guard.status != AppStatus::Recording {
            return Err("App is not recording");
        }
        guard.status = AppStatus::Processing;
        guard.recording.take().ok_or("Recording session missing")
    }

    pub fn set_idle(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.status = AppStatus::Idle;
            guard.recording = None;
        }
    }

    pub fn db_path(&self) -> PathBuf {
        (*self.db_path).clone()
    }

    pub fn model_path(&self) -> PathBuf {
        (*self.model_path).clone()
    }
}
